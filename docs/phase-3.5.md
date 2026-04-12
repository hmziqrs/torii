# Postman Clone Phase 3.5 Executable Plan

> Derived from `docs/plan.md` Phase 3.5
> Constrained by `docs/state_management.md`
> Builds on `docs/completed/phase-3.md`
> Date: 2026-04-12

## 1. Objective

Make the request-response loop usable day-to-day, not just lifecycle-correct.

Phase 3 made send/save/cancel safe at the state and persistence layer. The response panel is a single monolithic text dump, and every request-editor section is a plain single-line text input with ad-hoc serialization (`key=value\n` for params/headers, `basic username=foo password_ref=bar` for auth). Phase 3.5 replaces those with structured component editors and a proper multi-tab response panel — the minimum bar for a developer to actually debug an API.

## 2. Non-Negotiable Rules Carried Forward

These Phase 3 / V2 rules remain mandatory:

- Body rendering operates on the bounded in-memory preview by default (`RESPONSE_PREVIEW_CAP_BYTES` = 2 MiB)
- Per-tab volatile response footprint respects `PER_TAB_CAP_BYTES` (32 MiB)
- Copy, save-to-file, search, and full-body reload must transparently switch to blob-backed reads when the preview is truncated
- Reopening a request tab restores the latest-run response from persisted history/blob data without re-sending
- Late-response ignore semantics and operation ID checks are not bypassed
- All user-facing strings go through Fluent i18n
- Prefer `gpui-component` composition before building custom components

## 3. Current Repo Starting Point

Phase 3 established the full request lifecycle. Here is what Phase 3.5 replaces or extends:

- `RequestTabView` (1,875 lines) uses 11 `Entity<InputState>` fields — all plain single-line text inputs from `gpui-component`
- Section tabs exist as a `RequestSectionTab` enum (`Params | Auth | Headers | Body | Scripts | Tests`) rendered as a button strip — the tabs work, but their content is unstructured text
- Params/headers use `key=value\n` text serialization parsed by `parse_key_value_pairs()` and serialized by `key_value_pairs_to_text()`
- Auth uses a custom DSL (`basic username=foo password_ref=bar`, `bearer token_ref=xyz`, `api_key key=... value_ref=... location=header`)
- Body editor only handles `RawText`/`RawJson` content; `UrlEncoded`, `FormData`, `BinaryFile` render as empty inputs
- Response panel is monolithic: shows status code + body text dump for `Completed`, or a single status line for other exec states
- `ResponseSummary.headers_json` is stored and restored from history but **never displayed** in the UI
- No response tab system (no Headers/Cookies/Timing tabs)
- No `cookie` or XML parsing crate in `Cargo.toml`
- `gpui-component` provides `Table`/`DataTable` (static composable and virtualized delegate-based) but no dedicated key-value editor — one must be built, composing `Table` + inline `Input` cells
- Method is a plain text input, not a dropdown
- No `Cmd+W` close-tab shortcut exists
- Registered request-tab shortcuts: `Cmd+S` save, `Cmd+Enter` send, `Esc` cancel
- `ReentrancyGuard` protects URL↔params bidirectional sync in render and subscriptions

## 4. Phase 3.5 Deliverables

Phase 3.5 is complete only when all of the following exist:

- tabbed response panel with four tabs: Body, Headers, Cookies, Timing
- response metadata bar showing status code (color-coded), status text, formatted size, total time
- classified error display separating preflight failures from transport failures with human-readable categories
- response body improvements: copy-to-clipboard (text types only), save-to-file (streaming for DiskBlob), image preview (from preview bytes only), XML/HTML pretty-print, in-body search
- key-value editor component for params and headers (add/remove/enable/disable rows)
- auth type selector dropdown with structured per-type credential fields
- body type selector dropdown with type-appropriate editors
- method selector dropdown replacing the text input
- file-backed body UX for form-data file fields and binary body (pick/replace/clear/missing-state)
- expanded keyboard shortcuts: close tab, new request, duplicate, next/prev tab, focus URL bar, toggle sidebar
- all new Fluent i18n keys in both `en` and `zh-CN` locale files

## 5. Scope Boundary

Included now:

- response presentation: tabs, metadata bar, error classification, copy/save/search/image/pretty-print
- request editor: structured component editors replacing text inputs inside existing section tabs
- keyboard shortcut expansion for common tab and editor operations

Explicitly deferred:

- global history panel and advanced history filtering (Phase 5)
- environment-variable resolution and `{{variable}}` substitution UI (Phase 4)
- tree-wide CRUD affordances, drag/drop, and context menus (Phase 4)
- scripts/tests execution engine; this phase keeps them as plain text inputs (execution is Phase 5+)
- code editor component for body/scripts (a monospace `Input` is sufficient for 3.5; a real code editor with syntax highlighting is a later polish item)
- OAuth 2.0, AWS Signature, and other advanced auth flows
- response diff/compare across history entries (Phase 5)

## 6. New Dependencies

```toml
cookie = "0.18"            # RFC 6265 Set-Cookie parsing
quick-xml = "0.37"         # XML pretty-print for response bodies
```

No new migrations. Phase 3.5 is purely a UI/presentation phase that reads the existing domain model and persistence layer without schema changes.

## 7. Proposed Module Layout

```text
src/
  views/
    item_tabs/
      request_tab.rs              # refactored: delegates to sub-views
      request_tab/
        mod.rs                    # re-exports, shared types
        response_panel.rs         # tabbed response panel (Body, Headers, Cookies, Timing)
        response_metadata_bar.rs  # status code + size + time bar
        error_display.rs          # classified preflight/transport error rendering
        key_value_editor.rs       # reusable key-value row editor component
        auth_editor.rs            # auth type dropdown + per-type fields
        body_editor.rs            # body type dropdown + per-type editors
        body_search.rs            # in-body search bar and state
  services/
    error_classifier.rs           # reqwest error → classified failure categories
tests/
  response_panel_display.rs       # response tab rendering, cookie parsing, header display
  key_value_editor_roundtrip.rs   # KV editor ↔ domain model sync
  error_classifier.rs             # error classification coverage
```

Notes:

- `request_tab.rs` grows unwieldy at 1,875 lines — Phase 3.5 splits the response panel, error display, and editor sections into sub-modules under `request_tab/`
- the key-value editor is reusable across params, headers, urlencoded body entries, form-data text fields, and cookie display
- the error classifier lives in `services/` because it transforms `reqwest`/`anyhow` errors into domain-level failure categories used by the view layer

## 8. Execution Slices

## Slice 1: Response Panel Tabs and Metadata Bar

Purpose: replace the monolithic response dump with a tabbed panel and a status bar.

Tasks:

- introduce `ResponseTab { Body, Headers, Cookies, Timing }` enum and active-tab state on `RequestTabView`
- extract the response panel into `response_panel.rs` with a tab strip and tab content dispatcher
- implement the response metadata bar in `response_metadata_bar.rs`:
  - status code with color coding: 2xx green, 3xx blue, 4xx yellow, 5xx red
  - status text
  - formatted response size (B / KB / MB) derived from `BodyRef::size_bytes()`
  - total time in ms
- **Body tab**: preserve the existing preview-based rendering (monospace text, JSON pretty-print for `application/json`); add XML/HTML pretty-print using `quick-xml`; preserve the "Load Full Body" button for `DiskBlob` responses
- **Headers tab**: parse `ResponseSummary.headers_json` into a two-column table; preserve repeated headers as separate rows (do not merge into a lossy map); use `gpui-component` `Table` for the layout
- **Cookies tab**: parse `Set-Cookie` headers from `headers_json` using the `cookie` crate; render a table with columns: name, value (truncated preview), domain, path, expires/max-age, secure, httpOnly, sameSite; multiple cookies with the same name appear as distinct rows; if no `Set-Cookie` headers exist, show an empty state
- **Timing tab**: render total time, TTFB, request dispatched timestamp, request completed timestamp; show DNS, TCP connect, TLS handshake rows as `—` placeholders (always present for layout stability); timestamps formatted with `time` crate
- add Fluent keys for all tab labels, empty states, and timing row labels
- metadata bar and tabs render for `ExecStatus::Completed` only; other exec states retain their existing display (Idle empty state, Sending/Streaming progress, Failed/Cancelled messages)

Definition of done:

- response panel has 4 switchable tabs with correct content for each
- response metadata bar shows color-coded status, size, and time at a glance
- headers tab preserves repeated headers without lossy collapsing
- cookies tab parses RFC 6265 attributes correctly, including multiple same-name cookies
- timing tab is stable regardless of which timing fields the transport populates
- no Phase 3 memory/persistence rules are violated

## Slice 2: Classified Error Display

Purpose: replace generic error strings with human-readable failure categories.

Tasks:

- introduce `error_classifier.rs` in `src/services/` that converts `reqwest` and `anyhow` error chains into `ClassifiedError`:
  ```
  enum ClassifiedError {
      DnsFailure { host: String },
      ConnectionRefused { host: String, port: u16 },
      ConnectionTimeout,
      TlsError { reason: String },
      RequestTimeout,
      Cancelled,
      TransportError { summary: String, detail: String },
  }
  ```
- classify errors by inspecting `reqwest::Error` methods: `.is_timeout()`, `.is_connect()`, `.is_request()`, and the inner error chain (downcast to `std::io::Error` for `ConnectionRefused`, `hyper` errors for DNS, `rustls`/native-tls errors for TLS)
- extract `error_display.rs` into the request tab sub-module that renders `ClassifiedError` with:
  - an icon or color badge per category
  - a primary human-readable message (e.g., "Could not resolve host: api.example.com")
  - an expandable detail section for the full error chain
- preflight errors (`PreflightFailed` from `ExecOutcome`) are rendered separately with their own category labels (malformed URL, missing body file, secret resolution failure) — they do not go through the transport classifier
- update `RequestExecutionService` to return `ClassifiedError` for transport failures instead of raw `String`; the `ExecOutcome::Failed` variant changes from `Failed(String)` to `Failed(ClassifiedError)` — update `ExecStatus::Failed` to carry `ClassifiedError` (or a serializable form of it)
- the existing `error: String` serialization path for history persistence continues to store a plain string summary; the classified detail is view-local only

Definition of done:

- DNS, connection refused, timeout, TLS, and generic transport errors display distinct human-readable messages
- preflight failures display with their own labels, not as transport errors
- the full error chain is available via an expandable detail section
- history persistence continues to store a string summary (no schema change)

## Slice 3: Response Body Actions (Copy, Save, Image, Search)

Purpose: make response bodies actionable beyond read-only preview.

Tasks:

- **Copy to clipboard**:
  - enabled for text-like media types (`text/*`, `application/json`, `application/xml`, `application/javascript`, etc.)
  - copies from in-memory preview if available; if the body is `DiskBlob` and the full body has been loaded, copies from the loaded text
  - disabled with an explanatory tooltip for binary and image media types
  - add a "Copy" button to the Body tab header bar
- **Save to file**:
  - opens a native save dialog via `window.prompt_for_new_path()`
  - for `InMemoryPreview` bodies: writes from the in-memory bytes directly
  - for `DiskBlob` bodies: reads from the blob store using the reader/streaming path (`BlobStore::open_reader` or `read_all` only for bodies within the preview cap); **never** allocates a second full-body copy in RAM for large bodies — use `std::io::copy` from the blob file to the destination file
  - add a "Save" button to the Body tab header bar
- **Image preview**:
  - for `image/*` media types, render the preview bytes as an image instead of text
  - decode from `BodyRef` preview bytes only (either `InMemoryPreview.bytes` or `DiskBlob.preview`), never from the full blob by default
  - images truncated at the preview cap show a "Preview truncated — load full image" notice
  - after "Load Full Body" is triggered, the full image is decoded and rendered
  - use GPUI's `ImageSource::from(SharedUri)` or equivalent for rendering; support JPEG, PNG, GIF, WebP at minimum
  - if decode fails (corrupt or unsupported format), fall back to showing the raw bytes as hex dump with a format error message
- **XML/HTML pretty-print**:
  - when `media_type` matches `application/xml`, `text/xml`, or `text/html`, parse with `quick-xml` and re-indent
  - if parsing fails, fall back to raw text display with no error — the original text is always the safe fallback
- **In-body search** (`body_search.rs`):
  - case-insensitive substring search
  - search state is view-local (not persisted, not on `RequestEditorState`)
  - search bar appears at the top of the Body tab when activated (Cmd+F / Ctrl+F while Body tab is focused)
  - highlights all matches in the preview text; arrow buttons or Enter/Shift+Enter cycle through matches
  - if the preview is truncated (`DiskBlob` without full body loaded), show a "Load full body to search all content" prompt when the user searches
  - search operates on the rendered text (after pretty-print), not the raw bytes
- add Fluent keys for all button labels, tooltips, prompts, and error messages

Definition of done:

- copy works for text types and is disabled with tooltip for binary/image
- save-to-file streams for large bodies without a full in-memory allocation
- image preview renders from preview bytes; truncated images show a notice instead of corruption
- XML/HTML pretty-print works alongside existing JSON pretty-print
- search finds and highlights matches in the body; prompts for full-body load when truncated
- all actions respect Phase 3 preview cap and blob-backed rules

## Slice 4: Key-Value Editor Component

Purpose: build the reusable structured editor that replaces text-based params/headers editing.

Tasks:

- build `key_value_editor.rs` as a reusable component composing `gpui-component` `Table` with inline `Input` cells:
  - columns: enabled (checkbox), key (text input), value (text input), actions (delete button)
  - "Add Row" button at the bottom
  - each row corresponds to a `KeyValuePair { key, value, enabled }` from the domain model
  - the component takes a `Vec<KeyValuePair>` as input and emits change events with the updated vec
  - rows can be added, removed, and individually enabled/disabled
  - empty trailing rows are auto-trimmed on blur (no accumulation of blank rows)
  - keyboard: Tab moves between cells; Enter adds a new row when in the last row's value cell
- replace `params_input: Entity<InputState>` with the key-value editor:
  - the editor must preserve the Phase 3 URL↔params bidirectional sync contract
  - when the URL bar changes, `params_from_url_query()` produces a `Vec<KeyValuePair>` that feeds the editor
  - when params editor rows change, `url_with_params()` rewrites the URL query string
  - the `ReentrancyGuard` must still prevent infinite loops between URL and params
- replace `headers_input: Entity<InputState>` with the key-value editor
- remove `parse_key_value_pairs()` and `key_value_pairs_to_text()` — the text serialization path is no longer needed
- add Fluent keys for column headers, empty state, and add-row button

Definition of done:

- params and headers use a proper table with add/remove/enable/disable per row
- URL↔params sync is preserved (URL edits update the table, table edits update the URL)
- the text-based `key=value\n` input is fully removed
- the key-value editor is a standalone component reusable by other sections (urlencoded body, form-data text fields)

## Slice 5: Auth Type Selector and Structured Fields

Purpose: replace the text-protocol auth input with a dropdown and per-type structured fields.

Tasks:

- build `auth_editor.rs` with:
  - a dropdown (`gpui-component` `Dropdown` or `Popover` + `List`) for auth type selection: None, Basic, Bearer, API Key
  - per-type field panels:
    - **None**: empty / explanatory text
    - **Basic**: username (text input) + password (password input that reads/writes through `SecretManager`)
    - **Bearer**: token (password input via `SecretManager`)
    - **API Key**: key name (text input), value (password input via `SecretManager`), location dropdown (Header / Query)
  - password fields show/hide toggle for secret values
  - the component reads from `AuthType` and emits change events with the updated `AuthType`
  - secret ref binding continues to work exactly as Phase 3 established: the editor stores/retrieves secret refs through the existing `normalize_auth_secret_ownership_for_save` and `rebind_secret_ref` paths
- replace `auth_input: Entity<InputState>` with `auth_editor`
- remove `auth_to_text()` and `parse_auth_text()` — the text DSL is no longer needed
- add Fluent keys for field labels, dropdown options, show/hide toggle, and placeholder text

Definition of done:

- auth type is selected from a dropdown, not typed as text
- each auth type shows structured fields appropriate to that type
- secret values are stored/retrieved through the existing `SecretManager` flow
- the text DSL (`basic username=... password_ref=...`) is fully removed
- switching auth type preserves fields when switching back (e.g., changing from Basic to Bearer and back keeps the username)

## Slice 6: Body Type Selector and Structured Editors

Purpose: replace the text-only body input with type-appropriate editors.

Tasks:

- build `body_editor.rs` with:
  - a dropdown for body type selection: None, Raw Text, Raw JSON, URL Encoded, Form Data, Binary File
  - per-type editor panels:
    - **None**: empty / explanatory text
    - **Raw Text**: single text input (existing behavior, keep as-is)
    - **Raw JSON**: single text input (existing behavior, keep as-is); future phases may add a code editor
    - **URL Encoded**: reuse the key-value editor component from Slice 4 — reads/writes `BodyType::UrlEncoded { entries }`
    - **Form Data**: split into two sections:
      - text fields: reuse key-value editor
      - file fields: per-row display with file name, size, pick/replace/clear buttons (see file UX below)
    - **Binary File**: single file display with pick/replace/clear buttons
  - **File-backed body UX** (for form-data file fields and binary):
    - "Pick File" opens a native file dialog via `window.prompt_for_paths()`
    - selected file is read and written to the blob store; the resulting `blob_hash` and `file_name` are stored on the domain model (`FileField.blob_hash`, `BinaryFile.blob_hash`)
    - "Replace File" re-opens the file dialog and overwrites the blob ref
    - "Clear File" removes the blob ref
    - if the blob is missing or unreadable at send time, the preflight check surfaces a recoverable error with the file name — this already works via Phase 3's preflight path
    - file size cap: files over 100 MB trigger a confirmation dialog before reading; the dialog shows the file size and warns about memory use
    - files are read into the blob store, not held in RAM as `Bytes`; at send time, `build_request_body()` reads from the blob store
  - the component reads from `BodyType` and emits change events with the updated `BodyType`
- replace `body_input: Entity<InputState>` with `body_editor`
- remove `body_editor_value()` — the text extraction helper is no longer needed
- add a **method selector dropdown** to replace the `method_input` text field: options are GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS, with a freeform fallback for custom methods
- add Fluent keys for all type labels, field labels, file action buttons, empty states, and the 100 MB confirmation dialog

Definition of done:

- body type is selected from a dropdown
- each body type renders an appropriate editor (text, KV table, file picker)
- file-backed bodies store blob refs through the existing model
- files over 100 MB trigger a confirmation dialog
- method is selected from a dropdown
- the raw text body input is removed for types that have structured editors

## Slice 7: Keyboard Shortcuts

Purpose: add the remaining Postman-standard keyboard shortcuts.

Tasks:

- define new actions:
  - `CloseTab` — close the active tab; triggers the dirty confirm dialog when the request has unsaved changes
  - `NewRequest` — open a new draft request tab in the collection currently selected in the sidebar; show a toast and no-op if no collection is selected
  - `DuplicateRequest` — duplicate the active request tab (existing `duplicate()` flow)
  - `NextTab` / `PrevTab` — cycle through tabs in the tab manager
  - `FocusUrlBar` — focus the URL input in the active request tab
  - `ToggleSidebar` — toggle `WindowLayoutState.sidebar_visible`
- register bindings in `src/app.rs`:
  - `Cmd+W` / `Ctrl+W` → `CloseTab` (global, scoped to window)
  - `Cmd+N` / `Ctrl+N` → `NewRequest` (global)
  - `Cmd+D` / `Ctrl+D` → `DuplicateRequest` (scoped to `RequestTabView`)
  - `Cmd+Shift+]` / `Ctrl+Tab` → `NextTab` (global)
  - `Cmd+Shift+[` / `Ctrl+Shift+Tab` → `PrevTab` (global)
  - `Cmd+L` / `Ctrl+L` → `FocusUrlBar` (scoped to `RequestTabView`)
  - `Cmd+B` / `Ctrl+B` → `ToggleSidebar` (global)
  - `Cmd+F` / `Ctrl+F` → `ToggleBodySearch` (scoped to response Body tab)
- ensure all shortcuts are scoped correctly: text inputs must not swallow `Cmd+W` or `Cmd+N`; modal dialogs must not propagate shortcuts to the background
- add Fluent keys for toast messages (e.g., "No collection selected") and any shortcut hint labels
- shortcuts must be platform-correct: `Cmd` on macOS, `Ctrl` on Windows/Linux

Definition of done:

- all listed shortcuts are registered, functional, and platform-correct
- close-tab triggers the dirty confirm dialog for unsaved requests
- new-request requires a selected collection and shows a toast otherwise
- shortcuts do not fire from inside text inputs or modal dialogs where they conflict
- next/prev tab cycles through the tab manager's tab list

## Slice 8: File Decomposition and Cleanup

Purpose: split the oversized `request_tab.rs` into manageable sub-modules.

Tasks:

- create `src/views/item_tabs/request_tab/` directory with `mod.rs`
- move the response panel (Slice 1) into `response_panel.rs`
- move the metadata bar into `response_metadata_bar.rs`
- move classified error display (Slice 2) into `error_display.rs`
- move the key-value editor (Slice 4) into `key_value_editor.rs`
- move the auth editor (Slice 5) into `auth_editor.rs`
- move the body editor (Slice 6) into `body_editor.rs`
- move body search (Slice 3) into `body_search.rs`
- keep the top-level `RequestTabView` struct, `Render` impl, action handlers, save/send/cancel/duplicate flows, and draft sync logic in the main `mod.rs`
- update all imports and re-exports
- verify no public API changes leak beyond the `request_tab` module boundary

Definition of done:

- `request_tab.rs` is split into sub-modules with clear responsibilities
- no file exceeds ~500 lines
- the module compiles cleanly with no changes to external callers (`root.rs`, `app.rs`)

## 9. Recommended Slice Ordering

```
Slice 8 (file decomposition) — do first to avoid merge conflicts in a 1,875-line file
  ↓
Slice 4 (key-value editor) — foundational component needed by Slices 5 and 6
  ↓
Slice 5 (auth editor) + Slice 6 (body editor) — can run in parallel, both depend on Slice 4
  ↓
Slice 1 (response tabs + metadata bar) — independent of request editor slices
  ↓
Slice 2 (classified errors) — extends the response panel
  ↓
Slice 3 (body actions: copy, save, image, search) — extends the Body tab from Slice 1
  ↓
Slice 7 (keyboard shortcuts) — last, after all views are wired
```

Note: Slice 1 (response tabs) and Slices 4-6 (editor components) are independent and can be interleaved if preferred. The ordering above minimizes file conflicts.

## 10. Validation Gates

Phase 3.5 should not be considered complete without the following coverage.

Unit tests:

- key-value editor: add/remove/enable/disable rows, empty-row trimming, domain model round-trip
- URL↔params sync through the key-value editor (replacing the text-based round-trip test)
- cookie parsing: multiple cookies, duplicate names, all RFC 6265 attributes, malformed `Set-Cookie` headers
- XML pretty-print: well-formed XML, malformed XML (graceful fallback to raw text), empty input
- error classifier: DNS failure, connection refused, timeout, TLS error, generic transport, cancelled
- response metadata bar: size formatting (bytes, KB, MB), color coding for each status range

Integration tests:

- save-to-file for `DiskBlob` body: verify the destination file matches the blob content without a second full-body RAM allocation (assert peak memory delta stays bounded)
- image preview: verify a 3 MiB image body with 2 MiB preview cap renders the preview without loading the full blob; verify "Load Full Image" loads and renders the complete image
- search across preview boundary: verify search within preview works; verify search prompts for full-body load when preview is truncated; verify search after full-body load finds matches beyond the original preview
- close-tab shortcut with dirty request: verify the confirm dialog appears
- new-request shortcut with no collection selected: verify toast appears and no tab is created
- auth editor round-trip: select each auth type, fill fields, save, reopen — verify fields persist correctly through `SecretManager`
- body type switch: select each body type, edit content, save, reopen — verify content persists correctly
- file-backed body: pick a file, save, reopen — verify the blob ref is intact; pick a file over 100 MB — verify confirmation dialog appears

Security tests:

- auth editor password fields never expose secret values in the DOM / render tree — only the `SecretManager` ref is stored
- the copy-to-clipboard action does not include auth headers from the response headers tab

## 11. Fluent Key Naming Convention

Follow the Phase 3 prefix pattern: `request_tab_` + descriptive snake_case suffix.

New key groups for Phase 3.5:

```
# Response tabs
response_tab_body
response_tab_headers
response_tab_cookies
response_tab_timing

# Response metadata bar
response_meta_size_bytes
response_meta_size_kb
response_meta_size_mb

# Response body actions
response_action_copy
response_action_copy_disabled_tooltip
response_action_save
response_action_save_failed
response_body_search_placeholder
response_body_search_load_full
response_body_image_truncated

# Classified errors
error_dns_failure
error_connection_refused
error_connection_timeout
error_tls_failure
error_request_timeout
error_transport_generic
error_detail_expand
error_detail_collapse

# Timing tab
timing_total
timing_ttfb
timing_dispatched
timing_completed
timing_dns
timing_tcp
timing_tls
timing_placeholder

# Cookies tab
cookies_empty
cookies_col_name
cookies_col_value
cookies_col_domain
cookies_col_path
cookies_col_expires
cookies_col_secure
cookies_col_httponly
cookies_col_samesite

# Key-value editor
kv_col_enabled
kv_col_key
kv_col_value
kv_add_row

# Auth editor
auth_select_type
auth_basic_username
auth_basic_password
auth_bearer_token
auth_api_key_name
auth_api_key_value
auth_api_key_location
auth_api_key_location_header
auth_api_key_location_query
auth_secret_show
auth_secret_hide

# Body editor
body_select_type
body_file_pick
body_file_replace
body_file_clear
body_file_missing
body_file_size_warning_title
body_file_size_warning_body

# Method selector
method_custom

# Keyboard shortcuts
shortcut_no_collection
shortcut_close_tab
```

## 12. Exit Criteria Mapping Back to `docs/plan.md`

The Phase 3.5 goals from the main plan are satisfied only when:

- response panel shows body, headers, cookies, and timing in separate tabs without violating preview-memory caps
- latest-run response data reopens from persisted history/blob storage without requiring a resend
- preflight validation failures and transport failures display distinct, human-readable states
- response metadata bar shows status code with color coding, size, and time at a glance
- copy is enabled for text media types and disabled with a tooltip for binary/image types
- save-to-file streams from the blob store for `DiskBlob` bodies without a full in-memory load
- image preview decodes from preview bytes only; full-blob image decode does not occur by default
- search works within the active preview and prompts for full-body load when the preview is truncated
- header and cookie views preserve repeated headers and all RFC 6265 cookie attributes without lossy flattening; cookies with the same name appear as distinct rows
- timing panel always shows DNS/TCP/TLS rows as `—` placeholders rather than omitting them
- section tab content is replaced with structured component editors; text-input fallbacks are removed
- URL↔params bidirectional sync is preserved through the new key-value params editor
- auth type selector populates structured fields from the existing `AuthType` domain model without data migration
- file-backed body flows cover pick, replace, clear, and missing-file states before send
- files over 100 MB trigger a confirmation prompt; large file bodies are not fully loaded into RAM
- all new shortcuts are documented, platform-correct, scoped correctly, and functional
- close-tab shortcut respects the close-while-dirty confirm dialog
- new-request shortcut requires a selected collection and shows a toast when none is selected
- all new user-facing copy and error messages are Fluent-based

That is the minimum bar before moving on to Phase 4 tree CRUD and environment resolution work.
