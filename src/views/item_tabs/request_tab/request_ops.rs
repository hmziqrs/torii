use super::*;
use crate::services::request_draft::persist_new_draft_request;
use crate::{
    domain::{collection::CollectionStorageKind, revision::now_unix_ts},
    infra::linked_collection_format::{
        LinkedCollectionState, LinkedSiblingId, read_linked_collection, write_linked_collection,
    },
};
use anyhow::anyhow;

const UNTITLED_REQUEST_NAME: &str = "Untitled Request";

impl RequestTabView {
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
        let is_draft_identity = matches!(self.editor.identity(), EditorIdentity::Draft(_));
        let expected_revision = self.editor.baseline().map(|b| b.meta.revision).unwrap_or(0);

        self.persist_request_body_blob(&mut request, &services)?;
        self.normalize_auth_secret_ownership_for_save(&mut request, &services)?;

        self.editor.begin_save();
        cx.notify();

        let collection = services
            .repos
            .collection
            .get(request.collection_id)
            .map_err(|e| RequestRepoError::Storage(anyhow!("failed to load collection: {e}")))
            .and_then(|collection| {
                collection.ok_or(RequestRepoError::Storage(anyhow!(
                    "collection no longer exists"
                )))
            });

        let save_result = match collection {
            Ok(collection) if collection.storage_kind == CollectionStorageKind::Linked => {
                self.save_linked_request(&request, expected_revision, is_draft_identity, &services)
            }
            Ok(_) => {
                if is_draft_identity {
                    persist_new_draft_request(&services.repos.request, &request)
                } else {
                    services.repos.request.save(&request, expected_revision)
                }
            }
            Err(err) => Err(err),
        };

        match save_result {
            Ok(saved) => {
                self.editor.complete_save(&saved);

                if is_draft_identity {
                    self.editor.transition_to_persisted(saved.id, &saved);
                }

                cx.notify();
                Ok(())
            }
            Err(RequestRepoError::RevisionConflict { expected, actual }) => {
                let msg = format!(
                    "{} ({expected} -> {actual})",
                    es_fluent::localize("request_tab_save_conflict", None)
                );
                tracing::warn!(expected, actual, "save: revision conflict");
                self.editor.fail_save(msg.clone());
                cx.notify();
                Err(msg)
            }
            Err(RequestRepoError::NotFound(_id)) => {
                let msg = es_fluent::localize("request_tab_save_not_found", None).to_string();
                tracing::warn!(request_id = ?self.editor.request_id(), "save: not found");
                self.editor.fail_save(msg.clone());
                cx.notify();
                Err(msg)
            }
            Err(RequestRepoError::Storage(e)) => {
                let msg = format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_save_failed", None)
                );
                tracing::error!(error = %e, "save: storage error");
                self.editor.fail_save(msg.clone());
                cx.notify();
                Err(msg)
            }
        }
    }

    fn save_linked_request(
        &self,
        request: &RequestItem,
        expected_revision: i64,
        is_draft_identity: bool,
        services: &AppServices,
    ) -> Result<RequestItem, RequestRepoError> {
        let collection = services
            .repos
            .collection
            .get(request.collection_id)
            .map_err(|e| RequestRepoError::Storage(anyhow!("failed to load collection: {e}")))?
            .ok_or_else(|| RequestRepoError::Storage(anyhow!("collection no longer exists")))?;
        let root_path = collection
            .storage_config
            .linked_root_path
            .clone()
            .ok_or_else(|| {
                RequestRepoError::Storage(anyhow!("linked collection missing root path"))
            })?;

        let mut state = read_linked_collection(&root_path, &collection).map_err(|e| {
            RequestRepoError::Storage(anyhow!("failed to read linked collection: {e}"))
        })?;

        ensure_parent_exists(&state, request.parent_folder_id)?;

        let saved = if is_draft_identity {
            let mut saved = request.clone();
            saved.id = crate::domain::ids::RequestId::new();
            saved.collection_id = collection.id;
            let sibling_names = state
                .requests
                .iter()
                .filter(|request| request.parent_folder_id == saved.parent_folder_id)
                .map(|request| request.name.clone())
                .collect::<Vec<_>>();
            if saved.name == UNTITLED_REQUEST_NAME {
                saved.name = next_postman_style_name(UNTITLED_REQUEST_NAME, &sibling_names);
            }
            saved.sort_order = next_linked_request_sort(
                &state.folders,
                &state.requests,
                saved.parent_folder_id,
                None,
            );
            saved.meta = crate::domain::revision::RevisionMetadata::new_now();
            attach_request_order(&mut state, saved.id, saved.parent_folder_id);
            state.requests.push(saved.clone());
            saved
        } else {
            let Some(idx) = state
                .requests
                .iter()
                .position(|candidate| candidate.id == request.id)
            else {
                return Err(RequestRepoError::NotFound(request.id));
            };
            let existing = state.requests[idx].clone();
            if existing.meta.revision != expected_revision {
                return Err(RequestRepoError::RevisionConflict {
                    expected: expected_revision,
                    actual: existing.meta.revision,
                });
            }
            ensure_parent_exists(&state, request.parent_folder_id)?;

            let mut saved = request.clone();
            saved.collection_id = collection.id;
            saved.meta.created_at = existing.meta.created_at;
            saved.meta.updated_at = now_unix_ts();
            saved.meta.revision = existing.meta.revision + 1;

            if saved.parent_folder_id != existing.parent_folder_id {
                detach_request_order(&mut state, existing.id, existing.parent_folder_id);
                attach_request_order(&mut state, saved.id, saved.parent_folder_id);
                saved.sort_order = next_linked_request_sort(
                    &state.folders,
                    &state.requests,
                    saved.parent_folder_id,
                    Some(saved.id),
                );
            } else {
                saved.sort_order = existing.sort_order;
            }

            state.requests[idx] = saved.clone();
            saved
        };

        write_linked_collection(&root_path, &state).map_err(|e| {
            RequestRepoError::Storage(anyhow!("failed to write linked request state: {e}"))
        })?;
        Ok(saved)
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
        let source_collection = services
            .repos
            .collection
            .get(self.editor.draft().collection_id)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_duplicate_failed", None)
                )
            })?
            .ok_or_else(|| es_fluent::localize("request_tab_save_not_found", None).to_string())?;

        let (source, mut duplicate) =
            if source_collection.storage_kind == CollectionStorageKind::Linked {
                let root_path = source_collection
                    .storage_config
                    .linked_root_path
                    .clone()
                    .ok_or_else(|| {
                        format!(
                            "{}: linked collection missing root path",
                            es_fluent::localize("request_tab_duplicate_failed", None)
                        )
                    })?;
                let mut state =
                    read_linked_collection(&root_path, &source_collection).map_err(|e| {
                        format!(
                            "{}: {e}",
                            es_fluent::localize("request_tab_duplicate_failed", None)
                        )
                    })?;
                let source = state
                    .requests
                    .iter()
                    .find(|request| request.id == source_id)
                    .cloned()
                    .ok_or_else(|| {
                        es_fluent::localize("request_tab_save_not_found", None).to_string()
                    })?;

                let sibling_names = state
                    .requests
                    .iter()
                    .filter(|request| request.parent_folder_id == source.parent_folder_id)
                    .map(|request| request.name.clone())
                    .collect::<Vec<_>>();
                let mut duplicate = source.clone();
                duplicate.id = crate::domain::ids::RequestId::new();
                duplicate.name = next_duplicate_request_name(&source.name, &sibling_names);
                duplicate.sort_order = next_linked_request_sort(
                    &state.folders,
                    &state.requests,
                    duplicate.parent_folder_id,
                    None,
                );
                duplicate.meta = crate::domain::revision::RevisionMetadata::new_now();

                if let Err(err) =
                    self.clone_auth_secrets_for_duplicate(&source, &mut duplicate, &services)
                {
                    return Err(err);
                }

                attach_request_order(&mut state, duplicate.id, duplicate.parent_folder_id);
                state.requests.push(duplicate.clone());
                write_linked_collection(&root_path, &state).map_err(|e| {
                    format!(
                        "{}: {e}",
                        es_fluent::localize("request_tab_duplicate_failed", None)
                    )
                })?;
                return Ok(duplicate);
            } else {
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
                    .ok_or_else(|| {
                        es_fluent::localize("request_tab_save_not_found", None).to_string()
                    })?;

                let sibling_names = services
                    .repos
                    .request
                    .list_by_collection(source.collection_id)
                    .map_err(|e| {
                        format!(
                            "{}: {e}",
                            es_fluent::localize("request_tab_duplicate_failed", None)
                        )
                    })?
                    .into_iter()
                    .filter(|request| request.parent_folder_id == source.parent_folder_id)
                    .map(|request| request.name)
                    .collect::<Vec<_>>();

                let new_name = next_duplicate_request_name(&source.name, &sibling_names);
                let duplicate = services
                    .repos
                    .request
                    .duplicate(source_id, &new_name)
                    .map_err(|e| {
                        format!(
                            "{}: {e}",
                            es_fluent::localize("request_tab_duplicate_failed", None)
                        )
                    })?;
                (source, duplicate)
            };

        if let Err(err) = self.clone_auth_secrets_for_duplicate(&source, &mut duplicate, &services)
        {
            let _ = services.repos.request.delete(duplicate.id);
            return Err(err);
        }

        duplicate = match services
            .repos
            .request
            .save(&duplicate, duplicate.meta.revision)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_duplicate_failed", None)
                )
            }) {
            Ok(saved) => saved,
            Err(err) => {
                let _ = services.repos.request.delete(duplicate.id);
                return Err(err);
            }
        };

        Ok(duplicate)
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
                tracing::warn!(request_id = ?self.editor.request_id(), "send: no workspace id resolved");
                self.editor.set_preflight_error(
                    es_fluent::localize("request_tab_no_workspace", None).to_string(),
                );
                cx.notify();
                return;
            }
        };

        // Create pending history row with secret-safe snapshot
        let draft = self.editor.draft().clone();
        let active_environment_id = services
            .active_environments_by_workspace
            .read()
            .ok()
            .and_then(|map| map.get(&workspace_id).copied());
        let resolved_request = match services.variable_resolution.resolve_request(
            &draft,
            workspace_id,
            active_environment_id,
        ) {
            Ok(resolved) => resolved,
            Err(e) => {
                tracing::warn!(error = %e, "preflight rejected: variable resolution failed");
                self.editor.set_preflight_error(e.to_string());
                cx.notify();
                return;
            }
        };
        tracing::debug!(
            method = %resolved_request.method,
            url = %resolved_request.url,
            "send"
        );
        let history_entry = match services.request_execution.create_pending_history(
            workspace_id,
            self.editor.request_id(),
            &resolved_request,
        ) {
            Ok(entry) => entry,
            Err(e) => {
                tracing::error!(error = %e, request_id = ?self.editor.request_id(), "send: failed to create history entry");
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

        let exec_service = services.request_execution.clone();
        let Some(cancel_token) = self.editor.cancellation_token().cloned() else {
            self.editor.set_preflight_error(
                es_fluent::localize("request_tab_preflight", None).to_string(),
            );
            cx.notify();
            return;
        };
        let io_runtime = services.io_runtime.clone();
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<ExecProgressEvent>(1);

        cx.spawn(async move |this, cx| {
            while let Some(event) = progress_rx.recv().await {
                if let Err(err) = this.update(cx, |this, cx| {
                    if this.editor.active_operation_id() != Some(operation_id) {
                        return;
                    }
                    match event {
                        ExecProgressEvent::ResponseStreamingStarted => {
                            this.editor.transition_to_streaming();
                            cx.notify();
                        }
                    }
                }) {
                    tracing::warn!(error = %err, "failed to update request tab for streaming progress");
                    telemetry::inc_async_update_failures("dropped_entity");
                    break; // entity dropped — stop polling
                }
            }
        })
        .detach();

        cx.spawn(async move |this, cx| {
            let request = resolved_request.clone();
            let exec_service_for_task = exec_service.clone();
            let handle = io_runtime.spawn(async move {
                exec_service_for_task
                    .execute_with_progress(
                        &request,
                        workspace_id,
                        cancel_token.clone(),
                        Some(progress_tx),
                    )
                    .await
            });
            let result = handle
                .await
                .unwrap_or_else(|e| Err(anyhow::anyhow!("task join error: {e}")));

            exec_service.finalize_history(operation_id, &result);

            if let Err(err) = this.update(cx, |this, cx| {
                this.loaded_full_body_blob_id = None;
                this.loaded_full_body_text = None;
                match result {
                    Ok(ExecOutcome::Completed(summary)) => {
                        if this.editor.complete_exec(summary, operation_id) {
                            this.response_tables_dirty = true;
                        } else {
                            tracing::warn!(
                                op_id = %operation_id,
                                "late response ignored — operation no longer active"
                            );
                        }
                    }
                    Ok(ExecOutcome::Failed {
                        summary,
                        classified,
                    }) => {
                        if !this.editor.fail_exec(summary, classified, operation_id) {
                            tracing::warn!(
                                op_id = %operation_id,
                                "late failure ignored — operation no longer active"
                            );
                        }
                    }
                    Ok(ExecOutcome::Cancelled { partial_size }) => {
                        if !this.editor.cancel_exec(partial_size, operation_id) {
                            tracing::warn!(
                                op_id = %operation_id,
                                "late cancel ignored — operation no longer active"
                            );
                        }
                    }
                    Ok(ExecOutcome::PreflightFailed(msg)) => {
                        this.editor.reset_preflight();
                        this.editor.set_preflight_error(msg);
                    }
                    Err(e) => {
                        this.editor.fail_exec(e.to_string(), None, operation_id);
                    }
                }
                this.editor.set_latest_history_id(Some(operation_id));
                cx.notify();
            }) {
                tracing::warn!(error = %err, "failed to update request tab for terminal execution state");
                telemetry::inc_async_update_failures("dropped_entity");
            }
        })
        .detach();
    }

    // -----------------------------------------------------------------------
    // Cancel
    // -----------------------------------------------------------------------

    /// Cancel the active send operation.
    pub fn cancel_send(&mut self, cx: &mut Context<Self>) {
        let _span = tracing::info_span!("request.cancel").entered();
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

        let preview_bytes = self.current_preview_bytes();
        let available_for_full_body =
            ResponseBudgets::PER_TAB_CAP_BYTES.saturating_sub(preview_bytes);
        let (capped, was_truncated) = truncate_for_tab_cap(bytes, available_for_full_body);
        let mut text = render_preview_text(&capped, media_type.as_deref());
        if was_truncated {
            text.push('\n');
            text.push_str(&es_fluent::localize("request_tab_response_truncated", None));
        }
        self.loaded_full_body_text = Some(text);
        self.loaded_full_body_blob_id = Some(blob_id);
        cx.notify();
        Ok(())
    }

    pub fn copy_response_body(&self, cx: &mut Context<Self>) -> Result<(), String> {
        let Some((text, _media_type)) = self.response_body_text_for_actions(cx)? else {
            return Err(es_fluent::localize("request_tab_copy_unavailable", None).to_string());
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        Ok(())
    }

    pub fn save_response_body_to_file(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let services = cx.global::<AppServicesGlobal>().0.clone();
        let (source, suggested_name) = match self.editor.exec_status() {
            ExecStatus::Completed { response } => {
                let suggested = suggested_file_name(response.media_type.as_deref());
                match &response.body_ref {
                    BodyRef::Empty => {
                        return Err(
                            es_fluent::localize("request_tab_save_unavailable", None).to_string()
                        );
                    }
                    BodyRef::InMemoryPreview { bytes, .. } => {
                        (SaveSource::InMemory(bytes.to_vec()), suggested)
                    }
                    BodyRef::DiskBlob { blob_id, .. } => {
                        (SaveSource::Blob(blob_id.clone()), suggested)
                    }
                }
            }
            _ => {
                return Err(es_fluent::localize("request_tab_save_unavailable", None).to_string());
            }
        };

        let receiver = cx.prompt_for_new_path(
            &std::env::current_dir().unwrap_or_default(),
            Some(&suggested_name),
        );
        cx.spawn_in(window, async move |_, _| {
            let Some(path) = receiver.await.ok().into_iter().flatten().flatten().next() else {
                return;
            };
            let result = match source {
                SaveSource::InMemory(bytes) => {
                    std::fs::write(&path, bytes).map_err(anyhow::Error::from)
                }
                SaveSource::Blob(blob_id) => {
                    let mut reader = match services.blob_store.open_read(&blob_id) {
                        Ok(file) => file,
                        Err(err) => {
                            tracing::warn!(error = %err, "open blob for save failed");
                            return;
                        }
                    };
                    let mut writer = match std::fs::File::create(&path) {
                        Ok(file) => file,
                        Err(err) => {
                            tracing::warn!(error = %err, "create save destination failed");
                            return;
                        }
                    };
                    std::io::copy(&mut reader, &mut writer)
                        .map(|_| ())
                        .map_err(anyhow::Error::from)
                }
            };
            if let Err(err) = result {
                tracing::warn!(error = %err, "failed to save response body to file");
            }
        })
        .detach();
        Ok(())
    }

    fn response_body_text_for_actions(
        &self,
        cx: &Context<Self>,
    ) -> Result<Option<(String, Option<String>)>, String> {
        let ExecStatus::Completed { response } = self.editor.exec_status() else {
            return Ok(None);
        };

        let media_type = response.media_type.clone();
        if !is_text_like_media_type(media_type.as_deref()) {
            return Ok(None);
        }

        let text = match &response.body_ref {
            BodyRef::Empty => String::new(),
            BodyRef::InMemoryPreview { bytes, .. } => {
                render_preview_text(bytes, media_type.as_deref())
            }
            BodyRef::DiskBlob {
                blob_id,
                preview,
                size_bytes,
            } => {
                if *size_bytes > (8 * 1024 * 1024) {
                    return Err(es_fluent::localize("request_tab_copy_too_large", None).to_string());
                }
                let bytes = if self.loaded_full_body_blob_id.as_deref() == Some(blob_id.as_str()) {
                    cx.global::<AppServicesGlobal>()
                        .0
                        .blob_store
                        .read_all(blob_id)
                        .map_err(|e| {
                            format!(
                                "{}: {e}",
                                es_fluent::localize("request_tab_full_body_load_failed", None)
                            )
                        })?
                } else if let Some(preview) = preview {
                    preview.to_vec()
                } else {
                    cx.global::<AppServicesGlobal>()
                        .0
                        .blob_store
                        .read_preview(blob_id, ResponseBudgets::PREVIEW_CAP_BYTES)
                        .map_err(|e| {
                            format!(
                                "{}: {e}",
                                es_fluent::localize("request_tab_full_body_load_failed", None)
                            )
                        })?
                };
                render_preview_text(&bytes, media_type.as_deref())
            }
        };

        Ok(Some((text, media_type)))
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

    /// Mark the response tables (headers, cookies, timing) as needing a data push
    /// on the next render.  Call this whenever the exec status transitions to
    /// `Completed` from an external site (e.g., history restore in `request_pages.rs`).
    pub fn mark_response_tables_dirty(&mut self) {
        self.response_tables_dirty = true;
        self.last_preview_html = None;
    }
}

fn ensure_parent_exists(
    state: &LinkedCollectionState,
    parent_folder_id: Option<crate::domain::ids::FolderId>,
) -> Result<(), RequestRepoError> {
    if let Some(parent) = parent_folder_id {
        let exists = state.folders.iter().any(|folder| folder.id == parent);
        if !exists {
            return Err(RequestRepoError::Storage(anyhow!(
                "parent folder does not exist in linked collection"
            )));
        }
    }
    Ok(())
}

fn next_linked_request_sort(
    folders: &[crate::domain::folder::Folder],
    requests: &[RequestItem],
    parent_folder_id: Option<crate::domain::ids::FolderId>,
    excluding: Option<crate::domain::ids::RequestId>,
) -> i64 {
    folders
        .iter()
        .filter(|folder| folder.parent_folder_id == parent_folder_id)
        .map(|folder| folder.sort_order)
        .chain(
            requests
                .iter()
                .filter(|request| {
                    request.parent_folder_id == parent_folder_id
                        && excluding.is_none_or(|excluded| excluded != request.id)
                })
                .map(|request| request.sort_order),
        )
        .max()
        .unwrap_or(-1)
        + 1
}

fn attach_request_order(
    state: &mut LinkedCollectionState,
    request_id: crate::domain::ids::RequestId,
    parent_folder_id: Option<crate::domain::ids::FolderId>,
) {
    let sibling = LinkedSiblingId::Request {
        id: request_id.to_string(),
    };
    if let Some(parent_id) = parent_folder_id {
        state
            .folder_child_orders
            .entry(parent_id)
            .or_default()
            .push(sibling);
    } else {
        state.root_child_order.push(sibling);
    }
}

fn detach_request_order(
    state: &mut LinkedCollectionState,
    request_id: crate::domain::ids::RequestId,
    parent_folder_id: Option<crate::domain::ids::FolderId>,
) {
    let request_id = request_id.to_string();
    if let Some(parent_id) = parent_folder_id {
        if let Some(children) = state.folder_child_orders.get_mut(&parent_id) {
            children.retain(|child| match child {
                LinkedSiblingId::Request { id } => id != &request_id,
                _ => true,
            });
        }
    } else {
        state.root_child_order.retain(|child| match child {
            LinkedSiblingId::Request { id } => id != &request_id,
            _ => true,
        });
    }
}

fn next_duplicate_request_name(source_name: &str, existing_names: &[String]) -> String {
    let base = format!("{source_name} (Copy)");
    if !existing_names.iter().any(|name| name == &base) {
        return base;
    }

    let mut index = 2;
    loop {
        let candidate = format!("{source_name} (Copy {index})");
        if !existing_names.iter().any(|name| name == &candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn next_postman_style_name(base: &str, existing_names: &[String]) -> String {
    if !existing_names.iter().any(|name| name == base) {
        return base.to_string();
    }

    let mut index = 2;
    loop {
        let candidate = format!("{base} ({index})");
        if !existing_names.iter().any(|name| name == &candidate) {
            return candidate;
        }
        index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::{next_duplicate_request_name, next_postman_style_name};

    #[test]
    fn duplicate_name_uses_copy_suffix_and_increments() {
        let existing = vec![
            "Untitled Request".to_string(),
            "Untitled Request (Copy)".to_string(),
            "Untitled Request (Copy 2)".to_string(),
        ];
        assert_eq!(
            next_duplicate_request_name("Untitled Request", &existing),
            "Untitled Request (Copy 3)"
        );
    }

    #[test]
    fn postman_style_name_uses_parenthesized_increment() {
        let existing = vec![
            "Untitled Request".to_string(),
            "Untitled Request (2)".to_string(),
        ];
        assert_eq!(
            next_postman_style_name("Untitled Request", &existing),
            "Untitled Request (3)"
        );
    }
}
