mod common;

use anyhow::Result;
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
    assert_eq!(applied, 3);

    drop(db);

    let db2 = gpui_starter::infra::db::Database::connect(&paths)?;
    let applied2: i64 = db2.block_on(async {
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success = 1")
            .fetch_one(db2.pool())
            .await
    })?;
    assert_eq!(applied2, 3);

    Ok(())
}
