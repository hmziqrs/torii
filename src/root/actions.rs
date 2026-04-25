use super::AppRoot;
use crate::{
    app::{
        About, CloseTab, NewRequest, NextTab, OpenLayoutDebug, OpenSettings, PrevTab,
        ToggleSidebar, TreeDeleteSelected, TreeOpenSelected,
    },
    domain::item_id::ItemId,
    session::{item_key::ItemKey, workspace_session::ExpandableItem},
};
use gpui::{Context, Window};
use gpui_component::WindowExt as _;

impl AppRoot {
    pub(super) fn on_about_action(&mut self, _: &About, _: &mut Window, cx: &mut Context<Self>) {
        self.open_item(ItemKey::about(), cx);
    }

    pub(super) fn on_open_settings_action(
        &mut self,
        _: &OpenSettings,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_item(ItemKey::settings(), cx);
    }

    pub(super) fn on_open_layout_debug_action(
        &mut self,
        _: &OpenLayoutDebug,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_item(ItemKey::layout_debug(), cx);
    }

    pub(super) fn on_close_tab_action(
        &mut self,
        _: &CloseTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active = self.session.read(cx).tab_manager.active();
        if let Some(tab_key) = active {
            self.close_tab(tab_key, window, cx);
        }
    }

    pub(super) fn on_next_tab_action(
        &mut self,
        _: &NextTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session.update(cx, |session, cx| {
            session.move_active_tab_by(1, cx);
        });
        self.persist_session_state(cx);
    }

    pub(super) fn on_prev_tab_action(
        &mut self,
        _: &PrevTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session.update(cx, |session, cx| {
            session.move_active_tab_by(-1, cx);
        });
        self.persist_session_state(cx);
    }

    pub(super) fn on_toggle_sidebar_action(
        &mut self,
        _: &ToggleSidebar,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_sidebar(cx);
    }

    pub(super) fn on_new_request_action(
        &mut self,
        _: &NewRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = self.session.read(cx).sidebar_selection;
        let collection_id = selected
            .and_then(|item| match item.id {
                Some(ItemId::Collection(id)) => Some(id),
                _ => None,
            })
            .or_else(|| {
                self.catalog
                    .selected_workspace()
                    .and_then(|ws| ws.collections.first().map(|c| c.collection.id))
            });

        if let Some(collection_id) = collection_id {
            self.open_draft_request(collection_id, window, cx);
        } else {
            window.push_notification(
                es_fluent::localize("request_tab_shortcut_no_collection", None),
                cx,
            );
        }
    }

    pub(super) fn on_tree_open_selected_action(
        &mut self,
        _: &TreeOpenSelected,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        tracing::info!("tree keyboard: open-selected triggered");
        if let Some(item_key) = self.session.read(cx).sidebar_selection {
            tracing::info!(
                ?item_key,
                "tree keyboard: open-selected has sidebar selection"
            );
            match item_key.id {
                Some(ItemId::Collection(collection_id)) => {
                    let has_children = self.catalog.selected_workspace().is_some_and(|workspace| {
                        workspace
                            .collections
                            .iter()
                            .find(|collection| collection.collection.id == collection_id)
                            .is_some_and(|collection| !collection.children.is_empty())
                    });
                    if has_children {
                        tracing::info!(
                            collection_id = %collection_id,
                            "tree keyboard: toggling collection expansion via Enter"
                        );
                        if let Some(workspace_id) = self.catalog.selected_workspace_id() {
                            self.session.update(cx, |session, cx| {
                                session.toggle_expanded_item(
                                    workspace_id,
                                    ExpandableItem::Collection(collection_id),
                                    cx,
                                );
                            });
                            self.persist_session_state(cx);
                        }
                    }
                }
                Some(ItemId::Folder(folder_id)) => {
                    let has_children = self.catalog.selected_workspace().is_some_and(|workspace| {
                        workspace.collections.iter().any(|collection| {
                            collection
                                .find_folder_tree(folder_id)
                                .is_some_and(|folder| !folder.children.is_empty())
                        })
                    });
                    if has_children {
                        tracing::info!(
                            folder_id = %folder_id,
                            "tree keyboard: toggling folder expansion via Enter"
                        );
                        if let Some(workspace_id) = self.catalog.selected_workspace_id() {
                            self.session.update(cx, |session, cx| {
                                session.toggle_expanded_item(
                                    workspace_id,
                                    ExpandableItem::Folder(folder_id),
                                    cx,
                                );
                            });
                            self.persist_session_state(cx);
                        }
                    }
                }
                _ => {}
            }
            self.open_item(item_key, cx);
            tracing::info!(?item_key, "tree keyboard: open-selected completed");
        } else {
            tracing::warn!("tree keyboard: open-selected ignored (no sidebar selection)");
        }
    }

    pub(super) fn on_tree_delete_selected_action(
        &mut self,
        _: &TreeDeleteSelected,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        tracing::info!("tree keyboard: delete-selected triggered");
        let Some(item_key) = self.session.read(cx).sidebar_selection else {
            tracing::warn!("tree keyboard: delete-selected ignored (no sidebar selection)");
            return;
        };
        tracing::info!(
            ?item_key,
            "tree keyboard: delete-selected has sidebar selection"
        );
        match item_key.id {
            Some(
                ItemId::Workspace(_)
                | ItemId::Collection(_)
                | ItemId::Folder(_)
                | ItemId::Environment(_)
                | ItemId::Request(_),
            ) => {
                self.delete_item(item_key, window, cx);
                tracing::info!(?item_key, "tree keyboard: delete-selected completed");
            }
            _ => {
                tracing::warn!(
                    ?item_key,
                    "tree keyboard: delete-selected ignored (unsupported selection)"
                );
            }
        }
    }
}
