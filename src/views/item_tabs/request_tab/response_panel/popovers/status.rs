use gpui::{IntoElement, ParentElement as _, Styled as _, div, px};
use gpui_component::Anchor;

use super::*;

pub(super) fn render_status_popover(
    view: &mut RequestTabView,
    response: &crate::domain::response::ResponseSummary,
    cx: &mut Context<RequestTabView>,
) -> gpui::AnyElement {
    let status_color = status_code_color(response.status_code);
    let code = response.status_code;
    let status_reason = if response.status_text.trim().is_empty() {
        http::StatusCode::from_u16(code)
            .ok()
            .and_then(|s| s.canonical_reason().map(ToOwned::to_owned))
            .unwrap_or_else(|| "Unknown".to_string())
    } else {
        response.status_text.clone()
    };

    hover_popover_trigger(
        view,
        "response-status-popover",
        div()
            .text_xs()
            .font_weight(gpui::FontWeight::BOLD)
            .text_color(status_color)
            .px_2()
            .py_0p5()
            .border_1()
            .border_color(status_color.opacity(0.55))
            .bg(status_color.opacity(0.12))
            .rounded(px(6.))
            .child(code.to_string()),
        ResponseMetaHover::Status,
        view.status_meta_focus.clone(),
        Anchor::TopLeft,
        move |cx| {
            let title = es_fluent::localize("request_tab_response_meta_status", None).to_string();
            let desc = status_description_key(code);
            base_popover_container(title, cx)
                .child(row(
                    es_fluent::localize("request_tab_response_meta_status_code", None).to_string(),
                    code.to_string(),
                    cx.theme().muted_foreground,
                ))
                .child(row(
                    es_fluent::localize("request_tab_response_meta_reason", None).to_string(),
                    status_reason.clone(),
                    cx.theme().muted_foreground,
                ))
                .child(
                    div()
                        .mt_1()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(es_fluent::localize(desc, None)),
                )
                .into_any_element()
        },
        cx,
    )
}

fn status_description_key(code: u16) -> &'static str {
    match code {
        100 => "request_tab_status_desc_100",
        101 => "request_tab_status_desc_101",
        102 => "request_tab_status_desc_102",
        103 => "request_tab_status_desc_103",
        200 => "request_tab_status_desc_200",
        201 => "request_tab_status_desc_201",
        202 => "request_tab_status_desc_202",
        203 => "request_tab_status_desc_203",
        204 => "request_tab_status_desc_204",
        205 => "request_tab_status_desc_205",
        206 => "request_tab_status_desc_206",
        207 => "request_tab_status_desc_207",
        208 => "request_tab_status_desc_208",
        226 => "request_tab_status_desc_226",
        300 => "request_tab_status_desc_300",
        301 => "request_tab_status_desc_301",
        302 => "request_tab_status_desc_302",
        303 => "request_tab_status_desc_303",
        304 => "request_tab_status_desc_304",
        305 => "request_tab_status_desc_305",
        306 => "request_tab_status_desc_306",
        307 => "request_tab_status_desc_307",
        308 => "request_tab_status_desc_308",
        400 => "request_tab_status_desc_400",
        401 => "request_tab_status_desc_401",
        402 => "request_tab_status_desc_402",
        403 => "request_tab_status_desc_403",
        404 => "request_tab_status_desc_404",
        405 => "request_tab_status_desc_405",
        406 => "request_tab_status_desc_406",
        407 => "request_tab_status_desc_407",
        408 => "request_tab_status_desc_408",
        409 => "request_tab_status_desc_409",
        410 => "request_tab_status_desc_410",
        411 => "request_tab_status_desc_411",
        412 => "request_tab_status_desc_412",
        413 => "request_tab_status_desc_413",
        414 => "request_tab_status_desc_414",
        415 => "request_tab_status_desc_415",
        416 => "request_tab_status_desc_416",
        417 => "request_tab_status_desc_417",
        418 => "request_tab_status_desc_418",
        421 => "request_tab_status_desc_421",
        422 => "request_tab_status_desc_422",
        423 => "request_tab_status_desc_423",
        424 => "request_tab_status_desc_424",
        425 => "request_tab_status_desc_425",
        426 => "request_tab_status_desc_426",
        428 => "request_tab_status_desc_428",
        429 => "request_tab_status_desc_429",
        431 => "request_tab_status_desc_431",
        451 => "request_tab_status_desc_451",
        500 => "request_tab_status_desc_500",
        501 => "request_tab_status_desc_501",
        502 => "request_tab_status_desc_502",
        503 => "request_tab_status_desc_503",
        504 => "request_tab_status_desc_504",
        505 => "request_tab_status_desc_505",
        506 => "request_tab_status_desc_506",
        507 => "request_tab_status_desc_507",
        508 => "request_tab_status_desc_508",
        510 => "request_tab_status_desc_510",
        511 => "request_tab_status_desc_511",
        100..=199 => "request_tab_status_desc_1xx_generic",
        200..=299 => "request_tab_status_desc_2xx_generic",
        300..=399 => "request_tab_status_desc_3xx_generic",
        400..=499 => "request_tab_status_desc_4xx_generic",
        500..=599 => "request_tab_status_desc_5xx_generic",
        _ => "request_tab_status_desc_unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::status_description_key;

    #[test]
    fn uses_exact_known_status_descriptions() {
        assert_eq!(status_description_key(200), "request_tab_status_desc_200");
        assert_eq!(status_description_key(404), "request_tab_status_desc_404");
        assert_eq!(status_description_key(511), "request_tab_status_desc_511");
    }

    #[test]
    fn falls_back_to_class_level_descriptions_for_unknown_statuses() {
        assert_eq!(
            status_description_key(299),
            "request_tab_status_desc_2xx_generic"
        );
        assert_eq!(
            status_description_key(399),
            "request_tab_status_desc_3xx_generic"
        );
        assert_eq!(
            status_description_key(499),
            "request_tab_status_desc_4xx_generic"
        );
        assert_eq!(
            status_description_key(599),
            "request_tab_status_desc_5xx_generic"
        );
        assert_eq!(
            status_description_key(700),
            "request_tab_status_desc_unknown"
        );
    }
}
