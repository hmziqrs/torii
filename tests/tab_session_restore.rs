mod common;

use std::sync::Arc;

use anyhow::Result;
use torii::{
    repos::{
        collection_repo::{CollectionRepository, SqliteCollectionRepository},
        environment_repo::{EnvironmentRepository, SqliteEnvironmentRepository},
        folder_repo::SqliteFolderRepository,
        request_repo::{RequestRepository, SqliteRequestRepository},
        tab_session_repo::{SqliteTabSessionRepository, TabSessionRepository},
        workspace_repo::{SqliteWorkspaceRepository, WorkspaceRepository},
    },
    services::session_restore::SessionRestoreService,
    session::{
        item_key::{ItemKey, TabKey},
        tab_manager::TabState,
        workspace_session::SessionId,
    },
};

#[test]
fn tab_session_repo_roundtrip_and_restore_skip_missing_items() -> Result<()> {
    let (_paths, db) = common::test_database("tab-session-restore")?;
    let db = Arc::new(db);

    let workspace_repo = Arc::new(SqliteWorkspaceRepository::new(db.clone()));
    let collection_repo = Arc::new(SqliteCollectionRepository::new(db.clone()));
    let folder_repo = Arc::new(SqliteFolderRepository::new(db.clone()));
    let request_repo = Arc::new(SqliteRequestRepository::new(db.clone()));
    let environment_repo = Arc::new(SqliteEnvironmentRepository::new(db.clone()));
    let tab_session_repo = Arc::new(SqliteTabSessionRepository::new(db.clone()));

    let workspace = workspace_repo.create("Main")?;
    let collection = collection_repo.create(workspace.id, "Collection")?;
    let request = request_repo.create(collection.id, None, "Request", "GET", "https://example.test")?;
    let environment = environment_repo.create(workspace.id, "Env")?;

    let session_id = SessionId::new();
    let tabs = vec![
        TabState::new(ItemKey::workspace(workspace.id)),
        TabState::new(ItemKey::request(request.id)),
        TabState::new(ItemKey::collection(torii::domain::ids::CollectionId::new())),
        TabState::new(ItemKey::environment(environment.id)),
        TabState::new(ItemKey::settings()),
    ];
    tab_session_repo.save_session(session_id, &tabs, Some(TabKey::from(ItemKey::environment(environment.id))))?;

    let roundtrip = tab_session_repo
        .load_session(session_id)?
        .expect("session should be stored");
    assert_eq!(roundtrip.tabs, tabs);
    assert_eq!(roundtrip.active, Some(TabKey::from(ItemKey::environment(environment.id))));

    let restore = SessionRestoreService::new(
        tab_session_repo.clone(),
        workspace_repo,
        collection_repo,
        folder_repo,
        request_repo,
        environment_repo,
    )
    .restore_snapshot(roundtrip)?
    .expect("restore should keep valid tabs");

    assert_eq!(
        restore.tabs.iter().map(|tab| tab.key).collect::<Vec<_>>(),
        vec![
            TabKey::from(ItemKey::workspace(workspace.id)),
            TabKey::from(ItemKey::request(request.id)),
            TabKey::from(ItemKey::environment(environment.id)),
            TabKey::from(ItemKey::settings()),
        ]
    );
    assert_eq!(
        restore.active,
        Some(TabKey::from(ItemKey::environment(environment.id)))
    );
    assert_eq!(restore.selected_workspace_id, Some(workspace.id));

    Ok(())
}
