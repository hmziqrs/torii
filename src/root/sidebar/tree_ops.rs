use super::super::{AppRoot, services};
use super::tree_view::{
    TreeDragPayload as UiTreeDragPayload, TreeDropIntent as UiTreeDropIntent,
    TreeDropTarget as UiTreeDropTarget,
};
use crate::{
    domain::ids::{CollectionId, FolderId},
    services::tree_mutation::{
        TreeDragPayload, TreeDropIntent, TreeDropTarget, TreeMutationService,
    },
    session::workspace_session::ExpandableItem,
};

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
        dragged: UiTreeDragPayload,
        intent: UiTreeDropIntent,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        let workspace = self
            .catalog
            .selected_workspace()
            .cloned()
            .ok_or_else(|| "no selected workspace".to_string())?;
        let service = TreeMutationService::new(workspace, services(cx).repos.clone());
        service.apply_tree_drop(map_dragged(dragged), map_intent(intent))?;
        self.refresh_catalog(cx);
        self.persist_session_state(cx);
        Ok(())
    }
}

fn map_dragged(payload: UiTreeDragPayload) -> TreeDragPayload {
    match payload {
        UiTreeDragPayload::Collection(id) => TreeDragPayload::Collection(id),
        UiTreeDragPayload::Folder(id) => TreeDragPayload::Folder(id),
        UiTreeDragPayload::Request(id) => TreeDragPayload::Request(id),
    }
}

fn map_target(target: UiTreeDropTarget) -> TreeDropTarget {
    match target {
        UiTreeDropTarget::Collection(id) => TreeDropTarget::Collection(id),
        UiTreeDropTarget::Folder(id) => TreeDropTarget::Folder(id),
        UiTreeDropTarget::Request(id) => TreeDropTarget::Request(id),
    }
}

fn map_intent(intent: UiTreeDropIntent) -> TreeDropIntent {
    match intent {
        UiTreeDropIntent::Before(target) => TreeDropIntent::Before(map_target(target)),
        UiTreeDropIntent::Into(target) => TreeDropIntent::Into(map_target(target)),
        UiTreeDropIntent::After(target) => TreeDropIntent::After(map_target(target)),
    }
}
