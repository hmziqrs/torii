use serde::{Deserialize, Serialize};

use super::{
    ids::WorkspaceId,
    revision::{RevisionMetadata, now_unix_ts},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub meta: RevisionMetadata,
}

impl Workspace {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: WorkspaceId::new(),
            name: name.into(),
            meta: RevisionMetadata::new_now(),
        }
    }

    pub fn rename(&mut self, name: impl Into<String>) {
        self.name = name.into();
        self.meta.updated_at = now_unix_ts();
        self.meta.revision += 1;
    }
}
