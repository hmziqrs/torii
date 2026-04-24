mod common;

use std::sync::Arc;

use anyhow::Result;
use torii::repos::{
    collection_repo::{CollectionRepository, SqliteCollectionRepository},
    environment_repo::{EnvironmentRepository, SqliteEnvironmentRepository},
    folder_repo::{FolderRepository, SqliteFolderRepository},
    request_repo::{RequestRepository, SqliteRequestRepository},
    workspace_repo::{SqliteWorkspaceRepository, WorkspaceRepository},
};

#[test]
fn get_by_id_returns_correct_item_or_none() -> Result<()> {
    let (_paths, db) = common::test_database("get-by-id")?;
    let db = Arc::new(db);

    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let collection_repo = SqliteCollectionRepository::new(db.clone());
    let folder_repo = SqliteFolderRepository::new(db.clone());
    let request_repo = SqliteRequestRepository::new(db.clone());
    let environment_repo = SqliteEnvironmentRepository::new(db.clone());

    let workspace = workspace_repo.create("W")?;
    let collection = collection_repo.create(workspace.id, "C")?;
    let folder = folder_repo.create(collection.id, None, "F")?;
    let request = request_repo.create(collection.id, None, "R", "GET", "https://r.test")?;
    let environment = environment_repo.create(workspace.id, "Env")?;

    assert_eq!(
        collection_repo.get(collection.id)?.map(|c| c.id),
        Some(collection.id)
    );
    assert_eq!(folder_repo.get(folder.id)?.map(|f| f.id), Some(folder.id));
    assert_eq!(
        request_repo.get(request.id)?.map(|r| r.id),
        Some(request.id)
    );
    assert_eq!(
        environment_repo.get(environment.id)?.map(|e| e.id),
        Some(environment.id)
    );

    let missing_collection = torii::domain::ids::CollectionId::new();
    let missing_folder = torii::domain::ids::FolderId::new();
    let missing_request = torii::domain::ids::RequestId::new();
    let missing_environment = torii::domain::ids::EnvironmentId::new();
    assert!(collection_repo.get(missing_collection)?.is_none());
    assert!(folder_repo.get(missing_folder)?.is_none());
    assert!(request_repo.get(missing_request)?.is_none());
    assert!(environment_repo.get(missing_environment)?.is_none());

    Ok(())
}

#[test]
fn workspace_rename_persists_and_bumps_revision() -> Result<()> {
    let (_paths, db) = common::test_database("workspace-rename")?;
    let db = Arc::new(db);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());

    let workspace = workspace_repo.create("Original")?;
    assert_eq!(workspace.meta.revision, 1);

    workspace_repo.rename(workspace.id, "Renamed")?;

    let updated = workspace_repo
        .get(workspace.id)?
        .expect("workspace must still exist");
    assert_eq!(updated.name, "Renamed");
    assert_eq!(updated.meta.revision, 2);
    assert_eq!(updated.meta.created_at, workspace.meta.created_at);

    Ok(())
}

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

#[test]
fn mixed_sibling_sort_order_is_shared_across_folder_and_request_mutations() -> Result<()> {
    let (_paths, db) = common::test_database("mixed-sibling-sort-order")?;
    let db = Arc::new(db);

    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let collection_repo = SqliteCollectionRepository::new(db.clone());
    let folder_repo = SqliteFolderRepository::new(db.clone());
    let request_repo = SqliteRequestRepository::new(db.clone());

    let workspace = workspace_repo.create("Workspace")?;
    let collection = collection_repo.create(workspace.id, "Collection")?;

    let root_request_a =
        request_repo.create(collection.id, None, "Root A", "GET", "https://a.test")?;
    let root_folder = folder_repo.create(collection.id, None, "Root Folder")?;
    let root_request_b =
        request_repo.create(collection.id, None, "Root B", "GET", "https://b.test")?;

    assert_eq!(root_request_a.sort_order, 0);
    assert_eq!(root_folder.sort_order, 1);
    assert_eq!(root_request_b.sort_order, 2);

    let target_parent = folder_repo.create(collection.id, None, "Target Parent")?;
    let nested_folder = folder_repo.create(collection.id, Some(target_parent.id), "Nested")?;
    let nested_request = request_repo.create(
        collection.id,
        Some(target_parent.id),
        "Nested Request",
        "GET",
        "https://nested.test",
    )?;
    assert_eq!(nested_folder.sort_order, 0);
    assert_eq!(nested_request.sort_order, 1);

    request_repo.move_to(root_request_b.id, collection.id, Some(target_parent.id))?;
    let moved_request = request_repo
        .get(root_request_b.id)?
        .expect("moved request should exist");
    assert_eq!(
        moved_request.sort_order, 2,
        "moved request should append after existing folder+request siblings"
    );

    folder_repo.move_to(root_folder.id, collection.id, Some(target_parent.id))?;
    let moved_folder = folder_repo
        .get(root_folder.id)?
        .expect("moved folder should exist");
    assert_eq!(
        moved_folder.sort_order, 3,
        "moved folder should append after existing mixed siblings"
    );

    Ok(())
}

#[test]
fn deleting_collection_cascades_descendants_without_dangling_references() -> Result<()> {
    let (_paths, db) = common::test_database("delete-collection-cascade-dangling")?;
    let db = Arc::new(db);

    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let collection_repo = SqliteCollectionRepository::new(db.clone());
    let folder_repo = SqliteFolderRepository::new(db.clone());
    let request_repo = SqliteRequestRepository::new(db.clone());

    let workspace = workspace_repo.create("Workspace")?;
    let collection_a = collection_repo.create(workspace.id, "Collection A")?;
    let collection_b = collection_repo.create(workspace.id, "Collection B")?;

    let folder_root = folder_repo.create(collection_a.id, None, "Root")?;
    let folder_child = folder_repo.create(collection_a.id, Some(folder_root.id), "Child")?;
    let request_a = request_repo.create(
        collection_a.id,
        Some(folder_root.id),
        "Req A",
        "GET",
        "https://a.test",
    )?;
    let request_b = request_repo.create(
        collection_a.id,
        Some(folder_child.id),
        "Req B",
        "GET",
        "https://b.test",
    )?;

    let keep_folder = folder_repo.create(collection_b.id, None, "Keep")?;
    let keep_request = request_repo.create(
        collection_b.id,
        Some(keep_folder.id),
        "Keep Req",
        "GET",
        "https://keep.test",
    )?;

    collection_repo.delete(collection_a.id)?;

    assert!(folder_repo.get(folder_root.id)?.is_none());
    assert!(folder_repo.get(folder_child.id)?.is_none());
    assert!(request_repo.get(request_a.id)?.is_none());
    assert!(request_repo.get(request_b.id)?.is_none());

    assert!(folder_repo.get(keep_folder.id)?.is_some());
    assert!(request_repo.get(keep_request.id)?.is_some());

    let dangling_folder_collection: i64 = db.block_on(async {
        sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM folders f
             LEFT JOIN collections c ON c.id = f.collection_id
             WHERE c.id IS NULL",
        )
        .fetch_one(db.pool())
        .await
    })?;
    assert_eq!(dangling_folder_collection, 0);

    let dangling_folder_parent: i64 = db.block_on(async {
        sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM folders child
             LEFT JOIN folders parent ON parent.id = child.parent_folder_id
             WHERE child.parent_folder_id IS NOT NULL AND parent.id IS NULL",
        )
        .fetch_one(db.pool())
        .await
    })?;
    assert_eq!(dangling_folder_parent, 0);

    let dangling_request_collection: i64 = db.block_on(async {
        sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM requests r
             LEFT JOIN collections c ON c.id = r.collection_id
             WHERE c.id IS NULL",
        )
        .fetch_one(db.pool())
        .await
    })?;
    assert_eq!(dangling_request_collection, 0);

    let dangling_request_parent: i64 = db.block_on(async {
        sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM requests r
             LEFT JOIN folders f ON f.id = r.parent_folder_id
             WHERE r.parent_folder_id IS NOT NULL AND f.id IS NULL",
        )
        .fetch_one(db.pool())
        .await
    })?;
    assert_eq!(dangling_request_parent, 0);

    Ok(())
}
