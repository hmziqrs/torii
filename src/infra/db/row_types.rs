use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct UiPreferenceRow {
    pub key: String,
    pub value_json: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct SecretRefRow {
    pub id: String,
    pub owner_kind: String,
    pub owner_id: String,
    pub secret_kind: String,
    pub provider: String,
    pub namespace: String,
    pub key_name: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct HistoryBlobRefRow {
    pub blob_hash: String,
}
