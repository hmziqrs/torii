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
        "history_blob_refs",
        "secret_refs",
        "startup_recovery_log",
        "tab_session_state",
        "tab_session_metadata",
        "tab_session_workspace_state",
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

    let environment_columns = db.block_on(async {
        sqlx::query("PRAGMA table_info(environments);")
            .fetch_all(db.pool())
            .await
    })?;
    let environment_column_names = environment_columns
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect::<Vec<_>>();
    assert!(
        environment_column_names
            .iter()
            .any(|name| name == "workspace_id"),
        "environments table must contain workspace_id"
    );
    assert!(
        environment_column_names
            .iter()
            .all(|name| name != "collection_id"),
        "environments table must not contain collection_id"
    );

    let request_columns = db.block_on(async {
        sqlx::query("PRAGMA table_info(requests);")
            .fetch_all(db.pool())
            .await
    })?;
    let request_column_names = request_columns
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect::<Vec<_>>();
    assert!(
        request_column_names
            .iter()
            .any(|name| name == "protocol_kind"),
        "requests table must contain protocol_kind"
    );
    assert!(
        request_column_names
            .iter()
            .any(|name| name == "protocol_config_json"),
        "requests table must contain protocol_config_json"
    );

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
