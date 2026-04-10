mod common;

use std::sync::Arc;

use anyhow::Result;
use gpui_starter::{
    repos::preferences_repo::{PreferencesRepository, SqlitePreferencesRepository},
    services::{
        startup::migrate_legacy_state_json_if_needed_at_path,
        ui_preferences::UiPreferencesSnapshot,
    },
};

#[test]
fn migrates_legacy_state_json_once() -> Result<()> {
    let (_paths, db) = common::test_database("settings-migration")?;
    let db = Arc::new(db);
    let preferences_repo = Arc::new(SqlitePreferencesRepository::new(db));

    let legacy_dir = std::env::temp_dir().join(format!("torii-legacy-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&legacy_dir)?;
    let legacy_path = legacy_dir.join("state.json");
    let legacy = UiPreferencesSnapshot {
        theme: "Ayu Light".into(),
        scrollbar_show: None,
        theme_mode: Some("light".to_string()),
        locale: Some("zh-CN".to_string()),
    };
    std::fs::write(&legacy_path, serde_json::to_string_pretty(&legacy)?)?;

    let migrated = migrate_legacy_state_json_if_needed_at_path(&preferences_repo, &legacy_path)?;
    assert!(migrated);

    let saved = preferences_repo
        .load_ui_preferences()?
        .expect("preferences should be persisted");
    assert_eq!(saved.theme.as_ref(), "Ayu Light");
    assert_eq!(saved.locale.as_deref(), Some("zh-CN"));

    std::fs::write(
        &legacy_path,
        serde_json::to_string_pretty(&UiPreferencesSnapshot {
            theme: "Should Not Override".into(),
            scrollbar_show: None,
            theme_mode: Some("dark".to_string()),
            locale: Some("en".to_string()),
        })?,
    )?;
    let migrated_again =
        migrate_legacy_state_json_if_needed_at_path(&preferences_repo, &legacy_path)?;
    assert!(!migrated_again);

    let still_saved = preferences_repo
        .load_ui_preferences()?
        .expect("preferences should remain present");
    assert_eq!(still_saved.theme.as_ref(), "Ayu Light");

    Ok(())
}
