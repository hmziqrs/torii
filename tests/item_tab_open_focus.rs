use torii::{
    domain::ids::WorkspaceId,
    session::{
        item_key::{ItemKey, TabKey},
        tab_manager::TabManager,
    },
};

#[test]
fn opening_same_item_twice_focuses_existing_tab() {
    let item = ItemKey::workspace(WorkspaceId::new());
    let mut manager = TabManager::default();

    let first = manager.open_or_focus(item);
    let second = manager.open_or_focus(item);

    assert!(!first.already_open);
    assert!(second.already_open);
    assert_eq!(manager.tabs().len(), 1);
    assert_eq!(manager.active(), Some(TabKey::from(item)));
}
