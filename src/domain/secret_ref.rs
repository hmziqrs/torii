use serde::{Deserialize, Serialize};

use super::ids::SecretRefId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretRef {
    pub id: SecretRefId,
    pub owner_kind: String,
    pub owner_id: String,
    pub secret_kind: String,
    pub provider: String,
    pub namespace: String,
    pub key_name: String,
    pub created_at: i64,
    pub updated_at: i64,
}
