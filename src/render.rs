use gpui_kit::component::Theme;
use gpui_kit::component::text::{TextView, TextViewStyle};
use gpui_kit::*;

/// Shared by live descriptions, comments, and the offline regression preview.
pub fn discussion_view(id: impl Into<ElementId>, body: &str, theme: &Theme) -> TextView {
    TextView::markdown(id, discussion_markdown(body)).style(body_style(theme))
}

pub fn body_style(theme: &Theme) -> TextViewStyle {
    let mut style = TextViewStyle::default()
        .paragraph_gap(rems(0.5))
        .inline_code(HighlightStyle {
            color: Some(theme.foreground),
            background_color: Some(theme.muted),
            ..HighlightStyle::default()
        })
        .code_block(
            StyleRefinement::default()
                .bg(theme.muted)
                .border_1()
                .border_color(theme.border)
                .p_2()
                .my_1(),
        );
    style.highlight_theme = theme.highlight_theme.clone();
    style
}

use markdown::{ParseOptions, mdast::Node};

/// Prepare a GitHub discussion body for GPUI's GFM + embedded HTML parser.
///
/// GitHub bodies mix authored Markdown with embedded HTML blocks
/// (`<details>`, `<blockquote>`, `<ul>`). GPUI renders those HTML blocks
/// through a different code path that loses text (invisible list items) and
/// records no source positions for inline HTML, which breaks links inside
/// `<a><code>` constructs. So we convert *block-level* HTML nodes — and only
/// those — to Markdown, leaving authored Markdown (tables, fences, heading
/// whitespace) byte-identical and keeping inline HTML like `<br>` for GPUI.
pub fn discussion_markdown(body: &str) -> String {
    if body.trim().is_empty() {
        return "*No body*".to_string();
    }

    let Ok(Node::Root(root)) = markdown::to_mdast(body, &ParseOptions::gfm()) else {
        return body.to_string();
    };
    let mut replacements = Vec::new();
    collect_block_html(&root.children, &mut replacements);
    if replacements.is_empty() {
        return body.to_string();
    }

    let mut normalized = body.to_string();
    for (start, end, replacement) in replacements.into_iter().rev() {
        normalized.replace_range(start..end, &replacement);
    }
    normalized
}

fn collect_block_html(children: &[Node], replacements: &mut Vec<(usize, usize, String)>) {
    for child in children {
        match child {
            Node::Html(html) => {
                let Some(position) = &html.position else {
                    continue;
                };
                let converted = htmd::convert(&html.value).unwrap_or_else(|_| html.value.clone());
                // Block HTML sits inline with adjacent text when GitHub emits
                // `<p><em>...</em></p>` on the line after a paragraph. Blank
                // lines keep the converted block from merging into it.
                replacements.push((
                    position.start.offset,
                    position.end.offset,
                    format!("\n\n{converted}\n\n"),
                ));
            }
            // HTML can appear as a block child of blockquotes/lists too
            // (e.g. `> <p>x</p>`). Paragraph-level children are inline HTML
            // that GPUI handles natively, so recursion stops there.
            Node::Blockquote(_) | Node::List(_) | Node::ListItem(_) => {
                if let Some(nested) = child.children() {
                    collect_block_html(nested, replacements);
                }
            }
            _ => {}
        }
    }
}
