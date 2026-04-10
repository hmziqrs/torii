mod common;

use std::sync::Arc;

use anyhow::Result;
use torii::{
    repos::preferences_repo::{PreferencesRepoRef, SqlitePreferencesRepository},
    services::ui_preferences::{
        SqliteUiPreferencesStore, UiPreferencesSnapshot, UiPreferencesStore,
    },
};

#[test]
fn ui_preferences_persist_theme_locale_font_and_radius() -> Result<()> {
    let (_paths, db) = common::test_database("ui-preferences-roundtrip")?;
    let db = Arc::new(db);
    let repo: PreferencesRepoRef = Arc::new(SqlitePreferencesRepository::new(db));
    let store = SqliteUiPreferencesStore::new(repo.clone());

    let snapshot = UiPreferencesSnapshot {
        theme: "Ayu Light".into(),
        scrollbar_show: None,
        theme_mode: Some("dark".to_string()),
        locale: Some("zh-CN".to_string()),
        font_size_px: Some(18),
        radius_px: Some(4),
    };
    store.save(&snapshot)?;

    let loaded = store.load()?.expect("snapshot should persist");
    assert_eq!(loaded.theme.as_ref(), "Ayu Light");
    assert_eq!(loaded.theme_mode.as_deref(), Some("dark"));
    assert_eq!(loaded.locale.as_deref(), Some("zh-CN"));
    assert_eq!(loaded.font_size_px, Some(18));
    assert_eq!(loaded.radius_px, Some(4));

    let raw = repo
        .load_ui_preferences()?
        .expect("raw preferences should persist");
    assert_eq!(raw.font_size_px, Some(18));
    assert_eq!(raw.radius_px, Some(4));

    Ok(())
}
