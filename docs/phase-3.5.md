# Postman Clone Phase 3.5 Executable Plan

> Derived from `docs/plan.md` Phase 3.5
> Constrained by `docs/state_management.md`
> Builds on `docs/completed/phase-3.md`
> Date: 2026-04-12

## 1. Objective

Make the request-response loop usable day-to-day, not just lifecycle-correct.

Phase 3 made send/save/cancel safe at the state and persistence layer. The response panel is a single monolithic text dump, and every request-editor section is a plain single-line text input with ad-hoc serialization (`key=value\n` for params/headers, `basic username=foo password_ref=bar` for auth). Phase 3.5 replaces those with structured component editors and a proper multi-tab response panel — the minimum bar for a developer to actually debug an API.

## 1.1 Current Progress Snapshot (as of 2026-04-12)

Status legend: `done` / `partial` / `pending`

- Slice 1 (Response tabs + metadata): `partial`
  - done: Body/Headers/Cookies/Timing tabs, metadata bar, XML/HTML pretty-print, timing fields on `ResponseSummary`, lossless header row persistence + legacy fallback parser, cookie parsing, timing placeholders.
  - pending: extract to `request_tab/response_panel.rs` + `response_metadata_bar.rs`; Headers/Cookies currently render as structured text rows, not `gpui-component::Table`.
- Slice 2 (Classified error display): `partial`
  - done: `services/error_classifier.rs`, `ExecOutcome::Failed { summary, classified }`, `ExecStatus::Failed { summary, classified }`, fallback for restored history failures.
  - pending: dedicated `error_display.rs` submodule; expandable full-chain details UI; deeper DNS/TLS classification coverage.
- Slice 3 (Body actions): `partial`
  - done: copy (text-like types), save-to-file (blob streaming path), body-search toggle + match counting, XML/HTML fallback behavior.
  - pending: image preview render path from preview bytes, full-content blob scan search with snippets/highlights/navigation, explicit copy-disabled tooltip UX.
- Slice 4 (Key-value editor): `partial`
  - done: params/headers moved to row-based structured editor (add/remove/enable/disable), URL↔params sync preserved with disabled-row retention.
  - partial: URL-encoded body and form-data text fields now reuse the same row model in `request_tab.rs`.
  - pending: extract standalone reusable `key_value_editor.rs` component + table column header/empty-state polish.
- Slice 5 (Auth structured editor): `partial`
  - done: replaced text DSL input with auth type dropdown + per-type structured panels (None/Basic/Bearer/API Key), including API key location dropdown.
  - done: removed `auth_to_text()` / `parse_auth_text()` path.
  - pending: integrate secret value read/write UX via `SecretManager` (instead of direct secret-ref fields), show/hide secret toggles, extract `auth_editor.rs`.
- Slice 6 (Body structured editor + streamed request payload): `partial`
  - done: method dropdown replacing freeform method-only editing.
  - done: body type dropdown + per-type panels for None/Raw Text/Raw JSON/URL Encoded; removed `body_input` and `body_editor_value()`.
  - done: Raw JSON now uses `InputState::code_editor` (line numbers + searchable), and Raw Text/Scripts/Tests use multiline editor-mode input with fixed editor heights.
  - done: Form Data file fields + Binary file now support pick/replace/clear, including >100 MB confirmation prompt.
  - done: added `services/request_body_payload.rs` and switched request execution transport to stream-capable payloads for binary/form-data (no single giant `Bytes` allocation path).
  - pending: richer form-data file row UX (editable field key + stronger "no file selected" domain representation), extract `body_editor.rs`.
- Slice 7 (Keyboard shortcuts): `done`
  - implemented: close tab, new request, duplicate request, next/prev tab, focus URL bar, toggle sidebar, toggle body search.
- Slice 8 (File decomposition): `pending`
  - `request_tab.rs` is still monolithic.

Outstanding validation gates:

- Missing targeted/new tests for key-value editor, body full-content search, image preview behavior, and streamed large outbound request-body paths.

## 2. Non-Negotiable Rules Carried Forward

These Phase 3 / V2 rules remain mandatory:

- Body rendering operates on the bounded in-memory preview by default (`RESPONSE_PREVIEW_CAP_BYTES` = 2 MiB)
- Per-tab volatile response footprint respects `PER_TAB_CAP_BYTES` (32 MiB)
- Copy, save-to-file, search, and full-body reload must transparently switch to blob-backed reads when the preview is truncated
- No feature may require loading an entire large request/response payload into hot UI state just to function; blob-backed and chunked IO paths are required for large bodies
- Reopening a request tab restores the latest-run response from persisted history/blob data without re-sending
- Late-response ignore semantics and operation ID checks are not bypassed
- Repeated response headers must be preserved losslessly for all newly written history entries; lossy header maps are not acceptable for Headers/Cookies views
- Async UI flows (file dialogs, blob reads, clipboard/save actions, full-body load, search-all-content) must use explicit task ownership and treat dropped app/window/entity targets as normal non-panicking outcomes
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
- `ResponseSummary.headers_json` is stored and restored from history but **never displayed** in the UI; current serialization is lossy because repeated headers are merged into a single JSON object entry
- No response tab system (no Headers/Cookies/Timing tabs)
- No `cookie` or XML parsing crate in `Cargo.toml`
- `gpui-component` provides `Table`/`DataTable` (static composable and virtualized delegate-based) but no dedicated key-value editor — one must be built, composing `Table` + inline `Input` cells
- Method is a plain text input, not a dropdown
- No `Cmd+W` close-tab shortcut exists
- Registered request-tab shortcuts: `Cmd+S` save, `Cmd+Enter` send, `Esc` cancel
- `ReentrancyGuard` protects URL↔params bidirectional sync in render and subscriptions
- Request-body transport still builds `Option<Bytes>` for sends, so large binary/form-data bodies are not yet streamed from blob storage
- File picker/save picker APIs are asynchronous GPUI app prompts, not synchronous `Window` helpers
- `ResponseSummary` only carries size/type/status/`total_ms`/`ttfb_ms`; it does not yet carry wall-clock dispatch/first-byte/completed timestamps for the Timing tab

## 4. Phase 3.5 Deliverables

Phase 3.5 is complete only when all of the following exist:

- tabbed response panel with four tabs: Body, Headers, Cookies, Timing
- response metadata bar showing status code (color-coded), status text, formatted size, total time
- classified error display separating preflight failures from transport failures with human-readable categories
- response body improvements: copy-to-clipboard (text types only), save-to-file (streaming for DiskBlob), image preview (from preview bytes only), XML/HTML pretty-print, in-body search
- lossless response-header persistence/parse path that preserves repeated headers and `Set-Cookie` rows across history restore
- key-value editor component for params and headers (add/remove/enable/disable rows)
- auth type selector dropdown with structured per-type credential fields
- body type selector dropdown with type-appropriate editors
- method selector dropdown replacing the text input
- file-backed body UX for form-data file fields and binary body (pick/replace/clear/missing-state)
- request-body blob send path that streams large file-backed bodies instead of materializing a full `Bytes` buffer
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

No new database columns are required. `response_headers_json` can remain a text column if Phase 3.5 upgrades it from the current lossy object-map JSON into an ordered row-array JSON shape for new writes while keeping a backwards-compatible reader for legacy entries.

Phase 3.5 is primarily a UI/presentation phase, but it also includes narrowly scoped model/service changes needed to satisfy the V2 standards:

- add lossless response-header serialization/parsing for history restore
- add wall-clock timing fields required by the Timing tab
- add streamed request-body construction for large binary/form-data sends
- add optional/missing-state representation for file-backed body editors where the current model cannot represent "picked type, no file selected yet"

## 7. Proposed Module Layout

```text
src/
  views/
    item_tabs/
      request_tab.rs              # module entry point; retains RequestTabView/actions and delegates to sub-views
      request_tab/
        response_panel.rs         # tabbed response panel (Body, Headers, Cookies, Timing)
        response_metadata_bar.rs  # status code + size + time bar
        error_display.rs          # classified preflight/transport error rendering
        key_value_editor.rs       # reusable key-value row editor component
        auth_editor.rs            # auth type dropdown + per-type fields
        body_editor.rs            # body type dropdown + per-type editors
        body_search.rs            # in-body search bar and state
  services/
    error_classifier.rs           # reqwest error → classified failure categories
    request_body_payload.rs       # streamed request-body construction for blob-backed sends
tests/
  response_panel_display.rs       # response tab rendering, cookie parsing, header display
  response_headers_roundtrip.rs   # lossless repeated-header persistence + legacy reader fallback
  key_value_editor_roundtrip.rs   # KV editor ↔ domain model sync
  request_body_payload.rs         # large binary/form-data request-body streaming coverage
  error_classifier.rs             # error classification coverage
```

Notes:

- `request_tab.rs` grows unwieldy at 1,875 lines — Phase 3.5 splits the response panel, error display, and editor sections into sub-modules under `request_tab/`
- the key-value editor is reusable across params, headers, urlencoded body entries, form-data text fields, and cookie display
- the error classifier lives in `services/` because it transforms `reqwest`/`anyhow` errors into domain-level failure categories used by the view layer
- `request_body_payload.rs` owns large request-body assembly so the body editor UI does not also own streaming/blob IO concerns

Cross-cutting implementation rules:

- File chooser and save dialog flows use GPUI app prompt receivers (`cx.prompt_for_paths()`, `cx.prompt_for_new_path()`) and complete asynchronously
- Any async task started for body load/save/search/file-pick owns its lifecycle explicitly: either store the task handle on the owning entity or detach intentionally with weak-entity update paths
- Dropped window/app/entity targets are expected shutdown paths, not panic conditions
- Follow-up UI updates that touch URL↔params sync or response-body actions must not re-enter the same entity mutably during an active update path

## 8. Execution Slices

## Slice 1: Response Panel Tabs and Metadata Bar

Purpose: replace the monolithic response dump with a tabbed panel and a status bar.

Tasks:

- introduce `ResponseTab { Body, Headers, Cookies, Timing }` enum and active-tab state on `RequestTabView`
- extract the response panel into `response_panel.rs` with a tab strip and tab content dispatcher
  - `response_panel.rs` owns rendering for all `ExecStatus` branches after the refactor
  - `ExecStatus::Completed` renders the metadata bar + tab strip + active tab content
  - `ExecStatus::Idle | Sending | Streaming | Failed | Cancelled` continue to render their current single-panel states from inside `response_panel.rs`, not from ad hoc branches left in `request_tab.rs`
- implement the response metadata bar in `response_metadata_bar.rs`:
  - status code with color coding: 2xx green, 3xx blue, 4xx yellow, 5xx red
  - status text
  - formatted response size (B / KB / MB) derived from `BodyRef::size_bytes()`
  - total time in ms
- extend the response snapshot contract so the Timing tab can render real timestamps:
  - add exact fields to `ResponseSummary`: `dispatched_at_unix_ms: Option<i64>`, `first_byte_at_unix_ms: Option<i64>`, and `completed_at_unix_ms: Option<i64>`
  - keep `total_ms: Option<u64>` and `ttfb_ms: Option<u64>` as derived convenience durations for the metadata bar
  - `RequestExecutionService` persists `dispatched_at_unix_ms`, `first_byte_at_unix_ms`, and `completed_at_unix_ms` instead of leaving them `None`
- **Body tab**: preserve the existing preview-based rendering (monospace text, JSON pretty-print for `application/json`); add XML/HTML pretty-print using `quick-xml`; preserve the "Load Full Body" button for `DiskBlob` responses
- replace the current lossy header serialization with an ordered row-array JSON format for newly persisted history entries, for example:
  ```json
  [{"name":"set-cookie","value":"a=1; Path=/"},{"name":"set-cookie","value":"b=2; Path=/"}]
  ```
  - readers must accept both the new ordered row-array format and the legacy object-map format already in the repo
  - legacy object-map history is best-effort only; if duplicate-header fidelity is unavailable, the UI shows a small legacy-format note instead of pretending the data is lossless
- **Headers tab**: parse the ordered header rows into a two-column table; preserve repeated headers as separate rows; use `gpui-component` `Table` for the layout
- **Cookies tab**: parse `Set-Cookie` rows from the header-row representation using the `cookie` crate; render a table with columns: name, value (truncated preview), domain, path, expires/max-age, secure, httpOnly, sameSite; multiple cookies with the same name appear as distinct rows; if no `Set-Cookie` rows exist, show an empty state
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
      TransportError { summary: String, detail: String },
  }
  ```
- cancellation remains a separate lifecycle result (`ExecStatus::Cancelled` / `ExecOutcome::Cancelled`) and must not be represented as a transport classification
- classify errors by inspecting `reqwest::Error` methods: `.is_timeout()`, `.is_connect()`, `.is_request()`, and the inner error chain (downcast to `std::io::Error` for `ConnectionRefused`, `hyper` errors for DNS, `rustls`/native-tls errors for TLS)
- extract `error_display.rs` into the request tab sub-module that renders `ClassifiedError` with:
  - an icon or color badge per category
  - a primary human-readable message (e.g., "Could not resolve host: api.example.com")
  - an expandable detail section for the full error chain
- preflight errors (`PreflightFailed` from `ExecOutcome`) are rendered separately with their own category labels (malformed URL, missing body file, secret resolution failure) — they do not go through the transport classifier
- update the failure display contract so live failures can show classification while restored failures still have a defined fallback:
  - `ExecOutcome::Failed` / `ExecStatus::Failed` should carry `{ summary: String, classified: Option<ClassifiedError> }` or an equivalent display model
  - live transport failures populate both `summary` and `classified`
  - restored history failures populate `summary` and `classified = None`, rendering a generic failed card with the saved message
- the existing `error: String` history persistence path remains the durable source of truth; no schema change is required for classified detail
- enumerate the downstream edits required by the failed-state shape change:
  - `RequestEditorState::fail_exec` and `RequestEditorState::restore_failed_response`
  - all `match` arms over `ExecStatus` in `request_tab.rs` / `response_panel.rs`
  - reopen-from-history wiring in `root.rs`
  - latest-run summary helpers and tests that currently assume `Failed(String)`

Definition of done:

- DNS, connection refused, timeout, TLS, and generic transport errors display distinct human-readable messages
- preflight failures display with their own labels, not as transport errors
- the full error chain is available via an expandable detail section
- history persistence continues to store a string summary (no schema change), and restored failures render a defined fallback when classification data is unavailable

## Slice 3: Response Body Actions (Copy, Save, Image, Search)

Purpose: make response bodies actionable beyond read-only preview.

Tasks:

- **Copy to clipboard**:
  - enabled for text-like media types (`text/*`, `application/json`, `application/xml`, `application/javascript`, etc.)
  - copies from in-memory preview if available
  - if the body is `DiskBlob`, copy reads from the blob store rather than requiring `loaded_full_body_text`
  - use GPUI's clipboard API directly for dynamic copy paths: `cx.write_to_clipboard(ClipboardItem::new_string(...))`
  - `gpui-component::clipboard::Clipboard` may be used only for simple synchronous preview-copy buttons where the source string is already materialized and bounded
  - no additional clipboard dependency is required for Phase 3.5
  - if the blob-backed text exceeds a clipboard safety cap, prompt before materializing the final clipboard string; the temporary allocation must stay out of hot per-tab state
  - disabled with an explanatory tooltip for binary and image media types
  - add a "Copy" button to the Body tab header bar
- **Save to file**:
  - opens a native save dialog via GPUI's async prompt receiver path (`cx.prompt_for_new_path(...)`)
  - for `InMemoryPreview` bodies: writes from the in-memory bytes directly
  - for `DiskBlob` bodies: reads from the blob store using the reader/streaming path (`BlobStore::open_read` or equivalent); **never** allocates a second full-body copy in RAM for large bodies — use reader-to-writer streaming
  - add a "Save" button to the Body tab header bar
- **Image preview**:
  - before landing the full image-preview UX, build a small prototype that proves the chosen GPUI byte→image→render path works from preview bytes
  - the prototype must validate one concrete rendering path for preview bytes, such as a cached image/custom loader path or a temp-file URI/resource fallback
  - for `image/*` media types, render the preview bytes as an image instead of text
  - decode from `BodyRef` preview bytes only (either `InMemoryPreview.bytes` or `DiskBlob.preview`), never from the full blob by default
  - images truncated at the preview cap show a "Preview truncated — load full image" notice
  - after "Load Full Body" is triggered, the full image is decoded and rendered
  - use the concrete GPUI rendering path validated by the prototype; do not commit to an unverified byte-loading approach
  - if decode fails (corrupt or unsupported format), fall back to showing the raw bytes as hex dump with a format error message
- **XML/HTML pretty-print**:
  - when `media_type` matches `application/xml`, `text/xml`, or `text/html`, parse with `quick-xml` and re-indent
  - if parsing fails, fall back to raw text display with no error — the original text is always the safe fallback
- **In-body search** (`body_search.rs`):
  - case-insensitive substring search
  - search state is view-local (not persisted, not on `RequestEditorState`)
  - search bar appears at the top of the Body tab when activated (Cmd+F / Ctrl+F while Body tab is focused)
  - highlights all matches in the preview text; arrow buttons or Enter/Shift+Enter cycle through matches
  - if the preview is truncated, preview search still works immediately, and "Search all content" triggers a chunked blob scan rather than forcing the entire body into hot state
  - full-content search may populate a bounded transient text cache only when the full rendered text fits within `PER_TAB_CAP_BYTES`; otherwise it returns navigable match snippets/offsets from the blob scan
  - search operates on the rendered text (after pretty-print) when text is materialized; chunked scan falls back to raw decoded text blocks when the full rendered document is too large to cache
- add Fluent keys for all button labels, tooltips, prompts, and error messages

Definition of done:

- copy works for text types and is disabled with tooltip for binary/image
- save-to-file streams for large bodies without a full in-memory allocation
- image preview renders from preview bytes; truncated images show a notice instead of corruption
- XML/HTML pretty-print works alongside existing JSON pretty-print
- search finds and highlights preview matches and can scan full content via blob-backed reads when truncated
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
  - when params editor rows change, `url_with_params()` rewrites the URL query string using enabled, non-empty rows only
  - disabled params remain visible in the editor table but are omitted from URL serialization
  - manual URL edits replace the enabled rows derived from the URL while preserving disabled rows already present in the editor
  - the URL↔params bridge must use the existing `ReentrancyGuard` pattern (`enter()` / reciprocal update / `leave_and_take_deferred()`) or an equally explicit source-tagged guard; nested self-updates without that guard are not acceptable
- replace `headers_input: Entity<InputState>` with the key-value editor
- remove `parse_key_value_pairs()` and `key_value_pairs_to_text()` — the text serialization path is no longer needed
- add Fluent keys for column headers, empty state, and add-row button

Definition of done:

- params and headers use a proper table with add/remove/enable/disable per row
- URL↔params sync is preserved (URL edits update the table, table edits update the URL)
- disabled param rows do not leak into the URL and survive manual URL edits
- the text-based `key=value\n` input is fully removed
- the key-value editor is a standalone component reusable by other sections (urlencoded body, form-data text fields)

## Slice 5: Auth Type Selector and Structured Fields

Purpose: replace the text-protocol auth input with a dropdown and per-type structured fields.

Tasks:

- build `auth_editor.rs` with:
  - a dropdown (`gpui-component` `Dropdown` or `Popover` + `List`) for auth type selection: None, Basic, Bearer, API Key
  - a small view-local per-auth-type draft cache so switching auth types and switching back preserves the user's last inputs without changing the persisted `AuthType` shape
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
- create `src/services/request_body_payload.rs` as the explicit request-payload builder used by `RequestExecutionService`
  - `body_editor.rs` remains responsible for editing state only
  - `request_body_payload.rs` owns large body assembly, blob readers, and streaming adapters into `reqwest::Body`
- **File-backed body UX** (for form-data file fields and binary):
    - extend the body/file model where needed so "picked body type, no file selected yet", replace, clear, and missing-file states are representable without fake blob hashes
    - "Pick File" opens a native file dialog via GPUI's async path prompt receiver (`cx.prompt_for_paths(...)`)
    - selected file is read and written to the blob store; the resulting `blob_hash` and `file_name` are stored on the domain model (`FileField.blob_hash`, `BinaryFile.blob_hash`)
    - "Replace File" re-opens the file dialog and overwrites the blob ref
    - "Clear File" removes the blob ref
    - if the blob is missing or unreadable at send time, the preflight check surfaces a recoverable error with the file name — this already works via Phase 3's preflight path
    - file size cap: files over 100 MB trigger a confirmation dialog before reading; the dialog shows the file size and warns about memory use
    - files are written to the blob store without holding the entire file in RAM
    - at send time, request-body construction streams from blob storage; it does not materialize a single `Bytes` buffer for large binary/form-data bodies
  - refactor request-body transport/building so large file-backed bodies are streamable:
    - replace the current `Option<Bytes>`-only request-body path with a request-payload abstraction that can stream blob readers into `reqwest::Body`
    - multipart form-data file parts stream from blob readers; binary file bodies stream directly from the blob store
  - the component reads from `BodyType` and emits change events with the updated `BodyType`
- replace `body_input: Entity<InputState>` with `body_editor`
- remove `body_editor_value()` — the text extraction helper is no longer needed
- add a **method selector dropdown** to replace the `method_input` text field: options are GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS, with a freeform fallback for custom methods
- add Fluent keys for all type labels, field labels, file action buttons, empty states, and the 100 MB confirmation dialog

Definition of done:

- body type is selected from a dropdown
- each body type renders an appropriate editor (text, KV table, file picker)
- file-backed bodies store blob refs through the updated backward-compatible body model
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
  - `ToggleSidebar` — toggle `WindowLayoutState.sidebar_collapsed`
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

- create `src/views/item_tabs/request_tab/` directory for child modules
- keep `src/views/item_tabs/request_tab.rs` as the module entry point and add child modules under `src/views/item_tabs/request_tab/`
- move the response panel (Slice 1) into `response_panel.rs`
- move the metadata bar into `response_metadata_bar.rs`
- move classified error display (Slice 2) into `error_display.rs`
- move the key-value editor (Slice 4) into `key_value_editor.rs`
- move the auth editor (Slice 5) into `auth_editor.rs`
- move the body editor (Slice 6) into `body_editor.rs`
- move body search (Slice 3) into `body_search.rs`
- keep the top-level `RequestTabView` struct, `Render` impl, action handlers, save/send/cancel/duplicate flows, and draft sync logic in the main `request_tab.rs`
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
- response-header serialization/parsing: repeated headers preserved in the new ordered-row format; legacy object-map history still parses best-effort
- cookie parsing: multiple cookies, duplicate names, all RFC 6265 attributes, malformed `Set-Cookie` headers
- XML pretty-print: well-formed XML, malformed XML (graceful fallback to raw text), empty input
- error classifier: DNS failure, connection refused, timeout, TLS error, generic transport
- cancellation lifecycle: cancelled remains a separate exec-status path and is never wrapped in `ClassifiedError`
- response metadata bar: size formatting (bytes, KB, MB), color coding for each status range
- auth editor type-switch cache: Basic → Bearer → Basic restores prior Basic fields without changing persisted `AuthType`
- reentrancy regressions: URL↔params sync, body-search toggling, and response-body action callbacks do not trigger unsafe nested entity updates

Integration tests:

- save-to-file for `DiskBlob` body: verify the destination file matches the blob content without a second full-body RAM allocation (assert peak memory delta stays bounded)
- image preview: verify a 3 MiB image body with 2 MiB preview cap renders the preview without loading the full blob; verify "Load Full Image" loads and renders the complete image
- search across preview boundary: verify preview search works; verify "Search all content" uses the blob-backed path when preview is truncated; verify matches beyond the original preview are discoverable without forcing an unbounded hot-state allocation
- close-tab shortcut with dirty request: verify the confirm dialog appears
- new-request shortcut with no collection selected: verify toast appears and no tab is created
- auth editor round-trip: select each auth type, fill fields, save, reopen — verify fields persist correctly through `SecretManager`
- body type switch: select each body type, edit content, save, reopen — verify content persists correctly
- file-backed body: pick a file, save, reopen — verify the blob ref is intact; pick a file over 100 MB — verify confirmation dialog appears
- request-body streaming: sending a large binary or multipart file-backed request does not materialize a single full `Bytes` body in hot state
- history restore: reopen a completed request and verify Headers/Cookies/Timing tabs render from persisted history/blob data without re-send
- failed/cancelled restore: reopen failed and cancelled requests and verify the correct fallback display contract

Performance tests:

- large file-backed request send (for example 100 MB binary body) proves request memory remains bounded and does not scale with full payload size
- blob-backed save-to-file and search-all-content paths on large responses (for example 50 MB+) prove bounded peak memory and linear streaming/chunked behavior
- response-panel render for a large blob-backed response proves initial UI render stays preview-bound and does not trigger a full-body load

Security tests:

- auth editor password fields never expose secret values in the DOM / render tree — only the `SecretManager` ref is stored
- the copy-to-clipboard action does not include auth headers from the response headers tab
- async file/body actions tolerate dropped window/entity/app targets without panics or secret leakage

## 11. Fluent Key Naming Convention

Follow the Phase 3 prefix pattern: every new key keeps the full `request_tab_` prefix plus a descriptive snake_case suffix.

New key groups for Phase 3.5:

```
# Response tabs
request_tab_response_tab_body
request_tab_response_tab_headers
request_tab_response_tab_cookies
request_tab_response_tab_timing

# Response metadata bar
request_tab_response_meta_size_bytes
request_tab_response_meta_size_kb
request_tab_response_meta_size_mb

# Response body actions
request_tab_response_action_copy
request_tab_response_action_copy_disabled_tooltip
request_tab_response_action_save
request_tab_response_action_save_failed
request_tab_response_body_search_placeholder
request_tab_response_body_search_all_content
request_tab_response_body_image_truncated

# Classified errors
request_tab_error_dns_failure
request_tab_error_connection_refused
request_tab_error_connection_timeout
request_tab_error_tls_failure
request_tab_error_request_timeout
request_tab_error_transport_generic
request_tab_error_detail_expand
request_tab_error_detail_collapse

# Timing tab
request_tab_timing_total
request_tab_timing_ttfb
request_tab_timing_dispatched
request_tab_timing_completed
request_tab_timing_dns
request_tab_timing_tcp
request_tab_timing_tls
request_tab_timing_placeholder

# Cookies tab
request_tab_cookies_empty
request_tab_cookies_col_name
request_tab_cookies_col_value
request_tab_cookies_col_domain
request_tab_cookies_col_path
request_tab_cookies_col_expires
request_tab_cookies_col_secure
request_tab_cookies_col_httponly
request_tab_cookies_col_samesite

# Key-value editor
request_tab_kv_col_enabled
request_tab_kv_col_key
request_tab_kv_col_value
request_tab_kv_add_row

# Auth editor
request_tab_auth_select_type
request_tab_auth_basic_username
request_tab_auth_basic_password
request_tab_auth_bearer_token
request_tab_auth_api_key_name
request_tab_auth_api_key_value
request_tab_auth_api_key_location
request_tab_auth_api_key_location_header
request_tab_auth_api_key_location_query
request_tab_auth_secret_show
request_tab_auth_secret_hide

# Body editor
request_tab_body_select_type
request_tab_body_file_pick
request_tab_body_file_replace
request_tab_body_file_clear
request_tab_body_file_missing
request_tab_body_file_size_warning_title
request_tab_body_file_size_warning_body

# Method selector
request_tab_method_custom

# Keyboard shortcuts
request_tab_shortcut_no_collection
request_tab_shortcut_close_tab
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
- search works within the active preview and can scan all content via blob-backed reads when the preview is truncated
- header and cookie views preserve repeated headers and all RFC 6265 cookie attributes without lossy flattening for all newly written history entries; legacy history is best-effort with an explicit fallback note
- timing panel always shows DNS/TCP/TLS rows as `—` placeholders rather than omitting them
- section tab content is replaced with structured component editors; text-input fallbacks are removed
- URL↔params bidirectional sync is preserved through the new key-value params editor
- auth type selector populates structured fields from the existing `AuthType` domain model without data migration
- auth type switching preserves prior per-type field edits when switching back within the same editor session
- file-backed body flows cover pick, replace, clear, and missing-file states before send
- files over 100 MB trigger a confirmation prompt; large file bodies are not fully loaded into RAM
- large file-backed request sends stream from blob storage instead of materializing a single full request body buffer
- all new shortcuts are documented, platform-correct, scoped correctly, and functional
- close-tab shortcut respects the close-while-dirty confirm dialog
- new-request shortcut requires a selected collection and shows a toast when none is selected
- all new user-facing copy and error messages are Fluent-based

That is the minimum bar before moving on to Phase 4 tree CRUD and environment resolution work.
