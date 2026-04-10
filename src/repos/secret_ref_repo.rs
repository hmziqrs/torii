use std::sync::Arc;

use anyhow::Context as _;
use sqlx::Row as _;

use crate::domain::{ids::SecretRefId, revision::now_unix_ts, secret_ref::SecretRef};

use super::{DbRef, RepoResult};

pub trait SecretRefRepository: Send + Sync {
    fn upsert(
        &self,
        owner_kind: &str,
        owner_id: &str,
        secret_kind: &str,
        provider: &str,
        namespace: &str,
        key_name: &str,
    ) -> RepoResult<SecretRef>;
    fn get(
        &self,
        owner_kind: &str,
        owner_id: &str,
        secret_kind: &str,
    ) -> RepoResult<Option<SecretRef>>;
    fn delete(&self, owner_kind: &str, owner_id: &str, secret_kind: &str) -> RepoResult<()>;
}

#[derive(Clone)]
pub struct SqliteSecretRefRepository {
    db: DbRef,
}

impl SqliteSecretRefRepository {
    pub fn new(db: DbRef) -> Self {
        Self { db }
    }
}

impl SecretRefRepository for SqliteSecretRefRepository {
    fn upsert(
        &self,
        owner_kind: &str,
        owner_id: &str,
        secret_kind: &str,
        provider: &str,
        namespace: &str,
        key_name: &str,
    ) -> RepoResult<SecretRef> {
        self.db.block_on(async {
            let now = now_unix_ts();
            let id = SecretRefId::new();
            sqlx::query(
                "INSERT INTO secret_refs
                 (id, owner_kind, owner_id, secret_kind, provider, namespace, key_name, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(owner_kind, owner_id, secret_kind)
                 DO UPDATE SET
                   provider = excluded.provider,
                   namespace = excluded.namespace,
                   key_name = excluded.key_name,
                   updated_at = excluded.updated_at",
            )
            .bind(id.to_string())
            .bind(owner_kind)
            .bind(owner_id)
            .bind(secret_kind)
            .bind(provider)
            .bind(namespace)
            .bind(key_name)
            .bind(now)
            .bind(now)
            .execute(self.db.pool())
            .await
            .context("failed to upsert secret ref")?;

            let row = sqlx::query(
                "SELECT id, owner_kind, owner_id, secret_kind, provider, namespace, key_name, created_at, updated_at
                 FROM secret_refs
                 WHERE owner_kind = ? AND owner_id = ? AND secret_kind = ?",
            )
            .bind(owner_kind)
            .bind(owner_id)
            .bind(secret_kind)
            .fetch_one(self.db.pool())
            .await
            .context("failed to reload secret ref")?;

            map_secret_ref_row(row)
        })
    }

    fn get(
        &self,
        owner_kind: &str,
        owner_id: &str,
        secret_kind: &str,
    ) -> RepoResult<Option<SecretRef>> {
        self.db.block_on(async {
            let row = sqlx::query(
                "SELECT id, owner_kind, owner_id, secret_kind, provider, namespace, key_name, created_at, updated_at
                 FROM secret_refs
                 WHERE owner_kind = ? AND owner_id = ? AND secret_kind = ?",
            )
            .bind(owner_kind)
            .bind(owner_id)
            .bind(secret_kind)
            .fetch_optional(self.db.pool())
            .await
            .context("failed to fetch secret ref")?;

            row.map(map_secret_ref_row).transpose()
        })
    }

    fn delete(&self, owner_kind: &str, owner_id: &str, secret_kind: &str) -> RepoResult<()> {
        self.db.block_on(async {
            sqlx::query(
                "DELETE FROM secret_refs WHERE owner_kind = ? AND owner_id = ? AND secret_kind = ?",
            )
            .bind(owner_kind)
            .bind(owner_id)
            .bind(secret_kind)
            .execute(self.db.pool())
            .await
            .context("failed to delete secret ref")?;
            Ok::<(), anyhow::Error>(())
        })
    }
}

fn map_secret_ref_row(row: sqlx::sqlite::SqliteRow) -> RepoResult<SecretRef> {
    Ok(SecretRef {
        id: SecretRefId::parse(row.get::<&str, _>("id"))?,
        owner_kind: row.get("owner_kind"),
        owner_id: row.get("owner_id"),
        secret_kind: row.get("secret_kind"),
        provider: row.get("provider"),
        namespace: row.get("namespace"),
        key_name: row.get("key_name"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

pub type SecretRefRepoRef = Arc<dyn SecretRefRepository>;
