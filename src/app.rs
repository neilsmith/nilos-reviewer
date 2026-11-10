//! The app state and its event/nav logic. `view` in `view.rs` draws it.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender};

use rune::{Key, KeyEvent, TextEdit, Tx};
use rune_kit::diff::{self, DiffState};

use crate::github::{self, Event, Request};
use crate::model::*;
use crate::theme::OmarchyPalette;

/// How the window picks light vs dark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    Auto,
    Light,
    Dark,
}

/// What the scan is doing, for the status bar.
#[derive(Clone, Debug, Default)]
pub enum Sync {
    #[default]
    Idle,
    Scanning {
        text: String,
    },
    Done,
    Failed(String),
}

/// Whether the right pane shows in the narrow (single-column) layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Queue,
    Detail,
}

/// The detail pane's two tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DetailTab {
    #[default]
    Conversation,
    Files,
}

pub struct App {
    // service
    pub cmd: Option<Sender<Request>>,
    pub events: Option<Receiver<Event>>,
    pub scan_generation: u64,
    pub viewer: String,
    pub sync: Sync,
    pub sync_failures: Vec<(String, String)>,
    pub notice: Option<String>,
    /// A self-update was applied; restart to run it. Renders as the footer
    /// version chip until then.
    pub update_ready: Option<String>,
    /// "updated Ns ago" — refreshed by poll so the view reads pure state
    /// (deriving it from the clock at build time trips rune's live_only
    /// heuristic, which compares view output across builds).
    pub synced_label: String,

    // queue
    pub all_prs: Vec<PrSummary>,
    pub groups: Vec<RepoGroup>,
    pub collapsed: HashSet<String>,
    pub cursor: Option<PrKey>,
    pub filter: TextEdit,
    pub show_blocked: bool,
    pub show_unassigned: bool,
    pub hide_done: bool,
    pub blocked: HashSet<PrKey>,
    pub blocked_since: HashMap<PrKey, f64>,
    pub handled: HashSet<PrKey>,
    /// The head SHA a PR was handled at; a push invalidates the mark.
    pub handled_sha: HashMap<PrKey, String>,
    pub submitting: HashMap<PrKey, ReviewKind>,

    // detail
    pub detail: Option<PrDetail>,
    pub detail_cache: HashMap<PrKey, PrDetail>,
    pub detail_loading: Option<PrKey>,
    pub diff: DiffState,
    pub detail_tab: DetailTab,

    // composer
    pub comment: TextEdit,
    pub drafts: HashMap<PrKey, String>,
    pub confirming: Option<ReviewKind>,
    pub review_error: Option<String>,

    // theme
    pub appearance: Appearance,
    pub omarchy: Option<OmarchyPalette>,
    pub omarchy_checked_at: f64,

    // layout
    pub window: [f32; 2],
    /// Sidebar width, draggable.
    pub sidebar_w: f32,

    /// Sidebar width when the current divider drag began.
    pub resize_from: f32,
    /// Background self-updater result channel (None in tests/fixtures).
    pub update_rx: Option<std::sync::mpsc::Receiver<String>>,
    /// Wall-clock secs when the last scan finished (staleness display).
    pub last_synced: Option<f64>,
    /// Key → laid-out rect, for tests and layout-sensitive logic.
    pub rects: std::collections::HashMap<String, [f32; 4]>,
    pub pane: Pane,
    pub pane_by_user: bool,
}

impl Default for App {
    fn default() -> Self {
        App {
            cmd: None,
            events: None,
            scan_generation: 0,
            viewer: String::new(),
            sync: Sync::Idle,
            sync_failures: Vec::new(),
            notice: None,
            update_ready: None,
            synced_label: String::new(),
            all_prs: Vec::new(),
            groups: Vec::new(),
            collapsed: HashSet::new(),
            cursor: None,
            filter: TextEdit::new(""),
            show_blocked: false,
            show_unassigned: false,
            hide_done: false,
            blocked: HashSet::new(),
            blocked_since: HashMap::new(),
            handled: HashSet::new(),
            handled_sha: HashMap::new(),
            submitting: HashMap::new(),
            detail: None,
            detail_cache: HashMap::new(),
            detail_loading: None,
            diff: DiffState::new(Vec::new()),
            detail_tab: DetailTab::Conversation,
            comment: TextEdit::new(""),
            drafts: HashMap::new(),
            confirming: None,
            review_error: None,
            appearance: Appearance::Auto,
            omarchy: crate::theme::current_palette(),
            omarchy_checked_at: 0.0,
            window: [1280.0, 800.0],
            sidebar_w: 300.0,
            resize_from: 300.0,
            update_rx: None,
            last_synced: None,
            rects: Default::default(),
            pane: Pane::Queue,
            pane_by_user: false,
        }
    }
}

/// The queue filter as the model wants it.
impl App {
    pub fn queue_filter(&self) -> QueueFilter<'_> {
        QueueFilter {
            collapsed: &self.collapsed,
            filter: &self.filter.text,
            show_blocked: self.show_blocked,
            show_unassigned: self.show_unassigned,
            hide_done: self.hide_done,
            blocked: &self.blocked,
            handled: &self.handled,
        }
    }

    pub fn rows(&self) -> Vec<QueueRow> {
        flattened_rows(&self.groups, &self.queue_filter())
    }

    /// Rebuild `groups` from `all_prs`; a push since a PR was handled revives it.
    fn regroup(&mut self) {
        let shas = std::mem::take(&mut self.handled_sha);
        self.handled.retain(|key| match shas.get(key) {
            None => true,
            Some(sha) => self
                .all_prs
                .iter()
                .find(|p| &p.key == key)
                .map(|p| p.head_sha.is_empty() || &p.head_sha == sha)
                .unwrap_or(true),
        });
        self.handled_sha = shas;
        self.groups = group_by_repository(self.all_prs.clone());
    }

    pub fn summary(&self, key: &PrKey) -> Option<&PrSummary> {
        self.all_prs.iter().find(|p| &p.key == key)
    }

    /// Whether the selected PR carries the blocked state.
    pub fn selected_blocked(&self) -> bool {
        self.cursor
            .as_ref()
            .and_then(|k| self.summary(k))
            .map(|p| p.is_blocked(&self.blocked))
            .unwrap_or(false)
    }

    /// Remove the Blocked label; optimistic, reverts on failure.
    pub fn unblock_selected(&mut self) {
        if !self.selected_blocked() {
            return;
        }
        let Some(key) = self.cursor.clone() else {
            return;
        };
        self.blocked.remove(&key);
        self.blocked_since.remove(&key);
        if let Some(p) = self.all_prs.iter_mut().find(|p| p.key == key) {
            p.blocked_label = false;
        }
        self.send(Request::Unblock(key));
    }

    /// PRs still needing attention — handled (submitted) reviews don't count.
    pub fn pr_count(&self) -> usize {
        self.groups
            .iter()
            .flat_map(|g| &g.prs)
            .filter(|p| !self.handled.contains(&p.key))
            .count()
    }

    /// PR rows the queue shows under the current toggles and filter,
    /// excluding handled reviews.
    pub fn visible_pr_count(&self) -> usize {
        let rows = crate::model::flattened_rows(&self.groups, &self.queue_filter());
        rows.iter()
            .filter(|r| matches!(r, crate::model::QueueRow::Pr(k) if !self.handled.contains(k)))
            .count()
    }

    /// The single-column breakpoint.
    pub fn narrow(&self) -> bool {
        self.window[0] < 780.0
    }

    /// Whether the dark theme applies at `system_dark`.
    pub fn dark(&self, system_dark: bool) -> bool {
        match self.appearance {
            Appearance::Auto => self
                .omarchy
                .as_ref()
                .map(|o| o.is_dark())
                .unwrap_or(system_dark),
            Appearance::Light => false,
            Appearance::Dark => true,
        }
    }

    /// The kit for `dark`: Omarchy palette when present and on Auto, else neutral.
    pub fn kit(&self, dark: bool) -> rune_kit::Kit {
        if self.appearance == Appearance::Auto {
            if let Some(kit) = self.omarchy.as_ref().and_then(OmarchyPalette::kit) {
                return kit;
            }
        }
        rune_kit::Kit::neutral(dark)
    }

    pub fn send(&self, request: Request) {
        if let Some(cmd) = &self.cmd {
            let _ = cmd.send(request);
        }
    }

    pub fn refresh(&mut self) {
        self.scan_generation += 1;
        self.sync_failures.clear();
        self.send(Request::Refresh);
        self.sync = Sync::Scanning {
            text: "Starting scan…".to_string(),
        };
    }

    /// Select a PR: stash the open draft, point the cursor, ask for its detail.
    pub fn select(&mut self, key: Option<PrKey>, tx: &mut Tx) {
        if self.cursor == key {
            return;
        }
        if let Some(old) = self.cursor.take() {
            self.drafts
                .insert(old, std::mem::take(&mut self.comment.text));
        }
        self.cursor = key.clone();
        if let Some(key) = &key {
            self.comment.text = self.drafts.remove(key).unwrap_or_default();
            if let Some(detail) = self.detail_cache.get(key) {
                self.set_detail(detail.clone());
            } else {
                self.detail = None;
                self.detail_loading = Some(key.clone());
                self.send(Request::LoadDetail(key.clone()));
            }
            // Prefetch the row below so arrow-down lands instantly.
            if let Some(next) = move_cursor(&self.rows(), self.cursor.as_ref(), 1) {
                if !self.detail_cache.contains_key(&next)
                    && self.detail_loading.as_ref() != Some(&next)
                {
                    self.send(Request::LoadDetail(next));
                }
            }
            tx.scroll_into_view_in("queue", Self::row_key(key));
            if self.narrow() {
                self.pane = Pane::Detail;
                self.pane_by_user = false;
            }
        } else {
            self.detail = None;
            self.diff = DiffState::new(Vec::new());
        }
        self.confirming = None;
    }

    /// Fixture data for headless scenes and tests: the queue, a viewer, and a
    /// selected PR with its detail.
    pub fn load_fixture(&mut self) {
        self.load_groups(crate::fixtures::queue());
    }

    /// The long fixture queue, for scroll/perf tests.
    pub fn load_long_fixture(&mut self) {
        self.load_groups(crate::fixtures::long_queue());
    }

    fn load_groups(&mut self, groups: Vec<crate::model::RepoGroup>) {
        self.all_prs = groups.into_iter().flat_map(|g| g.prs).collect();
        self.regroup();
        self.sync = Sync::Done;
        self.viewer = "neilsmith".to_string();
        let detail = crate::fixtures::detail();
        self.cursor = Some(detail.key.clone());
        self.set_detail(detail);
    }

    fn set_detail(&mut self, detail: PrDetail) {
        self.diff = DiffState::new(diff::parse(&detail.diff));
        self.detail = Some(detail);
        self.detail_loading = None;
        self.detail_tab = DetailTab::Conversation;
    }

    pub fn row_key(key: &PrKey) -> String {
        format!("pr/{}-{}", key.repo.replace('/', "-"), key.number)
    }

    /// Move the queue cursor and select, as the arrow keys do.
    pub fn move_cursor(&mut self, delta: i32, tx: &mut Tx) {
        let next = move_cursor(&self.rows(), self.cursor.as_ref(), delta);
        self.select(next, tx);
    }

    /// The next unhandled row below the cursor, for auto-advance on submit.
    pub fn advance(&mut self, tx: &mut Tx) {
        let next = next_unhandled(
            &self.rows(),
            self.cursor.as_ref(),
            &self.handled,
            &self.blocked,
            &self.submitting,
        )
        .or_else(|| move_cursor(&self.rows(), self.cursor.as_ref(), 1));
        self.select(next, tx);
    }

    /// Begin a review of `kind` for the selected PR.
    pub fn begin_review(&mut self, kind: ReviewKind) {
        let Some(key) = &self.cursor else { return };
        if self.submitting.contains_key(key) {
            return;
        }
        if kind.needs_body() && self.comment.text.trim().is_empty() {
            self.review_error = Some(format!(
                "a {} review needs a comment",
                kind.name().to_lowercase()
            ));
            return;
        }
        self.confirming = Some(kind);
    }

    /// Confirm the dialog: optimistic — close, mark submitting, advance, send.
    pub fn confirm_review(&mut self, tx: &mut Tx) {
        let (Some(kind), Some(key)) = (self.confirming.take(), self.cursor.clone()) else {
            return;
        };
        if kind == ReviewKind::Blocked {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            self.blocked.insert(key.clone());
            self.blocked_since.insert(key.clone(), now);
        }
        self.submitting.insert(key.clone(), kind);
        self.send(Request::SubmitReview {
            key,
            kind,
            body: self.comment.text.clone(),
        });
        self.advance(tx);
    }

    /// Apply one service event. Returns whether visible state changed.
    pub fn on_event(&mut self, event: Event, tx: &mut Tx) -> bool {
        match event {
            Event::AuthFailed(error) => {
                self.sync = Sync::Failed(error);
                true
            }
            Event::Discovered { login } => {
                self.viewer = login;
                true
            }
            Event::Stage { gen, text } => {
                if gen != self.scan_generation {
                    return false;
                }
                self.sync = Sync::Scanning { text };
                true
            }
            Event::ScanResult { gen, prs } => {
                if gen != self.scan_generation {
                    return false;
                }
                self.all_prs = prs;
                self.regroup();
                true
            }
            Event::ScanFailed { gen, error } => {
                if gen != self.scan_generation {
                    return false;
                }
                self.sync_failures.push(("search".to_string(), error));
                true
            }
            Event::SyncFinished { gen } => {
                self.last_synced = Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0),
                );
                if gen != self.scan_generation {
                    return false;
                }
                self.sync = Sync::Done;
                self.reconcile_blocked();
                // A selection whose detail was dropped by the gen bump needs a reload.
                if let Some(key) = &self.detail_loading {
                    self.send(Request::LoadDetail(key.clone()));
                }
                if self.cursor.is_none() {
                    let next = move_cursor(&self.rows(), None, 1);
                    self.select(next, tx);
                }
                true
            }
            Event::DetailLoaded { key, detail } => {
                if let Some(cached) = self.detail_cache.get(&key) {
                    if cached.head_sha != detail.head_sha {
                        self.handled.remove(&key);
                    }
                }
                if let Some(pr) = self.all_prs.iter_mut().find(|p| p.key == key) {
                    if !detail.requested.is_empty() {
                        pr.reasons = detail.requested.clone();
                    }
                }
                self.detail_cache.insert(key.clone(), *detail.clone());
                if self.cursor.as_ref() == Some(&key) {
                    self.set_detail(*detail);
                }
                true
            }
            Event::DetailFailed { key, error } => {
                if self.cursor.as_ref() == Some(&key) {
                    self.review_error = Some(error);
                    self.detail_loading = None;
                }
                true
            }
            Event::Unblocked { key } => {
                self.notice = Some(format!("Unblocked {}", key.display()));
                true
            }
            Event::UnblockFailed { key, error } => {
                self.blocked.insert(key.clone());
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0);
                self.blocked_since.insert(key.clone(), now);
                if let Some(p) = self.all_prs.iter_mut().find(|p| p.key == key) {
                    p.blocked_label = true;
                }
                self.review_error = Some(error);
                true
            }
            Event::ReviewSubmitted {
                key,
                kind,
                labelled,
            } => {
                self.submitting.remove(&key);
                self.handled.insert(key.clone());
                if let Some(sha) = self
                    .detail_cache
                    .get(&key)
                    .map(|d| d.head_sha.clone())
                    .or_else(|| self.summary(&key).map(|p| p.head_sha.clone()))
                {
                    self.handled_sha.insert(key.clone(), sha);
                }
                self.drafts.remove(&key);
                if self.cursor.as_ref() == Some(&key) {
                    self.comment.text.clear();
                    self.advance(tx);
                }
                self.notice = Some(match labelled {
                    Some(false) => "Review submitted, but labelling failed".to_string(),
                    _ => format!("{} submitted", kind.name()),
                });
                true
            }
            Event::ReviewFailed { key, error } => {
                self.submitting.remove(&key);
                self.blocked.remove(&key);
                self.review_error = Some(error);
                true
            }
        }
    }

    /// Drop local blocked marks the server no longer backs: a key stays only
    /// while the PR is queued AND (still labelled OR marked under two minutes
    /// ago).
    fn reconcile_blocked(&mut self) {
        const GRACE: f64 = 120.0;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        let queued: HashSet<&PrKey> = self
            .groups
            .iter()
            .flat_map(|g| g.prs.iter().map(|p| &p.key))
            .collect();
        let labelled: HashSet<PrKey> = self
            .groups
            .iter()
            .flat_map(|g| g.prs.iter())
            .filter(|p| p.blocked_label)
            .map(|p| p.key.clone())
            .collect();
        let blocked_since = self.blocked_since.clone();
        self.blocked.retain(|key| {
            queued.contains(key)
                && (labelled.contains(key)
                    || blocked_since
                        .get(key)
                        .map(|t| now - t < GRACE)
                        .unwrap_or(false))
        });
    }

    /// Relaunch and quit so an applied self-update takes over. Inside a .app
    /// bundle, relaunch the bundle so Finder/dock routing behaves.
    pub fn restart(&mut self, tx: &mut Tx) {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let spawned = exe
            .ancestors()
            .find(|p| p.extension() == Some(std::ffi::OsStr::new("app")))
            .map(|b| std::process::Command::new("open").arg("-n").arg(b).spawn())
            .unwrap_or_else(|| std::process::Command::new(exe).spawn());
        match spawned {
            Ok(_) => tx.quit(),
            Err(_) => {
                self.notice = Some("Couldn't relaunch — quit and reopen to run the update".into())
            }
        }
    }

    /// Periodic poll work: drain service events, re-read the Omarchy palette.
    pub fn poll(&mut self, _t: f64, tx: &mut Tx) -> bool {
        let mut changed = false;
        if let Some(rx) = &self.update_rx {
            if let Ok(version) = rx.try_recv() {
                self.update_ready = Some(version);
                self.update_rx = None;
                changed = true;
            }
        }
        let events: Vec<Event> = self
            .events
            .as_ref()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            changed |= self.on_event(event, tx);
        }
        let now = tx.clock();
        if now - self.omarchy_checked_at > 0.75 {
            self.omarchy_checked_at = now;
            let palette = crate::theme::current_palette();
            if palette != self.omarchy {
                self.omarchy = palette;
                changed = true;
            }
        }
        let label = self
            .last_synced
            .map(|t| {
                let ago = now_secs() as f64 - t;
                let text = if ago < 10.0 {
                    "just now".to_string()
                } else if ago < 60.0 {
                    format!("{ago:.0}s ago")
                } else if ago < 3600.0 {
                    format!("{:.0}m ago", ago / 60.0)
                } else {
                    format!("{:.0}h ago", ago / 3600.0)
                };
                format!("updated {text}")
            })
            .unwrap_or_default();
        if label != self.synced_label {
            self.synced_label = label;
            changed = true;
        }
        changed
    }
}

/// Called from `AppSpec::setup`: start the service (unless headless) and the
/// first scan. Slow work lives here, not in `main` — setup runs on_ready,
/// after the first frame.
pub fn setup(s: &mut App, waker: rune::Waker) {
    if rune::tooling::is_headless() {
        return;
    }
    s.update_rx = Some(crate::update::spawn());
    let (cmd, events) = github::spawn(move || waker());
    s.cmd = Some(cmd);
    s.events = Some(events);
    s.refresh();
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The keyboard contract: the queue cursor drives everything while no field
/// is focused; fields own their keys except Escape (and Enter in the filter).
pub fn on_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    // The error popup renders on top, so it owns the keyboard first.
    if s.review_error.is_some() {
        if matches!(ev.key, Key::Escape | Key::Enter) {
            s.review_error = None;
        }
        return true;
    }
    // Then the confirmation dialog: Escape cancels, Enter (or ⌃Enter) submits.
    if s.confirming.is_some() {
        match ev.key {
            Key::Escape => s.confirming = None,
            Key::Enter => s.confirm_review(tx),
            _ => {}
        }
        return true;
    }
    // A focused field owns its keys; only Escape (and the filter's Enter)
    // leave. `/` never reaches here while a field is focused.
    if let Some(focused) = tx.focused().map(str::to_string) {
        if focused.contains("composer") || focused.contains("filter") {
            match &ev.key {
                Key::Escape => {
                    if focused.contains("filter") {
                        s.filter.text.clear();
                    }
                    tx.blur();
                }
                Key::Enter if focused.contains("filter") => {
                    tx.blur();
                    let next = move_cursor(&s.rows(), None, 1);
                    s.select(next, tx);
                }
                _ => return false,
            }
            return true;
        }
    }
    match &ev.key {
        Key::Up => s.move_cursor(-1, tx),
        Key::Down => s.move_cursor(1, tx),
        Key::PageUp => tx.scroll_page("detail", -1),
        Key::PageDown => tx.scroll_page("detail", 1),
        Key::Enter => {
            if s.cursor.is_none() {
                s.move_cursor(1, tx);
            }
        }
        Key::Function(5) => s.refresh(),
        Key::Tab => tx.focus("composer/input", false),
        Key::Escape => {
            if s.pane == Pane::Detail {
                s.pane = Pane::Queue;
                s.pane_by_user = true;
            }
        }
        Key::Text(t) if t == "/" => tx.focus("filter/input", false),
        _ => {
            let kind = if ev.is_command('a') {
                Some(ReviewKind::Approve)
            } else if ev.is_command('c') {
                Some(ReviewKind::Comment)
            } else if ev.is_command('r') {
                Some(ReviewKind::RequestChanges)
            } else if ev.is_command('b') {
                if s.selected_blocked() {
                    s.unblock_selected();
                    return true;
                }
                Some(ReviewKind::Blocked)
            } else {
                None
            };
            let Some(kind) = kind else { return false };
            s.begin_review(kind);
        }
    }
    true
}
