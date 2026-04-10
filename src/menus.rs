use gpui::{App, Entity, Menu, MenuItem, SharedString};
use gpui_component::{
    ActiveTheme as _, GlobalState, Theme, ThemeMode, ThemeRegistry, menu::AppMenuBar,
};

use crate::app::{
    About, Quit, SelectLocaleEnglish, SelectLocaleSimplifiedChinese, SwitchTheme, SwitchThemeMode,
    current_locale,
};

pub fn init(title: impl Into<SharedString>, cx: &mut App) -> Entity<AppMenuBar> {
    let app_menu_bar = AppMenuBar::new(cx);
    let title: SharedString = title.into();
    update_app_menu(title.clone(), app_menu_bar.clone(), cx);

    cx.on_action({
        let title = title.clone();
        let app_menu_bar = app_menu_bar.clone();
        move |_: &SelectLocaleEnglish, cx| {
            update_app_menu(title.clone(), app_menu_bar.clone(), cx);
        }
    });
    cx.on_action({
        let title = title.clone();
        let app_menu_bar = app_menu_bar.clone();
        move |_: &SelectLocaleSimplifiedChinese, cx| {
            update_app_menu(title.clone(), app_menu_bar.clone(), cx);
        }
    });

    cx.observe_global::<Theme>({
        let title = title.clone();
        let app_menu_bar = app_menu_bar.clone();
        move |cx| {
            update_app_menu(title.clone(), app_menu_bar.clone(), cx);
        }
    })
    .detach();

    app_menu_bar
}

fn update_app_menu(title: impl Into<SharedString>, app_menu_bar: Entity<AppMenuBar>, cx: &mut App) {
    let title: SharedString = title.into();

    cx.set_menus(build_menus(title.clone(), cx));
    let menus = build_menus(title, cx)
        .into_iter()
        .map(|menu| menu.owned())
        .collect();
    GlobalState::global_mut(cx).set_app_menus(menus);

    app_menu_bar.update(cx, |menu_bar, cx| {
        menu_bar.reload(cx);
    });
}

fn build_menus(title: impl Into<SharedString>, cx: &App) -> Vec<Menu> {
    vec![
        Menu {
            name: title.into(),
            items: vec![
                MenuItem::action(es_fluent::localize("menu_about", None), About),
                MenuItem::Separator,
                MenuItem::Submenu(Menu {
                    name: es_fluent::localize("menu_appearance", None).into(),
                    items: vec![
                        MenuItem::action(
                            es_fluent::localize("menu_appearance_light", None),
                            SwitchThemeMode(ThemeMode::Light),
                        )
                        .checked(!cx.theme().mode.is_dark()),
                        MenuItem::action(
                            es_fluent::localize("menu_appearance_dark", None),
                            SwitchThemeMode(ThemeMode::Dark),
                        )
                        .checked(cx.theme().mode.is_dark()),
                    ],
                    disabled: false,
                }),
                theme_menu(cx),
                language_menu(cx),
                MenuItem::Separator,
                MenuItem::action(es_fluent::localize("menu_quit", None), Quit),
            ],
            disabled: false,
        },
        Menu {
            name: es_fluent::localize("menu_edit", None).into(),
            items: vec![
                MenuItem::action(
                    es_fluent::localize("menu_undo", None),
                    gpui_component::input::Undo,
                ),
                MenuItem::action(
                    es_fluent::localize("menu_redo", None),
                    gpui_component::input::Redo,
                ),
                MenuItem::separator(),
                MenuItem::action(
                    es_fluent::localize("menu_cut", None),
                    gpui_component::input::Cut,
                ),
                MenuItem::action(
                    es_fluent::localize("menu_copy", None),
                    gpui_component::input::Copy,
                ),
                MenuItem::action(
                    es_fluent::localize("menu_paste", None),
                    gpui_component::input::Paste,
                ),
                MenuItem::separator(),
                MenuItem::action(
                    es_fluent::localize("menu_select_all", None),
                    gpui_component::input::SelectAll,
                ),
            ],
            disabled: false,
        },
        Menu {
            name: es_fluent::localize("menu_window", None).into(),
            items: vec![MenuItem::action(
                es_fluent::localize("menu_toggle_search", None),
                crate::app::ToggleSearch,
            )],
            disabled: false,
        },
    ]
}

fn language_menu(cx: &App) -> MenuItem {
    let locale = current_locale(cx).to_string();
    MenuItem::Submenu(Menu {
        name: es_fluent::localize("menu_language", None).into(),
        items: vec![
            MenuItem::action(
                es_fluent::localize("menu_language_english", None),
                SelectLocaleEnglish,
            )
            .checked(locale == "en"),
            MenuItem::action(
                es_fluent::localize("menu_language_simplified_chinese", None),
                SelectLocaleSimplifiedChinese,
            )
            .checked(locale == "zh-CN" || locale == "zh"),
        ],
        disabled: false,
    })
}

fn theme_menu(cx: &App) -> MenuItem {
    let themes = ThemeRegistry::global(cx).sorted_themes();
    let current_name = cx.theme().theme_name();
    MenuItem::Submenu(Menu {
        name: es_fluent::localize("menu_theme", None).into(),
        items: themes
            .iter()
            .map(|theme| {
                let checked = current_name == &theme.name;
                MenuItem::action(theme.name.clone(), SwitchTheme(theme.name.clone()))
                    .checked(checked)
            })
            .collect(),
        disabled: false,
    })
}
