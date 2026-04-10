use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use gpui::SharedString;
use gpui_component::scroll::ScrollbarShow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPreferencesSnapshot {
    pub theme: SharedString,
    pub scrollbar_show: Option<ScrollbarShow>,
}

impl Default for UiPreferencesSnapshot {
    fn default() -> Self {
        Self {
            theme: "Default Light".into(),
            scrollbar_show: None,
        }
    }
}

pub trait UiPreferencesStore: Send + Sync {
    fn load(&self) -> Result<Option<UiPreferencesSnapshot>>;
    fn save(&self, snapshot: &UiPreferencesSnapshot) -> Result<()>;
}

pub type UiPreferencesStoreRef = Arc<dyn UiPreferencesStore>;

pub struct JsonUiPreferencesStore {
    path: std::path::PathBuf,
}

impl JsonUiPreferencesStore {
    pub fn new(path: std::path::PathBuf) -> Self {
        Self { path }
    }
}

impl UiPreferencesStore for JsonUiPreferencesStore {
    fn load(&self) -> Result<Option<UiPreferencesSnapshot>> {
        if !self.path.exists() {
            return Ok(None);
        }

        let contents = std::fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        let snapshot = serde_json::from_str::<UiPreferencesSnapshot>(&contents)
            .with_context(|| format!("failed to parse {}", self.path.display()))?;

        Ok(Some(snapshot))
    }

    fn save(&self, snapshot: &UiPreferencesSnapshot) -> Result<()> {
        let json =
            serde_json::to_string_pretty(snapshot).context("failed to encode preferences")?;

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        std::fs::write(&self.path, json)
            .with_context(|| format!("failed to write {}", self.path.display()))?;
        Ok(())
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
