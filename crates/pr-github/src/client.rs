use crate::models::*;
use pr_core::{CheckSummary, PrDetails, PrSummary};
use serde::{Serialize, de::DeserializeOwned};
use std::process::Command;
use thiserror::Error;
use ureq::http::StatusCode;

const API: &str = "https://api.github.com";
const PER_PAGE: u64 = 100;
/// Comma-separated org allow-list. When unset every org the viewer belongs
/// to is scanned.
pub const ORGS_ENV: &str = "PR_APPROVER_ORGS";

#[derive(Debug, Error)]
pub enum GithubError {
    #[error("GitHub authentication failed: {0}")]
    Auth(String),
    #[error("GitHub request failed: {0}")]
    Transport(#[from] ureq::Error),
    #[error("GitHub returned HTTP {status} for {path}: {body}")]
    Status {
        status: u16,
        path: String,
        body: String,
    },
    #[error("GitHub response could not be decoded for {path}: {error}")]
    Decode { path: String, error: String },
}

impl GithubError {
    pub fn unauthorized(&self) -> bool {
        matches!(
            self,
            Self::Status {
                status: 401 | 403,
                ..
            }
        )
    }

    /// 5xx responses are GitHub-side failures worth one retry.
    pub fn transient(&self) -> bool {
        matches!(self, Self::Status { status, .. } if (500..600).contains(status))
    }
}

#[derive(Clone)]
pub struct GithubClient {
    token: String,
    agent: ureq::Agent,
}

impl GithubClient {
    /// Load the active `gh` credential without displaying or persisting it.
    pub fn from_gh() -> Result<Self, GithubError> {
        let output = Command::new("gh")
            .args(["auth", "token", "--hostname", "github.com"])
            .output()
            .map_err(|error| GithubError::Auth(format!("could not run gh: {error}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(GithubError::Auth(if stderr.is_empty() {
                "gh auth token failed".into()
            } else {
                stderr
            }));
        }
        let token = String::from_utf8(output.stdout)
            .map_err(|_| GithubError::Auth("gh returned a non-UTF-8 token".into()))?
            .trim()
            .to_string();
        if token.is_empty() {
            return Err(GithubError::Auth("gh returned an empty token".into()));
        }
        Ok(Self::new(token))
    }

    pub fn new(token: String) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(std::time::Duration::from_secs(30)))
            .build()
            .into();
        Self { token, agent }
    }

    pub fn viewer(&self) -> Result<User, GithubError> {
        self.get_json("/user")
    }

    pub fn teams(&self) -> Result<Vec<Team>, GithubError> {
        self.get_pages("/user/teams?per_page=100")
    }

    pub fn personal_repositories(&self) -> Result<Vec<Repository>, GithubError> {
        self.get_pages("/user/repos?affiliation=owner,collaborator&visibility=all&per_page=100")
    }

    /// Orgs to scan: the `PR_APPROVER_ORGS` allow-list if set, else every
    /// org the authenticated user belongs to.
    pub fn orgs(&self) -> Result<Vec<String>, GithubError> {
        if let Ok(list) = std::env::var(ORGS_ENV) {
            return Ok(list
                .split(',')
                .map(str::trim)
                .filter(|org| !org.is_empty())
                .map(str::to_string)
                .collect());
        }
        Ok(self
            .get_pages::<Org>("/user/orgs?per_page=100")?
            .into_iter()
            .map(|org| org.login)
            .collect())
    }

    pub fn org_repositories(&self, org: &str) -> Result<Vec<Repository>, GithubError> {
        self.get_pages(&format!("/orgs/{}/repos?per_page=100", encode(org)))
    }

    pub fn pulls(&self, repo: &Repository) -> Result<Vec<PullRequest>, GithubError> {
        self.get_pages(&format!(
            "/repos/{}/pulls?state=open&per_page=100",
            encode(&repo.full_name)
        ))
    }

    pub fn pull(&self, repo: &str, number: u64) -> Result<PullRequest, GithubError> {
        self.get_json(&format!("/repos/{}/pulls/{number}", encode(repo)))
    }

    pub fn issue_comments(
        &self,
        repo: &str,
        number: u64,
    ) -> Result<Vec<IssueComment>, GithubError> {
        self.get_pages(&format!(
            "/repos/{}/issues/{number}/comments?per_page=100",
            encode(repo)
        ))
    }

    pub fn reviews(&self, repo: &str, number: u64) -> Result<Vec<Review>, GithubError> {
        self.get_pages(&format!(
            "/repos/{}/pulls/{number}/reviews?per_page=100",
            encode(repo)
        ))
    }

    pub fn review_comments(
        &self,
        repo: &str,
        number: u64,
    ) -> Result<Vec<ReviewComment>, GithubError> {
        self.get_pages(&format!(
            "/repos/{}/pulls/{number}/comments?per_page=100",
            encode(repo)
        ))
    }

    /// GitHub caps the files endpoint at 3000 entries. The returned flag is
    /// true only when a subsequent empty page proves complete pagination.
    pub fn files(&self, repo: &str, number: u64) -> Result<(Vec<PullFile>, bool), GithubError> {
        let files = self.get_pages::<PullFile>(&format!(
            "/repos/{}/pulls/{number}/files?per_page=100",
            encode(repo)
        ))?;
        Ok((files.clone(), files.len() < 3000))
    }

    pub fn checks(&self, repo: &str, sha: &str) -> Result<CheckSummary, GithubError> {
        let runs = self.get_json::<CheckRunsResponse>(&format!(
            "/repos/{}/commits/{}/check-runs?per_page=100",
            encode(repo),
            encode(sha)
        ))?;
        let status = self.get_json::<CombinedStatus>(&format!(
            "/repos/{}/commits/{}/status?per_page=100",
            encode(repo),
            encode(sha)
        ))?;
        Ok(crate::models::summarize_checks(runs, status))
    }

    pub fn detail(&self, summary: &PrSummary, viewer: &User) -> Result<PrDetails, GithubError> {
        let repo = summary.key.full_repo();
        let number = summary.key.number;
        let pull = self.pull(&repo, number)?;
        let issue_comments = self.issue_comments(&repo, number)?;
        let reviews = self.reviews(&repo, number)?;
        let review_comments = self.review_comments(&repo, number)?;
        let checks = self.checks(&repo, &pull.head.sha)?;
        let (files, files_complete) = self.files(&repo, number)?;
        Ok(build_details(
            summary.clone(),
            pull,
            issue_comments,
            reviews,
            review_comments,
            checks,
            files,
            files_complete,
            viewer,
        ))
    }

    /// Re-check current state immediately before submitting, then submit against
    /// the inspected head commit.
    pub fn submit_review(
        &self,
        repo: &str,
        number: u64,
        body: &str,
        event: &str,
        inspected_sha: &str,
        viewer: &User,
    ) -> Result<ReviewResponse, GithubError> {
        let pull = self.pull(repo, number)?;
        if pull.state != "open" {
            return Err(GithubError::Status {
                status: 409,
                path: format!("pull {repo}#{number}"),
                body: "pull request is no longer open".into(),
            });
        }
        if pull.head.sha != inspected_sha {
            return Err(GithubError::Status {
                status: 409,
                path: format!("pull {repo}#{number}"),
                body: "pull request head changed; review the latest changes before submitting"
                    .into(),
            });
        }
        if pull.user.as_ref().map(|u| u.id) == Some(viewer.id) {
            return Err(GithubError::Status {
                status: 422,
                path: format!("pull {repo}#{number}"),
                body: "cannot review your own pull request".into(),
            });
        }
        let reviews = self.reviews(repo, number)?;
        if reviews.iter().any(|review| {
            review.user.as_ref().is_some_and(|u| u.id == viewer.id) && review.state == "PENDING"
        }) {
            return Err(GithubError::Status {
                status: 409,
                path: format!("pull {repo}#{number}"),
                body: "a pending review already exists; submit or discard it on GitHub first"
                    .into(),
            });
        }
        let path = format!("/repos/{}/pulls/{}/reviews", encode(repo), number);
        let request = ReviewRequest {
            body: (!body.trim().is_empty()).then_some(body.trim()),
            event,
            commit_id: Some(inspected_sha),
        };
        // GitHub occasionally answers 5xx here; retry once after a short
        // pause rather than making the user redo the submission.
        match self.post_json(&path, &request) {
            Err(error) if error.transient() => {
                std::thread::sleep(std::time::Duration::from_millis(750));
                self.post_json(&path, &request)
            }
            result => result,
        }
    }

    /// Add labels to an issue/PR, creating the label first if the repo does
    /// not have it (POST /labels 404s on unknown names; create returns 422
    /// "already_exists" which is ignored).
    pub fn add_labels(&self, repo: &str, number: u64, labels: &[&str]) -> Result<(), GithubError> {
        #[derive(Serialize)]
        struct AddLabels<'a> {
            labels: &'a [&'a str],
        }
        let path = format!("/repos/{}/issues/{}/labels", encode(repo), number);
        let result = self.post_json::<serde_json::Value, _>(&path, &AddLabels { labels });
        match result {
            Ok(_) => Ok(()),
            Err(error) => {
                // Only "no such label" justifies creating it and retrying —
                // any other failure is the real error to report.
                if !matches!(
                    error,
                    GithubError::Status {
                        status: 404 | 422,
                        ..
                    }
                ) {
                    return Err(error);
                }
                for label in labels {
                    #[derive(Serialize)]
                    struct NewLabel<'a> {
                        name: &'a str,
                        color: &'a str,
                        description: &'a str,
                    }
                    let _ = self.post_json::<serde_json::Value, _>(
                        &format!("/repos/{}/labels", encode(repo)),
                        &NewLabel {
                            name: label,
                            color: "b60205",
                            description: "Blocked from review queue",
                        },
                    );
                }
                self.post_json::<serde_json::Value, _>(&path, &AddLabels { labels })?;
                Ok(())
            }
        }
    }

    fn get_pages<T: DeserializeOwned>(&self, path: &str) -> Result<Vec<T>, GithubError> {
        let mut all = Vec::new();
        for page in 1..=100u64 {
            let separator = if path.contains('?') { '&' } else { '?' };
            let page_path = format!("{path}{separator}page={page}");
            let page_items = self.get_json::<Vec<T>>(&page_path)?;
            let count = page_items.len() as u64;
            all.extend(page_items);
            if count < PER_PAGE {
                break;
            }
        }
        Ok(all)
    }

    fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, GithubError> {
        let url = format!("{API}{path}");
        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.body_mut().read_to_string().unwrap_or_default();
            return Err(GithubError::Status {
                status,
                path: path.to_string(),
                body,
            });
        }
        response
            .body_mut()
            .read_json()
            .map_err(|error| GithubError::Decode {
                path: path.to_string(),
                error: error.to_string(),
            })
    }

    fn post_json<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, GithubError> {
        let url = format!("{API}{path}");
        let mut response = self
            .agent
            .post(&url)
            .header("Authorization", &format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send_json(body)?;
        if response.status() == StatusCode::UNPROCESSABLE_ENTITY || !response.status().is_success()
        {
            let status = response.status().as_u16();
            let body = response.body_mut().read_to_string().unwrap_or_default();
            return Err(GithubError::Status {
                status,
                path: path.to_string(),
                body,
            });
        }
        response
            .body_mut()
            .read_json()
            .map_err(|error| GithubError::Decode {
                path: path.to_string(),
                error: error.to_string(),
            })
    }
}

fn encode(value: &str) -> String {
    // Repository values are supplied as owner/name; slash must remain.
    value
        .split('/')
        .map(urlencoding::encode)
        .map(|part| part.into_owned())
        .collect::<Vec<_>>()
        .join("/")
}
