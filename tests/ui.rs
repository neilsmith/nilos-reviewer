//! Behavioural tests driving the real view through Rune's headless Runtime.
//! These pin the things snapshots can't: scroll offsets, layout bounds,
//! drags, key handling and state transitions.

use nilos_reviewer::app::{self, App, DetailTab, Pane, Sync};
use nilos_reviewer::github::Event;
use nilos_reviewer::model::{PrKey, QueueRow, ReviewKind};
use nilos_reviewer::view::view;
use rune::{Env, Key, KeyEvent, Mods, Runtime};

fn app() -> App {
    let mut s = App::default();
    s.load_fixture();
    s
}

fn rt(s: App) -> Runtime<App> {
    Runtime::new(s, view, (1280., 800.), Env::default())
}

fn key(k: Key) -> KeyEvent {
    KeyEvent {
        key: k,
        mods: Mods::default(),
    }
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent {
        key: Key::Command(c),
        mods: Mods {
            ctrl: true,
            ..Default::default()
        },
    }
}

fn rect(rt: &mut Runtime<App>, key: &str) -> [f32; 4] {
    let mut out = [0.0; 4];
    rt.update(|s, _| out = s.rects.get(key).copied().unwrap_or_default());
    out
}

// ── Scrolling ────────────────────────────────────────────────────────────

#[test]
fn queue_wheel_scrolls() {
    let mut s = App::default();
    s.load_long_fixture();
    let mut rt = rt(s);
    rt.frame(0.0);
    rt.frame(0.016);
    let before = rect(&mut rt, "row/acme/team-00-100");
    rt.wheel(150.0, 400.0, -400.0);
    rt.frame(0.016);
    rt.frame(0.016);
    let after = rect(&mut rt, "row/acme/team-00-100");
    assert!(
        after[1] < before[1],
        "wheel did not scroll the queue: {before:?} -> {after:?}"
    );
}

// ── Layout bounds ────────────────────────────────────────────────────────

#[test]
fn show_toggles_fit_sidebar() {
    let mut rt = rt(app());
    rt.frame(0.0);
    rt.frame(0.016);
    let sidebar = rect(&mut rt, "sidebar");
    assert_eq!(
        rect(&mut rt, "show-done"),
        [0.0; 4],
        "the Done switch is gone — approved rows hide behind Hide Approved"
    );
    let last = rect(&mut rt, "hide-approved");
    assert!(last[2] > 0.0, "Hide Approved switch has no rect");
    assert!(
        last[0] + last[2] <= sidebar[0] + sidebar[2] + 1.0,
        "Show row overflows sidebar: hide-approved {last:?} vs sidebar {sidebar:?}"
    );
}

#[test]
fn divider_exists_and_resizes() {
    let mut rt = rt(app());
    rt.frame(0.0);
    rt.frame(0.016);
    let div = rect(&mut rt, "divider");
    assert!(div[2] > 0.0, "no resize divider laid out");
    let cx = div[0] + div[2] / 2.0;
    rt.pointer_move(cx, 400.0);
    rt.pointer_down(cx, 400.0);
    rt.pointer_move(cx + 120.0, 400.0);
    rt.frame(0.016);
    rt.pointer_up(cx + 120.0, 400.0);
    rt.update(|s, _| {
        assert!(
            (s.sidebar_w - 460.0).abs() < 6.0,
            "sidebar_w = {}",
            s.sidebar_w
        )
    });
}

// ── Loading state ────────────────────────────────────────────────────────

#[test]
fn scanning_shows_prominent_loading() {
    let s = App {
        sync: Sync::Scanning {
            text: "Searching review requests…".into(),
        },
        ..Default::default()
    };
    let mut rt = rt(s);
    rt.frame(0.0);
    rt.frame(0.016);
    let r = rect(&mut rt, "scan-state");
    assert!(
        r[2] > 0.0 && r[3] > 0.0,
        "no scan-state element while scanning"
    );
}

// ── Keyboard contract ────────────────────────────────────────────────────

#[test]
fn arrows_move_selection() {
    let mut rt = rt(app());
    rt.frame(0.0);
    // Cursor starts on the fixture detail (acme/web#42); Down lands on #39.
    rt.update(|s, tx| assert!(app::on_key(s, &key(Key::Down), tx)));
    rt.update(|s, _| assert_eq!(s.cursor, Some(PrKey::new("acme/web", 39))));
    rt.update(|s, tx| assert!(app::on_key(s, &key(Key::Up), tx)));
    rt.update(|s, _| assert_eq!(s.cursor, Some(PrKey::new("acme/web", 42))));
}

#[test]
fn ctrl_a_opens_confirm_dialog() {
    let mut rt = rt(app());
    rt.frame(0.0);
    rt.update(|s, tx| assert!(app::on_key(s, &ctrl('a'), tx)));
    rt.update(|s, _| assert_eq!(s.confirming, Some(ReviewKind::Approve)));
}

#[test]
fn confirm_submits_and_advances_to_next_row() {
    let mut rt = rt(app());
    rt.frame(0.0);
    rt.update(|s, _tx| s.confirming = Some(ReviewKind::Approve));
    rt.update(|s, tx| s.confirm_review(tx));
    rt.update(|s, _| {
        assert!(s.submitting.contains_key(&PrKey::new("acme/web", 42)));
        // Advance goes DOWN the queue, not back to the top.
        assert_eq!(s.cursor, Some(PrKey::new("acme/web", 39)));
    });
}

#[test]
fn ctrl_b_on_blocked_pr_unblocks() {
    let mut s = app();
    let key = PrKey::new("acme/web", 42);
    s.blocked.insert(key.clone());
    s.cursor = Some(key.clone());
    let mut rt = rt(s);
    rt.frame(0.0);
    rt.update(|s, tx| assert!(app::on_key(s, &ctrl('b'), tx)));
    rt.update(|s, _| assert!(!s.blocked.contains(&key)));
}

// ── Filter and toggles ───────────────────────────────────────────────────

#[test]
fn filter_narrows_the_queue() {
    let mut rt = rt(app());
    rt.frame(0.0);
    rt.update(|s, _| {
        s.filter.text = "rework".into();
        assert_eq!(s.visible_pr_count(), 1);
    });
}

#[test]
fn toggles_reveal_hidden_rows() {
    let mut rt = rt(app());
    rt.frame(0.0);
    rt.update(|s, _| {
        let hidden = s.visible_pr_count();
        s.show_blocked = true;
        assert_eq!(s.visible_pr_count(), hidden + 1);
        s.show_unassigned = true;
        assert_eq!(s.visible_pr_count(), hidden + 2);
    });
}

#[test]
fn hide_approved_defaults_off_and_hides_rows() {
    let mut rt = rt(app());
    rt.frame(0.0);
    rt.update(|s, _| {
        assert!(!s.hide_approved, "Hide Approved defaults to off");
        assert!(s
            .rows()
            .contains(&QueueRow::Pr(PrKey::new("acme/api", 131))));
        s.hide_approved = true;
        assert!(!s
            .rows()
            .contains(&QueueRow::Pr(PrKey::new("acme/api", 131))));
    });
}

#[test]
fn collapse_hides_group_rows() {
    let mut rt = rt(app());
    rt.frame(0.0);
    rt.update(|s, _| {
        let open = s.visible_pr_count();
        s.collapsed.insert("acme/web".into());
        assert_eq!(s.visible_pr_count(), open - 3);
    });
}

#[test]
fn counts_exclude_handled() {
    let mut rt = rt(app());
    rt.frame(0.0);
    rt.update(|s, _| {
        let total = s.pr_count();
        s.handled.insert(PrKey::new("acme/web", 42));
        assert_eq!(s.pr_count(), total - 1);
        assert_eq!(s.visible_pr_count(), total - 3); // handled + blocked + unassigned hidden
    });
}

#[test]
fn approved_prs_are_marked_and_dont_count() {
    let mut rt = rt(app());
    rt.frame(0.0);
    rt.update(|s, _| {
        let approved = PrKey::new("acme/api", 131);
        assert!(
            s.summary(&approved).is_some_and(|p| p.approved),
            "the fixture's already-approved PR must read as approved"
        );
        // 7 queued; approved ones need no attention, like handled ones.
        assert_eq!(s.pr_count(), 6);
        // 5 visible rows (blocked + unassigned hidden) minus the approved one.
        assert_eq!(s.visible_pr_count(), 4);
    });
}

#[test]
fn approve_submit_marks_pr_approved() {
    let mut rt = rt(app());
    rt.frame(0.0);
    rt.update(|s, tx| {
        s.on_event(
            Event::ReviewSubmitted {
                key: PrKey::new("acme/web", 47),
                kind: ReviewKind::Approve,
                labelled: None,
            },
            tx,
        );
        assert!(s.summary(&PrKey::new("acme/web", 47)).unwrap().approved);
        s.on_event(
            Event::ReviewSubmitted {
                key: PrKey::new("acme/docs", 9),
                kind: ReviewKind::Comment,
                labelled: None,
            },
            tx,
        );
        assert!(
            !s.summary(&PrKey::new("acme/docs", 9)).unwrap().approved,
            "only approvals mark the row approved"
        );
    });
}

#[test]
fn confirm_advances_past_approved_rows() {
    let mut rt = rt(app());
    rt.frame(0.0);
    rt.update(|s, tx| s.select(Some(PrKey::new("acme/docs", 9)), tx));
    rt.update(|s, tx| {
        s.confirming = Some(ReviewKind::Approve);
        s.confirm_review(tx);
    });
    rt.update(|s, _| {
        // docs#9 submits; api#131 is approved → advance wraps to acme/web#47.
        assert_eq!(s.cursor, Some(PrKey::new("acme/web", 47)));
    });
}

// ── Review flow state ────────────────────────────────────────────────────

#[test]
fn unblock_is_optimistic() {
    let mut s = app();
    let key = PrKey::new("acme/web", 42);
    s.blocked.insert(key.clone());
    s.cursor = Some(key.clone());
    if let Some(p) = s.all_prs.iter_mut().find(|p| p.key == key) {
        p.blocked_label = true;
    }
    let mut rt = rt(s);
    rt.frame(0.0);
    rt.update(|s, _| {
        assert!(s.selected_blocked());
        s.unblock_selected();
        assert!(!s.selected_blocked());
        assert!(!s.all_prs.iter().any(|p| p.key == key && p.blocked_label));
    });
}

#[test]
fn detail_tab_defaults_and_switches() {
    let mut rt = rt(app());
    rt.frame(0.0);
    rt.update(|s, _| {
        assert_eq!(s.detail_tab, DetailTab::Conversation);
        s.detail_tab = DetailTab::Files;
        assert_eq!(s.detail_tab, DetailTab::Files);
    });
}

// ── Narrow layout ────────────────────────────────────────────────────────

#[test]
fn narrow_select_opens_detail_and_escape_returns() {
    let mut s = app();
    s.window = [640.0, 800.0];
    let mut rt = Runtime::new(s, view, (640., 800.), Env::default());
    rt.frame(0.0);
    rt.frame(0.016);
    rt.update(|s, _| {
        assert!(s.narrow());
        s.pane = Pane::Queue;
    });
    rt.update(|s, tx| s.select(Some(PrKey::new("acme/docs", 9)), tx));
    rt.update(|s, _| assert_eq!(s.pane, Pane::Detail));
    rt.update(|s, tx| assert!(app::on_key(s, &key(Key::Escape), tx)));
    rt.update(|s, _| assert_eq!(s.pane, Pane::Queue));
}

#[test]
fn unblock_failure_restores_blocked() {
    let mut rt = rt(app());
    rt.frame(0.016);
    let mut key = None;
    rt.update(|s, _| key = s.all_prs.first().map(|p| p.key.clone()));
    let key = key.unwrap();
    rt.update(|s, tx| {
        s.on_event(
            Event::UnblockFailed {
                key: key.clone(),
                error: "nope".into(),
            },
            tx,
        );
    });
    rt.frame(0.016);
    rt.update(move |s, _| {
        assert!(
            s.blocked.contains(&key),
            "failed unblock must restore blocked set"
        );
        assert!(s.review_error.is_some(), "failure should surface an error");
        let p = s.all_prs.iter().find(|p| p.key == key).unwrap();
        assert!(p.blocked_label, "summary flag restored");
    });
}

#[test]
fn review_failure_surfaces_error_and_draft() {
    let mut rt = rt(app());
    rt.frame(0.016);
    let mut key = None;
    rt.update(|s, _| key = s.all_prs.first().map(|p| p.key.clone()));
    let key = key.unwrap();
    rt.update(|s, _| {
        s.submitting.insert(key.clone(), ReviewKind::Approve);
        s.comment.text = "keep this".into();
    });
    rt.update(|s, tx| {
        s.on_event(
            Event::ReviewFailed {
                key: key.clone(),
                error: "422".into(),
            },
            tx,
        );
    });
    rt.frame(0.016);
    rt.update(move |s, _| {
        assert!(
            !s.submitting.contains_key(&key),
            "submitting cleared on failure"
        );
        assert!(
            s.review_error.is_some(),
            "review_error drives the error modal"
        );
        assert_eq!(s.comment.text, "keep this", "draft preserved for retry");
    });
}

#[test]
fn blocked_marks_expire_on_the_app_clock() {
    let mut s = app();
    s.scan_generation = 0;
    let key = s.all_prs[0].key.clone();
    s.blocked.insert(key.clone());
    s.blocked_since.insert(key.clone(), 900.0);
    let mut rt = rt(s);
    // Inside the two-minute grace the local mark survives a scan reconcile.
    rt.update(|s, tx| {
        s.now.set(1000.0);
        s.on_event(Event::SyncFinished { gen: 0 }, tx);
        assert!(s.blocked.contains(&key), "mark dropped inside grace");
    });
    // Past it, only a server-side label would keep the mark — this PR has none.
    rt.update(|s, tx| {
        s.now.set(1201.0);
        s.on_event(Event::SyncFinished { gen: 0 }, tx);
        assert!(!s.blocked.contains(&key), "stale mark kept past grace");
    });
}

#[test]
fn synced_label_ages() {
    let mut s = app();
    s.last_synced = Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64() - 100.0)
            .unwrap_or(0.0),
    );
    s.sync = Sync::Done;
    let mut rt = rt(s);
    rt.update(|s, tx| {
        s.poll(0.0, tx);
        assert_eq!(s.synced_label, "updated 2m ago");
    });
}

#[test]
fn theme_watch_flag_reloads_palette() {
    let mut s = app();
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    s.theme_changed = Some(flag.clone());
    // A stale palette forces an observable reload whether or not this host
    // runs Omarchy.
    s.omarchy = Some(nilos_reviewer::theme::OmarchyPalette {
        mode: Some("bogus".into()),
        ..Default::default()
    });
    let mut rt = rt(s);
    rt.update(|s, tx| {
        s.poll(0.0, tx);
        assert!(
            !flag.load(std::sync::atomic::Ordering::SeqCst),
            "the poll left the change flag set"
        );
        assert_eq!(
            s.omarchy,
            nilos_reviewer::theme::current_palette(),
            "the palette did not reload from disk"
        );
    });
}

#[test]
fn update_check_result_shows_chip() {
    let mut s = app();
    s.update
        .fake_check(Ok(Some(nilos_reviewer::update::Release {
            version: "9.9.9".into(),
            url: None,
        })));
    let mut rt = rt(s);
    rt.frame(0.016);
    rt.update(|s, tx| {
        assert!(
            s.poll(0.0, tx),
            "a landed update check must paint — an idle window draws nothing"
        );
        assert_eq!(
            s.update.available.as_ref().map(|r| r.version.as_str()),
            Some("9.9.9")
        );
    });
    rt.frame(0.016);
    rt.frame(0.016);
    assert!(rect(&mut rt, "update-chip")[2] > 0.0, "no footer chip");
}

#[test]
fn update_apply_error_shows_notice() {
    let mut s = app();
    s.update.fake_apply(Err("disk full".into()));
    let mut rt = rt(s);
    rt.frame(0.016);
    rt.update(|s, tx| {
        assert!(s.poll(0.0, tx));
        assert!(
            s.notice.as_deref().is_some_and(|n| n.contains("disk full")),
            "apply failure must surface as a notice, got {:?}",
            s.notice
        );
        assert!(!s.update.applying(), "the apply job is spent");
    });
}

#[test]
fn update_apply_done_relaunches() {
    let mut s = app();
    // A path that cannot spawn: the relaunch failure must surface the notice
    // (not die silently, and not spawn the test binary again).
    s.exe_path = "/nonexistent/nilos-reviewer".into();
    s.update.fake_apply(Ok(()));
    let mut rt = rt(s);
    rt.frame(0.016);
    rt.update(|s, tx| {
        s.poll(0.0, tx);
        assert_eq!(
            s.notice.as_deref(),
            Some("Couldn't relaunch — quit and reopen to run the update"),
            "the finished apply did not reach restart"
        );
    });
}

#[test]
fn update_chip_click_updates_or_opens_page() {
    let mut s = app();
    s.update.available = Some(nilos_reviewer::update::Release {
        version: "9.9.9".into(),
        url: Some("https://example.com/asset.tar.gz".into()),
    });
    let mut rt = rt(s);
    rt.frame(0.0);
    rt.frame(0.016);
    rt.frame(0.016);
    let r = rect(&mut rt, "update-chip");
    assert!(r[2] > 0.0, "no update chip laid out");
    let (cx, cy) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    rt.pointer_move(cx, cy);
    rt.pointer_down(cx, cy);
    rt.pointer_up(cx, cy);
    rt.frame(0.016);
    // The test binary is a dev build: the click must NOT start a self-update
    // (it falls back to opening the release page).
    rt.update(|s, _| {
        assert!(!s.update.applying(), "a dev build started a self-update");
    });
}
