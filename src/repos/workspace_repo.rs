use std::sync::Arc;

use anyhow::Context as _;
use sqlx::Row as _;

use crate::domain::{ids::WorkspaceId, revision::RevisionMetadata, workspace::Workspace};

use super::{DbRef, RepoResult};

pub trait WorkspaceRepository: Send + Sync {
    fn create(&self, name: &str) -> RepoResult<Workspace>;
    fn get(&self, id: WorkspaceId) -> RepoResult<Option<Workspace>>;
    fn list(&self) -> RepoResult<Vec<Workspace>>;
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
                "INSERT INTO workspaces (id, name, created_at, updated_at, revision)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&workspace.name)
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
                "SELECT id, name, created_at, updated_at, revision
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
                "SELECT id, name, created_at, updated_at, revision
                 FROM workspaces
                 ORDER BY created_at ASC, id ASC",
            )
            .fetch_all(self.db.pool())
            .await
            .context("failed to list workspaces")?;

            rows.into_iter().map(map_workspace_row).collect()
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
    Ok(Workspace {
        id: WorkspaceId::parse(row.get::<&str, _>("id"))?,
        name: row.get("name"),
        meta: RevisionMetadata {
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            revision: row.get("revision"),
        },
    })
}

pub type WorkspaceRepoRef = Arc<dyn WorkspaceRepository>;
