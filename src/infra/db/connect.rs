use std::path::Path;

use anyhow::Result;
use sqlx::{
    ConnectOptions as _, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

use super::pragmas;

pub async fn connect_pool(path: &Path) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5))
        .disable_statement_logging();

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;

    pragmas::apply_startup_pragmas(&pool).await?;

    Ok(pool)
}
