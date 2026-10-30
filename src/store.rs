use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;

/// Small user-level app state. GitHub tokens are deliberately never stored.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AppState {
    #[serde(default)]
    pub drafts: HashMap<String, String>,
    #[serde(default)]
    pub handled: Vec<pr_core::HandledPr>,
    /// PRs marked blocked before GitHub reports the label back on refresh.
    #[serde(default)]
    pub blocked: Vec<pr_core::PrKey>,
}

fn state_path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("nilos-reviewer/state.json"))
}

pub fn load() -> AppState {
    state_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

/// Write atomically enough for app settings: temp file then rename.
/// The file holds review drafts, so it is created owner-only (0600).
pub fn save(state: &AppState) -> io::Result<()> {
    let Some(path) = state_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(state)?)?;
    fs::rename(&tmp, &path)?;
    // chmod after rename covers pre-existing files, which OpenOptions.mode
    // would leave untouched.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
