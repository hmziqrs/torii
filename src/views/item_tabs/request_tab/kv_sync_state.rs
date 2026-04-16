use super::*;

impl RequestTabView {
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
                    // Keep URL input in sync after params changes.
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
}
