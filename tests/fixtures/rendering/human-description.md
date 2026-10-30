Fixes the crash reported in [incident 42](https://example.invalid/acme/infrastructure/exceptions/incidents/42) and closes #218.

## What happened

The exporter assumed every row has a `completed_at` timestamp. Rows still in
flight have `null`, so the batch aborted partway through and the dashboard
showed stale numbers overnight.

## The fix

- Treat `completed_at` as optional and fall back to `started_at` for ordering.
- Emit a `row_skipped` metric instead of aborting the whole batch.
- Added a regression test covering the in-flight row shape.

The first commit shows the failing test before the fix.<br>
Deployed to staging this morning — the backlog drained in ~40s and no rows
were skipped unexpectedly.
