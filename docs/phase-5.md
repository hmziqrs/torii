# Torii Phase 5 Implementation Plan

> Derived from `docs/plan.md` Phase 5
> Constrained by `docs/gpui-architecture.md` and `docs/gpui-performance.md`
> Builds on `docs/completed/phase-4.md`
> Date: 2026-04-26

## 0. Progress Status (Updated: 2026-04-30)

### 0.1 Latest Completed History Batch

- [x] Compare Previous button restored in History Details footer.
- [x] Compare Previous now supports linked/file-system rows without `request_id` via workspace/protocol/method/url fallback.
- [x] Global history card row now exposes Compare Previous action directly.
- [x] Compare dialog includes JSON/text/metadata fallback diff payload output.
- [x] Bottom Load More control added to global history surface.
- [x] History list timestamps normalized to local human-readable format.
- [x] History Details timestamps normalized to local human-readable format.
- [x] History Details enriched with transcript metadata, run summary, and request snapshot display.
- [x] Copy Details JSON action added for full entry export to clipboard.
- [x] Request-tab history dialog upgraded: larger fetch window (200), protocol-visible rows, local timestamps, and inline compare action.
- [x] History retention controls wired with in-app prune action and orphan blob cleanup pass.
- [x] History compare fallback handles non-linked/file-system rows via workspace/protocol/method/url matching.
- [x] History details inspector now shows richer response inspection context (status, media type, response size, header preview, timing preview, transcript metadata, run summary, request snapshot, JSON copy).

### 0.2 Still Pending In This Phase

- [x] Virtualized global history row rendering via `uniform_list` (replaces eager full-card list rendering path).
- [x] First-class per-request history panel in request editor section (modal history flow removed).
- [x] History details pane parity updates shipped (dedicated headers/cookies/timing/body preview inspection affordances in details flows).
- [x] Automated retention policy on startup recovery (default 30-day prune) plus manual prune controls.

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
- Startup recovery already treats `requests.body_blob_hash` as a live blob reference in addition to `history_index.blob_hash`.

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
- History/transcript/snapshot blob cleanup needs a normalized way to discover all Phase 5 history-owned blob references without JSON-scanning every history row.

## 4. Phase 5 Deliverables

Phase 5 is complete only when all of the following exist.

History:

- cursor-paginated `HistoryRepository::query` API
- typed `HistoryQuery`, `HistoryCursor`, `HistoryPage`, and `HistorySort`
- indexed filters for workspace, request, collection, protocol, state, status range, method, and time range
- bounded URL/general text search after indexed narrowing, with optional lazy FTS if needed
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
  - status, timing, headers, cookies, size, and body-summary diff
  - JSON-aware structured diff when both bodies are valid JSON and fit within `ResponseBudgets::PREVIEW_CAP_BYTES`
  - unified text diff when both bodies are text-like and fit within `ResponseBudgets::PREVIEW_CAP_BYTES`
  - metadata-only fallback for binary, missing, truncated, or disk-backed bodies that exceed the preview cap
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
- GraphQL subscriptions over WebSocket
- collection import/export changes for protocol-specific request types beyond preserving the new persisted fields
- Git UI and linked-collection reconcile hardening
- mock servers, monitors, cloud sync, team collaboration, and publishing
- gRPC load testing beyond correctness/performance gates needed for local streaming safety
- WebSocket/gRPC stream pause/resume controls

## 5.1 Dependency Families to Evaluate

Candidate dependency families are part of the Phase 5 design, but exact versions must be chosen in the slice that lands each family:

```toml
graphql-parser = "..."       # GraphQL operation parsing for the operation picker
tokio-tungstenite = "..."    # WebSocket client transport on the existing tokio runtime
tonic = "..."                # gRPC transport
prost = "..."                # protobuf message support
prost-reflect = "..."        # dynamic descriptors/messages for user-supplied schemas
tonic-reflection = "..."     # optional server reflection client support
```

Before landing them:

- confirm versions against the current Rust toolchain and dependency graph
- check licenses and transitive dependency size
- keep WebSocket and gRPC dependencies feature-scoped where practical
- add only the dependencies needed by the slice currently being implemented

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
- The repository currently has no writer that creates sentinel-method requests, so the 0005 migration intentionally does not rewrite existing rows based on `method`.

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
CREATE INDEX IF NOT EXISTS idx_history_workspace_started_id
ON history_index (workspace_id, started_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_history_workspace_protocol_started
ON history_index (workspace_id, protocol_kind, started_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_history_workspace_state_started
ON history_index (workspace_id, state, started_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_history_request_started
ON history_index (request_id, started_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_history_workspace_status_started
ON history_index (workspace_id, status_code, started_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_history_workspace_collection_started
ON history_index (workspace_id, request_collection_id, started_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_history_workspace_method_started
ON history_index (workspace_id, method COLLATE NOCASE, started_at DESC, id DESC);
```

Timestamp migration:

- `history_index.started_at`, `completed_at`, `dispatched_at`, `first_byte_at`, and `cancelled_at` must use one unit for all rows before cursor pagination ships.
- The 0005 migration must normalize existing second-precision values in those columns to Unix milliseconds in place.
- New Phase 5 writes must store Unix milliseconds in the same columns.
- The migration must be idempotent by updating only plausible second-precision values, using the same threshold as `normalize_unix_ms`.
- After 0005, query ordering and time filters operate on raw millisecond values; readers may keep normalization helpers only for defensive compatibility.

Index cleanup notes:

- `idx_history_workspace_started` from `0001_initial.sql` is superseded by `idx_history_workspace_started_id` for the baseline workspace-only cursor query.
- `idx_history_state` from `0001_initial.sql` is not useful for the new workspace-scoped query model.
- The migration may leave old indexes in place for compatibility, but query planning and performance gates must verify the new composite indexes are used.

Search implementation:

- First implementation may use bounded `LIKE` search over `method`, `url`, `request_name`, and `error_message` after indexed workspace/time/protocol/state narrowing.
- Search result ordering remains `started_at DESC, id DESC`, not relevance order.
- Do not add an FTS virtual table to the 0005 migration. `Database::connect` runs migrations at startup, so an unsupported FTS5 build would break app launch.
- If large-history search performance misses the gate, create the FTS table lazily on first search-index use after checking `PRAGMA compile_options` for FTS5 support:

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS history_search_fts
USING fts5(history_id UNINDEXED, workspace_id UNINDEXED, method, url, request_name, error_message);
```

If FTS5 is unavailable, keep the indexed narrowing plus bounded `LIKE` path and show no user-visible error.

### 6.3 History Blob References

Phase 5 must normalize history-owned blob references instead of discovering nested snapshot refs by parsing `request_snapshot_json` during startup recovery.

Add a live-reference table:

```sql
CREATE TABLE IF NOT EXISTS history_blob_refs (
    history_id TEXT NOT NULL REFERENCES history_index (id) ON DELETE CASCADE,
    blob_hash TEXT NOT NULL,
    ref_kind TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (history_id, blob_hash, ref_kind)
);

CREATE INDEX IF NOT EXISTS idx_history_blob_refs_blob_hash
ON history_blob_refs (blob_hash);
```

Reference kinds:

- `response_body`
- `request_snapshot`
- `request_snapshot_body`
- `stream_transcript`
- `grpc_descriptor`
- `other`

Rules:

- `CreateHistoryRun` and every finalizer input must carry a `Vec<HistoryBlobRefInput>` for all blob references introduced by that write.
- `history_index.blob_hash`, `request_snapshot_blob_hash`, and `transcript_blob_hash` remain denormalized display/detail columns.
- `history_blob_refs` is the authoritative recovery source for all history-owned blobs, including blobs nested inside `request_snapshot_json`.
- Snapshot creation and finalization must populate `history_blob_refs` in the same repository transaction that writes the history row update.
- Startup recovery continues to include existing `requests.body_blob_hash` refs; Phase 5 extends that sweep with `SELECT DISTINCT blob_hash FROM history_blob_refs`.
- The existing `HistoryRepository::referenced_blob_hashes()` should either read from `history_blob_refs` plus legacy `history_index.blob_hash`, or be replaced by a dedicated `BlobReferenceRepository`.

Blob reference concepts:

- `request_snapshot_blob_hash` stores the snapshot JSON itself when the snapshot is too large for `history_index.request_snapshot_json`.
- `request_snapshot_body` refs point to request body artifacts mentioned inside the snapshot JSON.
- `transcript_blob_hash` stores the WebSocket/gRPC transcript artifact.
- `blob_hash` remains the HTTP/GraphQL response-body artifact.

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
- Every blob hash mentioned by a snapshot body must also have a `history_blob_refs` row with `ref_kind = 'request_snapshot_body'`.
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

Stream pause/resume is explicitly out of scope for Phase 5. Streaming requests can connect, send where applicable, receive, cancel, and disconnect, but cannot pause the network reader and resume it later.

Add history query types:

```rust
pub struct HistoryQuery {
    pub workspace_id: WorkspaceId,
    pub request_id: Option<RequestId>,
    pub collection_id: Option<CollectionId>,
    pub protocol: Option<RequestProtocolKind>,
    pub state: Option<HistoryState>,
    pub status_family: Option<StatusFamily>,
    pub status_min: Option<u16>,
    pub status_max: Option<u16>,
    pub method: Option<String>,
    pub url_search: Option<String>,
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

Filter semantics:

- `workspace_id` is always required.
- `request_id` narrows to one persisted source request when present.
- `collection_id` matches the denormalized `request_collection_id` captured at run time.
- `method` is an exact case-insensitive method/action filter.
- `url_search` searches only the URL/endpoint field.
- `search` is the general text search across method/action, URL/endpoint, request name, and error summary.
- `status_family` is convenience syntax for `status_min/status_max`; repository code must reject contradictory status filters.

Timestamp and cursor precision:

- After the 0005 migration, all history timestamp columns used for ordering and filtering are Unix milliseconds.
- New Phase 5 history rows must write `started_at`, `completed_at`, `dispatched_at`, `first_byte_at`, and `cancelled_at` in Unix milliseconds.
- Cursor stability still uses `(started_at, id)` because multiple rows can share the same millisecond.
- `HistoryEntryId` is UUIDv7 today; lexicographic `id DESC` is a valid tie-breaker as long as the ID type remains time-sortable. If the ID type changes, the cursor contract must be revisited or replaced with an explicit monotonic sequence column.

## 8. Repository and Service Contracts

### 8.1 HistoryRepository

Replace the current list-only API with query primitives while keeping compatibility wrappers during migration:

```rust
pub trait HistoryRepository: Send + Sync {
    fn create_pending(&self, input: CreateHistoryRun) -> RepoResult<HistoryEntry>;
    fn finalize_completed(&self, input: FinalizeCompletedRun) -> RepoResult<()>;
    fn finalize_failed(&self, input: FinalizeFailedRun) -> RepoResult<()>;
    fn finalize_cancelled(&self, input: FinalizeCancelledRun) -> RepoResult<()>;
    fn finalize_stream_completed(&self, input: FinalizeStreamCompletedRun) -> RepoResult<()>;
    fn finalize_stream_failed(&self, input: FinalizeStreamFailedRun) -> RepoResult<()>;
    fn finalize_stream_cancelled(&self, input: FinalizeStreamCancelledRun) -> RepoResult<()>;
    fn mark_pending_as_failed_on_startup(&self) -> RepoResult<usize>;
    fn query(&self, query: HistoryQuery) -> RepoResult<HistoryPage>;
    fn get(&self, id: HistoryEntryId) -> RepoResult<Option<HistoryEntry>>;
    fn get_latest_for_request(&self, request_id: RequestId) -> RepoResult<Option<HistoryEntry>>;
    fn referenced_blob_hashes(&self) -> RepoResult<HashSet<String>>;
}
```

History write inputs:

```rust
pub struct HistoryBlobRefInput {
    pub blob_hash: String,
    pub ref_kind: HistoryBlobRefKind,
}

pub struct CreateHistoryRun {
    pub workspace_id: WorkspaceId,
    pub request_id: Option<RequestId>,
    pub protocol_kind: RequestProtocolKind,
    pub method: String,
    pub url: String,
    pub request_name: Option<String>,
    pub request_collection_id: Option<CollectionId>,
    pub request_parent_folder_id: Option<FolderId>,
    pub request_snapshot_json: Option<String>,
    pub request_snapshot_blob_hash: Option<String>,
    pub redacted_snapshot: RequestSnapshot,
    pub blob_refs: Vec<HistoryBlobRefInput>,
}
```

Every finalizer input must also include `blob_refs: Vec<HistoryBlobRefInput>` so response bodies, transcript blobs, snapshot overflow blobs, snapshot body blobs, and descriptor blobs are inserted into `history_blob_refs` transactionally with the terminal update.

Cursor ordering:

- primary sort: `started_at DESC`
- tie-breaker: `id DESC`
- cursor payload: `(started_at, id)`
- query condition for next page:
  - `started_at < cursor.started_at`
  - or `started_at = cursor.started_at AND id < cursor.id`

This prevents duplicates and missing rows when several requests start in the same second.

Finalization rules:

- `finalize_*` methods update only `pending` rows.
- If the row is already in a terminal state, the method is a no-op and logs a warning with the current state and attempted state.
- A late cancel must never overwrite completed, failed, or already-cancelled history.
- Entity-level operation ID checks still apply before calling the repository. Repository idempotence is a second safety layer, not a replacement.

### 8.2 HistoryService

Add `src/services/history.rs` for orchestration:

- builds secret-safe snapshots
- creates pending history rows for all protocols
- restores history rows into request tabs
- resolves restore destination when the original collection/folder was deleted
- loads response/transcript previews for details panes
- owns retention/delete-history operations

Ownership rule:

- Phase 5 moves pending-history creation and finalization ownership into `HistoryService`.
- `RequestExecutionService::create_pending_history` and `RequestExecutionService::finalize_history` must either be removed or become private helpers called only by `HistoryService`/`HttpExecutor`.
- Request-tab UI code must call the protocol dispatcher, not create or finalize history rows directly.
- `ProtocolExecutionService` receives a history operation ID from `HistoryService`, passes it through executor events, and asks `HistoryService` to finalize once.

Delete and retention operations:

- delete selected history row with confirmation
- delete all rows matching the current filter with confirmation and a count preview
- delete rows older than a configured cutoff for retention cleanup
- delete operations remove `history_index` rows and their `history_blob_refs` rows transactionally
- blob files are removed by the existing recovery/cleanup path only after they are no longer referenced by requests or history

Restore destination rules use the history row's `workspace_id`, not whichever workspace is currently active in the window:

1. If the original request still exists, open/focus it and restore the selected history response state.
2. If the original request is deleted but the snapshot collection still exists, create an unsaved draft in that collection. Use the original parent folder only if it still exists.
3. If the snapshot collection is gone, create the draft in the first managed collection in the history row's workspace ordered by `sort_order ASC, id ASC`.
4. If no usable collection exists in the history row's workspace, create a managed collection named `Restored History` in that workspace and create the draft at its root.

The auto-created `Restored History` collection is normal user-owned data: it can be renamed, moved, or deleted like any other managed collection and must not create orphan blob references.

The restore action must report any missing body artifacts or missing secret refs through a notification and a visible warning banner at the top of the restored draft tab.

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

`HttpExecutor` is a thin adapter around the existing `RequestExecutionService`; HTTP sends must route through `ProtocolExecutionService` before GraphQL lands. GraphQL then plugs into the same dispatcher by transforming GraphQL config into an HTTP execution input rather than re-implementing HTTP history/finalization.

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
- add request protocol schema columns only; repository read/write support lands in Slice 4
- add `idx_history_workspace_started_id`
- add `history_blob_refs`
- normalize existing history timestamp columns to Unix milliseconds
- extend `HistoryEntry`
- add `HistoryQuery`, `HistoryCursor`, `HistoryPage`
- implement cursor-paginated `HistoryRepository::query`
- update history creation/finalization writes to store millisecond timestamps
- preserve `list_recent` and `list_for_request` temporarily as wrappers
- update startup recovery blob reference collection
- keep `mark_pending_as_failed_on_startup` available for `RecoveryCoordinator`
- add indexes and migration tests

Acceptance:

- query returns stable cursor pages with no duplicate rows
- query clamps `limit`
- workspace filter is mandatory
- method, URL search, collection, status range, time range, request, protocol, state, and general text search filters are represented in `HistoryQuery`
- baseline workspace-only query uses `idx_history_workspace_started_id`
- request filter uses `idx_history_request_started`
- protocol/state/status filters are covered by indexes
- history timestamp columns are all milliseconds after migration
- `finalize_*` methods are no-ops when the row is already in a terminal state, with a logged warning
- migration round-trip passes from old DB state
- recovery does not delete transcript, snapshot, snapshot-body, descriptor, response-body, or existing request-body blobs
- create/finalize repository writes insert their `history_blob_refs` in the same transaction

Tests:

- `history_query_cursor_is_stable_with_same_started_at`
- `history_query_filters_by_workspace_protocol_state_status`
- `history_query_supports_method_url_collection_status_and_time_filters`
- `history_query_clamps_limit`
- `history_migration_normalizes_timestamps_to_millis`
- `history_finalize_terminal_row_is_idempotent`
- `history_blob_refs_are_authoritative_for_recovery`
- `history_write_inserts_blob_refs_transactionally`
- `migration_0005_adds_protocol_history_fields`
- `recovery_preserves_phase5_history_blob_refs`

### Slice 2: Virtualized Global History UI

Tasks:

- replace eager card rendering with a history view state entity
- verify whether `gpui-component` exposes a virtualized table/list primitive suitable for history rows
- add virtualized row delegate using `gpui-component` table/list primitives if available
- if `gpui-component` does not provide a suitable primitive, build or extract a minimal virtualized list primitive inside Phase 5 with its own row-height, selection, scroll, and keyboard acceptance tests
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
- search results are ordered by `started_at DESC, id DESC`, not relevance
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

- replace the current fixed-size modal with `request_tab/history_panel.rs`
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
- restored drafts are session-only until the user saves them; closing the window before save loses the draft but not the history row
- restoring after collection deletion uses deterministic destination rules
- pending rows cannot be restored as completed responses
- missing body blob produces a visible warning banner, not a panic
- secret refs are not resolved or copied as plaintext
- compare uses structured status/timing/header/body-summary tables
- compare uses JSON-aware structured diff for JSON bodies under `ResponseBudgets::PREVIEW_CAP_BYTES`
- compare uses unified text diff for text bodies under `ResponseBudgets::PREVIEW_CAP_BYTES`
- compare falls back to metadata-only for large, binary, truncated, missing, or non-text bodies

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
- wrap `RequestExecutionService` in an `HttpExecutor` adapter
- route HTTP sends through `ProtocolExecutionService`
- keep existing HTTP editor behavior unchanged for `protocol_kind = Http`

Acceptance:

- existing HTTP tests pass without changes to behavior
- selecting a protocol updates draft state and dirty state once
- saved protocol fields round-trip through SQLite
- linked collection round-trip preserves protocol fields
- tab title and breadcrumbs remain stable
- existing HTTP send still works through the protocol dispatcher
- method sentinel fallback is read-only compatibility, not the new write path

Tests:

- `request_protocol_defaults_to_http`
- `request_protocol_config_roundtrip`
- `linked_collection_preserves_protocol_config`
- `protocol_switch_marks_draft_dirty_once`
- `http_send_routes_through_protocol_dispatcher`

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
- subscription operations are visible but disabled, and attempting to send one returns a preflight error that GraphQL subscriptions over WebSocket are out of Phase 5 scope
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

- add `tokio-tungstenite` after the Phase 5 dependency/version/license check
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
- `websocket_history_snapshot_redacts_handshake_headers`

### Slice 7: gRPC Unary

Tasks:

- add gRPC dependencies after the Phase 5 dependency/version/license check:
  - `tonic` transport
  - `prost` / `prost-reflect` for descriptors and dynamic messages
  - `tonic-reflection` for server reflection support
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
- add retention cleanup service hooks
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

`ItemKind::History` is a non-persisted singleton tab key, so each window has one History tab. Workspace switching reuses that tab; the History view state must key filters, cursor state, selected row, and grouping by workspace ID.

Layout:

- top toolbar:
  - search input
  - protocol segmented control
  - state/status filters
  - date range
  - grouping menu
  - refresh action
  - delete selected row
  - delete all rows matching current filter
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
- save response artifact through the existing response save-to-file flow
- save transcript artifact as JSON Lines (`.jsonl`) through a native save dialog

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

Finalizers must be idempotent:

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

Restored deleted-source requests are unsaved draft tabs until the user explicitly saves them. This matches the current draft model: closing the window before saving loses the draft, but the source history row remains available for another restore attempt.

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
- `websocket_messages_in_total`
- `websocket_messages_out_total`
- `websocket_visible_messages_dropped_total`
- `grpc_unary_total`
- `grpc_stream_messages_in_total`
- `grpc_stream_messages_out_total`
- `stream_transcript_bytes_written_total`
- `stream_batch_flush_total`
- `protocol_cancel_total`

Counter naming convention:

- use plural nouns for counted items, e.g. `messages`
- use `_total` for monotonic counters
- use `_bytes_written_total` for byte accumulators

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
cargo test --test phase5_large_history -- --ignored
cargo clippy --package torii
```

`phase5_large_history.rs` should mark 100,000-row scenarios with `#[ignore]` so default `cargo test --package torii` stays practical. Smaller cursor and query-shape tests stay in the normal test suite.

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
- Existing method-sentinel protocol badges remain a permanent read-only compatibility fallback. New writers must use `protocol_kind`; no follow-up sentinel migration is planned unless real sentinel-authored rows are discovered.
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
- [ ] `history_blob_refs` is authoritative for Phase 5 history-owned blob cleanup
- [ ] history query API replaces eager fixed-limit list usage
- [ ] global history is virtualized
- [ ] per-request history uses the same query model
- [ ] deleted-source restore creates a draft from snapshot
- [ ] protocol kind/config persist for requests
- [ ] HTTP sends route through `ProtocolExecutionService`
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
