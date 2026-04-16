mod actions;
mod request_pages;
mod sidebar;
mod tab_ops;

use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Root, Sizable as _,
    button::{Button, ButtonRounded, ButtonVariants as _},
    h_flex,
    resizable::{h_resizable, resizable_panel},
    scroll::ScrollableElement as _,
    v_flex,
};

use crate::{
    domain::{
        ids::{RequestDraftId, RequestId},
        item_id::ItemId,
    },
    repos::tab_session_repo::TabSessionMetadata,
    services::{
        app_services::{AppServices, AppServicesGlobal},
        workspace_tree::{WorkspaceCatalog, load_workspace_catalog},
    },
    session::{
        item_key::{ItemKind, TabKey},
        workspace_session::WorkspaceSession,
    },
    title_bar::AppTitleBar,
    views::{
        AboutPage, SettingsPage,
        http_method::{RequestProtocol, protocol_badge},
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
    request_pages: std::collections::HashMap<RequestId, Entity<request_tab::RequestTabView>>,
    request_draft_pages:
        std::collections::HashMap<RequestDraftId, Entity<request_tab::RequestTabView>>,
    _subscriptions: Vec<Subscription>,
    /// Tracks the previously active tab so we can release webviews on tab switch.
    previous_active_tab: Option<TabKey>,
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
        let selected_workspace_id = restored
            .as_ref()
            .and_then(|restored| restored.selected_workspace_id);
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
            let mut last_workspace_id = selected_workspace_id;
            move |this, session, cx| {
                let selected_workspace_id = session.read(cx).selected_workspace_id;
                // Reload the catalog only when the selected workspace actually changed.
                // The session observer fires for every session mutation (tab open/close,
                // sidebar selection, etc.) — reloading on all of those runs 5 SQLite
                // queries per interaction unnecessarily. See render-loop-audit.md RLA-2.
                if selected_workspace_id != last_workspace_id {
                    last_workspace_id = selected_workspace_id;
                    match load_workspace_catalog(
                        &services.repos.workspace,
                        &services.repos.collection,
                        &services.repos.folder,
                        &services.repos.request,
                        &services.repos.environment,
                        selected_workspace_id,
                    ) {
                        Ok(catalog) => {
                            this.catalog = catalog;
                            cx.notify();
                        }
                        Err(err) => tracing::error!("failed to refresh workspace catalog: {err}"),
                    }
                }
            }
        })];

        Self {
            focus_handle: cx.focus_handle(),
            title_bar,
            session,
            catalog,
            settings_page,
            about_page,
            request_pages: std::collections::HashMap::new(),
            request_draft_pages: std::collections::HashMap::new(),
            _subscriptions: subscriptions,
            previous_active_tab: None,
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
        // Filter out draft tabs — they're ephemeral and not restorable
        let tabs: Vec<_> = snapshot
            .tab_manager
            .tabs()
            .iter()
            .filter(|tab| !matches!(tab.key.item().id, Some(ItemId::RequestDraft(_))))
            .cloned()
            .collect();
        let active = snapshot
            .tab_manager
            .active()
            .filter(|key| !matches!(key.item().id, Some(ItemId::RequestDraft(_))));
        if let Err(err) =
            services
                .repos
                .tab_session
                .save_session(snapshot.session_id, &tabs, active, &metadata)
        {
            tracing::error!("failed to persist tab session: {err}");
        }
    }

    fn render_active_tab_content(
        &mut self,
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
                        .find_map(|collection| collection.find_request(id).cloned())
                })
                .map(|request| self.request_page(&request, window, cx).into_any_element())
                .unwrap_or_else(|| {
                    render_empty_state(
                        es_fluent::localize("tab_missing_title", None).into(),
                        es_fluent::localize("tab_missing_body", None).into(),
                    )
                }),
            (ItemKind::Request, Some(ItemId::RequestDraft(draft_id))) => self
                .request_draft_pages
                .get(&draft_id)
                .map(|page| page.clone().into_any_element())
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
                .map(|(index, tab)| {
                    let (title, dirty, hover_subtitle) = match tab.key.item().id {
                        Some(ItemId::RequestDraft(draft_id)) => self
                            .request_draft_pages
                            .get(&draft_id)
                            .map(|p| {
                                let page = p.read(cx);
                                let draft = page.editor().draft();
                                let subtitle = if draft.url.is_empty() {
                                    None
                                } else {
                                    Some(format!("{} {}", draft.method, draft.url))
                                };
                                (draft.name.clone(), page.has_unsaved_changes(), subtitle)
                            })
                            .unwrap_or_else(|| {
                                (
                                    es_fluent::localize("request_tab_draft_title", None)
                                        .to_string(),
                                    false,
                                    None,
                                )
                            }),
                        Some(ItemId::Request(request_id)) => {
                            let base = self
                                .catalog
                                .find_title(tab.key.item())
                                .unwrap_or_else(|| es_fluent::localize("tab_missing_short", None));
                            let dirty = self
                                .request_pages
                                .get(&request_id)
                                .map(|p| p.read(cx).has_unsaved_changes())
                                .unwrap_or(false);
                            let subtitle = self
                                .catalog
                                .selected_workspace()
                                .and_then(|ws| {
                                    ws.collections
                                        .iter()
                                        .find_map(|c| c.find_request(request_id))
                                })
                                .and_then(|r| {
                                    if r.url.is_empty() {
                                        None
                                    } else {
                                        Some(format!("{} {}", r.method, r.url))
                                    }
                                });
                            (base, dirty, subtitle)
                        }
                        _ => (
                            self.catalog
                                .find_title(tab.key.item())
                                .unwrap_or_else(|| es_fluent::localize("tab_missing_short", None)),
                            false,
                            None,
                        ),
                    };
                    let title = if dirty { format!("* {title}") } else { title };
                    TabPresentation {
                        index,
                        key: tab.key,
                        title: title.into(),
                        icon: self.catalog.find_icon(tab.key.item()),
                        selected: active_tab == Some(tab.key),
                        hover_subtitle: hover_subtitle.map(Into::into),
                    }
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

        // Release the HTML preview webview when switching away from a request tab.
        // The response panel only cleans up the webview during its own render, but an
        // inactive tab's render is never called — so we must do it here on tab switch.
        if self.previous_active_tab != active_tab {
            self.release_html_webview_for_tab(self.previous_active_tab, cx);
            self.previous_active_tab = active_tab;
        }

        let weak_root = cx.entity().downgrade();

        v_flex()
            .size_full()
            .on_action(cx.listener(Self::on_about_action))
            .on_action(cx.listener(Self::on_close_tab_action))
            .on_action(cx.listener(Self::on_new_request_action))
            .on_action(cx.listener(Self::on_next_tab_action))
            .on_action(cx.listener(Self::on_prev_tab_action))
            .on_action(cx.listener(Self::on_toggle_sidebar_action))
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
                                    .size(px(if sidebar_collapsed { 140. } else { sidebar_width_px }))
                                    .size_range(
                                        px(if sidebar_collapsed { 140. } else { 180. })
                                            ..px(if sidebar_collapsed { 140. } else { 420. }),
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
                                                move |key, window, cx| {
                                                    let _ = weak_root.update(cx, |this, cx| {
                                                        this.close_tab(key, window, cx);
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
                                            window,
                                            cx,
                                        ))
                                        // Breadcrumbs — show path for active tab
                                        .children({
                                            let parts = if let Some(key) = active_tab {
                                                self.catalog.find_breadcrumb_path(key.item())
                                            } else {
                                                Vec::new()
                                            };
                                            // Derive protocol from the request method when active tab is a Request.
                                            let protocol = active_tab.and_then(|key| {
                                                if let (ItemKind::Request, Some(ItemId::Request(rid))) =
                                                    (key.item().kind, key.item().id)
                                                {
                                                    self.catalog
                                                        .find_request_method(rid)
                                                        .map(|m| RequestProtocol::from_method(&m))
                                                } else {
                                                    None
                                                }
                                            });
                                            let last_idx = parts.len().saturating_sub(1);
                                            if parts.is_empty() {
                                                None
                                            } else {
                                                Some(
                                                    h_flex()
                                                        .px_4()
                                                        .py_px()
                                                        .gap_px()
                                                        .items_center()
                                                        .children(parts.iter().enumerate().map(|(i, part)| {
                                                            let is_last = i == last_idx;
                                                            let proto = if is_last { protocol } else { None };
                                                            h_flex()
                                                                .gap_px()
                                                                .items_center()
                                                                .when(i > 0, |el| {
                                                                    el.child(
                                                                        Icon::new(IconName::ChevronRight)
                                                                            .small()
                                                                            .text_color(cx.theme().muted_foreground),
                                                                    )
                                                                })
                                                                .when_some(proto, |el, p| {
                                                                    el.child(protocol_badge(p))
                                                                })
                                                                .child(
                                                                    Button::new(SharedString::from(format!(
                                                                        "breadcrumb-{i}"
                                                                    )))
                                                                    .ghost()
                                                                    .xsmall()
                                                                    .rounded(ButtonRounded::Small)
                                                                    .label(part.clone())
                                                                    .on_click(|_, _, _| {}),
                                                                )
                                                        })),
                                                )
                                            }
                                        })
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
