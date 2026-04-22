use serde::{Deserialize, Serialize};

use super::{
    ids::{EnvironmentId, WorkspaceId},
    revision::RevisionMetadata,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    pub id: EnvironmentId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub variables_json: String,
    pub meta: RevisionMetadata,
}

impl Environment {
    pub fn new(workspace_id: WorkspaceId, name: impl Into<String>) -> Self {
        Self {
            id: EnvironmentId::new(),
            workspace_id,
            name: name.into(),
            variables_json: "[]".to_string(),
            meta: RevisionMetadata::new_now(),
        }
    }
}
