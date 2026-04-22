use std::sync::Arc;

use anyhow::Context as _;
use sqlx::Row as _;

use crate::domain::{
    ids::WorkspaceId,
    revision::{RevisionMetadata, now_unix_ts},
    workspace::Workspace,
};

use super::{DbRef, RepoResult};

pub trait WorkspaceRepository: Send + Sync {
    fn create(&self, name: &str) -> RepoResult<Workspace>;
    fn get(&self, id: WorkspaceId) -> RepoResult<Option<Workspace>>;
    fn list(&self) -> RepoResult<Vec<Workspace>>;
    fn update_variables(&self, id: WorkspaceId, variables_json: &str) -> RepoResult<()>;
    fn rename(&self, id: WorkspaceId, name: &str) -> RepoResult<()>;
    fn delete(&self, id: WorkspaceId) -> RepoResult<()>;
}

#[derive(Clone)]
pub struct SqliteWorkspaceRepository {
    db: DbRef,
}

impl SqliteWorkspaceRepository {
    pub fn new(db: DbRef) -> Self {
        Self { db }
    }
}

impl WorkspaceRepository for SqliteWorkspaceRepository {
    fn create(&self, name: &str) -> RepoResult<Workspace> {
        let workspace = Workspace::new(name.to_string());
        let id = workspace.id.to_string();
        self.db.block_on(async {
            sqlx::query(
                "INSERT INTO workspaces (id, name, variables_json, created_at, updated_at, revision)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&workspace.name)
            .bind(&workspace.variables_json)
            .bind(workspace.meta.created_at)
            .bind(workspace.meta.updated_at)
            .bind(workspace.meta.revision)
            .execute(self.db.pool())
            .await
            .context("failed to insert workspace")?;
            Ok::<(), anyhow::Error>(())
        })?;
        Ok(workspace)
    }

    fn get(&self, id: WorkspaceId) -> RepoResult<Option<Workspace>> {
        self.db.block_on(async {
            let row = sqlx::query(
                "SELECT id, name, variables_json, created_at, updated_at, revision
                 FROM workspaces WHERE id = ?",
            )
            .bind(id.to_string())
            .fetch_optional(self.db.pool())
            .await
            .context("failed to fetch workspace")?;

            row.map(map_workspace_row).transpose()
        })
    }

    fn list(&self) -> RepoResult<Vec<Workspace>> {
        self.db.block_on(async {
            let rows = sqlx::query(
                "SELECT id, name, variables_json, created_at, updated_at, revision
                 FROM workspaces
                 ORDER BY created_at ASC, id ASC",
            )
            .fetch_all(self.db.pool())
            .await
            .context("failed to list workspaces")?;

            rows.into_iter().map(map_workspace_row).collect()
        })
    }

    fn update_variables(&self, id: WorkspaceId, variables_json: &str) -> RepoResult<()> {
        let normalized = normalize_variables_json(variables_json);
        self.db.block_on(async {
            sqlx::query(
                "UPDATE workspaces
                 SET variables_json = ?, updated_at = ?, revision = revision + 1
                 WHERE id = ?",
            )
            .bind(normalized)
            .bind(now_unix_ts())
            .bind(id.to_string())
            .execute(self.db.pool())
            .await
            .context("failed to update workspace variables")?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn rename(&self, id: WorkspaceId, name: &str) -> RepoResult<()> {
        self.db.block_on(async {
            sqlx::query(
                "UPDATE workspaces
                 SET name = ?, updated_at = ?, revision = revision + 1
                 WHERE id = ?",
            )
            .bind(name)
            .bind(now_unix_ts())
            .bind(id.to_string())
            .execute(self.db.pool())
            .await
            .context("failed to rename workspace")?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn delete(&self, id: WorkspaceId) -> RepoResult<()> {
        self.db.block_on(async {
            sqlx::query("DELETE FROM workspaces WHERE id = ?")
                .bind(id.to_string())
                .execute(self.db.pool())
                .await
                .context("failed to delete workspace")?;
            Ok::<(), anyhow::Error>(())
        })
    }
}

fn map_workspace_row(row: sqlx::sqlite::SqliteRow) -> RepoResult<Workspace> {
    let raw_variables_json: String = row
        .try_get("variables_json")
        .unwrap_or_else(|_| "[]".to_string());
    Ok(Workspace {
        id: WorkspaceId::parse(row.get::<&str, _>("id"))?,
        name: row.get("name"),
        variables_json: normalize_variables_json(&raw_variables_json),
        meta: RevisionMetadata {
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            revision: row.get("revision"),
        },
    })
}

pub type WorkspaceRepoRef = Arc<dyn WorkspaceRepository>;

fn normalize_variables_json(variables_json: &str) -> String {
    let parsed = serde_json::from_str::<serde_json::Value>(variables_json)
        .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));

    match parsed {
        serde_json::Value::Array(_) => variables_json.to_string(),
        serde_json::Value::Object(map) => {
            let rows = map
                .into_iter()
                .map(|(key, value)| {
                    let plain_value = match value {
                        serde_json::Value::String(s) => s,
                        other => other.to_string(),
                    };
                    serde_json::json!({
                        "key": key,
                        "enabled": true,
                        "value": {
                            "Plain": { "value": plain_value }
                        }
                    })
                })
                .collect::<Vec<_>>();
            serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string())
        }
        _ => "[]".to_string(),
    }
}
