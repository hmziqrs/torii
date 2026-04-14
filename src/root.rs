use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Root, Sizable as _, WindowExt as _,
    button::{Button, ButtonRounded, ButtonVariants as _},
    h_flex,
    menu::PopupMenuItem,
    resizable::{h_resizable, resizable_panel},
    scroll::ScrollableElement as _,
    sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem},
    v_flex,
};

use crate::{
    app::{About, CloseTab, NewRequest, NextTab, PrevTab, ToggleSidebar},
    domain::{
        history::HistoryState,
        ids::{RequestDraftId, RequestId},
        item_id::ItemId,
        response::{BodyRef, ResponseBudgets, ResponseSummary, normalize_unix_ms},
    },
    repos::tab_session_repo::TabSessionMetadata,
    services::{
        app_services::{AppServices, AppServicesGlobal},
        workspace_tree::{
            CollectionTree, FolderTree, TreeItem, WorkspaceCatalog, load_workspace_catalog,
        },
    },
    session::{
        item_key::{ItemKey, ItemKind, TabKey},
        request_editor_state::EditorIdentity,
        window_layout::SidebarSection,
        workspace_session::WorkspaceSession,
    },
    title_bar::AppTitleBar,
    views::{
        AboutPage, SettingsPage,
        http_method::method_badge,
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

    fn on_about_action(&mut self, _: &About, _: &mut Window, cx: &mut Context<Self>) {
        self.open_item(ItemKey::about(), cx);
    }

    fn on_close_tab_action(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        let active = self.session.read(cx).tab_manager.active();
        if let Some(tab_key) = active {
            self.close_tab(tab_key, window, cx);
        }
    }

    fn on_next_tab_action(&mut self, _: &NextTab, _: &mut Window, cx: &mut Context<Self>) {
        self.session.update(cx, |session, cx| {
            session.move_active_tab_by(1, cx);
        });
        self.persist_session_state(cx);
    }

    fn on_prev_tab_action(&mut self, _: &PrevTab, _: &mut Window, cx: &mut Context<Self>) {
        self.session.update(cx, |session, cx| {
            session.move_active_tab_by(-1, cx);
        });
        self.persist_session_state(cx);
    }

    fn on_toggle_sidebar_action(
        &mut self,
        _: &ToggleSidebar,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_sidebar(cx);
    }

    fn on_new_request_action(
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

    fn set_selected_workspace_for_item(&mut self, item_key: ItemKey, cx: &mut Context<Self>) {
        let services = services(cx);
        match services.session_restore.workspace_for_item(item_key) {
            Ok(Some(workspace_id)) => {
                self.session.update(cx, |session, cx| {
                    session.set_selected_workspace(Some(workspace_id), cx)
                });
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

    /// Release the HTML preview webview for a request tab, if applicable.
    /// Safe to call with `None` or a non-request tab key — it will be a no-op.
    fn release_html_webview_for_tab(
        &mut self,
        tab_key: Option<TabKey>,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_key) = tab_key else {
            return;
        };
        let page = match tab_key.item().id {
            Some(ItemId::Request(id)) => self.request_pages.get(&id).cloned(),
            Some(ItemId::RequestDraft(id)) => self.request_draft_pages.get(&id).cloned(),
            _ => None,
        };
        if let Some(page) = page {
            page.update(cx, |tab, cx| {
                tab.release_html_webview(cx);
            });
        }
    }

    fn perform_close_tab(&mut self, tab_key: TabKey, cx: &mut Context<Self>) {
        self.release_html_webview_for_tab(Some(tab_key), cx);
        self.session.update(cx, |session, cx| {
            session.close_tab(tab_key, cx);
        });
        self.persist_session_state(cx);
    }

    fn close_tab(&mut self, tab_key: TabKey, window: &mut Window, cx: &mut Context<Self>) {
        let request_id = match (tab_key.item().kind, tab_key.item().id) {
            (ItemKind::Request, Some(ItemId::Request(id))) => Some(id),
            _ => None,
        };

        let draft_id = match tab_key.item().id {
            Some(ItemId::RequestDraft(id)) => Some(id),
            _ => None,
        };

        let should_confirm_dirty = request_id
            .and_then(|id| self.request_pages.get(&id))
            .map(|page: &Entity<request_tab::RequestTabView>| page.read(cx).has_unsaved_changes())
            .unwrap_or(false)
            || draft_id
                .and_then(|id| self.request_draft_pages.get(&id))
                .map(|page: &Entity<request_tab::RequestTabView>| {
                    page.read(cx).has_unsaved_changes()
                })
                .unwrap_or(false);

        if !should_confirm_dirty {
            self.perform_close_tab(tab_key, cx);
            return;
        }

        let weak_root = cx.entity().downgrade();
        let weak_root_save = weak_root.clone();
        let weak_root_discard = weak_root.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title(es_fluent::localize("request_tab_dirty_close_title", None))
                .overlay_closable(false)
                .keyboard(false)
                .child(es_fluent::localize("request_tab_dirty_close_body", None))
                .footer(
                    h_flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("dirty-close-save")
                                .primary()
                                .label(es_fluent::localize("request_tab_dirty_close_save", None))
                                .on_click({
                                    let weak_root_save = weak_root_save.clone();
                                    move |_, window, cx| {
                                        let mut close_ok = false;
                                        let mut err_msg = None;
                                        let _ = weak_root_save.update(cx, |this, cx| {
                                            match this.save_request_tab_by_key(tab_key, cx) {
                                                Ok(Some(new_key)) => {
                                                    // Draft was promoted — close using new key
                                                    this.perform_close_tab(new_key, cx);
                                                    close_ok = true;
                                                }
                                                Ok(None) => {
                                                    this.perform_close_tab(tab_key, cx);
                                                    close_ok = true;
                                                }
                                                Err(err) => err_msg = Some(err),
                                            }
                                        });

                                        if let Some(err) = err_msg {
                                            window.push_notification(err, cx);
                                        }
                                        if close_ok {
                                            window.close_dialog(cx);
                                        }
                                    }
                                }),
                        )
                        .child(
                            Button::new("dirty-close-discard")
                                .outline()
                                .label(es_fluent::localize("request_tab_dirty_close_discard", None))
                                .on_click({
                                    let weak_root_discard = weak_root_discard.clone();
                                    move |_, window, cx| {
                                        let _ = weak_root_discard.update(cx, |this, cx| {
                                            // Clean up draft entity if discarding a draft tab
                                            if let Some(ItemId::RequestDraft(draft_id)) =
                                                tab_key.item().id
                                            {
                                                this.request_draft_pages.remove(&draft_id);
                                            }
                                            this.perform_close_tab(tab_key, cx);
                                        });
                                        window.close_dialog(cx);
                                    }
                                }),
                        )
                        .child(
                            Button::new("dirty-close-cancel")
                                .ghost()
                                .label(es_fluent::localize("request_tab_dirty_close_cancel", None))
                                .on_click(move |_, window, cx| {
                                    window.close_dialog(cx);
                                }),
                        ),
                )
        });
    }

    fn save_request_tab_by_key(
        &mut self,
        tab_key: TabKey,
        cx: &mut Context<Self>,
    ) -> Result<Option<TabKey>, String> {
        let page = match tab_key.item().id {
            Some(ItemId::Request(id)) => self.request_pages.get(&id).cloned(),
            Some(ItemId::RequestDraft(draft_id)) => {
                self.request_draft_pages.get(&draft_id).cloned()
            }
            _ => None,
        };
        let Some(page) = page else {
            return Ok(None);
        };

        page.update(cx, |tab, cx| tab.save(cx))
            .map_err(|err| format!("failed to update request tab while saving: {err}"))?;

        // After save, the observer may have promoted a draft to persisted.
        // Detect the current tab key from the editor identity.
        let current_key = {
            let identity = page.read(cx).editor().identity().clone();
            match identity {
                EditorIdentity::Persisted(request_id) => {
                    let new_key = TabKey::from(ItemKey::request(request_id));
                    if new_key != tab_key {
                        Some(new_key)
                    } else {
                        None
                    }
                }
                EditorIdentity::Draft(_) => None,
            }
        };

        self.refresh_catalog(cx);
        Ok(current_key)
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

    fn refresh_catalog(&mut self, cx: &mut Context<Self>) {
        let services = services(cx);
        let selected_workspace_id = self.session.read(cx).selected_workspace_id;
        match load_workspace_catalog(
            &services.repos.workspace,
            &services.repos.collection,
            &services.repos.folder,
            &services.repos.request,
            &services.repos.environment,
            selected_workspace_id,
        ) {
            Ok(catalog) => self.catalog = catalog,
            Err(err) => tracing::error!("failed to refresh workspace catalog: {err}"),
        }
        cx.notify();
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
            (ItemKind::Workspace, Some(ItemId::Workspace(id))) => {
                services.repos.workspace.delete(id)
            }
            (ItemKind::Collection, Some(ItemId::Collection(id))) => {
                services.repos.collection.delete(id)
            }
            (ItemKind::Folder, Some(ItemId::Folder(id))) => services.repos.folder.delete(id),
            (ItemKind::Environment, Some(ItemId::Environment(id))) => {
                services.repos.environment.delete(id)
            }
            (ItemKind::Request, Some(ItemId::Request(id))) => {
                if let Some(page) = self.request_pages.get(&id).cloned() {
                    let _ = page.update(cx, |tab, cx| {
                        tab.cancel_send(cx);
                        tab.release_html_webview(cx);
                    });
                }
                self.request_pages.remove(&id);
                services.repos.request.delete(id)
            }
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

    fn render_sidebar(
        &self,
        active_key: Option<ItemKey>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected_workspace_id = self.session.read(cx).selected_workspace_id;
        let weak_root = cx.entity().downgrade();

        Sidebar::new("app-sidebar")
            .w(relative(1.))
            .border_0()
            // .header(
            //     v_flex().w_full().gap_4().child(
            //         SidebarHeader::new().w_full().child(
            //             div()
            //                 .flex()
            //                 .items_center()
            //                 .justify_center()
            //                 .rounded(cx.theme().radius_lg)
            //                 .bg(cx.theme().primary)
            //                 .text_color(cx.theme().primary_foreground)
            //                 .size_8()
            //                 .flex_shrink_0()
            //                 .child(Icon::new(IconName::Star)),
            //         ),
            //     ),
            // )
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
                                                session.window_layout.sidebar_section = SidebarSection::Collections;
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
                                                session.window_layout.sidebar_section = SidebarSection::Environments;
                                                cx.notify();
                                            });
                                        })),
                                ),
                        ),
                    )
                    // Collections section (gated)
                    .when(is_collections, |sidebar| {
                        sidebar.child(
                            SidebarGroup::new(es_fluent::localize("sidebar_collections", None)).child(
                                SidebarMenu::new().children(workspace.collections.iter().map(
                                    |collection| {
                                        render_collection_menu_item(collection, active_key, cx)
                                    },
                                )),
                            ),
                        )
                    })
                    // Environments section (gated)
                    .when(!is_collections, |sidebar| {
                        sidebar.child(
                            SidebarGroup::new(es_fluent::localize("sidebar_environments", None)).child(
                                SidebarMenu::new().children(workspace.environments.iter().map(
                                    |environment| {
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
                                    },
                                )),
                            ),
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
                        ),
                ),
            )
    }

    fn request_page(
        &mut self,
        request: &crate::domain::request::RequestItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<request_tab::RequestTabView> {
        if let Some(page) = self.request_pages.get(&request.id) {
            return page.clone();
        }

        let page = cx.new(|cx| request_tab::RequestTabView::new(request, window, cx));

        // Restore latest-run summary from history if available
        let services = services(cx);
        if let Ok(Some(history_entry)) = services.repos.history.get_latest_for_request(request.id) {
            page.update(cx, |tab, _cx| {
                tab.editor_mut()
                    .set_latest_history_id(Some(history_entry.id));

                match history_entry.state {
                    HistoryState::Completed => {
                        let body_ref = match (
                            history_entry.blob_hash.as_ref(),
                            history_entry.blob_size.map(|v| v as u64),
                        ) {
                            (Some(hash), Some(size_bytes)) => {
                                let preview = services
                                    .blob_store
                                    .read_preview(hash, ResponseBudgets::PREVIEW_CAP_BYTES)
                                    .ok()
                                    .map(bytes::Bytes::from);
                                BodyRef::DiskBlob {
                                    blob_id: hash.clone(),
                                    preview,
                                    size_bytes,
                                }
                            }
                            _ => BodyRef::Empty,
                        };
                        let status_code = history_entry
                            .status_code
                            .unwrap_or(0)
                            .clamp(0, u16::MAX as i64)
                            as u16;
                        let status_text = http::StatusCode::from_u16(status_code)
                            .ok()
                            .and_then(|status| status.canonical_reason().map(ToOwned::to_owned))
                            .unwrap_or_default();
                        let dispatched_at_unix_ms =
                            history_entry.dispatched_at.map(normalize_unix_ms);
                        let first_byte_at_unix_ms =
                            history_entry.first_byte_at.map(normalize_unix_ms);
                        let completed_at_unix_ms =
                            history_entry.completed_at.map(normalize_unix_ms);
                        let total_ms = match (dispatched_at_unix_ms, completed_at_unix_ms) {
                            (Some(dispatched), Some(completed)) if completed >= dispatched => {
                                Some((completed - dispatched) as u64)
                            }
                            _ => None,
                        };
                        let ttfb_ms = match (dispatched_at_unix_ms, first_byte_at_unix_ms) {
                            (Some(dispatched), Some(first_byte)) if first_byte >= dispatched => {
                                Some((first_byte - dispatched) as u64)
                            }
                            _ => None,
                        };

                        tab.editor_mut()
                            .restore_completed_response(ResponseSummary {
                                status_code,
                                status_text,
                                headers_json: history_entry.response_headers_json.clone(),
                                media_type: history_entry.response_media_type.clone(),
                                body_ref,
                                total_ms,
                                ttfb_ms,
                                dispatched_at_unix_ms,
                                first_byte_at_unix_ms,
                                completed_at_unix_ms,
                            });
                    }
                    HistoryState::Failed => {
                        tab.editor_mut().restore_failed_response(
                            history_entry
                                .error_message
                                .clone()
                                .unwrap_or_else(|| "request failed".to_string()),
                        );
                    }
                    HistoryState::Cancelled => {
                        tab.editor_mut().restore_cancelled_response(
                            history_entry.partial_size.map(|s| s as u64),
                        );
                    }
                    HistoryState::Pending => {}
                }
            });
        }

        let services_for_refresh = services.clone();
        let subscription = cx.observe(&page, move |this, _, cx| {
            let selected_workspace_id = this.session.read(cx).selected_workspace_id;
            if let Ok(catalog) = load_workspace_catalog(
                &services_for_refresh.repos.workspace,
                &services_for_refresh.repos.collection,
                &services_for_refresh.repos.folder,
                &services_for_refresh.repos.request,
                &services_for_refresh.repos.environment,
                selected_workspace_id,
            ) {
                this.catalog = catalog;
                cx.notify();
            }
        });
        self._subscriptions.push(subscription);

        self.request_pages.insert(request.id, page.clone());
        page
    }

    /// Open a new draft request tab under the given collection.
    pub fn open_draft_request(
        &mut self,
        collection_id: crate::domain::ids::CollectionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let draft_id = RequestDraftId::new();
        let page = cx.new(|cx| request_tab::RequestTabView::new_draft(collection_id, window, cx));

        // Observe entity for draft→persisted transition and catalog refresh
        let services = services(cx);
        let services_for_refresh = services.clone();
        let draft_id_for_observer = draft_id;
        let subscription = cx.observe(&page, move |this, observed_page, cx| {
            let identity = observed_page.read(cx).editor().identity().clone();

            if let EditorIdentity::Persisted(request_id) = identity {
                // Check if this draft hasn't been transitioned yet
                if this
                    .request_draft_pages
                    .contains_key(&draft_id_for_observer)
                {
                    let old_key = crate::session::item_key::TabKey::from(
                        crate::session::item_key::ItemKey::request_draft(draft_id_for_observer),
                    );
                    let new_key = crate::session::item_key::TabKey::from(
                        crate::session::item_key::ItemKey::request(request_id),
                    );

                    this.session.update(cx, |session, cx| {
                        session.tab_manager.replace_key(old_key, new_key);
                        cx.notify();
                    });

                    if let Some(page) = this.request_draft_pages.remove(&draft_id_for_observer) {
                        this.request_pages.insert(request_id, page);
                    }
                }
            }

            // Always refresh catalog
            let selected_workspace_id = this.session.read(cx).selected_workspace_id;
            if let Ok(catalog) = load_workspace_catalog(
                &services_for_refresh.repos.workspace,
                &services_for_refresh.repos.collection,
                &services_for_refresh.repos.folder,
                &services_for_refresh.repos.request,
                &services_for_refresh.repos.environment,
                selected_workspace_id,
            ) {
                this.catalog = catalog;
                cx.notify();
            }
        });
        self._subscriptions.push(subscription);

        self.request_draft_pages.insert(draft_id, page);

        // Register with TabManager so the tab appears in the tab bar
        let item_key = crate::session::item_key::ItemKey::request_draft(draft_id);
        self.session.update(cx, |session, cx| {
            session.open_or_focus(item_key, cx);
        });

        self.persist_session_state(cx);
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
                    let (title, dirty) = match tab.key.item().id {
                        Some(ItemId::RequestDraft(draft_id)) => self
                            .request_draft_pages
                            .get(&draft_id)
                            .map(|p| {
                                let page = p.read(cx);
                                (
                                    page.editor().draft().name.clone(),
                                    page.has_unsaved_changes(),
                                )
                            })
                            .unwrap_or_else(|| {
                                (
                                    es_fluent::localize("request_tab_draft_title", None)
                                        .to_string(),
                                    false,
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
                            (base, dirty)
                        }
                        _ => (
                            self.catalog
                                .find_title(tab.key.item())
                                .unwrap_or_else(|| es_fluent::localize("tab_missing_short", None)),
                            false,
                        ),
                    };
                    let title = if dirty { format!("* {title}") } else { title };
                    TabPresentation {
                        index,
                        key: tab.key,
                        title: title.into(),
                        icon: self.catalog.find_icon(tab.key.item()),
                        selected: active_tab == Some(tab.key),
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
                                        ))
                                        // Breadcrumbs — show path for active tab
                                        .children({
                                            let parts = if let Some(key) = active_tab {
                                                self.catalog.find_breadcrumb_path(key.item())
                                            } else {
                                                Vec::new()
                                            };
                                            // Look up HTTP method if the active tab is a Request.
                                            let request_method = active_tab.and_then(|key| {
                                                if let (ItemKind::Request, Some(ItemId::Request(rid))) =
                                                    (key.item().kind, key.item().id)
                                                {
                                                    self.catalog.find_request_method(rid)
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
                                                        .py_1()
                                                        .gap_0p5()
                                                        .items_center()
                                                        .text_xs()
                                                        .children(parts.iter().enumerate().map(|(i, part)| {
                                                            let is_last = i == last_idx;
                                                            let method = if is_last { request_method.clone() } else { None };
                                                            h_flex()
                                                                .gap_0p5()
                                                                .items_center()
                                                                .when(i > 0, |el| {
                                                                    el.child(
                                                                        div()
                                                                            .text_color(cx.theme().muted_foreground)
                                                                            .child(">"),
                                                                    )
                                                                })
                                                                .when_some(method, |el, m| {
                                                                    el.child(method_badge(&m))
                                                                })
                                                                .child(
                                                                    Button::new(SharedString::from(format!(
                                                                        "breadcrumb-{i}"
                                                                    )))
                                                                    .ghost()
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

fn render_collection_menu_item(
    collection: &CollectionTree,
    active_key: Option<ItemKey>,
    cx: &mut Context<AppRoot>,
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
        .children(
            folder
                .children
                .iter()
                .map(|item| render_tree_item(item, active_key, cx)),
        )
}
