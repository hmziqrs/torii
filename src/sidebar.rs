use gpui_component::IconName;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Page {
    Home,
    Form,
    Settings,
    About,
}

impl Page {
    pub fn title(&self) -> String {
        match self {
            Page::Home => es_fluent::localize("page-Home", None),
            Page::Form => es_fluent::localize("page-Form", None),
            Page::Settings => es_fluent::localize("page-Settings", None),
            Page::About => es_fluent::localize("page-About", None),
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
