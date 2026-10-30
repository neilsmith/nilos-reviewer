use gpui_kit::component::{ActiveTheme, text::TableData};
use gpui_kit::{
    AppContext, Context, IntoElement, ParentElement, Render, Styled, TestAppContext, Window, div,
    px,
};
use nilos_reviewer::{fixtures::RENDER_FIXTURES, render::discussion_view};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Observations {
    tables: Vec<TableData>,
    code: Vec<(Option<String>, String)>,
}

struct FixtureRoot {
    observations: Arc<Mutex<Observations>>,
}

impl Render for FixtureRoot {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tables = self.observations.clone();
        let code = self.observations.clone();
        div().w(px(800.)).child(
            discussion_view("screening", RENDER_FIXTURES[0].markdown, cx.theme())
                .scrollable(false)
                .table_actions(move |table, _, _| {
                    tables
                        .lock()
                        .expect("observations")
                        .tables
                        .push(table.clone());
                    div()
                })
                .code_block_actions(move |block, _, _| {
                    code.lock().expect("observations").code.push((
                        block.lang().map(|lang| lang.to_string()),
                        block.code().to_string(),
                    ));
                    div()
                }),
        )
    }
}

#[gpui::test]
fn native_screening_comment_renders_a_table_and_json_block(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let observations = Arc::new(Mutex::new(Observations::default()));
    let captured = observations.clone();
    let (_, cx) = cx.add_window_view(move |_, _| FixtureRoot { observations });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let captured = captured.lock().expect("observations");
    let table = captured
        .tables
        .last()
        .expect("native renderer created a table");
    assert_eq!(table.headers, ["Check", "Result"]);
    assert_eq!(table.rows.len(), 22);
    assert_eq!(
        table.rows[0],
        ["Unexpected external destinations", "0.28 ✅"]
    );
    let (language, code) = captured
        .code
        .last()
        .expect("native renderer created a code block");
    assert_eq!(language.as_deref(), Some("json"));
    assert!(code.contains("\n  {\n    \"number\": 1,"));
    let packets: serde_json::Value = serde_json::from_str(code).expect("JSON remained intact");
    assert_eq!(packets.as_array().expect("packets").len(), 3);
}

#[gpui::test]
fn native_dependabot_html_keeps_complete_list_item_text(cx: &mut TestAppContext) {
    use gpui_kit::base::text::TextViewState;
    use nilos_reviewer::render::discussion_markdown;
    cx.update(gpui_kit::init);
    let fixture = &RENDER_FIXTURES[2];
    let state = cx.update(|cx| {
        cx.new(|cx| TextViewState::markdown(&discussion_markdown(fixture.markdown), cx))
    });
    cx.run_until_parked();
    state.read_with(cx, |state, _| {
        let text = state.rendered_text();
        assert!(
            text.as_str()
                .contains("[ci] release: keep vendored caches out of the tracked config dir")
        );
        assert!(text.as_str().contains("@\u{200b}dev-mara"));
    });
}

#[gpui::test]
fn html_code_links_have_distinct_native_text_ranges(cx: &mut TestAppContext) {
    use gpui_kit::base::text::TextViewState;
    use nilos_reviewer::render::discussion_markdown;
    cx.update(gpui_kit::init);
    let source = discussion_markdown(RENDER_FIXTURES[2].markdown);
    let state = cx.update(|cx| cx.new(|cx| TextViewState::markdown(&source, cx)));
    cx.run_until_parked();
    state.read_with(cx, |state, _| {
        let text = state.rendered_text();
        let mut ranges = Vec::new();
        for label in ["@\u{200b}dev-mara", "@\u{200b}dev-ivan"] {
            let start = source.find(label).expect("code-link label");
            let range = text
                .range_for_source(start..start + label.len())
                .expect("each code link must belong to a source-addressable native text leaf");
            assert_eq!(&text.as_str()[range.clone()], label);
            ranges.push(range);
        }
        assert!(ranges[0].end < ranges[1].start);
    });
}
