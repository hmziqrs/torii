use std::sync::Arc;

use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputEvent, InputState, TabSize},
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
mod helpers;
mod init;
mod kv_editor;
mod layout;
mod request_ops;
mod response_panel;
mod state;
mod subscriptions;
mod ui_actions;

use helpers::*;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestSectionTab {
    Params,
    Auth,
    Headers,
    Body,
    Scripts,
    Tests,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponseTab {
    Body,
    Preview,
    Headers,
    Cookies,
    Timing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyFileTarget {
    Binary,
    FormDataIndex(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KvTarget {
    Params,
    Headers,
    BodyUrlEncoded,
    BodyFormDataText,
}

struct KeyValueEditorRow {
    id: u64,
    enabled: bool,
    key_input: Entity<InputState>,
    value_input: Entity<InputState>,
}

const LARGE_BODY_FILE_CONFIRM_BYTES: u64 = 100 * 1024 * 1024;

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
    _subscriptions: Vec<Subscription>,
    /// Subscriptions for KV row inputs — cleared and rebuilt on every `rebuild_kv_rows` call
    /// to prevent unbounded accumulation as rows are replaced.
    kv_subscriptions: Vec<Subscription>,
    draft_dirty: bool,
    /// Set to `true` whenever the exec status transitions to Completed so that
    /// `render_completed_response` pushes parsed header/cookie/timing rows into
    /// the table entities exactly once per new response rather than every frame.
    response_tables_dirty: bool,
}

#[derive(Debug, Default)]
struct ReentrancyGuard {
    active: bool,
    deferred: bool,
}

impl ReentrancyGuard {
    fn enter(&mut self) -> bool {
        if self.active {
            self.deferred = true;
            return false;
        }
        self.active = true;
        true
    }

    fn leave_and_take_deferred(&mut self) -> bool {
        self.active = false;
        let deferred = self.deferred;
        self.deferred = false;
        deferred
    }

    fn is_active(&self) -> bool {
        self.active
    }
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

    fn selected_auth_kind(&self, cx: &App) -> AuthKind {
        self.auth_type_select
            .read(cx)
            .selected_value()
            .map(|label| auth_kind_from_label(label))
            .unwrap_or(AuthKind::None)
    }

    fn read_secret_value(&self, secret_ref: &Option<String>, cx: &App) -> String {
        let Some(secret_ref) = secret_ref else {
            return String::new();
        };
        cx.global::<AppServicesGlobal>()
            .0
            .secret_store
            .get_secret(secret_ref)
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    fn upsert_auth_secret_value(
        &mut self,
        secret_kind: &str,
        value: String,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let services = cx.global::<AppServicesGlobal>().0.clone();
        let (owner_kind, owner_id) = match self.editor.identity() {
            EditorIdentity::Draft(id) => ("request_draft", id.to_string()),
            EditorIdentity::Persisted(id) => ("request", id.to_string()),
        };

        if value.is_empty() {
            let _ = services
                .secret_manager
                .delete_secret(owner_kind, &owner_id, secret_kind);
            return None;
        }

        match services
            .secret_manager
            .upsert_secret(owner_kind, &owner_id, secret_kind, &value)
        {
            Ok(secret_ref) => Some(secret_ref.key_name),
            Err(err) => {
                self.editor
                    .set_preflight_error(format!("failed to store auth secret: {err}"));
                None
            }
        }
    }

    fn auth_from_cached_inputs(&mut self, kind: AuthKind, cx: &mut Context<Self>) -> AuthType {
        match kind {
            AuthKind::None => AuthType::None,
            AuthKind::Basic => {
                let username = self.auth_basic_username_input.read(cx).value().to_string();
                let password_value = self
                    .auth_basic_password_ref_input
                    .read(cx)
                    .value()
                    .to_string();
                AuthType::Basic {
                    username,
                    password_secret_ref: self.upsert_auth_secret_value(
                        "basic_password",
                        password_value,
                        cx,
                    ),
                }
            }
            AuthKind::Bearer => {
                let token_value = self
                    .auth_bearer_token_ref_input
                    .read(cx)
                    .value()
                    .to_string();
                AuthType::Bearer {
                    token_secret_ref: self.upsert_auth_secret_value(
                        "bearer_token",
                        token_value,
                        cx,
                    ),
                }
            }
            AuthKind::ApiKey => {
                let key_name = self.auth_api_key_name_input.read(cx).value().to_string();
                let value_raw = self
                    .auth_api_key_value_ref_input
                    .read(cx)
                    .value()
                    .to_string();
                let location_ix = self
                    .auth_api_key_location_select
                    .read(cx)
                    .selected_index(cx)
                    .map(|ix| ix.row)
                    .unwrap_or(0);
                AuthType::ApiKey {
                    key_name,
                    value_secret_ref: self.upsert_auth_secret_value("api_key_value", value_raw, cx),
                    location: api_key_location_from_index(location_ix),
                }
            }
        }
    }

    fn set_auth_kind(&mut self, kind: AuthKind, cx: &mut Context<Self>) {
        let next = self.auth_from_cached_inputs(kind, cx);
        if self.editor.draft().auth != next {
            self.editor.draft_mut().auth = next;
            self.editor.refresh_save_status();
            cx.notify();
        }
    }

    fn sync_auth_from_inputs(&mut self, cx: &mut Context<Self>) {
        self.set_auth_kind(self.selected_auth_kind(cx), cx);
    }

    fn selected_body_kind(&self, _cx: &App) -> BodyKind {
        match &self.editor.draft().body {
            BodyType::None => BodyKind::None,
            BodyType::RawText { .. } => BodyKind::RawText,
            BodyType::RawJson { .. } => BodyKind::RawJson,
            BodyType::UrlEncoded { .. } => BodyKind::UrlEncoded,
            BodyType::FormData { .. } => BodyKind::FormData,
            BodyType::BinaryFile { .. } => BodyKind::BinaryFile,
        }
    }

    fn set_body_kind(&mut self, kind: BodyKind, cx: &mut Context<Self>) {
        let next = match kind {
            BodyKind::None => BodyType::None,
            BodyKind::RawText => BodyType::RawText {
                content: self.body_raw_text_input.read(cx).value().to_string(),
            },
            BodyKind::RawJson => BodyType::RawJson {
                content: self.body_raw_json_input.read(cx).value().to_string(),
            },
            BodyKind::UrlEncoded => BodyType::UrlEncoded {
                entries: self.collect_meaningful_pairs(KvTarget::BodyUrlEncoded, cx),
            },
            BodyKind::FormData => {
                let file_fields = match &self.editor.draft().body {
                    BodyType::FormData { file_fields, .. } => file_fields.clone(),
                    _ => Vec::new(),
                };
                BodyType::FormData {
                    text_fields: self.collect_meaningful_pairs(KvTarget::BodyFormDataText, cx),
                    file_fields,
                }
            }
            BodyKind::BinaryFile => match &self.editor.draft().body {
                BodyType::BinaryFile {
                    blob_hash,
                    file_name,
                } => BodyType::BinaryFile {
                    blob_hash: blob_hash.clone(),
                    file_name: file_name.clone(),
                },
                _ => BodyType::BinaryFile {
                    blob_hash: String::new(),
                    file_name: None,
                },
            },
        };
        if self.editor.draft().body != next {
            self.editor.draft_mut().body = next;
            self.editor.refresh_save_status();
            cx.notify();
        }
    }

    fn sync_inputs_from_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.input_sync_guard.enter() {
            return;
        }

        let draft = self.editor.draft().clone();
        if self.url_input.read(cx).value().as_ref() != draft.url.as_str() {
            self.url_input.update(cx, |s, cx| {
                s.set_value(draft.url.clone(), window, cx);
            });
        }
        self.sync_kv_rows_with_draft(KvTarget::Params, window, cx);
        self.sync_kv_rows_with_draft(KvTarget::Headers, window, cx);
        self.sync_kv_rows_with_draft(KvTarget::BodyUrlEncoded, window, cx);
        self.sync_kv_rows_with_draft(KvTarget::BodyFormDataText, window, cx);
        self.method_select.update(cx, |select, cx| {
            if let Some(ix) = standard_method_index(draft.method.as_str()) {
                if select.selected_index(cx).map(|it| it.row) != Some(ix) {
                    select.set_selected_index(
                        Some(gpui_component::IndexPath::default().row(ix)),
                        window,
                        cx,
                    );
                }
            } else if select.selected_value().is_some() {
                select.set_selected_index(None, window, cx);
            }
        });
        self.auth_type_select.update(cx, |select, cx| {
            let ix = auth_type_index(&draft.auth);
            if select.selected_index(cx).map(|it| it.row) != Some(ix) {
                select.set_selected_index(
                    Some(gpui_component::IndexPath::default().row(ix)),
                    window,
                    cx,
                );
            }
        });

        match &draft.auth {
            AuthType::Basic {
                username,
                password_secret_ref,
            } => {
                if self.auth_basic_username_input.read(cx).value().as_ref() != username.as_str() {
                    self.auth_basic_username_input.update(cx, |s, cx| {
                        s.set_value(username.clone(), window, cx);
                    });
                }
                let password_value = self.read_secret_value(password_secret_ref, cx);
                if self.auth_basic_password_ref_input.read(cx).value().as_ref()
                    != password_value.as_str()
                {
                    self.auth_basic_password_ref_input.update(cx, |s, cx| {
                        s.set_value(password_value.clone(), window, cx);
                    });
                }
            }
            AuthType::Bearer { token_secret_ref } => {
                let token_value = self.read_secret_value(token_secret_ref, cx);
                if self.auth_bearer_token_ref_input.read(cx).value().as_ref()
                    != token_value.as_str()
                {
                    self.auth_bearer_token_ref_input.update(cx, |s, cx| {
                        s.set_value(token_value.clone(), window, cx);
                    });
                }
            }
            AuthType::ApiKey {
                key_name,
                value_secret_ref,
                location,
            } => {
                if self.auth_api_key_name_input.read(cx).value().as_ref() != key_name.as_str() {
                    self.auth_api_key_name_input.update(cx, |s, cx| {
                        s.set_value(key_name.clone(), window, cx);
                    });
                }
                let value_raw = self.read_secret_value(value_secret_ref, cx);
                if self.auth_api_key_value_ref_input.read(cx).value().as_ref() != value_raw.as_str()
                {
                    self.auth_api_key_value_ref_input.update(cx, |s, cx| {
                        s.set_value(value_raw.clone(), window, cx);
                    });
                }
                self.auth_api_key_location_select.update(cx, |select, cx| {
                    let row = api_key_location_index(*location);
                    if select.selected_index(cx).map(|it| it.row) != Some(row) {
                        select.set_selected_index(
                            Some(gpui_component::IndexPath::default().row(row)),
                            window,
                            cx,
                        );
                    }
                });
            }
            AuthType::None => {}
        }

        match &draft.body {
            BodyType::RawText { content } => {
                if self.body_raw_text_input.read(cx).value().as_ref() != content.as_str() {
                    self.body_raw_text_input.update(cx, |s, cx| {
                        s.set_value(content.clone(), window, cx);
                    });
                }
            }
            BodyType::RawJson { content } => {
                if self.body_raw_json_input.read(cx).value().as_ref() != content.as_str() {
                    self.body_raw_json_input.update(cx, |s, cx| {
                        s.set_value(content.clone(), window, cx);
                    });
                }
            }
            _ => {}
        }

        if self.input_sync_guard.leave_and_take_deferred() {
            cx.notify();
        }
    }

    /// Create a fresh HTML webview for on-demand preview rendering.
    fn ensure_html_webview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.html_webview.is_some() {
            return;
        }
        use raw_window_handle::HasWindowHandle;
        let Ok(window_handle) = window.window_handle() else {
            return;
        };
        let Some(wry_webview) = lb_wry::WebViewBuilder::new()
            .build_as_child(&window_handle)
            .ok()
        else {
            return;
        };
        self.html_webview = Some(cx.new(|cx| WebView::new(wry_webview, window, cx)));
    }

    /// Release the HTML preview webview, hiding and dropping the native child view.
    ///
    /// This must be called when the tab becomes inactive or is closed; simply setting
    /// `html_webview = None` during render is insufficient because the render function
    /// for an inactive tab is never invoked, leaving the wry child view orphaned.
    pub fn release_html_webview(&mut self, cx: &mut Context<Self>) {
        if let Some(webview) = self.html_webview.take() {
            webview.update(cx, |w, _| {
                w.hide();
            });
            // Dropping the Entity<WebView> here decrements the Rc<wry::WebView>
            // refcount; when it reaches zero the native WKWebView is removed
            // from the window via removeFromSuperview (macOS) or equivalent.
        }
    }

    fn current_preview_bytes(&self) -> usize {
        match self.editor.exec_status() {
            ExecStatus::Completed { response } => match &response.body_ref {
                BodyRef::InMemoryPreview { bytes, .. } => bytes.len(),
                BodyRef::DiskBlob {
                    preview: Some(bytes),
                    ..
                } => bytes.len(),
                _ => 0,
            },
            _ => 0,
        }
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
