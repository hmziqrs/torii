use gpui::prelude::*;
use gpui::{Context, Entity, Window};
use crate::{
    domain::{
        history::HistoryState,
        ids::{CollectionId, RequestDraftId},
        response::{BodyRef, ResponseBudgets, ResponseSummary, normalize_unix_ms},
    },
    services::workspace_tree::load_workspace_catalog,
    session::{
        item_key::{ItemKey, TabKey},
        request_editor_state::EditorIdentity,
    },
    views::item_tabs::request_tab,
};
use super::{AppRoot, services};

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
                            });
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

        let services_for_refresh = services.clone();
        let subscription = cx.observe(&page, move |this, _, cx| {
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
        let draft_id = RequestDraftId::new();
        let page = cx.new(|cx| request_tab::RequestTabView::new_draft(collection_id, window, cx));

        // Observe entity for draft→persisted transition and catalog refresh
        let services = services(cx);
        let services_for_refresh = services.clone();
        let draft_id_for_observer = draft_id;
        let subscription = cx.observe(&page, move |this, observed_page, cx| {
            let identity = observed_page.read(cx).editor().identity().clone();

            if let EditorIdentity::Persisted(request_id) = identity {
                // Check if this draft hasn't been transitioned yet
                if this
                    .request_draft_pages
                    .contains_key(&draft_id_for_observer)
                {
                    let old_key = TabKey::from(ItemKey::request_draft(draft_id_for_observer));
                    let new_key = TabKey::from(ItemKey::request(request_id));

                    this.session.update(cx, |session, cx| {
                        session.tab_manager.replace_key(old_key, new_key);
                        cx.notify();
                    });

                    if let Some(page) = this.request_draft_pages.remove(&draft_id_for_observer) {
                        this.request_pages.insert(request_id, page);
                    }
                }
            }

            // Always refresh catalog
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

        self.request_draft_pages.insert(draft_id, page);

        // Register with TabManager so the tab appears in the tab bar
        let item_key = ItemKey::request_draft(draft_id);
        self.session.update(cx, |session, cx| {
            session.open_or_focus(item_key, cx);
        });

        self.persist_session_state(cx);
    }
}
