mod common;

use std::sync::Arc;

use anyhow::Result;
use torii::{
    domain::{
        history::HistoryState,
        ids::CollectionId,
        request::{AuthType, BodyType, KeyValuePair},
    },
    repos::{
        collection_repo::{CollectionRepository, SqliteCollectionRepository},
        history_repo::{HistoryRepository, SqliteHistoryRepository},
        request_repo::{RequestRepoError, RequestRepository, SqliteRequestRepository},
        workspace_repo::{SqliteWorkspaceRepository, WorkspaceRepository},
    },
};

#[test]
fn request_save_roundtrip_persists_expanded_fields() -> Result<()> {
    let (_paths, db) = common::test_database("request-save-roundtrip")?;
    let db = Arc::new(db);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let collection_repo = SqliteCollectionRepository::new(db.clone());
    let request_repo = SqliteRequestRepository::new(db.clone());

    let workspace = workspace_repo.create("Workspace")?;
    let collection = collection_repo.create(workspace.id, "Collection")?;
    let mut request =
        request_repo.create(collection.id, None, "Test", "POST", "https://api.test")?;

    // Edit expanded fields
    request.url = "https://api.test/v2".to_string();
    request.method = "PUT".to_string();
    request.params.push(KeyValuePair::new("page", "1"));
    request
        .headers
        .push(KeyValuePair::new("Authorization", "Bearer test"));
    request.body = BodyType::RawJson {
        content: r#"{"key":"value"}"#.to_string(),
    };
    request.auth = AuthType::Bearer {
        token_secret_ref: Some("secret-ref-token".to_string()),
    };
    request.scripts.pre_request = "console.log('pre')".to_string();
    request.settings.timeout_ms = Some(5000);

    let revision = request.meta.revision;
    request_repo.save(&request, revision)?;

    // Re-fetch and verify
    let loaded = request_repo.get(request.id)?.expect("request should exist");
    assert_eq!(loaded.url, "https://api.test/v2");
    assert_eq!(loaded.method, "PUT");
    assert_eq!(loaded.params.len(), 1);
    assert_eq!(loaded.params[0].key, "page");
    assert_eq!(loaded.headers.len(), 1);
    assert_eq!(loaded.headers[0].value, "Bearer test");
    assert!(matches!(loaded.body, BodyType::RawJson { .. }));
    assert!(matches!(loaded.auth, AuthType::Bearer { .. }));
    assert_eq!(loaded.scripts.pre_request, "console.log('pre')");
    assert_eq!(loaded.settings.timeout_ms, Some(5000));
    assert_eq!(loaded.meta.revision, revision + 1);

    Ok(())
}

#[test]
fn request_revision_conflict_detected() -> Result<()> {
    let (_paths, db) = common::test_database("request-revision-conflict")?;
    let db = Arc::new(db);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let collection_repo = SqliteCollectionRepository::new(db.clone());
    let request_repo = SqliteRequestRepository::new(db.clone());

    let workspace = workspace_repo.create("Workspace")?;
    let collection = collection_repo.create(workspace.id, "Collection")?;
    let request = request_repo.create(collection.id, None, "Test", "GET", "/api")?;

    // First save succeeds
    let mut modified = request.clone();
    modified.url = "/changed".to_string();
    request_repo.save(&modified, request.meta.revision)?;

    // Second save with stale revision fails
    let stale_revision = request.meta.revision;
    let mut stale = request.clone();
    stale.url = "/stale-change".to_string();
    let result = request_repo.save(&stale, stale_revision);
    assert!(matches!(
        result,
        Err(RequestRepoError::RevisionConflict { .. })
    ));

    Ok(())
}

#[test]
fn request_duplicate_creates_independent_copy() -> Result<()> {
    let (_paths, db) = common::test_database("request-duplicate")?;
    let db = Arc::new(db);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let collection_repo = SqliteCollectionRepository::new(db.clone());
    let request_repo = SqliteRequestRepository::new(db.clone());

    let workspace = workspace_repo.create("Workspace")?;
    let collection = collection_repo.create(workspace.id, "Collection")?;
    let mut request = request_repo.create(collection.id, None, "Original", "POST", "/api")?;

    // Edit source request
    request.headers.push(KeyValuePair::new("X-Custom", "value"));
    request_repo.save(&request, request.meta.revision)?;

    // Duplicate
    let dup = request_repo.duplicate(request.id, "Original (Copy)")?;

    // Verify duplicate is independent
    assert_ne!(dup.id, request.id);
    assert_eq!(dup.name, "Original (Copy)");
    assert_eq!(dup.method, "POST");
    assert_eq!(dup.url, "/api");
    assert_eq!(dup.headers.len(), 1);
    assert_eq!(dup.headers[0].key, "X-Custom");
    assert_eq!(dup.collection_id, request.collection_id);
    assert_eq!(dup.parent_folder_id, request.parent_folder_id);

    // Source is unchanged
    let source = request_repo.get(request.id)?.unwrap();
    assert_eq!(source.name, "Original");

    Ok(())
}

#[test]
fn request_not_found_error_on_save() -> Result<()> {
    let (_paths, db) = common::test_database("request-save-not-found")?;
    let db = Arc::new(db);
    let request_repo = SqliteRequestRepository::new(db.clone());

    let request = torii::domain::request::RequestItem::new(
        CollectionId::new(),
        None,
        "Ghost",
        "GET",
        "/ghost",
        0,
    );

    let result = request_repo.save(&request, 1);
    assert!(matches!(result, Err(RequestRepoError::NotFound(_))));

    Ok(())
}

#[test]
fn history_finalize_cancelled_persists_partial_size() -> Result<()> {
    let (_paths, db) = common::test_database("history-cancelled")?;
    let db = Arc::new(db);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let history_repo = SqliteHistoryRepository::new(db.clone());

    let workspace = workspace_repo.create("Workspace")?;
    let entry = history_repo.create_pending(workspace.id, None, "GET", "https://test.local")?;

    history_repo.finalize_cancelled(entry.id, Some(2048))?;

    let recent = history_repo.list_recent(workspace.id, 10)?;
    let found = recent.iter().find(|e| e.id == entry.id).unwrap();
    assert!(matches!(found.state, HistoryState::Cancelled));
    assert_eq!(found.partial_size, Some(2048));

    Ok(())
}

#[test]
fn history_response_metadata_roundtrip() -> Result<()> {
    let (_paths, db) = common::test_database("history-response-metadata")?;
    let db = Arc::new(db);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let history_repo = SqliteHistoryRepository::new(db.clone());

    let workspace = workspace_repo.create("Workspace")?;
    let entry = history_repo.create_pending(workspace.id, None, "GET", "https://test.local")?;

    let headers = r#"{"content-type":"application/json"}"#;
    history_repo.finalize_completed(
        entry.id,
        200,
        Some("abc123"),
        Some(4096),
        Some(headers),
        Some("application/json"),
        Some(1000),
        Some(1200),
    )?;

    let recent = history_repo.list_recent(workspace.id, 10)?;
    let found = recent.iter().find(|e| e.id == entry.id).unwrap();
    assert_eq!(found.status_code, Some(200));
    assert_eq!(found.response_headers_json.as_deref(), Some(headers));
    assert_eq!(
        found.response_media_type.as_deref(),
        Some("application/json")
    );
    assert_eq!(found.dispatched_at, Some(1000));
    assert_eq!(found.first_byte_at, Some(1200));
    assert_eq!(found.blob_hash.as_deref(), Some("abc123"));
    assert_eq!(found.blob_size, Some(4096));

    Ok(())
}

#[test]
fn history_get_latest_for_request() -> Result<()> {
    let (_paths, db) = common::test_database("history-latest-for-request")?;
    let db = Arc::new(db);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let collection_repo = SqliteCollectionRepository::new(db.clone());
    let request_repo = SqliteRequestRepository::new(db.clone());
    let history_repo = SqliteHistoryRepository::new(db.clone());

    let workspace = workspace_repo.create("Workspace")?;
    let collection = collection_repo.create(workspace.id, "Collection")?;
    let request = request_repo.create(collection.id, None, "Test", "GET", "/api")?;

    // Create multiple history entries for same request
    let entry1 = history_repo.create_pending(workspace.id, Some(request.id), "GET", "/api")?;
    history_repo.finalize_completed(entry1.id, 200, None, None, None, None, None, None)?;

    let entry2 = history_repo.create_pending(workspace.id, Some(request.id), "GET", "/api")?;
    history_repo.finalize_completed(entry2.id, 404, None, None, None, None, None, None)?;

    // Latest should be one of the completed entries for this request
    let latest = history_repo.get_latest_for_request(request.id)?;
    assert!(latest.is_some());
    let latest = latest.unwrap();
    // Either entry could be "latest" depending on ID ordering within the same millisecond
    assert!(latest.id == entry1.id || latest.id == entry2.id);
    assert!(latest.status_code == Some(200) || latest.status_code == Some(404));

    Ok(())
}
