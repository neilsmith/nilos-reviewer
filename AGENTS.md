# nilos-reviewer — agent notes

Native Rust desktop app on **Rune** (wgpu/winit, `rune` + `rune-kit` + `rune-fx`
git-pinned). GitHub PR review queue: search → read → approve/comment/
request-changes/block → next. Ported from the GPUI implementation at v0.2.0;
same product, new renderer.

## Commands

- Build: `cargo build` · Lint: `cargo clippy --workspace --all-targets -- -D warnings` (must be clean) · Format: `cargo fmt --all`
- Tests: `cargo test --workspace` — 30 tests (12 lib/model+github, 18 Runtime-driven UI)
- Run: `./target/debug/nilos-reviewer` (needs display + `gh` auth)
- Headless: `--snap out.png --scene <name> --size 1280x800 --at 0.5` (`--list-scenes`, `--theme dark|light`, `--script`)
- Perf probe: `--time-queue` — measures discover/search/detail inside the app's own code path; use it before quoting timings
- Release: `scripts/release.sh <ver>` — bumps `[package].version`, commits, tags `v<ver>`, pushes both refs atomically. Release workflow triggers on `main` pushes and builds only when HEAD is tagged `v*` — do NOT move it to tag-push triggers (branch-scoped caches mean tag runs rebuild ~400 crates cold)

## Layout

- `src/main.rs` — AppSpec + scenes + `on_key` keyboard contract + `Gfx` compositor wiring
- `src/app.rs` — `App` state + `on_event`/`poll` (service drain, Omarchy re-poll, update_rx drain) + nav/next-unhandled
- `src/github.rs` — `gh api`/`gh api graphql` service: REST discovery (viewer/teams/orgs), GraphQL queue search (progressive, two ScanResults), GraphQL detail + REST diff, review submit, Blocked label add/remove
- `src/model.rs` — domain types + pure queue logic (groups, flattened rows, cursor, filter, ages)
- `src/view.rs` — the `El` view: titlebar, queue, detail (Markdown + diff), composer, dialogs, statusbar (status · version · buttons), action_button/solid_row
- `src/fx.rs` — tiny rune-fx compositor: one GLOW mark for the loading state; `animating()` true only while an effect slot exists (window still idles)
- `src/theme.rs` — `Kit::neutral` or `Kit::from_palette` from Omarchy colors.toml
- `src/fixtures.rs` — fictional acme/* data for scenes/tests (never real bodies)
- `src/update.rs` — `self_update = "=1.3.2"` against release tarballs `nilos-reviewer-{target}.tar.gz`; `asset_matcher` must stay EXACT name match (the `.sha256` sibling collides on substring)
- `tests/ui.rs` — Runtime-driven tests: real wheel dispatch, layout rects via `on_layout` into `App::rects`, key/click events, event injection
- `packaging/macos/Info.plist` + `assets/icon.png` — the `.app` bundle bits the release workflow assembles + ad-hoc codesigns

## Contract (keep it)

- `gh` owns auth — every call is a `gh` subprocess; never store tokens.
- Keyboard: arrows move AND select; `/` filter, F5 refresh, Tab cycles,
  Esc back; Ctrl+A/C/R/B = Approve/Comment/RequestChanges/Blocked → confirm →
  Ctrl+Enter. `is_command` requires `Key::Command`, NOT `Key::Text`+ctrl — a
  real gotcha: tests must send `Key::Command`.
- Blocked = comment review + `Blocked` label; hidden unless the Blocked
  toggle is on; Unblock (button/Ctrl+B on a blocked row) removes the label.
- Optimistic submit → immediate advance to the NEXT row DOWN (not top).
- Counts exclude handled/approved; unassigned (open, non-draft, no assignee)
  hides behind its own toggle.
- Narrow (<780pt): single pane — queue, detail behind select + back button.

## Rune/platform gotchas (all burned once)

- `on_layout` rects land ONE FRAME LATE — advance an extra `rt.frame` before
  asserting bounds (also explains the narrow-mode flicker).
- `drag.x` is ELEMENT-LOCAL, not window-space — compute sidebar width from
  drag start + `drag.dx`, or the divider clamps to minimum.
- The queue scroll needs `min_h(0)` through every flex ancestor to bound it.
- GraphQL: `viewer.teams` does NOT exist — discovery is REST (`/user`,
  `/user/teams`, `/user/orgs`). Search aliases run server-sequentially, so
  queue emits progressively (direct `review-requested:@me` paints ~1.9s,
  team/unassigned merge later).
- Search: `user:` and `org:` qualifiers can't share a parenthesised OR —
  one `search` alias per scope, or it 422s/returns 0 silently.
- Effect API: `el().effect(mark).effect_data([f32;4])` → the app's Compositor
  maps mark→`FxLayer`, and needs `.at(rt.t)` or effects render frozen at
  phase 0 (the "non-breathing glow" bug).
- `hex()` is `rune::color::hex(&str) -> Option<Color>`, not `Color::hex`.
- `Button`/`Badge` look identical as chips — actions must be solid fills with
  real borders; metadata stays hollow (DESIGN.md's affordance rule).
