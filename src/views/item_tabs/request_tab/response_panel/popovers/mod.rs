use std::time::Duration;

use gpui::{AnyElement, FontWeight, IntoElement, ParentElement as _, Styled as _, div, px};
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
        .child(div().flex_1())
        .child(network::render_network_popover(view, response, cx))
}

pub(super) fn hover_popover_trigger(
    view: &mut RequestTabView,
    id: &'static str,
    token: gpui::Div,
    variant: ResponseMetaPopover,
    anchor: Anchor,
    content: impl Fn(&App) -> AnyElement + 'static,
    cx: &mut Context<RequestTabView>,
) -> AnyElement {
    let open_delay = Duration::from_millis(if view.active_meta_popover == Some(variant) {
        0
    } else {
        120
    });

    HoverCard::new(id)
        .anchor(anchor)
        .appearance(true)
        .open_delay(open_delay)
        .close_delay(Duration::from_millis(120))
        .on_open_change(cx.listener(move |this, open, _, cx| {
            if *open {
                this.set_active_meta_popover(Some(variant), cx);
            } else if this.active_meta_popover == Some(variant) {
                this.set_active_meta_popover(None, cx);
            }
        }))
        .trigger(token)
        .content(move |_, _, cx| content(cx))
        .into_any_element()
}

pub(super) fn token_text(label: impl Into<String>, color: gpui::Hsla, bold: bool) -> gpui::Div {
    div()
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
