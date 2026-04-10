use serde::{Deserialize, Serialize};

use gpui::SharedString;
use gpui_component::scroll::ScrollbarShow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPreferences {
    pub theme: SharedString,
    pub scrollbar_show: Option<ScrollbarShow>,
    pub theme_mode: Option<String>,
    pub locale: Option<String>,
    pub font_size_px: Option<i32>,
    pub radius_px: Option<i32>,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            theme: "Default Light".into(),
            scrollbar_show: None,
            theme_mode: Some("light".to_string()),
            locale: Some("en".to_string()),
            font_size_px: Some(16),
            radius_px: Some(6),
        }
    }
}
