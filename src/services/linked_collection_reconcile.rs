use std::path::PathBuf;

use crate::domain::ids::CollectionId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedCollectionEvent {
    pub collection_id: CollectionId,
    pub kind: LinkedCollectionEventKind,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkedCollectionEventKind {
    FileAdded,
    FileChanged,
    FileRemoved,
    DirectoryRemoved,
    FullRescanRequested,
}
