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

    let is_inflight = matches!(
        view.editor.exec_status(),
        ExecStatus::Sending | ExecStatus::Streaming
    );

    let response_panel = response_panel::render_response_panel(view, window, cx);

    let preflight_notice = view
        .editor
        .preflight_error()
        .map(|err| render_preflight_notice(&err.message, cx));

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
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .gap_3()
        .key_context("RequestTabView")
        .track_focus(&view.focus_handle(cx))
        .on_action(cx.listener(RequestTabView::handle_save_request))
        .on_action(cx.listener(RequestTabView::handle_send_request))
        .on_action(cx.listener(RequestTabView::handle_cancel_request))
        .on_action(cx.listener(RequestTabView::handle_duplicate_request))
        .on_action(cx.listener(RequestTabView::handle_focus_url_bar))
        .on_action(cx.listener(RequestTabView::handle_toggle_body_search))
        // URL bar — never shrinks
        .child({
            let url_focused = view.url_input.read(cx).focus_handle(cx).is_focused(window);

            h_flex()
                .gap_2()
                .h(px(52.))
                .items_center()
                .flex_shrink_0()
                .px_4()
                .pt_4()
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
                                .child(Select::new(&view.method_select).large().appearance(false)),
                        )
                        .child(Divider::vertical().color(cx.theme().border))
                        .child(
                            div()
                                .flex_1()
                                .overflow_hidden()
                                .rounded_tr(cx.theme().radius)
                                .rounded_br(cx.theme().radius)
                                .border_1()
                                .border_color(if url_focused {
                                    cx.theme().ring
                                } else {
                                    cx.theme().transparent
                                })
                                .child(Input::new(&view.url_input).large().appearance(false)),
                        ),
                )
                .when(!is_inflight, |el| {
                    el.child(
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
                .when(is_inflight, |el| {
                    el.child(
                        Button::new("request-cancel")
                            .outline()
                            .large()
                            .h(px(44.))
                            .label(es_fluent::localize("request_tab_action_cancel", None))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_send(cx);
                            })),
                    )
                })
        })
        // Sticky preflight notice: rendered inline directly below URL bar.
        .when_some(preflight_notice, |el, notice| el.child(notice))
        // Section tabs — never shrinks, no wrapping
        .child(
            h_flex()
                .gap_1()
                .flex_shrink_0()
                .px_4()
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
                ))
                .child(div().flex_1())
                .child(
                    div()
                        .text_xs()
                        .flex_shrink_0()
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
                        .flex_shrink_0()
                        .label(es_fluent::localize("request_tab_settings_label", None))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_settings_dialog(window, cx);
                        })),
                ),
        )
        // Resizable split: request section (top) / response section (bottom).
        //
        // Layout rule: NEVER use size_full() / height: 100% inside resizable panels.
        // Taffy resolves `height: 100%` against the panel's explicit `height` property
        // (which is itself `100%` of the whole group), not against the flex-basis-allocated
        // portion. This causes scroll containers to think they are taller than their panel,
        // so content fits without scrolling. The correct pattern is:
        //   v_flex().flex_1().min_h_0()  — fills panel via flex + disables min-content floor
        //   div().flex_1().min_h_0()     — scroll container uses its flex-allocated height
        .child(
            div().flex_1().min_h_0().overflow_hidden().child(
                v_resizable("request-tab-body-split")
                    // ── Top: request section content ──────────────────────
                    .child(
                        resizable_panel().size_range(px(80.)..px(99999.)).child(
                            // flex_1: grows horizontally inside the row-direction panel.
                            // min_h_0: allows the panel to shrink the wrapper vertically
                            //   (cross-axis stretch gives height; min_h_0 removes the
                            //    content-height floor that would block shrinking).
                            v_flex()
                                .flex_1()
                                .min_h_0()
                                .overflow_hidden()
                                // Save-failed banner: pinned above scroll, always visible.
                                .when(matches!(save_status, SaveStatus::SaveFailed { .. }), |el| {
                                    if let SaveStatus::SaveFailed { error } = &save_status {
                                        el.child(
                                            h_flex()
                                                .flex_shrink_0()
                                                .gap_2()
                                                .items_center()
                                                .px_4()
                                                .py_1()
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(gpui::red())
                                                        .child(error.clone()),
                                                )
                                                .child(
                                                    Button::new("request-reload")
                                                        .ghost()
                                                        .label(es_fluent::localize(
                                                            "request_tab_action_reload",
                                                            None,
                                                        ))
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.reload_baseline(cx);
                                                        })),
                                                ),
                                        )
                                    } else {
                                        el
                                    }
                                })
                                // Scrollable section content.
                                // flex_1 + min_h_0: gets the correct flex-allocated height
                                // so overflow_y_scroll clips at exactly the panel boundary.
                                .child(
                                    div()
                                        .id("request-tab-request-scroll")
                                        .flex_1()
                                        .min_h_0()
                                        .overflow_y_scroll()
                                        .px_4()
                                        .pb_4()
                                        .gap_3()
                                        .child(section_content),
                                ),
                        ),
                    )
                    // ── Bottom: response section (always visible) ─────────
                    .child(
                        resizable_panel()
                            .size(px(260.))
                            .size_range(px(120.)..px(99999.))
                            .child(v_flex().flex_1().min_h_0().px_4().child(response_panel)),
                    ),
            ),
        )
}

fn render_preflight_notice(message: &str, cx: &App) -> gpui::Div {
    let (missing_vars, scopes) = parse_preflight_message(message);

    v_flex()
        .mx_4()
        .px_3()
        .py_2()
        .gap_1()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().danger.opacity(0.6))
        .bg(cx.theme().danger.opacity(0.08))
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(cx.theme().danger)
                .child(es_fluent::localize("request_tab_preflight", None)),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().foreground)
                .child(match missing_vars {
                    Some(vars) => format!(
                        "{}: {vars}",
                        es_fluent::localize("request_tab_preflight_missing_vars", None)
                    ),
                    None => message.to_string(),
                }),
        )
        .when_some(scopes, |el, checked_scopes| {
            el.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "{}: {checked_scopes}",
                        es_fluent::localize("request_tab_preflight_checked_scopes", None)
                    )),
            )
        })
}

fn parse_preflight_message(message: &str) -> (Option<String>, Option<String>) {
    let missing_prefix = "missing variables:";
    let scopes_prefix = "; checked scopes:";
    if let Some(start) = message.find(missing_prefix) {
        let tail = message[start + missing_prefix.len()..].trim();
        if let Some(split) = tail.find(scopes_prefix) {
            let vars = tail[..split].trim();
            let scopes = tail[split + scopes_prefix.len()..].trim();
            return (Some(vars.to_string()), Some(scopes.to_string()));
        }
        return (Some(tail.to_string()), None);
    }
    (None, None)
}
