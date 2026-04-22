mod common;

use std::sync::Arc;

use anyhow::Result;
use torii::{
    domain::collection::{CollectionStorageConfig, CollectionStorageKind},
    infra::linked_collection_format::{LinkedCollectionState, write_linked_collection},
    repos::{
        collection_repo::{CollectionRepoRef, CollectionRepository, SqliteCollectionRepository},
        environment_repo::{EnvironmentRepoRef, SqliteEnvironmentRepository},
        request_repo::{RequestRepoRef, SqliteRequestRepository},
        workspace_repo::{SqliteWorkspaceRepository, WorkspaceRepository},
    },
    services::collection_store::{
        CollectionStoreRepos, CollectionStoreResolver, ResolvedCollectionStore,
    },
};

#[test]
fn collection_store_resolves_managed_and_linked_by_collection_id() -> Result<()> {
    let (_paths, db) = common::test_database("collection-store-dispatch")?;
    let db = Arc::new(db);

    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let collection_repo_impl = Arc::new(SqliteCollectionRepository::new(db.clone()));
    let collection_repo: CollectionRepoRef = collection_repo_impl.clone();
    let request_repo: RequestRepoRef = Arc::new(SqliteRequestRepository::new(db.clone()));
    let environment_repo: EnvironmentRepoRef =
        Arc::new(SqliteEnvironmentRepository::new(db.clone()));
    let store_repos = CollectionStoreRepos {
        requests: request_repo.clone(),
        environments: environment_repo.clone(),
    };

    let workspace = workspace_repo.create("Main")?;
    let managed = collection_repo_impl.create(workspace.id, "Managed")?;

    let linked_root = std::env::temp_dir().join(format!("torii-linked-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&linked_root)?;
    let linked = collection_repo_impl.create_with_storage(
        workspace.id,
        "Linked",
        CollectionStorageKind::Linked,
        CollectionStorageConfig {
            linked_root_path: Some(linked_root.clone()),
        },
    )?;
    let linked_collection = collection_repo_impl
        .get(linked.id)?
        .expect("linked collection must exist");
    write_linked_collection(
        &linked_root,
        &LinkedCollectionState {
            collection: linked_collection,
            folders: Vec::new(),
            requests: Vec::new(),
            environments: Vec::new(),
            root_child_order: Vec::new(),
            folder_child_orders: std::collections::HashMap::new(),
        },
    )?;

    let resolver = CollectionStoreResolver::new(collection_repo);

    let resolved_managed = resolver.resolve(managed.id)?;
    let resolved_linked = resolver.resolve(linked.id)?;

    assert!(matches!(
        resolved_managed,
        ResolvedCollectionStore::Managed(_)
    ));
    match resolved_linked {
        ResolvedCollectionStore::Linked(linked_store) => {
            assert_eq!(linked_store.root_path, linked_root);
        }
        ResolvedCollectionStore::Managed(_) => {
            panic!("expected linked store for linked collection");
        }
    }

    let managed_store = resolver.resolve(managed.id)?;
    let linked_store = resolver.resolve(linked.id)?;

    let _managed_request =
        managed_store.create_request(&store_repos, None, "Managed Request", "GET", "/managed")?;
    let _managed_env = managed_store.create_environment(&store_repos, "Managed Env")?;

    let _linked_request =
        linked_store.create_request(&store_repos, None, "Linked Request", "GET", "/linked")?;
    let _linked_env = linked_store.create_environment(&store_repos, "Linked Env")?;

    // Managed collection writes should stay in SQLite tables.
    let managed_request_count: i64 = db.block_on(async {
        sqlx::query_scalar("SELECT COUNT(*) FROM requests WHERE collection_id = ?")
            .bind(managed.id.to_string())
            .fetch_one(db.pool())
            .await
    })?;
    assert_eq!(managed_request_count, 1);
    let managed_env_count: i64 = db.block_on(async {
        sqlx::query_scalar("SELECT COUNT(*) FROM environments WHERE workspace_id = ? AND name = ?")
            .bind(workspace.id.to_string())
            .bind("Managed Env")
            .fetch_one(db.pool())
            .await
    })?;
    assert_eq!(managed_env_count, 1);

    // Linked collection writes should not touch managed SQLite request/environment rows.
    let linked_request_count: i64 = db.block_on(async {
        sqlx::query_scalar("SELECT COUNT(*) FROM requests WHERE collection_id = ?")
            .bind(linked.id.to_string())
            .fetch_one(db.pool())
            .await
    })?;
    assert_eq!(linked_request_count, 0);
    let linked_env_count: i64 = db.block_on(async {
        sqlx::query_scalar("SELECT COUNT(*) FROM environments WHERE workspace_id = ? AND name = ?")
            .bind(workspace.id.to_string())
            .bind("Linked Env")
            .fetch_one(db.pool())
            .await
    })?;
    assert_eq!(linked_env_count, 0);

    // Linked collection writes should be persisted on disk.
    let control_path = linked_root.join(".torii").join("collection.json");
    assert!(control_path.exists());
    let linked_request_files = std::fs::read_dir(&linked_root)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .file_name()
                .and_then(|it| it.to_str())
                .is_some_and(|name| name.ends_with(".request.json"))
        })
        .count();
    assert_eq!(linked_request_files, 1);
    let linked_env_files = std::fs::read_dir(&linked_root)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .file_name()
                .and_then(|it| it.to_str())
                .is_some_and(|name| name.ends_with(".env.json"))
        })
        .count();
    assert_eq!(linked_env_files, 1);

    Ok(())
}
