use super::{AppRoot, services};
use crate::{
    domain::{
        collection::CollectionStorageKind,
        ids::{CollectionId, EnvironmentId, FolderId, RequestId, WorkspaceId},
        item_id::ItemId,
    },
    session::{
        item_key::{ItemKey, ItemKind, TabKey},
        request_editor_state::EditorIdentity,
    },
    views::item_tabs::request_tab,
};
use gpui::prelude::*;
use gpui::{Context, Entity, Window};
use gpui_component::{
    WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    v_flex,
};

impl AppRoot {
    pub(crate) fn create_workspace(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let services = services(cx);
        let workspaces = services
            .repos
            .workspace
            .list()
            .map_err(|err| format!("failed to list workspaces: {err}"))?;
        let name = next_workspace_name(
            &workspaces
                .iter()
                .map(|workspace| workspace.name.clone())
                .collect::<Vec<_>>(),
        );
        let workspace = services
            .repos
            .workspace
            .create(&name)
            .map_err(|err| format!("failed to create workspace: {err}"))?;
        drop(services);

        self.refresh_catalog(cx);
        self.open_item(ItemKey::workspace(workspace.id), cx);
        Ok(())
    }

    pub(crate) fn create_collection(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let workspace_id = self
            .session
            .read(cx)
            .selected_workspace_id
            .or_else(|| self.catalog.first_workspace_id())
            .ok_or_else(|| es_fluent::localize("create_collection_no_workspace", None))?;

        let services = services(cx);
        let collections = services
            .repos
            .collection
            .list_by_workspace(workspace_id)
            .map_err(|err| format!("failed to list collections: {err}"))?;
        let name = next_item_name(
            es_fluent::localize("collection_default_name", None),
            &collections
                .iter()
                .map(|collection| collection.name.clone())
                .collect::<Vec<_>>(),
        );
        let collection = services
            .repos
            .collection
            .create(workspace_id, &name)
            .map_err(|err| format!("failed to create collection: {err}"))?;
        drop(services);

        self.refresh_catalog(cx);
        self.open_item(ItemKey::collection(collection.id), cx);
        Ok(())
    }

    pub(crate) fn create_environment(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let workspace_id = self
            .session
            .read(cx)
            .selected_workspace_id
            .or_else(|| self.catalog.first_workspace_id())
            .ok_or_else(|| es_fluent::localize("create_environment_no_workspace", None))?;

        let services = services(cx);
        let environments = services
            .repos
            .environment
            .list_by_workspace(workspace_id)
            .map_err(|err| format!("failed to list environments: {err}"))?;
        let name = next_item_name(
            es_fluent::localize("environment_default_name", None),
            &environments
                .iter()
                .map(|environment| environment.name.clone())
                .collect::<Vec<_>>(),
        );
        let environment = services
            .repos
            .environment
            .create(workspace_id, &name)
            .map_err(|err| format!("failed to create environment: {err}"))?;
        drop(services);

        self.refresh_catalog(cx);
        self.open_item(ItemKey::environment(environment.id), cx);
        Ok(())
    }

    pub(crate) fn open_workspace_variables_dialog(
        &mut self,
        workspace_id: crate::domain::ids::WorkspaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let services_ref = services(cx);
        let Some(workspace) = services_ref
            .repos
            .workspace
            .get(workspace_id)
            .map_err(|err| format!("failed to load workspace: {err}"))
            .ok()
            .flatten()
        else {
            window.push_notification("workspace no longer exists", cx);
            return;
        };

        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |state, cx| {
            state.set_value(workspace.variables_json.clone(), window, cx);
        });
        let weak_root = cx.entity().downgrade();

        window.open_dialog(cx, move |dialog, _, _cx| {
            let weak_root_save = weak_root.clone();
            let input_for_save = input.clone();
            dialog
                .title("Workspace Variables (JSON)")
                .overlay_closable(true)
                .keyboard(true)
                .child(
                    v_flex()
                        .gap_2()
                        .child(Input::new(&input).h(gpui::px(260.)).w_full()),
                )
                .footer(
                    h_flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("workspace-vars-cancel")
                                .label("Cancel")
                                .on_click(move |_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("workspace-vars-save")
                                .primary()
                                .label("Save")
                                .on_click(move |_, window, cx| {
                                    let payload = input_for_save.read(cx).value().to_string();
                                    let parsed =
                                        serde_json::from_str::<serde_json::Value>(&payload);
                                    let is_valid = matches!(
                                        parsed,
                                        Ok(serde_json::Value::Array(_))
                                            | Ok(serde_json::Value::Object(_))
                                    );
                                    if !is_valid {
                                        window.push_notification(
                                            "Variables JSON must be an array or object",
                                            cx,
                                        );
                                        return;
                                    }
                                    let _ = weak_root_save.update(cx, |this, cx| {
                                        let services = services(cx);
                                        match services
                                            .repos
                                            .workspace
                                            .update_variables(workspace_id, &payload)
                                        {
                                            Ok(()) => {
                                                this.refresh_catalog(cx);
                                                cx.notify();
                                            }
                                            Err(err) => {
                                                window.push_notification(
                                                    format!(
                                                        "failed to save workspace variables: {err}"
                                                    ),
                                                    cx,
                                                );
                                            }
                                        }
                                    });
                                    window.close_dialog(cx);
                                }),
                        ),
                )
        });
    }

    pub(crate) fn open_environment_variables_dialog(
        &mut self,
        environment_id: crate::domain::ids::EnvironmentId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let services_ref = services(cx);
        let Some(environment) = services_ref
            .repos
            .environment
            .get(environment_id)
            .map_err(|err| format!("failed to load environment: {err}"))
            .ok()
            .flatten()
        else {
            window.push_notification("environment no longer exists", cx);
            return;
        };

        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |state, cx| {
            state.set_value(environment.variables_json.clone(), window, cx);
        });
        let weak_root = cx.entity().downgrade();

        window.open_dialog(cx, move |dialog, _, _cx| {
            let weak_root_save = weak_root.clone();
            let input_for_save = input.clone();
            dialog
                .title("Environment Variables (JSON)")
                .overlay_closable(true)
                .keyboard(true)
                .child(
                    v_flex()
                        .gap_2()
                        .child(Input::new(&input).h(gpui::px(260.)).w_full()),
                )
                .footer(
                    h_flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("environment-vars-cancel")
                                .label("Cancel")
                                .on_click(move |_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("environment-vars-save")
                                .primary()
                                .label("Save")
                                .on_click(move |_, window, cx| {
                                    let payload =
                                        input_for_save.read(cx).value().to_string();
                                    let parsed =
                                        serde_json::from_str::<serde_json::Value>(&payload);
                                    let is_valid =
                                        matches!(parsed, Ok(serde_json::Value::Array(_)) | Ok(serde_json::Value::Object(_)));
                                    if !is_valid {
                                        window.push_notification(
                                            "Variables JSON must be an array or object",
                                            cx,
                                        );
                                        return;
                                    }
                                    let _ = weak_root_save.update(cx, |this, cx| {
                                        let services = services(cx);
                                        match services
                                            .repos
                                            .environment
                                            .update_variables(environment_id, &payload)
                                        {
                                            Ok(()) => {
                                                this.refresh_catalog(cx);
                                                cx.notify();
                                            }
                                            Err(err) => {
                                                window.push_notification(
                                                    format!(
                                                        "failed to save environment variables: {err}"
                                                    ),
                                                    cx,
                                                );
                                            }
                                        }
                                    });
                                    window.close_dialog(cx);
                                }),
                        ),
                )
        });
    }

    pub(crate) fn create_folder(
        &mut self,
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let services = services(cx);
        let collection = services
            .repos
            .collection
            .get(collection_id)
            .map_err(|err| format!("failed to load collection: {err}"))?
            .ok_or_else(|| "collection no longer exists".to_string())?;
        if collection.storage_kind == CollectionStorageKind::Linked {
            return Err(es_fluent::localize(
                "create_folder_linked_unsupported",
                None,
            ));
        }

        let folders = services
            .repos
            .folder
            .list_by_collection(collection_id)
            .map_err(|err| format!("failed to list folders: {err}"))?;
        let sibling_names = folders
            .iter()
            .filter(|folder| folder.parent_folder_id == parent_folder_id)
            .map(|folder| folder.name.clone())
            .collect::<Vec<_>>();
        let name = next_item_name(
            es_fluent::localize("folder_default_name", None),
            &sibling_names,
        );
        let folder = services
            .repos
            .folder
            .create(collection_id, parent_folder_id, &name)
            .map_err(|err| format!("failed to create folder: {err}"))?;
        drop(services);

        self.refresh_catalog(cx);
        self.open_item(ItemKey::folder(folder.id), cx);
        Ok(())
    }

    fn set_selected_workspace_for_item(&mut self, item_key: ItemKey, cx: &mut Context<Self>) {
        let services = services(cx);
        match services.session_restore.workspace_for_item(item_key) {
            Ok(Some(workspace_id)) => {
                self.session.update(cx, |session, cx| {
                    session.set_selected_workspace(Some(workspace_id), cx)
                });
            }
            Ok(None) => {}
            Err(err) => tracing::error!("failed to resolve item workspace: {err}"),
        }
    }

    pub(crate) fn open_item(&mut self, item_key: ItemKey, cx: &mut Context<Self>) {
        if item_key.is_persisted() {
            self.set_selected_workspace_for_item(item_key, cx);
        }
        self.session.update(cx, |session, cx| {
            session.open_or_focus(item_key, cx);
        });
        self.persist_session_state(cx);
    }

    pub(super) fn focus_tab(&mut self, tab_key: TabKey, cx: &mut Context<Self>) {
        self.set_selected_workspace_for_item(tab_key.item(), cx);
        self.session.update(cx, |session, cx| {
            session.focus_tab(tab_key, cx);
        });
        self.persist_session_state(cx);
    }

    /// Release the HTML preview webview for a request tab, if applicable.
    /// Safe to call with `None` or a non-request tab key — it will be a no-op.
    pub(super) fn release_html_webview_for_tab(
        &mut self,
        tab_key: Option<TabKey>,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_key) = tab_key else {
            return;
        };
        let page = match tab_key.item().id {
            Some(ItemId::Request(id)) => self.request_pages.get(&id).cloned(),
            Some(ItemId::RequestDraft(id)) => self.request_draft_pages.get(&id).cloned(),
            _ => None,
        };
        if let Some(page) = page {
            page.update(cx, |tab, cx| {
                tab.release_html_webview(cx);
            });
        }
    }

    fn perform_close_tab(&mut self, tab_key: TabKey, cx: &mut Context<Self>) {
        self.release_html_webview_for_tab(Some(tab_key), cx);
        self.session.update(cx, |session, cx| {
            session.close_tab(tab_key, cx);
        });
        self.persist_session_state(cx);
    }

    pub(crate) fn close_tab(
        &mut self,
        tab_key: TabKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let request_id = match (tab_key.item().kind, tab_key.item().id) {
            (ItemKind::Request, Some(ItemId::Request(id))) => Some(id),
            _ => None,
        };

        let draft_id = match tab_key.item().id {
            Some(ItemId::RequestDraft(id)) => Some(id),
            _ => None,
        };

        let should_confirm_dirty = request_id
            .and_then(|id| self.request_pages.get(&id))
            .map(|page: &Entity<request_tab::RequestTabView>| page.read(cx).has_unsaved_changes())
            .unwrap_or(false)
            || draft_id
                .and_then(|id| self.request_draft_pages.get(&id))
                .map(|page: &Entity<request_tab::RequestTabView>| {
                    page.read(cx).has_unsaved_changes()
                })
                .unwrap_or(false);

        if !should_confirm_dirty {
            self.perform_close_tab(tab_key, cx);
            return;
        }

        let weak_root = cx.entity().downgrade();
        let weak_root_save = weak_root.clone();
        let weak_root_discard = weak_root.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title(es_fluent::localize("request_tab_dirty_close_title", None))
                .overlay_closable(false)
                .keyboard(false)
                .child(es_fluent::localize("request_tab_dirty_close_body", None))
                .footer(
                    h_flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("dirty-close-save")
                                .primary()
                                .label(es_fluent::localize("request_tab_dirty_close_save", None))
                                .on_click({
                                    let weak_root_save = weak_root_save.clone();
                                    move |_, window, cx| {
                                        let mut close_ok = false;
                                        let mut err_msg = None;
                                        let _ = weak_root_save.update(cx, |this, cx| {
                                            match this.save_request_tab_by_key(tab_key, cx) {
                                                Ok(Some(new_key)) => {
                                                    // Draft was promoted — close using new key
                                                    this.perform_close_tab(new_key, cx);
                                                    close_ok = true;
                                                }
                                                Ok(None) => {
                                                    this.perform_close_tab(tab_key, cx);
                                                    close_ok = true;
                                                }
                                                Err(err) => err_msg = Some(err),
                                            }
                                        });

                                        if let Some(err) = err_msg {
                                            window.push_notification(err, cx);
                                        }
                                        if close_ok {
                                            window.close_dialog(cx);
                                        }
                                    }
                                }),
                        )
                        .child(
                            Button::new("dirty-close-discard")
                                .outline()
                                .label(es_fluent::localize("request_tab_dirty_close_discard", None))
                                .on_click({
                                    let weak_root_discard = weak_root_discard.clone();
                                    move |_, window, cx| {
                                        let _ = weak_root_discard.update(cx, |this, cx| {
                                            // Clean up draft entity if discarding a draft tab
                                            if let Some(ItemId::RequestDraft(draft_id)) =
                                                tab_key.item().id
                                            {
                                                this.request_draft_pages.remove(&draft_id);
                                            }
                                            this.perform_close_tab(tab_key, cx);
                                        });
                                        window.close_dialog(cx);
                                    }
                                }),
                        )
                        .child(
                            Button::new("dirty-close-cancel")
                                .ghost()
                                .label(es_fluent::localize("request_tab_dirty_close_cancel", None))
                                .on_click(move |_, window, cx| {
                                    window.close_dialog(cx);
                                }),
                        ),
                )
        });
    }

    pub(super) fn save_request_tab_by_key(
        &mut self,
        tab_key: TabKey,
        cx: &mut Context<Self>,
    ) -> Result<Option<TabKey>, String> {
        let page = match tab_key.item().id {
            Some(ItemId::Request(id)) => self.request_pages.get(&id).cloned(),
            Some(ItemId::RequestDraft(draft_id)) => {
                self.request_draft_pages.get(&draft_id).cloned()
            }
            _ => None,
        };
        let Some(page) = page else {
            return Ok(None);
        };

        page.update(cx, |tab, cx| tab.save(cx))
            .map_err(|err| format!("failed to update request tab while saving: {err}"))?;

        // After save, the observer may have promoted a draft to persisted.
        // Detect the current tab key from the editor identity.
        let current_key = {
            let identity = page.read(cx).editor().identity().clone();
            match identity {
                EditorIdentity::Persisted(request_id) => {
                    let new_key = TabKey::from(ItemKey::request(request_id));
                    if new_key != tab_key {
                        Some(new_key)
                    } else {
                        None
                    }
                }
                EditorIdentity::Draft(_) => None,
            }
        };

        self.refresh_catalog(cx);
        Ok(current_key)
    }

    pub(super) fn reorder_tabs(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        self.session.update(cx, |session, cx| {
            session.reorder_tabs(from, to, cx);
        });
        self.persist_session_state(cx);
    }

    pub(crate) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.session.update(cx, |session, cx| {
            session.toggle_sidebar(cx);
        });
        self.persist_session_state(cx);
    }

    pub(super) fn set_sidebar_width(&mut self, width_px: f32, cx: &mut Context<Self>) {
        self.session.update(cx, |session, cx| {
            session.set_sidebar_width(width_px, cx);
        });
        self.persist_session_state(cx);
    }

    pub(super) fn refresh_catalog(&mut self, cx: &mut Context<Self>) {
        let services = services(cx);
        let selected_workspace_id = self.session.read(cx).selected_workspace_id;
        match crate::services::workspace_tree::load_workspace_catalog(
            &services.repos.workspace,
            &services.repos.collection,
            &services.repos.folder,
            &services.repos.request,
            &services.repos.environment,
            selected_workspace_id,
        ) {
            Ok(catalog) => {
                if self.catalog != catalog {
                    self.catalog = catalog;
                    cx.notify();
                }
            }
            Err(err) => tracing::error!("failed to refresh workspace catalog: {err}"),
        }
    }

    pub(crate) fn duplicate_request(
        &mut self,
        request_id: RequestId,
        request_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let services = services(cx);
        let new_name = format!("{} (Copy)", request_name);
        match services.repos.request.duplicate(request_id, &new_name) {
            Ok(new_request) => {
                drop(services);
                self.refresh_catalog(cx);
                self.open_item(ItemKey::request(new_request.id), cx);
                window.push_notification(es_fluent::localize("request_tab_duplicate_ok", None), cx);
            }
            Err(err) => {
                tracing::error!("failed to duplicate request: {err}");
                window.push_notification(
                    es_fluent::localize("request_tab_duplicate_failed", None),
                    cx,
                );
            }
        }
    }

    pub(crate) fn delete_item(
        &mut self,
        item_key: ItemKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let services = services(cx);
        let close_keys = self.catalog.delete_closure(item_key);
        let selected_workspace = services
            .session_restore
            .workspace_for_item(item_key)
            .ok()
            .flatten();

        let result = match (item_key.kind, item_key.id) {
            (ItemKind::Workspace, Some(ItemId::Workspace(id))) => {
                services.repos.workspace.delete(id)
            }
            (ItemKind::Collection, Some(ItemId::Collection(id))) => {
                services.repos.collection.delete(id)
            }
            (ItemKind::Folder, Some(ItemId::Folder(id))) => services.repos.folder.delete(id),
            (ItemKind::Environment, Some(ItemId::Environment(id))) => {
                services.repos.environment.delete(id)
            }
            (ItemKind::Request, Some(ItemId::Request(id))) => {
                if let Some(page) = self.request_pages.get(&id).cloned() {
                    let _ = page.update(cx, |tab, cx| {
                        tab.cancel_send(cx);
                        tab.release_html_webview(cx);
                    });
                }
                self.request_pages.remove(&id);
                services.repos.request.delete(id)
            }
            _ => Ok(()),
        };

        match result {
            Ok(()) => {
                if let (ItemKind::Environment, Some(workspace_id)) =
                    (item_key.kind, selected_workspace)
                {
                    if let Ok(mut shared) = services.active_environments_by_workspace.write() {
                        shared.remove(&workspace_id);
                    }
                }
                let fallback_workspace = services
                    .repos
                    .workspace
                    .list()
                    .ok()
                    .and_then(|workspaces| workspaces.first().map(|workspace| workspace.id));

                self.session.update(cx, |session, cx| {
                    session.close_tabs(&close_keys, cx);
                    if let (ItemKind::Environment, Some(workspace_id)) =
                        (item_key.kind, selected_workspace)
                    {
                        session.set_active_environment_for_workspace(workspace_id, None, cx);
                    }
                    if should_reset_selected_workspace_on_delete(
                        item_key,
                        session.selected_workspace_id,
                        selected_workspace,
                    ) {
                        session.set_selected_workspace(fallback_workspace, cx);
                    }
                });
                self.refresh_catalog(cx);
                self.persist_session_state(cx);
                window.push_notification(es_fluent::localize("delete_success", None), cx);
            }
            Err(err) => {
                tracing::error!("failed to delete item: {err}");
                window.push_notification(es_fluent::localize("delete_failed", None), cx);
            }
        }
    }

    pub(crate) fn set_active_environment(
        &mut self,
        environment_id: EnvironmentId,
        cx: &mut Context<Self>,
    ) {
        let services = services(cx);
        let workspace_id = match services
            .session_restore
            .workspace_for_item(ItemKey::environment(environment_id))
        {
            Ok(Some(workspace_id)) => workspace_id,
            _ => return,
        };

        self.session.update(cx, |session, cx| {
            session.set_selected_workspace(Some(workspace_id), cx);
            session.set_active_environment_for_workspace(workspace_id, Some(environment_id), cx);
        });
        if let Ok(mut shared) = services.active_environments_by_workspace.write() {
            shared.insert(workspace_id, environment_id);
        }
        self.persist_session_state(cx);
    }
}

fn should_reset_selected_workspace_on_delete(
    item_key: ItemKey,
    selected_workspace_id: Option<WorkspaceId>,
    deleted_item_workspace: Option<WorkspaceId>,
) -> bool {
    matches!(item_key.kind, ItemKind::Workspace) && selected_workspace_id == deleted_item_workspace
}

fn next_workspace_name(existing_names: &[String]) -> String {
    next_item_name(
        es_fluent::localize("workspace_default_name", None),
        existing_names,
    )
}

fn next_item_name(base: String, existing_names: &[String]) -> String {
    if !existing_names.iter().any(|name| name == &base) {
        return base;
    }

    let mut index = 2;
    loop {
        let candidate = format!("{base} {index}");
        if !existing_names.iter().any(|name| name == &candidate) {
            return candidate;
        }
        index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::should_reset_selected_workspace_on_delete;
    use crate::{
        domain::ids::{CollectionId, WorkspaceId},
        session::item_key::ItemKey,
    };

    #[test]
    fn workspace_delete_resets_when_selected_workspace_is_deleted() {
        let workspace_id = WorkspaceId::new();
        assert!(should_reset_selected_workspace_on_delete(
            ItemKey::workspace(workspace_id),
            Some(workspace_id),
            Some(workspace_id),
        ));
    }

    #[test]
    fn non_workspace_delete_does_not_reset_selected_workspace() {
        let workspace_id = WorkspaceId::new();
        let collection_key = ItemKey::collection(CollectionId::new());
        assert!(!should_reset_selected_workspace_on_delete(
            collection_key,
            Some(workspace_id),
            Some(workspace_id),
        ));
    }

    #[test]
    fn deleting_other_workspace_does_not_reset_selection() {
        let selected_workspace_id = WorkspaceId::new();
        let deleted_workspace_id = WorkspaceId::new();
        assert!(!should_reset_selected_workspace_on_delete(
            ItemKey::workspace(deleted_workspace_id),
            Some(selected_workspace_id),
            Some(deleted_workspace_id),
        ));
    }
}
