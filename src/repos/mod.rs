pub mod collection_repo;
pub mod environment_repo;
pub mod folder_repo;
pub mod history_repo;
pub mod preferences_repo;
pub mod request_repo;
pub mod secret_ref_repo;
pub mod workspace_repo;

use std::sync::Arc;

use crate::infra::db::Database;

pub type RepoResult<T> = anyhow::Result<T>;
pub type DbRef = Arc<Database>;
