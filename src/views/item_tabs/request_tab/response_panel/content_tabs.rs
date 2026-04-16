use super::tables::TimingRow;
use super::*;

pub(super) fn refresh_response_tables_if_dirty(
    view: &mut RequestTabView,
    resp: &crate::domain::response::ResponseSummary,
    header_rows: &[crate::domain::response::ResponseHeaderRow],
    cookies: &[CookieRow],
    cx: &mut Context<RequestTabView>,
) {
    if !view.response_tables_dirty {
        return;
    }

    view.response_tables_dirty = false;
    let timing_rows = build_timing_rows(resp);
    view.headers_table.update(cx, |state, cx| {
        state.delegate_mut().set_rows(header_rows.to_vec());
        state.refresh(cx);
    });
    view.cookies_table.update(cx, |state, cx| {
        state.delegate_mut().set_rows(cookies.to_vec());
        state.refresh(cx);
    });
    view.timing_table.update(cx, |state, cx| {
        state.delegate_mut().set_rows(timing_rows);
        state.refresh(cx);
    });
}

fn build_timing_rows(resp: &crate::domain::response::ResponseSummary) -> Vec<TimingRow> {
    vec![
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
    ]
}

pub(super) fn render_body_content(
    view: &mut RequestTabView,
    resp: &crate::domain::response::ResponseSummary,
    body_preview: String,
    muted: Hsla,
    bg: Hsla,
    cx: &mut Context<RequestTabView>,
) -> gpui::Div {
    let body_search_query = view.body_search_input.read(cx).value().to_string();
    let body_matches = search_matches(&body_preview, &body_search_query);

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
            .child(body_content);
    }

    body_content
}

pub(super) fn render_headers_content(
    view: &RequestTabView,
    header_rows: &[crate::domain::response::ResponseHeaderRow],
    header_format: Option<HeaderJsonFormat>,
    muted: Hsla,
) -> gpui::Div {
    if header_rows.is_empty() {
        return div().text_sm().text_color(muted).child(es_fluent::localize(
            "request_tab_response_headers_empty",
            None,
        ));
    }

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
}

pub(super) fn render_cookies_content(
    view: &RequestTabView,
    cookies: &[CookieRow],
    muted: Hsla,
) -> gpui::Div {
    if cookies.is_empty() {
        return div().text_sm().text_color(muted).child(es_fluent::localize(
            "request_tab_response_cookies_empty",
            None,
        ));
    }

    div()
        .h(px(200.))
        .child(DataTable::new(&view.cookies_table).bordered(true))
}

pub(super) fn render_timing_content(view: &RequestTabView) -> gpui::Div {
    div()
        .h(px(280.))
        .child(DataTable::new(&view.timing_table).bordered(true))
}

pub(super) fn render_preview_content(
    view: &mut RequestTabView,
    is_html: bool,
    html_body_for_preview: &str,
    muted: Hsla,
    window: &mut Window,
    cx: &mut Context<RequestTabView>,
) -> gpui::Div {
    let is_preview_active = view.active_response_tab == ResponseTab::Preview;
    if is_preview_active && is_html && !html_body_for_preview.is_empty() {
        view.ensure_html_webview(window, cx);
        if let Some(webview) = &view.html_webview {
            webview.update(cx, |w, _| {
                let _ = w.raw().load_html(html_body_for_preview);
                w.show();
            });
        }
    } else {
        view.html_webview = None;
    }

    if is_html && is_preview_active && view.html_webview.is_some() {
        return div().h(px(400.)).child(view.html_webview.clone().unwrap());
    }
    if is_html && html_body_for_preview.is_empty() {
        return div().text_sm().text_color(muted).child(es_fluent::localize(
            "request_tab_response_preview_empty",
            None,
        ));
    }
    if !is_html {
        return div().text_sm().text_color(muted).child(es_fluent::localize(
            "request_tab_response_preview_not_html",
            None,
        ));
    }
    div()
}
