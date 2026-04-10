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
