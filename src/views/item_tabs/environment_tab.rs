use gpui::{AnyElement, IntoElement, ParentElement, Styled as _, div, px};
use gpui_component::{h_flex, v_flex};

use crate::domain::environment::Environment;

pub fn render(environment: &Environment) -> AnyElement {
    let variables = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(
        &environment.variables_json,
    )
    .map(|map| map.len())
    .unwrap_or_default();

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
            h_flex().gap_3().child(chip(format!(
                "{}: {}",
                es_fluent::localize("environment_tab_variables", None),
                variables
            ))),
        )
        .child(div().child(environment.variables_json.clone()))
        .into_any_element()
}

fn chip(label: String) -> impl IntoElement {
    div().px_2().py_1().rounded(px(999.)).border_1().child(label)
}
