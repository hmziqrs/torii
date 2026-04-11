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

            Tab::new()
                .label(tab.title.clone())
                .prefix(div().pl_2().child(Icon::new(tab.icon.clone()).size_3p5()))
                .selected(tab.selected)
                .on_drag(
                    DraggedTab {
                        from: index,
                        title: tab.title.clone(),
                    },
                    |drag: &DraggedTab, _, _, cx: &mut App| {
                        cx.new(|_| DragTabPreview::new(drag.title.clone()))
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
                .suffix(
                    Button::new(format!("close-tab-{key:?}"))
                        .ghost()
                        .xsmall()
                        .icon(IconName::Close)
                        .on_click(move |_, window, cx| {
                            on_close(close_key, window, cx);
                        }),
                )
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
        .child(div().text_xl().font_weight(gpui::FontWeight::BOLD).child(title))
        .child(div().text_sm().child(body))
        .into_any_element()
}

struct DragTabPreview {
    title: SharedString,
}

impl DragTabPreview {
    fn new(title: SharedString) -> Self {
        Self { title }
    }
}

impl Render for DragTabPreview {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded(px(8.))
            .border_1()
            .bg(rgb(0xFFFFFF))
            .child(self.title.clone())
    }
}
