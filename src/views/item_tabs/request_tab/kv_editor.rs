use super::*;

// ---------------------------------------------------------------------------
// Key-value row editor rendering — reusable across Params, Headers,
// BodyUrlEncoded, BodyFormDataText
// ---------------------------------------------------------------------------

pub(super) fn render_kv_rows(
    rows: &[KeyValueEditorRow],
    target: KvTarget,
    prefix: &'static str,
    cx: &mut Context<RequestTabView>,
) -> gpui::Div {
    v_flex()
        .gap_2()
        .children(rows.iter().map(|row| {
            let id = row.id;
            let enabled = row.enabled;
            let key_input = row.key_input.clone();
            let value_input = row.value_input.clone();
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    Checkbox::new((prefix, id))
                        .checked(enabled)
                        .on_click(cx.listener(move |this, checked, _, cx| {
                            this.set_kv_row_enabled(target, id, *checked, cx);
                        })),
                )
                .child(div().flex_1().child(Input::new(&key_input).large()))
                .child(div().flex_1().child(Input::new(&value_input).large()))
                .child(
                    Button::new((prefix, id))
                        .ghost()
                        .label(es_fluent::localize("request_tab_kv_remove_row", None))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.remove_kv_row(target, id, window, cx);
                        })),
                )
        }))
        .child(
            Button::new(prefix)
                .outline()
                .label(es_fluent::localize("request_tab_kv_add_row", None))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.add_kv_row(target, window, cx);
                })),
        )
}
