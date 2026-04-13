use super::*;
use crate::domain::response::ResponseHeaderRow;
use gpui_component::table::DataTable;

// ---------------------------------------------------------------------------
// Table delegates for response headers and cookies
// ---------------------------------------------------------------------------

// -- Headers table delegate --------------------------------------------------

pub(super) struct HeadersTableDelegate {
    rows: Vec<ResponseHeaderRow>,
    columns: Vec<Column>,
}

impl HeadersTableDelegate {
    pub(super) fn new() -> Self {
        Self {
            rows: Vec::new(),
            columns: vec![
                Column::new("name", es_fluent::localize("request_tab_response_headers_col_name", None))
                    .width(px(200.))
                    .resizable(true)
                    .movable(false),
                Column::new("value", es_fluent::localize("request_tab_response_headers_col_value", None))
                    .width(px(500.))
                    .resizable(true)
                    .movable(false),
            ],
        }
    }

    pub(super) fn set_rows(&mut self, rows: Vec<ResponseHeaderRow>) {
        self.rows = rows;
    }
}

impl TableDelegate for HeadersTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let row = &self.rows[row_ix];
        match col_ix {
            0 => div()
                .font_family("monospace")
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(row.name.clone())
                .into_any_element(),
            _ => div()
                .font_family("monospace")
                .text_sm()
                .text_color(gpui::hsla(0., 0., 0.35, 1.))
                .child(row.value.clone())
                .into_any_element(),
        }
    }
}

// -- Cookies table delegate ---------------------------------------------------

pub(super) struct CookiesTableDelegate {
    rows: Vec<CookieRow>,
    columns: Vec<Column>,
}

impl CookiesTableDelegate {
    pub(super) fn new() -> Self {
        Self {
            rows: Vec::new(),
            columns: vec![
                Column::new("name", es_fluent::localize("request_tab_cookies_col_name", None))
                    .width(px(120.))
                    .resizable(true)
                    .movable(false),
                Column::new("value", es_fluent::localize("request_tab_cookies_col_value", None))
                    .width(px(150.))
                    .resizable(true)
                    .movable(false),
                Column::new("domain", es_fluent::localize("request_tab_cookies_col_domain", None))
                    .width(px(120.))
                    .resizable(true)
                    .movable(false),
                Column::new("path", es_fluent::localize("request_tab_cookies_col_path", None))
                    .width(px(80.))
                    .resizable(true)
                    .movable(false),
                Column::new("expires", es_fluent::localize("request_tab_cookies_col_expires", None))
                    .width(px(120.))
                    .resizable(true)
                    .movable(false),
                Column::new("secure", es_fluent::localize("request_tab_cookies_col_secure", None))
                    .width(px(60.))
                    .resizable(false)
                    .movable(false),
                Column::new("httponly", es_fluent::localize("request_tab_cookies_col_httponly", None))
                    .width(px(70.))
                    .resizable(false)
                    .movable(false),
                Column::new("samesite", es_fluent::localize("request_tab_cookies_col_samesite", None))
                    .width(px(80.))
                    .resizable(true)
                    .movable(false),
            ],
        }
    }

    pub(super) fn set_rows(&mut self, rows: Vec<CookieRow>) {
        self.rows = rows;
    }
}

impl TableDelegate for CookiesTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let cookie = &self.rows[row_ix];
        match col_ix {
            0 => div()
                .font_weight(FontWeight::MEDIUM)
                .child(cookie.name.clone())
                .into_any_element(),
            1 => div().child(cookie.value_preview.clone()).into_any_element(),
            2 => div()
                .child(cookie.domain.clone().unwrap_or_else(|| "—".to_string()))
                .into_any_element(),
            3 => div()
                .child(cookie.path.clone().unwrap_or_else(|| "—".to_string()))
                .into_any_element(),
            4 => div()
                .child(
                    cookie
                        .expires_or_max_age
                        .clone()
                        .unwrap_or_else(|| "—".to_string()),
                )
                .into_any_element(),
            5 => div()
                .child(if cookie.secure { "true" } else { "false" })
                .into_any_element(),
            6 => div()
                .child(if cookie.http_only { "true" } else { "false" })
                .into_any_element(),
            _ => div()
                .child(cookie.same_site.clone().unwrap_or_else(|| "—".to_string()))
                .into_any_element(),
        }
    }
}

// -- Timing table delegate ----------------------------------------------------

pub(super) struct TimingRow {
    phase: String,
    value: String,
}

pub(super) struct TimingTableDelegate {
    rows: Vec<TimingRow>,
    columns: Vec<Column>,
}

impl TimingTableDelegate {
    pub(super) fn new() -> Self {
        Self {
            rows: Vec::new(),
            columns: vec![
                Column::new("phase", es_fluent::localize("request_tab_response_timing_col_phase", None))
                    .width(px(200.))
                    .resizable(false)
                    .movable(false),
                Column::new("value", es_fluent::localize("request_tab_response_timing_col_value", None))
                    .width(px(400.))
                    .resizable(true)
                    .movable(false),
            ],
        }
    }

    pub(super) fn set_rows(&mut self, rows: Vec<TimingRow>) {
        self.rows = rows;
    }
}

impl TableDelegate for TimingTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let row = &self.rows[row_ix];
        match col_ix {
            0 => div()
                .text_sm()
                .text_color(gpui::hsla(0., 0., 0.45, 1.))
                .child(row.phase.clone())
                .into_any_element(),
            _ => div()
                .text_sm()
                .font_family("monospace")
                .child(row.value.clone())
                .into_any_element(),
        }
    }
}

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
    window: &mut Window,
    cx: &mut Context<RequestTabView>,
) -> gpui::Div {
    let status_color = status_code_color(resp.status_code);
    let status_size = format_bytes(resp.body_ref.size_bytes());

    let mut body_preview = response_body_preview_text(resp, &view.loaded_full_body_text);
    let (header_rows, header_format) = parse_response_header_rows(resp.headers_json.as_deref());
    let cookies = parse_set_cookie_rows(&header_rows);

    // Feed data into the table delegates
    view.headers_table.update(cx, |state, cx| {
        state.delegate_mut().set_rows(header_rows.clone());
        state.refresh(cx);
    });
    view.cookies_table.update(cx, |state, cx| {
        state.delegate_mut().set_rows(cookies.clone());
        state.refresh(cx);
    });

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
            .child(
                div().h(px(200.)).child(DataTable::new(&view.headers_table).bordered(true)),
            )
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
        div().h(px(200.)).child(DataTable::new(&view.cookies_table).bordered(true))
    };

    let timing_rows = vec![
        TimingRow {
            phase: es_fluent::localize("request_tab_response_timing_total", None).to_string(),
            value: resp.total_ms
                .map(|ms| format!("{ms} ms"))
                .unwrap_or_else(|| "—".to_string()),
        },
        TimingRow {
            phase: es_fluent::localize("request_tab_response_timing_ttfb", None).to_string(),
            value: resp.ttfb_ms
                .map(|ms| format!("{ms} ms"))
                .unwrap_or_else(|| "—".to_string()),
        },
        TimingRow {
            phase: es_fluent::localize("request_tab_response_timing_dispatched_at", None).to_string(),
            value: format_unix_ms(resp.dispatched_at_unix_ms),
        },
        TimingRow {
            phase: es_fluent::localize("request_tab_response_timing_first_byte_at", None).to_string(),
            value: format_unix_ms(resp.first_byte_at_unix_ms),
        },
        TimingRow {
            phase: es_fluent::localize("request_tab_response_timing_completed_at", None).to_string(),
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
    view.timing_table.update(cx, |state, cx| {
        state.delegate_mut().set_rows(timing_rows);
        state.refresh(cx);
    });

    let timing_content = div().h(px(280.)).child(DataTable::new(&view.timing_table).bordered(true));

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

    // -- HTML Preview via embedded WebView ------------------------------------
    let is_preview_active = view.active_response_tab == ResponseTab::Preview;
    if is_preview_active && is_html {
        view.ensure_html_webview(window, cx);
    }
    if let Some(webview) = &view.html_webview {
        if is_preview_active && is_html && !html_body_for_preview.is_empty() {
            webview.update(cx, |w, _| {
                let _ = w.raw().load_html(&html_body_for_preview);
                w.show();
            });
        } else {
            webview.update(cx, |w, _| w.hide());
        }
    }
    let preview_content = if is_html && view.html_webview.is_some() {
        div()
            .h(px(400.))
            .child(view.html_webview.clone().unwrap())
    } else if is_html {
        div()
            .text_sm()
            .text_color(gpui::hsla(0., 0., 0.5, 1.))
            .child(es_fluent::localize("request_tab_response_preview_empty", None))
    } else {
        div()
            .text_sm()
            .text_color(gpui::hsla(0., 0., 0.5, 1.))
            .child(es_fluent::localize("request_tab_response_preview_not_html", None))
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
                .when(is_html, |el| {
                    el.child(response_tab_button(
                        "request-response-tab-preview",
                        es_fluent::localize("request_tab_response_tab_preview", None).to_string(),
                        view.active_response_tab == ResponseTab::Preview,
                        cx.listener(|this, _, _, cx| {
                            this.set_active_response_tab(ResponseTab::Preview, cx);
                        }),
                    ))
                })
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
