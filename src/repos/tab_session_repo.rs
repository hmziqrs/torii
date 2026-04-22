use std::sync::Arc;

use anyhow::{Context as _, Result};
use sqlx::Row as _;

use crate::{
    domain::ids::{EnvironmentId, WorkspaceId},
    domain::revision::now_unix_ts,
    session::{
        item_key::{ItemKey, TabKey},
        tab_manager::TabState,
        window_layout::WindowLayoutState,
        workspace_session::SessionId,
    },
};

use super::{DbRef, RepoResult};

#[derive(Debug, Clone, PartialEq)]
pub struct TabSessionSnapshot {
    pub session_id: SessionId,
    pub tabs: Vec<TabState>,
    pub active: Option<TabKey>,
    pub updated_at: i64,
    pub metadata: TabSessionMetadata,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TabSessionMetadata {
    pub selected_workspace_id: Option<String>,
    pub sidebar_selection: Option<ItemKey>,
    pub window_layout: WindowLayoutState,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TabSessionWorkspaceState {
    pub workspace_id: WorkspaceId,
    pub active_environment_id: Option<EnvironmentId>,
    pub expanded_items_json: String,
    pub created_at: i64,
    pub updated_at: i64,
}

pub trait TabSessionRepository: Send + Sync {
    fn save_session(
        &self,
        session_id: SessionId,
        tabs: &[TabState],
        active: Option<TabKey>,
        metadata: &TabSessionMetadata,
    ) -> RepoResult<()>;
    fn load_session(&self, session_id: SessionId) -> RepoResult<Option<TabSessionSnapshot>>;
    fn load_most_recent(&self) -> RepoResult<Option<TabSessionSnapshot>>;
    fn list_sessions(&self) -> RepoResult<Vec<TabSessionSnapshot>>;
    fn save_workspace_states(
        &self,
        session_id: SessionId,
        states: &[TabSessionWorkspaceState],
    ) -> RepoResult<()>;
    fn load_workspace_states(
        &self,
        session_id: SessionId,
    ) -> RepoResult<Vec<TabSessionWorkspaceState>>;
    fn clear_session(&self, session_id: SessionId) -> RepoResult<()>;
    fn clear_all(&self) -> RepoResult<()>;
}

#[derive(Clone)]
pub struct SqliteTabSessionRepository {
    db: DbRef,
}

impl SqliteTabSessionRepository {
    pub fn new(db: DbRef) -> Self {
        Self { db }
    }
}

impl TabSessionRepository for SqliteTabSessionRepository {
    fn save_session(
        &self,
        session_id: SessionId,
        tabs: &[TabState],
        active: Option<TabKey>,
        metadata: &TabSessionMetadata,
    ) -> RepoResult<()> {
        self.db.block_on(async {
            let mut tx = self.db.pool().begin().await?;
            let ts = now_unix_ts();

            sqlx::query("DELETE FROM tab_session_state WHERE session_id = ?")
                .bind(session_id.0.to_string())
                .execute(&mut *tx)
                .await
                .context("failed to clear previous tab session rows")?;

            let (sidebar_kind, sidebar_id) = metadata
                .sidebar_selection
                .map(|item| item.to_storage_parts())
                .unwrap_or_else(|| (String::new(), None));
            sqlx::query(
                "INSERT INTO tab_session_metadata
                 (session_id, selected_workspace_id, sidebar_item_kind, sidebar_item_id, sidebar_collapsed, sidebar_width_px, created_at, updated_at, revision)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1)
                 ON CONFLICT(session_id) DO UPDATE SET
                    selected_workspace_id = excluded.selected_workspace_id,
                    sidebar_item_kind = excluded.sidebar_item_kind,
                    sidebar_item_id = excluded.sidebar_item_id,
                    sidebar_collapsed = excluded.sidebar_collapsed,
                    sidebar_width_px = excluded.sidebar_width_px,
                    updated_at = excluded.updated_at,
                    revision = tab_session_metadata.revision + 1",
            )
            .bind(session_id.0.to_string())
            .bind(metadata.selected_workspace_id.clone())
            .bind(if sidebar_kind.is_empty() { None::<String> } else { Some(sidebar_kind) })
            .bind(sidebar_id)
            .bind(metadata.window_layout.sidebar_collapsed)
            .bind(metadata.window_layout.sidebar_width_px as f64)
            .bind(metadata.created_at)
            .bind(ts)
            .execute(&mut *tx)
            .await
            .context("failed to upsert tab session metadata")?;

            for (index, tab) in tabs.iter().enumerate() {
                let (item_kind, item_id) = tab.key.item().to_storage_parts();
                sqlx::query(
                    "INSERT INTO tab_session_state
                     (session_id, tab_order, item_kind, item_id, pinned, is_active, created_at, updated_at, revision)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(session_id.0.to_string())
                .bind(index as i64)
                .bind(item_kind)
                .bind(item_id)
                .bind(tab.pinned)
                .bind(active == Some(tab.key))
                .bind(ts)
                .bind(ts)
                .bind(1_i64)
                .execute(&mut *tx)
                .await
                .context("failed to insert tab session row")?;
            }

            tx.commit().await?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn load_session(&self, session_id: SessionId) -> RepoResult<Option<TabSessionSnapshot>> {
        load_snapshot(&self.db, Some(session_id))
    }

    fn load_most_recent(&self) -> RepoResult<Option<TabSessionSnapshot>> {
        load_snapshot(&self.db, None)
    }

    fn list_sessions(&self) -> RepoResult<Vec<TabSessionSnapshot>> {
        let session_ids = self.db.block_on(async {
            let rows = sqlx::query(
                "SELECT session_id
                 FROM tab_session_metadata
                 ORDER BY updated_at DESC, session_id DESC",
            )
            .fetch_all(self.db.pool())
            .await
            .context("failed to list tab session metadata")?;

            Ok::<Vec<String>, anyhow::Error>(
                rows.into_iter()
                    .map(|row| row.get::<String, _>("session_id"))
                    .collect(),
            )
        })?;

        let mut snapshots = Vec::new();
        for session_id in session_ids {
            let session_id = SessionId(
                uuid::Uuid::parse_str(&session_id).context("invalid stored tab session id")?,
            );
            if let Some(snapshot) = load_snapshot(&self.db, Some(session_id))? {
                snapshots.push(snapshot);
            }
        }
        Ok(snapshots)
    }

    fn save_workspace_states(
        &self,
        session_id: SessionId,
        states: &[TabSessionWorkspaceState],
    ) -> RepoResult<()> {
        self.db.block_on(async {
            let mut tx = self.db.pool().begin().await?;
            let ts = now_unix_ts();
            sqlx::query("DELETE FROM tab_session_workspace_state WHERE session_id = ?")
                .bind(session_id.0.to_string())
                .execute(&mut *tx)
                .await
                .context("failed to clear previous workspace session rows")?;

            for state in states {
                sqlx::query(
                    "INSERT INTO tab_session_workspace_state
                     (session_id, workspace_id, active_environment_id, expanded_items_json, created_at, updated_at, revision)
                     VALUES (?, ?, ?, ?, ?, ?, 1)",
                )
                .bind(session_id.0.to_string())
                .bind(state.workspace_id.to_string())
                .bind(state.active_environment_id.map(|id| id.to_string()))
                .bind(state.expanded_items_json.as_str())
                .bind(state.created_at)
                .bind(ts)
                .execute(&mut *tx)
                .await
                .context("failed to insert workspace session row")?;
            }

            tx.commit().await?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn load_workspace_states(
        &self,
        session_id: SessionId,
    ) -> RepoResult<Vec<TabSessionWorkspaceState>> {
        self.db.block_on(async {
            let rows = sqlx::query(
                "SELECT workspace_id, active_environment_id, expanded_items_json, created_at, updated_at
                 FROM tab_session_workspace_state
                 WHERE session_id = ?
                 ORDER BY workspace_id ASC",
            )
            .bind(session_id.0.to_string())
            .fetch_all(self.db.pool())
            .await
            .context("failed to load workspace session rows")?;

            rows.into_iter()
                .map(|row| {
                    Ok::<TabSessionWorkspaceState, anyhow::Error>(TabSessionWorkspaceState {
                        workspace_id: WorkspaceId::parse(row.get::<&str, _>("workspace_id"))?,
                        active_environment_id: row
                            .get::<Option<String>, _>("active_environment_id")
                            .map(|id| EnvironmentId::parse(&id))
                            .transpose()?,
                        expanded_items_json: row.get("expanded_items_json"),
                        created_at: row.get("created_at"),
                        updated_at: row.get("updated_at"),
                    })
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
    }

    fn clear_session(&self, session_id: SessionId) -> RepoResult<()> {
        self.db.block_on(async {
            sqlx::query("DELETE FROM tab_session_state WHERE session_id = ?")
                .bind(session_id.0.to_string())
                .execute(self.db.pool())
                .await
                .context("failed to clear tab session")?;
            sqlx::query("DELETE FROM tab_session_workspace_state WHERE session_id = ?")
                .bind(session_id.0.to_string())
                .execute(self.db.pool())
                .await
                .context("failed to clear tab session workspace state")?;
            sqlx::query("DELETE FROM tab_session_metadata WHERE session_id = ?")
                .bind(session_id.0.to_string())
                .execute(self.db.pool())
                .await
                .context("failed to clear tab session metadata")?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn clear_all(&self) -> RepoResult<()> {
        self.db.block_on(async {
            sqlx::query("DELETE FROM tab_session_state")
                .execute(self.db.pool())
                .await
                .context("failed to clear all tab sessions")?;
            sqlx::query("DELETE FROM tab_session_workspace_state")
                .execute(self.db.pool())
                .await
                .context("failed to clear all tab session workspace state")?;
            sqlx::query("DELETE FROM tab_session_metadata")
                .execute(self.db.pool())
                .await
                .context("failed to clear all tab session metadata")?;
            Ok::<(), anyhow::Error>(())
        })
    }
}

fn load_snapshot(
    db: &DbRef,
    session_id: Option<SessionId>,
) -> RepoResult<Option<TabSessionSnapshot>> {
    db.block_on(async {
        let rows = if let Some(session_id) = session_id {
            sqlx::query(
                "SELECT session_id, tab_order, item_kind, item_id, pinned, is_active, updated_at
                 FROM tab_session_state
                 WHERE session_id = ?
                 ORDER BY tab_order ASC",
            )
            .bind(session_id.0.to_string())
            .fetch_all(db.pool())
            .await
            .context("failed to load tab session rows")?
        } else {
            sqlx::query(
                "SELECT session_id, tab_order, item_kind, item_id, pinned, is_active, updated_at
                 FROM tab_session_state
                 WHERE session_id = (
                    SELECT session_id
                    FROM tab_session_state
                    GROUP BY session_id
                    ORDER BY MAX(updated_at) DESC, session_id DESC
                    LIMIT 1
                 )
                 ORDER BY tab_order ASC",
            )
            .fetch_all(db.pool())
            .await
            .context("failed to load most recent tab session rows")?
        };

        if rows.is_empty() {
            return Ok(None);
        }

        let session_id = SessionId(
            uuid::Uuid::parse_str(rows[0].get::<&str, _>("session_id"))
                .context("invalid stored tab session id")?,
        );
        let mut tabs = Vec::with_capacity(rows.len());
        let mut active = None;
        let mut updated_at = 0;
        let metadata_row = sqlx::query(
            "SELECT selected_workspace_id, sidebar_item_kind, sidebar_item_id, sidebar_collapsed, sidebar_width_px, created_at, updated_at
             FROM tab_session_metadata
             WHERE session_id = ?",
        )
        .bind(session_id.0.to_string())
        .fetch_optional(db.pool())
        .await
        .context("failed to load tab session metadata")?;

        for row in rows {
            let item_kind = row.get::<&str, _>("item_kind");
            let item_id = row.get::<Option<String>, _>("item_id");
            let item_key = ItemKey::from_storage_parts(item_kind, item_id.as_deref())?;
            let tab = TabState {
                key: TabKey::from(item_key),
                pinned: row.get("pinned"),
            };
            if row.get::<bool, _>("is_active") {
                active = Some(tab.key);
            }
            updated_at = updated_at.max(row.get::<i64, _>("updated_at"));
            tabs.push(tab);
        }

        let metadata = if let Some(row) = metadata_row {
            let sidebar_kind = row.get::<Option<String>, _>("sidebar_item_kind");
            let sidebar_id = row.get::<Option<String>, _>("sidebar_item_id");
            let sidebar_selection = match sidebar_kind {
                Some(kind) => Some(ItemKey::from_storage_parts(&kind, sidebar_id.as_deref())?),
                None => None,
            };

            TabSessionMetadata {
                selected_workspace_id: row.get("selected_workspace_id"),
                sidebar_selection,
                window_layout: WindowLayoutState {
                    sidebar_collapsed: row.get("sidebar_collapsed"),
                    sidebar_width_px: row.get::<f64, _>("sidebar_width_px") as f32,
                    sidebar_section: Default::default(),
                },
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            }
        } else {
            TabSessionMetadata {
                selected_workspace_id: None,
                sidebar_selection: None,
                window_layout: WindowLayoutState::default(),
                created_at: updated_at,
                updated_at,
            }
        };

        Ok(Some(TabSessionSnapshot {
            session_id,
            tabs,
            active,
            updated_at,
            metadata,
        }))
    })
}

pub type TabSessionRepoRef = Arc<dyn TabSessionRepository>;
