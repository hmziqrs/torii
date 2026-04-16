use super::*;
use gpui_component::table::DataTable;

mod actions;
mod content;
mod content_tabs;
mod tables;

pub(super) use tables::{CookiesTableDelegate, HeadersTableDelegate, TimingTableDelegate};

pub(super) fn render_response_panel(
    view: &mut RequestTabView,
    window: &mut Window,
    cx: &mut Context<RequestTabView>,
) -> gpui::Div {
    let muted = cx.theme().muted_foreground;

    match view.editor.exec_status() {
        ExecStatus::Idle => div().child(
            div()
                .text_sm()
                .text_color(muted)
                .child(es_fluent::localize("request_tab_response_empty", None)),
        ),
        ExecStatus::Sending => div().child(
            div()
                .text_sm()
                .text_color(muted)
                .child(es_fluent::localize("request_tab_sending", None)),
        ),
        ExecStatus::Streaming => div().child(
            div()
                .text_sm()
                .text_color(muted)
                .child(es_fluent::localize("request_tab_streaming", None)),
        ),
        ExecStatus::Completed { .. } => {
            let response = match view.editor.exec_status() {
                ExecStatus::Completed { response } => response.clone(),
                _ => unreachable!(),
            };
            content::render_completed_response(view, &response, window, cx)
        }
        ExecStatus::Failed { .. } => {
            let (summary, classified) = match view.editor.exec_status() {
                ExecStatus::Failed {
                    summary,
                    classified,
                } => (summary.clone(), classified.clone()),
                _ => unreachable!(),
            };
            let (title, detail) = classified_error_display(classified.as_ref(), &summary);
            let expanded = view.error_detail_expanded;
            div()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(gpui::red())
                        .child(title),
                )
                .child(
                    div()
                        .text_xs()
                        .font_family("monospace")
                        .text_color(muted)
                        .child(if expanded {
                            detail.clone()
                        } else {
                            summary.clone()
                        }),
                )
                .child(
                    Button::new("error-detail-toggle")
                        .ghost()
                        .label(if expanded {
                            es_fluent::localize("request_tab_error_detail_collapse", None)
                        } else {
                            es_fluent::localize("request_tab_error_detail_expand", None)
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.error_detail_expanded = !this.error_detail_expanded;
                            cx.notify();
                        })),
                )
        }
        ExecStatus::Cancelled { .. } => {
            let partial_size = match view.editor.exec_status() {
                ExecStatus::Cancelled { partial_size } => *partial_size,
                _ => unreachable!(),
            };
            let msg = match partial_size {
                Some(size) => format!(
                    "{} ({size})",
                    es_fluent::localize("request_tab_response_cancelled_with_bytes", None)
                ),
                None => es_fluent::localize("request_tab_response_cancelled", None).to_string(),
            };
            div().child(
                div()
                    .text_sm()
                    .text_color(gpui::hsla(30. / 360., 0.8, 0.45, 1.))
                    .child(msg),
            )
        }
    }
}
