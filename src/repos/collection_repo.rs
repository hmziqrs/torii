use std::sync::Arc;

use anyhow::{Context as _, anyhow};
use sqlx::Row as _;

use crate::domain::{
    collection::Collection,
    ids::{CollectionId, WorkspaceId},
    revision::{RevisionMetadata, now_unix_ts},
};

use super::{DbRef, RepoResult};

pub trait CollectionRepository: Send + Sync {
    fn create(&self, workspace_id: WorkspaceId, name: &str) -> RepoResult<Collection>;
    fn list_by_workspace(&self, workspace_id: WorkspaceId) -> RepoResult<Vec<Collection>>;
    fn rename(&self, id: CollectionId, name: &str) -> RepoResult<()>;
    fn move_to_workspace(&self, id: CollectionId, workspace_id: WorkspaceId) -> RepoResult<()>;
    fn reorder_in_workspace(
        &self,
        workspace_id: WorkspaceId,
        ordered_ids: &[CollectionId],
    ) -> RepoResult<()>;
    fn delete(&self, id: CollectionId) -> RepoResult<()>;
}

#[derive(Clone)]
pub struct SqliteCollectionRepository {
    db: DbRef,
}

impl SqliteCollectionRepository {
    pub fn new(db: DbRef) -> Self {
        Self { db }
    }
}

impl CollectionRepository for SqliteCollectionRepository {
    fn create(&self, workspace_id: WorkspaceId, name: &str) -> RepoResult<Collection> {
        let created_at = now_unix_ts();
        let collection = self.db.block_on(async {
            let mut tx = self.db.pool().begin().await?;
            let next_order: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM collections WHERE workspace_id = ?",
            )
            .bind(workspace_id.to_string())
            .fetch_one(&mut *tx)
            .await?;

            let collection = Collection {
                id: CollectionId::new(),
                workspace_id,
                name: name.to_string(),
                sort_order: next_order,
                meta: RevisionMetadata {
                    created_at,
                    updated_at: created_at,
                    revision: 1,
                },
            };

            sqlx::query(
                "INSERT INTO collections (id, workspace_id, name, sort_order, created_at, updated_at, revision)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(collection.id.to_string())
            .bind(collection.workspace_id.to_string())
            .bind(&collection.name)
            .bind(collection.sort_order)
            .bind(collection.meta.created_at)
            .bind(collection.meta.updated_at)
            .bind(collection.meta.revision)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            Ok::<Collection, anyhow::Error>(collection)
        })?;

        Ok(collection)
    }

    fn list_by_workspace(&self, workspace_id: WorkspaceId) -> RepoResult<Vec<Collection>> {
        self.db.block_on(async {
            let rows = sqlx::query(
                "SELECT id, workspace_id, name, sort_order, created_at, updated_at, revision
                 FROM collections
                 WHERE workspace_id = ?
                 ORDER BY sort_order ASC, id ASC",
            )
            .bind(workspace_id.to_string())
            .fetch_all(self.db.pool())
            .await
            .context("failed to list collections")?;
            rows.into_iter().map(map_collection_row).collect()
        })
    }

    fn rename(&self, id: CollectionId, name: &str) -> RepoResult<()> {
        self.db.block_on(async {
            let updated_at = now_unix_ts();
            sqlx::query(
                "UPDATE collections
                 SET name = ?, updated_at = ?, revision = revision + 1
                 WHERE id = ?",
            )
            .bind(name)
            .bind(updated_at)
            .bind(id.to_string())
            .execute(self.db.pool())
            .await
            .context("failed to rename collection")?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn move_to_workspace(&self, id: CollectionId, workspace_id: WorkspaceId) -> RepoResult<()> {
        self.db.block_on(async {
            let mut tx = self.db.pool().begin().await?;
            let next_order: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM collections WHERE workspace_id = ?",
            )
            .bind(workspace_id.to_string())
            .fetch_one(&mut *tx)
            .await?;

            sqlx::query(
                "UPDATE collections
                 SET workspace_id = ?, sort_order = ?, updated_at = ?, revision = revision + 1
                 WHERE id = ?",
            )
            .bind(workspace_id.to_string())
            .bind(next_order)
            .bind(now_unix_ts())
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn reorder_in_workspace(
        &self,
        workspace_id: WorkspaceId,
        ordered_ids: &[CollectionId],
    ) -> RepoResult<()> {
        self.db.block_on(async {
            let mut tx = self.db.pool().begin().await?;
            let existing_rows = sqlx::query("SELECT id FROM collections WHERE workspace_id = ?")
                .bind(workspace_id.to_string())
                .fetch_all(&mut *tx)
                .await?;
            let mut existing_ids = existing_rows
                .iter()
                .map(|row| row.get::<String, _>("id"))
                .collect::<Vec<_>>();
            let mut incoming_ids = ordered_ids
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            existing_ids.sort();
            incoming_ids.sort();
            if existing_ids != incoming_ids {
                return Err(anyhow!("collection reorder set does not match workspace contents"));
            }

            for (index, id) in ordered_ids.iter().enumerate() {
                sqlx::query(
                    "UPDATE collections
                     SET sort_order = ?, updated_at = ?, revision = revision + 1
                     WHERE id = ? AND workspace_id = ?",
                )
                .bind(index as i64)
                .bind(now_unix_ts())
                .bind(id.to_string())
                .bind(workspace_id.to_string())
                .execute(&mut *tx)
                .await?;
            }
            tx.commit().await?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn delete(&self, id: CollectionId) -> RepoResult<()> {
        self.db.block_on(async {
            sqlx::query("DELETE FROM collections WHERE id = ?")
                .bind(id.to_string())
                .execute(self.db.pool())
                .await
                .context("failed to delete collection")?;
            Ok::<(), anyhow::Error>(())
        })
    }
}

fn map_collection_row(row: sqlx::sqlite::SqliteRow) -> RepoResult<Collection> {
    Ok(Collection {
        id: CollectionId::parse(row.get::<&str, _>("id"))?,
        workspace_id: WorkspaceId::parse(row.get::<&str, _>("workspace_id"))?,
        name: row.get("name"),
        sort_order: row.get("sort_order"),
        meta: RevisionMetadata {
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            revision: row.get("revision"),
        },
    })
}

pub type CollectionRepoRef = Arc<dyn CollectionRepository>;
