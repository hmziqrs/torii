use super::*;

// ---------------------------------------------------------------------------
// Auth editor rendering — extracted from RequestTabView::render
// ---------------------------------------------------------------------------

pub(super) fn render_auth_editor(
    view: &RequestTabView,
    request: &RequestItem,
    cx: &mut Context<RequestTabView>,
) -> gpui::Div {
    let muted = cx.theme().muted_foreground;
    v_flex()
        .gap_2()
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child(es_fluent::localize("request_tab_auth_type_label", None)),
        )
        .child(div().w_56().child(Select::new(&view.auth_type_select)))
        .child(match &request.auth {
            AuthType::None => div()
                .text_xs()
                .text_color(muted)
                .child(es_fluent::localize("request_tab_auth_none_hint", None))
                .into_any_element(),
            AuthType::Basic { .. } => v_flex()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(es_fluent::localize("request_tab_auth_basic_username", None)),
                )
                .child(Input::new(&view.auth_basic_username_input).large())
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(es_fluent::localize("request_tab_auth_basic_password", None)),
                )
                .child(
                    Input::new(&view.auth_basic_password_ref_input)
                        .large()
                        .mask_toggle(),
                )
                .into_any_element(),
            AuthType::Bearer { .. } => v_flex()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(es_fluent::localize("request_tab_auth_bearer_token", None)),
                )
                .child(
                    Input::new(&view.auth_bearer_token_ref_input)
                        .large()
                        .mask_toggle(),
                )
                .into_any_element(),
            AuthType::ApiKey { .. } => v_flex()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(es_fluent::localize("request_tab_auth_api_key_name", None)),
                )
                .child(Input::new(&view.auth_api_key_name_input).large())
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(es_fluent::localize("request_tab_auth_api_key_value", None)),
                )
                .child(
                    Input::new(&view.auth_api_key_value_ref_input)
                        .large()
                        .mask_toggle(),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(es_fluent::localize("request_tab_auth_api_key_location", None)),
                )
                .child(
                    div()
                        .w_56()
                        .child(Select::new(&view.auth_api_key_location_select)),
                )
                .into_any_element(),
        })
}
