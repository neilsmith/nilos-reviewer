use gpui_kit::base::{StyledExt, h_flex, v_flex};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::*;
use nilos_reviewer::{fixtures::RENDER_FIXTURES, render};
use std::sync::Arc;

pub struct RenderPreview {
    selected: usize,
    focus: FocusHandle,
    scroll: [ScrollHandle; 2],
}

impl RenderPreview {
    pub fn new(name: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self {
            selected: RENDER_FIXTURES
                .iter()
                .position(|fixture| fixture.name == name)
                .unwrap_or(0),
            focus,
            scroll: [ScrollHandle::new(), ScrollHandle::new()],
        }
    }

    fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected = index;
        for scroll in &self.scroll {
            scroll.set_offset(point(px(0.), px(0.)));
        }
        cx.notify();
    }

    fn nav(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "left" => self.select(self.selected.saturating_sub(1), cx),
            "right" => self.select((self.selected + 1).min(RENDER_FIXTURES.len() - 1), cx),
            key @ ("up" | "down" | "pageup" | "pagedown" | "home" | "end") => {
                for scroll in &self.scroll {
                    let offset = scroll.offset();
                    let max = scroll.max_offset();
                    let delta = match key {
                        "up" => px(40.),
                        "down" => px(-40.),
                        "pageup" => px(500.),
                        "pagedown" => px(-500.),
                        "home" => max.y,
                        _ => -max.y,
                    };
                    scroll.set_offset(point(offset.x, (offset.y + delta).clamp(-max.y, px(0.))));
                }
                cx.notify();
            }
            _ => return,
        }
        cx.stop_propagation();
    }
}

// Neither a missing image nor a click may escape this offline test surface.
fn offline(view: TextView) -> TextView {
    view.image_source(|_| ImageSource::Custom(Arc::new(|_, _| None)))
        .on_link_click(|_, _, _, _| {})
}

impl Render for RenderPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let fixture = &RENDER_FIXTURES[self.selected];
        v_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .font_family(theme.font_family.clone())
            .text_sm()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::nav))
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_2()
                    .p_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .children(RENDER_FIXTURES.iter().enumerate().map(|(ix, fixture)| {
                        let button = Button::new(("fixture", ix)).small().label(fixture.name);
                        let button = if ix == self.selected { button.primary() } else { button.ghost() };
                        button.on_click(cx.listener(move |this, _, _, cx| this.select(ix, cx)))
                    })),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .children((0..2).map(|ix| {
                        let view = if ix == 0 {
                            TextView::html(
                                ElementId::Name(format!("reference-{}", fixture.name).into()),
                                fixture.github_html,
                            )
                            .style(render::body_style(theme))
                        } else {
                            render::discussion_view(
                                ElementId::Name(format!("native-{}", fixture.name).into()),
                                fixture.markdown,
                                theme,
                            )
                        };
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .border_r_1()
                            .border_color(theme.border)
                            .child(
                                div().p_3().border_b_1().border_color(theme.border).font_semibold()
                                    .child(if ix == 0 { "GitHub HTML reference (native)" } else { "App Markdown (shared renderer)" }),
                            )
                            .child(
                                div()
                                    .id(("fixture-scroll", ix))
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.scroll[ix])
                                    .p_4()
                                    .child(offline(view)),
                            )
                    })),
            )
            .child(
                div().p_2().text_xs().text_color(theme.muted_foreground)
                    .child("Offline fixtures - no GitHub authentication, requests or review actions. Left/Right: fixture; Up/Down/PageUp/PageDown/Home/End: scroll both panes. Links and remote images disabled."),
            )
    }
}
