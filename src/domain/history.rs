use serde::{Deserialize, Serialize};

use super::ids::{CollectionId, FolderId, HistoryEntryId, RequestId, WorkspaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryState {
    Pending,
    Completed,
    Failed,
    Cancelled,
}

impl HistoryState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: HistoryEntryId,
    pub workspace_id: WorkspaceId,
    pub request_id: Option<RequestId>,
    pub method: String,
    pub url: String,
    pub status_code: Option<i64>,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub state: HistoryState,
    pub blob_hash: Option<String>,
    pub blob_size: Option<i64>,
    pub error_message: Option<String>,
    pub recovery_attempts: i64,
    pub finalized_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,

    // Phase 3 response metadata
    pub response_headers_json: Option<String>,
    pub response_media_type: Option<String>,
    pub response_meta_v2_json: Option<String>,
    pub dispatched_at: Option<i64>,
    pub first_byte_at: Option<i64>,
    pub cancelled_at: Option<i64>,
    pub partial_size: Option<i64>,

    // Phase 3 secret-safe sent-request snapshot
    pub request_url_redacted: Option<String>,
    pub request_method: Option<String>,
    pub request_headers_redacted_json: Option<String>,
    pub request_auth_kind: Option<String>,
    pub request_body_summary_json: Option<String>,

    // Phase 5 protocol and restore/search fields
    pub protocol_kind: String,
    pub request_name: Option<String>,
    pub request_collection_id: Option<CollectionId>,
    pub request_parent_folder_id: Option<FolderId>,
    pub request_snapshot_json: Option<String>,
    pub request_snapshot_blob_hash: Option<String>,
    pub run_summary_json: Option<String>,
    pub transcript_blob_hash: Option<String>,
    pub transcript_size: Option<i64>,
    pub message_count_in: Option<i64>,
    pub message_count_out: Option<i64>,
    pub close_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusFamily {
    Informational,
    Success,
    Redirection,
    ClientError,
    ServerError,
}

impl StatusFamily {
    pub fn bounds(self) -> (u16, u16) {
        match self {
            Self::Informational => (100, 199),
            Self::Success => (200, 299),
            Self::Redirection => (300, 399),
            Self::ClientError => (400, 499),
            Self::ServerError => (500, 599),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistorySort {
    StartedAtDesc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryCursor {
    pub started_at: i64,
    pub id: HistoryEntryId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryQuery {
    pub workspace_id: WorkspaceId,
    pub request_id: Option<RequestId>,
    pub collection_id: Option<CollectionId>,
    pub protocol: Option<String>,
    pub state: Option<HistoryState>,
    pub status_family: Option<StatusFamily>,
    pub status_min: Option<u16>,
    pub status_max: Option<u16>,
    pub method: Option<String>,
    pub url_search: Option<String>,
    pub search: Option<String>,
    pub started_after: Option<i64>,
    pub started_before: Option<i64>,
    pub cursor: Option<HistoryCursor>,
    pub limit: usize,
    pub sort: HistorySort,
}

impl HistoryQuery {
    pub fn for_workspace(workspace_id: WorkspaceId) -> Self {
        Self {
            workspace_id,
            request_id: None,
            collection_id: None,
            protocol: None,
            state: None,
            status_family: None,
            status_min: None,
            status_max: None,
            method: None,
            url_search: None,
            search: None,
            started_after: None,
            started_before: None,
            cursor: None,
            limit: 50,
            sort: HistorySort::StartedAtDesc,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryPage {
    pub rows: Vec<HistoryEntry>,
    pub next_cursor: Option<HistoryCursor>,
    pub total_estimate: Option<u64>,
}
