use super::tables::TimingRow;
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

    if view.response_tables_dirty {
        view.response_tables_dirty = false;
        let timing_rows = vec![
            TimingRow {
                phase: es_fluent::localize("request_tab_response_timing_total", None).to_string(),
                value: resp
                    .total_ms
                    .map(|ms| format!("{ms} ms"))
                    .unwrap_or_else(|| "—".to_string()),
            },
            TimingRow {
                phase: es_fluent::localize("request_tab_response_timing_ttfb", None).to_string(),
                value: resp
                    .ttfb_ms
                    .map(|ms| format!("{ms} ms"))
                    .unwrap_or_else(|| "—".to_string()),
            },
            TimingRow {
                phase: es_fluent::localize("request_tab_response_timing_dispatched_at", None)
                    .to_string(),
                value: format_unix_ms(resp.dispatched_at_unix_ms),
            },
            TimingRow {
                phase: es_fluent::localize("request_tab_response_timing_first_byte_at", None)
                    .to_string(),
                value: format_unix_ms(resp.first_byte_at_unix_ms),
            },
            TimingRow {
                phase: es_fluent::localize("request_tab_response_timing_completed_at", None)
                    .to_string(),
                value: format_unix_ms(resp.completed_at_unix_ms),
            },
            TimingRow {
                phase: es_fluent::localize("request_tab_response_timing_dns", None).to_string(),
                value: "—".to_string(),
            },
            TimingRow {
                phase: es_fluent::localize("request_tab_response_timing_tcp", None).to_string(),
                value: "—".to_string(),
            },
            TimingRow {
                phase: es_fluent::localize("request_tab_response_timing_tls", None).to_string(),
                value: "—".to_string(),
            },
        ];
        view.headers_table.update(cx, |state, cx| {
            state.delegate_mut().set_rows(header_rows.clone());
            state.refresh(cx);
        });
        view.cookies_table.update(cx, |state, cx| {
            state.delegate_mut().set_rows(cookies.clone());
            state.refresh(cx);
        });
        view.timing_table.update(cx, |state, cx| {
            state.delegate_mut().set_rows(timing_rows);
            state.refresh(cx);
        });
    }

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

    let body_search_query = view.body_search_input.read(cx).value().to_string();
    let body_matches = search_matches(&body_preview, &body_search_query);

    let is_html = looks_like_html(resp.media_type.as_deref());
    let html_body_for_preview = body_preview.clone();

    let mut body_content = if looks_like_image(resp.media_type.as_deref()) {
        div().text_sm().text_color(muted).child(es_fluent::localize(
            "request_tab_response_image_preview_todo",
            None,
        ))
    } else if !body_preview.is_empty() {
        div()
            .mt_2()
            .p_3()
            .rounded(px(4.))
            .bg(bg)
            .text_sm()
            .font_family("monospace")
            .child(body_preview)
    } else {
        div()
            .text_sm()
            .text_color(muted)
            .child(es_fluent::localize("request_tab_response_body_empty", None))
    };

    if view.body_search_visible {
        body_content = v_flex()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .child(Input::new(&view.body_search_input).large()),
                    )
                    .child(div().text_xs().text_color(muted).child(format!(
                        "{} {}",
                        body_matches.len(),
                        es_fluent::localize("request_tab_search_matches", None)
                    ))),
            )
            .child(body_content)
    }

    let headers_content = if header_rows.is_empty() {
        div().text_sm().text_color(muted).child(es_fluent::localize(
            "request_tab_response_headers_empty",
            None,
        ))
    } else {
        v_flex()
            .gap_1()
            .when(
                matches!(header_format, Some(HeaderJsonFormat::LegacyObjectMap)),
                |el: gpui::Div| {
                    el.child(
                        div()
                            .text_xs()
                            .text_color(gpui::hsla(30. / 360., 0.9, 0.38, 1.))
                            .child(es_fluent::localize(
                                "request_tab_response_headers_legacy_note",
                                None,
                            )),
                    )
                },
            )
            .child(
                div()
                    .h(px(200.))
                    .child(DataTable::new(&view.headers_table).bordered(true)),
            )
    };

    let cookies_content = if cookies.is_empty() {
        div().text_sm().text_color(muted).child(es_fluent::localize(
            "request_tab_response_cookies_empty",
            None,
        ))
    } else {
        div()
            .h(px(200.))
            .child(DataTable::new(&view.cookies_table).bordered(true))
    };

    let timing_content = div()
        .h(px(280.))
        .child(DataTable::new(&view.timing_table).bordered(true));

    let can_copy = is_text_like_media_type(resp.media_type.as_deref());
    let body_actions = actions::render_body_actions(view, can_copy, cx);

    let is_preview_active = view.active_response_tab == ResponseTab::Preview;
    if is_preview_active && is_html && !html_body_for_preview.is_empty() {
        view.ensure_html_webview(window, cx);
        if let Some(webview) = &view.html_webview {
            webview.update(cx, |w, _| {
                let _ = w.raw().load_html(&html_body_for_preview);
                w.show();
            });
        }
    } else {
        view.html_webview = None;
    }
    let preview_content = if is_html && is_preview_active && view.html_webview.is_some() {
        div().h(px(400.)).child(view.html_webview.clone().unwrap())
    } else if is_html && html_body_for_preview.is_empty() {
        div().text_sm().text_color(muted).child(es_fluent::localize(
            "request_tab_response_preview_empty",
            None,
        ))
    } else if !is_html {
        div().text_sm().text_color(muted).child(es_fluent::localize(
            "request_tab_response_preview_not_html",
            None,
        ))
    } else {
        div()
    };

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
