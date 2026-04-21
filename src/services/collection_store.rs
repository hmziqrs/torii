use std::path::PathBuf;

use anyhow::{Result, anyhow};

use crate::{
    domain::{
        collection::CollectionStorageKind,
        ids::CollectionId,
    },
    repos::collection_repo::CollectionRepoRef,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedCollectionStore {
    pub collection_id: CollectionId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedCollectionStore {
    pub collection_id: CollectionId,
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
                    root_path,
                }))
            }
        }
    }
}
