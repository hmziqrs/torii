use serde::{Deserialize, Serialize};

use super::{
    ids::{CollectionId, FolderId},
    revision::RevisionMetadata,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub id: FolderId,
    pub collection_id: CollectionId,
    pub parent_folder_id: Option<FolderId>,
    pub name: String,
    pub sort_order: i64,
    pub meta: RevisionMetadata,
}

impl Folder {
    pub fn new(
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
        name: impl Into<String>,
        sort_order: i64,
    ) -> Self {
        Self {
            id: FolderId::new(),
            collection_id,
            parent_folder_id,
            name: name.into(),
            sort_order,
            meta: RevisionMetadata::new_now(),
        }
    }
}
