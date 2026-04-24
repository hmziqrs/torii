use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use anyhow::Result;

use crate::{
    domain::{
        ids::{EnvironmentId, WorkspaceId},
        item_id::ItemId,
    },
    repos::{
        collection_repo::CollectionRepoRef,
        environment_repo::EnvironmentRepoRef,
        folder_repo::FolderRepoRef,
        request_repo::RequestRepoRef,
        tab_session_repo::{TabSessionRepoRef, TabSessionSnapshot},
        workspace_repo::WorkspaceRepoRef,
    },
    session::{
        item_key::{ItemKey, ItemKind, TabKey},
        tab_manager::TabState,
        workspace_session::ExpandableItem,
    },
};

#[derive(Clone)]
pub struct SessionRestoreService {
    tab_sessions: TabSessionRepoRef,
    workspaces: WorkspaceRepoRef,
    collections: CollectionRepoRef,
    folders: FolderRepoRef,
    requests: RequestRepoRef,
    environments: EnvironmentRepoRef,
    claimed_sessions: Arc<Mutex<HashSet<crate::session::workspace_session::SessionId>>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RestoredSessionState {
    pub tabs: Vec<TabState>,
    pub active: Option<TabKey>,
    pub selected_workspace_id: Option<WorkspaceId>,
    pub active_environments_by_workspace: HashMap<WorkspaceId, EnvironmentId>,
    pub expanded_items_by_workspace: HashMap<WorkspaceId, HashSet<ExpandableItem>>,
    pub sidebar_selection: Option<ItemKey>,
    pub window_layout: crate::session::window_layout::WindowLayoutState,
}

impl SessionRestoreService {
    pub fn new(
        tab_sessions: TabSessionRepoRef,
        workspaces: WorkspaceRepoRef,
        collections: CollectionRepoRef,
        folders: FolderRepoRef,
        requests: RequestRepoRef,
        environments: EnvironmentRepoRef,
    ) -> Self {
        Self {
            tab_sessions,
            workspaces,
            collections,
            folders,
            requests,
            environments,
            claimed_sessions: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn take_next_restore(&self) -> Result<Option<RestoredSessionState>> {
        let snapshots = self.tab_sessions.list_sessions()?;
        let mut claimed = self
            .claimed_sessions
            .lock()
            .expect("session restore claims poisoned");
        for snapshot in snapshots {
            if claimed.contains(&snapshot.session_id) {
                continue;
            }
            claimed.insert(snapshot.session_id);
            if let Some(restored) = self.restore_snapshot(snapshot)? {
                return Ok(Some(restored));
            }
        }
        Ok(None)
    }

    pub fn restore_snapshot(
        &self,
        snapshot: TabSessionSnapshot,
    ) -> Result<Option<RestoredSessionState>> {
        let mut tabs = Vec::new();
        for tab in snapshot.tabs {
            if self.item_exists(tab.key.item())? {
                tabs.push(tab);
            }
        }

        if tabs.is_empty() {
            return Ok(None);
        }

        let active = snapshot
            .active
            .filter(|key| tabs.iter().any(|tab| tab.key == *key));
        let mut selected_workspace_id = None;
        if let Some(active) = active {
            selected_workspace_id = self.workspace_for_item(active.item())?;
        }
        if selected_workspace_id.is_none() {
            for tab in &tabs {
                selected_workspace_id = self.workspace_for_item(tab.key.item())?;
                if selected_workspace_id.is_some() {
                    break;
                }
            }
        }

        let workspace_states = self
            .tab_sessions
            .load_workspace_states(snapshot.session_id)?;

        let active_environments_by_workspace = workspace_states
            .iter()
            .filter_map(|state| {
                let active_environment_id = state.active_environment_id?;
                match self.environments.get(active_environment_id) {
                    Ok(Some(environment)) if environment.workspace_id == state.workspace_id => {
                        Some((state.workspace_id, active_environment_id))
                    }
                    _ => None,
                }
            })
            .collect::<HashMap<_, _>>();
        let expanded_items_by_workspace = workspace_states
            .into_iter()
            .filter_map(|state| {
                let expanded =
                    serde_json::from_str::<HashSet<ExpandableItem>>(&state.expanded_items_json)
                        .unwrap_or_default();
                if expanded.is_empty() {
                    None
                } else {
                    Some((state.workspace_id, expanded))
                }
            })
            .collect::<HashMap<_, _>>();

        Ok(Some(RestoredSessionState {
            tabs,
            active,
            selected_workspace_id,
            active_environments_by_workspace,
            expanded_items_by_workspace,
            sidebar_selection: snapshot.metadata.sidebar_selection,
            window_layout: snapshot.metadata.window_layout,
        }))
    }

    fn item_exists(&self, item: ItemKey) -> Result<bool> {
        let exists = match (item.kind, item.id) {
            (ItemKind::Workspace, Some(ItemId::Workspace(id))) => {
                self.workspaces.get(id)?.is_some()
            }
            (ItemKind::Collection, Some(ItemId::Collection(id))) => {
                self.collections.get(id)?.is_some()
            }
            (ItemKind::Folder, Some(ItemId::Folder(id))) => self.folders.get(id)?.is_some(),
            (ItemKind::Environment, Some(ItemId::Environment(id))) => {
                self.environments.get(id)?.is_some()
            }
            (ItemKind::Request, Some(ItemId::Request(id))) => self.requests.get(id)?.is_some(),
            (ItemKind::Settings | ItemKind::About | ItemKind::LayoutDebug, None) => true,
            _ => false,
        };

        Ok(exists)
    }

    pub fn workspace_for_item(&self, item: ItemKey) -> Result<Option<WorkspaceId>> {
        let workspace_id = match (item.kind, item.id) {
            (ItemKind::Workspace, Some(ItemId::Workspace(id))) => Some(id),
            (ItemKind::Collection, Some(ItemId::Collection(id))) => self
                .collections
                .get(id)?
                .map(|collection| collection.workspace_id),
            (ItemKind::Folder, Some(ItemId::Folder(id))) => {
                let Some(folder) = self.folders.get(id)? else {
                    return Ok(None);
                };
                self.collections
                    .get(folder.collection_id)?
                    .map(|collection| collection.workspace_id)
            }
            (ItemKind::Environment, Some(ItemId::Environment(id))) => {
                let Some(environment) = self.environments.get(id)? else {
                    return Ok(None);
                };
                Some(environment.workspace_id)
            }
            (ItemKind::Request, Some(ItemId::Request(id))) => {
                let Some(request) = self.requests.get(id)? else {
                    return Ok(None);
                };
                self.collections
                    .get(request.collection_id)?
                    .map(|collection| collection.workspace_id)
            }
            (ItemKind::Settings | ItemKind::About | ItemKind::LayoutDebug, None) => None,
            _ => None,
        };

        Ok(workspace_id)
    }
}
