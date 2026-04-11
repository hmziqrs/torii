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

    pub fn close_all(&mut self, keys: &[TabKey]) -> usize {
        let mut closed = 0;
        for key in keys {
            if self.close(*key).is_some() {
                closed += 1;
            }
        }
        closed
    }

    pub fn move_active_by(&mut self, delta: isize) -> bool {
        let Some(active) = self.active else {
            return false;
        };
        let Some(from) = self.index_of(active) else {
            return false;
        };
        let len = self.tabs.len();
        let target = from as isize + delta;
        if target < 0 || target >= len as isize {
            return false;
        }
        self.reorder(from, target as usize)
    }

    pub fn set_tabs(&mut self, tabs: Vec<TabState>, active: Option<TabKey>) {
        self.tabs = tabs;
        self.active = active.filter(|key| self.index_of(*key).is_some());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::{CollectionId, EnvironmentId, WorkspaceId};

    #[test]
    fn open_or_focus_dedupes_same_item() {
        let item = ItemKey::workspace(WorkspaceId::new());
        let mut manager = TabManager::default();

        let first = manager.open_or_focus(item);
        let second = manager.open_or_focus(item);

        assert!(!first.already_open);
        assert!(second.already_open);
        assert_eq!(manager.tabs.len(), 1);
        assert_eq!(manager.active, Some(TabKey::from(item)));
    }

    #[test]
    fn close_active_prefers_left_neighbor_then_none() {
        let first = ItemKey::workspace(WorkspaceId::new());
        let second = ItemKey::collection(CollectionId::new());

        let mut manager = TabManager::default();
        manager.open_or_focus(first);
        manager.open_or_focus(second);

        let outcome = manager.close(TabKey::from(second)).unwrap();
        assert_eq!(outcome.next_active, Some(TabKey::from(first)));

        let last = manager.close(TabKey::from(first)).unwrap();
        assert_eq!(last.next_active, None);
        assert!(manager.tabs.is_empty());
    }

    #[test]
    fn reorder_keeps_active_identity_stable() {
        let first = ItemKey::workspace(WorkspaceId::new());
        let second = ItemKey::collection(CollectionId::new());
        let third = ItemKey::environment(EnvironmentId::new());

        let mut manager = TabManager::default();
        manager.open_or_focus(first);
        manager.open_or_focus(second);
        manager.open_or_focus(third);

        assert!(manager.reorder(2, 0));
        assert_eq!(
            manager.tabs.iter().map(|tab| tab.key).collect::<Vec<_>>(),
            vec![
                TabKey::from(third),
                TabKey::from(first),
                TabKey::from(second)
            ]
        );
        assert_eq!(manager.active, Some(TabKey::from(third)));
    }
}
