use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowLayoutState {
    pub sidebar_collapsed: bool,
    pub sidebar_width_px: f32,
}

impl Default for WindowLayoutState {
    fn default() -> Self {
        Self {
            sidebar_collapsed: false,
            sidebar_width_px: 255.0,
        }
    }
}
