use super::*;

pub(super) fn render_body_actions(
    _view: &mut RequestTabView,
    can_copy: bool,
    cx: &mut Context<RequestTabView>,
) -> gpui::Div {
    h_flex()
        .gap_1()
        .child({
            let mut btn = Button::new("request-response-copy")
                .outline()
                .disabled(!can_copy)
                .label(es_fluent::localize(
                    "request_tab_response_action_copy",
                    None,
                ));
            if !can_copy {
                btn = btn.tooltip(es_fluent::localize(
                    "request_tab_response_action_copy_disabled_tooltip",
                    None,
                ));
            }
            btn.on_click(cx.listener(|this, _, window, cx| {
                if let Err(err) = this.copy_response_body(cx) {
                    window.push_notification(err, cx);
                } else {
                    window.push_notification(es_fluent::localize("request_tab_copy_ok", None), cx);
                }
            }))
        })
        .child(
            Button::new("request-response-save")
                .outline()
                .label(es_fluent::localize(
                    "request_tab_response_action_save",
                    None,
                ))
                .on_click(cx.listener(|this, _, window, cx| {
                    if let Err(err) = this.save_response_body_to_file(window, cx) {
                        window.push_notification(err, cx);
                    }
                })),
        )
}
