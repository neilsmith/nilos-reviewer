pub mod fixtures;
pub mod render;

use gpui_kit::assets::AllAssets;
use gpui_kit::{AssetSource, SharedString};
use std::borrow::Cow;

/// Application assets: everything the kit ships, plus our bundled logo.
pub struct AppAssets;

const LOGO: &[u8] = include_bytes!("../assets/nilos-logo.svg");

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        if path == "nilos-logo.svg" {
            return Ok(Some(Cow::Borrowed(LOGO)));
        }
        AllAssets.load(path)
    }

    fn list(&self, path: &str) -> gpui_kit::Result<Vec<SharedString>> {
        AllAssets.list(path)
    }
}
