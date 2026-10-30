//! Open links in the Omarchy chromeless webapp window and center only the
//! newly mapped browser client.

use serde::Deserialize;
use std::collections::HashSet;
use std::process::Command;
use std::time::{Duration, Instant};
use url::Url;

#[derive(Debug, Deserialize)]
struct HyprClient {
    address: String,
    #[serde(default)]
    class: String,
    #[serde(default, rename = "initialClass")]
    initial_class: String,
    #[serde(default)]
    monitor: i64,
}

#[derive(Debug, Deserialize)]
struct HyprMonitor {
    id: i64,
    #[serde(default)]
    x: i64,
    #[serde(default)]
    y: i64,
    #[serde(default)]
    width: i64,
    #[serde(default)]
    height: i64,
    #[serde(default = "one")]
    scale: f64,
    #[serde(default)]
    focused: bool,
}

fn one() -> f64 {
    1.0
}

/// Open an HTTP(S) URL as a webapp on the current workspace. If a new browser
/// client is identifiable, float and center exactly that window; otherwise the
/// opened browser is left untouched.
pub fn open(url: &str, fallback_open: impl FnOnce()) {
    if !is_http_url(url) {
        return;
    }
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none()
        || Command::new("omarchy-launch-webapp")
            .arg(url)
            .spawn()
            .is_err()
    {
        fallback_open();
        return;
    }
    std::thread::spawn(move || center_new_client(clients()));
}

fn is_http_url(url: &str) -> bool {
    Url::parse(url)
        .map(|url| matches!(url.scheme(), "http" | "https"))
        .unwrap_or(false)
}

fn clients() -> Vec<HyprClient> {
    let output = Command::new("hyprctl").args(["-j", "clients"]).output();
    output
        .ok()
        .and_then(|output| serde_json::from_slice::<Vec<HyprClient>>(&output.stdout).ok())
        .unwrap_or_default()
}

/// The new window must look like a browser before we touch it — an unrelated
/// client mapped in the same window (notification, launcher, editor) must
/// never be resized or moved.
fn looks_like_browser(client: &HyprClient) -> bool {
    ["chrom", "firefox", "brave", "edge", "webapp", "browser"]
        .iter()
        .any(|needle| {
            client.class.to_lowercase().contains(needle)
                || client.initial_class.to_lowercase().contains(needle)
        })
}

fn monitors() -> Vec<HyprMonitor> {
    let output = Command::new("hyprctl").args(["-j", "monitors"]).output();
    output
        .ok()
        .and_then(|output| serde_json::from_slice::<Vec<HyprMonitor>>(&output.stdout).ok())
        .unwrap_or_default()
}

fn dispatch(args: &[&str]) {
    let _ = Command::new("hyprctl").args(args).output();
}

fn center_new_client(before: Vec<HyprClient>) {
    let known: HashSet<String> = before.into_iter().map(|client| client.address).collect();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(client) = clients()
            .into_iter()
            .find(|client| !known.contains(&client.address) && looks_like_browser(client))
        {
            let address = client.address.clone();
            // Float and place by address only — no focuswindow, so the user's
            // focus never jumps to the browser unless they move it.
            dispatch(&["dispatch", &format!("setfloating address:{address}")]);
            let size = monitors()
                .into_iter()
                .find(|monitor| monitor.id == client.monitor)
                .or_else(|| monitors().into_iter().find(|monitor| monitor.focused))
                .map(|monitor| {
                    let mw = (monitor.width as f64 / monitor.scale) as i64;
                    let mh = (monitor.height as f64 / monitor.scale) as i64;
                    (monitor.x, monitor.y, mw, mh)
                });
            if let Some((mx, my, mw, mh)) = size {
                // ~2/3 width, ~4/5 height of the monitor, centred.
                let w = mw * 2 / 3;
                let h = mh * 4 / 5;
                let x = mx + (mw - w) / 2;
                let y = my + (mh - h) / 2;
                dispatch(&[
                    "dispatch",
                    &format!("resizewindowpixel exact {w} {h},address:{address}"),
                ]);
                dispatch(&[
                    "dispatch",
                    &format!("movewindowpixel exact {x} {y},address:{address}"),
                ]);
            } else {
                dispatch(&[
                    "dispatch",
                    &format!("resizewindowpixel exact 1300 900,address:{address}"),
                ]);
                dispatch(&["dispatch", "centerwindow"]);
            }
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
