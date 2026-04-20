use std::rc::Rc;

use gpui::{
    AnyElement, App, AppContext as _, Context, Corner, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Render, SharedString,
    Styled as _, Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, IconName, Sizable as _, Theme, TitleBar, WindowExt as _,
    button::{Button, ButtonVariants as _},
    label::Label,
    menu::{AppMenuBar, DropdownMenu as _},
};

use crate::app::{About, OpenLayoutDebug, OpenSettings, SelectFont, SelectRadius};
use crate::menus;

pub struct AppTitleBar {
    app_menu_bar: Entity<AppMenuBar>,
    settings: Entity<SettingsDropdown>,
    child: Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>,
}

impl AppTitleBar {
    pub fn new(
        title: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let app_menu_bar = menus::init(title, cx);
        let settings = cx.new(|cx| SettingsDropdown::new(window, cx));

        Self {
            app_menu_bar,
            settings,
            child: Rc::new(|_, _| div().into_any_element()),
        }
    }

    pub fn child<F, E>(mut self, f: F) -> Self
    where
        E: IntoElement,
        F: Fn(&mut Window, &mut App) -> E + 'static,
    {
        self.child = Rc::new(move |window, cx| f(window, cx).into_any_element());
        self
    }
}

impl Render for AppTitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        TitleBar::new()
            .child(div().flex().items_center().child(self.app_menu_bar.clone()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .px_2()
                    .gap_2()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child((self.child.clone())(window, cx))
                    .child(
                        Label::new(es_fluent::localize("titlebar_theme_label", None))
                            .secondary(cx.theme().theme_name())
                            .text_sm(),
                    )
                    .child(self.settings.clone())
                    .child(
                        Button::new("bell")
                            .small()
                            .ghost()
                            .compact()
                            .icon(IconName::Bell)
                            .on_click(|_, window, cx| {
                                window.push_notification(
                                    es_fluent::localize("titlebar_no_notifications", None),
                                    cx,
                                );
                            }),
                    ),
            )
    }
}

// ---------------------------------------------------------------------------
// Settings dropdown (font size, radius, scrollbar)
// ---------------------------------------------------------------------------

struct SettingsDropdown {
    focus_handle: FocusHandle,
}

impl SettingsDropdown {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }

    fn on_select_font(
        &mut self,
        font_size: &SelectFont,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        Theme::global_mut(cx).font_size = px(font_size.0 as f32);
        window.refresh();
    }

    fn on_select_radius(
        &mut self,
        radius: &SelectRadius,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        Theme::global_mut(cx).radius = px(radius.0 as f32);
        Theme::global_mut(cx).radius_lg = if cx.theme().radius > px(0.) {
            cx.theme().radius + px(2.)
        } else {
            px(0.)
        };
        window.refresh();
    }
}

impl Render for SettingsDropdown {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self.focus_handle.clone();
        let font_size = cx.theme().font_size.as_f32() as i32;
        let radius = cx.theme().radius.as_f32() as i32;

        div()
            .id("settings-dropdown")
            .track_focus(&focus_handle)
            .on_action(cx.listener(Self::on_select_font))
            .on_action(cx.listener(Self::on_select_radius))
            .child(
                Button::new("settings-btn")
                    .small()
                    .ghost()
                    .icon(IconName::Settings2)
                    .dropdown_menu(move |menu, _window, _cx| {
                        menu.scrollable(true)
                            .label(es_fluent::localize("titlebar_font_size", None))
                            .menu_with_check(
                                es_fluent::localize("titlebar_font_size_large", None),
                                font_size == 18,
                                Box::new(SelectFont(18)),
                            )
                            .menu_with_check(
                                es_fluent::localize("titlebar_font_size_medium_default", None),
                                font_size == 16,
                                Box::new(SelectFont(16)),
                            )
                            .menu_with_check(
                                es_fluent::localize("titlebar_font_size_small", None),
                                font_size == 14,
                                Box::new(SelectFont(14)),
                            )
                            .separator()
                            .label(es_fluent::localize("titlebar_border_radius", None))
                            .menu_with_check(
                                es_fluent::localize("titlebar_radius_8", None),
                                radius == 8,
                                Box::new(SelectRadius(8)),
                            )
                            .menu_with_check(
                                es_fluent::localize("titlebar_radius_6_default", None),
                                radius == 6,
                                Box::new(SelectRadius(6)),
                            )
                            .menu_with_check(
                                es_fluent::localize("titlebar_radius_4", None),
                                radius == 4,
                                Box::new(SelectRadius(4)),
                            )
                            .menu_with_check(
                                es_fluent::localize("titlebar_radius_0", None),
                                radius == 0,
                                Box::new(SelectRadius(0)),
                            )
                            .separator()
                            .menu(es_fluent::localize("tab_kind_settings", None), Box::new(OpenSettings))
                            .menu(es_fluent::localize("tab_kind_layout_debug", None), Box::new(OpenLayoutDebug))
                            .menu(es_fluent::localize("tab_kind_about", None), Box::new(About))
                    })
                    .anchor(Corner::TopRight),
            )
    }
}
