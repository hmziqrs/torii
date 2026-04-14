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
mod body_editor;
mod helpers;
mod kv_editor;
mod response_panel;
mod state;

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
    draft_dirty: bool,
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

    fn build_with_editor(
        mut editor: RequestEditorState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        if editor.draft().params.is_empty() {
            let from_url = params_from_url_query(editor.draft().url.as_str());
            if !from_url.is_empty() {
                editor.draft_mut().params = from_url;
                editor.refresh_save_status();
            }
        }
        let initial = editor.draft().clone();

        let name_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(initial.name.clone(), window, cx);
            state
        });
        let method_select = cx.new(|cx| {
            let mut select = SelectState::new(
                vec!["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"],
                Some(gpui_component::IndexPath::default()),
                window,
                cx,
            );
            if let Some(ix) = standard_method_index(initial.method.as_str()) {
                select.set_selected_index(
                    Some(gpui_component::IndexPath::default().row(ix)),
                    window,
                    cx,
                );
            } else {
                select.set_selected_index(None, window, cx);
            }
            select
        });
        let url_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(initial.url.clone(), window, cx);
            state
        });
        let auth_type_select = cx.new(|cx| {
            let mut select = SelectState::new(
                vec!["None", "Basic", "Bearer", "API Key"],
                Some(gpui_component::IndexPath::default()),
                window,
                cx,
            );
            select.set_selected_index(
                Some(gpui_component::IndexPath::default().row(auth_type_index(&initial.auth))),
                window,
                cx,
            );
            select
        });
        let auth_basic_username_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            if let AuthType::Basic { username, .. } = &initial.auth {
                state.set_value(username.clone(), window, cx);
            }
            state
        });
        let auth_basic_password_ref_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            if let AuthType::Basic {
                password_secret_ref,
                ..
            } = &initial.auth
            {
                state.set_value(password_secret_ref.clone().unwrap_or_default(), window, cx);
            }
            state
        });
        let auth_bearer_token_ref_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            if let AuthType::Bearer { token_secret_ref } = &initial.auth {
                state.set_value(token_secret_ref.clone().unwrap_or_default(), window, cx);
            }
            state
        });
        let auth_api_key_name_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            if let AuthType::ApiKey { key_name, .. } = &initial.auth {
                state.set_value(key_name.clone(), window, cx);
            }
            state
        });
        let auth_api_key_value_ref_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            if let AuthType::ApiKey {
                value_secret_ref, ..
            } = &initial.auth
            {
                state.set_value(value_secret_ref.clone().unwrap_or_default(), window, cx);
            }
            state
        });
        let auth_api_key_location_select = cx.new(|cx| {
            let mut select = SelectState::new(
                vec!["Header", "Query"],
                Some(gpui_component::IndexPath::default()),
                window,
                cx,
            );
            let row = match &initial.auth {
                AuthType::ApiKey { location, .. } => api_key_location_index(*location),
                _ => 0,
            };
            select.set_selected_index(
                Some(gpui_component::IndexPath::default().row(row)),
                window,
                cx,
            );
            select
        });
        let body_raw_text_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .rows(10)
                .searchable(true)
                .soft_wrap(true);
            if let BodyType::RawText { content } = &initial.body {
                state.set_value(content.clone(), window, cx);
            }
            state
        });
        let body_raw_json_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .code_editor("json")
                .line_number(true)
                .indent_guides(true)
                .tab_size(TabSize {
                    tab_size: 4,
                    hard_tabs: false,
                })
                .searchable(true)
                .soft_wrap(false);
            if let BodyType::RawJson { content } = &initial.body {
                state.set_value(content.clone(), window, cx);
            }
            state
        });
        let pre_request_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .code_editor("javascript")
                .multi_line(true)
                .rows(10)
                .line_number(true)
                .searchable(true)
                .soft_wrap(false);
            state.set_value(initial.scripts.pre_request.clone(), window, cx);
            state
        });
        let tests_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .code_editor("javascript")
                .multi_line(true)
                .rows(10)
                .line_number(true)
                .searchable(true)
                .soft_wrap(false);
            state.set_value(initial.scripts.tests.clone(), window, cx);
            state
        });
        let timeout_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            let value = initial
                .settings
                .timeout_ms
                .map(|v| v.to_string())
                .unwrap_or_default();
            state.set_value(value, window, cx);
            state
        });
        let follow_redirects_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            let value = initial
                .settings
                .follow_redirects
                .map(|v| if v { "true" } else { "false" }.to_string())
                .unwrap_or_default();
            state.set_value(value, window, cx);
            state
        });
        let body_search_input = cx.new(|cx| InputState::new(window, cx));

        let mut subscriptions = Vec::new();

        subscriptions.push(cx.subscribe(
            &name_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let name = state.read(cx).value().to_string();
                    if this.editor.draft().name != name {
                        this.editor.draft_mut().name = name;
                        this.editor.refresh_save_status();
                        cx.notify();
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe_in(
            &method_select,
            window,
            |this: &mut RequestTabView,
             _: &Entity<SelectState<Vec<&'static str>>>,
             event: &SelectEvent<Vec<&'static str>>,
             _window: &mut Window,
             cx| {
                let SelectEvent::Confirm(method) = event;
                let Some(method) = method.clone() else {
                    return;
                };
                if this.editor.draft().method != method {
                    this.editor.draft_mut().method = method.to_string();
                    this.editor.refresh_save_status();
                    cx.notify();
                }
            },
        ));

        subscriptions.push(cx.subscribe(
            &url_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    if this.input_sync_guard.is_active() {
                        this.input_sync_guard.deferred = true;
                        return;
                    }
                    let url = state.read(cx).value().to_string();
                    if this.editor.draft().url != url {
                        this.editor.draft_mut().url = url;
                        let enabled_from_url =
                            params_from_url_query(this.editor.draft().url.as_str());
                        let mut merged = enabled_from_url;
                        merged.extend(
                            this.editor
                                .draft()
                                .params
                                .iter()
                                .filter(|entry| !entry.enabled)
                                .cloned(),
                        );
                        if this.editor.draft().params != merged {
                            this.editor.draft_mut().params = merged;
                        }
                        this.editor.refresh_save_status();
                        cx.notify();
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe_in(
            &auth_type_select,
            window,
            |this: &mut RequestTabView,
             _: &Entity<SelectState<Vec<&'static str>>>,
             event: &SelectEvent<Vec<&'static str>>,
             _window: &mut Window,
             cx| {
                let SelectEvent::Confirm(kind) = event;
                let Some(kind) = kind.clone() else {
                    return;
                };
                this.set_auth_kind(auth_kind_from_label(kind), cx);
            },
        ));

        subscriptions.push(cx.subscribe(
            &auth_basic_username_input,
            |this: &mut RequestTabView, _: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.sync_auth_from_inputs(cx);
                }
            },
        ));
        subscriptions.push(cx.subscribe(
            &auth_basic_password_ref_input,
            |this: &mut RequestTabView, _: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.sync_auth_from_inputs(cx);
                }
            },
        ));
        subscriptions.push(cx.subscribe(
            &auth_bearer_token_ref_input,
            |this: &mut RequestTabView, _: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.sync_auth_from_inputs(cx);
                }
            },
        ));
        subscriptions.push(cx.subscribe(
            &auth_api_key_name_input,
            |this: &mut RequestTabView, _: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.sync_auth_from_inputs(cx);
                }
            },
        ));
        subscriptions.push(cx.subscribe(
            &auth_api_key_value_ref_input,
            |this: &mut RequestTabView, _: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.sync_auth_from_inputs(cx);
                }
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &auth_api_key_location_select,
            window,
            |this: &mut RequestTabView,
             _: &Entity<SelectState<Vec<&'static str>>>,
             _: &SelectEvent<Vec<&'static str>>,
             _window: &mut Window,
             cx| {
                this.sync_auth_from_inputs(cx);
            },
        ));

        subscriptions.push(cx.subscribe(
            &body_raw_text_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let content = state.read(cx).value().to_string();
                    if let BodyType::RawText { content: existing } =
                        &mut this.editor.draft_mut().body
                    {
                        if *existing != content {
                            *existing = content;
                            this.editor.refresh_save_status();
                            cx.notify();
                        }
                    }
                }
            },
        ));
        subscriptions.push(cx.subscribe(
            &body_raw_json_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let content = state.read(cx).value().to_string();
                    if let BodyType::RawJson { content: existing } =
                        &mut this.editor.draft_mut().body
                    {
                        if *existing != content {
                            *existing = content;
                            this.editor.refresh_save_status();
                            cx.notify();
                        }
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe(
            &pre_request_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let text = state.read(cx).value().to_string();
                    if this.editor.draft().scripts.pre_request != text {
                        this.editor.draft_mut().scripts.pre_request = text;
                        this.editor.refresh_save_status();
                        cx.notify();
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe(
            &tests_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let text = state.read(cx).value().to_string();
                    if this.editor.draft().scripts.tests != text {
                        this.editor.draft_mut().scripts.tests = text;
                        this.editor.refresh_save_status();
                        cx.notify();
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe(
            &timeout_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let raw = state.read(cx).value().trim().to_string();
                    let parsed = if raw.is_empty() {
                        None
                    } else {
                        raw.parse::<u64>().ok()
                    };
                    if this.editor.draft().settings.timeout_ms != parsed {
                        this.editor.draft_mut().settings.timeout_ms = parsed;
                        this.editor.refresh_save_status();
                        cx.notify();
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe(
            &follow_redirects_input,
            |this: &mut RequestTabView, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let raw = state.read(cx).value().trim().to_ascii_lowercase();
                    let parsed = if raw.is_empty() {
                        None
                    } else if raw == "true" || raw == "1" || raw == "yes" {
                        Some(true)
                    } else if raw == "false" || raw == "0" || raw == "no" {
                        Some(false)
                    } else {
                        this.editor.refresh_save_status();
                        cx.notify();
                        return;
                    };
                    if this.editor.draft().settings.follow_redirects != parsed {
                        this.editor.draft_mut().settings.follow_redirects = parsed;
                        this.editor.refresh_save_status();
                        cx.notify();
                    }
                }
            },
        ));

        let this_entity = cx.entity();
        let mut this = Self {
            editor,
            focus_handle: cx.focus_handle(),
            name_input,
            method_select,
            url_input,
            auth_type_select,
            auth_basic_username_input,
            auth_basic_password_ref_input,
            auth_bearer_token_ref_input,
            auth_api_key_name_input,
            auth_api_key_value_ref_input,
            auth_api_key_location_select,
            body_raw_text_input,
            body_raw_json_input,
            pre_request_input,
            tests_input,
            timeout_input,
            follow_redirects_input,
            params_rows: Vec::new(),
            headers_rows: Vec::new(),
            body_urlencoded_rows: Vec::new(),
            body_form_text_rows: Vec::new(),
            next_kv_row_id: 1,
            active_section: RequestSectionTab::Params,
            active_response_tab: ResponseTab::Body,
            loaded_full_body_blob_id: None,
            loaded_full_body_text: None,
            input_sync_guard: ReentrancyGuard::default(),
            body_search_visible: false,
            body_search_input,
            error_detail_expanded: false,
            headers_table: cx.new(|cx| {
                TableState::new(response_panel::HeadersTableDelegate::new(), window, cx)
                    .row_selectable(false)
                    .col_selectable(false)
                    .col_resizable(true)
                    .col_movable(false)
                    .sortable(false)
            }),
            cookies_table: cx.new(|cx| {
                TableState::new(response_panel::CookiesTableDelegate::new(), window, cx)
                    .row_selectable(false)
                    .col_selectable(false)
                    .col_resizable(true)
                    .col_movable(false)
                    .sortable(false)
            }),
            timing_table: cx.new(|cx| {
                TableState::new(response_panel::TimingTableDelegate::new(), window, cx)
                    .row_selectable(false)
                    .col_selectable(false)
                    .col_resizable(false)
                    .col_movable(false)
                    .sortable(false)
            }),
            params_kv_table: cx.new(|cx| {
                TableState::new(
                    kv_editor::KvTableDelegate::new(this_entity.clone(), KvTarget::Params, "params"),
                    window,
                    cx,
                )
                .row_selectable(false)
                .col_selectable(false)
                .col_resizable(true)
                .col_movable(false)
                .sortable(false)
            }),
            headers_kv_table: cx.new(|cx| {
                TableState::new(
                    kv_editor::KvTableDelegate::new(this_entity.clone(), KvTarget::Headers, "headers"),
                    window,
                    cx,
                )
                .row_selectable(false)
                .col_selectable(false)
                .col_resizable(true)
                .col_movable(false)
                .sortable(false)
            }),
            body_urlencoded_kv_table: cx.new(|cx| {
                TableState::new(
                    kv_editor::KvTableDelegate::new(this_entity.clone(), KvTarget::BodyUrlEncoded, "body-urlencoded"),
                    window,
                    cx,
                )
                .row_selectable(false)
                .col_selectable(false)
                .col_resizable(true)
                .col_movable(false)
                .sortable(false)
            }),
            body_form_text_kv_table: cx.new(|cx| {
                TableState::new(
                    kv_editor::KvTableDelegate::new(this_entity.clone(), KvTarget::BodyFormDataText, "body-form-text"),
                    window,
                    cx,
                )
                .row_selectable(false)
                .col_selectable(false)
                .col_resizable(true)
                .col_movable(false)
                .sortable(false)
            }),
            html_webview: None,
            _subscriptions: subscriptions,
            draft_dirty: true,
        };
        this.rebuild_kv_rows(KvTarget::Params, &initial.params, window, cx);
        this.rebuild_kv_rows(KvTarget::Headers, &initial.headers, window, cx);
        let urlencoded_entries = match &initial.body {
            BodyType::UrlEncoded { entries } => entries.clone(),
            _ => Vec::new(),
        };
        this.rebuild_kv_rows(KvTarget::BodyUrlEncoded, &urlencoded_entries, window, cx);
        let form_text_entries = match &initial.body {
            BodyType::FormData { text_fields, .. } => text_fields.clone(),
            _ => Vec::new(),
        };
        this.rebuild_kv_rows(KvTarget::BodyFormDataText, &form_text_entries, window, cx);
        this
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

    pub fn has_unsaved_changes(&self) -> bool {
        matches!(
            self.editor.save_status(),
            SaveStatus::Dirty | SaveStatus::SaveFailed { .. } | SaveStatus::Saving
        ) || self.editor.detect_dirty()
    }

    // -----------------------------------------------------------------------
    // Save
    // -----------------------------------------------------------------------

    pub fn save(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let services = cx.global::<AppServicesGlobal>().0.clone();
        let mut request = self.editor.draft().clone();
        let expected_revision = self.editor.baseline().map(|b| b.meta.revision).unwrap_or(0);

        self.persist_request_body_blob(&mut request, &services)?;
        self.normalize_auth_secret_ownership_for_save(&mut request, &services)?;

        self.editor.begin_save();
        cx.notify();

        match services.repos.request.save(&request, expected_revision) {
            Ok(saved) => {
                self.editor.complete_save(&saved);

                if matches!(self.editor.identity(), EditorIdentity::Draft(_)) {
                    self.editor.transition_to_persisted(saved.id, &saved);
                }

                cx.notify();
                Ok(())
            }
            Err(RequestRepoError::RevisionConflict { expected, actual }) => {
                let msg = format!(
                    "{} ({expected} -> {actual})",
                    es_fluent::localize("request_tab_save_conflict", None)
                );
                self.editor.fail_save(msg.clone());
                cx.notify();
                Err(msg)
            }
            Err(RequestRepoError::NotFound(_id)) => {
                let msg = es_fluent::localize("request_tab_save_not_found", None).to_string();
                self.editor.fail_save(msg.clone());
                cx.notify();
                Err(msg)
            }
            Err(RequestRepoError::Storage(e)) => {
                let msg = format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_save_failed", None)
                );
                self.editor.fail_save(msg.clone());
                cx.notify();
                Err(msg)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Duplicate
    // -----------------------------------------------------------------------

    pub fn duplicate(&mut self, cx: &mut Context<Self>) -> Result<RequestItem, String> {
        let services = cx.global::<AppServicesGlobal>().0.clone();

        let source_id = match self.editor.request_id() {
            Some(id) => id,
            None => {
                return Err(es_fluent::localize("request_tab_duplicate_unsaved", None).to_string());
            }
        };

        // Duplicate from persisted baseline (not dirty in-memory draft).
        let source = services
            .repos
            .request
            .get(source_id)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_duplicate_failed", None)
                )
            })?
            .ok_or_else(|| es_fluent::localize("request_tab_save_not_found", None).to_string())?;

        let new_name = format!("{} (Copy)", source.name);
        let mut duplicate = services
            .repos
            .request
            .duplicate(source_id, &new_name)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_duplicate_failed", None)
                )
            })?;

        if let Err(err) = self.clone_auth_secrets_for_duplicate(&source, &mut duplicate, &services)
        {
            let _ = services.repos.request.delete(duplicate.id);
            return Err(err);
        }

        duplicate = match services
            .repos
            .request
            .save(&duplicate, duplicate.meta.revision)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_duplicate_failed", None)
                )
            }) {
            Ok(saved) => saved,
            Err(err) => {
                let _ = services.repos.request.delete(duplicate.id);
                return Err(err);
            }
        };

        Ok(duplicate)
    }

    // -----------------------------------------------------------------------
    // Send
    // -----------------------------------------------------------------------

    /// Send the current draft request. Auto-cancels any in-flight operation.
    pub fn send(&mut self, cx: &mut Context<Self>) {
        let services = cx.global::<AppServicesGlobal>().0.clone();
        self.loaded_full_body_blob_id = None;
        self.loaded_full_body_text = None;

        // Determine workspace ID
        let workspace_id = match self.resolve_workspace_id(&services) {
            Some(id) => id,
            None => {
                self.editor.set_preflight_error(
                    es_fluent::localize("request_tab_no_workspace", None).to_string(),
                );
                cx.notify();
                return;
            }
        };

        // Create pending history row with secret-safe snapshot
        let draft = self.editor.draft().clone();
        let history_entry = match services.request_execution.create_pending_history(
            workspace_id,
            self.editor.request_id(),
            &draft,
        ) {
            Ok(entry) => entry,
            Err(e) => {
                self.editor.set_preflight_error(format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_history_create_failed", None)
                ));
                cx.notify();
                return;
            }
        };

        let operation_id = history_entry.id;

        // Begin send — auto-cancels any in-flight operation
        let old_token = self.editor.begin_send(operation_id);
        if let Some(token) = old_token {
            token.cancel();
        }
        cx.notify();

        let exec_service = services.request_execution.clone();
        let Some(cancel_token) = self.editor.cancellation_token().cloned() else {
            self.editor.set_preflight_error(
                es_fluent::localize("request_tab_preflight", None).to_string(),
            );
            cx.notify();
            return;
        };
        let io_runtime = services.io_runtime.clone();
        let (progress_tx, mut progress_rx) =
            tokio::sync::mpsc::unbounded_channel::<ExecProgressEvent>();

        cx.spawn(async move |this, cx| {
            while let Some(event) = progress_rx.recv().await {
                if let Err(err) = this.update(cx, |this, cx| {
                    if this.editor.active_operation_id() != Some(operation_id) {
                        return;
                    }
                    match event {
                        ExecProgressEvent::ResponseStreamingStarted => {
                            this.editor.transition_to_streaming();
                            cx.notify();
                        }
                    }
                }) {
                    tracing::warn!(error = %err, "failed to update request tab for streaming progress");
                    telemetry::inc_async_update_failures("dropped_entity");
                }
            }
        })
        .detach();

        cx.spawn(async move |this, cx| {
            let request = draft.clone();
            let exec_service_for_task = exec_service.clone();
            let handle = io_runtime.spawn(async move {
                exec_service_for_task
                    .execute_with_progress(
                        &request,
                        workspace_id,
                        cancel_token.clone(),
                        Some(progress_tx),
                    )
                    .await
            });
            let result = handle
                .await
                .unwrap_or_else(|e| Err(anyhow::anyhow!("task join error: {e}")));

            exec_service.finalize_history(operation_id, &result);

            if let Err(err) = this.update(cx, |this, cx| {
                this.loaded_full_body_blob_id = None;
                this.loaded_full_body_text = None;
                match result {
                    Ok(ExecOutcome::Completed(summary)) => {
                        if !this.editor.complete_exec(summary, operation_id) {
                            tracing::warn!(
                                op_id = %operation_id,
                                "late response ignored — operation no longer active"
                            );
                        }
                    }
                    Ok(ExecOutcome::Failed {
                        summary,
                        classified,
                    }) => {
                        if !this.editor.fail_exec(summary, classified, operation_id) {
                            tracing::warn!(
                                op_id = %operation_id,
                                "late failure ignored — operation no longer active"
                            );
                        }
                    }
                    Ok(ExecOutcome::Cancelled { partial_size }) => {
                        if !this.editor.cancel_exec(partial_size, operation_id) {
                            tracing::warn!(
                                op_id = %operation_id,
                                "late cancel ignored — operation no longer active"
                            );
                        }
                    }
                    Ok(ExecOutcome::PreflightFailed(msg)) => {
                        this.editor.reset_preflight();
                        this.editor.set_preflight_error(msg);
                    }
                    Err(e) => {
                        this.editor.fail_exec(e.to_string(), None, operation_id);
                    }
                }
                this.editor.set_latest_history_id(Some(operation_id));
                cx.notify();
            }) {
                tracing::warn!(error = %err, "failed to update request tab for terminal execution state");
                telemetry::inc_async_update_failures("dropped_entity");
            }
        })
        .detach();
    }

    // -----------------------------------------------------------------------
    // Cancel
    // -----------------------------------------------------------------------

    /// Cancel the active send operation.
    pub fn cancel_send(&mut self, cx: &mut Context<Self>) {
        let _span = tracing::info_span!("request.cancel").entered();
        if let Some(token) = self.editor.cancellation_token() {
            token.cancel();
        }
        cx.notify();
    }

    // -----------------------------------------------------------------------
    // Reload baseline
    // -----------------------------------------------------------------------

    pub fn reload_baseline(&mut self, cx: &mut Context<Self>) {
        let services = cx.global::<AppServicesGlobal>().0.clone();
        if let Some(id) = self.editor.request_id() {
            if let Ok(Some(persisted)) = services.repos.request.get(id) {
                self.editor.reload_baseline(persisted);
            }
        }
        cx.notify();
    }

    pub fn load_full_response_body(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let services = cx.global::<AppServicesGlobal>().0.clone();

        let (blob_id, media_type) = match self.editor.exec_status() {
            ExecStatus::Completed { response } => match &response.body_ref {
                BodyRef::DiskBlob { blob_id, .. } => (blob_id.clone(), response.media_type.clone()),
                _ => {
                    return Err(
                        es_fluent::localize("request_tab_full_body_unavailable", None).to_string(),
                    );
                }
            },
            _ => {
                return Err(
                    es_fluent::localize("request_tab_full_body_unavailable", None).to_string(),
                );
            }
        };

        let bytes = services.blob_store.read_all(&blob_id).map_err(|e| {
            format!(
                "{}: {e}",
                es_fluent::localize("request_tab_full_body_load_failed", None)
            )
        })?;

        let preview_bytes = self.current_preview_bytes();
        let available_for_full_body =
            ResponseBudgets::PER_TAB_CAP_BYTES.saturating_sub(preview_bytes);
        let (capped, was_truncated) = truncate_for_tab_cap(bytes, available_for_full_body);
        let mut text = render_preview_text(&capped, media_type.as_deref());
        if was_truncated {
            text.push('\n');
            text.push_str(&es_fluent::localize("request_tab_response_truncated", None));
        }
        self.loaded_full_body_text = Some(text);
        self.loaded_full_body_blob_id = Some(blob_id);
        cx.notify();
        Ok(())
    }

    pub fn copy_response_body(&self, cx: &mut Context<Self>) -> Result<(), String> {
        let Some((text, _media_type)) = self.response_body_text_for_actions(cx)? else {
            return Err(es_fluent::localize("request_tab_copy_unavailable", None).to_string());
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        Ok(())
    }

    pub fn save_response_body_to_file(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let services = cx.global::<AppServicesGlobal>().0.clone();
        let (source, suggested_name) = match self.editor.exec_status() {
            ExecStatus::Completed { response } => {
                let suggested = suggested_file_name(response.media_type.as_deref());
                match &response.body_ref {
                    BodyRef::Empty => {
                        return Err(
                            es_fluent::localize("request_tab_save_unavailable", None).to_string()
                        );
                    }
                    BodyRef::InMemoryPreview { bytes, .. } => {
                        (SaveSource::InMemory(bytes.to_vec()), suggested)
                    }
                    BodyRef::DiskBlob { blob_id, .. } => {
                        (SaveSource::Blob(blob_id.clone()), suggested)
                    }
                }
            }
            _ => {
                return Err(es_fluent::localize("request_tab_save_unavailable", None).to_string());
            }
        };

        let receiver = cx.prompt_for_new_path(
            &std::env::current_dir().unwrap_or_default(),
            Some(&suggested_name),
        );
        cx.spawn_in(window, async move |_, _| {
            let Some(path) = receiver.await.ok().into_iter().flatten().flatten().next() else {
                return;
            };
            let result = match source {
                SaveSource::InMemory(bytes) => {
                    std::fs::write(&path, bytes).map_err(anyhow::Error::from)
                }
                SaveSource::Blob(blob_id) => {
                    let mut reader = match services.blob_store.open_read(&blob_id) {
                        Ok(file) => file,
                        Err(err) => {
                            tracing::warn!(error = %err, "open blob for save failed");
                            return;
                        }
                    };
                    let mut writer = match std::fs::File::create(&path) {
                        Ok(file) => file,
                        Err(err) => {
                            tracing::warn!(error = %err, "create save destination failed");
                            return;
                        }
                    };
                    std::io::copy(&mut reader, &mut writer)
                        .map(|_| ())
                        .map_err(anyhow::Error::from)
                }
            };
            if let Err(err) = result {
                tracing::warn!(error = %err, "failed to save response body to file");
            }
        })
        .detach();
        Ok(())
    }

    fn response_body_text_for_actions(
        &self,
        cx: &Context<Self>,
    ) -> Result<Option<(String, Option<String>)>, String> {
        let ExecStatus::Completed { response } = self.editor.exec_status() else {
            return Ok(None);
        };

        let media_type = response.media_type.clone();
        if !is_text_like_media_type(media_type.as_deref()) {
            return Ok(None);
        }

        let text = match &response.body_ref {
            BodyRef::Empty => String::new(),
            BodyRef::InMemoryPreview { bytes, .. } => {
                render_preview_text(bytes, media_type.as_deref())
            }
            BodyRef::DiskBlob {
                blob_id,
                preview,
                size_bytes,
            } => {
                if *size_bytes > (8 * 1024 * 1024) {
                    return Err(es_fluent::localize("request_tab_copy_too_large", None).to_string());
                }
                let bytes = if self.loaded_full_body_blob_id.as_deref() == Some(blob_id.as_str()) {
                    cx.global::<AppServicesGlobal>()
                        .0
                        .blob_store
                        .read_all(blob_id)
                        .map_err(|e| {
                            format!(
                                "{}: {e}",
                                es_fluent::localize("request_tab_full_body_load_failed", None)
                            )
                        })?
                } else if let Some(preview) = preview {
                    preview.to_vec()
                } else {
                    cx.global::<AppServicesGlobal>()
                        .0
                        .blob_store
                        .read_preview(blob_id, ResponseBudgets::PREVIEW_CAP_BYTES)
                        .map_err(|e| {
                            format!(
                                "{}: {e}",
                                es_fluent::localize("request_tab_full_body_load_failed", None)
                            )
                        })?
                };
                render_preview_text(&bytes, media_type.as_deref())
            }
        };

        Ok(Some((text, media_type)))
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn resolve_workspace_id(
        &self,
        services: &std::sync::Arc<crate::services::app_services::AppServices>,
    ) -> Option<WorkspaceId> {
        let collection_id = self.editor.draft().collection_id;
        services
            .repos
            .collection
            .get(collection_id)
            .ok()
            .flatten()
            .map(|c| c.workspace_id)
    }

    fn persist_request_body_blob(
        &self,
        request: &mut RequestItem,
        services: &Arc<AppServices>,
    ) -> Result<(), String> {
        match &request.body {
            BodyType::RawText { content } | BodyType::RawJson { content } => {
                let media = match &request.body {
                    BodyType::RawJson { .. } => Some("application/json"),
                    _ => Some("text/plain"),
                };
                let blob = services
                    .blob_store
                    .write_bytes(content.as_bytes(), media)
                    .map_err(|e| {
                        format!(
                            "{}: {e}",
                            es_fluent::localize("request_tab_save_failed", None)
                        )
                    })?;
                request.body_blob_hash = Some(blob.hash);
            }
            _ => {
                request.body_blob_hash = None;
            }
        }
        Ok(())
    }

    fn normalize_auth_secret_ownership_for_save(
        &self,
        request: &mut RequestItem,
        services: &Arc<AppServices>,
    ) -> Result<(), String> {
        let target_owner_kind = "request";
        let target_owner_id = request.id.to_string();

        let source_owner = match self.editor.identity() {
            EditorIdentity::Draft(draft_id) => Some(("request_draft", draft_id.to_string())),
            EditorIdentity::Persisted(id) => Some(("request", id.to_string())),
        };

        match &mut request.auth {
            AuthType::None => Ok(()),
            AuthType::Basic {
                password_secret_ref,
                ..
            } => self.rebind_secret_ref(
                password_secret_ref,
                "basic_password",
                source_owner.as_ref().map(|(k, v)| (*k, v.as_str())),
                target_owner_kind,
                &target_owner_id,
                services,
            ),
            AuthType::Bearer { token_secret_ref } => self.rebind_secret_ref(
                token_secret_ref,
                "bearer_token",
                source_owner.as_ref().map(|(k, v)| (*k, v.as_str())),
                target_owner_kind,
                &target_owner_id,
                services,
            ),
            AuthType::ApiKey {
                value_secret_ref, ..
            } => self.rebind_secret_ref(
                value_secret_ref,
                "api_key_value",
                source_owner.as_ref().map(|(k, v)| (*k, v.as_str())),
                target_owner_kind,
                &target_owner_id,
                services,
            ),
        }
    }

    fn rebind_secret_ref(
        &self,
        slot: &mut Option<String>,
        secret_kind: &str,
        source_owner: Option<(&str, &str)>,
        target_owner_kind: &str,
        target_owner_id: &str,
        services: &Arc<AppServices>,
    ) -> Result<(), String> {
        let Some(current_ref) = slot.clone() else {
            return Ok(());
        };

        let value = services
            .secret_store
            .get_secret(&current_ref)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_save_failed", None)
                )
            })?
            .ok_or_else(|| es_fluent::localize("request_tab_secret_missing", None).to_string())?;

        let new_ref = services
            .secret_manager
            .upsert_secret(target_owner_kind, target_owner_id, secret_kind, &value)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_save_failed", None)
                )
            })?;

        *slot = Some(new_ref.key_name.clone());

        if let Some((owner_kind, owner_id)) = source_owner {
            if owner_kind == "request_draft" {
                let _ = services
                    .secret_manager
                    .delete_secret(owner_kind, owner_id, secret_kind);
                if current_ref != new_ref.key_name {
                    let _ = services.secret_store.delete_secret(&current_ref);
                }
            }
        }

        Ok(())
    }

    fn clone_auth_secrets_for_duplicate(
        &self,
        source: &RequestItem,
        duplicate: &mut RequestItem,
        services: &Arc<AppServices>,
    ) -> Result<(), String> {
        let target_owner_id = duplicate.id.to_string();

        match (&source.auth, &mut duplicate.auth) {
            (
                AuthType::Basic {
                    password_secret_ref: src,
                    ..
                },
                AuthType::Basic {
                    password_secret_ref: dst,
                    ..
                },
            ) => self.clone_one_secret(
                src.as_ref(),
                dst,
                "basic_password",
                &target_owner_id,
                services,
            ),
            (
                AuthType::Bearer {
                    token_secret_ref: src,
                },
                AuthType::Bearer {
                    token_secret_ref: dst,
                },
            ) => self.clone_one_secret(
                src.as_ref(),
                dst,
                "bearer_token",
                &target_owner_id,
                services,
            ),
            (
                AuthType::ApiKey {
                    value_secret_ref: src,
                    ..
                },
                AuthType::ApiKey {
                    value_secret_ref: dst,
                    ..
                },
            ) => self.clone_one_secret(
                src.as_ref(),
                dst,
                "api_key_value",
                &target_owner_id,
                services,
            ),
            _ => Ok(()),
        }
    }

    fn clone_one_secret(
        &self,
        source_ref: Option<&String>,
        destination_ref: &mut Option<String>,
        secret_kind: &str,
        target_owner_id: &str,
        services: &Arc<AppServices>,
    ) -> Result<(), String> {
        let Some(source_ref) = source_ref else {
            *destination_ref = None;
            return Ok(());
        };

        let value = services
            .secret_store
            .get_secret(source_ref)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_duplicate_failed", None)
                )
            })?
            .ok_or_else(|| es_fluent::localize("request_tab_secret_missing", None).to_string())?;

        let new_ref = services
            .secret_manager
            .upsert_secret("request", target_owner_id, secret_kind, &value)
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    es_fluent::localize("request_tab_duplicate_failed", None)
                )
            })?;

        *destination_ref = Some(new_ref.key_name);
        Ok(())
    }

    fn set_active_section(&mut self, section: RequestSectionTab, cx: &mut Context<Self>) {
        if self.active_section != section {
            self.active_section = section;
            cx.notify();
        }
    }

    fn set_active_response_tab(&mut self, tab: ResponseTab, cx: &mut Context<Self>) {
        if self.active_response_tab != tab {
            self.active_response_tab = tab;
            cx.notify();
        }
    }

    fn open_settings_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
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
                        // Name field (moved from top of editor §4.1)
                        .child(
                            v_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(es_fluent::localize(
                                            "request_tab_name_label",
                                            None,
                                        )),
                                )
                                .child(Input::new(&name_input).large()),
                        )
                        .child(
                            v_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(es_fluent::localize(
                                            "request_tab_timeout_label",
                                            None,
                                        )),
                                )
                                .child(Input::new(&timeout_input).large()),
                        )
                        .child(
                            v_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(es_fluent::localize(
                                            "request_tab_follow_redirects_label",
                                            None,
                                        )),
                                )
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
        if self.draft_dirty {
            self.sync_inputs_from_draft(window, cx);
            self.draft_dirty = false;
        }
        let draft = self.editor.draft().clone();
        let save_status = self.editor.save_status().clone();
        let is_dirty = matches!(
            save_status,
            SaveStatus::Dirty | SaveStatus::SaveFailed { .. } | SaveStatus::Saving
        );

        let dirty_indicator = if is_dirty {
            div()
                .text_xs()
                .text_color(gpui::red())
                .child(es_fluent::localize("request_tab_dirty", None))
        } else {
            div()
        };

        let response_panel = response_panel::render_response_panel(self, window, cx);

        let preflight_panel = match self.editor.preflight_error() {
            Some(err) => div().text_sm().text_color(gpui::red()).child(format!(
                "{}: {}",
                es_fluent::localize("request_tab_preflight", None),
                err.message
            )),
            None => div(),
        };

        let latest_run = latest_run_summary(self.editor.exec_status());


        let section_content = match self.active_section {
            RequestSectionTab::Params => kv_editor::render_kv_table(
                &self.params_kv_table,
                KvTarget::Params,
                "params",
                &self.params_rows,
                cx,
            ).into_any_element(),
            RequestSectionTab::Auth => auth_editor::render_auth_editor(self, &draft, cx)
                .into_any_element(),
            RequestSectionTab::Headers => kv_editor::render_kv_table(
                &self.headers_kv_table,
                KvTarget::Headers,
                "headers",
                &self.headers_rows,
                cx,
            ).into_any_element(),
            RequestSectionTab::Body => body_editor::render_body_editor(
                self, &draft, window, cx,
            ).into_any_element(),
            RequestSectionTab::Scripts => v_flex()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(es_fluent::localize("request_tab_pre_request_label", None)),
                )
                .child(
                    div()
                        .w_full()
                        .child(Input::new(&self.pre_request_input).h(px(240.))),
                )
                .into_any_element(),
            RequestSectionTab::Tests => v_flex()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(es_fluent::localize("request_tab_tests_label", None)),
                )
                .child(
                    div()
                        .w_full()
                        .child(Input::new(&self.tests_input).h(px(240.))),
                )
                .into_any_element(),
        };

        v_flex()
            .size_full()
            .p_4()
            .gap_3()
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::handle_save_request))
            .on_action(cx.listener(Self::handle_send_request))
            .on_action(cx.listener(Self::handle_cancel_request))
            .on_action(cx.listener(Self::handle_duplicate_request))
            .on_action(cx.listener(Self::handle_focus_url_bar))
            .on_action(cx.listener(Self::handle_toggle_body_search))
            // §4.2: Unified URL bar — method, URL, and Send at equal height
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .h(px(36.))
                    .child(div().w(px(120.)).child(Select::new(&self.method_select).large()))
                    .child(div().flex_1().child(Input::new(&self.url_input).large()))
                    .child(
                        Button::new("request-send")
                            .primary()
                            .large()
                            .label(es_fluent::localize("request_tab_action_send", None))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.send(cx);
                            })),
                    ),
            )
            // §4.3: Compressed action buttons + latest run + settings in one row
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .flex_wrap()
                    // Dirty indicator (moved from §4.1 name row)
                    .when(is_dirty, |el| {
                        el.child(dirty_indicator)
                    })
                    // Save — always visible, ghost style
                    .child(
                        Button::new("request-save")
                            .ghost()
                            .label(es_fluent::localize("request_tab_action_save", None))
                            .on_click(cx.listener(|this, _, window, cx| match this.save(cx) {
                                Ok(()) => {
                                    window.push_notification(
                                        es_fluent::localize("request_tab_save_ok", None),
                                        cx,
                                    );
                                }
                                Err(err) => window.push_notification(err, cx),
                            })),
                    )
                    // Duplicate — ghost
                    .child(
                        Button::new("request-duplicate")
                            .ghost()
                            .label(es_fluent::localize("request_tab_action_duplicate", None))
                            .on_click(cx.listener(
                                |this, _, window, cx| match this.duplicate(cx) {
                                    Ok(_) => {
                                        window.push_notification(
                                            es_fluent::localize("request_tab_duplicate_ok", None),
                                            cx,
                                        );
                                    }
                                    Err(err) => window.push_notification(err, cx),
                                },
                            )),
                    )
                    // Cancel — only when sending/streaming
                    .when(
                        matches!(self.editor.exec_status(), ExecStatus::Sending | ExecStatus::Streaming),
                        |el| {
                            el.child(
                                Button::new("request-cancel")
                                    .ghost()
                                    .label(es_fluent::localize("request_tab_action_cancel", None))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.cancel_send(cx);
                                    })),
                            )
                        },
                    )
                    // Reload — only when a baseline exists
                    .when(self.editor.baseline().is_some(), |el| {
                        el.child(
                            Button::new("request-reload")
                                .ghost()
                                .label(es_fluent::localize("request_tab_action_reload", None))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.reload_baseline(cx);
                                })),
                        )
                    })
                    // Spacer to push latest run + settings to the right
                    .child(div().flex_1())
                    // Latest run summary
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{}: {}",
                                es_fluent::localize("request_tab_latest_run_label", None),
                                latest_run
                            )),
                    )
                    // Settings
                    .child(
                        Button::new("request-settings-open")
                            .ghost()
                            .label(es_fluent::localize("request_tab_settings_label", None))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_settings_dialog(window, cx);
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .flex_wrap()
                    .child(section_tab_button(
                        "request-tab-params",
                        es_fluent::localize("request_tab_params_label", None).to_string(),
                        self.active_section == RequestSectionTab::Params,
                        cx,
                        cx.listener(|this, _, _, cx| {
                            this.set_active_section(RequestSectionTab::Params, cx);
                        }),
                    ))
                    .child(section_tab_button(
                        "request-tab-auth",
                        es_fluent::localize("request_tab_auth_label", None).to_string(),
                        self.active_section == RequestSectionTab::Auth,
                        cx,
                        cx.listener(|this, _, _, cx| {
                            this.set_active_section(RequestSectionTab::Auth, cx);
                        }),
                    ))
                    .child(section_tab_button(
                        "request-tab-headers",
                        es_fluent::localize("request_tab_headers_label", None).to_string(),
                        self.active_section == RequestSectionTab::Headers,
                        cx,
                        cx.listener(|this, _, _, cx| {
                            this.set_active_section(RequestSectionTab::Headers, cx);
                        }),
                    ))
                    .child(section_tab_button(
                        "request-tab-body",
                        es_fluent::localize("request_tab_body_label", None).to_string(),
                        self.active_section == RequestSectionTab::Body,
                        cx,
                        cx.listener(|this, _, _, cx| {
                            this.set_active_section(RequestSectionTab::Body, cx);
                        }),
                    ))
                    .child(section_tab_button(
                        "request-tab-scripts",
                        es_fluent::localize("request_tab_scripts_label", None).to_string(),
                        self.active_section == RequestSectionTab::Scripts,
                        cx,
                        cx.listener(|this, _, _, cx| {
                            this.set_active_section(RequestSectionTab::Scripts, cx);
                        }),
                    ))
                    .child(section_tab_button(
                        "request-tab-tests",
                        es_fluent::localize("request_tab_tests_label", None).to_string(),
                        self.active_section == RequestSectionTab::Tests,
                        cx,
                        cx.listener(|this, _, _, cx| {
                            this.set_active_section(RequestSectionTab::Tests, cx);
                        }),
                    )),
            )
            // §4.5: Section content flows directly, no border box
            .child(
                v_flex()
                    .w_full()
                    .items_stretch()
                    .pt_2()
                    .child(div().w_full().child(section_content)),
            )
            .when(
                matches!(save_status, SaveStatus::SaveFailed { .. }),
                |el: gpui::Div| {
                    if let SaveStatus::SaveFailed { error } = &save_status {
                        el.child(div().text_sm().text_color(gpui::red()).child(error.clone()))
                    } else {
                        el
                    }
                },
            )
            .child(preflight_panel)
            // Response panel — no border box
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(es_fluent::localize("request_tab_response_label", None)),
                    )
                    .child(response_panel),
            )
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
