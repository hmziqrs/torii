use super::AppRoot;
use crate::{
    app::{
        About, CloseTab, NewRequest, NextTab, OpenLayoutDebug, OpenSettings, PrevTab,
        ToggleSidebar, TreeDeleteSelected, TreeOpenItemMenu, TreeOpenSelected,
    },
    domain::item_id::ItemId,
    session::{item_key::ItemKey, workspace_session::ExpandableItem},
};
use gpui::{Context, Window, div, prelude::*};
use gpui_component::{
    WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};

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
        if let Some(item_key) = self.session.read(cx).sidebar_selection {
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
        }
    }

    pub(super) fn on_tree_delete_selected_action(
        &mut self,
        _: &TreeDeleteSelected,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item_key) = self.session.read(cx).sidebar_selection else {
            return;
        };
        match item_key.id {
            Some(
                ItemId::Workspace(_)
                | ItemId::Collection(_)
                | ItemId::Folder(_)
                | ItemId::Environment(_)
                | ItemId::Request(_),
            ) => self.delete_item(item_key, window, cx),
            _ => {}
        }
    }

    pub(super) fn on_tree_open_item_menu_action(
        &mut self,
        _: &TreeOpenItemMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item_key) = self.session.read(cx).sidebar_selection else {
            window.push_notification(es_fluent::localize("tree_item_menu_no_selection", None), cx);
            return;
        };

        let item_name = self
            .catalog
            .find_title(item_key)
            .unwrap_or_else(|| es_fluent::localize("tab_missing_short", None));
        let folder_collection_id = if let Some(ItemId::Folder(folder_id)) = item_key.id {
            self.catalog.selected_workspace().and_then(|workspace| {
                workspace.collections.iter().find_map(|collection| {
                    collection
                        .find_folder_tree(folder_id)
                        .map(|_| collection.collection.id)
                })
            })
        } else {
            None
        };
        let weak_root = cx.entity().downgrade();

        window.open_dialog(cx, move |dialog, _, _| {
            let mut actions = h_flex().justify_end().gap_2().child(
                Button::new("tree-item-menu-cancel")
                    .outline()
                    .label(es_fluent::localize("dialog_cancel", None))
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            );

            if matches!(
                item_key.id,
                Some(
                    ItemId::Workspace(_)
                        | ItemId::Collection(_)
                        | ItemId::Folder(_)
                        | ItemId::Environment(_)
                        | ItemId::Request(_)
                )
            ) {
                let weak_root_rename = weak_root.clone();
                let rename_name = item_name.clone();
                actions = actions.child(
                    Button::new("tree-item-menu-rename")
                        .outline()
                        .label(es_fluent::localize("menu_rename", None))
                        .on_click(move |_, window, cx| {
                            let _ = weak_root_rename.update(cx, |this, cx| {
                                this.open_rename_item_dialog(
                                    item_key,
                                    rename_name.clone(),
                                    window,
                                    cx,
                                );
                            });
                            window.close_dialog(cx);
                        }),
                );
            }

            if matches!(item_key.id, Some(ItemId::Workspace(_))) {
                let weak_root_new_collection = weak_root.clone();
                actions = actions.child(
                    Button::new("tree-item-menu-new-collection")
                        .outline()
                        .label(es_fluent::localize("sidebar_new_collection", None))
                        .on_click(move |_, window, cx| {
                            let _ = weak_root_new_collection.update(cx, |this, cx| {
                                let _ = this.create_collection(window, cx);
                            });
                            window.close_dialog(cx);
                        }),
                );
                let weak_root_new_environment = weak_root.clone();
                actions = actions.child(
                    Button::new("tree-item-menu-new-environment")
                        .outline()
                        .label(es_fluent::localize("sidebar_new_environment", None))
                        .on_click(move |_, window, cx| {
                            let _ = weak_root_new_environment.update(cx, |this, cx| {
                                let _ = this.create_environment(cx);
                            });
                            window.close_dialog(cx);
                        }),
                );
            }

            if let Some(ItemId::Collection(collection_id)) = item_key.id {
                let weak_root_new_request = weak_root.clone();
                actions = actions.child(
                    Button::new("tree-item-menu-new-request")
                        .outline()
                        .label(es_fluent::localize("menu_new_request", None))
                        .on_click(move |_, window, cx| {
                            let _ = weak_root_new_request.update(cx, |this, cx| {
                                this.open_auto_saved_request(collection_id, window, cx);
                            });
                            window.close_dialog(cx);
                        }),
                );
                let weak_root_new_folder = weak_root.clone();
                actions = actions.child(
                    Button::new("tree-item-menu-new-folder")
                        .outline()
                        .label(es_fluent::localize("menu_new_folder", None))
                        .on_click(move |_, window, cx| {
                            let _ = weak_root_new_folder.update(cx, |this, cx| {
                                let _ = this.create_folder(collection_id, None, cx);
                            });
                            window.close_dialog(cx);
                        }),
                );
            }

            if let Some(ItemId::Folder(folder_id)) = item_key.id {
                if let Some(collection_id) = folder_collection_id {
                    let weak_root_new_request = weak_root.clone();
                    actions = actions.child(
                        Button::new("tree-item-menu-folder-new-request")
                            .outline()
                            .label(es_fluent::localize("menu_new_request", None))
                            .on_click(move |_, window, cx| {
                                let _ = weak_root_new_request.update(cx, |this, cx| {
                                    this.open_auto_saved_request_in_folder(
                                        collection_id,
                                        folder_id,
                                        window,
                                        cx,
                                    );
                                });
                                window.close_dialog(cx);
                            }),
                    );
                    let weak_root_new_folder = weak_root.clone();
                    actions = actions.child(
                        Button::new("tree-item-menu-folder-new-folder")
                            .outline()
                            .label(es_fluent::localize("menu_new_folder", None))
                            .on_click(move |_, window, cx| {
                                let _ = weak_root_new_folder.update(cx, |this, cx| {
                                    let _ = this.create_folder(collection_id, Some(folder_id), cx);
                                });
                                window.close_dialog(cx);
                            }),
                    );
                }
            }

            if let Some(ItemId::Environment(environment_id)) = item_key.id {
                let weak_root_set_active = weak_root.clone();
                actions = actions.child(
                    Button::new("tree-item-menu-set-active")
                        .outline()
                        .label(es_fluent::localize("menu_set_active_environment", None))
                        .on_click(move |_, window, cx| {
                            let _ = weak_root_set_active.update(cx, |this, cx| {
                                this.set_active_environment(environment_id, cx);
                            });
                            window.close_dialog(cx);
                        }),
                );
            }

            if matches!(
                item_key.id,
                Some(
                    ItemId::Workspace(_)
                        | ItemId::Collection(_)
                        | ItemId::Folder(_)
                        | ItemId::Environment(_)
                        | ItemId::Request(_)
                )
            ) {
                let weak_root_delete = weak_root.clone();
                actions = actions.child(
                    Button::new("tree-item-menu-delete")
                        .primary()
                        .label(es_fluent::localize("menu_delete", None))
                        .on_click(move |_, window, cx| {
                            let _ = weak_root_delete.update(cx, |this, cx| {
                                this.delete_item(item_key, window, cx);
                            });
                            window.close_dialog(cx);
                        }),
                );
            }

            dialog
                .title(es_fluent::localize("tree_item_menu_title", None))
                .overlay_closable(true)
                .keyboard(true)
                .child(
                    v_flex()
                        .gap_2()
                        .child(div().text_sm().child(format!(
                            "{}: {}",
                            es_fluent::localize("tree_item_menu_selected_item", None),
                            item_name
                        )))
                        .child(actions),
                )
        });
    }
}
