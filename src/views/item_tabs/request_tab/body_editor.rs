use super::*;
use super::kv_editor::render_kv_table;

// ---------------------------------------------------------------------------
// Body editor rendering — extracted from RequestTabView::render
// ---------------------------------------------------------------------------

pub(super) fn render_body_editor(
    view: &mut RequestTabView,
    request: &RequestItem,
    _window: &mut Window,
    cx: &mut Context<RequestTabView>,
) -> gpui::Div {
    let urlencoded_table = render_kv_table(
        &view.body_urlencoded_kv_table,
        KvTarget::BodyUrlEncoded,
        "body-urlencoded",
        &view.body_urlencoded_rows,
        cx,
    );
    let form_text_table = render_kv_table(
        &view.body_form_text_kv_table,
        KvTarget::BodyFormDataText,
        "body-form-text",
        &view.body_form_text_rows,
        cx,
    );

    v_flex()
        .w_full()
        .items_stretch()
        .gap_2()
        .child(
            div()
                .text_xs()
                .text_color(gpui::hsla(0., 0., 0.45, 1.))
                .child(es_fluent::localize("request_tab_body_type_label", None)),
        )
        .child(div().w_56().child(Select::new(&view.body_type_select)))
        .child(match &request.body {
            BodyType::None => div()
                .text_xs()
                .text_color(gpui::hsla(0., 0., 0.45, 1.))
                .child(es_fluent::localize("request_tab_body_none_hint", None))
                .into_any_element(),
            BodyType::RawText { .. } => div()
                .w_full()
                .child(Input::new(&view.body_raw_text_input).w_full().h(px(220.)))
                .into_any_element(),
            BodyType::RawJson { .. } => div()
                .w_full()
                .child(Input::new(&view.body_raw_json_input).w_full().h(px(220.)))
                .into_any_element(),
            BodyType::UrlEncoded { .. } => urlencoded_table.into_any_element(),
            BodyType::FormData { file_fields, .. } => v_flex()
                .gap_3()
                .child(
                    div()
                        .text_xs()
                        .text_color(gpui::hsla(0., 0., 0.45, 1.))
                        .child(es_fluent::localize("request_tab_body_form_text_fields", None)),
                )
                .child(form_text_table)
                .child(
                    div()
                        .text_xs()
                        .text_color(gpui::hsla(0., 0., 0.45, 1.))
                        .child(es_fluent::localize("request_tab_body_form_file_fields", None)),
                )
                .children(file_fields.iter().enumerate().map(|(index, field)| {
                    let file_label = field
                        .file_name
                        .clone()
                        .filter(|name| !name.trim().is_empty())
                        .unwrap_or_else(|| {
                            es_fluent::localize("request_tab_body_no_file_selected", None)
                                .to_string()
                        });
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            Checkbox::new(("body-form-file-enabled", index))
                                .checked(field.enabled)
                                .on_click(cx.listener(move |this, checked, _, cx| {
                                    this.set_form_data_file_field_enabled(index, *checked, cx);
                                })),
                        )
                        .child(div().w_32().child(field.key.clone()))
                        .child(div().flex_1().child(file_label))
                        .child(
                            Button::new(("body-form-file-pick", index))
                                .outline()
                                .label(if field.blob_hash.trim().is_empty() {
                                    es_fluent::localize("request_tab_body_pick_file", None)
                                } else {
                                    es_fluent::localize("request_tab_body_replace_file", None)
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.pick_form_data_file_field(index, window, cx);
                                })),
                        )
                        .child(
                            Button::new(("body-form-file-clear", index))
                                .ghost()
                                .label(es_fluent::localize("request_tab_body_clear_file", None))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.clear_form_data_file_field(index, cx);
                                })),
                        )
                        .child(
                            Button::new(("body-form-file-remove", index))
                                .ghost()
                                .label(es_fluent::localize(
                                    "request_tab_body_remove_file_field",
                                    None,
                                ))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.remove_form_data_file_field(index, cx);
                                })),
                        )
                }))
                .child(
                    Button::new("body-form-file-add")
                        .outline()
                        .label(es_fluent::localize("request_tab_body_add_file_field", None))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.add_form_data_file_field(cx);
                        })),
                )
                .into_any_element(),
            BodyType::BinaryFile {
                blob_hash,
                file_name,
            } => v_flex()
                .gap_2()
                .child(
                    div().text_sm().child(
                        file_name
                            .clone()
                            .filter(|name| !name.trim().is_empty())
                            .unwrap_or_else(|| {
                                es_fluent::localize("request_tab_body_no_file_selected", None)
                                    .to_string()
                            }),
                    ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("body-binary-pick")
                                .outline()
                                .label(if blob_hash.trim().is_empty() {
                                    es_fluent::localize("request_tab_body_pick_file", None)
                                } else {
                                    es_fluent::localize("request_tab_body_replace_file", None)
                                })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.pick_binary_body_file(window, cx);
                                })),
                        )
                        .child(
                            Button::new("body-binary-clear")
                                .ghost()
                                .label(es_fluent::localize("request_tab_body_clear_file", None))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.clear_binary_body_file(cx);
                                })),
                        ),
                )
                .into_any_element(),
        })
}
