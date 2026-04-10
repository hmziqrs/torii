mod common;

use std::sync::Arc;

use anyhow::Result;
use torii::{
    infra::{
        blobs::BlobStore,
        secrets::{InMemorySecretStore, SecretStore},
    },
    repos::secret_ref_repo::{SecretRefRepository, SqliteSecretRefRepository},
};

#[test]
fn secrets_are_not_persisted_to_sqlite_or_blobs() -> Result<()> {
    let (paths, db) = common::test_database("secret-storage")?;
    let db = Arc::new(db);
    let secret_repo = SqliteSecretRefRepository::new(db.clone());
    let secret_store = InMemorySecretStore::new();
    let blob_store = BlobStore::new(&paths)?;

    let secret_value = "super-secret-api-token-123";
    let secret_ref = secret_repo.upsert(
        "request",
        "request-1",
        "bearer_token",
        "keyring",
        "torii",
        "request-1:bearer_token",
    )?;
    secret_store.put_secret(&secret_ref.key_name, secret_value)?;
    assert_eq!(
        secret_store.get_secret(&secret_ref.key_name)?,
        Some(secret_value.to_string())
    );

    blob_store.write_bytes(b"non-secret-payload", Some("text/plain"))?;

    let sqlite_bytes = std::fs::read(db.path())?;
    let sqlite_text = String::from_utf8_lossy(&sqlite_bytes);
    assert!(
        !sqlite_text.contains(secret_value),
        "secret material leaked into sqlite"
    );

    for entry in std::fs::read_dir(paths.blobs_dir())? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let bytes = std::fs::read(&path)?;
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains(secret_value),
            "secret material leaked into blob file {}",
            path.display()
        );
    }

    Ok(())
}
