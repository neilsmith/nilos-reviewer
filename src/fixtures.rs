//! Synthetic fixture data for headless scenes and tests (fictional acme/dev-*
//! world, never captured from real PRs).

use crate::model::{Check, Discussion, DiscussionKind, PrDetail, PrKey, PrSummary, RepoGroup};

const BODY: &str = "\
## Summary

Adds a `--watch` flag to the dev server so fixtures regenerate on save, and
fixes the redirect loop reported in #41.

- Debounces rebuilds at 150 ms
- Keeps the last good bundle on a failed build
- Redirects `/docs` to `/docs/` instead of looping

## Test plan

- [x] `cargo test` passes
- [x] Manual check on `acme/web` staging
- [ ] Load test with 10k files

```json
{ \"watch\": true, \"debounce_ms\": 150 }
```

| Command | Result |
| --- | --- |
| `--watch` | rebuilds on save |
| `--watch --debounce 300` | rebuilds every 300 ms |

> Heads up: this changes the default bind address to `127.0.0.1`.

1. Check out the branch
2. Run `dev --watch`
3. Edit `fixtures/hello.md`
";

const DIFF: &str = "\
diff --git a/src/dev.rs b/src/dev.rs
index 1111111..2222222 100644
--- a/src/dev.rs
+++ b/src/dev.rs
@@ -10,9 +10,14 @@ impl DevServer {
     pub fn run(&self) {
         let addr = self.addr();
-        let bind = \"0.0.0.0\";
+        let bind = \"127.0.0.1\";
         listen(bind, addr);
+        if self.watch {
+            self.spawn_watcher();
+        }
     }
+    fn spawn_watcher(&self) {
+        watch(self.fixtures(), Duration::from_millis(150), |event| {
+            self.rebuild(event.path());
+        });
+    }
 }
diff --git a/src/args.rs b/src/args.rs
index 3333333..4444444 100644
--- a/src/args.rs
+++ b/src/args.rs
@@ -1,6 +1,10 @@
 pub struct Args {
+    /// Rebuild fixtures on save.
+    pub watch: bool,
+    /// Debounce interval, in milliseconds.
+    pub debounce_ms: u64,
 }
@@ -20,6 +24,7 @@ impl Args {
         Args {
+            watch: false,
+            debounce_ms: 150,
         }
     }
 }
";

fn pr(repo: &str, n: u64, title: &str, author: &str, created: &str, reasons: &[&str]) -> PrSummary {
    PrSummary {
        key: PrKey::new(repo, n),
        title: title.into(),
        author: author.into(),
        created_at: created.into(),
        draft: false,
        reasons: reasons.iter().map(|s| s.to_string()).collect(),
        blocked_label: false,
        unassigned: false,
        head_sha: format!("{n:040x}"),
        url: format!("https://github.com/{repo}/pull/{n}"),
    }
}

/// A queue across three repos: normal, draft, blocked and reviewed PRs.
pub fn queue() -> Vec<RepoGroup> {
    let mut groups = vec![
        RepoGroup {
            repo: "acme/web".into(),
            prs: vec![
                pr("acme/web", 42, "Add --watch to the dev server", "fern", "2026-11-06T09:12:00Z", &["You", "web-reviewers"]),
                pr("acme/web", 47, "Bump dependencies for the freeze", "bot", "2026-11-08T14:40:00Z", &["You"]),
                {
                    let mut p = pr("acme/web", 39, "Rework settings layout", "io", "2026-11-02T11:05:00Z", &["web-reviewers"]);
                    p.draft = true;
                    p
                },
            ],
        },
        RepoGroup {
            repo: "acme/api".into(),
            prs: vec![
                {
                    let mut p = pr("acme/api", 128, "Paginate the fixtures endpoint", "jo", "2026-11-05T16:22:00Z", &["You"]);
                    p.blocked_label = true;
                    p
                },
                pr("acme/api", 131, "Return 404 for missing fixtures", "fern", "2026-11-08T08:01:00Z", &["api-reviewers"]),
            ],
        },
        RepoGroup {
            repo: "acme/docs".into(),
            prs: vec![
                pr("acme/docs", 9, "Document the release checklist", "sam", "2026-11-07T12:00:00Z", &["You"]),
                {
                    let mut p = pr("acme/docs", 12, "Mirror the api docs to the site", "rae", "2026-11-04T15:30:00Z", &["Unassigned"]);
                    p.unassigned = true;
                    p
                },
            ],
        },
    ];
    for group in &mut groups {
        group.prs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    }
    groups
}

/// A long queue across many repos, for scroll/perf scenes.
pub fn long_queue() -> Vec<RepoGroup> {
    (0..24)
        .map(|i| RepoGroup {
            repo: format!("acme/team-{i:02}"),
            prs: (0..4)
                .map(|n| {
                    let num = 100 + i * 10 + n;
                    pr(&format!("acme/team-{i:02}"), num,
                        &format!("Change the thing in area {num}"),
                        "fern", "2026-11-08T10:00:00Z", &["You"])
                })
                .collect(),
        })
        .collect()
}

/// The detail for acme/web#42, the scene's selected PR.
pub fn detail() -> PrDetail {
    PrDetail {
        key: PrKey::new("acme/web", 42),
        title: "Add --watch to the dev server".into(),
        body: BODY.into(),
        author: "fern".into(),
        url: "https://github.com/acme/web/pull/42".into(),
        head_sha: format!("{:040x}", 42),
        base_ref: "main".into(),
        head_ref: "dev-watch".into(),
        draft: false,
        additions: 63,
        deletions: 12,
        changed_files: 2,
        diff: DIFF.into(),
        events: vec![
            Discussion {
                author: "jo".into(),
                created_at: "2026-11-06T10:04:00Z".into(),
                body: "Nice — this closes a gap we've hit every release. One question inline about the debounce window.".into(),
                kind: DiscussionKind::Comment,
            },
            Discussion {
                author: "jo".into(),
                created_at: "2026-11-06T10:06:00Z".into(),
                body: "150 ms feels tight for big fixture trees — did you try 300?".into(),
                kind: DiscussionKind::ReviewComment { path: "src/dev.rs".into(), line: Some(15) },
            },
            Discussion {
                author: "fern".into(),
                created_at: "2026-11-06T10:40:00Z".into(),
                body: "Good call, bumped to 150 with a flag to tune it. <details><summary>Benchmarks</summary>\n\n10k files: 42 ms median, 61 ms p95.\n\n</details>".into(),
                kind: DiscussionKind::Comment,
            },
            Discussion {
                author: "jo".into(),
                created_at: "2026-11-07T09:00:00Z".into(),
                body: "".into(),
                kind: DiscussionKind::Review("APPROVED".into()),
            },
        ],
        requested: vec!["You".into(), "web-reviewers".into()],
        checks: vec![
            Check { name: "build".into(), status: "completed".into(), conclusion: Some("success".into()) },
            Check { name: "test".into(), status: "completed".into(), conclusion: Some("success".into()) },
            Check { name: "lint".into(), status: "in_progress".into(), conclusion: None },
        ],
    }
}
