use super::*;
use gpui_component::IconName;
use gpui_component::table::DataTable;

// ---------------------------------------------------------------------------
// Key-value table delegate — reusable across Params, Headers,
// BodyUrlEncoded, BodyFormDataText
// ---------------------------------------------------------------------------

pub(super) struct KvDelegateRow {
    pub(super) id: u64,
    pub(super) enabled: bool,
    pub(super) key_input: Entity<InputState>,
    pub(super) value_input: Entity<InputState>,
}

pub(super) struct KvTableDelegate {
    rows: Vec<KvDelegateRow>,
    columns: Vec<Column>,
    view: Entity<RequestTabView>,
    target: KvTarget,
    prefix: &'static str,
}

impl KvTableDelegate {
    pub(super) fn new(
        view: Entity<RequestTabView>,
        target: KvTarget,
        prefix: &'static str,
    ) -> Self {
        Self {
            rows: Vec::new(),
            columns: vec![
                Column::new("", "").width(px(40.)).resizable(false).movable(false),
                Column::new("key", es_fluent::localize("request_tab_kv_col_key", None))
                    .width(px(200.))
                    .resizable(true)
                    .movable(false),
                Column::new("value", es_fluent::localize("request_tab_kv_col_value", None))
                    .width(px(300.))
                    .resizable(true)
                    .movable(false),
                Column::new("", "")
                    .width(px(80.))
                    .resizable(false)
                    .movable(false),
            ],
            view,
            target,
            prefix,
        }
    }

    pub(super) fn set_rows(&mut self, rows: Vec<KvDelegateRow>) {
        self.rows = rows;
    }
}

impl TableDelegate for KvTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let row = &self.rows[row_ix];
        match col_ix {
            // Checkbox
            0 => {
                let view = self.view.clone();
                let target = self.target;
                let id = row.id;
                let enabled = row.enabled;
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Checkbox::new((self.prefix, id))
                            .checked(enabled)
                            .on_click(move |checked, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.set_kv_row_enabled(target, id, *checked, window, cx);
                                });
                            }),
                    )
                    .into_any_element()
            }
            // Key input
            1 => div()
                .child(
                    Input::new(&row.key_input)
                        .appearance(false)
                        .bordered(false),
                )
                .into_any_element(),
            // Value input
            2 => div()
                .child(
                    Input::new(&row.value_input)
                        .appearance(false)
                        .bordered(false),
                )
                .into_any_element(),
            // Remove button
            _ => {
                let view = self.view.clone();
                let target = self.target;
                let id = row.id;
                div()
                    .flex()
                    .items_center()
                    .child(
                        Button::new((self.prefix, id))
                            .ghost()
                            .label(es_fluent::localize("request_tab_kv_remove_row", None))
                            .on_click(move |_event, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.remove_kv_row(target, id, window, cx);
                                });
                            }),
                    )
                    .into_any_element()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public render function
// ---------------------------------------------------------------------------

pub(super) fn render_kv_table(
    table: &Entity<TableState<KvTableDelegate>>,
    target: KvTarget,
    prefix: &'static str,
    rows: &[KeyValueEditorRow],
    cx: &mut Context<RequestTabView>,
) -> gpui::Div {
    // Push current row data into the delegate
    let delegate_rows: Vec<KvDelegateRow> = rows
        .iter()
        .map(|r| KvDelegateRow {
            id: r.id,
            enabled: r.enabled,
            key_input: r.key_input.clone(),
            value_input: r.value_input.clone(),
        })
        .collect();

    table.update(cx, |state, cx| {
        state.delegate_mut().set_rows(delegate_rows);
        state.refresh(cx);
    });

    // Dynamic height that grows with rows, capped at 8 visible
    let row_height: f32 = 32.;
    let header_height: f32 = 36.;
    let max_rows_visible = 8;
    let rows_visible = (rows.len() + 1).min(max_rows_visible);
    let table_height = header_height + rows_visible as f32 * row_height;

    let target_for_add = target;
    v_flex()
        .gap_2()
        .child(
            div()
                .h(px(table_height))
                .child(DataTable::new(table).bordered(true)),
        )
        .child(
            Button::new(prefix)
                .ghost()
                .small()
                .icon(IconName::Plus)
                .label(es_fluent::localize("request_tab_kv_add_row", None))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.add_kv_row(target_for_add, window, cx);
                })),
        )
}
