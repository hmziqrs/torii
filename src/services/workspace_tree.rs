use anyhow::Result;
use gpui_component::IconName;

use crate::{
    domain::{
        collection::Collection,
        environment::Environment,
        folder::Folder,
        ids::{CollectionId, FolderId, WorkspaceId},
        item_id::ItemId,
        request::RequestItem,
        workspace::Workspace,
    },
    repos::{
        collection_repo::CollectionRepoRef, environment_repo::EnvironmentRepoRef,
        folder_repo::FolderRepoRef, request_repo::RequestRepoRef, workspace_repo::WorkspaceRepoRef,
    },
    session::item_key::{ItemKey, ItemKind},
};

#[derive(Debug, Clone)]
pub struct WorkspaceCatalog {
    pub workspaces: Vec<Workspace>,
    pub selected_workspace: Option<WorkspaceTree>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceTree {
    pub workspace: Workspace,
    pub collections: Vec<CollectionTree>,
    pub environments: Vec<Environment>,
}

#[derive(Debug, Clone)]
pub struct CollectionTree {
    pub collection: Collection,
    pub children: Vec<TreeItem>,
}

#[derive(Debug, Clone)]
pub struct FolderTree {
    pub folder: Folder,
    pub children: Vec<TreeItem>,
}

#[derive(Debug, Clone)]
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
        .map(|workspace| build_workspace_tree(workspace, collections, folders, requests, environments))
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
        let folder_rows = folders.list_by_collection(collection.id)?;
        let request_rows = requests.list_by_collection(collection.id)?;
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
        self.selected_workspace.as_ref().map(|workspace| workspace.workspace.id)
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
        self.selected_workspace
            .as_ref()
            .and_then(|workspace| workspace.collections.iter().find(|collection| collection.collection.id == id))
    }

    pub fn find_environment(
        &self,
        id: crate::domain::ids::EnvironmentId,
    ) -> Option<&Environment> {
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
        }
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
                meta: RevisionMetadata::new_now(),
            }],
            selected_workspace: Some(WorkspaceTree {
                workspace: Workspace {
                    id: workspace_id,
                    name: "Workspace A".into(),
                    meta: RevisionMetadata::new_now(),
                },
                collections: vec![CollectionTree {
                    collection: Collection {
                        id: collection_id,
                        workspace_id,
                        name: "Collection A".into(),
                        sort_order: 0,
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
                        children: vec![TreeItem::Request(RequestItem {
                            id: request_id,
                            collection_id,
                            parent_folder_id: Some(folder_id),
                            name: "Request A".into(),
                            method: "GET".into(),
                            url: "https://example.test".into(),
                            body_blob_hash: None,
                            sort_order: 0,
                            meta: RevisionMetadata::new_now(),
                        })],
                    })],
                }],
                environments: vec![Environment {
                    id: environment_id,
                    workspace_id,
                    name: "Env A".into(),
                    variables_json: "{}".into(),
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
