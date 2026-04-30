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

    pub(super) fn refresh_request_history(&mut self, cx: &mut Context<Self>) {
        let Some(request_id) = self.editor.request_id() else {
            self.request_history_entries.clear();
            self.request_history_error =
                Some(es_fluent::localize("request_tab_history_no_request", None));
            cx.notify();
            return;
        };
        let services = cx.global::<AppServicesGlobal>().0.clone();
        match services.repos.history.list_for_request(request_id, 200) {
            Ok(entries) => {
                self.request_history_entries = entries;
                self.request_history_error = None;
            }
            Err(err) => {
                self.request_history_entries.clear();
                self.request_history_error = Some(format!(
                    "{}: {err}",
                    es_fluent::localize("request_tab_history_load_failed", None)
                ));
            }
        }
        cx.notify();
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
                            .read_preview(
                                hash,
                                crate::domain::response::ResponseBudgets::PREVIEW_CAP_BYTES,
                            )
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
                let dispatched_at_unix_ms = history_entry
                    .dispatched_at
                    .map(crate::domain::response::normalize_unix_ms);
                let first_byte_at_unix_ms = history_entry
                    .first_byte_at
                    .map(crate::domain::response::normalize_unix_ms);
                let completed_at_unix_ms = history_entry
                    .completed_at
                    .map(crate::domain::response::normalize_unix_ms);
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

                self.editor
                    .restore_completed_response(crate::domain::response::ResponseSummary {
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
                return Err(
                    es_fluent::localize("request_tab_history_restore_pending", None).to_string(),
                );
            }
        }
        cx.notify();
        Ok(())
    }

    pub(super) fn render_request_history_section(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_xs().text_color(muted).child(es_fluent::localize(
                        "request_tab_history_dialog_title",
                        None,
                    )))
                    .child(
                        Button::new("request-history-refresh-inline")
                            .ghost()
                            .xsmall()
                            .label(es_fluent::localize("history_tab_refresh", None))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.refresh_request_history(cx);
                            })),
                    ),
            )
            .when_some(self.request_history_error.clone(), |el, err| {
                el.child(div().text_sm().text_color(gpui::red()).child(err))
            })
            .child(
                div()
                    .max_h(px(360.))
                    .overflow_y_scrollbar()
                    .child(v_flex().gap_2().children(
                        self.request_history_entries.iter().cloned().map(|entry| {
                            let state = format!("{:?}", entry.state);
                            let entry_for_restore = entry.clone();
                            let entry_for_compare = entry.clone();
                            let entry_for_details = entry.clone();
                            v_flex()
                                .gap_1()
                                .p_2()
                                .border_1()
                                .rounded(px(6.))
                                .child(format!(
                                    "[{}] {} {}",
                                    entry.protocol_kind, entry.method, entry.url
                                ))
                                .child(div().text_xs().text_color(muted).child(format!(
                                    "{} | {}",
                                    state,
                                    format_history_timestamp(entry.started_at)
                                )))
                                .child(h_flex().gap_2()
                                    .child(
                                        Button::new(format!("request-history-inline-restore-{}", entry.id))
                                            .ghost()
                                            .xsmall()
                                            .label(es_fluent::localize("request_tab_history_restore", None))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                match this.restore_from_history_entry(entry_for_restore.clone(), cx) {
                                                    Ok(()) => window.push_notification(
                                                        es_fluent::localize("request_tab_history_restore_ok", None),
                                                        cx,
                                                    ),
                                                    Err(err) => window.push_notification(err, cx),
                                                }
                                            })),
                                    )
                                    .child(
                                        Button::new(format!("request-history-inline-compare-{}", entry.id))
                                            .ghost()
                                            .xsmall()
                                            .label(es_fluent::localize("history_tab_compare_previous", None))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                let current_ix = this
                                                    .request_history_entries
                                                    .iter()
                                                    .position(|it| it.id == entry_for_compare.id);
                                                let report = current_ix.and_then(|ix| {
                                                    this.request_history_entries
                                                        .iter()
                                                        .skip(ix + 1)
                                                        .find(|candidate| {
                                                            candidate.protocol_kind == entry_for_compare.protocol_kind
                                                                && candidate.method.eq_ignore_ascii_case(&entry_for_compare.method)
                                                                && candidate.url == entry_for_compare.url
                                                        })
                                                        .map(|previous| serde_json::json!({
                                                            "current": {
                                                                "id": entry_for_compare.id.to_string(),
                                                                "protocol": entry_for_compare.protocol_kind,
                                                                "method": entry_for_compare.method,
                                                                "url": entry_for_compare.url,
                                                                "state": format!("{:?}", entry_for_compare.state),
                                                                "status_code": entry_for_compare.status_code,
                                                                "started_at": entry_for_compare.started_at,
                                                            },
                                                            "previous": {
                                                                "id": previous.id.to_string(),
                                                                "protocol": previous.protocol_kind,
                                                                "method": previous.method,
                                                                "url": previous.url,
                                                                "state": format!("{:?}", previous.state),
                                                                "status_code": previous.status_code,
                                                                "started_at": previous.started_at,
                                                            },
                                                        }))
                                                });
                                                let Some(report) = report else {
                                                    window.push_notification(
                                                        es_fluent::localize("history_tab_compare_no_previous", None),
                                                        cx,
                                                    );
                                                    return;
                                                };
                                                let report = serde_json::to_string_pretty(&report)
                                                    .unwrap_or_else(|_| "{}".to_string());
                                                window.open_dialog(cx, move |dialog, _, _| {
                                                    dialog
                                                        .title(es_fluent::localize("history_tab_compare_previous", None))
                                                        .overlay_closable(true)
                                                        .keyboard(true)
                                                        .child(
                                                            div()
                                                                .h(px(360.))
                                                                .overflow_y_scrollbar()
                                                                .child(div().text_xs().font_family("monospace").child(report.clone())),
                                                        )
                                                        .footer(
                                                            h_flex().justify_end().child(
                                                                Button::new("request-history-inline-compare-close")
                                                                    .primary()
                                                                    .label(es_fluent::localize("history_tab_dialog_cancel", None))
                                                                    .on_click(move |_, window, cx| {
                                                                        window.close_dialog(cx);
                                                                    }),
                                                            ),
                                                        )
                                                });
                                            })),
                                    )
                                    .child(
                                        Button::new(format!("request-history-inline-details-{}", entry.id))
                                            .ghost()
                                            .xsmall()
                                            .label(es_fluent::localize("history_tab_details", None))
                                            .on_click(cx.listener(move |_, _, window, cx| {
                                                let raw = serde_json::to_string_pretty(&entry_for_details)
                                                    .unwrap_or_else(|_| "{}".to_string());
                                                window.open_dialog(cx, move |dialog, _, _| {
                                                    dialog
                                                        .title(es_fluent::localize("history_tab_details", None))
                                                        .overlay_closable(true)
                                                        .keyboard(true)
                                                        .child(
                                                            div()
                                                                .h(px(360.))
                                                                .overflow_y_scrollbar()
                                                                .child(div().text_xs().font_family("monospace").child(raw.clone())),
                                                        )
                                                        .footer(
                                                            h_flex().justify_end().child(
                                                                Button::new("request-history-inline-details-close")
                                                                    .primary()
                                                                    .label(es_fluent::localize("history_tab_dialog_cancel", None))
                                                                    .on_click(move |_, window, cx| {
                                                                        window.close_dialog(cx);
                                                                    }),
                                                            ),
                                                        )
                                                });
                                            })),
                                    )
                                )
                        }),
                    )),
            )
            .into_any_element()
    }
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
