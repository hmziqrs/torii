use super::*;

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
