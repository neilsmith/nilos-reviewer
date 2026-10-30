//! NilosReviewer — a native GPUI review queue for GitHub.

use anyhow::Context as _;
use gpui_kit::*;
use nilos_reviewer::AppAssets;

mod app;
mod browser;
mod omarchy;
mod preview;
mod service;
mod store;
mod themes;
mod update;

use app::ApproverView;

fn main() {
    let mut args = std::env::args().skip(1);
    let fixture = match args.next().as_deref() {
        None => None,
        Some("--render-fixture") => {
            let name = args.next().unwrap_or_else(|| "screening-comment".into());
            if !nilos_reviewer::fixtures::RENDER_FIXTURES
                .iter()
                .any(|fixture| fixture.name == name)
            {
                eprintln!("Unknown fixture: {name}");
                std::process::exit(2);
            }
            Some(name)
        }
        Some(_) => {
            eprintln!("Usage: nilos-reviewer [--render-fixture [NAME]]");
            std::process::exit(2);
        }
    };
    application().with_assets(AppAssets).run(|cx| {
        init(cx);
        update::spawn();
        if omarchy::install(cx).is_none() {
            themes::install(cx);
        }
        if let Some(fixture) = fixture {
            if let Err(err) = open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| preview::RenderPreview::new(&fixture, window, cx))
            }) {
                eprintln!("failed to open rendering preview: {err:#}");
                cx.quit();
            }
            return;
        }
        if let Err(err) = open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1280.), px(820.)),
                    cx,
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| ApproverView::new(window, cx)),
        )
        .context("failed to open NilosReviewer")
        {
            eprintln!("{err:#}");
            cx.quit();
        }
    });
}
