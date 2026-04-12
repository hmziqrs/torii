use serde::{Deserialize, Serialize};

use crate::domain::ids::{
    CollectionId, EnvironmentId, FolderId, RequestDraftId, RequestId, WorkspaceId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemId {
    Workspace(WorkspaceId),
    Collection(CollectionId),
    Folder(FolderId),
    Environment(EnvironmentId),
    Request(RequestId),
    RequestDraft(RequestDraftId),
}

impl From<WorkspaceId> for ItemId {
    fn from(value: WorkspaceId) -> Self {
        Self::Workspace(value)
    }
}

impl From<CollectionId> for ItemId {
    fn from(value: CollectionId) -> Self {
        Self::Collection(value)
    }
}

impl From<FolderId> for ItemId {
    fn from(value: FolderId) -> Self {
        Self::Folder(value)
    }
}

impl From<EnvironmentId> for ItemId {
    fn from(value: EnvironmentId) -> Self {
        Self::Environment(value)
    }
}

impl From<RequestId> for ItemId {
    fn from(value: RequestId) -> Self {
        Self::Request(value)
    }
}

impl From<RequestDraftId> for ItemId {
    fn from(value: RequestDraftId) -> Self {
        Self::RequestDraft(value)
    }
}
