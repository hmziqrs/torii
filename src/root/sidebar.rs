use super::{AppRoot, services};
use crate::{
    domain::collection::{Collection, CollectionStorageKind},
    domain::ids::{CollectionId, FolderId, RequestId, WorkspaceId},
    infra::linked_collection_format::{
        LinkedCollectionState, LinkedSiblingId, read_linked_collection, write_linked_collection,
    },
    services::workspace_tree::{CollectionTree, FolderTree, LinkedCollectionHealth, TreeItem},
    session::{item_key::ItemKey, window_layout::SidebarSection},
};
use gpui::{
    AnyElement, App, InteractiveElement as _, Render, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Window, div, prelude::*, px, relative,
};
use gpui_component::{
    ActiveTheme as _, Collapsible, Icon, IconName, Selectable as _, Sizable as _, StyledExt as _,
    WindowExt as _,
    button::{Button, ButtonRounded, ButtonVariants as _},
    h_flex,
    hover_card::HoverCard,
    menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem},
    scroll::ScrollableElement as _,
    sidebar::{Sidebar, SidebarGroup, SidebarItem, SidebarMenu, SidebarMenuItem},
    v_flex,
};
use std::time::Duration;

#[derive(Clone)]
enum TreeDragPayload {
    Collection(CollectionId),
    Folder(FolderId),
    Request(RequestId),
}

#[derive(Clone, Copy)]
enum TreeDropTarget {
    Collection(CollectionId),
    Folder(FolderId),
}

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
                        Sidebar::<AppSidebarNode>::new("app-sidebar-content")
                            .collapsible(false)
                            .w(gpui::relative(1.))
                            .border_0()
                            .child(AppSidebarNode::GroupMenu(
                                SidebarGroup::new(es_fluent::localize("sidebar_workspaces", None)).child(
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
                                                    let workspace_name = workspace.name.clone();
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
                                                            let weak_root_rename = weak_root.clone();
                                                            let workspace_name_for_rename =
                                                                workspace_name.clone();
                                                            menu.item(
                                                                PopupMenuItem::new(
                                                                    es_fluent::localize(
                                                                        "menu_rename",
                                                                        None,
                                                                    ),
                                                                )
                                                                .on_click(move |_, window, cx| {
                                                                    let _ = weak_root_rename
                                                                        .update(cx, |this, cx| {
                                                                            this.open_rename_item_dialog(
                                                                                item_key,
                                                                                workspace_name_for_rename.clone(),
                                                                                window,
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
                            ))
                            .when_some(self.catalog.selected_workspace(), |sidebar, workspace| {
                                // Collections section (gated)
                                sidebar
                                    .when(is_collections, |sidebar| {
                                        let weak_root_tree = weak_root.clone();
                                        sidebar.child(AppSidebarNode::GroupTree(
                                            SidebarGroup::new(es_fluent::localize(
                                                "sidebar_collections",
                                                None,
                                            ))
                                            .child(CollectionTreeMenu::new(
                                                workspace.collections.clone(),
                                                active_key,
                                                weak_root_tree,
                                            )),
                                        ))
                                    })
                                    // Environments section (gated)
                                    .when(!is_collections, |sidebar| {
                                        sidebar.child(AppSidebarNode::GroupMenu(
                                            SidebarGroup::new(es_fluent::localize(
                                                "sidebar_environments",
                                                None,
                                            ))
                                            .child(SidebarMenu::new().children(
                                                workspace.environments.iter().map(|environment| {
                                                    let environment_id = environment.id;
                                                    let environment_name =
                                                        environment.name.clone();
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
                                                        let weak_root_rename = weak_root.clone();
                                                        let weak_root_delete = weak_root.clone();
                                                        let environment_name_for_rename =
                                                            environment_name.clone();
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
                                                                    "menu_rename",
                                                                    None,
                                                                ),
                                                            )
                                                            .on_click(move |_, window, cx| {
                                                                let _ = weak_root_rename.update(
                                                                    cx,
                                                                    |this, cx| {
                                                                        this.open_rename_item_dialog(
                                                                            item_key,
                                                                            environment_name_for_rename.clone(),
                                                                            window,
                                                                            cx,
                                                                        );
                                                                    },
                                                                );
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
                                                                let _ = weak_root_delete.update(
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
                                        ))
                                    })
                            }),
                    ),
                )
            })
    }
}

#[derive(Clone)]
enum AppSidebarNode {
    GroupMenu(SidebarGroup<SidebarMenu>),
    GroupTree(SidebarGroup<CollectionTreeMenu>),
}

impl Collapsible for AppSidebarNode {
    fn is_collapsed(&self) -> bool {
        match self {
            Self::GroupMenu(group) => group.is_collapsed(),
            Self::GroupTree(group) => group.is_collapsed(),
        }
    }

    fn collapsed(self, collapsed: bool) -> Self {
        match self {
            Self::GroupMenu(group) => Self::GroupMenu(group.collapsed(collapsed)),
            Self::GroupTree(group) => Self::GroupTree(group.collapsed(collapsed)),
        }
    }
}

impl SidebarItem for AppSidebarNode {
    fn render(
        self,
        id: impl Into<gpui::ElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        match self {
            Self::GroupMenu(group) => group.render(id, window, cx).into_any_element(),
            Self::GroupTree(group) => group.render(id, window, cx).into_any_element(),
        }
    }
}

#[derive(Clone)]
struct CollectionTreeMenu {
    collections: Vec<CollectionTree>,
    active_key: Option<ItemKey>,
    weak_root: gpui::WeakEntity<AppRoot>,
    collapsed: bool,
}

impl CollectionTreeMenu {
    fn new(
        collections: Vec<CollectionTree>,
        active_key: Option<ItemKey>,
        weak_root: gpui::WeakEntity<AppRoot>,
    ) -> Self {
        Self {
            collections,
            active_key,
            weak_root,
            collapsed: false,
        }
    }
}

impl Collapsible for CollectionTreeMenu {
    fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }
}

impl SidebarItem for CollectionTreeMenu {
    fn render(
        self,
        id: impl Into<gpui::ElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let id = id.into();
        v_flex().id(id).gap_1().children(
            self.collections
                .into_iter()
                .map(|collection| {
                    render_collection_tree_row(
                        &collection,
                        self.active_key,
                        &self.weak_root,
                        0,
                        window,
                        cx,
                    )
                })
                .collect::<Vec<_>>(),
        )
    }
}

fn render_collection_tree_row(
    collection: &CollectionTree,
    active_key: Option<ItemKey>,
    weak_root: &gpui::WeakEntity<AppRoot>,
    depth: usize,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let collection_id = collection.collection.id;
    let item_key = ItemKey::collection(collection_id);
    let weak_root_click = weak_root.clone();
    let weak_root_drop = weak_root.clone();
    let weak_root_menu_new = weak_root.clone();
    let weak_root_menu_new_folder = weak_root.clone();
    let weak_root_menu_rename = weak_root.clone();
    let weak_root_menu_delete = weak_root.clone();
    let collection_name = collection.collection.name.clone();
    let linked_root_path_for_menu = collection
        .collection
        .storage_config
        .linked_root_path
        .as_ref()
        .map(|path| path.display().to_string());
    let payload = TreeDragPayload::Collection(collection_id);
    let drop_target = TreeDropTarget::Collection(collection_id);

    let row = tree_row_base(depth, active_key == Some(item_key), cx)
        .id(format!("tree-collection-row-{}", collection_id))
        .on_click(move |_, _, cx| {
            let _ = weak_root_click.update(cx, |this, cx| this.open_item(item_key, cx));
        })
        .on_drag(payload.clone(), {
            let title = collection.collection.name.clone();
            move |_, _, _, cx: &mut App| {
                cx.new(|_| DragTreePreview::new(title.clone(), IconName::BookOpen))
            }
        })
        .drag_over::<TreeDragPayload>(move |style: StyleRefinement, _, _, _| {
            style.border_1().border_color(gpui::rgb(0x2563EB))
        })
        .on_drop(move |dragged: &TreeDragPayload, window, cx| {
            let result = weak_root_drop
                .update(cx, |this, cx| {
                    this.apply_tree_drop(dragged.clone(), drop_target, cx)
                })
                .unwrap_or_else(|_| Err("workspace view was closed".to_string()));
            if let Err(err) = result {
                window.push_notification(err, cx);
            }
        })
        .context_menu(move |menu: PopupMenu, _, _| {
            let menu = menu.item(
                PopupMenuItem::new(es_fluent::localize("menu_new_request", None))
                    .icon(Icon::new(IconName::Plus))
                    .on_click({
                        let weak_root_menu_new = weak_root_menu_new.clone();
                        move |_, window, cx| {
                            let _ = weak_root_menu_new.update(cx, |this, cx| {
                                this.open_auto_saved_request(collection_id, window, cx);
                            });
                        }
                    }),
            );
            let menu = menu.item(
                PopupMenuItem::new(es_fluent::localize("menu_new_folder", None))
                    .icon(Icon::new(IconName::Plus))
                    .on_click({
                        let weak_root_menu_new_folder = weak_root_menu_new_folder.clone();
                        move |_, window, cx| {
                            let _ = weak_root_menu_new_folder.update(cx, |this, cx| {
                                if let Err(err) = this.create_folder(collection_id, None, cx) {
                                    window.push_notification(err.clone(), cx);
                                    tracing::error!("failed to create folder: {err}");
                                }
                            });
                        }
                    }),
            );
            let menu = menu.item(
                PopupMenuItem::new(es_fluent::localize("menu_rename", None)).on_click({
                    let weak_root_menu_rename = weak_root_menu_rename.clone();
                    let collection_name = collection_name.clone();
                    move |_, window, cx| {
                        let _ = weak_root_menu_rename.update(cx, |this, cx| {
                            this.open_rename_item_dialog(
                                item_key,
                                collection_name.clone(),
                                window,
                                cx,
                            );
                        });
                    }
                }),
            );
            let menu = if let Some(linked_root_path) = linked_root_path_for_menu.clone() {
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
                    .on_click({
                        let weak_root_menu_delete = weak_root_menu_delete.clone();
                        move |_, window, cx| {
                            let _ = weak_root_menu_delete.update(cx, |this, cx| {
                                this.delete_item(item_key, window, cx);
                            });
                        }
                    }),
            )
        })
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(Icon::new(IconName::BookOpen).small())
                        .child(div().text_sm().child(collection.collection.name.clone())),
                )
                .when_some(render_linked_badge(collection), |this: gpui::Div, badge| {
                    this.child(badge)
                }),
        );

    let children = v_flex().gap_1().children(
        collection
            .children
            .iter()
            .map(|item| render_tree_item_row(item, active_key, weak_root, depth + 1, cx))
            .collect::<Vec<_>>(),
    );

    v_flex()
        .gap_1()
        .child(row)
        .child(children)
        .into_any_element()
}

fn render_tree_item_row(
    item: &TreeItem,
    active_key: Option<ItemKey>,
    weak_root: &gpui::WeakEntity<AppRoot>,
    depth: usize,
    cx: &mut App,
) -> AnyElement {
    match item {
        TreeItem::Folder(folder) => {
            render_folder_tree_row(folder, active_key, weak_root, depth, cx)
        }
        TreeItem::Request(request) => {
            render_request_tree_row(request, active_key, weak_root, depth, cx)
        }
    }
}

fn render_folder_tree_row(
    folder: &FolderTree,
    active_key: Option<ItemKey>,
    weak_root: &gpui::WeakEntity<AppRoot>,
    depth: usize,
    cx: &mut App,
) -> AnyElement {
    let folder_id = folder.folder.id;
    let item_key = ItemKey::folder(folder_id);
    let weak_root_click = weak_root.clone();
    let weak_root_drop = weak_root.clone();
    let weak_root_menu_new = weak_root.clone();
    let weak_root_menu_new_request = weak_root.clone();
    let weak_root_menu_rename = weak_root.clone();
    let weak_root_menu_delete = weak_root.clone();
    let payload = TreeDragPayload::Folder(folder_id);
    let drop_target = TreeDropTarget::Folder(folder_id);
    let folder_name = folder.folder.name.clone();
    let collection_id = folder.folder.collection_id;

    let row = tree_row_base(depth, active_key == Some(item_key), cx)
        .id(format!("tree-folder-row-{}", folder_id))
        .on_click(move |_, _, cx| {
            let _ = weak_root_click.update(cx, |this, cx| this.open_item(item_key, cx));
        })
        .on_drag(payload.clone(), {
            let title = folder.folder.name.clone();
            move |_, _, _, cx: &mut App| {
                cx.new(|_| DragTreePreview::new(title.clone(), IconName::Folder))
            }
        })
        .drag_over::<TreeDragPayload>(move |style: StyleRefinement, _, _, _| {
            style.border_1().border_color(gpui::rgb(0x2563EB))
        })
        .on_drop(move |dragged: &TreeDragPayload, window, cx| {
            let result = weak_root_drop
                .update(cx, |this, cx| {
                    this.apply_tree_drop(dragged.clone(), drop_target, cx)
                })
                .unwrap_or_else(|_| Err("workspace view was closed".to_string()));
            if let Err(err) = result {
                window.push_notification(err, cx);
            }
        })
        .context_menu(move |menu: PopupMenu, _, _| {
            menu.item(
                PopupMenuItem::new(es_fluent::localize("menu_new_request", None))
                    .icon(Icon::new(IconName::Plus))
                    .on_click({
                        let weak_root_menu_new_request = weak_root_menu_new_request.clone();
                        move |_, window, cx| {
                            let _ = weak_root_menu_new_request.update(cx, |this, cx| {
                                this.open_auto_saved_request_in_folder(
                                    collection_id,
                                    folder_id,
                                    window,
                                    cx,
                                );
                            });
                        }
                    }),
            )
            .item(
                PopupMenuItem::new(es_fluent::localize("menu_new_folder", None))
                    .icon(Icon::new(IconName::Plus))
                    .on_click({
                        let weak_root_menu_new = weak_root_menu_new.clone();
                        move |_, window, cx| {
                            let _ = weak_root_menu_new.update(cx, |this, cx| {
                                if let Err(err) =
                                    this.create_folder(collection_id, Some(folder_id), cx)
                                {
                                    window.push_notification(err.clone(), cx);
                                    tracing::error!("failed to create folder: {err}");
                                }
                            });
                        }
                    }),
            )
            .item(
                PopupMenuItem::new(es_fluent::localize("menu_rename", None)).on_click({
                    let weak_root_menu_rename = weak_root_menu_rename.clone();
                    let folder_name = folder_name.clone();
                    move |_, window, cx| {
                        let _ = weak_root_menu_rename.update(cx, |this, cx| {
                            this.open_rename_item_dialog(item_key, folder_name.clone(), window, cx);
                        });
                    }
                }),
            )
            .item(
                PopupMenuItem::new(es_fluent::localize("menu_delete", None))
                    .icon(Icon::new(IconName::Close))
                    .on_click({
                        let weak_root_menu_delete = weak_root_menu_delete.clone();
                        move |_, window, cx| {
                            let _ = weak_root_menu_delete.update(cx, |this, cx| {
                                this.delete_item(item_key, window, cx);
                            });
                        }
                    }),
            )
        })
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(Icon::new(IconName::Folder).small())
                .child(div().text_sm().child(folder.folder.name.clone())),
        );

    let children = v_flex().gap_1().children(
        folder
            .children
            .iter()
            .map(|item| render_tree_item_row(item, active_key, weak_root, depth + 1, cx))
            .collect::<Vec<_>>(),
    );

    v_flex()
        .gap_1()
        .child(row)
        .child(children)
        .into_any_element()
}

fn render_request_tree_row(
    request: &crate::domain::request::RequestItem,
    active_key: Option<ItemKey>,
    weak_root: &gpui::WeakEntity<AppRoot>,
    depth: usize,
    cx: &mut App,
) -> AnyElement {
    let request_id = request.id;
    let item_key = ItemKey::request(request_id);
    let weak_root_click = weak_root.clone();
    let weak_root_menu_dup = weak_root.clone();
    let weak_root_menu_delete = weak_root.clone();
    let payload = TreeDragPayload::Request(request_id);
    let request_name = request.name.clone();

    tree_row_base(depth, active_key == Some(item_key), cx)
        .id(format!("tree-request-row-{}", request_id))
        .on_click(move |_, _, cx| {
            let _ = weak_root_click.update(cx, |this, cx| this.open_item(item_key, cx));
        })
        .on_drag(payload, {
            let title = request.name.clone();
            move |_, _, _, cx: &mut App| {
                cx.new(|_| DragTreePreview::new(title.clone(), IconName::File))
            }
        })
        .context_menu(move |menu: PopupMenu, _, _| {
            menu.item(
                PopupMenuItem::new(es_fluent::localize("menu_duplicate", None))
                    .icon(Icon::new(IconName::Copy))
                    .on_click({
                        let weak_root_menu_dup = weak_root_menu_dup.clone();
                        let request_name = request_name.clone();
                        move |_, window, cx| {
                            let _ = weak_root_menu_dup.update(cx, |this, cx| {
                                this.duplicate_request(
                                    request_id,
                                    request_name.clone(),
                                    window,
                                    cx,
                                );
                            });
                        }
                    }),
            )
            .item(
                PopupMenuItem::new(es_fluent::localize("menu_delete", None))
                    .icon(Icon::new(IconName::Close))
                    .on_click({
                        let weak_root_menu_delete = weak_root_menu_delete.clone();
                        move |_, window, cx| {
                            let _ = weak_root_menu_delete.update(cx, |this, cx| {
                                this.delete_item(item_key, window, cx);
                            });
                        }
                    }),
            )
        })
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(Icon::new(IconName::File).small())
                .child(div().text_sm().child(request.name.clone())),
        )
        .into_any_element()
}

fn tree_row_base(depth: usize, is_active: bool, cx: &mut App) -> gpui::Div {
    div()
        .w_full()
        .h_7()
        .rounded(cx.theme().radius)
        .px_2()
        .pl(px((depth as f32 * 16.0) + 8.0))
        .items_center()
        .when(is_active, |this| {
            this.font_medium()
                .bg(cx.theme().sidebar_accent)
                .text_color(cx.theme().sidebar_accent_foreground)
        })
        .when(!is_active, |this| {
            this.hover(|this| {
                this.bg(cx.theme().sidebar_accent.opacity(0.8))
                    .text_color(cx.theme().sidebar_accent_foreground)
            })
        })
}

fn render_linked_badge(collection: &CollectionTree) -> Option<AnyElement> {
    if collection.collection.storage_kind != CollectionStorageKind::Linked {
        return None;
    }
    let collection_id = collection.collection.id;
    let linked_root_path = collection
        .collection
        .storage_config
        .linked_root_path
        .as_ref()
        .map(|path| path.display().to_string());
    let root_line = match linked_root_path.clone() {
        Some(path) => format!(
            "{} {path}",
            es_fluent::localize("sidebar_linked_collection_badge_root", None),
        ),
        None => es_fluent::localize("sidebar_linked_collection_badge_root_missing", None),
    };
    let (status_line, icon_name) = match collection.linked_health.clone() {
        Some(LinkedCollectionHealth::Healthy) | None => (
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
                es_fluent::localize("sidebar_linked_collection_badge_status_missing_root", None),
            ),
            IconName::Info,
        ),
        Some(LinkedCollectionHealth::Unavailable { reason }) => (
            format!(
                "{} {} ({reason})",
                es_fluent::localize("sidebar_linked_collection_badge_status", None),
                es_fluent::localize("sidebar_linked_collection_badge_status_unavailable", None),
            ),
            IconName::Info,
        ),
    };

    Some(
        HoverCard::new(SharedString::from(format!(
            "linked-collection-badge-{collection_id}"
        )))
        .open_delay(Duration::from_millis(120))
        .close_delay(Duration::from_millis(180))
        .trigger(
            div()
                .id(format!("linked-collection-badge-trigger-{}", collection_id))
                .child(Icon::new(icon_name).small()),
        )
        .content({
            let root_line = root_line.clone();
            let status_line = status_line.clone();
            let copy_button_path = linked_root_path.clone();
            move |_, _window, _cx| {
                v_flex()
                    .w(px(320.))
                    .gap_2()
                    .p_3()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(es_fluent::localize(
                                "sidebar_linked_collection_badge_tooltip",
                                None,
                            )),
                    )
                    .child(div().text_xs().child(root_line.clone()))
                    .child(div().text_xs().child(status_line.clone()))
                    .when_some(copy_button_path.clone(), |this, path| {
                        this.child(
                            Button::new(format!(
                                "linked-collection-copy-root-path-{collection_id}"
                            ))
                            .xsmall()
                            .label(es_fluent::localize("menu_copy_linked_root_path", None))
                            .on_click(move |_, window, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                    path.clone(),
                                ));
                                window.push_notification(
                                    es_fluent::localize("copy_linked_root_path_success", None),
                                    cx,
                                );
                            }),
                        )
                    })
                    .into_any_element()
            }
        })
        .into_any_element(),
    )
}

struct DragTreePreview {
    title: SharedString,
    icon: IconName,
}

impl DragTreePreview {
    fn new(title: impl Into<SharedString>, icon: IconName) -> Self {
        Self {
            title: title.into(),
            icon,
        }
    }
}

impl Render for DragTreePreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        h_flex()
            .px_2()
            .py_1()
            .gap_2()
            .rounded_sm()
            .bg(gpui::rgb(0x1F2937))
            .text_color(gpui::rgb(0xF9FAFB))
            .child(Icon::new(self.icon.clone()).small())
            .child(div().text_sm().child(self.title.clone()))
    }
}

impl AppRoot {
    fn apply_tree_drop(
        &mut self,
        dragged: TreeDragPayload,
        target: TreeDropTarget,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        match dragged {
            TreeDragPayload::Collection(id) => self.drop_collection(id, target, cx),
            TreeDragPayload::Folder(id) => self.drop_folder(id, target, cx),
            TreeDragPayload::Request(id) => self.drop_request(id, target, cx),
        }
    }

    fn drop_collection(
        &mut self,
        dragged: CollectionId,
        target: TreeDropTarget,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        let TreeDropTarget::Collection(target_collection_id) = target else {
            return Err("collection can only be dropped onto another collection".to_string());
        };
        if dragged == target_collection_id {
            return Ok(());
        }

        let Some(workspace) = self.catalog.selected_workspace() else {
            return Err("no selected workspace".to_string());
        };
        let workspace_id: WorkspaceId = workspace.workspace.id;
        let mut ordered = workspace
            .collections
            .iter()
            .map(|collection| collection.collection.id)
            .collect::<Vec<_>>();
        let Some(source_idx) = ordered.iter().position(|id| *id == dragged) else {
            return Err("dragged collection no longer exists".to_string());
        };
        let Some(target_idx) = ordered.iter().position(|id| *id == target_collection_id) else {
            return Err("drop target collection no longer exists".to_string());
        };
        let moved = ordered.remove(source_idx);
        let insert_at = if source_idx < target_idx {
            target_idx.saturating_sub(1)
        } else {
            target_idx
        };
        ordered.insert(insert_at, moved);

        services(cx)
            .repos
            .collection
            .reorder_in_workspace(workspace_id, &ordered)
            .map_err(|err| format!("failed to reorder collections: {err}"))?;
        self.refresh_catalog(cx);
        self.persist_session_state(cx);
        Ok(())
    }

    fn drop_folder(
        &mut self,
        dragged_folder_id: FolderId,
        target: TreeDropTarget,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        let Some((source_collection_id, _)) = self.find_folder_location(dragged_folder_id) else {
            return Err("dragged folder no longer exists".to_string());
        };
        let (target_collection_id, target_parent) = self.resolve_drop_parent(target)?;
        if target_parent == Some(dragged_folder_id) {
            return Err("cannot drop a folder into itself".to_string());
        }

        if let Some(target_folder_id) = target_parent {
            if self.folder_is_descendant_of(target_folder_id, dragged_folder_id) {
                return Err("cannot drop a folder into its descendant".to_string());
            }
        }

        let storage_kind =
            self.ensure_same_storage_kind(source_collection_id, target_collection_id)?;
        match storage_kind {
            CollectionStorageKind::Managed => {
                services(cx)
                    .repos
                    .folder
                    .move_to(dragged_folder_id, target_collection_id, target_parent)
                    .map_err(|err| format!("failed to move folder: {err}"))?;
            }
            CollectionStorageKind::Linked => {
                if source_collection_id != target_collection_id {
                    return Err(
                        "cross-collection drag/drop for linked collections is not supported yet"
                            .to_string(),
                    );
                }
                self.move_linked_folder(dragged_folder_id, target_parent, target_collection_id)?;
            }
        }
        self.refresh_catalog(cx);
        self.persist_session_state(cx);
        Ok(())
    }

    fn drop_request(
        &mut self,
        dragged_request_id: RequestId,
        target: TreeDropTarget,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        let Some((source_collection_id, _)) = self.find_request_location(dragged_request_id) else {
            return Err("dragged request no longer exists".to_string());
        };
        let (target_collection_id, target_parent) = self.resolve_drop_parent(target)?;
        let storage_kind =
            self.ensure_same_storage_kind(source_collection_id, target_collection_id)?;
        match storage_kind {
            CollectionStorageKind::Managed => {
                services(cx)
                    .repos
                    .request
                    .move_to(dragged_request_id, target_collection_id, target_parent)
                    .map_err(|err| format!("failed to move request: {err}"))?;
            }
            CollectionStorageKind::Linked => {
                if source_collection_id != target_collection_id {
                    return Err(
                        "cross-collection drag/drop for linked collections is not supported yet"
                            .to_string(),
                    );
                }
                self.move_linked_request(dragged_request_id, target_parent, target_collection_id)?;
            }
        }
        self.refresh_catalog(cx);
        self.persist_session_state(cx);
        Ok(())
    }

    fn resolve_drop_parent(
        &self,
        target: TreeDropTarget,
    ) -> Result<(CollectionId, Option<FolderId>), String> {
        match target {
            TreeDropTarget::Collection(collection_id) => Ok((collection_id, None)),
            TreeDropTarget::Folder(folder_id) => self
                .find_folder_location(folder_id)
                .map(|(collection_id, _)| (collection_id, Some(folder_id)))
                .ok_or_else(|| "drop target folder no longer exists".to_string()),
        }
    }

    fn ensure_same_storage_kind(
        &self,
        source_collection_id: CollectionId,
        target_collection_id: CollectionId,
    ) -> Result<CollectionStorageKind, String> {
        let Some(workspace) = self.catalog.selected_workspace() else {
            return Err("no selected workspace".to_string());
        };
        let source = workspace
            .collections
            .iter()
            .find(|collection| collection.collection.id == source_collection_id)
            .ok_or_else(|| "source collection no longer exists".to_string())?;
        let target = workspace
            .collections
            .iter()
            .find(|collection| collection.collection.id == target_collection_id)
            .ok_or_else(|| "target collection no longer exists".to_string())?;
        if source.collection.storage_kind != target.collection.storage_kind {
            return Err(
                "cross-storage drag/drop is not supported (managed and linked cannot mix)"
                    .to_string(),
            );
        }
        Ok(source.collection.storage_kind)
    }

    fn find_folder_location(
        &self,
        folder_id: FolderId,
    ) -> Option<(CollectionId, Option<FolderId>)> {
        self.catalog.selected_workspace().and_then(|workspace| {
            workspace.collections.iter().find_map(|collection| {
                collection
                    .find_folder_tree(folder_id)
                    .map(|folder| (collection.collection.id, folder.folder.parent_folder_id))
            })
        })
    }

    fn find_request_location(
        &self,
        request_id: RequestId,
    ) -> Option<(CollectionId, Option<FolderId>)> {
        self.catalog.selected_workspace().and_then(|workspace| {
            workspace.collections.iter().find_map(|collection| {
                collection
                    .find_request(request_id)
                    .map(|request| (collection.collection.id, request.parent_folder_id))
            })
        })
    }

    fn folder_is_descendant_of(&self, candidate: FolderId, ancestor: FolderId) -> bool {
        fn contains_descendant(folder: &FolderTree, candidate: FolderId) -> bool {
            folder.children.iter().any(|item| match item {
                TreeItem::Folder(child) => {
                    child.folder.id == candidate || contains_descendant(child, candidate)
                }
                TreeItem::Request(_) => false,
            })
        }

        self.catalog.selected_workspace().is_some_and(|workspace| {
            workspace.collections.iter().any(|collection| {
                collection
                    .find_folder_tree(ancestor)
                    .is_some_and(|folder| contains_descendant(folder, candidate))
            })
        })
    }

    fn move_linked_folder(
        &self,
        dragged_folder_id: FolderId,
        target_parent: Option<FolderId>,
        collection_id: CollectionId,
    ) -> Result<(), String> {
        let collection = self.load_linked_collection_row(collection_id)?;
        let root_path = collection
            .storage_config
            .linked_root_path
            .clone()
            .ok_or_else(|| "linked collection is missing root path".to_string())?;
        let mut state = read_linked_collection(&root_path, &collection)
            .map_err(|err| format!("failed to read linked collection: {err}"))?;

        let source_idx = state
            .folders
            .iter()
            .position(|folder| folder.id == dragged_folder_id)
            .ok_or_else(|| "dragged folder no longer exists".to_string())?;
        let previous_parent = state.folders[source_idx].parent_folder_id;

        if target_parent == previous_parent {
            return Ok(());
        }

        detach_linked_sibling(
            &mut state,
            previous_parent,
            &LinkedSiblingId::Folder {
                id: dragged_folder_id.to_string(),
            },
        );
        attach_linked_sibling(
            &mut state,
            target_parent,
            LinkedSiblingId::Folder {
                id: dragged_folder_id.to_string(),
            },
        );

        let next_sort = next_linked_sibling_sort(&state, target_parent);
        state.folders[source_idx].parent_folder_id = target_parent;
        state.folders[source_idx].sort_order = next_sort;

        write_linked_collection(&root_path, &state)
            .map_err(|err| format!("failed to write linked collection: {err}"))?;
        Ok(())
    }

    fn move_linked_request(
        &self,
        dragged_request_id: RequestId,
        target_parent: Option<FolderId>,
        collection_id: CollectionId,
    ) -> Result<(), String> {
        let collection = self.load_linked_collection_row(collection_id)?;
        let root_path = collection
            .storage_config
            .linked_root_path
            .clone()
            .ok_or_else(|| "linked collection is missing root path".to_string())?;
        let mut state = read_linked_collection(&root_path, &collection)
            .map_err(|err| format!("failed to read linked collection: {err}"))?;

        let source_idx = state
            .requests
            .iter()
            .position(|request| request.id == dragged_request_id)
            .ok_or_else(|| "dragged request no longer exists".to_string())?;
        let previous_parent = state.requests[source_idx].parent_folder_id;

        if target_parent == previous_parent {
            return Ok(());
        }

        detach_linked_sibling(
            &mut state,
            previous_parent,
            &LinkedSiblingId::Request {
                id: dragged_request_id.to_string(),
            },
        );
        attach_linked_sibling(
            &mut state,
            target_parent,
            LinkedSiblingId::Request {
                id: dragged_request_id.to_string(),
            },
        );

        let next_sort = next_linked_sibling_sort(&state, target_parent);
        state.requests[source_idx].parent_folder_id = target_parent;
        state.requests[source_idx].sort_order = next_sort;

        write_linked_collection(&root_path, &state)
            .map_err(|err| format!("failed to write linked collection: {err}"))?;
        Ok(())
    }

    fn load_linked_collection_row(
        &self,
        collection_id: CollectionId,
    ) -> Result<Collection, String> {
        let Some(workspace) = self.catalog.selected_workspace() else {
            return Err("no selected workspace".to_string());
        };
        workspace
            .collections
            .iter()
            .find(|collection| collection.collection.id == collection_id)
            .map(|collection| collection.collection.clone())
            .ok_or_else(|| "collection no longer exists".to_string())
    }
}

fn detach_linked_sibling(
    state: &mut LinkedCollectionState,
    parent_folder_id: Option<FolderId>,
    sibling: &LinkedSiblingId,
) {
    match parent_folder_id {
        Some(parent_id) => {
            if let Some(children) = state.folder_child_orders.get_mut(&parent_id) {
                children.retain(|child| child != sibling);
            }
        }
        None => {
            state.root_child_order.retain(|child| child != sibling);
        }
    }
}

fn attach_linked_sibling(
    state: &mut LinkedCollectionState,
    parent_folder_id: Option<FolderId>,
    sibling: LinkedSiblingId,
) {
    match parent_folder_id {
        Some(parent_id) => state
            .folder_child_orders
            .entry(parent_id)
            .or_default()
            .push(sibling),
        None => state.root_child_order.push(sibling),
    }
}

fn next_linked_sibling_sort(
    state: &LinkedCollectionState,
    parent_folder_id: Option<FolderId>,
) -> i64 {
    state
        .folders
        .iter()
        .filter(|folder| folder.parent_folder_id == parent_folder_id)
        .map(|folder| folder.sort_order)
        .chain(
            state
                .requests
                .iter()
                .filter(|request| request.parent_folder_id == parent_folder_id)
                .map(|request| request.sort_order),
        )
        .max()
        .unwrap_or(-1)
        + 1
}
