use super::*;

pub(super) fn render_request_tab(
    view: &mut RequestTabView,
    window: &mut Window,
    cx: &mut Context<RequestTabView>,
) -> gpui::Div {
    if view.draft_dirty {
        view.sync_inputs_from_draft(window, cx);
        view.draft_dirty = false;
    }
    let draft = view.editor.draft().clone();
    let save_status = view.editor.save_status().clone();
    let is_dirty = matches!(
        save_status,
        SaveStatus::Dirty | SaveStatus::SaveFailed { .. } | SaveStatus::Saving
    );

    let dirty_indicator = if is_dirty {
        div()
            .text_xs()
            .text_color(gpui::red())
            .child(es_fluent::localize("request_tab_dirty", None))
    } else {
        div()
    };

    let response_panel = response_panel::render_response_panel(view, window, cx);

    let preflight_panel = match view.editor.preflight_error() {
        Some(err) => div().text_sm().text_color(gpui::red()).child(format!(
            "{}: {}",
            es_fluent::localize("request_tab_preflight", None),
            err.message
        )),
        None => div(),
    };

    let latest_run = latest_run_summary(view.editor.exec_status());

    let section_content = match view.active_section {
        RequestSectionTab::Params => {
            let dirty = std::mem::take(&mut view.params_kv_dirty);
            kv_editor::render_kv_table(
                &view.params_kv_table,
                KvTarget::Params,
                "params",
                &view.params_rows,
                dirty,
                cx,
            )
            .into_any_element()
        }
        RequestSectionTab::Auth => {
            auth_editor::render_auth_editor(view, &draft, cx).into_any_element()
        }
        RequestSectionTab::Headers => {
            let dirty = std::mem::take(&mut view.headers_kv_dirty);
            kv_editor::render_kv_table(
                &view.headers_kv_table,
                KvTarget::Headers,
                "headers",
                &view.headers_rows,
                dirty,
                cx,
            )
            .into_any_element()
        }
        RequestSectionTab::Body => {
            body_editor::render_body_editor(view, &draft, window, cx).into_any_element()
        }
        RequestSectionTab::Scripts => v_flex()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(es_fluent::localize("request_tab_pre_request_label", None)),
            )
            .child(
                div()
                    .w_full()
                    .child(Input::new(&view.pre_request_input).h(px(240.))),
            )
            .into_any_element(),
        RequestSectionTab::Tests => v_flex()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(es_fluent::localize("request_tab_tests_label", None)),
            )
            .child(
                div()
                    .w_full()
                    .child(Input::new(&view.tests_input).h(px(240.))),
            )
            .into_any_element(),
    };

    v_flex()
        .size_full()
        .p_4()
        .gap_3()
        .track_focus(&view.focus_handle(cx))
        .on_action(cx.listener(RequestTabView::handle_save_request))
        .on_action(cx.listener(RequestTabView::handle_send_request))
        .on_action(cx.listener(RequestTabView::handle_cancel_request))
        .on_action(cx.listener(RequestTabView::handle_duplicate_request))
        .on_action(cx.listener(RequestTabView::handle_focus_url_bar))
        .on_action(cx.listener(RequestTabView::handle_toggle_body_search))
        .child({
            let url_focused = view
                .url_input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window);

            h_flex()
                .gap_2()
                .items_center()
                .child(
                    h_flex()
                        .items_center()
                        .flex_1()
                        .border_1()
                        .border_color(cx.theme().input)
                        .bg(cx.theme().input_background())
                        .rounded(cx.theme().radius)
                        .when(cx.theme().shadow, |el| el.shadow_xs())
                        .child(
                            div()
                                .w(px(120.))
                                .overflow_hidden()
                                .rounded_tl(cx.theme().radius)
                                .rounded_bl(cx.theme().radius)
                                .child(
                                    Select::new(&view.method_select)
                                        .large()
                                        .appearance(false),
                                ),
                        )
                        .child(Divider::vertical().color(cx.theme().border))
                        .child(
                            div()
                                .flex_1()
                                .overflow_hidden()
                                .rounded_tr(cx.theme().radius)
                                .rounded_br(cx.theme().radius)
                                .when(url_focused, |el| {
                                    el.border_1().border_color(cx.theme().ring)
                                })
                                .child(
                                    Input::new(&view.url_input)
                                        .large()
                                        .appearance(false),
                                ),
                        ),
                )
                .child(
                    Button::new("request-send")
                        .primary()
                        .large()
                        .h(px(44.))
                        .label(es_fluent::localize("request_tab_action_send", None))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.send(cx);
                        })),
                )
        })
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .flex_wrap()
                .when(is_dirty, |el| el.child(dirty_indicator))
                .child(
                    Button::new("request-save")
                        .ghost()
                        .label(es_fluent::localize("request_tab_action_save", None))
                        .on_click(cx.listener(|this, _, window, cx| match this.save(cx) {
                            Ok(()) => {
                                window.push_notification(
                                    es_fluent::localize("request_tab_save_ok", None),
                                    cx,
                                );
                            }
                            Err(err) => window.push_notification(err, cx),
                        })),
                )
                .child(
                    Button::new("request-duplicate")
                        .ghost()
                        .label(es_fluent::localize("request_tab_action_duplicate", None))
                        .on_click(cx.listener(|this, _, window, cx| match this.duplicate(cx) {
                            Ok(_) => {
                                window.push_notification(
                                    es_fluent::localize("request_tab_duplicate_ok", None),
                                    cx,
                                );
                            }
                            Err(err) => window.push_notification(err, cx),
                        })),
                )
                .when(
                    matches!(
                        view.editor.exec_status(),
                        ExecStatus::Sending | ExecStatus::Streaming
                    ),
                    |el| {
                        el.child(
                            Button::new("request-cancel")
                                .ghost()
                                .label(es_fluent::localize("request_tab_action_cancel", None))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.cancel_send(cx);
                                })),
                        )
                    },
                )
                .when(view.editor.baseline().is_some(), |el| {
                    el.child(
                        Button::new("request-reload")
                            .ghost()
                            .label(es_fluent::localize("request_tab_action_reload", None))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reload_baseline(cx);
                            })),
                    )
                })
                .child(div().flex_1())
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!(
                            "{}: {}",
                            es_fluent::localize("request_tab_latest_run_label", None),
                            latest_run
                        )),
                )
                .child(
                    Button::new("request-settings-open")
                        .ghost()
                        .label(es_fluent::localize("request_tab_settings_label", None))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_settings_dialog(window, cx);
                        })),
                ),
        )
        .child(
            h_flex()
                .gap_1()
                .flex_wrap()
                .child(section_tab_button(
                    "request-tab-params",
                    es_fluent::localize("request_tab_params_label", None).to_string(),
                    view.active_section == RequestSectionTab::Params,
                    cx,
                    cx.listener(|this, _, _, cx| {
                        this.set_active_section(RequestSectionTab::Params, cx);
                    }),
                ))
                .child(section_tab_button(
                    "request-tab-auth",
                    es_fluent::localize("request_tab_auth_label", None).to_string(),
                    view.active_section == RequestSectionTab::Auth,
                    cx,
                    cx.listener(|this, _, _, cx| {
                        this.set_active_section(RequestSectionTab::Auth, cx);
                    }),
                ))
                .child(section_tab_button(
                    "request-tab-headers",
                    es_fluent::localize("request_tab_headers_label", None).to_string(),
                    view.active_section == RequestSectionTab::Headers,
                    cx,
                    cx.listener(|this, _, _, cx| {
                        this.set_active_section(RequestSectionTab::Headers, cx);
                    }),
                ))
                .child(section_tab_button(
                    "request-tab-body",
                    es_fluent::localize("request_tab_body_label", None).to_string(),
                    view.active_section == RequestSectionTab::Body,
                    cx,
                    cx.listener(|this, _, _, cx| {
                        this.set_active_section(RequestSectionTab::Body, cx);
                    }),
                ))
                .child(section_tab_button(
                    "request-tab-scripts",
                    es_fluent::localize("request_tab_scripts_label", None).to_string(),
                    view.active_section == RequestSectionTab::Scripts,
                    cx,
                    cx.listener(|this, _, _, cx| {
                        this.set_active_section(RequestSectionTab::Scripts, cx);
                    }),
                ))
                .child(section_tab_button(
                    "request-tab-tests",
                    es_fluent::localize("request_tab_tests_label", None).to_string(),
                    view.active_section == RequestSectionTab::Tests,
                    cx,
                    cx.listener(|this, _, _, cx| {
                        this.set_active_section(RequestSectionTab::Tests, cx);
                    }),
                )),
        )
        .child(
            v_flex()
                .w_full()
                .items_stretch()
                .pt_2()
                .child(div().w_full().child(section_content)),
        )
        .when(
            matches!(save_status, SaveStatus::SaveFailed { .. }),
            |el: gpui::Div| {
                if let SaveStatus::SaveFailed { error } = &save_status {
                    el.child(div().text_sm().text_color(gpui::red()).child(error.clone()))
                } else {
                    el
                }
            },
        )
        .child(preflight_panel)
        .child(
            v_flex()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(es_fluent::localize("request_tab_response_label", None)),
                )
                .child(response_panel),
        )
}
