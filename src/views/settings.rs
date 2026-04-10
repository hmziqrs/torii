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
                    .child("Settings"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(Label::new("Dark Mode"))
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
                    .child(Label::new("Push a Notification"))
                    .child(
                        Button::new("notify")
                            .label("Notify")
                            .on_click(|_, window, cx| {
                                window.push_notification("Hello from Settings!", cx);
                            }),
                    ),
            )
    }
}
