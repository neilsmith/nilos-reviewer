# nilos-reviewer (Rune)

GitHub PR review queue in Rust/Rune (wgpu, winit). Port of the GPUI
implementation in `~/Work/tries/2026-10-29-github-pr-approver-2` — same core
loop: scan review requests → read → approve/comment/request-changes/block → next.

## Commands

- Build: `cargo build` · Lint: `cargo clippy` (keep clean) · Tests: `cargo test`
- Run: `./target/debug/nilos-reviewer` (needs a display + `gh` auth)
- Headless: `./target/debug/nilos-reviewer --snap out.png --scene loaded --size 1280x800`
  (`--list-scenes`, `--theme dark|light`, `--at 1.0`, `--script scripts/*.json`, `--tree`)
- Toolchain: `mise.toml` pins rust 1.99.1 (rune's rust-toolchain).

## Layout

- `src/model.rs` — domain types + pure queue logic (grouping, flattening, cursor, ages). Tested.
- `src/github.rs` — `gh api` subprocess service: discovery, repo scan (6 workers,
  generation-cancelled), detail load, review submit, Blocked label. Events on mpsc → `poll`.
- `src/app.rs` — `App` state + event/nav logic (`setup` spawns service unless `is_headless`).
- `src/view.rs` — the `El` view: titlebar, queue sidebar, detail (Markdown + DiffView), composer, dialogs.
- `src/theme.rs` — `Kit::neutral` or `Kit::from_palette` seeded by Omarchy's
  `~/.local/state/omarchy/current/theme/colors.toml` (re-polled ~750ms).
- `src/fixtures.rs` — fictional acme/* PR data for scenes/tests (never real bodies).
- `src/main.rs` — AppSpec, scenes, keyboard contract in `on_key`.

## Contract (from the reference; keep it)

- `gh` owns auth entirely — every call is a `gh api` subprocess; never store tokens.
- Arrows move AND select; `/`→filter, F5 refresh, Tab→composer, Esc→back,
  Ctrl+A/C/R/B = Approve/Comment/Request changes/Blocked → confirm dialog → Enter.
  Ctrl+Enter submits. Fields own their keys except Escape.
- Blocked = COMMENT review + "Blocked" label; row hides until Show blocked is on;
  blocked survives only while queued AND (server label OR <120s since marked).
- Optimistic submit: confirm closes instantly, row spins, queue auto-advances;
  new head SHAs invalidate handled marks on next scan.
- `PR_APPROVER_ORGS` (comma list) restricts org scans, else all `/user/orgs`.
- Narrow (<780pt): single pane — queue, detail behind select with a back button.
