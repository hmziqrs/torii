use anyhow::Result;
use gpui_component::IconName;

use crate::{
    domain::{
        collection::{Collection, CollectionStorageKind},
        environment::Environment,
        folder::Folder,
        ids::{CollectionId, FolderId, WorkspaceId},
        item_id::ItemId,
        request::RequestItem,
        workspace::Workspace,
    },
    infra::linked_collection_format::read_linked_collection,
    repos::{
        collection_repo::CollectionRepoRef, environment_repo::EnvironmentRepoRef,
        folder_repo::FolderRepoRef, request_repo::RequestRepoRef, workspace_repo::WorkspaceRepoRef,
    },
    session::item_key::{ItemKey, ItemKind},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceCatalog {
    pub workspaces: Vec<Workspace>,
    pub selected_workspace: Option<WorkspaceTree>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceTree {
    pub workspace: Workspace,
    pub collections: Vec<CollectionTree>,
    pub environments: Vec<Environment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionTree {
    pub collection: Collection,
    pub children: Vec<TreeItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderTree {
    pub folder: Folder,
    pub children: Vec<TreeItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeItem {
    Folder(FolderTree),
    Request(RequestItem),
}

pub fn load_workspace_catalog(
    workspaces: &WorkspaceRepoRef,
    collections: &CollectionRepoRef,
    folders: &FolderRepoRef,
    requests: &RequestRepoRef,
    environments: &EnvironmentRepoRef,
    selected_workspace_id: Option<WorkspaceId>,
) -> Result<WorkspaceCatalog> {
    let workspace_rows = workspaces.list()?;
    let selected_workspace = selected_workspace_id
        .or_else(|| workspace_rows.first().map(|workspace| workspace.id))
        .and_then(|workspace_id| {
            workspace_rows
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .cloned()
        })
        .map(|workspace| {
            build_workspace_tree(workspace, collections, folders, requests, environments)
        })
        .transpose()?;

    Ok(WorkspaceCatalog {
        workspaces: workspace_rows,
        selected_workspace,
    })
}

fn build_workspace_tree(
    workspace: Workspace,
    collections: &CollectionRepoRef,
    folders: &FolderRepoRef,
    requests: &RequestRepoRef,
    environments: &EnvironmentRepoRef,
) -> Result<WorkspaceTree> {
    let collection_rows = collections.list_by_workspace(workspace.id)?;
    let environment_rows = environments.list_by_workspace(workspace.id)?;

    let mut collection_trees = Vec::with_capacity(collection_rows.len());
    for collection in collection_rows {
        let (folder_rows, request_rows) = match collection.storage_kind {
            CollectionStorageKind::Managed => (
                folders.list_by_collection(collection.id)?,
                requests.list_by_collection(collection.id)?,
            ),
            CollectionStorageKind::Linked => {
                match collection.storage_config.linked_root_path.clone() {
                    Some(root) => match read_linked_collection(&root) {
                        Ok(state) => (state.folders, state.requests),
                        Err(err) => {
                            tracing::warn!(
                                collection_id = %collection.id,
                                root = %root.display(),
                                "failed to read linked collection tree: {err}"
                            );
                            (Vec::new(), Vec::new())
                        }
                    },
                    None => {
                        tracing::warn!(
                            collection_id = %collection.id,
                            "linked collection missing root path; rendering empty tree"
                        );
                        (Vec::new(), Vec::new())
                    }
                }
            }
        };
        collection_trees.push(CollectionTree {
            collection,
            children: build_tree_items(&folder_rows, &request_rows, None),
        });
    }

    Ok(WorkspaceTree {
        workspace,
        collections: collection_trees,
        environments: environment_rows,
    })
}

fn build_tree_items(
    folders: &[Folder],
    requests: &[RequestItem],
    parent_folder_id: Option<FolderId>,
) -> Vec<TreeItem> {
    let mut child_folders = folders
        .iter()
        .filter(|folder| folder.parent_folder_id == parent_folder_id)
        .cloned()
        .collect::<Vec<_>>();
    child_folders.sort_by_key(|folder| (folder.sort_order, folder.id.to_string()));

    let mut child_requests = requests
        .iter()
        .filter(|request| request.parent_folder_id == parent_folder_id)
        .cloned()
        .collect::<Vec<_>>();
    child_requests.sort_by_key(|request| (request.sort_order, request.id.to_string()));

    let mut items = Vec::with_capacity(child_folders.len() + child_requests.len());
    for folder in child_folders {
        items.push(TreeItem::Folder(FolderTree {
            folder: folder.clone(),
            children: build_tree_items(folders, requests, Some(folder.id)),
        }));
    }
    for request in child_requests {
        items.push(TreeItem::Request(request));
    }

    items
}

impl WorkspaceCatalog {
    pub fn selected_workspace_id(&self) -> Option<WorkspaceId> {
        self.selected_workspace
            .as_ref()
            .map(|workspace| workspace.workspace.id)
    }

    pub fn first_workspace_id(&self) -> Option<WorkspaceId> {
        self.workspaces.first().map(|workspace| workspace.id)
    }

    pub fn contains(&self, item: ItemKey) -> bool {
        self.find_title(item).is_some()
    }

    pub fn selected_workspace(&self) -> Option<&WorkspaceTree> {
        self.selected_workspace.as_ref()
    }

    pub fn find_collection(&self, id: CollectionId) -> Option<&CollectionTree> {
        self.selected_workspace.as_ref().and_then(|workspace| {
            workspace
                .collections
                .iter()
                .find(|collection| collection.collection.id == id)
        })
    }

    pub fn find_environment(&self, id: crate::domain::ids::EnvironmentId) -> Option<&Environment> {
        self.selected_workspace.as_ref().and_then(|workspace| {
            workspace
                .environments
                .iter()
                .find(|environment| environment.id == id)
        })
    }

    pub fn find_title(&self, item: ItemKey) -> Option<String> {
        match (item.kind, item.id) {
            (ItemKind::Workspace, Some(ItemId::Workspace(id))) => self
                .workspaces
                .iter()
                .find(|workspace| workspace.id == id)
                .map(|workspace| workspace.name.clone()),
            (ItemKind::Collection, Some(ItemId::Collection(id))) => self
                .selected_workspace
                .as_ref()
                .and_then(|workspace| {
                    workspace
                        .collections
                        .iter()
                        .find(|collection| collection.collection.id == id)
                })
                .map(|collection| collection.collection.name.clone()),
            (ItemKind::Folder, Some(ItemId::Folder(id))) => self
                .selected_workspace
                .as_ref()
                .and_then(|workspace| {
                    workspace
                        .collections
                        .iter()
                        .find_map(|collection| collection.find_folder(id))
                })
                .map(|folder| folder.name.clone()),
            (ItemKind::Environment, Some(ItemId::Environment(id))) => self
                .selected_workspace
                .as_ref()
                .and_then(|workspace| {
                    workspace
                        .environments
                        .iter()
                        .find(|environment| environment.id == id)
                })
                .map(|environment| environment.name.clone()),
            (ItemKind::Request, Some(ItemId::Request(id))) => self
                .selected_workspace
                .as_ref()
                .and_then(|workspace| {
                    workspace
                        .collections
                        .iter()
                        .find_map(|collection| collection.find_request(id))
                })
                .map(|request| request.name.clone()),
            (ItemKind::Settings, None) => Some(es_fluent::localize("tab_kind_settings", None)),
            (ItemKind::About, None) => Some(es_fluent::localize("tab_kind_about", None)),
            (ItemKind::LayoutDebug, None) => {
                Some(es_fluent::localize("tab_kind_layout_debug", None))
            }
            _ => None,
        }
    }

    pub fn find_icon(&self, item: ItemKey) -> IconName {
        match item.kind {
            ItemKind::Workspace => IconName::Inbox,
            ItemKind::Collection => IconName::BookOpen,
            ItemKind::Folder => IconName::Folder,
            ItemKind::Environment => IconName::Globe,
            ItemKind::Request => IconName::File,
            ItemKind::Settings => IconName::Settings2,
            ItemKind::About => IconName::Info,
            ItemKind::LayoutDebug => IconName::Settings2,
        }
    }

    /// Walk the tree top-down to locate the target item, collecting path segments.
    /// Returns an empty vec for Settings/About or if the item isn't found.
    pub fn find_breadcrumb_path(&self, item: ItemKey) -> Vec<String> {
        let Some(id) = &item.id else {
            return Vec::new();
        };
        match item.kind {
            ItemKind::Workspace => {
                if let ItemId::Workspace(wid) = id {
                    if let Some(ws) = self.workspaces.iter().find(|w| w.id == *wid) {
                        return vec![ws.name.clone()];
                    }
                }
            }
            ItemKind::Collection => {
                if let ItemId::Collection(cid) = id {
                    if let Some(ws) = &self.selected_workspace {
                        let ws_name = ws.workspace.name.clone();
                        if let Some(col) = ws.collections.iter().find(|c| c.collection.id == *cid) {
                            return vec![ws_name, col.collection.name.clone()];
                        }
                    }
                }
            }
            ItemKind::Folder => {
                if let ItemId::Folder(fid) = id {
                    if let Some(ws) = &self.selected_workspace {
                        let ws_name = ws.workspace.name.clone();
                        for col in &ws.collections {
                            if let Some(folder) = col.find_folder_tree(*fid) {
                                return vec![
                                    ws_name.clone(),
                                    col.collection.name.clone(),
                                    folder.folder.name.clone(),
                                ];
                            }
                        }
                    }
                }
            }
            ItemKind::Request => {
                if let ItemId::Request(rid) = id {
                    if let Some(ws) = &self.selected_workspace {
                        let ws_name = ws.workspace.name.clone();
                        for col in &ws.collections {
                            if let Some(path) = Self::find_request_path(&ws_name, col, *rid) {
                                return path;
                            }
                        }
                    }
                }
            }
            ItemKind::Environment => {
                if let ItemId::Environment(eid) = id {
                    if let Some(ws) = &self.selected_workspace {
                        let ws_name = ws.workspace.name.clone();
                        if let Some(env) = ws.environments.iter().find(|e| e.id == *eid) {
                            return vec![ws_name, env.name.clone()];
                        }
                    }
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// Returns the HTTP method string for the given request, or None if not found.
    pub fn find_request_method(&self, id: crate::domain::ids::RequestId) -> Option<String> {
        self.selected_workspace.as_ref().and_then(|ws| {
            ws.collections
                .iter()
                .find_map(|col| col.find_request(id))
                .map(|r| r.method.clone())
        })
    }

    fn find_request_path(
        ws_name: &str,
        col: &CollectionTree,
        request_id: crate::domain::ids::RequestId,
    ) -> Option<Vec<String>> {
        for item in &col.children {
            match item {
                TreeItem::Request(r) if r.id == request_id => {
                    return Some(vec![
                        ws_name.to_string(),
                        col.collection.name.clone(),
                        r.name.clone(),
                    ]);
                }
                TreeItem::Folder(folder) => {
                    if let Some(path) = Self::find_request_in_folder(
                        ws_name,
                        &col.collection.name,
                        folder,
                        request_id,
                    ) {
                        return Some(path);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn find_request_in_folder(
        ws_name: &str,
        col_name: &str,
        folder: &FolderTree,
        request_id: crate::domain::ids::RequestId,
    ) -> Option<Vec<String>> {
        for item in &folder.children {
            match item {
                TreeItem::Request(r) if r.id == request_id => {
                    return Some(vec![
                        ws_name.to_string(),
                        col_name.to_string(),
                        folder.folder.name.clone(),
                        r.name.clone(),
                    ]);
                }
                TreeItem::Folder(child) => {
                    if let Some(path) =
                        Self::find_request_in_folder(ws_name, col_name, child, request_id)
                    {
                        return Some(path);
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub fn delete_closure(&self, item: ItemKey) -> Vec<ItemKey> {
        let mut keys = vec![item];
        if let Some(workspace) = &self.selected_workspace {
            match (item.kind, item.id) {
                (ItemKind::Workspace, Some(ItemId::Workspace(id)))
                    if workspace.workspace.id == id =>
                {
                    for collection in &workspace.collections {
                        keys.push(ItemKey::collection(collection.collection.id));
                        collection.collect_descendants(&mut keys);
                    }
                    for environment in &workspace.environments {
                        keys.push(ItemKey::environment(environment.id));
                    }
                }
                (ItemKind::Collection, Some(ItemId::Collection(id))) => {
                    if let Some(collection) = workspace
                        .collections
                        .iter()
                        .find(|collection| collection.collection.id == id)
                    {
                        collection.collect_descendants(&mut keys);
                    }
                }
                (ItemKind::Folder, Some(ItemId::Folder(id))) => {
                    for collection in &workspace.collections {
                        if let Some(folder) = collection.find_folder_tree(id) {
                            folder.collect_descendants(&mut keys);
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
        keys
    }
}

impl CollectionTree {
    pub fn request_count(&self) -> usize {
        self.children.iter().map(TreeItem::request_count).sum()
    }

    fn find_folder(&self, id: FolderId) -> Option<&Folder> {
        self.find_folder_tree(id).map(|folder| &folder.folder)
    }

    pub fn find_folder_tree(&self, id: FolderId) -> Option<&FolderTree> {
        self.children.iter().find_map(|item| match item {
            TreeItem::Folder(folder) if folder.folder.id == id => Some(folder),
            TreeItem::Folder(folder) => folder.find_folder_tree(id),
            TreeItem::Request(_) => None,
        })
    }

    pub fn find_request(&self, id: crate::domain::ids::RequestId) -> Option<&RequestItem> {
        self.children.iter().find_map(|item| match item {
            TreeItem::Folder(folder) => folder.find_request(id),
            TreeItem::Request(request) if request.id == id => Some(request),
            TreeItem::Request(_) => None,
        })
    }

    fn collect_descendants(&self, keys: &mut Vec<ItemKey>) {
        for item in &self.children {
            match item {
                TreeItem::Folder(folder) => {
                    keys.push(ItemKey::folder(folder.folder.id));
                    folder.collect_descendants(keys);
                }
                TreeItem::Request(request) => keys.push(ItemKey::request(request.id)),
            }
        }
    }
}

impl FolderTree {
    pub fn request_count(&self) -> usize {
        self.children.iter().map(TreeItem::request_count).sum()
    }

    fn find_folder_tree(&self, id: FolderId) -> Option<&FolderTree> {
        self.children.iter().find_map(|item| match item {
            TreeItem::Folder(folder) if folder.folder.id == id => Some(folder),
            TreeItem::Folder(folder) => folder.find_folder_tree(id),
            TreeItem::Request(_) => None,
        })
    }

    fn find_request(&self, id: crate::domain::ids::RequestId) -> Option<&RequestItem> {
        self.children.iter().find_map(|item| match item {
            TreeItem::Folder(folder) => folder.find_request(id),
            TreeItem::Request(request) if request.id == id => Some(request),
            TreeItem::Request(_) => None,
        })
    }

    fn collect_descendants(&self, keys: &mut Vec<ItemKey>) {
        for item in &self.children {
            match item {
                TreeItem::Folder(folder) => {
                    keys.push(ItemKey::folder(folder.folder.id));
                    folder.collect_descendants(keys);
                }
                TreeItem::Request(request) => keys.push(ItemKey::request(request.id)),
            }
        }
    }
}

impl TreeItem {
    fn request_count(&self) -> usize {
        match self {
            TreeItem::Folder(folder) => folder.request_count(),
            TreeItem::Request(_) => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        environment::Environment,
        ids::{CollectionId, EnvironmentId, FolderId, RequestId, WorkspaceId},
        request::RequestItem,
        revision::RevisionMetadata,
        workspace::Workspace,
    };
    use std::mem::discriminant;

    #[test]
    fn utility_item_titles_and_icons_resolve_by_kind() {
        let catalog = WorkspaceCatalog {
            workspaces: Vec::new(),
            selected_workspace: None,
        };

        assert_eq!(
            catalog.find_title(ItemKey::settings()),
            Some(es_fluent::localize("tab_kind_settings", None))
        );
        assert_eq!(
            catalog.find_title(ItemKey::about()),
            Some(es_fluent::localize("tab_kind_about", None))
        );
        assert_eq!(
            discriminant(&catalog.find_icon(ItemKey::settings())),
            discriminant(&IconName::Settings2)
        );
        assert_eq!(
            discriminant(&catalog.find_icon(ItemKey::about())),
            discriminant(&IconName::Info)
        );
    }

    #[test]
    fn persisted_item_titles_and_icons_resolve_by_kind() {
        let workspace_id = WorkspaceId::new();
        let collection_id = CollectionId::new();
        let folder_id = FolderId::new();
        let request_id = RequestId::new();
        let environment_id = EnvironmentId::new();

        let catalog = WorkspaceCatalog {
            workspaces: vec![Workspace {
                id: workspace_id,
                name: "Workspace A".into(),
                variables_json: "[]".into(),
                meta: RevisionMetadata::new_now(),
            }],
            selected_workspace: Some(WorkspaceTree {
                workspace: Workspace {
                    id: workspace_id,
                    name: "Workspace A".into(),
                    variables_json: "[]".into(),
                    meta: RevisionMetadata::new_now(),
                },
                collections: vec![CollectionTree {
                    collection: Collection {
                        id: collection_id,
                        workspace_id,
                        name: "Collection A".into(),
                        sort_order: 0,
                        storage_kind: crate::domain::collection::CollectionStorageKind::Managed,
                        storage_config: crate::domain::collection::CollectionStorageConfig::default(
                        ),
                        meta: RevisionMetadata::new_now(),
                    },
                    children: vec![TreeItem::Folder(FolderTree {
                        folder: Folder {
                            id: folder_id,
                            collection_id,
                            parent_folder_id: None,
                            name: "Folder A".into(),
                            sort_order: 0,
                            meta: RevisionMetadata::new_now(),
                        },
                        children: vec![TreeItem::Request({
                            let mut r = RequestItem::new(
                                collection_id,
                                Some(folder_id),
                                "Request A",
                                "GET",
                                "https://example.test",
                                0,
                            );
                            r.id = request_id;
                            r
                        })],
                    })],
                }],
                environments: vec![Environment {
                    id: environment_id,
                    workspace_id,
                    name: "Env A".into(),
                    variables_json: "[]".into(),
                    meta: RevisionMetadata::new_now(),
                }],
            }),
        };

        assert_eq!(
            catalog.find_title(ItemKey::workspace(workspace_id)),
            Some("Workspace A".into())
        );
        assert_eq!(
            catalog.find_title(ItemKey::collection(collection_id)),
            Some("Collection A".into())
        );
        assert_eq!(
            catalog.find_title(ItemKey::folder(folder_id)),
            Some("Folder A".into())
        );
        assert_eq!(
            catalog.find_title(ItemKey::request(request_id)),
            Some("Request A".into())
        );
        assert_eq!(
            catalog.find_title(ItemKey::environment(environment_id)),
            Some("Env A".into())
        );
        assert_eq!(
            discriminant(&catalog.find_icon(ItemKey::workspace(workspace_id))),
            discriminant(&IconName::Inbox)
        );
        assert_eq!(
            discriminant(&catalog.find_icon(ItemKey::collection(collection_id))),
            discriminant(&IconName::BookOpen)
        );
        assert_eq!(
            discriminant(&catalog.find_icon(ItemKey::folder(folder_id))),
            discriminant(&IconName::Folder)
        );
        assert_eq!(
            discriminant(&catalog.find_icon(ItemKey::request(request_id))),
            discriminant(&IconName::File)
        );
        assert_eq!(
            discriminant(&catalog.find_icon(ItemKey::environment(environment_id))),
            discriminant(&IconName::Globe)
        );
    }
}
