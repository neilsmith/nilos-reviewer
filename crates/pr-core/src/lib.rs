pub mod omarchy;

use std::collections::{BTreeMap, HashSet};
use std::fmt;

/// A stable pull request identity.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct PrKey {
    pub owner: String,
    pub repo: String,
    pub number: u64,
}

impl PrKey {
    pub fn new(owner: impl Into<String>, repo: impl Into<String>, number: u64) -> Self {
        Self {
            owner: owner.into(),
            repo: repo.into(),
            number,
        }
    }

    pub fn full_repo(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    pub fn display(&self) -> String {
        format!("{}#{}", self.full_repo(), self.number)
    }
}

impl fmt::Display for PrKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.display().fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReviewKind {
    Approve,
    Comment,
    RequestChanges,
    Blocked,
}

impl ReviewKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Approve => "Approve",
            Self::Comment => "Comment",
            Self::RequestChanges => "Request changes",
            Self::Blocked => "Mark as Blocked",
        }
    }

    pub fn github_event(self) -> &'static str {
        match self {
            Self::Approve => "APPROVE",
            Self::Comment | Self::Blocked => "COMMENT",
            Self::RequestChanges => "REQUEST_CHANGES",
        }
    }

    pub fn requires_body(self) -> bool {
        matches!(self, Self::Comment | Self::RequestChanges | Self::Blocked)
    }
}

/// Queue item for a currently requested review.
#[derive(Debug, Clone)]
pub struct PrSummary {
    pub key: PrKey,
    pub title: String,
    pub author: String,
    pub html_url: String,
    pub base_ref: String,
    pub head_ref: String,
    pub head_sha: String,
    pub created_at: String,
    pub updated_at: String,
    pub draft: bool,
    pub reasons: Vec<String>,
    /// The PR carries the `Blocked` label on GitHub.
    pub blocked: bool,
}

impl PrSummary {
    pub fn repo_name(&self) -> String {
        self.key.full_repo()
    }

    pub fn reason_label(&self) -> String {
        self.reasons.join(", ")
    }
}

/// A queue entry processed successfully during this app session.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HandledPr {
    pub key: PrKey,
    pub kind: ReviewKind,
    pub head_sha: String,
}

/// Current synchronization status shown under the queue.
#[derive(Debug, Default, Clone)]
pub struct SyncProgress {
    pub stage: String,
    pub repositories_total: usize,
    pub repositories_checked: usize,
    pub repositories_failed: usize,
    pub pull_requests_found: usize,
    pub started: bool,
    pub complete: bool,
}

impl SyncProgress {
    pub fn fraction(&self) -> f32 {
        if self.repositories_total == 0 {
            0.
        } else {
            self.repositories_checked as f32 / self.repositories_total as f32
        }
    }
}

/// A review conversation item, ordered chronologically.
#[derive(Debug, Clone)]
pub struct DiscussionEntry {
    pub kind: DiscussionKind,
    pub author: String,
    pub created_at: String,
    pub body: String,
    pub html_url: String,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscussionKind {
    Description,
    Comment,
    Review,
    InlineComment,
}

#[derive(Debug, Clone)]
pub struct CheckItem {
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub html_url: String,
}

#[derive(Debug, Default, Clone)]
pub struct CheckSummary {
    pub items: Vec<CheckItem>,
    pub pending: usize,
    pub failed: usize,
    pub successful: usize,
    pub reported: bool,
}

impl CheckSummary {
    pub fn label(&self) -> String {
        if !self.reported {
            return "No checks reported".into();
        }
        format!(
            "{} passed · {} pending · {} failed",
            self.successful, self.pending, self.failed
        )
    }
}

#[derive(Debug, Clone)]
pub struct PrFile {
    pub filename: String,
    pub previous_filename: Option<String>,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    pub changes: u64,
    pub patch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PrDetails {
    pub summary: PrSummary,
    pub body: String,
    pub discussion: Vec<DiscussionEntry>,
    pub checks: CheckSummary,
    pub files: Vec<PrFile>,
    pub files_complete: bool,
    pub pending_review_exists: bool,
    pub can_review: bool,
    pub review_notice: Option<String>,
    /// Login of the authenticated account reviews will be submitted as.
    pub viewer_login: String,
}

impl PrDetails {
    pub fn patch_document(&self) -> String {
        let mut patch = String::new();
        for file in &self.files {
            let Some(file_patch) = &file.patch else {
                continue;
            };
            let old_name = file.previous_filename.as_deref().unwrap_or(&file.filename);
            patch.push_str(&format!(
                "diff --git a/{} b/{}\n--- a/{}\n+++ b/{}\n{}\n",
                old_name, file.filename, old_name, file.filename, file_patch
            ));
        }
        patch
    }
}

/// One collapsible repository section.
#[derive(Debug, Clone)]
pub struct RepoGroup {
    pub name: String,
    pub rows: Vec<PrSummary>,
}

/// Group a stable, sorted queue by repository name.
pub fn group_by_repository(prs: &[PrSummary]) -> Vec<RepoGroup> {
    let mut grouped: BTreeMap<String, Vec<PrSummary>> = BTreeMap::new();
    for pr in prs {
        grouped.entry(pr.repo_name()).or_default().push(pr.clone());
    }
    grouped
        .into_iter()
        .map(|(name, mut rows)| {
            rows.sort_by(|a, b| {
                a.created_at
                    .cmp(&b.created_at)
                    .then(a.key.number.cmp(&b.key.number))
            });
            RepoGroup { name, rows }
        })
        .collect()
}

/// Flatten filtered groups into the order rendered in the sidebar.
pub fn flattened_rows(groups: &[RepoGroup], collapsed: &HashSet<String>) -> Vec<PrSummary> {
    groups
        .iter()
        .flat_map(|group| {
            if collapsed.contains(&group.name) {
                Vec::new()
            } else {
                group.rows.clone()
            }
        })
        .collect()
}

/// Filter queue entries without changing the displayed grouping order.
pub fn filtered_groups(
    groups: &[RepoGroup],
    query: &str,
    show_handled: bool,
    handled: &HashSet<PrKey>,
    show_blocked: bool,
    blocked: &HashSet<PrKey>,
) -> Vec<RepoGroup> {
    let query = query.trim().to_lowercase();
    groups
        .iter()
        .filter_map(|group| {
            let rows = group
                .rows
                .iter()
                .filter(|pr| show_handled || !handled.contains(&pr.key))
                .filter(|pr| show_blocked || !(pr.blocked || blocked.contains(&pr.key)))
                .filter(|pr| {
                    query.is_empty()
                        || group.name.to_lowercase().contains(&query)
                        || pr.title.to_lowercase().contains(&query)
                        || pr.key.number.to_string() == query.trim_start_matches('#')
                })
                .cloned()
                .collect::<Vec<_>>();
            (!rows.is_empty()).then(|| RepoGroup {
                name: group.name.clone(),
                rows,
            })
        })
        .collect()
}

/// The next unhandled row after `current`, wrapping to the beginning.
pub fn next_unhandled(
    rows: &[PrSummary],
    current: Option<&PrKey>,
    handled: &HashSet<PrKey>,
) -> Option<PrKey> {
    let candidates = rows
        .iter()
        .enumerate()
        .filter(|(_, pr)| !handled.contains(&pr.key));
    let Some((start_ix, _)) =
        current.and_then(|current| rows.iter().enumerate().find(|(_, pr)| &pr.key == current))
    else {
        return candidates
            .min_by_key(|(ix, _)| *ix)
            .map(|(_, pr)| pr.key.clone());
    };
    candidates
        .clone()
        .find(|(ix, _)| *ix > start_ix)
        .or_else(|| candidates.min_by_key(|(ix, _)| *ix))
        .map(|(_, pr)| pr.key.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(number: u64, repo: &str) -> PrSummary {
        PrSummary {
            key: PrKey::new("o", repo, number),
            title: format!("PR {number}"),
            author: "author".into(),
            html_url: String::new(),
            base_ref: "main".into(),
            head_ref: "branch".into(),
            head_sha: "sha".into(),
            created_at: format!("2026-01-{number:02}T00:00:00Z"),
            updated_at: String::new(),
            draft: false,
            reasons: vec!["You".into()],
            blocked: false,
        }
    }

    #[test]
    fn groups_and_advances_across_repositories() {
        let queue = vec![pr(2, "a"), pr(1, "b"), pr(3, "a")];
        let groups = group_by_repository(&queue);
        assert_eq!(groups[0].name, "o/a");
        assert_eq!(groups[0].rows[0].key.number, 2);
        assert_eq!(groups[1].rows[0].key.number, 1);

        let flat = flattened_rows(&groups, &HashSet::new());
        let first = flat[0].key.clone();
        let handled = HashSet::from([first.clone()]);
        assert_eq!(
            next_unhandled(&flat, Some(&first), &handled)
                .expect("an unhandled PR remains")
                .number,
            3
        );
    }

    fn blocked_pr(number: u64, repo: &str) -> PrSummary {
        PrSummary {
            blocked: true,
            ..pr(number, repo)
        }
    }

    fn filter(queue: &[PrSummary], show_blocked: bool, blocked: &HashSet<PrKey>) -> Vec<RepoGroup> {
        filtered_groups(
            &group_by_repository(queue),
            "",
            true,
            &HashSet::new(),
            show_blocked,
            blocked,
        )
    }

    #[test]
    fn blocked_rows_hidden_unless_toggled() {
        let queue = vec![pr(1, "a"), blocked_pr(2, "a")];
        let hidden = filter(&queue, false, &HashSet::new());
        assert_eq!(hidden[0].rows.len(), 1);
        assert_eq!(hidden[0].rows[0].key.number, 1);

        let shown = filter(&queue, true, &HashSet::new());
        assert_eq!(shown[0].rows.len(), 2);
    }

    #[test]
    fn locally_blocked_marks_hide_without_server_label() {
        // A just-marked PR has no server label yet; the local set must hide it.
        let queue = vec![pr(1, "a"), pr(2, "a")];
        let blocked = HashSet::from([queue[1].key.clone()]);
        let groups = filter(&queue, false, &blocked);
        assert_eq!(groups[0].rows.len(), 1);
        assert_eq!(groups[0].rows[0].key.number, 1);

        // And reappears under Show blocked.
        let shown = filter(&queue, true, &blocked);
        assert_eq!(shown[0].rows.len(), 2);
    }

    #[test]
    fn advance_skips_handled_and_wraps() {
        let queue = vec![pr(1, "a"), pr(2, "a"), pr(3, "a")];
        let rows = flattened_rows(&group_by_repository(&queue), &HashSet::new());
        let handled = HashSet::from([rows[1].key.clone()]);

        // From row 0 the next unhandled is row 2 (row 1 is handled).
        assert_eq!(
            next_unhandled(&rows, Some(&rows[0].key), &handled)
                .expect("unhandled PR remains")
                .number,
            3
        );
        // From the last row it wraps to the first unhandled.
        assert_eq!(
            next_unhandled(&rows, Some(&rows[2].key), &handled)
                .expect("unhandled PR remains")
                .number,
            1
        );
        // With no selection it starts at the first unhandled row.
        assert_eq!(
            next_unhandled(&rows, None, &handled)
                .expect("unhandled PR remains")
                .number,
            1
        );
    }
}
