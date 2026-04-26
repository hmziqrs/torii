mod common;

use std::sync::Arc;

use anyhow::Result;
use torii::{
    domain::history::{HistoryQuery, HistoryState, StatusFamily},
    repos::{
        history_repo::{HistoryRepository, SqliteHistoryRepository},
        workspace_repo::{SqliteWorkspaceRepository, WorkspaceRepository},
    },
};

#[test]
fn history_query_cursor_is_stable_with_same_started_at() -> Result<()> {
    let (_paths, db) = common::test_database("history-query-cursor")?;
    let db = Arc::new(db);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let history_repo = SqliteHistoryRepository::new(db.clone());

    let workspace = workspace_repo.create("Main")?;

    let e1 = history_repo.create_pending(workspace.id, None, "GET", "https://api.local/1", None)?;
    let e2 = history_repo.create_pending(workspace.id, None, "GET", "https://api.local/2", None)?;
    let e3 = history_repo.create_pending(workspace.id, None, "GET", "https://api.local/3", None)?;

    db.block_on(async {
        sqlx::query("UPDATE history_index SET started_at = ? WHERE id IN (?, ?, ?)")
            .bind(1_800_000_000_000_i64)
            .bind(e1.id.to_string())
            .bind(e2.id.to_string())
            .bind(e3.id.to_string())
            .execute(db.pool())
            .await
    })?;

    let mut query = HistoryQuery::for_workspace(workspace.id);
    query.limit = 2;
    let first = history_repo.query(query.clone())?;
    assert_eq!(first.rows.len(), 2);
    assert!(first.next_cursor.is_some());

    let mut second_query = query;
    second_query.cursor = first.next_cursor.clone();
    let second = history_repo.query(second_query)?;
    assert_eq!(second.rows.len(), 1);

    let mut seen = std::collections::HashSet::new();
    for row in first.rows.iter().chain(second.rows.iter()) {
        assert!(seen.insert(row.id), "duplicate row in cursor pagination");
    }
    assert_eq!(seen.len(), 3);

    Ok(())
}

#[test]
fn history_query_filters_by_workspace_protocol_state_status() -> Result<()> {
    let (_paths, db) = common::test_database("history-query-filters")?;
    let db = Arc::new(db);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let history_repo = SqliteHistoryRepository::new(db.clone());

    let w1 = workspace_repo.create("W1")?;
    let w2 = workspace_repo.create("W2")?;

    let completed =
        history_repo.create_pending(w1.id, None, "POST", "https://api.local/graphql", None)?;
    history_repo.finalize_completed(completed.id, 201, None, None, None, None, None, None, None)?;
    db.block_on(async {
        sqlx::query("UPDATE history_index SET protocol_kind = 'graphql' WHERE id = ?")
            .bind(completed.id.to_string())
            .execute(db.pool())
            .await
    })?;

    let failed = history_repo.create_pending(w1.id, None, "GET", "https://api.local/http", None)?;
    history_repo.mark_failed(failed.id, "boom")?;

    let other = history_repo.create_pending(w2.id, None, "GET", "https://api.local/other", None)?;
    history_repo.finalize_completed(other.id, 200, None, None, None, None, None, None, None)?;

    let mut query = HistoryQuery::for_workspace(w1.id);
    query.protocol = Some("graphql".to_string());
    query.state = Some(HistoryState::Completed);
    query.status_family = Some(StatusFamily::Success);

    let page = history_repo.query(query)?;
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].id, completed.id);

    Ok(())
}

#[test]
fn history_query_clamps_limit() -> Result<()> {
    let (_paths, db) = common::test_database("history-query-limit")?;
    let db = Arc::new(db);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let history_repo = SqliteHistoryRepository::new(db.clone());

    let workspace = workspace_repo.create("Main")?;
    for i in 0..260 {
        let _ = history_repo.create_pending(
            workspace.id,
            None,
            "GET",
            &format!("https://api.local/{i}"),
            None,
        )?;
    }

    let mut query = HistoryQuery::for_workspace(workspace.id);
    query.limit = 5000;
    let page = history_repo.query(query)?;

    assert_eq!(page.rows.len(), 200);
    assert!(page.next_cursor.is_some());

    Ok(())
}

#[test]
fn history_query_url_search_filters_case_insensitively() -> Result<()> {
    let (_paths, db) = common::test_database("history-query-url-search")?;
    let db = Arc::new(db);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let history_repo = SqliteHistoryRepository::new(db.clone());

    let workspace = workspace_repo.create("Main")?;
    let keep = history_repo.create_pending(
        workspace.id,
        None,
        "GET",
        "https://Example.COM/API/Users",
        None,
    )?;
    let _drop = history_repo.create_pending(
        workspace.id,
        None,
        "GET",
        "https://service.local/health",
        None,
    )?;

    let mut query = HistoryQuery::for_workspace(workspace.id);
    query.url_search = Some("example.com/api".to_string());
    let page = history_repo.query(query)?;
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].id, keep.id);

    Ok(())
}

#[test]
fn history_finalize_terminal_row_is_idempotent() -> Result<()> {
    let (_paths, db) = common::test_database("history-finalize-idempotent")?;
    let db = Arc::new(db);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let history_repo = SqliteHistoryRepository::new(db.clone());

    let workspace = workspace_repo.create("Main")?;
    let row = history_repo.create_pending(workspace.id, None, "GET", "https://api.local", None)?;
    history_repo.finalize_completed(row.id, 200, None, None, None, None, None, None, None)?;

    history_repo.finalize_cancelled(row.id, Some(10))?;
    history_repo.mark_failed(row.id, "late")?;

    let rows = history_repo.list_recent(workspace.id, 10)?;
    let row = rows
        .into_iter()
        .find(|it| it.id == row.id)
        .expect("row exists");
    assert_eq!(row.state, HistoryState::Completed);
    assert_eq!(row.status_code, Some(200));

    Ok(())
}
