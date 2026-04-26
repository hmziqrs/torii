use gpui::{AnyElement, IntoElement, ParentElement, Styled as _, WeakEntity, div, px};
use gpui_component::{
    Disableable as _, Selectable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};

use crate::{
    domain::{
        history::{HistoryEntry, HistoryState},
        ids::WorkspaceId,
    },
    root::{AppRoot, HistoryProtocolFilter, HistoryWorkspaceView},
};

pub(crate) fn render(
    workspace_id: WorkspaceId,
    view: &HistoryWorkspaceView,
    root: WeakEntity<AppRoot>,
) -> AnyElement {
    let weak_root_refresh = root.clone();
    let state_filter = view.state_filter;
    let protocol_filter = view.protocol_filter;
    let entries = view.entries.as_slice();
    let has_more = view.next_cursor.is_some();

    v_flex()
        .size_full()
        .p_6()
        .gap_4()
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_2xl()
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(es_fluent::localize("history_tab_title", None)),
                )
                .child(
                    Button::new("history-refresh")
                        .label(es_fluent::localize("history_tab_refresh", None))
                        .on_click(move |_, _, cx| {
                            let _ = weak_root_refresh.update(cx, |this, cx| {
                                this.refresh_history_for_workspace(workspace_id, cx);
                            });
                        }),
                ),
        )
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(gpui::transparent_black())
                        .child(es_fluent::localize("history_tab_filter_label", None)),
                )
                .child(filter_button(
                    workspace_id,
                    state_filter,
                    None,
                    es_fluent::localize("history_tab_filter_all", None),
                    root.clone(),
                ))
                .child(filter_button(
                    workspace_id,
                    state_filter,
                    Some(HistoryState::Completed),
                    es_fluent::localize("history_tab_state_completed", None),
                    root.clone(),
                ))
                .child(filter_button(
                    workspace_id,
                    state_filter,
                    Some(HistoryState::Failed),
                    es_fluent::localize("history_tab_state_failed", None),
                    root.clone(),
                ))
                .child(filter_button(
                    workspace_id,
                    state_filter,
                    Some(HistoryState::Cancelled),
                    es_fluent::localize("history_tab_state_cancelled", None),
                    root.clone(),
                ))
                .child(filter_button(
                    workspace_id,
                    state_filter,
                    Some(HistoryState::Pending),
                    es_fluent::localize("history_tab_state_pending", None),
                    root.clone(),
                )),
        )
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(div().text_sm().text_color(gpui::transparent_black()).child(
                    es_fluent::localize("history_tab_protocol_filter_label", None),
                ))
                .child(protocol_filter_button(
                    workspace_id,
                    protocol_filter,
                    HistoryProtocolFilter::All,
                    es_fluent::localize("history_tab_filter_all", None),
                    root.clone(),
                ))
                .child(protocol_filter_button(
                    workspace_id,
                    protocol_filter,
                    HistoryProtocolFilter::Http,
                    es_fluent::localize("history_tab_protocol_http", None),
                    root.clone(),
                ))
                .child(protocol_filter_button(
                    workspace_id,
                    protocol_filter,
                    HistoryProtocolFilter::Graphql,
                    es_fluent::localize("history_tab_protocol_graphql", None),
                    root.clone(),
                ))
                .child(protocol_filter_button(
                    workspace_id,
                    protocol_filter,
                    HistoryProtocolFilter::WebSocket,
                    es_fluent::localize("history_tab_protocol_websocket", None),
                    root.clone(),
                ))
                .child(protocol_filter_button(
                    workspace_id,
                    protocol_filter,
                    HistoryProtocolFilter::Grpc,
                    es_fluent::localize("history_tab_protocol_grpc", None),
                    root.clone(),
                )),
        )
        .child(chip(format!(
            "{}: {}",
            es_fluent::localize("history_tab_total", None),
            entries.len()
        )))
        .children(if entries.is_empty() {
            Some(
                div()
                    .text_color(gpui::transparent_black())
                    .child(es_fluent::localize("history_tab_empty", None))
                    .into_any_element(),
            )
            .into_iter()
            .collect::<Vec<_>>()
        } else {
            entries
                .iter()
                .map(|entry| {
                    let weak_root = root.clone();
                    let request_id = entry.request_id;
                    let meta_row = {
                        let mut row = h_flex()
                            .gap_3()
                            .text_sm()
                            .text_color(gpui::transparent_black())
                            .child(format!(
                                "{}: {}",
                                es_fluent::localize("history_tab_started_at", None),
                                entry.started_at
                            ));
                        if let Some(completed_at) = entry.completed_at {
                            row = row.child(format!(
                                "{}: {}",
                                es_fluent::localize("history_tab_completed_at", None),
                                completed_at
                            ));
                        }
                        row
                    };

                    let mut card = v_flex()
                        .gap_1()
                        .p_3()
                        .rounded(px(6.))
                        .border_1()
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .child(format!(
                                    "[{}] {} {}",
                                    protocol_label(entry),
                                    entry.method,
                                    entry.url
                                ))
                                .child(status_chip(entry.state, entry.status_code)),
                        )
                        .child(meta_row);
                    card = card.child(
                        Button::new(format!("history-restore-request-{}", entry.id))
                            .ghost()
                            .xsmall()
                            .label(es_fluent::localize("history_tab_restore", None))
                            .on_click(move |_, window, cx| {
                                let _ = weak_root.update(cx, |this, cx| {
                                    let Some(request_id) = request_id else {
                                        window.push_notification(
                                            es_fluent::localize(
                                                "history_tab_restore_no_request",
                                                None,
                                            ),
                                            cx,
                                        );
                                        return;
                                    };
                                    let item_key =
                                        crate::session::item_key::ItemKey::request(request_id);
                                    if !this.can_open_item(item_key) {
                                        window.push_notification(
                                            es_fluent::localize(
                                                "history_tab_restore_request_deleted",
                                                None,
                                            ),
                                            cx,
                                        );
                                        return;
                                    }
                                    this.open_item(item_key, cx);
                                });
                            }),
                    );
                    card.into_any_element()
                })
                .collect::<Vec<_>>()
        })
        .child(
            h_flex().justify_center().items_center().child(
                Button::new("history-load-more")
                    .ghost()
                    .xsmall()
                    .disabled(!has_more)
                    .label(if has_more {
                        es_fluent::localize("history_tab_load_more", None)
                    } else {
                        es_fluent::localize("history_tab_no_more", None)
                    })
                    .on_click(move |_, _, cx| {
                        let _ = root.update(cx, |this, cx| {
                            this.load_more_history_for_workspace(workspace_id, cx);
                        });
                    }),
            ),
        )
        .into_any_element()
}

fn chip(label: String) -> impl IntoElement {
    div()
        .px_2()
        .py_1()
        .rounded(px(999.))
        .border_1()
        .child(label)
}

fn status_chip(state: HistoryState, status_code: Option<i64>) -> impl IntoElement {
    let label = match state {
        HistoryState::Pending => es_fluent::localize("history_tab_state_pending", None),
        HistoryState::Completed => status_code.map_or_else(
            || es_fluent::localize("history_tab_state_completed", None),
            |code| {
                format!(
                    "{} ({code})",
                    es_fluent::localize("history_tab_state_completed", None)
                )
            },
        ),
        HistoryState::Failed => es_fluent::localize("history_tab_state_failed", None),
        HistoryState::Cancelled => es_fluent::localize("history_tab_state_cancelled", None),
    };
    div()
        .px_2()
        .py_1()
        .rounded(px(999.))
        .border_1()
        .child(label)
}

fn filter_button(
    workspace_id: WorkspaceId,
    active_filter: Option<HistoryState>,
    button_filter: Option<HistoryState>,
    label: String,
    root: WeakEntity<AppRoot>,
) -> impl IntoElement {
    Button::new(format!("history-filter-{button_filter:?}"))
        .ghost()
        .xsmall()
        .selected(active_filter == button_filter)
        .label(label)
        .on_click(move |_, _, cx| {
            let _ = root.update(cx, |this, cx| {
                this.set_history_state_filter_for_workspace(workspace_id, button_filter, cx);
            });
        })
}

fn protocol_filter_button(
    workspace_id: WorkspaceId,
    active_filter: HistoryProtocolFilter,
    button_filter: HistoryProtocolFilter,
    label: String,
    root: WeakEntity<AppRoot>,
) -> impl IntoElement {
    Button::new(format!("history-protocol-filter-{button_filter:?}"))
        .ghost()
        .xsmall()
        .selected(active_filter == button_filter)
        .label(label)
        .on_click(move |_, _, cx| {
            let _ = root.update(cx, |this, cx| {
                this.set_history_protocol_filter_for_workspace(workspace_id, button_filter, cx);
            });
        })
}

fn protocol_label(entry: &HistoryEntry) -> String {
    match entry.protocol_kind.as_str() {
        "http" => es_fluent::localize("history_tab_protocol_http", None),
        "graphql" => es_fluent::localize("history_tab_protocol_graphql", None),
        "websocket" => es_fluent::localize("history_tab_protocol_websocket", None),
        "grpc" => es_fluent::localize("history_tab_protocol_grpc", None),
        _ => es_fluent::localize("history_tab_protocol_http", None),
    }
}
