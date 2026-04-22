use gpui::{AnyElement, IntoElement, ParentElement, Styled as _, WeakEntity, div, px};
use gpui_component::{button::Button, h_flex, v_flex};

use crate::domain::environment::Environment;
use crate::root::AppRoot;

pub fn render(environment: &Environment, root: WeakEntity<AppRoot>) -> AnyElement {
    let variables = serde_json::from_str::<Vec<serde_json::Value>>(&environment.variables_json)
        .map(|map| map.len())
        .unwrap_or_default();
    let environment_id = environment.id;

    v_flex()
        .size_full()
        .p_6()
        .gap_5()
        .child(
            div()
                .text_2xl()
                .font_weight(gpui::FontWeight::BOLD)
                .child(environment.name.clone()),
        )
        .child(
            h_flex()
                .gap_3()
                .child(chip(format!(
                    "{}: {}",
                    es_fluent::localize("environment_tab_variables", None),
                    variables
                )))
                .child(
                    Button::new("environment-vars-edit")
                        .label("Edit Variables")
                        .on_click(move |_, window, cx| {
                            let _ = root.update(cx, |this, cx| {
                                this.open_environment_variables_dialog(environment_id, window, cx);
                            });
                        }),
                ),
        )
        .child(div().child(environment.variables_json.clone()))
        .into_any_element()
}

fn chip(label: String) -> impl IntoElement {
    div()
        .px_2()
        .py_1()
        .rounded(px(999.))
        .border_1()
        .child(label)
}
