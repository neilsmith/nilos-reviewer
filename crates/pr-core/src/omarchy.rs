/// Palette parsed from Omarchy's active `colors.toml`.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct OmarchyPalette {
    pub mode: Option<String>,
    pub accent: Option<String>,
    pub selection: Option<String>,
    pub muted: Option<String>,
    pub background: Option<String>,
    pub dark_background: Option<String>,
    pub darker_background: Option<String>,
    pub lighter_background: Option<String>,
    pub foreground: Option<String>,
    pub dark_foreground: Option<String>,
    pub light_foreground: Option<String>,
    pub bright_foreground: Option<String>,
    pub red: Option<String>,
    pub yellow: Option<String>,
    pub orange: Option<String>,
    pub green: Option<String>,
    pub cyan: Option<String>,
    pub blue: Option<String>,
    pub magenta: Option<String>,
    pub brown: Option<String>,
    pub bright_red: Option<String>,
    pub bright_yellow: Option<String>,
    pub bright_green: Option<String>,
    pub bright_cyan: Option<String>,
    pub bright_blue: Option<String>,
    pub bright_magenta: Option<String>,
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
            "selection" => &mut palette.selection,
            "muted" => &mut palette.muted,
            "background" => &mut palette.background,
            "dark_background" => &mut palette.dark_background,
            "darker_background" => &mut palette.darker_background,
            "lighter_background" => &mut palette.lighter_background,
            "foreground" => &mut palette.foreground,
            "dark_foreground" => &mut palette.dark_foreground,
            "light_foreground" => &mut palette.light_foreground,
            "bright_foreground" => &mut palette.bright_foreground,
            "red" => &mut palette.red,
            "yellow" => &mut palette.yellow,
            "orange" => &mut palette.orange,
            "green" => &mut palette.green,
            "cyan" => &mut palette.cyan,
            "blue" => &mut palette.blue,
            "magenta" => &mut palette.magenta,
            "brown" => &mut palette.brown,
            "bright_red" => &mut palette.bright_red,
            "bright_yellow" => &mut palette.bright_yellow,
            "bright_green" => &mut palette.bright_green,
            "bright_cyan" => &mut palette.bright_cyan,
            "bright_blue" => &mut palette.bright_blue,
            "bright_magenta" => &mut palette.bright_magenta,
            _ => continue,
        };
        *slot = Some(value);
    }
    palette
}

impl OmarchyPalette {
    pub fn is_dark(&self) -> bool {
        self.mode.as_deref() != Some("light")
    }

    /// Enough palette to theme safely; a mid-write read leaves the previous
    /// complete theme installed instead.
    pub fn is_complete(&self) -> bool {
        hex_field(&self.background).is_some() && hex_field(&self.foreground).is_some()
    }

    /// Project onto gpui-component's ThemeConfigColors key space.
    pub fn theme_colors(&self) -> Vec<(String, String)> {
        let accent = pick(&[&self.accent, &self.blue]);
        let accent_bright = pick(&[&self.bright_blue, &self.accent, &self.blue]);
        let on_accent = pick(&[&self.darker_background, &self.background]);
        let mut map = Vec::new();
        put(&mut map, "background", &[&self.background]);
        put(
            &mut map,
            "foreground",
            &[&self.foreground, &self.light_foreground],
        );
        put(
            &mut map,
            "border",
            &[&self.lighter_background, &self.selection, &self.muted],
        );
        put(
            &mut map,
            "muted.background",
            &[&self.lighter_background, &self.selection],
        );
        put(
            &mut map,
            "muted.foreground",
            &[&self.dark_foreground, &self.muted],
        );
        put(&mut map, "sidebar.background", &[&self.darker_background]);
        put(
            &mut map,
            "status_bar.background",
            &[&self.darker_background],
        );
        put(
            &mut map,
            "title_bar.background",
            &[&self.darker_background, &self.background],
        );
        put(
            &mut map,
            "elevated_surface.background",
            &[&self.dark_background, &self.lighter_background],
        );
        put(
            &mut map,
            "surface.background",
            &[&self.lighter_background, &self.dark_background],
        );
        put(
            &mut map,
            "selection.background",
            &[&self.selection, &self.lighter_background],
        );
        put(
            &mut map,
            "list.hover.background",
            &[&self.selection, &self.lighter_background],
        );
        put(
            &mut map,
            "list.active.background",
            &[&self.selection, &self.lighter_background],
        );
        put(
            &mut map,
            "tab.background",
            &[&self.darker_background, &self.background],
        );
        put(&mut map, "tab.active.background", &[&self.background]);
        put(
            &mut map,
            "editor.background",
            &[&self.darker_background, &self.background],
        );
        put(
            &mut map,
            "scrollbar.thumb.background",
            &[&self.lighter_background, &self.selection],
        );
        put(
            &mut map,
            "window.border",
            &[&self.lighter_background, &self.selection],
        );

        if let Some(accent) = accent {
            map.extend([
                ("accent.background".to_string(), accent.to_string()),
                ("primary.background".to_string(), accent.to_string()),
                ("ring".to_string(), accent.to_string()),
                ("caret".to_string(), accent.to_string()),
                ("link".to_string(), accent.to_string()),
                ("drag.border".to_string(), accent.to_string()),
                ("slider.thumb.background".to_string(), accent.to_string()),
                ("progress.bar.background".to_string(), accent.to_string()),
                (
                    "scrollbar.thumb.hover.background".to_string(),
                    accent.to_string(),
                ),
            ]);
            if let Some(fg) = on_accent {
                map.push(("accent.foreground".to_string(), fg.to_string()));
                map.push(("primary.foreground".to_string(), fg.to_string()));
            }
            if let Some(bright) = accent_bright {
                map.extend([
                    ("primary.hover.background".to_string(), bright.to_string()),
                    ("primary.active.background".to_string(), bright.to_string()),
                    ("link.hover".to_string(), bright.to_string()),
                    ("link.active".to_string(), bright.to_string()),
                ]);
            }
        }

        for (key, normal, bright) in [
            ("danger", hex_field(&self.red), hex_field(&self.bright_red)),
            (
                "warning",
                hex_field(&self.yellow),
                hex_field(&self.bright_yellow),
            ),
            (
                "success",
                hex_field(&self.green),
                hex_field(&self.bright_green),
            ),
            ("info", hex_field(&self.cyan), hex_field(&self.bright_cyan)),
        ] {
            if let Some(normal) = normal {
                map.push((format!("{key}.background"), normal.to_string()));
                if let Some(fg) = on_accent {
                    map.push((format!("{key}.foreground"), fg.to_string()));
                }
                if let Some(bright) = bright {
                    map.push((format!("{key}.hover.background"), bright.to_string()));
                    map.push((format!("{key}.active.background"), bright.to_string()));
                }
            }
        }

        for (key, normal, bright) in [
            ("button.primary", accent, accent_bright),
            (
                "button.danger",
                hex_field(&self.red),
                hex_field(&self.bright_red),
            ),
            (
                "button.warning",
                hex_field(&self.yellow),
                hex_field(&self.bright_yellow),
            ),
            (
                "button.success",
                hex_field(&self.green),
                hex_field(&self.bright_green),
            ),
            (
                "button.info",
                hex_field(&self.cyan),
                hex_field(&self.bright_cyan),
            ),
        ] {
            if let Some(normal) = normal {
                map.push((format!("{key}.background"), normal.to_string()));
                if let Some(fg) = on_accent {
                    map.push((format!("{key}.foreground"), fg.to_string()));
                }
                if let Some(bright) = bright {
                    map.push((format!("{key}.hover.background"), bright.to_string()));
                    map.push((format!("{key}.active.background"), bright.to_string()));
                }
            }
        }

        if let Some(overlay) =
            hex_field(&self.darker_background).or_else(|| hex_field(&self.background))
        {
            map.push(("overlay".to_string(), alpha_hex(overlay, 0.7)));
        }
        map
    }
}

fn pick<'a>(values: &[&'a Option<String>]) -> Option<&'a str> {
    values.iter().find_map(|value| hex_field(value))
}

fn put(map: &mut Vec<(String, String)>, key: &str, values: &[&Option<String>]) {
    if let Some(value) = pick(values) {
        map.push((key.to_string(), value.to_string()));
    }
}

fn hex_field(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|value| is_hex(value))
}

fn is_hex(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 3 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

fn alpha_hex(value: &str, alpha: f32) -> String {
    let normalized = if value.len() == 4 {
        value
            .chars()
            .skip(1)
            .flat_map(|c| [c, c])
            .collect::<String>()
    } else {
        value.trim_start_matches('#').chars().take(6).collect()
    };
    format!("#{normalized}{:02x}", (alpha * 255.).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_projects_theme_colors() {
        let palette = parse_colors(
            "mode = \"dark\"\nbackground = \"#111111\"\nforeground = \"#eeeeee\"\naccent = \"#aa55ff\"\n",
        );
        assert!(palette.is_dark());
        assert!(palette.is_complete());
        let colors: std::collections::BTreeMap<_, _> = palette.theme_colors().into_iter().collect();
        assert_eq!(colors["primary.background"], "#aa55ff");
        assert_eq!(colors["link"], "#aa55ff");
    }
}
