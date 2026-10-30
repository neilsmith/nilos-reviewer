//! Check GitHub releases for a newer build on startup and self-update.

const OWNER: &str = "neilsmith";
const REPO: &str = "nilos-reviewer";
const APP: &str = "nilos-reviewer";

/// Spawn a background updater: if a newer GitHub release exists, download it,
/// replace this binary, and restart into it. Failures (no releases yet,
/// offline, rate-limited) are logged and ignored — the app always boots.
pub fn spawn() {
    std::thread::spawn(|| {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        match rt.block_on(run()) {
            Ok(true) => {
                // Binary replaced on disk — restart into the new build.
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe).spawn();
                }
                std::process::exit(0);
            }
            Ok(false) => {}
            Err(error) => eprintln!("update check failed: {error}"),
        }
    });
}

/// Returns Ok(true) when a release was downloaded and installed.
#[allow(clippy::result_large_err)]
async fn run() -> Result<bool, axoupdater::AxoupdateError> {
    use axoupdater::{AxoUpdater, ReleaseSource, ReleaseSourceType};
    let mut updater = AxoUpdater::new_for(APP);
    let _ = updater.set_current_version(env!("CARGO_PKG_VERSION").parse()?);
    updater.set_release_source(ReleaseSource {
        release_type: ReleaseSourceType::GitHub,
        owner: OWNER.to_string(),
        name: REPO.to_string(),
        app_name: APP.to_string(),
    });
    Ok(updater.run().await?.is_some())
}
