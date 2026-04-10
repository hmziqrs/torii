use std::sync::Arc;

use anyhow::Result;

use crate::infra::paths::AppPaths;

use super::{
    app_services::AppServices,
    ui_preferences::{InMemoryUiPreferencesStore, JsonUiPreferencesStore},
};

pub fn bootstrap_app_services() -> Arc<AppServices> {
    match build_app_services() {
        Ok(services) => Arc::new(services),
        Err(err) => {
            tracing::error!("failed to bootstrap app services: {err}");
            Arc::new(fallback_app_services())
        }
    }
}

fn build_app_services() -> Result<AppServices> {
    let paths = AppPaths::from_system()?;
    let ui_preferences = Arc::new(JsonUiPreferencesStore::new(paths.ui_preferences_path()));

    Ok(AppServices {
        paths,
        ui_preferences,
    })
}

fn fallback_app_services() -> AppServices {
    AppServices {
        paths: AppPaths {
            config_dir: std::env::temp_dir().join("torii-config"),
            data_dir: std::env::temp_dir().join("torii-data"),
            cache_dir: std::env::temp_dir().join("torii-cache"),
        },
        ui_preferences: Arc::new(InMemoryUiPreferencesStore::new(None)),
    }
}
