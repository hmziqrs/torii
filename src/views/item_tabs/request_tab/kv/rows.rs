use super::super::*;

impl RequestTabView {
    pub(in super::super) fn make_kv_row(
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

        self.kv_subscriptions.entry(target).or_default().push(cx.subscribe_in(
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
        self.kv_subscriptions.entry(target).or_default().push(cx.subscribe_in(
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

    pub(in super::super) fn rebuild_kv_rows(
        &mut self,
        target: KvTarget,
        entries: &[KeyValuePair],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Drop only this target's subscriptions before creating new row entities.
        // Using per-target storage so that rebuilding one target (e.g., Params)
        // doesn't drop subscriptions for the other three (Headers, etc.).
        self.kv_subscriptions.remove(&target);
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

    pub(in super::super) fn ensure_trailing_empty_row(
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

    pub(in super::super) fn collect_meaningful_pairs(
        &self,
        target: KvTarget,
        cx: &App,
    ) -> Vec<KeyValuePair> {
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

    pub(in super::super) fn add_kv_row(
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

    pub(in super::super) fn remove_kv_row(
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

    pub(in super::super) fn set_kv_row_enabled(
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
