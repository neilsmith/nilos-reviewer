//! Domain types and pure queue logic for the review queue.

use std::collections::{HashMap, HashSet};

/// Identifies a pull request: `owner/name#number`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PrKey {
    pub repo: String,
    pub number: u64,
}

impl PrKey {
    pub fn new(repo: impl Into<String>, number: u64) -> Self {
        PrKey { repo: repo.into(), number }
    }
    pub fn display(&self) -> String {
        format!("{}#{}", self.repo, self.number)
    }
}

/// A queued pull request summary, as the scan reports it.
#[derive(Clone, Debug)]
pub struct PrSummary {
    pub key: PrKey,
    pub title: String,
    pub author: String,
    /// ISO-8601 creation time; lexicographic order is chronological.
    pub created_at: String,
    pub draft: bool,
    /// Why it is queued: "You", team names or "Unassigned", deduped.
    pub reasons: Vec<String>,
    /// Open, non-draft, nobody assigned — a triage row, hidden unless the
    /// Unassigned toggle is on.
    pub unassigned: bool,
    /// The server carries a `blocked` label (case-insensitive).
    pub blocked_label: bool,
    pub head_sha: String,
    pub url: String,
}

/// Everything the detail pane draws for one pull request.
#[derive(Clone, Debug)]
pub struct PrDetail {
    pub key: PrKey,
    pub title: String,
    pub body: String,
    pub author: String,
    pub url: String,
    pub head_sha: String,
    pub base_ref: String,
    pub head_ref: String,
    pub draft: bool,
    pub additions: u64,
    pub deletions: u64,
    pub changed_files: u64,
    /// Raw unified diff for the whole pull request.
    pub diff: String,
    pub checks: Vec<Check>,
    /// The conversation so far: issue comments, reviews and inline comments,
    /// oldest first.
    pub events: Vec<Discussion>,
    /// Outstanding review requests on the full PR: "You" and team names.
    pub requested: Vec<String>,
}

/// One entry in the pull request discussion.
#[derive(Clone, Debug)]
pub struct Discussion {
    pub author: String,
    pub created_at: String,
    pub body: String,
    pub kind: DiscussionKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DiscussionKind {
    /// An issue-conversation comment.
    Comment,
    /// A submitted review, e.g. APPROVED or CHANGES_REQUESTED.
    Review(String),
    /// An inline comment on a diff line: `path:line`.
    ReviewComment { path: String, line: Option<u64> },
}

#[derive(Clone, Debug)]
pub struct Check {
    pub name: String,
    /// queued | in_progress | completed
    pub status: String,
    /// success | failure | neutral | cancelled | skipped | timed_out | action_required
    pub conclusion: Option<String>,
}

impl Check {
    pub fn ok(&self) -> Option<bool> {
        if self.status != "completed" {
            return None;
        }
        Some(matches!(self.conclusion.as_deref(), Some("success") | Some("neutral") | Some("skipped")))
    }
}

/// What a submitted review does on GitHub.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewKind {
    Approve,
    Comment,
    RequestChanges,
    /// A COMMENT review plus a `Blocked` label; the row hides itself.
    Blocked,
}

impl ReviewKind {
    pub fn event(&self) -> &'static str {
        match self {
            ReviewKind::Approve => "APPROVE",
            ReviewKind::Comment | ReviewKind::Blocked => "COMMENT",
            ReviewKind::RequestChanges => "REQUEST_CHANGES",
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            ReviewKind::Approve => "Approve",
            ReviewKind::Comment => "Comment",
            ReviewKind::RequestChanges => "Request changes",
            ReviewKind::Blocked => "Blocked",
        }
    }
    /// GitHub accepts an empty body on approve and request-changes; comment and
    /// blocked need one to say anything.
    pub fn needs_body(&self) -> bool {
        matches!(self, ReviewKind::Comment | ReviewKind::Blocked)
    }
}

/// A repository's queued pull requests, newest first.
#[derive(Clone, Debug, Default)]
pub struct RepoGroup {
    pub repo: String,
    pub prs: Vec<PrSummary>,
}

/// One visible row in the flattened queue.
#[derive(Clone, Debug, PartialEq)]
pub enum QueueRow {
    Header(String),
    Pr(PrKey),
}

/// Inputs deciding which rows the queue shows.
pub struct QueueFilter<'a> {
    pub collapsed: &'a HashSet<String>,
    pub filter: &'a str,
    pub show_blocked: bool,
    pub show_unassigned: bool,
    pub hide_done: bool,
    pub blocked: &'a HashSet<PrKey>,
    pub handled: &'a HashSet<PrKey>,
}

impl PrSummary {
    pub fn is_blocked(&self, blocked: &HashSet<PrKey>) -> bool {
        self.blocked_label || blocked.contains(&self.key)
    }
    pub fn matches(&self, filter: &str) -> bool {
        let filter = filter.trim();
        if filter.is_empty() {
            return true;
        }
        let haystack = format!(
            "{} {} {} {}",
            self.key.repo,
            self.key.number,
            self.title,
            self.reasons.join(" ")
        )
        .to_lowercase();
        filter.to_lowercase().split_whitespace().all(|w| haystack.contains(w))
    }
}

/// Groups per-repo results, drops empty repos, sorts rows newest-first.
pub fn group_by_repository(prs: Vec<PrSummary>) -> Vec<RepoGroup> {
    let mut order = Vec::<String>::new();
    let mut by_repo = HashMap::<String, Vec<PrSummary>>::new();
    for pr in prs {
        let repo = pr.key.repo.clone();
        if !by_repo.contains_key(&repo) {
            order.push(repo.clone());
        }
        by_repo.entry(repo).or_default().push(pr);
    }
    order.sort();
    order
        .into_iter()
        .filter_map(|repo| {
            let mut prs = by_repo.remove(&repo)?;
            prs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            (!prs.is_empty()).then_some(RepoGroup { repo, prs })
        })
        .collect()
}

/// The queue's visible rows in tree order: a header per visible repo, then its
/// PR rows unless collapsed.
pub fn flattened_rows(groups: &[RepoGroup], f: &QueueFilter) -> Vec<QueueRow> {
    let mut rows = Vec::new();
    for group in groups {
        let prs: Vec<&PrSummary> = group
            .prs
            .iter()
            .filter(|pr| {
                (f.show_blocked || !pr.is_blocked(f.blocked))
                    && (f.show_unassigned || !pr.unassigned)
                    && (!f.hide_done || !f.handled.contains(&pr.key))
                    && pr.matches(f.filter)
            })
            .collect();
        if prs.is_empty() {
            continue;
        }
        rows.push(QueueRow::Header(group.repo.clone()));
        if !f.collapsed.contains(&group.repo) {
            rows.extend(prs.into_iter().map(|pr| QueueRow::Pr(pr.key.clone())));
        }
    }
    rows
}

/// The PR rows in order, for cursor movement.
pub fn pr_rows(rows: &[QueueRow]) -> Vec<PrKey> {
    rows.iter()
        .filter_map(|r| match r {
            QueueRow::Pr(k) => Some(k.clone()),
            _ => None,
        })
        .collect()
}

/// Move the cursor `delta` PR rows through `rows`; `current` may be absent.
pub fn move_cursor(rows: &[QueueRow], current: Option<&PrKey>, delta: i32) -> Option<PrKey> {
    let keys = pr_rows(rows);
    if keys.is_empty() {
        return None;
    }
    let idx = current.and_then(|c| keys.iter().position(|k| k == c));
    let next = match (idx, delta >= 0) {
        (None, true) => 0,
        (None, false) => keys.len() - 1,
        (Some(i), _) => (i as i32 + delta).clamp(0, keys.len() as i32 - 1) as usize,
    };
    Some(keys[next].clone())
}

/// The next queued PR after `cursor` that is neither handled, blocked nor
/// mid-submit; wraps to the top when nothing below qualifies.
pub fn next_unhandled(
    rows: &[QueueRow],
    cursor: Option<&PrKey>,
    handled: &HashSet<PrKey>,
    blocked: &HashSet<PrKey>,
    submitting: &HashMap<PrKey, ReviewKind>,
) -> Option<PrKey> {
    let keys = pr_rows(rows);
    let start = cursor
        .and_then(|c| keys.iter().position(|k| k == c))
        .map(|p| p + 1)
        .unwrap_or(0);
    keys.iter()
        .cycle()
        .skip(start)
        .take(keys.len())
        .find(|k| !handled.contains(*k) && !blocked.contains(*k) && !submitting.contains_key(*k))
        .cloned()
}

/// Normalize a GitHub body: authored Markdown mostly passes through; the
/// common embedded HTML (`<details><summary>`, `<br>`) is unrolled so no
/// discussion content is invisible.
pub fn normalize_body(body: &str) -> String {
    let mut out = String::with_capacity(body.len() + 8);
    let mut rest = body;
    while let Some(open) = rest.find("<details") {
        out.push_str(&rest[..open]);
        let after_open = &rest[open..];
        let Some(tag_end) = after_open.find('>') else {
            break;
        };
        let inner = &after_open[tag_end + 1..];
        let Some(close) = inner.find("</details>") else {
            out.push_str(&rest[open..]);
            rest = "";
            break;
        };
        let content = &inner[..close];
        rest = &inner[close + "</details>".len()..];
        out.push_str("\n\n");
        // <summary>Title</summary> reads as a bold heading line.
        if let (Some(s0), Some(s1)) = (content.find("<summary>"), content.find("</summary>")) {
            if s0 < s1 {
                out.push_str(&format!("**{}**\n\n", content[s0 + "<summary>".len()..s1].trim()));
                out.push_str(content[s1 + "</summary>".len()..].trim());
            } else {
                out.push_str(content.trim());
            }
        } else {
            out.push_str(content.trim());
        }
        out.push_str("\n\n");
    }
    out.push_str(rest);
    out.replace("<br>", "\n").replace("<br/>", "\n").replace("<br />", "\n")
}

/// Seconds-to-human age ("3d", "5h", "12m").
pub fn age_of(created_at: &str, now: i64) -> String {
    let created = parse_iso(created_at).unwrap_or(now);
    let secs = (now - created).max(0);
    if secs >= 86_400 {
        format!("{}d", secs / 86_400)
    } else if secs >= 3_600 {
        format!("{}h", secs / 3_600)
    } else if secs >= 60 {
        format!("{}m", secs / 60)
    } else {
        "now".to_string()
    }
}

/// Seconds since the epoch for an ISO-8601 `YYYY-MM-DDTHH:MM:SSZ` timestamp.
pub fn parse_iso(s: &str) -> Option<i64> {
    let (date, time) = s.trim_end_matches('Z').split_once('T')?;
    let mut d = date.split('-');
    let (y, mo, da): (i64, i64, i64) =
        (d.next()?.parse().ok()?, d.next()?.parse().ok()?, d.next()?.parse().ok()?);
    let mut t = time.split(':');
    let (h, mi, se): (i64, i64, i64) =
        (t.next()?.parse().ok()?, t.next()?.parse().ok()?, t.next()?.parse().ok()?);
    // Days since epoch via Howard Hinnant's civil-from-days inverse.
    let y = if mo <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + da - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3_600 + mi * 60 + se)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(repo: &str, n: u64, title: &str, created: &str) -> PrSummary {
        PrSummary {
            key: PrKey::new(repo, n),
            title: title.into(),
            author: "dev".into(),
            created_at: created.into(),
            draft: false,
            reasons: vec!["You".into()],
            unassigned: false,
            blocked_label: false,
            head_sha: "abc".into(),
            url: String::new(),
        }
    }

    fn filter<'a>(
        collapsed: &'a HashSet<String>,
        blocked: &'a HashSet<PrKey>,
        handled: &'a HashSet<PrKey>,
    ) -> QueueFilter<'a> {
        QueueFilter {
            collapsed,
            filter: "",
            show_blocked: false,
            show_unassigned: false,
            hide_done: false,
            blocked,
            handled,
        }
    }

    #[test]
    fn groups_sort_repos_and_newest_first() {
        let prs = vec![
            pr("b/two", 2, "two-old", "2026-01-01T00:00:00Z"),
            pr("a/one", 1, "one", "2026-01-03T00:00:00Z"),
            pr("b/two", 3, "two-new", "2026-01-02T00:00:00Z"),
        ];
        let groups = group_by_repository(prs);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].repo, "a/one");
        assert_eq!(groups[1].repo, "b/two");
        assert_eq!(groups[1].prs[0].key.number, 3);
        assert_eq!(groups[1].prs[1].key.number, 2);
    }

    #[test]
    fn flattened_rows_hides_blocked_and_collapsed() {
        let mut blocked_pr = pr("a/one", 1, "blocked", "2026-01-01T00:00:00Z");
        blocked_pr.blocked_label = true;
        let groups = group_by_repository(vec![blocked_pr, pr("a/one", 2, "open", "2026-01-02T00:00:00Z"), pr("b/two", 3, "other", "2026-01-03T00:00:00Z")]);
        let (collapsed, blocked, handled) = (HashSet::new(), HashSet::new(), HashSet::new());
        let rows = flattened_rows(&groups, &filter(&collapsed, &blocked, &handled));
        assert_eq!(
            rows,
            vec![
                QueueRow::Header("a/one".into()),
                QueueRow::Pr(PrKey::new("a/one", 2)),
                QueueRow::Header("b/two".into()),
                QueueRow::Pr(PrKey::new("b/two", 3)),
            ]
        );
        let collapsed = HashSet::from(["a/one".to_string()]);
        let rows = flattened_rows(&groups, &filter(&collapsed, &blocked, &handled));
        assert_eq!(
            rows,
            vec![QueueRow::Header("a/one".into()), QueueRow::Header("b/two".into()), QueueRow::Pr(PrKey::new("b/two", 3))]
        );
        // Show blocked reveals blocked rows, but a collapsed repo still hides its PRs.
        let f = QueueFilter { show_blocked: true, ..filter(&collapsed, &blocked, &handled) };
        let rows = flattened_rows(&groups, &f);
        assert_eq!(rows.len(), 3);
        let uncollapsed = HashSet::new();
        let f = QueueFilter { show_blocked: true, ..filter(&uncollapsed, &blocked, &handled) };
        assert_eq!(flattened_rows(&groups, &f).len(), 5);
    }

    #[test]
    fn filter_matches_words_across_fields() {
        let (collapsed, blocked, handled) = (HashSet::new(), HashSet::new(), HashSet::new());
        let mut f = filter(&collapsed, &blocked, &handled);
        let groups = group_by_repository(vec![
            pr("acme/web", 1, "Fix login redirect", "2026-01-01T00:00:00Z"),
            pr("acme/api", 2, "Add metrics endpoint", "2026-01-02T00:00:00Z"),
        ]);
        f.filter = "login";
        assert_eq!(pr_rows(&flattened_rows(&groups, &f)).len(), 1);
        f.filter = "acme metrics";
        assert_eq!(pr_rows(&flattened_rows(&groups, &f))[0].number, 2);
        f.filter = "nope";
        assert!(flattened_rows(&groups, &f).is_empty());
    }

    #[test]
    fn cursor_moves_and_clamps() {
        let groups = group_by_repository(vec![
            pr("a/one", 1, "a", "2026-01-01T00:00:00Z"),
            pr("a/one", 2, "b", "2026-01-02T00:00:00Z"),
        ]);
        let (collapsed, blocked, handled) = (HashSet::new(), HashSet::new(), HashSet::new());
        let rows = flattened_rows(&groups, &filter(&collapsed, &blocked, &handled));
        let first = PrKey::new("a/one", 2);
        let last = PrKey::new("a/one", 1);
        assert_eq!(move_cursor(&rows, None, 1), Some(first.clone()));
        assert_eq!(move_cursor(&rows, Some(&first), 1), Some(last.clone()));
        assert_eq!(move_cursor(&rows, Some(&last), 1), Some(last.clone()));
        assert_eq!(move_cursor(&rows, None, -1), Some(last));
    }

    #[test]
    fn next_unhandled_skips_terminal_rows() {
        let groups = group_by_repository(vec![
            pr("a/one", 1, "a", "2026-01-01T00:00:00Z"),
            pr("a/one", 2, "b", "2026-01-02T00:00:00Z"),
        ]);
        let (collapsed, handled) = (HashSet::new(), HashSet::new());
        let blocked = HashSet::from([PrKey::new("a/one", 2)]);
        let f = QueueFilter { show_blocked: true, ..filter(&collapsed, &blocked, &handled) };
        let rows = flattened_rows(&groups, &f);
        assert_eq!(next_unhandled(&rows, None, &handled, &blocked, &HashMap::new()), Some(PrKey::new("a/one", 1)));
        let handled = HashSet::from([PrKey::new("a/one", 1)]);
        assert_eq!(next_unhandled(&rows, None, &handled, &blocked, &HashMap::new()), None);
        // From the cursor it wraps to the only unhandled row.
        let handled2 = HashSet::new();
        assert_eq!(
            next_unhandled(&rows, Some(&PrKey::new("a/one", 2)), &handled2, &blocked, &HashMap::new()),
            Some(PrKey::new("a/one", 1))
        );
    }

    #[test]
    fn iso_parses_and_ages() {
        assert_eq!(parse_iso("2026-01-02T03:04:05Z"), Some(1_767_323_045));
        assert_eq!(age_of("2026-01-01T00:00:00Z", parse_iso("2026-01-02T01:00:00Z").unwrap()), "1d");
        assert_eq!(age_of("2026-01-02T00:30:00Z", parse_iso("2026-01-02T01:00:00Z").unwrap()), "30m");
    }
}
