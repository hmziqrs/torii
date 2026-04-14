mod about;
mod form_page;
mod home;
pub mod http_method;
pub mod item_tabs;
mod settings;
pub mod tab_host;

pub use about::AboutPage;
pub use form_page::FormPage;
pub use home::HomePage;
pub use http_method::{method_badge, method_color};
pub use settings::SettingsPage;
