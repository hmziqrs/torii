use std::sync::Arc;

use anyhow::{Context, Result};

use crate::{
    infra::{
        blobs::BlobStore,
        db::Database,
        paths::AppPaths,
        secrets::{InMemorySecretStore, SecretStoreRef, keyring_store::KeyringSecretStore},
    },
    repos::{
        collection_repo::{CollectionRepoRef, SqliteCollectionRepository},
        environment_repo::{EnvironmentRepoRef, SqliteEnvironmentRepository},
        folder_repo::{FolderRepoRef, SqliteFolderRepository},
        history_repo::{HistoryRepoRef, SqliteHistoryRepository},
        preferences_repo::{PreferencesRepoRef, SqlitePreferencesRepository},
        request_repo::{RequestRepoRef, SqliteRequestRepository},
        secret_ref_repo::{SecretRefRepoRef, SqliteSecretRefRepository},
        tab_session_repo::{SqliteTabSessionRepository, TabSessionRepoRef},
        workspace_repo::{SqliteWorkspaceRepository, WorkspaceRepoRef},
    },
};

use super::{
    app_services::{AppServices, Repositories},
    recovery::RecoveryCoordinator,
    request_execution::{RequestExecutionService, ReqwestTransport},
    secret_manager::SecretManager,
    session_restore::SessionRestoreService,
    tokio_runtime::TokioRuntime,
    ui_preferences::{
        InMemoryUiPreferencesStore, SqliteUiPreferencesStore, UiPreferencesSnapshot,
        UiPreferencesStoreRef,
    },
};

pub fn bootstrap_app_services() -> Arc<AppServices> {
    match build_app_services() {
        Ok(services) => Arc::new(services),
        Err(err) => {
            tracing::error!("failed to bootstrap app services: {err}");
            Arc::new(fallback_app_services())
        }
    }
}

fn build_app_services() -> Result<AppServices> {
    let paths = AppPaths::from_system()?;
    let db = Arc::new(Database::connect(&paths)?);
    let blob_store = Arc::new(BlobStore::new(&paths)?);
    let io_runtime = Arc::new(TokioRuntime::new().context("failed to build I/O runtime")?);

    let workspace_repo: WorkspaceRepoRef = Arc::new(SqliteWorkspaceRepository::new(db.clone()));
    let collection_repo: CollectionRepoRef = Arc::new(SqliteCollectionRepository::new(db.clone()));
    let folder_repo: FolderRepoRef = Arc::new(SqliteFolderRepository::new(db.clone()));
    let request_repo: RequestRepoRef = Arc::new(SqliteRequestRepository::new(db.clone()));
    let environment_repo: EnvironmentRepoRef =
        Arc::new(SqliteEnvironmentRepository::new(db.clone()));
    let history_repo: HistoryRepoRef = Arc::new(SqliteHistoryRepository::new(db.clone()));
    let preferences_repo: PreferencesRepoRef =
        Arc::new(SqlitePreferencesRepository::new(db.clone()));
    let secret_ref_repo: SecretRefRepoRef = Arc::new(SqliteSecretRefRepository::new(db.clone()));
    let tab_session_repo: TabSessionRepoRef = Arc::new(SqliteTabSessionRepository::new(db.clone()));

    let secret_store: SecretStoreRef = Arc::new(KeyringSecretStore::new(format!(
        "{}.{}.{}",
        crate::infra::paths::APP_QUALIFIER,
        crate::infra::paths::APP_ORGANIZATION,
        crate::infra::paths::APP_NAME
    )));
    let secret_manager = SecretManager::new(
        secret_ref_repo.clone(),
        secret_store.clone(),
        "keyring",
        format!(
            "{}.{}.{}",
            crate::infra::paths::APP_QUALIFIER,
            crate::infra::paths::APP_ORGANIZATION,
            crate::infra::paths::APP_NAME
        ),
    );
    let ui_preferences: UiPreferencesStoreRef =
        Arc::new(SqliteUiPreferencesStore::new(preferences_repo.clone()));
    let transport = Arc::new(ReqwestTransport::new().context("failed to build HTTP transport")?);
    let request_execution = Arc::new(RequestExecutionService::new(
        transport,
        history_repo.clone(),
        blob_store.clone(),
        secret_store.clone(),
    ));

    let recovery = RecoveryCoordinator::new(db.clone(), history_repo.clone(), blob_store.clone());
    recovery
        .run_startup_recovery()
        .context("startup recovery failed")?;
    ensure_sample_workspace(
        &workspace_repo,
        &collection_repo,
        &folder_repo,
        &request_repo,
        &environment_repo,
    )?;
    let session_restore = SessionRestoreService::new(
        tab_session_repo.clone(),
        workspace_repo.clone(),
        collection_repo.clone(),
        folder_repo.clone(),
        request_repo.clone(),
        environment_repo.clone(),
    );

    Ok(AppServices {
        paths,
        db,
        io_runtime,
        request_execution,
        blob_store,
        secret_store,
        secret_manager,
        repos: Repositories {
            workspace: workspace_repo,
            collection: collection_repo,
            folder: folder_repo,
            request: request_repo,
            environment: environment_repo,
            history: history_repo,
            preferences: preferences_repo,
            secret_refs: secret_ref_repo,
            tab_session: tab_session_repo,
        },
        ui_preferences,
        recovery,
        session_restore,
    })
}

fn fallback_app_services() -> AppServices {
    let fallback_paths = {
        let base = std::env::temp_dir().join(format!("torii-fallback-{}", uuid::Uuid::now_v7()));
        match AppPaths::from_test_base(&base) {
            Ok(paths) => paths,
            Err(err) => {
                tracing::error!("failed to build fallback app paths: {err}");
                AppPaths {
                    config_dir: std::env::temp_dir().join("torii-config"),
                    data_dir: std::env::temp_dir().join("torii-data"),
                    cache_dir: std::env::temp_dir().join("torii-cache"),
                }
            }
        }
    };

    let db = match Database::connect(&fallback_paths) {
        Ok(db) => Arc::new(db),
        Err(err) => {
            tracing::error!("failed to initialize fallback sqlite: {err}");
            let second_try_paths =
                AppPaths::from_test_base(&std::env::temp_dir().join("torii-last-resort"))
                    .unwrap_or(AppPaths {
                        config_dir: std::env::temp_dir().join("torii-config-last"),
                        data_dir: std::env::temp_dir().join("torii-data-last"),
                        cache_dir: std::env::temp_dir().join("torii-cache-last"),
                    });
            Arc::new(
                Database::connect(&second_try_paths).unwrap_or_else(|fatal| {
                    panic!("unable to initialize fallback database: {fatal}")
                }),
            )
        }
    };

    let blob_store = Arc::new(BlobStore::new(&fallback_paths).unwrap_or_else(|err| {
        panic!("unable to initialize fallback blob store: {err}");
    }));
    let io_runtime = Arc::new(TokioRuntime::new().unwrap_or_else(|err| {
        panic!("unable to initialize fallback I/O runtime: {err}");
    }));

    let workspace_repo: WorkspaceRepoRef = Arc::new(SqliteWorkspaceRepository::new(db.clone()));
    let collection_repo: CollectionRepoRef = Arc::new(SqliteCollectionRepository::new(db.clone()));
    let folder_repo: FolderRepoRef = Arc::new(SqliteFolderRepository::new(db.clone()));
    let request_repo: RequestRepoRef = Arc::new(SqliteRequestRepository::new(db.clone()));
    let environment_repo: EnvironmentRepoRef =
        Arc::new(SqliteEnvironmentRepository::new(db.clone()));
    let history_repo: HistoryRepoRef = Arc::new(SqliteHistoryRepository::new(db.clone()));
    let preferences_repo: PreferencesRepoRef =
        Arc::new(SqlitePreferencesRepository::new(db.clone()));
    let secret_ref_repo: SecretRefRepoRef = Arc::new(SqliteSecretRefRepository::new(db.clone()));
    let tab_session_repo: TabSessionRepoRef = Arc::new(SqliteTabSessionRepository::new(db.clone()));

    let secret_store: SecretStoreRef = Arc::new(InMemorySecretStore::new());
    let secret_manager = SecretManager::new(
        secret_ref_repo.clone(),
        secret_store.clone(),
        "memory",
        format!(
            "{}.{}.{}",
            crate::infra::paths::APP_QUALIFIER,
            crate::infra::paths::APP_ORGANIZATION,
            crate::infra::paths::APP_NAME
        ),
    );
    let ui_preferences: UiPreferencesStoreRef = Arc::new(InMemoryUiPreferencesStore::new(Some(
        UiPreferencesSnapshot::default(),
    )));
    let transport =
        Arc::new(ReqwestTransport::new().expect("failed to build fallback HTTP transport"));
    let request_execution = Arc::new(RequestExecutionService::new(
        transport,
        history_repo.clone(),
        blob_store.clone(),
        secret_store.clone(),
    ));

    let recovery = RecoveryCoordinator::new(db.clone(), history_repo.clone(), blob_store.clone());
    let _ = recovery.run_startup_recovery();
    let _ = ensure_sample_workspace(
        &workspace_repo,
        &collection_repo,
        &folder_repo,
        &request_repo,
        &environment_repo,
    );
    let session_restore = SessionRestoreService::new(
        tab_session_repo.clone(),
        workspace_repo.clone(),
        collection_repo.clone(),
        folder_repo.clone(),
        request_repo.clone(),
        environment_repo.clone(),
    );

    AppServices {
        paths: fallback_paths,
        db,
        io_runtime,
        request_execution,
        blob_store,
        secret_store,
        secret_manager,
        repos: Repositories {
            workspace: workspace_repo,
            collection: collection_repo,
            folder: folder_repo,
            request: request_repo,
            environment: environment_repo,
            history: history_repo,
            preferences: preferences_repo,
            secret_refs: secret_ref_repo,
            tab_session: tab_session_repo,
        },
        ui_preferences,
        recovery,
        session_restore,
    }
}

fn ensure_sample_workspace(
    workspace_repo: &WorkspaceRepoRef,
    collection_repo: &CollectionRepoRef,
    folder_repo: &FolderRepoRef,
    request_repo: &RequestRepoRef,
    environment_repo: &EnvironmentRepoRef,
) -> Result<()> {
    if !workspace_repo.list()?.is_empty() {
        return Ok(());
    }

    let workspace = workspace_repo.create("Postman Demo Workspace")?;

    let api_collection = collection_repo.create(workspace.id, "Core API")?;
    let auth_folder = folder_repo.create(api_collection.id, None, "Auth")?;
    let users_folder = folder_repo.create(api_collection.id, None, "Users")?;

    request_repo.create(
        api_collection.id,
        Some(auth_folder.id),
        "Sign In",
        "POST",
        "https://api.example.test/auth/sign-in",
    )?;
    request_repo.create(
        api_collection.id,
        Some(auth_folder.id),
        "Refresh Token",
        "POST",
        "https://api.example.test/auth/refresh",
    )?;
    request_repo.create(
        api_collection.id,
        Some(users_folder.id),
        "List Users",
        "GET",
        "https://api.example.test/users",
    )?;
    request_repo.create(
        api_collection.id,
        Some(users_folder.id),
        "Get User",
        "GET",
        "https://api.example.test/users/:id",
    )?;

    let admin_collection = collection_repo.create(workspace.id, "Admin API")?;
    request_repo.create(
        admin_collection.id,
        None,
        "Health Check",
        "GET",
        "https://admin.example.test/health",
    )?;

    let environment = environment_repo.create(api_collection.id, "Local")?;
    environment_repo.update_variables(
        environment.id,
        r#"{"baseUrl":"https://api.example.test","token":"demo-token"}"#,
    )?;

    Ok(())
}
