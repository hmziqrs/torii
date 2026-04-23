use std::collections::{BTreeSet, HashMap};

use anyhow::{Result, anyhow};

use crate::{
    domain::{
        ids::{EnvironmentId, WorkspaceId},
        request::{AuthType, BodyType, KeyValuePair, RequestItem},
        variable::{VariableEntry, VariableValue},
    },
    infra::secrets::SecretStoreRef,
    repos::{environment_repo::EnvironmentRepoRef, workspace_repo::WorkspaceRepoRef},
};

#[derive(Clone)]
pub struct VariableResolutionService {
    workspaces: WorkspaceRepoRef,
    environments: EnvironmentRepoRef,
    secret_store: SecretStoreRef,
}

impl VariableResolutionService {
    pub fn new(
        workspaces: WorkspaceRepoRef,
        environments: EnvironmentRepoRef,
        secret_store: SecretStoreRef,
    ) -> Self {
        Self {
            workspaces,
            environments,
            secret_store,
        }
    }

    pub fn resolve_request(
        &self,
        request: &RequestItem,
        workspace_id: WorkspaceId,
        active_environment_id: Option<EnvironmentId>,
    ) -> Result<RequestItem> {
        let workspace = self
            .workspaces
            .get(workspace_id)?
            .ok_or_else(|| anyhow!("workspace not found: {workspace_id}"))?;

        let mut vars = variable_map_from_json(&workspace.variables_json, &self.secret_store);

        if let Some(environment_id) = active_environment_id {
            if let Some(environment) = self.environments.get(environment_id)? {
                if environment.workspace_id == workspace_id {
                    vars.extend(variable_map_from_json(
                        &environment.variables_json,
                        &self.secret_store,
                    ));
                }
            }
        }

        vars.extend(variable_map_from_json(
            &request.variable_overrides_json,
            &self.secret_store,
        ));

        let mut resolved = request.clone();
        resolved.method = resolve_text(&request.method, &vars);
        resolved.url = resolve_text(&request.url, &vars);
        resolved.params = request
            .params
            .iter()
            .map(|entry| KeyValuePair {
                key: resolve_text(&entry.key, &vars),
                value: resolve_text(&entry.value, &vars),
                enabled: entry.enabled,
            })
            .collect();
        resolved.headers = request
            .headers
            .iter()
            .map(|entry| KeyValuePair {
                key: resolve_text(&entry.key, &vars),
                value: resolve_text(&entry.value, &vars),
                enabled: entry.enabled,
            })
            .collect();
        resolved.auth = resolve_auth(&request.auth, &vars);
        resolved.body = resolve_body(&request.body, &vars);
        let missing = collect_missing_placeholders(&resolved);
        if !missing.is_empty() {
            let joined = missing.into_iter().collect::<Vec<_>>().join(", ");
            return Err(anyhow!(
                "missing variables: {joined}; checked scopes: request overrides -> active environment -> workspace"
            ));
        }
        Ok(resolved)
    }
}

fn resolve_auth(auth: &AuthType, vars: &HashMap<String, String>) -> AuthType {
    match auth {
        AuthType::None => AuthType::None,
        AuthType::Basic {
            username,
            password_secret_ref,
        } => AuthType::Basic {
            username: resolve_text(username, vars),
            password_secret_ref: password_secret_ref.clone(),
        },
        AuthType::Bearer { token_secret_ref } => AuthType::Bearer {
            token_secret_ref: token_secret_ref.clone(),
        },
        AuthType::ApiKey {
            key_name,
            value_secret_ref,
            location,
        } => AuthType::ApiKey {
            key_name: resolve_text(key_name, vars),
            value_secret_ref: value_secret_ref.clone(),
            location: *location,
        },
    }
}

fn resolve_body(body: &BodyType, vars: &HashMap<String, String>) -> BodyType {
    match body {
        BodyType::None => BodyType::None,
        BodyType::RawText { content } => BodyType::RawText {
            content: resolve_text(content, vars),
        },
        BodyType::RawJson { content } => BodyType::RawJson {
            content: resolve_text(content, vars),
        },
        BodyType::UrlEncoded { entries } => BodyType::UrlEncoded {
            entries: entries
                .iter()
                .map(|entry| KeyValuePair {
                    key: resolve_text(&entry.key, vars),
                    value: resolve_text(&entry.value, vars),
                    enabled: entry.enabled,
                })
                .collect(),
        },
        BodyType::FormData {
            text_fields,
            file_fields,
        } => BodyType::FormData {
            text_fields: text_fields
                .iter()
                .map(|entry| KeyValuePair {
                    key: resolve_text(&entry.key, vars),
                    value: resolve_text(&entry.value, vars),
                    enabled: entry.enabled,
                })
                .collect(),
            file_fields: file_fields.clone(),
        },
        BodyType::BinaryFile {
            file_name,
            blob_hash,
        } => BodyType::BinaryFile {
            file_name: file_name.clone(),
            blob_hash: blob_hash.clone(),
        },
    }
}

fn variable_map_from_json(
    variables_json: &str,
    secret_store: &SecretStoreRef,
) -> HashMap<String, String> {
    parse_variable_entries(variables_json)
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| entry.enabled && !entry.key.trim().is_empty())
        .map(|entry| {
            let value = match entry.value {
                VariableValue::Plain { value } => value,
                VariableValue::Secret { secret_ref } => secret_ref
                    .and_then(|key| secret_store.get_secret(&key).ok().flatten())
                    .unwrap_or_default(),
            };
            (entry.key.trim().to_string(), value)
        })
        .collect()
}

fn parse_variable_entries(variables_json: &str) -> Result<Vec<VariableEntry>> {
    let value = serde_json::from_str::<serde_json::Value>(variables_json)
        .unwrap_or_else(|_| serde_json::json!([]));
    match value {
        serde_json::Value::Array(items) => Ok(serde_json::from_value::<Vec<VariableEntry>>(
            serde_json::Value::Array(items),
        )?),
        serde_json::Value::Object(map) => Ok(map
            .into_iter()
            .map(|(key, value)| VariableEntry {
                key,
                enabled: true,
                value: VariableValue::Plain {
                    value: match value {
                        serde_json::Value::String(text) => text,
                        other => other.to_string(),
                    },
                },
            })
            .collect()),
        _ => Ok(Vec::new()),
    }
}

fn resolve_text(input: &str, vars: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    loop {
        let Some(start) = rest.find("{{") else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("}}") else {
            out.push_str(&rest[start..]);
            break;
        };

        let key = after_start[..end].trim();
        if let Some(value) = vars.get(key) {
            out.push_str(value);
        } else {
            out.push_str("{{");
            out.push_str(&after_start[..end]);
            out.push_str("}}");
        }

        rest = &after_start[end + 2..];
    }

    out
}

fn collect_missing_placeholders(request: &RequestItem) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    collect_placeholders_in_text(&request.method, &mut out);
    collect_placeholders_in_text(&request.url, &mut out);

    for entry in &request.params {
        collect_placeholders_in_text(&entry.key, &mut out);
        collect_placeholders_in_text(&entry.value, &mut out);
    }
    for entry in &request.headers {
        collect_placeholders_in_text(&entry.key, &mut out);
        collect_placeholders_in_text(&entry.value, &mut out);
    }

    match &request.auth {
        AuthType::None | AuthType::Bearer { .. } => {}
        AuthType::Basic { username, .. } => {
            collect_placeholders_in_text(username, &mut out);
        }
        AuthType::ApiKey { key_name, .. } => {
            collect_placeholders_in_text(key_name, &mut out);
        }
    }

    match &request.body {
        BodyType::None | BodyType::BinaryFile { .. } => {}
        BodyType::RawText { content } | BodyType::RawJson { content } => {
            collect_placeholders_in_text(content, &mut out);
        }
        BodyType::UrlEncoded { entries } => {
            for entry in entries {
                collect_placeholders_in_text(&entry.key, &mut out);
                collect_placeholders_in_text(&entry.value, &mut out);
            }
        }
        BodyType::FormData {
            text_fields,
            file_fields: _,
        } => {
            for entry in text_fields {
                collect_placeholders_in_text(&entry.key, &mut out);
                collect_placeholders_in_text(&entry.value, &mut out);
            }
        }
    }

    out
}

fn collect_placeholders_in_text(value: &str, out: &mut BTreeSet<String>) {
    let mut rest = value;
    loop {
        let Some(start) = rest.find("{{") else {
            break;
        };
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("}}") else {
            break;
        };
        let key = after_start[..end].trim();
        if !key.is_empty() {
            out.insert(key.to_string());
        }
        rest = &after_start[end + 2..];
    }
}

#[cfg(test)]
mod tests {
    use super::{collect_missing_placeholders, resolve_text};
    use crate::domain::request::RequestItem;
    use std::collections::HashMap;

    #[test]
    fn resolve_text_replaces_known_variables() {
        let mut vars = HashMap::new();
        vars.insert("baseUrl".to_string(), "https://api.example.com".to_string());
        vars.insert("id".to_string(), "123".to_string());

        let resolved = resolve_text("{{baseUrl}}/users/{{ id }}", &vars);
        assert_eq!(resolved, "https://api.example.com/users/123");
    }

    #[test]
    fn resolve_text_keeps_unknown_variables() {
        let vars = HashMap::new();
        let resolved = resolve_text("{{missing}}/users", &vars);
        assert_eq!(resolved, "{{missing}}/users");
    }

    #[test]
    fn collect_missing_placeholders_extracts_unique_trimmed_keys() {
        let mut request = RequestItem::new(
            crate::domain::ids::CollectionId::new(),
            None,
            "Req",
            "GET",
            "{{ baseUrl }}/users/{{userId}}",
            0,
        );
        request.headers = vec![crate::domain::request::KeyValuePair::new(
            "X-Env",
            "{{ env }} {{userId}}",
        )];

        let missing = collect_missing_placeholders(&request)
            .into_iter()
            .collect::<Vec<_>>();
        assert_eq!(missing, vec!["baseUrl", "env", "userId"]);
    }
}
