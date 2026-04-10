use gpui::{
    Action, App, AppContext as _, Bounds, Focusable as _, KeyBinding, SharedString, WindowBounds,
    WindowKind, WindowOptions, actions, px, size,
};
use gpui_component::{ActiveTheme, Root, TitleBar, WindowExt, text::markdown};

use crate::services::{
    app_services::AppServicesGlobal, startup::bootstrap_app_services,
    ui_preferences::UiPreferencesSnapshot,
};

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

actions!(app, [About, Quit, ToggleSearch]);

#[derive(Action, Clone, PartialEq, Eq, serde::Deserialize)]
#[action(namespace = app, no_json)]
pub struct SelectLocale(pub SharedString);

#[derive(Action, Clone, PartialEq, Eq, serde::Deserialize)]
#[action(namespace = app, no_json)]
pub struct SelectFont(pub usize);

#[derive(Action, Clone, PartialEq, Eq, serde::Deserialize)]
#[action(namespace = app, no_json)]
pub struct SelectRadius(pub usize);

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

pub fn init(cx: &mut App) {
    use tracing_subscriber::{layer::SubscriberExt as _, util::SubscriberInitExt as _};
    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("gpui_starter=trace".parse().unwrap()),
        )
        .try_init();

    // Must be called before using any gpui-component features.
    gpui_component::init(cx);

    // Initialize es-fluent i18n for app and form text
    es_fluent_manager_embedded::init();
    let startup_locale = rust_i18n::locale()
        .parse()
        .unwrap_or_else(|_| es_fluent::unic_langid::langid!("en"));
    es_fluent_manager_embedded::select_language(startup_locale);

    let services = bootstrap_app_services();
    cx.set_global(AppServicesGlobal(services.clone()));

    let persisted = match services.ui_preferences.load() {
        Ok(value) => value,
        Err(err) => {
            tracing::error!("failed to load UI preferences: {err}");
            None
        }
    };

    // Load extra themes from the themes/ directory (with hot-reload)
    let persisted_for_closure = persisted.clone();
    if let Err(err) = gpui_component::ThemeRegistry::watch_dir(
        std::path::PathBuf::from("./themes"),
        cx,
        move |cx| {
            if let Some(ref s) = persisted_for_closure {
                if let Some(theme) = gpui_component::ThemeRegistry::global(cx)
                    .themes()
                    .get(&s.theme)
                    .cloned()
                {
                    gpui_component::Theme::global_mut(cx).apply_config(&theme);
                }
            }
        },
    ) {
        tracing::error!("Failed to watch themes directory: {}", err);
    }

    if let Some(ref s) = persisted {
        if let Some(show) = s.scrollbar_show {
            gpui_component::Theme::global_mut(cx).scrollbar_show = show;
        }
    }
    cx.refresh_windows();

    // Persist theme on change
    cx.observe_global::<gpui_component::Theme>(|cx| {
        let snapshot = UiPreferencesSnapshot {
            theme: cx.theme().theme_name().clone(),
            scrollbar_show: Some(cx.theme().scrollbar_show),
        };
        let services = cx.global::<AppServicesGlobal>().0.clone();
        if let Err(err) = services.ui_preferences.save(&snapshot) {
            tracing::error!("failed to save UI preferences: {err}");
        }
    })
    .detach();

    // Theme switching actions
    cx.on_action(|switch: &SwitchTheme, cx| {
        if let Some(config) = gpui_component::ThemeRegistry::global(cx)
            .themes()
            .get(&switch.0)
            .cloned()
        {
            gpui_component::Theme::global_mut(cx).apply_config(&config);
        }
        cx.refresh_windows();
    });
    cx.on_action(|switch: &SwitchThemeMode, cx| {
        gpui_component::Theme::change(switch.0, None, cx);
        cx.refresh_windows();
    });
    cx.on_action(|locale: &SelectLocale, cx| {
        rust_i18n::set_locale(&locale.0.as_str());
        es_fluent_manager_embedded::select_language(
            locale
                .0
                .as_str()
                .parse()
                .unwrap_or_else(|_| es_fluent::unic_langid::langid!("en")),
        );
        cx.refresh_windows();
    });

    // Key bindings
    cx.bind_keys([
        KeyBinding::new("/", ToggleSearch, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-q", Quit, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-f4", Quit, None),
    ]);

    cx.on_action(|_: &Quit, cx| {
        cx.quit();
    });

    cx.on_action(|_: &About, cx| {
        if let Some(window) = cx.active_window().and_then(|w| w.downcast::<Root>()) {
            cx.defer(move |cx| {
                if let Err(err) = window.update(cx, |_, window, cx| {
                    window.defer(cx, |window, cx| {
                        window.open_alert_dialog(cx, |alert, _, _| {
                            alert
                                .title(es_fluent::localize("app_about_title", None))
                                .description(markdown(&es_fluent::localize(
                                    "app_about_description",
                                    None,
                                )))
                        });
                    });
                }) {
                    tracing::error!("failed to open about dialog: {err}");
                }
            });
        }
    });

    cx.activate(true);
}

// ---------------------------------------------------------------------------
// Window creation
// ---------------------------------------------------------------------------

pub fn create_new_window(title: &str, cx: &mut App) {
    let mut window_size = size(px(1400.0), px(900.0));
    if let Some(display) = cx.primary_display() {
        let display_size = display.bounds().size;
        window_size.width = window_size.width.min(display_size.width * 0.85);
        window_size.height = window_size.height.min(display_size.height * 0.85);
    }
    let window_bounds = Bounds::centered(None, window_size, cx);
    let title = SharedString::from(title.to_string());

    cx.spawn(async move |cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(window_bounds)),
            titlebar: Some(TitleBar::title_bar_options()),
            window_min_size: Some(gpui::Size {
                width: px(480.),
                height: px(320.),
            }),
            kind: WindowKind::Normal,
            #[cfg(target_os = "linux")]
            window_background: gpui::WindowBackgroundAppearance::Transparent,
            #[cfg(target_os = "linux")]
            window_decorations: Some(gpui::WindowDecorations::Client),
            ..Default::default()
        };

        let window = cx.open_window(options, |window, cx| {
            let root_view = cx.new(|cx| crate::root::AppRoot::new(title.clone(), window, cx));

            let focus_handle = root_view.focus_handle(cx);
            window.defer(cx, move |window, cx| {
                focus_handle.focus(window, cx);
            });

            cx.new(|cx| Root::new(root_view, window, cx))
        })?;

        window.update(cx, |_, window, _| {
            window.activate_window();
            window.set_window_title(&title);
        })?;

        Ok::<_, anyhow::Error>(())
    })
    .detach();
}

// ---------------------------------------------------------------------------
// Re-exported action types used by menus and title_bar
// ---------------------------------------------------------------------------

use gpui_component::ThemeMode;

#[derive(Action, Clone, PartialEq, Eq, serde::Deserialize)]
#[action(namespace = app, no_json)]
pub struct SwitchTheme(pub SharedString);

#[derive(Action, Clone, PartialEq, Eq, serde::Deserialize)]
#[action(namespace = app, no_json)]
pub struct SwitchThemeMode(pub ThemeMode);
