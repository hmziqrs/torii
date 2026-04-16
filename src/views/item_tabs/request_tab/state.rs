use super::*;

impl RequestTabView {
    pub(super) fn mark_kv_table_dirty(&mut self, target: KvTarget) {
        match target {
            KvTarget::Params => self.params_kv_dirty = true,
            KvTarget::Headers => self.headers_kv_dirty = true,
            KvTarget::BodyUrlEncoded => self.body_urlencoded_kv_dirty = true,
            KvTarget::BodyFormDataText => self.body_form_text_kv_dirty = true,
        }
    }

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

        self.kv_subscriptions.push(cx.subscribe_in(
            &key_input,
            window,
            move |this: &mut RequestTabView,
                  _: &Entity<InputState>,
                  event: &InputEvent,
                  window: &mut Window,
                  cx| {
                if let InputEvent::Change = event {
                    this.on_kv_rows_changed(target, window, cx);
                }
            },
        ));
        self.kv_subscriptions.push(cx.subscribe_in(
            &value_input,
            window,
            move |this: &mut RequestTabView,
                  _: &Entity<InputState>,
                  event: &InputEvent,
                  window: &mut Window,
                  cx| {
                if let InputEvent::Change = event {
                    this.on_kv_rows_changed(target, window, cx);
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
        // Drop old KV subscriptions before creating new row entities.  Keeping them around
        // would let _dead_ subscriptions (for dropped InputState entities) accumulate in the
        // Vec without bound — see idle-cpu-audit.md Bug #2.
        self.kv_subscriptions.clear();
        // Mark the corresponding table delegate as needing a row-data push on next render.
        self.mark_kv_table_dirty(target);

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

    pub(super) fn on_kv_rows_changed(
        &mut self,
        target: KvTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if target == KvTarget::Params && self.input_sync_guard.is_active() {
            self.input_sync_guard.deferred = true;
            return;
        }
        let next = self.collect_meaningful_pairs(target, cx);
        let mut draft_changed = false;
        match target {
            KvTarget::Params => {
                if self.editor.draft().params != next {
                    self.editor.draft_mut().params = next;
                    draft_changed = true;
                }
                let next_url = url_with_params(
                    self.editor.draft().url.as_str(),
                    self.editor.draft().params.as_slice(),
                );
                if self.editor.draft().url != next_url {
                    self.editor.draft_mut().url = next_url;
                    draft_changed = true;
                    // Sync URL input to match the draft (params changed → URL rebuilt).
                    // Guard is in the url subscription handler: it checks draft.url == input
                    // value before propagating, so this does not create a feedback loop.
                    self.url_input.update(cx, |s, cx| {
                        s.set_value(self.editor.draft().url.clone(), window, cx);
                    });
                }
            }
            KvTarget::Headers => {
                if self.editor.draft().headers != next {
                    self.editor.draft_mut().headers = next;
                    draft_changed = true;
                }
            }
            KvTarget::BodyUrlEncoded => {
                if self.selected_body_kind(cx) != BodyKind::UrlEncoded {
                    return;
                }
                let next_body = BodyType::UrlEncoded { entries: next };
                if self.editor.draft().body != next_body {
                    self.editor.draft_mut().body = next_body;
                    draft_changed = true;
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
                    draft_changed = true;
                }
            }
        }
        if draft_changed {
            self.editor.refresh_save_status();
            cx.notify();
        }
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
        self.mark_kv_table_dirty(target);
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
        self.mark_kv_table_dirty(target);
        self.ensure_trailing_empty_row(target, window, cx);
        self.on_kv_rows_changed(target, window, cx);
    }

    pub(super) fn set_kv_row_enabled(
        &mut self,
        target: KvTarget,
        id: u64,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self.kv_rows_mut(target).iter_mut().find(|row| row.id == id) {
            row.enabled = enabled;
            self.mark_kv_table_dirty(target);
            self.on_kv_rows_changed(target, window, cx);
        }
    }
}
