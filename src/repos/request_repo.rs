use std::sync::Arc;

use anyhow::{Context as _, anyhow};
use sqlx::Row as _;

use crate::domain::{
    ids::{CollectionId, FolderId, RequestId},
    request::RequestItem,
    revision::{RevisionMetadata, now_unix_ts},
};

use super::{DbRef, RepoResult};

// ---------------------------------------------------------------------------
// Explicit repo errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum RequestRepoError {
    #[error("request not found: {0}")]
    NotFound(RequestId),
    #[error("revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: i64, actual: i64 },
    #[error("storage failure: {0}")]
    Storage(#[source] anyhow::Error),
}

impl From<anyhow::Error> for RequestRepoError {
    fn from(err: anyhow::Error) -> Self {
        Self::Storage(err)
    }
}

pub trait RequestRepository: Send + Sync {
    fn create(
        &self,
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
        name: &str,
        method: &str,
        url: &str,
    ) -> RepoResult<RequestItem>;
    fn get(&self, id: RequestId) -> RepoResult<Option<RequestItem>>;
    fn list_by_collection(&self, collection_id: CollectionId) -> RepoResult<Vec<RequestItem>>;
    fn rename(&self, id: RequestId, name: &str) -> RepoResult<()>;
    fn move_to(
        &self,
        id: RequestId,
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
    ) -> RepoResult<()>;
    fn reorder_in_parent(
        &self,
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
        ordered_ids: &[RequestId],
    ) -> RepoResult<()>;
    fn delete(&self, id: RequestId) -> RepoResult<()>;

    // Phase 3 additions
    fn save(&self, request: &RequestItem, expected_revision: i64) -> Result<(), RequestRepoError>;
    fn duplicate(&self, source_id: RequestId, new_name: &str) -> RepoResult<RequestItem>;
}

#[derive(Clone)]
pub struct SqliteRequestRepository {
    db: DbRef,
}

impl SqliteRequestRepository {
    pub fn new(db: DbRef) -> Self {
        Self { db }
    }
}

impl RequestRepository for SqliteRequestRepository {
    fn create(
        &self,
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
        name: &str,
        method: &str,
        url: &str,
    ) -> RepoResult<RequestItem> {
        self.db.block_on(async {
            let mut tx = self.db.pool().begin().await?;
            if let Some(parent) = parent_folder_id {
                let parent_exists: Option<i64> =
                    sqlx::query_scalar("SELECT 1 FROM folders WHERE id = ? AND collection_id = ?")
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
                 FROM requests
                 WHERE collection_id = ? AND parent_folder_id IS ?",
            )
            .bind(collection_id.to_string())
            .bind(parent_folder_id.map(|it| it.to_string()))
            .fetch_one(&mut *tx)
            .await?;

            let request = RequestItem::new(
                collection_id,
                parent_folder_id,
                name,
                method,
                url,
                next_sort,
            );
            insert_request(&mut tx, &request).await?;
            tx.commit().await?;
            Ok::<RequestItem, anyhow::Error>(request)
        })
    }

    fn get(&self, id: RequestId) -> RepoResult<Option<RequestItem>> {
        self.db.block_on(async {
            let row = sqlx::query(
                "SELECT id, collection_id, parent_folder_id, name, method, url, body_blob_hash,
                        sort_order, created_at, updated_at, revision,
                        params_json, headers_json, auth_json, body_json, scripts_json, settings_json
                 FROM requests WHERE id = ?",
            )
            .bind(id.to_string())
            .fetch_optional(self.db.pool())
            .await
            .context("failed to fetch request")?;
            row.map(map_request_row).transpose()
        })
    }

    fn list_by_collection(&self, collection_id: CollectionId) -> RepoResult<Vec<RequestItem>> {
        self.db.block_on(async {
            let rows = sqlx::query(
                "SELECT id, collection_id, parent_folder_id, name, method, url, body_blob_hash,
                        sort_order, created_at, updated_at, revision,
                        params_json, headers_json, auth_json, body_json, scripts_json, settings_json
                 FROM requests
                 WHERE collection_id = ?
                 ORDER BY parent_folder_id ASC, sort_order ASC, id ASC",
            )
            .bind(collection_id.to_string())
            .fetch_all(self.db.pool())
            .await
            .context("failed to list requests")?;
            rows.into_iter().map(map_request_row).collect()
        })
    }

    fn rename(&self, id: RequestId, name: &str) -> RepoResult<()> {
        self.db.block_on(async {
            sqlx::query(
                "UPDATE requests
                 SET name = ?, updated_at = ?, revision = revision + 1
                 WHERE id = ?",
            )
            .bind(name)
            .bind(now_unix_ts())
            .bind(id.to_string())
            .execute(self.db.pool())
            .await
            .context("failed to rename request")?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn move_to(
        &self,
        id: RequestId,
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
    ) -> RepoResult<()> {
        self.db.block_on(async {
            let mut tx = self.db.pool().begin().await?;
            let source_row = sqlx::query(
                "SELECT collection_id, parent_folder_id FROM requests WHERE id = ?",
            )
            .bind(id.to_string())
            .fetch_optional(&mut *tx)
            .await?;
            let Some(source_row) = source_row else {
                return Err(anyhow!("request does not exist"));
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
                 FROM requests
                 WHERE collection_id = ? AND parent_folder_id IS ?",
            )
            .bind(collection_id.to_string())
            .bind(parent_folder_id.map(|it| it.to_string()))
            .fetch_one(&mut *tx)
            .await?;

            sqlx::query(
                "UPDATE requests
                 SET collection_id = ?, parent_folder_id = ?, sort_order = ?, updated_at = ?, revision = revision + 1
                 WHERE id = ?",
            )
            .bind(collection_id.to_string())
            .bind(parent_folder_id.map(|it| it.to_string()))
            .bind(next_sort)
            .bind(now_unix_ts())
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;

            normalize_request_sort_orders(&mut tx, source_collection_id, source_parent_folder_id)
                .await?;
            normalize_request_sort_orders(&mut tx, collection_id, parent_folder_id).await?;

            tx.commit().await?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn reorder_in_parent(
        &self,
        collection_id: CollectionId,
        parent_folder_id: Option<FolderId>,
        ordered_ids: &[RequestId],
    ) -> RepoResult<()> {
        self.db.block_on(async {
            let mut tx = self.db.pool().begin().await?;
            let sibling_rows = sqlx::query(
                "SELECT id FROM requests
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
                return Err(anyhow!("request reorder set does not match siblings"));
            }

            for (index, id) in ordered_ids.iter().enumerate() {
                sqlx::query(
                    "UPDATE requests
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

    fn delete(&self, id: RequestId) -> RepoResult<()> {
        self.db.block_on(async {
            let mut tx = self.db.pool().begin().await?;
            let row =
                sqlx::query("SELECT collection_id, parent_folder_id FROM requests WHERE id = ?")
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

            sqlx::query("DELETE FROM requests WHERE id = ?")
                .bind(id.to_string())
                .execute(&mut *tx)
                .await
                .context("failed to delete request")?;
            normalize_request_sort_orders(&mut tx, collection_id, parent_folder_id).await?;
            tx.commit().await?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn save(&self, request: &RequestItem, expected_revision: i64) -> Result<(), RequestRepoError> {
        self.db.block_on(async {
            let storage = |e: sqlx::Error| RequestRepoError::Storage(anyhow::Error::new(e));
            let mut tx = self.db.pool().begin().await.map_err(storage)?;

            let current_revision: Option<i64> =
                sqlx::query_scalar("SELECT revision FROM requests WHERE id = ?")
                    .bind(request.id.to_string())
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(storage)?;

            let Some(current_revision) = current_revision else {
                return Err(RequestRepoError::NotFound(request.id));
            };

            if current_revision != expected_revision {
                return Err(RequestRepoError::RevisionConflict {
                    expected: expected_revision,
                    actual: current_revision,
                });
            }

            let ts = now_unix_ts();
            let params_json = serde_json::to_string(&request.params)
                .map_err(|e| RequestRepoError::Storage(e.into()))?;
            let headers_json = serde_json::to_string(&request.headers)
                .map_err(|e| RequestRepoError::Storage(e.into()))?;
            let auth_json = serde_json::to_string(&request.auth)
                .map_err(|e| RequestRepoError::Storage(e.into()))?;
            let body_json = serde_json::to_string(&request.body)
                .map_err(|e| RequestRepoError::Storage(e.into()))?;
            let scripts_json = serde_json::to_string(&request.scripts)
                .map_err(|e| RequestRepoError::Storage(e.into()))?;
            let settings_json = serde_json::to_string(&request.settings)
                .map_err(|e| RequestRepoError::Storage(e.into()))?;

            sqlx::query(
                "UPDATE requests
                 SET name = ?, method = ?, url = ?, body_blob_hash = ?,
                     params_json = ?, headers_json = ?, auth_json = ?,
                     body_json = ?, scripts_json = ?, settings_json = ?,
                     updated_at = ?, revision = revision + 1
                 WHERE id = ? AND revision = ?",
            )
            .bind(&request.name)
            .bind(&request.method)
            .bind(&request.url)
            .bind(request.body_blob_hash.clone())
            .bind(params_json)
            .bind(headers_json)
            .bind(auth_json)
            .bind(body_json)
            .bind(scripts_json)
            .bind(settings_json)
            .bind(ts)
            .bind(request.id.to_string())
            .bind(expected_revision)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;

            tx.commit().await.map_err(storage)?;
            Ok(())
        })
    }

    fn duplicate(&self, source_id: RequestId, new_name: &str) -> RepoResult<RequestItem> {
        self.db.block_on(async {
            let mut tx = self.db.pool().begin().await?;

            let row = sqlx::query(
                "SELECT id, collection_id, parent_folder_id, name, method, url, body_blob_hash,
                        sort_order, created_at, updated_at, revision,
                        params_json, headers_json, auth_json, body_json, scripts_json, settings_json
                 FROM requests WHERE id = ?",
            )
            .bind(source_id.to_string())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| anyhow!("source request not found for duplicate"))?;

            let source = map_request_row(row)?;

            let next_sort: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(sort_order), -1) + 1
                 FROM requests
                 WHERE collection_id = ? AND parent_folder_id IS ?",
            )
            .bind(source.collection_id.to_string())
            .bind(source.parent_folder_id.map(|it| it.to_string()))
            .fetch_one(&mut *tx)
            .await?;

            let mut dup = RequestItem::new(
                source.collection_id,
                source.parent_folder_id,
                new_name,
                &source.method,
                &source.url,
                next_sort,
            );
            dup.params = source.params;
            dup.headers = source.headers;
            dup.auth = source.auth;
            dup.body = source.body;
            dup.scripts = source.scripts;
            dup.settings = source.settings;
            dup.body_blob_hash = source.body_blob_hash;

            insert_request(&mut tx, &dup).await?;
            tx.commit().await?;
            Ok::<RequestItem, anyhow::Error>(dup)
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn insert_request(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    request: &RequestItem,
) -> RepoResult<()> {
    let params_json = serde_json::to_string(&request.params)?;
    let headers_json = serde_json::to_string(&request.headers)?;
    let auth_json = serde_json::to_string(&request.auth)?;
    let body_json = serde_json::to_string(&request.body)?;
    let scripts_json = serde_json::to_string(&request.scripts)?;
    let settings_json = serde_json::to_string(&request.settings)?;

    sqlx::query(
        "INSERT INTO requests
         (id, collection_id, parent_folder_id, name, method, url, body_blob_hash,
          sort_order, created_at, updated_at, revision,
          params_json, headers_json, auth_json, body_json, scripts_json, settings_json)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(request.id.to_string())
    .bind(request.collection_id.to_string())
    .bind(request.parent_folder_id.map(|it| it.to_string()))
    .bind(&request.name)
    .bind(&request.method)
    .bind(&request.url)
    .bind(request.body_blob_hash.clone())
    .bind(request.sort_order)
    .bind(request.meta.created_at)
    .bind(request.meta.updated_at)
    .bind(request.meta.revision)
    .bind(params_json)
    .bind(headers_json)
    .bind(auth_json)
    .bind(body_json)
    .bind(scripts_json)
    .bind(settings_json)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn normalize_request_sort_orders(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    collection_id: CollectionId,
    parent_folder_id: Option<FolderId>,
) -> RepoResult<()> {
    let rows = sqlx::query(
        "SELECT id FROM requests
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
            "UPDATE requests
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

fn map_request_row(row: sqlx::sqlite::SqliteRow) -> RepoResult<RequestItem> {
    let parent_folder_id = row
        .get::<Option<String>, _>("parent_folder_id")
        .map(|value| FolderId::parse(&value))
        .transpose()?;

    let params_json: String = row
        .try_get("params_json")
        .unwrap_or_else(|_| "{}".to_string());
    let headers_json: String = row
        .try_get("headers_json")
        .unwrap_or_else(|_| "[]".to_string());
    let auth_json: String = row
        .try_get("auth_json")
        .unwrap_or_else(|_| r#"{"type":"none"}"#.to_string());
    let body_json: String = row
        .try_get("body_json")
        .unwrap_or_else(|_| r#"{"type":"none"}"#.to_string());
    let scripts_json: String = row
        .try_get("scripts_json")
        .unwrap_or_else(|_| r#"{"pre_request":"","tests":""}"#.to_string());
    let settings_json: String = row
        .try_get("settings_json")
        .unwrap_or_else(|_| "{}".to_string());

    Ok(RequestItem {
        id: RequestId::parse(row.get::<&str, _>("id"))?,
        collection_id: CollectionId::parse(row.get::<&str, _>("collection_id"))?,
        parent_folder_id,
        name: row.get("name"),
        method: row.get("method"),
        url: row.get("url"),
        body_blob_hash: row.get("body_blob_hash"),
        sort_order: row.get("sort_order"),
        meta: RevisionMetadata {
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            revision: row.get("revision"),
        },
        params: serde_json::from_str(&params_json).unwrap_or_default(),
        headers: serde_json::from_str(&headers_json).unwrap_or_default(),
        auth: serde_json::from_str(&auth_json).unwrap_or_default(),
        body: serde_json::from_str(&body_json).unwrap_or_default(),
        scripts: serde_json::from_str(&scripts_json).unwrap_or_default(),
        settings: serde_json::from_str(&settings_json).unwrap_or_default(),
    })
}

pub type RequestRepoRef = Arc<dyn RequestRepository>;
