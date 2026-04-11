use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Root, Sizable as _, WindowExt as _,
    menu::PopupMenuItem,
    resizable::{h_resizable, resizable_panel},
    scroll::ScrollableElement as _,
    sidebar::{Sidebar, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    v_flex,
};

use crate::{
    app::About,
    domain::item_id::ItemId,
    repos::tab_session_repo::TabSessionMetadata,
    services::{
        app_services::{AppServices, AppServicesGlobal},
        workspace_tree::{CollectionTree, FolderTree, TreeItem, WorkspaceCatalog, load_workspace_catalog},
    },
    session::{
        item_key::{ItemKey, ItemKind, TabKey},
        workspace_session::WorkspaceSession,
    },
    title_bar::AppTitleBar,
    views::{
        AboutPage, SettingsPage,
        item_tabs::{collection_tab, environment_tab, folder_tab, request_tab, workspace_tab},
        tab_host::{TabPresentation, render_empty_state, render_tab_bar},
    },
};

pub struct AppRoot {
    focus_handle: FocusHandle,
    title_bar: Entity<AppTitleBar>,
    session: Entity<WorkspaceSession>,
    catalog: WorkspaceCatalog,
    settings_page: Entity<SettingsPage>,
    about_page: Entity<AboutPage>,
    _subscriptions: Vec<Subscription>,
}

impl AppRoot {
    pub fn new(
        title: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let services = services(cx);
        let title_bar = cx.new(|cx| AppTitleBar::new(title, window, cx));
        let session = cx.new(|_| WorkspaceSession::new());
        let settings_page = cx.new(|cx| SettingsPage::new(window, cx));
        let about_page = cx.new(|cx| AboutPage::new(window, cx));

        let restored = services.session_restore.take_next_restore().ok().flatten();
        let selected_workspace_id = restored.as_ref().and_then(|restored| restored.selected_workspace_id);
        let mut catalog = load_workspace_catalog(
            &services.repos.workspace,
            &services.repos.collection,
            &services.repos.folder,
            &services.repos.request,
            &services.repos.environment,
            selected_workspace_id,
        )
        .unwrap_or_else(|err| {
            tracing::error!("failed to load workspace catalog: {err}");
            WorkspaceCatalog {
                workspaces: Vec::new(),
                selected_workspace: None,
            }
        });

        let selected_workspace_id = catalog
            .selected_workspace_id()
            .or_else(|| catalog.first_workspace_id());

        session.update(cx, |session, cx| {
            if let Some(restored) = restored {
                session.restore_tabs(
                    restored.tabs,
                    restored.active,
                    selected_workspace_id,
                    restored.sidebar_selection,
                    restored.window_layout,
                    cx,
                );
            } else {
                session.set_selected_workspace(selected_workspace_id, cx);
            }
        });

        catalog = load_workspace_catalog(
            &services.repos.workspace,
            &services.repos.collection,
            &services.repos.folder,
            &services.repos.request,
            &services.repos.environment,
            selected_workspace_id,
        )
        .unwrap_or(catalog);

        let subscriptions = vec![cx.observe(&session, {
            let services = services.clone();
            move |this, session, cx| {
                let selected_workspace_id = session.read(cx).selected_workspace_id;
                match load_workspace_catalog(
                    &services.repos.workspace,
                    &services.repos.collection,
                    &services.repos.folder,
                    &services.repos.request,
                    &services.repos.environment,
                    selected_workspace_id,
                ) {
                    Ok(catalog) => this.catalog = catalog,
                    Err(err) => tracing::error!("failed to refresh workspace catalog: {err}"),
                }
                cx.notify();
            }
        })];

        Self {
            focus_handle: cx.focus_handle(),
            title_bar,
            session,
            catalog,
            settings_page,
            about_page,
            _subscriptions: subscriptions,
        }
    }

    pub fn persist_session_state(&self, cx: &App) {
        let services = services(cx);
        let snapshot = self.session.read(cx);
        let now = crate::domain::revision::now_unix_ts();
        let metadata = TabSessionMetadata {
            selected_workspace_id: snapshot.selected_workspace_id.map(|id| id.to_string()),
            sidebar_selection: snapshot.sidebar_selection,
            window_layout: snapshot.window_layout.clone(),
            created_at: now,
            updated_at: now,
        };
        if let Err(err) = services.repos.tab_session.save_session(
            snapshot.session_id,
            snapshot.tab_manager.tabs(),
            snapshot.tab_manager.active(),
            &metadata,
        ) {
            tracing::error!("failed to persist tab session: {err}");
        }
    }

    fn on_about_action(&mut self, _: &About, _: &mut Window, cx: &mut Context<Self>) {
        self.open_item(ItemKey::about(), cx);
    }

    fn set_selected_workspace_for_item(&mut self, item_key: ItemKey, cx: &mut Context<Self>) {
        let services = services(cx);
        match services.session_restore.workspace_for_item(item_key) {
            Ok(Some(workspace_id)) => {
                self.session
                    .update(cx, |session, cx| session.set_selected_workspace(Some(workspace_id), cx));
            }
            Ok(None) => {}
            Err(err) => tracing::error!("failed to resolve item workspace: {err}"),
        }
    }

    fn open_item(&mut self, item_key: ItemKey, cx: &mut Context<Self>) {
        if item_key.is_persisted() {
            self.set_selected_workspace_for_item(item_key, cx);
        }
        self.session.update(cx, |session, cx| {
            session.open_or_focus(item_key, cx);
        });
        self.persist_session_state(cx);
    }

    fn focus_tab(&mut self, tab_key: TabKey, cx: &mut Context<Self>) {
        self.set_selected_workspace_for_item(tab_key.item(), cx);
        self.session.update(cx, |session, cx| {
            session.focus_tab(tab_key, cx);
        });
        self.persist_session_state(cx);
    }

    fn close_tab(&mut self, tab_key: TabKey, cx: &mut Context<Self>) {
        self.session.update(cx, |session, cx| {
            session.close_tab(tab_key, cx);
        });
        self.persist_session_state(cx);
    }

    fn reorder_tabs(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        self.session.update(cx, |session, cx| {
            session.reorder_tabs(from, to, cx);
        });
        self.persist_session_state(cx);
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.session.update(cx, |session, cx| {
            session.toggle_sidebar(cx);
        });
        self.persist_session_state(cx);
    }

    fn set_sidebar_width(&mut self, width_px: f32, cx: &mut Context<Self>) {
        self.session.update(cx, |session, cx| {
            session.set_sidebar_width(width_px, cx);
        });
        self.persist_session_state(cx);
    }

    fn delete_item(&mut self, item_key: ItemKey, window: &mut Window, cx: &mut Context<Self>) {
        let services = services(cx);
        let close_keys = self.catalog.delete_closure(item_key);
        let selected_workspace = services
            .session_restore
            .workspace_for_item(item_key)
            .ok()
            .flatten();

        let result = match (item_key.kind, item_key.id) {
            (ItemKind::Workspace, Some(ItemId::Workspace(id))) => services.repos.workspace.delete(id),
            (ItemKind::Collection, Some(ItemId::Collection(id))) => services.repos.collection.delete(id),
            (ItemKind::Folder, Some(ItemId::Folder(id))) => services.repos.folder.delete(id),
            (ItemKind::Environment, Some(ItemId::Environment(id))) => services.repos.environment.delete(id),
            (ItemKind::Request, Some(ItemId::Request(id))) => services.repos.request.delete(id),
            _ => Ok(()),
        };

        match result {
            Ok(()) => {
                let fallback_workspace = services
                    .repos
                    .workspace
                    .list()
                    .ok()
                    .and_then(|workspaces| workspaces.first().map(|workspace| workspace.id));

                self.session.update(cx, |session, cx| {
                    session.close_tabs(&close_keys, cx);
                    if session.selected_workspace_id == selected_workspace {
                        session.set_selected_workspace(fallback_workspace, cx);
                    }
                });
                self.persist_session_state(cx);
                window.push_notification(es_fluent::localize("delete_success", None), cx);
            }
            Err(err) => {
                tracing::error!("failed to delete item: {err}");
                window.push_notification(es_fluent::localize("delete_failed", None), cx);
            }
        }
    }

    fn render_sidebar(&self, active_key: Option<ItemKey>, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_workspace_id = self.session.read(cx).selected_workspace_id;
        let weak_root = cx.entity().downgrade();

        Sidebar::new("app-sidebar")
            .w(relative(1.))
            .border_0()
            .collapsed(self.session.read(cx).window_layout.sidebar_collapsed)
            .header(
                v_flex().w_full().gap_4().child(
                    SidebarHeader::new().w_full().child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(cx.theme().radius_lg)
                            .bg(cx.theme().primary)
                            .text_color(cx.theme().primary_foreground)
                            .size_8()
                            .flex_shrink_0()
                            .child(Icon::new(IconName::Star)),
                    ),
                ),
            )
            .child(
                SidebarGroup::new(es_fluent::localize("sidebar_workspaces", None)).child(
                    SidebarMenu::new().children(self.catalog.workspaces.iter().map(|workspace| {
                        let item_key = ItemKey::workspace(workspace.id);
                        let weak_root = weak_root.clone();
                        SidebarMenuItem::new(workspace.name.clone())
                            .icon(Icon::new(IconName::Inbox).small())
                            .active(active_key == Some(item_key) || selected_workspace_id == Some(workspace.id))
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
                sidebar
                    .child(
                        SidebarGroup::new(es_fluent::localize("sidebar_collections", None)).child(
                            SidebarMenu::new().children(workspace.collections.iter().map(|collection| {
                                render_collection_menu_item(collection, active_key, cx)
                            })),
                        ),
                    )
                    .child(
                        SidebarGroup::new(es_fluent::localize("sidebar_environments", None)).child(
                            SidebarMenu::new().children(workspace.environments.iter().map(|environment| {
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
                        ),
                ),
            )
    }

    fn render_active_tab_content(
        &self,
        active: TabKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match (active.item().kind, active.item().id) {
            (ItemKind::Workspace, Some(ItemId::Workspace(id))) => self
                .catalog
                .selected_workspace()
                .filter(|workspace| workspace.workspace.id == id)
                .map(workspace_tab::render)
                .unwrap_or_else(|| {
                    render_empty_state(
                        es_fluent::localize("tab_missing_title", None).into(),
                        es_fluent::localize("tab_missing_body", None).into(),
                    )
                }),
            (ItemKind::Collection, Some(ItemId::Collection(id))) => self
                .catalog
                .find_collection(id)
                .map(collection_tab::render)
                .unwrap_or_else(|| {
                    render_empty_state(
                        es_fluent::localize("tab_missing_title", None).into(),
                        es_fluent::localize("tab_missing_body", None).into(),
                    )
                }),
            (ItemKind::Folder, Some(ItemId::Folder(id))) => self
                .catalog
                .selected_workspace()
                .and_then(|workspace| {
                    workspace
                        .collections
                        .iter()
                        .find_map(|collection| collection.find_folder_tree(id))
                })
                .map(folder_tab::render)
                .unwrap_or_else(|| {
                    render_empty_state(
                        es_fluent::localize("tab_missing_title", None).into(),
                        es_fluent::localize("tab_missing_body", None).into(),
                    )
                }),
            (ItemKind::Environment, Some(ItemId::Environment(id))) => self
                .catalog
                .find_environment(id)
                .map(environment_tab::render)
                .unwrap_or_else(|| {
                    render_empty_state(
                        es_fluent::localize("tab_missing_title", None).into(),
                        es_fluent::localize("tab_missing_body", None).into(),
                    )
                }),
            (ItemKind::Request, Some(ItemId::Request(id))) => self
                .catalog
                .selected_workspace()
                .and_then(|workspace| {
                    workspace
                        .collections
                        .iter()
                        .find_map(|collection| collection.find_request(id))
                })
                .map(|request| request_tab::render(request, window, cx))
                .unwrap_or_else(|| {
                    render_empty_state(
                        es_fluent::localize("tab_missing_title", None).into(),
                        es_fluent::localize("tab_missing_body", None).into(),
                    )
                }),
            (ItemKind::Settings, None) => self.settings_page.clone().into_any_element(),
            (ItemKind::About, None) => self.about_page.clone().into_any_element(),
            _ => render_empty_state(
                es_fluent::localize("tab_missing_title", None).into(),
                es_fluent::localize("tab_missing_body", None).into(),
            ),
        }
    }
}

impl Focusable for AppRoot {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AppRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sheet_layer = Root::render_sheet_layer(window, cx);
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);
        let (active_tab, sidebar_selection, sidebar_collapsed, sidebar_width_px, tabs) = {
            let session = self.session.read(cx);
            let active_tab = session.tab_manager.active();
            let tabs = session
                .tab_manager
                .tabs()
                .iter()
                .enumerate()
                .map(|(index, tab)| TabPresentation {
                    index,
                    key: tab.key,
                    title: self
                        .catalog
                        .find_title(tab.key.item())
                        .unwrap_or_else(|| es_fluent::localize("tab_missing_short", None))
                        .into(),
                    icon: self.catalog.find_icon(tab.key.item()),
                    selected: active_tab == Some(tab.key),
                })
                .collect::<Vec<_>>();

            (
                active_tab,
                session.sidebar_selection,
                session.window_layout.sidebar_collapsed,
                session.window_layout.sidebar_width_px,
                tabs,
            )
        };
        let weak_root = cx.entity().downgrade();

        v_flex()
            .size_full()
            .on_action(cx.listener(Self::on_about_action))
            .child(self.title_bar.clone())
            .child(
                div()
                    .track_focus(&self.focus_handle)
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        h_resizable("app-layout")
                            .on_resize({
                                let weak_root = weak_root.clone();
                                move |state, _, cx| {
                                    let width = state
                                        .read(cx)
                                        .sizes()
                                        .first()
                                        .map(|size| size.as_f32())
                                        .unwrap_or(255.0);
                                    let _ = weak_root.update(cx, |this, cx| {
                                        this.set_sidebar_width(width, cx);
                                    });
                                }
                            })
                            .child(
                                resizable_panel()
                                    .size(px(if sidebar_collapsed { 48. } else { sidebar_width_px }))
                                    .size_range(
                                        px(if sidebar_collapsed { 48. } else { 180. })
                                            ..px(if sidebar_collapsed { 48. } else { 420. }),
                                    )
                                    .child(self.render_sidebar(sidebar_selection, cx)),
                            )
                            .child(
                                resizable_panel().child(
                                    v_flex()
                                        .flex_1()
                                        .h_full()
                                        .overflow_x_hidden()
                                        .child(render_tab_bar(
                                            &tabs,
                                            sidebar_collapsed,
                                            {
                                                let weak_root = weak_root.clone();
                                                move |key, _, cx| {
                                                    let _ = weak_root.update(cx, |this, cx| {
                                                        this.focus_tab(key, cx);
                                                    });
                                                }
                                            },
                                            {
                                                let weak_root = weak_root.clone();
                                                move |key, _, cx| {
                                                    let _ = weak_root.update(cx, |this, cx| {
                                                        this.close_tab(key, cx);
                                                    });
                                                }
                                            },
                                            {
                                                let weak_root = weak_root.clone();
                                                move |_, _, cx| {
                                                    let _ = weak_root.update(cx, |this, cx| {
                                                        this.toggle_sidebar(cx);
                                                    });
                                                }
                                            },
                                            move |from, to, _, cx| {
                                                let _ = weak_root.update(cx, |this, cx| {
                                                    this.reorder_tabs(from, to, cx);
                                                });
                                            },
                                        ))
                                        .child(
                                            div()
                                                .flex_1()
                                                .overflow_y_scrollbar()
                                                .child(
                                                    active_tab
                                                        .map(|active| self.render_active_tab_content(active, window, cx))
                                                        .unwrap_or_else(|| {
                                                            if self.catalog.workspaces.is_empty() {
                                                                render_empty_state(
                                                                    es_fluent::localize("empty_state_no_workspace_title", None).into(),
                                                                    es_fluent::localize("empty_state_no_workspace_body", None).into(),
                                                                )
                                                            } else {
                                                                render_empty_state(
                                                                    es_fluent::localize("empty_state_no_tab_title", None).into(),
                                                                    es_fluent::localize("empty_state_no_tab_body", None).into(),
                                                                )
                                                            }
                                                        }),
                                                ),
                                        ),
                                ),
                            ),
                    ),
            )
            .children(sheet_layer)
            .children(dialog_layer)
            .children(notification_layer)
    }
}

fn services(cx: &App) -> std::sync::Arc<AppServices> {
    cx.global::<AppServicesGlobal>().0.clone()
}

fn render_collection_menu_item(
    collection: &CollectionTree,
    active_key: Option<ItemKey>,
    cx: &mut Context<AppRoot>,
) -> SidebarMenuItem {
    let collection_key = ItemKey::collection(collection.collection.id);
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
        .children(collection.children.iter().map(|item| render_tree_item(item, active_key, cx)))
}

fn render_tree_item(
    item: &TreeItem,
    active_key: Option<ItemKey>,
    cx: &mut Context<AppRoot>,
) -> SidebarMenuItem {
    match item {
        TreeItem::Folder(folder) => render_folder_menu_item(folder, active_key, cx),
        TreeItem::Request(request) => {
            let request_key = ItemKey::request(request.id);
            let weak_root = cx.entity().downgrade();
            SidebarMenuItem::new(request.name.clone())
                .icon(Icon::new(IconName::File).small())
                .active(active_key == Some(request_key))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_item(request_key, cx);
                }))
                .context_menu(move |menu, _, _| {
                    let weak_root = weak_root.clone();
                    menu.item(
                        PopupMenuItem::new(es_fluent::localize("menu_delete", None))
                            .icon(Icon::new(IconName::Close))
                            .on_click(move |_, window, cx| {
                                let _ = weak_root.update(cx, |this, cx| {
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
    cx: &mut Context<AppRoot>,
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
        .children(folder.children.iter().map(|item| render_tree_item(item, active_key, cx)))
}
