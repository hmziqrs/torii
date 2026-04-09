use gpui_component::IconName;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Page {
    Home,
    Form,
    Settings,
    About,
}

impl Page {
    pub fn title(&self) -> &'static str {
        match self {
            Page::Home => "Home",
            Page::Form => "Form",
            Page::Settings => "Settings",
            Page::About => "About",
        }
    }

    pub fn icon(&self) -> IconName {
        match self {
            Page::Home => IconName::Inbox,
            Page::Form => IconName::File,
            Page::Settings => IconName::Settings2,
            Page::About => IconName::Info,
        }
    }

    pub fn all() -> &'static [Page] {
        &[Page::Home, Page::Form, Page::Settings, Page::About]
    }
}
