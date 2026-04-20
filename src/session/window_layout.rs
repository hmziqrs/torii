use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SidebarSection {
    Collections,
    Environments,
}

impl Default for SidebarSection {
    fn default() -> Self {
        Self::Collections
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowLayoutState {
    pub sidebar_collapsed: bool,
    pub sidebar_width_px: f32,
    pub sidebar_section: SidebarSection,
}

impl Default for WindowLayoutState {
    fn default() -> Self {
        Self {
            sidebar_collapsed: false,
            sidebar_width_px: 260.0,
            sidebar_section: SidebarSection::default(),
        }
    }
}
