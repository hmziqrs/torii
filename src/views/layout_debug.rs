use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme as _, Sizable as _,
    button::Button,
    h_flex,
    input::{Input, InputState},
    resizable::{resizable_panel, v_resizable},
    v_flex,
};

pub struct LayoutDebugPage {
    url_like_input: Entity<InputState>,
    request_item_count: usize,
    response_item_count: usize,
}

impl LayoutDebugPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url_like_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value("https://api.example.test/users?sad=", window, cx);
            state
        });

        Self {
            url_like_input,
            request_item_count: 1,
            response_item_count: 1,
        }
    }
}

impl Render for LayoutDebugPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let box_colors = [
            gpui::hsla(210. / 360., 0.25, 0.20, 1.0),
            gpui::hsla(190. / 360., 0.28, 0.22, 1.0),
            gpui::hsla(170. / 360., 0.22, 0.24, 1.0),
            gpui::hsla(230. / 360., 0.22, 0.20, 1.0),
        ];

        v_flex()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .child(
                div()
                    .h(px(52.))
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .w_full()
                            .child(Input::new(&self.url_like_input).large()),
                    ),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .p_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("layout-debug-request-dec")
                            .outline()
                            .label("Request -")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.request_item_count =
                                    this.request_item_count.saturating_sub(1).max(1);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("layout-debug-request-inc")
                            .outline()
                            .label("Request +")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.request_item_count = this.request_item_count.saturating_add(1);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("layout-debug-response-dec")
                            .outline()
                            .label("Response -")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.response_item_count =
                                    this.response_item_count.saturating_sub(1).max(1);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("layout-debug-response-inc")
                            .outline()
                            .label("Response +")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.response_item_count =
                                    this.response_item_count.saturating_add(1);
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "request={} response={}",
                                self.request_item_count, self.response_item_count
                            )),
                    ),
            )
            .child(
                div().flex_1().min_h_0().overflow_hidden().child(
                    v_resizable("layout-debug-split")
                        .child(
                            resizable_panel()
                                .size(px(320.))
                                .size_range(px(80.)..px(99999.))
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_h_0()
                                        .overflow_hidden()
                                        .child(
                                            div()
                                                .flex_shrink_0()
                                                .px_3()
                                                .py_2()
                                                .text_sm()
                                                .border_b_1()
                                                .border_color(cx.theme().border)
                                                .child("Request panel (debug)"),
                                        )
                                        .child(
                                            div()
                                                .id("layout-debug-request-scroll")
                                                .flex_1()
                                                .min_h_0()
                                                .overflow_y_scroll()
                                                .child(v_flex().gap_2().p_2().children(
                                                    (0..self.request_item_count).map(|i| {
                                                        div()
                                                            .flex_shrink_0()
                                                            .w_full()
                                                            .h(px(68.))
                                                            .rounded(px(6.))
                                                            .bg(box_colors[i % box_colors.len()])
                                                            .border_1()
                                                            .border_color(cx.theme().border)
                                                            .px_3()
                                                            .py_2()
                                                            .child(format!(
                                                                "Mock request content row {}",
                                                                i + 1
                                                            ))
                                                    }),
                                                )),
                                        ),
                                ),
                        )
                        .child(
                            resizable_panel()
                                .size(px(220.))
                                .size_range(px(100.)..px(99999.))
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_h_0()
                                        .overflow_hidden()
                                        .border_1()
                                        .border_color(cx.theme().border)
                                        .child(
                                            div()
                                                .px_3()
                                                .py_2()
                                                .text_sm()
                                                .border_b_1()
                                                .border_color(cx.theme().border)
                                                .child("Response panel (debug)"),
                                        )
                                        .child(
                                            div()
                                                .id("layout-debug-response-scroll")
                                                .flex_1()
                                                .min_h_0()
                                                .overflow_y_scroll()
                                                .child(v_flex().px_3().py_2().gap_2().children(
                                                    (0..self.response_item_count).map(|i| {
                                                        div()
                                                            .flex_shrink_0()
                                                            .w_full()
                                                            .h(px(44.))
                                                            .rounded(px(4.))
                                                            .bg(gpui::hsla(
                                                                35. / 360.,
                                                                0.30,
                                                                0.18 + (i % 2) as f32 * 0.05,
                                                                1.0,
                                                            ))
                                                            .child(format!(
                                                                "Mock response block {}",
                                                                i + 1
                                                            ))
                                                    }),
                                                )),
                                        ),
                                ),
                        ),
                ),
            )
    }
}
