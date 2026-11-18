//! nilos-reviewer: a GitHub PR review queue, in Rune.
//!
//! `cargo run` opens the window; `-- --snap out.png --scene loaded --size 1280x800`
//! renders it headless. The GitHub service (gh CLI) never starts headless;
//! scenes carry fixture data instead.

use nilos_reviewer::app::{App, Pane, Sync};
use nilos_reviewer::model::ReviewKind;
use nilos_reviewer::{app, view};
use rune::AppSpec;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--time-queue") {
        nilos_reviewer::github::timing_probe();
        return;
    }
    // The updater verifies a downloaded binary by running `--version` on it —
    // the output must start with the app name.
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("nilos-reviewer {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "nilos-reviewer: a GitHub PR review queue.\n\n\
             Usage:\n  nilos-reviewer            Open the window\n  \
             nilos-reviewer --version  Print the version"
        );
        return;
    }

    rune::set_app_id("nilos-reviewer");
    let app = rune::App::new(AppSpec::<App> {
        min_size: Some((420.0, 420.0)),
        scenes: &[
            ("empty", |_| {}),
            ("loading", |s| {
                s.sync = Sync::Scanning {
                    text: "Scanning acme/web (3/12)".into(),
                }
            }),
            ("loaded", App::load_fixture),
            ("light", |s| {
                s.appearance = app::Appearance::Light;
                s.load_fixture();
            }),
            ("review", |s| {
                s.load_fixture();
                s.comment.text = "Looks good — the debounce keeps rebuilds cheap.".into();
                s.confirming = Some(ReviewKind::Approve);
            }),
            ("blocked", |s| {
                s.load_fixture();
                s.show_blocked = true;
            }),
            ("unassigned", |s| {
                s.load_fixture();
                s.show_unassigned = true;
            }),
            ("files", |s| {
                s.load_fixture();
                s.detail_tab = app::DetailTab::Files;
            }),
            ("updated", |s| {
                s.load_fixture();
                s.update.available = Some(nilos_reviewer::update::Release {
                    version: "0.2.3".into(),
                    url: None,
                });
            }),
            ("narrow-detail", |s| {
                s.load_fixture();
                s.window = [640.0, 800.0];
                s.pane = Pane::Detail;
            }),
        ],
        setup: Some(app::setup),
        poll: Some(App::poll),
        on_key: Some(nilos_reviewer::app::on_key),
        ..AppSpec::new("Nilos Reviewer", (1280.0, 800.0), App::default, view::view)
    })
    .compositor(nilos_reviewer::fx::Gfx::default())
    .titlebar(rune::Titlebar::new(40.0));
    app.run();
}
