use gpui::{
    AnyElement, IntoElement, ParentElement, Styled as _, WeakEntity, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::{
    Disableable as _, Selectable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
    scroll::ScrollableElement as _,
};

use crate::{
    domain::{
        history::{HistoryEntry, HistoryState, StatusFamily},
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
        || view.search.is_some()
        || view.status_family_filter.is_some()
        || view.started_after.is_some()
        || view.started_before.is_some();
    let grouped = group_entries(entries, view.group_by);
    let weak_root_search = root.clone();
    let weak_root_method = root.clone();
    let weak_root_url = root.clone();
    let weak_root_clear = root.clone();
    let weak_root_load_more = root.clone();
    let weak_root_load_more_bottom = root.clone();
    let weak_root_after = root.clone();
    let weak_root_before = root.clone();

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
        )
        .child(
            Button::new("history-open-started-after-dialog")
                .ghost()
                .xsmall()
                .label(es_fluent::localize("history_tab_started_after", None))
                .on_click(move |_, window, cx| {
                    let _ = weak_root_after.update(cx, |this, cx| {
                        this.open_history_started_after_dialog(workspace_id, window, cx);
                    });
                }),
        )
        .child(
            Button::new("history-open-started-before-dialog")
                .ghost()
                .xsmall()
                .label(es_fluent::localize("history_tab_started_before", None))
                .on_click(move |_, window, cx| {
                    let _ = weak_root_before.update(cx, |this, cx| {
                        this.open_history_started_before_dialog(workspace_id, window, cx);
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
                    es_fluent::localize("history_tab_status_family_filter_label", None),
                ))
                .child(status_family_filter_button(
                    workspace_id,
                    view.status_family_filter,
                    None,
                    es_fluent::localize("history_tab_filter_all", None),
                    root.clone(),
                ))
                .child(status_family_filter_button(
                    workspace_id,
                    view.status_family_filter,
                    Some(StatusFamily::Informational),
                    es_fluent::localize("history_tab_status_family_1xx", None),
                    root.clone(),
                ))
                .child(status_family_filter_button(
                    workspace_id,
                    view.status_family_filter,
                    Some(StatusFamily::Success),
                    es_fluent::localize("history_tab_status_family_2xx", None),
                    root.clone(),
                ))
                .child(status_family_filter_button(
                    workspace_id,
                    view.status_family_filter,
                    Some(StatusFamily::Redirection),
                    es_fluent::localize("history_tab_status_family_3xx", None),
                    root.clone(),
                ))
                .child(status_family_filter_button(
                    workspace_id,
                    view.status_family_filter,
                    Some(StatusFamily::ClientError),
                    es_fluent::localize("history_tab_status_family_4xx", None),
                    root.clone(),
                ))
                .child(status_family_filter_button(
                    workspace_id,
                    view.status_family_filter,
                    Some(StatusFamily::ServerError),
                    es_fluent::localize("history_tab_status_family_5xx", None),
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
        .children(
            active_filter_chips(view)
                .into_iter()
                .map(|label| chip(label).into_any_element()),
        )
        .children(history_rows_elements(&grouped, has_filters, root.clone()))
        .child(
            h_flex().justify_center().child(
                Button::new("history-load-more-bottom")
                    .ghost()
                    .xsmall()
                    .disabled(!has_more)
                    .label(if has_more {
                        es_fluent::localize("history_tab_load_more", None)
                    } else {
                        es_fluent::localize("history_tab_no_more", None)
                    })
                    .on_click(move |_, _, cx| {
                        let _ = weak_root_load_more_bottom.update(cx, |this, cx| {
                            this.load_more_history_for_workspace(workspace_id, cx);
                        });
                    }),
            ),
        )
        .into_any_element()
}

fn history_rows_elements(
    grouped: &[(String, Vec<HistoryEntry>)],
    has_filters: bool,
    root: WeakEntity<AppRoot>,
) -> Vec<AnyElement> {
    if grouped.is_empty() {
        return vec![
            div()
                .text_color(gpui::transparent_black())
                .child(if has_filters {
                    es_fluent::localize("history_tab_no_results", None)
                } else {
                    es_fluent::localize("history_tab_empty", None)
                })
                .into_any_element(),
        ];
    }

    let mut elements = Vec::new();
    for (group_title, rows) in grouped {
        elements.push(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(gpui::transparent_black())
                .child(group_title.clone())
                .into_any_element(),
        );

        for entry in rows {
            let entry = entry.clone();
            let weak_root_open = root.clone();
            let weak_root_details = root.clone();
            let weak_root_restore = root.clone();
            let weak_root_compare = root.clone();
            let request_id = entry.request_id;
            let entry_id = entry.id;
            let details_entry = entry.clone();
            let entry_for_restore = entry.clone();
            let entry_for_compare = entry.clone();
            let meta_row = {
                let mut row = h_flex()
                    .gap_3()
                    .text_sm()
                    .text_color(gpui::transparent_black())
                    .child(format!(
                        "{}: {}",
                        es_fluent::localize("history_tab_started_at", None),
                        format_history_timestamp(entry.started_at)
                    ));
                if let Some(completed_at) = entry.completed_at {
                    row = row.child(format!(
                        "{}: {}",
                        es_fluent::localize("history_tab_completed_at", None),
                        format_history_timestamp(completed_at)
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
                            protocol_label(&entry),
                            entry.method,
                            entry.url
                        ))
                        .child(status_chip(entry.state, entry.status_code)),
                )
                .child(meta_row);
            card = card.child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new(format!("history-compare-request-{entry_id}"))
                            .ghost()
                            .xsmall()
                            .label(es_fluent::localize("history_tab_compare_previous", None))
                            .on_click(move |_, window, cx| {
                                match weak_root_compare.update(cx, |this, cx| {
                                    this.compare_history_entry_with_previous(&entry_for_compare, cx)
                                }) {
                                    Ok(Ok(report)) => open_history_compare_dialog(report, window, cx),
                                    Ok(Err(err)) => window.push_notification(err, cx),
                                    Err(_) => window.push_notification(
                                        es_fluent::localize("history_tab_compare_failed", None),
                                        cx,
                                    ),
                                }
                            }),
                    )
                    .child(
                        Button::new(format!("history-select-entry-{entry_id}"))
                            .ghost()
                            .xsmall()
                            .label(es_fluent::localize("history_tab_details", None))
                            .on_click(move |_, window, cx| {
                                open_history_details_dialog(
                                    details_entry.clone(),
                                    weak_root_details.clone(),
                                    window,
                                    cx,
                                );
                            }),
                    )
                    .child(
                        Button::new(format!("history-open-request-{entry_id}"))
                            .ghost()
                            .xsmall()
                            .disabled(request_id.is_none())
                            .label(es_fluent::localize("history_tab_open_request", None))
                            .on_click(move |_, window, cx| {
                                let _ = weak_root_open.update(cx, |this, cx| {
                                    let Some(request_id) = request_id else {
                                        window.push_notification(
                                            es_fluent::localize(
                                                "history_tab_open_request_no_request",
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
                                                "history_tab_open_request_deleted",
                                                None,
                                            ),
                                            cx,
                                        );
                                        return;
                                    }
                                    this.open_item(item_key, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new(format!("history-restore-request-{entry_id}"))
                            .ghost()
                            .xsmall()
                            .label(es_fluent::localize("history_tab_restore", None))
                            .on_click(move |_, window, cx| {
                                match weak_root_restore.update(cx, |this, cx| {
                                    this.restore_history_entry(
                                        entry_for_restore.clone(),
                                        window,
                                        cx,
                                    )
                                }) {
                                    Ok(Ok(created_draft)) => {
                                        if created_draft {
                                            window.push_notification(
                                                es_fluent::localize(
                                                    "history_tab_restore_draft_created",
                                                    None,
                                                ),
                                                cx,
                                            );
                                        }
                                    }
                                    Ok(Err(err)) => window.push_notification(err, cx),
                                    Err(_) => window.push_notification(
                                        es_fluent::localize(
                                            "history_tab_restore_draft_failed",
                                            None,
                                        ),
                                        cx,
                                    ),
                                }
                            }),
                    ),
            );
            elements.push(card.into_any_element());
        }
    }
    elements
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

fn status_family_filter_button(
    workspace_id: WorkspaceId,
    active_filter: Option<StatusFamily>,
    button_filter: Option<StatusFamily>,
    label: String,
    root: WeakEntity<AppRoot>,
) -> impl IntoElement {
    Button::new(format!("history-status-family-filter-{button_filter:?}"))
        .ghost()
        .xsmall()
        .selected(active_filter == button_filter)
        .label(label)
        .on_click(move |_, _, cx| {
            let _ = root.update(cx, |this, cx| {
                this.set_history_status_family_filter_for_workspace(
                    workspace_id,
                    button_filter,
                    cx,
                );
            });
        })
}

fn active_filter_chips(view: &HistoryWorkspaceView) -> Vec<String> {
    let mut chips = Vec::new();
    if let Some(state) = view.state_filter {
        let label = match state {
            HistoryState::Pending => es_fluent::localize("history_tab_state_pending", None),
            HistoryState::Completed => es_fluent::localize("history_tab_state_completed", None),
            HistoryState::Failed => es_fluent::localize("history_tab_state_failed", None),
            HistoryState::Cancelled => es_fluent::localize("history_tab_state_cancelled", None),
        };
        chips.push(format!(
            "{}: {}",
            es_fluent::localize("history_tab_filter_label", None),
            label
        ));
    }
    if view.protocol_filter != HistoryProtocolFilter::All {
        let label = match view.protocol_filter {
            HistoryProtocolFilter::All => es_fluent::localize("history_tab_filter_all", None),
            HistoryProtocolFilter::Http => es_fluent::localize("history_tab_protocol_http", None),
            HistoryProtocolFilter::Graphql => {
                es_fluent::localize("history_tab_protocol_graphql", None)
            }
            HistoryProtocolFilter::WebSocket => {
                es_fluent::localize("history_tab_protocol_websocket", None)
            }
            HistoryProtocolFilter::Grpc => es_fluent::localize("history_tab_protocol_grpc", None),
        };
        chips.push(format!(
            "{}: {}",
            es_fluent::localize("history_tab_protocol_filter_label", None),
            label
        ));
    }
    if let Some(method) = &view.method_filter {
        chips.push(format!(
            "{}: {method}",
            es_fluent::localize("history_tab_method_filter", None)
        ));
    }
    if let Some(url) = &view.url_search {
        chips.push(format!(
            "{}: {url}",
            es_fluent::localize("history_tab_url_filter", None)
        ));
    }
    if let Some(search) = &view.search {
        chips.push(format!(
            "{}: {search}",
            es_fluent::localize("history_tab_search", None)
        ));
    }
    if let Some(status_family) = view.status_family_filter {
        chips.push(format!(
            "{}: {}",
            es_fluent::localize("history_tab_status_family_filter_label", None),
            status_family_label(match status_family {
                StatusFamily::Informational => Some(100),
                StatusFamily::Success => Some(200),
                StatusFamily::Redirection => Some(300),
                StatusFamily::ClientError => Some(400),
                StatusFamily::ServerError => Some(500),
            })
        ));
    }
    if let Some(after) = view.started_after {
        chips.push(format!(
            "{}: {}",
            es_fluent::localize("history_tab_started_after", None),
            format_filter_time(after)
        ));
    }
    if let Some(before) = view.started_before {
        chips.push(format!(
            "{}: {}",
            es_fluent::localize("history_tab_started_before", None),
            format_filter_time(before)
        ));
    }
    chips
}

fn format_filter_time(raw: i64) -> String {
    format_history_timestamp(raw)
}

fn format_history_timestamp(raw: i64) -> String {
    let ms = crate::domain::response::normalize_unix_ms(raw);
    let seconds = ms / 1000;
    let nanos = ((ms % 1000) * 1_000_000) as u32;
    chrono::DateTime::from_timestamp(seconds, nanos)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| raw.to_string())
}

fn format_bytes_i64(size: i64) -> String {
    if size <= 0 {
        return "0 B".to_string();
    }
    let size = size as f64;
    if size < 1024.0 {
        format!("{:.0} B", size)
    } else if size < (1024.0 * 1024.0) {
        format!("{:.1} KB", size / 1024.0)
    } else {
        format!("{:.2} MB", size / (1024.0 * 1024.0))
    }
}

fn parse_optional_json(value: &Option<String>) -> Option<String> {
    let raw = value.as_ref()?;
    let parsed = serde_json::from_str::<serde_json::Value>(raw).ok()?;
    serde_json::to_string_pretty(&parsed).ok()
}

fn open_history_details_dialog(
    entry: HistoryEntry,
    root: WeakEntity<AppRoot>,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) {
    let weak_root_open = root.clone();
    let weak_root_restore = root.clone();
    let weak_root_compare = root.clone();
    let entry_for_restore = entry.clone();
    let entry_for_compare = entry.clone();
    let request_id = entry.request_id;
    let url = entry.url.clone();
    let details_protocol = protocol_label(&entry);
    let details_method = entry.method.clone();
    let details_url = entry.url.clone();
    let details_started = entry.started_at;
    let details_completed = entry.completed_at;
    let details_state = entry.state;
    let details_request_name = entry.request_name.clone();
    let details_close_reason = entry.close_reason.clone();
    let details_message_count_in = entry.message_count_in;
    let details_message_count_out = entry.message_count_out;
    let details_run_summary = parse_optional_json(&entry.run_summary_json);
    let details_request_snapshot = parse_optional_json(&entry.request_snapshot_json);
    let details_transcript_size = entry.transcript_size;
    let details_transcript_blob_hash = entry.transcript_blob_hash.clone();
    let entry_for_copy = entry.clone();

    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title(es_fluent::localize("history_tab_details", None))
            .overlay_closable(true)
            .keyboard(true)
            .child(
                v_flex()
                    .gap_2()
                    .child(format!(
                        "{}: {}",
                        es_fluent::localize("history_tab_protocol_filter_label", None),
                        details_protocol
                    ))
                    .child(format!(
                        "{}: {}",
                        es_fluent::localize("history_tab_method_filter", None),
                        details_method
                    ))
                    .child(format!(
                        "{}: {}",
                        es_fluent::localize("history_tab_url_filter", None),
                        details_url
                    ))
                    .when_some(details_request_name.clone(), |el, request_name| {
                        el.child(format!(
                            "{}: {}",
                            es_fluent::localize("history_tab_request_name", None),
                            request_name
                        ))
                    })
                    .child(format!(
                        "{}: {}",
                        es_fluent::localize("history_tab_started_at", None),
                        format_history_timestamp(details_started)
                    ))
                    .when_some(details_completed, |el, completed| {
                        el.child(format!(
                            "{}: {}",
                            es_fluent::localize("history_tab_completed_at", None),
                            format_history_timestamp(completed)
                        ))
                    })
                    .child(format!(
                        "{}: {}",
                        es_fluent::localize("history_tab_filter_label", None),
                        match details_state {
                            HistoryState::Pending => {
                                es_fluent::localize("history_tab_state_pending", None)
                            }
                            HistoryState::Completed => {
                                es_fluent::localize("history_tab_state_completed", None)
                            }
                            HistoryState::Failed => {
                                es_fluent::localize("history_tab_state_failed", None)
                            }
                            HistoryState::Cancelled => {
                                es_fluent::localize("history_tab_state_cancelled", None)
                            }
                        }
                    ))
                    .when_some(details_message_count_in, |el, count| {
                        el.child(format!(
                            "{}: {count}",
                            es_fluent::localize("history_tab_message_count_in", None)
                        ))
                    })
                    .when_some(details_message_count_out, |el, count| {
                        el.child(format!(
                            "{}: {count}",
                            es_fluent::localize("history_tab_message_count_out", None)
                        ))
                    })
                    .when_some(details_close_reason.clone(), |el, reason| {
                        el.child(format!(
                            "{}: {}",
                            es_fluent::localize("history_tab_close_reason", None),
                            reason
                        ))
                    })
                    .when_some(details_transcript_size, |el, size| {
                        el.child(format!(
                            "{}: {}",
                            es_fluent::localize("history_tab_transcript_size", None),
                            format_bytes_i64(size)
                        ))
                    })
                    .when_some(details_transcript_blob_hash.clone(), |el, blob_hash| {
                        el.child(format!(
                            "{}: {}",
                            es_fluent::localize("history_tab_transcript_blob_hash", None),
                            blob_hash
                        ))
                    })
                    .when_some(details_run_summary.clone(), |el, run_summary| {
                        el.child(format!(
                            "{}:\n{}",
                            es_fluent::localize("history_tab_run_summary", None),
                            run_summary
                        ))
                    })
                    .when_some(details_request_snapshot.clone(), |el, request_snapshot| {
                        el.child(format!(
                            "{}:\n{}",
                            es_fluent::localize("history_tab_request_snapshot", None),
                            request_snapshot
                        ))
                    }),
            )
            .footer(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child({
                        let weak_root_open = weak_root_open.clone();
                        let request_id = request_id.clone();
                        Button::new(format!("history-details-open-request-{}", entry.id))
                            .ghost()
                            .xsmall()
                            .disabled(request_id.is_none())
                            .label(es_fluent::localize("history_tab_open_request", None))
                            .on_click(move |_, window, cx| {
                                let _ = weak_root_open.update(cx, |this, cx| {
                                    let Some(request_id) = request_id else {
                                        window.push_notification(
                                            es_fluent::localize(
                                                "history_tab_open_request_no_request",
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
                                                "history_tab_open_request_deleted",
                                                None,
                                            ),
                                            cx,
                                        );
                                        return;
                                    }
                                    this.open_item(item_key, cx);
                                });
                            })
                    })
                    .child({
                        let weak_root_restore = weak_root_restore.clone();
                        let entry_for_restore = entry_for_restore.clone();
                        Button::new(format!("history-details-restore-request-{}", entry.id))
                            .ghost()
                            .xsmall()
                            .label(es_fluent::localize("history_tab_restore", None))
                            .on_click(move |_, window, cx| {
                                match weak_root_restore.update(cx, |this, cx| {
                                    this.restore_history_entry(
                                        entry_for_restore.clone(),
                                        window,
                                        cx,
                                    )
                                }) {
                                    Ok(Ok(created_draft)) => {
                                        if created_draft {
                                            window.push_notification(
                                                es_fluent::localize(
                                                    "history_tab_restore_draft_created",
                                                    None,
                                                ),
                                                cx,
                                            );
                                        }
                                    }
                                    Ok(Err(err)) => window.push_notification(err, cx),
                                    Err(_) => window.push_notification(
                                        es_fluent::localize(
                                            "history_tab_restore_draft_failed",
                                            None,
                                        ),
                                        cx,
                                    ),
                                }
                            })
                    })
                    .child({
                        let weak_root_compare = weak_root_compare.clone();
                        let entry_for_compare = entry_for_compare.clone();
                        Button::new(format!("history-details-compare-{}", entry.id))
                            .ghost()
                            .xsmall()
                            .label(es_fluent::localize("history_tab_compare_previous", None))
                            .on_click(move |_, window, cx| {
                                match weak_root_compare.update(cx, |this, cx| {
                                    this.compare_history_entry_with_previous(&entry_for_compare, cx)
                                }) {
                                    Ok(Ok(report)) => {
                                        open_history_compare_dialog(report, window, cx);
                                    }
                                    Ok(Err(err)) => window.push_notification(err, cx),
                                    Err(_) => window.push_notification(
                                        es_fluent::localize("history_tab_compare_failed", None),
                                        cx,
                                    ),
                                }
                            })
                    })
                    .child({
                        let entry_for_copy = entry_for_copy.clone();
                        Button::new(format!("history-details-copy-json-{}", entry.id))
                            .ghost()
                            .xsmall()
                            .label(es_fluent::localize("history_tab_copy_details_json", None))
                            .on_click(move |_, window, cx| {
                                match serde_json::to_string_pretty(&entry_for_copy) {
                                    Ok(json) => {
                                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                            json,
                                        ));
                                        window.push_notification(
                                            es_fluent::localize("request_tab_copy_ok", None),
                                            cx,
                                        );
                                    }
                                    Err(err) => window.push_notification(
                                        format!(
                                            "{}: {err}",
                                            es_fluent::localize(
                                                "history_tab_copy_details_json_failed",
                                                None,
                                            )
                                        ),
                                        cx,
                                    ),
                                }
                            })
                    })
                    .child({
                        let url = url.clone();
                        Button::new(format!("history-details-copy-url-{}", entry.id))
                            .ghost()
                            .xsmall()
                            .label(es_fluent::localize("history_tab_copy_url", None))
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(url.clone()));
                            })
                    })
                    .child(
                        Button::new(format!("history-details-close-{}", entry.id))
                            .primary()
                            .xsmall()
                            .label(es_fluent::localize("history_tab_dialog_cancel", None))
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                            }),
                    ),
            )
    });
}

fn open_history_compare_dialog(report: String, window: &mut gpui::Window, cx: &mut gpui::App) {
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title(es_fluent::localize("history_tab_compare_previous", None))
            .overlay_closable(true)
            .keyboard(true)
            .child(
                div().max_h(px(420.)).overflow_y_scrollbar().child(
                    div()
                        .text_xs()
                        .font_family("monospace")
                        .child(report.clone()),
                ),
            )
            .footer(
                h_flex().justify_end().child(
                    Button::new("history-compare-close")
                        .primary()
                        .xsmall()
                        .label(es_fluent::localize("history_tab_dialog_cancel", None))
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                        }),
                ),
            )
    });
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

#[cfg(test)]
mod tests {
    use super::{active_filter_chips, format_bytes_i64, format_history_timestamp, parse_optional_json};
    use crate::root::{HistoryProtocolFilter, HistoryWorkspaceView};
    use crate::domain::history::HistoryState;

    #[test]
    fn format_history_timestamp_is_human_readable() {
        let result = format_history_timestamp(1_800_000_000_000);
        assert!(result.contains('-'));
        assert!(result.contains(':'));
    }

    #[test]
    fn format_bytes_i64_scales() {
        assert_eq!(format_bytes_i64(10), "10 B");
        assert_eq!(format_bytes_i64(2048), "2.0 KB");
    }

    #[test]
    fn parse_optional_json_handles_invalid_and_valid() {
        assert!(parse_optional_json(&Some("invalid".to_string())).is_none());
        let pretty = parse_optional_json(&Some("{\"k\":1}".to_string())).expect("pretty json");
        assert!(pretty.contains("\"k\": 1"));
    }

    #[test]
    fn active_filter_chips_include_state_and_protocol() {
        let mut view = HistoryWorkspaceView::default();
        view.state_filter = Some(HistoryState::Failed);
        view.protocol_filter = HistoryProtocolFilter::Graphql;
        let chips = active_filter_chips(&view);
        assert!(chips.iter().any(|chip| chip.contains("Failed")));
        assert!(chips.iter().any(|chip| chip.contains("GraphQL")));
    }
}
