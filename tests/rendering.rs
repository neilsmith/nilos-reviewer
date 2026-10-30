use nilos_reviewer::render::discussion_markdown;
use scraper::{Html, Selector};

fn render(body: &str) -> Html {
    // GPUI's Markdown view uses markdown-rs with GFM enabled. Compile the same
    // source to HTML here to assert structure against an independent GitHub
    // reference; native typography/layout is checked in the fixture preview.
    let mut options = markdown::Options::gfm();
    options.compile.allow_dangerous_html = true;
    Html::parse_fragment(
        &markdown::to_html_with_options(&discussion_markdown(body), &options).expect("valid GFM"),
    )
}

fn texts(html: &Html, selector: &str) -> Vec<String> {
    html.select(&Selector::parse(selector).expect("valid selector"))
        .map(|element| {
            element
                .text()
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|text| !text.is_empty())
        .collect()
}

/// Paragraph and summary texts, in document order, excluding paragraphs that
/// only exist because a loose list wraps its items (GFM dialect difference:
/// GitHub emits bare `<li>` text, markdown-rs may emit `<li><p>`).
fn paragraphs(html: &Html) -> Vec<String> {
    let mut paragraphs = texts(html, "p, summary");
    for inner in texts(html, "li p") {
        if let Some(ix) = paragraphs.iter().position(|text| *text == inner) {
            paragraphs.remove(ix);
        }
    }
    paragraphs
}

#[test]
fn screening_comment_keeps_github_heading_and_paragraph_boundaries() {
    let body = include_str!("fixtures/rendering/screening-comment.md");
    let reference = Html::parse_fragment(include_str!(
        "fixtures/rendering/screening-comment.github.html"
    ));
    let rendered = render(body);
    assert_eq!(texts(&rendered, "h2, h4"), texts(&reference, "h2, h4"));
    assert_eq!(paragraphs(&rendered), paragraphs(&reference));
}

#[test]
fn screening_table_has_the_same_rows_and_cells_as_github() {
    let rendered = render(include_str!("fixtures/rendering/screening-comment.md"));
    let reference = Html::parse_fragment(include_str!(
        "fixtures/rendering/screening-comment.github.html"
    ));
    assert_eq!(texts(&rendered, "table").len(), 1);
    assert_eq!(texts(&rendered, "tr").len(), 23);
    assert_eq!(texts(&rendered, "th, td"), texts(&reference, "th, td"));
}

#[test]
fn four_backtick_json_is_a_block_with_exact_whitespace_and_valid_json() {
    let rendered = render(include_str!("fixtures/rendering/screening-comment.md"));
    let reference = Html::parse_fragment(include_str!(
        "fixtures/rendering/screening-comment.github.html"
    ));
    let selector = Selector::parse("pre").expect("selector");
    // markdown-rs emits a terminal newline that GitHub omits. All internal
    // whitespace, indentation and content must still match byte for byte.
    let code_text = |html: &Html| {
        html.select(&selector)
            .map(|node| {
                node.text()
                    .collect::<String>()
                    .trim_end_matches('\n')
                    .to_string()
            })
            .collect::<Vec<_>>()
    };
    let code = code_text(&rendered);
    let expected = code_text(&reference);
    assert_eq!(code.len(), 1);
    assert_eq!(code, expected);
    let packets: serde_json::Value =
        serde_json::from_str(&code[0]).expect("formatted JSON remains valid");
    assert_eq!(packets.as_array().expect("packets").len(), 3);
}

macro_rules! fixture_structure {
    ($name:ident, $fixture:literal) => {
        #[test]
        fn $name() {
            let rendered = render(include_str!(concat!(
                "fixtures/rendering/",
                $fixture,
                ".md"
            )));
            let reference = Html::parse_fragment(include_str!(concat!(
                "fixtures/rendering/",
                $fixture,
                ".github.html"
            )));
            // GFM implementations differ in tight/loose list paragraph
            // wrappers; compare list-item content independently. `<summary>`
            // is flattened into the paragraph sequence since we expand
            // details blocks rather than rendering collapsible sections.
            assert_eq!(
                paragraphs(&rendered),
                paragraphs(&reference),
                "paragraphs/summaries differ for {}",
                $fixture
            );
            for selector in [
                "h1, h2, h3, h4, h5, h6",
                "li",
                "th, td",
                "pre",
                "strong",
                "em",
            ] {
                assert_eq!(
                    texts(&rendered, selector),
                    texts(&reference, selector),
                    "{selector} differs for {}",
                    $fixture
                );
            }
        }
    };
}

fixture_structure!(dependabot_changelog_matches_github, "dependabot-changelog");
fixture_structure!(
    dependabot_release_notes_match_github,
    "dependabot-release-notes"
);
fixture_structure!(renovate_description_matches_github, "renovate-description");
fixture_structure!(human_paragraphs_match_github, "human-description");
fixture_structure!(
    feature_headings_and_nested_lists_match_github,
    "feature-description"
);

#[test]
fn nested_html_code_links_keep_their_target_and_code_style() {
    let rendered = render(include_str!(
        "fixtures/rendering/dependabot-release-notes.md"
    ));
    let reference = Html::parse_fragment(include_str!(
        "fixtures/rendering/dependabot-release-notes.github.html"
    ));
    let selector = Selector::parse("li a:has(code)").expect("selector");
    let links = |html: &Html| {
        html.select(&selector)
            .map(|node| {
                (
                    node.value().attr("href").expect("href").to_owned(),
                    node.text().collect::<String>(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert!(!links(&rendered).is_empty());
    assert_eq!(links(&rendered), links(&reference));
}
