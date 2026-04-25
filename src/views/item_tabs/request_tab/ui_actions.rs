use super::*;
use gpui_component::scroll::ScrollableElement as _;

impl RequestTabView {
    pub(super) fn set_active_section(
        &mut self,
        section: RequestSectionTab,
        cx: &mut Context<Self>,
    ) {
        if self.active_section != section {
            tracing::debug!(
                from = ?self.active_section,
                to = ?section,
                "request tab section switch"
            );
            self.active_section = section;
            cx.notify();
        }
    }

    pub(super) fn set_active_response_tab(&mut self, tab: ResponseTab, cx: &mut Context<Self>) {
        if self.active_response_tab != tab {
            self.active_response_tab = tab;
            cx.notify();
        }
    }

    pub(super) fn open_settings_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
        let name_input = self.name_input.clone();
        let timeout_input = self.timeout_input.clone();
        let follow_redirects_input = self.follow_redirects_input.clone();
        let variable_overrides_input = self.variable_overrides_input.clone();

        window.open_dialog(cx, move |dialog, _, cx| {
            let muted = cx.theme().muted_foreground;
            dialog
                .title(es_fluent::localize("request_tab_settings_label", None))
                .overlay_closable(true)
                .keyboard(true)
                .child(
                    v_flex()
                        .gap_3()
                        .child(
                            v_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(es_fluent::localize("request_tab_name_label", None)),
                                )
                                .child(Input::new(&name_input).large()),
                        )
                        .child(
                            v_flex()
                                .gap_2()
                                .child(
                                    div().text_xs().text_color(muted).child(es_fluent::localize(
                                        "request_tab_timeout_label",
                                        None,
                                    )),
                                )
                                .child(Input::new(&timeout_input).large()),
                        )
                        .child(
                            v_flex()
                                .gap_2()
                                .child(div().text_xs().text_color(muted).child(
                                    es_fluent::localize("request_tab_follow_redirects_label", None),
                                ))
                                .child(Input::new(&follow_redirects_input).large()),
                        )
                        .child(
                            v_flex()
                                .gap_2()
                                .child(div().text_xs().text_color(muted).child(
                                    es_fluent::localize(
                                        "request_tab_variable_overrides_label",
                                        None,
                                    ),
                                ))
                                .child(Input::new(&variable_overrides_input).h(px(180.)).large()),
                        ),
                )
                .footer(
                    h_flex().justify_end().child(
                        Button::new("request-settings-close")
                            .primary()
                            .label(es_fluent::localize("request_tab_dirty_close_cancel", None))
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                            }),
                    ),
                )
        });
    }

    pub(super) fn open_history_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request_id) = self.editor.request_id() else {
            window.push_notification(es_fluent::localize("request_tab_history_no_request", None), cx);
            return;
        };

        let services = cx.global::<AppServicesGlobal>().0.clone();
        let entries = match services.repos.history.list_for_request(request_id, 50) {
            Ok(entries) => entries,
            Err(err) => {
                window.push_notification(
                    format!("{}: {err}", es_fluent::localize("request_tab_history_load_failed", None)),
                    cx,
                );
                return;
            }
        };
        let weak_view = cx.entity().downgrade();

        window.open_dialog(cx, move |dialog, _window, cx| {
            let muted = cx.theme().muted_foreground;
            dialog
                .title(es_fluent::localize("request_tab_history_dialog_title", None))
                .overlay_closable(true)
                .keyboard(true)
                .child(
                    div().max_h(px(420.)).overflow_y_scrollbar().child(
                        v_flex()
                            .gap_2()
                            .when(entries.is_empty(), |el| {
                                el.child(
                                    div()
                                        .text_sm()
                                        .text_color(muted)
                                        .child(es_fluent::localize(
                                            "request_tab_history_empty",
                                            None,
                                        )),
                                )
                            })
                            .children(entries.iter().map(|entry| {
                                let weak_view = weak_view.clone();
                                let entry_for_restore = entry.clone();
                                let state_label = match entry.state {
                                    crate::domain::history::HistoryState::Pending => {
                                        es_fluent::localize("history_tab_state_pending", None)
                                    }
                                    crate::domain::history::HistoryState::Completed => {
                                        es_fluent::localize("history_tab_state_completed", None)
                                    }
                                    crate::domain::history::HistoryState::Failed => {
                                        es_fluent::localize("history_tab_state_failed", None)
                                    }
                                    crate::domain::history::HistoryState::Cancelled => {
                                        es_fluent::localize("history_tab_state_cancelled", None)
                                    }
                                };
                                v_flex()
                                    .gap_1()
                                    .p_2()
                                    .rounded(cx.theme().radius)
                                    .border_1()
                                    .border_color(cx.theme().border)
                                    .child(
                                        h_flex()
                                            .justify_between()
                                            .items_center()
                                            .child(format!("{} {}", entry.method, entry.url))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(muted)
                                                    .child(state_label),
                                            ),
                                    )
                                    .child(
                                        h_flex()
                                            .justify_between()
                                            .items_center()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(muted)
                                                    .child(format!(
                                                        "{}: {}",
                                                        es_fluent::localize(
                                                            "history_tab_started_at",
                                                            None,
                                                        ),
                                                        entry.started_at
                                                    )),
                                            )
                                            .child(
                                                Button::new(format!(
                                                    "request-history-restore-{}",
                                                    entry.id
                                                ))
                                                .ghost()
                                                .xsmall()
                                                .label(es_fluent::localize(
                                                    "request_tab_history_restore",
                                                    None,
                                                ))
                                                .on_click(move |_, window, cx| {
                                                    let result = weak_view
                                                        .update(cx, |this, cx| {
                                                            this.restore_from_history_entry(
                                                                entry_for_restore.clone(),
                                                                cx,
                                                            )
                                                        })
                                                        .unwrap_or_else(|_| {
                                                            Err(
                                                                es_fluent::localize(
                                                                    "request_tab_history_restore_failed",
                                                                    None,
                                                                )
                                                                .to_string(),
                                                            )
                                                        });
                                                    match result {
                                                        Ok(()) => {
                                                            window.push_notification(
                                                                es_fluent::localize(
                                                                    "request_tab_history_restore_ok",
                                                                    None,
                                                                ),
                                                                cx,
                                                            );
                                                            window.close_dialog(cx);
                                                        }
                                                        Err(err) => {
                                                            window.push_notification(err, cx);
                                                        }
                                                    }
                                                }),
                                            ),
                                    )
                            })),
                    ),
                )
                .footer(
                    h_flex().justify_end().child(
                        Button::new("request-history-close")
                            .primary()
                            .label(es_fluent::localize("request_tab_dirty_close_cancel", None))
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                            }),
                    ),
                )
        });
    }

    fn restore_from_history_entry(
        &mut self,
        history_entry: crate::domain::history::HistoryEntry,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let services = cx.global::<AppServicesGlobal>().0.clone();
        self.editor.set_latest_history_id(Some(history_entry.id));
        match history_entry.state {
            crate::domain::history::HistoryState::Completed => {
                let body_ref = match (
                    history_entry.blob_hash.as_ref(),
                    history_entry.blob_size.map(|v| v as u64),
                ) {
                    (Some(hash), Some(size_bytes)) => {
                        let preview = services
                            .blob_store
                            .read_preview(hash, crate::domain::response::ResponseBudgets::PREVIEW_CAP_BYTES)
                            .ok()
                            .map(bytes::Bytes::from);
                        crate::domain::response::BodyRef::DiskBlob {
                            blob_id: hash.clone(),
                            preview,
                            size_bytes,
                        }
                    }
                    _ => crate::domain::response::BodyRef::Empty,
                };
                let status_code = history_entry
                    .status_code
                    .unwrap_or(0)
                    .clamp(0, u16::MAX as i64) as u16;
                let status_text = http::StatusCode::from_u16(status_code)
                    .ok()
                    .and_then(|status| status.canonical_reason().map(ToOwned::to_owned))
                    .unwrap_or_default();
                let dispatched_at_unix_ms =
                    history_entry.dispatched_at.map(crate::domain::response::normalize_unix_ms);
                let first_byte_at_unix_ms =
                    history_entry.first_byte_at.map(crate::domain::response::normalize_unix_ms);
                let completed_at_unix_ms =
                    history_entry.completed_at.map(crate::domain::response::normalize_unix_ms);
                let total_ms = match (dispatched_at_unix_ms, completed_at_unix_ms) {
                    (Some(dispatched), Some(completed)) if completed >= dispatched => {
                        Some((completed - dispatched) as u64)
                    }
                    _ => None,
                };
                let ttfb_ms = match (dispatched_at_unix_ms, first_byte_at_unix_ms) {
                    (Some(dispatched), Some(first_byte)) if first_byte >= dispatched => {
                        Some((first_byte - dispatched) as u64)
                    }
                    _ => None,
                };
                let body_decoded_bytes = body_ref.size_bytes();
                let meta_v2 = history_entry
                    .response_meta_v2_json
                    .as_deref()
                    .and_then(|raw| {
                        serde_json::from_str::<crate::domain::response::ResponseMetaV2>(raw).ok()
                    })
                    .unwrap_or_default();

                self.editor.restore_completed_response(crate::domain::response::ResponseSummary {
                    status_code,
                    status_text,
                    headers_json: history_entry.response_headers_json.clone(),
                    media_type: history_entry.response_media_type.clone(),
                    body_ref,
                    total_ms,
                    ttfb_ms,
                    dispatched_at_unix_ms,
                    first_byte_at_unix_ms,
                    completed_at_unix_ms,
                    http_version: meta_v2.http_version,
                    local_addr: meta_v2.local_addr,
                    remote_addr: meta_v2.remote_addr,
                    tls: meta_v2.tls,
                    size: crate::domain::response::ResponseSizeBreakdown {
                        body_decoded_bytes,
                        ..meta_v2.size
                    },
                    request_size: meta_v2.request_size,
                    phase_timings: if meta_v2.phase_timings.ttfb_ms.is_some() {
                        meta_v2.phase_timings
                    } else {
                        crate::domain::response::PhaseTimings {
                            ttfb_ms,
                            ..meta_v2.phase_timings
                        }
                    },
                });
                self.mark_response_tables_dirty();
            }
            crate::domain::history::HistoryState::Failed => {
                self.editor.restore_failed_response(
                    history_entry
                        .error_message
                        .unwrap_or_else(|| es_fluent::localize("request_tab_exec_failed", None)),
                );
            }
            crate::domain::history::HistoryState::Cancelled => {
                self.editor
                    .restore_cancelled_response(history_entry.partial_size.map(|s| s as u64));
            }
            crate::domain::history::HistoryState::Pending => {
                return Err(es_fluent::localize("request_tab_history_restore_pending", None).to_string());
            }
        }
        cx.notify();
        Ok(())
    }
}
