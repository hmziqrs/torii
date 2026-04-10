use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme as _, Selectable as _, Theme, ThemeMode, WindowExt as _, button::Button,
    label::Label, switch::Switch, v_flex,
};

pub struct SettingsPage {
    dark_mode: bool,
    locale: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl SettingsPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![
            cx.observe_global_in::<Theme>(window, |this, _, cx| {
                let dark_mode = cx.theme().mode.is_dark();
                if this.dark_mode != dark_mode {
                    this.dark_mode = dark_mode;
                    cx.notify();
                }
            }),
            cx.observe_global_in::<crate::app::LocaleState>(window, |this, _, cx| {
                let locale = crate::app::current_locale(cx);
                if this.locale != locale {
                    this.locale = locale;
                    cx.notify();
                }
            }),
        ];

        Self {
            dark_mode: cx.theme().mode.is_dark(),
            locale: crate::app::current_locale(cx),
            _subscriptions: subscriptions,
        }
    }
}

impl Render for SettingsPage {
    fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .p_6()
            .gap_6()
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::BOLD)
                    .child(es_fluent::localize("settings_title", None)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(Label::new(es_fluent::localize("settings_dark_mode", None)))
                    .child(
                        Switch::new("dark-mode")
                            .checked(self.dark_mode)
                            .on_click(|checked, _, cx| {
                                let mode = if *checked {
                                    ThemeMode::Dark
                                } else {
                                    ThemeMode::Light
                                };
                                crate::app::set_theme_mode(mode, cx);
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(Label::new(es_fluent::localize("settings_language", None)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("settings-language-en")
                                    .outline()
                                    .selected(self.locale.as_ref() == crate::app::LOCALE_EN)
                                    .label(es_fluent::localize("settings_language_english", None))
                                    .on_click(|_, _, cx| {
                                        crate::app::set_locale(crate::app::LOCALE_EN, cx);
                                    }),
                            )
                            .child(
                                Button::new("settings-language-zh-cn")
                                    .outline()
                                    .selected(self.locale.as_ref() == crate::app::LOCALE_ZH_CN)
                                    .label(es_fluent::localize(
                                        "settings_language_simplified_chinese",
                                        None,
                                    ))
                                    .on_click(|_, _, cx| {
                                        crate::app::set_locale(crate::app::LOCALE_ZH_CN, cx);
                                    }),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(Label::new(es_fluent::localize(
                        "settings_push_notification",
                        None,
                    )))
                    .child(
                        Button::new("notify")
                            .label(es_fluent::localize("settings_notify", None))
                            .on_click(|_, window, cx| {
                                window.push_notification(
                                    es_fluent::localize("settings_hello_notification", None),
                                    cx,
                                );
                            }),
                    ),
            )
    }
}
