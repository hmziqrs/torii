use super::*;

pub(super) fn render_status_and_meta(
    resp: &crate::domain::response::ResponseSummary,
    status_color: Hsla,
    status_size: String,
    muted: Hsla,
) -> gpui::Div {
    v_flex()
        .gap_2()
        .child(
            h_flex().gap_3().items_center().child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(status_color)
                    .child(format!("{} {}", resp.status_code, resp.status_text)),
            ),
        )
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .text_xs()
                .text_color(muted)
                .child(format!(
                    "{}: {}",
                    es_fluent::localize("request_tab_response_size", None),
                    status_size
                ))
                .child("•")
                .child(format!(
                    "{}: {}",
                    es_fluent::localize("request_tab_response_total_time", None),
                    resp.total_ms
                        .map(|ms| format!("{ms} ms"))
                        .unwrap_or_else(|| "—".to_string())
                )),
        )
}

pub(super) fn render_tab_strip(
    view: &mut RequestTabView,
    is_html: bool,
    cx: &mut Context<RequestTabView>,
) -> gpui::Div {
    h_flex()
        .gap_1()
        .flex_wrap()
        .child(response_tab_button(
            "request-response-tab-body",
            es_fluent::localize("request_tab_response_tab_body", None).to_string(),
            view.active_response_tab == ResponseTab::Body,
            cx,
            cx.listener(|this, _, _, cx| {
                this.set_active_response_tab(ResponseTab::Body, cx);
            }),
        ))
        .when(is_html, |el| {
            el.child(response_tab_button(
                "request-response-tab-preview",
                es_fluent::localize("request_tab_response_tab_preview", None).to_string(),
                view.active_response_tab == ResponseTab::Preview,
                cx,
                cx.listener(|this, _, _, cx| {
                    this.set_active_response_tab(ResponseTab::Preview, cx);
                }),
            ))
        })
        .child(response_tab_button(
            "request-response-tab-headers",
            es_fluent::localize("request_tab_response_tab_headers", None).to_string(),
            view.active_response_tab == ResponseTab::Headers,
            cx,
            cx.listener(|this, _, _, cx| {
                this.set_active_response_tab(ResponseTab::Headers, cx);
            }),
        ))
        .child(response_tab_button(
            "request-response-tab-cookies",
            es_fluent::localize("request_tab_response_tab_cookies", None).to_string(),
            view.active_response_tab == ResponseTab::Cookies,
            cx,
            cx.listener(|this, _, _, cx| {
                this.set_active_response_tab(ResponseTab::Cookies, cx);
            }),
        ))
        .child(response_tab_button(
            "request-response-tab-timing",
            es_fluent::localize("request_tab_response_tab_timing", None).to_string(),
            view.active_response_tab == ResponseTab::Timing,
            cx,
            cx.listener(|this, _, _, cx| {
                this.set_active_response_tab(ResponseTab::Timing, cx);
            }),
        ))
}
