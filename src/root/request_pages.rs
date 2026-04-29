use super::{AppRoot, services};
use crate::{
    domain::{
        history::{HistoryEntry, HistoryState},
        ids::{CollectionId, FolderId, RequestDraftId},
        response::{
            BodyRef, PhaseTimings, ResponseBudgets, ResponseMetaV2, ResponseSizeBreakdown,
            ResponseSummary, normalize_unix_ms,
        },
    },
    services::workspace_tree::load_workspace_catalog,
    session::{
        item_key::{ItemKey, TabKey},
        request_editor_state::EditorIdentity,
    },
    views::item_tabs::request_tab,
};
use gpui::prelude::*;
use gpui::{Context, Entity, Window};
use gpui_component::WindowExt as _;

impl AppRoot {
    pub(super) fn request_page(
        &mut self,
        request: &crate::domain::request::RequestItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<request_tab::RequestTabView> {
        if let Some(page) = self.request_pages.get(&request.id) {
            return page.clone();
        }

        let page = cx.new(|cx| request_tab::RequestTabView::new(request, window, cx));

        // Restore latest-run summary from history if available
        let services = services(cx);
        if let Ok(Some(history_entry)) = services.repos.history.get_latest_for_request(request.id) {
            page.update(cx, |tab, _cx| {
                tab.editor_mut()
                    .set_latest_history_id(Some(history_entry.id));

                match history_entry.state {
                    HistoryState::Completed => {
                        let body_ref = match (
                            history_entry.blob_hash.as_ref(),
                            history_entry.blob_size.map(|v| v as u64),
                        ) {
                            (Some(hash), Some(size_bytes)) => {
                                let preview = services
                                    .blob_store
                                    .read_preview(hash, ResponseBudgets::PREVIEW_CAP_BYTES)
                                    .ok()
                                    .map(bytes::Bytes::from);
                                BodyRef::DiskBlob {
                                    blob_id: hash.clone(),
                                    preview,
                                    size_bytes,
                                }
                            }
                            _ => BodyRef::Empty,
                        };
                        let status_code = history_entry
                            .status_code
                            .unwrap_or(0)
                            .clamp(0, u16::MAX as i64)
                            as u16;
                        let status_text = http::StatusCode::from_u16(status_code)
                            .ok()
                            .and_then(|status| status.canonical_reason().map(ToOwned::to_owned))
                            .unwrap_or_default();
                        let dispatched_at_unix_ms =
                            history_entry.dispatched_at.map(normalize_unix_ms);
                        let first_byte_at_unix_ms =
                            history_entry.first_byte_at.map(normalize_unix_ms);
                        let completed_at_unix_ms =
                            history_entry.completed_at.map(normalize_unix_ms);
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
                            .and_then(|raw| serde_json::from_str::<ResponseMetaV2>(raw).ok())
                            .unwrap_or_default();

                        tab.editor_mut()
                            .restore_completed_response(ResponseSummary {
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
                                size: ResponseSizeBreakdown {
                                    body_decoded_bytes,
                                    ..meta_v2.size
                                },
                                request_size: meta_v2.request_size,
                                phase_timings: if meta_v2.phase_timings.ttfb_ms.is_some() {
                                    meta_v2.phase_timings
                                } else {
                                    PhaseTimings {
                                        ttfb_ms,
                                        ..meta_v2.phase_timings
                                    }
                                },
                            });
                        tab.mark_response_tables_dirty();
                    }
                    HistoryState::Failed => {
                        tab.editor_mut().restore_failed_response(
                            history_entry
                                .error_message
                                .clone()
                                .unwrap_or_else(|| "request failed".to_string()),
                        );
                    }
                    HistoryState::Cancelled => {
                        tab.editor_mut().restore_cancelled_response(
                            history_entry.partial_size.map(|s| s as u64),
                        );
                    }
                    HistoryState::Pending => {}
                }
            });
        }

        // Track last-seen baseline revision so the catalog is only reloaded when the
        // request is actually saved (revision bumps).  Without this guard the observer
        // fires on every keystroke → 5 SQLite queries + full AppRoot re-render per key.
        // See idle-cpu-audit.md RLA-2.
        let mut last_revision: Option<i64> = Some(request.meta.revision);
        let services_for_refresh = services.clone();
        let subscription = cx.observe(&page, move |this, page, cx| {
            let current_revision = page.read(cx).editor().baseline().map(|b| b.meta.revision);
            if current_revision == last_revision {
                return;
            }
            last_revision = current_revision;
            let selected_workspace_id = this.session.read(cx).selected_workspace_id;
            if let Ok(catalog) = load_workspace_catalog(
                &services_for_refresh.repos.workspace,
                &services_for_refresh.repos.collection,
                &services_for_refresh.repos.folder,
                &services_for_refresh.repos.request,
                &services_for_refresh.repos.environment,
                selected_workspace_id,
            ) {
                this.catalog = catalog;
                cx.notify();
            }
        });
        self._subscriptions.push(subscription);

        self.request_pages.insert(request.id, page.clone());
        page
    }

    /// Open a new draft request tab under the given collection.
    pub fn open_draft_request(
        &mut self,
        collection_id: CollectionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.open_draft_request_with_parent(collection_id, None, false, window, cx);
    }

    /// Open a new draft request tab under the given folder.
    pub fn open_draft_request_in_folder(
        &mut self,
        collection_id: CollectionId,
        parent_folder_id: FolderId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.open_draft_request_with_parent(
            collection_id,
            Some(parent_folder_id),
            false,
            window,
            cx,
        );
    }

    /// Open a new request and persist it immediately with default values.
    pub fn open_auto_saved_request(
        &mut self,
        collection_id: CollectionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.open_draft_request_with_parent(collection_id, None, true, window, cx);
    }

    /// Open a new request in folder and persist it immediately with default values.
    pub fn open_auto_saved_request_in_folder(
        &mut self,
        collection_id: CollectionId,
        parent_folder_id: FolderId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.open_draft_request_with_parent(
            collection_id,
            Some(parent_folder_id),
            true,
            window,
            cx,
        );
    }

    fn open_draft_request_with_parent(
        &mut self,
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
        persist_immediately: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> RequestDraftId {
        let draft_id = RequestDraftId::new();
        let page = cx.new(|cx| request_tab::RequestTabView::new_draft(collection_id, window, cx));
        if parent_folder_id.is_some() {
            page.update(cx, |tab, cx| {
                tab.editor_mut().draft_mut().parent_folder_id = parent_folder_id;
                tab.editor_mut().refresh_save_status();
                tab.mark_draft_dirty(cx);
            });
        }

        // Observe entity for draft→persisted transition and catalog refresh.
        // Track identity + revision so we only reload the catalog when structural
        // changes happen (save / first save / rename) — not on every keystroke.
        // See idle-cpu-audit.md RLA-2.
        let services = services(cx);
        let services_for_refresh = services.clone();
        let draft_id_for_observer = draft_id;
        let mut last_identity: Option<EditorIdentity> = None;
        let mut last_revision: Option<i64> = None;
        let subscription = cx.observe(&page, move |this, observed_page, cx| {
            let current_identity = observed_page.read(cx).editor().identity().clone();
            let current_revision = observed_page
                .read(cx)
                .editor()
                .baseline()
                .map(|b| b.meta.revision);

            let identity_changed = Some(&current_identity) != last_identity.as_ref();
            let revision_changed = current_revision != last_revision;

            // Draft → persisted promotion: always handle regardless of other guards.
            if identity_changed {
                if let EditorIdentity::Persisted(request_id) = &current_identity {
                    if this
                        .request_draft_pages
                        .contains_key(&draft_id_for_observer)
                    {
                        let old_key = TabKey::from(ItemKey::request_draft(draft_id_for_observer));
                        let new_key = TabKey::from(ItemKey::request(*request_id));

                        this.session.update(cx, |session, cx| {
                            session.tab_manager.replace_key(old_key, new_key);
                            cx.notify();
                        });

                        if let Some(page) = this.request_draft_pages.remove(&draft_id_for_observer)
                        {
                            this.request_pages.insert(*request_id, page);
                        }
                    }
                }
                last_identity = Some(current_identity);
            }

            // Reload catalog only when something structural changed.
            if identity_changed || revision_changed {
                last_revision = current_revision;
                let selected_workspace_id = this.session.read(cx).selected_workspace_id;
                if let Ok(catalog) = load_workspace_catalog(
                    &services_for_refresh.repos.workspace,
                    &services_for_refresh.repos.collection,
                    &services_for_refresh.repos.folder,
                    &services_for_refresh.repos.request,
                    &services_for_refresh.repos.environment,
                    selected_workspace_id,
                ) {
                    this.catalog = catalog;
                    cx.notify();
                }
            }
        });
        self._subscriptions.push(subscription);

        self.request_draft_pages.insert(draft_id, page.clone());

        // Register with TabManager so the tab appears in the tab bar
        let item_key = ItemKey::request_draft(draft_id);
        self.session.update(cx, |session, cx| {
            session.open_or_focus(item_key, cx);
        });

        self.persist_session_state(cx);

        if persist_immediately {
            let save_result = page.update(cx, |tab, cx| tab.save(cx));
            if let Err(err) = save_result {
                window.push_notification(err.clone(), cx);
                tracing::error!("failed to auto-save new request: {err}");
            }
        }

        draft_id
    }

    pub(crate) fn restore_history_entry_as_draft(
        &mut self,
        entry: &HistoryEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(selected_workspace) = self.catalog.selected_workspace() else {
            return Err(es_fluent::localize("request_tab_no_workspace", None));
        };
        let collection_id = entry
            .request_collection_id
            .filter(|collection_id| {
                selected_workspace
                    .collections
                    .iter()
                    .any(|collection| collection.collection.id == *collection_id)
            })
            .or_else(|| {
                selected_workspace
                    .collections
                    .first()
                    .map(|collection| collection.collection.id)
            })
            .ok_or_else(|| es_fluent::localize("history_tab_restore_no_collection", None))?;
        let parent_folder_id = entry.request_parent_folder_id.filter(|folder_id| {
            selected_workspace
                .collections
                .iter()
                .any(|collection| collection.find_folder_tree(*folder_id).is_some())
        });

        let draft_id =
            self.open_draft_request_with_parent(collection_id, parent_folder_id, false, window, cx);
        let Some(page) = self.request_draft_pages.get(&draft_id).cloned() else {
            return Err(es_fluent::localize(
                "history_tab_restore_draft_failed",
                None,
            ));
        };

        page.update(cx, |tab, cx| {
            let draft = tab.editor_mut().draft_mut();
            draft.name = entry
                .request_name
                .clone()
                .unwrap_or_else(|| es_fluent::localize("history_tab_restore_draft_name", None));
            draft.method = entry.method.to_ascii_uppercase();
            draft.url = entry.url.clone();
            draft.parent_folder_id = parent_folder_id;
            tab.editor_mut().refresh_save_status();
            tab.mark_draft_dirty(cx);
        });

        Ok(())
    }
}
