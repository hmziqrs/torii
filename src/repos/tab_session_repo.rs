use std::sync::Arc;

use anyhow::Context as _;
use sqlx::Row as _;

use crate::{
    domain::revision::now_unix_ts,
    session::{
        item_key::{ItemKey, TabKey},
        tab_manager::TabState,
        workspace_session::SessionId,
    },
};

use super::{DbRef, RepoResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabSessionSnapshot {
    pub session_id: SessionId,
    pub tabs: Vec<TabState>,
    pub active: Option<TabKey>,
    pub updated_at: i64,
}

pub trait TabSessionRepository: Send + Sync {
    fn save_session(
        &self,
        session_id: SessionId,
        tabs: &[TabState],
        active: Option<TabKey>,
    ) -> RepoResult<()>;
    fn load_session(&self, session_id: SessionId) -> RepoResult<Option<TabSessionSnapshot>>;
    fn load_most_recent(&self) -> RepoResult<Option<TabSessionSnapshot>>;
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
    ) -> RepoResult<()> {
        self.db.block_on(async {
            let mut tx = self.db.pool().begin().await?;
            let ts = now_unix_ts();

            sqlx::query("DELETE FROM tab_session_state WHERE session_id = ?")
                .bind(session_id.0.to_string())
                .execute(&mut *tx)
                .await
                .context("failed to clear previous tab session rows")?;

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

    fn clear_session(&self, session_id: SessionId) -> RepoResult<()> {
        self.db.block_on(async {
            sqlx::query("DELETE FROM tab_session_state WHERE session_id = ?")
                .bind(session_id.0.to_string())
                .execute(self.db.pool())
                .await
                .context("failed to clear tab session")?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn clear_all(&self) -> RepoResult<()> {
        self.db.block_on(async {
            sqlx::query("DELETE FROM tab_session_state")
                .execute(self.db.pool())
                .await
                .context("failed to clear all tab sessions")?;
            Ok::<(), anyhow::Error>(())
        })
    }
}

fn load_snapshot(db: &DbRef, session_id: Option<SessionId>) -> RepoResult<Option<TabSessionSnapshot>> {
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

        Ok(Some(TabSessionSnapshot {
            session_id,
            tabs,
            active,
            updated_at,
        }))
    })
}

pub type TabSessionRepoRef = Arc<dyn TabSessionRepository>;
