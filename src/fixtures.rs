pub struct RenderFixture {
    pub name: &'static str,
    pub markdown: &'static str,
    pub github_html: &'static str,
}

macro_rules! fixture {
    ($name:literal) => {
        RenderFixture {
            name: $name,
            markdown: include_str!(concat!("../tests/fixtures/rendering/", $name, ".md")),
            github_html: include_str!(concat!(
                "../tests/fixtures/rendering/",
                $name,
                ".github.html"
            )),
        }
    };
}

pub const RENDER_FIXTURES: &[RenderFixture] = &[
    fixture!("screening-comment"),
    fixture!("dependabot-changelog"),
    fixture!("dependabot-release-notes"),
    fixture!("renovate-description"),
    fixture!("human-description"),
    fixture!("feature-description"),
];
