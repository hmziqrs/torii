# Torii Phase 5 Implementation Plan

> Derived from `docs/plan.md` Phase 5
> Constrained by `docs/gpui-architecture.md` and `docs/gpui-performance.md`
> Builds on `docs/completed/phase-4.md`
> Date: 2026-04-26

## 1. Objective

Expand Torii from a REST-focused API client into a history-first multi-protocol client without weakening the V2 state architecture.

Phase 5 has two jobs:

- make history a real, scalable workspace surface instead of a small eager list
- add GraphQL, WebSocket, and gRPC support through shared request tab, lifecycle, history, blob, cancellation, telemetry, and restore contracts

The important product behavior is that every protocol run leaves a useful history trail, and every history row can be inspected or restored without keeping large bodies, transcripts, or message buffers in hot GPUI state.

Phase 5 is complete only when:

- global history is virtualized, filterable, searchable, grouped, inspectable, and usable at large row counts
- per-request history is a first-class request-tab surface, not a fixed-size modal
- restoring from history never dead-ends when the original request was deleted
- GraphQL runs through the existing HTTP execution core while getting GraphQL-specific editor affordances
- WebSocket and gRPC streaming use bounded queues, bounded visible buffers, batch-notify rendering, and disk-backed transcripts
- protocol-specific UI reuses the unified tab/session/request/history model rather than growing separate app islands

## 1.1 Implementation Order

The correct order inside this phase is:

1. History data/query/restore foundation
2. Virtualized global and per-request history UI
3. Protocol domain model and request-editor routing
4. GraphQL editor and execution on the shared HTTP core
5. WebSocket lifecycle, transcript writer, and visible ring buffer
6. gRPC unary with dynamic descriptors
7. gRPC streaming once unary and stream transcript infrastructure are stable
8. Cross-protocol polish, observability, performance gates, and security audit

Do not start with WebSocket or gRPC UI. Those features need the history, transcript, bounded-buffer, and batch-render infrastructure first.

## 2. Non-Negotiable Rules

The cross-cutting standards in `docs/plan.md`, `docs/gpui-architecture.md`, and `docs/gpui-performance.md` apply directly to this phase.

History rules:

- No large history surface may render from an eager `Vec` of all rows.
- No history query may run from `render()`.
- Filtering, search, grouping, and pagination are event-driven and owned by a history view state entity.
- Global history uses cursor pagination and a virtualized row delegate.
- Per-request history uses the same query path as global history with an added `request_id` filter.
- Response bodies and stream transcripts stay in blobs. History rows reference them by hash/ID.
- Details panes load previews on demand and obey `ResponseBudgets::PREVIEW_CAP_BYTES`.

Streaming rules:

- WebSocket and gRPC streams use bounded network-to-UI channels.
- Visible stream state is a fixed-size ring buffer, not an unbounded `Vec`.
- Full transcripts spill to disk incrementally.
- UI notification is batched by time and/or message count. Never notify once per inbound message.
- Stream state tracks aggregate counters separately from visible rows:
  - total inbound messages
  - total outbound messages
  - visible dropped count
  - transcript bytes written
  - decode failures
  - close/error reason

Lifecycle rules:

- Every protocol run has an operation ID.
- Every protocol run has a cancellation primitive.
- Every protocol run has terminal states.
- Late events are dropped by operation ID, not by checking entity liveness alone.
- Closing a tab, switching workspaces, cancelling, disconnecting, and dropping a window are normal paths.
- Protocol-specific code must finalize history exactly once.

Security rules:

- No resolved managed secret values in SQLite, blobs, logs, exports, telemetry, transcripts, or crash output.
- History restore snapshots may store request structure and opaque secret refs, but not resolved secret values.
- WebSocket and gRPC transcripts must redact configured secret-bearing headers/metadata before persistence.
- Request snapshots must preserve enough non-secret structure to restore a draft while clearly marking missing or redacted secret values.

GPUI render rules:

- `render()` remains a pure projection:
  - no `cx.notify()`
  - no repo reads
  - no blob reads
  - no `cx.spawn()`
  - no `cx.subscribe()` or `cx.observe()`
  - no entity updates that can emit events
- Table/delegate row data is pushed from event handlers, async completions, or dirty-flag guarded one-time updates.
- Search inputs debounce before query execution.
- Row selection changes load details through an explicit action, not by doing IO in row render.

## 3. Current Repo Starting Point

What already exists:

- `history_index` persists HTTP request runs.
- `HistoryRepository` supports:
  - `create_pending`
  - `finalize_completed`
  - `mark_failed`
  - `finalize_cancelled`
  - `list_recent(workspace_id, limit)`
  - `list_for_request(request_id, limit)`
  - `get_latest_for_request(request_id)`
  - `referenced_blob_hashes`
- `RequestExecutionService` creates pending history rows, streams HTTP response bodies, spills large bodies to blobs, and finalizes history.
- `ResponseSummary` already carries metadata needed by the response panel:
  - status
  - headers
  - media type
  - body ref
  - timing fields
  - size fields
  - protocol/tls metadata through `ResponseMetaV2`
- Request tabs restore the latest completed/failed/cancelled response from history on open.
- A global history tab exists as `ItemKind::History`.
- A per-request history dialog exists and can restore a response snapshot into the current request tab.
- `BodyRef` and `ResponseBudgets` already enforce preview and per-tab memory limits for HTTP responses.
- The sidebar can open the History tab for the selected workspace.
- `views/http_method.rs` has a `RequestProtocol` badge type for HTTP, WS, and gRPC, but it is inferred from method sentinel strings.

What is not ready for Phase 5:

- Global history currently loads a fixed limit of 200 rows into `AppRoot`.
- Global history filtering happens by cloning rows in `history_tab::render()`.
- Global history rows render as eager cards, not a virtualized table.
- History search does not exist.
- Grouping does not exist.
- History details are minimal and do not reuse the response panel as an inspectable read-only surface.
- Per-request history is a fixed-size modal limited to 50 rows.
- Global history restore fails when `request_id` is `NULL` or the original request no longer exists.
- `history_index.request_id` uses `ON DELETE SET NULL`, so deleted-source restore cannot rely on the request row.
- The current request snapshot is intentionally redacted and summary-only. It is not sufficient to reconstruct a draft request.
- There is no protocol kind field on `RequestItem`.
- GraphQL has no editor model.
- WebSocket has no transport, tab state, transcript model, or lifecycle state.
- gRPC has no descriptor/source model, dynamic message encoding, transport, or transcript model.
- History blob cleanup only considers `history_index.blob_hash`; Phase 5 will introduce additional blob references.

## 4. Phase 5 Deliverables

Phase 5 is complete only when all of the following exist.

History:

- cursor-paginated `HistoryRepository::query` API
- typed `HistoryQuery`, `HistoryCursor`, `HistoryPage`, and `HistorySort`
- indexed filters for workspace, request, protocol, state, status range, method, URL/search text, and time range
- global History tab rendered through a virtualized delegate/table
- per-request History panel using the same query model
- details pane that can inspect response metadata, headers, cookies, timing, body preview, errors, and stream summaries
- grouped history display by date, request, status family, protocol, or collection
- explicit refresh, clear search, and filter reset actions
- history restore service that:
  - opens/focuses the original request if it still exists
  - creates a new draft from the stored snapshot if the original request is deleted
  - restores completed/failed/cancelled response state where available
  - never fails solely because the source request row was deleted
- response compare for two history entries of the same request/protocol:
  - status/timing/header/body-summary diff in Phase 5
  - full body diff only for bounded text bodies
- retention and cleanup hooks for history rows and all referenced blobs

Protocol foundation:

- persisted protocol kind on requests instead of method sentinel inference
- protocol-specific config stored as versioned JSON
- shared protocol execution interface for request tabs
- shared operation lifecycle state for HTTP, GraphQL, WebSocket, and gRPC
- shared protocol history finalization path
- protocol badge/title rendering from persisted protocol kind
- migration path for existing HTTP requests

GraphQL:

- GraphQL request type over the existing HTTP core
- endpoint, method preference, query, variables, operation name, and headers/auth reuse
- structured query editor area
- JSON variables editor with validation
- operation picker derived from parsed query operations
- GraphQL request serialization for POST and optional GET
- response rendering through the existing HTTP response panel
- history rows that identify protocol kind as `graphql`

WebSocket:

- WebSocket request type and editor
- connect/disconnect lifecycle
- text and binary message send
- bounded inbound/outbound queues
- bounded visible message ring buffer
- transcript spill-to-disk
- stream summary in history
- restore transcript preview from history
- cancel/close behavior that finalizes history once

gRPC:

- gRPC request type and editor
- schema source model for reflection and descriptor/proto inputs
- service/method picker
- metadata editor reusing key-value editor patterns
- unary request/response first
- server-streaming and bidirectional streaming after unary is stable
- bounded decode/render queues
- disk-backed transcript/body archive
- history rows that identify protocol kind as `grpc`

Tests and validation:

- repository query pagination/filter tests
- history restore tests for existing, deleted, and missing collection cases
- request snapshot round-trip tests
- virtualized history row model tests
- GraphQL serialization and operation picker tests
- WebSocket ring buffer, transcript, cancellation, and history finalization tests
- gRPC descriptor, unary, streaming buffer, cancellation, and transcript tests
- performance tests for large history and high-throughput streams
- security tests for snapshot/transcript redaction

## 5. Scope Boundary

Included in Phase 5:

- history list virtualization
- history filtering, search, grouping, details, and restore
- history-backed response restore for deleted requests
- GraphQL editor and execution over HTTP
- WebSocket connect/send/receive/disconnect
- gRPC unary and streaming basics
- protocol-specific history summaries
- stream transcript persistence
- bounded stream UI
- protocol metrics/tracing

Explicitly deferred:

- scripts/tests execution engine
- OAuth 2.0, AWS Signature, mTLS setup UI, and advanced auth flows
- GraphQL schema explorer, schema docs, and autocomplete driven by introspection
- full visual body diff for large disk-backed bodies
- collection import/export changes for protocol-specific request types beyond preserving the new persisted fields
- Git UI and linked-collection reconcile hardening
- mock servers, monitors, cloud sync, team collaboration, and publishing
- gRPC load testing beyond correctness/performance gates needed for local streaming safety

## 6. Data Model and Migration

Add migration:

```text
migrations/0005_phase5_history_protocols.sql
```

### 6.1 Request Protocol Fields

Add protocol fields to `requests`:

```sql
ALTER TABLE requests
ADD COLUMN protocol_kind TEXT NOT NULL DEFAULT 'http';

ALTER TABLE requests
ADD COLUMN protocol_config_json TEXT NOT NULL DEFAULT '{"version":1,"kind":"http"}';
```

Rules:

- Existing rows become `protocol_kind = 'http'`.
- Existing REST fields stay canonical for HTTP requests.
- GraphQL uses `protocol_kind = 'graphql'` but still uses the HTTP transport.
- WebSocket uses `protocol_kind = 'websocket'`.
- gRPC uses `protocol_kind = 'grpc'`.
- The old `RequestProtocol::from_method()` sentinel behavior remains only as a compatibility fallback.
- New UI must not encode WS/gRPC by setting `method = 'WS'` or `method = 'GRPC'`.

Add indexes:

```sql
CREATE INDEX IF NOT EXISTS idx_requests_protocol_kind
ON requests (protocol_kind);
```

### 6.2 History Index Extensions

Extend `history_index` so all protocol runs can share one indexed history surface:

```sql
ALTER TABLE history_index
ADD COLUMN protocol_kind TEXT NOT NULL DEFAULT 'http';

ALTER TABLE history_index
ADD COLUMN request_name TEXT;

ALTER TABLE history_index
ADD COLUMN request_collection_id TEXT;

ALTER TABLE history_index
ADD COLUMN request_parent_folder_id TEXT;

ALTER TABLE history_index
ADD COLUMN request_snapshot_json TEXT;

ALTER TABLE history_index
ADD COLUMN request_snapshot_blob_hash TEXT;

ALTER TABLE history_index
ADD COLUMN run_summary_json TEXT;

ALTER TABLE history_index
ADD COLUMN transcript_blob_hash TEXT;

ALTER TABLE history_index
ADD COLUMN transcript_size INTEGER;

ALTER TABLE history_index
ADD COLUMN message_count_in INTEGER;

ALTER TABLE history_index
ADD COLUMN message_count_out INTEGER;

ALTER TABLE history_index
ADD COLUMN close_reason TEXT;
```

Add indexes:

```sql
CREATE INDEX IF NOT EXISTS idx_history_workspace_protocol_started
ON history_index (workspace_id, protocol_kind, started_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_history_workspace_state_started
ON history_index (workspace_id, state, started_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_history_request_started
ON history_index (request_id, started_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_history_workspace_status_started
ON history_index (workspace_id, status_code, started_at DESC, id DESC);
```

Search implementation:

- First implementation may use bounded `LIKE` search over `method`, `url`, `request_name`, and `error_message` after indexed workspace/time/protocol/state narrowing.
- If large-history performance misses the gate, add a SQLite FTS table in the same phase:

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS history_search_fts
USING fts5(history_id UNINDEXED, workspace_id UNINDEXED, method, url, request_name, error_message);
```

Do not add FTS unless the bundled SQLite build and migration test confirm support.

### 6.3 Stream Transcript References

`history_index.transcript_blob_hash` references the full persisted transcript artifact. It is separate from `blob_hash`, which remains the HTTP/GraphQL response-body artifact.

Blob cleanup must treat all of these as live references:

- `history_index.blob_hash`
- `history_index.request_snapshot_blob_hash`
- `history_index.transcript_blob_hash`
- request body blobs referenced by restored request snapshots

Update `HistoryRepository::referenced_blob_hashes()` or split it into a `BlobReferenceRepository` so startup recovery cannot delete valid Phase 5 artifacts.

### 6.4 Request Snapshot Format

History restore requires a secret-safe snapshot that can rebuild a draft request.

Add a versioned JSON shape:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryRequestSnapshotV1 {
    pub version: u16,
    pub protocol_kind: RequestProtocolKind,
    pub request_name: String,
    pub collection_id: Option<CollectionId>,
    pub parent_folder_id: Option<FolderId>,
    pub method: String,
    pub url: String,
    pub params: Vec<KeyValuePair>,
    pub headers: Vec<KeyValuePair>,
    pub auth: AuthType,
    pub body: HistorySnapshotBody,
    pub scripts: ScriptsContent,
    pub settings: RequestSettings,
    pub variable_overrides_json: String,
    pub protocol_config: ProtocolConfig,
    pub redaction_warnings: Vec<SnapshotRedactionWarning>,
}
```

Rules:

- Store user-authored request structure before resolving environment variables or secret values.
- Store opaque secret refs where the request model already stores refs.
- Never store resolved Basic/Bearer/API-key secret values.
- Do not inline large bodies in `request_snapshot_json`.
- Body snapshots may reference a blob hash when the body is large or file-backed.
- If a body artifact cannot be retained, the snapshot must mark the body as missing and restore the draft with a visible missing-body state.
- Keep the existing redacted summary fields for list display and backwards compatibility.

The snapshot must be good enough to create a useful draft even after `request_id` becomes `NULL`.

## 7. Domain Model

Add `src/domain/protocol.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestProtocolKind {
    Http,
    Graphql,
    WebSocket,
    Grpc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ProtocolConfig {
    Http { version: u16 },
    Graphql(GraphqlConfig),
    WebSocket(WebSocketConfig),
    Grpc(GrpcConfig),
}
```

Add protocol-specific model files:

```text
src/domain/graphql.rs
src/domain/websocket.rs
src/domain/grpc.rs
src/domain/stream.rs
```

GraphQL model:

```rust
pub struct GraphqlConfig {
    pub version: u16,
    pub query: String,
    pub variables_json: String,
    pub operation_name: Option<String>,
    pub use_get_for_queries: bool,
}
```

WebSocket model:

```rust
pub struct WebSocketConfig {
    pub version: u16,
    pub url: String,
    pub protocols: Vec<String>,
    pub headers: Vec<KeyValuePair>,
    pub initial_message: Option<StreamMessageDraft>,
}
```

gRPC model:

```rust
pub struct GrpcConfig {
    pub version: u16,
    pub endpoint: String,
    pub service: Option<String>,
    pub method: Option<String>,
    pub schema_source: GrpcSchemaSource,
    pub metadata: Vec<KeyValuePair>,
    pub message_json: String,
    pub streaming_mode: GrpcStreamingMode,
}
```

Stream model:

```rust
pub struct StreamMessage {
    pub id: u64,
    pub direction: StreamDirection,
    pub at_unix_ms: i64,
    pub opcode_or_type: String,
    pub preview: String,
    pub size_bytes: u64,
    pub truncated: bool,
}

pub struct StreamSummary {
    pub total_inbound: u64,
    pub total_outbound: u64,
    pub visible_dropped: u64,
    pub transcript_blob_hash: Option<String>,
    pub transcript_size: u64,
    pub close_reason: Option<String>,
}
```

Add history query types:

```rust
pub struct HistoryQuery {
    pub workspace_id: WorkspaceId,
    pub request_id: Option<RequestId>,
    pub protocol: Option<RequestProtocolKind>,
    pub state: Option<HistoryState>,
    pub status_family: Option<StatusFamily>,
    pub search: Option<String>,
    pub started_after: Option<i64>,
    pub started_before: Option<i64>,
    pub cursor: Option<HistoryCursor>,
    pub limit: usize,
    pub sort: HistorySort,
}

pub struct HistoryPage {
    pub rows: Vec<HistoryEntry>,
    pub next_cursor: Option<HistoryCursor>,
    pub total_estimate: Option<u64>,
}
```

`HistoryQuery.limit` must be clamped by the repository. UI callers do not control unbounded row counts.

## 8. Repository and Service Contracts

### 8.1 HistoryRepository

Replace the current list-only API with query primitives while keeping compatibility wrappers during migration:

```rust
pub trait HistoryRepository: Send + Sync {
    fn create_pending(&self, input: CreateHistoryRun) -> RepoResult<HistoryEntry>;
    fn finalize_completed(&self, input: FinalizeCompletedRun) -> RepoResult<()>;
    fn finalize_failed(&self, input: FinalizeFailedRun) -> RepoResult<()>;
    fn finalize_cancelled(&self, input: FinalizeCancelledRun) -> RepoResult<()>;
    fn finalize_stream(&self, input: FinalizeStreamRun) -> RepoResult<()>;
    fn query(&self, query: HistoryQuery) -> RepoResult<HistoryPage>;
    fn get(&self, id: HistoryEntryId) -> RepoResult<Option<HistoryEntry>>;
    fn get_latest_for_request(&self, request_id: RequestId) -> RepoResult<Option<HistoryEntry>>;
    fn referenced_blob_hashes(&self) -> RepoResult<HashSet<String>>;
}
```

Cursor ordering:

- primary sort: `started_at DESC`
- tie-breaker: `id DESC`
- cursor payload: `(started_at, id)`
- query condition for next page:
  - `started_at < cursor.started_at`
  - or `started_at = cursor.started_at AND id < cursor.id`

This prevents duplicates and missing rows when several requests start in the same second.

### 8.2 HistoryService

Add `src/services/history.rs` for orchestration:

- builds secret-safe snapshots
- creates pending history rows for all protocols
- restores history rows into request tabs
- resolves restore destination when the original collection/folder was deleted
- loads response/transcript previews for details panes
- owns retention/delete-history operations

Restore destination rules:

1. If the original request still exists, open/focus it and restore the selected history response state.
2. If the original request is deleted but the snapshot collection still exists, create an unsaved draft in that collection. Use the original parent folder only if it still exists.
3. If the snapshot collection is gone but the selected workspace has a selected collection, create the draft there.
4. If no usable collection exists, create a managed collection named `Restored History` in the workspace and create the draft at its root.

The restore action must report any missing body artifacts or missing secret refs through a notification and a visible warning in the restored draft.

### 8.3 ProtocolExecutionService

Keep the current HTTP execution path but introduce a protocol dispatcher:

```rust
#[async_trait::async_trait]
pub trait ProtocolExecutor: Send + Sync {
    async fn execute(
        &self,
        input: ProtocolExecutionInput,
        events: ProtocolEventSink,
        cancel: CancellationToken,
    ) -> anyhow::Result<ProtocolExecutionOutcome>;
}
```

Executor implementations:

- `HttpExecutor`
- `GraphqlExecutor`
- `WebSocketExecutor`
- `GrpcExecutor`

The dispatcher should live in `src/services/protocol_execution.rs` and compose existing services:

- `RequestExecutionService` for HTTP and GraphQL
- `StreamTranscriptWriter` for WebSocket and gRPC streams
- `HistoryService` for pending/final history rows
- `SecretManager`/`SecretStoreRef` for auth resolution
- `VariableResolutionService` for Phase 4 variables

## 9. Proposed Module Layout

```text
src/
  domain/
    protocol.rs
    graphql.rs
    websocket.rs
    grpc.rs
    stream.rs
    history.rs                 # extend existing models with query/snapshot types

  repos/
    history_repo.rs            # query API, snapshot fields, stream finalization
    request_repo.rs            # protocol_kind/protocol_config_json persistence

  services/
    history.rs                 # restore/query/detail orchestration
    protocol_execution.rs      # shared dispatcher and executor trait
    graphql_execution.rs       # GraphQL -> HTTP request builder
    websocket_execution.rs     # WS lifecycle and network loop
    grpc_execution.rs          # gRPC unary/stream execution
    stream_transcript.rs       # chunked transcript writer/reader
    stream_buffer.rs           # bounded ring buffer helpers

  views/
    item_tabs/
      history_tab.rs           # module entry, delegates below
      history_tab/
        delegate.rs            # virtualized row/table delegate
        filters.rs             # search/filter/group controls
        details.rs             # response/transcript details pane
        restore.rs             # restore UI affordances

      request_tab/
        protocol_selector.rs
        protocol_editor.rs
        graphql_editor.rs
        websocket_editor.rs
        grpc_editor.rs
        history_panel.rs       # replaces fixed-size per-request history modal
        stream_panel.rs        # visible ring buffer renderer

tests/
  history_query.rs
  history_restore.rs
  history_snapshot.rs
  history_virtual_rows.rs
  graphql_execution.rs
  websocket_streaming.rs
  grpc_unary.rs
  grpc_streaming.rs
  stream_transcript.rs
  phase5_large_history.rs
```

Keep protocol-specific network code in `services/`. Views own presentation and editor state only.

## 10. Slice Plan

### Slice 1: History Query and Schema Foundation

Tasks:

- add `0005_phase5_history_protocols.sql`
- add request protocol persistence
- extend `HistoryEntry`
- add `HistoryQuery`, `HistoryCursor`, `HistoryPage`
- implement cursor-paginated `HistoryRepository::query`
- preserve `list_recent` and `list_for_request` temporarily as wrappers
- update startup recovery blob reference collection
- add indexes and migration tests

Acceptance:

- query returns stable cursor pages with no duplicate rows
- query clamps `limit`
- workspace filter is mandatory
- request filter uses `idx_history_request_started`
- protocol/state/status filters are covered by indexes
- migration round-trip passes from old DB state
- recovery does not delete transcript or snapshot blobs

Tests:

- `history_query_cursor_is_stable_with_same_started_at`
- `history_query_filters_by_workspace_protocol_state_status`
- `history_query_clamps_limit`
- `migration_0005_adds_protocol_history_fields`
- `recovery_preserves_phase5_history_blob_refs`

### Slice 2: Virtualized Global History UI

Tasks:

- replace eager card rendering with a history view state entity
- add virtualized row delegate using `gpui-component` table/list primitives
- move filtering/search/group state out of `AppRoot`
- debounce search input
- fetch next pages explicitly when scroll nears end
- add details pane with lazy preview load
- add grouped display modes:
  - date
  - request
  - protocol
  - status family
  - collection
- add empty, loading, error, and no-results states

Acceptance:

- opening History does not load more than the first page
- changing filters does not query from `render()`
- search debounce avoids per-keystroke DB churn
- 10,000 generated rows remain scrollable
- row selection loads details asynchronously and tolerates dropped window/entity
- response body preview uses existing budget constants

Tests:

- row projection unit tests
- filter state transition tests
- query debounce tests where practical
- large history performance smoke test

### Slice 3: Per-Request History Panel and Restore

Tasks:

- replace or augment the current fixed-size modal with `request_tab/history_panel.rs`
- reuse `HistoryQuery { request_id: Some(...) }`
- add compare selection for two rows
- add `HistoryService::restore_entry`
- capture versioned request snapshots for new runs
- support deleted-source restore into a draft request
- expose restore warnings in the draft tab
- ensure restored draft preserves protocol kind and protocol config

Acceptance:

- restoring an existing-source row focuses the existing request tab
- restoring a deleted-source row creates a draft request from snapshot
- restoring after collection deletion uses deterministic destination rules
- pending rows cannot be restored as completed responses
- missing body blob produces a visible warning, not a panic
- secret refs are not resolved or copied as plaintext
- compare works for bounded text responses and metadata-only fallback works for large/disk bodies

Tests:

- `history_restore_existing_request_focuses_request`
- `history_restore_deleted_request_creates_draft`
- `history_restore_missing_collection_creates_restored_history_collection`
- `history_restore_missing_body_blob_marks_warning`
- `history_snapshot_never_contains_secret_values`

### Slice 4: Protocol Domain and Request Editor Routing

Tasks:

- add `RequestProtocolKind` and `ProtocolConfig`
- persist protocol fields in `RequestRepository`
- update `RequestItem::new` defaults
- update linked collection format serialization for protocol fields
- update sidebar/breadcrumb/history badges to read `protocol_kind`
- add protocol selector to request tab
- split protocol-specific editor panels behind a shared request tab shell
- make save/send/dirty-state protocol-aware
- keep existing HTTP editor behavior unchanged for `protocol_kind = Http`

Acceptance:

- existing HTTP tests pass without changes to behavior
- selecting a protocol updates draft state and dirty state once
- saved protocol fields round-trip through SQLite
- linked collection round-trip preserves protocol fields
- tab title and breadcrumbs remain stable
- method sentinel fallback is read-only compatibility, not the new write path

Tests:

- `request_protocol_defaults_to_http`
- `request_protocol_config_roundtrip`
- `linked_collection_preserves_protocol_config`
- `protocol_switch_marks_draft_dirty_once`

### Slice 5: GraphQL over HTTP

Tasks:

- add GraphQL config model
- add parser-backed operation extraction
- add query editor area
- add variables JSON editor with validation and formatting
- add operation picker
- build GraphQL HTTP request:
  - default POST body: `{ "query": ..., "variables": ..., "operationName": ... }`
  - content type: `application/json`
  - optional GET for query operations when enabled
- run through existing HTTP request execution and response panel
- persist GraphQL protocol history rows

Acceptance:

- invalid variables JSON blocks send with preflight error
- operation picker lists named query/mutation/subscription operations
- selected operation name is included in the request body
- headers/auth/environment variable resolution reuse existing HTTP behavior
- response panel and history restore work exactly like HTTP
- GraphQL request history identifies protocol as GraphQL

Tests:

- `graphql_operation_picker_extracts_named_operations`
- `graphql_variables_validation_rejects_invalid_json`
- `graphql_post_body_serialization`
- `graphql_uses_http_history_and_response_restore`

### Slice 6: WebSocket Lifecycle and Transcript

Tasks:

- add WebSocket transport dependency after version/license check
- add `WebSocketExecutor`
- add `WebSocketSessionState`
- add connect/disconnect/send actions
- add bounded inbound and outbound channels
- add visible ring buffer helper
- add transcript writer with chunked blob output
- add stream panel for visible messages
- add message composer for text and binary payloads
- redact handshake headers before history persistence
- finalize history on normal close, error, cancel, and tab close

Suggested constants:

```rust
pub struct StreamBudgets;
impl StreamBudgets {
    pub const VISIBLE_MESSAGE_CAP: usize = 1_000;
    pub const CHANNEL_CAP: usize = 256;
    pub const FLUSH_INTERVAL_MS: u64 = 33;
    pub const FLUSH_MESSAGE_CAP: usize = 64;
    pub const TRANSCRIPT_CHUNK_BYTES: usize = 1024 * 1024;
    pub const MESSAGE_PREVIEW_BYTES: usize = 16 * 1024;
}
```

Acceptance:

- sustained inbound messages do not grow memory without bound
- visible dropped count increments when ring overwrites old rows
- transcript persists full message records until cancellation/close
- UI notifications are batched
- disconnect is idempotent
- tab close cancels and finalizes once
- reconnect starts a new history row

Tests:

- `stream_ring_buffer_drops_oldest_and_counts`
- `websocket_transcript_writer_roundtrip`
- `websocket_cancel_finalizes_history_once`
- `websocket_batch_flush_limits_notify_frequency`

### Slice 7: gRPC Unary

Tasks:

- add gRPC dependencies after version/license check:
  - tonic transport
  - prost/prost-reflect for descriptors and dynamic messages
  - tonic reflection support if used
- add schema source model:
  - server reflection
  - descriptor set file/blob
  - proto file paths if feasible
- add service/method picker from descriptors
- add metadata editor
- add JSON message editor
- encode JSON to dynamic protobuf message
- execute unary call
- decode response to JSON for display
- persist response body through existing blob path or gRPC-specific body summary

Acceptance:

- descriptor load errors are classified as preflight failures
- missing service/method blocks send before network call
- unary response can be viewed as JSON
- metadata values go through secret-safe redaction before history persistence
- cancellation works before and during call
- history restore rehydrates endpoint, method, metadata, message JSON, and descriptor source reference

Tests:

- `grpc_descriptor_load_lists_services`
- `grpc_unary_json_encode_decode_roundtrip`
- `grpc_unary_cancel_marks_history_cancelled`
- `grpc_history_snapshot_redacts_metadata`

### Slice 8: gRPC Streaming

Tasks:

- extend `GrpcExecutor` for server streaming
- add client-streaming and bidi only after server streaming is stable
- use the same stream budgets and transcript writer as WebSocket
- decode each message into a visible preview plus transcript record
- add send queue for client/bidi streams
- batch UI updates
- finalize history with message counts and transcript ref

Acceptance:

- server streaming cannot allocate unbounded decoded messages
- bidi send queue is bounded and reports backpressure
- decode failures are visible per message and counted
- transcript restore can show summary and preview rows
- cancellation closes network stream and finalizes history once

Tests:

- `grpc_server_stream_uses_bounded_ring`
- `grpc_bidi_send_queue_backpressure`
- `grpc_stream_decode_error_is_recorded`
- `grpc_stream_transcript_restore_preview`

### Slice 9: Cross-Protocol Polish and Release Gates

Tasks:

- unify protocol badges in sidebar, breadcrumbs, history, and tab chrome
- add history details for stream transcripts
- add retention cleanup UI or service hook
- add structured telemetry
- add security/redaction audit
- run performance gates
- update docs and plan closeout notes

Acceptance:

- all protocol runs appear in global history
- per-request history works for all protocol kinds
- history restore works for all protocol kinds
- memory plateaus under configured caps during stream tests
- no raw UI strings added
- all new strings exist in `i18n/en/torii.ftl` and `i18n/zh-CN/torii.ftl`

## 11. UI Contracts

### 11.1 Global History

Layout:

- top toolbar:
  - search input
  - protocol segmented control
  - state/status filters
  - date range
  - grouping menu
  - refresh action
- main area:
  - virtualized history rows
  - details pane for selected row
- row content:
  - protocol badge
  - method or stream action
  - request name
  - URL/endpoint
  - status/state
  - duration
  - started time
  - size/message count summary

Do not render a large card list. Dense table/list presentation is the right fit for an operational history surface.

### 11.2 Per-Request History

Per-request history should become a panel or tab within the request editor surface, not only a modal.

Required actions:

- restore selected run
- compare two selected runs
- open in global history
- copy URL/endpoint
- save response/transcript artifact where applicable

### 11.3 Protocol Editor

The request tab shell remains shared:

- title/dirty state
- collection/folder ownership
- save/send/cancel shortcuts
- environment selector usage
- response/history panels

Protocol-specific panels are swapped inside the editor body:

- HTTP: existing Params/Auth/Headers/Body/Scripts/Tests sections
- GraphQL: Endpoint/Auth/Headers/Query/Variables
- WebSocket: URL/Auth/Headers/Messages/Transcript
- gRPC: Endpoint/Metadata/Schema/Message/Transcript

The UI must not describe how the feature works in visible instructional text. Use labels, placeholders, tooltips, empty states, and validation messages.

## 12. Persistence and Restore Details

### 12.1 Pending History Creation

Create pending history before the network operation starts for every protocol.

Required fields:

- workspace id
- request id if persisted
- protocol kind
- method/action label
- URL/endpoint
- request name
- collection id
- folder id
- secret-safe request snapshot
- redacted display fields

For draft requests, `request_id` may be `None`, but snapshot must still be present.

### 12.2 Finalization

Each operation must finalize exactly once.

Completion fields:

- HTTP/GraphQL:
  - status code
  - response body blob
  - headers JSON
  - media type
  - response meta JSON
- WebSocket/gRPC streaming:
  - transcript blob hash
  - transcript size
  - inbound/outbound counts
  - close reason
  - run summary JSON
- Failed:
  - classified error summary where available
  - terminal state
- Cancelled:
  - partial size/transcript info where available
  - terminal state

Finalizers should be idempotent where practical:

- no panic if the row is already terminal
- log duplicate finalization as a warning
- do not overwrite a completed terminal state with cancelled from a late cancel path

### 12.3 Restore

History restore is a service operation, not view logic.

Restore output:

```rust
pub enum HistoryRestoreOutcome {
    FocusedExistingRequest { request_id: RequestId },
    OpenedDraft { draft_id: RequestDraftId, warnings: Vec<RestoreWarning> },
    RestoredResponseOnly { warnings: Vec<RestoreWarning> },
}
```

Views use this result to open/focus tabs and show warnings. Views do not hand-assemble draft requests from JSON.

## 13. Stream Transcript Format

Use JSON Lines for the first implementation unless profiling proves a binary format is needed:

```json
{"version":1,"seq":1,"direction":"inbound","at_unix_ms":1777200000000,"kind":"text","size_bytes":42,"preview":"...","payload_ref":{"inline_text":"..."}}
{"version":1,"seq":2,"direction":"outbound","at_unix_ms":1777200000100,"kind":"binary","size_bytes":4096,"payload_ref":{"blob_offset":1234,"length":4096}}
```

Rules:

- Small text messages may be inlined if they pass redaction.
- Large/binary messages are written as payload records in the transcript blob.
- Visible rows hold only preview text and metadata.
- Transcript reader supports:
  - first N records
  - last N records
  - record count and byte summary
  - future search hooks

The transcript writer must flush safely on close/cancel and leave either:

- a valid finalized transcript blob
- or a cancelled/failed history row with partial transcript metadata

## 14. Observability

Add spans:

- `history.query`
- `history.restore`
- `history.snapshot.create`
- `history.details.load`
- `protocol.dispatch`
- `graphql.execute`
- `websocket.connect`
- `websocket.flush_batch`
- `websocket.finalize`
- `grpc.descriptor.load`
- `grpc.unary`
- `grpc.stream`
- `stream.transcript.write`
- `stream.transcript.restore`

Add counters:

- `history_query_total`
- `history_restore_total`
- `history_restore_deleted_source_total`
- `history_restore_missing_artifact_total`
- `graphql_send_total`
- `websocket_connect_total`
- `websocket_message_in_total`
- `websocket_message_out_total`
- `websocket_visible_drop_total`
- `grpc_unary_total`
- `grpc_stream_message_in_total`
- `grpc_stream_message_out_total`
- `stream_transcript_bytes_written_total`
- `stream_batch_flush_total`
- `protocol_cancel_total`

Add structured fields where useful:

- workspace id
- protocol kind
- history id
- operation id
- request id when present
- terminal state
- message counts
- transcript bytes

Never log raw headers, metadata, auth values, body payloads, GraphQL variables, WebSocket messages, or gRPC messages.

## 15. Validation Gates

Run these before closing Phase 5:

```bash
cargo test --package torii
cargo test --test history_query
cargo test --test history_restore
cargo test --test history_snapshot
cargo test --test graphql_execution
cargo test --test websocket_streaming
cargo test --test grpc_unary
cargo test --test grpc_streaming
cargo test --test stream_transcript
cargo test --test phase5_large_history
cargo clippy --package torii
```

Performance gates:

- 10,000 history rows:
  - first page query remains bounded
  - filter changes remain responsive
  - opening History does not allocate all rows
- 100,000 history rows:
  - query still uses indexes
  - cursor paging remains stable
- WebSocket high-throughput stream:
  - memory plateaus with ring cap
  - transcript bytes grow on disk, not in hot state
  - notify count is batched
- gRPC server stream:
  - decoded visible messages stay capped
  - transcript restore reads preview pages, not the full transcript
- large response restore:
  - preview loads under `ResponseBudgets::PREVIEW_CAP_BYTES`
  - full body actions stream from blob

Security gates:

- no resolved managed secret values in `history_index`
- no resolved managed secret values in transcript blobs
- no resolved managed secret values in logs
- request snapshot JSON stores refs or redacted placeholders only
- restore warnings appear for missing secret refs and missing body artifacts

Manual smoke flows:

- send HTTP request, inspect in global history, restore latest response
- delete the request, restore its history row into a draft
- run GraphQL query with variables and restore it
- connect to WebSocket echo endpoint, send messages, disconnect, inspect transcript summary
- run gRPC unary call from descriptor/reflection and inspect response
- run gRPC server stream long enough to exercise ring overwrite
- close tabs/windows during active operations and verify terminal history states

## 16. Compatibility and Rollout

Compatibility rules:

- Existing HTTP requests remain valid.
- Existing history rows with missing `protocol_kind` read as HTTP.
- Existing rows without request snapshots can restore response state but cannot reconstruct deleted request input. The UI should say the request snapshot is unavailable instead of failing generically.
- Existing method-sentinel protocol badges remain read-only fallback until all rows are migrated.
- New protocol fields are included in linked collection serialization.

Rollout strategy:

1. Land schema and repository changes with compatibility wrappers.
2. Convert history UI to the query model while preserving old send behavior.
3. Add snapshot capture for all new HTTP runs.
4. Implement restore from snapshots.
5. Add protocol fields to requests and save/load paths.
6. Add GraphQL.
7. Add WebSocket.
8. Add gRPC unary.
9. Add gRPC streaming.

This keeps REST usable throughout the phase and avoids coupling protocol work to the history UI rewrite.

## 17. Risks and Mitigations

Risk: history UI accidentally reloads on every request-tab keystroke.

Mitigation:

- history state lives in a dedicated entity
- queries run only from filter/search/page actions
- no broad observer on request tab state

Risk: request snapshots duplicate secrets.

Mitigation:

- snapshot builder operates before secret resolution
- tests scan serialized snapshots for seeded secret values
- transcript writer receives already-redacted metadata for persistence

Risk: WebSocket/gRPC stream rendering saturates GPUI.

Mitigation:

- bounded channels
- visible ring buffers
- time/count batched flush
- no per-message `cx.notify()`

Risk: gRPC dynamic support becomes too large.

Mitigation:

- ship unary with descriptor/reflection first
- keep streaming behind the shared stream infrastructure
- defer schema explorer/autocomplete

Risk: deleted-source restore needs a collection destination.

Mitigation:

- deterministic restore destination rules
- create `Restored History` managed collection only when no usable collection exists
- keep restored request as a draft so the user can choose where to save it later

## 18. Closeout Checklist

- [ ] `docs/plan.md` links to this document
- [ ] `0005_phase5_history_protocols.sql` exists and migration tests pass
- [ ] history query API replaces eager fixed-limit list usage
- [ ] global history is virtualized
- [ ] per-request history uses the same query model
- [ ] deleted-source restore creates a draft from snapshot
- [ ] protocol kind/config persist for requests
- [ ] GraphQL editor and execution are complete
- [ ] WebSocket lifecycle and transcript persistence are complete
- [ ] gRPC unary is complete
- [ ] gRPC streaming is complete or explicitly split into a follow-up with all unary gates satisfied
- [ ] stream buffers are bounded and tested
- [ ] transcript blobs are included in recovery/live-blob accounting
- [ ] all new UI copy is localized in English and Simplified Chinese
- [ ] performance gates pass
- [ ] security/redaction gates pass
- [ ] `cargo test --package torii` passes
- [ ] `cargo clippy --package torii` passes
