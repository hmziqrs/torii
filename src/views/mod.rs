mod about;
mod form_page;
mod home;
pub mod http_method;
pub mod item_tabs;
mod layout_debug;
mod settings;
pub mod tab_host;

pub use about::AboutPage;
pub use form_page::FormPage;
pub use home::HomePage;
pub use http_method::{RequestProtocol, method_badge, method_color, protocol_badge};
pub use layout_debug::LayoutDebugPage;
pub use settings::SettingsPage;
