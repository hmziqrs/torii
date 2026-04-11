use torii::{
    domain::ids::{CollectionId, WorkspaceId},
    session::{
        item_key::{ItemKey, TabKey},
        tab_manager::TabManager,
    },
};

#[test]
fn focused_tab_remains_stable_after_reorder_and_delete() {
    let workspace = ItemKey::workspace(WorkspaceId::new());
    let collection = ItemKey::collection(CollectionId::new());

    let mut manager = TabManager::default();
    manager.open_or_focus(workspace);
    manager.open_or_focus(collection);

    assert!(manager.reorder(1, 0));
    assert_eq!(manager.active(), Some(TabKey::from(collection)));

    let close = manager.close(TabKey::from(collection)).expect("collection tab should close");
    assert_eq!(close.next_active, Some(TabKey::from(workspace)));
    assert_eq!(manager.active(), Some(TabKey::from(workspace)));
    assert_eq!(manager.tabs(), &[torii::session::tab_manager::TabState::new(workspace)]);
}
