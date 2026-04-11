use gpui::{AnyElement, ClickEvent, IntoElement, ParentElement, SharedString, Styled as _, Window, div};
use gpui_component::{
    Disableable as _, Icon, IconName, Selectable as _, Sizable as _, Size,
    button::{Button, ButtonVariants as _},
    h_flex,
    tab::{Tab, TabBar},
    v_flex,
};

use crate::session::item_key::TabKey;

#[derive(Clone)]
pub struct TabPresentation {
    pub key: TabKey,
    pub title: SharedString,
    pub icon: IconName,
    pub selected: bool,
}

pub fn render_tab_bar(
    tabs: &[TabPresentation],
    can_move_left: bool,
    can_move_right: bool,
    on_select: impl Fn(TabKey, &mut Window, &mut gpui::App) + Clone + 'static,
    on_close: impl Fn(TabKey, &mut Window, &mut gpui::App) + Clone + 'static,
    on_move_left: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    on_move_right: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let selected_index = tabs.iter().position(|tab| tab.selected).unwrap_or_default();

    TabBar::new("workspace-tabs")
        .w_full()
        .with_size(Size::Small)
        .menu(true)
        .selected_index(selected_index)
        .prefix(
            h_flex()
                .mx_1()
                .gap_1()
                .child(
                    Button::new("move-tab-left")
                        .ghost()
                        .xsmall()
                        .icon(IconName::ChevronLeft)
                        .disabled(!can_move_left)
                        .on_click(on_move_left),
                )
                .child(
                    Button::new("move-tab-right")
                        .ghost()
                        .xsmall()
                        .icon(IconName::ChevronRight)
                        .disabled(!can_move_right)
                        .on_click(on_move_right),
                ),
        )
        .children(tabs.iter().map(|tab| {
            let key = tab.key;
            let close_key = tab.key;
            let on_select = on_select.clone();
            let on_close = on_close.clone();
            Tab::new()
                .label(tab.title.clone())
                .prefix(
                    div()
                        .pl_2()
                        .child(Icon::new(tab.icon.clone()).size_3p5()),
                )
                .selected(tab.selected)
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
