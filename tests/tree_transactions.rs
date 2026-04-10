mod common;

use std::sync::Arc;

use anyhow::Result;
use gpui_starter::repos::{
    collection_repo::{CollectionRepository, SqliteCollectionRepository},
    folder_repo::{FolderRepository, SqliteFolderRepository},
    request_repo::{RequestRepository, SqliteRequestRepository},
    workspace_repo::{SqliteWorkspaceRepository, WorkspaceRepository},
};

#[test]
fn folder_move_and_reorder_remains_consistent() -> Result<()> {
    let (_paths, db) = common::test_database("tree-transactions")?;
    let db = Arc::new(db);

    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let collection_repo = SqliteCollectionRepository::new(db.clone());
    let folder_repo = SqliteFolderRepository::new(db.clone());
    let request_repo = SqliteRequestRepository::new(db.clone());

    let workspace = workspace_repo.create("Workspace")?;
    let source_collection = collection_repo.create(workspace.id, "A")?;
    let target_collection = collection_repo.create(workspace.id, "B")?;

    let root = folder_repo.create(source_collection.id, None, "root")?;
    let child = folder_repo.create(source_collection.id, Some(root.id), "child")?;
    let request = request_repo.create(
        source_collection.id,
        Some(child.id),
        "request",
        "GET",
        "https://example.test",
    )?;

    folder_repo.move_to(root.id, target_collection.id, None)?;

    let target_folders = folder_repo.list_by_collection(target_collection.id)?;
    assert!(
        target_folders.iter().any(|folder| folder.id == root.id),
        "moved root folder should exist in target collection"
    );
    assert!(
        target_folders.iter().any(|folder| folder.id == child.id),
        "child folder should move with its root"
    );
    assert!(target_folders
        .iter()
        .all(|folder| folder.collection_id == target_collection.id));

    let target_requests = request_repo.list_by_collection(target_collection.id)?;
    assert!(
        target_requests.iter().any(|item| item.id == request.id),
        "request should move with folder subtree"
    );

    let sibling = folder_repo.create(target_collection.id, None, "sibling")?;
    folder_repo.reorder_in_parent(target_collection.id, None, &[sibling.id, root.id])?;
    let reordered = folder_repo.list_by_collection(target_collection.id)?;
    let root_order = reordered
        .iter()
        .find(|folder| folder.id == root.id)
        .map(|folder| folder.sort_order)
        .unwrap_or(-1);
    let sibling_order = reordered
        .iter()
        .find(|folder| folder.id == sibling.id)
        .map(|folder| folder.sort_order)
        .unwrap_or(-1);
    assert_eq!(sibling_order, 0);
    assert_eq!(root_order, 1);

    folder_repo.delete(root.id)?;
    let after_delete_requests = request_repo.list_by_collection(target_collection.id)?;
    assert!(
        !after_delete_requests.iter().any(|item| item.id == request.id),
        "deleting a folder subtree must delete descendant requests"
    );

    Ok(())
}
