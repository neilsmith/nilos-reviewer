# NilosReviewer

A native Rust desktop app for working through GitHub pull requests that are
waiting on your personal review or a review from a team you belong to. Built on
[Rune](https://github.com/anomalyco/rune) (wgpu/winit) with GitHub access via
the `gh` CLI — the app never holds a token. It renders a review queue grouped
by repository, the PR body + discussion + inline review comments, status
checks, and the full diff, and lets you approve/comment/request-changes/block
with keyboard shortcuts — optimized for clearing a backlog without fuss.

## Run

Prerequisite: the GitHub CLI must already be authenticated (`gh auth login`).
The app shells out to `gh` for every call and never stores or displays a token.

```bash
cargo build
./target/debug/nilos-reviewer
```

## Install a release

Prebuilt assets ship on [GitHub Releases](../../releases):

- **Linux** — `nilos-reviewer-x86_64-unknown-linux-gnu.tar.gz`
- **macOS (Apple Silicon)** — `nilos-reviewer-aarch64-apple-darwin.tar.gz`, or
  `nilos-reviewer-aarch64-apple-darwin.app.tar.gz` for a double-clickable
  `Nilos Reviewer.app`.

The app self-updates in the background from these releases; it replaces the
running binary (inside the `.app` too) and shows a "restart to apply" notice.

### macOS without signing

The `.app` is **unsigned** (ad-hoc codesigned only), so Gatekeeper flags it on
first open. Either of these works — you only need to do it once:

1. **Right-click the app → Open → Open** (the menu item must be used — a
   double-click won't offer it).
2. Or remove the quarantine flag after extracting:
   `xattr -dr com.apple.quarantine "Nilos Reviewer.app"`

Self-updates after that may re-trigger the prompt once, for the same reason.

## Workflow

- The queue searches GitHub directly (`review-requested:@me`, one
  `team-review-requested:org/team` per team, plus an unassigned-triage search
  per org) and paints progressively — your direct requests appear first.
- Rows show reason badges (You / team name / Unassigned), Draft and Blocked
  states; Blocked and Unassigned rows hide behind sidebar toggles.
- Refresh (F5) reruns the search. A completed review stays visible muted;
  a newer head SHA returns it to the outstanding queue.
- `Up`/`Down` navigate and select; `Enter` opens, `/` filters, `Esc` backs out,
  `PageUp`/`PageDown` page the detail. Tab cycles Conversation/Files and the
  composer. Click a group header to collapse it. Drag the pane divider.
- `Ctrl+A` / `Ctrl+C` / `Ctrl+R` / `Ctrl+B` begin Approve, Comment,
  Request Changes, and Mark as Blocked (or Unblock on a blocked PR) —
  `Ctrl+Enter` confirms. The action buttons live in the bottom bar.
- Mark as Blocked posts a comment review and applies the `Blocked` label
  (creating it if needed); Unblock removes the label.
- Before submitting, the app re-checks the PR state and head SHA; a new commit
  refuses a stale submission. Submissions are optimistic — the queue advances
  to the next row immediately.

To restrict the organization search, set `PR_APPROVER_ORGS` to a
comma-separated allow-list.

## Data

Drafts and handled entries live at
`~/.config/nilos-reviewer/state.json` (0600). No credential is persisted.

## Themes

On Omarchy/Hyprland the app follows the live Omarchy palette
(`~/.local/state/omarchy/current/theme/colors.toml`, re-polled). Elsewhere it
uses the OS light/dark appearance.

## Development

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                                   # 30 tests
./target/debug/nilos-reviewer --list-scenes              # headless fixtures
./target/debug/nilos-reviewer --snap out.png --scene loaded --size 1280x800
./target/debug/nilos-reviewer --time-queue               # real perf probe
scripts/release.sh 0.2.2                                 # bump + tag + push
```

## License

MIT — see [LICENSE](LICENSE).
