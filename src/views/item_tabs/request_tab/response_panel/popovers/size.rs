use gpui::IntoElement;
use gpui_component::Anchor;

use super::*;

pub(super) fn render_size_popover(
    view: &mut RequestTabView,
    response: &crate::domain::response::ResponseSummary,
    cx: &mut Context<RequestTabView>,
) -> gpui::AnyElement {
    let token = format!(
        "{}: {}",
        es_fluent::localize("request_tab_response_size", None),
        format_bytes(response.size.body_decoded_bytes)
    );
    let size = response.size.clone();
    let request_size = response.request_size.clone();

    hover_popover_trigger(
        view,
        "response-size-popover",
        token_text(token, cx.theme().muted_foreground, false),
        ResponseMetaPopover::Size,
        Anchor::TopLeft,
        move |cx| {
            let muted = cx.theme().muted_foreground;
            base_popover_container(
                es_fluent::localize("request_tab_response_size_popover_title", None).to_string(),
                cx,
            )
            .child(row(
                es_fluent::localize("request_tab_response_size_popover_response", None).to_string(),
                String::new(),
                muted,
            ))
            .child(row(
                es_fluent::localize("request_tab_response_size_popover_headers", None).to_string(),
                format_optional_bytes(size.headers_bytes),
                muted,
            ))
            .child(row(
                es_fluent::localize("request_tab_response_size_popover_body", None).to_string(),
                format_optional_bytes(size.body_wire_bytes),
                muted,
            ))
            .child(row(
                es_fluent::localize("request_tab_response_size_popover_uncompressed", None)
                    .to_string(),
                format_bytes(size.body_decoded_bytes),
                muted,
            ))
            .child(row(
                es_fluent::localize("request_tab_response_size_popover_request", None).to_string(),
                String::new(),
                muted,
            ))
            .child(row(
                es_fluent::localize("request_tab_response_size_popover_headers", None).to_string(),
                format_optional_bytes(request_size.headers_bytes),
                muted,
            ))
            .child(row(
                es_fluent::localize("request_tab_response_size_popover_body", None).to_string(),
                format_bytes(request_size.body_bytes),
                muted,
            ))
            .into_any_element()
        },
        cx,
    )
}
