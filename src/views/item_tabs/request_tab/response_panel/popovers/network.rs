use gpui::IntoElement;
use gpui_component::Anchor;

use super::*;

pub(super) fn render_network_popover(
    view: &mut RequestTabView,
    response: &crate::domain::response::ResponseSummary,
    cx: &mut Context<RequestTabView>,
) -> gpui::AnyElement {
    let token = es_fluent::localize("request_tab_response_details_popover_title", None).to_string();
    let http_version = response.http_version.clone();
    let remote_addr = response.remote_addr.clone();
    let tls = response.tls.clone();

    hover_popover_trigger(
        view,
        "response-network-popover",
        token_text(token, cx.theme().muted_foreground, false),
        ResponseMetaHover::Network,
        view.network_meta_focus.clone(),
        Anchor::TopRight,
        move |cx| {
            let muted = cx.theme().muted_foreground;
            let mut content = base_popover_container(
                es_fluent::localize("request_tab_response_details_popover_title", None).to_string(),
                cx,
            )
            .child(row(
                es_fluent::localize("request_tab_response_details_http_version", None).to_string(),
                dash_or(http_version.clone()),
                muted,
            ))
            .child(row(
                es_fluent::localize("request_tab_response_details_remote_addr", None).to_string(),
                dash_or(remote_addr.clone()),
                muted,
            ));

            if let Some(tls) = &tls {
                content = content
                    .child(row(
                        es_fluent::localize("request_tab_response_details_cert_cn", None)
                            .to_string(),
                        dash_or(tls.certificate_cn.clone()),
                        muted,
                    ))
                    .child(row(
                        es_fluent::localize("request_tab_response_details_issuer_cn", None)
                            .to_string(),
                        dash_or(tls.issuer_cn.clone()),
                        muted,
                    ))
                    .child(row(
                        es_fluent::localize("request_tab_response_details_valid_until", None)
                            .to_string(),
                        format_unix_ms(tls.valid_until),
                        muted,
                    ));
            }
            content.into_any_element()
        },
        cx,
    )
}
