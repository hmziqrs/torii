use std::path::PathBuf;

use anyhow::{Result, anyhow};

use crate::{
    domain::{
        collection::{Collection, CollectionStorageConfig, CollectionStorageKind},
        environment::Environment,
        ids::{CollectionId, FolderId, WorkspaceId},
        request::RequestItem,
    },
    infra::linked_collection_format::{
        LinkedCollectionState, LinkedSiblingId, ensure_not_reserved_name, read_linked_collection,
        write_linked_collection,
    },
    repos::{
        collection_repo::CollectionRepoRef, environment_repo::EnvironmentRepoRef,
        request_repo::RequestRepoRef,
    },
};

const UNTITLED_REQUEST_NAME: &str = "Untitled Request";

#[derive(Clone)]
pub struct CollectionStoreRepos {
    pub requests: RequestRepoRef,
    pub environments: EnvironmentRepoRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedCollectionStore {
    pub collection_id: CollectionId,
    pub workspace_id: WorkspaceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedCollectionStore {
    pub collection_id: CollectionId,
    pub workspace_id: WorkspaceId,
    pub root_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedCollectionStore {
    Managed(ManagedCollectionStore),
    Linked(LinkedCollectionStore),
}

#[derive(Clone)]
pub struct CollectionStoreResolver {
    collections: CollectionRepoRef,
}

impl CollectionStoreResolver {
    pub fn new(collections: CollectionRepoRef) -> Self {
        Self { collections }
    }

    pub fn resolve(&self, collection_id: CollectionId) -> Result<ResolvedCollectionStore> {
        let collection = self
            .collections
            .get(collection_id)?
            .ok_or_else(|| anyhow!("collection not found: {}", collection_id))?;

        match collection.storage_kind {
            CollectionStorageKind::Managed => {
                Ok(ResolvedCollectionStore::Managed(ManagedCollectionStore {
                    collection_id: collection.id,
                    workspace_id: collection.workspace_id,
                }))
            }
            CollectionStorageKind::Linked => {
                let root_path = collection
                    .storage_config
                    .linked_root_path
                    .clone()
                    .ok_or_else(|| {
                        anyhow!(
                            "linked collection {} is missing linked_root_path",
                            collection.id
                        )
                    })?;
                Ok(ResolvedCollectionStore::Linked(LinkedCollectionStore {
                    collection_id: collection.id,
                    workspace_id: collection.workspace_id,
                    root_path,
                }))
            }
        }
    }
}

impl ResolvedCollectionStore {
    pub fn list_requests(&self, repos: &CollectionStoreRepos) -> Result<Vec<RequestItem>> {
        match self {
            Self::Managed(store) => repos.requests.list_by_collection(store.collection_id),
            Self::Linked(store) => {
                let state =
                    read_linked_collection(&store.root_path, &linked_collection_stub(store))?;
                Ok(state.requests)
            }
        }
    }

    pub fn create_request(
        &self,
        repos: &CollectionStoreRepos,
        parent_folder_id: Option<FolderId>,
        name: &str,
        method: &str,
        url: &str,
    ) -> Result<RequestItem> {
        match self {
            Self::Managed(store) => {
                repos
                    .requests
                    .create(store.collection_id, parent_folder_id, name, method, url)
            }
            Self::Linked(store) => {
                let mut state =
                    read_linked_collection(&store.root_path, &linked_collection_stub(store))?;
                let sibling_names = state
                    .requests
                    .iter()
                    .filter(|request| request.parent_folder_id == parent_folder_id)
                    .map(|request| request.name.clone())
                    .collect::<Vec<_>>();
                let effective_name = if name == UNTITLED_REQUEST_NAME {
                    next_postman_style_name(UNTITLED_REQUEST_NAME, &sibling_names)
                } else {
                    name.to_string()
                };
                ensure_not_reserved_name(&effective_name)?;

                let next_sort =
                    next_request_sort(&state.folders, &state.requests, parent_folder_id);
                let request = RequestItem::new(
                    store.collection_id,
                    parent_folder_id,
                    &effective_name,
                    method,
                    url,
                    next_sort,
                );

                if let Some(parent) = parent_folder_id {
                    let parent_exists = state.folders.iter().any(|folder| folder.id == parent);
                    if !parent_exists {
                        return Err(anyhow!("target parent folder does not exist"));
                    }
                    state.folder_child_orders.entry(parent).or_default().push(
                        LinkedSiblingId::Request {
                            id: request.id.to_string(),
                        },
                    );
                } else {
                    state.root_child_order.push(LinkedSiblingId::Request {
                        id: request.id.to_string(),
                    });
                }
                state.requests.push(request.clone());

                write_linked_collection(&store.root_path, &state)?;
                Ok(request)
            }
        }
    }

    pub fn list_environments(&self, repos: &CollectionStoreRepos) -> Result<Vec<Environment>> {
        match self {
            Self::Managed(store) => repos.environments.list_by_workspace(store.workspace_id),
            Self::Linked(store) => {
                let state =
                    read_linked_collection(&store.root_path, &linked_collection_stub(store))?;
                Ok(state
                    .environments
                    .into_iter()
                    .map(|mut environment| {
                        environment.workspace_id = store.workspace_id;
                        environment
                    })
                    .collect())
            }
        }
    }

    pub fn create_environment(
        &self,
        repos: &CollectionStoreRepos,
        name: &str,
    ) -> Result<Environment> {
        match self {
            Self::Managed(store) => repos.environments.create(store.workspace_id, name),
            Self::Linked(store) => {
                let mut state =
                    read_linked_collection(&store.root_path, &linked_collection_stub(store))?;
                let environment = Environment::new(store.workspace_id, name.to_string());
                state.environments.push(environment.clone());
                write_linked_collection(&store.root_path, &state)?;
                Ok(environment)
            }
        }
    }
}

fn next_request_sort(
    folders: &[crate::domain::folder::Folder],
    requests: &[RequestItem],
    parent_folder_id: Option<FolderId>,
) -> i64 {
    folders
        .iter()
        .filter(|folder| folder.parent_folder_id == parent_folder_id)
        .map(|folder| folder.sort_order)
        .chain(
            requests
                .iter()
                .filter(|request| request.parent_folder_id == parent_folder_id)
                .map(|request| request.sort_order),
        )
        .max()
        .unwrap_or(-1)
        + 1
}

fn linked_collection_stub(store: &LinkedCollectionStore) -> Collection {
    let mut collection = Collection::new(store.workspace_id, "Linked Collection", 0);
    collection.id = store.collection_id;
    collection.storage_kind = CollectionStorageKind::Linked;
    collection.storage_config = CollectionStorageConfig {
        linked_root_path: Some(store.root_path.clone()),
    };
    collection
}

fn next_postman_style_name(base: &str, existing_names: &[String]) -> String {
    if !existing_names.iter().any(|name| name == base) {
        return base.to_string();
    }

    let mut index = 2;
    loop {
        let candidate = format!("{base} ({index})");
        if !existing_names.iter().any(|name| name == &candidate) {
            return candidate;
        }
        index += 1;
    }
}

#[allow(dead_code)]
fn _empty_linked_state(state: &LinkedCollectionState) -> bool {
    state.folders.is_empty() && state.requests.is_empty() && state.environments.is_empty()
}
