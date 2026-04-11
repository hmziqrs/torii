use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme as _, Sizable as _,
    input::{Input, InputState},
    v_flex,
};

pub struct AboutPage {
    input: Entity<InputState>,
}

impl AboutPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value("About test input", window, cx);
            state
        });

        Self { input }
    }
}

impl Render for AboutPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .child(
                div()
                    .text_2xl()
                    .child(es_fluent::localize("about_title", None)),
            )
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(es_fluent::localize("about_version", None)),
            )
            .child(
                div()
                    .w(px(360.))
                    .child(Input::new(&self.input).large()),
            )
    }
}
