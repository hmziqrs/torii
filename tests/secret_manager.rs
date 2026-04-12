mod common;

use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use torii::{
    infra::secrets::{SecretStore, SecretStoreRef},
    repos::secret_ref_repo::{SecretRefRepository, SqliteSecretRefRepository},
    services::secret_manager::{SecretManager, SecretManagerError},
};

#[derive(Default)]
struct FailingPutSecretStore {
    values: Mutex<std::collections::HashMap<String, String>>,
}

impl SecretStore for FailingPutSecretStore {
    fn put_secret(&self, _key: &str, _value: &str) -> Result<()> {
        Err(anyhow!("forced put failure"))
    }

    fn get_secret(&self, key: &str) -> Result<Option<String>> {
        let values = self
            .values
            .lock()
            .map_err(|_| anyhow!("secret store mutex poisoned"))?;
        Ok(values.get(key).cloned())
    }

    fn delete_secret(&self, key: &str) -> Result<()> {
        let mut values = self
            .values
            .lock()
            .map_err(|_| anyhow!("secret store mutex poisoned"))?;
        values.remove(key);
        Ok(())
    }
}

#[test]
fn upsert_and_delete_secret_keeps_ref_and_store_in_sync() -> Result<()> {
    let (_paths, db) = common::test_database("secret-manager-sync")?;
    let db = Arc::new(db);
    let refs_repo = Arc::new(SqliteSecretRefRepository::new(db.clone()));
    let store: SecretStoreRef = Arc::new(torii::infra::secrets::InMemorySecretStore::new());
    let manager = SecretManager::new(refs_repo.clone(), store, "memory", "torii.test");

    let upserted = manager.upsert_secret("request", "request-42", "bearer_token", "abc-123")?;
    let loaded = manager.get_secret("request", "request-42", "bearer_token")?;
    assert_eq!(loaded.as_deref(), Some("abc-123"));
    assert_eq!(upserted.owner_kind, "request");

    manager.delete_secret("request", "request-42", "bearer_token")?;
    let from_manager = manager.get_secret("request", "request-42", "bearer_token")?;
    assert!(from_manager.is_none(), "deleted secret should be absent");
    let from_repo = refs_repo.get("request", "request-42", "bearer_token")?;
    assert!(from_repo.is_none(), "secret ref should be deleted");

    Ok(())
}

#[test]
fn upsert_rolls_back_secret_ref_when_store_write_fails() -> Result<()> {
    let (_paths, db) = common::test_database("secret-manager-rollback")?;
    let db = Arc::new(db);
    let refs_repo = Arc::new(SqliteSecretRefRepository::new(db.clone()));
    let failing_store: SecretStoreRef = Arc::new(FailingPutSecretStore::default());
    let manager = SecretManager::new(refs_repo.clone(), failing_store, "memory", "torii.test");

    let result = manager.upsert_secret("request", "request-1", "api_key", "top-secret");
    assert!(matches!(
        result,
        Err(SecretManagerError::Store {
            operation: "put_secret",
            ..
        })
    ));

    let stale_ref = refs_repo.get("request", "request-1", "api_key")?;
    assert!(
        stale_ref.is_none(),
        "secret ref must be rolled back when secret write fails"
    );

    Ok(())
}

#[test]
fn duplicated_requests_have_independent_secret_ownership() -> Result<()> {
    // When a request is duplicated, the duplicate gets its own secret refs.
    // Deleting one request's secrets must not break the other's.
    let (_paths, db) = common::test_database("secret-duplicate-isolation")?;
    let db = Arc::new(db);
    let refs_repo = Arc::new(SqliteSecretRefRepository::new(db.clone()));
    let store: SecretStoreRef = Arc::new(torii::infra::secrets::InMemorySecretStore::new());
    let manager = SecretManager::new(refs_repo.clone(), store.clone(), "memory", "torii.test");

    // Source request owns a bearer token
    let _source_ref =
        manager.upsert_secret("request", "source-1", "bearer_token", "original-token")?;

    // Duplicate gets its own secret ref with same value but different ownership
    let _dup_ref =
        manager.upsert_secret("request", "duplicate-1", "bearer_token", "original-token")?;

    // Both should resolve independently
    let source_val = manager.get_secret("request", "source-1", "bearer_token")?;
    assert_eq!(source_val.as_deref(), Some("original-token"));
    let dup_val = manager.get_secret("request", "duplicate-1", "bearer_token")?;
    assert_eq!(dup_val.as_deref(), Some("original-token"));

    // Delete source request's secret
    manager.delete_secret("request", "source-1", "bearer_token")?;

    // Duplicate's secret must still work
    let dup_after_delete = manager.get_secret("request", "duplicate-1", "bearer_token")?;
    assert_eq!(
        dup_after_delete.as_deref(),
        Some("original-token"),
        "duplicate's secret must survive source deletion"
    );

    // Source's secret must be gone
    let source_after_delete = manager.get_secret("request", "source-1", "bearer_token")?;
    assert!(
        source_after_delete.is_none(),
        "source secret must be deleted"
    );

    Ok(())
}
