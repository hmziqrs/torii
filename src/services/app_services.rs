use std::sync::Arc;

use gpui::Global;

use crate::infra::paths::AppPaths;

use super::ui_preferences::UiPreferencesStoreRef;

#[derive(Clone)]
pub struct AppServices {
    pub paths: AppPaths,
    pub ui_preferences: UiPreferencesStoreRef,
}

#[derive(Clone)]
pub struct AppServicesGlobal(pub Arc<AppServices>);

impl Global for AppServicesGlobal {}
