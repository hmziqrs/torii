use super::*;

impl RequestTabView {
    pub(super) fn kv_rows(&self, target: KvTarget) -> &Vec<KeyValueEditorRow> {
        match target {
            KvTarget::Params => &self.params_rows,
            KvTarget::Headers => &self.headers_rows,
            KvTarget::BodyUrlEncoded => &self.body_urlencoded_rows,
            KvTarget::BodyFormDataText => &self.body_form_text_rows,
        }
    }

    pub(super) fn kv_rows_mut(&mut self, target: KvTarget) -> &mut Vec<KeyValueEditorRow> {
        match target {
            KvTarget::Params => &mut self.params_rows,
            KvTarget::Headers => &mut self.headers_rows,
            KvTarget::BodyUrlEncoded => &mut self.body_urlencoded_rows,
            KvTarget::BodyFormDataText => &mut self.body_form_text_rows,
        }
    }

    pub(super) fn next_kv_row_id(&mut self) -> u64 {
        let id = self.next_kv_row_id;
        self.next_kv_row_id += 1;
        id
    }

    pub(super) fn make_kv_row(
        &mut self,
        target: KvTarget,
        entry: KeyValuePair,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> KeyValueEditorRow {
        let id = self.next_kv_row_id();
        let key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(entry.key.clone(), window, cx);
            state
        });
        let value_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(entry.value.clone(), window, cx);
            state
        });

        self._subscriptions.push(cx.subscribe(
            &key_input,
            move |this: &mut RequestTabView, _: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.on_kv_rows_changed(target, cx);
                }
            },
        ));
        self._subscriptions.push(cx.subscribe(
            &value_input,
            move |this: &mut RequestTabView, _: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.on_kv_rows_changed(target, cx);
                }
            },
        ));

        KeyValueEditorRow {
            id,
            enabled: entry.enabled,
            key_input,
            value_input,
        }
    }

    pub(super) fn rebuild_kv_rows(
        &mut self,
        target: KvTarget,
        entries: &[KeyValuePair],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let normalized = if entries.is_empty() {
            vec![KeyValuePair {
                key: String::new(),
                value: String::new(),
                enabled: true,
            }]
        } else {
            entries.to_vec()
        };
        let mut rows = Vec::with_capacity(normalized.len());
        for entry in normalized {
            rows.push(self.make_kv_row(target, entry, window, cx));
        }
        *self.kv_rows_mut(target) = rows;
        self.ensure_trailing_empty_row(target, window, cx);
    }

    pub(super) fn sync_kv_rows_with_draft(
        &mut self,
        target: KvTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if target == KvTarget::BodyUrlEncoded
            && !matches!(self.editor.draft().body, BodyType::UrlEncoded { .. })
        {
            return;
        }
        if target == KvTarget::BodyFormDataText
            && !matches!(self.editor.draft().body, BodyType::FormData { .. })
        {
            return;
        }
        let draft_entries = match target {
            KvTarget::Params => self.editor.draft().params.clone(),
            KvTarget::Headers => self.editor.draft().headers.clone(),
            KvTarget::BodyUrlEncoded => match &self.editor.draft().body {
                BodyType::UrlEncoded { entries } => entries.clone(),
                _ => Vec::new(),
            },
            KvTarget::BodyFormDataText => match &self.editor.draft().body {
                BodyType::FormData { text_fields, .. } => text_fields.clone(),
                _ => Vec::new(),
            },
        };
        let current = self.collect_meaningful_pairs(target, cx);
        if current != draft_entries {
            self.rebuild_kv_rows(target, &draft_entries, window, cx);
        }
    }

    pub(super) fn ensure_trailing_empty_row(
        &mut self,
        target: KvTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let has_trailing_empty = self
            .kv_rows(target)
            .last()
            .map(|row| {
                row.key_input.read(cx).value().trim().is_empty()
                    && row.value_input.read(cx).value().trim().is_empty()
            })
            .unwrap_or(false);
        if !has_trailing_empty {
            let empty = KeyValuePair {
                key: String::new(),
                value: String::new(),
                enabled: true,
            };
            let row = self.make_kv_row(target, empty, window, cx);
            self.kv_rows_mut(target).push(row);
        }
    }

    pub(super) fn collect_meaningful_pairs(&self, target: KvTarget, cx: &App) -> Vec<KeyValuePair> {
        self.kv_rows(target)
            .iter()
            .filter_map(|row| {
                let key = row.key_input.read(cx).value().to_string();
                let value = row.value_input.read(cx).value().to_string();
                if key.trim().is_empty() && value.trim().is_empty() {
                    None
                } else {
                    Some(KeyValuePair {
                        key,
                        value,
                        enabled: row.enabled,
                    })
                }
            })
            .collect()
    }

    pub(super) fn on_kv_rows_changed(&mut self, target: KvTarget, cx: &mut Context<Self>) {
        if target == KvTarget::Params && self.input_sync_guard.is_active() {
            self.input_sync_guard.deferred = true;
            return;
        }
        let next = self.collect_meaningful_pairs(target, cx);
        match target {
            KvTarget::Params => {
                if self.editor.draft().params != next {
                    self.editor.draft_mut().params = next;
                }
                let next_url = url_with_params(
                    self.editor.draft().url.as_str(),
                    self.editor.draft().params.as_slice(),
                );
                if self.editor.draft().url != next_url {
                    self.editor.draft_mut().url = next_url;
                }
            }
            KvTarget::Headers => {
                if self.editor.draft().headers != next {
                    self.editor.draft_mut().headers = next;
                }
            }
            KvTarget::BodyUrlEncoded => {
                if self.selected_body_kind(cx) != BodyKind::UrlEncoded {
                    return;
                }
                let next_body = BodyType::UrlEncoded { entries: next };
                if self.editor.draft().body != next_body {
                    self.editor.draft_mut().body = next_body;
                }
            }
            KvTarget::BodyFormDataText => {
                if self.selected_body_kind(cx) != BodyKind::FormData {
                    return;
                }
                let file_fields = match &self.editor.draft().body {
                    BodyType::FormData { file_fields, .. } => file_fields.clone(),
                    _ => Vec::new(),
                };
                let next_body = BodyType::FormData {
                    text_fields: next,
                    file_fields,
                };
                if self.editor.draft().body != next_body {
                    self.editor.draft_mut().body = next_body;
                }
            }
        }
        self.editor.refresh_save_status();
        cx.notify();
    }

    pub(super) fn add_kv_row(
        &mut self,
        target: KvTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let row = self.make_kv_row(
            target,
            KeyValuePair {
                key: String::new(),
                value: String::new(),
                enabled: true,
            },
            window,
            cx,
        );
        self.kv_rows_mut(target).push(row);
        cx.notify();
    }

    pub(super) fn remove_kv_row(
        &mut self,
        target: KvTarget,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ix) = self.kv_rows(target).iter().position(|row| row.id == id) {
            self.kv_rows_mut(target).remove(ix);
        }
        self.ensure_trailing_empty_row(target, window, cx);
        self.on_kv_rows_changed(target, cx);
    }

    pub(super) fn set_kv_row_enabled(
        &mut self,
        target: KvTarget,
        id: u64,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self.kv_rows_mut(target).iter_mut().find(|row| row.id == id) {
            row.enabled = enabled;
            self.on_kv_rows_changed(target, cx);
        }
    }

    pub(super) fn add_form_data_file_field(&mut self, cx: &mut Context<Self>) {
        let BodyType::FormData {
            text_fields,
            mut file_fields,
        } = self.editor.draft().body.clone()
        else {
            return;
        };
        file_fields.push(crate::domain::request::FileField {
            key: format!("file{}", file_fields.len() + 1),
            blob_hash: String::new(),
            file_name: None,
            enabled: true,
        });
        self.editor.draft_mut().body = BodyType::FormData {
            text_fields,
            file_fields,
        };
        self.editor.refresh_save_status();
        cx.notify();
    }

    pub(super) fn remove_form_data_file_field(&mut self, index: usize, cx: &mut Context<Self>) {
        let BodyType::FormData {
            text_fields,
            mut file_fields,
        } = self.editor.draft().body.clone()
        else {
            return;
        };
        if index < file_fields.len() {
            file_fields.remove(index);
            self.editor.draft_mut().body = BodyType::FormData {
                text_fields,
                file_fields,
            };
            self.editor.refresh_save_status();
            cx.notify();
        }
    }

    pub(super) fn set_form_data_file_field_enabled(
        &mut self,
        index: usize,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        let BodyType::FormData {
            text_fields,
            mut file_fields,
        } = self.editor.draft().body.clone()
        else {
            return;
        };
        if let Some(field) = file_fields.get_mut(index) {
            field.enabled = enabled;
            self.editor.draft_mut().body = BodyType::FormData {
                text_fields,
                file_fields,
            };
            self.editor.refresh_save_status();
            cx.notify();
        }
    }

    pub(super) fn clear_form_data_file_field(&mut self, index: usize, cx: &mut Context<Self>) {
        let BodyType::FormData {
            text_fields,
            mut file_fields,
        } = self.editor.draft().body.clone()
        else {
            return;
        };
        if let Some(field) = file_fields.get_mut(index) {
            field.blob_hash.clear();
            field.file_name = None;
            self.editor.draft_mut().body = BodyType::FormData {
                text_fields,
                file_fields,
            };
            self.editor.refresh_save_status();
            cx.notify();
        }
    }

    pub(super) fn clear_binary_body_file(&mut self, cx: &mut Context<Self>) {
        if let BodyType::BinaryFile {
            blob_hash,
            file_name,
        } = &mut self.editor.draft_mut().body
        {
            blob_hash.clear();
            *file_name = None;
            self.editor.refresh_save_status();
            cx.notify();
        }
    }

    pub(super) fn pick_binary_body_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_body_file_for_target(BodyFileTarget::Binary, window, cx);
    }

    pub(super) fn pick_form_data_file_field(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pick_body_file_for_target(BodyFileTarget::FormDataIndex(index), window, cx);
    }

    pub(super) fn pick_body_file_for_target(
        &mut self,
        target: BodyFileTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                es_fluent::localize("request_tab_body_pick_file", None)
                    .to_string()
                    .into(),
            ),
        });
        let services = cx.global::<AppServicesGlobal>().0.clone();

        cx.spawn_in(window, async move |this, cx| {
            let picked_path = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let Some(path) = picked_path else {
                return;
            };

            let size_bytes = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            if size_bytes > LARGE_BODY_FILE_CONFIRM_BYTES {
                let detail = format!(
                    "{} {}",
                    es_fluent::localize("request_tab_body_large_file_detail", None),
                    format_bytes(size_bytes)
                );
                let answers = vec![
                    gpui::PromptButton::ok(es_fluent::localize(
                        "request_tab_body_large_file_continue",
                        None,
                    )),
                    gpui::PromptButton::cancel(es_fluent::localize(
                        "request_tab_body_large_file_cancel",
                        None,
                    )),
                ];
                let response = cx
                    .prompt(
                        gpui::PromptLevel::Warning,
                        &es_fluent::localize("request_tab_body_large_file_title", None),
                        Some(&detail),
                        &answers,
                    )
                    .await
                    .unwrap_or(1);
                if response != 0 {
                    return;
                }
            }

            let path_for_import = path.clone();
            let services_for_import = services.clone();
            let imported =
                tokio::task::spawn_blocking(move || -> Result<(String, Option<String>), String> {
                    let file = std::fs::File::open(&path_for_import).map_err(|e| {
                        format!(
                            "{}: {e}",
                            es_fluent::localize("request_tab_body_file_load_failed", None)
                        )
                    })?;
                    let blob = services_for_import
                        .blob_store
                        .write_from_reader(file, None)
                        .map_err(|e| {
                            format!(
                                "{}: {e}",
                                es_fluent::localize("request_tab_body_file_load_failed", None)
                            )
                        })?;
                    let file_name = path_for_import
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string());
                    Ok((blob.hash, file_name))
                })
                .await;

            let (blob_hash, file_name) = match imported {
                Ok(Ok(value)) => value,
                Ok(Err(err)) => {
                    let _ = this.update(cx, |this, cx| {
                        this.editor.set_preflight_error(err);
                        cx.notify();
                    });
                    return;
                }
                Err(err) => {
                    let _ = this.update(cx, |this, cx| {
                        this.editor
                            .set_preflight_error(format!("file import task failed: {err}"));
                        cx.notify();
                    });
                    return;
                }
            };

            let _ = this.update(cx, |this, cx| {
                this.apply_body_file_selection(target, blob_hash.clone(), file_name.clone());
                this.editor.refresh_save_status();
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn apply_body_file_selection(
        &mut self,
        target: BodyFileTarget,
        blob_hash: String,
        file_name: Option<String>,
    ) {
        match target {
            BodyFileTarget::Binary => {
                if let BodyType::BinaryFile {
                    blob_hash: current_hash,
                    file_name: current_name,
                } = &mut self.editor.draft_mut().body
                {
                    *current_hash = blob_hash;
                    *current_name = file_name;
                }
            }
            BodyFileTarget::FormDataIndex(index) => {
                if let BodyType::FormData { file_fields, .. } = &mut self.editor.draft_mut().body
                    && let Some(field) = file_fields.get_mut(index)
                {
                    field.blob_hash = blob_hash;
                    field.file_name = file_name;
                }
            }
        }
    }
}
