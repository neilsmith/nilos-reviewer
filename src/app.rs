use crate::service::{ServiceCommand, ServiceEvent, ServiceHandle};
use crate::{browser, omarchy, store, update};
use gpui_kit::assets::IconName;
use gpui_kit::base::{StyledExt, h_flex, v_flex};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::diff::{Diff, DiffFile, DiffHoverHighlight, DiffState};
use gpui_kit::component::input::{Editor, EditorState, InputEvent, Textarea, TextareaState};
use gpui_kit::component::progress::Progress;
use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, Sizable, Theme};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use nilos_reviewer::render;
use pr_core::{
    DiscussionEntry, DiscussionKind, HandledPr, PrDetails, PrKey, PrSummary, QueueItem, ReviewKind,
    SyncProgress, filtered_groups, flattened_children, flattened_rows, group_by_repository,
    next_unhandled, pr_child_index,
};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const UI_POLL: Duration = Duration::from_millis(100);
/// Locally-blocked PRs stay hidden this long while GitHub catches up with a
/// just-submitted `Blocked` label; afterwards the server label is the truth.
const BLOCKED_GRACE: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    Queue,
    Details,
    Composer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailTab {
    Conversation,
    Files,
}

#[derive(Debug)]
enum DetailState {
    Loading,
    Loaded(Box<PrDetails>),
    Failed(String),
}

pub struct ApproverView {
    service: ServiceHandle,
    update_events: mpsc::Receiver<String>,
    updated_to: Option<String>,
    queue: Vec<PrSummary>,
    scan_queue: Vec<PrSummary>,
    selected: Option<PrKey>,
    details: HashMap<PrKey, DetailState>,
    handled: HashMap<PrKey, HandledPr>,
    blocked: HashSet<PrKey>,
    /// When each locally-blocked key was marked — short grace window so a
    /// refresh doesn't un-hide a PR before GitHub reports its new label.
    blocked_since: HashMap<PrKey, Instant>,
    /// Set when drafts change; `poll_service` flushes it after a quiet
    /// period instead of writing on every keystroke.
    state_dirty: Option<Instant>,
    /// Generation of the most recent Refresh the UI sent — stale scan events
    /// from a superseded scan are dropped in `poll_service`.
    scan_generation: u64,
    collapsed: HashSet<String>,
    show_handled: bool,
    show_blocked: bool,
    progress: SyncProgress,
    sync_failures: Vec<String>,
    auth_error: Option<String>,
    notice: Option<String>,
    pane: Pane,
    list_cursor: usize,
    detail_tab: DetailTab,
    filter: Entity<TextareaState>,
    composer: Entity<EditorState>,
    composer_preview: bool,
    confirming: Option<ReviewKind>,
    submitting: Option<PrKey>,
    queue_scroll: ScrollHandle,
    details_scroll: ScrollHandle,
    diff_state: Entity<DiffState>,
    view_focus: FocusHandle,
    state: store::AppState,
    omarchy_theme: Option<Rc<component::ThemeConfig>>,
    _filter_sub: Subscription,
    _composer_sub: Subscription,
    _omarchy_watch: Option<Task<()>>,
    _poller: Task<()>,
}

fn relative_github_url(url: &str, context: &(String, String)) -> Option<String> {
    if url.starts_with('#') {
        return None;
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        return Some(url.to_string());
    }
    if url.starts_with('/') {
        return Some(format!("https://github.com{url}"));
    }
    Some(format!(
        "https://github.com/{}/blob/{}/{}",
        context.0,
        context.1,
        url.trim_start_matches("./")
    ))
}

fn open_rendered_link(url: &str, context: Option<&(String, String)>, cx: &mut App) {
    let resolved = context
        .and_then(|context| relative_github_url(url, context))
        .or_else(|| {
            (url.starts_with("http://") || url.starts_with("https://")).then(|| url.to_string())
        });
    let Some(resolved) = resolved else { return };
    let fallback = resolved.clone();
    browser::open(&resolved, move || cx.open_url(&fallback));
}

fn review_badge(kind: DiscussionKind) -> &'static str {
    match kind {
        DiscussionKind::Description => "Description",
        DiscussionKind::Comment => "Comment",
        DiscussionKind::Review => "Review",
        DiscussionKind::InlineComment => "Inline",
    }
}

impl ApproverView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let service = ServiceHandle::start();
        let filter = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 1)
                .placeholder("Filter queue")
        });
        let filter_sub = cx.subscribe(&filter, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.list_cursor = 0;
                cx.notify();
            }
        });
        let composer = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("markdown")
                .line_number(false)
                .folding(false)
                .searchable(false)
                .placeholder("Review comment — Markdown supported")
        });
        let composer_sub = cx.subscribe(&composer, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.save_current_draft(cx);
                cx.notify();
            }
        });
        let service_for_refresh = service.commands.clone();
        let poller = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(UI_POLL).await;
                if this
                    .update_in(cx, |view, window, cx| view.poll_service(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let omarchy_theme = omarchy::current_config();
        let follow_omarchy = omarchy_theme.is_some();
        let omarchy_watch = cx.spawn_in(window, async move |this, cx| {
            let mut last_colors = omarchy::read_colors().unwrap_or_default();
            let mut last_dark: Option<bool> = None;
            loop {
                cx.background_executor().timer(omarchy::POLL).await;
                let result = this.update_in(cx, |view, _, cx| {
                    if follow_omarchy {
                        view.poll_omarchy(&mut last_colors, cx);
                    } else {
                        view.poll_system_theme(&mut last_dark, cx);
                    }
                });
                if result.is_err() {
                    break;
                }
            }
        });
        let diff_state = cx.new(|cx| DiffState::new(Vec::<DiffFile>::new(), cx));
        let state = store::load();
        let persisted_handled = state
            .handled
            .iter()
            .map(|handled| (handled.key.clone(), handled.clone()))
            .collect();
        let scan_generation = service.current_generation();
        let view = Self {
            service,
            update_events: update::spawn(),
            updated_to: None,
            queue: Vec::new(),
            scan_queue: Vec::new(),
            selected: None,
            details: HashMap::new(),
            handled: persisted_handled,
            blocked: state.blocked.iter().cloned().collect(),
            blocked_since: HashMap::new(),
            state_dirty: None,
            scan_generation,
            collapsed: HashSet::new(),
            show_handled: true,
            show_blocked: false,
            progress: SyncProgress::default(),
            sync_failures: Vec::new(),
            auth_error: None,
            notice: None,
            pane: Pane::Queue,
            list_cursor: 0,
            detail_tab: DetailTab::Conversation,
            filter,
            composer,
            composer_preview: false,
            confirming: None,
            submitting: None,
            queue_scroll: ScrollHandle::new(),
            details_scroll: ScrollHandle::new(),
            diff_state,
            view_focus: cx.focus_handle(),
            state,
            omarchy_theme,
            _filter_sub: filter_sub,
            _composer_sub: composer_sub,
            _omarchy_watch: Some(omarchy_watch),
            _poller: poller,
        };
        let _ = service_for_refresh.send(ServiceCommand::Refresh {
            generation: view.service.current_generation(),
        });
        view.view_focus.focus(window, cx);
        view
    }

    /// Non-Omarchy path (e.g. macOS): follow the OS light/dark appearance.
    fn poll_system_theme(&mut self, last: &mut Option<bool>, cx: &mut Context<Self>) {
        let dark = matches!(
            cx.window_appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        if Some(dark) != *last {
            *last = Some(dark);
            crate::themes::apply(cx, dark);
        }
    }

    fn poll_omarchy(&mut self, last: &mut String, cx: &mut Context<Self>) {
        let current = omarchy::read_colors().unwrap_or_default();
        if current == *last {
            return;
        }
        *last = current;
        if let Some(config) = omarchy::current_config() {
            self.omarchy_theme = Some(Rc::clone(&config));
            Theme::update(cx, |theme| theme.apply_config(&config));
            cx.notify();
        }
    }

    fn poll_service(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Debounced draft flush — writes at most ~500ms after typing stops.
        if self
            .state_dirty
            .is_some_and(|since| since.elapsed() > Duration::from_millis(500))
        {
            self.persist_state();
        }
        let mut changed = false;
        while let Ok(version) = self.update_events.try_recv() {
            self.notice = Some(format!("Updated to {version} — restart to apply"));
            self.updated_to = Some(version);
            changed = true;
        }
        while let Ok(event) = self.service.events.try_recv() {
            // Scan-scoped events from a superseded scan must not merge into
            // the current queue — drop them. Detail/review events carry the
            // generation they were requested under and stay valid for the
            // still-selected PR, so they are not filtered.
            let stale = match &event {
                ServiceEvent::AuthFailed { generation, .. }
                | ServiceEvent::Stage { generation, .. }
                | ServiceEvent::RepositoryDone { generation, .. }
                | ServiceEvent::RepositoryFailed { generation, .. }
                | ServiceEvent::SyncFinished { generation, .. } => {
                    *generation != self.scan_generation
                }
                _ => false,
            };
            if stale {
                continue;
            }
            changed = true;
            match event {
                ServiceEvent::AuthFailed {
                    generation: _,
                    message,
                } => {
                    self.auth_error = Some(message);
                }
                ServiceEvent::Stage { progress, .. } => {
                    self.progress = progress;
                    self.sync_failures.clear();
                }
                ServiceEvent::RepositoryDone {
                    pull_requests,
                    progress,
                    ..
                } => {
                    self.progress = progress;
                    let mut seen = self
                        .scan_queue
                        .iter()
                        .map(|pr| pr.key.clone())
                        .collect::<HashSet<_>>();
                    for pr in pull_requests {
                        if seen.insert(pr.key.clone()) {
                            self.scan_queue.push(pr);
                        }
                    }
                    self.merge_scan_queue();
                }
                ServiceEvent::RepositoryFailed {
                    repo,
                    message,
                    progress,
                    ..
                } => {
                    self.progress = progress;
                    self.sync_failures.push(format!("{repo}: {message}"));
                }
                ServiceEvent::SyncFinished {
                    progress, failures, ..
                } => {
                    self.progress = progress;
                    self.sync_failures = failures;
                    self.queue = self.scan_queue.clone();
                    self.queue.sort_by(|a, b| {
                        a.repo_name()
                            .cmp(&b.repo_name())
                            .then(a.created_at.cmp(&b.created_at))
                            .then(a.key.number.cmp(&b.key.number))
                    });
                    self.scan_queue.clear();
                    // A handled entry is only complete for the inspected head;
                    // new commits return it to the outstanding queue.
                    let head_by_key = self
                        .queue
                        .iter()
                        .map(|pr| (pr.key.clone(), pr.head_sha.clone()))
                        .collect::<HashMap<_, _>>();
                    self.handled.retain(|key, handled| {
                        head_by_key
                            .get(key)
                            .is_none_or(|head| head == &handled.head_sha)
                    });
                    // Reconcile local blocked marks with the server: a PR
                    // stays hidden while the server still carries the label
                    // or while a just-submitted label is within the grace
                    // window. If GitHub's label was removed, the PR returns
                    // to the queue. Keys for PRs that left the queue are
                    // dropped so the set can't grow stale entries forever.
                    let server_blocked = self
                        .queue
                        .iter()
                        .filter(|pr| pr.blocked)
                        .map(|pr| pr.key.clone())
                        .collect::<HashSet<_>>();
                    self.blocked.retain(|key| {
                        head_by_key.contains_key(key)
                            && (server_blocked.contains(key)
                                || self
                                    .blocked_since
                                    .get(key)
                                    .is_some_and(|since| since.elapsed() < BLOCKED_GRACE))
                    });
                    self.blocked_since
                        .retain(|key, _| self.blocked.contains(key));
                    if self.selected.is_none() {
                        let rows = self.visible_rows(cx);
                        if let Some(first) = rows.first() {
                            self.select_pr(&first.key, window, cx);
                        }
                    }
                    // A detail load dropped by the generation bump leaves the
                    // selected PR stuck in Loading — re-request it.
                    if let Some(key) = &self.selected
                        && matches!(self.details.get(key), Some(DetailState::Loading))
                    {
                        self.service.load_details(key.clone());
                    }
                }
                ServiceEvent::DetailLoaded { mut detail, .. } => {
                    // Refresh the summary from the queue only when the PR is
                    // still present — substituting a different (selected)
                    // PR's summary would show the wrong PR's conversation.
                    if let Some(summary) = self.find_pr(&detail.summary.key) {
                        detail.summary = summary;
                    }
                    let patch = detail.patch_document();
                    let key = detail.summary.key.clone();
                    self.update_diff_state(&key, &patch, cx);
                    self.details
                        .insert(detail.summary.key.clone(), DetailState::Loaded(detail));
                }
                ServiceEvent::DetailFailed { key, message, .. } => {
                    self.details.insert(key, DetailState::Failed(message));
                }
                ServiceEvent::ReviewSubmitted {
                    key,
                    kind,
                    label_error,
                    ..
                } => {
                    if kind == ReviewKind::Blocked {
                        // Blocked PRs park in the blocked list rather than
                        // "done", so Show blocked can reveal them again.
                        self.blocked.insert(key.clone());
                        self.blocked_since.insert(key.clone(), Instant::now());
                    } else if let Some(summary) = self.find_pr(&key) {
                        self.handled.insert(
                            key.clone(),
                            HandledPr {
                                key: key.clone(),
                                kind,
                                head_sha: summary.head_sha,
                            },
                        );
                    }
                    self.submitting = None;
                    self.confirming = None;
                    // The submitted body was posted to GitHub — it must not
                    // linger as a draft or it would reappear next session.
                    self.state.drafts.remove(&key.display());
                    if self.selected.as_ref() == Some(&key) {
                        self.composer
                            .update(cx, |state, cx| state.set_value(String::new(), window, cx));
                    }
                    self.notice = Some(match label_error {
                        Some(error) => {
                            format!(
                                "{} submitted for {key}, but labelling failed: {error}",
                                kind.label()
                            )
                        }
                        None => format!("{} submitted for {key}", kind.label()),
                    });
                    self.persist_state();
                    self.select_next_unhandled(window, cx);
                }
                ServiceEvent::ReviewFailed { key, message, .. } => {
                    self.submitting = None;
                    self.confirming = None;
                    self.notice = Some(format!("Review failed for {key}: {message}"));
                }
            }
        }
        if changed {
            cx.notify();
        }
    }

    fn merge_scan_queue(&mut self) {
        if self.scan_queue.is_empty() {
            return;
        }
        let mut existing = self
            .queue
            .iter()
            .map(|pr| pr.key.clone())
            .collect::<HashSet<_>>();
        for pr in &self.scan_queue {
            if existing.insert(pr.key.clone()) {
                self.queue.push(pr.clone());
            }
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.save_current_draft(cx);
        self.scan_queue.clear();
        self.sync_failures.clear();
        self.progress = SyncProgress {
            stage: "Starting sync".into(),
            started: true,
            ..SyncProgress::default()
        };
        self.auth_error = None;
        self.service.refresh();
        self.scan_generation = self.service.current_generation();
        cx.notify();
    }

    fn find_pr(&self, key: &PrKey) -> Option<PrSummary> {
        self.queue
            .iter()
            .chain(self.scan_queue.iter())
            .find(|pr| &pr.key == key)
            .cloned()
    }

    fn groups(&self, cx: &App) -> Vec<pr_core::RepoGroup> {
        let query = self.filter.read(cx).value().to_string();
        let handled = self.handled.keys().cloned().collect::<HashSet<_>>();
        filtered_groups(
            &group_by_repository(&self.queue),
            &query,
            self.show_handled,
            &handled,
            self.show_blocked,
            &self.blocked,
        )
    }

    fn visible_rows(&self, cx: &App) -> Vec<PrSummary> {
        flattened_rows(&self.groups(cx), &self.collapsed)
    }

    fn select_pr(&mut self, key: &PrKey, window: &mut Window, cx: &mut Context<Self>) {
        self.save_current_draft(cx);
        self.selected = Some(key.clone());
        if self.pane != Pane::Composer {
            self.pane = Pane::Details;
            self.view_focus.focus(window, cx);
        }
        self.detail_tab = DetailTab::Conversation;
        self.details_scroll = ScrollHandle::new();
        let draft = self
            .state
            .drafts
            .get(&key.display())
            .cloned()
            .unwrap_or_default();
        self.composer
            .update(cx, |state, cx| state.set_value(draft, window, cx));
        if !matches!(self.details.get(key), Some(DetailState::Loaded(_))) {
            self.details.insert(key.clone(), DetailState::Loading);
            self.service.load_details(key.clone());
        } else {
            let patch = self.details.get(key).and_then(|state| match state {
                DetailState::Loaded(detail) => Some(detail.patch_document()),
                _ => None,
            });
            if let Some(patch) = patch {
                self.update_diff_state(&key.clone(), &patch, cx);
            }
        }
        cx.notify();
    }

    /// Drafts update in memory immediately but persist lazily — the composer
    /// fires this on every keystroke, so the disk write is debounced and
    /// flushed by `poll_service` after a short quiet period.
    fn save_current_draft(&mut self, cx: &App) {
        let Some(key) = &self.selected else { return };
        let value = self.composer.read(cx).value().to_string();
        if value.is_empty() {
            self.state.drafts.remove(&key.display());
        } else {
            self.state.drafts.insert(key.display(), value);
        }
        self.state_dirty = Some(Instant::now());
    }

    fn persist_state(&mut self) {
        let mut state = self.state.clone();
        state.handled = self.handled.values().cloned().collect();
        state.handled.sort_by(|a, b| a.key.cmp(&b.key));
        state.blocked = self.blocked.iter().cloned().collect();
        if let Err(error) = store::save(&state) {
            eprintln!("failed to save NilosReviewer state: {error}");
        }
        self.state_dirty = None;
    }

    fn update_diff_state(&mut self, key: &PrKey, patch: &str, cx: &mut Context<Self>) {
        if self.selected.as_ref() != Some(key) {
            return;
        }
        let files = DiffFile::parse(patch).unwrap_or_default();
        self.diff_state = cx.new(|cx| DiffState::new(files, cx));
    }

    fn select_next_unhandled(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let handled = self.handled.keys().cloned().collect::<HashSet<_>>();
        // Blocked PRs are parked, not done — never auto-advance back onto one
        // even when Show blocked makes it visible.
        let rows = self
            .visible_rows(cx)
            .into_iter()
            .filter(|pr| !pr.blocked && !self.blocked.contains(&pr.key))
            .collect::<Vec<_>>();
        if let Some(next) = next_unhandled(&rows, self.selected.as_ref(), &handled) {
            self.select_pr(&next, window, cx);
        }
    }

    fn validate_review(&self, kind: ReviewKind, cx: &App) -> Result<(), String> {
        let Some(key) = &self.selected else {
            return Err("No pull request selected".into());
        };
        let Some(DetailState::Loaded(detail)) = self.details.get(key) else {
            return Err("Wait for this pull request to finish loading".into());
        };
        if !detail.can_review || detail.pending_review_exists {
            return Err(detail
                .review_notice
                .clone()
                .unwrap_or_else(|| "This pull request cannot be reviewed".into()));
        }
        let body = self.composer.read(cx).value();
        if kind.requires_body() && body.trim().is_empty() {
            return Err(format!("{} requires a comment", kind.label()));
        }
        Ok(())
    }

    fn request_review(&mut self, kind: ReviewKind, window: &mut Window, cx: &mut Context<Self>) {
        // Buttons disable while submitting but hotkeys don't — ignore a
        // second request so it can't post a duplicate review.
        if self.submitting.is_some() {
            return;
        }
        if let Err(message) = self.validate_review(kind, cx) {
            self.notice = Some(message);
            self.view_focus.focus(window, cx);
            cx.notify();
            return;
        }
        self.confirming = Some(kind);
        cx.notify();
    }

    fn submit_review(&mut self, kind: ReviewKind, cx: &mut Context<Self>) {
        if self.submitting.is_some() {
            return;
        }
        let Some(key) = self.selected.clone() else {
            return;
        };
        let Some(DetailState::Loaded(detail)) = self.details.get(&key) else {
            return;
        };
        let body = self.composer.read(cx).value().to_string();
        self.submitting = Some(key.clone());
        self.service
            .submit_review(key, kind, body, detail.summary.head_sha.clone());
        cx.notify();
    }

    fn nav_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if self.confirming.is_some() {
            match key {
                "escape" => self.confirming = None,
                "enter" if event.keystroke.modifiers.secondary(cx) => {
                    if let Some(kind) = self.confirming.take() {
                        self.submit_review(kind, cx);
                    }
                }
                _ => {}
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        // While the filter is focused it owns the keyboard — typing `/` or
        // Enter must reach the text field, not trigger queue actions.
        // Enter/Escape hand focus back to the view.
        if self.filter.read(cx).focus_handle(cx).is_focused(window) {
            if matches!(key, "escape" | "enter") {
                self.view_focus.focus(window, cx);
                cx.stop_propagation();
                cx.notify();
            }
            return;
        }
        match (key, self.pane) {
            ("tab", Pane::Queue) => {
                self.pane = Pane::Details;
                self.view_focus.focus(window, cx);
                cx.stop_propagation();
            }
            ("tab", Pane::Details) => {
                self.pane = Pane::Composer;
                self.composer
                    .update(cx, |state, cx| state.focus_handle(cx).focus(window, cx));
                cx.stop_propagation();
            }
            ("tab", Pane::Composer) => {
                self.pane = Pane::Queue;
                self.view_focus.focus(window, cx);
                cx.stop_propagation();
            }
            // Up/Down always moves through the queue and selects the row —
            // the detail loads as you arrow. The composer keeps arrows for
            // caret movement.
            ("up" | "down", _) if !self.composer.read(cx).focus_handle(cx).is_focused(window) => {
                let groups = self.groups(cx);
                let rows = flattened_rows(&groups, &self.collapsed);
                if !rows.is_empty() {
                    self.list_cursor = if key == "up" {
                        self.list_cursor.saturating_sub(1)
                    } else {
                        (self.list_cursor + 1).min(rows.len() - 1)
                    };
                    // scroll_to_item indexes the scroll container's direct
                    // children — headers included — not the flat PR cursor.
                    let children = flattened_children(&groups, &self.collapsed);
                    if let Some(child_ix) = pr_child_index(&children, self.list_cursor) {
                        self.queue_scroll.scroll_to_item(child_ix);
                    }
                    if let Some(pr) = rows.get(self.list_cursor).cloned() {
                        self.select_pr(&pr.key, window, cx);
                    }
                    cx.stop_propagation();
                }
            }
            ("pageup" | "pagedown", _) => {
                let offset = self.details_scroll.offset();
                let max = self.details_scroll.max_offset();
                let viewport = self.details_scroll.bounds().size.height;
                let delta = if key == "pageup" {
                    viewport * 0.66
                } else {
                    -viewport * 0.66
                };
                self.details_scroll
                    .set_offset(point(offset.x, (offset.y + delta).clamp(-max.y, px(0.))));
                cx.stop_propagation();
            }
            ("enter", Pane::Queue) => {
                if let Some(pr) = self.visible_rows(cx).get(self.list_cursor).cloned() {
                    self.select_pr(&pr.key, window, cx);
                }
                cx.stop_propagation();
            }
            ("/", Pane::Queue | Pane::Details) => {
                self.filter
                    .update(cx, |state, cx| state.focus_handle(cx).focus(window, cx));
                cx.stop_propagation();
            }
            ("a" | "c" | "r" | "b", _)
                if event.keystroke.modifiers.secondary(cx)
                    && !self.composer.read(cx).focus_handle(cx).is_focused(window)
                    && !self.filter.read(cx).focus_handle(cx).is_focused(window) =>
            {
                let kind = match key {
                    "a" => ReviewKind::Approve,
                    "c" => ReviewKind::Comment,
                    "r" => ReviewKind::RequestChanges,
                    _ => ReviewKind::Blocked,
                };
                self.request_review(kind, window, cx);
                cx.stop_propagation();
            }
            ("f5", _) => {
                if self.submitting.is_none() {
                    self.refresh(cx);
                }
                cx.stop_propagation();
            }
            ("escape", _) => {
                match self.pane {
                    Pane::Composer => {
                        self.pane = Pane::Details;
                        self.view_focus.focus(window, cx);
                    }
                    Pane::Details => {
                        self.pane = Pane::Queue;
                        self.view_focus.focus(window, cx);
                    }
                    Pane::Queue => {}
                }
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    fn render_sidebar(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let groups = self.groups(cx);
        let rows = flattened_rows(&groups, &self.collapsed);
        let outstanding = self
            .queue
            .iter()
            .filter(|pr| !self.handled.contains_key(&pr.key))
            .filter(|pr| !pr.blocked && !self.blocked.contains(&pr.key))
            .count();
        v_flex()
            .w_full()
            .h_full()
            .bg(theme.sidebar)
            .child(
                v_flex()
                    .px_3()
                    .pt_3()
                    .pb_2()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        svg()
                                            .path("nilos-logo.svg")
                                            .size(px(20.))
                                            .text_color(theme.accent),
                                    )
                                    .child(
                                        div()
                                            .text_base()
                                            .font_semibold()
                                            .text_color(theme.accent)
                                            .child("NilosReviewer"),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(theme.foreground)
                                    .child("Review Queue"),
                            )
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(format!("{outstanding} left")),
                                    )
                                    .child(
                                        Button::new("refresh")
                                            .ghost()
                                            .xsmall()
                                            .icon(Icon::new(IconName::RefreshCw))
                                            .tooltip("Refresh")
                                            .disabled(
                                                self.submitting.is_some()
                                                    || (self.progress.started
                                                        && !self.progress.complete),
                                            )
                                            .on_click(
                                                cx.listener(|this, _, _, cx| this.refresh(cx)),
                                            ),
                                    ),
                            ),
                    )
                    .child(
                        Textarea::new(&self.filter)
                            .h(px(34.))
                            .aria_label("Filter pull requests"),
                    ),
            )
            .child(
                v_flex()
                    .id("review-queue")
                    .flex_1()
                    .min_h_0()
                    .gap_1()
                    .overflow_y_scroll()
                    .track_scroll(&self.queue_scroll)
                    .px_2()
                    .pb_2()
                    .children(
                        flattened_children(&groups, &self.collapsed)
                            .into_iter()
                            .map(|item| match item {
                                QueueItem::Header(group) => {
                                    self.group_header(&group, cx).into_any_element()
                                }
                                QueueItem::Row(pr) => {
                                    let flat_ix =
                                        rows.iter().position(|row| row.key == pr.key).unwrap_or(0);
                                    self.pr_row(&pr, flat_ix, cx).into_any_element()
                                }
                            }),
                    )
                    .when(rows.is_empty() && self.progress.complete, |el| {
                        el.child(
                            div()
                                .m_3()
                                .p_4()
                                .rounded_lg()
                                .border_1()
                                .border_color(theme.border)
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child(if self.progress.repositories_failed > 0 {
                                    "Sync incomplete — some repositories could not be checked"
                                } else {
                                    "No reviews waiting"
                                }),
                        )
                    }),
            )
            .child(self.render_sync_footer(cx))
    }

    fn group_header(&self, group: &pr_core::RepoGroup, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let group_name = group.name.clone();
        let collapsed = self.collapsed.contains(&group_name);
        let remaining = group
            .rows
            .iter()
            .filter(|pr| !self.handled.contains_key(&pr.key))
            .count();
        div()
            .id(ElementId::Name(format!("repo-{group_name}").into()))
            .px_2()
            .py_2()
            .rounded_md()
            .cursor_pointer()
            .hover(|style| style.bg(theme.list_hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.collapsed.contains(&group_name) {
                    this.collapsed.remove(&group_name);
                } else {
                    this.collapsed.insert(group_name.clone());
                }
                cx.notify();
            }))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Icon::new(if collapsed {
                                    IconName::ChevronRight
                                } else {
                                    IconName::ChevronDown
                                })
                                .size_3p5()
                                .text_color(theme.muted_foreground),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(theme.muted_foreground)
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .child(group.name.clone()),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("{remaining}/{}", group.rows.len())),
                    ),
            )
    }

    fn pr_row(&self, pr: &PrSummary, flat_ix: usize, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let selected = self.selected.as_ref() == Some(&pr.key);
        let cursor = self.pane == Pane::Queue && self.list_cursor == flat_ix;
        let handled = self.handled.get(&pr.key);
        let key = pr.key.clone();
        div()
            .id(ElementId::Name(format!("pr-{}", pr.key.display()).into()))
            .w_full()
            .px_2()
            .py_2()
            .rounded_md()
            .cursor_pointer()
            .bg(if selected {
                theme.list_active
            } else {
                theme.transparent
            })
            .border_1()
            .border_color(if cursor || selected {
                theme.primary
            } else {
                theme.transparent
            })
            .hover(|style| {
                style.bg(if selected {
                    theme.list_active
                } else {
                    theme.list_hover
                })
            })
            .opacity(if handled.is_some() { 0.58 } else { 1. })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.list_cursor = flat_ix;
                this.select_pr(&key, window, cx)
            }))
            .child(
                h_flex()
                    .w_full()
                    .items_start()
                    .gap_2()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_medium()
                                    .text_color(theme.foreground)
                                    .child(format!("#{} {}", pr.key.number, pr.title)),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(pr.author.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(if pr.reasons.iter().any(|r| r == "You") {
                                                theme.primary
                                            } else {
                                                theme.info
                                            })
                                            .child(
                                                if pr.blocked || self.blocked.contains(&pr.key) {
                                                    format!("Blocked · {}", pr.reason_label())
                                                } else if pr.draft {
                                                    format!("Draft · {}", pr.reason_label())
                                                } else {
                                                    pr.reason_label()
                                                },
                                            ),
                                    ),
                            ),
                    )
                    .children(handled.map(|_| {
                        Icon::new(IconName::CircleCheck)
                            .size_4()
                            .text_color(theme.success)
                            .into_any_element()
                    })),
            )
    }

    fn render_sync_footer(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let progress = &self.progress;
        let status = if let Some(error) = &self.auth_error {
            format!("Authentication failed: {error}")
        } else if !progress.started {
            "Not synced".into()
        } else if progress.complete {
            format!(
                "{} repositories checked · {} found{}",
                progress.repositories_checked,
                progress.pull_requests_found,
                if progress.repositories_failed > 0 {
                    format!(" · {} failed", progress.repositories_failed)
                } else {
                    String::new()
                },
            )
        } else if progress.repositories_total == 0 {
            progress.stage.clone()
        } else {
            format!(
                "{} · {}/{} repositories · {} found",
                progress.stage,
                progress.repositories_checked,
                progress.repositories_total,
                progress.pull_requests_found
            )
        };
        v_flex()
            .border_t_1()
            .border_color(theme.border)
            .px_3()
            .py_2()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(if self.auth_error.is_some() {
                        theme.danger
                    } else {
                        theme.muted_foreground
                    })
                    .child(status),
            )
            .child(
                Progress::new("sync")
                    .value(progress.fraction() * 100.)
                    .loading(progress.started && !progress.complete)
                    .small(),
            )
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        Button::new("toggle-handled")
                            .ghost()
                            .xsmall()
                            .icon(Icon::new(if self.show_handled {
                                IconName::Eye
                            } else {
                                IconName::EyeOff
                            }))
                            .label(if self.show_handled {
                                "Hide done"
                            } else {
                                "Show done"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_handled = !this.show_handled;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("toggle-blocked")
                            .ghost()
                            .xsmall()
                            .icon(Icon::new(IconName::OctagonAlert))
                            .label(if self.show_blocked {
                                "Hide blocked"
                            } else {
                                "Show blocked"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_blocked = !this.show_blocked;
                                cx.notify();
                            })),
                    ),
            )
            .children(self.sync_failures.first().map(|failure| {
                div()
                    .text_xs()
                    .text_color(theme.warning)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(failure.clone())
                    .into_any_element()
            }))
            .children(self.updated_to.as_ref().map(|version| {
                div()
                    .text_xs()
                    .text_color(theme.accent)
                    .child(format!("Updated to {version} — restart to apply"))
                    .into_any_element()
            }))
    }

    fn render_detail(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let Some(key) = &self.selected else {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .child(
                    v_flex()
                        .items_center()
                        .gap_3()
                        .child(
                            Icon::new(IconName::GitPullRequest)
                                .size_10()
                                .text_color(theme.muted_foreground),
                        )
                        .child(
                            div()
                                .text_lg()
                                .font_medium()
                                .child(if self.progress.complete {
                                    "No pull request selected"
                                } else {
                                    "Scanning review requests"
                                }),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child("The queue will appear as repositories finish checking"),
                        ),
                )
                .into_any_element();
        };
        let Some(state) = self.details.get(key) else {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .child("Loading pull request")
                .into_any_element();
        };
        match state {
            DetailState::Loading => v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_3()
                .child(Spinner::new().large().color(theme.primary))
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("Loading pull request details"),
                )
                .into_any_element(),
            DetailState::Failed(message) => v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_3()
                .child(
                    Icon::new(IconName::OctagonAlert)
                        .size_8()
                        .text_color(theme.danger),
                )
                .child(
                    div()
                        .text_lg()
                        .font_medium()
                        .child("Could not load pull request"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(message.clone()),
                )
                .into_any_element(),
            DetailState::Loaded(detail) => self.render_loaded_detail(detail, cx).into_any_element(),
        }
    }

    fn render_loaded_detail(&self, detail: &PrDetails, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .size_full()
            .min_w_0()
            .child(self.render_pr_header(detail, cx))
            .child(
                h_flex()
                    .px_4()
                    .border_b_1()
                    .border_color(theme.border)
                    .gap_2()
                    .child(self.detail_tab_button(DetailTab::Conversation, "Conversation", cx))
                    .child(self.detail_tab_button(
                        DetailTab::Files,
                        &format!("Files Changed ({})", detail.files.len()),
                        cx,
                    )),
            )
            .child(
                div()
                    .id("detail-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.details_scroll)
                    .child(match self.detail_tab {
                        DetailTab::Conversation => {
                            self.render_conversation(detail, cx).into_any_element()
                        }
                        DetailTab::Files => self.render_files(detail, cx).into_any_element(),
                    }),
            )
            .child(self.render_review_bar(detail, cx))
    }

    fn render_pr_header(&self, detail: &PrDetails, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let summary = &detail.summary;
        let url = summary.html_url.clone();
        let _link_context = (summary.key.full_repo(), summary.head_ref.clone());
        h_flex()
            .w_full()
            .px_4()
            .py_3()
            .gap_3()
            .items_start()
            .justify_between()
            .border_b_1()
            .border_color(theme.border)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!(
                                "{} · {} → {} · by {}",
                                summary.key.full_repo(),
                                summary.head_ref,
                                summary.base_ref,
                                summary.author
                            )),
                    )
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .child(format!("#{} {}", summary.key.number, summary.title)),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .px_2()
                                    .py_0p5()
                                    .rounded_full()
                                    .text_xs()
                                    .font_medium()
                                    .bg(if detail.checks.failed > 0 {
                                        theme.danger.opacity(0.15)
                                    } else if detail.checks.pending > 0 {
                                        theme.warning.opacity(0.15)
                                    } else if !detail.checks.reported {
                                        theme.muted
                                    } else {
                                        theme.success.opacity(0.15)
                                    })
                                    .text_color(if detail.checks.failed > 0 {
                                        theme.danger
                                    } else if detail.checks.pending > 0 {
                                        theme.warning
                                    } else if !detail.checks.reported {
                                        theme.muted_foreground
                                    } else {
                                        theme.success
                                    })
                                    .child(detail.checks.label()),
                            )
                            .children(summary.draft.then(|| {
                                div()
                                    .px_2()
                                    .py_0p5()
                                    .rounded_full()
                                    .text_xs()
                                    .bg(theme.muted)
                                    .text_color(theme.muted_foreground)
                                    .child("Draft")
                                    .into_any_element()
                            })),
                    ),
            )
            .child(
                Button::new("open-github")
                    .secondary()
                    .small()
                    .icon(Icon::new(IconName::ExternalLink))
                    .label("GitHub")
                    .on_click(move |_, _, cx| {
                        let fallback = url.clone();
                        browser::open(&url, move || cx.open_url(&fallback));
                    }),
            )
    }

    fn detail_tab_button(
        &self,
        tab: DetailTab,
        label: &str,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let active = self.detail_tab == tab;
        div()
            .id(ElementId::Name(format!("detail-tab-{label}").into()))
            .px_3()
            .py_2()
            .cursor_pointer()
            .border_b_2()
            .border_color(if active {
                theme.primary
            } else {
                theme.transparent
            })
            .text_sm()
            .font_medium()
            .text_color(if active {
                theme.foreground
            } else {
                theme.muted_foreground
            })
            .hover(|style| style.text_color(theme.foreground))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.detail_tab = tab;
                cx.notify();
            }))
            .child(label.to_string())
    }

    fn render_conversation(&self, detail: &PrDetails, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .w_full()
            .p_4()
            .gap_3()
            .children(
                detail
                    .discussion
                    .iter()
                    .enumerate()
                    .map(|(ix, entry)| self.discussion_card(ix, entry, detail, cx)),
            )
            .children(detail.discussion.is_empty().then(|| {
                div()
                    .p_6()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("This pull request has no readable conversation")
                    .into_any_element()
            }))
    }

    fn discussion_card(
        &self,
        ix: usize,
        entry: &DiscussionEntry,
        detail: &PrDetails,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let url = entry.html_url.clone();
        let link_context = (
            detail.summary.key.full_repo(),
            detail.summary.head_ref.clone(),
        );
        v_flex()
            .w_full()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .child(
                h_flex()
                    .px_3()
                    .py_2()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme.border)
                    .bg(theme.muted.opacity(0.25))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(theme.foreground)
                                    .child(entry.author.clone()),
                            )
                            .child(div().text_xs().text_color(theme.muted_foreground).child(
                                format!("{} · {}", review_badge(entry.kind), entry.created_at),
                            ))
                            .children(entry.context.as_ref().map(|context| {
                                div()
                                    .text_xs()
                                    .font_family(theme.mono_font_family.clone())
                                    .text_color(theme.info)
                                    .child(context.clone())
                                    .into_any_element()
                            })),
                    )
                    .when(!url.is_empty(), |el| {
                        el.child(
                            Button::new(ElementId::Name(format!("entry-link-{ix}").into()))
                                .ghost()
                                .xsmall()
                                .icon(Icon::new(IconName::ExternalLink))
                                .on_click(move |_, _, cx| {
                                    let fallback = url.clone();
                                    browser::open(&url, move || cx.open_url(&fallback));
                                }),
                        )
                    }),
            )
            .child(
                div().px_3().py_3().child(
                    render::discussion_view(
                        ElementId::Name(
                            format!("entry-body-{}-{ix}", detail.summary.key.display()).into(),
                        ),
                        &entry.body,
                        theme,
                    )
                    .on_link_click(move |url, _, _, cx| {
                        open_rendered_link(url.as_ref(), Some(&link_context), cx)
                    }),
                ),
            )
    }

    fn render_files(&self, detail: &PrDetails, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .w_full()
            .p_4()
            .gap_3()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().font_medium().child(format!(
                        "{} files · +{} −{}",
                        detail.files.len(),
                        detail.files.iter().map(|f| f.additions).sum::<u64>(),
                        detail.files.iter().map(|f| f.deletions).sum::<u64>()
                    )))
                    .when(!detail.files_complete, |el| {
                        el.child(
                            div()
                                .text_xs()
                                .text_color(theme.warning)
                                .child("GitHub's file list is incomplete for this PR"),
                        )
                    }),
            )
            .child(v_flex().gap_1().children(detail.files.iter().map(|file| {
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_xs()
                    .font_family(theme.mono_font_family.clone())
                    .text_color(theme.muted_foreground)
                    .child(format!(
                        "{}  {}  +{} −{}",
                        file.status, file.filename, file.additions, file.deletions
                    ))
            })))
            .child(
                div().h_full().min_h(px(300.)).child(
                    Diff::new(&self.diff_state)
                        .hover_highlight(DiffHoverHighlight::Both)
                        .soft_wrap(false),
                ),
            )
            .children(
                detail
                    .files
                    .iter()
                    .filter(|file| file.patch.is_none())
                    .map(|file| {
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!(
                                "{}: no text patch supplied by GitHub ({} changed lines)",
                                file.filename, file.changes
                            ))
                            .into_any_element()
                    }),
            )
    }

    fn render_review_bar(&self, detail: &PrDetails, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let body = self.composer.read(cx).value().to_string();
        let link_context = (
            detail.summary.key.full_repo(),
            detail.summary.head_ref.clone(),
        );
        let disabled_reason = detail.review_notice.clone().or_else(|| {
            (!detail.can_review || detail.pending_review_exists)
                .then(|| "Review unavailable".to_string())
        });
        v_flex()
            .w_full()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .p_3()
            .gap_2()
            .children(disabled_reason.map(|message| {
                div()
                    .text_xs()
                    .text_color(theme.warning)
                    .child(message)
                    .into_any_element()
            }))
            .children(self.notice.clone().map(|message| {
                div()
                    .text_xs()
                    .text_color(theme.info)
                    .child(message)
                    .into_any_element()
            }))
            .child(if self.composer_preview {
                div()
                    .min_h(px(88.))
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .p_2()
                    .child(
                        TextView::markdown(
                            ElementId::Name("review-preview".into()),
                            if body.trim().is_empty() {
                                "*Preview is empty*"
                            } else {
                                &body
                            },
                        )
                        .style(render::body_style(theme))
                        .on_link_click(move |url, _, _, cx| {
                            open_rendered_link(url.as_ref(), Some(&link_context), cx)
                        }),
                    )
                    .into_any_element()
            } else {
                Editor::new(&self.composer)
                    .h(theme.font_size * 1.5 * body.lines().count().clamp(1, 8) as f32 + px(32.))
                    .font_family(theme.mono_font_family.clone())
                    .text_size(theme.font_size)
                    .into_any_element()
            })
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(
                        Button::new("preview")
                            .ghost()
                            .small()
                            .icon(Icon::new(if self.composer_preview {
                                IconName::NotebookPen
                            } else {
                                IconName::Eye
                            }))
                            .label(if self.composer_preview {
                                "Edit"
                            } else {
                                "Preview"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.composer_preview = !this.composer_preview;
                                cx.notify();
                            })),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                Button::new("approve")
                                    .success()
                                    .icon(Icon::new(IconName::CircleCheck))
                                    .label("Approve")
                                    .loading(self.submitting.is_some())
                                    .disabled(
                                        detail.review_notice.is_some() || self.submitting.is_some(),
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.request_review(ReviewKind::Approve, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("mark-blocked")
                                    .warning()
                                    .icon(Icon::new(IconName::OctagonAlert))
                                    .label("Blocked")
                                    .disabled(
                                        detail.review_notice.is_some() || self.submitting.is_some(),
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.request_review(ReviewKind::Blocked, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("comment")
                                    .info()
                                    .icon(Icon::new(IconName::MessageSquare))
                                    .label("Comment")
                                    .disabled(
                                        detail.review_notice.is_some() || self.submitting.is_some(),
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.request_review(ReviewKind::Comment, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("request-changes")
                                    .danger()
                                    .icon(Icon::new(IconName::SquareX))
                                    .label("Request Changes")
                                    .disabled(
                                        detail.review_notice.is_some() || self.submitting.is_some(),
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.request_review(ReviewKind::RequestChanges, window, cx)
                                    })),
                            ),
                    ),
            )
    }

    fn render_confirmation(
        &self,
        kind: ReviewKind,
        detail: &PrDetails,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let body = self.composer.read(cx).value().to_string();
        div()
            .id("review-confirmation")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.overlay)
            .child(
                v_flex()
                    .w(px(520.))
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.background)
                    .shadow_lg()
                    .p_4()
                    .gap_3()
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .child(format!("Confirm {}", kind.label())),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(format!(
                                "{} as {}\n\n{}",
                                detail.summary.key.display(),
                                detail.viewer_login,
                                if body.trim().is_empty() {
                                    "No comment".into()
                                } else {
                                    body.lines().take(6).collect::<Vec<_>>().join("\n")
                                }
                            )),
                    )
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("cancel-review")
                                    .secondary()
                                    .label("Cancel")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.confirming = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("confirm-review")
                                    .primary()
                                    .label(format!("Submit {} (Ctrl+Enter)", kind.label()))
                                    .loading(self.submitting.is_some())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.submit_review(kind, cx)
                                    })),
                            ),
                    ),
            )
    }
}

impl Render for ApproverView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let confirmation = self.confirming.and_then(|kind| {
            self.selected.as_ref().and_then(|key| {
                self.details.get(key).and_then(|state| match state {
                    DetailState::Loaded(detail) => Some((kind, detail.clone())),
                    _ => None,
                })
            })
        });
        div()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .track_focus(&self.view_focus)
            .capture_key_down(
                cx.listener(|this, event, window, cx| this.nav_key(event, window, cx)),
            )
            .child(
                h_resizable("queue-split")
                    .child(
                        resizable_panel()
                            .size(px(330.))
                            .size_range(px(240.)..px(520.))
                            .fixed()
                            .child(self.render_sidebar(cx)),
                    )
                    .child(resizable_panel().child(self.render_detail(cx))),
            )
            .children(confirmation.map(|(kind, detail)| {
                self.render_confirmation(kind, &detail, cx)
                    .into_any_element()
            }))
    }
}
