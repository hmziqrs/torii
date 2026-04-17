use super::*;

pub(super) fn render_completed_response_body(
    view: &mut RequestTabView,
    resp: &crate::domain::response::ResponseSummary,
    window: &mut Window,
    cx: &mut Context<RequestTabView>,
) -> gpui::Div {
    let muted = cx.theme().muted_foreground;
    let bg = cx.theme().background;

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
                None
            } else {
                Some(
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
        _ => None,
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

    v_flex()
        .flex_1()
        .min_h_0()
        .gap_2()
        // Tab strip — never shrinks
        .child(chrome::render_tab_strip(view, is_html, cx).flex_shrink_0())
        // Body action buttons — only when on the Body tab
        .when(view.active_response_tab == ResponseTab::Body, |el| {
            el.child(body_actions.flex_shrink_0())
        })
        // Scrollable content area
        .child(
            div()
                .id("response-body-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(active_content),
        )
        // Load full body button — only for large disk blobs
        .when_some(load_full_button, |el, btn| {
            el.child(div().flex_shrink_0().child(btn))
        })
}
