# nilos-reviewer — agent notes

Native Rust/GPUI desktop app ("NilosReviewer") for working through a
queue of GitHub PRs awaiting the authenticated user's personal review or
review by their teams. Standalone app.

## Commands

- Build: `cargo build`
- Lint: `cargo clippy --workspace --all-targets -- -D warnings` (must be clean)
- Format: `cargo fmt --all`
- Tests: `cargo test --workspace` (14 tests; rendering suite alone: `cargo test --test rendering`, native GPUI: `cargo test --test native_rendering`)
- Run: `./target/debug/nilos-reviewer`
- Offline rendering preview (no auth/network): `./target/debug/nilos-reviewer --render-fixture <name>` — side-by-side GitHub-HTML vs app Markdown for every fixture, Left/Right switches fixture.
- GitHub auth: reuses `gh` CLI session via `gh auth token` — never add a credential flow or store tokens (`src/store.rs` must never persist secrets).

## Layout

- `src/main.rs` — entrypoint, theme install, `--render-fixture` preview mode
- `src/app.rs` — ALL the UI state: queue, details, diff, composer, review submission, keyboard nav (`nav_key`), theme watchers
- `src/render.rs` — `discussion_markdown()` body normalization + `discussion_view()`/`body_style()` shared by live bodies and the offline preview
- `src/fixtures.rs` — `RENDER_FIXTURES` (name + markdown + GitHub-rendered HTML) for tests/preview
- `src/themes.rs` + `src/macos-classic.json` — bundled gpui-kit themes for non-Omarchy
- `src/omarchy.rs` — Omarchy palette → ThemeConfig; `install()` returns `None` off-Omarchy
- `src/browser.rs` — opens links via `omarchy-launch-webapp`, then polls `hyprctl -j clients` for a NEW client whose `class`/`initialClass` matches a browser (chrom/firefox/brave/edge/webapp) — never acts on unverified windows. Floats + sizes ~⅔×⅘ of that window's monitor (via `hyprctl -j monitors`, scale-aware) and pixel-moves it centred, all address-targeted so focus never jumps. `cx.open_url` fallback off-Hyprland
- `src/store.rs` — `~/.config/nilos-reviewer/state.json` (chmod 0600): drafts, handled entries, blocked keys. Draft writes are debounced ~500ms via `state_dirty` + `poll_service` — `save_current_draft` updates memory only; `persist_state()` writes eagerly for transitions.
- `src/update.rs` — background self-update via `self_update` (pinned `=1.3.2`) against GitHub release tarballs `nilos-reviewer-{target}.tar.gz`. `asset_matcher` must stay an exact name match — the `.sha256` sibling collides on the target substring. Spawned in `ApproverView::new` (never in `--render-fixture`), reports the new tag over mpsc → `poll_service` → `self.notice` ("Updated to X — restart to apply"). Replaces the binary on disk but never restarts the process. Do NOT reintroduce axoupdater — it only supports cargo-dist receipts + `*-installer.sh` assets this repo doesn't ship.
- `crates/pr-core` — domain models + queue logic (`PrSummary`, `PrKey`, `ReviewKind`, `filtered_groups`, `flattened_rows`, `group_by_repository`, `next_unhandled`, `OmarchyPalette` parser)
- `crates/pr-github` — `client.rs` (typed REST calls, paginates, `submit_review`, `add_labels`), `service.rs` (background thread: scan/details/submit via `ServiceCommand`/`ServiceEvent` mpsc), `models.rs`
- `tests/fixtures/rendering/*.md` + `*.github.html` — SYNTHETIC fixture bodies (fictional acme/dev-* world, same Markdown/HTML combos as real GitHub bodies) + reference HTML rendered from each `.md` with markdown-rs GFM. To regenerate references after editing a fixture: render `Options::gfm()` + `allow_dangerous_html` over the `.md`. Real PR bodies were deliberately replaced, not anonymized — never reintroduce captured data.
- `PLAN.md`, `PRODUCT.md` — original spec/requirements (gitignored — internal notes, do not publish)

## Keyboard contract (product requirement)

`nav_key` in app.rs is bound via `capture_key_down` + `track_focus` on the root:

- `Up`/`Down` ALWAYS move the queue cursor AND select the row (detail loads as you arrow) — except while the composer is focused, where arrows stay for caret movement.
- `PageUp`/`PageDown` ALWAYS page the detail pane, any pane.
- `Ctrl+A`/`Ctrl+C`/`Ctrl+R`/`Ctrl+B` = Approve/Comment/RequestChanges/Blocked → confirmation → `Ctrl+Enter` submits. Ignored while composer/filter focused (they're text-editing keys there).
- While the FILTER is focused it owns the keyboard: `nav_key` early-returns so `/` and ordinary typing reach the field; only Enter/Escape are intercepted (they hand focus back). Without this guard `capture_key_down` swallows filter input.
- `F5` = refresh (Ctrl+R is taken). `Enter` opens the cursor row, `/` focuses the filter, `Tab` cycles panes, `Esc` backs out one level.
- GOTCHA: GPUI key handling only works once SOMETHING has focus. `view_focus` is focused in `new()` and re-focused on row select so keys work at boot and after mouse clicks. If keys "stop working" it's almost always a lost focus handle, not the bindings.

## Review workflow

- Buttons: Approve (success) | Blocked (warning) | Comment (info) | Request Changes (danger), confirmation modal first, fixed bottom bar with comment editor + Markdown preview.
- `ReviewKind::Blocked` = `COMMENT` event + `POST /repos/{repo}/issues/{n}/labels` `["Blocked"]` (client creates the label only when the add 404s/422s — the labels endpoint 404s on unknown names; other errors propagate as-is). Labelling failure does NOT fail the review; notice says "submitted, but labelling failed".
- Blocked PRs are hidden unless "Show blocked" (footer, next to "Hide done") is on; rows show `Blocked · <reasons>`. Blocked is parked in `self.blocked`, NOT `handled`, so Show blocked can reveal it. Persisted via `state.blocked` so it hides before GitHub confirms the label. `pr.blocked` also reads the server label (case-insensitive `blocked` — GitHub labels can be lowercase).
- On SyncFinished, `self.blocked` is reconciled against the server: a key stays only if the PR is still queued AND (server label present OR marked < `BLOCKED_GRACE` = 120s ago via `blocked_since`). Server label removal un-hides the PR; vanished PRs drop out of the set.
- Successful submission clears the draft (`state.drafts` + composer) — a submitted body must not resurface next session. Auto-advance filters blocked rows even when Show blocked is on.
- Before submitting, client re-fetches the PR: refuses on closed PR, changed head SHA (stale review), own PR, or existing PENDING review. Failed submissions keep you on the PR with the draft.
- Successful submission → entry marked handled (muted + tick), auto-advance to next unhandled row. `Comment`/`Blocked` require a body; `Approve` allows empty.
- `COMMENT` reviews can leave the review request outstanding — they count as handled for the session but may return on next launch. New head SHAs invalidate persisted handled state.

## Rendering — hard-won, don't regress

GitHub bodies mix authored Markdown with embedded HTML (`<details>`, `<blockquote>`, `<ul><li><a><code>`). Two GPUI bugs forced the current design in `render::discussion_markdown`:

1. GPUI's HTML-list path DROPS list-item text (renders an empty code pill) — verified side-by-side: `TextView::html` shows pills, Markdown path shows text.
2. GPUI's HTML inline nodes have NO source positions — `range_for_source` returns `None`, which breaks link clicks inside `<a><code>`.

The fix: parse with `markdown::to_mdast` (GFM), convert ONLY block-level `Node::Html` nodes to Markdown via `htmd` (padded with `\n\n` so summaries don't merge into the previous paragraph), splice back by source byte range. Authored Markdown (tables, ```` fences, heading whitespace) stays byte-identical; inline HTML like `<br>` stays for GPUI's working inline path.

Do NOT reintroduce: whole-body `TextView::html` (bug 1), whole-document `htmd`/`html2text` conversion (they collapse Markdown whitespace → giant merged headings, lost fences, escaped `\[`, dropped link URLs). All three were tried and caused the exact bugs the user reported.

Tests that pin this (fixture-driven TDD — the user asked for this explicitly after ad-hoc fixes circled):
- `tests/rendering.rs` — compiles `discussion_markdown` output with markdown-rs and compares block structure to GitHub's own `body_html` reference (headings, paragraphs incl. flattened `<summary>`, `<li>` text, table cells, `<pre>` whitespace+valid JSON). Selector gotcha: scraper can't do complex `:not()`, so `paragraphs()` filters `li p` explicitly (loose-list dialect diff).
- `tests/native_rendering.rs` — real `gpui::test` window asserts TextView produces a 23-row table, a `json` code block, and complete list text.

## Themes

- Omarchy machines: `omarchy::install()` projects the live palette into a `ThemeConfig` and a poll task (~750ms) re-reads `colors.toml`. Light Omarchy accents caused "white on light-green" inline code — `body_style()` forces dark muted code backgrounds + `theme.highlight_theme`.
- Non-Omarchy (macOS): `themes::install` applies "macOS Classic Dark"/"Light" from bundled `src/macos-classic.json` (from github.com/longbridge/gpui-kit/themes) matching `cx.window_appearance()`; `poll_system_theme` follows live OS changes.
- `WindowAppearance` has `Dark`/`VibrantDark`/etc variants — `matches!` on them, there is no `is_dark()`.
- `TextViewStyle.inline_code` defaults to `theme.accent` background — override it; inline-code color = foreground, bg = muted.

## Discovery / service

- Service thread scans personal + org repos concurrently (bounded workers), matches `requested_reviewers`/`requested_teams` against viewer+team IDs, emits progress; dedupes and labels "You"/team reasons.
- Orgs come from `PR_APPROVER_ORGS` (comma-separated allow-list) if set, else every org from `/user/orgs` — `client.orgs()` in pr-github. Org-scan failures accumulate into `sync_failures` like repo failures (never a silent empty queue).
- Generation IDs ARE the cancellation: workers compare their scan's generation against the shared counter and stop taking work when a newer command bumps it (a shared bool had a reset window where stale workers kept running). The UI records `scan_generation` on each refresh and drops stale Stage/RepositoryDone/RepositoryFailed/SyncFinished/AuthFailed events in `poll_service`; Detail/Review events pass through (valid for the still-selected PR). A detail load dropped by a gen bump is re-requested in SyncFinished.
- Repo failures accumulate as `sync_failures` → "Sync incomplete" footer warning, never a silent empty queue.
- EVERY command kind spawns its own thread in `service_loop` (`Refresh`/`LoadDetails`/`SubmitReview`) — nothing queues behind a running scan; concurrent scans self-cancel via generation. Refresh can't invalidate an in-flight submission (`submitting` guard + generation still current at receipt).
- Review hotkeys guard on `self.submitting` — buttons disable but `Ctrl+*` paths must not send a duplicate SubmitReview. `DetailLoaded` only substitutes the queue summary when the detail's OWN key is still queued — falling back to the selected PR's summary contaminates the detail pane.

## Misc gotchas

- `pgrep -f`/`pkill -f` with the binary path kills the invoking shell too (its cmdline matches). Use `[g]ithub-pr-approver` bracket pattern.
- `Button` has `.tooltip()`, not `.aria_label`; `Textarea` has `aria_label`.
- Icon names available: `CircleCheck`, `MessageSquare`, `SquareX`, `OctagonAlert`, `RefreshCw`, `Eye`/`EyeOff`, `ExternalLink`, `GitPullRequest`, `ChevronDown/Right`, `NotebookPen`.
- `submit_review` retries once after 750ms on 5xx (`GithubError::transient()`) — a 503 is GitHub-side, NOT a client bug (4xx would be). No retry on 4xx or transport errors (double-submit risk).
- `gpui` dev-dependency pins `gpui-pre = "=0.3.10"` with `test-support` for native tests.
- Preview/test windows disable links and remote images (`image_source` → `ImageSource::Custom` returning None).

## TODO / known gaps

- macOS is theme-ready but NOT build-verified (no Mac CI).
- Scan ETA is approximate; no scan cancellation UX beyond refresh.
- Next-PR details prefetch during first scan.
- Richer check names in the checks row.
- Restore `<details>` collapsibility? Currently expanded inline — acceptable per tests.
