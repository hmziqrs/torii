mod actions;
mod request_pages;
mod sidebar;
mod tab_ops;

use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Root, Sizable as _, WindowExt as _,
    button::{Button, ButtonRounded, ButtonVariants as _},
    h_flex,
    resizable::{h_resizable, resizable_panel},
    scroll::ScrollableElement as _,
    v_flex,
};

use crate::{
    domain::{
        collection::CollectionStorageKind,
        history::{HistoryCursor, HistoryEntry, HistoryQuery, HistoryState},
        ids::{RequestDraftId, RequestId, WorkspaceId},
        item_id::ItemId,
    },
    repos::tab_session_repo::{TabSessionMetadata, TabSessionWorkspaceState},
    services::{
        app_services::{AppServices, AppServicesGlobal},
        linked_collection_reconcile::{LinkedCollectionEvent, LinkedCollectionMonitor},
        telemetry,
        workspace_tree::{WorkspaceCatalog, load_workspace_catalog},
    },
    session::{
        item_key::{ItemKind, TabKey},
        workspace_session::WorkspaceSession,
    },
    title_bar::AppTitleBar,
    views::{
        AboutPage, LayoutDebugPage, SettingsPage,
        http_method::{RequestProtocol, protocol_badge},
        item_tabs::{
            collection_tab, environment_tab, folder_tab, history_tab, request_tab, workspace_tab,
        },
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
    layout_debug_page: Entity<LayoutDebugPage>,
    request_pages: std::collections::HashMap<RequestId, Entity<request_tab::RequestTabView>>,
    request_draft_pages:
        std::collections::HashMap<RequestDraftId, Entity<request_tab::RequestTabView>>,
    history_views_by_workspace: std::collections::HashMap<WorkspaceId, HistoryWorkspaceView>,
    _subscriptions: Vec<Subscription>,
    /// Tracks the previously active tab so we can release webviews on tab switch.
    previous_active_tab: Option<TabKey>,
    linked_collection_monitor: Option<LinkedCollectionMonitor>,
    linked_monitor_workspace_id: Option<WorkspaceId>,
    drag_auto_expand_target: Option<crate::session::workspace_session::ExpandableItem>,
    drag_auto_expand_epoch: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryProtocolFilter {
    All,
    Http,
    Graphql,
    WebSocket,
    Grpc,
}

impl HistoryProtocolFilter {
    fn as_query_value(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Http => Some("http"),
            Self::Graphql => Some("graphql"),
            Self::WebSocket => Some("websocket"),
            Self::Grpc => Some("grpc"),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HistoryWorkspaceView {
    pub entries: Vec<HistoryEntry>,
    pub state_filter: Option<HistoryState>,
    pub protocol_filter: HistoryProtocolFilter,
    pub next_cursor: Option<HistoryCursor>,
    pub has_loaded_once: bool,
}

impl Default for HistoryWorkspaceView {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            state_filter: None,
            protocol_filter: HistoryProtocolFilter::All,
            next_cursor: None,
            has_loaded_once: false,
        }
    }
}

impl AppRoot {
    pub(crate) fn can_open_item(&self, item_key: crate::session::item_key::ItemKey) -> bool {
        self.catalog.contains(item_key)
    }

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
        let layout_debug_page = cx.new(|cx| LayoutDebugPage::new(window, cx));

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
                    restored.active_environments_by_workspace,
                    restored.expanded_items_by_workspace,
                    restored.sidebar_selection,
                    restored.window_layout,
                    cx,
                );
            } else {
                session.set_selected_workspace(selected_workspace_id, cx);
            }
        });
        {
            let active_map = session.read(cx).active_environments_by_workspace.clone();
            if let Ok(mut shared) = services.active_environments_by_workspace.write() {
                *shared = active_map;
            }
        }

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
                // queries per interaction unnecessarily.
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
                            telemetry::inc_tree_catalog_reload();
                            this.catalog = catalog
                        }
                        Err(err) => {
                            tracing::error!("failed to refresh workspace catalog: {err}")
                        }
                    }
                    this.sync_expansion_state_with_catalog(cx);
                    this.sync_linked_collection_monitor(selected_workspace_id, cx);
                }

                // Release the HTML preview webview when switching away from a request tab.
                // Moved here from render() to avoid entity.update() inside render —
                // see idle-cpu-audit-claude.md Bug 5.
                let active_tab = session.read(cx).tab_manager.active();
                if this.previous_active_tab != active_tab {
                    this.release_html_webview_for_tab(this.previous_active_tab, cx);
                    this.previous_active_tab = active_tab;
                }

                // cx.notify() must fire unconditionally — all session mutations (tab switch,
                // sidebar toggle, reorder, etc.) need AppRoot to re-render.
                if let Ok(mut shared) = services.active_environments_by_workspace.write() {
                    *shared = session.read(cx).active_environments_by_workspace.clone();
                }
                cx.notify();
            }
        })];

        let io_runtime = services.io_runtime.clone();
        cx.spawn(async move |this, cx| {
            loop {
                // Run timer on Tokio runtime; GPUI async context does not provide a Tokio reactor.
                let sleep_join = io_runtime.spawn(async {
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                });
                let _ = sleep_join.await;

                let update = this.update(cx, |this, cx| {
                    let events = this
                        .linked_collection_monitor
                        .as_ref()
                        .map(|monitor| monitor.drain_events())
                        .unwrap_or_default();
                    if !events.is_empty() {
                        this.apply_linked_reconcile_events(events, cx);
                    }
                });
                if update.is_err() {
                    break;
                }
            }
        })
        .detach();

        let mut root = Self {
            focus_handle: cx.focus_handle(),
            title_bar,
            session,
            catalog,
            settings_page,
            about_page,
            layout_debug_page,
            request_pages: std::collections::HashMap::new(),
            request_draft_pages: std::collections::HashMap::new(),
            history_views_by_workspace: std::collections::HashMap::new(),
            _subscriptions: subscriptions,
            previous_active_tab: None,
            linked_collection_monitor: None,
            linked_monitor_workspace_id: None,
            drag_auto_expand_target: None,
            drag_auto_expand_epoch: 0,
        };
        root.sync_expansion_state_with_catalog(cx);
        root.sync_linked_collection_monitor(selected_workspace_id, cx);
        root
    }

    fn apply_linked_reconcile_events(
        &mut self,
        events: Vec<LinkedCollectionEvent>,
        cx: &mut Context<Self>,
    ) {
        if events.is_empty() {
            return;
        }
        let _span = tracing::info_span!("linked_collection.reconcile", event_count = events.len())
            .entered();

        self.refresh_catalog(cx);

        let stale_selection = {
            let session = self.session.read(cx);
            session
                .sidebar_selection
                .is_some_and(|selection| !self.catalog.contains(selection))
        };
        if stale_selection {
            self.session.update(cx, |session, cx| {
                let fallback = session.tab_manager.active().map(|active| active.item());
                session.set_sidebar_selection(fallback, cx);
            });
            self.persist_session_state(cx);
        }
    }

    fn sync_linked_collection_monitor(
        &mut self,
        selected_workspace_id: Option<WorkspaceId>,
        cx: &mut Context<Self>,
    ) {
        if self.linked_monitor_workspace_id == selected_workspace_id {
            return;
        }

        self.linked_collection_monitor = None;
        self.linked_monitor_workspace_id = selected_workspace_id;

        let Some(workspace_id) = selected_workspace_id else {
            return;
        };
        let services = services(cx);
        let linked_roots = services
            .repos
            .collection
            .list_by_workspace(workspace_id)
            .ok()
            .into_iter()
            .flatten()
            .filter(|collection| collection.storage_kind == CollectionStorageKind::Linked)
            .filter_map(|collection| {
                collection
                    .storage_config
                    .linked_root_path
                    .map(|root| (collection.id, root))
            })
            .collect::<Vec<_>>();

        if linked_roots.is_empty() {
            return;
        }

        match LinkedCollectionMonitor::start_for_roots(linked_roots) {
            Ok(monitor) => {
                self.linked_collection_monitor = Some(monitor);
            }
            Err(err) => {
                tracing::warn!("failed to start linked collection monitor: {err}");
            }
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

        let workspace_ids = snapshot
            .active_environments_by_workspace
            .keys()
            .chain(snapshot.expanded_items_by_workspace.keys())
            .copied()
            .collect::<std::collections::HashSet<_>>();
        let workspace_states = workspace_ids
            .into_iter()
            .map(|workspace_id| TabSessionWorkspaceState {
                workspace_id,
                active_environment_id: snapshot
                    .active_environments_by_workspace
                    .get(&workspace_id)
                    .copied(),
                expanded_items_json: serde_json::to_string(
                    snapshot
                        .expanded_items_by_workspace
                        .get(&workspace_id)
                        .cloned()
                        .unwrap_or_default()
                        .iter()
                        .copied()
                        .collect::<Vec<_>>()
                        .as_slice(),
                )
                .unwrap_or_else(|_| "[]".to_string()),
                created_at: now,
                updated_at: now,
            })
            .collect::<Vec<_>>();
        if let Err(err) = services
            .repos
            .tab_session
            .save_workspace_states(snapshot.session_id, &workspace_states)
        {
            tracing::error!("failed to persist workspace session state: {err}");
        }
    }

    fn render_active_tab_header_actions(
        &self,
        active: TabKey,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let is_dirty = match (active.item().kind, active.item().id) {
            (ItemKind::Request, Some(ItemId::Request(id))) => {
                self.request_pages.get(&id)?.read(cx).has_unsaved_changes()
            }
            (ItemKind::Request, Some(ItemId::RequestDraft(did))) => self
                .request_draft_pages
                .get(&did)?
                .read(cx)
                .has_unsaved_changes(),
            _ => return None,
        };

        if !is_dirty {
            return None;
        }

        let weak_root = cx.entity().downgrade();

        Some(
            Button::new("tab-header-save")
                .primary()
                .xsmall()
                .label(es_fluent::localize("request_tab_action_save", None))
                .on_click(move |_, window, cx| {
                    let mut save_result: Option<Result<Option<TabKey>, String>> = None;
                    let _ = weak_root.update(cx, |this, cx| {
                        save_result = Some(this.save_request_tab_by_key(active, cx));
                    });
                    match save_result {
                        Some(Ok(_)) => {
                            window.push_notification(
                                es_fluent::localize("request_tab_save_ok", None),
                                cx,
                            );
                        }
                        Some(Err(e)) => {
                            window.push_notification(e, cx);
                        }
                        None => {}
                    }
                })
                .into_any_element(),
        )
    }

    pub(crate) fn ensure_history_loaded_for_workspace(
        &mut self,
        workspace_id: WorkspaceId,
        cx: &mut Context<Self>,
    ) {
        let view = self
            .history_views_by_workspace
            .entry(workspace_id)
            .or_default();
        if !view.has_loaded_once {
            self.refresh_history_for_workspace(workspace_id, cx);
        }
    }

    pub(crate) fn refresh_history_for_workspace(
        &mut self,
        workspace_id: WorkspaceId,
        cx: &mut Context<Self>,
    ) {
        let Some((entries, next_cursor)) =
            self.query_history_page_for_workspace(workspace_id, None, cx)
        else {
            return;
        };

        let view = self
            .history_views_by_workspace
            .entry(workspace_id)
            .or_default();
        view.entries = entries;
        view.next_cursor = next_cursor;
        view.has_loaded_once = true;
        cx.notify();
    }

    pub(crate) fn load_more_history_for_workspace(
        &mut self,
        workspace_id: WorkspaceId,
        cx: &mut Context<Self>,
    ) {
        let cursor = self
            .history_views_by_workspace
            .get(&workspace_id)
            .and_then(|view| view.next_cursor.clone());
        let Some(cursor) = cursor else {
            return;
        };
        let Some((new_entries, next_cursor)) =
            self.query_history_page_for_workspace(workspace_id, Some(cursor), cx)
        else {
            return;
        };

        let view = self
            .history_views_by_workspace
            .entry(workspace_id)
            .or_default();
        view.entries.extend(new_entries);
        view.next_cursor = next_cursor;
        view.has_loaded_once = true;
        cx.notify();
    }

    pub(crate) fn set_history_state_filter_for_workspace(
        &mut self,
        workspace_id: WorkspaceId,
        filter: Option<HistoryState>,
        cx: &mut Context<Self>,
    ) {
        let view = self
            .history_views_by_workspace
            .entry(workspace_id)
            .or_default();
        if view.state_filter == filter {
            return;
        }
        view.state_filter = filter;
        self.refresh_history_for_workspace(workspace_id, cx);
    }

    pub(crate) fn set_history_protocol_filter_for_workspace(
        &mut self,
        workspace_id: WorkspaceId,
        filter: HistoryProtocolFilter,
        cx: &mut Context<Self>,
    ) {
        let view = self
            .history_views_by_workspace
            .entry(workspace_id)
            .or_default();
        if view.protocol_filter == filter {
            return;
        }
        view.protocol_filter = filter;
        self.refresh_history_for_workspace(workspace_id, cx);
    }

    fn query_history_page_for_workspace(
        &self,
        workspace_id: WorkspaceId,
        cursor: Option<HistoryCursor>,
        cx: &Context<Self>,
    ) -> Option<(Vec<HistoryEntry>, Option<HistoryCursor>)> {
        let services = services(cx);
        let view = self.history_views_by_workspace.get(&workspace_id);
        let mut query = HistoryQuery::for_workspace(workspace_id);
        query.limit = 100;
        query.cursor = cursor;
        query.state = view.and_then(|state| state.state_filter);
        query.protocol = view
            .and_then(|state| state.protocol_filter.as_query_value())
            .map(ToOwned::to_owned);

        match services.repos.history.query(query) {
            Ok(page) => Some((page.rows, page.next_cursor)),
            Err(err) => {
                tracing::error!("failed to query history rows for workspace {workspace_id}: {err}");
                None
            }
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
                .map(|workspace| workspace_tab::render(workspace, cx.entity().downgrade()))
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
                .map(|environment| environment_tab::render(environment, cx.entity().downgrade()))
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
            (ItemKind::History, None) => self
                .catalog
                .selected_workspace_id()
                .map(|workspace_id| {
                    self.ensure_history_loaded_for_workspace(workspace_id, cx);
                    let view = self
                        .history_views_by_workspace
                        .entry(workspace_id)
                        .or_default();
                    history_tab::render(workspace_id, view, cx.entity().downgrade())
                })
                .unwrap_or_else(|| {
                    render_empty_state(
                        es_fluent::localize("tab_missing_title", None).into(),
                        es_fluent::localize("tab_missing_body", None).into(),
                    )
                }),
            (ItemKind::Settings, None) => self.settings_page.clone().into_any_element(),
            (ItemKind::About, None) => self.about_page.clone().into_any_element(),
            (ItemKind::LayoutDebug, None) => self.layout_debug_page.clone().into_any_element(),
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

        // HTML preview webview release now happens in the session observer —
        // no more entity.update() inside render().

        let weak_root = cx.entity().downgrade();
        let sidebar_rail_width = 72.0;
        let sidebar_content_min_width = 150.0;
        let sidebar_expanded_min_width = sidebar_rail_width + sidebar_content_min_width;
        let sidebar_expanded_max_width = 440.0;

        v_flex()
            .size_full()
            .on_action(cx.listener(Self::on_about_action))
            .on_action(cx.listener(Self::on_open_settings_action))
            .on_action(cx.listener(Self::on_open_layout_debug_action))
            .on_action(cx.listener(Self::on_close_tab_action))
            .on_action(cx.listener(Self::on_new_request_action))
            .on_action(cx.listener(Self::on_next_tab_action))
            .on_action(cx.listener(Self::on_prev_tab_action))
            .on_action(cx.listener(Self::on_toggle_sidebar_action))
            .on_action(cx.listener(Self::on_tree_open_selected_action))
            .on_action(cx.listener(Self::on_tree_delete_selected_action))
            .on_action(cx.listener(Self::on_tree_open_item_menu_action))
            .child(self.title_bar.clone())
            .child(
                div()
                    .track_focus(&self.focus_handle)
                    .key_context("AppRoot")
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
                                    .size(px(if sidebar_collapsed {
                                        sidebar_rail_width
                                    } else {
                                        sidebar_width_px.max(sidebar_expanded_min_width)
                                    }))
                                    .size_range(
                                        px(if sidebar_collapsed {
                                            sidebar_rail_width
                                        } else {
                                            sidebar_expanded_min_width
                                        })..px(if sidebar_collapsed {
                                            sidebar_rail_width
                                        } else {
                                            sidebar_expanded_max_width
                                        }),
                                    )
                                    .child(
                                        div()
                                            .size_full()
                                            .overflow_hidden()
                                            .child(self.render_sidebar(sidebar_selection, cx)),
                                    ),
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
                                                        }))
                                                        .child(div().flex_1())
                                                        .when_some(
                                                            active_tab.and_then(|key| self.render_active_tab_header_actions(key, cx)),
                                                            |el, actions| el.child(actions),
                                                        ),
                                                )
                                            }
                                        })
                                        .child({
                                            // Request-like tabs manage their own internal resizable split
                                            // and per-section scroll areas. Wrapping them in an outer
                                            // scroll container (overflow_y_scrollbar) makes the entire
                                            // request+response split appear as one scrollable region,
                                            // because the Scrollable wrapper renders the content with
                                            // height:auto which prevents the inner flex layout from
                                            // resolving correctly. Use a plain flex container instead.
                                            let uses_internal_scroll_layout = active_tab
                                                .map(|k| {
                                                    matches!(
                                                        k.item().kind,
                                                        ItemKind::Request | ItemKind::LayoutDebug
                                                    )
                                                })
                                                .unwrap_or(false);
                                            let content = active_tab
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
                                                });
                                            if uses_internal_scroll_layout {
                                                v_flex()
                                                    .flex_1()
                                                    .min_h_0()
                                                    .child(content)
                                                    .into_any_element()
                                            } else {
                                                div()
                                                    .flex_1()
                                                    .overflow_y_scrollbar()
                                                    .child(content)
                                                    .into_any_element()
                                            }
                                        }),
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
