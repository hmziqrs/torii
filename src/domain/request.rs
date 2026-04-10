use serde::{Deserialize, Serialize};

use super::{
    ids::{CollectionId, FolderId, RequestId},
    revision::RevisionMetadata,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestItem {
    pub id: RequestId,
    pub collection_id: CollectionId,
    pub parent_folder_id: Option<FolderId>,
    pub name: String,
    pub method: String,
    pub url: String,
    pub body_blob_hash: Option<String>,
    pub sort_order: i64,
    pub meta: RevisionMetadata,
}

impl RequestItem {
    pub fn new(
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
        name: impl Into<String>,
        method: impl Into<String>,
        url: impl Into<String>,
        sort_order: i64,
    ) -> Self {
        Self {
            id: RequestId::new(),
            collection_id,
            parent_folder_id,
            name: name.into(),
            method: method.into(),
            url: url.into(),
            body_blob_hash: None,
            sort_order,
            meta: RevisionMetadata::new_now(),
        }
    }
}
