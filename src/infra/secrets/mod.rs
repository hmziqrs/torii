use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use anyhow::{Result, anyhow};

pub mod keyring_store;

pub trait SecretStore: Send + Sync {
    fn put_secret(&self, key: &str, value: &str) -> Result<()>;
    fn get_secret(&self, key: &str) -> Result<Option<String>>;
    fn delete_secret(&self, key: &str) -> Result<()>;
}

pub type SecretStoreRef = Arc<dyn SecretStore>;

#[derive(Default)]
pub struct InMemorySecretStore {
    values: Mutex<HashMap<String, String>>,
}

impl InMemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecretStore for InMemorySecretStore {
    fn put_secret(&self, key: &str, value: &str) -> Result<()> {
        let mut values = self
            .values
            .lock()
            .map_err(|_| anyhow!("in-memory secret store mutex poisoned"))?;
        values.insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn get_secret(&self, key: &str) -> Result<Option<String>> {
        let values = self
            .values
            .lock()
            .map_err(|_| anyhow!("in-memory secret store mutex poisoned"))?;
        Ok(values.get(key).cloned())
    }

    fn delete_secret(&self, key: &str) -> Result<()> {
        let mut values = self
            .values
            .lock()
            .map_err(|_| anyhow!("in-memory secret store mutex poisoned"))?;
        values.remove(key);
        Ok(())
    }
}
