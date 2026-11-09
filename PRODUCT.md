# Product

<!-- impeccable:product-schema 1 -->

## Platform

Native desktop (Rust/GPUI-style GPU UI via Rune; wgpu/winit). Primary host:
Omarchy Linux; must also run on macOS.

## Users

Neil and members of his small team — developers who receive review
requests across their own repositories and their organisations. They are
keyboard-first, often in Omarchy, and want to work through a review
queue faster than the GitHub web UI allows.

## Product Purpose

A fast, native review queue for GitHub pull requests. Two jobs equally:
**speed-triage** — approve-advance through obvious requests quickly —
and **careful review** — read the diff and discussion properly before
deciding. Neither may be traded off for the other.

## Positioning

Not a GitHub client, a queue. Everything targets throughput: search-based
discovery, optimistic submit with immediate advance to the next row,
prefetched details, and a keyboard contract as the primary interface.
Native rendering (not web) makes the queue feel instant; `gh` CLI owns
authentication so the app never holds credentials.

## Constraints

- `gh` subprocesses own all GitHub access; no token storage, no extra
  credential flow.
- Keyboard contract is the primary interface: arrows select, `⌃A/C/R/B`
  act, `/` filters, `Esc` backs out.
- Omarchy live theming (palette re-poll) plus dark/light elsewhere.
- Responsive to narrow windows (single-pane flow under ~780pt).
- Queue discovery via GitHub search/GraphQL — never repo enumeration.

## Vocabulary

- **Queue** — the grouped, filterable list of pending PRs (left).
- **Detail** — the selected PR: Conversation + Files tabs (right).
- **Review actions** — Approve, Comment, Request changes, Mark blocked.
- **Blocked** — a PR labelled `Blocked`, hidden behind the Blocked toggle;
  unblocking removes the label in-app.
- **Unassigned** — open, non-draft PRs with no assignee; hidden by default.
