use gpui::{
    AnyElement, App, FocusHandle, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderOnce, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::{ActiveTheme as _, Anchor, Selectable, h_flex, popover::Popover};

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
    variant: ResponseMetaHover,
    focus_handle: FocusHandle,
    anchor: Anchor,
    content: impl Fn(&App) -> AnyElement + 'static,
    cx: &mut Context<RequestTabView>,
) -> AnyElement {
    let view_entity = cx.entity();
    let trigger = token
        .id(id)
        .track_focus(&focus_handle)
        .on_hover(cx.listener(move |this, hovered, _, cx| {
            if *hovered {
                this.meta_hover_enter(variant, cx);
            } else {
                this.meta_hover_leave(variant, cx);
            }
        }));

    Popover::new(id)
        .anchor(anchor)
        .open(view.meta_hover == variant)
        .overlay_closable(false)
        .appearance(true)
        .trigger(PopoverTrigger::new(trigger.into_any_element()))
        .content(move |_, window, popover_cx| {
            div()
                .id(format!("{id}-content"))
                .on_hover(
                    window.listener_for(&view_entity, move |this, hovered, _, cx| {
                        if *hovered {
                            this.meta_hover_enter(variant, cx);
                        } else {
                            this.meta_hover_leave(variant, cx);
                        }
                    }),
                )
                .child(content(popover_cx))
        })
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

#[derive(IntoElement)]
struct PopoverTrigger {
    element: AnyElement,
    selected: bool,
}

impl PopoverTrigger {
    fn new(element: AnyElement) -> Self {
        Self {
            element,
            selected: false,
        }
    }
}

impl Selectable for PopoverTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for PopoverTrigger {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.element
    }
}
