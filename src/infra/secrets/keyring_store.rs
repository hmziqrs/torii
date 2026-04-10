use anyhow::{Context, Result};

use super::SecretStore;

#[derive(Debug, Clone)]
pub struct KeyringSecretStore {
    service_name: String,
}

impl KeyringSecretStore {
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
        }
    }

    fn entry(&self, key: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service_name, key).context("failed to create keyring entry")
    }
}

impl SecretStore for KeyringSecretStore {
    fn put_secret(&self, key: &str, value: &str) -> Result<()> {
        self.entry(key)?
            .set_password(value)
            .with_context(|| format!("failed to write secret keyring entry: {}", key))
    }

    fn get_secret(&self, key: &str) -> Result<Option<String>> {
        match self.entry(key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(anyhow::anyhow!(err))
                .with_context(|| format!("failed to read secret keyring entry: {}", key)),
        }
    }

    fn delete_secret(&self, key: &str) -> Result<()> {
        match self.entry(key)?.delete_credential() {
            Ok(_) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(anyhow::anyhow!(err))
                .with_context(|| format!("failed to delete secret keyring entry: {}", key)),
        }
    }
}
