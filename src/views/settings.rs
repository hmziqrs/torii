use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme as _, WindowExt as _, button::Button, label::Label, switch::Switch, v_flex,
};

pub struct SettingsPage {
    dark_mode: bool,
}

impl SettingsPage {
    pub fn new(_: &mut Window, cx: &mut App) -> Self {
        Self {
            dark_mode: cx.theme().mode.is_dark(),
        }
    }
}

impl Render for SettingsPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                            .on_click(cx.listener(|this, checked: &bool, _, _| {
                                this.dark_mode = *checked;
                            })),
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
