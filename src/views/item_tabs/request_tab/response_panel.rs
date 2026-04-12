use super::*;

// ---------------------------------------------------------------------------
// Response panel rendering — extracted from RequestTabView::render
// ---------------------------------------------------------------------------

pub(super) fn render_response_panel(
    view: &mut RequestTabView,
    window: &mut Window,
    cx: &mut Context<RequestTabView>,
) -> gpui::Div {
    match view.editor.exec_status() {
        ExecStatus::Idle => div().child(
            div()
                .text_sm()
                .text_color(gpui::hsla(0., 0., 0.5, 1.))
                .child(es_fluent::localize("request_tab_response_empty", None)),
        ),
        ExecStatus::Sending => div().child(
            div()
                .text_sm()
                .text_color(gpui::hsla(0., 0., 0.5, 1.))
                .child(es_fluent::localize("request_tab_sending", None)),
        ),
        ExecStatus::Streaming => div().child(
            div()
                .text_sm()
                .text_color(gpui::hsla(0., 0., 0.5, 1.))
                .child(es_fluent::localize("request_tab_streaming", None)),
        ),
        ExecStatus::Completed { .. } => {
            // Clone the response to release the immutable borrow on view.editor
            // before calling render_completed_response which needs &mut view.
            let response = match view.editor.exec_status() {
                ExecStatus::Completed { response } => response.clone(),
                _ => unreachable!(),
            };
            render_completed_response(view, &response, window, cx)
        }
        ExecStatus::Failed { .. } => {
            let (summary, classified) = match view.editor.exec_status() {
                ExecStatus::Failed { summary, classified } => (summary.clone(), classified.clone()),
                _ => unreachable!(),
            };
            let (title, detail) = classified_error_display(classified.as_ref(), &summary);
            let expanded = view.error_detail_expanded;
            div()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(gpui::red())
                        .child(title),
                )
                .child(
                    div()
                        .text_xs()
                        .font_family("monospace")
                        .text_color(gpui::hsla(0., 0., 0.45, 1.))
                        .child(if expanded { detail.clone() } else { summary.clone() }),
                )
                .child(
                    Button::new("error-detail-toggle")
                        .ghost()
                        .label(if expanded {
                            es_fluent::localize("request_tab_error_detail_collapse", None)
                        } else {
                            es_fluent::localize("request_tab_error_detail_expand", None)
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.error_detail_expanded = !this.error_detail_expanded;
                            cx.notify();
                        })),
                )
        }
        ExecStatus::Cancelled { .. } => {
            let partial_size = match view.editor.exec_status() {
                ExecStatus::Cancelled { partial_size } => *partial_size,
                _ => unreachable!(),
            };
            let msg = match partial_size {
                Some(size) => format!(
                    "{} ({size})",
                    es_fluent::localize("request_tab_response_cancelled_with_bytes", None)
                ),
                None => es_fluent::localize("request_tab_response_cancelled", None).to_string(),
            };
            div().child(
                div()
                    .text_sm()
                    .text_color(gpui::hsla(30. / 360., 0.8, 0.45, 1.))
                    .child(msg),
            )
        }
    }
}

fn render_completed_response(
    view: &mut RequestTabView,
    resp: &crate::domain::response::ResponseSummary,
    _window: &mut Window,
    cx: &mut Context<RequestTabView>,
) -> gpui::Div {
    let status_color = status_code_color(resp.status_code);
    let status_size = format_bytes(resp.body_ref.size_bytes());

    let mut body_preview = response_body_preview_text(resp, &view.loaded_full_body_text);
    let (header_rows, header_format) = parse_response_header_rows(resp.headers_json.as_deref());
    let cookies = parse_set_cookie_rows(&header_rows);

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

    let mut body_content = if looks_like_image(resp.media_type.as_deref()) {
        div()
            .text_sm()
            .text_color(gpui::hsla(0., 0., 0.5, 1.))
            .child(es_fluent::localize(
                "request_tab_response_image_preview_todo",
                None,
            ))
    } else if !body_preview.is_empty() {
        div()
            .mt_2()
            .p_3()
            .rounded(px(4.))
            .bg(gpui::hsla(0., 0., 0.97, 1.))
            .text_sm()
            .font_family("monospace")
            .child(body_preview)
    } else {
        div()
            .text_sm()
            .text_color(gpui::hsla(0., 0., 0.5, 1.))
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
                    .child(
                        div()
                            .text_xs()
                            .text_color(gpui::hsla(0., 0., 0.5, 1.))
                            .child(format!(
                                "{} {}",
                                body_matches.len(),
                                es_fluent::localize("request_tab_search_matches", None)
                            )),
                    ),
            )
            .child(body_content)
    }

    let headers_content = if header_rows.is_empty() {
        div()
            .text_sm()
            .text_color(gpui::hsla(0., 0., 0.5, 1.))
            .child(es_fluent::localize(
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
            .children(header_rows.iter().enumerate().map(|(idx, row)| {
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .font_family("monospace")
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(row.name.clone()),
                    )
                    .child(
                        div()
                            .font_family("monospace")
                            .text_sm()
                            .text_color(gpui::hsla(0., 0., 0.35, 1.))
                            .child(row.value.clone()),
                    )
                    .id(("response-header-row", idx))
            }))
    };

    let cookies_content = if cookies.is_empty() {
        div()
            .text_sm()
            .text_color(gpui::hsla(0., 0., 0.5, 1.))
            .child(es_fluent::localize(
                "request_tab_response_cookies_empty",
                None,
            ))
    } else {
        v_flex().gap_1().children(cookies.iter().enumerate().map(
            |(idx, cookie)| {
                let same_site = cookie.same_site.clone().unwrap_or_else(|| "—".to_string());
                let expires = cookie
                    .expires_or_max_age
                    .clone()
                    .unwrap_or_else(|| "—".to_string());
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .font_family("monospace")
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(cookie.name.clone()),
                    )
                    .child(
                        div()
                            .font_family("monospace")
                            .text_sm()
                            .text_color(gpui::hsla(0., 0., 0.35, 1.))
                            .child(format!(
                            "{}; domain={}; path={}; expires/max-age={}; secure={}; httpOnly={}; sameSite={}",
                            cookie.value_preview,
                            cookie.domain.clone().unwrap_or_else(|| "—".to_string()),
                            cookie.path.clone().unwrap_or_else(|| "—".to_string()),
                            expires,
                            if cookie.secure { "true" } else { "false" },
                            if cookie.http_only { "true" } else { "false" },
                            same_site,
                        )),
                    )
                    .id(("response-cookie-row", idx))
            },
        ))
    };

    let timing_content = v_flex()
        .gap_1()
        .child(timing_row(
            es_fluent::localize("request_tab_response_timing_total", None).to_string(),
            resp.total_ms
                .map(|ms| format!("{ms} ms"))
                .unwrap_or_else(|| "—".to_string()),
        ))
        .child(timing_row(
            es_fluent::localize("request_tab_response_timing_ttfb", None).to_string(),
            resp.ttfb_ms
                .map(|ms| format!("{ms} ms"))
                .unwrap_or_else(|| "—".to_string()),
        ))
        .child(timing_row(
            es_fluent::localize("request_tab_response_timing_dispatched_at", None).to_string(),
            format_unix_ms(resp.dispatched_at_unix_ms),
        ))
        .child(timing_row(
            es_fluent::localize("request_tab_response_timing_first_byte_at", None).to_string(),
            format_unix_ms(resp.first_byte_at_unix_ms),
        ))
        .child(timing_row(
            es_fluent::localize("request_tab_response_timing_completed_at", None).to_string(),
            format_unix_ms(resp.completed_at_unix_ms),
        ))
        .child(timing_row(
            es_fluent::localize("request_tab_response_timing_dns", None).to_string(),
            "—".to_string(),
        ))
        .child(timing_row(
            es_fluent::localize("request_tab_response_timing_tcp", None).to_string(),
            "—".to_string(),
        ))
        .child(timing_row(
            es_fluent::localize("request_tab_response_timing_tls", None).to_string(),
            "—".to_string(),
        ));

    let can_copy = is_text_like_media_type(resp.media_type.as_deref());
    let body_actions = h_flex()
        .gap_1()
        .child({
            let mut btn = Button::new("request-response-copy")
                .outline()
                .disabled(!can_copy)
                .label(es_fluent::localize(
                    "request_tab_response_action_copy",
                    None,
                ));
            if !can_copy {
                btn = btn.tooltip(es_fluent::localize(
                    "request_tab_response_action_copy_disabled_tooltip",
                    None,
                ));
            }
            btn
                .on_click(cx.listener(|this, _, window, cx| {
                    if let Err(err) = this.copy_response_body(cx) {
                        window.push_notification(err, cx);
                    } else {
                        window.push_notification(
                            es_fluent::localize("request_tab_copy_ok", None),
                            cx,
                        );
                    }
                }))
        })
        .child(
            Button::new("request-response-save")
                .outline()
                .label(es_fluent::localize(
                    "request_tab_response_action_save",
                    None,
                ))
                .on_click(cx.listener(|this, _, window, cx| {
                    if let Err(err) = this.save_response_body_to_file(window, cx) {
                        window.push_notification(err, cx);
                    }
                })),
        );

    let active_content = match view.active_response_tab {
        ResponseTab::Body => body_content.into_any_element(),
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
                .text_color(gpui::hsla(0., 0., 0.5, 1.))
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
                    cx.listener(|this, _, _, cx| {
                        this.set_active_response_tab(ResponseTab::Body, cx);
                    }),
                ))
                .child(response_tab_button(
                    "request-response-tab-headers",
                    es_fluent::localize("request_tab_response_tab_headers", None).to_string(),
                    view.active_response_tab == ResponseTab::Headers,
                    cx.listener(|this, _, _, cx| {
                        this.set_active_response_tab(ResponseTab::Headers, cx);
                    }),
                ))
                .child(response_tab_button(
                    "request-response-tab-cookies",
                    es_fluent::localize("request_tab_response_tab_cookies", None).to_string(),
                    view.active_response_tab == ResponseTab::Cookies,
                    cx.listener(|this, _, _, cx| {
                        this.set_active_response_tab(ResponseTab::Cookies, cx);
                    }),
                ))
                .child(response_tab_button(
                    "request-response-tab-timing",
                    es_fluent::localize("request_tab_response_tab_timing", None).to_string(),
                    view.active_response_tab == ResponseTab::Timing,
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
