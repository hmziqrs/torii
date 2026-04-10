mod common;

use std::sync::Arc;

use anyhow::Result;
use torii::repos::{
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
    let source_sibling = folder_repo.create(source_collection.id, None, "source-sibling")?;
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
    let source_folders = folder_repo.list_by_collection(source_collection.id)?;
    assert!(
        target_folders.iter().any(|folder| folder.id == root.id),
        "moved root folder should exist in target collection"
    );
    assert!(
        target_folders.iter().any(|folder| folder.id == child.id),
        "child folder should move with its root"
    );
    assert!(
        target_folders
            .iter()
            .all(|folder| folder.collection_id == target_collection.id)
    );
    let moved_out_source_sibling = source_folders
        .iter()
        .find(|folder| folder.id == source_sibling.id)
        .expect("source sibling should remain in source collection");
    assert_eq!(
        moved_out_source_sibling.sort_order, 0,
        "source sibling order should compact after move-out"
    );

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
        !after_delete_requests
            .iter()
            .any(|item| item.id == request.id),
        "deleting a folder subtree must delete descendant requests"
    );

    Ok(())
}

#[test]
fn collection_and_request_mutations_compact_source_order() -> Result<()> {
    let (_paths, db) = common::test_database("ordering-compaction")?;
    let db = Arc::new(db);

    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let collection_repo = SqliteCollectionRepository::new(db.clone());
    let folder_repo = SqliteFolderRepository::new(db.clone());
    let request_repo = SqliteRequestRepository::new(db.clone());

    let workspace_a = workspace_repo.create("Workspace A")?;
    let workspace_b = workspace_repo.create("Workspace B")?;

    let collection_a = collection_repo.create(workspace_a.id, "Collection A")?;
    let collection_b = collection_repo.create(workspace_a.id, "Collection B")?;
    let collection_c = collection_repo.create(workspace_a.id, "Collection C")?;

    collection_repo.move_to_workspace(collection_a.id, workspace_b.id)?;
    let after_move = collection_repo.list_by_workspace(workspace_a.id)?;
    assert_eq!(
        after_move
            .iter()
            .map(|collection| collection.sort_order)
            .collect::<Vec<_>>(),
        vec![0, 1],
        "collection sort orders should compact after move-out"
    );

    collection_repo.delete(collection_b.id)?;
    let after_delete = collection_repo.list_by_workspace(workspace_a.id)?;
    assert_eq!(after_delete.len(), 1);
    assert_eq!(after_delete[0].id, collection_c.id);
    assert_eq!(after_delete[0].sort_order, 0);

    let folder = folder_repo.create(collection_c.id, None, "Folder")?;
    let request_a = request_repo.create(collection_c.id, None, "A", "GET", "https://a.test")?;
    let request_b = request_repo.create(collection_c.id, None, "B", "GET", "https://b.test")?;
    let request_c = request_repo.create(collection_c.id, None, "C", "GET", "https://c.test")?;

    request_repo.move_to(request_a.id, collection_c.id, Some(folder.id))?;
    let root_requests = request_repo
        .list_by_collection(collection_c.id)?
        .into_iter()
        .filter(|request| request.parent_folder_id.is_none())
        .collect::<Vec<_>>();
    assert_eq!(
        root_requests
            .iter()
            .map(|request| (request.id, request.sort_order))
            .collect::<Vec<_>>(),
        vec![(request_b.id, 0), (request_c.id, 1)],
        "request sort orders should compact after move-out"
    );

    request_repo.delete(request_b.id)?;
    let root_requests_after_delete = request_repo
        .list_by_collection(collection_c.id)?
        .into_iter()
        .filter(|request| request.parent_folder_id.is_none())
        .collect::<Vec<_>>();
    assert_eq!(root_requests_after_delete.len(), 1);
    assert_eq!(root_requests_after_delete[0].id, request_c.id);
    assert_eq!(root_requests_after_delete[0].sort_order, 0);

    Ok(())
}
