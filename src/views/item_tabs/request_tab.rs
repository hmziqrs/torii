use std::sync::Arc;

use gpui::{prelude::*, *};
use gpui_component::{
    Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex,
};

use crate::{
    domain::{
        ids::WorkspaceId,
        request::{AuthType, BodyType, RequestItem},
        response::BodyRef,
    },
    repos::request_repo::RequestRepoError,
    services::{
        app_services::AppServicesGlobal,
        request_execution::{ExecOutcome, RequestExecutionService},
    },
    session::request_editor_state::{EditorIdentity, ExecStatus, RequestEditorState, SaveStatus},
};

pub struct RequestTabView {
    editor: RequestEditorState,
    name_input: Entity<InputState>,
    method_input: Entity<InputState>,
    url_input: Entity<InputState>,
    body_input: Entity<InputState>,
    pre_request_input: Entity<InputState>,
    tests_input: Entity<InputState>,
    timeout_input: Entity<InputState>,
    follow_redirects_input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl RequestTabView {
    pub fn new(request: &RequestItem, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = RequestEditorState::from_persisted(request.clone());
        Self::build_with_editor(editor, window, cx)
    }

    /// Create a draft request tab for a new unsaved request.
    pub fn new_draft(
        collection_id: crate::domain::ids::CollectionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = RequestEditorState::new_draft(collection_id);
        Self::build_with_editor(editor, window, cx)
    }

    fn build_with_editor(
        editor: RequestEditorState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let initial = editor.draft().clone();

        let name_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(initial.name.clone(), window, cx);
            state
        });
        let method_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(initial.method.clone(), window, cx);
            state
        });
        let url_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(initial.url.clone(), window, cx);
            state
        });
        let body_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(body_editor_value(&initial.body), window, cx);
            state
        });
        let pre_request_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(initial.scripts.pre_request.clone(), window, cx);
            state
        });
        let tests_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(initial.scripts.tests.clone(), window, cx);
            state
        });
        let timeout_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            let value = initial
                .settings
                .timeout_ms
                .map(|v| v.to_string())
                .unwrap_or_default();
            state.set_value(value, window, cx);
            state
        });
        let follow_redirects_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            let value = initial
                .settings
                .follow_redirects
                .map(|v| if v { "true" } else { "false" }.to_string())
                .unwrap_or_default();
            state.set_value(value, window, cx);
            state
        });

        let mut subscriptions = Vec::new();

        subscriptions.push(cx.subscribe(
            &name_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let name = state.read(cx).value().to_string();
                    if this.editor.draft().name != name {
                        this.editor.draft_mut().name = name;
                        this.editor.refresh_save_status();
                        cx.notify();
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe(
            &method_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let method = state.read(cx).value().to_string();
                    if this.editor.draft().method != method {
                        this.editor.draft_mut().method = method;
                        this.editor.refresh_save_status();
                        cx.notify();
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe(
            &url_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let url = state.read(cx).value().to_string();
                    if this.editor.draft().url != url {
                        this.editor.draft_mut().url = url;
                        this.editor.refresh_save_status();
                        cx.notify();
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe(
            &body_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let content = state.read(cx).value().to_string();
                    let draft = this.editor.draft_mut();
                    match &mut draft.body {
                        BodyType::RawText { content: body }
                        | BodyType::RawJson { content: body } => {
                            if *body != content {
                                *body = content;
                                this.editor.refresh_save_status();
                                cx.notify();
                            }
                        }
                        BodyType::None => {
                            if !content.is_empty() {
                                draft.body = BodyType::RawText { content };
                                this.editor.refresh_save_status();
                                cx.notify();
                            }
                        }
                        _ => {}
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe(
            &pre_request_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let text = state.read(cx).value().to_string();
                    if this.editor.draft().scripts.pre_request != text {
                        this.editor.draft_mut().scripts.pre_request = text;
                        this.editor.refresh_save_status();
                        cx.notify();
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe(
            &tests_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let text = state.read(cx).value().to_string();
                    if this.editor.draft().scripts.tests != text {
                        this.editor.draft_mut().scripts.tests = text;
                        this.editor.refresh_save_status();
                        cx.notify();
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe(
            &timeout_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let raw = state.read(cx).value().trim().to_string();
                    let parsed = if raw.is_empty() {
                        None
                    } else {
                        raw.parse::<u64>().ok()
                    };
                    if this.editor.draft().settings.timeout_ms != parsed {
                        this.editor.draft_mut().settings.timeout_ms = parsed;
                        this.editor.refresh_save_status();
                        cx.notify();
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe(
            &follow_redirects_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let raw = state.read(cx).value().trim().to_ascii_lowercase();
                    let parsed = if raw.is_empty() {
                        None
                    } else if raw == "true" || raw == "1" || raw == "yes" {
                        Some(true)
                    } else if raw == "false" || raw == "0" || raw == "no" {
                        Some(false)
                    } else {
                        this.editor.refresh_save_status();
                        cx.notify();
                        return;
                    };
                    if this.editor.draft().settings.follow_redirects != parsed {
                        this.editor.draft_mut().settings.follow_redirects = parsed;
                        this.editor.refresh_save_status();
                        cx.notify();
                    }
                }
            },
        ));

        Self {
            editor,
            name_input,
            method_input,
            url_input,
            body_input,
            pre_request_input,
            tests_input,
            timeout_input,
            follow_redirects_input,
            _subscriptions: subscriptions,
        }
    }

    pub fn editor(&self) -> &RequestEditorState {
        &self.editor
    }

    pub fn editor_mut(&mut self) -> &mut RequestEditorState {
        &mut self.editor
    }

    // -----------------------------------------------------------------------
    // Save
    // -----------------------------------------------------------------------

    pub fn save(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let services = cx.global::<AppServicesGlobal>().0.clone();
        let request = self.editor.draft().clone();
        let expected_revision = self.editor.baseline().map(|b| b.meta.revision).unwrap_or(0);

        self.editor.begin_save();
        cx.notify();

        match services.repos.request.save(&request, expected_revision) {
            Ok(()) => {
                if let Ok(Some(saved)) = services.repos.request.get(request.id) {
                    self.editor.complete_save(&saved);
                } else {
                    self.editor.complete_save(&request);
                }

                if matches!(self.editor.identity(), EditorIdentity::Draft(_)) {
                    self.editor.transition_to_persisted(request.id, &request);
                }

                cx.notify();
                Ok(())
            }
            Err(RequestRepoError::RevisionConflict { expected, actual }) => {
                let msg = format!(
                    "{} ({expected} -> {actual})",
                    es_fluent::localize("request_tab_save_conflict", None)
                );
                self.editor.fail_save(msg.clone());
                cx.notify();
                Err(msg)
            }
            Err(RequestRepoError::NotFound(_id)) => {
                let msg = es_fluent::localize("request_tab_save_not_found", None).to_string();
                self.editor.fail_save(msg.clone());
                cx.notify();
                Err(msg)
            }
            Err(RequestRepoError::Storage(e)) => {
                let msg = format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_save_failed", None)
                );
                self.editor.fail_save(msg.clone());
                cx.notify();
                Err(msg)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Duplicate
    // -----------------------------------------------------------------------

    pub fn duplicate(&mut self, cx: &mut Context<Self>) -> Result<RequestItem, String> {
        let services = cx.global::<AppServicesGlobal>().0.clone();

        let source_id = match self.editor.request_id() {
            Some(id) => id,
            None => {
                return Err(es_fluent::localize("request_tab_duplicate_unsaved", None).to_string());
            }
        };

        let new_name = format!("{} (Copy)", self.editor.draft().name);
        services
            .repos
            .request
            .duplicate(source_id, &new_name)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_duplicate_failed", None)
                )
            })
    }

    // -----------------------------------------------------------------------
    // Send
    // -----------------------------------------------------------------------

    /// Send the current draft request. Auto-cancels any in-flight operation.
    pub fn send(&mut self, cx: &mut Context<Self>) {
        let services = cx.global::<AppServicesGlobal>().0.clone();

        // Determine workspace ID
        let workspace_id = match self.resolve_workspace_id(&services) {
            Some(id) => id,
            None => {
                self.editor.set_preflight_error(
                    es_fluent::localize("request_tab_no_workspace", None).to_string(),
                );
                cx.notify();
                return;
            }
        };

        // Create pending history row
        let draft = self.editor.draft().clone();
        let history_entry = match services.repos.history.create_pending(
            workspace_id,
            self.editor.request_id(),
            &draft.method,
            &draft.url,
        ) {
            Ok(entry) => entry,
            Err(e) => {
                self.editor.set_preflight_error(format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_history_create_failed", None)
                ));
                cx.notify();
                return;
            }
        };

        let operation_id = history_entry.id;

        // Begin send — auto-cancels any in-flight operation
        let old_token = self.editor.begin_send(operation_id);
        if let Some(token) = old_token {
            token.cancel();
        }
        cx.notify();

        // Spawn execution via cx.spawn for async + entity bridge
        let exec_service = build_execution_service(&services);
        let cancel_token = self.editor.cancellation_token().unwrap().clone();
        let history_repo = services.repos.history.clone();
        let blob_store = services.blob_store.clone();
        let io_runtime = services.io_runtime.clone();

        let _ = cx.spawn(async move |this, cx| {
            let request = draft.clone();
            // Spawn the network request on the dedicated I/O runtime (not the GPUI thread)
            let handle = io_runtime.spawn(async move {
                exec_service
                    .execute(&request, workspace_id, cancel_token.clone())
                    .await
            });
            let result = handle
                .await
                .unwrap_or_else(|e| Err(anyhow::anyhow!("task join error: {e}")));

            // Finalize history
            match &result {
                Ok(ExecOutcome::Completed(summary)) => {
                    let headers_json = summary.headers_json.as_deref();
                    let (blob_hash_owned, blob_size) = match &summary.body_ref {
                        BodyRef::DiskBlob {
                            blob_id,
                            size_bytes,
                            ..
                        } => (Some(blob_id.clone()), Some(*size_bytes as i64)),
                        BodyRef::InMemoryPreview { bytes, .. } => {
                            match blob_store.write_bytes(bytes, summary.media_type.as_deref()) {
                                Ok(meta) => (Some(meta.hash), Some(meta.size_bytes as i64)),
                                Err(_) => (None, None),
                            }
                        }
                        BodyRef::Empty => (None, None),
                    };
                    let _ = history_repo.finalize_completed(
                        operation_id,
                        summary.status_code as i64,
                        blob_hash_owned.as_deref(),
                        blob_size,
                        headers_json,
                        summary.media_type.as_deref(),
                        None,
                        None,
                    );
                }
                Ok(ExecOutcome::Failed(error)) => {
                    let _ = history_repo.mark_failed(operation_id, error);
                }
                Ok(ExecOutcome::Cancelled { partial_size }) => {
                    let _ = history_repo
                        .finalize_cancelled(operation_id, partial_size.map(|s| s as i64));
                }
                Ok(ExecOutcome::PreflightFailed(msg)) => {
                    let _ = history_repo.mark_failed(operation_id, msg);
                }
                Err(e) => {
                    let _ = history_repo.mark_failed(operation_id, &e.to_string());
                }
            }

            // Bridge result back to entity
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(ExecOutcome::Completed(summary)) => {
                        this.editor.complete_exec(summary, operation_id);
                    }
                    Ok(ExecOutcome::Failed(error)) => {
                        this.editor.fail_exec(error, operation_id);
                    }
                    Ok(ExecOutcome::Cancelled { partial_size }) => {
                        this.editor.cancel_exec(partial_size, operation_id);
                    }
                    Ok(ExecOutcome::PreflightFailed(msg)) => {
                        this.editor.set_preflight_error(msg);
                    }
                    Err(e) => {
                        this.editor.fail_exec(e.to_string(), operation_id);
                    }
                }
                this.editor.set_latest_history_id(Some(operation_id));
                cx.notify();
            });
        });
    }

    // -----------------------------------------------------------------------
    // Cancel
    // -----------------------------------------------------------------------

    /// Cancel the active send operation.
    pub fn cancel_send(&mut self, cx: &mut Context<Self>) {
        if let Some(token) = self.editor.cancellation_token() {
            token.cancel();
        }
        cx.notify();
    }

    // -----------------------------------------------------------------------
    // Reload baseline
    // -----------------------------------------------------------------------

    pub fn reload_baseline(&mut self, cx: &mut Context<Self>) {
        let services = cx.global::<AppServicesGlobal>().0.clone();
        if let Some(id) = self.editor.request_id() {
            if let Ok(Some(persisted)) = services.repos.request.get(id) {
                self.editor.reload_baseline(persisted);
            }
        }
        cx.notify();
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn resolve_workspace_id(
        &self,
        services: &std::sync::Arc<crate::services::app_services::AppServices>,
    ) -> Option<WorkspaceId> {
        let collection_id = self.editor.draft().collection_id;
        // Walk from collection -> workspace
        services
            .repos
            .collection
            .get(collection_id)
            .ok()
            .flatten()
            .map(|c| c.workspace_id)
    }
}

fn build_execution_service(
    services: &std::sync::Arc<crate::services::app_services::AppServices>,
) -> RequestExecutionService {
    use crate::services::request_execution::ReqwestTransport;

    let transport = Arc::new(ReqwestTransport::new().expect("failed to build HTTP transport"));
    RequestExecutionService::new(
        transport,
        services.repos.history.clone(),
        services.blob_store.clone(),
        services.secret_store.clone(),
    )
}

impl Render for RequestTabView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let request = self.editor.draft();
        let save_status = self.editor.save_status().clone();
        let is_dirty = matches!(
            save_status,
            SaveStatus::Dirty | SaveStatus::SaveFailed { .. }
        );
        let exec_status = self.editor.exec_status();

        let dirty_indicator = if is_dirty {
            div()
                .text_xs()
                .text_color(gpui::red())
                .child(es_fluent::localize("request_tab_dirty", None))
        } else {
            div()
        };

        // Build response panel
        let response_panel = match exec_status {
            ExecStatus::Idle => div().child(
                div()
                    .text_sm()
                    .text_color(gpui::hsla(0., 0., 0.5, 1.))
                    .child(es_fluent::localize("request_tab_response_empty", None)),
            ),
            ExecStatus::Sending => div().child(
                div()
                    .text_sm()
                    .text_color(gpui::hsla(0., 0., 0.5, 1.))
                    .child(es_fluent::localize("request_tab_sending", None)),
            ),
            ExecStatus::Streaming => div().child(
                div()
                    .text_sm()
                    .text_color(gpui::hsla(0., 0., 0.5, 1.))
                    .child(es_fluent::localize("request_tab_streaming", None)),
            ),
            ExecStatus::Completed { response } => {
                let resp = response.as_ref();
                let status_color = if resp.status_code < 400 {
                    gpui::hsla(120. / 360., 0.7, 0.35, 1.)
                } else {
                    gpui::red()
                };

                let body_preview = match &resp.body_ref {
                    BodyRef::Empty => String::new(),
                    BodyRef::InMemoryPreview { bytes, .. } => {
                        render_preview_text(bytes.as_ref(), resp.media_type.as_deref())
                    }
                    BodyRef::DiskBlob {
                        preview,
                        size_bytes,
                        ..
                    } => {
                        let preview_text = preview
                            .as_ref()
                            .map(|b| render_preview_text(b.as_ref(), resp.media_type.as_deref()))
                            .unwrap_or_default();
                        let preview_len = preview.as_ref().map(|b| b.len()).unwrap_or(0) as u64;
                        if *size_bytes > preview_len {
                            format!(
                                "{}\n{}",
                                preview_text,
                                es_fluent::localize("request_tab_response_truncated", None)
                            )
                        } else {
                            preview_text
                        }
                    }
                };

                div()
                    .gap_3()
                    .child(
                        h_flex().gap_3().child(
                            div()
                                .text_sm()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(status_color)
                                .child(format!("{} {}", resp.status_code, resp.status_text)),
                        ),
                    )
                    .when(resp.total_ms.is_some(), |el: gpui::Div| {
                        el.child(
                            div()
                                .text_xs()
                                .text_color(gpui::hsla(0., 0., 0.5, 1.))
                                .child(format!(
                                    "{} {}",
                                    resp.total_ms.unwrap(),
                                    es_fluent::localize("request_tab_ms", None)
                                )),
                        )
                    })
                    .when(resp.ttfb_ms.is_some(), |el: gpui::Div| {
                        el.child(
                            div()
                                .text_xs()
                                .text_color(gpui::hsla(0., 0., 0.5, 1.))
                                .child(format!(
                                    "TTFB: {} {}",
                                    resp.ttfb_ms.unwrap(),
                                    es_fluent::localize("request_tab_ms", None)
                                )),
                        )
                    })
                    .when(!body_preview.is_empty(), |el: gpui::Div| {
                        el.child(
                            div()
                                .mt_2()
                                .p_3()
                                .rounded(px(4.))
                                .bg(gpui::hsla(0., 0., 0.97, 1.))
                                .text_sm()
                                .font_family("monospace")
                                .child(body_preview),
                        )
                    })
            }
            ExecStatus::Failed { error } => {
                div().child(div().text_sm().text_color(gpui::red()).child(format!(
                    "{}: {error}",
                    es_fluent::localize("request_tab_response_failed", None)
                )))
            }
            ExecStatus::Cancelled { partial_size } => {
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
        };

        // Preflight error
        let preflight_panel = match self.editor.preflight_error() {
            Some(err) => div().text_sm().text_color(gpui::red()).child(format!(
                "{}: {}",
                es_fluent::localize("request_tab_preflight", None),
                err.message
            )),
            None => div(),
        };

        let auth_label = auth_type_label(&request.auth);

        v_flex()
            .size_full()
            .p_6()
            .gap_5()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(es_fluent::localize("request_tab_title", None)),
                    )
                    .child(dirty_indicator),
            )
            .child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        Button::new("request-save")
                            .primary()
                            .label(es_fluent::localize("request_tab_action_save", None))
                            .on_click(cx.listener(|this, _, window, cx| match this.save(cx) {
                                Ok(()) => {
                                    window.push_notification(
                                        es_fluent::localize("request_tab_save_ok", None),
                                        cx,
                                    );
                                }
                                Err(err) => {
                                    window.push_notification(err, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("request-duplicate")
                            .outline()
                            .label(es_fluent::localize("request_tab_action_duplicate", None))
                            .on_click(cx.listener(
                                |this, _, window, cx| match this.duplicate(cx) {
                                    Ok(_) => {
                                        window.push_notification(
                                            es_fluent::localize("request_tab_duplicate_ok", None),
                                            cx,
                                        );
                                    }
                                    Err(err) => {
                                        window.push_notification(err, cx);
                                    }
                                },
                            )),
                    )
                    .child(
                        Button::new("request-send")
                            .primary()
                            .label(es_fluent::localize("request_tab_action_send", None))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.send(cx);
                            })),
                    )
                    .child(
                        Button::new("request-cancel")
                            .outline()
                            .label(es_fluent::localize("request_tab_action_cancel", None))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_send(cx);
                            })),
                    )
                    .child(
                        Button::new("request-reload")
                            .ghost()
                            .label(es_fluent::localize("request_tab_action_reload", None))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reload_baseline(cx);
                            })),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .p_3()
                    .rounded(px(6.))
                    .border_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(es_fluent::localize("request_tab_name_label", None)),
                    )
                    .child(Input::new(&self.name_input).large()),
            )
            .child(
                v_flex().gap_2().p_3().rounded(px(6.)).border_1().child(
                    h_flex()
                        .gap_3()
                        .items_end()
                        .child(
                            v_flex()
                                .w_32()
                                .gap_2()
                                .child(
                                    div().text_sm().font_weight(gpui::FontWeight::MEDIUM).child(
                                        es_fluent::localize("request_tab_method_label", None),
                                    ),
                                )
                                .child(Input::new(&self.method_input).large()),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .gap_2()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .child(es_fluent::localize("request_tab_url_label", None)),
                                )
                                .child(Input::new(&self.url_input).large()),
                        ),
                ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .p_3()
                    .rounded(px(6.))
                    .border_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(es_fluent::localize("request_tab_params_label", None)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(gpui::hsla(0., 0., 0.45, 1.))
                            .child(format!(
                                "{}: {}",
                                es_fluent::localize("request_tab_items", None),
                                request.params.len()
                            )),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .p_3()
                    .rounded(px(6.))
                    .border_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(es_fluent::localize("request_tab_auth_label", None)),
                    )
                    .child(div().text_sm().child(auth_label)),
            )
            .child(
                v_flex()
                    .gap_2()
                    .p_3()
                    .rounded(px(6.))
                    .border_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(es_fluent::localize("request_tab_headers_label", None)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(gpui::hsla(0., 0., 0.45, 1.))
                            .child(format!(
                                "{}: {}",
                                es_fluent::localize("request_tab_items", None),
                                request.headers.len()
                            )),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .p_3()
                    .rounded(px(6.))
                    .border_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(es_fluent::localize("request_tab_body_label", None)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(gpui::hsla(0., 0., 0.45, 1.))
                            .child(body_kind_label(&request.body)),
                    )
                    .child(Input::new(&self.body_input).large()),
            )
            .child(
                v_flex()
                    .gap_2()
                    .p_3()
                    .rounded(px(6.))
                    .border_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(es_fluent::localize("request_tab_scripts_label", None)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(gpui::hsla(0., 0., 0.45, 1.))
                            .child(es_fluent::localize("request_tab_pre_request_label", None)),
                    )
                    .child(Input::new(&self.pre_request_input).large())
                    .child(
                        div()
                            .text_xs()
                            .text_color(gpui::hsla(0., 0., 0.45, 1.))
                            .child(es_fluent::localize("request_tab_tests_label", None)),
                    )
                    .child(Input::new(&self.tests_input).large()),
            )
            .child(
                v_flex()
                    .gap_2()
                    .p_3()
                    .rounded(px(6.))
                    .border_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(es_fluent::localize("request_tab_settings_label", None)),
                    )
                    .child(
                        h_flex()
                            .gap_3()
                            .items_end()
                            .child(
                                v_flex()
                                    .w_40()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(gpui::hsla(0., 0., 0.45, 1.))
                                            .child(es_fluent::localize(
                                                "request_tab_timeout_label",
                                                None,
                                            )),
                                    )
                                    .child(Input::new(&self.timeout_input).large()),
                            )
                            .child(
                                v_flex()
                                    .w_40()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(gpui::hsla(0., 0., 0.45, 1.))
                                            .child(es_fluent::localize(
                                                "request_tab_follow_redirects_label",
                                                None,
                                            )),
                                    )
                                    .child(Input::new(&self.follow_redirects_input).large()),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .p_3()
                    .rounded(px(6.))
                    .border_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(es_fluent::localize("request_tab_latest_run_label", None)),
                    )
                    .child(
                        div().text_sm().child(
                            self.editor
                                .latest_history_id()
                                .map(|id| id.to_string())
                                .unwrap_or_else(|| {
                                    es_fluent::localize("request_tab_latest_run_none", None)
                                        .to_string()
                                }),
                        ),
                    ),
            )
            .when(
                matches!(save_status, SaveStatus::SaveFailed { .. }),
                |el: gpui::Div| {
                    if let SaveStatus::SaveFailed { error } = &save_status {
                        el.child(div().text_sm().text_color(gpui::red()).child(error.clone()))
                    } else {
                        el
                    }
                },
            )
            .child(preflight_panel)
            .child(
                v_flex()
                    .gap_2()
                    .p_3()
                    .rounded(px(6.))
                    .border_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(es_fluent::localize("request_tab_response_label", None)),
                    )
                    .child(response_panel),
            )
    }
}

fn body_editor_value(body: &BodyType) -> String {
    match body {
        BodyType::RawText { content } | BodyType::RawJson { content } => content.clone(),
        _ => String::new(),
    }
}

fn body_kind_label(body: &BodyType) -> String {
    match body {
        BodyType::None => es_fluent::localize("request_tab_body_kind_none", None).to_string(),
        BodyType::RawText { .. } => {
            es_fluent::localize("request_tab_body_kind_raw_text", None).to_string()
        }
        BodyType::RawJson { .. } => {
            es_fluent::localize("request_tab_body_kind_raw_json", None).to_string()
        }
        BodyType::UrlEncoded { .. } => {
            es_fluent::localize("request_tab_body_kind_urlencoded", None).to_string()
        }
        BodyType::FormData { .. } => {
            es_fluent::localize("request_tab_body_kind_form_data", None).to_string()
        }
        BodyType::BinaryFile { .. } => {
            es_fluent::localize("request_tab_body_kind_binary_file", None).to_string()
        }
    }
}

fn auth_type_label(auth: &AuthType) -> String {
    match auth {
        AuthType::None => es_fluent::localize("request_tab_auth_none", None).to_string(),
        AuthType::Basic { .. } => es_fluent::localize("request_tab_auth_basic", None).to_string(),
        AuthType::Bearer { .. } => es_fluent::localize("request_tab_auth_bearer", None).to_string(),
        AuthType::ApiKey { .. } => {
            es_fluent::localize("request_tab_auth_api_key", None).to_string()
        }
    }
}

fn render_preview_text(bytes: &[u8], media_type: Option<&str>) -> String {
    let text = String::from_utf8_lossy(bytes).to_string();
    if matches!(media_type, Some(mt) if mt.eq_ignore_ascii_case("application/json")) {
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(value) => serde_json::to_string_pretty(&value).unwrap_or(text),
            Err(_) => text,
        }
    } else {
        text
    }
}
