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
        app_services::{AppServices, AppServicesGlobal},
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
    loaded_full_body_blob_id: Option<String>,
    loaded_full_body_text: Option<String>,
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
            loaded_full_body_blob_id: None,
            loaded_full_body_text: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn editor(&self) -> &RequestEditorState {
        &self.editor
    }

    pub fn editor_mut(&mut self) -> &mut RequestEditorState {
        &mut self.editor
    }

    pub fn has_unsaved_changes(&self) -> bool {
        matches!(
            self.editor.save_status(),
            SaveStatus::Dirty | SaveStatus::SaveFailed { .. } | SaveStatus::Saving
        ) || self.editor.detect_dirty()
    }

    // -----------------------------------------------------------------------
    // Save
    // -----------------------------------------------------------------------

    pub fn save(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let services = cx.global::<AppServicesGlobal>().0.clone();
        let mut request = self.editor.draft().clone();
        let expected_revision = self.editor.baseline().map(|b| b.meta.revision).unwrap_or(0);

        self.persist_request_body_blob(&mut request, &services)?;
        self.normalize_auth_secret_ownership_for_save(&mut request, &services)?;

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

        // Duplicate from persisted baseline (not dirty in-memory draft).
        let source = services
            .repos
            .request
            .get(source_id)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_duplicate_failed", None)
                )
            })?
            .ok_or_else(|| es_fluent::localize("request_tab_save_not_found", None).to_string())?;

        let new_name = format!("{} (Copy)", source.name);
        let mut duplicate = services
            .repos
            .request
            .duplicate(source_id, &new_name)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_duplicate_failed", None)
                )
            })?;

        if let Err(err) = self.clone_auth_secrets_for_duplicate(&source, &mut duplicate, &services)
        {
            let _ = services.repos.request.delete(duplicate.id);
            return Err(err);
        }

        if let Err(err) = services
            .repos
            .request
            .save(&duplicate, duplicate.meta.revision)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_duplicate_failed", None)
                )
            })
        {
            let _ = services.repos.request.delete(duplicate.id);
            return Err(err);
        }

        services
            .repos
            .request
            .get(duplicate.id)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_duplicate_failed", None)
                )
            })?
            .ok_or_else(|| es_fluent::localize("request_tab_duplicate_failed", None).to_string())
    }

    // -----------------------------------------------------------------------
    // Send
    // -----------------------------------------------------------------------

    /// Send the current draft request. Auto-cancels any in-flight operation.
    pub fn send(&mut self, cx: &mut Context<Self>) {
        let services = cx.global::<AppServicesGlobal>().0.clone();
        self.loaded_full_body_blob_id = None;
        self.loaded_full_body_text = None;

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

        let exec_service = build_execution_service(&services);
        let cancel_token = self.editor.cancellation_token().unwrap().clone();
        let history_repo = services.repos.history.clone();
        let blob_store = services.blob_store.clone();
        let io_runtime = services.io_runtime.clone();

        let _ = cx.spawn(async move |this, cx| {
            let request = draft.clone();
            let handle = io_runtime.spawn(async move {
                exec_service
                    .execute(&request, workspace_id, cancel_token.clone())
                    .await
            });
            let result = handle
                .await
                .unwrap_or_else(|e| Err(anyhow::anyhow!("task join error: {e}")));

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

            let _ = this.update(cx, |this, cx| {
                this.loaded_full_body_blob_id = None;
                this.loaded_full_body_text = None;
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

    pub fn load_full_response_body(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let services = cx.global::<AppServicesGlobal>().0.clone();

        let (blob_id, media_type) = match self.editor.exec_status() {
            ExecStatus::Completed { response } => match &response.body_ref {
                BodyRef::DiskBlob { blob_id, .. } => (blob_id.clone(), response.media_type.clone()),
                _ => {
                    return Err(
                        es_fluent::localize("request_tab_full_body_unavailable", None).to_string(),
                    );
                }
            },
            _ => {
                return Err(
                    es_fluent::localize("request_tab_full_body_unavailable", None).to_string(),
                );
            }
        };

        let bytes = services.blob_store.read_all(&blob_id).map_err(|e| {
            format!(
                "{}: {e}",
                es_fluent::localize("request_tab_full_body_load_failed", None)
            )
        })?;

        self.loaded_full_body_text = Some(render_preview_text(&bytes, media_type.as_deref()));
        self.loaded_full_body_blob_id = Some(blob_id);
        cx.notify();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn resolve_workspace_id(
        &self,
        services: &std::sync::Arc<crate::services::app_services::AppServices>,
    ) -> Option<WorkspaceId> {
        let collection_id = self.editor.draft().collection_id;
        services
            .repos
            .collection
            .get(collection_id)
            .ok()
            .flatten()
            .map(|c| c.workspace_id)
    }

    fn persist_request_body_blob(
        &self,
        request: &mut RequestItem,
        services: &Arc<AppServices>,
    ) -> Result<(), String> {
        match &request.body {
            BodyType::RawText { content } | BodyType::RawJson { content } => {
                let media = match &request.body {
                    BodyType::RawJson { .. } => Some("application/json"),
                    _ => Some("text/plain"),
                };
                let blob = services
                    .blob_store
                    .write_bytes(content.as_bytes(), media)
                    .map_err(|e| {
                        format!(
                            "{}: {e}",
                            es_fluent::localize("request_tab_save_failed", None)
                        )
                    })?;
                request.body_blob_hash = Some(blob.hash);
            }
            _ => {
                request.body_blob_hash = None;
            }
        }
        Ok(())
    }

    fn normalize_auth_secret_ownership_for_save(
        &self,
        request: &mut RequestItem,
        services: &Arc<AppServices>,
    ) -> Result<(), String> {
        let target_owner_kind = "request";
        let target_owner_id = request.id.to_string();

        let source_owner = match self.editor.identity() {
            EditorIdentity::Draft(draft_id) => Some(("request_draft", draft_id.to_string())),
            EditorIdentity::Persisted(id) => Some(("request", id.to_string())),
        };

        match &mut request.auth {
            AuthType::None => Ok(()),
            AuthType::Basic {
                password_secret_ref,
                ..
            } => self.rebind_secret_ref(
                password_secret_ref,
                "basic_password",
                source_owner.as_ref().map(|(k, v)| (*k, v.as_str())),
                target_owner_kind,
                &target_owner_id,
                services,
            ),
            AuthType::Bearer { token_secret_ref } => self.rebind_secret_ref(
                token_secret_ref,
                "bearer_token",
                source_owner.as_ref().map(|(k, v)| (*k, v.as_str())),
                target_owner_kind,
                &target_owner_id,
                services,
            ),
            AuthType::ApiKey {
                value_secret_ref, ..
            } => self.rebind_secret_ref(
                value_secret_ref,
                "api_key_value",
                source_owner.as_ref().map(|(k, v)| (*k, v.as_str())),
                target_owner_kind,
                &target_owner_id,
                services,
            ),
        }
    }

    fn rebind_secret_ref(
        &self,
        slot: &mut Option<String>,
        secret_kind: &str,
        source_owner: Option<(&str, &str)>,
        target_owner_kind: &str,
        target_owner_id: &str,
        services: &Arc<AppServices>,
    ) -> Result<(), String> {
        let Some(current_ref) = slot.clone() else {
            return Ok(());
        };

        let value = services
            .secret_store
            .get_secret(&current_ref)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_save_failed", None)
                )
            })?
            .ok_or_else(|| es_fluent::localize("request_tab_secret_missing", None).to_string())?;

        let new_ref = services
            .secret_manager
            .upsert_secret(target_owner_kind, target_owner_id, secret_kind, &value)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_save_failed", None)
                )
            })?;

        *slot = Some(new_ref.key_name.clone());

        if let Some((owner_kind, owner_id)) = source_owner {
            if owner_kind == "request_draft" {
                let _ = services
                    .secret_manager
                    .delete_secret(owner_kind, owner_id, secret_kind);
                if current_ref != new_ref.key_name {
                    let _ = services.secret_store.delete_secret(&current_ref);
                }
            }
        }

        Ok(())
    }

    fn clone_auth_secrets_for_duplicate(
        &self,
        source: &RequestItem,
        duplicate: &mut RequestItem,
        services: &Arc<AppServices>,
    ) -> Result<(), String> {
        let target_owner_id = duplicate.id.to_string();

        match (&source.auth, &mut duplicate.auth) {
            (
                AuthType::Basic {
                    password_secret_ref: src,
                    ..
                },
                AuthType::Basic {
                    password_secret_ref: dst,
                    ..
                },
            ) => self.clone_one_secret(
                src.as_ref(),
                dst,
                "basic_password",
                &target_owner_id,
                services,
            ),
            (
                AuthType::Bearer {
                    token_secret_ref: src,
                },
                AuthType::Bearer {
                    token_secret_ref: dst,
                },
            ) => self.clone_one_secret(
                src.as_ref(),
                dst,
                "bearer_token",
                &target_owner_id,
                services,
            ),
            (
                AuthType::ApiKey {
                    value_secret_ref: src,
                    ..
                },
                AuthType::ApiKey {
                    value_secret_ref: dst,
                    ..
                },
            ) => self.clone_one_secret(
                src.as_ref(),
                dst,
                "api_key_value",
                &target_owner_id,
                services,
            ),
            _ => Ok(()),
        }
    }

    fn clone_one_secret(
        &self,
        source_ref: Option<&String>,
        destination_ref: &mut Option<String>,
        secret_kind: &str,
        target_owner_id: &str,
        services: &Arc<AppServices>,
    ) -> Result<(), String> {
        let Some(source_ref) = source_ref else {
            *destination_ref = None;
            return Ok(());
        };

        let value = services
            .secret_store
            .get_secret(source_ref)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_duplicate_failed", None)
                )
            })?
            .ok_or_else(|| es_fluent::localize("request_tab_secret_missing", None).to_string())?;

        let new_ref = services
            .secret_manager
            .upsert_secret("request", target_owner_id, secret_kind, &value)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_duplicate_failed", None)
                )
            })?;

        *destination_ref = Some(new_ref.key_name);
        Ok(())
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
            SaveStatus::Dirty | SaveStatus::SaveFailed { .. } | SaveStatus::Saving
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

                let mut body_preview = match &resp.body_ref {
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

                let load_full_button = match &resp.body_ref {
                    BodyRef::DiskBlob { blob_id, .. } => {
                        if self.loaded_full_body_blob_id.as_deref() == Some(blob_id.as_str()) {
                            if let Some(full) = &self.loaded_full_body_text {
                                body_preview = full.clone();
                            }
                            div()
                        } else {
                            div().child(
                                Button::new("request-load-full-body")
                                    .outline()
                                    .label(es_fluent::localize(
                                        "request_tab_action_load_full_body",
                                        None,
                                    ))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if let Err(err) = this.load_full_response_body(cx) {
                                            window.push_notification(err, cx);
                                        }
                                    })),
                            )
                        }
                    }
                    _ => div(),
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
                    .child(load_full_button)
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
