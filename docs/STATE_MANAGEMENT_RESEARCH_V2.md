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

Add guardrails:
- hard fail with clear UI error when memory caps cannot be honored
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

## 7.3 Performance tests
- large payload benchmarks (10MB, 50MB, 200MB)
- long stream soak tests
- memory plateau tests proving caps and eviction work

---

## 8. Acceptance Criteria for "Built for Scale"

This architecture is considered scale-ready only when:
- No unbounded in-memory payload/message/history structures remain in hot path
- Streaming updates are batched and backpressured
- Cancellation is deterministic and protocol-propagated
- Persistence is crash-safe and migration-backed
- Multi-window ownership rules are explicit and tested
- Performance/memory tests pass under target workload

---

## 9. Migration Plan from V1

1. Fix factual text issues in V1 references (`Global`, `detach`, observe-global bounds).
2. Introduce data-tier model and ownership clarifications.
3. Implement blob-backed response storage and history index schema.
4. Replace stream `Vec` buffers with bounded ring buffers + batch notify.
5. Add operation IDs + cancellation tokens across HTTP/WS/gRPC.
6. Add stress/perf test suite and memory budget enforcement.

---

## 10. Final Decision

V1 should be treated as a strong technical primer, not a final production architecture.

This V2 document is the corrected baseline for implementation.
