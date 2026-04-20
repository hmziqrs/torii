# Request Console Logging Plan

> Purpose: define a scale-safe request console for REST executions that can show detailed lifecycle logs, tail them live, and reopen them from history.
> Date: 2026-04-17

---

## 1. Goal

Add a response-side Console surface that shows request execution details similar to verbose API clients:

- request preparation and effective settings
- outgoing request line and headers
- incoming status line and headers
- body streaming progress
- transport and TLS details when the active transport can expose them
- terminal outcome for completed, failed, and cancelled requests

The console must fit Torii's existing state model:

- hot UI state stays bounded
- execution remains service-owned, not view-owned
- transcripts reopen from history without re-sending the request
- secret material never lands in SQLite, blobs, or rendered logs

---

## 2. Current State

The current REST stack already has the right ownership boundaries, but not enough transport detail for a verbose console.

Relevant code today:

- `src/services/request_execution.rs`
  - `HttpTransport::send(...)` returns only status, headers, media type, and a body stream.
  - `ExecProgressEvent` only exposes `ResponseStreamingStarted`.
- `src/session/request_editor_state.rs`
  - the editor tracks coarse execution states only: `Idle | Sending | Streaming | Completed | Failed | Cancelled`.
- `src/views/item_tabs/request_tab/types.rs`
  - response tabs are `Body | Preview | Headers | Cookies | Timing`.
- `src/views/item_tabs/request_tab/response_panel/chrome.rs`
  - the response tab strip has no Console tab yet.

Important existing constraint:

- `docs/completed/phase-3.md` explicitly deferred DNS / connect / TLS phase breakdown because the current `reqwest` integration does not expose it in the existing design.

This means the console design must distinguish between:

1. a good request console on top of the current `reqwest` transport
2. a near-`curl -v` transport transcript, which requires deeper transport access

---

## 3. UX Target

The Console tab should behave like a request-local execution transcript.

Desired behavior:

- available beside `Body`, `Headers`, `Cookies`, and `Timing`
- tails live during send and stream phases
- stays visible after completion
- reopens from persisted history for the latest run
- uses a monospaced, copyable, scrollable transcript view
- clearly distinguishes:
  - local lifecycle events
  - outbound request data
  - inbound response data
  - warnings or truncation notices

Representative transcript shape:

```text
Preparing request to https://example.com/
* Current time is 2026-04-09T08:45:11.116Z
* Timeout: 30000 ms
* SSL validation: enabled
> GET / HTTP/2
> Host: example.com
> user-agent: torii/...

< HTTP/2 200
< content-type: text/html
< server: cloudflare

* Received 528 B
* Request completed in 842 ms
```

The exact content depends on transport fidelity.

---

## 4. Verbosity Model

Torii should adopt curl-style verbosity naming for the Console tab:

- `-v`
- `-vv`
- `-vvv`
- `-vvvv`

This is a product convention inspired by curl's public CLI behavior, not a web standard. It is useful because many API developers already understand that more `v`s mean more detail.

Important rule:

- the label expresses the user's requested verbosity
- the actual lines available at each level remain transport-dependent

Recommended semantic mapping:

- `-v`
  - high-signal lifecycle only
  - preparing request
  - effective target
  - connected / request sent
  - response started
  - completed / failed / cancelled
- `-vv`
  - request and response metadata
  - sanitized request line and headers
  - status line and response headers
  - negotiated protocol version if known
  - timing summary
- `-vvv`
  - transport detail
  - redirects
  - peer socket information if known
  - certificate summary
  - chunk progress
  - transport warnings and retry / resend events
- `-vvvv`
  - deepest transport diagnostics Torii can truthfully expose
  - DNS / connect / TLS phase detail
  - ALPN negotiation
  - network-component events
  - raw wire-adjacent events if the transport supports them

This maps well to curl's own progression:

- `-vv` adds timestamps, IDs, and broader protocol tracing
- `-vvv` adds transfer content and SSL/read/write tracing
- `-vvvv` adds all network components

Torii does not need to reproduce curl output exactly, but it should preserve the same user expectation: each added `v` requests a strictly deeper diagnostic view.

---

## 5. Fidelity Levels

## 5.1 Level 1: Reqwest-backed Console

This is the recommended first delivery.

What it can reliably show:

- request preparation
- effective URL after query/auth injection
- request method and sanitized headers
- timeout / redirect / TLS-validation settings chosen by Torii
- dispatch time
- response-start event
- status line and response headers
- chunk and total byte progress
- TTFB and total time
- cancellation / failure summary
- optional leaf-certificate details if `reqwest` TLS info is enabled

What it cannot reliably show:

- DNS lookup timing
- individual IP connect attempts
- socket addresses in a portable way
- ALPN offers and selection details
- TLS handshake record-by-record events
- CAfile / CApath details
- HTTP/2 stream IDs and wire-level frame diagnostics

Notes:

- `reqwest::ClientBuilder::tls_info(true)` can provide leaf certificate bytes on the response.
- `reqwest::ClientBuilder::connection_verbose(true)` is not enough for the product surface by itself:
  - it emits generic `log` crate TRACE messages rather than structured Torii events
  - it still does not provide a `curl -v` style semantic transcript
  - Torii currently initializes `tracing`, not a dedicated `log`-to-console capture path

## 5.2 Level 2: Curl-style Transport Transcript

If the product requirement is "show logs close to the sample output", the REST transport must change or gain a second implementation.

That transport would need to expose:

- connect-attempt callbacks
- remote address details
- ALPN negotiation
- richer TLS metadata
- verbose request/response wire events

The clean boundary for that work is still the existing `HttpTransport` trait, but the trait contract must become much richer.

Rust ecosystem candidates for this level:

- `curl`
  - best fit for a deep `-vvv` / `-vvvv` implementation
  - exposes libcurl-style verbose and debug callbacks
  - exposes timing and socket metadata such as DNS lookup, connect, TLS handshake, primary IP, and effective URL
- `isahc`
  - possible middle ground
  - built on libcurl
  - exposes request metrics and wire logging, but Torii would still need an integration layer to turn those into structured console events

`reqwest` remains a good fit for `-v` and much of `-vv`, but it is not the best foundation for curl-like deep transport transcripts.

## 5.3 Recommendation

Ship in two stages:

1. add a strong reqwest-backed Console tab first
2. evaluate a higher-fidelity transport later only if exact verbose network diagnostics are required

---

## 6. Proposed Architecture

## 6.1 Transcript Event Model

Do not persist pre-rendered English lines as the source of truth.

Torii's UI rule is that user-facing strings must be localized. The console should therefore persist structured event kinds plus arguments, and render them through Fluent at display time.

Suggested model:

```rust
pub struct RequestConsoleEvent {
    pub seq: u32,
    pub occurred_at_unix_ms: i64,
    pub phase: ConsolePhase,
    pub direction: ConsoleDirection,
    pub kind: ConsoleEventKind,
    pub args_json: Option<String>,
}
```

Suggested enums:

- `ConsolePhase`
  - `Preflight`
  - `Dispatch`
  - `Response`
  - `Streaming`
  - `Terminal`
- `ConsoleDirection`
  - `Local`
  - `Outbound`
  - `Inbound`
- `ConsoleEventKind`
  - `PreparingRequest`
  - `CurrentTime`
  - `SettingTimeout`
  - `SettingRedirects`
  - `SettingSslValidation`
  - `EffectiveUrl`
  - `RequestStartLine`
  - `RequestHeader`
  - `ResponseStartLine`
  - `ResponseHeader`
  - `ChunkReceived`
  - `CertificateSummary`
  - `RequestCompleted`
  - `RequestFailed`
  - `RequestCancelled`
  - `TranscriptTruncated`

Why structured events instead of raw lines:

- localization remains possible
- redaction rules are easier to enforce before persistence
- rendering can evolve without changing the stored shape
- tests can assert semantic event order instead of string fragments only

## 6.2 Persistence

Use the `operation_logs` concept that already appears in `docs/plan.md`.

Recommended schema shape:

```sql
CREATE TABLE operation_logs (
    history_entry_id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    occurred_at INTEGER NOT NULL,
    phase TEXT NOT NULL,
    direction TEXT NOT NULL,
    kind TEXT NOT NULL,
    args_json TEXT,
    PRIMARY KEY (history_entry_id, seq)
);
```

Repository boundary:

- add `operation_log_repo.rs`
- expose:
  - `replace_for_history_entry(history_entry_id, events)`
  - `list_for_history_entry(history_entry_id)`
  - optional `delete_for_history_entries(ids)` for later retention work

Persistence strategy for REST:

- collect structured events in memory during one execution
- stream live updates to the tab while the request runs
- persist the final bounded transcript at terminal state in one repo call

This keeps write amplification low and fits the current Phase 3 execution architecture.

---

## 6.3 Execution Flow Changes

`RequestExecutionService` remains the owner of execution semantics.

Changes:

1. Introduce a console sink for execution-time events.
2. Extend `ExecProgressEvent` so the UI can receive batched console updates while a request is active.
3. Extend `TransportResponse` with any metadata the concrete transport can expose cleanly, such as:
   - HTTP version
   - optional TLS leaf-certificate summary
4. Keep `ExecOutcome` focused on terminal response state, but include a bounded transcript snapshot alongside it or make the service persist transcript rows before notifying the UI.

Suggested event flow:

1. `RequestTabView::send` creates the pending history row as it does today.
2. The execution service builds a transcript collector for that `HistoryEntryId`.
3. Preflight and request-build steps emit console events.
4. The transport emits transport-level events through a callback or event sink.
5. Response streaming emits throttled progress events.
6. On `Completed | Failed | Cancelled`, the service persists transcript rows, then finalizes history, then updates the UI.

Important rule:

- late responses after cancel must not overwrite console state for a newer operation
- this uses the same operation ID guard already used for response state

## 6.4 Streaming and Batching

Do not append one UI notification per console line.

Rules:

- batch UI console updates the same way high-volume response/table updates are already guarded elsewhere in the request tab
- keep a bounded ring buffer for live transcript lines in hot state
- coalesce noisy streaming logs

Recommended caps:

- live per-tab transcript buffer: 256 events
- persisted REST transcript cap: 1024 events or 256 KiB rendered-equivalent payload

When caps are exceeded:

- drop oldest persisted-irrelevant streaming-detail events first
- insert a synthetic `TranscriptTruncated` event

Chunk logging policy:

- do not blindly log every network chunk forever
- log the first few chunk events verbatim
- then switch to periodic aggregate progress events for large bodies

This prevents the Console tab from becoming an accidental unbounded stream surface.

## 6.5 Redaction and Security

Console logs must follow the same security posture as history snapshots.

Never persist or render:

- authorization header values
- cookie values
- bearer/basic/api-key secret values
- secret-derived query parameter values
- raw request body content when the body may contain secrets by default

Recommended request-side console policy:

- request line is allowed
- request headers are rendered only after redaction
- request body is summarized, not dumped, in v1

Recommended response-side console policy:

- status line is allowed
- response headers are allowed except `set-cookie` values should be masked or summarized

This keeps the Console tab useful without creating a second secret-leak surface.

## 6.6 UI Integration

Request-tab changes:

- add `ResponseTab::Console`
- add a new console renderer under `src/views/item_tabs/request_tab/response_panel/`
- keep the view read-only in v1
- add copy actions:
  - copy selected lines if selection support already exists
  - otherwise copy full transcript

State changes:

- add console transcript state to `RequestTabView`
- do not add raw transcript storage to `RequestEditorState`; keep editor state focused on request/save/exec lifecycle
- on request reopen, load the latest persisted transcript through the new repo using `latest_history_id`

Rendering guidance:

- monospaced text
- stable line prefixes for local / outbound / inbound
- scroll-to-bottom while active unless the user has manually scrolled away
- no webview and no rich formatting requirements in v1

---

## 7. Rollout Plan

## Slice 1: Transcript Data and Repo

- add `operation_logs` migration
- add domain type for console events
- add repository and roundtrip tests

## Slice 2: Execution Instrumentation

- add console collector and progress events
- emit service-level preflight / dispatch / terminal events
- persist transcript on terminal state

## Slice 3: Reqwest-backed Metadata

- add any safe reqwest-enriched details Torii can actually expose
- optionally enable TLS info and summarize the peer certificate
- do not promise curl-level connect and TLS event fidelity in this slice

## Slice 4: Request Tab Console UI

- add `Console` response tab
- live tail while request runs
- reopen from latest history entry
- copy transcript action

## Slice 5: High-fidelity Transport Evaluation

- only if product requirements demand logs close to `curl -v`
- evaluate a second REST transport implementation behind `HttpTransport`

---

## 8. Test Plan

Unit tests:

- redaction of request headers, query params, cookies, and auth-derived fields
- localization formatting from `kind + args_json`
- transcript truncation and synthetic notice insertion

Integration tests:

- transcript event order for success
- transcript terminal event on failure
- transcript terminal event on cancel
- late response after cancel does not overwrite newer transcript state
- reopening a request tab restores the latest transcript without re-sending

Performance and behavior checks:

- large response stream does not spam `cx.notify()`
- transcript stays within hot-state caps
- no render-loop regression from console updates

---

## 9. Open Questions

1. Should request body previews ever be shown in the console, or should v1 keep request bodies summary-only for safety?
2. Should `set-cookie` lines be fully hidden, value-masked, or summarized by cookie name only?
3. Is a reqwest-backed console sufficient for the product, or is near-`curl -v` parity a real requirement?
4. If high-fidelity transport logging is required, should Torii:
   - replace the REST transport
   - add a second "diagnostic" transport
   - or offer both behind a setting?

---

## 10. Recommendation Summary

The right first implementation is a structured, localized, secret-safe Console tab built on top of the existing request execution pipeline.

That version should:

- persist bounded transcripts per history entry
- tail live during request execution
- reopen from history
- expose only the transport details Torii can truthfully observe

If the product later needs logs that closely match the sample verbose transcript, that should be treated as a transport project, not as a UI-only enhancement.
