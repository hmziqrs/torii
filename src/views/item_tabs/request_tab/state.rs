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
}
