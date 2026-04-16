use super::*;

pub(super) fn render_completed_response(
    view: &mut RequestTabView,
    resp: &crate::domain::response::ResponseSummary,
    window: &mut Window,
    cx: &mut Context<RequestTabView>,
) -> gpui::Div {
    let muted = cx.theme().muted_foreground;
    let bg = cx.theme().background;
    let status_color = status_code_color(resp.status_code);
    let status_size = format_bytes(resp.body_ref.size_bytes());

    let mut body_preview = response_body_preview_text(resp, &view.loaded_full_body_text);
    let (header_rows, header_format) = parse_response_header_rows(resp.headers_json.as_deref());
    let cookies = parse_set_cookie_rows(&header_rows);

    content_tabs::refresh_response_tables_if_dirty(view, resp, &header_rows, &cookies, cx);

    let load_full_button = match &resp.body_ref {
        BodyRef::DiskBlob { blob_id, .. } => {
            if view.loaded_full_body_blob_id.as_deref() == Some(blob_id.as_str()) {
                if let Some(full) = &view.loaded_full_body_text {
                    body_preview = full.clone();
                }
                div()
            } else {
                div().child(
                    Button::new("request-load-full-body")
                        .outline()
                        .label(es_fluent::localize(
                            "request_tab_action_load_full_body",
                            None,
                        ))
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Err(err) = this.load_full_response_body(cx) {
                                window.push_notification(err, cx);
                            }
                        })),
                )
            }
        }
        _ => div(),
    };

    let is_html = looks_like_html(resp.media_type.as_deref());
    let html_body_for_preview = body_preview.clone();
    let body_content =
        content_tabs::render_body_content(view, resp, body_preview.clone(), muted, bg, cx);
    let headers_content =
        content_tabs::render_headers_content(view, &header_rows, header_format, muted);
    let cookies_content = content_tabs::render_cookies_content(view, &cookies, muted);
    let timing_content = content_tabs::render_timing_content(view);

    let can_copy = is_text_like_media_type(resp.media_type.as_deref());
    let body_actions = actions::render_body_actions(view, can_copy, cx);

    let preview_content = content_tabs::render_preview_content(
        view,
        is_html,
        &html_body_for_preview,
        muted,
        window,
        cx,
    );

    let active_content = match view.active_response_tab {
        ResponseTab::Body => body_content.into_any_element(),
        ResponseTab::Preview => preview_content.into_any_element(),
        ResponseTab::Headers => headers_content.into_any_element(),
        ResponseTab::Cookies => cookies_content.into_any_element(),
        ResponseTab::Timing => timing_content.into_any_element(),
    };

    div()
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
        .child(
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
                )),
        )
        .when(
            view.active_response_tab == ResponseTab::Body,
            |el: gpui::Div| el.child(body_actions),
        )
        .child(active_content)
        .child(load_full_button)
}
