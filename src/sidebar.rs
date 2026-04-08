use gpui_component::IconName;

// ---------------------------------------------------------------------------
// Navigation pages
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Page {
    Home,
    Settings,
    About,
}

impl Page {
    pub fn title(&self) -> &'static str {
        match self {
            Page::Home => "Home",
            Page::Settings => "Settings",
            Page::About => "About",
        }
    }

    pub fn icon(&self) -> IconName {
        match self {
            Page::Home => IconName::Inbox,
            Page::Settings => IconName::Settings2,
            Page::About => IconName::Info,
        }
    }

    pub fn all() -> &'static [Page] {
        &[Page::Home, Page::Settings, Page::About]
    }
}
