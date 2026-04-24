use super::super::AppRoot;
use crate::{
    domain::{
        collection::CollectionStorageKind,
        ids::{CollectionId, FolderId, RequestId},
    },
    services::workspace_tree::{CollectionTree, FolderTree, LinkedCollectionHealth, TreeItem},
    session::item_key::ItemKey,
};
use gpui::{
    AnyElement, App, InteractiveElement as _, Pixels, Point, Render, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Window, div, prelude::*, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _, WindowExt as _,
    button::Button,
    h_flex,
    hover_card::HoverCard,
    menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem},
    v_flex,
};
use std::time::Duration;

#[derive(Clone)]
pub(super) enum TreeDragPayload {
    Collection(CollectionId),
    Folder(FolderId),
    Request(RequestId),
}

#[derive(Clone, Copy)]
pub(super) enum TreeDropTarget {
    Collection(CollectionId),
    Folder(FolderId),
    Request(RequestId),
}

pub(super) fn render_collection_tree_row(
    collection: &CollectionTree,
    active_key: Option<ItemKey>,
    weak_root: &gpui::WeakEntity<AppRoot>,
    collapsed_folder_ids: &std::collections::HashSet<FolderId>,
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
            move |_, position, _, cx: &mut App| {
                cx.new(|_| DragTreePreview::new(title.clone(), IconName::BookOpen, position))
            }
        })
        .drag_over::<TreeDragPayload>(move |style: StyleRefinement, dragged, _, _| {
            if matches!(dragged, TreeDragPayload::Request(_)) {
                style.border_1().border_color(gpui::rgb(0x2563EB))
            } else {
                style
            }
        })
        .on_drop(move |dragged: &TreeDragPayload, window, cx| {
            if !matches!(dragged, TreeDragPayload::Request(_)) {
                return;
            }
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
            .map(|item| {
                render_tree_item_row(
                    item,
                    active_key,
                    weak_root,
                    collapsed_folder_ids,
                    depth + 1,
                    cx,
                )
            })
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
    collapsed_folder_ids: &std::collections::HashSet<FolderId>,
    depth: usize,
    cx: &mut App,
) -> AnyElement {
    match item {
        TreeItem::Folder(folder) => render_folder_tree_row(
            folder,
            active_key,
            weak_root,
            collapsed_folder_ids,
            depth,
            cx,
        ),
        TreeItem::Request(request) => {
            render_request_tree_row(request, active_key, weak_root, depth, cx)
        }
    }
}

fn render_folder_tree_row(
    folder: &FolderTree,
    active_key: Option<ItemKey>,
    weak_root: &gpui::WeakEntity<AppRoot>,
    collapsed_folder_ids: &std::collections::HashSet<FolderId>,
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
    let is_collapsed = collapsed_folder_ids.contains(&folder_id);
    let has_children = !folder.children.is_empty();

    let row = tree_row_base(depth, active_key == Some(item_key), cx)
        .id(format!("tree-folder-row-{}", folder_id))
        .on_click(move |_, _, cx| {
            let _ = weak_root_click.update(cx, |this, cx| {
                this.toggle_folder_collapsed(folder_id, cx);
                this.open_item(item_key, cx);
            });
        })
        .on_drag(payload.clone(), {
            let title = folder.folder.name.clone();
            move |_, position, _, cx: &mut App| {
                cx.new(|_| DragTreePreview::new(title.clone(), IconName::Folder, position))
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
                .w_full()
                .gap_2()
                .items_center()
                .child(
                    h_flex()
                        .w_4()
                        .justify_center()
                        .items_center()
                        .when(has_children, |this| {
                            this.child(
                                Icon::new(if is_collapsed {
                                    IconName::ChevronRight
                                } else {
                                    IconName::ChevronDown
                                })
                                .small()
                                .text_color(cx.theme().muted_foreground),
                            )
                        }),
                )
                .child(Icon::new(IconName::Folder).small())
                .child(div().text_sm().child(folder.folder.name.clone())),
        );

    let content = v_flex().gap_1().child(row);
    if is_collapsed {
        content.into_any_element()
    } else {
        let children = v_flex().gap_1().children(
            folder
                .children
                .iter()
                .map(|item| {
                    render_tree_item_row(
                        item,
                        active_key,
                        weak_root,
                        collapsed_folder_ids,
                        depth + 1,
                        cx,
                    )
                })
                .collect::<Vec<_>>(),
        );
        content.child(children).into_any_element()
    }
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
    let weak_root_drop = weak_root.clone();
    let payload = TreeDragPayload::Request(request_id);
    let drop_target = TreeDropTarget::Request(request_id);
    let request_name = request.name.clone();

    tree_row_base(depth, active_key == Some(item_key), cx)
        .id(format!("tree-request-row-{}", request_id))
        .on_click(move |_, _, cx| {
            let _ = weak_root_click.update(cx, |this, cx| this.open_item(item_key, cx));
        })
        .on_drag(payload, {
            let title = request.name.clone();
            move |_, position, _, cx: &mut App| {
                cx.new(|_| DragTreePreview::new(title.clone(), IconName::File, position))
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
        .flex()
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
    position: Point<Pixels>,
}

impl DragTreePreview {
    fn new(title: impl Into<SharedString>, icon: IconName, position: Point<Pixels>) -> Self {
        Self {
            title: title.into(),
            icon,
            position,
        }
    }
}

impl Render for DragTreePreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().pl(self.position.x).pt(self.position.y).child(
            h_flex()
                .px_2()
                .py_1()
                .gap_2()
                .rounded_sm()
                .bg(gpui::rgb(0x1F2937))
                .text_color(gpui::rgb(0xF9FAFB))
                .child(Icon::new(self.icon.clone()).small())
                .child(div().text_sm().child(self.title.clone())),
        )
    }
}
