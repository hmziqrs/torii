use anyhow::Result;
use torii::infra::{db::Database, paths::AppPaths};
use uuid::Uuid;

pub fn test_paths(suite: &str) -> Result<AppPaths> {
    let base = std::env::temp_dir().join(format!("torii-{suite}-{}", Uuid::now_v7()));
    AppPaths::from_test_base(&base)
}

pub fn test_database(suite: &str) -> Result<(AppPaths, Database)> {
    let paths = test_paths(suite)?;
    let db = Database::connect(&paths)?;
    Ok((paths, db))
}
