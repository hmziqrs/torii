use std::sync::Arc;

use anyhow::Context as _;
use sqlx::Row as _;

use crate::domain::{
    environment::Environment,
    ids::{EnvironmentId, WorkspaceId},
    revision::{RevisionMetadata, now_unix_ts},
};

use super::{DbRef, RepoResult};

pub trait EnvironmentRepository: Send + Sync {
    fn create(&self, workspace_id: WorkspaceId, name: &str) -> RepoResult<Environment>;
    fn list_by_workspace(&self, workspace_id: WorkspaceId) -> RepoResult<Vec<Environment>>;
    fn update_variables(&self, id: EnvironmentId, variables_json: &str) -> RepoResult<()>;
    fn rename(&self, id: EnvironmentId, name: &str) -> RepoResult<()>;
    fn delete(&self, id: EnvironmentId) -> RepoResult<()>;
}

#[derive(Clone)]
pub struct SqliteEnvironmentRepository {
    db: DbRef,
}

impl SqliteEnvironmentRepository {
    pub fn new(db: DbRef) -> Self {
        Self { db }
    }
}

impl EnvironmentRepository for SqliteEnvironmentRepository {
    fn create(&self, workspace_id: WorkspaceId, name: &str) -> RepoResult<Environment> {
        let environment = Environment::new(workspace_id, name.to_string());
        self.db.block_on(async {
            sqlx::query(
                "INSERT INTO environments
                 (id, workspace_id, name, variables_json, created_at, updated_at, revision)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(environment.id.to_string())
            .bind(environment.workspace_id.to_string())
            .bind(&environment.name)
            .bind(&environment.variables_json)
            .bind(environment.meta.created_at)
            .bind(environment.meta.updated_at)
            .bind(environment.meta.revision)
            .execute(self.db.pool())
            .await
            .context("failed to insert environment")?;
            Ok::<(), anyhow::Error>(())
        })?;
        Ok(environment)
    }

    fn list_by_workspace(&self, workspace_id: WorkspaceId) -> RepoResult<Vec<Environment>> {
        self.db.block_on(async {
            let rows = sqlx::query(
                "SELECT id, workspace_id, name, variables_json, created_at, updated_at, revision
                 FROM environments
                 WHERE workspace_id = ?
                 ORDER BY created_at ASC, id ASC",
            )
            .bind(workspace_id.to_string())
            .fetch_all(self.db.pool())
            .await
            .context("failed to list environments")?;

            rows.into_iter().map(map_environment_row).collect()
        })
    }

    fn update_variables(&self, id: EnvironmentId, variables_json: &str) -> RepoResult<()> {
        self.db.block_on(async {
            sqlx::query(
                "UPDATE environments
                 SET variables_json = ?, updated_at = ?, revision = revision + 1
                 WHERE id = ?",
            )
            .bind(variables_json)
            .bind(now_unix_ts())
            .bind(id.to_string())
            .execute(self.db.pool())
            .await
            .context("failed to update environment variables")?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn rename(&self, id: EnvironmentId, name: &str) -> RepoResult<()> {
        self.db.block_on(async {
            sqlx::query(
                "UPDATE environments
                 SET name = ?, updated_at = ?, revision = revision + 1
                 WHERE id = ?",
            )
            .bind(name)
            .bind(now_unix_ts())
            .bind(id.to_string())
            .execute(self.db.pool())
            .await
            .context("failed to rename environment")?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn delete(&self, id: EnvironmentId) -> RepoResult<()> {
        self.db.block_on(async {
            sqlx::query("DELETE FROM environments WHERE id = ?")
                .bind(id.to_string())
                .execute(self.db.pool())
                .await
                .context("failed to delete environment")?;
            Ok::<(), anyhow::Error>(())
        })
    }
}

fn map_environment_row(row: sqlx::sqlite::SqliteRow) -> RepoResult<Environment> {
    Ok(Environment {
        id: EnvironmentId::parse(row.get::<&str, _>("id"))?,
        workspace_id: WorkspaceId::parse(row.get::<&str, _>("workspace_id"))?,
        name: row.get("name"),
        variables_json: row.get("variables_json"),
        meta: RevisionMetadata {
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            revision: row.get("revision"),
        },
    })
}

pub type EnvironmentRepoRef = Arc<dyn EnvironmentRepository>;
