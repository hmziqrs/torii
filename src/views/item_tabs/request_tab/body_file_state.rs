use super::*;

impl RequestTabView {
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
