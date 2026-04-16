use super::AppRoot;
use crate::{
    app::{About, CloseTab, NewRequest, NextTab, PrevTab, ToggleSidebar},
    domain::item_id::ItemId,
    session::item_key::ItemKey,
};
use gpui::{Context, Window};
use gpui_component::WindowExt as _;

impl AppRoot {
    pub(super) fn on_about_action(&mut self, _: &About, _: &mut Window, cx: &mut Context<Self>) {
        self.open_item(ItemKey::about(), cx);
    }

    pub(super) fn on_close_tab_action(
        &mut self,
        _: &CloseTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active = self.session.read(cx).tab_manager.active();
        if let Some(tab_key) = active {
            self.close_tab(tab_key, window, cx);
        }
    }

    pub(super) fn on_next_tab_action(
        &mut self,
        _: &NextTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session.update(cx, |session, cx| {
            session.move_active_tab_by(1, cx);
        });
        self.persist_session_state(cx);
    }

    pub(super) fn on_prev_tab_action(
        &mut self,
        _: &PrevTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session.update(cx, |session, cx| {
            session.move_active_tab_by(-1, cx);
        });
        self.persist_session_state(cx);
    }

    pub(super) fn on_toggle_sidebar_action(
        &mut self,
        _: &ToggleSidebar,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_sidebar(cx);
    }

    pub(super) fn on_new_request_action(
        &mut self,
        _: &NewRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = self.session.read(cx).sidebar_selection;
        let collection_id = selected
            .and_then(|item| match item.id {
                Some(ItemId::Collection(id)) => Some(id),
                _ => None,
            })
            .or_else(|| {
                self.catalog
                    .selected_workspace()
                    .and_then(|ws| ws.collections.first().map(|c| c.collection.id))
            });

        if let Some(collection_id) = collection_id {
            self.open_draft_request(collection_id, window, cx);
        } else {
            window.push_notification(
                es_fluent::localize("request_tab_shortcut_no_collection", None),
                cx,
            );
        }
    }
}
