mod common;

use std::sync::Arc;

use anyhow::Result;
use chrono::{Datelike, Duration, Local, TimeZone as _, Timelike};
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

#[test]
fn history_query_started_before_filters_by_local_day_end() -> Result<()> {
    let (_paths, db) = common::test_database("history-query-started-before")?;
    let db = Arc::new(db);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let history_repo = SqliteHistoryRepository::new(db.clone());

    let workspace = workspace_repo.create("Main")?;
    let early =
        history_repo.create_pending(workspace.id, None, "GET", "https://api.local/early", None)?;
    let edge =
        history_repo.create_pending(workspace.id, None, "GET", "https://api.local/edge", None)?;
    let late =
        history_repo.create_pending(workspace.id, None, "GET", "https://api.local/late", None)?;

    let day = chrono::NaiveDate::from_ymd_opt(2026, 4, 28).expect("valid date");
    let day_start = local_ts_ms(day, 0, 0, 0, 0);
    let day_end = local_ts_ms(day, 23, 59, 59, 999);
    let next_day_start = local_ts_ms(day + Duration::days(1), 0, 0, 0, 0);

    db.block_on(async {
        sqlx::query("UPDATE history_index SET started_at = ? WHERE id = ?")
            .bind(day_start)
            .bind(early.id.to_string())
            .execute(db.pool())
            .await?;
        sqlx::query("UPDATE history_index SET started_at = ? WHERE id = ?")
            .bind(day_end)
            .bind(edge.id.to_string())
            .execute(db.pool())
            .await?;
        sqlx::query("UPDATE history_index SET started_at = ? WHERE id = ?")
            .bind(next_day_start)
            .bind(late.id.to_string())
            .execute(db.pool())
            .await?;
        Ok::<(), sqlx::Error>(())
    })?;

    let mut query = HistoryQuery::for_workspace(workspace.id);
    query.started_before = Some(day_end);
    query.limit = 10;
    let page = history_repo.query(query)?;

    let ids: std::collections::HashSet<_> = page.rows.iter().map(|it| it.id).collect();
    assert!(ids.contains(&early.id));
    assert!(ids.contains(&edge.id));
    assert!(!ids.contains(&late.id));

    Ok(())
}

#[test]
fn history_query_started_after_filters_by_local_day_start() -> Result<()> {
    let (_paths, db) = common::test_database("history-query-started-after")?;
    let db = Arc::new(db);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let history_repo = SqliteHistoryRepository::new(db.clone());

    let workspace = workspace_repo.create("Main")?;
    let prev =
        history_repo.create_pending(workspace.id, None, "GET", "https://api.local/prev", None)?;
    let start =
        history_repo.create_pending(workspace.id, None, "GET", "https://api.local/start", None)?;
    let later =
        history_repo.create_pending(workspace.id, None, "GET", "https://api.local/later", None)?;

    let day = chrono::NaiveDate::from_ymd_opt(2026, 4, 28).expect("valid date");
    let prev_day_end = local_ts_ms(day - Duration::days(1), 23, 59, 59, 999);
    let day_start = local_ts_ms(day, 0, 0, 0, 0);
    let midday = local_ts_ms(day, 12, 0, 0, 0);

    db.block_on(async {
        sqlx::query("UPDATE history_index SET started_at = ? WHERE id = ?")
            .bind(prev_day_end)
            .bind(prev.id.to_string())
            .execute(db.pool())
            .await?;
        sqlx::query("UPDATE history_index SET started_at = ? WHERE id = ?")
            .bind(day_start)
            .bind(start.id.to_string())
            .execute(db.pool())
            .await?;
        sqlx::query("UPDATE history_index SET started_at = ? WHERE id = ?")
            .bind(midday)
            .bind(later.id.to_string())
            .execute(db.pool())
            .await?;
        Ok::<(), sqlx::Error>(())
    })?;

    let mut query = HistoryQuery::for_workspace(workspace.id);
    query.started_after = Some(day_start);
    query.limit = 10;
    let page = history_repo.query(query)?;

    let ids: std::collections::HashSet<_> = page.rows.iter().map(|it| it.id).collect();
    assert!(!ids.contains(&prev.id));
    assert!(ids.contains(&start.id));
    assert!(ids.contains(&later.id));

    Ok(())
}

#[test]
fn history_query_filters_informational_and_redirection_families() -> Result<()> {
    let (_paths, db) = common::test_database("history-query-status-families")?;
    let db = Arc::new(db);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let history_repo = SqliteHistoryRepository::new(db.clone());

    let workspace = workspace_repo.create("Main")?;
    let informational =
        history_repo.create_pending(workspace.id, None, "GET", "https://api.local/101", None)?;
    let redirection =
        history_repo.create_pending(workspace.id, None, "GET", "https://api.local/302", None)?;
    let client_error =
        history_repo.create_pending(workspace.id, None, "GET", "https://api.local/404", None)?;

    history_repo.finalize_completed(
        informational.id,
        101,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;
    history_repo.finalize_completed(
        redirection.id,
        302,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;
    history_repo.finalize_completed(
        client_error.id,
        404,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;

    let mut query = HistoryQuery::for_workspace(workspace.id);
    query.status_family = Some(StatusFamily::Informational);
    let informational_page = history_repo.query(query)?;
    assert_eq!(informational_page.rows.len(), 1);
    assert_eq!(informational_page.rows[0].id, informational.id);

    let mut query = HistoryQuery::for_workspace(workspace.id);
    query.status_family = Some(StatusFamily::Redirection);
    let redirection_page = history_repo.query(query)?;
    assert_eq!(redirection_page.rows.len(), 1);
    assert_eq!(redirection_page.rows[0].id, redirection.id);

    Ok(())
}

#[test]
fn history_delete_before_prunes_old_rows() -> Result<()> {
    let (_paths, db) = common::test_database("history-delete-before")?;
    let db = Arc::new(db);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let history_repo = SqliteHistoryRepository::new(db.clone());
    let workspace = workspace_repo.create("Main")?;

    let old =
        history_repo.create_pending(workspace.id, None, "GET", "https://api.local/old", None)?;
    let keep =
        history_repo.create_pending(workspace.id, None, "GET", "https://api.local/keep", None)?;
    db.block_on(async {
        sqlx::query("UPDATE history_index SET started_at = ? WHERE id = ?")
            .bind(1_700_000_000_000_i64)
            .bind(old.id.to_string())
            .execute(db.pool())
            .await?;
        sqlx::query("UPDATE history_index SET started_at = ? WHERE id = ?")
            .bind(1_900_000_000_000_i64)
            .bind(keep.id.to_string())
            .execute(db.pool())
            .await?;
        Ok::<(), sqlx::Error>(())
    })?;

    let removed = history_repo.delete_before(workspace.id, 1_800_000_000_000_i64)?;
    assert_eq!(removed, 1);
    let rows = history_repo.list_recent(workspace.id, 10)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, keep.id);
    Ok(())
}

#[test]
fn history_query_returns_message_counts_and_close_reason_fields() -> Result<()> {
    let (_paths, db) = common::test_database("history-query-message-metadata")?;
    let db = Arc::new(db);
    let workspace_repo = SqliteWorkspaceRepository::new(db.clone());
    let history_repo = SqliteHistoryRepository::new(db.clone());

    let workspace = workspace_repo.create("Main")?;
    let row = history_repo.create_pending(
        workspace.id,
        None,
        "GET",
        "https://api.local/messages",
        None,
    )?;
    history_repo.finalize_completed(row.id, 200, None, None, None, None, None, None, None)?;

    db.block_on(async {
        sqlx::query(
            "UPDATE history_index
             SET request_name = ?, message_count_in = ?, message_count_out = ?, close_reason = ?
             WHERE id = ?",
        )
        .bind("Synthetic conversation request")
        .bind(11_i64)
        .bind(7_i64)
        .bind("server_closed")
        .bind(row.id.to_string())
        .execute(db.pool())
        .await?;
        Ok::<(), sqlx::Error>(())
    })?;

    let page = history_repo.query(HistoryQuery::for_workspace(workspace.id))?;
    assert_eq!(page.rows.len(), 1);
    let loaded = &page.rows[0];
    assert_eq!(
        loaded.request_name.as_deref(),
        Some("Synthetic conversation request")
    );
    assert_eq!(loaded.message_count_in, Some(11));
    assert_eq!(loaded.message_count_out, Some(7));
    assert_eq!(loaded.close_reason.as_deref(), Some("server_closed"));

    Ok(())
}

fn local_ts_ms(date: chrono::NaiveDate, hour: u32, minute: u32, second: u32, millis: u32) -> i64 {
    let dt = Local
        .with_ymd_and_hms(date.year(), date.month(), date.day(), hour, minute, second)
        .single()
        .expect("unambiguous local datetime")
        + Duration::milliseconds(i64::from(millis));
    assert_eq!(dt.hour(), hour);
    assert_eq!(dt.minute(), minute);
    assert_eq!(dt.second(), second);
    dt.timestamp_millis()
}
