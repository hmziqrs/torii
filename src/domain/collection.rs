use serde::{Deserialize, Serialize};

use super::{
    ids::{CollectionId, WorkspaceId},
    revision::RevisionMetadata,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collection {
    pub id: CollectionId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub sort_order: i64,
    pub meta: RevisionMetadata,
}

impl Collection {
    pub fn new(workspace_id: WorkspaceId, name: impl Into<String>, sort_order: i64) -> Self {
        Self {
            id: CollectionId::new(),
            workspace_id,
            name: name.into(),
            sort_order,
            meta: RevisionMetadata::new_now(),
        }
    }
}
