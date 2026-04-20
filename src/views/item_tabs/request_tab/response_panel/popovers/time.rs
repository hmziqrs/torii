use gpui::{FontWeight, IntoElement, ParentElement as _, Styled as _, div, px};
use gpui_component::Anchor;

use super::*;

const WATERFALL_WIDTH: f32 = 260.;

pub(super) fn render_time_popover(
    view: &mut RequestTabView,
    response: &crate::domain::response::ResponseSummary,
    cx: &mut Context<RequestTabView>,
) -> gpui::AnyElement {
    let token = format!(
        "{}: {}",
        es_fluent::localize("request_tab_response_total_time", None),
        format_ms(response.total_ms)
    );
    let phase = response.phase_timings.clone();
    let ttfb = response.ttfb_ms;
    let total_ms = response.total_ms;

    hover_popover_trigger(
        view,
        "response-time-popover",
        token_text(token, cx.theme().muted_foreground, false),
        ResponseMetaPopover::Time,
        Anchor::TopLeft,
        move |cx| {
            let muted = cx.theme().muted_foreground.opacity(0.8);
            let total_str = total_ms
                .map(|ms| format!("{:.2} ms", ms as f64))
                .unwrap_or_else(|| "—".to_string());

            let waiting_ms = phase.ttfb_ms.or(ttfb);
            let download_fallback = match (total_ms, waiting_ms) {
                (Some(total), Some(waiting)) => Some(total.saturating_sub(waiting)),
                _ => None,
            };
            let prepare_ms = phase.prepare_ms.or(Some(0));
            let dns_ms = phase.dns_ms.or(Some(0));
            let connect_ms = phase.connect_ms.or(Some(0));
            let download_ms = phase.download_ms.or(download_fallback).or(Some(0));
            let process_ms = phase.process_ms.or(Some(0));

            let mut rows = vec![
                (
                    es_fluent::localize("request_tab_response_time_phase_prepare", None)
                        .to_string(),
                    prepare_ms,
                    cx.theme().muted_foreground.opacity(0.22),
                    false,
                ),
                (
                    es_fluent::localize("request_tab_response_time_phase_dns", None).to_string(),
                    dns_ms,
                    cx.theme().warning,
                    false,
                ),
                (
                    es_fluent::localize("request_tab_response_time_phase_connect", None)
                        .to_string(),
                    connect_ms,
                    gpui::hsla(214. / 360., 0.65, 0.56, 1.0),
                    false,
                ),
                (
                    es_fluent::localize("request_tab_response_time_phase_ttfb", None).to_string(),
                    waiting_ms.or(Some(0)),
                    gpui::hsla(0., 0.8, 0.55, 1.0),
                    true,
                ),
                (
                    es_fluent::localize("request_tab_response_time_phase_download", None)
                        .to_string(),
                    download_ms,
                    cx.theme().success,
                    false,
                ),
                (
                    es_fluent::localize("request_tab_response_time_phase_process", None)
                        .to_string(),
                    process_ms,
                    cx.theme().muted_foreground.opacity(0.22),
                    false,
                ),
            ];

            let total_ms = rows
                .iter()
                .filter_map(|row| row.1)
                .sum::<u64>()
                .max(1) as f32;
            let mut offset_ms = 0f32;
            let mut layout = Vec::with_capacity(rows.len());
            for row in rows.drain(..) {
                let ms = row.1.unwrap_or(0) as f32;
                let left = (offset_ms / total_ms) * WATERFALL_WIDTH;
                let width = if ms > 0.0 {
                    ((ms / total_ms) * WATERFALL_WIDTH).max(2.0)
                } else {
                    0.0
                };
                layout.push((row, left, width));
                offset_ms += ms;
            }

            v_flex()
                .w(px(400.))
                .gap_2()
                .p_3()
                .child(
                    h_flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::BOLD)
                                .child(es_fluent::localize(
                                    "request_tab_response_time_popover_title",
                                    None,
                                )),
                        )
                        .child(
                            div()
                                .text_sm()
                                .font_family("monospace")
                                .font_weight(FontWeight::BOLD)
                                .child(total_str),
                        ),
                )
                .child(
                    v_flex()
                        .gap_1()
                        .children(layout.into_iter().map(|(row, left, width)| {
                            let ms_text = row
                                .1
                                .map(|ms| format!("{ms} ms"))
                                .unwrap_or_else(|| "—".to_string());
                            h_flex()
                                .items_center()
                                .h(px(34.))
                                .child(div().w(px(130.)).text_xs().text_color(muted).child(row.0))
                                .child(
                                    div()
                                        .relative()
                                        .w(px(WATERFALL_WIDTH))
                                        .h(px(24.))
                                        .child(
                                            div()
                                                .absolute()
                                                .left_0()
                                                .top_0()
                                                .h_full()
                                                .w(px(1.))
                                                .bg(cx.theme().border),
                                        )
                                        .child(
                                            div()
                                                .absolute()
                                                .right_0()
                                                .top_0()
                                                .h_full()
                                                .w(px(1.))
                                                .bg(cx.theme().border),
                                        )
                                        .child(
                                            div()
                                                .absolute()
                                                .left(px(left))
                                                .top(px(4.))
                                                .h(px(16.))
                                                .w(px(width))
                                                .when(row.3, |el| {
                                                    el.border_1()
                                                        .border_color(row.2)
                                                        .border_dashed()
                                                        .bg(gpui::transparent_black())
                                                })
                                                .when(!row.3, |el| el.bg(row.2)),
                                        ),
                                )
                                .child(
                                    div()
                                        .w(px(68.))
                                        .text_xs()
                                        .text_color(muted)
                                        .font_family("monospace")
                                        .text_right()
                                        .child(ms_text),
                                )
                        })),
                )
                .child(
                    h_flex()
                        .justify_between()
                        .text_xs()
                        .text_color(muted.opacity(0.55))
                        .child(div().child("Start"))
                        .child(div().child("End")),
                )
                .into_any_element()
        },
        cx,
    )
}
