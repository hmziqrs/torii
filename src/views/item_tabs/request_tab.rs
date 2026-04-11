use gpui::{AppContext as _, Context, Entity, IntoElement, ParentElement, Render, Styled as _, Window, div, px};
use gpui_component::{
    h_flex,
    input::{Input, InputState},
    Sizable as _,
    v_flex,
};

use crate::domain::request::RequestItem;

pub struct RequestTabView {
    request: RequestItem,
    input: Entity<InputState>,
}

impl RequestTabView {
    pub fn new(request: &RequestItem, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let initial_url = request.url.clone();
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(initial_url, window, cx);
            state
        });

        Self {
            request: request.clone(),
            input,
        }
    }
}

impl Render for RequestTabView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .p_6()
            .gap_5()
            .child(
                div()
                    .text_2xl()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(self.request.name.clone()),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(chip(self.request.method.clone()))
                    .child(chip(self.request.url.clone())),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(es_fluent::localize("request_tab_url_label", None)),
                    )
                    .child(Input::new(&self.input).large()),
            )
            .child(div().child(es_fluent::localize("request_tab_hint", None)))
    }
}

fn chip(label: String) -> impl IntoElement {
    div().px_2().py_1().rounded(px(999.)).border_1().child(label)
}
