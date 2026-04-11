use gpui::{AnyElement, IntoElement, ParentElement, Styled as _, div, px};
use gpui_component::{h_flex, v_flex};

use crate::services::workspace_tree::FolderTree;

pub fn render(folder: &FolderTree) -> AnyElement {
    let child_folders = folder
        .children
        .iter()
        .filter(|item| matches!(item, crate::services::workspace_tree::TreeItem::Folder(_)))
        .count();
    let request_count = folder.request_count();

    v_flex()
        .size_full()
        .p_6()
        .gap_5()
        .child(
            div()
                .text_2xl()
                .font_weight(gpui::FontWeight::BOLD)
                .child(folder.folder.name.clone()),
        )
        .child(
            h_flex()
                .gap_3()
                .child(chip(format!(
                    "{}: {}",
                    es_fluent::localize("folder_tab_subfolders", None),
                    child_folders
                )))
                .child(chip(format!(
                    "{}: {}",
                    es_fluent::localize("folder_tab_requests", None),
                    request_count
                ))),
        )
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
