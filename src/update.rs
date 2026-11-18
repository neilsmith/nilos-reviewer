//! Update checks and self-updates.
//!
//! A background check against GitHub releases lights an "Update" chip in the
//! status bar. Pressing it downloads the release asset, runs the new binary
//! once to prove it works, swaps it into place and restarts the app. A copy
//! that cannot update itself — a dev build, a read-only install dir, a
//! platform with no asset — opens the release page instead.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rune::Tx;
use self_update::{Download, Extract};

use crate::app::App;

const OWNER: &str = "neilsmith";
const REPO: &str = "nilos-reviewer";
const APP: &str = "nilos-reviewer";
const PAGE: &str = "https://github.com/neilsmith/nilos-reviewer/releases/latest";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
/// The time from one automatic check to the next, in seconds.
const EVERY: f64 = 6.0 * 3600.0;
/// The time from startup to the first check, in seconds.
const FIRST: f64 = 10.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    /// The asset for this install. None: there is no file for it, so the chip
    /// opens the release page.
    pub url: Option<String>,
}

#[derive(Default)]
pub struct UpdateState {
    check: Option<Job<Option<Release>>>,
    apply: Option<Job<()>>,
    /// The next automatic check, on the app clock.
    next_check: Option<f64>,
    /// A newer release.
    pub available: Option<Release>,
}

impl UpdateState {
    pub fn applying(&self) -> bool {
        self.apply.is_some()
    }

    /// Finished jobs, for tests.
    pub fn fake_check(&mut self, r: Result<Option<Release>, String>) {
        self.check = Some(Job::resolved(r));
    }
    pub fn fake_apply(&mut self, r: Result<(), String>) {
        self.apply = Some(Job::resolved(r));
    }
}

/// Work on another thread whose answer `poll` picks up.
pub(crate) struct Job<T> {
    slot: Arc<Mutex<Option<Result<T, String>>>>,
}

impl<T: Send + 'static> Job<T> {
    fn spawn(
        waker: Option<rune::Waker>,
        f: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Job<T> {
        let slot: Arc<Mutex<Option<Result<T, String>>>> = Arc::default();
        let s2 = slot.clone();
        let _ = std::thread::Builder::new()
            .name("nilos-update".into())
            .spawn(move || {
                let r = f();
                if let Ok(mut g) = s2.lock() {
                    *g = Some(r);
                }
                if let Some(w) = waker {
                    w();
                }
            });
        Job { slot }
    }

    fn take(&self) -> Option<Result<T, String>> {
        self.slot.lock().ok().and_then(|mut g| g.take())
    }

    /// An already-finished job, for tests.
    fn resolved(r: Result<T, String>) -> Job<T> {
        Job {
            slot: Arc::new(Mutex::new(Some(r))),
        }
    }
}

/// The release asset for this install: the bare binary tarball, or the `.app`
/// bundle tarball when we run inside a bundle.
pub fn asset_name(target: &str, in_bundle: bool) -> Option<String> {
    match (target, in_bundle) {
        ("x86_64-unknown-linux-gnu", _) => Some(format!("{APP}-x86_64-unknown-linux-gnu.tar.gz")),
        ("aarch64-apple-darwin", true) => Some(format!("{APP}-aarch64-apple-darwin.app.tar.gz")),
        ("aarch64-apple-darwin", false) => Some(format!("{APP}-aarch64-apple-darwin.tar.gz")),
        _ => None,
    }
}

/// The download URL of `name` in `assets` — exact match only (the `.sha256`
/// and `.app.tar.gz` siblings collide on substring).
fn asset_url(assets: &[self_update::ReleaseAsset], name: &str) -> Option<String> {
    assets
        .iter()
        .find(|a| a.name() == name)
        .map(|a| a.download_url().to_string())
}

/// The newest strictly-newer release, with this install's asset URL when it
/// has one. Ok(None): already current.
fn latest() -> Result<Option<Release>, String> {
    let update = self_update::backends::github::Update::configure()
        .repo_owner(OWNER)
        .repo_name(REPO)
        .bin_name(APP)
        .target(self_update::get_target())
        .current_version(CURRENT)
        .no_confirm(true)
        .show_output(false)
        .show_download_progress(false)
        .build()
        .map_err(|e| e.to_string())?;
    let releases = update.get_newer_releases().map_err(|e| e.to_string())?;
    let Some(rel) = releases.latest().cloned() else {
        return Ok(None);
    };
    let in_bundle = current_exe().ok().and_then(|e| bundle_of(&e)).is_some();
    let url = asset_name(self_update::get_target(), in_bundle)
        .and_then(|name| asset_url(rel.assets(), &name));
    Ok(Some(Release {
        version: rel.version().to_string(),
        url,
    }))
}

fn download(url: &str, to: &Path) -> Result<(), String> {
    let mut file =
        std::fs::File::create(to).map_err(|e| format!("could not save the download: {e}"))?;
    Download::from_url(url)
        .download_to(&mut file)
        .map_err(|e| format!("the download failed: {e}"))
}

/// Runs the new binary once. A truncated download or a build for another
/// system does not answer `--version` with our name.
fn check_binary(exe: &Path) -> Result<(), String> {
    let out = Command::new(exe)
        .arg("--version")
        .output()
        .map_err(|e| format!("the new version did not start: {e}"))?;
    if out.status.success() && String::from_utf8_lossy(&out.stdout).starts_with(APP) {
        Ok(())
    } else {
        Err("the new version did not start".into())
    }
}

/// The running binary. Linux adds " (deleted)" when the file was replaced
/// since the start.
pub fn current_exe() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("could not find its own file: {e}"))?;
    let s = exe.to_string_lossy();
    Ok(match s.strip_suffix(" (deleted)") {
        Some(t) => PathBuf::from(t),
        None => exe,
    })
}

/// A build in a Cargo `target` folder is not an installed copy.
fn is_dev_build(exe: &Path) -> bool {
    let parts: Vec<_> = exe
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts
        .windows(2)
        .any(|w| w[0] == "target" && (w[1] == "release" || w[1] == "debug"))
}

/// The `.app` bundle around a macOS binary: `X.app/Contents/MacOS/nilos-reviewer`.
pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let b = exe.parent()?.parent()?.parent()?;
    (b.extension().is_some_and(|e| e == "app") && exe.parent()?.file_name()? == "MacOS")
        .then(|| b.to_path_buf())
}

/// Whether we can make files in `dir`.
fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".nilos-write-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// The place an update replaces: the `.app` bundle when we run inside one,
/// else the binary itself. Err when this copy cannot be replaced.
fn target() -> Result<PathBuf, String> {
    let exe = current_exe()?;
    if is_dev_build(&exe) {
        return Err("this copy runs from a build folder".into());
    }
    let place = bundle_of(&exe).unwrap_or(exe);
    let dir = place.parent().ok_or("the app is in no folder")?;
    if !writable(dir) {
        return Err(format!("cannot write to {}", dir.display()));
    }
    Ok(place)
}

/// Move `new` over `place`, keeping the old version aside to restore on failure.
fn swap(place: &Path, new: &Path, old: &Path) -> Result<(), String> {
    std::fs::rename(place, old)
        .map_err(|e| format!("could not move the old version aside: {e}"))?;
    if let Err(e) = std::fs::rename(new, place) {
        let _ = std::fs::rename(old, place);
        return Err(format!("could not put the new version in place: {e}"));
    }
    Ok(())
}

/// Downloads `url` and puts the new version in place of `place`.
pub fn apply_to(place: &Path, url: &str) -> Result<(), String> {
    let dir = place.parent().ok_or("the app is in no folder")?;
    // The work folder is next to the app, so a rename moves the new version in place.
    let work = dir.join(format!(".nilos-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)
        .map_err(|e| format!("could not make {}: {e}", work.display()))?;
    let r = apply_in(place, url, &work);
    let _ = std::fs::remove_dir_all(&work);
    r
}

fn apply_in(place: &Path, url: &str, work: &Path) -> Result<(), String> {
    let file = work.join(url.rsplit('/').next().unwrap_or("nilos-update.tar.gz"));
    download(url, &file)?;
    let out = work.join("x");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    if place.extension().is_some_and(|e| e == "app") {
        // The .app tarball holds the whole signed bundle; swap it, not just the
        // Mach-O inside, so the signature stays intact.
        Extract::from_source(&file)
            .extract_into(&out)
            .map_err(|e| format!("the download did not unpack: {e}"))?;
        let new = std::fs::read_dir(&out)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|x| x == "app"))
            .ok_or("the download has no app bundle")?;
        check_binary(&new.join("Contents/MacOS").join(APP))?;
        return swap(place, &new, &work.join("old.app"));
    }
    Extract::from_source(&file)
        .extract_file(&out, APP)
        .map_err(|e| format!("the download did not unpack: {e}"))?;
    let new = out.join(APP);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755));
    }
    check_binary(&new)?;
    // A rename over the running binary is safe on Linux; on any OS the old
    // file moves aside first so a failed swap restores it.
    swap(place, &new, &work.join("old-bin"))
}

impl App {
    /// A check runs when `waker` exists — a real window. Tests and headless
    /// runs stay offline.
    pub fn check_updates(&mut self) {
        let Some(waker) = self.waker.clone() else {
            return;
        };
        if self.update.check.is_some() {
            return;
        }
        self.update.check = Some(Job::spawn(Some(waker), latest));
    }

    /// The Update chip: download, verify, swap and restart — or open the
    /// release page when this copy cannot update itself.
    pub fn start_update(&mut self, tx: &mut Tx) {
        let Some(rel) = self.update.available.clone() else {
            return;
        };
        if self.update.apply.is_some() {
            return;
        }
        match (rel.url, target()) {
            (Some(url), Ok(place)) => {
                let waker = self.waker.clone();
                self.update.apply = Some(Job::spawn(waker, move || apply_to(&place, &url)));
            }
            (url, why) => {
                if let Err(e) = why {
                    eprintln!("update: {e}");
                }
                if url.is_none() {
                    eprintln!("update: no asset for this platform");
                }
                let _ = tx.open_url(PAGE);
            }
        }
    }

    /// Picks up finished jobs and starts the automatic check when it is due.
    pub fn poll_update(&mut self, t: f64, tx: &mut Tx) -> bool {
        let mut changed = false;
        if let Some(r) = self.update.check.as_ref().and_then(|j| j.take()) {
            self.update.check = None;
            match r {
                Ok(rel) => self.update.available = rel,
                Err(e) => eprintln!("update check failed: {e}"),
            }
            changed = true;
        }
        if let Some(r) = self.update.apply.as_ref().and_then(|j| j.take()) {
            self.update.apply = None;
            match r {
                Ok(()) => self.restart(tx),
                Err(e) => self.notice = Some(format!("Could not update: {e}")),
            }
            changed = true;
        }
        if self.waker.is_none() {
            return changed;
        }
        let due = *self.update.next_check.get_or_insert(t + FIRST);
        if t >= due && self.update.check.is_none() {
            self.update.next_check = Some(t + EVERY);
            self.check_updates();
        } else if t < due {
            tx.wake_after(Duration::from_secs_f64(due - t));
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_names_match_the_release_workflow() {
        assert_eq!(
            asset_name("x86_64-unknown-linux-gnu", false).as_deref(),
            Some("nilos-reviewer-x86_64-unknown-linux-gnu.tar.gz")
        );
        assert_eq!(
            asset_name("aarch64-apple-darwin", true).as_deref(),
            Some("nilos-reviewer-aarch64-apple-darwin.app.tar.gz")
        );
        assert_eq!(
            asset_name("aarch64-apple-darwin", false).as_deref(),
            Some("nilos-reviewer-aarch64-apple-darwin.tar.gz")
        );
        assert_eq!(asset_name("x86_64-pc-windows-msvc", false), None);
    }

    #[test]
    fn asset_url_matches_exactly() {
        let v = "0.3.0";
        let assets = vec![
            self_update::ReleaseAsset::new(
                format!("{APP}-{v}-aarch64-apple-darwin.app.tar.gz"),
                "https://example.com/app",
            ),
            self_update::ReleaseAsset::new(
                format!("{APP}-{v}-aarch64-apple-darwin.tar.gz"),
                "https://example.com/bin",
            ),
            self_update::ReleaseAsset::new(
                format!("{APP}-{v}-aarch64-apple-darwin.tar.gz.sha256"),
                "https://example.com/sum",
            ),
        ];
        // The bare tarball name must not land on the .app or .sha256 siblings.
        assert_eq!(
            asset_url(&assets, "nilos-reviewer-0.3.0-aarch64-apple-darwin.tar.gz"),
            Some("https://example.com/bin".into())
        );
        assert_eq!(
            asset_url(
                &assets,
                "nilos-reviewer-0.3.0-aarch64-apple-darwin.app.tar.gz"
            ),
            Some("https://example.com/app".into())
        );
        assert_eq!(
            asset_url(&assets, "nilos-reviewer-0.3.0-missing.tar.gz"),
            None
        );
    }

    #[test]
    fn dev_builds_and_bundles_are_recognised() {
        assert!(is_dev_build(Path::new(
            "/home/a/app/target/release/nilos-reviewer"
        )));
        assert!(is_dev_build(Path::new(
            "/home/a/app/target/debug/nilos-reviewer"
        )));
        assert!(!is_dev_build(Path::new(
            "/home/a/.local/bin/nilos-reviewer"
        )));
        assert_eq!(
            bundle_of(Path::new(
                "/Applications/Nilos Reviewer.app/Contents/MacOS/nilos-reviewer"
            )),
            Some(PathBuf::from("/Applications/Nilos Reviewer.app"))
        );
        assert_eq!(bundle_of(Path::new("/usr/local/bin/nilos-reviewer")), None);
    }

    #[cfg(unix)]
    #[test]
    fn check_binary_runs_the_download() {
        let dir = std::env::temp_dir().join(format!("nilos-check-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, body: &str| {
            let p = dir.join(name);
            std::fs::write(&p, body).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p
        };
        let good = write("good", "#!/bin/sh\necho 'nilos-reviewer 9.9.9'\n");
        let wrong = write("wrong", "#!/bin/sh\necho 'something else'\n");
        let fails = write("fails", "#!/bin/sh\nexit 1\n");
        assert!(check_binary(&good).is_ok());
        assert!(check_binary(&wrong).is_err());
        assert!(check_binary(&fails).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Downloads the latest Linux release and puts it in place of a scratch
    /// binary. Needs the network; run it on request:
    /// `cargo test live_update -- --ignored`.
    #[test]
    #[ignore]
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn live_update() {
        let rel = latest().expect("a check").expect("a newer release");
        let url = rel.url.expect("a Linux asset");
        let dir = std::env::temp_dir().join(format!("nilos-live-update-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("nilos-reviewer");
        std::fs::write(&exe, b"#!/bin/sh\necho old\n").unwrap();
        apply_to(&exe, &url).expect("apply");
        check_binary(&exe).expect("the new binary runs");
        let left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect();
        assert_eq!(left.len(), 1, "only the binary is left: {left:?}");
        let _ = std::fs::remove_dir_all(&dir);
        println!("updated to {}", rel.version);
    }
}
