use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    domain::ids::WorkspaceId,
    session::{
        item_key::ItemKey, tab_manager::TabManager, window_layout::WindowLayoutState,
    },
};

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
    pub sidebar_selection: Option<ItemKey>,
    pub tab_manager: TabManager,
    pub window_layout: WindowLayoutState,
}

impl WorkspaceSession {
    pub fn new() -> Self {
        Self {
            session_id: SessionId::new(),
            selected_workspace_id: None,
            sidebar_selection: None,
            tab_manager: TabManager::default(),
            window_layout: WindowLayoutState::default(),
        }
    }
}

impl Default for WorkspaceSession {
    fn default() -> Self {
        Self::new()
    }
}
