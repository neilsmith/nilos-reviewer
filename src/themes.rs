//! Built-in gpui-kit themes used when Omarchy theming is unavailable
//! (e.g. macOS). The matching macOS Classic theme is applied per the OS
//! light/dark appearance and updated when it changes.

use gpui_kit::component::{Theme, ThemeRegistry};
use gpui_kit::*;

/// Bundled from <https://github.com/longbridge/gpui-kit/tree/main/themes>.
const THEMES: &str = include_str!("macos-classic.json");

pub fn install(cx: &mut App) {
    if let Err(err) = ThemeRegistry::global_mut(cx).load_themes_from_str(THEMES) {
        eprintln!("failed to load bundled themes: {err}");
        return;
    }
    apply(
        cx,
        matches!(
            cx.window_appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ),
    );
}

/// Apply "macOS Classic Dark" or "macOS Classic Light".
pub fn apply(cx: &mut App, dark: bool) {
    let name = if dark {
        "macOS Classic Dark"
    } else {
        "macOS Classic Light"
    };
    let Some(config) = ThemeRegistry::global(cx).themes().get(name).cloned() else {
        return;
    };
    Theme::change(
        if dark {
            component::ThemeMode::Dark
        } else {
            component::ThemeMode::Light
        },
        None,
        cx,
    );
    Theme::update(cx, |theme| theme.apply_config(&config));
}
