use super::*;

impl RequestTabView {
    pub(super) fn selected_auth_kind(&self, cx: &App) -> AuthKind {
        self.auth_type_select
            .read(cx)
            .selected_value()
            .map(|label| auth_kind_from_label(label))
            .unwrap_or(AuthKind::None)
    }

    pub(super) fn read_secret_value(&self, secret_ref: &Option<String>, cx: &App) -> String {
        let Some(secret_ref) = secret_ref else {
            return String::new();
        };
        cx.global::<AppServicesGlobal>()
            .0
            .secret_store
            .get_secret(secret_ref)
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    pub(super) fn upsert_auth_secret_value(
        &mut self,
        secret_kind: &str,
        value: String,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let services = cx.global::<AppServicesGlobal>().0.clone();
        let (owner_kind, owner_id) = match self.editor.identity() {
            EditorIdentity::Draft(id) => ("request_draft", id.to_string()),
            EditorIdentity::Persisted(id) => ("request", id.to_string()),
        };

        if value.is_empty() {
            let _ = services
                .secret_manager
                .delete_secret(owner_kind, &owner_id, secret_kind);
            return None;
        }

        match services
            .secret_manager
            .upsert_secret(owner_kind, &owner_id, secret_kind, &value)
        {
            Ok(secret_ref) => Some(secret_ref.key_name),
            Err(err) => {
                self.editor
                    .set_preflight_error(format!("failed to store auth secret: {err}"));
                None
            }
        }
    }

    pub(super) fn auth_from_cached_inputs(
        &mut self,
        kind: AuthKind,
        cx: &mut Context<Self>,
    ) -> AuthType {
        match kind {
            AuthKind::None => AuthType::None,
            AuthKind::Basic => {
                let username = self.auth_basic_username_input.read(cx).value().to_string();
                let password_value = self
                    .auth_basic_password_ref_input
                    .read(cx)
                    .value()
                    .to_string();
                AuthType::Basic {
                    username,
                    password_secret_ref: self.upsert_auth_secret_value(
                        "basic_password",
                        password_value,
                        cx,
                    ),
                }
            }
            AuthKind::Bearer => {
                let token_value = self
                    .auth_bearer_token_ref_input
                    .read(cx)
                    .value()
                    .to_string();
                AuthType::Bearer {
                    token_secret_ref: self.upsert_auth_secret_value(
                        "bearer_token",
                        token_value,
                        cx,
                    ),
                }
            }
            AuthKind::ApiKey => {
                let key_name = self.auth_api_key_name_input.read(cx).value().to_string();
                let value_raw = self
                    .auth_api_key_value_ref_input
                    .read(cx)
                    .value()
                    .to_string();
                let location_ix = self
                    .auth_api_key_location_select
                    .read(cx)
                    .selected_index(cx)
                    .map(|ix| ix.row)
                    .unwrap_or(0);
                AuthType::ApiKey {
                    key_name,
                    value_secret_ref: self.upsert_auth_secret_value("api_key_value", value_raw, cx),
                    location: api_key_location_from_index(location_ix),
                }
            }
        }
    }

    pub(super) fn set_auth_kind(&mut self, kind: AuthKind, cx: &mut Context<Self>) {
        let next = self.auth_from_cached_inputs(kind, cx);
        if self.editor.draft().auth != next {
            self.editor.draft_mut().auth = next;
            self.editor.refresh_save_status();
            cx.notify();
        }
    }

    pub(super) fn sync_auth_from_inputs(&mut self, cx: &mut Context<Self>) {
        self.set_auth_kind(self.selected_auth_kind(cx), cx);
    }

    pub(super) fn selected_body_kind(&self, _cx: &App) -> BodyKind {
        match &self.editor.draft().body {
            BodyType::None => BodyKind::None,
            BodyType::RawText { .. } => BodyKind::RawText,
            BodyType::RawJson { .. } => BodyKind::RawJson,
            BodyType::UrlEncoded { .. } => BodyKind::UrlEncoded,
            BodyType::FormData { .. } => BodyKind::FormData,
            BodyType::BinaryFile { .. } => BodyKind::BinaryFile,
        }
    }

    pub(super) fn set_body_kind(&mut self, kind: BodyKind, cx: &mut Context<Self>) {
        let next = match kind {
            BodyKind::None => BodyType::None,
            BodyKind::RawText => BodyType::RawText {
                content: self.body_raw_text_input.read(cx).value().to_string(),
            },
            BodyKind::RawJson => BodyType::RawJson {
                content: self.body_raw_json_input.read(cx).value().to_string(),
            },
            BodyKind::UrlEncoded => BodyType::UrlEncoded {
                entries: self.collect_meaningful_pairs(KvTarget::BodyUrlEncoded, cx),
            },
            BodyKind::FormData => {
                let file_fields = match &self.editor.draft().body {
                    BodyType::FormData { file_fields, .. } => file_fields.clone(),
                    _ => Vec::new(),
                };
                BodyType::FormData {
                    text_fields: self.collect_meaningful_pairs(KvTarget::BodyFormDataText, cx),
                    file_fields,
                }
            }
            BodyKind::BinaryFile => match &self.editor.draft().body {
                BodyType::BinaryFile {
                    blob_hash,
                    file_name,
                } => BodyType::BinaryFile {
                    blob_hash: blob_hash.clone(),
                    file_name: file_name.clone(),
                },
                _ => BodyType::BinaryFile {
                    blob_hash: String::new(),
                    file_name: None,
                },
            },
        };
        if self.editor.draft().body != next {
            self.editor.draft_mut().body = next;
            self.editor.refresh_save_status();
            cx.notify();
        }
    }

    pub(super) fn sync_inputs_from_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.input_sync_guard.enter() {
            return;
        }

        let draft = self.editor.draft().clone();
        if self.url_input.read(cx).value().as_ref() != draft.url.as_str() {
            self.url_input.update(cx, |s, cx| {
                s.set_value(draft.url.clone(), window, cx);
            });
        }
        self.sync_kv_rows_with_draft(KvTarget::Params, window, cx);
        self.sync_kv_rows_with_draft(KvTarget::Headers, window, cx);
        self.sync_kv_rows_with_draft(KvTarget::BodyUrlEncoded, window, cx);
        self.sync_kv_rows_with_draft(KvTarget::BodyFormDataText, window, cx);
        self.method_select.update(cx, |select, cx| {
            if let Some(ix) = standard_method_index(draft.method.as_str()) {
                if select.selected_index(cx).map(|it| it.row) != Some(ix) {
                    select.set_selected_index(
                        Some(gpui_component::IndexPath::default().row(ix)),
                        window,
                        cx,
                    );
                }
            } else if select.selected_value().is_some() {
                select.set_selected_index(None, window, cx);
            }
        });
        self.auth_type_select.update(cx, |select, cx| {
            let ix = auth_type_index(&draft.auth);
            if select.selected_index(cx).map(|it| it.row) != Some(ix) {
                select.set_selected_index(
                    Some(gpui_component::IndexPath::default().row(ix)),
                    window,
                    cx,
                );
            }
        });

        match &draft.auth {
            AuthType::Basic {
                username,
                password_secret_ref,
            } => {
                if self.auth_basic_username_input.read(cx).value().as_ref() != username.as_str() {
                    self.auth_basic_username_input.update(cx, |s, cx| {
                        s.set_value(username.clone(), window, cx);
                    });
                }
                let password_value = self.read_secret_value(password_secret_ref, cx);
                if self.auth_basic_password_ref_input.read(cx).value().as_ref()
                    != password_value.as_str()
                {
                    self.auth_basic_password_ref_input.update(cx, |s, cx| {
                        s.set_value(password_value.clone(), window, cx);
                    });
                }
            }
            AuthType::Bearer { token_secret_ref } => {
                let token_value = self.read_secret_value(token_secret_ref, cx);
                if self.auth_bearer_token_ref_input.read(cx).value().as_ref()
                    != token_value.as_str()
                {
                    self.auth_bearer_token_ref_input.update(cx, |s, cx| {
                        s.set_value(token_value.clone(), window, cx);
                    });
                }
            }
            AuthType::ApiKey {
                key_name,
                value_secret_ref,
                location,
            } => {
                if self.auth_api_key_name_input.read(cx).value().as_ref() != key_name.as_str() {
                    self.auth_api_key_name_input.update(cx, |s, cx| {
                        s.set_value(key_name.clone(), window, cx);
                    });
                }
                let value_raw = self.read_secret_value(value_secret_ref, cx);
                if self.auth_api_key_value_ref_input.read(cx).value().as_ref() != value_raw.as_str()
                {
                    self.auth_api_key_value_ref_input.update(cx, |s, cx| {
                        s.set_value(value_raw.clone(), window, cx);
                    });
                }
                self.auth_api_key_location_select.update(cx, |select, cx| {
                    let row = api_key_location_index(*location);
                    if select.selected_index(cx).map(|it| it.row) != Some(row) {
                        select.set_selected_index(
                            Some(gpui_component::IndexPath::default().row(row)),
                            window,
                            cx,
                        );
                    }
                });
            }
            AuthType::None => {}
        }

        match &draft.body {
            BodyType::RawText { content } => {
                if self.body_raw_text_input.read(cx).value().as_ref() != content.as_str() {
                    self.body_raw_text_input.update(cx, |s, cx| {
                        s.set_value(content.clone(), window, cx);
                    });
                }
            }
            BodyType::RawJson { content } => {
                if self.body_raw_json_input.read(cx).value().as_ref() != content.as_str() {
                    self.body_raw_json_input.update(cx, |s, cx| {
                        s.set_value(content.clone(), window, cx);
                    });
                }
            }
            _ => {}
        }

        if self.input_sync_guard.leave_and_take_deferred() {
            cx.notify();
        }
    }

    pub(super) fn ensure_html_webview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.html_webview.is_some() {
            return;
        }
        use raw_window_handle::HasWindowHandle;
        let Ok(window_handle) = window.window_handle() else {
            return;
        };
        let Some(wry_webview) = lb_wry::WebViewBuilder::new()
            .build_as_child(&window_handle)
            .ok()
        else {
            return;
        };
        self.html_webview = Some(cx.new(|cx| WebView::new(wry_webview, window, cx)));
    }

    pub fn release_html_webview(&mut self, cx: &mut Context<Self>) {
        if let Some(webview) = self.html_webview.take() {
            webview.update(cx, |w, _| {
                w.hide();
            });
        }
    }

    pub(super) fn current_preview_bytes(&self) -> usize {
        match self.editor.exec_status() {
            ExecStatus::Completed { response } => match &response.body_ref {
                BodyRef::InMemoryPreview { bytes, .. } => bytes.len(),
                BodyRef::DiskBlob {
                    preview: Some(bytes),
                    ..
                } => bytes.len(),
                _ => 0,
            },
            _ => 0,
        }
    }
}
