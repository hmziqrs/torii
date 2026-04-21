use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{
    ids::{CollectionId, WorkspaceId},
    revision::RevisionMetadata,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionStorageKind {
    Managed,
    Linked,
}

impl Default for CollectionStorageKind {
    fn default() -> Self {
        Self::Managed
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CollectionStorageConfig {
    pub linked_root_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collection {
    pub id: CollectionId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub sort_order: i64,
    pub storage_kind: CollectionStorageKind,
    pub storage_config: CollectionStorageConfig,
    pub meta: RevisionMetadata,
}

impl Collection {
    pub fn new(workspace_id: WorkspaceId, name: impl Into<String>, sort_order: i64) -> Self {
        Self {
            id: CollectionId::new(),
            workspace_id,
            name: name.into(),
            sort_order,
            storage_kind: CollectionStorageKind::Managed,
            storage_config: CollectionStorageConfig::default(),
            meta: RevisionMetadata::new_now(),
        }
    }
}
