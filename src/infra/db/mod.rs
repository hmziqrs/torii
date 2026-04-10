use std::{path::PathBuf, sync::Arc};

use anyhow::{Context as _, Result};
use sqlx::{SqlitePool, migrate::Migrator};
use tokio::runtime::Runtime;

use crate::infra::paths::AppPaths;

pub mod connect;
pub mod pragmas;
pub mod row_types;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
    runtime: Arc<Runtime>,
    path: PathBuf,
}

impl Database {
    pub fn connect(paths: &AppPaths) -> Result<Self> {
        let path = paths.sqlite_path();
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("failed to build tokio runtime for sqlite")?,
        );

        let pool = runtime
            .block_on(connect::connect_pool(&path))
            .with_context(|| format!("failed to connect sqlite at {}", path.display()))?;

        runtime
            .block_on(async {
                MIGRATOR.run(&pool).await?;
                Ok::<_, sqlx::Error>(())
            })
            .context("failed to run sqlite migrations")?;

        let applied_version = runtime
            .block_on(async {
                sqlx::query_scalar::<_, Option<i64>>(
                    "SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1",
                )
                .fetch_one(&pool)
                .await
            })
            .context("failed to query migration version")?
            .unwrap_or_default();

        tracing::info!(
            sqlite_path = %path.display(),
            migration_version = applied_version,
            "sqlite initialized"
        );

        Ok(Self {
            pool,
            runtime,
            path,
        })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn block_on<T>(&self, future: impl std::future::Future<Output = T>) -> T {
        self.runtime.block_on(future)
    }
}
