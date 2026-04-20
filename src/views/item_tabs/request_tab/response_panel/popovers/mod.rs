use std::time::Duration;

use gpui::{AnyElement, App, FontWeight, IntoElement, ParentElement as _, Styled as _, div, px};
use gpui_component::{ActiveTheme as _, Anchor, h_flex, hover_card::HoverCard};

use super::*;

mod network;
mod size;
mod status;
mod time;

pub(super) fn render_meta_bar(
    view: &mut RequestTabView,
    response: &crate::domain::response::ResponseSummary,
    response_label: gpui::Div,
    cx: &mut Context<RequestTabView>,
) -> gpui::Div {
    h_flex()
        .flex_shrink_0()
        .gap_3()
        .items_center()
        .pt_3()
        .child(response_label)
        .child(Divider::vertical().color(cx.theme().border))
        .child(status::render_status_popover(view, response, cx))
        .child(dot(cx))
        .child(time::render_time_popover(view, response, cx))
        .child(dot(cx))
        .child(size::render_size_popover(view, response, cx))
        .child(dot(cx))
        .child(
            div()
                .flex_none()
                .child(network::render_network_popover(view, response, cx)),
        )
}

pub(super) fn hover_popover_trigger(
    id: &'static str,
    token: gpui::Div,
    anchor: Anchor,
    content: impl Fn(&App) -> AnyElement + 'static,
    _cx: &mut Context<RequestTabView>,
) -> AnyElement {
    let trigger_id = format!("{id}-trigger");
    HoverCard::new(id)
        .anchor(anchor)
        .open_delay(Duration::from_millis(80))
        .close_delay(Duration::from_millis(160))
        .appearance(true)
        .trigger(div().flex_none().child(token.id(trigger_id)))
        .content(move |_, _, popover_cx| content(popover_cx))
        .into_any_element()
}

pub(super) fn token_text(label: impl Into<String>, color: gpui::Hsla, bold: bool) -> gpui::Div {
    div()
        .flex_none()
        .whitespace_nowrap()
        .text_xs()
        .when(bold, |el| el.font_weight(FontWeight::BOLD))
        .text_color(color)
        .child(label.into())
}

pub(super) fn dash_or(value: Option<String>) -> String {
    value.unwrap_or_else(|| "—".to_string())
}

pub(super) fn format_ms(value: Option<u64>) -> String {
    value
        .map(|ms| format!("{ms} ms"))
        .unwrap_or_else(|| "—".to_string())
}

pub(super) fn format_optional_bytes(value: Option<u64>) -> String {
    value.map(format_bytes).unwrap_or_else(|| "—".to_string())
}

fn dot(cx: &App) -> gpui::Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child("•")
}

pub(super) fn base_popover_container(title: String, cx: &App) -> gpui::Div {
    v_flex()
        .w(px(340.))
        .gap_2()
        .p_3()
        .child(div().text_sm().font_weight(FontWeight::BOLD).child(title))
        .text_color(cx.theme().foreground)
}

pub(super) fn row(label: String, value: String, muted: gpui::Hsla) -> gpui::Div {
    h_flex()
        .items_center()
        .justify_between()
        .gap_2()
        .child(div().text_xs().text_color(muted).child(label))
        .child(div().text_xs().font_family("monospace").child(value))
}
