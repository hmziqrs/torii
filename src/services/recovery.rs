use std::{collections::HashSet, sync::Arc, time::Duration};

use anyhow::{Context, Result};

use crate::{
    infra::{blobs::BlobStore, db::Database},
    repos::history_repo::HistoryRepoRef,
};

#[derive(Debug, Clone, Default)]
pub struct RecoveryReport {
    pub stale_temp_removed: usize,
    pub orphan_blob_removed: usize,
    pub pending_history_failed: usize,
}

#[derive(Clone)]
pub struct RecoveryCoordinator {
    db: Arc<Database>,
    history_repo: HistoryRepoRef,
    blob_store: Arc<BlobStore>,
    stale_temp_max_age: Duration,
    history_retention_days: i64,
}

impl RecoveryCoordinator {
    pub fn new(
        db: Arc<Database>,
        history_repo: HistoryRepoRef,
        blob_store: Arc<BlobStore>,
    ) -> Self {
        Self {
            db,
            history_repo,
            blob_store,
            stale_temp_max_age: Duration::from_secs(60 * 60 * 24),
            history_retention_days: 30,
        }
    }

    pub fn with_stale_temp_max_age(mut self, stale_temp_max_age: Duration) -> Self {
        self.stale_temp_max_age = stale_temp_max_age;
        self
    }

    pub fn with_history_retention_days(mut self, days: i64) -> Self {
        self.history_retention_days = days.max(1);
        self
    }

    pub fn run_startup_recovery(&self) -> Result<RecoveryReport> {
        let started_at = time::OffsetDateTime::now_utc().unix_timestamp();

        let stale_temp_removed = self
            .blob_store
            .cleanup_stale_temp_files(self.stale_temp_max_age)
            .context("failed to clean stale temp blobs")?;
        let pending_history_failed = self
            .history_repo
            .mark_pending_as_failed_on_startup()
            .context("failed to reconcile pending history rows")?;
        let history_retention_cutoff_ms = time::OffsetDateTime::now_utc()
            .checked_sub(time::Duration::days(self.history_retention_days))
            .map(|ts| ts.unix_timestamp() * 1000)
            .unwrap_or(0);
        let workspace_ids = self
            .db
            .block_on(async {
                let workspace_rows = sqlx::query(
                    "SELECT DISTINCT workspace_id FROM history_index WHERE workspace_id IS NOT NULL",
                )
                .fetch_all(self.db.pool())
                .await?;
                let mut ids = Vec::new();
                for row in workspace_rows {
                    let workspace_id: String = sqlx::Row::get(&row, "workspace_id");
                    ids.push(crate::domain::ids::WorkspaceId::parse(&workspace_id)?);
                }
                Ok::<Vec<crate::domain::ids::WorkspaceId>, anyhow::Error>(ids)
            })
            .context("failed to enumerate workspaces for history retention pruning")?;
        let mut deleted_history_rows = 0usize;
        for workspace_id in workspace_ids {
            deleted_history_rows += self
                .history_repo
                .delete_before(workspace_id, history_retention_cutoff_ms)
                .context("failed to prune retained history rows during startup recovery")?;
        }

        let referenced_hashes: HashSet<String> = self
            .history_repo
            .referenced_blob_hashes()
            .context("failed to query referenced blob hashes")?;
        let request_body_hashes = self
            .db
            .block_on(async {
                let rows = sqlx::query(
                    "SELECT DISTINCT body_blob_hash
                     FROM requests
                     WHERE body_blob_hash IS NOT NULL",
                )
                .fetch_all(self.db.pool())
                .await?;

                let mut values = HashSet::new();
                for row in rows {
                    let value: String = sqlx::Row::get(&row, "body_blob_hash");
                    values.insert(value);
                }

                Ok::<HashSet<String>, sqlx::Error>(values)
            })
            .context("failed to query request blob hashes")?;
        let mut referenced_hashes = referenced_hashes;
        referenced_hashes.extend(request_body_hashes);
        let orphan_blob_removed = self
            .blob_store
            .cleanup_orphan_blobs(&referenced_hashes)
            .context("failed to cleanup orphan blobs")?;
        if orphan_blob_removed > 0 {
            tracing::warn!(
                orphan_blob_removed,
                "orphan-blob cleanup removed unreferenced blobs during startup recovery"
            );
        }

        let finished_at = time::OffsetDateTime::now_utc().unix_timestamp();
        self.db.block_on(async {
            sqlx::query(
                "INSERT INTO startup_recovery_log
                 (started_at, finished_at, stale_temp_removed, orphan_blob_removed, pending_history_failed)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(started_at)
            .bind(finished_at)
            .bind(stale_temp_removed as i64)
            .bind(orphan_blob_removed as i64)
            .bind(pending_history_failed as i64)
            .execute(self.db.pool())
            .await
        })?;

        let report = RecoveryReport {
            stale_temp_removed,
            orphan_blob_removed,
            pending_history_failed,
        };

        tracing::info!(
            stale_temp_removed = report.stale_temp_removed,
            orphan_blob_removed = report.orphan_blob_removed,
            pending_history_failed = report.pending_history_failed,
            deleted_history_rows,
            "startup recovery completed"
        );

        Ok(report)
    }
}
