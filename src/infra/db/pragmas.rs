use anyhow::Result;
use sqlx::{Executor as _, SqlitePool};

pub async fn apply_startup_pragmas(pool: &SqlitePool) -> Result<()> {
    pool.execute("PRAGMA journal_mode = WAL;").await?;
    pool.execute("PRAGMA foreign_keys = ON;").await?;
    pool.execute("PRAGMA busy_timeout = 5000;").await?;
    pool.execute("PRAGMA synchronous = NORMAL;").await?;
    Ok(())
}
