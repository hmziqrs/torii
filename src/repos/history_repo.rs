use std::{collections::HashSet, sync::Arc};

use anyhow::{Context as _, anyhow};
use sqlx::Row as _;

use crate::domain::{
    history::{HistoryEntry, HistoryState},
    ids::{HistoryEntryId, RequestId, WorkspaceId},
    revision::now_unix_ts,
};

use super::{DbRef, RepoResult};

/// Redacted snapshot of what was sent, persisted alongside the history row.
pub struct RequestSnapshot {
    pub method: String,
    pub url_redacted: String,
    pub headers_redacted_json: Option<String>,
    pub auth_kind: Option<String>,
    pub body_summary_json: Option<String>,
}

pub trait HistoryRepository: Send + Sync {
    fn create_pending(
        &self,
        workspace_id: WorkspaceId,
        request_id: Option<RequestId>,
        method: &str,
        url: &str,
        snapshot: Option<RequestSnapshot>,
    ) -> RepoResult<HistoryEntry>;
    fn finalize_completed(
        &self,
        id: HistoryEntryId,
        status_code: i64,
        blob_hash: Option<&str>,
        blob_size: Option<i64>,
        response_headers_json: Option<&str>,
        response_media_type: Option<&str>,
        response_meta_v2_json: Option<&str>,
        dispatched_at: Option<i64>,
        first_byte_at: Option<i64>,
    ) -> RepoResult<()>;
    fn mark_failed(&self, id: HistoryEntryId, message: &str) -> RepoResult<()>;
    fn finalize_cancelled(&self, id: HistoryEntryId, partial_size: Option<i64>) -> RepoResult<()>;
    fn mark_pending_as_failed_on_startup(&self) -> RepoResult<usize>;
    fn list_recent(&self, workspace_id: WorkspaceId, limit: usize)
    -> RepoResult<Vec<HistoryEntry>>;
    fn list_for_request(&self, request_id: RequestId, limit: usize)
    -> RepoResult<Vec<HistoryEntry>>;
    fn get_latest_for_request(&self, request_id: RequestId) -> RepoResult<Option<HistoryEntry>>;
    fn referenced_blob_hashes(&self) -> RepoResult<HashSet<String>>;
}

#[derive(Clone)]
pub struct SqliteHistoryRepository {
    db: DbRef,
}

impl SqliteHistoryRepository {
    pub fn new(db: DbRef) -> Self {
        Self { db }
    }
}

impl HistoryRepository for SqliteHistoryRepository {
    fn create_pending(
        &self,
        workspace_id: WorkspaceId,
        request_id: Option<RequestId>,
        method: &str,
        url: &str,
        snapshot: Option<RequestSnapshot>,
    ) -> RepoResult<HistoryEntry> {
        let ts = now_unix_ts();
        let snap = snapshot.as_ref();
        let entry = HistoryEntry {
            id: HistoryEntryId::new(),
            workspace_id,
            request_id,
            method: method.to_string(),
            url: url.to_string(),
            status_code: None,
            started_at: ts,
            completed_at: None,
            state: HistoryState::Pending,
            blob_hash: None,
            blob_size: None,
            error_message: None,
            recovery_attempts: 0,
            finalized_at: None,
            created_at: ts,
            updated_at: ts,
            response_headers_json: None,
            response_media_type: None,
            response_meta_v2_json: None,
            dispatched_at: None,
            first_byte_at: None,
            cancelled_at: None,
            partial_size: None,
            request_method: snap.map(|s| s.method.clone()),
            request_url_redacted: snap.map(|s| s.url_redacted.clone()),
            request_headers_redacted_json: snap.and_then(|s| s.headers_redacted_json.clone()),
            request_auth_kind: snap.and_then(|s| s.auth_kind.clone()),
            request_body_summary_json: snap.and_then(|s| s.body_summary_json.clone()),
        };

        self.db.block_on(async {
            sqlx::query(
                "INSERT INTO history_index
                 (id, workspace_id, request_id, method, url, status_code, started_at, completed_at,
                  state, blob_hash, blob_size, error_message, created_at, updated_at,
                  recovery_attempts, finalized_at,
                  response_headers_json, response_media_type, response_meta_v2_json, dispatched_at, first_byte_at,
                  cancelled_at, partial_size,
                  request_method, request_url_redacted, request_headers_redacted_json, request_auth_kind, request_body_summary_json)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(entry.id.to_string())
            .bind(entry.workspace_id.to_string())
            .bind(entry.request_id.map(|it| it.to_string()))
            .bind(&entry.method)
            .bind(&entry.url)
            .bind(entry.status_code)
            .bind(entry.started_at)
            .bind(entry.completed_at)
            .bind(entry.state.as_str())
            .bind(entry.blob_hash.clone())
            .bind(entry.blob_size)
            .bind(entry.error_message.clone())
            .bind(entry.created_at)
            .bind(entry.updated_at)
            .bind(entry.recovery_attempts)
            .bind(entry.finalized_at)
            .bind(entry.response_headers_json.clone())
            .bind(entry.response_media_type.clone())
            .bind(entry.response_meta_v2_json.clone())
            .bind(entry.dispatched_at)
            .bind(entry.first_byte_at)
            .bind(entry.cancelled_at)
            .bind(entry.partial_size)
            .bind(entry.request_method.clone())
            .bind(entry.request_url_redacted.clone())
            .bind(entry.request_headers_redacted_json.clone())
            .bind(entry.request_auth_kind.clone())
            .bind(entry.request_body_summary_json.clone())
            .execute(self.db.pool())
            .await
            .context("failed to insert pending history row")?;
            Ok::<(), anyhow::Error>(())
        })?;

        Ok(entry)
    }

    fn finalize_completed(
        &self,
        id: HistoryEntryId,
        status_code: i64,
        blob_hash: Option<&str>,
        blob_size: Option<i64>,
        response_headers_json: Option<&str>,
        response_media_type: Option<&str>,
        response_meta_v2_json: Option<&str>,
        dispatched_at: Option<i64>,
        first_byte_at: Option<i64>,
    ) -> RepoResult<()> {
        self.db.block_on(async {
            let ts = now_unix_ts();
            sqlx::query(
                "UPDATE history_index
                 SET status_code = ?,
                     completed_at = ?,
                     state = ?,
                     blob_hash = ?,
                     blob_size = ?,
                     error_message = NULL,
                     updated_at = ?,
                     finalized_at = ?,
                     response_headers_json = ?,
                     response_media_type = ?,
                     response_meta_v2_json = ?,
                     dispatched_at = ?,
                     first_byte_at = ?
                 WHERE id = ?",
            )
            .bind(status_code)
            .bind(ts)
            .bind(HistoryState::Completed.as_str())
            .bind(blob_hash)
            .bind(blob_size)
            .bind(ts)
            .bind(ts)
            .bind(response_headers_json)
            .bind(response_media_type)
            .bind(response_meta_v2_json)
            .bind(dispatched_at)
            .bind(first_byte_at)
            .bind(id.to_string())
            .execute(self.db.pool())
            .await
            .context("failed to finalize history entry")?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn mark_failed(&self, id: HistoryEntryId, message: &str) -> RepoResult<()> {
        self.db.block_on(async {
            let ts = now_unix_ts();
            sqlx::query(
                "UPDATE history_index
                 SET state = ?,
                     completed_at = ?,
                     error_message = ?,
                     updated_at = ?,
                     finalized_at = ?
                 WHERE id = ?",
            )
            .bind(HistoryState::Failed.as_str())
            .bind(ts)
            .bind(message)
            .bind(ts)
            .bind(ts)
            .bind(id.to_string())
            .execute(self.db.pool())
            .await
            .context("failed to mark history as failed")?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn finalize_cancelled(&self, id: HistoryEntryId, partial_size: Option<i64>) -> RepoResult<()> {
        self.db.block_on(async {
            let ts = now_unix_ts();
            sqlx::query(
                "UPDATE history_index
                 SET state = ?,
                     completed_at = ?,
                     cancelled_at = ?,
                     partial_size = ?,
                     blob_hash = NULL,
                     updated_at = ?,
                     finalized_at = ?
                 WHERE id = ?",
            )
            .bind(HistoryState::Cancelled.as_str())
            .bind(ts)
            .bind(ts)
            .bind(partial_size)
            .bind(ts)
            .bind(ts)
            .bind(id.to_string())
            .execute(self.db.pool())
            .await
            .context("failed to finalize cancelled history entry")?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn mark_pending_as_failed_on_startup(&self) -> RepoResult<usize> {
        self.db.block_on(async {
            let ts = now_unix_ts();
            let result = sqlx::query(
                "UPDATE history_index
                 SET state = ?,
                     completed_at = ?,
                     error_message = COALESCE(error_message, 'Recovered as failed during startup'),
                     recovery_attempts = recovery_attempts + 1,
                     updated_at = ?,
                     finalized_at = ?
                 WHERE state = ?",
            )
            .bind(HistoryState::Failed.as_str())
            .bind(ts)
            .bind(ts)
            .bind(ts)
            .bind(HistoryState::Pending.as_str())
            .execute(self.db.pool())
            .await
            .context("failed to mark stale pending history rows")?;
            Ok::<usize, anyhow::Error>(result.rows_affected() as usize)
        })
    }

    fn list_recent(
        &self,
        workspace_id: WorkspaceId,
        limit: usize,
    ) -> RepoResult<Vec<HistoryEntry>> {
        self.db.block_on(async {
            let rows = sqlx::query(
                "SELECT * FROM history_index
                 WHERE workspace_id = ?
                 ORDER BY started_at DESC
                 LIMIT ?",
            )
            .bind(workspace_id.to_string())
            .bind(limit as i64)
            .fetch_all(self.db.pool())
            .await
            .context("failed to list recent history rows")?;
            rows.into_iter().map(map_history_row).collect()
        })
    }

    fn get_latest_for_request(&self, request_id: RequestId) -> RepoResult<Option<HistoryEntry>> {
        self.db.block_on(async {
            let row = sqlx::query(
                "SELECT * FROM history_index
                 WHERE request_id = ? AND state != 'pending'
                 ORDER BY started_at DESC, id DESC
                 LIMIT 1",
            )
            .bind(request_id.to_string())
            .fetch_optional(self.db.pool())
            .await
            .context("failed to get latest history for request")?;
            row.map(map_history_row).transpose()
        })
    }

    fn list_for_request(&self, request_id: RequestId, limit: usize) -> RepoResult<Vec<HistoryEntry>> {
        self.db.block_on(async {
            let rows = sqlx::query(
                "SELECT * FROM history_index
                 WHERE request_id = ?
                 ORDER BY started_at DESC, id DESC
                 LIMIT ?",
            )
            .bind(request_id.to_string())
            .bind(limit as i64)
            .fetch_all(self.db.pool())
            .await
            .context("failed to list history for request")?;
            rows.into_iter().map(map_history_row).collect()
        })
    }

    fn referenced_blob_hashes(&self) -> RepoResult<HashSet<String>> {
        self.db.block_on(async {
            let rows = sqlx::query(
                "SELECT DISTINCT blob_hash FROM history_index WHERE blob_hash IS NOT NULL",
            )
            .fetch_all(self.db.pool())
            .await
            .context("failed to load history blob references")?;
            let mut values = HashSet::new();
            for row in rows {
                let value: String = row.get("blob_hash");
                values.insert(value);
            }
            Ok::<HashSet<String>, anyhow::Error>(values)
        })
    }
}

fn map_history_row(row: sqlx::sqlite::SqliteRow) -> RepoResult<HistoryEntry> {
    let state = match row.get::<String, _>("state").as_str() {
        "pending" => HistoryState::Pending,
        "completed" => HistoryState::Completed,
        "failed" => HistoryState::Failed,
        "cancelled" => HistoryState::Cancelled,
        value => return Err(anyhow!("unknown history state in db: {}", value)),
    };

    Ok(HistoryEntry {
        id: HistoryEntryId::parse(row.get::<&str, _>("id"))?,
        workspace_id: WorkspaceId::parse(row.get::<&str, _>("workspace_id"))?,
        request_id: row
            .get::<Option<String>, _>("request_id")
            .map(|value| RequestId::parse(&value))
            .transpose()?,
        method: row.get("method"),
        url: row.get("url"),
        status_code: row.get("status_code"),
        started_at: row.get("started_at"),
        completed_at: row.get("completed_at"),
        state,
        blob_hash: row.get("blob_hash"),
        blob_size: row.get("blob_size"),
        error_message: row.get("error_message"),
        recovery_attempts: row.get("recovery_attempts"),
        finalized_at: row.get("finalized_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        response_headers_json: row.try_get("response_headers_json").unwrap_or(None),
        response_media_type: row.try_get("response_media_type").unwrap_or(None),
        response_meta_v2_json: row.try_get("response_meta_v2_json").unwrap_or(None),
        dispatched_at: row.try_get("dispatched_at").unwrap_or(None),
        first_byte_at: row.try_get("first_byte_at").unwrap_or(None),
        cancelled_at: row.try_get("cancelled_at").unwrap_or(None),
        partial_size: row.try_get("partial_size").unwrap_or(None),
        request_method: row.try_get("request_method").unwrap_or(None),
        request_url_redacted: row.try_get("request_url_redacted").unwrap_or(None),
        request_headers_redacted_json: row.try_get("request_headers_redacted_json").unwrap_or(None),
        request_auth_kind: row.try_get("request_auth_kind").unwrap_or(None),
        request_body_summary_json: row.try_get("request_body_summary_json").unwrap_or(None),
    })
}

pub type HistoryRepoRef = Arc<dyn HistoryRepository>;

/// Build a secret-safe snapshot of the request about to be sent.
/// Auth secret values are replaced with `[REDACTED]`; all other fields are kept.
pub fn build_request_snapshot(request: &crate::domain::request::RequestItem) -> RequestSnapshot {
    use crate::domain::request::{AuthType, BodyType};

    // Redact URL query values while preserving keys/shape.
    let url_redacted = redact_url_query_values(&request.url);

    // Redact headers: replace auth-derived header values
    let headers_redacted: Vec<(String, String)> = request
        .headers
        .iter()
        .filter(|kv| kv.enabled)
        .map(|kv| {
            let key_lower = kv.key.to_lowercase();
            if key_lower == "authorization" || key_lower == "cookie" {
                (kv.key.clone(), "[REDACTED]".to_string())
            } else {
                (kv.key.clone(), kv.value.clone())
            }
        })
        .collect();
    let headers_redacted_json = if headers_redacted.is_empty() {
        None
    } else {
        serde_json::to_string(&headers_redacted).ok()
    };

    // Auth kind only — never the secret ref or value
    let auth_kind = match &request.auth {
        AuthType::None => None,
        AuthType::Basic { .. } => Some("basic".to_string()),
        AuthType::Bearer { .. } => Some("bearer".to_string()),
        AuthType::ApiKey { .. } => Some("api_key".to_string()),
    };

    // Body summary — kind + size only, never the content
    let body_summary = match &request.body {
        BodyType::None => serde_json::json!({"kind": "none"}),
        BodyType::RawText { content } => {
            serde_json::json!({"kind": "raw_text", "size": content.len()})
        }
        BodyType::RawJson { content } => {
            serde_json::json!({"kind": "raw_json", "size": content.len()})
        }
        BodyType::UrlEncoded { entries } => {
            serde_json::json!({"kind": "urlencoded", "entries": entries.len()})
        }
        BodyType::FormData {
            text_fields,
            file_fields,
        } => {
            serde_json::json!({"kind": "form_data", "text_fields": text_fields.len(), "file_fields": file_fields.len()})
        }
        BodyType::BinaryFile {
            blob_hash,
            file_name,
            ..
        } => {
            serde_json::json!({"kind": "binary_file", "has_blob": !blob_hash.is_empty(), "file_name": file_name})
        }
    };
    let body_summary_json = serde_json::to_string(&body_summary).ok();

    RequestSnapshot {
        method: request.method.clone(),
        url_redacted,
        headers_redacted_json,
        auth_kind,
        body_summary_json,
    }
}

fn redact_url_query_values(raw_url: &str) -> String {
    if let Ok(mut absolute) = url::Url::parse(raw_url) {
        let has_query = absolute.query().is_some();
        if has_query {
            let keys: Vec<String> = absolute.query_pairs().map(|(k, _)| k.to_string()).collect();
            absolute
                .query_pairs_mut()
                .clear()
                .extend_pairs(keys.iter().map(|k| (k.as_str(), "[REDACTED]")));
        }
        return absolute.to_string();
    }

    let (base, fragment) = match raw_url.split_once('#') {
        Some((b, f)) => (b, Some(f)),
        None => (raw_url, None),
    };
    let Some((path, query)) = base.split_once('?') else {
        return raw_url.to_string();
    };

    let redacted_query = query
        .split('&')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let key = segment.split_once('=').map(|(k, _)| k).unwrap_or(segment);
            format!("{key}=[REDACTED]")
        })
        .collect::<Vec<_>>()
        .join("&");

    match fragment {
        Some(fragment) => format!("{path}?{redacted_query}#{fragment}"),
        None => format!("{path}?{redacted_query}"),
    }
}
