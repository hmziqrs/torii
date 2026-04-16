use super::*;

impl RequestTabView {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn register_input_subscriptions(
        window: &mut Window,
        cx: &mut Context<Self>,
        name_input: &Entity<InputState>,
        method_select: &Entity<SelectState<Vec<&'static str>>>,
        url_input: &Entity<InputState>,
        auth_type_select: &Entity<SelectState<Vec<&'static str>>>,
        auth_basic_username_input: &Entity<InputState>,
        auth_basic_password_ref_input: &Entity<InputState>,
        auth_bearer_token_ref_input: &Entity<InputState>,
        auth_api_key_name_input: &Entity<InputState>,
        auth_api_key_value_ref_input: &Entity<InputState>,
        auth_api_key_location_select: &Entity<SelectState<Vec<&'static str>>>,
        body_raw_text_input: &Entity<InputState>,
        body_raw_json_input: &Entity<InputState>,
        pre_request_input: &Entity<InputState>,
        tests_input: &Entity<InputState>,
        timeout_input: &Entity<InputState>,
        follow_redirects_input: &Entity<InputState>,
    ) -> Vec<Subscription> {
        let mut subscriptions = Vec::new();

        subscriptions.push(cx.subscribe(
            name_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let name = state.read(cx).value().to_string();
                    if this.editor.draft().name != name {
                        this.editor.draft_mut().name = name;
                        this.editor.refresh_save_status();
                        cx.notify();
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe_in(
            method_select,
            window,
            |this: &mut RequestTabView,
             _: &Entity<SelectState<Vec<&'static str>>>,
             event: &SelectEvent<Vec<&'static str>>,
             _window: &mut Window,
             cx| {
                let SelectEvent::Confirm(method) = event;
                let Some(method) = method.clone() else {
                    return;
                };
                if this.editor.draft().method != method {
                    this.editor.draft_mut().method = method.to_string();
                    this.editor.refresh_save_status();
                    cx.notify();
                }
            },
        ));

        subscriptions.push(cx.subscribe_in(
            url_input,
            window,
            |this: &mut RequestTabView,
             state: &Entity<InputState>,
             event: &InputEvent,
             window: &mut Window,
             cx: &mut Context<Self>| {
                if let InputEvent::Change = event {
                    if this.input_sync_guard.is_active() {
                        this.input_sync_guard.deferred = true;
                        return;
                    }
                    let url = state.read(cx).value().to_string();
                    if this.editor.draft().url != url {
                        this.editor.draft_mut().url = url;
                        let enabled_from_url =
                            params_from_url_query(this.editor.draft().url.as_str());
                        let mut merged = enabled_from_url;
                        merged.extend(
                            this.editor
                                .draft()
                                .params
                                .iter()
                                .filter(|entry| !entry.enabled)
                                .cloned(),
                        );
                        if this.editor.draft().params != merged {
                            this.editor.draft_mut().params = merged;
                        }
                        this.editor.refresh_save_status();
                        this.sync_kv_rows_with_draft(KvTarget::Params, window, cx);
                        cx.notify();
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe_in(
            auth_type_select,
            window,
            |this: &mut RequestTabView,
             _: &Entity<SelectState<Vec<&'static str>>>,
             event: &SelectEvent<Vec<&'static str>>,
             _window: &mut Window,
             cx| {
                let SelectEvent::Confirm(kind) = event;
                let Some(kind) = kind.clone() else {
                    return;
                };
                this.set_auth_kind(auth_kind_from_label(kind), cx);
            },
        ));

        subscriptions.push(cx.subscribe(
            auth_basic_username_input,
            |this: &mut RequestTabView, _: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.sync_auth_from_inputs(cx);
                }
            },
        ));
        subscriptions.push(cx.subscribe(
            auth_basic_password_ref_input,
            |this: &mut RequestTabView, _: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.sync_auth_from_inputs(cx);
                }
            },
        ));
        subscriptions.push(cx.subscribe(
            auth_bearer_token_ref_input,
            |this: &mut RequestTabView, _: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.sync_auth_from_inputs(cx);
                }
            },
        ));
        subscriptions.push(cx.subscribe(
            auth_api_key_name_input,
            |this: &mut RequestTabView, _: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.sync_auth_from_inputs(cx);
                }
            },
        ));
        subscriptions.push(cx.subscribe(
            auth_api_key_value_ref_input,
            |this: &mut RequestTabView, _: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.sync_auth_from_inputs(cx);
                }
            },
        ));
        subscriptions.push(cx.subscribe_in(
            auth_api_key_location_select,
            window,
            |this: &mut RequestTabView,
             _: &Entity<SelectState<Vec<&'static str>>>,
             _: &SelectEvent<Vec<&'static str>>,
             _window: &mut Window,
             cx| {
                this.sync_auth_from_inputs(cx);
            },
        ));

        subscriptions.push(cx.subscribe(
            body_raw_text_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let content = state.read(cx).value().to_string();
                    if let BodyType::RawText { content: existing } =
                        &mut this.editor.draft_mut().body
                    {
                        if *existing != content {
                            *existing = content;
                            this.editor.refresh_save_status();
                            cx.notify();
                        }
                    }
                }
            },
        ));
        subscriptions.push(cx.subscribe(
            body_raw_json_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let content = state.read(cx).value().to_string();
                    if let BodyType::RawJson { content: existing } =
                        &mut this.editor.draft_mut().body
                    {
                        if *existing != content {
                            *existing = content;
                            this.editor.refresh_save_status();
                            cx.notify();
                        }
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe(
            pre_request_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let text = state.read(cx).value().to_string();
                    if this.editor.draft().scripts.pre_request != text {
                        this.editor.draft_mut().scripts.pre_request = text;
                        this.editor.refresh_save_status();
                        cx.notify();
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe(
            tests_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let text = state.read(cx).value().to_string();
                    if this.editor.draft().scripts.tests != text {
                        this.editor.draft_mut().scripts.tests = text;
                        this.editor.refresh_save_status();
                        cx.notify();
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe(
            timeout_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let raw = state.read(cx).value().trim().to_string();
                    let parsed = if raw.is_empty() {
                        None
                    } else {
                        raw.parse::<u64>().ok()
                    };
                    if this.editor.draft().settings.timeout_ms != parsed {
                        this.editor.draft_mut().settings.timeout_ms = parsed;
                        this.editor.refresh_save_status();
                        cx.notify();
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe(
            follow_redirects_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let raw = state.read(cx).value().trim().to_ascii_lowercase();
                    let parsed = if raw.is_empty() {
                        None
                    } else if raw == "true" || raw == "1" || raw == "yes" {
                        Some(true)
                    } else if raw == "false" || raw == "0" || raw == "no" {
                        Some(false)
                    } else {
                        this.editor.refresh_save_status();
                        return;
                    };
                    if this.editor.draft().settings.follow_redirects != parsed {
                        this.editor.draft_mut().settings.follow_redirects = parsed;
                        this.editor.refresh_save_status();
                        cx.notify();
                    }
                }
            },
        ));

        subscriptions
    }
}
