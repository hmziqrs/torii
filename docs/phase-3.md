# Postman Clone Phase 3 Executable Plan

> Derived from `docs/plan.md` Phase 3
> Constrained by `docs/state_management.md`
> Date: 2026-04-11

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
- Every send owns an explicit `OperationId`, task handle, cancellation primitive, and terminal lifecycle state
- Terminal request states are mutually exclusive: `Completed | Failed | Cancelled`
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
- bounded response preview behavior with full-body reload from blob storage
- deterministic late-response ignore behavior enforced by operation ID checks
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

- `docs/plan.md` Section 4.3 requires draft tab IDs for unsaved new items
- if Phase 3 needs true unsaved request creation, add request-draft tab identity only for request tabs in this phase
- if that proves too invasive for the first landing, the fallback is to create a persisted stub request immediately and open it in-place, but that is a fallback, not the preferred plan

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
- save status
- active operation metadata
- latest response metadata and preview
- latest history entry ID for reopen/refresh

Recommended rules:

- sending is allowed from dirty draft state; save is not a prerequisite to execution
- dirty draft state is session-local hot state in this phase
- persisted request rows remain the reopen source of truth after restart
- if dirty-draft restore across restart is needed later, it should be added explicitly rather than leaking partial draft data into tab-session persistence by accident

## 6.3 Response/body model

Follow `docs/state_management.md` Section 5.1.

Use a bounded response body reference shape along these lines:

```rust
pub enum BodyRef {
    Empty,
    InMemoryPreview { bytes: Vec<u8>, truncated: bool },
    DiskBlob {
        blob_id: String,
        preview: Option<Vec<u8>>,
        size_bytes: u64,
    },
}
```

Rules:

- keep only the preview in hot entity state
- persist the full response body in the existing blob store
- latest-run reopen should use blob preview/full-body load paths instead of loading the whole body eagerly

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
- keep migrations additive and roundtrip-testable

Definition of done:

- app boots with the new dependencies and schema
- migration roundtrip coverage exists for the new request/history shape
- no secret material is introduced into request/history schema columns

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

- a structured request roundtrips through SQLite cleanly
- duplicate creates a new request with a new ID and correct parent/collection ownership
- save detects stale revisions instead of blindly overwriting another window's changes

## Slice 1: Request Editor State and Lifecycle FSM

Purpose: establish the hot-state contract before UI or networking complexity grows.

Tasks:

- introduce `RequestEditorState` as the hot per-tab entity for active request editing
- define explicit lifecycle states:
  - `Idle`
  - `Dirty`
  - `Sending`
  - `Waiting`
  - `Receiving`
  - `Completed`
  - `Failed`
  - `Cancelled`
- add `OperationId` and active-operation tracking
- add helpers for dirty detection, baseline reset, save-success baseline replacement, and cancel eligibility
- make late-response ignore behavior part of the state API rather than a view-layer convention

Definition of done:

- lifecycle transitions are unit-tested
- terminal states are mutually exclusive
- stale operation completions are ignored deterministically
- no request networking is launched directly from the view layer

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
- add latest-run summary panel region
- add Fluent keys for every new label, empty state, and error
- prefer `gpui-component` controls and layout primitives before building custom widgets

Definition of done:

- request edits mutate hot editor state instead of mutating catalog values directly
- UI can represent idle, dirty, sending, completed, failed, and cancelled states
- no raw user-facing strings are introduced

## Slice 3: Save, Duplicate, and Draft Ownership

Purpose: make request editing safe and recoverable before adding execution.

Tasks:

- wire save action to persist the draft request value through the repo layer
- write request body payloads to the blob store on save when needed
- resolve auth secrets through `SecretManager` and persist only secret refs
- wire duplicate action to create a new request from the current draft/baseline safely
- define how request-tab identity behaves for unsaved new requests:
  - preferred: request-draft tab identity for unsaved new requests
  - fallback: immediate persisted stub request creation
- refresh sidebar/catalog state after successful save or duplicate

Definition of done:

- save updates the persisted request and resets dirty state to the new baseline
- duplicate opens a distinct request tab without corrupting the source request
- save conflicts are surfaced as recoverable errors, not silent last-write-wins overwrites

## Slice 4: REST Execution Service and Cancellation

Purpose: implement send/cancel safely through shared services instead of ad hoc tab-local networking.

Tasks:

- introduce `RequestExecutionService` backed by the Tokio runtime and `reqwest`
- convert the current draft request value into a sendable HTTP request without depending on UI types
- create a pending history row before the network operation starts
- store the active task handle and cancellation primitive on the editor entity
- propagate cancel by signalling the active operation and dropping/aborting the in-flight request future
- map completion into `Completed | Failed | Cancelled` only
- handle dropped window/entity/app targets after async boundaries as no-op outcomes with logs

Definition of done:

- send from a dirty or clean draft works
- cancel transitions to `Cancelled` deterministically
- a late successful response after cancel does not overwrite cancelled UI state
- window close during in-flight request does not panic

## Slice 5: Response Persistence and Latest-Run Summary

Purpose: make responses durable and bounded.

Tasks:

- capture response metadata:
  - status
  - headers
  - media type
  - size
  - timings
- keep only a bounded preview in hot state
- persist the full response body through the existing blob store
- finalize the history row with response metadata and blob refs
- add request-tab latest-run summary loading from history + blob preview on reopen
- add "load full body from disk" behavior for the request tab response panel

Definition of done:

- large responses do not stay fully resident in hot memory by default
- reopening a request can show the latest-run summary without resending
- full response bodies can be reopened from disk on demand

## Slice 6: Root/Session Integration and Restore Semantics

Purpose: keep request tabs coherent with the Phase 2 tab/session model.

Tasks:

- update `AppRoot.request_pages` ownership so it can manage request editor entities rather than static placeholder views
- support whichever request-tab identity strategy Slice 3 chooses
- ensure close/delete cleanup drops request editor entities intentionally
- on tab restore, rebuild request editor state from persisted request data plus latest-run summary
- do not treat dirty request drafts as durable session state unless explicitly implemented and tested

Definition of done:

- saved requests reopen with persisted editor data and latest-run summary
- restoring a missing/deleted request still degrades gracefully to the existing empty state
- no entity reentrancy or accidental task-drop regressions are introduced

## 9. Validation Gates

Phase 3 should not be considered complete without the following coverage.

Unit tests:

- request repo save/duplicate roundtrip
- revision-conflict detection
- lifecycle FSM transitions including cancel races
- response truncation/body-ref behavior

Integration tests:

- send/cancel race with delayed completion
- request save followed by reopen from persisted state
- duplicate request then reopen both source and duplicate tabs
- latest-run summary restore from history/blob store
- window close during in-flight request with no panic

Performance tests:

- 10 MB, 50 MB, and 200 MB response handling on the bounded preview path
- multiple open request tabs without unbounded response-memory growth

Security tests:

- auth secrets never persist in request rows, history rows, or blob files
- log/error paths do not emit raw auth values
- secret-store lookup failure produces a recoverable error path

## 10. Exit Criteria Mapping Back to `docs/plan.md`

The Phase 3 goals from the main plan are satisfied only when:

- REST requests can be created or duplicated into a usable tab, edited, saved, sent, cancelled, and reopened safely
- request-tab state obeys the explicit lifecycle FSM
- response previews respect configured memory caps
- full response bodies are reopenable from disk
- cancelled requests never re-enter completed UI state from late responses

That is the minimum bar before moving on to Phase 4 tree CRUD and environment resolution work.
