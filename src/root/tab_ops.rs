use super::{AppRoot, services};
use crate::{
    domain::revision::now_unix_ts,
    domain::{
        collection::{CollectionStorageConfig, CollectionStorageKind},
        folder::Folder,
        ids::{CollectionId, EnvironmentId, FolderId, RequestId, WorkspaceId},
        item_id::ItemId,
    },
    infra::linked_collection_format::{
        LinkedCollectionState, LinkedSiblingId, linked_folder_paths, move_linked_folder_directory,
        read_linked_collection, write_linked_collection,
    },
    session::{
        item_key::{ItemKey, ItemKind, TabKey},
        request_editor_state::EditorIdentity,
    },
    views::item_tabs::request_tab,
};
use std::collections::{HashMap, HashSet};

use gpui::prelude::*;
use gpui::{Context, Entity, Window, div};
use gpui_component::{
    WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    v_flex,
};

impl AppRoot {
    pub(crate) fn create_workspace(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let services = services(cx);
        let workspaces = services
            .repos
            .workspace
            .list()
            .map_err(|err| format!("failed to list workspaces: {err}"))?;
        let name = next_workspace_name(
            &workspaces
                .iter()
                .map(|workspace| workspace.name.clone())
                .collect::<Vec<_>>(),
        );
        let workspace = services
            .repos
            .workspace
            .create(&name)
            .map_err(|err| format!("failed to create workspace: {err}"))?;
        drop(services);

        self.refresh_catalog(cx);
        self.open_item(ItemKey::workspace(workspace.id), cx);
        Ok(())
    }

    pub(crate) fn create_collection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let workspace_id = self
            .session
            .read(cx)
            .selected_workspace_id
            .or_else(|| self.catalog.first_workspace_id())
            .ok_or_else(|| es_fluent::localize("create_collection_no_workspace", None))?;

        let services = services(cx);
        let collections = services
            .repos
            .collection
            .list_by_workspace(workspace_id)
            .map_err(|err| format!("failed to list collections: {err}"))?;
        let name = next_item_name(
            es_fluent::localize("collection_default_name", None),
            &collections
                .iter()
                .map(|collection| collection.name.clone())
                .collect::<Vec<_>>(),
        );

        let name_input = cx.new(|cx| InputState::new(window, cx));
        name_input.update(cx, |state, cx| {
            state.set_value(name, window, cx);
        });
        let linked_root_input = cx.new(|cx| InputState::new(window, cx));
        let weak_root = cx.entity().downgrade();

        window.open_dialog(cx, move |dialog, _, _cx| {
            let weak_root_managed = weak_root.clone();
            let weak_root_linked = weak_root.clone();
            let name_input_managed = name_input.clone();
            let name_input_linked = name_input.clone();
            let linked_root_input = linked_root_input.clone();
            dialog
                .title(es_fluent::localize("create_collection_dialog_title", None))
                .overlay_closable(true)
                .keyboard(true)
                .child(
                    v_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .child(es_fluent::localize("create_collection_dialog_name", None)),
                        )
                        .child(Input::new(&name_input).w_full())
                        .child(div().text_sm().child(es_fluent::localize(
                            "create_collection_dialog_linked_root",
                            None,
                        )))
                        .child(Input::new(&linked_root_input).w_full())
                        .child(
                            div()
                                .text_sm()
                                .text_color(gpui::hsla(0.0, 0.0, 0.5, 1.0))
                                .child(es_fluent::localize(
                                    "create_collection_dialog_linked_root_placeholder",
                                    None,
                                )),
                        ),
                )
                .footer(
                    h_flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("create-collection-cancel")
                                .label(es_fluent::localize("dialog_cancel", None))
                                .on_click(move |_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("create-collection-linked-browse")
                                .label(es_fluent::localize("create_collection_dialog_browse", None))
                                .on_click({
                                    let linked_root_input = linked_root_input.clone();
                                    move |_, window, cx| {
                                        let receiver =
                                            cx.prompt_for_paths(gpui::PathPromptOptions {
                                                files: false,
                                                directories: true,
                                                multiple: false,
                                                prompt: Some(
                                                    es_fluent::localize(
                                                        "create_collection_dialog_browse_prompt",
                                                        None,
                                                    )
                                                    .to_string()
                                                    .into(),
                                                ),
                                            });
                                        let window_handle = window.window_handle();
                                        let linked_root_input = linked_root_input.clone();
                                        cx.spawn(async move |cx| {
                                            let picked_path = match receiver.await {
                                                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                                                _ => None,
                                            };
                                            let Some(path) = picked_path else {
                                                return;
                                            };
                                            let value = path.display().to_string();
                                            let _ = window_handle.update(cx, |_, window, cx| {
                                                let _ =
                                                    linked_root_input.update(cx, |state, cx| {
                                                        state.set_value(value, window, cx);
                                                    });
                                            });
                                        })
                                        .detach();
                                    }
                                }),
                        )
                        .child(
                            Button::new("create-collection-managed")
                                .label(es_fluent::localize(
                                    "create_collection_dialog_create_managed",
                                    None,
                                ))
                                .on_click(move |_, window, cx| {
                                    let name =
                                        name_input_managed.read(cx).value().trim().to_string();
                                    if name.is_empty() {
                                        window.push_notification(
                                            es_fluent::localize(
                                                "create_collection_dialog_name_required",
                                                None,
                                            ),
                                            cx,
                                        );
                                        return;
                                    }
                                    let result = weak_root_managed.update(cx, |this, cx| {
                                        this.create_collection_with_storage(
                                            workspace_id,
                                            name.clone(),
                                            CollectionStorageKind::Managed,
                                            None,
                                            cx,
                                        )
                                    });
                                    match result {
                                        Ok(Ok(())) => window.close_dialog(cx),
                                        Ok(Err(err)) => window.push_notification(err, cx),
                                        Err(err) => window.push_notification(
                                            format!("failed to create collection: {err}"),
                                            cx,
                                        ),
                                    }
                                }),
                        )
                        .child(
                            Button::new("create-collection-linked")
                                .primary()
                                .label(es_fluent::localize(
                                    "create_collection_dialog_create_linked",
                                    None,
                                ))
                                .on_click(move |_, window, cx| {
                                    let name =
                                        name_input_linked.read(cx).value().trim().to_string();
                                    if name.is_empty() {
                                        window.push_notification(
                                            es_fluent::localize(
                                                "create_collection_dialog_name_required",
                                                None,
                                            ),
                                            cx,
                                        );
                                        return;
                                    }
                                    let raw_path =
                                        linked_root_input.read(cx).value().trim().to_string();
                                    if raw_path.is_empty() {
                                        window.push_notification(
                                            es_fluent::localize(
                                                "create_collection_linked_root_required",
                                                None,
                                            ),
                                            cx,
                                        );
                                        return;
                                    }

                                    let mut root_path = std::path::PathBuf::from(raw_path);
                                    if root_path.is_relative() {
                                        if let Ok(cwd) = std::env::current_dir() {
                                            root_path = cwd.join(root_path);
                                        }
                                    }
                                    if root_path.exists() && root_path.is_file() {
                                        window.push_notification(
                                            es_fluent::localize(
                                                "create_collection_linked_root_not_directory",
                                                None,
                                            ),
                                            cx,
                                        );
                                        return;
                                    }

                                    let result = weak_root_linked.update(cx, |this, cx| {
                                        this.create_collection_with_storage(
                                            workspace_id,
                                            name.clone(),
                                            CollectionStorageKind::Linked,
                                            Some(root_path.clone()),
                                            cx,
                                        )
                                    });
                                    match result {
                                        Ok(Ok(())) => window.close_dialog(cx),
                                        Ok(Err(err)) => window.push_notification(err, cx),
                                        Err(err) => window.push_notification(
                                            format!("failed to create collection: {err}"),
                                            cx,
                                        ),
                                    }
                                }),
                        ),
                )
        });
        Ok(())
    }

    fn create_collection_with_storage(
        &mut self,
        workspace_id: WorkspaceId,
        name: String,
        storage_kind: CollectionStorageKind,
        linked_root_path: Option<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let services = services(cx);
        let storage_config = CollectionStorageConfig {
            linked_root_path: linked_root_path.clone(),
        };
        let collection = services
            .repos
            .collection
            .create_with_storage(workspace_id, &name, storage_kind, storage_config)
            .map_err(|err| format!("failed to create collection: {err}"))?;

        if storage_kind == CollectionStorageKind::Linked {
            let Some(root_path) = linked_root_path else {
                return Err(es_fluent::localize(
                    "create_collection_linked_root_required",
                    None,
                ));
            };
            let state = LinkedCollectionState {
                collection: collection.clone(),
                folders: Vec::new(),
                requests: Vec::new(),
                environments: Vec::new(),
                root_child_order: Vec::new(),
                folder_child_orders: HashMap::new(),
            };
            if let Err(err) = write_linked_collection(&root_path, &state) {
                let _ = services.repos.collection.delete(collection.id);
                return Err(format!(
                    "{}: {err}",
                    es_fluent::localize("create_collection_linked_init_failed", None)
                ));
            }
        }
        drop(services);

        self.refresh_catalog(cx);
        self.open_item(ItemKey::collection(collection.id), cx);
        Ok(())
    }

    pub(crate) fn create_environment(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let workspace_id = self
            .session
            .read(cx)
            .selected_workspace_id
            .or_else(|| self.catalog.first_workspace_id())
            .ok_or_else(|| es_fluent::localize("create_environment_no_workspace", None))?;

        let services = services(cx);
        let environments = services
            .repos
            .environment
            .list_by_workspace(workspace_id)
            .map_err(|err| format!("failed to list environments: {err}"))?;
        let name = next_item_name(
            es_fluent::localize("environment_default_name", None),
            &environments
                .iter()
                .map(|environment| environment.name.clone())
                .collect::<Vec<_>>(),
        );
        let environment = services
            .repos
            .environment
            .create(workspace_id, &name)
            .map_err(|err| format!("failed to create environment: {err}"))?;
        drop(services);

        self.refresh_catalog(cx);
        self.open_item(ItemKey::environment(environment.id), cx);
        Ok(())
    }

    pub(crate) fn open_workspace_variables_dialog(
        &mut self,
        workspace_id: crate::domain::ids::WorkspaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let services_ref = services(cx);
        let Some(workspace) = services_ref
            .repos
            .workspace
            .get(workspace_id)
            .map_err(|err| format!("failed to load workspace: {err}"))
            .ok()
            .flatten()
        else {
            window.push_notification(es_fluent::localize("workspace_missing", None), cx);
            return;
        };

        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |state, cx| {
            state.set_value(workspace.variables_json.clone(), window, cx);
        });
        let weak_root = cx.entity().downgrade();

        window.open_dialog(cx, move |dialog, _, _cx| {
            let weak_root_save = weak_root.clone();
            let input_for_save = input.clone();
            dialog
                .title(es_fluent::localize(
                    "workspace_variables_dialog_title",
                    None,
                ))
                .overlay_closable(true)
                .keyboard(true)
                .child(
                    v_flex()
                        .gap_2()
                        .child(Input::new(&input).h(gpui::px(260.)).w_full()),
                )
                .footer(
                    h_flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("workspace-vars-cancel")
                                .label(es_fluent::localize("dialog_cancel", None))
                                .on_click(move |_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("workspace-vars-save")
                                .primary()
                                .label(es_fluent::localize("dialog_save", None))
                                .on_click(move |_, window, cx| {
                                    let payload = input_for_save.read(cx).value().to_string();
                                    let parsed =
                                        serde_json::from_str::<serde_json::Value>(&payload);
                                    let is_valid = matches!(
                                        parsed,
                                        Ok(serde_json::Value::Array(_))
                                            | Ok(serde_json::Value::Object(_))
                                    );
                                    if !is_valid {
                                        window.push_notification(
                                            es_fluent::localize("variables_json_invalid", None),
                                            cx,
                                        );
                                        return;
                                    }
                                    let _ = weak_root_save.update(cx, |this, cx| {
                                        let services = services(cx);
                                        match services
                                            .repos
                                            .workspace
                                            .update_variables(workspace_id, &payload)
                                        {
                                            Ok(()) => {
                                                this.refresh_catalog(cx);
                                                cx.notify();
                                            }
                                            Err(err) => {
                                                window.push_notification(
                                                    format!(
                                                        "{}: {err}",
                                                        es_fluent::localize(
                                                            "workspace_variables_save_failed",
                                                            None
                                                        )
                                                    ),
                                                    cx,
                                                );
                                            }
                                        }
                                    });
                                    window.close_dialog(cx);
                                }),
                        ),
                )
        });
    }

    pub(crate) fn open_environment_variables_dialog(
        &mut self,
        environment_id: crate::domain::ids::EnvironmentId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let services_ref = services(cx);
        let Some(environment) = services_ref
            .repos
            .environment
            .get(environment_id)
            .map_err(|err| format!("failed to load environment: {err}"))
            .ok()
            .flatten()
        else {
            window.push_notification(es_fluent::localize("environment_missing", None), cx);
            return;
        };

        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |state, cx| {
            state.set_value(environment.variables_json.clone(), window, cx);
        });
        let weak_root = cx.entity().downgrade();

        window.open_dialog(cx, move |dialog, _, _cx| {
            let weak_root_save = weak_root.clone();
            let input_for_save = input.clone();
            dialog
                .title(es_fluent::localize(
                    "environment_variables_dialog_title",
                    None,
                ))
                .overlay_closable(true)
                .keyboard(true)
                .child(
                    v_flex()
                        .gap_2()
                        .child(Input::new(&input).h(gpui::px(260.)).w_full()),
                )
                .footer(
                    h_flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("environment-vars-cancel")
                                .label(es_fluent::localize("dialog_cancel", None))
                                .on_click(move |_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("environment-vars-save")
                                .primary()
                                .label(es_fluent::localize("dialog_save", None))
                                .on_click(move |_, window, cx| {
                                    let payload = input_for_save.read(cx).value().to_string();
                                    let parsed =
                                        serde_json::from_str::<serde_json::Value>(&payload);
                                    let is_valid = matches!(
                                        parsed,
                                        Ok(serde_json::Value::Array(_))
                                            | Ok(serde_json::Value::Object(_))
                                    );
                                    if !is_valid {
                                        window.push_notification(
                                            es_fluent::localize("variables_json_invalid", None),
                                            cx,
                                        );
                                        return;
                                    }
                                    let _ = weak_root_save.update(cx, |this, cx| {
                                        let services = services(cx);
                                        match services
                                            .repos
                                            .environment
                                            .update_variables(environment_id, &payload)
                                        {
                                            Ok(()) => {
                                                this.refresh_catalog(cx);
                                                cx.notify();
                                            }
                                            Err(err) => {
                                                window.push_notification(
                                                    format!(
                                                        "{}: {err}",
                                                        es_fluent::localize(
                                                            "environment_variables_save_failed",
                                                            None
                                                        )
                                                    ),
                                                    cx,
                                                );
                                            }
                                        }
                                    });
                                    window.close_dialog(cx);
                                }),
                        ),
                )
        });
    }

    pub(crate) fn create_folder(
        &mut self,
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let services = services(cx);
        let collection = services
            .repos
            .collection
            .get(collection_id)
            .map_err(|err| format!("failed to load collection: {err}"))?
            .ok_or_else(|| "collection no longer exists".to_string())?;

        let folder = if collection.storage_kind == CollectionStorageKind::Linked {
            let root_path = collection
                .storage_config
                .linked_root_path
                .clone()
                .ok_or_else(|| "linked collection is missing root path".to_string())?;
            let mut state = read_linked_collection(&root_path, &collection)
                .map_err(|err| format!("failed to load linked collection: {err}"))?;

            if let Some(parent_id) = parent_folder_id {
                let parent_exists = state.folders.iter().any(|folder| folder.id == parent_id);
                if !parent_exists {
                    return Err("parent folder no longer exists".to_string());
                }
            }

            let sibling_names = state
                .folders
                .iter()
                .filter(|folder| folder.parent_folder_id == parent_folder_id)
                .map(|folder| folder.name.clone())
                .collect::<Vec<_>>();
            let name = next_item_name(
                es_fluent::localize("folder_default_name", None),
                &sibling_names,
            );
            let next_sort = state
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
                + 1;
            let folder = Folder::new(collection_id, parent_folder_id, name, next_sort);

            if let Some(parent_id) = parent_folder_id {
                state
                    .folder_child_orders
                    .entry(parent_id)
                    .or_default()
                    .push(LinkedSiblingId::Folder {
                        id: folder.id.to_string(),
                    });
            } else {
                state.root_child_order.push(LinkedSiblingId::Folder {
                    id: folder.id.to_string(),
                });
            }
            state.folder_child_orders.entry(folder.id).or_default();
            state.folders.push(folder.clone());
            write_linked_collection(&root_path, &state)
                .map_err(|err| format!("failed to write linked collection: {err}"))?;
            folder
        } else {
            let folders = services
                .repos
                .folder
                .list_by_collection(collection_id)
                .map_err(|err| format!("failed to list folders: {err}"))?;
            let sibling_names = folders
                .iter()
                .filter(|folder| folder.parent_folder_id == parent_folder_id)
                .map(|folder| folder.name.clone())
                .collect::<Vec<_>>();
            let name = next_item_name(
                es_fluent::localize("folder_default_name", None),
                &sibling_names,
            );
            services
                .repos
                .folder
                .create(collection_id, parent_folder_id, &name)
                .map_err(|err| format!("failed to create folder: {err}"))?
        };
        drop(services);

        self.refresh_catalog(cx);
        self.open_item(ItemKey::folder(folder.id), cx);
        Ok(())
    }

    pub(crate) fn open_rename_item_dialog(
        &mut self,
        item_key: ItemKey,
        current_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |state, cx| {
            state.set_value(current_name.clone(), window, cx);
        });
        let weak_root = cx.entity().downgrade();

        window.open_dialog(cx, move |dialog, _, _cx| {
            let weak_root_save = weak_root.clone();
            let input_for_save = input.clone();
            dialog
                .title(es_fluent::localize("rename_dialog_title", None))
                .overlay_closable(true)
                .keyboard(true)
                .child(
                    v_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .child(es_fluent::localize("rename_dialog_name_label", None)),
                        )
                        .child(Input::new(&input).w_full()),
                )
                .footer(
                    h_flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("rename-item-cancel")
                                .label(es_fluent::localize("dialog_cancel", None))
                                .on_click(move |_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("rename-item-save")
                                .primary()
                                .label(es_fluent::localize("dialog_save", None))
                                .on_click(move |_, window, cx| {
                                    let name = input_for_save.read(cx).value().trim().to_string();
                                    if name.is_empty() {
                                        window.push_notification(
                                            es_fluent::localize(
                                                "rename_dialog_name_required",
                                                None,
                                            ),
                                            cx,
                                        );
                                        return;
                                    }
                                    let result = weak_root_save.update(cx, |this, cx| {
                                        this.rename_item(item_key, &name, cx)
                                    });
                                    match result {
                                        Ok(Ok(())) => {
                                            window.push_notification(
                                                es_fluent::localize("rename_success", None),
                                                cx,
                                            );
                                            window.close_dialog(cx);
                                        }
                                        Ok(Err(err)) => window.push_notification(err, cx),
                                        Err(err) => window.push_notification(
                                            format!(
                                                "{}: {err}",
                                                es_fluent::localize("rename_failed", None)
                                            ),
                                            cx,
                                        ),
                                    }
                                }),
                        ),
                )
        });
    }

    fn rename_item(
        &mut self,
        item_key: ItemKey,
        new_name: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let services = services(cx);
        match (item_key.kind, item_key.id) {
            (ItemKind::Workspace, Some(ItemId::Workspace(id))) => services
                .repos
                .workspace
                .rename(id, new_name)
                .map_err(|err| format!("failed to rename workspace: {err}"))?,
            (ItemKind::Collection, Some(ItemId::Collection(id))) => services
                .repos
                .collection
                .rename(id, new_name)
                .map_err(|err| format!("failed to rename collection: {err}"))?,
            (ItemKind::Environment, Some(ItemId::Environment(id))) => services
                .repos
                .environment
                .rename(id, new_name)
                .map_err(|err| format!("failed to rename environment: {err}"))?,
            (ItemKind::Request, Some(ItemId::Request(id))) => services
                .repos
                .request
                .rename(id, new_name)
                .map_err(|err| format!("failed to rename request: {err}"))?,
            (ItemKind::Folder, Some(ItemId::Folder(id))) => {
                let maybe_collection = self.catalog.selected_workspace().and_then(|workspace| {
                    workspace.collections.iter().find_map(|collection| {
                        collection
                            .find_folder_tree(id)
                            .map(|_| collection.collection.clone())
                    })
                });
                if let Some(collection) = maybe_collection {
                    if collection.storage_kind == CollectionStorageKind::Linked {
                        let root_path = collection
                            .storage_config
                            .linked_root_path
                            .clone()
                            .ok_or_else(|| "linked collection is missing root path".to_string())?;
                        let mut state = read_linked_collection(&root_path, &collection)
                            .map_err(|err| format!("failed to load linked collection: {err}"))?;
                        let previous_paths = linked_folder_paths(&root_path, &state.folders)
                            .map_err(|err| {
                                format!("failed to resolve linked folder paths: {err}")
                            })?;

                        {
                            let folder = state
                                .folders
                                .iter_mut()
                                .find(|folder| folder.id == id)
                                .ok_or_else(|| "folder no longer exists".to_string())?;
                            folder.name = new_name.to_string();
                            folder.meta.updated_at = now_unix_ts();
                            folder.meta.revision += 1;
                        }

                        let next_paths =
                            linked_folder_paths(&root_path, &state.folders).map_err(|err| {
                                format!("failed to resolve linked folder paths: {err}")
                            })?;
                        let old_path = previous_paths
                            .get(&id)
                            .cloned()
                            .ok_or_else(|| "folder path no longer exists".to_string())?;
                        let new_path = next_paths.get(&id).cloned().ok_or_else(|| {
                            "target folder path could not be resolved".to_string()
                        })?;

                        let mut moved_dir = false;
                        if old_path != new_path {
                            move_linked_folder_directory(&old_path, &new_path).map_err(|err| {
                                format!("failed to move linked folder directory: {err}")
                            })?;
                            moved_dir = true;
                        }

                        if let Err(err) = write_linked_collection(&root_path, &state) {
                            if moved_dir {
                                if let Err(rollback_err) =
                                    move_linked_folder_directory(&new_path, &old_path)
                                {
                                    tracing::error!(
                                        "failed to rollback linked folder rename after write failure: {rollback_err}"
                                    );
                                }
                            }
                            return Err(format!("failed to write linked collection: {err}"));
                        }
                    } else {
                        services
                            .repos
                            .folder
                            .rename(id, new_name)
                            .map_err(|err| format!("failed to rename folder: {err}"))?;
                    }
                } else {
                    services
                        .repos
                        .folder
                        .rename(id, new_name)
                        .map_err(|err| format!("failed to rename folder: {err}"))?;
                }
            }
            _ => return Err(es_fluent::localize("rename_unsupported", None).to_string()),
        }

        drop(services);
        self.refresh_catalog(cx);
        self.persist_session_state(cx);
        Ok(())
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

    pub(crate) fn open_item(&mut self, item_key: ItemKey, cx: &mut Context<Self>) {
        if item_key.is_persisted() {
            self.set_selected_workspace_for_item(item_key, cx);
        }
        self.session.update(cx, |session, cx| {
            session.open_or_focus(item_key, cx);
        });
        self.persist_session_state(cx);
    }

    pub(super) fn focus_tab(&mut self, tab_key: TabKey, cx: &mut Context<Self>) {
        self.set_selected_workspace_for_item(tab_key.item(), cx);
        self.session.update(cx, |session, cx| {
            session.focus_tab(tab_key, cx);
        });
        self.persist_session_state(cx);
    }

    /// Release the HTML preview webview for a request tab, if applicable.
    /// Safe to call with `None` or a non-request tab key — it will be a no-op.
    pub(super) fn release_html_webview_for_tab(
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

    pub(crate) fn close_tab(
        &mut self,
        tab_key: TabKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    pub(super) fn save_request_tab_by_key(
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

    pub(super) fn reorder_tabs(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        self.session.update(cx, |session, cx| {
            session.reorder_tabs(from, to, cx);
        });
        self.persist_session_state(cx);
    }

    pub(crate) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.session.update(cx, |session, cx| {
            session.toggle_sidebar(cx);
        });
        self.persist_session_state(cx);
    }

    pub(super) fn set_sidebar_width(&mut self, width_px: f32, cx: &mut Context<Self>) {
        self.session.update(cx, |session, cx| {
            session.set_sidebar_width(width_px, cx);
        });
        self.persist_session_state(cx);
    }

    pub(super) fn refresh_catalog(&mut self, cx: &mut Context<Self>) {
        let services = services(cx);
        let selected_workspace_id = self.session.read(cx).selected_workspace_id;
        match crate::services::workspace_tree::load_workspace_catalog(
            &services.repos.workspace,
            &services.repos.collection,
            &services.repos.folder,
            &services.repos.request,
            &services.repos.environment,
            selected_workspace_id,
        ) {
            Ok(catalog) => {
                if self.catalog != catalog {
                    self.catalog = catalog;
                    self.sync_expansion_state_with_catalog(cx);
                    cx.notify();
                }
            }
            Err(err) => tracing::error!("failed to refresh workspace catalog: {err}"),
        }
    }

    pub(crate) fn duplicate_request(
        &mut self,
        request_id: RequestId,
        _request_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let services = services(cx);
        let managed_source = match services.repos.request.get(request_id) {
            Ok(source) => source,
            Err(err) => {
                tracing::error!("failed to load source request: {err}");
                window.push_notification(
                    es_fluent::localize("request_tab_duplicate_failed", None),
                    cx,
                );
                return;
            }
        };

        if let Some(source) = managed_source {
            let siblings = match services
                .repos
                .request
                .list_by_collection(source.collection_id)
            {
                Ok(requests) => requests
                    .into_iter()
                    .filter(|request| request.parent_folder_id == source.parent_folder_id)
                    .map(|request| request.name)
                    .collect::<Vec<_>>(),
                Err(err) => {
                    tracing::error!("failed to list sibling requests: {err}");
                    window.push_notification(
                        es_fluent::localize("request_tab_duplicate_failed", None),
                        cx,
                    );
                    return;
                }
            };
            let new_name = next_duplicate_request_name(&source.name, &siblings);
            match services.repos.request.duplicate(request_id, &new_name) {
                Ok(new_request) => {
                    drop(services);
                    self.refresh_catalog(cx);
                    self.open_item(ItemKey::request(new_request.id), cx);
                    window.push_notification(
                        es_fluent::localize("request_tab_duplicate_ok", None),
                        cx,
                    );
                }
                Err(err) => {
                    tracing::error!("failed to duplicate request: {err}");
                    window.push_notification(
                        es_fluent::localize("request_tab_duplicate_failed", None),
                        cx,
                    );
                }
            }
            return;
        }

        let maybe_collection = self.catalog.selected_workspace().and_then(|workspace| {
            workspace.collections.iter().find_map(|collection| {
                collection
                    .find_request(request_id)
                    .map(|_| collection.collection.clone())
            })
        });

        let Some(collection) = maybe_collection else {
            window.push_notification(
                es_fluent::localize("request_tab_duplicate_failed", None),
                cx,
            );
            return;
        };

        if collection.storage_kind != CollectionStorageKind::Linked {
            window.push_notification(
                es_fluent::localize("request_tab_duplicate_failed", None),
                cx,
            );
            return;
        }

        let root_path = match collection.storage_config.linked_root_path.clone() {
            Some(path) => path,
            None => {
                window.push_notification(
                    es_fluent::localize("request_tab_duplicate_failed", None),
                    cx,
                );
                return;
            }
        };
        let mut state = match read_linked_collection(&root_path, &collection) {
            Ok(state) => state,
            Err(err) => {
                tracing::error!("failed to load linked collection for duplicate: {err}");
                window.push_notification(
                    es_fluent::localize("request_tab_duplicate_failed", None),
                    cx,
                );
                return;
            }
        };
        let Some(source) = state
            .requests
            .iter()
            .find(|request| request.id == request_id)
            .cloned()
        else {
            window.push_notification(
                es_fluent::localize("request_tab_duplicate_failed", None),
                cx,
            );
            return;
        };

        let sibling_names = state
            .requests
            .iter()
            .filter(|request| request.parent_folder_id == source.parent_folder_id)
            .map(|request| request.name.clone())
            .collect::<Vec<_>>();
        let mut duplicate = source.clone();
        duplicate.id = RequestId::new();
        duplicate.name = next_duplicate_request_name(&source.name, &sibling_names);
        duplicate.sort_order = state
            .folders
            .iter()
            .filter(|folder| folder.parent_folder_id == duplicate.parent_folder_id)
            .map(|folder| folder.sort_order)
            .chain(
                state
                    .requests
                    .iter()
                    .filter(|request| request.parent_folder_id == duplicate.parent_folder_id)
                    .map(|request| request.sort_order),
            )
            .max()
            .unwrap_or(-1)
            + 1;
        duplicate.meta = crate::domain::revision::RevisionMetadata::new_now();

        if let Some(parent_id) = duplicate.parent_folder_id {
            state
                .folder_child_orders
                .entry(parent_id)
                .or_default()
                .push(LinkedSiblingId::Request {
                    id: duplicate.id.to_string(),
                });
        } else {
            state.root_child_order.push(LinkedSiblingId::Request {
                id: duplicate.id.to_string(),
            });
        }
        state.requests.push(duplicate.clone());
        match write_linked_collection(&root_path, &state) {
            Ok(()) => {
                drop(services);
                self.refresh_catalog(cx);
                self.open_item(ItemKey::request(duplicate.id), cx);
                window.push_notification(es_fluent::localize("request_tab_duplicate_ok", None), cx);
            }
            Err(err) => {
                tracing::error!("failed to duplicate linked request: {err}");
                window.push_notification(
                    es_fluent::localize("request_tab_duplicate_failed", None),
                    cx,
                );
            }
        }
    }

    pub(crate) fn delete_item(
        &mut self,
        item_key: ItemKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let weak_root = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title(es_fluent::localize("delete_confirm_title", None))
                .overlay_closable(false)
                .keyboard(false)
                .child(es_fluent::localize("delete_confirm_body", None))
                .footer(
                    h_flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("delete-confirm-cancel")
                                .outline()
                                .label(es_fluent::localize("delete_confirm_cancel", None))
                                .on_click(move |_, window, cx| {
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new("delete-confirm-ok")
                                .primary()
                                .label(es_fluent::localize("delete_confirm_ok", None))
                                .on_click({
                                    let weak_root = weak_root.clone();
                                    move |_, window, cx| {
                                        let _ = weak_root.update(cx, |this, cx| {
                                            this.perform_delete_item(item_key, window, cx);
                                        });
                                        window.close_dialog(cx);
                                    }
                                }),
                        ),
                )
        });
    }

    fn perform_delete_item(
        &mut self,
        item_key: ItemKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let services = services(cx);
        let close_keys = self.catalog.delete_closure(item_key);
        let draft_close_keys = draft_descendant_close_keys(
            &close_keys,
            self.request_draft_pages
                .iter()
                .map(|(draft_id, page)| DraftLocation {
                    draft_id: *draft_id,
                    collection_id: page.read(cx).editor().draft().collection_id,
                    parent_folder_id: page.read(cx).editor().draft().parent_folder_id,
                }),
        );
        let mut all_close_keys = close_keys.clone();
        all_close_keys.extend(draft_close_keys.iter().copied());
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
                if let (ItemKind::Environment, Some(workspace_id)) =
                    (item_key.kind, selected_workspace)
                {
                    if let Ok(mut shared) = services.active_environments_by_workspace.write() {
                        shared.remove(&workspace_id);
                    }
                }
                let fallback_workspace = services
                    .repos
                    .workspace
                    .list()
                    .ok()
                    .and_then(|workspaces| workspaces.first().map(|workspace| workspace.id));

                self.session.update(cx, |session, cx| {
                    session.close_tabs(&all_close_keys, cx);
                    if let (ItemKind::Environment, Some(workspace_id)) =
                        (item_key.kind, selected_workspace)
                    {
                        session.set_active_environment_for_workspace(workspace_id, None, cx);
                    }
                    if should_reset_selected_workspace_on_delete(
                        item_key,
                        session.selected_workspace_id,
                        selected_workspace,
                    ) {
                        session.set_selected_workspace(fallback_workspace, cx);
                    }
                });
                for key in draft_close_keys {
                    if let Some(ItemId::RequestDraft(draft_id)) = key.id {
                        if let Some(page) = self.request_draft_pages.remove(&draft_id) {
                            let _ = page.update(cx, |tab, cx| {
                                tab.cancel_send(cx);
                                tab.release_html_webview(cx);
                            });
                        }
                    }
                }
                self.refresh_catalog(cx);
                self.persist_session_state(cx);
                window.push_notification(es_fluent::localize("delete_success", None), cx);
            }
            Err(err) => {
                tracing::error!("failed to delete item: {err}");
                window.push_notification(es_fluent::localize("delete_failed", None), cx);
            }
        }
    }

    pub(crate) fn set_active_environment(
        &mut self,
        environment_id: EnvironmentId,
        cx: &mut Context<Self>,
    ) {
        let services = services(cx);
        let workspace_id = match services
            .session_restore
            .workspace_for_item(ItemKey::environment(environment_id))
        {
            Ok(Some(workspace_id)) => workspace_id,
            _ => return,
        };

        self.session.update(cx, |session, cx| {
            session.set_selected_workspace(Some(workspace_id), cx);
            session.set_active_environment_for_workspace(workspace_id, Some(environment_id), cx);
        });
        if let Ok(mut shared) = services.active_environments_by_workspace.write() {
            shared.insert(workspace_id, environment_id);
        }
        self.persist_session_state(cx);
    }
}

#[derive(Clone, Copy)]
struct DraftLocation {
    draft_id: crate::domain::ids::RequestDraftId,
    collection_id: CollectionId,
    parent_folder_id: Option<FolderId>,
}

fn draft_descendant_close_keys(
    close_keys: &[ItemKey],
    draft_locations: impl IntoIterator<Item = DraftLocation>,
) -> Vec<ItemKey> {
    let deleted_collections: HashSet<CollectionId> = close_keys
        .iter()
        .filter_map(|key| match key.id {
            Some(ItemId::Collection(id)) => Some(id),
            _ => None,
        })
        .collect();
    let deleted_folders: HashSet<FolderId> = close_keys
        .iter()
        .filter_map(|key| match key.id {
            Some(ItemId::Folder(id)) => Some(id),
            _ => None,
        })
        .collect();

    draft_locations
        .into_iter()
        .filter(|draft| {
            deleted_collections.contains(&draft.collection_id)
                || draft
                    .parent_folder_id
                    .is_some_and(|folder_id| deleted_folders.contains(&folder_id))
        })
        .map(|draft| ItemKey::request_draft(draft.draft_id))
        .collect()
}

fn should_reset_selected_workspace_on_delete(
    item_key: ItemKey,
    selected_workspace_id: Option<WorkspaceId>,
    deleted_item_workspace: Option<WorkspaceId>,
) -> bool {
    matches!(item_key.kind, ItemKind::Workspace) && selected_workspace_id == deleted_item_workspace
}

fn next_workspace_name(existing_names: &[String]) -> String {
    next_item_name(
        es_fluent::localize("workspace_default_name", None),
        existing_names,
    )
}

fn next_duplicate_request_name(source_name: &str, existing_names: &[String]) -> String {
    let base = format!("{source_name} (Copy)");
    if !existing_names.iter().any(|name| name == &base) {
        return base;
    }

    let mut index = 2;
    loop {
        let candidate = format!("{source_name} (Copy {index})");
        if !existing_names.iter().any(|name| name == &candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn next_item_name(base: String, existing_names: &[String]) -> String {
    if !existing_names.iter().any(|name| name == &base) {
        return base;
    }

    let mut index = 2;
    loop {
        let candidate = format!("{base} {index}");
        if !existing_names.iter().any(|name| name == &candidate) {
            return candidate;
        }
        index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DraftLocation, draft_descendant_close_keys, next_duplicate_request_name,
        should_reset_selected_workspace_on_delete,
    };
    use crate::{
        domain::ids::{CollectionId, FolderId, RequestDraftId, WorkspaceId},
        session::item_key::ItemKey,
    };

    #[test]
    fn workspace_delete_resets_when_selected_workspace_is_deleted() {
        let workspace_id = WorkspaceId::new();
        assert!(should_reset_selected_workspace_on_delete(
            ItemKey::workspace(workspace_id),
            Some(workspace_id),
            Some(workspace_id),
        ));
    }

    #[test]
    fn non_workspace_delete_does_not_reset_selected_workspace() {
        let workspace_id = WorkspaceId::new();
        let collection_key = ItemKey::collection(CollectionId::new());
        assert!(!should_reset_selected_workspace_on_delete(
            collection_key,
            Some(workspace_id),
            Some(workspace_id),
        ));
    }

    #[test]
    fn deleting_other_workspace_does_not_reset_selection() {
        let selected_workspace_id = WorkspaceId::new();
        let deleted_workspace_id = WorkspaceId::new();
        assert!(!should_reset_selected_workspace_on_delete(
            ItemKey::workspace(deleted_workspace_id),
            Some(selected_workspace_id),
            Some(deleted_workspace_id),
        ));
    }

    #[test]
    fn draft_descendant_closure_matches_deleted_collection() {
        let deleted_collection = CollectionId::new();
        let keep_collection = CollectionId::new();
        let matching_draft = RequestDraftId::new();
        let other_draft = RequestDraftId::new();

        let close_keys = vec![ItemKey::collection(deleted_collection)];
        let draft_keys = draft_descendant_close_keys(
            &close_keys,
            [
                DraftLocation {
                    draft_id: matching_draft,
                    collection_id: deleted_collection,
                    parent_folder_id: None,
                },
                DraftLocation {
                    draft_id: other_draft,
                    collection_id: keep_collection,
                    parent_folder_id: None,
                },
            ],
        );

        assert_eq!(draft_keys, vec![ItemKey::request_draft(matching_draft)]);
    }

    #[test]
    fn draft_descendant_closure_matches_deleted_folder() {
        let collection_id = CollectionId::new();
        let deleted_folder = FolderId::new();
        let child_draft = RequestDraftId::new();
        let root_draft = RequestDraftId::new();

        let close_keys = vec![ItemKey::folder(deleted_folder)];
        let draft_keys = draft_descendant_close_keys(
            &close_keys,
            [
                DraftLocation {
                    draft_id: child_draft,
                    collection_id,
                    parent_folder_id: Some(deleted_folder),
                },
                DraftLocation {
                    draft_id: root_draft,
                    collection_id,
                    parent_folder_id: None,
                },
            ],
        );

        assert_eq!(draft_keys, vec![ItemKey::request_draft(child_draft)]);
    }

    #[test]
    fn draft_descendant_closure_avoids_duplicates_when_both_match() {
        let collection_id = CollectionId::new();
        let folder_id = FolderId::new();
        let draft_id = RequestDraftId::new();

        let close_keys = vec![
            ItemKey::collection(collection_id),
            ItemKey::folder(folder_id),
        ];
        let draft_keys = draft_descendant_close_keys(
            &close_keys,
            [DraftLocation {
                draft_id,
                collection_id,
                parent_folder_id: Some(folder_id),
            }],
        );

        assert_eq!(draft_keys, vec![ItemKey::request_draft(draft_id)]);
    }

    #[test]
    fn duplicate_request_name_uses_copy_suffix_with_increment() {
        let existing = vec![
            "My Request".to_string(),
            "My Request (Copy)".to_string(),
            "My Request (Copy 2)".to_string(),
        ];
        assert_eq!(
            next_duplicate_request_name("My Request", &existing),
            "My Request (Copy 3)"
        );
    }
}
