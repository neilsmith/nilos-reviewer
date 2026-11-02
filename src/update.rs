//! Check GitHub releases for a newer build on startup and self-update.

use std::sync::mpsc;

const OWNER: &str = "neilsmith";
const REPO: &str = "nilos-reviewer";
const APP: &str = "nilos-reviewer";

/// Spawn a background updater: if a newer GitHub release exists, download it
/// and replace this binary — the new build takes effect on the next launch.
/// The applied version tag is sent over the returned channel so the UI can
/// tell the user. Failures (no releases yet, offline, rate-limited) are
/// logged and ignored — the app always boots.
pub fn spawn() -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || match run() {
        Ok(Some(version)) => {
            let _ = tx.send(version);
        }
        Ok(None) => {}
        Err(error) => eprintln!("update check failed: {error}"),
    });
    rx
}

/// Returns Ok(Some(version)) when a release was downloaded and installed.
fn run() -> Result<Option<String>, self_update::errors::Error> {
    let target = self_update::get_target();
    let expected_asset = format!("{APP}-{target}.tar.gz");
    let status = self_update::backends::github::Update::configure()
        .repo_owner(OWNER)
        .repo_name(REPO)
        .bin_name(APP)
        .target(target)
        .current_version(self_update::cargo_crate_version!())
        .asset_matcher(move |assets| assets.iter().find(|a| a.name() == expected_asset).cloned())
        .no_confirm(true)
        .show_output(false)
        .show_download_progress(false)
        .build()?
        .update()?;
    Ok(match status {
        self_update::VersionStatus::Updated(version) => Some(version),
        _ => None,
    })
}
