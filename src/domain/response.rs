use bytes::Bytes;

/// Bounded response body reference — keeps only a preview in hot state.
#[derive(Debug, Clone)]
pub enum BodyRef {
    Empty,
    InMemoryPreview {
        bytes: Bytes,
        truncated: bool,
    },
    DiskBlob {
        blob_id: String,
        preview: Option<Bytes>,
        size_bytes: u64,
    },
}

impl BodyRef {
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub fn size_bytes(&self) -> u64 {
        match self {
            Self::Empty => 0,
            Self::InMemoryPreview { bytes, .. } => bytes.len() as u64,
            Self::DiskBlob { size_bytes, .. } => *size_bytes,
        }
    }
}

/// Summary of a response attached to the editor state.
#[derive(Debug, Clone)]
pub struct ResponseSummary {
    pub status_code: u16,
    pub status_text: String,
    pub headers_json: Option<String>,
    pub media_type: Option<String>,
    pub body_ref: BodyRef,
    pub total_ms: Option<u64>,
    pub ttfb_ms: Option<u64>,
}

/// Response budget constants (Phase 3).
pub struct ResponseBudgets;

impl ResponseBudgets {
    /// Per-response in-memory preview cap: 2 MiB.
    pub const PREVIEW_CAP_BYTES: usize = 2 * 1024 * 1024;
    /// Per-tab total volatile response footprint cap: 32 MiB.
    pub const PER_TAB_CAP_BYTES: usize = 32 * 1024 * 1024;
}
