mod common;

use std::sync::Arc;

use anyhow::Result;
use torii::{
    domain::collection::{CollectionStorageConfig, CollectionStorageKind},
    repos::{
        collection_repo::{CollectionRepoRef, CollectionRepository, SqliteCollectionRepository},
        environment_repo::{EnvironmentRepoRef, SqliteEnvironmentRepository},
        folder_repo::{FolderRepoRef, SqliteFolderRepository},
        request_repo::{RequestRepoRef, SqliteRequestRepository},
        workspace_repo::{SqliteWorkspaceRepository, WorkspaceRepoRef, WorkspaceRepository},
    },
    services::workspace_tree::{LinkedCollectionHealth, load_workspace_catalog},
};

#[test]
fn linked_collection_health_reports_healthy_missing_and_unavailable_roots() -> Result<()> {
    let (_paths, db) = common::test_database("workspace-tree-linked-health")?;
    let db = Arc::new(db);

    let workspace_repo_impl = Arc::new(SqliteWorkspaceRepository::new(db.clone()));
    let collection_repo_impl = Arc::new(SqliteCollectionRepository::new(db.clone()));
    let folder_repo_impl = Arc::new(SqliteFolderRepository::new(db.clone()));
    let request_repo_impl = Arc::new(SqliteRequestRepository::new(db.clone()));
    let environment_repo_impl = Arc::new(SqliteEnvironmentRepository::new(db.clone()));

    let workspace_repo: WorkspaceRepoRef = workspace_repo_impl.clone();
    let collection_repo: CollectionRepoRef = collection_repo_impl.clone();
    let folder_repo: FolderRepoRef = folder_repo_impl.clone();
    let request_repo: RequestRepoRef = request_repo_impl.clone();
    let environment_repo: EnvironmentRepoRef = environment_repo_impl.clone();

    let workspace = workspace_repo_impl.create("Main")?;

    let healthy_root = std::env::temp_dir().join(format!(
        "torii-linked-health-healthy-{}",
        uuid::Uuid::now_v7()
    ));
    std::fs::create_dir_all(&healthy_root)?;
    let unhealthy_root_file = std::env::temp_dir().join(format!(
        "torii-linked-health-unavailable-{}",
        uuid::Uuid::now_v7()
    ));
    std::fs::write(&unhealthy_root_file, b"not a directory")?;

    let healthy = collection_repo_impl.create_with_storage(
        workspace.id,
        "Healthy Linked",
        CollectionStorageKind::Linked,
        CollectionStorageConfig {
            linked_root_path: Some(healthy_root),
        },
    )?;
    let missing_root = collection_repo_impl.create_with_storage(
        workspace.id,
        "Missing Root Linked",
        CollectionStorageKind::Linked,
        CollectionStorageConfig {
            linked_root_path: None,
        },
    )?;
    let unavailable = collection_repo_impl.create_with_storage(
        workspace.id,
        "Unavailable Linked",
        CollectionStorageKind::Linked,
        CollectionStorageConfig {
            linked_root_path: Some(unhealthy_root_file),
        },
    )?;

    let catalog = load_workspace_catalog(
        &workspace_repo,
        &collection_repo,
        &folder_repo,
        &request_repo,
        &environment_repo,
        Some(workspace.id),
    )?;
    let selected = catalog
        .selected_workspace
        .as_ref()
        .expect("selected workspace should exist");

    let healthy_state = selected
        .collections
        .iter()
        .find(|row| row.collection.id == healthy.id)
        .and_then(|row| row.linked_health.clone());
    assert!(matches!(
        healthy_state,
        Some(LinkedCollectionHealth::Healthy)
    ));

    let missing_state = selected
        .collections
        .iter()
        .find(|row| row.collection.id == missing_root.id)
        .and_then(|row| row.linked_health.clone());
    assert!(matches!(
        missing_state,
        Some(LinkedCollectionHealth::MissingRootPath)
    ));

    let unavailable_state = selected
        .collections
        .iter()
        .find(|row| row.collection.id == unavailable.id)
        .and_then(|row| row.linked_health.clone());
    assert!(matches!(
        unavailable_state,
        Some(LinkedCollectionHealth::Unavailable { .. })
    ));

    Ok(())
}
