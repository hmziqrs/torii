use gpui::{AnyElement, IntoElement, ParentElement, Styled as _, div, px};
use gpui_component::{h_flex, v_flex};

use crate::services::workspace_tree::WorkspaceTree;

pub fn render(workspace: &WorkspaceTree) -> AnyElement {
    let collection_count = workspace.collections.len();
    let environment_count = workspace.environments.len();
    let request_count = workspace
        .collections
        .iter()
        .map(|collection| collection.request_count())
        .sum::<usize>();

    v_flex()
        .size_full()
        .p_6()
        .gap_5()
        .child(
            v_flex()
                .gap_2()
                .child(
                    div()
                        .text_2xl()
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(workspace.workspace.name.clone()),
                )
                .child(
                    div()
                        .text_color(gpui::transparent_black())
                        .child(es_fluent::localize("workspace_tab_subtitle", None)),
                ),
        )
        .child(
            h_flex()
                .gap_3()
                .child(chip(format!(
                    "{}: {}",
                    es_fluent::localize("workspace_tab_collections", None),
                    collection_count
                )))
                .child(chip(format!(
                    "{}: {}",
                    es_fluent::localize("workspace_tab_requests", None),
                    request_count
                )))
                .child(chip(format!(
                    "{}: {}",
                    es_fluent::localize("workspace_tab_environments", None),
                    environment_count
                ))),
        )
        .child(
            v_flex()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(es_fluent::localize("workspace_tab_collections_heading", None)),
                )
                .children(workspace.collections.iter().map(|collection| {
                    div()
                        .p_3()
                        .rounded(px(6.))
                        .border_1()
                        .child(format!(
                            "{} ({})",
                            collection.collection.name,
                            collection.request_count()
                        ))
                })),
        )
        .into_any_element()
}

fn chip(label: String) -> impl IntoElement {
    div().px_2().py_1().rounded(px(999.)).border_1().child(label)
}
