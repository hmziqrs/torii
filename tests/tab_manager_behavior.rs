use torii::{
    domain::ids::{CollectionId, WorkspaceId},
    session::{
        item_key::{ItemKey, ItemKind, TabKey},
        tab_manager::TabManager,
        workspace_session::WorkspaceSession,
    },
};

#[test]
fn item_key_distinguishes_persisted_and_utility_items() {
    let workspace_key = ItemKey::workspace(WorkspaceId::new());
    let collection_key = ItemKey::collection(CollectionId::new());
    let settings_key = ItemKey::settings();
    let about_key = ItemKey::about();

    assert_eq!(workspace_key.kind, ItemKind::Workspace);
    assert!(workspace_key.is_persisted());
    assert_ne!(workspace_key, collection_key);

    assert_eq!(settings_key.kind, ItemKind::Settings);
    assert!(!settings_key.is_persisted());
    assert_eq!(settings_key.id, None);

    assert_eq!(about_key.kind, ItemKind::About);
    assert!(!about_key.is_persisted());
    assert_eq!(about_key.id, None);
}

#[test]
fn open_or_focus_prevents_duplicate_tabs_for_same_item() {
    let workspace_key = ItemKey::workspace(WorkspaceId::new());
    let request_key = ItemKey::request(torii::domain::ids::RequestId::new());
    let mut manager = TabManager::default();

    let first = manager.open_or_focus(workspace_key);
    let duplicate = manager.open_or_focus(workspace_key);
    let second = manager.open_or_focus(request_key);

    assert!(!first.already_open);
    assert_eq!(first.index, 0);

    assert!(duplicate.already_open);
    assert_eq!(duplicate.index, 0);
    assert_eq!(manager.tabs().len(), 2);

    assert!(!second.already_open);
    assert_eq!(manager.active(), Some(TabKey::from(request_key)));
}

#[test]
fn close_active_tab_prefers_left_neighbor_then_none() {
    let first = ItemKey::workspace(WorkspaceId::new());
    let second = ItemKey::collection(CollectionId::new());
    let third = ItemKey::settings();
    let mut manager = TabManager::default();

    manager.open_or_focus(first);
    manager.open_or_focus(second);
    manager.open_or_focus(third);

    let close_third = manager
        .close(TabKey::from(third))
        .expect("third tab should exist");
    assert_eq!(close_third.next_active, Some(TabKey::from(second)));
    assert_eq!(manager.active(), Some(TabKey::from(second)));

    let close_first = manager
        .close(TabKey::from(first))
        .expect("first tab should exist");
    assert_eq!(close_first.next_active, Some(TabKey::from(second)));
    assert_eq!(manager.active(), Some(TabKey::from(second)));

    let close_second = manager
        .close(TabKey::from(second))
        .expect("second tab should exist");
    assert_eq!(close_second.next_active, None);
    assert_eq!(manager.active(), None);
    assert!(manager.tabs().is_empty());
}

#[test]
fn reorder_moves_tabs_without_changing_active_identity() {
    let first = ItemKey::workspace(WorkspaceId::new());
    let second = ItemKey::collection(CollectionId::new());
    let third = ItemKey::about();
    let mut manager = TabManager::default();

    manager.open_or_focus(first);
    manager.open_or_focus(second);
    manager.open_or_focus(third);

    assert!(manager.reorder(2, 0));
    assert_eq!(
        manager
            .tabs()
            .iter()
            .map(|tab| tab.key)
            .collect::<Vec<_>>(),
        vec![TabKey::from(third), TabKey::from(first), TabKey::from(second)]
    );
    assert_eq!(manager.active(), Some(TabKey::from(third)));

    assert!(!manager.reorder(4, 0));
    assert!(!manager.reorder(0, 4));
    assert!(!manager.reorder(1, 1));
}

#[test]
fn workspace_session_generates_stable_session_id_on_creation() {
    let first = WorkspaceSession::new();
    let second = WorkspaceSession::new();

    assert_ne!(first.session_id, second.session_id);
    assert!(first.tab_manager.tabs().is_empty());
    assert_eq!(first.sidebar_selection, None);
    assert_eq!(first.selected_workspace_id, None);
}
