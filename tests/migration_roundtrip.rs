mod common;

use std::path::Path;

use anyhow::{Context as _, Result, anyhow};
use sqlx::Connection as _;
use sqlx::Row as _;

#[test]
fn migration_roundtrip_creates_and_reuses_schema() -> Result<()> {
    let (paths, db) = common::test_database("migration-roundtrip")?;

    let tables = db.block_on(async {
        sqlx::query("SELECT name FROM sqlite_master WHERE type = 'table'")
            .fetch_all(db.pool())
            .await
    })?;
    let table_names = tables
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect::<Vec<_>>();

    for expected in [
        "workspaces",
        "collections",
        "folders",
        "requests",
        "environments",
        "ui_preferences",
        "history_index",
        "secret_refs",
        "tab_session_state",
        "tab_session_metadata",
        "_sqlx_migrations",
    ] {
        assert!(
            table_names.iter().any(|name| name == expected),
            "missing table: {expected}"
        );
    }

    let applied: i64 = db.block_on(async {
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success = 1")
            .fetch_one(db.pool())
            .await
    })?;
    assert_eq!(applied, 5);

    let journal_mode: String = db.block_on(async {
        sqlx::query_scalar("PRAGMA journal_mode;")
            .fetch_one(db.pool())
            .await
    })?;
    assert_eq!(journal_mode.to_lowercase(), "wal");

    let foreign_keys: i64 = db.block_on(async {
        sqlx::query_scalar("PRAGMA foreign_keys;")
            .fetch_one(db.pool())
            .await
    })?;
    assert_eq!(foreign_keys, 1);

    let busy_timeout: i64 = db.block_on(async {
        sqlx::query_scalar("PRAGMA busy_timeout;")
            .fetch_one(db.pool())
            .await
    })?;
    assert!(busy_timeout >= 5000);

    drop(db);

    let db2 = torii::infra::db::Database::connect(&paths)?;
    let applied2: i64 = db2.block_on(async {
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success = 1")
            .fetch_one(db2.pool())
            .await
    })?;
    assert_eq!(applied2, 5);

    Ok(())
}

#[test]
fn migration_upgrade_from_v1_applies_remaining_versions() -> Result<()> {
    let paths = common::test_paths("migration-upgrade-from-v1")?;
    let sqlite_path = paths.sqlite_path();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime for migration setup")?;

    runtime.block_on(async {
        let migrator = sqlx::migrate::Migrator::new(Path::new("./migrations")).await?;
        let v1 = migrator
            .iter()
            .find(|migration| migration.version == 1)
            .ok_or_else(|| anyhow!("missing v1 migration"))?;

        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&sqlite_path)
            .create_if_missing(true)
            .foreign_keys(true);
        let mut conn = sqlx::SqliteConnection::connect_with(&options).await?;

        sqlx::query(v1.sql.as_ref()).execute(&mut conn).await?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS _sqlx_migrations (
                version BIGINT PRIMARY KEY,
                description TEXT NOT NULL,
                installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                success BOOLEAN NOT NULL,
                checksum BLOB NOT NULL,
                execution_time BIGINT NOT NULL
            );
            "#,
        )
        .execute(&mut conn)
        .await?;

        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
             VALUES (?, ?, TRUE, ?, 0)",
        )
        .bind(v1.version)
        .bind(v1.description.as_ref())
        .bind(v1.checksum.as_ref())
        .execute(&mut conn)
        .await?;

        Ok::<(), anyhow::Error>(())
    })?;

    let upgraded = torii::infra::db::Database::connect(&paths)?;
    let applied_after_upgrade: i64 = upgraded.block_on(async {
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success = 1")
            .fetch_one(upgraded.pool())
            .await
    })?;
    assert_eq!(applied_after_upgrade, 5);

    Ok(())
}
