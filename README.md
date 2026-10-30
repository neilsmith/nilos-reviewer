# NilosReviewer

A native Rust/GPUI desktop app for working through GitHub pull requests that are
waiting on your personal review or a review from a team you belong to. It scans
your repositories, groups the review queue by repository, renders PR bodies and
comments like GitHub, shows diffs, and lets you submit reviews with keyboard
shortcuts — optimized for clearing a backlog without fuss.

## Run

Prerequisite: the GitHub CLI must already be authenticated. The app shells out
to `gh auth token` and never stores or displays the token.

```bash
cargo build
./target/debug/nilos-reviewer
```

Development checks:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Repository discovery

At startup the app scans repositories you own and repositories in every
organization you belong to (via `/user/orgs`), checking open PRs concurrently
for review requests addressed to you or your teams.

To restrict the organization scan, set `PR_APPROVER_ORGS` to a comma-separated
allow-list:

```bash
PR_APPROVER_ORGS="my-org,another-org" ./target/debug/nilos-reviewer
```

## Workflow

- The queue groups results by repository and identifies whether the request is
  for `You`, a team, or both.
- Refresh reruns discovery. A completed review remains visible but muted; a
  newer head SHA returns it to the outstanding queue.
- `Tab` cycles queue → details → composer. `Up`/`Down` navigate the queue,
  `PageUp`/`PageDown` page the PR detail, `Enter` opens, `/` focuses filter,
  and `Esc` backs out. `F5` (or the refresh icon at the top-right of the
  queue) refreshes.
- `Ctrl+A` / `Ctrl+C` / `Ctrl+R` / `Ctrl+B` begin Approve, Comment,
  Request Changes, and Mark as Blocked; `Ctrl+Enter` confirms.
- Mark as Blocked posts your comment as a comment-only review and applies the
  `Blocked` label to the PR (creating the label if needed). Blocked PRs are
  hidden unless "Show blocked" (next to "Hide done") is enabled.
- Approve, Comment, and Request Changes show a confirmation first. Comment
  and Request Changes require a body.
- Before submitting, the app re-checks the PR state and head SHA; a new commit
  refuses stale submission.

## Data

Drafts and handled entries are stored at:

`$XDG_CONFIG_HOME/nilos-reviewer/state.json` or `~/.config/nilos-reviewer/state.json`.

No credential is persisted.

## Themes

On Omarchy/Hyprland the app follows the live Omarchy palette. Elsewhere it
uses bundled gpui-kit themes ("macOS Classic Light"/"macOS Classic Dark")
selected from the OS appearance.

## License

MIT — see [LICENSE](LICENSE).
