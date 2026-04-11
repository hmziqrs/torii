use serde::{Deserialize, Serialize};

use crate::session::item_key::{ItemKey, TabKey};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabState {
    pub key: TabKey,
    pub pinned: bool,
}

impl TabState {
    pub fn new(key: impl Into<TabKey>) -> Self {
        Self {
            key: key.into(),
            pinned: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenTabOutcome {
    pub key: TabKey,
    pub index: usize,
    pub already_open: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseTabOutcome {
    pub closed: TabKey,
    pub closed_index: usize,
    pub next_active: Option<TabKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TabManager {
    tabs: Vec<TabState>,
    active: Option<TabKey>,
}

impl TabManager {
    pub fn tabs(&self) -> &[TabState] {
        &self.tabs
    }

    pub fn active(&self) -> Option<TabKey> {
        self.active
    }

    pub fn open_or_focus(&mut self, item_key: ItemKey) -> OpenTabOutcome {
        let tab_key = TabKey::from(item_key);

        if let Some(index) = self.index_of(tab_key) {
            self.active = Some(tab_key);
            return OpenTabOutcome {
                key: tab_key,
                index,
                already_open: true,
            };
        }

        let index = self.tabs.len();
        self.tabs.push(TabState::new(tab_key));
        self.active = Some(tab_key);

        OpenTabOutcome {
            key: tab_key,
            index,
            already_open: false,
        }
    }

    pub fn close(&mut self, tab_key: TabKey) -> Option<CloseTabOutcome> {
        let closed_index = self.index_of(tab_key)?;
        self.tabs.remove(closed_index);

        let next_active = if self.tabs.is_empty() {
            None
        } else if self.active == Some(tab_key) {
            let fallback_index = closed_index.saturating_sub(1).min(self.tabs.len() - 1);
            Some(self.tabs[fallback_index].key)
        } else {
            self.active
        };

        self.active = next_active;

        Some(CloseTabOutcome {
            closed: tab_key,
            closed_index,
            next_active,
        })
    }

    pub fn set_active(&mut self, tab_key: TabKey) -> bool {
        if self.index_of(tab_key).is_some() {
            self.active = Some(tab_key);
            return true;
        }

        false
    }

    pub fn reorder(&mut self, from: usize, to: usize) -> bool {
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return false;
        }

        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        true
    }

    fn index_of(&self, tab_key: TabKey) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.key == tab_key)
    }
}
