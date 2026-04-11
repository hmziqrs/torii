use std::sync::Arc;

use gpui::{prelude::*, *};
use gpui_component::{
    h_flex,
    input::{Input, InputState},
    Sizable as _,
    v_flex,
};

use crate::{
    domain::{
        ids::WorkspaceId,
        request::RequestItem,
        response::BodyRef,
    },
    infra::secrets::SecretStoreRef,
    repos::request_repo::RequestRepoError,
    services::{
        app_services::AppServicesGlobal,
        request_execution::{ExecOutcome, RequestExecutionService},
    },
    session::request_editor_state::{EditorIdentity, ExecStatus, RequestEditorState, SaveStatus},
};

pub struct RequestTabView {
    editor: RequestEditorState,
    url_input: Entity<InputState>,
}

impl RequestTabView {
    pub fn new(request: &RequestItem, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = RequestEditorState::from_persisted(request.clone());
        let url_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(request.url.clone(), window, cx);
            state
        });

        Self { editor, url_input }
    }

    /// Create a draft request tab for a new unsaved request.
    pub fn new_draft(
        collection_id: crate::domain::ids::CollectionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = RequestEditorState::new_draft(collection_id);
        let url_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(String::new(), window, cx);
            state
        });

        Self { editor, url_input }
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
                let msg = format!("Save conflict: expected revision {expected}, but current is {actual}");
                self.editor.fail_save(msg.clone());
                cx.notify();
                Err(msg)
            }
            Err(RequestRepoError::NotFound(id)) => {
                let msg = format!("Request {id} no longer exists");
                self.editor.fail_save(msg.clone());
                cx.notify();
                Err(msg)
            }
            Err(RequestRepoError::Storage(e)) => {
                let msg = format!("Save failed: {e}");
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
            None => return Err("Cannot duplicate an unsaved draft".to_string()),
        };

        let new_name = format!("{} (Copy)", self.editor.draft().name);
        services
            .repos
            .request
            .duplicate(source_id, &new_name)
            .map_err(|e| format!("Duplicate failed: {e}"))
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
                self.editor.set_preflight_error("No workspace context available".to_string());
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
                self.editor.set_preflight_error(format!("Failed to create history entry: {e}"));
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
        let db = services.db.clone();

        cx.spawn(async move |this, cx| {
            let request = draft.clone();
            // Run the network request on the database tokio runtime
            let result = db.block_on(async {
                exec_service.execute(&request, workspace_id, cancel_token.clone()).await
            });

            // Finalize history
            match &result {
                Ok(ExecOutcome::Completed(summary)) => {
                    let headers_json = summary.headers_json.as_deref();
                    let blob_hash;
                    let blob_size;
                    let owned_hash;

                    match &summary.body_ref {
                        BodyRef::DiskBlob { blob_id, size_bytes, .. } => {
                            owned_hash = blob_id.clone();
                            blob_hash = Some(owned_hash.as_str());
                            blob_size = Some(*size_bytes as i64);
                        }
                        BodyRef::InMemoryPreview { bytes, .. } => {
                            match blob_store.write_bytes(bytes, summary.media_type.as_deref()) {
                                Ok(meta) => {
                                    owned_hash = meta.hash.clone();
                                    blob_hash = Some(owned_hash.as_str());
                                    blob_size = Some(meta.size_bytes as i64);
                                }
                                Err(_) => {
                                    owned_hash = String::new();
                                    blob_hash = None;
                                    blob_size = None;
                                }
                            }
                        }
                        BodyRef::Empty => {
                            owned_hash = String::new();
                            blob_hash = None;
                            blob_size = None;
                        }
                    };
                    let _ = history_repo.finalize_completed(
                        operation_id,
                        summary.status_code as i64,
                        blob_hash,
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
                    let _ = history_repo.finalize_cancelled(
                        operation_id,
                        partial_size.map(|s| s as i64),
                    );
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let request = self.editor.draft();
        let save_status = self.editor.save_status().clone();
        let is_dirty = matches!(save_status, SaveStatus::Dirty | SaveStatus::SaveFailed { .. });
        let exec_status = self.editor.exec_status();

        // Sync URL input with editor state
        let url = request.url.clone();
        self.url_input.update(cx, |state, cx| {
            state.set_value(url, window, cx);
        });

        let dirty_indicator = if is_dirty {
            div().text_xs().text_color(gpui::red()).child(" *")
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
                        String::from_utf8_lossy(bytes).to_string()
                    }
                    BodyRef::DiskBlob { preview, size_bytes, .. } => {
                        let preview_text = preview
                            .as_ref()
                            .map(|b| String::from_utf8_lossy(b).to_string())
                            .unwrap_or_default();
                        let preview_len = preview.as_ref().map(|b| b.len()).unwrap_or(0) as u64;
                        if *size_bytes > preview_len {
                            format!("{preview_text}\n... (truncated, {size_bytes} bytes total)")
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
                            div().text_xs().text_color(gpui::hsla(0., 0., 0.5, 1.)).child(
                                format!("{}ms", resp.total_ms.unwrap()),
                            ),
                        )
                    })
                    .when(resp.ttfb_ms.is_some(), |el: gpui::Div| {
                        el.child(
                            div().text_xs().text_color(gpui::hsla(0., 0., 0.5, 1.)).child(
                                format!("TTFB: {}ms", resp.ttfb_ms.unwrap()),
                            ),
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
            ExecStatus::Failed { error } => div().child(
                div()
                    .text_sm()
                    .text_color(gpui::red())
                    .child(format!("Error: {error}")),
            ),
            ExecStatus::Cancelled { partial_size } => {
                let msg = match partial_size {
                    Some(size) => format!("Cancelled ({} bytes received)", size),
                    None => "Cancelled".to_string(),
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
            Some(err) => div()
                .text_sm()
                .text_color(gpui::red())
                .child(format!("Preflight: {}", err.message)),
            None => div(),
        };

        v_flex()
            .size_full()
            .p_6()
            .gap_5()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(request.name.clone()),
                    )
                    .child(dirty_indicator),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(chip(request.method.clone()))
                    .child(chip(request.url.clone())),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(es_fluent::localize("request_tab_url_label", None)),
                    )
                    .child(Input::new(&self.url_input).large()),
            )
            .when(
                matches!(save_status, SaveStatus::SaveFailed { .. }),
                |el: gpui::Div| {
                    if let SaveStatus::SaveFailed { error } = &save_status {
                        el.child(
                            div()
                                .text_sm()
                                .text_color(gpui::red())
                                .child(error.clone()),
                        )
                    } else {
                        el
                    }
                },
            )
            .child(preflight_panel)
            .child(response_panel)
    }
}

fn chip(label: String) -> impl IntoElement {
    div().px_2().py_1().rounded(px(999.)).border_1().child(label)
}
