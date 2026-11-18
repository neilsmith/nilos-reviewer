//! Theming: `Kit::neutral` for ordinary dark/light, and on Omarchy a
//! `Kit::from_palette` seeded from the live `colors.toml`. A watcher thread
//! stats the theme dir and wakes the app on a switch, so a running window
//! restyles even while idle (`watch`).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use rune::color::hex;
use rune::Color;
use rune_kit::{Kit, PaletteSeed};

/// The parsed subset of Omarchy's `colors.toml` the app themes itself with.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct OmarchyPalette {
    pub mode: Option<String>,
    pub accent: Option<String>,
    pub background: Option<String>,
    pub foreground: Option<String>,
    pub red: Option<String>,
    pub yellow: Option<String>,
    pub green: Option<String>,
    pub cyan: Option<String>,
}

impl OmarchyPalette {
    pub fn is_dark(&self) -> bool {
        self.mode.as_deref() != Some("light")
    }
    /// Enough palette to theme safely; a mid-write read leaves the previous
    /// complete theme installed instead.
    pub fn is_complete(&self) -> bool {
        self.color_of(&self.background).is_some() && self.color_of(&self.foreground).is_some()
    }
    pub fn color_of(&self, field: &Option<String>) -> Option<Color> {
        hex(field.as_deref()?)
    }
    /// The kit this palette describes: surface colors from `from_palette`,
    /// status colors straight from the palette's own red/yellow/green/cyan.
    pub fn kit(&self) -> Option<Kit> {
        let bg = self.color_of(&self.background)?;
        let fg = self.color_of(&self.foreground)?;
        let accent = self
            .color_of(&self.accent)
            .or_else(|| self.color_of(&self.cyan))
            .unwrap_or(fg);
        let mut kit = Kit::from_palette(PaletteSeed { bg, fg, accent });
        let slots: [(&mut Color, Option<Color>); 4] = [
            (&mut kit.c.success.fg, self.color_of(&self.green)),
            (&mut kit.c.warning.fg, self.color_of(&self.yellow)),
            (&mut kit.c.danger.fg, self.color_of(&self.red)),
            (&mut kit.c.info.fg, self.color_of(&self.cyan)),
        ];
        for (slot, color) in slots {
            if let Some(c) = color {
                *slot = c;
            }
        }
        Some(kit)
    }
}

/// Parse the flat `key = "value"` surface without a TOML dependency.
pub fn parse_colors(input: &str) -> OmarchyPalette {
    let mut palette = OmarchyPalette::default();
    for line in input.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let quoted = (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
            || (value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2);
        if !quoted {
            continue;
        }
        let value = value[1..value.len() - 1].to_string();
        let slot = match key.trim() {
            "mode" => &mut palette.mode,
            "accent" => &mut palette.accent,
            "background" => &mut palette.background,
            "foreground" => &mut palette.foreground,
            "red" => &mut palette.red,
            "yellow" => &mut palette.yellow,
            "green" => &mut palette.green,
            "cyan" => &mut palette.cyan,
            _ => continue,
        };
        *slot = Some(value);
    }
    palette
}

/// `$XDG_STATE_HOME/omarchy/current/theme/colors.toml`, when it exists.
pub fn colors_path() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    let path = state.join("omarchy/current/theme/colors.toml");
    path.is_file().then_some(path)
}

pub fn read_colors() -> Option<String> {
    std::fs::read_to_string(colors_path()?).ok()
}

/// The palette file's current palette, when complete.
pub fn current_palette() -> Option<OmarchyPalette> {
    let palette = parse_colors(&read_colors()?);
    palette.is_complete().then_some(palette)
}

/// What `watch` samples: the colors file's mtime+len, and Omarchy's
/// `theme.generation` marker beside the theme dir. `current/theme` is a
/// symlink — a switch repoints it without touching the file's own mtime,
/// so the generation marker (which Omarchy bumps) is watched too. None when
/// the theme dir is absent.
fn theme_stamp(theme_dir: &Path) -> Option<(Option<SystemTime>, Option<SystemTime>, u64)> {
    let mtime = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let generation = theme_dir.parent()?.join("theme.generation");
    let colors = theme_dir.join("colors.toml");
    let len = std::fs::metadata(&colors).map(|m| m.len()).unwrap_or(0);
    theme_dir
        .is_dir()
        .then(|| (mtime(&generation), mtime(&colors), len))
}

/// Stats `theme_dir` every `period`; when the stamp changes the flag turns
/// on and `wake` runs so the app reloads the palette.
fn watch_dir(
    theme_dir: PathBuf,
    period: Duration,
    wake: impl Fn() + Send + 'static,
) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let f2 = flag.clone();
    let _ = std::thread::Builder::new()
        .name("nilos-theme".into())
        .spawn(move || {
            let mut last = theme_stamp(&theme_dir);
            loop {
                std::thread::sleep(period);
                let now = theme_stamp(&theme_dir);
                if now != last {
                    last = now;
                    f2.store(true, Ordering::SeqCst);
                    wake();
                }
            }
        });
    flag
}

/// The current Omarchy theme dir, watched for switches. None off Omarchy —
/// with no palette file there's nothing to reload anyway.
pub fn watch(wake: impl Fn() + Send + 'static) -> Option<Arc<AtomicBool>> {
    let theme_dir = colors_path()?.parent()?.to_path_buf();
    Some(watch_dir(theme_dir, Duration::from_millis(1500), wake))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"
mode = "dark"
accent = "#f38d70"
selection = "#403e41"
muted = "#72696a"
background = "#2c2525"
dark_background = "#211b1b"
darker_background = "#181414"
lighter_background = "#3d2f2a"
foreground = "#e6d9db"
dark_foreground = "#72696a"
light_foreground = "#c3b7b8"
bright_foreground = "#e6d9db"
red = "#fd6883"
yellow = "#f9cc6c"
green = "#adda78"
cyan = "#85dacc"
blue = "#f38d70"
"##;

    #[test]
    fn parses_omarchy_colors() {
        let p = parse_colors(SAMPLE);
        assert!(p.is_dark());
        assert!(p.is_complete());
        assert_eq!(p.accent.as_deref(), Some("#f38d70"));
        let kit = p.kit().unwrap();
        assert_ne!(kit.c.success.fg, kit.c.danger.fg);
    }

    #[test]
    fn incomplete_palette_yields_no_kit() {
        assert!(parse_colors("mode = \"dark\"\n").kit().is_none());
    }

    fn wait_flag(flag: &AtomicBool, timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if flag.load(Ordering::SeqCst) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn watch_flags_colors_and_generation_changes() {
        let dir = std::env::temp_dir().join(format!("nilos-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let theme = dir.join("current/theme");
        std::fs::create_dir_all(&theme).unwrap();
        std::fs::write(theme.join("colors.toml"), SAMPLE).unwrap();
        let flag = watch_dir(theme.clone(), Duration::from_millis(30), || {});
        assert!(
            !wait_flag(&flag, Duration::from_millis(300)),
            "flag set with no change"
        );
        // A colors.toml rewrite trips it.
        std::fs::write(theme.join("colors.toml"), format!("{SAMPLE}\n")).unwrap();
        assert!(
            wait_flag(&flag, Duration::from_secs(2)),
            "colors rewrite not seen"
        );
        flag.store(false, Ordering::SeqCst);
        // Omarchy repoints `current` without touching colors.toml's mtime —
        // its generation marker bump is what the watcher catches.
        std::fs::write(dir.join("current/theme.generation"), "2").unwrap();
        assert!(
            wait_flag(&flag, Duration::from_secs(2)),
            "generation bump not seen"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn watch_flags_theme_dir_appearing() {
        let dir = std::env::temp_dir().join(format!("nilos-watch-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let theme = dir.join("current/theme");
        let flag = watch_dir(theme.clone(), Duration::from_millis(30), || {});
        std::fs::create_dir_all(&theme).unwrap();
        std::fs::write(theme.join("colors.toml"), SAMPLE).unwrap();
        assert!(
            wait_flag(&flag, Duration::from_secs(2)),
            "theme appearing not seen"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
