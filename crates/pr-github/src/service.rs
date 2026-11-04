use crate::{GithubClient, GithubError, Repository, ScanContext, User};
use pr_core::{PrDetails, PrKey, PrSummary, ReviewKind, SyncProgress};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

/// Commands accepted by the background GitHub service.
#[derive(Debug)]
pub enum ServiceCommand {
    Refresh {
        generation: u64,
    },
    LoadDetails {
        generation: u64,
        key: PrKey,
    },
    SubmitReview {
        generation: u64,
        key: PrKey,
        kind: ReviewKind,
        body: String,
        inspected_sha: String,
    },
}

/// UI-facing progress and results. Every event carries the requesting
/// generation so a stale scan cannot replace newer state.
#[derive(Debug)]
pub enum ServiceEvent {
    AuthFailed {
        generation: u64,
        message: String,
    },
    Stage {
        generation: u64,
        progress: SyncProgress,
    },
    RepositoryDone {
        generation: u64,
        repo: String,
        pull_requests: Vec<PrSummary>,
        progress: SyncProgress,
    },
    RepositoryFailed {
        generation: u64,
        repo: String,
        message: String,
        progress: SyncProgress,
    },
    SyncFinished {
        generation: u64,
        progress: SyncProgress,
        failures: Vec<String>,
    },
    DetailLoaded {
        generation: u64,
        detail: Box<PrDetails>,
    },
    DetailFailed {
        generation: u64,
        key: PrKey,
        message: String,
    },
    ReviewSubmitted {
        generation: u64,
        key: PrKey,
        kind: ReviewKind,
        /// Labelling failed after the review itself succeeded.
        label_error: Option<String>,
    },
    ReviewFailed {
        generation: u64,
        key: PrKey,
        message: String,
    },
}

#[derive(Debug)]
pub struct ServiceHandle {
    pub commands: mpsc::Sender<ServiceCommand>,
    pub events: mpsc::Receiver<ServiceEvent>,
    generation: Arc<AtomicU64>,
    _worker: thread::JoinHandle<()>,
}

impl ServiceHandle {
    pub fn start() -> Self {
        let (commands, command_rx) = mpsc::channel::<ServiceCommand>();
        let (events, event_rx) = mpsc::channel::<ServiceEvent>();
        let generation = Arc::new(AtomicU64::new(1));
        let worker_generation = Arc::clone(&generation);
        let worker = thread::spawn(move || service_loop(command_rx, events, worker_generation));
        Self {
            commands,
            events: event_rx,
            generation,
            _worker: worker,
        }
    }

    pub fn refresh(&self) {
        let generation = self.next_generation();
        let _ = self.commands.send(ServiceCommand::Refresh { generation });
    }

    pub fn load_details(&self, key: PrKey) {
        // Detail work follows the active scan generation; it must not mark the
        // scan stale merely because the user selected another PR.
        let generation = self.current_generation();
        let _ = self
            .commands
            .send(ServiceCommand::LoadDetails { generation, key });
    }

    pub fn submit_review(&self, key: PrKey, kind: ReviewKind, body: String, inspected_sha: String) {
        let generation = self.current_generation();
        let _ = self.commands.send(ServiceCommand::SubmitReview {
            generation,
            key,
            kind,
            body,
            inspected_sha,
        });
    }

    fn next_generation(&self) -> u64 {
        // Bumping the generation IS the cancellation: scan workers compare
        // their scan's generation against this counter and stop taking work
        // once a newer command exists. No flag window to miss.
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn current_generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
}

fn service_loop(
    commands: mpsc::Receiver<ServiceCommand>,
    events: mpsc::Sender<ServiceEvent>,
    generation: Arc<AtomicU64>,
) {
    let mut client = None;
    let mut viewer = None;
    while let Ok(command) = commands.recv() {
        let command_generation = match command {
            ServiceCommand::Refresh { generation }
            | ServiceCommand::LoadDetails { generation, .. }
            | ServiceCommand::SubmitReview { generation, .. } => generation,
        };
        if command_generation != generation.load(Ordering::SeqCst) {
            continue;
        }
        if client.is_none() {
            match GithubClient::from_gh()
                .and_then(|client| client.viewer().map(|viewer| (client, viewer)))
            {
                Ok((new_client, new_viewer)) => {
                    client = Some(new_client);
                    viewer = Some(new_viewer);
                }
                Err(error) => {
                    let _ = events.send(ServiceEvent::AuthFailed {
                        generation: command_generation,
                        message: error.to_string(),
                    });
                    continue;
                }
            }
        }
        let client = client.clone().expect("client initialized");
        let viewer = viewer.clone().expect("viewer initialized");
        let generation_counter = generation.clone();
        match command {
            // Scans, detail loads and review submissions each run on their
            // own thread — nothing queues behind a running scan, and a stale
            // scan's workers self-cancel via the generation counter.
            ServiceCommand::Refresh { generation } => {
                let events = events.clone();
                thread::spawn(move || {
                    if let Err(error) = scan(
                        client,
                        viewer,
                        generation,
                        generation_counter,
                        events.clone(),
                    ) {
                        let progress = SyncProgress {
                            stage: format!("Discovery failed: {error}"),
                            complete: true,
                            ..SyncProgress::default()
                        };
                        let _ = events.send(ServiceEvent::SyncFinished {
                            generation,
                            progress,
                            failures: vec![error.to_string()],
                        });
                    }
                });
            }
            ServiceCommand::LoadDetails { generation, key } => {
                let events = events.clone();
                thread::spawn(move || {
                    let summary = PrSummary {
                        key: key.clone(),
                        title: "Pull request".into(),
                        author: String::new(),
                        html_url: String::new(),
                        base_ref: String::new(),
                        head_ref: String::new(),
                        head_sha: String::new(),
                        created_at: String::new(),
                        updated_at: String::new(),
                        draft: false,
                        reasons: Vec::new(),
                        blocked: false,
                    };
                    match client.detail(&summary, &viewer) {
                        Ok(detail) => {
                            let _ = events.send(ServiceEvent::DetailLoaded {
                                generation,
                                detail: Box::new(detail),
                            });
                        }
                        Err(error) => {
                            let _ = events.send(ServiceEvent::DetailFailed {
                                generation,
                                key,
                                message: error.to_string(),
                            });
                        }
                    }
                });
            }
            ServiceCommand::SubmitReview {
                generation,
                key,
                kind,
                body,
                inspected_sha,
            } => {
                let events = events.clone();
                thread::spawn(move || {
                    match client.submit_review(
                        &key.full_repo(),
                        key.number,
                        &body,
                        kind.github_event(),
                        &inspected_sha,
                        &viewer,
                    ) {
                        Ok(_) => {
                            let label_error = (kind == ReviewKind::Blocked)
                                .then(|| {
                                    client.add_labels(&key.full_repo(), key.number, &["Blocked"])
                                })
                                .and_then(|result| result.err().map(|error| error.to_string()));
                            let _ = events.send(ServiceEvent::ReviewSubmitted {
                                generation,
                                key,
                                kind,
                                label_error,
                            });
                        }
                        Err(error) => {
                            let _ = events.send(ServiceEvent::ReviewFailed {
                                generation,
                                key,
                                message: error.to_string(),
                            });
                        }
                    }
                });
            }
        }
    }
}

fn scan(
    client: GithubClient,
    viewer: User,
    my_generation: u64,
    current: Arc<AtomicU64>,
    events: mpsc::Sender<ServiceEvent>,
) -> Result<(), GithubError> {
    // A scan is stale the moment a newer command bumps the generation — a
    // shared flag would give stale workers a window to keep running.
    let stale = || current.load(Ordering::SeqCst) != my_generation;
    let mut progress = SyncProgress {
        stage: "Reading account and teams".into(),
        started: true,
        ..SyncProgress::default()
    };
    let _ = events.send(ServiceEvent::Stage {
        generation: my_generation,
        progress: progress.clone(),
    });
    let teams = client.teams()?;
    let personal = client.personal_repositories()?;
    let mut unique = HashMap::new();
    for repo in personal {
        unique.insert(repo.id, repo);
    }
    // Org enumeration failures are recorded, not silently dropped — a failed
    // org scan must still surface as "Sync incomplete".
    let mut pre_failures = Vec::new();
    match client.orgs() {
        Ok(orgs) => {
            for org_name in orgs {
                match client.org_repositories(&org_name) {
                    Ok(repos) => {
                        for repo in repos {
                            unique.insert(repo.id, repo);
                        }
                    }
                    Err(error) => {
                        pre_failures.push(format!("org {org_name}: {error}"));
                    }
                }
            }
        }
        Err(error) => pre_failures.push(format!("list orgs: {error}")),
    }
    let mut repositories: Vec<_> = unique.into_values().collect();
    repositories.sort_by_key(|repo| repo.full_name.to_lowercase());
    let scan_context = ScanContext {
        team_ids: teams.iter().map(|team| team.id).collect(),
        team_names: teams.iter().map(|team| team.name.clone()).collect(),
        repositories: repositories.clone(),
        viewer,
    };
    progress.stage = "Checking pull requests".into();
    progress.repositories_total = repositories.len();
    let _ = events.send(ServiceEvent::Stage {
        generation: my_generation,
        progress: progress.clone(),
    });

    let queue = Arc::new(Mutex::new(repositories));
    let checked = Arc::new(AtomicUsize::new(0));
    let found = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicUsize::new(0));
    let found_prs = Arc::new(Mutex::new(Vec::<PrSummary>::new()));
    let failures = Arc::new(Mutex::new(pre_failures));
    let mut workers = Vec::new();
    for _ in 0..6 {
        let queue = Arc::clone(&queue);
        let checked = Arc::clone(&checked);
        let found_count = Arc::clone(&found);
        let failed = Arc::clone(&failed);
        let found_prs = Arc::clone(&found_prs);
        let failures = Arc::clone(&failures);
        let context = scan_context.clone();
        let events = events.clone();
        let current = Arc::clone(&current);
        let client = client.clone();
        workers.push(thread::spawn(move || {
            loop {
                if current.load(Ordering::SeqCst) != my_generation {
                    break;
                }
                let repo = queue.lock().expect("repository queue poisoned").pop();
                let Some(repo) = repo else { break };
                match scan_repository(&client, &context, &repo) {
                    Ok(prs) => {
                        found_count.fetch_add(prs.len(), Ordering::SeqCst);
                        if !prs.is_empty() {
                            found_prs
                                .lock()
                                .expect("found queue poisoned")
                                .extend(prs.iter().cloned());
                        }
                        let done = checked.fetch_add(1, Ordering::SeqCst) + 1;
                        let mut progress = SyncProgress {
                            stage: "Checking pull requests".into(),
                            repositories_total: context.repositories.len(),
                            repositories_checked: done,
                            repositories_failed: failed.load(Ordering::SeqCst),
                            pull_requests_found: found_count.load(Ordering::SeqCst),
                            started: true,
                            complete: done == context.repositories.len(),
                        };
                        if current.load(Ordering::SeqCst) != my_generation {
                            progress.stage = "Cancelled".into();
                        }
                        let _ = events.send(ServiceEvent::RepositoryDone {
                            generation: my_generation,
                            repo: repo.full_name.clone(),
                            pull_requests: prs,
                            progress,
                        });
                    }
                    Err(error) => {
                        failed.fetch_add(1, Ordering::SeqCst);
                        failures
                            .lock()
                            .expect("failures poisoned")
                            .push(format!("{}: {error}", repo.full_name));
                        let done = checked.fetch_add(1, Ordering::SeqCst) + 1;
                        let progress = SyncProgress {
                            stage: "Checking pull requests".into(),
                            repositories_total: context.repositories.len(),
                            repositories_checked: done,
                            repositories_failed: failed.load(Ordering::SeqCst),
                            pull_requests_found: found_count.load(Ordering::SeqCst),
                            started: true,
                            complete: done == context.repositories.len(),
                        };
                        let _ = events.send(ServiceEvent::RepositoryFailed {
                            generation: my_generation,
                            repo: repo.full_name.clone(),
                            message: error.to_string(),
                            progress,
                        });
                    }
                }
            }
        }));
    }
    for worker in workers {
        let _ = worker.join();
    }
    let mut found_prs = found_prs.lock().expect("found queue poisoned").clone();
    found_prs.sort_by(|a, b| {
        a.repo_name()
            .cmp(&b.repo_name())
            .then(b.created_at.cmp(&a.created_at))
            .then(b.key.number.cmp(&a.key.number))
    });
    let mut progress = SyncProgress {
        stage: if stale() {
            "Cancelled".into()
        } else {
            "Ready".into()
        },
        repositories_total: scan_context.repositories.len(),
        repositories_checked: checked.load(Ordering::SeqCst),
        repositories_failed: failed.load(Ordering::SeqCst),
        pull_requests_found: found_prs.len(),
        started: true,
        complete: !stale(),
    };
    if stale() {
        progress.complete = false;
        return Ok(());
    }
    progress.stage = "Ready".into();
    progress.complete = true;
    let failures = failures.lock().expect("failures poisoned").clone();
    let _ = events.send(ServiceEvent::SyncFinished {
        generation: my_generation,
        progress,
        failures,
    });
    Ok(())
}

fn scan_repository(
    client: &GithubClient,
    context: &ScanContext,
    repo: &Repository,
) -> Result<Vec<PrSummary>, GithubError> {
    if repo.disabled || repo.archived {
        return Ok(Vec::new());
    }
    Ok(client
        .pulls(repo)?
        .into_iter()
        .filter_map(|pull| {
            let reasons = context.requested_reasons(&pull);
            (!reasons.is_empty()).then(|| pull.summary(repo, reasons))
        })
        .collect())
}
