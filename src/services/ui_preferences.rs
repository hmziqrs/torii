use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use gpui::SharedString;
use gpui_component::scroll::ScrollbarShow;
use serde::{Deserialize, Serialize};

use crate::{domain::preferences::UiPreferences, repos::preferences_repo::PreferencesRepoRef};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPreferencesSnapshot {
    pub theme: SharedString,
    pub scrollbar_show: Option<ScrollbarShow>,
    pub theme_mode: Option<String>,
    pub locale: Option<String>,
}

impl Default for UiPreferencesSnapshot {
    fn default() -> Self {
        Self {
            theme: "Default Light".into(),
            scrollbar_show: None,
            theme_mode: Some("light".to_string()),
            locale: Some("en".to_string()),
        }
    }
}

impl From<UiPreferences> for UiPreferencesSnapshot {
    fn from(value: UiPreferences) -> Self {
        Self {
            theme: value.theme,
            scrollbar_show: value.scrollbar_show,
            theme_mode: value.theme_mode,
            locale: value.locale,
        }
    }
}

impl From<UiPreferencesSnapshot> for UiPreferences {
    fn from(value: UiPreferencesSnapshot) -> Self {
        Self {
            theme: value.theme,
            scrollbar_show: value.scrollbar_show,
            theme_mode: value.theme_mode,
            locale: value.locale,
        }
    }
}

pub trait UiPreferencesStore: Send + Sync {
    fn load(&self) -> Result<Option<UiPreferencesSnapshot>>;
    fn save(&self, snapshot: &UiPreferencesSnapshot) -> Result<()>;
}

pub type UiPreferencesStoreRef = Arc<dyn UiPreferencesStore>;

#[derive(Clone)]
pub struct SqliteUiPreferencesStore {
    repo: PreferencesRepoRef,
}

impl SqliteUiPreferencesStore {
    pub fn new(repo: PreferencesRepoRef) -> Self {
        Self { repo }
    }
}

impl UiPreferencesStore for SqliteUiPreferencesStore {
    fn load(&self) -> Result<Option<UiPreferencesSnapshot>> {
        Ok(self.repo.load_ui_preferences()?.map(Into::into))
    }

    fn save(&self, snapshot: &UiPreferencesSnapshot) -> Result<()> {
        self.repo.save_ui_preferences(&snapshot.clone().into())
    }
}

pub struct InMemoryUiPreferencesStore {
    state: Mutex<Option<UiPreferencesSnapshot>>,
}

impl InMemoryUiPreferencesStore {
    pub fn new(initial: Option<UiPreferencesSnapshot>) -> Self {
        Self {
            state: Mutex::new(initial),
        }
    }
}

impl UiPreferencesStore for InMemoryUiPreferencesStore {
    fn load(&self) -> Result<Option<UiPreferencesSnapshot>> {
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("preferences mutex poisoned"))?;
        Ok(state.clone())
    }

    fn save(&self, snapshot: &UiPreferencesSnapshot) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("preferences mutex poisoned"))?;
        *state = Some(snapshot.clone());
        Ok(())
    }
}

pub fn load_legacy_ui_preferences_file(path: &Path) -> Result<Option<UiPreferencesSnapshot>> {
    if !path.exists() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed = serde_json::from_str::<UiPreferencesSnapshot>(&contents)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(parsed))
}
