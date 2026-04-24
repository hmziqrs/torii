use super::super::{AppRoot, services};
use super::tree_view::{TreeDragPayload, TreeDropTarget};
use crate::{
    domain::{
        collection::{Collection, CollectionStorageKind},
        ids::{CollectionId, FolderId, RequestId, WorkspaceId},
    },
    infra::linked_collection_format::{
        LinkedCollectionState, LinkedSiblingId, linked_folder_paths, move_linked_folder_directory,
        read_linked_collection, write_linked_collection,
    },
    services::workspace_tree::{FolderTree, TreeItem},
    session::workspace_session::ExpandableItem,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MixedSibling {
    Folder(FolderId),
    Request(RequestId),
}

impl MixedSibling {
    fn to_linked(self) -> LinkedSiblingId {
        match self {
            Self::Folder(id) => LinkedSiblingId::Folder { id: id.to_string() },
            Self::Request(id) => LinkedSiblingId::Request { id: id.to_string() },
        }
    }
}

impl AppRoot {
    pub(crate) fn sync_expansion_state_with_catalog(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(workspace) = self.catalog.selected_workspace() else {
            return;
        };
        let workspace_id = workspace.workspace.id;
        let expandable_items = workspace.expandable_items();
        self.session.update(cx, |session, cx| {
            session.seed_expanded_items_for_workspace(workspace_id, expandable_items.clone(), cx);
            session.prune_expanded_items_for_workspace(workspace_id, &expandable_items, cx);
        });
    }

    pub(super) fn toggle_collection_expanded(
        &mut self,
        collection_id: CollectionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(workspace_id) = self.catalog.selected_workspace_id() else {
            return;
        };
        self.session.update(cx, |session, cx| {
            session.toggle_expanded_item(
                workspace_id,
                ExpandableItem::Collection(collection_id),
                cx,
            );
        });
        self.persist_session_state(cx);
    }

    pub(super) fn toggle_folder_expanded(
        &mut self,
        folder_id: FolderId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(workspace_id) = self.catalog.selected_workspace_id() else {
            return;
        };
        self.session.update(cx, |session, cx| {
            session.toggle_expanded_item(workspace_id, ExpandableItem::Folder(folder_id), cx);
        });
        self.persist_session_state(cx);
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
        let Some((source_collection_id, source_parent)) =
            self.find_folder_location(dragged_folder_id)
        else {
            return Err("dragged folder no longer exists".to_string());
        };

        let (target_collection_id, target_parent, insert_before) = match target {
            TreeDropTarget::Collection(collection_id) => (collection_id, None, None),
            TreeDropTarget::Folder(target_folder_id) => {
                if target_folder_id == dragged_folder_id {
                    return Ok(());
                }
                let Some((collection_id, target_folder_parent)) =
                    self.find_folder_location(target_folder_id)
                else {
                    return Err("drop target folder no longer exists".to_string());
                };
                if source_collection_id == collection_id && source_parent == target_folder_parent {
                    (
                        collection_id,
                        target_folder_parent,
                        Some(MixedSibling::Folder(target_folder_id)),
                    )
                } else {
                    (collection_id, Some(target_folder_id), None)
                }
            }
            TreeDropTarget::Request(_) => {
                return Err("folders cannot be dropped onto requests".to_string());
            }
        };

        if let Some(target_folder_id) = target_parent {
            if target_folder_id == dragged_folder_id {
                return Err("cannot drop a folder into itself".to_string());
            }
            if self.folder_is_descendant_of(target_folder_id, dragged_folder_id) {
                return Err("cannot drop a folder into its descendant".to_string());
            }
        }

        let storage_kind =
            self.ensure_same_storage_kind(source_collection_id, target_collection_id)?;
        match storage_kind {
            CollectionStorageKind::Managed => {
                if let Some(insert_before) = insert_before {
                    self.reorder_mixed_after_drop_managed(
                        MixedSibling::Folder(dragged_folder_id),
                        source_collection_id,
                        source_parent,
                        target_collection_id,
                        target_parent,
                        insert_before,
                        cx,
                    )?;
                } else {
                    services(cx)
                        .repos
                        .folder
                        .move_to(dragged_folder_id, target_collection_id, target_parent)
                        .map_err(|err| format!("failed to move folder: {err}"))?;
                }
            }
            CollectionStorageKind::Linked => {
                if source_collection_id != target_collection_id {
                    return Err(
                        "cross-collection drag/drop for linked collections is not supported yet"
                            .to_string(),
                    );
                }
                if let Some(insert_before) = insert_before {
                    self.reorder_mixed_after_drop_linked(
                        MixedSibling::Folder(dragged_folder_id),
                        source_parent,
                        target_parent,
                        insert_before,
                        target_collection_id,
                    )?;
                } else {
                    self.move_linked_folder(
                        dragged_folder_id,
                        target_parent,
                        target_collection_id,
                    )?;
                }
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
        let Some((source_collection_id, source_parent)) =
            self.find_request_location(dragged_request_id)
        else {
            return Err("dragged request no longer exists".to_string());
        };

        let (target_collection_id, target_parent, insert_before) = match target {
            TreeDropTarget::Collection(collection_id) => (collection_id, None, None),
            TreeDropTarget::Folder(target_folder_id) => {
                let Some((collection_id, target_folder_parent)) =
                    self.find_folder_location(target_folder_id)
                else {
                    return Err("drop target folder no longer exists".to_string());
                };
                if source_collection_id == collection_id && source_parent == target_folder_parent {
                    (
                        collection_id,
                        target_folder_parent,
                        Some(MixedSibling::Folder(target_folder_id)),
                    )
                } else {
                    (collection_id, Some(target_folder_id), None)
                }
            }
            TreeDropTarget::Request(target_request_id) => {
                if target_request_id == dragged_request_id {
                    return Ok(());
                }
                let Some((collection_id, target_request_parent)) =
                    self.find_request_location(target_request_id)
                else {
                    return Err("drop target request no longer exists".to_string());
                };
                (
                    collection_id,
                    target_request_parent,
                    Some(MixedSibling::Request(target_request_id)),
                )
            }
        };

        let storage_kind =
            self.ensure_same_storage_kind(source_collection_id, target_collection_id)?;
        match storage_kind {
            CollectionStorageKind::Managed => {
                if let Some(insert_before) = insert_before {
                    self.reorder_mixed_after_drop_managed(
                        MixedSibling::Request(dragged_request_id),
                        source_collection_id,
                        source_parent,
                        target_collection_id,
                        target_parent,
                        insert_before,
                        cx,
                    )?;
                } else {
                    services(cx)
                        .repos
                        .request
                        .move_to(dragged_request_id, target_collection_id, target_parent)
                        .map_err(|err| format!("failed to move request: {err}"))?;
                }
            }
            CollectionStorageKind::Linked => {
                if source_collection_id != target_collection_id {
                    return Err(
                        "cross-collection drag/drop for linked collections is not supported yet"
                            .to_string(),
                    );
                }
                if let Some(insert_before) = insert_before {
                    self.reorder_mixed_after_drop_linked(
                        MixedSibling::Request(dragged_request_id),
                        source_parent,
                        target_parent,
                        insert_before,
                        target_collection_id,
                    )?;
                } else {
                    self.move_linked_request(
                        dragged_request_id,
                        target_parent,
                        target_collection_id,
                    )?;
                }
            }
        }

        self.refresh_catalog(cx);
        self.persist_session_state(cx);
        Ok(())
    }

    fn reorder_mixed_after_drop_managed(
        &mut self,
        dragged: MixedSibling,
        source_collection_id: CollectionId,
        source_parent: Option<FolderId>,
        target_collection_id: CollectionId,
        target_parent: Option<FolderId>,
        insert_before: MixedSibling,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        if dragged == insert_before {
            return Ok(());
        }

        if source_collection_id != target_collection_id || source_parent != target_parent {
            self.move_sibling_managed(dragged, target_collection_id, target_parent, cx)?;
        }

        self.refresh_catalog(cx);
        let mut siblings = self
            .mixed_siblings_for_parent(target_collection_id, target_parent)
            .ok_or_else(|| "drop target parent no longer exists".to_string())?;

        let Some(dragged_pos) = siblings.iter().position(|sibling| *sibling == dragged) else {
            return Err("dragged item no longer exists after move".to_string());
        };
        siblings.remove(dragged_pos);

        let Some(insert_idx) = siblings
            .iter()
            .position(|sibling| *sibling == insert_before)
        else {
            return Err("drop target no longer exists after move".to_string());
        };
        siblings.insert(insert_idx, dragged);

        self.apply_mixed_order_managed(target_collection_id, target_parent, &siblings, cx)
    }

    fn apply_mixed_order_managed(
        &self,
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
        ordered: &[MixedSibling],
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        for sibling in ordered {
            self.move_sibling_managed(*sibling, collection_id, parent_folder_id, cx)?;
        }
        Ok(())
    }

    fn move_sibling_managed(
        &self,
        sibling: MixedSibling,
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        match sibling {
            MixedSibling::Folder(folder_id) => services(cx)
                .repos
                .folder
                .move_to(folder_id, collection_id, parent_folder_id)
                .map_err(|err| format!("failed to move folder: {err}")),
            MixedSibling::Request(request_id) => services(cx)
                .repos
                .request
                .move_to(request_id, collection_id, parent_folder_id)
                .map_err(|err| format!("failed to move request: {err}")),
        }
    }

    fn reorder_mixed_after_drop_linked(
        &self,
        dragged: MixedSibling,
        source_parent: Option<FolderId>,
        target_parent: Option<FolderId>,
        insert_before: MixedSibling,
        collection_id: CollectionId,
    ) -> Result<(), String> {
        self.move_linked_sibling_with_order(
            dragged,
            source_parent,
            target_parent,
            Some(insert_before),
            collection_id,
        )
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

    fn mixed_siblings_for_parent(
        &self,
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
    ) -> Option<Vec<MixedSibling>> {
        let workspace = self.catalog.selected_workspace()?;
        let collection_tree = workspace
            .collections
            .iter()
            .find(|collection| collection.collection.id == collection_id)?;
        let children = match parent_folder_id {
            Some(folder_id) => &collection_tree.find_folder_tree(folder_id)?.children,
            None => &collection_tree.children,
        };
        Some(
            children
                .iter()
                .map(|item| match item {
                    TreeItem::Folder(folder) => MixedSibling::Folder(folder.folder.id),
                    TreeItem::Request(request) => MixedSibling::Request(request.id),
                })
                .collect(),
        )
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
        let source_parent = self
            .find_folder_location(dragged_folder_id)
            .map(|(_, parent)| parent)
            .ok_or_else(|| "dragged folder no longer exists".to_string())?;
        self.move_linked_sibling_with_order(
            MixedSibling::Folder(dragged_folder_id),
            source_parent,
            target_parent,
            None,
            collection_id,
        )
    }

    fn move_linked_request(
        &self,
        dragged_request_id: RequestId,
        target_parent: Option<FolderId>,
        collection_id: CollectionId,
    ) -> Result<(), String> {
        let source_parent = self
            .find_request_location(dragged_request_id)
            .map(|(_, parent)| parent)
            .ok_or_else(|| "dragged request no longer exists".to_string())?;
        self.move_linked_sibling_with_order(
            MixedSibling::Request(dragged_request_id),
            source_parent,
            target_parent,
            None,
            collection_id,
        )
    }

    fn move_linked_sibling_with_order(
        &self,
        dragged: MixedSibling,
        source_parent: Option<FolderId>,
        target_parent: Option<FolderId>,
        insert_before: Option<MixedSibling>,
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
        let previous_folder_paths = linked_folder_paths(&root_path, &state.folders)
            .map_err(|err| format!("failed to resolve linked folder paths: {err}"))?;

        let dragged_linked = dragged.to_linked();
        detach_linked_sibling(&mut state, source_parent, &dragged_linked);
        let insert_before_linked = insert_before.map(MixedSibling::to_linked);

        {
            let destination = linked_children_mut(&mut state, target_parent);
            let insert_idx = match insert_before_linked.as_ref() {
                Some(target) => destination
                    .iter()
                    .position(|sibling| sibling == target)
                    .unwrap_or(destination.len()),
                None => destination.len(),
            };
            destination.insert(insert_idx, dragged_linked.clone());
        }

        match dragged {
            MixedSibling::Folder(folder_id) => {
                let Some(folder) = state
                    .folders
                    .iter_mut()
                    .find(|folder| folder.id == folder_id)
                else {
                    return Err("dragged folder no longer exists".to_string());
                };
                folder.parent_folder_id = target_parent;
            }
            MixedSibling::Request(request_id) => {
                let Some(request) = state
                    .requests
                    .iter_mut()
                    .find(|request| request.id == request_id)
                else {
                    return Err("dragged request no longer exists".to_string());
                };
                request.parent_folder_id = target_parent;
            }
        }

        let mut moved_folder_dir = None;
        if let MixedSibling::Folder(folder_id) = dragged {
            let next_folder_paths = linked_folder_paths(&root_path, &state.folders)
                .map_err(|err| format!("failed to resolve linked folder paths: {err}"))?;
            let old_path = previous_folder_paths
                .get(&folder_id)
                .cloned()
                .ok_or_else(|| "dragged folder path no longer exists".to_string())?;
            let new_path = next_folder_paths
                .get(&folder_id)
                .cloned()
                .ok_or_else(|| "target folder path could not be resolved".to_string())?;
            if old_path != new_path {
                move_linked_folder_directory(&old_path, &new_path)
                    .map_err(|err| format!("failed to move linked folder directory: {err}"))?;
                moved_folder_dir = Some((old_path, new_path));
            }
        }

        renumber_linked_parent(&mut state, source_parent);
        if target_parent != source_parent {
            renumber_linked_parent(&mut state, target_parent);
        }

        if let Err(err) = write_linked_collection(&root_path, &state) {
            if let Some((old_path, new_path)) = moved_folder_dir {
                if let Err(rollback_err) = move_linked_folder_directory(&new_path, &old_path) {
                    tracing::error!(
                        "failed to rollback linked folder move after write failure: {rollback_err}"
                    );
                }
            }
            return Err(format!("failed to write linked collection: {err}"));
        }
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

fn linked_children_mut(
    state: &mut LinkedCollectionState,
    parent_folder_id: Option<FolderId>,
) -> &mut Vec<LinkedSiblingId> {
    match parent_folder_id {
        Some(parent_id) => state.folder_child_orders.entry(parent_id).or_default(),
        None => &mut state.root_child_order,
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

fn renumber_linked_parent(state: &mut LinkedCollectionState, parent_folder_id: Option<FolderId>) {
    let ordered = match parent_folder_id {
        Some(parent_id) => state
            .folder_child_orders
            .get(&parent_id)
            .cloned()
            .unwrap_or_default(),
        None => state.root_child_order.clone(),
    };

    for (index, sibling) in ordered.iter().enumerate() {
        match sibling {
            LinkedSiblingId::Folder { id } => {
                if let Some(folder) = state
                    .folders
                    .iter_mut()
                    .find(|folder| folder.id.to_string() == *id)
                {
                    folder.parent_folder_id = parent_folder_id;
                    folder.sort_order = index as i64;
                }
            }
            LinkedSiblingId::Request { id } => {
                if let Some(request) = state
                    .requests
                    .iter_mut()
                    .find(|request| request.id.to_string() == *id)
                {
                    request.parent_folder_id = parent_folder_id;
                    request.sort_order = index as i64;
                }
            }
        }
    }
}
