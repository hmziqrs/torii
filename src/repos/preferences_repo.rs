use std::sync::Arc;

use anyhow::Context as _;
use sqlx::Row as _;

use crate::domain::{preferences::UiPreferences, revision::now_unix_ts};

use super::{DbRef, RepoResult};

const UI_PREFERENCES_KEY: &str = "ui.preferences.v1";

pub trait PreferencesRepository: Send + Sync {
    fn load_ui_preferences(&self) -> RepoResult<Option<UiPreferences>>;
    fn save_ui_preferences(&self, value: &UiPreferences) -> RepoResult<()>;
}

#[derive(Clone)]
pub struct SqlitePreferencesRepository {
    db: DbRef,
}

impl SqlitePreferencesRepository {
    pub fn new(db: DbRef) -> Self {
        Self { db }
    }
}

impl PreferencesRepository for SqlitePreferencesRepository {
    fn load_ui_preferences(&self) -> RepoResult<Option<UiPreferences>> {
        self.db.block_on(async {
            let row = sqlx::query("SELECT value_json FROM ui_preferences WHERE key = ?")
                .bind(UI_PREFERENCES_KEY)
                .fetch_optional(self.db.pool())
                .await
                .context("failed to load ui preferences row")?;

            if let Some(row) = row {
                let value_json: String = row.get("value_json");
                let parsed = serde_json::from_str::<UiPreferences>(&value_json)
                    .context("invalid ui preferences json")?;
                Ok(Some(parsed))
            } else {
                Ok(None)
            }
        })
    }

    fn save_ui_preferences(&self, value: &UiPreferences) -> RepoResult<()> {
        let value_json =
            serde_json::to_string(value).context("failed to serialize ui preferences")?;
        self.db.block_on(async {
            sqlx::query(
                "INSERT INTO ui_preferences (key, value_json, updated_at)
                 VALUES (?, ?, ?)
                 ON CONFLICT(key) DO UPDATE SET
                   value_json = excluded.value_json,
                   updated_at = excluded.updated_at",
            )
            .bind(UI_PREFERENCES_KEY)
            .bind(value_json)
            .bind(now_unix_ts())
            .execute(self.db.pool())
            .await
            .context("failed to save ui preferences")?;
            Ok::<(), anyhow::Error>(())
        })
    }
}

pub type PreferencesRepoRef = Arc<dyn PreferencesRepository>;
