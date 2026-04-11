use gpui::{AnyElement, IntoElement, ParentElement, Styled as _, div, px};
use gpui_component::{h_flex, v_flex};

use crate::domain::request::RequestItem;

pub fn render(request: &RequestItem) -> AnyElement {
    v_flex()
        .size_full()
        .p_6()
        .gap_5()
        .child(
            div()
                .text_2xl()
                .font_weight(gpui::FontWeight::BOLD)
                .child(request.name.clone()),
        )
        .child(
            h_flex()
                .gap_3()
                .child(chip(request.method.clone()))
                .child(chip(request.url.clone())),
        )
        .child(div().child(es_fluent::localize("request_tab_hint", None)))
        .into_any_element()
}

fn chip(label: String) -> impl IntoElement {
    div().px_2().py_1().rounded(px(999.)).border_1().child(label)
}
