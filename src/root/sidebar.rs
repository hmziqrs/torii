use super::AppRoot;
use crate::{
    services::workspace_tree::{CollectionTree, FolderTree, TreeItem},
    session::{item_key::ItemKey, window_layout::SidebarSection},
};
use gpui::prelude::*;
use gpui_component::{
    Icon, IconName, Sizable as _,
    menu::PopupMenuItem,
    sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem},
};

impl AppRoot {
    pub(super) fn render_sidebar(
        &self,
        active_key: Option<ItemKey>,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let selected_workspace_id = self.session.read(cx).selected_workspace_id;
        let weak_root = cx.entity().downgrade();

        Sidebar::new("app-sidebar")
            .w(gpui::relative(1.))
            .border_0()
            .child(
                SidebarGroup::new(es_fluent::localize("sidebar_workspaces", None)).child(
                    SidebarMenu::new().children(self.catalog.workspaces.iter().map(|workspace| {
                        let item_key = ItemKey::workspace(workspace.id);
                        let weak_root = weak_root.clone();
                        SidebarMenuItem::new(workspace.name.clone())
                            .icon(Icon::new(IconName::Inbox).small())
                            .active(
                                active_key == Some(item_key)
                                    || selected_workspace_id == Some(workspace.id),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_item(item_key, cx);
                            }))
                            .context_menu(move |menu, _, _| {
                                let weak_root = weak_root.clone();
                                menu.item(
                                    PopupMenuItem::new(es_fluent::localize("menu_delete", None))
                                        .icon(Icon::new(IconName::Close))
                                        .on_click(move |_, window, cx| {
                                            let _ = weak_root.update(cx, |this, cx| {
                                                this.delete_item(item_key, window, cx);
                                            });
                                        }),
                                )
                            })
                    })),
                ),
            )
            .when_some(self.catalog.selected_workspace(), |sidebar, workspace| {
                let section = self.session.read(cx).window_layout.sidebar_section;
                let is_collections = section == SidebarSection::Collections;
                sidebar
                    // Section switcher row — two SidebarMenuItems acting as tab buttons
                    .child(
                        SidebarGroup::new("").child(
                            SidebarMenu::new()
                                .child(
                                    SidebarMenuItem::new("Collections")
                                        .icon(Icon::new(IconName::BookOpen).small())
                                        .active(is_collections)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.session.update(cx, |session, cx| {
                                                session.window_layout.sidebar_section =
                                                    SidebarSection::Collections;
                                                cx.notify();
                                            });
                                        })),
                                )
                                .child(
                                    SidebarMenuItem::new("Environments")
                                        .icon(Icon::new(IconName::Globe).small())
                                        .active(!is_collections)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.session.update(cx, |session, cx| {
                                                session.window_layout.sidebar_section =
                                                    SidebarSection::Environments;
                                                cx.notify();
                                            });
                                        })),
                                ),
                        ),
                    )
                    // Collections section (gated)
                    .when(is_collections, |sidebar| {
                        sidebar.child(
                            SidebarGroup::new(es_fluent::localize("sidebar_collections", None))
                                .child(SidebarMenu::new().children(
                                    workspace.collections.iter().map(|collection| {
                                        render_collection_menu_item(collection, active_key, cx)
                                    }),
                                )),
                        )
                    })
                    // Environments section (gated)
                    .when(!is_collections, |sidebar| {
                        sidebar.child(
                            SidebarGroup::new(es_fluent::localize("sidebar_environments", None))
                                .child(SidebarMenu::new().children(
                                    workspace.environments.iter().map(|environment| {
                                        let item_key = ItemKey::environment(environment.id);
                                        let weak_root = weak_root.clone();
                                        SidebarMenuItem::new(environment.name.clone())
                                            .icon(Icon::new(IconName::Globe).small())
                                            .active(active_key == Some(item_key))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.open_item(item_key, cx);
                                            }))
                                            .context_menu(move |menu, _, _| {
                                                let weak_root = weak_root.clone();
                                                menu.item(
                                                    PopupMenuItem::new(es_fluent::localize(
                                                        "menu_delete",
                                                        None,
                                                    ))
                                                    .icon(Icon::new(IconName::Close))
                                                    .on_click(move |_, window, cx| {
                                                        let _ = weak_root.update(cx, |this, cx| {
                                                            this.delete_item(item_key, window, cx);
                                                        });
                                                    }),
                                                )
                                            })
                                    }),
                                )),
                        )
                    })
            })
            .child(
                SidebarGroup::new(es_fluent::localize("sidebar_utilities", None)).child(
                    SidebarMenu::new()
                        .child(
                            SidebarMenuItem::new(es_fluent::localize("tab_kind_settings", None))
                                .icon(Icon::new(IconName::Settings2).small())
                                .active(active_key == Some(ItemKey::settings()))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.open_item(ItemKey::settings(), cx);
                                })),
                        )
                        .child(
                            SidebarMenuItem::new(es_fluent::localize("tab_kind_about", None))
                                .icon(Icon::new(IconName::Info).small())
                                .active(active_key == Some(ItemKey::about()))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.open_item(ItemKey::about(), cx);
                                })),
                        )
                        .child(
                            SidebarMenuItem::new(es_fluent::localize(
                                "tab_kind_layout_debug",
                                None,
                            ))
                            .icon(Icon::new(IconName::Settings2).small())
                            .active(active_key == Some(ItemKey::layout_debug()))
                            .on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.open_item(ItemKey::layout_debug(), cx);
                                },
                            )),
                        ),
                ),
            )
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
    SidebarMenuItem::new(collection.collection.name.clone())
        .icon(Icon::new(IconName::BookOpen).small())
        .active(active_key == Some(collection_key))
        .default_open(true)
        .click_to_open(true)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.open_item(collection_key, cx);
        }))
        .context_menu(move |menu, _, _| {
            let weak_root = weak_root.clone();
            let weak_root_new = weak_root.clone();
            menu.item(
                PopupMenuItem::new(es_fluent::localize("menu_new_request", None))
                    .icon(Icon::new(IconName::Plus))
                    .on_click(move |_, window, cx| {
                        let _ = weak_root_new.update(cx, |this, cx| {
                            this.open_draft_request(collection_id_for_new, window, cx);
                        });
                    }),
            )
            .item(
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
            menu.item(
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
