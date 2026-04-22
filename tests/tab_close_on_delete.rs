mod common;

use std::sync::Arc;

use anyhow::Result;
use torii::{
    repos::{
        collection_repo::{CollectionRepoRef, CollectionRepository, SqliteCollectionRepository},
        environment_repo::{
            EnvironmentRepoRef, EnvironmentRepository, SqliteEnvironmentRepository,
        },
        folder_repo::{FolderRepoRef, FolderRepository, SqliteFolderRepository},
        request_repo::{RequestRepoRef, RequestRepository, SqliteRequestRepository},
        workspace_repo::{SqliteWorkspaceRepository, WorkspaceRepoRef, WorkspaceRepository},
    },
    services::workspace_tree::load_workspace_catalog,
    session::{
        item_key::{ItemKey, TabKey},
        tab_manager::TabManager,
    },
};

#[test]
fn deleting_collection_closes_collection_and_descendant_tabs() -> Result<()> {
    let (_paths, db) = common::test_database("tab-close-delete-collection")?;
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
    let collection = collection_repo_impl.create(workspace.id, "Collection")?;
    let folder = folder_repo_impl.create(collection.id, None, "Folder")?;
    let request = request_repo_impl.create(
        collection.id,
        Some(folder.id),
        "Request",
        "GET",
        "https://example.test",
    )?;
    let environment = environment_repo_impl.create(workspace.id, "Env")?;

    let catalog = load_workspace_catalog(
        &workspace_repo,
        &collection_repo,
        &folder_repo,
        &request_repo,
        &environment_repo,
        Some(workspace.id),
    )?;

    let mut manager = TabManager::default();
    manager.open_or_focus(ItemKey::collection(collection.id));
    manager.open_or_focus(ItemKey::folder(folder.id));
    manager.open_or_focus(ItemKey::request(request.id));
    manager.open_or_focus(ItemKey::environment(environment.id));

    let close_keys = catalog
        .delete_closure(ItemKey::collection(collection.id))
        .into_iter()
        .map(TabKey::from)
        .collect::<Vec<_>>();
    assert_eq!(manager.close_all(&close_keys), 3);
    assert_eq!(manager.tabs().len(), 1);
    assert_eq!(
        manager.tabs()[0].key,
        TabKey::from(ItemKey::environment(environment.id))
    );

    Ok(())
}

#[test]
fn deleting_workspace_closes_all_workspace_descendant_tabs() -> Result<()> {
    let (_paths, db) = common::test_database("tab-close-delete-workspace")?;
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
    let collection = collection_repo_impl.create(workspace.id, "Collection")?;
    let request = request_repo_impl.create(
        collection.id,
        None,
        "Request",
        "GET",
        "https://example.test",
    )?;
    let environment = environment_repo_impl.create(workspace.id, "Env")?;

    let catalog = load_workspace_catalog(
        &workspace_repo,
        &collection_repo,
        &folder_repo,
        &request_repo,
        &environment_repo,
        Some(workspace.id),
    )?;

    let mut manager = TabManager::default();
    manager.open_or_focus(ItemKey::workspace(workspace.id));
    manager.open_or_focus(ItemKey::collection(collection.id));
    manager.open_or_focus(ItemKey::request(request.id));
    manager.open_or_focus(ItemKey::environment(environment.id));
    manager.open_or_focus(ItemKey::settings());

    let close_keys = catalog
        .delete_closure(ItemKey::workspace(workspace.id))
        .into_iter()
        .map(TabKey::from)
        .collect::<Vec<_>>();
    assert_eq!(manager.close_all(&close_keys), 4);
    assert_eq!(manager.tabs().len(), 1);
    assert_eq!(manager.tabs()[0].key, TabKey::from(ItemKey::settings()));

    Ok(())
}
