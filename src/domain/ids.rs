use std::{fmt, str::FromStr};

use anyhow::{Context as _, Result, anyhow};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! typed_uuid_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            pub fn parse(value: &str) -> Result<Self> {
                Ok(Self(Uuid::parse_str(value).with_context(|| {
                    format!("invalid {}: {}", stringify!($name), value)
                })?))
            }

            pub fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0.to_string()
            }
        }

        impl FromStr for $name {
            type Err = anyhow::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::parse(s)
            }
        }
    };
}

typed_uuid_id!(WorkspaceId);
typed_uuid_id!(CollectionId);
typed_uuid_id!(FolderId);
typed_uuid_id!(RequestId);
typed_uuid_id!(EnvironmentId);
typed_uuid_id!(HistoryEntryId);
typed_uuid_id!(SecretRefId);
typed_uuid_id!(RequestDraftId);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlobId(pub String);

impl BlobId {
    pub fn new(hash: impl Into<String>) -> Result<Self> {
        let hash = hash.into();
        if hash.trim().is_empty() {
            return Err(anyhow!("blob hash cannot be empty"));
        }
        Ok(Self(hash))
    }
}

impl fmt::Display for BlobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
