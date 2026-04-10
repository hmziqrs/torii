use gpui_component_assets::Assets;
use torii::app;

fn main() {
    let app = gpui_platform::application().with_assets(Assets);
    app.run(move |cx| {
        app::init(cx);
        cx.activate(true);

        app::create_new_window(&es_fluent::localize("window_title_main", None), cx);
    });
}
