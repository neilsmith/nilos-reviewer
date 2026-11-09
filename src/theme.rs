//! Theming: `Kit::neutral` for ordinary dark/light, and on Omarchy a
//! `Kit::from_palette` seeded from the live `colors.toml` (re-read by `poll`,
//! so OS theme switches restyle the running app).

use std::path::PathBuf;

use rune::Color;
use rune::color::hex;
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
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))?;
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
}
