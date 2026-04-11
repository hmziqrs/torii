use std::sync::Arc;

use anyhow::{Context as _, anyhow};
use sqlx::Row as _;

use crate::domain::{
    folder::Folder,
    ids::{CollectionId, FolderId},
    revision::{RevisionMetadata, now_unix_ts},
};

use super::{DbRef, RepoResult};

pub trait FolderRepository: Send + Sync {
    fn create(
        &self,
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
        name: &str,
    ) -> RepoResult<Folder>;
    fn get(&self, id: FolderId) -> RepoResult<Option<Folder>>;
    fn list_by_collection(&self, collection_id: CollectionId) -> RepoResult<Vec<Folder>>;
    fn rename(&self, id: FolderId, name: &str) -> RepoResult<()>;
    fn move_to(
        &self,
        id: FolderId,
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
    ) -> RepoResult<()>;
    fn reorder_in_parent(
        &self,
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
        ordered_ids: &[FolderId],
    ) -> RepoResult<()>;
    fn delete(&self, id: FolderId) -> RepoResult<()>;
}

#[derive(Clone)]
pub struct SqliteFolderRepository {
    db: DbRef,
}

impl SqliteFolderRepository {
    pub fn new(db: DbRef) -> Self {
        Self { db }
    }
}

impl FolderRepository for SqliteFolderRepository {
    fn create(
        &self,
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
        name: &str,
    ) -> RepoResult<Folder> {
        self.db.block_on(async {
            let mut tx = self.db.pool().begin().await?;

            if let Some(parent) = parent_folder_id {
                let parent_exists: Option<i64> = sqlx::query_scalar(
                    "SELECT 1 FROM folders WHERE id = ? AND collection_id = ?",
                )
                .bind(parent.to_string())
                .bind(collection_id.to_string())
                .fetch_optional(&mut *tx)
                .await?;
                if parent_exists.is_none() {
                    return Err(anyhow!("parent folder does not exist in target collection"));
                }
            }

            let next_sort: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(sort_order), -1) + 1
                 FROM folders
                 WHERE collection_id = ? AND parent_folder_id IS ?",
            )
            .bind(collection_id.to_string())
            .bind(parent_folder_id.map(|it| it.to_string()))
            .fetch_one(&mut *tx)
            .await?;

            let folder = Folder::new(collection_id, parent_folder_id, name.to_string(), next_sort);
            sqlx::query(
                "INSERT INTO folders (id, collection_id, parent_folder_id, name, sort_order, created_at, updated_at, revision)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(folder.id.to_string())
            .bind(folder.collection_id.to_string())
            .bind(folder.parent_folder_id.map(|it| it.to_string()))
            .bind(&folder.name)
            .bind(folder.sort_order)
            .bind(folder.meta.created_at)
            .bind(folder.meta.updated_at)
            .bind(folder.meta.revision)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            Ok::<Folder, anyhow::Error>(folder)
        })
    }

    fn get(&self, id: FolderId) -> RepoResult<Option<Folder>> {
        self.db.block_on(async {
            let row = sqlx::query(
                "SELECT id, collection_id, parent_folder_id, name, sort_order, created_at, updated_at, revision
                 FROM folders WHERE id = ?",
            )
            .bind(id.to_string())
            .fetch_optional(self.db.pool())
            .await
            .context("failed to fetch folder")?;
            row.map(map_folder_row).transpose()
        })
    }

    fn list_by_collection(&self, collection_id: CollectionId) -> RepoResult<Vec<Folder>> {
        self.db.block_on(async {
            let rows = sqlx::query(
                "SELECT id, collection_id, parent_folder_id, name, sort_order, created_at, updated_at, revision
                 FROM folders
                 WHERE collection_id = ?
                 ORDER BY parent_folder_id ASC, sort_order ASC, id ASC",
            )
            .bind(collection_id.to_string())
            .fetch_all(self.db.pool())
            .await
            .context("failed to list folders")?;
            rows.into_iter().map(map_folder_row).collect()
        })
    }

    fn rename(&self, id: FolderId, name: &str) -> RepoResult<()> {
        self.db.block_on(async {
            sqlx::query(
                "UPDATE folders
                 SET name = ?, updated_at = ?, revision = revision + 1
                 WHERE id = ?",
            )
            .bind(name)
            .bind(now_unix_ts())
            .bind(id.to_string())
            .execute(self.db.pool())
            .await
            .context("failed to rename folder")?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn move_to(
        &self,
        id: FolderId,
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
    ) -> RepoResult<()> {
        self.db.block_on(async {
            let mut tx = self.db.pool().begin().await?;
            let source_row = sqlx::query(
                "SELECT collection_id, parent_folder_id FROM folders WHERE id = ?",
            )
            .bind(id.to_string())
            .fetch_optional(&mut *tx)
            .await?;
            let Some(source_row) = source_row else {
                return Err(anyhow!("folder does not exist"));
            };
            let source_collection_id =
                CollectionId::parse(source_row.get::<&str, _>("collection_id"))?;
            let source_parent_folder_id = source_row
                .get::<Option<String>, _>("parent_folder_id")
                .map(|value| FolderId::parse(&value))
                .transpose()?;

            if let Some(parent) = parent_folder_id {
                let parent_exists: Option<i64> = sqlx::query_scalar(
                    "SELECT 1 FROM folders WHERE id = ? AND collection_id = ?",
                )
                .bind(parent.to_string())
                .bind(collection_id.to_string())
                .fetch_optional(&mut *tx)
                .await?;
                if parent_exists.is_none() {
                    return Err(anyhow!("target parent folder does not exist"));
                }
            }

            let next_sort: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(sort_order), -1) + 1
                 FROM folders
                 WHERE collection_id = ? AND parent_folder_id IS ?",
            )
            .bind(collection_id.to_string())
            .bind(parent_folder_id.map(|it| it.to_string()))
            .fetch_one(&mut *tx)
            .await?;

            let ts = now_unix_ts();
            sqlx::query(
                "UPDATE folders
                 SET collection_id = ?, parent_folder_id = ?, sort_order = ?, updated_at = ?, revision = revision + 1
                 WHERE id = ?",
            )
            .bind(collection_id.to_string())
            .bind(parent_folder_id.map(|it| it.to_string()))
            .bind(next_sort)
            .bind(ts)
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "WITH RECURSIVE descendants(id) AS (
                    SELECT id FROM folders WHERE id = ?
                    UNION ALL
                    SELECT f.id FROM folders f
                    JOIN descendants d ON f.parent_folder_id = d.id
                 )
                 UPDATE folders
                 SET collection_id = ?, updated_at = ?, revision = revision + 1
                 WHERE id IN descendants",
            )
            .bind(id.to_string())
            .bind(collection_id.to_string())
            .bind(ts)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "WITH RECURSIVE descendants(id) AS (
                    SELECT id FROM folders WHERE id = ?
                    UNION ALL
                    SELECT f.id FROM folders f
                    JOIN descendants d ON f.parent_folder_id = d.id
                 )
                 UPDATE requests
                 SET collection_id = ?, updated_at = ?, revision = revision + 1
                 WHERE parent_folder_id IN descendants",
            )
            .bind(id.to_string())
            .bind(collection_id.to_string())
            .bind(ts)
            .execute(&mut *tx)
            .await?;

            normalize_folder_sort_orders(&mut tx, source_collection_id, source_parent_folder_id)
                .await?;
            normalize_folder_sort_orders(&mut tx, collection_id, parent_folder_id).await?;

            tx.commit().await?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn reorder_in_parent(
        &self,
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
        ordered_ids: &[FolderId],
    ) -> RepoResult<()> {
        self.db.block_on(async {
            let mut tx = self.db.pool().begin().await?;
            let sibling_rows = sqlx::query(
                "SELECT id FROM folders
                 WHERE collection_id = ? AND parent_folder_id IS ?
                 ORDER BY sort_order ASC, id ASC",
            )
            .bind(collection_id.to_string())
            .bind(parent_folder_id.map(|it| it.to_string()))
            .fetch_all(&mut *tx)
            .await?;

            let mut existing = sibling_rows
                .iter()
                .map(|row| row.get::<String, _>("id"))
                .collect::<Vec<_>>();
            let mut incoming = ordered_ids
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            existing.sort();
            incoming.sort();
            if existing != incoming {
                return Err(anyhow!("folder reorder set does not match siblings"));
            }

            for (index, id) in ordered_ids.iter().enumerate() {
                sqlx::query(
                    "UPDATE folders
                     SET sort_order = ?, updated_at = ?, revision = revision + 1
                     WHERE id = ?",
                )
                .bind(index as i64)
                .bind(now_unix_ts())
                .bind(id.to_string())
                .execute(&mut *tx)
                .await?;
            }

            tx.commit().await?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn delete(&self, id: FolderId) -> RepoResult<()> {
        self.db.block_on(async {
            let mut tx = self.db.pool().begin().await?;
            let row =
                sqlx::query("SELECT collection_id, parent_folder_id FROM folders WHERE id = ?")
                    .bind(id.to_string())
                    .fetch_optional(&mut *tx)
                    .await?;
            let Some(row) = row else {
                return Ok::<(), anyhow::Error>(());
            };
            let collection_id = CollectionId::parse(row.get::<&str, _>("collection_id"))?;
            let parent_folder_id = row
                .get::<Option<String>, _>("parent_folder_id")
                .map(|value| FolderId::parse(&value))
                .transpose()?;

            sqlx::query("DELETE FROM folders WHERE id = ?")
                .bind(id.to_string())
                .execute(&mut *tx)
                .await
                .context("failed to delete folder")?;
            normalize_folder_sort_orders(&mut tx, collection_id, parent_folder_id).await?;
            tx.commit().await?;
            Ok::<(), anyhow::Error>(())
        })
    }
}

async fn normalize_folder_sort_orders(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    collection_id: CollectionId,
    parent_folder_id: Option<FolderId>,
) -> RepoResult<()> {
    let rows = sqlx::query(
        "SELECT id FROM folders
         WHERE collection_id = ? AND parent_folder_id IS ?
         ORDER BY sort_order ASC, id ASC",
    )
    .bind(collection_id.to_string())
    .bind(parent_folder_id.map(|it| it.to_string()))
    .fetch_all(&mut **tx)
    .await?;

    let updated_at = now_unix_ts();
    for (index, row) in rows.iter().enumerate() {
        let id: String = row.get("id");
        sqlx::query(
            "UPDATE folders
             SET sort_order = ?, updated_at = ?, revision = revision + 1
             WHERE id = ?",
        )
        .bind(index as i64)
        .bind(updated_at)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

fn map_folder_row(row: sqlx::sqlite::SqliteRow) -> RepoResult<Folder> {
    let parent_folder_id = row
        .get::<Option<String>, _>("parent_folder_id")
        .map(|value| FolderId::parse(&value))
        .transpose()?;

    Ok(Folder {
        id: FolderId::parse(row.get::<&str, _>("id"))?,
        collection_id: CollectionId::parse(row.get::<&str, _>("collection_id"))?,
        parent_folder_id,
        name: row.get("name"),
        sort_order: row.get("sort_order"),
        meta: RevisionMetadata {
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            revision: row.get("revision"),
        },
    })
}

pub type FolderRepoRef = Arc<dyn FolderRepository>;
