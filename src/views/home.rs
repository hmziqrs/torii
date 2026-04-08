use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme as _, v_flex,
    button::{Button, ButtonVariants as _},
};

pub struct HomePage;

impl HomePage {
    pub fn new() -> Self {
        Self
    }
}

impl Render for HomePage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_6()
            .child(
                div()
                    .text_3xl()
                    .font_weight(FontWeight::BOLD)
                    .child("Welcome to GPUI Starter"),
            )
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child("A boilerplate for building desktop apps with GPUI"),
            )
            .child(
                Button::new("get-started")
                    .primary()
                    .label("Get Started")
                    .on_click(|_, _, _| {
                        println!("Get Started clicked!");
                    }),
            )
    }
}
