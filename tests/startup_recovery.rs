mod common;

use std::sync::Arc;

use anyhow::Result;
use gpui_starter::{
    infra::blobs::BlobStore,
    repos::{
        history_repo::{HistoryRepository, SqliteHistoryRepository},
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

    let pending = history_repo.create_pending(workspace.id, None, "GET", "https://pending.local")?;
    let referenced_blob = blob_store.write_bytes(b"referenced-blob", Some("text/plain"))?;
    let completed = history_repo.create_pending(workspace.id, None, "GET", "https://ok.local")?;
    history_repo.finalize_completed(
        completed.id,
        200,
        Some(&referenced_blob.hash),
        Some(referenced_blob.size_bytes as i64),
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
