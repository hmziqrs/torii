use super::*;

impl RequestTabView {
    pub(super) fn build_with_editor(
        mut editor: RequestEditorState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        if editor.draft().params.is_empty() {
            let from_url = params_from_url_query(editor.draft().url.as_str());
            if !from_url.is_empty() {
                editor.draft_mut().params = from_url;
                editor.refresh_save_status();
            }
        }
        let initial = editor.draft().clone();

        let name_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(initial.name.clone(), window, cx);
            state
        });
        let method_select = cx.new(|cx| {
            let mut select = SelectState::new(
                vec!["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"],
                Some(gpui_component::IndexPath::default()),
                window,
                cx,
            );
            if let Some(ix) = standard_method_index(initial.method.as_str()) {
                select.set_selected_index(
                    Some(gpui_component::IndexPath::default().row(ix)),
                    window,
                    cx,
                );
            } else {
                select.set_selected_index(None, window, cx);
            }
            select
        });
        let url_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(initial.url.clone(), window, cx);
            state
        });
        let auth_type_select = cx.new(|cx| {
            let mut select = SelectState::new(
                vec!["None", "Basic", "Bearer", "API Key"],
                Some(gpui_component::IndexPath::default()),
                window,
                cx,
            );
            select.set_selected_index(
                Some(gpui_component::IndexPath::default().row(auth_type_index(&initial.auth))),
                window,
                cx,
            );
            select
        });
        let auth_basic_username_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            if let AuthType::Basic { username, .. } = &initial.auth {
                state.set_value(username.clone(), window, cx);
            }
            state
        });
        let auth_basic_password_ref_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            if let AuthType::Basic {
                password_secret_ref,
                ..
            } = &initial.auth
            {
                state.set_value(password_secret_ref.clone().unwrap_or_default(), window, cx);
            }
            state
        });
        let auth_bearer_token_ref_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            if let AuthType::Bearer { token_secret_ref } = &initial.auth {
                state.set_value(token_secret_ref.clone().unwrap_or_default(), window, cx);
            }
            state
        });
        let auth_api_key_name_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            if let AuthType::ApiKey { key_name, .. } = &initial.auth {
                state.set_value(key_name.clone(), window, cx);
            }
            state
        });
        let auth_api_key_value_ref_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            if let AuthType::ApiKey {
                value_secret_ref, ..
            } = &initial.auth
            {
                state.set_value(value_secret_ref.clone().unwrap_or_default(), window, cx);
            }
            state
        });
        let auth_api_key_location_select = cx.new(|cx| {
            let mut select = SelectState::new(
                vec!["Header", "Query"],
                Some(gpui_component::IndexPath::default()),
                window,
                cx,
            );
            let row = match &initial.auth {
                AuthType::ApiKey { location, .. } => api_key_location_index(*location),
                _ => 0,
            };
            select.set_selected_index(
                Some(gpui_component::IndexPath::default().row(row)),
                window,
                cx,
            );
            select
        });
        let body_raw_text_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .rows(10)
                .searchable(true)
                .soft_wrap(true);
            if let BodyType::RawText { content } = &initial.body {
                state.set_value(content.clone(), window, cx);
            }
            state
        });
        let body_raw_json_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .code_editor("json")
                .line_number(true)
                .indent_guides(true)
                .tab_size(TabSize {
                    tab_size: 4,
                    hard_tabs: false,
                })
                .searchable(true)
                .soft_wrap(false);
            if let BodyType::RawJson { content } = &initial.body {
                state.set_value(content.clone(), window, cx);
            }
            state
        });
        let pre_request_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .code_editor("javascript")
                .multi_line(true)
                .rows(10)
                .line_number(true)
                .searchable(true)
                .soft_wrap(false);
            state.set_value(initial.scripts.pre_request.clone(), window, cx);
            state
        });
        let tests_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .code_editor("javascript")
                .multi_line(true)
                .rows(10)
                .line_number(true)
                .searchable(true)
                .soft_wrap(false);
            state.set_value(initial.scripts.tests.clone(), window, cx);
            state
        });
        let timeout_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            let value = initial
                .settings
                .timeout_ms
                .map(|v| v.to_string())
                .unwrap_or_default();
            state.set_value(value, window, cx);
            state
        });
        let follow_redirects_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            let value = initial
                .settings
                .follow_redirects
                .map(|v| if v { "true" } else { "false" }.to_string())
                .unwrap_or_default();
            state.set_value(value, window, cx);
            state
        });
        let body_search_input = cx.new(|cx| InputState::new(window, cx));
        let status_meta_focus = cx.focus_handle().tab_stop(true);
        let time_meta_focus = cx.focus_handle().tab_stop(true);
        let size_meta_focus = cx.focus_handle().tab_stop(true);
        let network_meta_focus = cx.focus_handle().tab_stop(true);

        let mut subscriptions = Self::register_input_subscriptions(
            window,
            cx,
            &name_input,
            &method_select,
            &url_input,
            &auth_type_select,
            &auth_basic_username_input,
            &auth_basic_password_ref_input,
            &auth_bearer_token_ref_input,
            &auth_api_key_name_input,
            &auth_api_key_value_ref_input,
            &auth_api_key_location_select,
            &body_raw_text_input,
            &body_raw_json_input,
            &pre_request_input,
            &tests_input,
            &timeout_input,
            &follow_redirects_input,
        );
        subscriptions.push(cx.on_focus(&status_meta_focus, window, |this, _, cx| {
            this.meta_hover_enter(ResponseMetaHover::Status, cx);
        }));
        subscriptions.push(cx.on_blur(&status_meta_focus, window, |this, _, cx| {
            this.meta_hover_leave(ResponseMetaHover::Status, cx);
        }));
        subscriptions.push(cx.on_focus(&time_meta_focus, window, |this, _, cx| {
            this.meta_hover_enter(ResponseMetaHover::Time, cx);
        }));
        subscriptions.push(cx.on_blur(&time_meta_focus, window, |this, _, cx| {
            this.meta_hover_leave(ResponseMetaHover::Time, cx);
        }));
        subscriptions.push(cx.on_focus(&size_meta_focus, window, |this, _, cx| {
            this.meta_hover_enter(ResponseMetaHover::Size, cx);
        }));
        subscriptions.push(cx.on_blur(&size_meta_focus, window, |this, _, cx| {
            this.meta_hover_leave(ResponseMetaHover::Size, cx);
        }));
        subscriptions.push(cx.on_focus(&network_meta_focus, window, |this, _, cx| {
            this.meta_hover_enter(ResponseMetaHover::Network, cx);
        }));
        subscriptions.push(cx.on_blur(&network_meta_focus, window, |this, _, cx| {
            this.meta_hover_leave(ResponseMetaHover::Network, cx);
        }));

        let this_entity = cx.entity();
        let mut this = Self {
            editor,
            focus_handle: cx.focus_handle(),
            name_input,
            method_select,
            url_input,
            auth_type_select,
            auth_basic_username_input,
            auth_basic_password_ref_input,
            auth_bearer_token_ref_input,
            auth_api_key_name_input,
            auth_api_key_value_ref_input,
            auth_api_key_location_select,
            body_raw_text_input,
            body_raw_json_input,
            pre_request_input,
            tests_input,
            timeout_input,
            follow_redirects_input,
            params_rows: Vec::new(),
            headers_rows: Vec::new(),
            body_urlencoded_rows: Vec::new(),
            body_form_text_rows: Vec::new(),
            next_kv_row_id: 1,
            active_section: RequestSectionTab::Params,
            active_response_tab: ResponseTab::Body,
            meta_hover: ResponseMetaHover::None,
            meta_hover_close_task: None,
            status_meta_focus,
            time_meta_focus,
            size_meta_focus,
            network_meta_focus,
            loaded_full_body_blob_id: None,
            loaded_full_body_text: None,
            input_sync_guard: ReentrancyGuard::default(),
            body_search_visible: false,
            body_search_input,
            error_detail_expanded: false,
            headers_table: cx.new(|cx| {
                TableState::new(response_panel::HeadersTableDelegate::new(), window, cx)
                    .row_selectable(false)
                    .col_selectable(false)
                    .col_resizable(true)
                    .col_movable(false)
                    .sortable(false)
            }),
            cookies_table: cx.new(|cx| {
                TableState::new(response_panel::CookiesTableDelegate::new(), window, cx)
                    .row_selectable(false)
                    .col_selectable(false)
                    .col_resizable(true)
                    .col_movable(false)
                    .sortable(false)
            }),
            timing_table: cx.new(|cx| {
                TableState::new(response_panel::TimingTableDelegate::new(), window, cx)
                    .row_selectable(false)
                    .col_selectable(false)
                    .col_resizable(false)
                    .col_movable(false)
                    .sortable(false)
            }),
            params_kv_table: cx.new(|cx| {
                TableState::new(
                    kv_editor::KvTableDelegate::new(
                        this_entity.clone(),
                        KvTarget::Params,
                        "params",
                    ),
                    window,
                    cx,
                )
                .row_selectable(false)
                .col_selectable(false)
                .col_resizable(true)
                .col_movable(false)
                .sortable(false)
            }),
            headers_kv_table: cx.new(|cx| {
                TableState::new(
                    kv_editor::KvTableDelegate::new(
                        this_entity.clone(),
                        KvTarget::Headers,
                        "headers",
                    ),
                    window,
                    cx,
                )
                .row_selectable(false)
                .col_selectable(false)
                .col_resizable(true)
                .col_movable(false)
                .sortable(false)
            }),
            body_urlencoded_kv_table: cx.new(|cx| {
                TableState::new(
                    kv_editor::KvTableDelegate::new(
                        this_entity.clone(),
                        KvTarget::BodyUrlEncoded,
                        "body-urlencoded",
                    ),
                    window,
                    cx,
                )
                .row_selectable(false)
                .col_selectable(false)
                .col_resizable(true)
                .col_movable(false)
                .sortable(false)
            }),
            body_form_text_kv_table: cx.new(|cx| {
                TableState::new(
                    kv_editor::KvTableDelegate::new(
                        this_entity.clone(),
                        KvTarget::BodyFormDataText,
                        "body-form-text",
                    ),
                    window,
                    cx,
                )
                .row_selectable(false)
                .col_selectable(false)
                .col_resizable(true)
                .col_movable(false)
                .sortable(false)
            }),
            html_webview: None,
            last_preview_html: None,
            _subscriptions: subscriptions,
            kv_subscriptions: HashMap::new(),
            draft_dirty: true,
            response_tables_dirty: false,
            params_kv_dirty: true,
            headers_kv_dirty: true,
            body_urlencoded_kv_dirty: true,
            body_form_text_kv_dirty: true,
        };
        this.rebuild_kv_rows(KvTarget::Params, &initial.params, window, cx);
        this.rebuild_kv_rows(KvTarget::Headers, &initial.headers, window, cx);
        let urlencoded_entries = match &initial.body {
            BodyType::UrlEncoded { entries } => entries.clone(),
            _ => Vec::new(),
        };
        this.rebuild_kv_rows(KvTarget::BodyUrlEncoded, &urlencoded_entries, window, cx);
        let form_text_entries = match &initial.body {
            BodyType::FormData { text_fields, .. } => text_fields.clone(),
            _ => Vec::new(),
        };
        this.rebuild_kv_rows(KvTarget::BodyFormDataText, &form_text_entries, window, cx);
        this
    }
}
