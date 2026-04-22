mod common;

use std::sync::Arc;

use anyhow::Result;
use torii::{
    domain::{request::BodyType, request::KeyValuePair, request::RequestItem},
    repos::{
        collection_repo::{CollectionRepository, SqliteCollectionRepository},
        environment_repo::{EnvironmentRepository, SqliteEnvironmentRepository},
        request_repo::{RequestRepoRef, SqliteRequestRepository},
        workspace_repo::{SqliteWorkspaceRepository, WorkspaceRepository},
    },
    services::request_draft::persist_new_draft_request,
};

#[test]
fn create_workspace_collection_environment_and_save_new_draft_request() -> Result<()> {
    let (_paths, db) = common::test_database("workspace-collection-environment-draft-save")?;
    let db = Arc::new(db);

    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let collection_repo = SqliteCollectionRepository::new(db.clone());
    let environment_repo = SqliteEnvironmentRepository::new(db.clone());
    let request_repo: RequestRepoRef = Arc::new(SqliteRequestRepository::new(db.clone()));

    let workspace = workspace_repo.create("Workspace A")?;
    let collection = collection_repo.create(workspace.id, "Collection A")?;
    let environment = environment_repo.create(collection.id, "Local")?;

    let mut draft = RequestItem::new(
        collection.id,
        None,
        "Draft Request",
        "POST",
        "https://api.example.test/items",
        0,
    );
    draft.headers.push(KeyValuePair::new("X-Test", "1"));
    draft.params.push(KeyValuePair::new("page", "2"));
    draft.body = BodyType::RawJson {
        content: r#"{"name":"draft-save"}"#.to_string(),
    };

    let saved = persist_new_draft_request(&request_repo, &draft)?;
    let loaded = request_repo
        .get(saved.id)?
        .expect("saved request should exist after first draft save");

    assert_eq!(workspace.name, "Workspace A");
    assert_eq!(collection.name, "Collection A");
    assert_eq!(environment.collection_id, collection.id);
    assert_eq!(loaded.collection_id, collection.id);
    assert_eq!(loaded.name, "Draft Request");
    assert_eq!(loaded.method, "POST");
    assert_eq!(loaded.url, "https://api.example.test/items");
    assert_eq!(loaded.params.len(), 1);
    assert_eq!(loaded.headers.len(), 1);
    assert!(matches!(loaded.body, BodyType::RawJson { .. }));

    Ok(())
}

#[test]
fn draft_save_targets_the_intended_collection() -> Result<()> {
    let (_paths, db) = common::test_database("draft-save-target-collection")?;
    let db = Arc::new(db);

    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let collection_repo = SqliteCollectionRepository::new(db.clone());
    let request_repo: RequestRepoRef = Arc::new(SqliteRequestRepository::new(db.clone()));

    let workspace = workspace_repo.create("Workspace B")?;
    let first = collection_repo.create(workspace.id, "Collection First")?;
    let second = collection_repo.create(workspace.id, "Collection Second")?;

    let draft = RequestItem::new(second.id, None, "Second Draft", "GET", "/v1/second", 0);
    let saved = persist_new_draft_request(&request_repo, &draft)?;
    let loaded = request_repo
        .get(saved.id)?
        .expect("saved request should exist for selected collection");

    assert_eq!(first.workspace_id, workspace.id);
    assert_eq!(second.workspace_id, workspace.id);
    assert_eq!(loaded.collection_id, second.id);
    assert_eq!(loaded.name, "Second Draft");
    assert_eq!(loaded.url, "/v1/second");

    Ok(())
}
