//! GitHub access through the `gh` CLI: every call is a `gh api` subprocess, so
//! `gh` owns authentication, SSO prompts and token refresh. The service thread
//! runs scans, detail loads and review submissions off the UI thread and lands
//! `Event`s on an `mpsc` channel that `poll` drains.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use serde_json::Value;

use crate::model::{Check, PrDetail, PrKey, PrSummary, ReviewKind};

/// What the app can ask the service to do.
pub enum Request {
    Refresh,
    LoadDetail(PrKey),
    SubmitReview {
        key: PrKey,
        kind: ReviewKind,
        body: String,
    },
    /// Remove the Blocked label.
    Unblock(PrKey),
}

/// What the service reports back.
pub enum Event {
    /// The authenticated user could not be established.
    AuthFailed(String),
    /// Discovery completed; `login` is the authenticated user.
    Discovered {
        login: String,
    },
    /// Scan progress: "Scanning acme/web…" (3/12).
    Stage {
        gen: u64,
        text: String,
    },
    /// The queue search finished; `prs` is the complete queue.
    ScanResult {
        gen: u64,
        prs: Vec<PrSummary>,
    },
    /// The queue search failed (permissions, SSO, rate limit).
    ScanFailed {
        gen: u64,
        error: String,
    },
    /// Every repository finished; the queue is complete for this generation.
    SyncFinished {
        gen: u64,
    },
    /// Detail for one PR arrived.
    DetailLoaded {
        key: PrKey,
        detail: Box<PrDetail>,
    },
    DetailFailed {
        key: PrKey,
        error: String,
    },
    /// The Blocked label was removed.
    Unblocked {
        key: PrKey,
    },
    /// Label removal failed; the row reverts to blocked.
    UnblockFailed {
        key: PrKey,
        error: String,
    },
    ReviewSubmitted {
        key: PrKey,
        kind: ReviewKind,
        labelled: Option<bool>,
    },
    ReviewFailed {
        key: PrKey,
        error: String,
    },
}

/// A `gh api` call's parsed body, or the error text to show.
fn gh(args: &[&str]) -> Result<String, String> {
    gh_input(args, None)
}

/// How to invoke `gh`. Finder-launched apps get a minimal PATH — and `gh` may
/// only work via a shell alias (`op plugin run -- gh` injecting its token).
/// Ask the login shell for its `gh` first (aliases stripped → the real
/// binary), then probe install spots; a shell-wrapped invocation stays as
/// the last resort: a retriable failure (missing binary, bad auth) retries
/// the next entry and sticks to what worked.
enum GhCmd {
    /// A real executable path (or bare `gh` for a PATH lookup).
    Bin(String),
    /// Invoke via the login shell so aliases/functions apply.
    Shell(String),
}

/// Index into gh_cmds() that last succeeded — later calls start there.
static GH_ORDER: AtomicUsize = AtomicUsize::new(0);

fn gh_cmds() -> &'static [GhCmd] {
    static CMDS: std::sync::OnceLock<Vec<GhCmd>> = std::sync::OnceLock::new();
    CMDS.get_or_init(|| {
        let mut cmds = Vec::new();
        let mut shell = None;
        if let Some((sh, out)) = shell_probe_gh() {
            if let Some(p) = out
                .lines()
                .map(str::trim)
                .rfind(|l| l.starts_with('/') && std::path::Path::new(l).exists())
            {
                cmds.push(GhCmd::Bin(p.to_string()));
            }
            if !out.trim().is_empty() {
                shell = Some(sh);
            }
        }
        let mut spots: Vec<std::path::PathBuf> = ["/opt/homebrew/bin", "/usr/local/bin"]
            .iter()
            .map(|d| std::path::PathBuf::from(format!("{d}/gh")))
            .collect();
        if let Some(home) = std::env::var_os("HOME") {
            let home = std::path::Path::new(&home);
            for rel in [
                ".local/bin/gh",
                ".local/share/mise/shims/gh",
                ".asdf/shims/gh",
                ".nix-profile/bin/gh",
            ] {
                spots.push(home.join(rel));
            }
        }
        for p in &spots {
            if p.exists() {
                let p = p.to_string_lossy().into_owned();
                if !cmds.iter().any(|c| matches!(c, GhCmd::Bin(b) if b == &p)) {
                    cmds.push(GhCmd::Bin(p));
                }
            }
        }
        cmds.push(GhCmd::Bin("gh".to_string()));
        if let Some(sh) = shell {
            cmds.push(GhCmd::Shell(sh));
        }
        cmds
    })
}

/// `command -v gh` inside the user's login+interactive shell — the PATH their
/// terminal sees (`-i` sources the rc where version-manager shims and aliases
/// get defined). Aliases/functions are cleared first so -v yields the real
/// binary. Poll-bounded so a blocking rc file can't wedge startup.
/// Returns (shell, raw stdout).
fn shell_probe_gh() -> Option<(String, String)> {
    static OUT: std::sync::OnceLock<Option<(String, String)>> = std::sync::OnceLock::new();
    OUT.get_or_init(|| {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        // `functions` is fish's unset; a harmless error elsewhere.
        let mut child = Command::new(&shell)
            .args([
                "-ilc",
                "unalias gh 2>/dev/null; unset -f gh 2>/dev/null; functions -e gh 2>/dev/null; command -v gh",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
            }
        }
        let out = child.wait_with_output().ok()?;
        Some((shell, String::from_utf8(out.stdout).ok()?))
    })
    .clone()
}

/// Failure kinds worth trying the next invocation for: a missing binary, or
/// an auth failure — a real binary can still be unauthenticated where an
/// alias like `op plugin run -- gh` injects the token.
fn retriable(e: &str) -> bool {
    [
        "could not run gh",
        "Bad credentials",
        "gh auth login",
        "HTTP 401",
        "not logged in",
    ]
    .iter()
    .any(|m| e.contains(m))
}

/// `gh api` with an optional JSON document piped to `--input -`.
fn gh_input(args: &[&str], input: Option<&Value>) -> Result<String, String> {
    let cmds = gh_cmds();
    let first = GH_ORDER.load(Ordering::Relaxed);
    let mut err = "could not run gh".to_string();
    for o in 0..cmds.len() {
        let i = (first + o) % cmds.len();
        match gh_run(&cmds[i], args, input) {
            Ok(s) => {
                GH_ORDER.store(i, Ordering::Relaxed);
                return Ok(s);
            }
            Err(e) => {
                err = e;
                if !retriable(&err) {
                    return Err(err);
                }
            }
        }
    }
    Err(err)
}

fn gh_run(c: &GhCmd, args: &[&str], input: Option<&Value>) -> Result<String, String> {
    let mut cmd = match c {
        GhCmd::Bin(bin) => {
            let mut c = Command::new(bin);
            c.arg("api").args(args);
            // `gh`/shim scripts may exec other tools — put its own dir on
            // PATH, since launchd's default is bare.
            if let Some(dir) = std::path::Path::new(bin)
                .parent()
                .filter(|d| !d.as_os_str().is_empty())
            {
                let inherited = std::env::var("PATH").unwrap_or_default();
                c.env("PATH", format!("{}:{inherited}", dir.display()));
            }
            c
        }
        GhCmd::Shell(shell) => {
            // `gh` is an alias/function — run it inside the login shell.
            // Positional args keep the values out of shell parsing.
            let mut c = Command::new(shell);
            if std::path::Path::new(shell).file_stem() == Some(std::ffi::OsStr::new("fish")) {
                c.args(["-ilc", "gh api $argv"]).args(args);
            } else {
                c.args(["-ilc", "gh api \"$@\"", "gh"]).args(args);
            }
            c
        }
    };
    let mut child = cmd
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run gh: {e}"))?;
    if let Some(doc) = input {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(doc.to_string().as_bytes());
        }
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        String::from_utf8(out.stdout).map_err(|e| e.to_string())
    } else {
        let mut err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if err.is_empty() {
            err = format!("gh api exited {}", out.status);
        }
        Err(err)
    }
}

/// `gh api` JSON for one page or a `--paginate --slurp` flattened array.
fn gh_json(args: &[&str]) -> Result<Value, String> {
    let text = gh(args)?;
    serde_json::from_str(&text).map_err(|e| format!("bad gh api response: {e}"))
}

/// A paginated JSON array (`--paginate --slurp` wraps pages in an outer array).
fn gh_json_pages(path: &str) -> Result<Vec<Value>, String> {
    let v = gh_json(&[path, "--paginate", "--slurp"])?;
    Ok(match v {
        Value::Array(pages) => pages
            .into_iter()
            .flat_map(|page| match page {
                Value::Array(items) => items,
                other => vec![other],
            })
            .collect(),
        other => vec![other],
    })
}

/// One `gh api graphql` round trip → the `data` object.
fn gh_graphql(query: &str) -> Result<Value, String> {
    let text = gh(&["graphql", "-f", &format!("query={query}")])?;
    let doc: Value =
        serde_json::from_str(&text).map_err(|e| format!("bad gh graphql response: {e}"))?;
    doc.get("data").cloned().ok_or_else(|| {
        format!(
            "graphql error: {}",
            doc.get("errors").map(|e| e.to_string()).unwrap_or_default()
        )
    })
}

fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)?.as_str()
}

#[derive(Clone)]
struct Discovery {
    login: String,
    /// (`org/slug`, name) pairs for team-review-requested searches.
    teams: Vec<(String, String)>,
    /// Org logins for unassigned-triage searches.
    orgs: Vec<String>,
}

/// Viewer and team names; the queue itself is two search calls.
fn discover() -> Result<Discovery, String> {
    let user = gh_json(&["user"])?;
    let login = str_field(&user, "login")
        .ok_or_else(|| "gh api user returned no login".to_string())?
        .to_string();

    let (team_list, org_list) = std::thread::scope(|scope| {
        let teams = scope.spawn(|| gh_json_pages("user/teams?per_page=100"));
        let orgs = scope.spawn(|| gh_json_pages("user/orgs?per_page=100"));
        (
            teams
                .join()
                .unwrap_or_else(|_| Err("teams thread panicked".into())),
            orgs.join()
                .unwrap_or_else(|_| Err("orgs thread panicked".into())),
        )
    });
    let mut teams = Vec::new();
    if let Ok(list) = team_list {
        for team in &list {
            let org = team
                .get("organization")
                .and_then(|o| str_field(o, "login"))
                .unwrap_or("");
            if let (Some(slug), Some(name)) = (str_field(team, "slug"), str_field(team, "name")) {
                if !org.is_empty() {
                    teams.push((format!("{org}/{slug}"), name.to_string()));
                }
            }
        }
    }
    let orgs = match std::env::var("PR_APPROVER_ORGS")
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        Some(v) => v
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        None => org_list
            .unwrap_or_default()
            .iter()
            .filter_map(|o| str_field(o, "login").map(str::to_string))
            .collect(),
    };
    Ok(Discovery { login, teams, orgs })
}

/// The PR fields every queue search returns — reviewRequests carry the real
/// reason names, so rows never need a follow-up call to label them.
const GQL_PR: &str = r#"... on PullRequest {
  number title createdAt isDraft url
  author { login }
  repository { nameWithOwner }
  labels(first: 20) { nodes { name } }
  reviewRequests(first: 20) { nodes { requestedReviewer {
    __typename ... on User { login } ... on Team { name }
  } } }
}"#;

/// A GraphQL PullRequest node → queue row.
fn gql_pr(node: &Value, login: &str, unassigned: bool) -> Option<PrSummary> {
    let repo = node
        .get("repository")
        .and_then(|r| str_field(r, "nameWithOwner"))?
        .to_string();
    let number = node.get("number").and_then(Value::as_u64)?;
    let mut reasons = Vec::new();
    if let Some(reqs) = node
        .get("reviewRequests")
        .and_then(|r| r.get("nodes"))
        .and_then(Value::as_array)
    {
        for req in reqs {
            let r = req.get("requestedReviewer").cloned().unwrap_or_default();
            match str_field(&r, "__typename") {
                Some("User") if str_field(&r, "login") == Some(login) => {
                    reasons.push("You".to_string())
                }
                Some("Team") => reasons.push(str_field(&r, "name").unwrap_or("Team").to_string()),
                _ => {}
            }
        }
    }
    Some(PrSummary {
        key: PrKey::new(repo, number),
        title: str_field(node, "title").unwrap_or("").to_string(),
        author: node
            .get("author")
            .and_then(|a| str_field(a, "login"))
            .unwrap_or("")
            .to_string(),
        created_at: str_field(node, "createdAt").unwrap_or("").to_string(),
        draft: node
            .get("isDraft")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        reasons,
        blocked_label: node
            .get("labels")
            .and_then(|l| l.get("nodes"))
            .and_then(Value::as_array)
            .map(|labels| {
                labels.iter().any(|l| {
                    str_field(l, "name")
                        .map(|n| n.eq_ignore_ascii_case("blocked"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false),
        unassigned,
        head_sha: String::new(),
        url: str_field(node, "url").unwrap_or("").to_string(),
    })
}

/// The whole queue in ONE `gh api graphql` call: every search is an alias in a
/// single request. `review-requested:@me` covers direct requests; per-team
/// aliases cover team requests (`@me` is not a valid team name); the unassigned
/// triage query ORs the viewer's repos and orgs together.
/// Merge a result row into the queue: same PR unions reasons instead of
/// duplicating. `unassigned` means "only reachable via triage" — it ANDs so
/// a direct/team request keeps the row visible when triage also finds it.
fn merge_pr(prs: &mut Vec<PrSummary>, pr: PrSummary) {
    match prs.iter_mut().find(|p| p.key == pr.key) {
        Some(existing) => {
            for reason in pr.reasons {
                if !existing.reasons.contains(&reason) {
                    existing.reasons.push(reason);
                }
            }
            existing.unassigned &= pr.unassigned;
            existing.blocked_label |= pr.blocked_label;
        }
        None => prs.push(pr),
    }
}

/// The search aliases for one batch: `requested_only` picks the direct
/// review-request search (fast, paints the queue early); the rest covers
/// team requests and unassigned triage.
fn queue_aliases(disco: &Discovery, requested_only: bool) -> Vec<(String, bool, String)> {
    if requested_only {
        return vec![
            (
                "requested".to_string(),
                false,
                "is:pr is:open review-requested:@me".to_string(),
            ),
            // Assigned-but-unrequested (dependabot et al.) is its own scope —
            // it matches none of the review-requested searches.
            (
                "assigned".to_string(),
                false,
                "is:pr is:open assignee:@me".to_string(),
            ),
        ];
    }
    let mut out = Vec::new();
    for (i, (team, _)) in disco.teams.iter().enumerate() {
        out.push((
            format!("team{i}"),
            false,
            format!("is:pr is:open team-review-requested:{team}"),
        ));
    }
    // One alias per scope: user: and org: qualifiers can't share a
    // parenthesised OR (the combined query silently returns 0).
    out.push((
        "unassigned_user".to_string(),
        true,
        "is:pr is:open no:assignee -is:draft user:@me".to_string(),
    ));
    for (i, org) in disco.orgs.iter().enumerate() {
        out.push((
            format!("unassigned_org{i}"),
            true,
            format!("is:pr is:open no:assignee -is:draft org:{org}"),
        ));
    }
    out
}

/// One GraphQL call covering a set of search aliases.
fn search_queue_batch(
    disco: &Discovery,
    requested_only: bool,
) -> (Vec<PrSummary>, Vec<String>, Option<String>) {
    let specs = queue_aliases(disco, requested_only);
    let mut query = String::from("query {");
    for (alias, _, q) in &specs {
        query.push_str(&format!(
            " {alias}: search(first: 100, type: ISSUE, query: \"{q}\") {{ nodes {{ {GQL_PR} }} }}"
        ));
    }
    query.push_str(" }");

    let data = match gh_graphql(&query) {
        Ok(d) => d,
        Err(e) => return (Vec::new(), Vec::new(), Some(e)),
    };
    let mut prs = Vec::new();
    let mut failures = Vec::new();
    for (alias, unassigned, _) in &specs {
        let nodes = data
            .get(alias)
            .and_then(|s| s.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if !data.get(alias).is_some_and(|s| s.is_object()) {
            failures.push(format!("{alias}: no result"));
            continue;
        }
        for node in &nodes {
            if let Some(mut pr) = gql_pr(node, &disco.login, *unassigned) {
                if *unassigned {
                    pr.unassigned = true;
                }
                if alias == "assigned" && pr.reasons.iter().all(|r| r != "assigned") {
                    pr.reasons.push("assigned".to_string());
                }
                merge_pr(&mut prs, pr);
            }
        }
    }
    (prs, failures, None)
}

fn load_detail(disco: &Discovery, key: &PrKey) -> Result<PrDetail, String> {
    let (owner, name) = key.repo.split_once('/').unwrap_or(("", &key.repo));
    let n = key.number;
    // Everything except the diff arrives in one GraphQL call.
    let detail_query = format!(
        r#"query {{ repository(owner: "{owner}", name: "{name}") {{ pullRequest(number: {n}) {{
            title body createdAt isDraft url additions deletions changedFiles
            headRefOid headRefName baseRefName
            author {{ login }}
            reviewRequests(first: 20) {{ nodes {{ requestedReviewer {{
                __typename ... on User {{ login }} ... on Team {{ name }}
            }} }} }}
            comments(first: 100) {{ nodes {{ author {{ login }} body createdAt }} }}
            reviews(first: 100) {{ nodes {{ author {{ login }} state submittedAt body }} }}
            reviewThreads(first: 100) {{ nodes {{ comments(first: 10) {{ nodes {{
                author {{ login }} body createdAt path line
            }} }} }} }}
            commits(last: 1) {{ nodes {{ commit {{ statusCheckRollup {{ contexts(first: 50) {{ nodes {{
                __typename ... on CheckRun {{ name status conclusion }}
                ... on StatusContext {{ context state }}
            }} }} }} }} }} }}
        }} }} }}"#
    );
    let repo = &key.repo;
    let (pr_result, diff) = std::thread::scope(|scope| {
        let g = scope.spawn(|| gh_graphql(&detail_query));
        let d = scope.spawn(|| {
            gh(&[
                &format!("repos/{repo}/pulls/{n}"),
                "-H",
                "Accept: application/vnd.github.diff",
            ])
            .unwrap_or_default()
        });
        (
            g.join().unwrap_or_else(|_| Err("panic".into())),
            d.join().unwrap_or_default(),
        )
    });
    let data = pr_result?;
    let pr = data
        .get("repository")
        .and_then(|r| r.get("pullRequest"))
        .ok_or_else(|| "PR not found".to_string())?
        .clone();

    // Discussion: comments, submitted reviews, inline review comments, oldest first.
    let mut events = Vec::new();
    if let Some(list) = pr
        .get("comments")
        .and_then(|c| c.get("nodes"))
        .and_then(Value::as_array)
    {
        for c in list {
            events.push(crate::model::Discussion {
                author: c
                    .get("author")
                    .and_then(|a| str_field(a, "login"))
                    .unwrap_or("")
                    .to_string(),
                created_at: str_field(c, "createdAt").unwrap_or("").to_string(),
                body: str_field(c, "body").unwrap_or("").to_string(),
                kind: crate::model::DiscussionKind::Comment,
            });
        }
    }
    if let Some(list) = pr
        .get("reviews")
        .and_then(|r| r.get("nodes"))
        .and_then(Value::as_array)
    {
        for r in list {
            let state = str_field(r, "state").unwrap_or("");
            if state.eq_ignore_ascii_case("pending") {
                continue;
            }
            events.push(crate::model::Discussion {
                author: r
                    .get("author")
                    .and_then(|a| str_field(a, "login"))
                    .unwrap_or("")
                    .to_string(),
                created_at: str_field(r, "submittedAt").unwrap_or("").to_string(),
                body: str_field(r, "body").unwrap_or("").to_string(),
                kind: crate::model::DiscussionKind::Review(state.to_string()),
            });
        }
    }
    if let Some(threads) = pr
        .get("reviewThreads")
        .and_then(|t| t.get("nodes"))
        .and_then(Value::as_array)
    {
        for t in threads {
            if let Some(cs) = t
                .get("comments")
                .and_then(|c| c.get("nodes"))
                .and_then(Value::as_array)
            {
                for c in cs {
                    events.push(crate::model::Discussion {
                        author: c
                            .get("author")
                            .and_then(|a| str_field(a, "login"))
                            .unwrap_or("")
                            .to_string(),
                        created_at: str_field(c, "createdAt").unwrap_or("").to_string(),
                        body: str_field(c, "body").unwrap_or("").to_string(),
                        kind: crate::model::DiscussionKind::ReviewComment {
                            path: str_field(c, "path").unwrap_or("").to_string(),
                            line: c.get("line").and_then(Value::as_u64),
                        },
                    });
                }
            }
        }
    }
    events.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    // Checks via the status check rollup (CheckRun + StatusContext).
    let mut checks: Vec<Check> = Vec::new();
    if let Some(nodes) = pr
        .get("commits")
        .and_then(|c| c.get("nodes"))
        .and_then(Value::as_array)
        .and_then(|ns| ns.first())
        .and_then(|n| n.get("commit"))
        .and_then(|c| c.get("statusCheckRollup"))
        .and_then(|s| s.get("contexts"))
        .and_then(|c| c.get("nodes"))
        .and_then(Value::as_array)
    {
        for ctx in nodes {
            match str_field(ctx, "__typename") {
                Some("CheckRun") => checks.push(Check {
                    name: str_field(ctx, "name").unwrap_or("check").to_string(),
                    status: str_field(ctx, "status").unwrap_or("").to_ascii_lowercase(),
                    conclusion: ctx
                        .get("conclusion")
                        .and_then(Value::as_str)
                        .map(|s| s.to_ascii_lowercase()),
                }),
                Some("StatusContext") => checks.push(Check {
                    name: str_field(ctx, "context").unwrap_or("check").to_string(),
                    status: match str_field(ctx, "state").unwrap_or("") {
                        "PENDING" | "EXPECTED" => "in_progress".to_string(),
                        _ => "completed".to_string(),
                    },
                    conclusion: ctx
                        .get("state")
                        .and_then(Value::as_str)
                        .map(|s| s.to_ascii_lowercase()),
                }),
                _ => {}
            }
        }
    }

    // Reasons straight off the node — no enrichment call.
    let mut requested = Vec::new();
    if let Some(reqs) = pr
        .get("reviewRequests")
        .and_then(|r| r.get("nodes"))
        .and_then(Value::as_array)
    {
        for req in reqs {
            let r = req.get("requestedReviewer").cloned().unwrap_or_default();
            match str_field(&r, "__typename") {
                Some("User") if str_field(&r, "login") == Some(disco.login.as_str()) => {
                    requested.push("You".to_string())
                }
                Some("Team") => requested.push(str_field(&r, "name").unwrap_or("Team").to_string()),
                _ => {}
            }
        }
    }

    Ok(PrDetail {
        key: key.clone(),
        title: str_field(&pr, "title").unwrap_or("").to_string(),
        body: str_field(&pr, "body").unwrap_or("").to_string(),
        author: pr
            .get("author")
            .and_then(|a| str_field(a, "login"))
            .unwrap_or("")
            .to_string(),
        url: str_field(&pr, "url").unwrap_or("").to_string(),
        head_sha: str_field(&pr, "headRefOid").unwrap_or("").to_string(),
        base_ref: str_field(&pr, "baseRefName").unwrap_or("").to_string(),
        head_ref: str_field(&pr, "headRefName").unwrap_or("").to_string(),
        draft: pr.get("isDraft").and_then(Value::as_bool).unwrap_or(false),
        additions: pr.get("additions").and_then(Value::as_u64).unwrap_or(0),
        deletions: pr.get("deletions").and_then(Value::as_u64).unwrap_or(0),
        changed_files: pr.get("changedFiles").and_then(Value::as_u64).unwrap_or(0),
        diff,
        checks,
        events,
        requested,
    })
}

/// Remove the Blocked label.
fn unblock_pr(key: &PrKey) -> Result<(), String> {
    gh(&[
        "-X",
        "DELETE",
        &format!("repos/{}/issues/{}/labels/Blocked", key.repo, key.number),
    ])
    .map(|_| ())
}

fn submit_review(
    disco: &Discovery,
    key: &PrKey,
    kind: ReviewKind,
    body: &str,
) -> Result<Option<bool>, String> {
    let repo = &key.repo;
    let n = key.number;
    let pr = gh_json(&[&format!("repos/{repo}/pulls/{n}")])?;
    if str_field(&pr, "state") != Some("open") {
        return Err("this pull request is closed".to_string());
    }
    if pr.get("user").and_then(|u| str_field(u, "login")) == Some(disco.login.as_str()) {
        return Err("you cannot review your own pull request".to_string());
    }
    if let Ok(reviews) = gh_json_pages(&format!("repos/{repo}/pulls/{n}/reviews?per_page=100")) {
        let pending = reviews.iter().any(|r| {
            str_field(r, "state")
                .map(|s| s.eq_ignore_ascii_case("pending"))
                .unwrap_or(false)
                && r.get("user").and_then(|u| str_field(u, "login")) == Some(disco.login.as_str())
        });
        if pending {
            return Err("you already have a pending review on this pull request".to_string());
        }
    }

    let payload = serde_json::json!({ "event": kind.event(), "body": body });
    gh_input(
        &[
            &format!("repos/{repo}/pulls/{n}/reviews"),
            "-X",
            "POST",
            "--input",
            "-",
        ],
        Some(&payload),
    )?;

    if kind == ReviewKind::Blocked {
        let labels = serde_json::json!({ "labels": ["Blocked"] });
        let added = gh_input(
            &[
                &format!("repos/{repo}/issues/{n}/labels"),
                "-X",
                "POST",
                "--input",
                "-",
            ],
            Some(&labels),
        );
        let labelled = match added {
            Ok(_) => Some(true),
            Err(_) => {
                // The label may not exist yet: create it, then retry once.
                let create = serde_json::json!({ "name": "Blocked", "color": "b60205" });
                let _ = gh_input(
                    &[
                        &format!("repos/{repo}/labels"),
                        "-X",
                        "POST",
                        "--input",
                        "-",
                    ],
                    Some(&create),
                );
                gh_input(
                    &[
                        &format!("repos/{repo}/issues/{n}/labels"),
                        "-X",
                        "POST",
                        "--input",
                        "-",
                    ],
                    Some(&labels),
                )
                .map(|_| true)
                .ok()
            }
        };
        return Ok(labelled);
    }
    Ok(None)
}

/// The service loop: discovery once, then commands on demand. Each `Refresh`
/// bumps the generation, which is the cancellation — stale scans stop taking
/// work and their events are ignored.
fn service_loop(rx: Receiver<Request>, ev: Sender<Event>, wake: Arc<dyn Fn() + Send + Sync>) {
    let discovery: Discovery = match discover() {
        Ok(d) => d,
        Err(e) => {
            let _ = ev.send(Event::AuthFailed(e));
            wake();
            Discovery {
                login: String::new(),
                teams: Vec::new(),
                orgs: Vec::new(),
            }
        }
    };
    let _ = ev.send(Event::Discovered {
        login: discovery.login.clone(),
    });
    wake();

    let generation = Arc::new(AtomicU64::new(0));
    for request in rx {
        match request {
            Request::Refresh => {
                let gen = generation.fetch_add(1, Ordering::AcqRel) + 1;
                let (ev, wake, current, disco) = (
                    ev.clone(),
                    wake.clone(),
                    generation.clone(),
                    discovery.clone(),
                );
                std::thread::spawn(move || {
                    let _ = ev.send(Event::Stage {
                        gen,
                        text: "Searching review requests…".to_string(),
                    });
                    wake();
                    // The direct-request search paints the queue first; team
                    // and unassigned-triage searches land in a second result.
                    std::thread::scope(|scope| {
                        let rest = scope.spawn(|| search_queue_batch(&disco, false));
                        let (mut prs, mut failures, fatal) = search_queue_batch(&disco, true);
                        if let Some(error) = fatal {
                            let _ = ev.send(Event::ScanFailed { gen, error });
                            wake();
                            return;
                        }
                        let _ = ev.send(Event::ScanResult {
                            gen,
                            prs: prs.clone(),
                        });
                        wake();
                        let (rest_prs, rest_failures, rest_fatal) =
                            rest.join().unwrap_or_else(|_| {
                                (
                                    Vec::new(),
                                    Vec::new(),
                                    Some("triage thread panicked".into()),
                                )
                            });
                        for pr in rest_prs {
                            merge_pr(&mut prs, pr);
                        }
                        failures.extend(rest_failures);
                        if let Some(e) = rest_fatal {
                            failures.push(e);
                        }
                        let _ = ev.send(Event::ScanResult { gen, prs });
                        for error in failures {
                            let _ = ev.send(Event::ScanFailed { gen, error });
                        }
                        if gen == current.load(Ordering::Acquire) {
                            let _ = ev.send(Event::SyncFinished { gen });
                        }
                        wake();
                    });
                });
            }
            Request::Unblock(key) => {
                let (ev, wake) = (ev.clone(), wake.clone());
                std::thread::spawn(move || {
                    let event = match unblock_pr(&key) {
                        Ok(()) => Event::Unblocked { key },
                        Err(error) => Event::UnblockFailed { key, error },
                    };
                    let _ = ev.send(event);
                    wake();
                });
            }
            Request::LoadDetail(key) => {
                let (ev, wake, disco) = (ev.clone(), wake.clone(), discovery.clone());
                std::thread::spawn(move || {
                    match load_detail(&disco, &key) {
                        Ok(detail) => {
                            let _ = ev.send(Event::DetailLoaded {
                                key,
                                detail: Box::new(detail),
                            });
                        }
                        Err(error) => {
                            let _ = ev.send(Event::DetailFailed { key, error });
                        }
                    }
                    wake();
                });
            }
            Request::SubmitReview { key, kind, body } => {
                let (ev, wake, disco) = (ev.clone(), wake.clone(), discovery.clone());
                std::thread::spawn(move || {
                    match submit_review(&disco, &key, kind, &body) {
                        Ok(labelled) => {
                            let _ = ev.send(Event::ReviewSubmitted {
                                key,
                                kind,
                                labelled,
                            });
                        }
                        Err(error) => {
                            let _ = ev.send(Event::ReviewFailed { key, error });
                        }
                    }
                    wake();
                });
            }
        }
    }
}

/// Start the service thread. Returns the command channel; `events` lands in the
/// receiver, and each send wakes the window through `waker`.
pub fn spawn(waker: impl Fn() + Send + Sync + 'static) -> (Sender<Request>, Receiver<Event>) {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
    let (evt_tx, evt_rx) = std::sync::mpsc::channel();
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(waker);
    std::thread::spawn(move || service_loop(cmd_rx, evt_tx, wake));
    (cmd_tx, evt_rx)
}

/// Time the real scan path end-to-end (`--time-queue`), printing wall
/// time per stage so boot cost is measured, not estimated.
pub fn timing_probe() {
    let t0 = std::time::Instant::now();
    eprintln!("probe: starting");
    let disco = match discover() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("probe: discovery FAILED after {:?}: {e}", t0.elapsed());
            return;
        }
    };
    eprintln!(
        "probe: discover  {:?} ({} teams, {} orgs)",
        t0.elapsed(),
        disco.teams.len(),
        disco.orgs.len()
    );
    let t1 = std::time::Instant::now();
    let mut prs = Vec::new();
    let mut failures = Vec::new();
    std::thread::scope(|scope| {
        let rest = scope.spawn(|| search_queue_batch(&disco, false));
        let (first, f, fatal) = search_queue_batch(&disco, true);
        eprintln!(
            "probe: first-paint {:?} (+{:?})  {} prs, fatal={fatal:?}",
            t0.elapsed(),
            t1.elapsed(),
            first.len()
        );
        prs = first;
        failures = f;
        let (rest_prs, rf, _) = rest
            .join()
            .unwrap_or_else(|_| (Vec::new(), Vec::new(), None));
        eprintln!(
            "probe: complete   {:?} (+{:?})  {} rest prs",
            t0.elapsed(),
            t1.elapsed(),
            rest_prs.len()
        );
        for pr in rest_prs {
            merge_pr(&mut prs, pr);
        }
        failures.extend(rf);
    });
    if let Some(key) = prs.first().map(|p| p.key.clone()) {
        let t2 = std::time::Instant::now();
        match load_detail(&disco, &key) {
            Ok(d) => eprintln!(
                "probe: detail    {:?} (+{:?})  {} checks, {} events",
                t0.elapsed(),
                t2.elapsed(),
                d.checks.len(),
                d.events.len()
            ),
            Err(e) => eprintln!(
                "probe: detail    {:?} (+{:?})  FAILED: {e}",
                t0.elapsed(),
                t2.elapsed()
            ),
        }
    }
    eprintln!("probe: total     {:?}", t0.elapsed());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disco() -> Discovery {
        Discovery {
            login: "me".into(),
            teams: vec![("thisisfocus/owners".into(), "Owners".into())],
            orgs: vec!["thisisfocus".into(), "2coloursblue".into()],
        }
    }

    /// Resolution never panics/hangs; every invocation is a bare `gh` (PATH
    /// lookup), an existing binary, or an existing shell.
    #[test]
    fn gh_resolution_is_sane() {
        let cmds = gh_cmds();
        assert!(!cmds.is_empty());
        for c in cmds {
            match c {
                GhCmd::Bin(p) => {
                    assert!(p == "gh" || std::path::Path::new(p).exists(), "bad gh: {p}")
                }
                GhCmd::Shell(s) => {
                    assert!(std::path::Path::new(s).exists(), "bad shell: {s}")
                }
            }
        }
    }

    /// user: and org: qualifiers can't share a parenthesised OR — the
    /// combined query 422s/silently returns 0. Each scope gets its own alias.
    #[test]
    fn unassigned_uses_one_alias_per_scope() {
        let specs = queue_aliases(&disco(), false);
        let unassigned: Vec<_> = specs
            .iter()
            .filter(|(a, _, _)| a.starts_with("unassigned"))
            .collect();
        assert_eq!(
            unassigned.len(),
            3,
            "expected user + 2 org scopes: {unassigned:?}"
        );
        for (_, _, q) in &unassigned {
            assert!(!q.contains(" OR "), "scope must be single-qualifier: {q}");
            assert!(!q.contains('('), "no parenthesised groups: {q}");
        }
        assert!(unassigned.iter().any(|(_, _, q)| q.contains("user:@me")));
        assert!(unassigned
            .iter()
            .any(|(_, _, q)| q.contains("org:thisisfocus")));
        assert!(unassigned
            .iter()
            .any(|(_, _, q)| q.contains("org:2coloursblue")));
    }

    fn pr(n: u64, unassigned: bool) -> PrSummary {
        PrSummary {
            key: PrKey::new("acme/web", n),
            title: "t".into(),
            author: "dev".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            draft: false,
            reasons: vec![],
            unassigned,
            blocked_label: false,
            head_sha: String::new(),
            url: String::new(),
        }
    }

    /// A PR reachable through the direct/team searches must stay visible when
    /// the unassigned triage also finds it — `unassigned` means "ONLY
    /// reachable via triage", so it merges with AND, not OR. Reasons union.
    #[test]
    fn merge_dedupes_reasons_and_ands_unassigned() {
        let mut prs = vec![pr(1, false)];
        let mut dupe = pr(1, true);
        dupe.reasons = vec!["Owners".into()];
        merge_pr(&mut prs, dupe);
        assert_eq!(prs.len(), 1);
        assert!(
            !prs[0].unassigned,
            "a directly-requested PR must not hide when triage also finds it"
        );
        assert_eq!(prs[0].reasons, vec!["Owners"]);

        let mut prs = vec![pr(2, true)];
        merge_pr(&mut prs, pr(2, true));
        assert!(prs[0].unassigned, "a triage-only PR stays unassigned");
        merge_pr(&mut prs, pr(3, true));
        assert_eq!(prs.len(), 2);
    }

    /// A search node parses into a summary with title/author/flags/reasons.
    #[test]
    fn gql_pr_parses_node() {
        let node = serde_json::json!({
            "number": 42, "title": "Ship it", "createdAt": "2026-01-01T00:00:00Z",
            "isDraft": false, "url": "https://github.com/acme/web/pull/42",
            "author": {"login": "dev"}, "repository": {"nameWithOwner": "acme/web"},
            "labels": {"nodes": [{"name": "Blocked"}]},
            "reviewRequests": {"nodes": [
                {"requestedReviewer": {"__typename": "User", "login": "me"}},
                {"requestedReviewer": {"__typename": "Team", "name": "Owners"}},
            ]},
        });
        let pr = gql_pr(&node, "me", true).expect("node should parse");
        assert_eq!(pr.key.number, 42);
        assert_eq!(pr.author, "dev");
        assert!(pr.unassigned);
        assert!(pr.blocked_label, "blocked label should map");
        assert!(pr.reasons.iter().any(|r| r.contains("Owners")));
    }

    /// The direct-request + assigned searches paint first, in their own batch.
    #[test]
    fn requested_is_its_own_batch() {
        let specs = queue_aliases(&disco(), true);
        assert_eq!(specs.len(), 2);
        assert!(specs
            .iter()
            .any(|(a, _, q)| a == "requested" && q.contains("review-requested:@me")));
        assert!(specs
            .iter()
            .any(|(a, _, q)| a == "assigned" && q.contains("assignee:@me")));
    }
}
