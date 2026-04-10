mod app;
mod infra;
mod menus;
mod root;
mod services;
mod sidebar;
mod title_bar;
mod views;

es_fluent_manager_embedded::define_i18n_module!();

use gpui_component_assets::Assets;

fn main() {
    let app = gpui_platform::application().with_assets(Assets);
    app.run(move |cx| {
        app::init(cx);
        cx.activate(true);

        app::create_new_window("My App", cx);
    });
}
