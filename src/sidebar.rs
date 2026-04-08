use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme as _, Icon, IconName,
    input::{Input, InputEvent, InputState},
    resizable::resizable_panel,
    sidebar::{Sidebar, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    v_flex, Sizable as _,
};

// ---------------------------------------------------------------------------
// Navigation pages
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Page {
    Home,
    Settings,
    About,
}

impl Page {
    pub fn title(&self) -> &'static str {
        match self {
            Page::Home => "Home",
            Page::Settings => "Settings",
            Page::About => "About",
        }
    }

    fn icon(&self) -> IconName {
        match self {
            Page::Home => IconName::Inbox,
            Page::Settings => IconName::Settings2,
            Page::About => IconName::Info,
        }
    }

    fn all() -> &'static [Page] {
        &[Page::Home, Page::Settings, Page::About]
    }
}

// ---------------------------------------------------------------------------
// App sidebar
// ---------------------------------------------------------------------------

pub struct AppSidebar {
    active_page: Page,
    collapsed: bool,
    search_input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl AppSidebar {
    pub fn new(active_page: Page, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search..."));
        let _subscriptions = vec![cx.subscribe(&search_input, |_, _, _: &InputEvent, cx| {
            cx.notify();
        })];

        Self {
            active_page,
            collapsed: false,
            search_input,
            _subscriptions,
        }
    }

    pub fn set_active_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.active_page = page;
        cx.notify();
    }
}

impl Render for AppSidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_page = self.active_page;

        resizable_panel()
            .size(px(255.))
            .size_range(px(60.)..px(320.))
            .child(
                Sidebar::new("app-sidebar")
                    .w(relative(1.))
                    .border_0()
                    .collapsed(self.collapsed)
                    .header(
                        v_flex()
                            .w_full()
                            .gap_4()
                            .child(
                                SidebarHeader::new()
                                    .w_full()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(cx.theme().radius_lg)
                                            .bg(cx.theme().primary)
                                            .text_color(cx.theme().primary_foreground)
                                            .size_8()
                                            .flex_shrink_0()
                                            .child(Icon::new(IconName::Star)),
                                    )
                                    .when(!self.collapsed, |this| {
                                        this.child(
                                            v_flex()
                                                .gap_0()
                                                .text_sm()
                                                .flex_1()
                                                .line_height(relative(1.25))
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .child("My App")
                                                .child(
                                                    div()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child("Starter")
                                                        .text_xs(),
                                                ),
                                        )
                                    }),
                            )
                            .when(!self.collapsed, |this| {
                                this.child(
                                    div()
                                        .bg(cx.theme().sidebar_accent)
                                        .rounded_full()
                                        .px_1()
                                        .flex_1()
                                        .mx_1()
                                        .child(
                                            Input::new(&self.search_input)
                                                .appearance(false)
                                                .cleanable(true),
                                        ),
                                )
                            }),
                    )
                    .child(
                        SidebarGroup::new("Navigation").child(
                            SidebarMenu::new().children(
                                Page::all().iter().map(|page| {
                                    SidebarMenuItem::new(page.title())
                                        .icon(Icon::new(page.icon()).small())
                                        .active(active_page == *page)
                                        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                                            cx.emit(*page);
                                            cx.notify();
                                        }))
                                }),
                            ),
                        ),
                    ),
            )
    }
}

impl EventEmitter<Page> for AppSidebar {}
