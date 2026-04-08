use gpui::{prelude::*, *};
use gpui_component::{ActiveTheme as _, Root, v_flex};

use crate::sidebar::{AppSidebar, Page};
use crate::title_bar::AppTitleBar;
use crate::views::{AboutPage, HomePage, SettingsPage};

pub struct AppRoot {
    focus_handle: FocusHandle,
    title_bar: Entity<AppTitleBar>,
    sidebar: Entity<AppSidebar>,
    active_page: Page,
    home_page: Entity<HomePage>,
    settings_page: Entity<SettingsPage>,
    about_page: Entity<AboutPage>,
}

impl AppRoot {
    pub fn new(
        title: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let title_bar = cx.new(|cx| AppTitleBar::new(title, window, cx));
        let sidebar = cx.new(|cx| AppSidebar::new(Page::Home, window, cx));
        let home_page = cx.new(|_| HomePage::new());
        let settings_page = cx.new(|cx| SettingsPage::new(window, cx));
        let about_page = cx.new(|_| AboutPage::new());

        // Listen for page changes from sidebar
        cx.subscribe(&sidebar, |this, _, page: &Page, cx| {
            this.active_page = *page;
            cx.notify();
        })
        .detach();

        Self {
            focus_handle: cx.focus_handle(),
            title_bar,
            sidebar,
            active_page: Page::Home,
            home_page,
            settings_page,
            about_page,
        }
    }

    fn active_page_view(&self) -> AnyView {
        match self.active_page {
            Page::Home => self.home_page.clone().into(),
            Page::Settings => self.settings_page.clone().into(),
            Page::About => self.about_page.clone().into(),
        }
    }
}

impl Focusable for AppRoot {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AppRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sheet_layer = Root::render_sheet_layer(window, cx);
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);
        let page_title = self.active_page.title();

        v_flex()
            .size_full()
            .child(self.title_bar.clone())
            .child(
                div()
                    .track_focus(&self.focus_handle)
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        div().flex().size_full().child(self.sidebar.clone()).child(
                            v_flex()
                                .flex_1()
                                .h_full()
                                .overflow_x_hidden()
                                .child(
                                    div()
                                        .id("header")
                                        .p_4()
                                        .border_b_1()
                                        .border_color(cx.theme().border)
                                        .child(
                                            div()
                                                .text_xl()
                                                .font_weight(FontWeight::BOLD)
                                                .child(page_title),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("page")
                                        .flex_1()
                                        .overflow_y_scroll()
                                        .child(self.active_page_view()),
                                ),
                        ),
                    ),
            )
            .children(sheet_layer)
            .children(dialog_layer)
            .children(notification_layer)
    }
}
