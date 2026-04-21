use serde::{Deserialize, Serialize};

use super::request::{AuthType, BodyType, KeyValuePair, RequestSettings};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VariableValue {
    Plain { value: String },
    Secret { secret_ref: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariableEntry {
    pub key: String,
    pub enabled: bool,
    pub value: VariableValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedRequest {
    pub method: String,
    pub url: String,
    pub params: Vec<KeyValuePair>,
    pub headers: Vec<KeyValuePair>,
    pub auth: AuthType,
    pub body: BodyType,
    pub settings: RequestSettings,
}
