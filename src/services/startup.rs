use std::sync::Arc;
use std::path::Path;

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
        preferences_repo::{PreferencesRepoRef, PreferencesRepository, SqlitePreferencesRepository},
        request_repo::{RequestRepoRef, SqliteRequestRepository},
        secret_ref_repo::{SecretRefRepoRef, SqliteSecretRefRepository},
        workspace_repo::{SqliteWorkspaceRepository, WorkspaceRepoRef},
    },
};

use super::{
    app_services::{AppServices, Repositories},
    recovery::RecoveryCoordinator,
    ui_preferences::{
        InMemoryUiPreferencesStore, SqliteUiPreferencesStore, UiPreferencesSnapshot,
        UiPreferencesStoreRef, load_legacy_ui_preferences_file,
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

    let workspace_repo: WorkspaceRepoRef = Arc::new(SqliteWorkspaceRepository::new(db.clone()));
    let collection_repo: CollectionRepoRef = Arc::new(SqliteCollectionRepository::new(db.clone()));
    let folder_repo: FolderRepoRef = Arc::new(SqliteFolderRepository::new(db.clone()));
    let request_repo: RequestRepoRef = Arc::new(SqliteRequestRepository::new(db.clone()));
    let environment_repo: EnvironmentRepoRef = Arc::new(SqliteEnvironmentRepository::new(db.clone()));
    let history_repo: HistoryRepoRef = Arc::new(SqliteHistoryRepository::new(db.clone()));
    let preferences_repo: PreferencesRepoRef = Arc::new(SqlitePreferencesRepository::new(db.clone()));
    let secret_ref_repo: SecretRefRepoRef = Arc::new(SqliteSecretRefRepository::new(db.clone()));

    let secret_store: SecretStoreRef = Arc::new(KeyringSecretStore::new(format!(
        "{}.{}.{}",
        crate::infra::paths::APP_QUALIFIER,
        crate::infra::paths::APP_ORGANIZATION,
        crate::infra::paths::APP_NAME
    )));
    let ui_preferences: UiPreferencesStoreRef =
        Arc::new(SqliteUiPreferencesStore::new(preferences_repo.clone()));

    migrate_legacy_state_json_if_needed(&preferences_repo)
        .context("failed to run legacy settings migration")?;

    let recovery = RecoveryCoordinator::new(db.clone(), history_repo.clone(), blob_store.clone());
    recovery
        .run_startup_recovery()
        .context("startup recovery failed")?;

    Ok(AppServices {
        paths,
        db,
        blob_store,
        secret_store,
        repos: Repositories {
            workspace: workspace_repo,
            collection: collection_repo,
            folder: folder_repo,
            request: request_repo,
            environment: environment_repo,
            history: history_repo,
            preferences: preferences_repo,
            secret_refs: secret_ref_repo,
        },
        ui_preferences,
        recovery,
    })
}

fn migrate_legacy_state_json_if_needed(preferences_repo: &PreferencesRepoRef) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to resolve current working directory")?;
    let legacy_path = cwd.join("target").join("state.json");
    let _ = migrate_legacy_state_json_if_needed_at_path(preferences_repo, &legacy_path)?;
    Ok(())
}

pub fn migrate_legacy_state_json_if_needed_at_path(
    preferences_repo: &PreferencesRepoRef,
    legacy_path: &Path,
) -> Result<bool> {
    if preferences_repo.load_ui_preferences()?.is_some() {
        return Ok(false);
    }

    let Some(legacy) = load_legacy_ui_preferences_file(legacy_path)? else {
        return Ok(false);
    };

    preferences_repo.save_ui_preferences(&legacy.into())?;
    tracing::info!(
        legacy_file = %legacy_path.display(),
        "migrated legacy ui preferences into sqlite"
    );
    Ok(true)
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
            let second_try_paths = AppPaths::from_test_base(&std::env::temp_dir().join("torii-last-resort"))
                .unwrap_or(AppPaths {
                    config_dir: std::env::temp_dir().join("torii-config-last"),
                    data_dir: std::env::temp_dir().join("torii-data-last"),
                    cache_dir: std::env::temp_dir().join("torii-cache-last"),
                });
            Arc::new(
                Database::connect(&second_try_paths)
                    .unwrap_or_else(|fatal| panic!("unable to initialize fallback database: {fatal}")),
            )
        }
    };

    let blob_store = Arc::new(
        BlobStore::new(&fallback_paths).unwrap_or_else(|err| {
            panic!("unable to initialize fallback blob store: {err}");
        }),
    );

    let workspace_repo: WorkspaceRepoRef = Arc::new(SqliteWorkspaceRepository::new(db.clone()));
    let collection_repo: CollectionRepoRef = Arc::new(SqliteCollectionRepository::new(db.clone()));
    let folder_repo: FolderRepoRef = Arc::new(SqliteFolderRepository::new(db.clone()));
    let request_repo: RequestRepoRef = Arc::new(SqliteRequestRepository::new(db.clone()));
    let environment_repo: EnvironmentRepoRef = Arc::new(SqliteEnvironmentRepository::new(db.clone()));
    let history_repo: HistoryRepoRef = Arc::new(SqliteHistoryRepository::new(db.clone()));
    let preferences_repo: PreferencesRepoRef = Arc::new(SqlitePreferencesRepository::new(db.clone()));
    let secret_ref_repo: SecretRefRepoRef = Arc::new(SqliteSecretRefRepository::new(db.clone()));

    let secret_store: SecretStoreRef = Arc::new(InMemorySecretStore::new());
    let ui_preferences: UiPreferencesStoreRef = Arc::new(InMemoryUiPreferencesStore::new(Some(
        UiPreferencesSnapshot::default(),
    )));

    let recovery = RecoveryCoordinator::new(db.clone(), history_repo.clone(), blob_store.clone());
    let _ = recovery.run_startup_recovery();

    AppServices {
        paths: fallback_paths,
        db,
        blob_store,
        secret_store,
        repos: Repositories {
            workspace: workspace_repo,
            collection: collection_repo,
            folder: folder_repo,
            request: request_repo,
            environment: environment_repo,
            history: history_repo,
            preferences: preferences_repo,
            secret_refs: secret_ref_repo,
        },
        ui_preferences,
        recovery,
    }
}
