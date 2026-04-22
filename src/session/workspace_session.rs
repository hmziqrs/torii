use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    domain::ids::{EnvironmentId, WorkspaceId},
    session::{
        item_key::{ItemKey, TabKey},
        tab_manager::{CloseTabOutcome, OpenTabOutcome, TabManager, TabState},
        window_layout::WindowLayoutState,
    },
};
use gpui::Context;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSession {
    pub session_id: SessionId,
    pub selected_workspace_id: Option<WorkspaceId>,
    pub active_environments_by_workspace: std::collections::HashMap<WorkspaceId, EnvironmentId>,
    pub sidebar_selection: Option<ItemKey>,
    pub tab_manager: TabManager,
    pub window_layout: WindowLayoutState,
}

impl WorkspaceSession {
    pub fn new() -> Self {
        Self {
            session_id: SessionId::new(),
            selected_workspace_id: None,
            active_environments_by_workspace: std::collections::HashMap::new(),
            sidebar_selection: None,
            tab_manager: TabManager::default(),
            window_layout: WindowLayoutState::default(),
        }
    }

    pub fn open_or_focus(&mut self, item_key: ItemKey, cx: &mut Context<Self>) -> OpenTabOutcome {
        self.sidebar_selection = Some(item_key);
        let outcome = self.tab_manager.open_or_focus(item_key);
        cx.notify();
        outcome
    }

    pub fn focus_tab(&mut self, tab_key: TabKey, cx: &mut Context<Self>) -> bool {
        let changed = self.tab_manager.set_active(tab_key);
        if changed {
            self.sidebar_selection = Some(tab_key.item());
            cx.notify();
        }
        changed
    }

    pub fn close_tab(
        &mut self,
        tab_key: TabKey,
        cx: &mut Context<Self>,
    ) -> Option<CloseTabOutcome> {
        let outcome = self.tab_manager.close(tab_key)?;
        self.sidebar_selection = self.tab_manager.active().map(|active| active.item());
        cx.notify();
        Some(outcome)
    }

    pub fn move_active_tab_by(&mut self, delta: isize, cx: &mut Context<Self>) -> bool {
        let changed = self.tab_manager.move_active_by(delta);
        if changed {
            cx.notify();
        }
        changed
    }

    pub fn set_selected_workspace(
        &mut self,
        workspace_id: Option<WorkspaceId>,
        cx: &mut Context<Self>,
    ) {
        if self.selected_workspace_id != workspace_id {
            self.selected_workspace_id = workspace_id;
            cx.notify();
        }
    }

    pub fn set_sidebar_selection(&mut self, selection: Option<ItemKey>, cx: &mut Context<Self>) {
        if self.sidebar_selection != selection {
            self.sidebar_selection = selection;
            cx.notify();
        }
    }

    pub fn active_environment_for_workspace(
        &self,
        workspace_id: WorkspaceId,
    ) -> Option<EnvironmentId> {
        self.active_environments_by_workspace
            .get(&workspace_id)
            .copied()
    }

    pub fn set_active_environment_for_workspace(
        &mut self,
        workspace_id: WorkspaceId,
        environment_id: Option<EnvironmentId>,
        cx: &mut Context<Self>,
    ) {
        let changed = match environment_id {
            Some(env_id) => {
                if self.active_environments_by_workspace.get(&workspace_id) == Some(&env_id) {
                    false
                } else {
                    self.active_environments_by_workspace
                        .insert(workspace_id, env_id);
                    true
                }
            }
            None => self
                .active_environments_by_workspace
                .remove(&workspace_id)
                .is_some(),
        };
        if changed {
            cx.notify();
        }
    }

    pub fn restore_tabs(
        &mut self,
        tabs: Vec<TabState>,
        active: Option<TabKey>,
        selected_workspace_id: Option<WorkspaceId>,
        active_environments_by_workspace: std::collections::HashMap<WorkspaceId, EnvironmentId>,
        sidebar_selection: Option<ItemKey>,
        window_layout: WindowLayoutState,
        cx: &mut Context<Self>,
    ) {
        self.tab_manager.set_tabs(tabs, active);
        self.sidebar_selection =
            sidebar_selection.or_else(|| self.tab_manager.active().map(|tab| tab.item()));
        self.selected_workspace_id = selected_workspace_id;
        self.active_environments_by_workspace = active_environments_by_workspace;
        self.window_layout = window_layout;
        cx.notify();
    }

    pub fn set_sidebar_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        if self.window_layout.sidebar_collapsed != collapsed {
            self.window_layout.sidebar_collapsed = collapsed;
            cx.notify();
        }
    }

    pub fn set_sidebar_width(&mut self, width_px: f32, cx: &mut Context<Self>) {
        if (self.window_layout.sidebar_width_px - width_px).abs() > f32::EPSILON {
            self.window_layout.sidebar_width_px = width_px;
            cx.notify();
        }
    }

    pub fn reorder_tabs(&mut self, from: usize, to: usize, cx: &mut Context<Self>) -> bool {
        let changed = self.tab_manager.reorder(from, to);
        if changed {
            cx.notify();
        }
        changed
    }

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.window_layout.sidebar_collapsed = !self.window_layout.sidebar_collapsed;
        cx.notify();
    }

    pub fn close_tabs(&mut self, item_keys: &[ItemKey], cx: &mut Context<Self>) -> usize {
        let keys = item_keys
            .iter()
            .copied()
            .map(TabKey::from)
            .collect::<Vec<_>>();
        let closed = self.tab_manager.close_all(&keys);
        if closed > 0 {
            self.sidebar_selection = self.tab_manager.active().map(|tab| tab.item());
            cx.notify();
        }
        closed
    }
}

impl Default for WorkspaceSession {
    fn default() -> Self {
        Self::new()
    }
}
