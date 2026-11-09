//! The view: title bar, queue sidebar, detail pane, composer, status bar and
//! dialogs. Pure `El` builders over `App` state; handlers mutate `App`.

use crate::fx::GLOW;
use rune::color::hex;
use rune::{col, el, icon, row, text, Color, Cx, El, Icon, ScrollSpec, Tx};
use rune_kit::{
    diff::DiffView, root, Badge, BadgeVariant, Button, ButtonVariant, ControlSize, Dialog,
    IconButton, IconButtonVariant, Kit, Markdown, SegmentedControl, Spinner, Switch, TextArea,
    TextInput,
};

use crate::app::{App, Appearance, DetailTab, Pane, Sync};
use crate::model::{
    age_of, normalize_body, Discussion, DiscussionKind, PrSummary, QueueRow, ReviewKind,
};

// ── Icons ────────────────────────────────────────────────────────────────

static PR_ICON: Icon = Icon {
    name: "pull-request",
    view: 16.0,
    body: r#"<circle cx="4.5" cy="3.5" r="1.75" stroke="currentColor" stroke-width="1.25" fill="none"/><circle cx="4.5" cy="12.5" r="1.75" stroke="currentColor" stroke-width="1.25" fill="none"/><circle cx="11.5" cy="12.5" r="1.75" stroke="currentColor" stroke-width="1.25" fill="none"/><path d="M4.5 5.25v5.5M11.5 10.75v-4a2 2 0 0 0-2-2H7" stroke="currentColor" stroke-width="1.25" fill="none"/>"#,
};

static REFRESH: Icon = Icon {
    name: "refresh",
    view: 16.0,
    body: r#"<path d="M13.5 8a5.5 5.5 0 1 1-1.61-3.89M13.5 1.5v3h-3" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#,
};

static EXTERNAL: Icon = Icon {
    name: "external",
    view: 16.0,
    body: r#"<path d="M9.5 3h3.5v3.5M13 3L7.5 8.5M11 9.5V12a1.5 1.5 0 0 1-1.5 1.5h-5A1.5 1.5 0 0 1 3 12V6.5A1.5 1.5 0 0 1 4.5 5H7" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#,
};

static CIRCLE_CHECK: Icon = Icon {
    name: "circle-check",
    view: 16.0,
    body: r#"<circle cx="8" cy="8" r="6" stroke="currentColor" stroke-width="1.25" fill="none"/><path d="M5.5 8.2l1.8 1.8 3.4-3.6" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#,
};

static OCTAGON: Icon = Icon {
    name: "octagon",
    view: 16.0,
    body: r#"<path d="M5.2 2h5.6L13.9 5.2v5.6L10.8 14H5.2L2.1 10.8V5.2z" stroke="currentColor" stroke-width="1.25" stroke-linejoin="round" fill="none"/><path d="M8 5v3.5M8 11v.01" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#,
};

static BACK: Icon = Icon {
    name: "back",
    view: 16.0,
    body: r#"<path d="M10 3L5 8l5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#,
};

static FILTER: Icon = Icon {
    name: "filter",
    view: 16.0,
    body: r#"<circle cx="7" cy="7" r="4.5" stroke="currentColor" stroke-width="1.4" fill="none"/><path d="M10.5 10.5L14 14" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>"#,
};

// ── View root ────────────────────────────────────────────────────────────

const HAIR: f32 = 0.5;

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let dark = s.dark(cx.system_dark());
    let k = s.kit(dark);
    cx.foreground(k.c.text_base);
    cx.focus_ring(2.0, 2.0, k.c.border_focus);

    let mut overlays = Vec::new();
    confirm_dialog(s, &k, &mut overlays);
    error_dialog(s, &k, &mut overlays);

    let narrow = s.narrow();
    let body = if narrow {
        match s.pane {
            Pane::Queue => queue_pane(s, &k).grow(1.0).min_w(0.0),
            Pane::Detail => detail_pane(s, &k, true).grow(1.0).min_w(0.0),
        }
    } else {
        row()
            .grow(1.0)
            .min_h(0.0)
            .child(
                queue_pane(s, &k)
                    .w(s.sidebar_w)
                    .border_sides([0.0, 0.0, 0.0, HAIR], k.c.border_muted),
            )
            .child(
                el().key("divider")
                    .on_layout(track("divider"))
                    .w(5.0)
                    .h_full()
                    .fill(k.alpha(k.c.text_base, 0.045))
                    .hover(k.alpha(k.c.text_base, 0.14))
                    .cursor(rune::Cursor::ResizeColumn)
                    .hover(k.alpha(k.c.bg_accent, 0.3))
                    .on_drag_start(|s: &mut App, _tx| s.resize_from = s.sidebar_w)
                    .on_drag(|s: &mut App, drag, _tx| {
                        s.sidebar_w =
                            (s.resize_from + drag.dx).clamp(220.0, (s.window[0] * 0.6).max(300.0));
                    }),
            )
            .child(detail_pane(s, &k, false).grow(1.0).min_w(0.0))
    };

    root(
        col()
            .key("window")
            .role("window")
            .w_full()
            .h_full()
            .fill(k.c.page)
            .on_layout(|s: &mut App, r: [f32; 4]| s.window = [r[2], r[3]])
            .child(titlebar(s, &k, cx.chrome_insets().leading))
            .child(body)
            .child(statusbar(s, &k)),
        overlays,
    )
}

// ── Title bar ────────────────────────────────────────────────────────────

fn titlebar(s: &App, k: &Kit, leading_inset: f32) -> El {
    let scanning = matches!(s.sync, Sync::Scanning { .. });
    row()
        .key("titlebar")
        .h(40.0)
        .w_full()
        .pl(leading_inset + 12.0)
        .pr(12.0)
        .window_drag()
        .center_y()
        .gap(8.0)
        .border_sides([0.0, 0.0, HAIR, 0.0], k.c.border_muted)
        .child(icon(&PR_ICON, 20.0).color(k.c.text_accent))
        .child(
            text("Nilos")
                .font(rune::TextStyle {
                    size: 17.0,
                    tracking: -0.3,
                    ..k.ty.ui_medium
                })
                .color(k.c.text_base),
        )
        .child(
            text("Reviewer")
                .font(rune::TextStyle {
                    size: 17.0,
                    tracking: -0.3,
                    ..k.ty.ui_medium
                })
                .color(k.c.text_accent),
        )
        .child(if s.viewer.is_empty() {
            text("")
        } else {
            text(format!("@{}", s.viewer))
                .font(k.ty.small)
                .color(k.c.text_faint)
        })
        .child(el().grow(1.0))
        .child(appearance_button(s, k))
        .child(
            IconButton::new(&REFRESH)
                .label("Refresh")
                .variant(IconButtonVariant::Ghost)
                .control_size(ControlSize::Regular)
                .disabled(scanning)
                .help("Refresh the review queue (F5)")
                .on_press(|s: &mut App| s.refresh())
                .build(k),
        )
}

fn appearance_button(s: &App, k: &Kit) -> El {
    let (label, next) = match s.appearance {
        Appearance::Auto => ("Auto", Appearance::Light),
        Appearance::Light => ("Light", Appearance::Dark),
        Appearance::Dark => ("Dark", Appearance::Auto),
    };
    Button::new(label)
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .help("Appearance: cycle Auto → Light → Dark")
        .on_press_tx(move |s: &mut App, tx| {
            s.appearance = next;
            tx.color_motion(rune::tok::motion::CALM);
        })
        .build(k)
}

// ── Queue pane ───────────────────────────────────────────────────────────

fn queue_pane(s: &App, k: &Kit) -> El {
    let mut list = col()
        .key("queue")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        });
    let rows = s.rows();
    if let Sync::Scanning { text: stage } = &s.sync {
        if rows.is_empty() {
            list = list.child(
                col()
                    .key("scan-state")
                    .on_layout(track("scan-state"))
                    .w_full()
                    .center()
                    .py(48.0)
                    .child(text(stage.clone()).font(k.ty.small).color(k.c.text_muted)),
            );
        }
    } else if rows.is_empty() {
        list = list.child(
            col()
                .w_full()
                .center()
                .py(32.0)
                .gap(6.0)
                .child(icon(&CIRCLE_CHECK, 22.0).color(k.c.success.fg))
                .child(
                    text("Queue’s clear — go write some code")
                        .font(k.ty.ui_medium)
                        .color(k.c.text_base),
                )
                .child(
                    text("No pull requests need your review")
                        .font(k.ty.small)
                        .color(k.c.text_muted),
                ),
        );
    }
    for row_el in rows {
        match row_el {
            QueueRow::Header(repo) => list = list.child(group_header(s, k, &repo)),
            QueueRow::Pr(key) => {
                if let Some(pr) = s.summary(&key) {
                    list = list.child(pr_row(s, k, pr));
                }
            }
        }
    }

    col()
        .key("sidebar")
        .w_full()
        .h_full()
        .min_h(0.0)
        .on_layout(track("sidebar"))
        .child(
            col().w_full().padding(8.0).child(
                col()
                    .w_full()
                    .radius(k.radius.md)
                    .fill(k.c.bg_deep)
                    .outline(1.0, 0.0, k.c.text_muted)
                    .child(
                        TextInput::new(&s.filter)
                            .placeholder("Filter pull requests  ( / )")
                            .leading_icon(&FILTER)
                            .width(None)
                            .on_edit(|s: &mut App| &mut s.filter)
                            .build(k)
                            .key("filter"),
                    ),
            ),
        )
        .child(show_toggles(s, k))
        .child(list)
}

/// All queue-visibility switches in one place.
fn show_toggles(s: &App, k: &Kit) -> El {
    let toggle = |label: &'static str, on: bool, aria: &'static str, f: fn(&mut App)| {
        (
            text(label).font(k.ty.small).color(k.c.text_muted),
            Switch::new(on)
                .label(aria)
                .hide_label()
                .on_toggle(f)
                .build(k),
        )
    };
    let (ut, us) = toggle(
        "Unassigned",
        s.show_unassigned,
        "Show unassigned pull requests",
        |s| s.show_unassigned = !s.show_unassigned,
    );
    let (bt, bs) = toggle(
        "Blocked",
        s.show_blocked,
        "Show blocked pull requests",
        |s| s.show_blocked = !s.show_blocked,
    );
    let (dt, ds) = toggle("Done", !s.hide_done, "Show reviewed pull requests", |s| {
        s.hide_done = !s.hide_done
    });
    let ds = ds.on_layout(track("show-done"));
    row()
        .w_full()
        .px(10.0)
        .pb(6.0)
        .gap(6.0)
        .center_y()
        .key("show-toggles")
        .child(ut)
        .child(us)
        .child(bt)
        .child(bs)
        .child(dt)
        .child(ds.key("show-done"))
}

fn group_header(s: &App, k: &Kit, repo: &str) -> El {
    let collapsed = s.collapsed.contains(repo);
    let count = s
        .groups
        .iter()
        .find(|g| g.repo == repo)
        .map(|g| g.prs.len())
        .unwrap_or(0);
    let repo = repo.to_string();
    row()
        .key(format!("gh-{repo}"))
        .w_full()
        .h(30.0)
        .px(8.0)
        .gap(6.0)
        .center_y()
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(format!("{repo}, {count} pull requests"))
        .focusable(true)
        .hover(k.c.hover)
        .on_press({
            let repo = repo.clone();
            move |s: &mut App| {
                if !s.collapsed.remove(&repo) {
                    s.collapsed.insert(repo.clone());
                }
            }
        })
        .child(
            icon(
                if collapsed {
                    k.glyph.triangle_right
                } else {
                    k.glyph.triangle_down
                },
                10.0,
            )
            .color(k.c.text_muted),
        )
        .child(
            text(repo)
                .font(k.ty.small_medium)
                .color(k.c.text_muted)
                .truncate()
                .grow(1.0),
        )
        .child(
            Badge::new(format!("{count}"))
                .variant(BadgeVariant::Subtle)
                .build(k),
        )
}

fn pr_row(s: &App, k: &Kit, pr: &PrSummary) -> El {
    let key = pr.key.clone();
    let selected = s.cursor.as_ref() == Some(&key);
    let submitting = s.submitting.get(&key).copied();
    let handled = s.handled.contains(&key);
    let blocked = pr.is_blocked(&s.blocked);
    let age = age_of(&pr.created_at, now_secs());

    let mut meta = row().gap(6.0).center_y().child(
        text(format!("#{} · {} · {age}", pr.key.number, pr.author))
            .font(k.ty.small)
            .color(k.c.text_muted)
            .truncate(),
    );
    for reason in &pr.reasons {
        meta = meta.child(
            Badge::new(reason.clone())
                .variant(if reason == "You" {
                    BadgeVariant::Accent
                } else {
                    BadgeVariant::Neutral
                })
                .build(k),
        );
    }
    if pr.draft {
        meta = meta.child(Badge::new("Draft").variant(BadgeVariant::Subtle).build(k));
    }

    let mut status = row().gap(4.0).center_y();
    if let Some(kind) = submitting {
        status = status
            .child(Spinner::new().build(k))
            .child(text(kind.name()).font(k.ty.small).color(k.c.text_muted));
    } else if blocked {
        status = status.child(icon(&OCTAGON, 13.0).color(k.c.warning.fg));
    } else if handled {
        status = status.child(icon(k.glyph.check, 13.0).color(k.c.success.fg));
    }

    col()
        .key(App::row_key(&key))
        .on_layout({
            let k = key.clone();
            move |s: &mut App, r: [f32; 4]| {
                s.rects.insert(format!("row/{}-{}", k.repo, k.number), r);
            }
        })
        .w_full()
        .px(10.0)
        .py(7.0)
        .gap(3.0)
        .cursor(rune::Cursor::Pointer)
        .role("row")
        .label(format!("{} #{}", pr.title, pr.key.number))
        .checked(selected)
        .focusable(true)
        .fill(if selected {
            k.alpha(k.c.bg_accent, 0.14)
        } else {
            Color::TRANSPARENT
        })
        .hover(k.c.hover)
        .on_press_tx({
            let key = key.clone();
            move |s: &mut App, tx| s.select(Some(key.clone()), tx)
        })
        .on_secondary_press({
            let url = pr.url.clone();
            move |_: &mut App, _point, tx: &mut Tx| {
                if !url.is_empty() {
                    let _ = tx.open_url(&url);
                }
            }
        })
        .child(
            row()
                .w_full()
                .gap(6.0)
                .center_y()
                .child(
                    text(pr.title.clone())
                        .font(k.ty.ui_compact)
                        .color(if handled || blocked {
                            k.c.text_muted
                        } else {
                            k.c.text_base
                        })
                        .truncate()
                        .grow(1.0),
                )
                .child(status),
        )
        .child(meta)
}

fn track(key: &str) -> impl Fn(&mut App, [f32; 4]) {
    let k = key.to_string();
    move |s: &mut App, r: [f32; 4]| {
        s.rects.insert(k.clone(), r);
    }
}

// ── Detail pane ──────────────────────────────────────────────────────────

fn detail_pane(s: &App, k: &Kit, narrow: bool) -> El {
    let mut pane = col()
        .key("detail-pane")
        .w_full()
        .h_full()
        .min_h(0.0)
        .min_w(0.0);
    if narrow {
        pane = pane.child(
            row()
                .h(36.0)
                .w_full()
                .px(8.0)
                .center_y()
                .border_sides([0.0, 0.0, HAIR, 0.0], k.c.border_muted)
                .child(
                    IconButton::new(&BACK)
                        .label("Back to the queue")
                        .variant(IconButtonVariant::Ghost)
                        .on_press(|s: &mut App| {
                            s.pane = Pane::Queue;
                            s.pane_by_user = true;
                        })
                        .build(k),
                ),
        );
    }

    match &s.detail {
        Some(detail) => {
            let files_label = format!("Files {}", detail.changed_files);
            let tabs = row().w_full().px(16.0).pt(8.0).child(
                SegmentedControl::new(
                    &["Conversation", files_label.as_str()],
                    Some(match s.detail_tab {
                        DetailTab::Conversation => 0,
                        DetailTab::Files => 1,
                    }),
                )
                .fit()
                .on_pick(|s: &mut App, i, _tx| {
                    s.detail_tab = if i == 0 {
                        DetailTab::Conversation
                    } else {
                        DetailTab::Files
                    };
                })
                .build(k),
            );
            match s.detail_tab {
                DetailTab::Conversation => {
                    let scroll = col()
                        .key("detail")
                        .w_full()
                        .grow(1.0)
                        .min_h(0.0)
                        .scroll(ScrollSpec {
                            stick_bottom: false,
                        })
                        .child(detail_header(s, k))
                        .child(tabs)
                        .child(conversation_section(s, k));
                    pane = pane.child(scroll);
                }
                DetailTab::Files => {
                    pane = pane
                        .child(col().w_full().child(detail_header(s, k)).child(tabs))
                        .child(
                            col()
                                .key("detail")
                                .w_full()
                                .grow(1.0)
                                .min_h(0.0)
                                .child(files_section(s, k)),
                        );
                }
            }
            pane = pane.child(composer(s, k));
        }
        None => {
            let mut empty = col().w_full().grow(1.0).center().gap(8.0);
            if let Sync::Scanning { text: stage } = &s.sync {
                empty = empty
                    .child(
                        el().size(56.0, 56.0)
                            .radius(16.0)
                            .effect(GLOW)
                            .effect_data([0.0, 1.0, 0.45, 13.0])
                            .center()
                            .child(icon(&PR_ICON, 24.0).color(k.c.text_base)),
                    )
                    .child(text(stage.clone()).font(k.ty.ui).color(k.c.text_muted));
            } else if s.detail_loading.is_some() {
                empty = empty
                    .child(
                        el().size(56.0, 56.0)
                            .radius(16.0)
                            .effect(GLOW)
                            .effect_data([0.0, 1.0, 0.45, 13.0])
                            .center()
                            .child(icon(&PR_ICON, 24.0).color(k.c.text_base)),
                    )
                    .child(
                        text("Loading pull request…")
                            .font(k.ty.ui)
                            .color(k.c.text_muted),
                    );
            } else {
                empty = empty
                    .child(icon(&PR_ICON, 28.0).color(k.c.text_faint))
                    .child(
                        text("Select a pull request")
                            .font(k.ty.ui_medium)
                            .color(k.c.text_muted),
                    )
                    .child(
                        text("↑ ↓ to move · Enter to select · / to filter")
                            .font(k.ty.small)
                            .color(k.c.text_faint),
                    );
            }
            pane = pane.child(empty);
        }
    }
    pane
}

fn detail_header(s: &App, k: &Kit) -> El {
    let d = s.detail.as_ref().unwrap();
    let url = d.url.clone();
    col()
        .w_full()
        .px(16.0)
        .pt(14.0)
        .pb(10.0)
        .gap(8.0)
        .border_sides([0.0, 0.0, HAIR, 0.0], k.c.border_muted)
        .child(
            row()
                .w_full()
                .gap(8.0)
                .center_y()
                .child(
                    text(d.title.clone())
                        .font(k.ty.body_medium.lh(22.0))
                        .color(k.c.text_base)
                        .grow(1.0),
                )
                .child(
                    IconButton::new(&EXTERNAL)
                        .label("Open on GitHub")
                        .variant(IconButtonVariant::Ghost)
                        .help("Open in the browser")
                        .on_press_tx(move |_: &mut App, tx| {
                            let _ = tx.open_url(&url);
                        })
                        .build(k),
                ),
        )
        .child(
            row()
                .w_full()
                .gap(8.0)
                .center_y()
                .flex_wrap()
                .child(
                    text(format!("{}#{}", d.key.repo, d.key.number))
                        .font(k.ty.ui_compact)
                        .color(k.c.text_accent),
                )
                .child(
                    text(format!("by {}", d.author))
                        .font(k.ty.ui_compact)
                        .color(k.c.text_muted),
                )
                .child(
                    text(format!("{} → {}", d.head_ref, d.base_ref))
                        .font(k.ty.mono)
                        .color(k.c.text_muted),
                )
                .child(if d.draft {
                    Badge::new("Draft").variant(BadgeVariant::Subtle).build(k)
                } else {
                    text("")
                })
                .child(checks_badge(s, k))
                .child(
                    text(format!(
                        "+{} −{} · {} files",
                        d.additions, d.deletions, d.changed_files
                    ))
                    .font(k.ty.small)
                    .color(k.c.text_faint),
                ),
        )
}

fn checks_badge(s: &App, k: &Kit) -> El {
    let d = s.detail.as_ref().unwrap();
    if d.checks.is_empty() {
        return text("");
    }
    let (passing, failing, pending) =
        d.checks
            .iter()
            .fold((0, 0, 0), |(p, f, q), c| match c.ok() {
                Some(true) => (p + 1, f, q),
                Some(false) => (p, f + 1, q),
                None => (p, f, q + 1),
            });
    let (label, color) = if failing > 0 {
        (format!("✗ {failing} failing"), k.c.danger.fg)
    } else if pending > 0 {
        (format!("● {pending} pending"), k.c.warning.fg)
    } else {
        (format!("✓ {passing} passing"), k.c.success.fg)
    };
    row().gap(4.0).center_y().child(
        text(label).font(k.ty.small_medium).color(color).help(
            d.checks
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    )
}

/// The Conversation tab: the PR body, then the discussion oldest-first.
fn conversation_section(s: &App, k: &Kit) -> El {
    let d = s.detail.as_ref().unwrap();
    let mut section = col().w_full().px(16.0).py(12.0).gap(12.0);
    if !d.body.trim().is_empty() {
        section = section.child(discussion_card(
            k,
            &d.author,
            "opened this pull request",
            None,
            normalize_body(&d.body),
            k.c.border_muted,
        ));
    }
    for event in &d.events {
        section = section.child(discussion_event(k, event));
    }
    if d.body.trim().is_empty() && d.events.is_empty() {
        section = section.child(
            text("No description or discussion yet")
                .font(k.ty.ui)
                .color(k.c.text_muted),
        );
    }
    section
}

/// One discussion entry: author line, kind badge, body markdown.
fn discussion_event(k: &Kit, event: &Discussion) -> El {
    let (action, badge) = match &event.kind {
        DiscussionKind::Comment => ("commented", None),
        DiscussionKind::Review(state) => (
            match state.as_str() {
                "APPROVED" => "approved",
                "CHANGES_REQUESTED" => "requested changes",
                "DISMISSED" => "dismissed a review",
                _ => "reviewed",
            },
            Some(match state.as_str() {
                "APPROVED" => ("Approved".to_string(), k.c.success.fg),
                "CHANGES_REQUESTED" => ("Changes requested".to_string(), k.c.danger.fg),
                s => (s.to_string(), k.c.text_muted),
            }),
        ),
        DiscussionKind::ReviewComment { path, line } => {
            let loc = line
                .map(|l| format!("{path}:{l}"))
                .unwrap_or_else(|| path.clone());
            ("commented on", Some((loc, k.c.text_muted)))
        }
    };
    let edge = match &event.kind {
        DiscussionKind::Comment => k.c.border_muted,
        DiscussionKind::Review(s) => match s.as_str() {
            "APPROVED" => k.c.success.fg,
            "CHANGES_REQUESTED" => k.c.danger.fg,
            _ => k.c.text_muted,
        },
        DiscussionKind::ReviewComment { .. } => k.c.text_accent,
    };
    discussion_card(
        k,
        &event.author,
        action,
        badge,
        normalize_body(&event.body),
        edge,
    )
}

/// A discussion card: `author action · badge`, then the body.
fn discussion_card(
    k: &Kit,
    author: &str,
    action: &str,
    badge: Option<(String, Color)>,
    body: String,
    edge: Color,
) -> El {
    let mut head = row()
        .gap(6.0)
        .center_y()
        .child(
            text(author.to_string())
                .font(k.ty.ui_compact)
                .color(k.c.text_base),
        )
        .child(
            text(action.to_string())
                .font(k.ty.small)
                .color(k.c.text_muted),
        );
    if let Some((label, color)) = badge {
        head = head.child(text(label).font(k.ty.small_medium).color(color));
    }
    let mut card = col()
        .w_full()
        .padding(12.0)
        .gap(8.0)
        .child(el().absolute(0.0, 0.0).w(3.0).h_full().fill(edge))
        .fill(k.c.layer_01)
        .radius(k.radius.lg)
        .border(1.0, k.c.border_muted)
        .child(head);
    if !body.trim().is_empty() {
        card = card.child(Markdown::new(body).build(k));
    }
    card
}

fn files_section(s: &App, k: &Kit) -> El {
    DiffView::new(|s: &App| &s.diff, |s: &mut App| &mut s.diff)
        .key("diff")
        .build(s, k)
        .grow(1.0)
        .min_h(0.0)
        .w_full()
}

// ── Composer ─────────────────────────────────────────────────────────────

fn composer(s: &App, k: &Kit) -> El {
    col()
        .key("composer")
        .w_full()
        .px(12.0)
        .pt(10.0)
        .pb(10.0)
        .gap(8.0)
        .border_sides([HAIR, 0.0, 0.0, 0.0], k.c.border_muted)
        .child(
            col()
                .w_full()
                .radius(k.radius.md)
                .fill(k.c.bg_deep)
                .outline(1.0, 0.0, k.c.text_muted)
                .child(
                    TextArea::new(&s.comment)
                        .placeholder("Leave a comment (required for Comment and Blocked)")
                        .width(None)
                        .auto_grow(3, 8)
                        .on_edit(|s: &mut App| &mut s.comment)
                        .build(k),
                ),
        )
        .child(
            row().w_full().justify_end().child(
                text("⌃A approve · ⌃C comment · ⌃R changes · ⌃B block")
                    .font(k.ty.small)
                    .color(k.c.text_faint),
            ),
        )
}

/// A filled action button — reads as a button, not a badge.
#[allow(clippy::too_many_arguments)]
fn action_button(
    _s: &App,
    k: &Kit,
    label: &'static str,
    bg: Color,
    hover_bg: Color,
    fg: Color,
    border: Color,
    disabled: bool,
    press: impl Fn(&mut App, &mut rune::Tx) + 'static,
) -> El {
    col()
        .px(14.0)
        .h(32.0)
        .radius(8.0)
        .fill(if disabled {
            k.alpha(k.c.text_base, 0.06)
        } else {
            bg
        })
        .hover(hover_bg)
        .border(1.0, if disabled { k.c.border_muted } else { border })
        .cursor(if disabled {
            rune::Cursor::Default
        } else {
            rune::Cursor::Pointer
        })
        .on_press_tx(move |s: &mut App, tx| {
            if !disabled {
                press(s, tx);
            }
        })
        .center()
        .child(
            text(label)
                .font(k.ty.ui_medium)
                .color(if disabled { k.c.text_faint } else { fg }),
        )
}

/// (bg, hover, fg, border) for a review action — the same in the composer
/// and the confirm dialog so they always read as one control.
fn action_colors(k: &Kit, kind: ReviewKind) -> (Color, Color, Color, Color) {
    let white = hex("#f8f8f2").unwrap_or(Color::WHITE);
    let dark = hex("#111111").unwrap_or(Color::WHITE);
    match kind {
        ReviewKind::Approve => (
            hex("#1f883d").unwrap_or(k.c.success.fg),
            hex("#2da44e").unwrap_or(k.c.success.fg),
            white,
            hex("#1a7434").unwrap_or(k.c.success.fg),
        ),
        ReviewKind::Comment => (
            k.c.layer_02,
            k.alpha(k.c.text_base, 0.12),
            k.c.text_base,
            k.c.border_muted,
        ),
        ReviewKind::RequestChanges => (
            hex("#b82e28").unwrap_or(k.c.danger.fg),
            hex("#d9453a").unwrap_or(k.c.danger.fg),
            white,
            hex("#a32722").unwrap_or(k.c.danger.fg),
        ),
        ReviewKind::Blocked => (
            hex("#e3a61a").unwrap_or(k.c.warning.fg),
            hex("#f0b83d").unwrap_or(k.c.warning.fg),
            dark,
            hex("#c99013").unwrap_or(k.c.warning.fg),
        ),
    }
}

fn solid_row(s: &App, k: &Kit, submitting: bool) -> El {
    let dis = submitting || s.cursor.is_none();
    let btn = |label, kind| {
        let (bg, hov, fg, b) = action_colors(k, kind);
        action_button(s, k, label, bg, hov, fg, b, dis, move |s, _| {
            s.begin_review(kind)
        })
    };
    let (nbg, nhov, nfg, nb) = action_colors(k, ReviewKind::Comment);
    row()
        .gap(8.0)
        .child(btn("Approve", ReviewKind::Approve))
        .child(btn("Comment", ReviewKind::Comment))
        .child(btn("Request changes", ReviewKind::RequestChanges))
        .child(if s.selected_blocked() {
            action_button(s, k, "Unblock", nbg, nhov, nfg, nb, dis, |s, _| {
                s.unblock_selected()
            })
        } else {
            btn("Mark blocked", ReviewKind::Blocked)
        })
}

// ── Status bar ───────────────────────────────────────────────────────────

fn statusbar(s: &App, k: &Kit) -> El {
    let submitting = s
        .cursor
        .as_ref()
        .is_some_and(|key| s.submitting.contains_key(key));
    let status = match &s.sync {
        Sync::Idle => "Ready".to_string(),
        Sync::Scanning { text } => text.clone(),
        Sync::Done => s
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
            .unwrap_or_else(|| "Ready".to_string()),
        Sync::Failed(e) => format!("Sync failed: {}", first_line(e)),
    };
    // Left column footer: count + status + version, at the sidebar's width.
    let n = s.visible_pr_count();
    let mut left = row()
        .w(s.sidebar_w)
        .h_full()
        .px(10.0)
        .gap(8.0)
        .center_y()
        .border_sides([0.0, HAIR, 0.0, 0.0], k.c.border_muted)
        .child(
            text(format!("{n} PR{}", if n == 1 { "" } else { "s" }))
                .font(k.ty.small)
                .color(k.c.text_muted),
        )
        .child(
            text(format!("{} total", s.pr_count()))
                .font(k.ty.small)
                .color(k.c.text_faint),
        );
    if matches!(s.sync, Sync::Scanning { .. }) {
        left = left.child(Spinner::new().build(k));
    }
    left = left
        .child(
            text(status)
                .font(k.ty.small)
                .color(k.c.text_muted)
                .truncate(),
        )
        .child(el().grow(1.0))
        .child(
            text(format!("v{}", env!("CARGO_PKG_VERSION")))
                .font(k.ty.small)
                .color(k.c.text_faint),
        );
    // Right column: failures, notice, and the review actions.
    let right = row()
        .grow(1.0)
        .h_full()
        .px(10.0)
        .gap(10.0)
        .center_y()
        .child(if s.sync_failures.is_empty() {
            text("")
        } else {
            text(format!(
                "{} search{} failed: {}",
                s.sync_failures.len(),
                if s.sync_failures.len() == 1 { "" } else { "s" },
                s.sync_failures
                    .first()
                    .map(|(_, e)| first_line(e))
                    .unwrap_or(""),
            ))
            .font(k.ty.small)
            .color(k.c.warning.fg)
            .help(
                s.sync_failures
                    .iter()
                    .map(|(r, e)| format!("{r}: {e}"))
                    .collect::<Vec<_>>()
                    .join(
                        "
",
                    ),
            )
        })
        .child(el().grow(1.0))
        .child(if let Some(notice) = &s.notice {
            text(notice.clone()).font(k.ty.small).color(k.c.text_muted)
        } else {
            text("")
        })
        .child(solid_row(s, k, submitting));
    row()
        .key("statusbar")
        .h(40.0)
        .w_full()
        .child(left)
        .child(right)
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or(s)
}

// ── Dialogs ──────────────────────────────────────────────────────────────

fn confirm_dialog(s: &App, k: &Kit, layer: &mut Vec<El>) {
    let (Some(kind), Some(key)) = (s.confirming, s.cursor.as_ref()) else {
        return;
    };
    let title = match kind {
        ReviewKind::Approve => format!("Approve {}?", key.display()),
        ReviewKind::Comment => format!("Comment on {}?", key.display()),
        ReviewKind::RequestChanges => format!("Request changes on {}?", key.display()),
        ReviewKind::Blocked => format!("Mark {} as blocked?", key.display()),
    };
    Dialog::new("review", title)
        .open(true)
        .fit()
        .description(match kind {
            ReviewKind::Blocked => "Submits a comment review and applies the Blocked label; the pull request hides until unblocked.".to_string(),
            ReviewKind::Approve => "Submits an approving review on GitHub.".to_string(),
            ReviewKind::Comment => "Submits your comment as a review.".to_string(),
            ReviewKind::RequestChanges => "Submits a request-changes review on GitHub.".to_string(),
        })
        .on_close(|s: &mut App| s.confirming = None)
        .child(
            col().w_full().padding(16.0).gap(12.0).child(
                row().w_full().gap(8.0).justify_end().child({
                    let (nbg, nhov, nfg, nb) = action_colors(k, ReviewKind::Comment);
                    action_button(s, k, "Cancel", nbg, nhov, nfg, nb, false, |s, _| s.confirming = None)
                }).child({
                    let (bg, hov, fg, b) = action_colors(k, kind);
                    action_button(s, k, kind.name(), bg, hov, fg, b, false, |s, tx| s.confirm_review(tx))
                }),
            ),
        )
        .build(k, layer);
}

fn error_dialog(s: &App, k: &Kit, layer: &mut Vec<El>) {
    let Some(error) = &s.review_error else { return };
    Dialog::new("error", "Review not submitted")
        .open(true)
        .fit()
        .description(error.clone())
        .on_close(|s: &mut App| s.review_error = None)
        .child(
            col().w_full().padding(16.0).child(
                row().w_full().justify_end().child(
                    Button::new("Dismiss")
                        .variant(ButtonVariant::Neutral)
                        .on_press(|s: &mut App| s.review_error = None)
                        .build(k),
                ),
            ),
        )
        .build(k, layer);
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
