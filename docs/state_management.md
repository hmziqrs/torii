# GPUI State Management Research (V2)

> Purpose: Correct factual issues in `STATE_MANAGEMENT_RESEARCH.md` and provide a scale-ready state architecture for a Postman-like desktop API client.
> Date: 2026-04-10

---

## 1. Executive Verdict

The V1 document was a strong GPUI-oriented foundation, but not production-ready for Postman-scale workloads.

Primary blockers identified:
- Unbounded memory growth for responses, streams, and history
- Underspecified cancellation and stream backpressure
- Persistence model too weak for crash safety and large datasets
- A few GPUI factual mismatches and API usage inconsistencies

This V2 document fixes those issues and defines scale guardrails.

---

## 2. Factual Corrections (From V1)

## 2.1 `Global` trait bounds

V1 claim:
```rust
pub trait Global: 'static + Send + Sync {}
```

Correct for current pinned GPUI:
```rust
pub trait Global: 'static {}
```

Reference:
- `/Users/hmziq/.cargo/git/checkouts/zed-a70e2ad075855582/b473ead/crates/gpui/src/global.rs:22`

## 2.2 Global lifetime semantics

V1 stated globals live for entire app lifetime. That is too absolute.

Correct behavior:
- `set_global` can replace
- `remove_global` can remove a global
- tests can clear globals

References:
- `/Users/hmziq/.cargo/git/checkouts/zed-a70e2ad075855582/b473ead/crates/gpui/src/app.rs:1719`
- `/Users/hmziq/.cargo/git/checkouts/zed-a70e2ad075855582/b473ead/crates/gpui/src/app.rs:1732`
- `/Users/hmziq/.cargo/git/checkouts/zed-a70e2ad075855582/b473ead/crates/gpui/src/app.rs:1725`

## 2.3 `Subscription::detach()` return type

V1 had an example that implied storing `cx.subscribe(...).detach()` in `Vec<Subscription>`.

Correct behavior:
- `detach()` consumes `Subscription` and returns `()`
- It cannot be stored as `Subscription`

Reference:
- `/Users/hmziq/.cargo/git/checkouts/zed-a70e2ad075855582/b473ead/crates/gpui/src/subscription.rs:166`

## 2.4 `Context::observe_global` bound nuance

V1 implied marker-trait-only semantics for all observe-global usage.

Current signatures:
- `App::observe_global<G: Global>(...)`
- `Context<T>::observe_global<G: 'static>(...)`

References:
- `/Users/hmziq/.cargo/git/checkouts/zed-a70e2ad075855582/b473ead/crates/gpui/src/app.rs:1744`
- `/Users/hmziq/.cargo/git/checkouts/zed-a70e2ad075855582/b473ead/crates/gpui/src/app/context.rs:176`

## 2.5 Rule wording: Weak references in async

V1 phrasing "always use `WeakEntity<T>` in async closures" is a safe default, but technically over-absolute.

Correct wording:
- Use `WeakEntity<T>` by default in async tasks that may outlive UI entities
- Strong `Entity<T>` can be valid in short-lived scoped flows where ownership cycles cannot form

---

## 3. Scale Findings (Consolidated)

## 3.1 P0: Unbounded memory risk

High-risk patterns in V1:
- `ResponseBody::Text(String)` and `ResponseBody::Binary(Vec<u8>)` as full in-memory bodies
- `WsSession.messages: Vec<WsMessage>` unbounded
- History and other lists modeled as unbounded collections

Impact:
- Large payloads + many tabs + long sessions => high RSS growth and OOM risk.

## 3.2 P0: Cancellation and stream lifecycle not fully specified

V1 only states "drop task to cancel". This is necessary but insufficient for robust networking.

Missing:
- Request abort propagation to protocol layer
- Deterministic final state transition on cancel/error/race
- Dead-task cleanup guarantees

## 3.3 P1: Render storm risk in streaming

Per-message `cx.notify()` in high-throughput streams can cause UI invalidation churn and jank.

## 3.4 P1: Persistence is not production-grade

Plain file serialization is insufficient for:
- Atomic writes
- Incremental history growth
- Fast querying
- Migration/versioning
- Crash recovery

## 3.5 P1: Inconsistent modeling

V1 mixed value/entity semantics for `Collection`, causing ambiguity in ownership and update strategy.

---

## 4. Scale-Ready Architecture (V2)

## 4.1 State tiers

Use a three-tier model:

1. Hot reactive state (`Entity<T>`)
- UI-local, rapidly changing, currently visible
- Examples: active tab draft, in-flight lifecycle state, selected history row

2. Warm indexed value state (global store values, not entity-per-item)
- Medium churn, potentially large cardinality
- Examples: collections tree metadata, history index rows, environment metadata

3. Cold durable state (disk-backed)
- Large bodies, stream logs, archived history, imports/exports

## 4.2 Ownership policy

- `Workspace` remains per-window `Entity`
- `TabManager` and active tabs remain entities
- Collections/history/env catalogs in normalized value store with IDs
- Materialize entity wrappers only for active editors/panels

Do not keep `Vec<Entity<Collection>>` or `Vec<Entity<HistoryEntry>>` as the primary long-lived store.

## 4.3 Memory budgets and retention

Define explicit budgets (configurable):
- Per response in-memory preview cap: 1-4 MiB
- Per tab total volatile payload cap: 16-64 MiB
- WS visible message buffer: fixed ring buffer (for example 1,000-10,000 messages)
- Global in-memory history index cap: fixed row count, old rows spill to disk

Behavior when cap exceeded:
- Keep only preview in memory
- Persist full payload to disk blob store
- Mark UI as truncated with "load from disk" path

## 4.4 Streaming/backpressure model

For WS/gRPC streaming:
- Use bounded channels/queues between network readers and UI model updates
- Coalesce updates by time/window (for example every 16-100ms) or batch size
- Append to ring buffer, not unbounded `Vec`
- Apply policy on overflow: drop oldest, aggregate, or pause producer (protocol-dependent)

UI rule:
- No per-message immediate notify at high throughput.
- Notify on batch flush.

## 4.5 Cancellation model

Each in-flight operation gets:
- `Task<()>` handle for GPUI-side cancellation
- Protocol cancellation primitive (abort handle/token)
- Operation ID and lifecycle FSM
- Explicit task ownership policy:
  - Long-lived operations: store `Task` on owning entity
  - Fire-and-forget operations: call `.detach()`
  - Never drop tasks accidentally (drop means cancel)

Required transitions:
- `Sending/Waiting/Receiving -> Cancelled` on user cancel
- Terminal states are mutually exclusive: `Completed | Failed | Cancelled`
- Late responses after cancellation are ignored via operation ID check

## 4.6 Persistence model

Adopt SQLite (WAL mode) + blob files for large payloads.

Recommended split:
- `requests`, `collections`, `folders`, `environments`, `history_index` in SQLite
- Large response/stream payloads as file blobs keyed by content hash or history ID

Required features:
- Schema version + migrations
- Crash-safe writes
- Compaction/cleanup policy
- Indexed queries for sidebar/history filtering

## 4.7 Multi-window policy

- Shared app data store is global
- Window UI state remains window-local entities
- Active selection and ephemeral filters are per-window, never global unless explicitly synchronized
- Conflict policy is explicit per domain object:
  - Requests/collections/environments carry revision/version metadata
  - Default strategy: optimistic write with conflict detection
  - On conflict: prompt merge/reload or apply deterministic last-write-wins policy by object type

## 4.8 Fallible async operations policy

All async operations that touch app/window/entity state are treated as fallible.

Rules:
- Never rely on `unwrap()`/`expect()` after async boundaries for app/window/entity updates
- Handle dropped-app and dropped-window cases as normal shutdown behavior
- Treat failed `update`/`read` calls as non-fatal unless invariants are violated
- Emit structured logs/metrics for failure categories (dropped target, cancellation, real error)

## 4.9 Security and credential storage

Persistence must separate operational data from secrets.

Rules:
- API tokens, passwords, OAuth refresh/access tokens are stored in OS credential store (keychain/secret service), not SQLite/blob files
- SQLite/blob store only opaque secret references/IDs
- Export/import flows require explicit secret redaction or rebind flow
- Telemetry/logging must never include raw secret values

## 4.10 Protocol-specific backpressure defaults

Default overflow behavior is defined by protocol:

- HTTP response streaming:
  - Keep bounded preview buffer in memory
  - Continue writing full body to disk blob
  - UI updates are batched on cadence

- WebSocket:
  - Bounded inbound queue + bounded visible ring buffer
  - On sustained overflow: drop oldest visible messages, increment dropped counter
  - Optional pause control if protocol/session supports it

- gRPC server/bidi streaming:
  - Bounded decode queue and bounded rendered message buffer
  - On overflow: apply flow-control/backpressure first; if still saturated, degrade to sampled rendering and persist full stream payload to disk

These defaults can be tuned, but behavior must be deterministic and test-covered.

## 4.11 Virtualization requirements

Large list-based UI surfaces must use virtualization:
- history list
- collection/request tree with large node counts
- stream/message viewers

Requirements:
- no full render of entire dataset for steady-state interaction
- scrolling and selection stay smooth under target maximum dataset size
- virtualization behavior is validated by performance tests

## 4.12 Entity update safety (lease/reentrancy)

Entity updates must avoid unsafe nested update patterns on the same entity in a single logical operation.

Rules:
- do not re-enter mutable update of the same entity from within its own active update path
- defer/queue follow-up updates when needed
- include regression tests for reentrancy and lifecycle race conditions

---

## 5. Corrected Modeling Recommendations

## 5.1 Request/Response models

- Keep request editor model as entity for active tabs
- Response model stores:
  - metadata (status, headers, timings)
  - small preview payload in memory
  - optional disk reference for full body

Example shape:

```rust
pub enum BodyRef {
    Empty,
    InMemoryPreview { bytes: Vec<u8>, truncated: bool },
    DiskBlob { blob_id: String, preview: Option<Vec<u8>>, size_bytes: u64 },
}
```

## 5.2 WebSocket session model

- Replace unbounded `Vec<WsMessage>` with ring buffer
- Keep aggregate counters separately (`total_received`, `dropped_count`)
- Store full transcript optionally to disk by policy
- Maintain protocol-level backpressure counters (queue depth, dropped frames, flush latency)

## 5.3 History model

- `HistoryEntry` should be value rows in indexed store
- Only selected/active entry gets entity wrapper for detailed panel editing/annotations

---

## 6. Observability and Guardrails

Track and expose metrics:
- in-memory bytes per workspace/tab
- queued stream messages
- dropped/coalesced message counts
- response truncation count
- persistence latency and failure count
- cancelled-vs-completed request ratios
- conflict-detection rate across windows
- async update failure categories (dropped app/window/entity)

Add guardrails:
- graceful degradation first (truncate preview, spill to disk, pause stream, disable heavy pane features)
- hard fail with clear UI error only when safety/integrity cannot be preserved
- rate-limited warnings for repeated stream overflow

---

## 7. Test Strategy (Scale + Correctness)

## 7.1 Unit tests
- lifecycle FSM transitions including race cases
- truncation and blob persistence logic
- ring buffer overflow behavior

## 7.2 Integration tests
- send/cancel race with delayed network completion
- 100+ concurrent tab simulation
- high-throughput WS stream with coalesced rendering assertions
- restart recovery with partially written history
- window-close during in-flight request/stream (no panic, clean terminal state)
- multi-window concurrent edit conflict handling path

## 7.3 Performance tests
- large payload benchmarks (10MB, 50MB, 200MB)
- long stream soak tests
- memory plateau tests proving caps and eviction work
- render frame-time checks for virtualized history/stream panes under large datasets

## 7.4 Security tests
- secret-at-rest checks (no plaintext credentials in DB/blob files)
- redaction checks for logs/exports
- keychain lookup failure behavior

---

## 8. Acceptance Criteria for "Built for Scale"

This architecture is considered scale-ready only when:
- No unbounded in-memory payload/message/history structures remain in hot path
- Streaming updates are batched and backpressured
- Cancellation is deterministic and protocol-propagated
- Persistence is crash-safe and migration-backed
- Multi-window ownership rules are explicit and tested
- Performance/memory tests pass under target workload
- Async app/window/entity failures are handled without panics
- Task lifecycle is explicit (`hold` or `detach`) with no accidental cancellation
- Secrets are stored in platform credential storage, never plaintext in app DB/blobs
- Large list UIs are virtualized and meet target frame-time thresholds

---

## 9. Migration Plan from V1

1. Fix factual text issues in V1 references (`Global`, `detach`, observe-global bounds).
2. Introduce data-tier model and ownership clarifications.
3. Implement blob-backed response storage and history index schema.
4. Replace stream `Vec` buffers with bounded ring buffers + batch notify.
5. Add operation IDs + cancellation tokens across HTTP/WS/gRPC.
6. Add stress/perf test suite and memory budget enforcement.
7. Add explicit task lifecycle lint/checklist (`retain or detach`).
8. Add platform keychain-backed secret store and redaction tests.
9. Add protocol-specific backpressure defaults and overflow behavior tests.
10. Add multi-window conflict detection/resolution for mutable shared resources.

---

## 10. Final Decision

V1 should be treated as a strong technical primer, not a final production architecture.

This V2 document is the corrected baseline for implementation.

---

## 11. GPUI Standards Recheck Addendum

Additional shortcomings identified during standards recheck:

1. Task lifecycle must be explicit. Spawns require intentional ownership (`store Task`) or detachment.
2. Async operations are fallible by design; dropped app/window/entity targets are expected conditions.
3. Secret handling requires platform credential storage integration, not DB/blob-only persistence.
4. Large history/stream UIs require virtualization as a hard requirement, not an optional optimization.
5. Reentrancy/lease hazards must be tested in entity update flows to avoid runtime panics.
6. Backpressure policy must define per-protocol defaults rather than generic overflow options.

These are now encoded in Sections 4, 6, 7, 8, and 9 as implementation requirements.
