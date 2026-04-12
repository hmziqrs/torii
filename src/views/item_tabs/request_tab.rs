use std::sync::Arc;

use gpui::{prelude::*, *};
use gpui_component::{
    Disableable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputEvent, InputState, TabSize},
    select::{Select, SelectEvent, SelectState},
    v_flex,
};

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
    body_type_select: Entity<SelectState<Vec<&'static str>>>,
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
    _subscriptions: Vec<Subscription>,
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
        let body_type_select = cx.new(|cx| {
            let mut select = SelectState::new(
                vec![
                    "None",
                    "Raw Text",
                    "Raw JSON",
                    "URL Encoded",
                    "Form Data",
                    "Binary File",
                ],
                Some(gpui_component::IndexPath::default()),
                window,
                cx,
            );
            select.set_selected_index(
                Some(gpui_component::IndexPath::default().row(body_type_index(&initial.body))),
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

        subscriptions.push(cx.subscribe_in(
            &body_type_select,
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
                this.set_body_kind(body_kind_from_label(kind), cx);
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
            body_type_select,
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
            _subscriptions: subscriptions,
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
        let timeout_input = self.timeout_input.clone();
        let follow_redirects_input = self.follow_redirects_input.clone();

        window.open_dialog(cx, move |dialog, _, _| {
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
                                        .text_color(gpui::hsla(0., 0., 0.45, 1.))
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
                                        .text_color(gpui::hsla(0., 0., 0.45, 1.))
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

    fn auth_from_cached_inputs(&self, kind: AuthKind, cx: &App) -> AuthType {
        match kind {
            AuthKind::None => AuthType::None,
            AuthKind::Basic => {
                let username = self.auth_basic_username_input.read(cx).value().to_string();
                let password_ref = self
                    .auth_basic_password_ref_input
                    .read(cx)
                    .value()
                    .trim()
                    .to_string();
                AuthType::Basic {
                    username,
                    password_secret_ref: if password_ref.is_empty() {
                        None
                    } else {
                        Some(password_ref)
                    },
                }
            }
            AuthKind::Bearer => {
                let token_ref = self
                    .auth_bearer_token_ref_input
                    .read(cx)
                    .value()
                    .trim()
                    .to_string();
                AuthType::Bearer {
                    token_secret_ref: if token_ref.is_empty() {
                        None
                    } else {
                        Some(token_ref)
                    },
                }
            }
            AuthKind::ApiKey => {
                let key_name = self.auth_api_key_name_input.read(cx).value().to_string();
                let value_ref = self
                    .auth_api_key_value_ref_input
                    .read(cx)
                    .value()
                    .trim()
                    .to_string();
                let location_ix = self
                    .auth_api_key_location_select
                    .read(cx)
                    .selected_index(cx)
                    .map(|ix| ix.row)
                    .unwrap_or(0);
                AuthType::ApiKey {
                    key_name,
                    value_secret_ref: if value_ref.is_empty() {
                        None
                    } else {
                        Some(value_ref)
                    },
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

    fn selected_body_kind(&self, cx: &App) -> BodyKind {
        self.body_type_select
            .read(cx)
            .selected_value()
            .map(|label| body_kind_from_label(label))
            .unwrap_or(BodyKind::None)
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
        self.body_type_select.update(cx, |select, cx| {
            let ix = body_type_index(&draft.body);
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
                let password_ref = password_secret_ref.clone().unwrap_or_default();
                if self.auth_basic_password_ref_input.read(cx).value().as_ref()
                    != password_ref.as_str()
                {
                    self.auth_basic_password_ref_input.update(cx, |s, cx| {
                        s.set_value(password_ref.clone(), window, cx);
                    });
                }
            }
            AuthType::Bearer { token_secret_ref } => {
                let token_ref = token_secret_ref.clone().unwrap_or_default();
                if self.auth_bearer_token_ref_input.read(cx).value().as_ref() != token_ref.as_str()
                {
                    self.auth_bearer_token_ref_input.update(cx, |s, cx| {
                        s.set_value(token_ref.clone(), window, cx);
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
                let value_ref = value_secret_ref.clone().unwrap_or_default();
                if self.auth_api_key_value_ref_input.read(cx).value().as_ref() != value_ref.as_str()
                {
                    self.auth_api_key_value_ref_input.update(cx, |s, cx| {
                        s.set_value(value_ref.clone(), window, cx);
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

    fn kv_rows(&self, target: KvTarget) -> &Vec<KeyValueEditorRow> {
        match target {
            KvTarget::Params => &self.params_rows,
            KvTarget::Headers => &self.headers_rows,
            KvTarget::BodyUrlEncoded => &self.body_urlencoded_rows,
            KvTarget::BodyFormDataText => &self.body_form_text_rows,
        }
    }

    fn kv_rows_mut(&mut self, target: KvTarget) -> &mut Vec<KeyValueEditorRow> {
        match target {
            KvTarget::Params => &mut self.params_rows,
            KvTarget::Headers => &mut self.headers_rows,
            KvTarget::BodyUrlEncoded => &mut self.body_urlencoded_rows,
            KvTarget::BodyFormDataText => &mut self.body_form_text_rows,
        }
    }

    fn next_kv_row_id(&mut self) -> u64 {
        let id = self.next_kv_row_id;
        self.next_kv_row_id += 1;
        id
    }

    fn make_kv_row(
        &mut self,
        target: KvTarget,
        entry: KeyValuePair,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> KeyValueEditorRow {
        let id = self.next_kv_row_id();
        let key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(entry.key.clone(), window, cx);
            state
        });
        let value_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(entry.value.clone(), window, cx);
            state
        });

        self._subscriptions.push(cx.subscribe(
            &key_input,
            move |this: &mut RequestTabView, _: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.on_kv_rows_changed(target, cx);
                }
            },
        ));
        self._subscriptions.push(cx.subscribe(
            &value_input,
            move |this: &mut RequestTabView, _: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.on_kv_rows_changed(target, cx);
                }
            },
        ));

        KeyValueEditorRow {
            id,
            enabled: entry.enabled,
            key_input,
            value_input,
        }
    }

    fn rebuild_kv_rows(
        &mut self,
        target: KvTarget,
        entries: &[KeyValuePair],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let normalized = if entries.is_empty() {
            vec![KeyValuePair {
                key: String::new(),
                value: String::new(),
                enabled: true,
            }]
        } else {
            entries.to_vec()
        };
        let mut rows = Vec::with_capacity(normalized.len());
        for entry in normalized {
            rows.push(self.make_kv_row(target, entry, window, cx));
        }
        *self.kv_rows_mut(target) = rows;
        self.ensure_trailing_empty_row(target, window, cx);
    }

    fn sync_kv_rows_with_draft(
        &mut self,
        target: KvTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if target == KvTarget::BodyUrlEncoded
            && !matches!(self.editor.draft().body, BodyType::UrlEncoded { .. })
        {
            return;
        }
        if target == KvTarget::BodyFormDataText
            && !matches!(self.editor.draft().body, BodyType::FormData { .. })
        {
            return;
        }
        let draft_entries = match target {
            KvTarget::Params => self.editor.draft().params.clone(),
            KvTarget::Headers => self.editor.draft().headers.clone(),
            KvTarget::BodyUrlEncoded => match &self.editor.draft().body {
                BodyType::UrlEncoded { entries } => entries.clone(),
                _ => Vec::new(),
            },
            KvTarget::BodyFormDataText => match &self.editor.draft().body {
                BodyType::FormData { text_fields, .. } => text_fields.clone(),
                _ => Vec::new(),
            },
        };
        let current = self.collect_meaningful_pairs(target, cx);
        if current != draft_entries {
            self.rebuild_kv_rows(target, &draft_entries, window, cx);
        }
    }

    fn ensure_trailing_empty_row(
        &mut self,
        target: KvTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let has_trailing_empty = self
            .kv_rows(target)
            .last()
            .map(|row| {
                row.key_input.read(cx).value().trim().is_empty()
                    && row.value_input.read(cx).value().trim().is_empty()
            })
            .unwrap_or(false);
        if !has_trailing_empty {
            let empty = KeyValuePair {
                key: String::new(),
                value: String::new(),
                enabled: true,
            };
            let row = self.make_kv_row(target, empty, window, cx);
            self.kv_rows_mut(target).push(row);
        }
    }

    fn collect_meaningful_pairs(&self, target: KvTarget, cx: &App) -> Vec<KeyValuePair> {
        self.kv_rows(target)
            .iter()
            .filter_map(|row| {
                let key = row.key_input.read(cx).value().to_string();
                let value = row.value_input.read(cx).value().to_string();
                if key.trim().is_empty() && value.trim().is_empty() {
                    None
                } else {
                    Some(KeyValuePair {
                        key,
                        value,
                        enabled: row.enabled,
                    })
                }
            })
            .collect()
    }

    fn on_kv_rows_changed(&mut self, target: KvTarget, cx: &mut Context<Self>) {
        if target == KvTarget::Params && self.input_sync_guard.is_active() {
            self.input_sync_guard.deferred = true;
            return;
        }
        let next = self.collect_meaningful_pairs(target, cx);
        match target {
            KvTarget::Params => {
                if self.editor.draft().params != next {
                    self.editor.draft_mut().params = next;
                }
                let next_url = url_with_params(
                    self.editor.draft().url.as_str(),
                    self.editor.draft().params.as_slice(),
                );
                if self.editor.draft().url != next_url {
                    self.editor.draft_mut().url = next_url;
                }
            }
            KvTarget::Headers => {
                if self.editor.draft().headers != next {
                    self.editor.draft_mut().headers = next;
                }
            }
            KvTarget::BodyUrlEncoded => {
                if self.selected_body_kind(cx) != BodyKind::UrlEncoded {
                    return;
                }
                let next_body = BodyType::UrlEncoded { entries: next };
                if self.editor.draft().body != next_body {
                    self.editor.draft_mut().body = next_body;
                }
            }
            KvTarget::BodyFormDataText => {
                if self.selected_body_kind(cx) != BodyKind::FormData {
                    return;
                }
                let file_fields = match &self.editor.draft().body {
                    BodyType::FormData { file_fields, .. } => file_fields.clone(),
                    _ => Vec::new(),
                };
                let next_body = BodyType::FormData {
                    text_fields: next,
                    file_fields,
                };
                if self.editor.draft().body != next_body {
                    self.editor.draft_mut().body = next_body;
                }
            }
        }
        self.editor.refresh_save_status();
        cx.notify();
    }

    fn add_kv_row(&mut self, target: KvTarget, window: &mut Window, cx: &mut Context<Self>) {
        let row = self.make_kv_row(
            target,
            KeyValuePair {
                key: String::new(),
                value: String::new(),
                enabled: true,
            },
            window,
            cx,
        );
        self.kv_rows_mut(target).push(row);
        cx.notify();
    }

    fn remove_kv_row(
        &mut self,
        target: KvTarget,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ix) = self.kv_rows(target).iter().position(|row| row.id == id) {
            self.kv_rows_mut(target).remove(ix);
        }
        self.ensure_trailing_empty_row(target, window, cx);
        self.on_kv_rows_changed(target, cx);
    }

    fn set_kv_row_enabled(
        &mut self,
        target: KvTarget,
        id: u64,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self.kv_rows_mut(target).iter_mut().find(|row| row.id == id) {
            row.enabled = enabled;
            self.on_kv_rows_changed(target, cx);
        }
    }

    fn add_form_data_file_field(&mut self, cx: &mut Context<Self>) {
        let BodyType::FormData {
            text_fields,
            mut file_fields,
        } = self.editor.draft().body.clone()
        else {
            return;
        };
        file_fields.push(crate::domain::request::FileField {
            key: format!("file{}", file_fields.len() + 1),
            blob_hash: String::new(),
            file_name: None,
            enabled: true,
        });
        self.editor.draft_mut().body = BodyType::FormData {
            text_fields,
            file_fields,
        };
        self.editor.refresh_save_status();
        cx.notify();
    }

    fn remove_form_data_file_field(&mut self, index: usize, cx: &mut Context<Self>) {
        let BodyType::FormData {
            text_fields,
            mut file_fields,
        } = self.editor.draft().body.clone()
        else {
            return;
        };
        if index < file_fields.len() {
            file_fields.remove(index);
            self.editor.draft_mut().body = BodyType::FormData {
                text_fields,
                file_fields,
            };
            self.editor.refresh_save_status();
            cx.notify();
        }
    }

    fn set_form_data_file_field_enabled(
        &mut self,
        index: usize,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        let BodyType::FormData {
            text_fields,
            mut file_fields,
        } = self.editor.draft().body.clone()
        else {
            return;
        };
        if let Some(field) = file_fields.get_mut(index) {
            field.enabled = enabled;
            self.editor.draft_mut().body = BodyType::FormData {
                text_fields,
                file_fields,
            };
            self.editor.refresh_save_status();
            cx.notify();
        }
    }

    fn clear_form_data_file_field(&mut self, index: usize, cx: &mut Context<Self>) {
        let BodyType::FormData {
            text_fields,
            mut file_fields,
        } = self.editor.draft().body.clone()
        else {
            return;
        };
        if let Some(field) = file_fields.get_mut(index) {
            field.blob_hash.clear();
            field.file_name = None;
            self.editor.draft_mut().body = BodyType::FormData {
                text_fields,
                file_fields,
            };
            self.editor.refresh_save_status();
            cx.notify();
        }
    }

    fn clear_binary_body_file(&mut self, cx: &mut Context<Self>) {
        if let BodyType::BinaryFile {
            blob_hash,
            file_name,
        } = &mut self.editor.draft_mut().body
        {
            blob_hash.clear();
            *file_name = None;
            self.editor.refresh_save_status();
            cx.notify();
        }
    }

    fn pick_binary_body_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_body_file_for_target(BodyFileTarget::Binary, window, cx);
    }

    fn pick_form_data_file_field(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pick_body_file_for_target(BodyFileTarget::FormDataIndex(index), window, cx);
    }

    fn pick_body_file_for_target(
        &mut self,
        target: BodyFileTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                es_fluent::localize("request_tab_body_pick_file", None)
                    .to_string()
                    .into(),
            ),
        });
        let services = cx.global::<AppServicesGlobal>().0.clone();

        cx.spawn_in(window, async move |this, cx| {
            let picked_path = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let Some(path) = picked_path else {
                return;
            };

            let size_bytes = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            if size_bytes > LARGE_BODY_FILE_CONFIRM_BYTES {
                let detail = format!(
                    "{} {}",
                    es_fluent::localize("request_tab_body_large_file_detail", None),
                    format_bytes(size_bytes)
                );
                let answers = vec![
                    gpui::PromptButton::ok(es_fluent::localize(
                        "request_tab_body_large_file_continue",
                        None,
                    )),
                    gpui::PromptButton::cancel(es_fluent::localize(
                        "request_tab_body_large_file_cancel",
                        None,
                    )),
                ];
                let response = cx
                    .prompt(
                        gpui::PromptLevel::Warning,
                        &es_fluent::localize("request_tab_body_large_file_title", None),
                        Some(&detail),
                        &answers,
                    )
                    .await
                    .unwrap_or(1);
                if response != 0 {
                    return;
                }
            }

            let path_for_import = path.clone();
            let services_for_import = services.clone();
            let imported =
                tokio::task::spawn_blocking(move || -> Result<(String, Option<String>), String> {
                    let file = std::fs::File::open(&path_for_import).map_err(|e| {
                        format!(
                            "{}: {e}",
                            es_fluent::localize("request_tab_body_file_load_failed", None)
                        )
                    })?;
                    let blob = services_for_import
                        .blob_store
                        .write_from_reader(file, None)
                        .map_err(|e| {
                            format!(
                                "{}: {e}",
                                es_fluent::localize("request_tab_body_file_load_failed", None)
                            )
                        })?;
                    let file_name = path_for_import
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string());
                    Ok((blob.hash, file_name))
                })
                .await;

            let (blob_hash, file_name) = match imported {
                Ok(Ok(value)) => value,
                Ok(Err(err)) => {
                    let _ = this.update(cx, |this, cx| {
                        this.editor.set_preflight_error(err);
                        cx.notify();
                    });
                    return;
                }
                Err(err) => {
                    let _ = this.update(cx, |this, cx| {
                        this.editor
                            .set_preflight_error(format!("file import task failed: {err}"));
                        cx.notify();
                    });
                    return;
                }
            };

            let _ = this.update(cx, |this, cx| {
                this.apply_body_file_selection(target, blob_hash.clone(), file_name.clone());
                this.editor.refresh_save_status();
                cx.notify();
            });
        })
        .detach();
    }

    fn apply_body_file_selection(
        &mut self,
        target: BodyFileTarget,
        blob_hash: String,
        file_name: Option<String>,
    ) {
        match target {
            BodyFileTarget::Binary => {
                if let BodyType::BinaryFile {
                    blob_hash: current_hash,
                    file_name: current_name,
                } = &mut self.editor.draft_mut().body
                {
                    *current_hash = blob_hash;
                    *current_name = file_name;
                }
            }
            BodyFileTarget::FormDataIndex(index) => {
                if let BodyType::FormData { file_fields, .. } = &mut self.editor.draft_mut().body
                    && let Some(field) = file_fields.get_mut(index)
                {
                    field.blob_hash = blob_hash;
                    field.file_name = file_name;
                }
            }
        }
    }
}

impl Focusable for RequestTabView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for RequestTabView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_inputs_from_draft(window, cx);
        let draft = self.editor.draft().clone();
        let request = &draft;
        let save_status = self.editor.save_status().clone();
        let is_dirty = matches!(
            save_status,
            SaveStatus::Dirty | SaveStatus::SaveFailed { .. } | SaveStatus::Saving
        );
        let exec_status = self.editor.exec_status();

        let dirty_indicator = if is_dirty {
            div()
                .text_xs()
                .text_color(gpui::red())
                .child(es_fluent::localize("request_tab_dirty", None))
        } else {
            div()
        };

        let response_panel = match exec_status {
            ExecStatus::Idle => div().child(
                div()
                    .text_sm()
                    .text_color(gpui::hsla(0., 0., 0.5, 1.))
                    .child(es_fluent::localize("request_tab_response_empty", None)),
            ),
            ExecStatus::Sending => div().child(
                div()
                    .text_sm()
                    .text_color(gpui::hsla(0., 0., 0.5, 1.))
                    .child(es_fluent::localize("request_tab_sending", None)),
            ),
            ExecStatus::Streaming => div().child(
                div()
                    .text_sm()
                    .text_color(gpui::hsla(0., 0., 0.5, 1.))
                    .child(es_fluent::localize("request_tab_streaming", None)),
            ),
            ExecStatus::Completed { response } => {
                let resp = response.as_ref();
                let status_color = status_code_color(resp.status_code);
                let status_size = format_bytes(resp.body_ref.size_bytes());

                let mut body_preview =
                    response_body_preview_text(resp, &self.loaded_full_body_text);
                let (header_rows, header_format) =
                    parse_response_header_rows(resp.headers_json.as_deref());
                let cookies = parse_set_cookie_rows(&header_rows);

                let load_full_button = match &resp.body_ref {
                    BodyRef::DiskBlob { blob_id, .. } => {
                        if self.loaded_full_body_blob_id.as_deref() == Some(blob_id.as_str()) {
                            if let Some(full) = &self.loaded_full_body_text {
                                body_preview = full.clone();
                            }
                            div()
                        } else {
                            div().child(
                                Button::new("request-load-full-body")
                                    .outline()
                                    .label(es_fluent::localize(
                                        "request_tab_action_load_full_body",
                                        None,
                                    ))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if let Err(err) = this.load_full_response_body(cx) {
                                            window.push_notification(err, cx);
                                        }
                                    })),
                            )
                        }
                    }
                    _ => div(),
                };

                let body_search_query = self.body_search_input.read(cx).value().to_string();
                let body_matches = search_matches(&body_preview, &body_search_query);

                let mut body_content = if looks_like_image(resp.media_type.as_deref()) {
                    div()
                        .text_sm()
                        .text_color(gpui::hsla(0., 0., 0.5, 1.))
                        .child(es_fluent::localize(
                            "request_tab_response_image_preview_todo",
                            None,
                        ))
                } else if !body_preview.is_empty() {
                    div()
                        .mt_2()
                        .p_3()
                        .rounded(px(4.))
                        .bg(gpui::hsla(0., 0., 0.97, 1.))
                        .text_sm()
                        .font_family("monospace")
                        .child(body_preview)
                } else {
                    div()
                        .text_sm()
                        .text_color(gpui::hsla(0., 0., 0.5, 1.))
                        .child(es_fluent::localize("request_tab_response_body_empty", None))
                };

                if self.body_search_visible {
                    body_content = v_flex()
                        .gap_2()
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .flex_1()
                                        .child(Input::new(&self.body_search_input).large()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(gpui::hsla(0., 0., 0.5, 1.))
                                        .child(format!(
                                            "{} {}",
                                            body_matches.len(),
                                            es_fluent::localize("request_tab_search_matches", None)
                                        )),
                                ),
                        )
                        .child(body_content)
                }

                let headers_content = if header_rows.is_empty() {
                    div()
                        .text_sm()
                        .text_color(gpui::hsla(0., 0., 0.5, 1.))
                        .child(es_fluent::localize(
                            "request_tab_response_headers_empty",
                            None,
                        ))
                } else {
                    v_flex()
                        .gap_1()
                        .when(
                            matches!(header_format, Some(HeaderJsonFormat::LegacyObjectMap)),
                            |el: gpui::Div| {
                                el.child(
                                    div()
                                        .text_xs()
                                        .text_color(gpui::hsla(30. / 360., 0.9, 0.38, 1.))
                                        .child(es_fluent::localize(
                                            "request_tab_response_headers_legacy_note",
                                            None,
                                        )),
                                )
                            },
                        )
                        .children(header_rows.iter().enumerate().map(|(idx, row)| {
                            h_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .font_family("monospace")
                                        .text_sm()
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .child(row.name.clone()),
                                )
                                .child(
                                    div()
                                        .font_family("monospace")
                                        .text_sm()
                                        .text_color(gpui::hsla(0., 0., 0.35, 1.))
                                        .child(row.value.clone()),
                                )
                                .id(("response-header-row", idx))
                        }))
                };

                let cookies_content = if cookies.is_empty() {
                    div()
                        .text_sm()
                        .text_color(gpui::hsla(0., 0., 0.5, 1.))
                        .child(es_fluent::localize(
                            "request_tab_response_cookies_empty",
                            None,
                        ))
                } else {
                    v_flex().gap_1().children(cookies.iter().enumerate().map(|(idx, cookie)| {
                        let same_site = cookie.same_site.clone().unwrap_or_else(|| "—".to_string());
                        let expires = cookie
                            .expires_or_max_age
                            .clone()
                            .unwrap_or_else(|| "—".to_string());
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .font_family("monospace")
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(cookie.name.clone()),
                            )
                            .child(
                                div()
                                    .font_family("monospace")
                                    .text_sm()
                                    .text_color(gpui::hsla(0., 0., 0.35, 1.))
                                    .child(format!(
                                        "{}; domain={}; path={}; expires/max-age={}; secure={}; httpOnly={}; sameSite={}",
                                        cookie.value_preview,
                                        cookie.domain.clone().unwrap_or_else(|| "—".to_string()),
                                        cookie.path.clone().unwrap_or_else(|| "—".to_string()),
                                        expires,
                                        if cookie.secure { "true" } else { "false" },
                                        if cookie.http_only { "true" } else { "false" },
                                        same_site,
                                    )),
                            )
                            .id(("response-cookie-row", idx))
                    }))
                };

                let timing_content = v_flex()
                    .gap_1()
                    .child(timing_row(
                        es_fluent::localize("request_tab_response_timing_total", None).to_string(),
                        resp.total_ms
                            .map(|ms| format!("{ms} ms"))
                            .unwrap_or_else(|| "—".to_string()),
                    ))
                    .child(timing_row(
                        es_fluent::localize("request_tab_response_timing_ttfb", None).to_string(),
                        resp.ttfb_ms
                            .map(|ms| format!("{ms} ms"))
                            .unwrap_or_else(|| "—".to_string()),
                    ))
                    .child(timing_row(
                        es_fluent::localize("request_tab_response_timing_dispatched_at", None)
                            .to_string(),
                        format_unix_ms(resp.dispatched_at_unix_ms),
                    ))
                    .child(timing_row(
                        es_fluent::localize("request_tab_response_timing_first_byte_at", None)
                            .to_string(),
                        format_unix_ms(resp.first_byte_at_unix_ms),
                    ))
                    .child(timing_row(
                        es_fluent::localize("request_tab_response_timing_completed_at", None)
                            .to_string(),
                        format_unix_ms(resp.completed_at_unix_ms),
                    ))
                    .child(timing_row(
                        es_fluent::localize("request_tab_response_timing_dns", None).to_string(),
                        "—".to_string(),
                    ))
                    .child(timing_row(
                        es_fluent::localize("request_tab_response_timing_tcp", None).to_string(),
                        "—".to_string(),
                    ))
                    .child(timing_row(
                        es_fluent::localize("request_tab_response_timing_tls", None).to_string(),
                        "—".to_string(),
                    ));

                let body_actions = h_flex()
                    .gap_1()
                    .child(
                        Button::new("request-response-copy")
                            .outline()
                            .disabled(!is_text_like_media_type(resp.media_type.as_deref()))
                            .label(es_fluent::localize(
                                "request_tab_response_action_copy",
                                None,
                            ))
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Err(err) = this.copy_response_body(cx) {
                                    window.push_notification(err, cx);
                                } else {
                                    window.push_notification(
                                        es_fluent::localize("request_tab_copy_ok", None),
                                        cx,
                                    );
                                }
                            })),
                    )
                    .child(
                        Button::new("request-response-save")
                            .outline()
                            .label(es_fluent::localize(
                                "request_tab_response_action_save",
                                None,
                            ))
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Err(err) = this.save_response_body_to_file(window, cx) {
                                    window.push_notification(err, cx);
                                }
                            })),
                    );

                let active_content = match self.active_response_tab {
                    ResponseTab::Body => body_content.into_any_element(),
                    ResponseTab::Headers => headers_content.into_any_element(),
                    ResponseTab::Cookies => cookies_content.into_any_element(),
                    ResponseTab::Timing => timing_content.into_any_element(),
                };

                div()
                    .gap_2()
                    .child(
                        h_flex().gap_3().items_center().child(
                            div()
                                .text_sm()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(status_color)
                                .child(format!("{} {}", resp.status_code, resp.status_text)),
                        ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .text_xs()
                            .text_color(gpui::hsla(0., 0., 0.5, 1.))
                            .child(format!(
                                "{}: {}",
                                es_fluent::localize("request_tab_response_size", None),
                                status_size
                            ))
                            .child("•")
                            .child(format!(
                                "{}: {}",
                                es_fluent::localize("request_tab_response_total_time", None),
                                resp.total_ms
                                    .map(|ms| format!("{ms} ms"))
                                    .unwrap_or_else(|| "—".to_string())
                            )),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .flex_wrap()
                            .child(response_tab_button(
                                "request-response-tab-body",
                                es_fluent::localize("request_tab_response_tab_body", None)
                                    .to_string(),
                                self.active_response_tab == ResponseTab::Body,
                                cx.listener(|this, _, _, cx| {
                                    this.set_active_response_tab(ResponseTab::Body, cx);
                                }),
                            ))
                            .child(response_tab_button(
                                "request-response-tab-headers",
                                es_fluent::localize("request_tab_response_tab_headers", None)
                                    .to_string(),
                                self.active_response_tab == ResponseTab::Headers,
                                cx.listener(|this, _, _, cx| {
                                    this.set_active_response_tab(ResponseTab::Headers, cx);
                                }),
                            ))
                            .child(response_tab_button(
                                "request-response-tab-cookies",
                                es_fluent::localize("request_tab_response_tab_cookies", None)
                                    .to_string(),
                                self.active_response_tab == ResponseTab::Cookies,
                                cx.listener(|this, _, _, cx| {
                                    this.set_active_response_tab(ResponseTab::Cookies, cx);
                                }),
                            ))
                            .child(response_tab_button(
                                "request-response-tab-timing",
                                es_fluent::localize("request_tab_response_tab_timing", None)
                                    .to_string(),
                                self.active_response_tab == ResponseTab::Timing,
                                cx.listener(|this, _, _, cx| {
                                    this.set_active_response_tab(ResponseTab::Timing, cx);
                                }),
                            )),
                    )
                    .when(
                        self.active_response_tab == ResponseTab::Body,
                        |el: gpui::Div| el.child(body_actions),
                    )
                    .child(active_content)
                    .child(load_full_button)
            }
            ExecStatus::Failed {
                summary,
                classified,
            } => {
                let (title, detail) = classified_error_display(classified.as_ref(), summary);
                div()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(gpui::red())
                            .child(title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_family("monospace")
                            .text_color(gpui::hsla(0., 0., 0.45, 1.))
                            .child(detail),
                    )
            }
            ExecStatus::Cancelled { partial_size } => {
                let msg = match partial_size {
                    Some(size) => format!(
                        "{} ({size})",
                        es_fluent::localize("request_tab_response_cancelled_with_bytes", None)
                    ),
                    None => es_fluent::localize("request_tab_response_cancelled", None).to_string(),
                };
                div().child(
                    div()
                        .text_sm()
                        .text_color(gpui::hsla(30. / 360., 0.8, 0.45, 1.))
                        .child(msg),
                )
            }
        };

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
            RequestSectionTab::Params => {
                let row_ids = self
                    .params_rows
                    .iter()
                    .map(|row| {
                        (
                            row.id,
                            row.enabled,
                            row.key_input.clone(),
                            row.value_input.clone(),
                        )
                    })
                    .collect::<Vec<_>>();
                v_flex()
                    .gap_2()
                    .children(
                        row_ids
                            .into_iter()
                            .map(|(id, enabled, key_input, value_input)| {
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        Checkbox::new(("params-enabled", id))
                                            .checked(enabled)
                                            .on_click(cx.listener(move |this, checked, _, cx| {
                                                this.set_kv_row_enabled(
                                                    KvTarget::Params,
                                                    id,
                                                    *checked,
                                                    cx,
                                                );
                                            })),
                                    )
                                    .child(div().flex_1().child(Input::new(&key_input).large()))
                                    .child(div().flex_1().child(Input::new(&value_input).large()))
                                    .child(
                                        Button::new(("params-remove", id))
                                            .ghost()
                                            .label(es_fluent::localize(
                                                "request_tab_kv_remove_row",
                                                None,
                                            ))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.remove_kv_row(
                                                    KvTarget::Params,
                                                    id,
                                                    window,
                                                    cx,
                                                );
                                            })),
                                    )
                            }),
                    )
                    .child(
                        Button::new("params-add-row")
                            .outline()
                            .label(es_fluent::localize("request_tab_kv_add_row", None))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_kv_row(KvTarget::Params, window, cx);
                            })),
                    )
                    .into_any_element()
            }
            RequestSectionTab::Auth => v_flex()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(gpui::hsla(0., 0., 0.45, 1.))
                        .child(es_fluent::localize("request_tab_auth_type_label", None)),
                )
                .child(div().w_56().child(Select::new(&self.auth_type_select)))
                .child(match &request.auth {
                    AuthType::None => div()
                        .text_xs()
                        .text_color(gpui::hsla(0., 0., 0.45, 1.))
                        .child(es_fluent::localize("request_tab_auth_none_hint", None))
                        .into_any_element(),
                    AuthType::Basic { .. } => v_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(gpui::hsla(0., 0., 0.45, 1.))
                                .child(es_fluent::localize(
                                    "request_tab_auth_basic_username",
                                    None,
                                )),
                        )
                        .child(Input::new(&self.auth_basic_username_input).large())
                        .child(
                            div()
                                .text_xs()
                                .text_color(gpui::hsla(0., 0., 0.45, 1.))
                                .child(es_fluent::localize(
                                    "request_tab_auth_basic_password_ref",
                                    None,
                                )),
                        )
                        .child(Input::new(&self.auth_basic_password_ref_input).large())
                        .into_any_element(),
                    AuthType::Bearer { .. } => v_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(gpui::hsla(0., 0., 0.45, 1.))
                                .child(es_fluent::localize(
                                    "request_tab_auth_bearer_token_ref",
                                    None,
                                )),
                        )
                        .child(Input::new(&self.auth_bearer_token_ref_input).large())
                        .into_any_element(),
                    AuthType::ApiKey { .. } => v_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(gpui::hsla(0., 0., 0.45, 1.))
                                .child(es_fluent::localize("request_tab_auth_api_key_name", None)),
                        )
                        .child(Input::new(&self.auth_api_key_name_input).large())
                        .child(
                            div()
                                .text_xs()
                                .text_color(gpui::hsla(0., 0., 0.45, 1.))
                                .child(es_fluent::localize(
                                    "request_tab_auth_api_key_value_ref",
                                    None,
                                )),
                        )
                        .child(Input::new(&self.auth_api_key_value_ref_input).large())
                        .child(
                            div()
                                .text_xs()
                                .text_color(gpui::hsla(0., 0., 0.45, 1.))
                                .child(es_fluent::localize(
                                    "request_tab_auth_api_key_location",
                                    None,
                                )),
                        )
                        .child(
                            div()
                                .w_56()
                                .child(Select::new(&self.auth_api_key_location_select)),
                        )
                        .into_any_element(),
                })
                .into_any_element(),
            RequestSectionTab::Headers => {
                let row_ids = self
                    .headers_rows
                    .iter()
                    .map(|row| {
                        (
                            row.id,
                            row.enabled,
                            row.key_input.clone(),
                            row.value_input.clone(),
                        )
                    })
                    .collect::<Vec<_>>();
                v_flex()
                    .gap_2()
                    .children(
                        row_ids
                            .into_iter()
                            .map(|(id, enabled, key_input, value_input)| {
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        Checkbox::new(("headers-enabled", id))
                                            .checked(enabled)
                                            .on_click(cx.listener(move |this, checked, _, cx| {
                                                this.set_kv_row_enabled(
                                                    KvTarget::Headers,
                                                    id,
                                                    *checked,
                                                    cx,
                                                );
                                            })),
                                    )
                                    .child(div().flex_1().child(Input::new(&key_input).large()))
                                    .child(div().flex_1().child(Input::new(&value_input).large()))
                                    .child(
                                        Button::new(("headers-remove", id))
                                            .ghost()
                                            .label(es_fluent::localize(
                                                "request_tab_kv_remove_row",
                                                None,
                                            ))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.remove_kv_row(
                                                    KvTarget::Headers,
                                                    id,
                                                    window,
                                                    cx,
                                                );
                                            })),
                                    )
                            }),
                    )
                    .child(
                        Button::new("headers-add-row")
                            .outline()
                            .label(es_fluent::localize("request_tab_kv_add_row", None))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_kv_row(KvTarget::Headers, window, cx);
                            })),
                    )
                    .into_any_element()
            }
            RequestSectionTab::Body => {
                let urlencoded_row_ids = self
                    .body_urlencoded_rows
                    .iter()
                    .map(|row| {
                        (
                            row.id,
                            row.enabled,
                            row.key_input.clone(),
                            row.value_input.clone(),
                        )
                    })
                    .collect::<Vec<_>>();
                let form_text_row_ids = self
                    .body_form_text_rows
                    .iter()
                    .map(|row| {
                        (
                            row.id,
                            row.enabled,
                            row.key_input.clone(),
                            row.value_input.clone(),
                        )
                    })
                    .collect::<Vec<_>>();
                v_flex()
                    .w_full()
                    .items_stretch()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(gpui::hsla(0., 0., 0.45, 1.))
                            .child(es_fluent::localize("request_tab_body_type_label", None)),
                    )
                    .child(div().w_56().child(Select::new(&self.body_type_select)))
                    .child(match &request.body {
                        BodyType::None => div()
                            .text_xs()
                            .text_color(gpui::hsla(0., 0., 0.45, 1.))
                            .child(es_fluent::localize("request_tab_body_none_hint", None))
                            .into_any_element(),
                        BodyType::RawText { .. } => div()
                            .w_full()
                            .child(Input::new(&self.body_raw_text_input).w_full().h(px(220.)))
                            .into_any_element(),
                        BodyType::RawJson { .. } => div()
                            .w_full()
                            .child(Input::new(&self.body_raw_json_input).w_full().h(px(220.)))
                            .into_any_element(),
                        BodyType::UrlEncoded { .. } => v_flex()
                            .gap_2()
                            .children(urlencoded_row_ids.into_iter().map(
                                |(id, enabled, key_input, value_input)| {
                                    h_flex()
                                        .gap_2()
                                        .items_center()
                                        .child(
                                            Checkbox::new(("body-urlencoded-enabled", id))
                                                .checked(enabled)
                                                .on_click(cx.listener(
                                                    move |this, checked, _, cx| {
                                                        this.set_kv_row_enabled(
                                                            KvTarget::BodyUrlEncoded,
                                                            id,
                                                            *checked,
                                                            cx,
                                                        );
                                                    },
                                                )),
                                        )
                                        .child(div().flex_1().child(Input::new(&key_input).large()))
                                        .child(
                                            div().flex_1().child(Input::new(&value_input).large()),
                                        )
                                        .child(
                                            Button::new(("body-urlencoded-remove", id))
                                                .ghost()
                                                .label(es_fluent::localize(
                                                    "request_tab_kv_remove_row",
                                                    None,
                                                ))
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.remove_kv_row(
                                                            KvTarget::BodyUrlEncoded,
                                                            id,
                                                            window,
                                                            cx,
                                                        );
                                                    },
                                                )),
                                        )
                                },
                            ))
                            .child(
                                Button::new("body-urlencoded-add-row")
                                    .outline()
                                    .label(es_fluent::localize("request_tab_kv_add_row", None))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.add_kv_row(KvTarget::BodyUrlEncoded, window, cx);
                                    })),
                            )
                            .into_any_element(),
                        BodyType::FormData { file_fields, .. } => v_flex()
                            .gap_3()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(gpui::hsla(0., 0., 0.45, 1.))
                                    .child(es_fluent::localize(
                                        "request_tab_body_form_text_fields",
                                        None,
                                    )),
                            )
                            .children(form_text_row_ids.into_iter().map(
                                |(id, enabled, key_input, value_input)| {
                                    h_flex()
                                        .gap_2()
                                        .items_center()
                                        .child(
                                            Checkbox::new(("body-form-text-enabled", id))
                                                .checked(enabled)
                                                .on_click(cx.listener(
                                                    move |this, checked, _, cx| {
                                                        this.set_kv_row_enabled(
                                                            KvTarget::BodyFormDataText,
                                                            id,
                                                            *checked,
                                                            cx,
                                                        );
                                                    },
                                                )),
                                        )
                                        .child(div().flex_1().child(Input::new(&key_input).large()))
                                        .child(
                                            div().flex_1().child(Input::new(&value_input).large()),
                                        )
                                        .child(
                                            Button::new(("body-form-text-remove", id))
                                                .ghost()
                                                .label(es_fluent::localize(
                                                    "request_tab_kv_remove_row",
                                                    None,
                                                ))
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.remove_kv_row(
                                                            KvTarget::BodyFormDataText,
                                                            id,
                                                            window,
                                                            cx,
                                                        );
                                                    },
                                                )),
                                        )
                                },
                            ))
                            .child(
                                Button::new("body-form-text-add-row")
                                    .outline()
                                    .label(es_fluent::localize("request_tab_kv_add_row", None))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.add_kv_row(KvTarget::BodyFormDataText, window, cx);
                                    })),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(gpui::hsla(0., 0., 0.45, 1.))
                                    .child(es_fluent::localize(
                                        "request_tab_body_form_file_fields",
                                        None,
                                    )),
                            )
                            .children(file_fields.iter().enumerate().map(|(index, field)| {
                                let file_label = field
                                    .file_name
                                    .clone()
                                    .filter(|name| !name.trim().is_empty())
                                    .unwrap_or_else(|| {
                                        es_fluent::localize(
                                            "request_tab_body_no_file_selected",
                                            None,
                                        )
                                        .to_string()
                                    });
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        Checkbox::new(("body-form-file-enabled", index))
                                            .checked(field.enabled)
                                            .on_click(cx.listener(move |this, checked, _, cx| {
                                                this.set_form_data_file_field_enabled(
                                                    index, *checked, cx,
                                                );
                                            })),
                                    )
                                    .child(div().w_32().child(field.key.clone()))
                                    .child(div().flex_1().child(file_label))
                                    .child(
                                        Button::new(("body-form-file-pick", index))
                                            .outline()
                                            .label(if field.blob_hash.trim().is_empty() {
                                                es_fluent::localize(
                                                    "request_tab_body_pick_file",
                                                    None,
                                                )
                                            } else {
                                                es_fluent::localize(
                                                    "request_tab_body_replace_file",
                                                    None,
                                                )
                                            })
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.pick_form_data_file_field(index, window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new(("body-form-file-clear", index))
                                            .ghost()
                                            .label(es_fluent::localize(
                                                "request_tab_body_clear_file",
                                                None,
                                            ))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.clear_form_data_file_field(index, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new(("body-form-file-remove", index))
                                            .ghost()
                                            .label(es_fluent::localize(
                                                "request_tab_body_remove_file_field",
                                                None,
                                            ))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.remove_form_data_file_field(index, cx);
                                            })),
                                    )
                            }))
                            .child(
                                Button::new("body-form-file-add")
                                    .outline()
                                    .label(es_fluent::localize(
                                        "request_tab_body_add_file_field",
                                        None,
                                    ))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.add_form_data_file_field(cx);
                                    })),
                            )
                            .into_any_element(),
                        BodyType::BinaryFile {
                            blob_hash,
                            file_name,
                        } => v_flex()
                            .gap_2()
                            .child(
                                div().text_sm().child(
                                    file_name
                                        .clone()
                                        .filter(|name| !name.trim().is_empty())
                                        .unwrap_or_else(|| {
                                            es_fluent::localize(
                                                "request_tab_body_no_file_selected",
                                                None,
                                            )
                                            .to_string()
                                        }),
                                ),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Button::new("body-binary-pick")
                                            .outline()
                                            .label(if blob_hash.trim().is_empty() {
                                                es_fluent::localize(
                                                    "request_tab_body_pick_file",
                                                    None,
                                                )
                                            } else {
                                                es_fluent::localize(
                                                    "request_tab_body_replace_file",
                                                    None,
                                                )
                                            })
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.pick_binary_body_file(window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("body-binary-clear")
                                            .ghost()
                                            .label(es_fluent::localize(
                                                "request_tab_body_clear_file",
                                                None,
                                            ))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.clear_binary_body_file(cx);
                                            })),
                                    ),
                            )
                            .into_any_element(),
                    })
                    .into_any_element()
            }
            RequestSectionTab::Scripts => v_flex()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(gpui::hsla(0., 0., 0.45, 1.))
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
                        .text_color(gpui::hsla(0., 0., 0.45, 1.))
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
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .text_color(gpui::hsla(0., 0., 0.45, 1.))
                            .child(es_fluent::localize("request_tab_name_label", None)),
                    )
                    .child(dirty_indicator),
            )
            .child(Input::new(&self.name_input).large())
            .child(
                h_flex()
                    .gap_2()
                    .items_end()
                    .child(div().w_40().child(Select::new(&self.method_select)))
                    .child(div().flex_1().child(Input::new(&self.url_input).large()))
                    .child(
                        Button::new("request-send")
                            .primary()
                            .label(es_fluent::localize("request_tab_action_send", None))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.send(cx);
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        Button::new("request-save")
                            .primary()
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
                    .child(
                        Button::new("request-duplicate")
                            .outline()
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
                    .child(
                        Button::new("request-cancel")
                            .outline()
                            .label(es_fluent::localize("request_tab_action_cancel", None))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_send(cx);
                            })),
                    )
                    .child(
                        Button::new("request-reload")
                            .ghost()
                            .label(es_fluent::localize("request_tab_action_reload", None))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reload_baseline(cx);
                            })),
                    ),
            )
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(gpui::hsla(0., 0., 0.45, 1.))
                            .child(format!(
                                "{}: {}",
                                es_fluent::localize("request_tab_latest_run_label", None),
                                latest_run
                            )),
                    )
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
                        cx.listener(|this, _, _, cx| {
                            this.set_active_section(RequestSectionTab::Params, cx);
                        }),
                    ))
                    .child(section_tab_button(
                        "request-tab-auth",
                        es_fluent::localize("request_tab_auth_label", None).to_string(),
                        self.active_section == RequestSectionTab::Auth,
                        cx.listener(|this, _, _, cx| {
                            this.set_active_section(RequestSectionTab::Auth, cx);
                        }),
                    ))
                    .child(section_tab_button(
                        "request-tab-headers",
                        es_fluent::localize("request_tab_headers_label", None).to_string(),
                        self.active_section == RequestSectionTab::Headers,
                        cx.listener(|this, _, _, cx| {
                            this.set_active_section(RequestSectionTab::Headers, cx);
                        }),
                    ))
                    .child(section_tab_button(
                        "request-tab-body",
                        es_fluent::localize("request_tab_body_label", None).to_string(),
                        self.active_section == RequestSectionTab::Body,
                        cx.listener(|this, _, _, cx| {
                            this.set_active_section(RequestSectionTab::Body, cx);
                        }),
                    ))
                    .child(section_tab_button(
                        "request-tab-scripts",
                        es_fluent::localize("request_tab_scripts_label", None).to_string(),
                        self.active_section == RequestSectionTab::Scripts,
                        cx.listener(|this, _, _, cx| {
                            this.set_active_section(RequestSectionTab::Scripts, cx);
                        }),
                    ))
                    .child(section_tab_button(
                        "request-tab-tests",
                        es_fluent::localize("request_tab_tests_label", None).to_string(),
                        self.active_section == RequestSectionTab::Tests,
                        cx.listener(|this, _, _, cx| {
                            this.set_active_section(RequestSectionTab::Tests, cx);
                        }),
                    )),
            )
            .child(
                v_flex()
                    .w_full()
                    .items_stretch()
                    .gap_2()
                    .p_3()
                    .rounded(px(6.))
                    .border_1()
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
            .child(
                v_flex()
                    .gap_2()
                    .p_3()
                    .rounded(px(6.))
                    .border_1()
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

fn section_tab_button(
    id: &'static str,
    label: String,
    active: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    if active {
        Button::new(id).primary().label(label).on_click(on_click)
    } else {
        Button::new(id).ghost().label(label).on_click(on_click)
    }
}

fn response_tab_button(
    id: &'static str,
    label: String,
    active: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    if active {
        Button::new(id).primary().label(label).on_click(on_click)
    } else {
        Button::new(id).ghost().label(label).on_click(on_click)
    }
}

#[derive(Debug, Clone)]
struct CookieRow {
    name: String,
    value_preview: String,
    domain: Option<String>,
    path: Option<String>,
    expires_or_max_age: Option<String>,
    secure: bool,
    http_only: bool,
    same_site: Option<String>,
}

fn status_code_color(status_code: u16) -> Hsla {
    match status_code {
        200..=299 => gpui::hsla(120. / 360., 0.7, 0.35, 1.),
        300..=399 => gpui::hsla(210. / 360., 0.8, 0.45, 1.),
        400..=499 => gpui::hsla(42. / 360., 0.9, 0.4, 1.),
        _ => gpui::hsla(0., 0.78, 0.45, 1.),
    }
}

enum SaveSource {
    InMemory(Vec<u8>),
    Blob(String),
}

fn response_body_preview_text(
    response: &crate::domain::response::ResponseSummary,
    loaded_full_body_text: &Option<String>,
) -> String {
    if let Some(full) = loaded_full_body_text {
        return full.clone();
    }

    match &response.body_ref {
        BodyRef::Empty => String::new(),
        BodyRef::InMemoryPreview { bytes, .. } => {
            render_preview_text(bytes.as_ref(), response.media_type.as_deref())
        }
        BodyRef::DiskBlob {
            preview,
            size_bytes,
            ..
        } => {
            let preview_text = preview
                .as_ref()
                .map(|b| render_preview_text(b.as_ref(), response.media_type.as_deref()))
                .unwrap_or_default();
            let preview_len = preview.as_ref().map(|b| b.len()).unwrap_or(0) as u64;
            if *size_bytes > preview_len {
                format!(
                    "{}\n{}",
                    preview_text,
                    es_fluent::localize("request_tab_response_truncated", None)
                )
            } else {
                preview_text
            }
        }
    }
}

fn looks_like_image(media_type: Option<&str>) -> bool {
    matches!(media_type, Some(value) if value.to_ascii_lowercase().starts_with("image/"))
}

fn is_text_like_media_type(media_type: Option<&str>) -> bool {
    let Some(media_type) = media_type else {
        return true;
    };
    let media_type = media_type.to_ascii_lowercase();
    media_type.starts_with("text/")
        || matches!(
            media_type.as_str(),
            "application/json"
                | "application/xml"
                | "text/xml"
                | "text/html"
                | "application/javascript"
                | "application/x-www-form-urlencoded"
        )
}

fn search_matches(text: &str, query: &str) -> Vec<usize> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let text_lower = text.to_ascii_lowercase();
    let query_lower = query.to_ascii_lowercase();
    let mut matches = Vec::new();
    let mut offset = 0;
    while let Some(found) = text_lower[offset..].find(&query_lower) {
        let absolute = offset + found;
        matches.push(absolute);
        offset = absolute + query_lower.len().max(1);
        if offset >= text.len() {
            break;
        }
    }
    matches
}

fn suggested_file_name(media_type: Option<&str>) -> String {
    let ext = match media_type.map(|v| v.to_ascii_lowercase()) {
        Some(mt) if mt == "application/json" => "json",
        Some(mt) if mt == "application/xml" || mt == "text/xml" => "xml",
        Some(mt) if mt == "text/html" => "html",
        Some(mt) if mt.starts_with("text/") => "txt",
        Some(mt) if mt.starts_with("image/") => "img",
        _ => "bin",
    };
    format!("response.{ext}")
}

fn standard_method_index(method: &str) -> Option<usize> {
    match method.to_ascii_uppercase().as_str() {
        "GET" => Some(0),
        "POST" => Some(1),
        "PUT" => Some(2),
        "PATCH" => Some(3),
        "DELETE" => Some(4),
        "HEAD" => Some(5),
        "OPTIONS" => Some(6),
        _ => None,
    }
}

fn parse_set_cookie_rows(rows: &[crate::domain::response::ResponseHeaderRow]) -> Vec<CookieRow> {
    let mut parsed = Vec::new();
    for row in rows {
        if !row.name.eq_ignore_ascii_case("set-cookie") {
            continue;
        }
        let Ok(cookie) = cookie::Cookie::parse(row.value.clone()) else {
            continue;
        };

        let raw_value = cookie.value().to_string();
        let value_preview = if raw_value.len() > 80 {
            format!("{}…", &raw_value[..80])
        } else {
            raw_value
        };
        let expires_or_max_age = cookie
            .max_age()
            .map(|d| format!("{}s", d.whole_seconds()))
            .or_else(|| {
                cookie.expires().and_then(|v| {
                    v.datetime().and_then(|dt| {
                        time::format_description::parse(
                            "[year]-[month]-[day] [hour]:[minute]:[second] UTC",
                        )
                        .ok()
                        .and_then(|fmt| dt.format(&fmt).ok())
                    })
                })
            });
        let same_site = cookie.same_site().map(|s| format!("{s:?}"));

        parsed.push(CookieRow {
            name: cookie.name().to_string(),
            value_preview,
            domain: cookie.domain().map(ToOwned::to_owned),
            path: cookie.path().map(ToOwned::to_owned),
            expires_or_max_age,
            secure: cookie.secure().unwrap_or(false),
            http_only: cookie.http_only().unwrap_or(false),
            same_site,
        });
    }
    parsed
}

fn timing_row(label: String, value: String) -> gpui::Div {
    h_flex()
        .justify_between()
        .gap_3()
        .child(
            div()
                .text_sm()
                .text_color(gpui::hsla(0., 0., 0.45, 1.))
                .child(label),
        )
        .child(div().text_sm().font_family("monospace").child(value))
}

fn format_unix_ms(value: Option<i64>) -> String {
    let Some(value) = value else {
        return "—".to_string();
    };
    let ts = crate::domain::response::normalize_unix_ms(value);
    let nanos = ts.saturating_mul(1_000_000) as i128;
    let Ok(dt) = time::OffsetDateTime::from_unix_timestamp_nanos(nanos) else {
        return "—".to_string();
    };
    let Ok(fmt) = time::format_description::parse(
        "[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3] UTC",
    ) else {
        return "—".to_string();
    };
    dt.format(&fmt).unwrap_or_else(|_| "—".to_string())
}

fn format_bytes(size: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    if size < 1024 {
        format!("{size} B")
    } else if (size as f64) < MB {
        format!("{:.1} KB", (size as f64) / KB)
    } else {
        format!("{:.2} MB", (size as f64) / MB)
    }
}

fn params_from_url_query(url: &str) -> Vec<KeyValuePair> {
    let raw_query = if let Ok(parsed) = url::Url::parse(url) {
        parsed.query().map(ToOwned::to_owned)
    } else {
        url.split_once('?')
            .map(|(_, q)| q.split_once('#').map(|(qq, _)| qq).unwrap_or(q).to_string())
    };

    raw_query
        .map(|q| {
            url::form_urlencoded::parse(q.as_bytes())
                .map(|(k, v)| KeyValuePair::new(k.to_string(), v.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn url_with_params(base_url: &str, params: &[KeyValuePair]) -> String {
    let enabled: Vec<(String, String)> = params
        .iter()
        .filter(|p| p.enabled && !p.key.trim().is_empty())
        .map(|p| (p.key.clone(), p.value.clone()))
        .collect();

    if let Ok(mut parsed) = url::Url::parse(base_url) {
        if enabled.is_empty() {
            parsed.set_query(None);
        } else {
            parsed
                .query_pairs_mut()
                .clear()
                .extend_pairs(enabled.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        }
        return parsed.to_string();
    }

    let (base, fragment) = match base_url.split_once('#') {
        Some((b, f)) => (b, Some(f)),
        None => (base_url, None),
    };
    let path_only = base.split_once('?').map(|(p, _)| p).unwrap_or(base);
    let query = if enabled.is_empty() {
        String::new()
    } else {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (k, v) in &enabled {
            serializer.append_pair(k, v);
        }
        serializer.finish()
    };

    match (query.is_empty(), fragment) {
        (true, Some(f)) => format!("{path_only}#{f}"),
        (true, None) => path_only.to_string(),
        (false, Some(f)) => format!("{path_only}?{query}#{f}"),
        (false, None) => format!("{path_only}?{query}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthKind {
    None,
    Basic,
    Bearer,
    ApiKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyKind {
    None,
    RawText,
    RawJson,
    UrlEncoded,
    FormData,
    BinaryFile,
}

fn auth_kind_from_label(label: &str) -> AuthKind {
    match label {
        "Basic" => AuthKind::Basic,
        "Bearer" => AuthKind::Bearer,
        "API Key" => AuthKind::ApiKey,
        _ => AuthKind::None,
    }
}

fn auth_type_index(auth: &AuthType) -> usize {
    match auth {
        AuthType::None => 0,
        AuthType::Basic { .. } => 1,
        AuthType::Bearer { .. } => 2,
        AuthType::ApiKey { .. } => 3,
    }
}

fn api_key_location_index(location: ApiKeyLocation) -> usize {
    match location {
        ApiKeyLocation::Header => 0,
        ApiKeyLocation::Query => 1,
    }
}

fn api_key_location_from_index(index: usize) -> ApiKeyLocation {
    match index {
        1 => ApiKeyLocation::Query,
        _ => ApiKeyLocation::Header,
    }
}

fn body_kind_from_label(label: &str) -> BodyKind {
    match label {
        "Raw Text" => BodyKind::RawText,
        "Raw JSON" => BodyKind::RawJson,
        "URL Encoded" => BodyKind::UrlEncoded,
        "Form Data" => BodyKind::FormData,
        "Binary File" => BodyKind::BinaryFile,
        _ => BodyKind::None,
    }
}

fn body_type_index(body: &BodyType) -> usize {
    match body {
        BodyType::None => 0,
        BodyType::RawText { .. } => 1,
        BodyType::RawJson { .. } => 2,
        BodyType::UrlEncoded { .. } => 3,
        BodyType::FormData { .. } => 4,
        BodyType::BinaryFile { .. } => 5,
    }
}

fn latest_run_summary(exec_status: &ExecStatus) -> String {
    match exec_status {
        ExecStatus::Idle => es_fluent::localize("request_tab_latest_run_none", None).to_string(),
        ExecStatus::Sending => es_fluent::localize("request_tab_sending", None).to_string(),
        ExecStatus::Streaming => es_fluent::localize("request_tab_streaming", None).to_string(),
        ExecStatus::Completed { response } => {
            let status = format!("{} {}", response.status_code, response.status_text);
            if let Some(ms) = response.total_ms {
                format!("{status} • {ms} ms")
            } else {
                status
            }
        }
        ExecStatus::Failed { summary, .. } => format!(
            "{}: {}",
            es_fluent::localize("request_tab_response_failed", None),
            summary
        ),
        ExecStatus::Cancelled { partial_size } => match partial_size {
            Some(size) => format!(
                "{} ({size})",
                es_fluent::localize("request_tab_response_cancelled_with_bytes", None)
            ),
            None => es_fluent::localize("request_tab_response_cancelled", None).to_string(),
        },
    }
}

fn classified_error_display(
    classified: Option<&ClassifiedError>,
    summary: &str,
) -> (String, String) {
    match classified {
        Some(ClassifiedError::DnsFailure { host }) => (
            es_fluent::localize("request_tab_error_dns_failure", None).to_string(),
            format!("Could not resolve host: {host}"),
        ),
        Some(ClassifiedError::ConnectionRefused { host, port }) => (
            es_fluent::localize("request_tab_error_connection_refused", None).to_string(),
            format!("Connection refused: {host}:{port}"),
        ),
        Some(ClassifiedError::ConnectionTimeout) => (
            es_fluent::localize("request_tab_error_connection_timeout", None).to_string(),
            summary.to_string(),
        ),
        Some(ClassifiedError::RequestTimeout) => (
            es_fluent::localize("request_tab_error_request_timeout", None).to_string(),
            summary.to_string(),
        ),
        Some(ClassifiedError::TlsError { reason }) => (
            es_fluent::localize("request_tab_error_tls_failure", None).to_string(),
            reason.clone(),
        ),
        Some(ClassifiedError::TransportError { summary, detail }) => {
            (summary.clone(), detail.clone())
        }
        None => (
            es_fluent::localize("request_tab_error_transport_generic", None).to_string(),
            summary.to_string(),
        ),
    }
}

fn render_preview_text(bytes: &[u8], media_type: Option<&str>) -> String {
    let text = String::from_utf8_lossy(bytes).to_string();
    if matches!(media_type, Some(mt) if mt.eq_ignore_ascii_case("application/json")) {
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(value) => serde_json::to_string_pretty(&value).unwrap_or(text),
            Err(_) => text,
        }
    } else if matches!(
        media_type,
        Some(mt)
            if mt.eq_ignore_ascii_case("application/xml")
                || mt.eq_ignore_ascii_case("text/xml")
                || mt.eq_ignore_ascii_case("text/html")
    ) {
        pretty_print_xml_like(&text).unwrap_or(text)
    } else {
        text
    }
}

fn pretty_print_xml_like(input: &str) -> Option<String> {
    use quick_xml::Writer;
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(input);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(event) => {
                if writer.write_event(event).is_err() {
                    return None;
                }
            }
            Err(_) => return None,
        }
        buf.clear();
    }

    String::from_utf8(writer.into_inner()).ok()
}

fn truncate_for_tab_cap(bytes: Vec<u8>, max_bytes: usize) -> (Vec<u8>, bool) {
    if bytes.len() > max_bytes {
        (bytes[..max_bytes].to_vec(), true)
    } else {
        (bytes, false)
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
