mod tree_ops;
mod tree_view;

use super::AppRoot;
use crate::{
    services::workspace_tree::TreeRow,
    session::{item_key::ItemKey, window_layout::SidebarSection},
};
use gpui::{App, Window, div, prelude::*, px, relative};
use gpui_component::{
    ActiveTheme as _, Collapsible, Icon, IconName, Selectable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonRounded, ButtonVariants as _},
    h_flex,
    menu::PopupMenuItem,
    scroll::ScrollableElement as _,
    sidebar::{Sidebar, SidebarGroup, SidebarItem, SidebarMenu, SidebarMenuItem},
    v_flex,
};

use self::tree_view::render_flat_tree_row;

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
                                sidebar
                                    .when(is_collections, |sidebar| {
                                        let weak_root_tree = weak_root.clone();
                                        let expanded_items = self
                                            .session
                                            .read(cx)
                                            .selected_workspace_id
                                            .and_then(|workspace_id| {
                                                self.session
                                                    .read(cx)
                                                    .expanded_items_for_workspace(workspace_id)
                                                    .cloned()
                                            })
                                            .unwrap_or_default();
                                        sidebar.child(AppSidebarNode::GroupTree(
                                            SidebarGroup::new(es_fluent::localize(
                                                "sidebar_collections",
                                                None,
                                            ))
                                            .child(CollectionTreeMenu::new(
                                                workspace.flat_collection_rows(&expanded_items),
                                                active_key,
                                                weak_root_tree,
                                            )),
                                        ))
                                    })
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
    rows: Vec<TreeRow>,
    active_key: Option<ItemKey>,
    weak_root: gpui::WeakEntity<AppRoot>,
    collapsed: bool,
}

impl CollectionTreeMenu {
    fn new(
        rows: Vec<TreeRow>,
        active_key: Option<ItemKey>,
        weak_root: gpui::WeakEntity<AppRoot>,
    ) -> Self {
        Self {
            rows,
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
            self.rows
                .into_iter()
                .map(|row| render_flat_tree_row(&row, self.active_key, &self.weak_root, window, cx))
                .collect::<Vec<_>>(),
        )
    }
}
