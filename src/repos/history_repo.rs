use std::{collections::HashSet, sync::Arc};

use anyhow::{Context as _, anyhow};
use sqlx::Row as _;

use crate::domain::{
    history::{HistoryEntry, HistoryState},
    ids::{HistoryEntryId, RequestId, WorkspaceId},
    revision::now_unix_ts,
};

use super::{DbRef, RepoResult};

pub trait HistoryRepository: Send + Sync {
    fn create_pending(
        &self,
        workspace_id: WorkspaceId,
        request_id: Option<RequestId>,
        method: &str,
        url: &str,
    ) -> RepoResult<HistoryEntry>;
    fn finalize_completed(
        &self,
        id: HistoryEntryId,
        status_code: i64,
        blob_hash: Option<&str>,
        blob_size: Option<i64>,
    ) -> RepoResult<()>;
    fn mark_failed(&self, id: HistoryEntryId, message: &str) -> RepoResult<()>;
    fn mark_pending_as_failed_on_startup(&self) -> RepoResult<usize>;
    fn list_recent(&self, workspace_id: WorkspaceId, limit: usize)
    -> RepoResult<Vec<HistoryEntry>>;
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
    ) -> RepoResult<HistoryEntry> {
        let ts = now_unix_ts();
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
        };

        self.db.block_on(async {
            sqlx::query(
                "INSERT INTO history_index
                 (id, workspace_id, request_id, method, url, status_code, started_at, completed_at, state, blob_hash, blob_size, error_message, created_at, updated_at, recovery_attempts, finalized_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
                     finalized_at = ?
                 WHERE id = ?",
            )
            .bind(status_code)
            .bind(ts)
            .bind(HistoryState::Completed.as_str())
            .bind(blob_hash)
            .bind(blob_size)
            .bind(ts)
            .bind(ts)
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
                "SELECT id, workspace_id, request_id, method, url, status_code, started_at, completed_at, state, blob_hash, blob_size, error_message, recovery_attempts, finalized_at, created_at, updated_at
                 FROM history_index
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
    })
}

pub type HistoryRepoRef = Arc<dyn HistoryRepository>;
