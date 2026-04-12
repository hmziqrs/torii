use gpui::{
    Action, App, AppContext as _, Bounds, Focusable as _, Global, KeyBinding, SharedString,
    WindowBounds, WindowKind, WindowOptions, actions, px, size,
};

use crate::views::item_tabs::request_tab::{CancelRequest, SaveRequest, SendRequest};
use gpui_component::{ActiveTheme, Root, Theme, ThemeMode, TitleBar};

use crate::services::{
    app_services::AppServicesGlobal, startup::bootstrap_app_services,
    ui_preferences::UiPreferencesSnapshot,
};

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

actions!(app, [About, Quit, ToggleSearch]);

#[derive(Action, Clone, PartialEq, Eq, serde::Deserialize)]
#[action(namespace = app)]
pub struct SelectLocaleEnglish;

#[derive(Action, Clone, PartialEq, Eq, serde::Deserialize)]
#[action(namespace = app)]
pub struct SelectLocaleSimplifiedChinese;

#[derive(Action, Clone, PartialEq, Eq, serde::Deserialize)]
#[action(namespace = app, no_json)]
pub struct SelectFont(pub usize);

#[derive(Action, Clone, PartialEq, Eq, serde::Deserialize)]
#[action(namespace = app, no_json)]
pub struct SelectRadius(pub usize);

pub const LOCALE_EN: &str = "en";
pub const LOCALE_ZH_CN: &str = "zh-CN";

#[derive(Clone)]
pub struct LocaleState {
    pub current: SharedString,
}

impl Global for LocaleState {}

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

pub fn init(cx: &mut App) {
    use tracing_subscriber::{layer::SubscriberExt as _, util::SubscriberInitExt as _};
    let mut filter = tracing_subscriber::EnvFilter::from_default_env();
    if let Ok(directive) = "torii=trace".parse() {
        filter = filter.add_directive(directive);
    }
    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter)
        .try_init();

    // Must be called before using any gpui-component features.
    gpui_component::init(cx);
    es_fluent_manager_embedded::init();

    let services = bootstrap_app_services();
    cx.set_global(AppServicesGlobal(services.clone()));

    let persisted = match services.ui_preferences.load() {
        Ok(value) => value,
        Err(err) => {
            tracing::error!("failed to load UI preferences: {err}");
            None
        }
    };

    let startup_locale_source = persisted
        .as_ref()
        .and_then(|snapshot| snapshot.locale.as_deref())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| rust_i18n::locale().to_string());
    set_locale(&startup_locale_source, cx);

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
        if let Some(font_size_px) = s.font_size_px {
            gpui_component::Theme::global_mut(cx).font_size = px(font_size_px as f32);
        }
        if let Some(radius_px) = s.radius_px {
            gpui_component::Theme::global_mut(cx).radius = px(radius_px as f32);
            gpui_component::Theme::global_mut(cx).radius_lg = if cx.theme().radius > px(0.) {
                cx.theme().radius + px(2.)
            } else {
                cx.theme().radius
            };
        }
        if let Some(mode) = s.theme_mode.as_deref().and_then(parse_theme_mode) {
            set_theme_mode(mode, cx);
        }
    }
    cx.refresh_windows();

    // Persist theme on change
    cx.observe_global::<Theme>(|cx| {
        persist_ui_preferences(cx);
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
        set_theme_mode(switch.0, cx);
    });
    cx.on_action(|_: &SelectLocaleEnglish, cx| {
        set_locale(LOCALE_EN, cx);
    });
    cx.on_action(|_: &SelectLocaleSimplifiedChinese, cx| {
        set_locale(LOCALE_ZH_CN, cx);
    });

    // Key bindings
    cx.bind_keys([
        KeyBinding::new("/", ToggleSearch, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-q", Quit, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-f4", Quit, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-s", SaveRequest, Some("RequestTabView")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-s", SaveRequest, Some("RequestTabView")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-enter", SendRequest, Some("RequestTabView")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-enter", SendRequest, Some("RequestTabView")),
        KeyBinding::new("escape", CancelRequest, Some("RequestTabView")),
    ]);

    cx.on_action(|_: &Quit, cx| {
        cx.quit();
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
            let root_for_close = root_view.clone();
            window.on_window_should_close(cx, move |_, cx| {
                let _ = root_for_close.update(cx, |root, cx| {
                    root.persist_session_state(cx);
                });
                true
            });

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

#[derive(Action, Clone, PartialEq, Eq, serde::Deserialize)]
#[action(namespace = app, no_json)]
pub struct SwitchTheme(pub SharedString);

#[derive(Action, Clone, PartialEq, Eq, serde::Deserialize)]
#[action(namespace = app, no_json)]
pub struct SwitchThemeMode(pub ThemeMode);

pub fn current_locale(cx: &App) -> SharedString {
    cx.try_global::<LocaleState>()
        .map(|state| state.current.clone())
        .unwrap_or_else(|| normalize_locale(&rust_i18n::locale().to_string()))
}

pub fn set_locale(locale: &str, cx: &mut App) {
    let locale = normalize_locale(locale);
    apply_locale(locale.as_ref());
    cx.set_global(LocaleState {
        current: locale.clone(),
    });
    persist_ui_preferences(cx);
    cx.refresh_windows();
}

pub fn set_theme_mode(mode: ThemeMode, cx: &mut App) {
    Theme::change(mode, None, cx);
    cx.refresh_windows();
}

fn normalize_locale(locale: &str) -> SharedString {
    match locale {
        "zh" | LOCALE_ZH_CN => LOCALE_ZH_CN.into(),
        "en-US" | LOCALE_EN => LOCALE_EN.into(),
        other => other.to_string().into(),
    }
}

fn resolve_language_identifier(locale: &str) -> es_fluent::unic_langid::LanguageIdentifier {
    match locale {
        "zh-CN" | "zh" => es_fluent::unic_langid::langid!("zh-CN"),
        "en" | "en-US" => es_fluent::unic_langid::langid!("en"),
        _ => locale
            .parse()
            .unwrap_or_else(|_| es_fluent::unic_langid::langid!("en")),
    }
}

fn apply_locale(locale: &str) {
    rust_i18n::set_locale(locale);
    let language = resolve_language_identifier(locale);
    es_fluent_manager_embedded::select_language(language.clone());
    es_fluent::select_language(&language);
}

fn parse_theme_mode(mode: &str) -> Option<ThemeMode> {
    match mode {
        "light" => Some(ThemeMode::Light),
        "dark" => Some(ThemeMode::Dark),
        _ => None,
    }
}

fn persist_ui_preferences(cx: &App) {
    let Some(services) = cx
        .try_global::<AppServicesGlobal>()
        .map(|global| global.0.clone())
    else {
        return;
    };

    let snapshot = UiPreferencesSnapshot {
        theme: cx.theme().theme_name().clone(),
        scrollbar_show: Some(cx.theme().scrollbar_show),
        theme_mode: Some(if cx.theme().mode.is_dark() {
            "dark".to_string()
        } else {
            "light".to_string()
        }),
        locale: Some(current_locale(cx).to_string()),
        font_size_px: Some(cx.theme().font_size.as_f32().round() as i32),
        radius_px: Some(cx.theme().radius.as_f32().round() as i32),
    };

    if let Err(err) = services.ui_preferences.save(&snapshot) {
        tracing::error!("failed to save UI preferences: {err}");
    }
}
