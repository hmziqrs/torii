mod common;

use std::sync::Arc;

use anyhow::Result;
use torii::{
    domain::{
        request::{AuthType, BodyType, KeyValuePair},
        variable::{VariableEntry, VariableValue},
    },
    infra::secrets::InMemorySecretStore,
    repos::{
        collection_repo::{CollectionRepository, SqliteCollectionRepository},
        environment_repo::{EnvironmentRepository, SqliteEnvironmentRepository},
        request_repo::{RequestRepository, SqliteRequestRepository},
        workspace_repo::{SqliteWorkspaceRepository, WorkspaceRepository},
    },
    services::variable_resolution::VariableResolutionService,
};

fn to_json(entries: Vec<VariableEntry>) -> String {
    serde_json::to_string(&entries).expect("variable entry json")
}

#[test]
fn variable_resolution_precedence_request_over_env_over_workspace() -> Result<()> {
    let (_paths, db) = common::test_database("variable-resolution-precedence")?;
    let workspace_repo = Arc::new(SqliteWorkspaceRepository::new(Arc::new(db.clone())));
    let collection_repo = Arc::new(SqliteCollectionRepository::new(Arc::new(db.clone())));
    let request_repo = Arc::new(SqliteRequestRepository::new(Arc::new(db.clone())));
    let environment_repo = Arc::new(SqliteEnvironmentRepository::new(Arc::new(db)));

    let workspace = workspace_repo.create("Workspace A")?;
    workspace_repo.update_variables(
        workspace.id,
        &to_json(vec![
            VariableEntry {
                key: "baseUrl".to_string(),
                enabled: true,
                value: VariableValue::Plain {
                    value: "https://workspace.example".to_string(),
                },
            },
            VariableEntry {
                key: "shared".to_string(),
                enabled: true,
                value: VariableValue::Plain {
                    value: "workspace".to_string(),
                },
            },
        ]),
    )?;

    let environment = environment_repo.create(workspace.id, "Dev")?;
    environment_repo.update_variables(
        environment.id,
        &to_json(vec![
            VariableEntry {
                key: "baseUrl".to_string(),
                enabled: true,
                value: VariableValue::Plain {
                    value: "https://env.example".to_string(),
                },
            },
            VariableEntry {
                key: "shared".to_string(),
                enabled: true,
                value: VariableValue::Plain {
                    value: "environment".to_string(),
                },
            },
        ]),
    )?;

    let collection = collection_repo.create(workspace.id, "Main")?;
    let mut request = request_repo.create(
        collection.id,
        None,
        "List Users",
        "GET",
        "{{baseUrl}}/users?scope={{shared}}",
    )?;
    request.headers = vec![KeyValuePair::new("X-Scope", "{{shared}}")];
    request.params = vec![KeyValuePair::new("q", "{{shared}}")];
    request.body = BodyType::RawJson {
        content: r#"{"scope":"{{shared}}"}"#.to_string(),
    };
    request.auth = AuthType::Basic {
        username: "{{shared}}".to_string(),
        password_secret_ref: None,
    };
    request.variable_overrides_json = to_json(vec![VariableEntry {
        key: "shared".to_string(),
        enabled: true,
        value: VariableValue::Plain {
            value: "request".to_string(),
        },
    }]);
    let request = request_repo.save(&request, request.meta.revision)?;

    let secret_store: Arc<dyn torii::infra::secrets::SecretStore> =
        Arc::new(InMemorySecretStore::new());
    let resolver = VariableResolutionService::new(
        workspace_repo.clone(),
        environment_repo.clone(),
        secret_store,
    );

    let resolved = resolver.resolve_request(&request, workspace.id, Some(environment.id))?;
    assert_eq!(resolved.url, "https://env.example/users?scope=request");
    assert_eq!(resolved.headers[0].value, "request");
    assert_eq!(resolved.params[0].value, "request");
    match resolved.body {
        BodyType::RawJson { content } => assert_eq!(content, r#"{"scope":"request"}"#),
        _ => panic!("expected raw json body"),
    }
    match resolved.auth {
        AuthType::Basic { username, .. } => assert_eq!(username, "request"),
        _ => panic!("expected basic auth"),
    }
    Ok(())
}

#[test]
fn variable_resolution_supports_secret_values() -> Result<()> {
    let (_paths, db) = common::test_database("variable-resolution-secret")?;
    let workspace_repo = Arc::new(SqliteWorkspaceRepository::new(Arc::new(db.clone())));
    let collection_repo = Arc::new(SqliteCollectionRepository::new(Arc::new(db.clone())));
    let request_repo = Arc::new(SqliteRequestRepository::new(Arc::new(db.clone())));
    let environment_repo = Arc::new(SqliteEnvironmentRepository::new(Arc::new(db)));

    let workspace = workspace_repo.create("Workspace Secret")?;
    let collection = collection_repo.create(workspace.id, "Main")?;
    let mut request = request_repo.create(
        collection.id,
        None,
        "Secret Request",
        "GET",
        "{{baseUrl}}/v1",
    )?;
    request.variable_overrides_json = to_json(vec![VariableEntry {
        key: "baseUrl".to_string(),
        enabled: true,
        value: VariableValue::Secret {
            secret_ref: Some("var.baseUrl".to_string()),
        },
    }]);
    let request = request_repo.save(&request, request.meta.revision)?;

    let secret_store: Arc<dyn torii::infra::secrets::SecretStore> =
        Arc::new(InMemorySecretStore::new());
    secret_store.put_secret("var.baseUrl", "https://secret.example")?;

    let resolver = VariableResolutionService::new(
        workspace_repo.clone(),
        environment_repo.clone(),
        secret_store,
    );
    let resolved = resolver.resolve_request(&request, workspace.id, None)?;
    assert_eq!(resolved.url, "https://secret.example/v1");
    Ok(())
}

#[test]
fn variable_resolution_fails_preflight_for_missing_variables() -> Result<()> {
    let (_paths, db) = common::test_database("variable-resolution-missing-preflight")?;
    let workspace_repo = Arc::new(SqliteWorkspaceRepository::new(Arc::new(db.clone())));
    let collection_repo = Arc::new(SqliteCollectionRepository::new(Arc::new(db.clone())));
    let request_repo = Arc::new(SqliteRequestRepository::new(Arc::new(db.clone())));
    let environment_repo = Arc::new(SqliteEnvironmentRepository::new(Arc::new(db)));

    let workspace = workspace_repo.create("Workspace Missing")?;
    let collection = collection_repo.create(workspace.id, "Main")?;
    let request = request_repo.create(
        collection.id,
        None,
        "Missing Vars",
        "GET",
        "{{baseUrl}}/users/{{userId}}",
    )?;

    let secret_store: Arc<dyn torii::infra::secrets::SecretStore> =
        Arc::new(InMemorySecretStore::new());
    let resolver = VariableResolutionService::new(
        workspace_repo.clone(),
        environment_repo.clone(),
        secret_store,
    );

    let err = resolver
        .resolve_request(&request, workspace.id, None)
        .expect_err("expected missing variable preflight failure");
    let msg = err.to_string();
    assert!(msg.contains("missing variables"));
    assert!(msg.contains("baseUrl"));
    assert!(msg.contains("userId"));
    assert!(msg.contains("checked scopes"));
    Ok(())
}
