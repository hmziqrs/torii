# Postman Clone Phase 3 Executable Plan

> Derived from `docs/plan.md` Phase 3
> Constrained by `docs/state_management.md`
> Date: 2026-04-11
> Last audit: 2026-04-12 — all slices verified, all validation gates passing

## 1. Objective

Land a production-safe REST request workflow on top of the Phase 2 item-driven tab shell:

- editable request drafts in active request tabs
- safe save, duplicate, reopen, and cancel flows
- explicit request lifecycle state with deterministic terminal transitions
- bounded response previews with blob-backed full-body persistence
- per-request latest-run summary without introducing global history UI yet

Phase 3 must make REST solid enough that GraphQL, WebSocket, and gRPC can reuse the same lifecycle, persistence, and history contracts later instead of forcing a redesign in Phase 5.

## 2. Non-Negotiable V2 Rules

These are mandatory for Phase 3:

- Keep active request draft state in hot per-tab entities; do not promote all persisted requests into entities
- Keep long-lived request catalogs repo/value-backed; no `Vec<Entity<_>>` or `HashMap<Id, Entity<_>>` as the primary source of truth
- Every send owns an explicit `OperationId`, task handle, cancellation primitive, and terminal execution state
- Terminal execution states are mutually exclusive: `Completed | Failed | Cancelled`
- Save status and execution status are tracked on **orthogonal axes** (see §6.2); a request can legally be `Dirty × Completed` (edited after a successful response) or `Dirty × Sending` (resending while still dirty)
- Late responses are ignored unless their operation ID still matches the active request operation
- Treat post-`await` app/window/entity updates as fallible; no `unwrap()` or `expect()` on those paths
- Persist structured request metadata in SQLite and large request/response bodies in the blob store
- Keep response previews bounded in memory and reload full bodies from disk on demand
- Store auth secrets in the platform credential store through `SecretManager`; only persist opaque secret refs in SQLite
- Keep scripts/tests as placeholders only in this phase; do not add ad hoc execution logic that bypasses the shared lifecycle model
- Use Fluent for all new labels, actions, placeholders, errors, and notifications
- Prefer `gpui-component` composition before creating custom request-builder UI primitives

## 3. Current Repo Starting Point

Phase 2 established the shell, but the request workflow is still intentionally thin:

- `RequestItem` only persists `name`, `method`, `url`, and `body_blob_hash` in `src/domain/request.rs`
- `SqliteRequestRepository` supports create/get/list/rename/move/reorder/delete, but not full request save, duplicate, or revision-checked update in `src/repos/request_repo.rs`
- `RequestTabView` is still a placeholder shell with a URL input and a "deferred to Phase 3" message in `src/views/item_tabs/request_tab.rs`
- `HistoryRepository` can create pending rows and finalize completed/failed rows, but Phase 3 still needs explicit cancel finalization and richer response metadata in `src/repos/history_repo.rs`
- `BlobStore` already supports atomic writes plus preview/full-body reads in `src/infra/blobs/mod.rs`
- `AppRoot` currently caches request tabs by persisted `RequestId`, which is sufficient for saved requests but not yet sufficient for unsaved request drafts in `src/root.rs`
- The current dependency set does not yet include the concrete REST client stack called for in `docs/plan.md` (`reqwest`, `http`, and direct `url` usage)

That means Phase 3 should build on the current tab/session infrastructure, but must expand the request domain, repo contract, and request-tab ownership model before wiring network execution.

## 4. Phase 3 Deliverables

Phase 3 is complete only when all of the following exist:

- expanded persisted REST request model covering:
  - method
  - URL
  - params
  - auth config with secret refs only
  - headers
  - body config + optional blob-backed body payload
  - scripts/tests placeholder content
  - request settings
- hot `RequestEditorState` per open request tab with:
  - draft value
  - persisted baseline
  - dirty tracking
  - save status
  - execution lifecycle
  - latest response snapshot
- request save flow with optimistic revision conflict detection
- request duplicate flow that produces a new persisted request safely
- request send and cancel flows on top of a shared execution service
- history-backed latest-run summary panel in the request tab
- secret-safe sent-request history snapshots persisted alongside response metadata for each run
- bounded response preview behavior with full-body reload from blob storage
- deterministic late-response ignore behavior enforced by operation ID checks
- startup recovery coverage for interrupted sends, stale pending history rows, and orphan response blobs
- test coverage for save, duplicate, send, cancel, truncation, reopen, and conflict paths

## 5. Scope Boundary

Phase 3 is intentionally narrow even though it touches several layers.

Included now:

- REST request editing and execution
- request-local draft state for active tabs
- minimal request creation support required for the request-editor workflow
- latest-run summary inside the request tab

Explicitly deferred:

- global history panel and advanced history filtering (Phase 5)
- GraphQL, WebSocket, and gRPC execution (Phase 5)
- environment-variable resolution pipeline and inheritance UI (Phase 4)
- tree-wide CRUD affordances, drag/drop, and context-menu parity (Phase 4)
- scripts/tests execution engine; this phase only lands placeholders and persistence shape
- full merge UX for multi-window write conflicts; this phase only requires revision conflict detection and a recoverable save error path

Note on new request creation:

- Phase 3 lands request-draft tab identity for unsaved new requests, matching `docs/plan.md` Section 4.3
- A draft tab carries a `RequestDraftId` that transitions to a persisted `RequestId` on first save; the tab stays focused across the transition without closing and reopening
- The "create a persisted stub immediately" shortcut is rejected: it pollutes the sidebar with empty rows and diverges from the tab identity model other item kinds already use

REST client configuration defaults (locked in Phase 3, configurable UI deferred to later phases):

- 30-second total request timeout
- follow redirects up to 10 hops
- no persistent cookie jar
- no proxy
- native TLS verification enabled
- no custom CA trust store

Any change to these defaults is out of scope for Phase 3 and must go through a later request-settings phase.

## 6. Persistence and State Shape

## 6.1 Request model

Keep the existing `RequestItem` name unless a rename materially reduces confusion. Expand it into a structured REST request value model rather than scattering request fields across ad hoc UI state.

Recommended persisted shape:

- scalar columns for the fields used by catalogs and list surfaces:
  - `name`
  - `method`
  - `url`
  - `body_blob_hash`
  - timestamps/revision
- JSON columns for nested editor sections that are request-local and naturally loaded as one value:
  - `params_json`
  - `headers_json`
  - `auth_json`
  - `body_json`
  - `scripts_json`
  - `settings_json`

Scripts column shape:

- `scripts_json` is a single JSON object with two string fields: `pre_request` and `tests`
- both default to empty strings; Phase 3 persists and edits them but never executes them
- this shape is stable enough for a later script-engine phase to consume without another migration

Migration 0006 backfill rule:

- new JSON columns are added with additive `ALTER TABLE ... ADD COLUMN ... DEFAULT '{}'`
- `scripts_json` defaults to `'{"pre_request":"","tests":""}'`
- existing request rows produced by Phase 2 load through `map_request_row` without a separate backfill step

Body type variants supported in Phase 3:

- `none` — no body
- `raw_text` — `text/plain` payload, edited as a string
- `raw_json` — `application/json` payload, edited as a string and pretty-printed by the response panel
- `urlencoded` — `application/x-www-form-urlencoded` key-value list
- `form_data` — `multipart/form-data` with text fields and file fields (file fields hold blob refs)
- `binary_file` — single binary payload backed by a blob ref

GraphQL body, raw XML/HTML body, and other variants are deferred to Phase 5 alongside the GraphQL editor.

Auth type variants supported in Phase 3:

- `none` — no auth applied
- `basic` — username + password secret ref pair
- `bearer` — token secret ref
- `api_key` — key name + value secret ref + location (`header` or `query`)

OAuth 2.0, AWS Signature, and other auth flows are deferred to a later phase. All Phase 3 auth variants persist only `secret_refs` in `auth_json`; the actual secret values live in the platform credential store via `SecretManager`.

Secret ownership and lifecycle rules:

- before first save, an unsaved request draft may own **draft-scoped** secret refs keyed by `RequestDraftId`; these refs are session-local support for draft editing and send-before-save
- on first successful save, any draft-scoped secret refs are migrated to new owner-scoped refs keyed by the persisted `RequestId`, and the draft-scoped refs are best-effort deleted
- after first save, secret refs are always owned by the persisted request ID; a request never points at another request's secret refs
- saving an existing request upserts the same owner-scoped secret refs in place for that request ID
- duplicating a request clones the source request's secret values into **new** secret refs owned by the duplicate request ID; it must never reuse the source request's secret refs verbatim
- if secret cloning fails during duplicate, the duplicate flow fails as a recoverable error and performs compensating cleanup for any newly created request row or secret refs
- removing an auth scheme or deleting a request best-effort deletes the secret refs owned by that request; cleanup failure is logged and remains recoverable

Settings column shape (`settings_json`):

- `timeout_ms` — optional per-request timeout override; `None` falls back to the §5 default of 30,000 ms
- `follow_redirects` — optional per-request override; `None` falls back to the §5 default of `true`
- additional fields (proxy override, custom CA, etc.) are explicitly out of scope for Phase 3 and must be added in a later request-settings phase
- per-request `ssl_verify` is **not** introduced in Phase 3; native TLS verification stays on globally

HTTP type crate boundary:

- `RequestItem` and the repo layer stay serde-friendly: method is a `String`, headers/params are JSON values
- `http::Method`, `http::HeaderMap`, and `http::HeaderName` only appear inside `RequestExecutionService` at the wire boundary
- no `http` types leak into the domain or repo modules

Variable placeholder behavior in Phase 3:

- `{{name}}` placeholders in URL, headers, params, body, or auth fields are sent **literally** to the network
- the execution service emits a structured warning log when it detects unresolved placeholders before sending
- environment-variable resolution lands in Phase 4 and replaces the warning with substitution; Phase 3 must not introduce any ad hoc placeholder replacement

Rules:

- auth JSON must only contain references and non-secret config; actual secrets stay in `secret_refs` + keychain
- large body payloads must continue to use blob refs instead of unbounded inline strings in SQLite
- repo save APIs must accept an expected revision and fail cleanly on mismatch

This keeps the warm store normalized by request ID without over-normalizing header/param rows into separate tables before we actually need cross-request querying on them.

## 6.2 Request editor entity

Introduce a hot entity dedicated to the active request tab state.

Recommended responsibilities:

- current draft request value
- persisted baseline for dirty diff checks
- save status (axis A — see below)
- execution status (axis B — see below)
- active operation metadata
- latest response metadata and preview
- latest history entry ID for reopen/refresh

State model — two orthogonal axes:

`SaveStatus` (axis A — about persistence):

- `Pristine` — draft equals baseline
- `Dirty` — draft diverges from baseline
- `Saving` — save in flight
- `SaveFailed { recoverable_error }` — save attempt failed; draft still in memory; transitions back to `Dirty` after the user retries or reloads baseline

`ExecStatus` (axis B — about the network operation):

- `Idle` — no operation has run yet, or the previous operation was cleared
- `Sending` — request handed to `RequestExecutionService`, awaiting connection/headers
- `Streaming` — response headers received, body streaming into preview + blob writer
- `Completed { response_summary }` — terminal
- `Failed { error }` — terminal
- `Cancelled { partial_size }` — terminal

Combined-state rules:

- the editor entity holds **both** axes; UI renders them independently
- valid combinations include `Dirty × Completed` (edit after a response), `Dirty × Sending` (resend while dirty), `Pristine × Cancelled` (saved request whose run was aborted)
- terminal `ExecStatus` values are mutually exclusive on the exec axis only — they say nothing about the save axis
- a save in flight (`Saving`) does not block sending; a send in flight does not block saving

Recommended rules:

- sending is allowed from dirty draft state; save is not a prerequisite to execution
- dirty draft state is session-local hot state in this phase
- persisted request rows remain the reopen source of truth after restart
- if dirty-draft restore across restart is needed later, it should be added explicitly rather than leaking partial draft data into tab-session persistence by accident

Operation identity:

- `OperationId` is a type alias for `HistoryEntryId`
- The send flow creates the pending history row first, then launches the network task; the history row's ID identifies the operation end-to-end
- Late-response ignore checks compare the active operation ID stored on the editor entity against the completing task's operation ID
- An editor entity has at most one active operation; clicking send while a previous operation is in flight **auto-cancels** the in-flight operation, then immediately starts the new one (no confirm dialog, no queueing)
- The auto-cancel path still emits the late-response ignore guard so a delayed in-flight response cannot stomp the new operation's state

Cross-window conflict propagation:

- Phase 3 detects revision conflicts at save time only, via the request repo `save` API with an expected revision
- Live broadcast of another window's save into an open editor is deferred to Phase 4 alongside the repo subscription layer
- On save conflict, the editor surfaces a recoverable error with a "reload baseline" action that refetches the persisted row and replaces the baseline without discarding the in-memory draft

## 6.3 Response/body model

Follow `docs/state_management.md` Section 5.1.

Use a bounded response body reference shape along these lines:

```rust
pub enum BodyRef {
    Empty,
    InMemoryPreview { bytes: bytes::Bytes, truncated: bool },
    DiskBlob {
        blob_id: String,
        preview: Option<bytes::Bytes>,
        size_bytes: u64,
    },
}
```

Rules:

- keep only the preview in hot entity state
- persist the full response body in the existing blob store
- latest-run reopen should use blob preview/full-body load paths instead of loading the whole body eagerly
- use `bytes::Bytes` (not `Vec<u8>`) for preview buffers so slicing the in-memory preview from a larger streamed buffer is zero-copy; the `bytes` crate is already a Phase 3 dependency per `docs/plan.md` §3.1

Concrete caps for Phase 3:

- per-response in-memory preview cap: 2 MiB (`RESPONSE_PREVIEW_CAP_BYTES`)
- per-tab total volatile response footprint cap: 32 MiB across preview + metadata
- these caps live in a `response_budgets` config struct so unit and performance tests can assert against them instead of magic numbers
- bodies larger than the preview cap are still streamed to the blob store in full; the in-memory `InMemoryPreview` variant is never used for responses that exceeded the cap

Orphan blob handling on cancel or failure:

- if a send is cancelled or fails mid-stream, the partially written blob is best-effort deleted and the history row is finalized without a `blob_hash`
- delete failures are logged but not fatal; Phase 7 orphan compaction will reclaim any residue
- a successfully completed but unclaimed blob (e.g. entity dropped before finalize) is treated the same way

## 6.4 History snapshot model

Phase 3 history rows must carry a secret-safe immutable snapshot of what was sent, not just the response.

Persisted sent-request snapshot shape:

- `request_method`
- `request_url_redacted`
- `request_headers_redacted_json`
- `request_auth_kind`
- `request_body_summary_json`:
  - body kind
  - media type
  - payload size
  - existing request body blob ref when the payload already came from persisted request storage

Rules:

- the snapshot is created before the network request starts and never mutated afterward
- secret-bearing query values, auth headers, and other secret-derived header values are redacted before persistence
- Phase 3 does **not** persist a second full copy of the sent request body into history storage; it stores a summary plus any existing persisted request-body blob ref
- this is intentionally a secret-safe execution snapshot, not a byte-for-byte wire capture
- later history compare or reopen features in Phase 5 must build on this shape rather than redesigning the schema from scratch

## 7. Proposed Module Layout

```text
src/
  domain/
    request.rs
    history.rs
  session/
    request_editor_state.rs
  services/
    request_execution.rs
  repos/
    request_repo.rs
    history_repo.rs
  views/
    item_tabs/
      request_tab.rs
tests/
  request_repo_roundtrip.rs
  request_editor_lifecycle.rs
  request_send_cancel_race.rs
  request_response_blob.rs
  request_conflict_detection.rs
migrations/
  0006_request_editor_core.sql
  0007_history_response_metadata.sql
```

Notes:

- keep request execution in a service layer, not in the tab view
- keep repo interfaces free of GPUI entity types
- reuse the existing blob store and secret manager instead of introducing parallel storage helpers

## 8. Execution Slices

## Slice 0a: Dependency and Schema Contract

Purpose: add the external and persistence primitives required for the rest of Phase 3.

Tasks:

- add `reqwest`, `http`, and `url` dependencies called for by the main plan
- add migration `0006_request_editor_core.sql` to expand request persistence for editor sections
- add migration `0007_history_response_metadata.sql` to expand latest-run/history metadata needed by the request tab
- migration 0006 must use additive `ALTER TABLE ... ADD COLUMN` only with the JSON column defaults defined in §6.1; no data backfill step
- migration 0007 must add the columns required for `finalize_cancelled` (e.g. `cancelled_at`, `partial_size`) so Slice 4 and Slice 5 share a single schema landing
- migration 0007 must also add the secret-safe sent-request snapshot columns from §6.4 so history rows are useful beyond the Phase 3 latest-run panel
- keep migrations additive and roundtrip-testable
- migration rollback is dev-only via `DROP COLUMN` (SQLite ≥ 3.35); production migrations are forward-only and crash-safe

Definition of done:

- [x] app boots with the new dependencies and schema
- [x] migration roundtrip coverage exists for the new request/history shape
- [x] no secret material is introduced into request/history schema columns

## Slice 0b: Request Domain and Repository Expansion

Purpose: make persisted request rows rich enough to support a real editor.

Tasks:

- expand `RequestItem` from a thin shell into a structured REST request value
- add request repo APIs for:
  - full get
  - save/update with expected revision
  - duplicate
  - minimal create path for request-editor flows
- define explicit repo errors for not found vs revision conflict vs storage failure
- keep tree-ordering and move semantics intact

Definition of done:

- [x] a structured request roundtrips through SQLite cleanly
- [x] duplicate creates a new request with a new ID and correct parent/collection ownership
- [x] save detects stale revisions instead of blindly overwriting another window's changes

## Slice 1: Request Editor State and Lifecycle FSM

Purpose: establish the hot-state contract before UI or networking complexity grows.

Tasks:

- introduce `RequestEditorState` as the hot per-tab entity for active request editing
- model save status and execution status as **two orthogonal axes** (see §6.2):
  - `SaveStatus { Pristine, Dirty, Saving, SaveFailed }`
  - `ExecStatus { Idle, Sending, Streaming, Completed, Failed, Cancelled }`
- adopt `OperationId` (alias for `HistoryEntryId`, see §6.2) and track at most one active operation per editor entity
- send-while-sending auto-cancels the in-flight operation and starts the new one (see §6.2); no queueing, no confirm dialog
- add helpers for dirty detection, baseline reset, save-success baseline replacement, and cancel eligibility
- make late-response ignore behavior part of the state API rather than a view-layer convention
- define a preflight error channel that is **distinct** from the `Failed` exec terminal state: secret-resolution failures, URL parse errors, and other pre-send validation problems do not transition `ExecStatus` to `Failed` because nothing was sent; the editor stays in its current state and surfaces a recoverable error

Definition of done:

- [x] both save-status and exec-status transitions are unit-tested independently
- [x] terminal `ExecStatus` values are mutually exclusive on the exec axis
- [x] stale operation completions are ignored deterministically, including across an auto-cancel-then-resend
- [x] no request networking is launched directly from the view layer

## Slice 2: Request Tab UI Shell

Purpose: replace the placeholder request tab with a real Postman-like editor layout.

Tasks:

- rebuild `src/views/item_tabs/request_tab.rs` around editor-state-driven sections:
  - method + URL bar
  - params
  - auth
  - headers
  - body
  - scripts/tests placeholder
  - request settings
- add save, duplicate, send, and cancel actions
- bind keyboard shortcuts in the request tab: `Cmd/Ctrl+S` saves, `Cmd/Ctrl+Enter` sends, `Esc` cancels the active send (no-op when `ExecStatus::Idle`)
- add latest-run summary panel region
- treat the URL bar as the canonical store for query params; the params tab is a derived view that re-serializes its edits into the URL bar on change, so there is no separate params source of truth to keep in sync
- response panel rendering in Phase 3: raw text view plus pretty-printed JSON when the response media type is `application/json`; XML, HTML, and image preview are deferred
- show a dirty indicator on the tab title when the editor draft diverges from the persisted baseline
- close-while-dirty opens a confirm dialog with Save / Discard / Cancel actions; Cancel keeps the tab open, Discard drops the editor entity intentionally
- add Fluent keys for every new label, empty state, error, and dialog button
- prefer `gpui-component` controls and layout primitives before building custom widgets

Definition of done:

- [x] request edits mutate hot editor state instead of mutating catalog values directly
- [x] UI renders `SaveStatus { Pristine, Dirty, Saving, SaveFailed }` live; placeholder regions exist for `ExecStatus` states wired up by Slice 4
- [x] no raw user-facing strings are introduced

## Slice 3: Save, Duplicate, and Draft Ownership

Purpose: make request editing safe and recoverable before adding execution.

Tasks:

- wire save action to persist the draft request value through the repo layer
- write request body payloads to the blob store on save when needed
- resolve auth secrets through `SecretManager` and persist only secret refs
- wire duplicate action to create a new persisted request from the **persisted baseline** (not the dirty draft); matches Postman behavior and avoids cross-tab dirty-state leakage
- make secret ownership explicit per §6.1: duplicate clones secrets into new owner-scoped refs, save updates refs owned by the current request, and delete or auth removal best-effort cleans them up
- on first save of an unsaved draft, migrate any draft-scoped secret refs from `RequestDraftId` ownership to the new persisted `RequestId`; a migration failure aborts the save and leaves the draft open with a recoverable error
- treat duplicate as a compensating workflow rather than a single repo call: if request-row creation succeeds but secret cloning fails, rollback the new request row and any new secret refs before surfacing the recoverable error
- implement request-draft tab identity for unsaved new requests (see §5); on first successful save, the draft tab transitions its identity to the persisted `RequestId` without closing/reopening
- refresh sidebar/catalog state after successful save or duplicate

Definition of done:

- [x] save updates the persisted request and resets dirty state to the new baseline
- [x] duplicate opens a distinct request tab without corrupting the source request
- [x] duplicate never aliases the source request's secret ownership, even when the auth config is unchanged
- [x] first save of an auth-bearing unsaved draft rebinds its secrets from `RequestDraftId` ownership to the persisted `RequestId` without losing send capability
- [x] save conflicts are surfaced as recoverable errors, not silent last-write-wins overwrites

## Slice 4: REST Execution Service and Cancellation

Purpose: implement send/cancel safely through shared services instead of ad hoc tab-local networking.

Tasks:

- introduce `RequestExecutionService` backed by the Tokio runtime and `reqwest`
- introduce a `TokioRuntime` app-level service that owns a multi-threaded `tokio::runtime::Runtime`; spawn the request future via `runtime.handle().spawn(...)` and bridge the resulting `JoinHandle` back to the editor entity through `cx.background_spawn(async move { handle.await })` followed by `entity.update(...)`
- never call `tokio::spawn` from a GPUI thread directly; the Tokio reactor is only available through the runtime handle held by `TokioRuntime`
- introduce an `HttpTransport` trait so the execution service can be tested with a `MockTransport` that controls timing (delays, drip-feeds bytes, fails on cue); production builds wire `ReqwestTransport`
- build the shared `reqwest::Client` once with the Phase 3 locked defaults from §5 (timeout, redirects, TLS, no cookie jar, no proxy)
- convert the draft `RequestItem` into `http::Method` + `http::HeaderMap` + URL inside `RequestExecutionService`; `http` types must not appear in `domain/`, `repos/`, or `views/` (see §6.1 HTTP type crate boundary)
- preflight: resolve auth secrets via `SecretManager` before touching the network; preflight failures stay off the FSM terminal states and surface as recoverable errors (see Slice 1)
- preflight secret resolution supports both persisted request ownership and unsaved-draft `RequestDraftId` ownership so send-before-save remains valid
- emit a structured warning log when the outgoing request still contains `{{ }}` placeholders (see §6.1 variable placeholder rule)
- create a pending history row before the network operation starts; its `HistoryEntryId` becomes the `OperationId`, and the row includes the secret-safe sent-request snapshot from §6.4
- use `tokio_util::sync::CancellationToken` as the concrete cancellation primitive; the editor entity holds a clone, the execution task holds a clone, and dropping the request future on cancel propagates the abort to `reqwest`
- store the active task handle and cancellation token on the editor entity
- stream response bodies via `reqwest::Response::bytes_stream()` into a `BlobStore` writer while populating the in-memory preview up to `RESPONSE_PREVIEW_CAP_BYTES`; never buffer the full body in RAM first
- propagate cancel by signalling the cancellation token and dropping/aborting the in-flight request future
- on cancel mid-stream, abort the writer, best-effort delete the partial blob, and call `HistoryRepository::finalize_cancelled` (added in Slice 5) with any partial metadata
- map completion into `Completed | Failed | Cancelled` only
- handle dropped window/entity/app targets after async boundaries as no-op outcomes with logs

Definition of done:

- [x] send from a dirty or clean draft works
- [x] cancel transitions to `Cancelled` deterministically
- [x] a late successful response after cancel does not overwrite cancelled UI state
- [x] window close during in-flight request does not panic

## Slice 5: Response Persistence and Latest-Run Summary

Purpose: make responses durable and bounded.

Tasks:

- capture response metadata:
  - status
  - headers
  - media type
  - size
  - timings: `dispatched_at`, `first_byte_at` (TTFB), `completed_at`, derived `total_ms` and `ttfb_ms` — DNS / connect / TLS phase breakdown is deferred (reqwest does not expose it without middleware)
- keep the secret-safe sent-request snapshot immutable: response finalization enriches the row with response data only and does not rewrite the request snapshot captured at dispatch time
- keep only a bounded preview in hot state, capped per §6.3
- persist the full response body through the existing blob store
- extend `HistoryRepository` with `finalize_cancelled(id, cancelled_at, partial_size)` and update `finalize_completed` to carry the new response metadata columns
- finalize the history row with response metadata and blob refs through the appropriate API for each terminal state
- enforce the caps from §6.3 when populating preview bytes; responses over the cap store `DiskBlob { preview, .. }` with `preview` truncated to the cap (or `None` for binary media)
- add request-tab latest-run summary loading from history + blob preview on reopen
- add "load full body from disk" behavior for the request tab response panel

Definition of done:

- [x] large responses do not stay fully resident in hot memory by default
- [x] reopening a request can show the latest-run summary without resending
- [x] full response bodies can be reopened from disk on demand

## Slice 6: Root/Session Integration and Restore Semantics

Purpose: keep request tabs coherent with the Phase 2 tab/session model.

Tasks:

- update `AppRoot.request_pages` ownership so it can manage request editor entities rather than static placeholder views
- support the locked request-draft tab identity from §5 and Slice 3, including the transition from `RequestDraftId` to persisted `RequestId`
- on delete of a request item with an active operation, issue cancel first, then close the tab after the operation is marked cancelled or detached safely; never rely on entity drop as the only cancellation mechanism
- ensure close/delete cleanup drops request editor entities intentionally after cancel-first semantics have run
- keep delete behavior consistent across windows: deleting a request closes every open tab for that request after cancel is propagated
- on tab restore, rebuild request editor state from persisted request data plus latest-run summary
- integrate with the existing startup recovery path so stale pending rows from interrupted sends become failed rows and orphan response blobs are reclaimed on the next launch
- do not treat dirty request drafts as durable session state unless explicitly implemented and tested

Definition of done:

- [x] saved requests reopen with persisted editor data and latest-run summary
- [x] restoring a missing/deleted request still degrades gracefully to the existing empty state
- [x] delete of an in-flight request cancels first and then closes every corresponding tab without panicking
- [x] startup recovery leaves no request tab pointing at a permanently pending run after restart
- [x] no entity reentrancy or accidental task-drop regressions are introduced

## 9. Validation Gates

Phase 3 should not be considered complete without the following coverage.

Unit tests:

- [x] request repo save/duplicate roundtrip
- [x] revision-conflict detection
- [x] lifecycle FSM transitions including cancel races
- [x] response truncation/body-ref behavior
- [x] editor entity reentrancy guard: follow-up updates triggered from inside an active update path defer instead of re-entering (see `state_management.md` §4.12)
- [x] preflight failure path: secret-resolution and URL-parse errors do not move the FSM to `Failed`

Integration tests:

- [x] send/cancel race with delayed completion
- [x] send-while-sending auto-cancel: a second send during an in-flight operation cancels the first and the late response of the first is ignored
- [x] request save followed by reopen from persisted state
- [x] duplicate request then reopen both source and duplicate tabs
- [x] latest-run summary restore from history/blob store
- [x] restart recovery with a stale pending request run marks the history row failed and keeps latest-run restore usable
- [x] window close during in-flight request with no panic
- [x] deleting a request with an active operation cancels first and closes all open tabs for that request cleanly
- [x] cancel mid-stream leaves no `blob_hash` reference and best-effort deletes the partial blob file
- [x] `MockTransport` drip-feed and stall scenarios deterministically reproduce the cancel race and preview-cap behaviors without hitting the network

Performance tests:

- [x] 10 MB, 50 MB, and 200 MB response handling on the bounded preview path; assert against `RESPONSE_PREVIEW_CAP_BYTES` and the per-tab cap from §6.3
- [x] multiple open request tabs without unbounded response-memory growth, asserted against the per-tab volatile cap

Security tests:

- [x] auth secrets never persist in request rows, history rows, or blob files
- [x] unsaved request drafts with auth can send via draft-scoped secret refs, and first save migrates those refs to the persisted request without plaintext leakage
- [x] duplicated requests receive new owner-scoped secret refs; deleting one request does not break the other's secrets
- [x] log/error paths do not emit raw auth values
- [x] history request snapshots redact secret-derived query/header values before persistence
- [x] secret-store lookup failure produces a recoverable error path

Observability requirements:

- [x] `tracing` spans on every send: `request.send`, `request.cancel`, `response.persist`
- [x] counters: `requests_completed_total`, `requests_cancelled_total`, `requests_failed_total`, `responses_truncated_total`, `preview_bytes_histogram`, `async_update_failures_total` (tagged by category: `dropped_app` | `dropped_window` | `dropped_entity` | `real_error`)
- [x] structured warning logs on every preflight rejection, late-response ignore, and orphan-blob cleanup outcome

## 10. Exit Criteria Mapping Back to `docs/plan.md`

The Phase 3 goals from the main plan are satisfied only when:

- [x] REST requests can be created or duplicated into a usable tab, edited, saved, sent, cancelled, and reopened safely
- [x] request-tab state obeys the explicit lifecycle FSM
- [x] deleting an in-flight request cancels it before tab teardown across windows
- [x] response previews respect configured memory caps
- [x] full response bodies are reopenable from disk
- [x] interrupted sends recover cleanly on the next startup without leaving pending history rows stuck forever
- [x] cancelled requests never re-enter completed UI state from late responses

That is the minimum bar before moving on to Phase 4 tree CRUD and environment resolution work.
