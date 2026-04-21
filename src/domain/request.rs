use serde::{Deserialize, Serialize};

use super::{
    ids::{CollectionId, FolderId, RequestId},
    revision::RevisionMetadata,
};

// ---------------------------------------------------------------------------
// Body type variants (Phase 3)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum BodyType {
    None,
    RawText {
        content: String,
    },
    RawJson {
        content: String,
    },
    UrlEncoded {
        entries: Vec<KeyValuePair>,
    },
    FormData {
        text_fields: Vec<KeyValuePair>,
        file_fields: Vec<FileField>,
    },
    BinaryFile {
        blob_hash: String,
        file_name: Option<String>,
    },
}

impl BodyType {
    pub fn none() -> Self {
        Self::None
    }
}

impl Default for BodyType {
    fn default() -> Self {
        Self::None
    }
}

// ---------------------------------------------------------------------------
// Auth type variants (Phase 3)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AuthType {
    None,
    Basic {
        username: String,
        password_secret_ref: Option<String>,
    },
    Bearer {
        token_secret_ref: Option<String>,
    },
    ApiKey {
        key_name: String,
        value_secret_ref: Option<String>,
        location: ApiKeyLocation,
    },
}

impl AuthType {
    pub fn none() -> Self {
        Self::None
    }
}

impl Default for AuthType {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyLocation {
    Header,
    Query,
}

// ---------------------------------------------------------------------------
// Shared value types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyValuePair {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

impl KeyValuePair {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: true,
        }
    }

    pub fn enabled(mut self) -> Self {
        self.enabled = true;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileField {
    pub key: String,
    pub blob_hash: String,
    pub file_name: Option<String>,
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// Scripts placeholder (Phase 3 — persisted but never executed)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptsContent {
    pub pre_request: String,
    pub tests: String,
}

impl Default for ScriptsContent {
    fn default() -> Self {
        Self {
            pre_request: String::new(),
            tests: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Per-request settings (Phase 3)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestSettings {
    /// Per-request timeout override in milliseconds. `None` falls back to the default (30_000 ms).
    pub timeout_ms: Option<u64>,
    /// Per-request redirect override. `None` falls back to the default (true).
    pub follow_redirects: Option<bool>,
}

impl Default for RequestSettings {
    fn default() -> Self {
        Self {
            timeout_ms: None,
            follow_redirects: None,
        }
    }
}

// ---------------------------------------------------------------------------
// RequestItem — the persisted REST request value
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

    // Expanded editor sections (Phase 3)
    pub params: Vec<KeyValuePair>,
    pub headers: Vec<KeyValuePair>,
    pub auth: AuthType,
    pub body: BodyType,
    pub scripts: ScriptsContent,
    pub settings: RequestSettings,
    pub variable_overrides_json: String,
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
            params: Vec::new(),
            headers: Vec::new(),
            auth: AuthType::none(),
            body: BodyType::none(),
            scripts: ScriptsContent::default(),
            settings: RequestSettings::default(),
            variable_overrides_json: "[]".to_string(),
        }
    }

    /// Returns true if the body variant holds a payload that should be persisted in the blob store.
    pub fn needs_body_blob(&self) -> bool {
        matches!(
            self.body,
            BodyType::RawText { .. } | BodyType::RawJson { .. }
        )
    }
}
