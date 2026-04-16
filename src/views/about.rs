use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme as _,
    // ui::{IndexPath},
    IndexPath,
    Sizable as _,
    input::{Input, InputState},
    select::{Select, SelectState},
    v_flex,
};

pub struct AboutPage {
    select: Entity<SelectState<Vec<String>>>,
    input: Entity<InputState>,
}

impl AboutPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value("About test input", window, cx);
            state
        });

        let select = cx.new(|cx| {
            SelectState::new(
                vec!["Apple".into(), "Orange".into(), "Banana".into()],
                Some(IndexPath::default()), // Select first item
                window,
                cx,
            )
        });

        Self { input, select }
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
            .child(div().w(px(360.)).child(Input::new(&self.input).large()))
            .child(Select::new(&self.select))
    }
}
