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
    root::{AppRoot, HistoryGroupBy, HistoryProtocolFilter, HistoryWorkspaceView},
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
    let has_filters = state_filter.is_some()
        || protocol_filter != HistoryProtocolFilter::All
        || view.method_filter.is_some()
        || view.url_search.is_some()
        || view.search.is_some();
    let grouped = group_entries(entries, view.group_by);
    let weak_root_search = root.clone();
    let weak_root_method = root.clone();
    let weak_root_url = root.clone();
    let weak_root_clear = root.clone();
    let weak_root_load_more = root.clone();

    let mut quick_filter_row = h_flex()
        .gap_2()
        .items_center()
        .child(
            Button::new("history-open-search-dialog")
                .ghost()
                .xsmall()
                .label(es_fluent::localize("history_tab_search", None))
                .on_click(move |_, window, cx| {
                    let _ = weak_root_search.update(cx, |this, cx| {
                        this.open_history_search_dialog(workspace_id, window, cx);
                    });
                }),
        )
        .child(
            Button::new("history-open-method-filter-dialog")
                .ghost()
                .xsmall()
                .label(es_fluent::localize("history_tab_method_filter", None))
                .on_click(move |_, window, cx| {
                    let _ = weak_root_method.update(cx, |this, cx| {
                        this.open_history_method_filter_dialog(workspace_id, window, cx);
                    });
                }),
        )
        .child(
            Button::new("history-open-url-filter-dialog")
                .ghost()
                .xsmall()
                .label(es_fluent::localize("history_tab_url_filter", None))
                .on_click(move |_, window, cx| {
                    let _ = weak_root_url.update(cx, |this, cx| {
                        this.open_history_url_filter_dialog(workspace_id, window, cx);
                    });
                }),
        );
    if has_filters {
        quick_filter_row = quick_filter_row.child(
            Button::new("history-clear-filters")
                .ghost()
                .xsmall()
                .label(es_fluent::localize("history_tab_clear_filters", None))
                .on_click(move |_, _, cx| {
                    let _ = weak_root_clear.update(cx, |this, cx| {
                        this.clear_history_filters_for_workspace(workspace_id, cx);
                    });
                }),
        );
    }

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
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            Button::new("history-refresh")
                                .label(es_fluent::localize("history_tab_refresh", None))
                                .on_click(move |_, _, cx| {
                                    let _ = weak_root_refresh.update(cx, |this, cx| {
                                        this.refresh_history_for_workspace(workspace_id, cx);
                                    });
                                }),
                        )
                        .child(
                            Button::new("history-load-more-top")
                                .ghost()
                                .xsmall()
                                .disabled(!has_more)
                                .label(if has_more {
                                    es_fluent::localize("history_tab_load_more", None)
                                } else {
                                    es_fluent::localize("history_tab_no_more", None)
                                })
                                .on_click(move |_, _, cx| {
                                    let _ = weak_root_load_more.update(cx, |this, cx| {
                                        this.load_more_history_for_workspace(workspace_id, cx);
                                    });
                                }),
                        ),
                ),
        )
        .child(quick_filter_row)
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
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(gpui::transparent_black())
                        .child(es_fluent::localize("history_tab_group_by_label", None)),
                )
                .child(group_button(
                    workspace_id,
                    view.group_by,
                    HistoryGroupBy::None,
                    es_fluent::localize("history_tab_group_none", None),
                    root.clone(),
                ))
                .child(group_button(
                    workspace_id,
                    view.group_by,
                    HistoryGroupBy::Date,
                    es_fluent::localize("history_tab_group_date", None),
                    root.clone(),
                ))
                .child(group_button(
                    workspace_id,
                    view.group_by,
                    HistoryGroupBy::Protocol,
                    es_fluent::localize("history_tab_group_protocol", None),
                    root.clone(),
                ))
                .child(group_button(
                    workspace_id,
                    view.group_by,
                    HistoryGroupBy::StatusFamily,
                    es_fluent::localize("history_tab_group_status_family", None),
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
                    .child(if has_filters {
                        es_fluent::localize("history_tab_no_results", None)
                    } else {
                        es_fluent::localize("history_tab_empty", None)
                    })
                    .into_any_element(),
            )
            .into_iter()
            .collect::<Vec<_>>()
        } else {
            grouped
                .iter()
                .flat_map(|(group_title, rows)| {
                    let group_header = Some(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(gpui::transparent_black())
                            .child(group_title.clone())
                            .into_any_element(),
                    );
                    let group_rows = rows.iter().map(|entry| {
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
                    });
                    group_header
                        .into_iter()
                        .chain(group_rows)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
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

fn group_button(
    workspace_id: WorkspaceId,
    active_group: HistoryGroupBy,
    button_group: HistoryGroupBy,
    label: String,
    root: WeakEntity<AppRoot>,
) -> impl IntoElement {
    Button::new(format!("history-group-by-{button_group:?}"))
        .ghost()
        .xsmall()
        .selected(active_group == button_group)
        .label(label)
        .on_click(move |_, _, cx| {
            let _ = root.update(cx, |this, cx| {
                this.set_history_group_by_for_workspace(workspace_id, button_group, cx);
            });
        })
}

fn group_entries(
    entries: &[HistoryEntry],
    group_by: HistoryGroupBy,
) -> Vec<(String, Vec<HistoryEntry>)> {
    use std::collections::BTreeMap;

    let mut map: BTreeMap<String, Vec<HistoryEntry>> = BTreeMap::new();
    for entry in entries {
        let key = match group_by {
            HistoryGroupBy::None => es_fluent::localize("history_tab_group_none", None),
            HistoryGroupBy::Date => {
                let ms = crate::domain::response::normalize_unix_ms(entry.started_at);
                let seconds = ms / 1000;
                if let Ok(dt) = time::OffsetDateTime::from_unix_timestamp(seconds) {
                    dt.date().to_string()
                } else {
                    "unknown-date".to_string()
                }
            }
            HistoryGroupBy::Protocol => protocol_label(entry),
            HistoryGroupBy::StatusFamily => status_family_label(entry.status_code),
        };
        map.entry(key).or_default().push(entry.clone());
    }
    let mut grouped: Vec<(String, Vec<HistoryEntry>)> = map.into_iter().collect();
    if group_by == HistoryGroupBy::Date {
        grouped.reverse();
    }
    grouped
}

fn status_family_label(status_code: Option<i64>) -> String {
    match status_code {
        Some(code) if (100..=199).contains(&code) => {
            es_fluent::localize("history_tab_status_family_1xx", None)
        }
        Some(code) if (200..=299).contains(&code) => {
            es_fluent::localize("history_tab_status_family_2xx", None)
        }
        Some(code) if (300..=399).contains(&code) => {
            es_fluent::localize("history_tab_status_family_3xx", None)
        }
        Some(code) if (400..=499).contains(&code) => {
            es_fluent::localize("history_tab_status_family_4xx", None)
        }
        Some(code) if (500..=599).contains(&code) => {
            es_fluent::localize("history_tab_status_family_5xx", None)
        }
        _ => es_fluent::localize("history_tab_status_family_unknown", None),
    }
}
