use std::collections::HashMap;
use std::sync::Arc;

use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme as _, Disableable as _, IconName, Selectable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    divider::Divider,
    h_flex,
    input::{Input, InputEvent, InputState, TabSize},
    resizable::{resizable_panel, v_resizable},
    select::{Select, SelectEvent, SelectState},
    table::{Column, TableDelegate, TableState},
    v_flex,
};
use gpui_wry::WebView;

use crate::{
    domain::{
        ids::WorkspaceId,
        request::{ApiKeyLocation, AuthType, BodyType, KeyValuePair, RequestItem},
        response::{BodyRef, HeaderJsonFormat, ResponseBudgets, parse_response_header_rows},
    },
    repos::request_repo::RequestRepoError,
    services::{
        app_services::{AppServices, AppServicesGlobal},
        error_classifier::ClassifiedError,
        request_execution::{ExecOutcome, ExecProgressEvent},
        telemetry,
    },
    session::request_editor_state::{EditorIdentity, ExecStatus, RequestEditorState, SaveStatus},
};

mod auth_editor;
mod auth_secret_ops;
mod body_editor;
mod body_file_state;
mod helpers;
mod init;
mod kv;
mod kv_editor;
mod layout;
mod request_ops;
mod response_panel;
mod subscriptions;
mod sync;
mod types;
mod ui_actions;

use helpers::*;
use types::*;

// ---------------------------------------------------------------------------
// Actions for request tab keyboard shortcuts
// ---------------------------------------------------------------------------

actions!(
    request_tab,
    [
        SaveRequest,
        SendRequest,
        CancelRequest,
        DuplicateRequest,
        FocusUrlBar,
        ToggleBodySearch
    ]
);

pub struct RequestTabView {
    editor: RequestEditorState,
    focus_handle: FocusHandle,
    name_input: Entity<InputState>,
    method_select: Entity<SelectState<Vec<&'static str>>>,
    url_input: Entity<InputState>,
    auth_type_select: Entity<SelectState<Vec<&'static str>>>,
    auth_basic_username_input: Entity<InputState>,
    auth_basic_password_ref_input: Entity<InputState>,
    auth_bearer_token_ref_input: Entity<InputState>,
    auth_api_key_name_input: Entity<InputState>,
    auth_api_key_value_ref_input: Entity<InputState>,
    auth_api_key_location_select: Entity<SelectState<Vec<&'static str>>>,
    body_raw_text_input: Entity<InputState>,
    body_raw_json_input: Entity<InputState>,
    pre_request_input: Entity<InputState>,
    tests_input: Entity<InputState>,
    variable_overrides_input: Entity<InputState>,
    timeout_input: Entity<InputState>,
    follow_redirects_input: Entity<InputState>,
    params_rows: Vec<KeyValueEditorRow>,
    headers_rows: Vec<KeyValueEditorRow>,
    body_urlencoded_rows: Vec<KeyValueEditorRow>,
    body_form_text_rows: Vec<KeyValueEditorRow>,
    next_kv_row_id: u64,
    active_section: RequestSectionTab,
    active_response_tab: ResponseTab,
    loaded_full_body_blob_id: Option<String>,
    loaded_full_body_text: Option<String>,
    input_sync_guard: ReentrancyGuard,
    body_search_visible: bool,
    body_search_input: Entity<InputState>,
    error_detail_expanded: bool,
    headers_table: Entity<TableState<response_panel::HeadersTableDelegate>>,
    cookies_table: Entity<TableState<response_panel::CookiesTableDelegate>>,
    timing_table: Entity<TableState<response_panel::TimingTableDelegate>>,
    params_kv_table: Entity<TableState<kv_editor::KvTableDelegate>>,
    headers_kv_table: Entity<TableState<kv_editor::KvTableDelegate>>,
    body_urlencoded_kv_table: Entity<TableState<kv_editor::KvTableDelegate>>,
    body_form_text_kv_table: Entity<TableState<kv_editor::KvTableDelegate>>,
    html_webview: Option<Entity<WebView>>,
    /// Last HTML pushed into the webview via `load_html`. Compared on every render
    /// to avoid pushing identical content into WKWebView — see idle-cpu-audit-2.md Bug 6.
    last_preview_html: Option<String>,
    _subscriptions: Vec<Subscription>,
    /// Per-target subscriptions for KV row inputs. Only the target being rebuilt
    /// has its subscriptions cleared, so that rebuilding one target doesn't drop
    /// subscriptions for the other three. See render-loop-audit.md.
    kv_subscriptions: HashMap<KvTarget, Vec<Subscription>>,
    draft_dirty: bool,
    /// Set to `true` whenever the exec status transitions to Completed so that
    /// `render_completed_response` pushes parsed header/cookie/timing rows into
    /// the table entities exactly once per new response rather than every frame.
    response_tables_dirty: bool,
    /// Per-target dirty flags for the KV editor tables. Set whenever rows are
    /// added, removed, or rebuilt for that target. Cleared in `render_kv_table`
    /// after the delegate is updated. Prevents `entity.update + refresh` on every
    /// render frame — see idle-cpu-audit.md RLA-4.
    params_kv_dirty: bool,
    headers_kv_dirty: bool,
    body_urlencoded_kv_dirty: bool,
    body_form_text_kv_dirty: bool,
}

impl RequestTabView {
    pub fn new(request: &RequestItem, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = RequestEditorState::from_persisted(request.clone());
        Self::build_with_editor(editor, window, cx)
    }

    /// Create a draft request tab for a new unsaved request.
    pub fn new_draft(
        collection_id: crate::domain::ids::CollectionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = RequestEditorState::new_draft(collection_id);
        Self::build_with_editor(editor, window, cx)
    }

    pub fn editor(&self) -> &RequestEditorState {
        &self.editor
    }

    pub fn editor_mut(&mut self) -> &mut RequestEditorState {
        &mut self.editor
    }

    fn handle_save_request(
        &mut self,
        _action: &SaveRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.save(cx) {
            Ok(()) => {
                window.push_notification(es_fluent::localize("request_tab_save_ok", None), cx);
            }
            Err(err) => {
                window.push_notification(err, cx);
            }
        }
    }

    fn handle_send_request(
        &mut self,
        _action: &SendRequest,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.send(cx);
    }

    fn handle_cancel_request(
        &mut self,
        _action: &CancelRequest,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_send(cx);
    }

    fn handle_duplicate_request(
        &mut self,
        _action: &DuplicateRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.duplicate(cx) {
            Ok(_) => {
                window.push_notification(es_fluent::localize("request_tab_duplicate_ok", None), cx)
            }
            Err(err) => window.push_notification(err, cx),
        }
    }

    fn handle_focus_url_bar(
        &mut self,
        _action: &FocusUrlBar,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.url_input.read(cx).focus_handle(cx).focus(window, cx);
    }

    fn handle_toggle_body_search(
        &mut self,
        _action: &ToggleBodySearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.body_search_visible = !self.body_search_visible;
        if self.body_search_visible {
            self.body_search_input
                .read(cx)
                .focus_handle(cx)
                .focus(window, cx);
        }
        cx.notify();
    }
}

impl Focusable for RequestTabView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl RequestTabView {
    /// Mark the draft as changed so the next render syncs inputs from it.
    /// Call this when the draft is replaced externally (e.g., tab switch).
    pub fn mark_draft_dirty(&mut self, cx: &mut Context<Self>) {
        self.draft_dirty = true;
        cx.notify();
    }
}

impl Render for RequestTabView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        layout::render_request_tab(self, window, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::{ReentrancyGuard, truncate_for_tab_cap};

    #[test]
    fn reentrancy_guard_defers_nested_entry() {
        let mut guard = ReentrancyGuard::default();
        assert!(guard.enter());
        assert!(!guard.enter());
        assert!(guard.leave_and_take_deferred());
    }

    #[test]
    fn truncate_respects_custom_max_bytes() {
        let bytes = vec![1_u8; 16];
        let (truncated, was_truncated) = truncate_for_tab_cap(bytes, 8);
        assert!(was_truncated);
        assert_eq!(truncated.len(), 8);
    }
}
