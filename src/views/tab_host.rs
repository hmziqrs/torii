use gpui::{
    AnyElement, App, AppContext as _, ClickEvent, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement as _, StyleRefinement,
    Styled as _, Window, div, px, rgb,
};
use gpui_component::{
    Icon, IconName, Selectable as _, Sizable as _, Size,
    button::{Button, ButtonVariants as _},
    h_flex,
    tab::{Tab, TabBar},
    v_flex,
};

use crate::session::item_key::TabKey;

#[derive(Clone)]
pub struct TabPresentation {
    pub index: usize,
    pub key: TabKey,
    pub title: SharedString,
    pub icon: IconName,
    pub selected: bool,
}

#[derive(Clone)]
struct DraggedTab {
    from: usize,
    title: SharedString,
    icon: IconName,
    selected: bool,
}

pub fn render_tab_bar(
    tabs: &[TabPresentation],
    sidebar_collapsed: bool,
    on_select: impl Fn(TabKey, &mut Window, &mut App) + Clone + 'static,
    on_close: impl Fn(TabKey, &mut Window, &mut App) + Clone + 'static,
    on_toggle_sidebar: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_reorder: impl Fn(usize, usize, &mut Window, &mut App) + Clone + 'static,
) -> AnyElement {
    let selected_index = tabs.iter().position(|tab| tab.selected).unwrap_or_default();

    TabBar::new("workspace-tabs")
        .w_full()
        .with_size(Size::Small)
        .menu(true)
        .selected_index(selected_index)
        .prefix(
            h_flex().mx_1().gap_1().child(
                Button::new("toggle-sidebar")
                    .ghost()
                    .xsmall()
                    .icon(if sidebar_collapsed {
                        IconName::PanelLeftOpen
                    } else {
                        IconName::PanelLeftClose
                    })
                    .on_click(on_toggle_sidebar),
            ),
        )
        .children(tabs.iter().map(|tab| {
            let key = tab.key;
            let close_key = tab.key;
            let index = tab.index;
            let on_select = on_select.clone();
            let on_close = on_close.clone();
            let on_reorder = on_reorder.clone();

            build_tab(tab.title.clone(), tab.icon.clone(), tab.selected)
                .selected(tab.selected)
                .on_drag(
                    DraggedTab {
                        from: index,
                        title: tab.title.clone(),
                        icon: tab.icon.clone(),
                        selected: tab.selected,
                    },
                    |drag: &DraggedTab, _, _, cx: &mut App| {
                        cx.new(|_| {
                            DragTabPreview::new(
                                drag.title.clone(),
                                drag.icon.clone(),
                                drag.selected,
                            )
                        })
                    },
                )
                .drag_over::<DraggedTab>({
                    let index = index;
                    move |style: StyleRefinement, dragged: &DraggedTab, _, _| {
                        let mut style = style.border_color(rgb(0x2563EB));
                        if index < dragged.from {
                            style = style.border_l_2();
                        } else if index > dragged.from {
                            style = style.border_r_2();
                        }
                        style
                    }
                })
                .on_drop(move |dragged: &DraggedTab, window, cx| {
                    on_reorder(dragged.from, index, window, cx);
                })
                .suffix(close_tab_button(
                    format!("close-tab-{key:?}"),
                    move |_, window, cx| {
                        on_close(close_key, window, cx);
                    },
                ))
                .on_click(move |_, window, cx| {
                    on_select(key, window, cx);
                })
        }))
        .into_any_element()
}

pub fn render_empty_state(title: SharedString, body: SharedString) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_3()
        .child(
            div()
                .text_xl()
                .font_weight(gpui::FontWeight::BOLD)
                .child(title),
        )
        .child(div().text_sm().child(body))
        .into_any_element()
}

struct DragTabPreview {
    title: SharedString,
    icon: IconName,
    selected: bool,
}

impl DragTabPreview {
    fn new(title: SharedString, icon: IconName, selected: bool) -> Self {
        Self {
            title,
            icon,
            selected,
        }
    }
}

impl Render for DragTabPreview {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().child(
            TabBar::new("drag-preview-tabbar")
                .with_size(Size::Small)
                .selected_index(0)
                .child(
                    build_tab(self.title.clone(), self.icon.clone(), self.selected).suffix(
                        Button::new("drag-preview-close")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Close),
                    ),
                ),
        )
    }
}

fn build_tab(title: SharedString, icon: IconName, selected: bool) -> Tab {
    Tab::new()
        .w(px(160.))
        .label(title)
        .prefix(div().pl_2().child(Icon::new(icon).size_3p5()))
        .selected(selected)
        .with_size(Size::Small)
}

fn close_tab_button(
    id: impl Into<gpui::ElementId>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    Button::new(id)
        .ghost()
        .xsmall()
        .icon(IconName::Close)
        .on_click(on_click)
}
