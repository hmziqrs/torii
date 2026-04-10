use serde::{Deserialize, Serialize};

use gpui::SharedString;
use gpui_component::scroll::ScrollbarShow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPreferences {
    pub theme: SharedString,
    pub scrollbar_show: Option<ScrollbarShow>,
    pub theme_mode: Option<String>,
    pub locale: Option<String>,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            theme: "Default Light".into(),
            scrollbar_show: None,
            theme_mode: Some("light".to_string()),
            locale: Some("en".to_string()),
        }
    }
}
