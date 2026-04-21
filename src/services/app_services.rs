use std::sync::Arc;

use gpui::Global;

use crate::{
    infra::{blobs::BlobStore, db::Database, paths::AppPaths, secrets::SecretStoreRef},
    repos::{
        collection_repo::CollectionRepoRef, environment_repo::EnvironmentRepoRef,
        folder_repo::FolderRepoRef, history_repo::HistoryRepoRef,
        preferences_repo::PreferencesRepoRef, request_repo::RequestRepoRef,
        secret_ref_repo::SecretRefRepoRef, tab_session_repo::TabSessionRepoRef,
        workspace_repo::WorkspaceRepoRef,
    },
};

use super::{
    recovery::RecoveryCoordinator, request_execution::RequestExecutionService,
    secret_manager::SecretManager, session_restore::SessionRestoreService,
    tokio_runtime::TokioRuntime, ui_preferences::UiPreferencesStoreRef,
};

#[derive(Clone)]
pub struct Repositories {
    pub workspace: WorkspaceRepoRef,
    pub collection: CollectionRepoRef,
    pub folder: FolderRepoRef,
    pub request: RequestRepoRef,
    pub environment: EnvironmentRepoRef,
    pub history: HistoryRepoRef,
    pub preferences: PreferencesRepoRef,
    pub secret_refs: SecretRefRepoRef,
    pub tab_session: TabSessionRepoRef,
}

#[derive(Clone)]
pub struct AppServices {
    pub paths: AppPaths,
    pub db: Arc<Database>,
    pub io_runtime: Arc<TokioRuntime>,
    pub request_execution: Arc<RequestExecutionService>,
    pub blob_store: Arc<BlobStore>,
    pub secret_store: SecretStoreRef,
    pub secret_manager: SecretManager,
    pub repos: Repositories,
    pub ui_preferences: UiPreferencesStoreRef,
    pub recovery: RecoveryCoordinator,
    pub session_restore: SessionRestoreService,
}

#[derive(Clone)]
pub struct AppServicesGlobal(pub Arc<AppServices>);

impl Global for AppServicesGlobal {}
