mod common;

use std::sync::Arc;

use anyhow::Result;
use torii::{
    infra::blobs::BlobStore,
    repos::{
        collection_repo::{CollectionRepository, SqliteCollectionRepository},
        history_repo::{HistoryRepository, SqliteHistoryRepository},
        request_repo::{RequestRepository, SqliteRequestRepository},
        workspace_repo::{SqliteWorkspaceRepository, WorkspaceRepository},
    },
    services::recovery::RecoveryCoordinator,
};

#[test]
fn startup_recovery_reconciles_pending_history_and_orphans() -> Result<()> {
    let (paths, db) = common::test_database("startup-recovery")?;
    let db = Arc::new(db);
    let blob_store = Arc::new(BlobStore::new(&paths)?);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let history_repo = Arc::new(SqliteHistoryRepository::new(db.clone()));

    let workspace = workspace_repo.create("Main")?;

    let pending =
        history_repo.create_pending(workspace.id, None, "GET", "https://pending.local", None)?;
    let referenced_blob = blob_store.write_bytes(b"referenced-blob", Some("text/plain"))?;
    let completed =
        history_repo.create_pending(workspace.id, None, "GET", "https://ok.local", None)?;
    history_repo.finalize_completed(
        completed.id,
        200,
        Some(&referenced_blob.hash),
        Some(referenced_blob.size_bytes as i64),
        None,
        None,
        None,
        None,
        None,
    )?;

    let orphan_blob = blob_store.write_bytes(b"orphan-blob", Some("text/plain"))?;
    assert!(orphan_blob.path.exists());
    assert!(referenced_blob.path.exists());

    let temp_path = paths.blobs_temp_dir().join("stale.temp");
    std::fs::write(&temp_path, b"temp bytes")?;
    assert!(temp_path.exists());

    let recovery = RecoveryCoordinator::new(db.clone(), history_repo.clone(), blob_store.clone())
        .with_stale_temp_max_age(std::time::Duration::ZERO);
    let report = recovery.run_startup_recovery()?;

    assert!(report.pending_history_failed >= 1);
    assert!(report.orphan_blob_removed >= 1);
    assert!(report.stale_temp_removed >= 1);

    let recent = history_repo.list_recent(workspace.id, 10)?;
    let recovered = recent
        .iter()
        .find(|entry| entry.id == pending.id)
        .expect("pending entry must still exist");
    assert_eq!(recovered.state.as_str(), "failed");

    assert!(!orphan_blob.path.exists(), "orphan blob should be deleted");
    assert!(referenced_blob.path.exists(), "referenced blob must remain");
    assert!(!temp_path.exists(), "stale temp file should be removed");

    Ok(())
}

#[test]
fn startup_recovery_preserves_request_body_blobs() -> Result<()> {
    let (paths, db) = common::test_database("startup-recovery-request-blobs")?;
    let db = Arc::new(db);
    let blob_store = Arc::new(BlobStore::new(&paths)?);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let collection_repo = SqliteCollectionRepository::new(db.clone());
    let request_repo = SqliteRequestRepository::new(db.clone());
    let history_repo = Arc::new(SqliteHistoryRepository::new(db.clone()));

    let workspace = workspace_repo.create("Main")?;
    let collection = collection_repo.create(workspace.id, "Collection")?;
    let request = request_repo.create(
        collection.id,
        None,
        "Request",
        "POST",
        "https://request-body.local",
    )?;

    let request_blob = blob_store.write_bytes(b"request-body", Some("text/plain"))?;
    db.block_on(async {
        sqlx::query("UPDATE requests SET body_blob_hash = ? WHERE id = ?")
            .bind(&request_blob.hash)
            .bind(request.id.to_string())
            .execute(db.pool())
            .await
    })?;

    let orphan_blob = blob_store.write_bytes(b"orphan", Some("text/plain"))?;
    let recovery = RecoveryCoordinator::new(db.clone(), history_repo, blob_store.clone())
        .with_stale_temp_max_age(std::time::Duration::ZERO);
    let report = recovery.run_startup_recovery()?;

    assert!(
        request_blob.path.exists(),
        "request body blob should be preserved by recovery"
    );
    assert!(
        !orphan_blob.path.exists(),
        "unreferenced blob should still be removed"
    );
    assert!(report.orphan_blob_removed >= 1);

    Ok(())
}

#[test]
fn startup_recovery_marks_stale_pending_for_request_as_failed() -> Result<()> {
    // Simulate an interrupted send: a pending history row exists for a
    // persisted request. After restart, recovery marks it failed and
    // latest-run restore remains usable.
    let (paths, db) = common::test_database("startup-recovery-stale-pending")?;
    let db = Arc::new(db);
    let blob_store = Arc::new(BlobStore::new(&paths)?);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let collection_repo = SqliteCollectionRepository::new(db.clone());
    let request_repo = SqliteRequestRepository::new(db.clone());
    let history_repo = Arc::new(SqliteHistoryRepository::new(db.clone()));

    let workspace = workspace_repo.create("Main")?;
    let collection = collection_repo.create(workspace.id, "Collection")?;
    let request = request_repo.create(
        collection.id,
        None,
        "Interrupted Request",
        "POST",
        "https://api.test/slow",
    )?;

    // Create a stale pending row (simulates interrupted send)
    let pending = history_repo.create_pending(
        workspace.id,
        Some(request.id),
        "POST",
        "https://api.test/slow",
        None,
    )?;

    // Also create a previously completed run for the same request
    let completed = history_repo.create_pending(
        workspace.id,
        Some(request.id),
        "POST",
        "https://api.test/slow",
        None,
    )?;
    history_repo.finalize_completed(completed.id, 200, None, None, None, None, None, None, None)?;

    // Run recovery
    let recovery = RecoveryCoordinator::new(db.clone(), history_repo.clone(), blob_store)
        .with_stale_temp_max_age(std::time::Duration::ZERO);
    let report = recovery.run_startup_recovery()?;
    assert!(report.pending_history_failed >= 1);

    // The stale pending row is now failed
    let recent = history_repo.list_recent(workspace.id, 10)?;
    let recovered = recent
        .iter()
        .find(|e| e.id == pending.id)
        .expect("pending entry must still exist");
    assert_eq!(recovered.state.as_str(), "failed");

    // The completed row is untouched
    let completed_entry = recent
        .iter()
        .find(|e| e.id == completed.id)
        .expect("completed entry must still exist");
    assert_eq!(completed_entry.state.as_str(), "completed");

    // get_latest_for_request should still return a usable entry
    let latest = history_repo.get_latest_for_request(request.id)?;
    assert!(
        latest.is_some(),
        "latest-for-request should return an entry"
    );

    Ok(())
}

#[test]
fn recovery_preserves_phase5_history_blob_refs() -> Result<()> {
    let (paths, db) = common::test_database("startup-recovery-history-blob-refs")?;
    let db = Arc::new(db);
    let blob_store = Arc::new(BlobStore::new(&paths)?);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let history_repo = Arc::new(SqliteHistoryRepository::new(db.clone()));

    let workspace = workspace_repo.create("Main")?;
    let entry =
        history_repo.create_pending(workspace.id, None, "GET", "https://phase5.local", None)?;

    let transcript_blob = blob_store.write_bytes(b"stream transcript", Some("application/json"))?;
    db.block_on(async {
        sqlx::query(
            "INSERT INTO history_blob_refs (history_id, blob_hash, ref_kind, created_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(entry.id.to_string())
        .bind(&transcript_blob.hash)
        .bind("stream_transcript")
        .bind(time::OffsetDateTime::now_utc().unix_timestamp())
        .execute(db.pool())
        .await
    })?;

    let orphan_blob = blob_store.write_bytes(b"orphan", Some("text/plain"))?;
    let recovery = RecoveryCoordinator::new(db.clone(), history_repo, blob_store.clone())
        .with_stale_temp_max_age(std::time::Duration::ZERO);
    let report = recovery.run_startup_recovery()?;

    assert!(
        transcript_blob.path.exists(),
        "history_blob_refs references must be preserved"
    );
    assert!(
        !orphan_blob.path.exists(),
        "unreferenced blob should be cleaned up"
    );
    assert!(report.orphan_blob_removed >= 1);

    Ok(())
}
