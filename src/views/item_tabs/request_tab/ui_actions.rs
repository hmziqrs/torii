use super::*;
use std::time::Duration;

impl RequestTabView {
    pub(super) fn set_active_section(
        &mut self,
        section: RequestSectionTab,
        cx: &mut Context<Self>,
    ) {
        if self.active_section != section {
            tracing::debug!(
                from = ?self.active_section,
                to = ?section,
                "request tab section switch"
            );
            self.active_section = section;
            cx.notify();
        }
    }

    pub(super) fn set_active_response_tab(&mut self, tab: ResponseTab, cx: &mut Context<Self>) {
        if self.active_response_tab != tab {
            self.active_response_tab = tab;
            cx.notify();
        }
    }

    pub(super) fn set_meta_hover(&mut self, hover: ResponseMetaHover, cx: &mut Context<Self>) {
        if self.meta_hover != hover {
            self.meta_hover = hover;
            cx.notify();
        }
    }

    pub(super) fn meta_hover_enter(&mut self, hover: ResponseMetaHover, cx: &mut Context<Self>) {
        self.meta_hover_close_task = None;
        self.set_meta_hover(hover, cx);
    }

    pub(super) fn meta_hover_leave(&mut self, hover: ResponseMetaHover, cx: &mut Context<Self>) {
        self.meta_hover_close_task = None;
        self.meta_hover_close_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(120))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.meta_hover == hover {
                    this.meta_hover = ResponseMetaHover::None;
                    cx.notify();
                }
                this.meta_hover_close_task = None;
            });
        }));
    }

    pub(super) fn open_settings_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
        let name_input = self.name_input.clone();
        let timeout_input = self.timeout_input.clone();
        let follow_redirects_input = self.follow_redirects_input.clone();

        window.open_dialog(cx, move |dialog, _, cx| {
            let muted = cx.theme().muted_foreground;
            dialog
                .title(es_fluent::localize("request_tab_settings_label", None))
                .overlay_closable(true)
                .keyboard(true)
                .child(
                    v_flex()
                        .gap_3()
                        .child(
                            v_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(es_fluent::localize("request_tab_name_label", None)),
                                )
                                .child(Input::new(&name_input).large()),
                        )
                        .child(
                            v_flex()
                                .gap_2()
                                .child(
                                    div().text_xs().text_color(muted).child(es_fluent::localize(
                                        "request_tab_timeout_label",
                                        None,
                                    )),
                                )
                                .child(Input::new(&timeout_input).large()),
                        )
                        .child(
                            v_flex()
                                .gap_2()
                                .child(div().text_xs().text_color(muted).child(
                                    es_fluent::localize("request_tab_follow_redirects_label", None),
                                ))
                                .child(Input::new(&follow_redirects_input).large()),
                        ),
                )
                .footer(
                    h_flex().justify_end().child(
                        Button::new("request-settings-close")
                            .primary()
                            .label(es_fluent::localize("request_tab_dirty_close_cancel", None))
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                            }),
                    ),
                )
        });
    }
}
