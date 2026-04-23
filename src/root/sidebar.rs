use super::AppRoot;
use crate::{
    domain::collection::CollectionStorageKind,
    services::workspace_tree::{CollectionTree, FolderTree, LinkedCollectionHealth, TreeItem},
    session::{item_key::ItemKey, window_layout::SidebarSection},
};
use gpui::{div, prelude::*, px, relative};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Selectable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonRounded, ButtonVariants as _},
    h_flex,
    menu::PopupMenuItem,
    scroll::ScrollableElement as _,
    sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem},
    tooltip::Tooltip,
    v_flex,
};

impl AppRoot {
    pub(super) fn render_sidebar(
        &self,
        active_key: Option<ItemKey>,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let (selected_workspace_id, active_environment_id, sidebar_section, sidebar_collapsed) = {
            let session = self.session.read(cx);
            (
                session.selected_workspace_id,
                session.selected_workspace_id.and_then(|workspace_id| {
                    session.active_environment_for_workspace(workspace_id)
                }),
                session.window_layout.sidebar_section,
                session.window_layout.sidebar_collapsed,
            )
        };
        let weak_root = cx.entity().downgrade();
        let is_collections = sidebar_section == SidebarSection::Collections;
        let collections_selected = is_collections;
        let environments_selected = !is_collections;

        h_flex()
            .size_full()
            .overflow_hidden()
            .child(
                div()
                    .w(px(72.))
                    .h_full()
                    .flex_shrink_0()
                    .overflow_hidden()
                    .border_r_1()
                    .border_color(cx.theme().sidebar_border)
                    .child(
                        v_flex()
                            .size_full()
                            .overflow_y_scrollbar()
                            .px_px()
                            .py_1()
                            .gap_px()
                            .child(
                                Button::new("rail-collections")
                                    .ghost()
                                    .selected(collections_selected)
                                    .rounded(ButtonRounded::None)
                                    .xsmall()
                                    .compact()
                                    .w_full()
                                    .h(px(56.))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.session.update(cx, |session, cx| {
                                            session.window_layout.sidebar_section =
                                                SidebarSection::Collections;
                                            cx.notify();
                                        });
                                    }))
                                    .child(
                                        v_flex()
                                            .items_center()
                                            .gap_0()
                                            .child(Icon::new(IconName::BookOpen).size_6())
                                            .child(
                                                div()
                                                    .text_size(px(8.))
                                                    .text_center()
                                                    .line_height(relative(1.))
                                                    .child(es_fluent::localize(
                                                        "sidebar_rail_collections_short",
                                                        None,
                                                    )),
                                            ),
                                    ),
                            )
                            .child(
                                Button::new("rail-environments")
                                    .ghost()
                                    .selected(environments_selected)
                                    .rounded(ButtonRounded::None)
                                    .xsmall()
                                    .compact()
                                    .w_full()
                                    .h(px(56.))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.session.update(cx, |session, cx| {
                                            session.window_layout.sidebar_section =
                                                SidebarSection::Environments;
                                            cx.notify();
                                        });
                                    }))
                                    .child(
                                        v_flex()
                                            .items_center()
                                            .gap_0()
                                            .child(Icon::new(IconName::Globe).size_6())
                                            .child(
                                                div()
                                                    .text_size(px(8.))
                                                    .text_center()
                                                    .line_height(relative(1.))
                                                    .child(es_fluent::localize(
                                                        "sidebar_rail_environments_short",
                                                        None,
                                                    )),
                                            ),
                                    ),
                            ),
                    ),
            )
            .when(!sidebar_collapsed, |this| {
                this.child(
                    div().flex_1().min_w_0().h_full().overflow_hidden().child(
                        Sidebar::new("app-sidebar-content")
                            .collapsible(false)
                            .w(gpui::relative(1.))
                            .border_0()
                            .child(
                                SidebarGroup::new(es_fluent::localize("sidebar_workspaces", None))
                                    .child(
                                        SidebarMenu::new().children(
                                            std::iter::once(
                                                SidebarMenuItem::new(es_fluent::localize(
                                                    "sidebar_new_workspace",
                                                    None,
                                                ))
                                                .icon(Icon::new(IconName::Plus).small())
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    if let Err(err) = this.create_workspace(cx) {
                                                        window.push_notification(err.clone(), cx);
                                                        tracing::error!(
                                                            "failed to create workspace: {err}"
                                                        );
                                                    }
                                                })),
                                            )
                                            .chain(std::iter::once(
                                                SidebarMenuItem::new(es_fluent::localize(
                                                    "sidebar_new_collection",
                                                    None,
                                                ))
                                                .icon(Icon::new(IconName::Plus).small())
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    if let Err(err) =
                                                        this.create_collection(window, cx)
                                                    {
                                                        window.push_notification(err.clone(), cx);
                                                        tracing::error!(
                                                            "failed to create collection: {err}"
                                                        );
                                                    }
                                                })),
                                            ))
                                            .chain(std::iter::once(
                                                SidebarMenuItem::new(es_fluent::localize(
                                                    "sidebar_new_environment",
                                                    None,
                                                ))
                                                .icon(Icon::new(IconName::Plus).small())
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    if let Err(err) = this.create_environment(cx) {
                                                        window.push_notification(err.clone(), cx);
                                                        tracing::error!(
                                                            "failed to create environment: {err}"
                                                        );
                                                    }
                                                })),
                                            ))
                                            .chain(
                                                self.catalog.workspaces.iter().map(|workspace| {
                                                    let item_key = ItemKey::workspace(workspace.id);
                                                    let weak_root = weak_root.clone();
                                                    SidebarMenuItem::new(workspace.name.clone())
                                                        .icon(Icon::new(IconName::Inbox).small())
                                                        .active(
                                                            active_key == Some(item_key)
                                                                || selected_workspace_id
                                                                    == Some(workspace.id),
                                                        )
                                                        .on_click(cx.listener(
                                                            move |this, _, _, cx| {
                                                                this.open_item(item_key, cx);
                                                            },
                                                        ))
                                                        .context_menu(move |menu, _, _| {
                                                            let weak_root = weak_root.clone();
                                                            menu.item(
                                                                PopupMenuItem::new(
                                                                    es_fluent::localize(
                                                                        "menu_delete",
                                                                        None,
                                                                    ),
                                                                )
                                                                .icon(Icon::new(IconName::Close))
                                                                .on_click(move |_, window, cx| {
                                                                    let _ = weak_root.update(
                                                                        cx,
                                                                        |this, cx| {
                                                                            this.delete_item(
                                                                                item_key, window,
                                                                                cx,
                                                                            );
                                                                        },
                                                                    );
                                                                }),
                                                            )
                                                        })
                                                }),
                                            ),
                                        ),
                                    ),
                            )
                            .when_some(self.catalog.selected_workspace(), |sidebar, workspace| {
                                // Collections section (gated)
                                sidebar
                                    .when(is_collections, |sidebar| {
                                        sidebar.child(
                                            SidebarGroup::new(es_fluent::localize(
                                                "sidebar_collections",
                                                None,
                                            ))
                                            .child(
                                                SidebarMenu::new().children(
                                                    workspace.collections.iter().map(
                                                        |collection| {
                                                            render_collection_menu_item(
                                                                collection, active_key, cx,
                                                            )
                                                        },
                                                    ),
                                                ),
                                            ),
                                        )
                                    })
                                    // Environments section (gated)
                                    .when(!is_collections, |sidebar| {
                                        sidebar.child(
                                            SidebarGroup::new(es_fluent::localize(
                                                "sidebar_environments",
                                                None,
                                            ))
                                            .child(SidebarMenu::new().children(
                                                workspace.environments.iter().map(|environment| {
                                                    let environment_id = environment.id;
                                                    let item_key =
                                                        ItemKey::environment(environment_id);
                                                    let set_active_label = es_fluent::localize(
                                                        "menu_set_active_environment",
                                                        None,
                                                    )
                                                    .to_string();
                                                    let is_active_environment =
                                                        active_environment_id
                                                            == Some(environment.id);
                                                    let weak_root = weak_root.clone();
                                                    SidebarMenuItem::new(if is_active_environment {
                                                        format!(
                                                            "{} {}",
                                                            environment.name,
                                                            es_fluent::localize(
                                                                "sidebar_environment_active_suffix",
                                                                None
                                                            )
                                                        )
                                                    } else {
                                                        environment.name.clone()
                                                    })
                                                    .icon(Icon::new(IconName::Globe).small())
                                                    .active(active_key == Some(item_key))
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.open_item(item_key, cx);
                                                    }))
                                                    .context_menu(move |menu, _, _| {
                                                        let weak_root = weak_root.clone();
                                                        let weak_root_set_active =
                                                            weak_root.clone();
                                                        menu.item(
                                                            PopupMenuItem::new(
                                                                set_active_label.clone(),
                                                            )
                                                            .icon(Icon::new(IconName::Check))
                                                            .on_click(move |_, _, cx| {
                                                                let _ = weak_root_set_active
                                                                    .update(cx, |this, cx| {
                                                                        this.set_active_environment(
                                                                                    environment_id,
                                                                                    cx,
                                                                                );
                                                                    });
                                                            }),
                                                        )
                                                        .item(
                                                            PopupMenuItem::new(
                                                                es_fluent::localize(
                                                                    "menu_delete",
                                                                    None,
                                                                ),
                                                            )
                                                            .icon(Icon::new(IconName::Close))
                                                            .on_click(move |_, window, cx| {
                                                                let _ = weak_root.update(
                                                                    cx,
                                                                    |this, cx| {
                                                                        this.delete_item(
                                                                            item_key, window, cx,
                                                                        );
                                                                    },
                                                                );
                                                            }),
                                                        )
                                                    })
                                                }),
                                            )),
                                        )
                                    })
                            }),
                    ),
                )
            })
    }
}

pub(super) fn render_collection_menu_item(
    collection: &CollectionTree,
    active_key: Option<ItemKey>,
    cx: &mut gpui::Context<AppRoot>,
) -> SidebarMenuItem {
    let collection_key = ItemKey::collection(collection.collection.id);
    let collection_id_for_new = collection.collection.id;
    let weak_root = cx.entity().downgrade();
    let is_linked = collection.collection.storage_kind == CollectionStorageKind::Linked;
    let linked_root_path = collection
        .collection
        .storage_config
        .linked_root_path
        .as_ref()
        .map(|path| path.display().to_string());
    let linked_health = collection.linked_health.clone();
    SidebarMenuItem::new(collection.collection.name.clone())
        .icon(Icon::new(IconName::BookOpen).small())
        .active(active_key == Some(collection_key))
        .default_open(true)
        .click_to_open(true)
        .when(is_linked, |item| {
            let root_line = match linked_root_path.clone() {
                Some(path) => format!(
                    "{} {path}",
                    es_fluent::localize("sidebar_linked_collection_badge_root", None),
                ),
                None => es_fluent::localize("sidebar_linked_collection_badge_root_missing", None),
            };
            let (status_line, icon_name) = match linked_health.clone() {
                Some(LinkedCollectionHealth::Healthy) => (
                    format!(
                        "{} {}",
                        es_fluent::localize("sidebar_linked_collection_badge_status", None),
                        es_fluent::localize("sidebar_linked_collection_badge_status_ok", None),
                    ),
                    IconName::Github,
                ),
                Some(LinkedCollectionHealth::MissingRootPath) => (
                    format!(
                        "{} {}",
                        es_fluent::localize("sidebar_linked_collection_badge_status", None),
                        es_fluent::localize(
                            "sidebar_linked_collection_badge_status_missing_root",
                            None
                        ),
                    ),
                    IconName::Info,
                ),
                Some(LinkedCollectionHealth::Unavailable { reason }) => (
                    format!(
                        "{} {} ({reason})",
                        es_fluent::localize("sidebar_linked_collection_badge_status", None),
                        es_fluent::localize(
                            "sidebar_linked_collection_badge_status_unavailable",
                            None
                        ),
                    ),
                    IconName::Info,
                ),
                None => (
                    format!(
                        "{} {}",
                        es_fluent::localize("sidebar_linked_collection_badge_status", None),
                        es_fluent::localize("sidebar_linked_collection_badge_status_ok", None),
                    ),
                    IconName::Github,
                ),
            };
            let tooltip = format!(
                "{}\n{}\n{}\n{}",
                es_fluent::localize("sidebar_linked_collection_badge_tooltip", None),
                root_line,
                status_line,
                es_fluent::localize("sidebar_linked_collection_badge_actions_hint", None),
            );
            let collection_id = collection.collection.id;
            item.suffix(move |_, _| {
                div()
                    .id(format!("linked-collection-badge-{}", collection_id))
                    .child(Icon::new(icon_name.clone()).small())
                    .tooltip({
                        let tooltip = tooltip.clone();
                        move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx)
                    })
            })
        })
        .on_click(cx.listener(move |this, _, _, cx| {
            this.open_item(collection_key, cx);
        }))
        .context_menu(move |menu, _, _| {
            let weak_root = weak_root.clone();
            let weak_root_new = weak_root.clone();
            let weak_root_new_folder = weak_root.clone();
            let menu = menu.item(
                PopupMenuItem::new(es_fluent::localize("menu_new_request", None))
                    .icon(Icon::new(IconName::Plus))
                    .on_click(move |_, window, cx| {
                        let _ = weak_root_new.update(cx, |this, cx| {
                            this.open_auto_saved_request(collection_id_for_new, window, cx);
                        });
                    }),
            );
            let menu = menu.item(
                PopupMenuItem::new(es_fluent::localize("menu_new_folder", None))
                    .icon(Icon::new(IconName::Plus))
                    .on_click(move |_, window, cx| {
                        let _ = weak_root_new_folder.update(cx, |this, cx| {
                            if let Err(err) = this.create_folder(collection_id_for_new, None, cx) {
                                window.push_notification(err.clone(), cx);
                                tracing::error!("failed to create folder: {err}");
                            }
                        });
                    }),
            );
            let menu = if let Some(linked_root_path) = linked_root_path.clone() {
                menu.item(
                    PopupMenuItem::new(es_fluent::localize("menu_copy_linked_root_path", None))
                        .icon(Icon::new(IconName::Copy))
                        .on_click(move |_, window, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                linked_root_path.clone(),
                            ));
                            window.push_notification(
                                es_fluent::localize("copy_linked_root_path_success", None),
                                cx,
                            );
                        }),
                )
            } else {
                menu
            };
            menu.item(
                PopupMenuItem::new(es_fluent::localize("menu_delete", None))
                    .icon(Icon::new(IconName::Close))
                    .on_click(move |_, window, cx| {
                        let _ = weak_root.update(cx, |this, cx| {
                            this.delete_item(collection_key, window, cx);
                        });
                    }),
            )
        })
        .children(
            collection
                .children
                .iter()
                .map(|item| render_tree_item(item, active_key, cx)),
        )
}

fn render_tree_item(
    item: &TreeItem,
    active_key: Option<ItemKey>,
    cx: &mut gpui::Context<AppRoot>,
) -> SidebarMenuItem {
    match item {
        TreeItem::Folder(folder) => render_folder_menu_item(folder, active_key, cx),
        TreeItem::Request(request) => {
            let request_key = ItemKey::request(request.id);
            let request_id = request.id;
            let request_name = request.name.clone();
            let weak_root = cx.entity().downgrade();
            SidebarMenuItem::new(request.name.clone())
                .icon(Icon::new(IconName::File).small())
                .active(active_key == Some(request_key))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_item(request_key, cx);
                }))
                .context_menu(move |menu, _, _| {
                    let weak_root_dup = weak_root.clone();
                    let weak_root_del = weak_root.clone();
                    let dup_name = request_name.clone();
                    menu.item(
                        PopupMenuItem::new(es_fluent::localize("menu_duplicate", None))
                            .icon(Icon::new(IconName::Copy))
                            .on_click(move |_, window, cx| {
                                let _ = weak_root_dup.update(cx, |this, cx| {
                                    this.duplicate_request(
                                        request_id,
                                        dup_name.clone(),
                                        window,
                                        cx,
                                    );
                                });
                            }),
                    )
                    .item(
                        PopupMenuItem::new(es_fluent::localize("menu_delete", None))
                            .icon(Icon::new(IconName::Close))
                            .on_click(move |_, window, cx| {
                                let _ = weak_root_del.update(cx, |this, cx| {
                                    this.delete_item(request_key, window, cx);
                                });
                            }),
                    )
                })
        }
    }
}

fn render_folder_menu_item(
    folder: &FolderTree,
    active_key: Option<ItemKey>,
    cx: &mut gpui::Context<AppRoot>,
) -> SidebarMenuItem {
    let folder_key = ItemKey::folder(folder.folder.id);
    let collection_id_for_new = folder.folder.collection_id;
    let parent_folder_id_for_new = folder.folder.id;
    let weak_root = cx.entity().downgrade();
    SidebarMenuItem::new(folder.folder.name.clone())
        .icon(Icon::new(IconName::Folder).small())
        .active(active_key == Some(folder_key))
        .default_open(true)
        .click_to_open(true)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.open_item(folder_key, cx);
        }))
        .context_menu(move |menu, _, _| {
            let weak_root = weak_root.clone();
            let weak_root_new_request = weak_root.clone();
            let weak_root_new = weak_root.clone();
            menu.item(
                PopupMenuItem::new(es_fluent::localize("menu_new_request", None))
                    .icon(Icon::new(IconName::Plus))
                    .on_click(move |_, window, cx| {
                        let _ = weak_root_new_request.update(cx, |this, cx| {
                            this.open_auto_saved_request_in_folder(
                                collection_id_for_new,
                                parent_folder_id_for_new,
                                window,
                                cx,
                            );
                        });
                    }),
            )
            .item(
                PopupMenuItem::new(es_fluent::localize("menu_new_folder", None))
                    .icon(Icon::new(IconName::Plus))
                    .on_click(move |_, window, cx| {
                        let _ = weak_root_new.update(cx, |this, cx| {
                            if let Err(err) = this.create_folder(
                                collection_id_for_new,
                                Some(parent_folder_id_for_new),
                                cx,
                            ) {
                                window.push_notification(err.clone(), cx);
                                tracing::error!("failed to create folder: {err}");
                            }
                        });
                    }),
            )
            .item(
                PopupMenuItem::new(es_fluent::localize("menu_delete", None))
                    .icon(Icon::new(IconName::Close))
                    .on_click(move |_, window, cx| {
                        let _ = weak_root.update(cx, |this, cx| {
                            this.delete_item(folder_key, window, cx);
                        });
                    }),
            )
        })
        .children(
            folder
                .children
                .iter()
                .map(|item| render_tree_item(item, active_key, cx)),
        )
}
