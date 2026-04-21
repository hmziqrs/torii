use serde::{Deserialize, Serialize};

use super::{
    ids::{CollectionId, EnvironmentId},
    revision::RevisionMetadata,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    pub id: EnvironmentId,
    pub collection_id: CollectionId,
    pub name: String,
    pub variables_json: String,
    pub meta: RevisionMetadata,
}

impl Environment {
    pub fn new(collection_id: CollectionId, name: impl Into<String>) -> Self {
        Self {
            id: EnvironmentId::new(),
            collection_id,
            name: name.into(),
            variables_json: "[]".to_string(),
            meta: RevisionMetadata::new_now(),
        }
    }
}
