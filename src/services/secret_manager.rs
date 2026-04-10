use std::{error::Error, fmt};

use crate::{
    domain::secret_ref::SecretRef, infra::secrets::SecretStoreRef,
    repos::secret_ref_repo::SecretRefRepoRef,
};

#[derive(Debug)]
pub enum SecretManagerError {
    Repository {
        operation: &'static str,
        source: anyhow::Error,
    },
    Store {
        operation: &'static str,
        source: anyhow::Error,
    },
    InconsistentState {
        message: String,
    },
}

impl fmt::Display for SecretManagerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repository { operation, .. } => {
                write!(f, "secret ref repository operation failed: {operation}")
            }
            Self::Store { operation, .. } => {
                write!(f, "secret store operation failed: {operation}")
            }
            Self::InconsistentState { message } => {
                write!(f, "secret state is inconsistent: {message}")
            }
        }
    }
}

impl Error for SecretManagerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Repository { source, .. } => Some(source.as_ref()),
            Self::Store { source, .. } => Some(source.as_ref()),
            Self::InconsistentState { .. } => None,
        }
    }
}

pub type SecretManagerResult<T> = Result<T, SecretManagerError>;

#[derive(Clone)]
pub struct SecretManager {
    secret_refs: SecretRefRepoRef,
    secret_store: SecretStoreRef,
    provider: String,
    namespace: String,
}

impl SecretManager {
    pub fn new(
        secret_refs: SecretRefRepoRef,
        secret_store: SecretStoreRef,
        provider: impl Into<String>,
        namespace: impl Into<String>,
    ) -> Self {
        Self {
            secret_refs,
            secret_store,
            provider: provider.into(),
            namespace: namespace.into(),
        }
    }

    pub fn upsert_secret(
        &self,
        owner_kind: &str,
        owner_id: &str,
        secret_kind: &str,
        value: &str,
    ) -> SecretManagerResult<SecretRef> {
        let key_name = self.key_name(owner_kind, owner_id, secret_kind);
        let secret_ref = self
            .secret_refs
            .upsert(
                owner_kind,
                owner_id,
                secret_kind,
                &self.provider,
                &self.namespace,
                &key_name,
            )
            .map_err(|source| SecretManagerError::Repository {
                operation: "upsert",
                source,
            })?;

        if let Err(source) = self.secret_store.put_secret(&key_name, value) {
            if let Err(rollback_source) = self.secret_refs.delete(owner_kind, owner_id, secret_kind)
            {
                return Err(SecretManagerError::InconsistentState {
                    message: format!(
                        "failed to persist secret value and failed to rollback secret ref (store={source}, rollback={rollback_source})"
                    ),
                });
            }

            return Err(SecretManagerError::Store {
                operation: "put_secret",
                source,
            });
        }

        Ok(secret_ref)
    }

    pub fn get_secret(
        &self,
        owner_kind: &str,
        owner_id: &str,
        secret_kind: &str,
    ) -> SecretManagerResult<Option<String>> {
        let Some(secret_ref) = self
            .secret_refs
            .get(owner_kind, owner_id, secret_kind)
            .map_err(|source| SecretManagerError::Repository {
                operation: "get",
                source,
            })?
        else {
            return Ok(None);
        };

        match self
            .secret_store
            .get_secret(&secret_ref.key_name)
            .map_err(|source| SecretManagerError::Store {
                operation: "get_secret",
                source,
            })? {
            Some(value) => Ok(Some(value)),
            None => Err(SecretManagerError::InconsistentState {
                message: format!(
                    "secret value missing for owner_kind={owner_kind}, owner_id={owner_id}, secret_kind={secret_kind}"
                ),
            }),
        }
    }

    pub fn delete_secret(
        &self,
        owner_kind: &str,
        owner_id: &str,
        secret_kind: &str,
    ) -> SecretManagerResult<()> {
        let Some(secret_ref) = self
            .secret_refs
            .get(owner_kind, owner_id, secret_kind)
            .map_err(|source| SecretManagerError::Repository {
                operation: "get",
                source,
            })?
        else {
            return Ok(());
        };

        self.secret_store
            .delete_secret(&secret_ref.key_name)
            .map_err(|source| SecretManagerError::Store {
                operation: "delete_secret",
                source,
            })?;
        self.secret_refs
            .delete(owner_kind, owner_id, secret_kind)
            .map_err(|source| SecretManagerError::Repository {
                operation: "delete",
                source,
            })?;

        Ok(())
    }

    pub fn key_name(&self, owner_kind: &str, owner_id: &str, secret_kind: &str) -> String {
        format!("{}:{owner_kind}:{owner_id}:{secret_kind}", self.namespace)
    }
}
