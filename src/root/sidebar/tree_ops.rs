use super::super::{AppRoot, services};
use super::tree_view::{TreeDragPayload, TreeDropTarget};
use crate::{
    domain::{
        collection::{Collection, CollectionStorageKind},
        ids::{CollectionId, FolderId, RequestId, WorkspaceId},
    },
    infra::linked_collection_format::{
        LinkedCollectionState, LinkedSiblingId, read_linked_collection, write_linked_collection,
    },
    services::workspace_tree::{FolderTree, TreeItem},
};

impl AppRoot {
    pub(crate) fn prune_collapsed_folder_ids(&mut self) {
        let Some(workspace) = self.catalog.selected_workspace() else {
            self.collapsed_folder_ids.clear();
            return;
        };

        self.collapsed_folder_ids.retain(|folder_id| {
            workspace
                .collections
                .iter()
                .any(|collection| collection.find_folder_tree(*folder_id).is_some())
        });
    }

    pub(super) fn toggle_folder_collapsed(
        &mut self,
        folder_id: FolderId,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.collapsed_folder_ids.insert(folder_id) {
            self.collapsed_folder_ids.remove(&folder_id);
        }
        cx.notify();
    }

    pub(super) fn apply_tree_drop(
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

        if let Some(target_folder_id) = target_parent
            && self.folder_is_descendant_of(target_folder_id, dragged_folder_id)
        {
            return Err("cannot drop a folder into its descendant".to_string());
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
