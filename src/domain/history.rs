use serde::{Deserialize, Serialize};

use super::ids::{HistoryEntryId, RequestId, WorkspaceId};

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
}
