//! Follow the active Omarchy theme, self-contained.

use gpui_kit::component::{Theme, ThemeConfig, ThemeRegistry, ThemeSet};
use gpui_kit::*;
use pr_core::omarchy::{OmarchyPalette, parse_colors};
use std::{path::PathBuf, rc::Rc, time::Duration};

pub const THEME_NAME: &str = "Omarchy";
pub const POLL: Duration = Duration::from_millis(750);

fn colors_path() -> Option<PathBuf> {
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

pub fn config_from(content: &str) -> Option<Rc<ThemeConfig>> {
    let palette = parse_colors(content);
    if !palette.is_complete() {
        return None;
    }
    theme_set(&palette)
        .and_then(|set| set.themes.into_iter().next())
        .map(Rc::new)
}

pub fn current_config() -> Option<Rc<ThemeConfig>> {
    config_from(&read_colors()?)
}

fn theme_set(palette: &OmarchyPalette) -> Option<ThemeSet> {
    let colors: serde_json::Map<String, serde_json::Value> = palette
        .theme_colors()
        .into_iter()
        .map(|(key, value)| (key, serde_json::Value::String(value)))
        .collect();
    let highlight = if palette.is_dark() {
        component::highlighter::HighlightTheme::default_dark()
    } else {
        component::highlighter::HighlightTheme::default_light()
    };
    let json = serde_json::json!({
        "name": "nilos-reviewer",
        "themes": [{
            "name": THEME_NAME,
            "mode": if palette.is_dark() { "dark" } else { "light" },
            "colors": colors,
            "highlight": highlight.style,
            "font.family": "Noto Sans",
            "mono_font.family": "JetBrainsMono Nerd Font",
        }],
    });
    serde_json::from_value(json).ok()
}

/// Register and apply the current desktop palette. No-op away from Omarchy.
pub fn install(cx: &mut App) -> Option<Rc<ThemeConfig>> {
    let config = current_config()?;
    let set = serde_json::json!({
        "name": "nilos-reviewer",
        "author": null,
        "url": null,
        "themes": [(*config).clone()],
    });
    if let Err(err) = ThemeRegistry::global_mut(cx)
        .load_themes_from_str(&serde_json::to_string(&set).unwrap_or_default())
    {
        eprintln!("failed to register Omarchy theme: {err}");
        return None;
    }
    Theme::change(
        if config.mode.is_dark() {
            component::ThemeMode::Dark
        } else {
            component::ThemeMode::Light
        },
        None,
        cx,
    );
    Theme::update(cx, |theme| theme.apply_config(&config));
    Some(config)
}
