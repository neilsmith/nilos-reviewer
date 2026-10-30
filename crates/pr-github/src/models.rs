use pr_core::{
    CheckItem, CheckSummary, DiscussionEntry, DiscussionKind, PrDetails, PrFile, PrSummary,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub id: u64,
    pub login: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Team {
    pub id: u64,
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Org {
    pub login: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepositoryOwner {
    pub login: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Repository {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub owner: RepositoryOwner,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub html_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitRef {
    #[serde(default)]
    pub sha: String,
    #[serde(rename = "ref", default)]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PullRequest {
    pub id: u64,
    pub number: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub user: Option<User>,
    #[serde(default)]
    pub requested_reviewers: Vec<User>,
    #[serde(default)]
    pub requested_teams: Vec<Team>,
    #[serde(default)]
    pub labels: Vec<Label>,
    pub head: GitRef,
    pub base: GitRef,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub mergeable: Option<bool>,
    #[serde(default)]
    pub mergeable_state: String,
    #[serde(default)]
    pub state: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Label {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueComment {
    pub id: u64,
    #[serde(default)]
    pub user: Option<User>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub html_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Review {
    pub id: u64,
    #[serde(default)]
    pub user: Option<User>,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub submitted_at: Option<String>,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub commit_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReviewComment {
    pub id: u64,
    #[serde(default)]
    pub user: Option<User>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub line: Option<u64>,
    #[serde(default)]
    pub original_line: Option<u64>,
    #[serde(default)]
    pub side: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub html_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PullFile {
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub previous_filename: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
    #[serde(default)]
    pub changes: u64,
    #[serde(default)]
    pub patch: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CheckRun {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub conclusion: Option<String>,
    #[serde(default)]
    pub html_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CheckRunsResponse {
    #[serde(default)]
    pub total_count: u64,
    #[serde(default)]
    pub check_runs: Vec<CheckRun>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StatusContext {
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub target_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CombinedStatus {
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub statuses: Vec<StatusContext>,
    #[serde(default)]
    pub total_count: u64,
}

#[derive(Debug, Serialize)]
pub struct ReviewRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<&'a str>,
    pub event: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_id: Option<&'a str>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReviewResponse {
    pub id: u64,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub html_url: String,
}

#[derive(Debug, Clone)]
pub struct ScanContext {
    pub viewer: User,
    pub team_ids: Vec<u64>,
    pub team_names: Vec<String>,
    pub repositories: Vec<Repository>,
}

impl ScanContext {
    pub fn requested_reasons(&self, pr: &PullRequest) -> Vec<String> {
        let mut reasons = Vec::new();
        if pr
            .requested_reviewers
            .iter()
            .any(|u| u.id == self.viewer.id)
        {
            reasons.push("You".to_string());
        }
        for team in &pr.requested_teams {
            if self.team_ids.contains(&team.id) {
                reasons.push(team.name.clone());
            }
        }
        reasons
    }
}

impl PullRequest {
    pub fn summary(&self, repo: &Repository, reasons: Vec<String>) -> PrSummary {
        let (owner, name) = repo
            .full_name
            .split_once('/')
            .map(|(o, r)| (o.to_string(), r.to_string()))
            .unwrap_or_else(|| (repo.owner.login.clone(), repo.name.clone()));
        PrSummary {
            key: pr_core::PrKey::new(owner, name, self.number),
            title: self.title.clone(),
            author: self
                .user
                .as_ref()
                .map(|u| u.login.clone())
                .unwrap_or_default(),
            html_url: self.html_url.clone(),
            base_ref: self.base.name.clone(),
            head_ref: self.head.name.clone(),
            head_sha: self.head.sha.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            draft: self.draft,
            reasons,
            blocked: self
                .labels
                .iter()
                .any(|label| label.name.eq_ignore_ascii_case("blocked")),
        }
    }
}

/// Merge GitHub's lists into one chronological discussion.
#[allow(clippy::too_many_arguments)]
pub fn build_details(
    summary: PrSummary,
    pull: PullRequest,
    issue_comments: Vec<IssueComment>,
    reviews: Vec<Review>,
    review_comments: Vec<ReviewComment>,
    checks: CheckSummary,
    files: Vec<PullFile>,
    files_complete: bool,
    viewer: &User,
) -> PrDetails {
    let mut discussion = vec![DiscussionEntry {
        kind: DiscussionKind::Description,
        author: pull
            .user
            .as_ref()
            .map(|u| u.login.clone())
            .unwrap_or_else(|| summary.author.clone()),
        created_at: pull.created_at.clone(),
        body: pull.body.clone().unwrap_or_default(),
        html_url: pull.html_url.clone(),
        context: Some(format!("{} → {}", pull.head.name, pull.base.name)),
    }];
    discussion.extend(issue_comments.into_iter().map(|comment| DiscussionEntry {
        kind: DiscussionKind::Comment,
        author: comment.user.map(|u| u.login).unwrap_or_default(),
        created_at: comment.created_at,
        body: comment.body.unwrap_or_default(),
        html_url: comment.html_url,
        context: None,
    }));
    let pending_review_exists = reviews.iter().any(|review| {
        review.user.as_ref().is_some_and(|u| u.id == viewer.id) && review.state == "PENDING"
    });
    discussion.extend(reviews.into_iter().map(|review| DiscussionEntry {
        kind: DiscussionKind::Review,
        author: review.user.map(|u| u.login).unwrap_or_default(),
        created_at: review.submitted_at.unwrap_or_else(|| "0000".into()),
        body: format!(
            "**{}**\n\n{}",
            review.state,
            review.body.unwrap_or_default()
        ),
        html_url: review.html_url,
        context: None,
    }));
    discussion.extend(review_comments.into_iter().map(|comment| DiscussionEntry {
        kind: DiscussionKind::InlineComment,
        author: comment.user.map(|u| u.login).unwrap_or_default(),
        created_at: comment.created_at,
        body: comment.body.unwrap_or_default(),
        html_url: comment.html_url,
        context: Some(format!(
            "{}:{}",
            comment.path,
            comment.line.or(comment.original_line).unwrap_or_default()
        )),
    }));
    discussion.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    PrDetails {
        summary,
        body: pull.body.unwrap_or_default(),
        discussion,
        checks,
        files: files
            .into_iter()
            .map(|file| PrFile {
                filename: file.filename,
                previous_filename: file.previous_filename,
                status: file.status,
                additions: file.additions,
                deletions: file.deletions,
                changes: file.changes,
                patch: file.patch,
            })
            .collect(),
        files_complete,
        pending_review_exists,
        viewer_login: viewer.login.clone(),
        can_review: !pull.draft && pull.user.as_ref().map(|u| u.id) != Some(viewer.id),
        review_notice: if pull.draft {
            Some("Draft pull requests cannot be reviewed".into())
        } else if pull.user.as_ref().map(|u| u.id) == Some(viewer.id) {
            Some("GitHub does not allow reviewing your own pull request".into())
        } else if pending_review_exists {
            Some("A pending review already exists; use GitHub to submit or discard it".into())
        } else {
            None
        },
    }
}

pub fn summarize_checks(check_runs: CheckRunsResponse, combined: CombinedStatus) -> CheckSummary {
    let mut summary = CheckSummary::default();
    summary
        .items
        .extend(check_runs.check_runs.into_iter().map(|run| {
            match (run.status.as_str(), run.conclusion.as_deref()) {
                ("completed", Some("success" | "neutral" | "skipped")) => summary.successful += 1,
                ("completed", _) => summary.failed += 1,
                _ => summary.pending += 1,
            }
            CheckItem {
                name: run.name,
                status: run.status,
                conclusion: run.conclusion,
                html_url: run.html_url,
            }
        }));
    summary
        .items
        .extend(combined.statuses.into_iter().map(|status| {
            match status.state.as_str() {
                "success" => summary.successful += 1,
                "failure" | "error" => summary.failed += 1,
                _ => summary.pending += 1,
            }
            CheckItem {
                name: status.context,
                status: status.state.clone(),
                conclusion: Some(status.state),
                html_url: status.target_url.unwrap_or_default(),
            }
        }));
    summary.reported =
        !summary.items.is_empty() || check_runs.total_count > 0 || combined.total_count > 0;
    summary
}
