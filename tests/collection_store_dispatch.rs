mod common;

use std::sync::Arc;

use anyhow::Result;
use torii::{
    domain::collection::{CollectionStorageConfig, CollectionStorageKind},
    repos::{
        collection_repo::{CollectionRepoRef, CollectionRepository, SqliteCollectionRepository},
        workspace_repo::{SqliteWorkspaceRepository, WorkspaceRepository},
    },
    services::collection_store::{CollectionStoreResolver, ResolvedCollectionStore},
};

#[test]
fn collection_store_resolves_managed_and_linked_by_collection_id() -> Result<()> {
    let (_paths, db) = common::test_database("collection-store-dispatch")?;
    let db = Arc::new(db);

    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let collection_repo_impl = Arc::new(SqliteCollectionRepository::new(db.clone()));
    let collection_repo: CollectionRepoRef = collection_repo_impl.clone();

    let workspace = workspace_repo.create("Main")?;
    let managed = collection_repo_impl.create(workspace.id, "Managed")?;

    let linked_root = std::env::temp_dir().join(format!(
        "torii-linked-{}",
        uuid::Uuid::now_v7()
    ));
    std::fs::create_dir_all(&linked_root)?;
    let linked = collection_repo_impl.create_with_storage(
        workspace.id,
        "Linked",
        CollectionStorageKind::Linked,
        CollectionStorageConfig {
            linked_root_path: Some(linked_root.clone()),
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

    Ok(())
}
