# Response Metadata Popovers Plan

> Purpose: design the four hover-activated detail popovers that hang off the response metadata bar — Status, Response Time (waterfall), Response/Request Size, and Network/Details.
> Date: 2026-04-19
> Scope: Phase 3 REST response surface. Assets in `response_popovers/` (`status_code.png`, `response_time.png`, `res_req_size.png`, `details.png`) are the visual target.
>
> **Transport scope:** this plan is committed to **reqwest 0.12 only** — no fork, no alternate transport. The goal is to extract the maximum amount of data every public reqwest API + companion crates can give us, and to be explicit about what stays permanently unreachable until a future transport project lands. Unreachable fields keep their `Option`-shaped slots on `ResponseSummary` so a future transport lights them up without UI changes.

---

## 1. Goal

Replace the inert metadata text (`src/views/item_tabs/request_tab/response_panel.rs:83-118`) with four hover-activated popovers. Each popover must pull from a single typed source of truth on `ResponseSummary`, render allocation-free in the hot path, persist via history metadata so reopened requests look identical, and localize every visible string.

Constraints inherited from the codebase:

- all user-facing strings go through `es_fluent::localize(...)` with entries in `i18n/en/torii.ftl` and `i18n/zh-CN/torii.ftl` (CLAUDE.md).
- render-loop: no per-frame `Vec` allocations — content is computed lazily inside the popover's `content(...)` closure (`docs/diagnose/render-loop-audit.md:57`).
- all new fields are `Option`-shaped; missing values render `—` per the Timing tab precedent (`docs/plan.md:439`).
- reqwest transport stability: use the public API; no monkey-patching.

---

## 2. Per-Popover Data Availability

Verified against `reqwest-0.12.24` source. Each row lists the real reqwest path and the verdict under this plan's scope.

### 2.1 Status popover (`status_code.png`)

| Field | Source | Verdict |
|---|---|---|
| Status code | `response.status().as_u16()` | ✅ |
| Canonical reason | `response.status().canonical_reason()` | ✅ |
| Description text | Fluent keys keyed by code / class | ✅ |

**Coverage: 100%.**

### 2.2 Size popover (`res_req_size.png`)

| Field | Source | Verdict |
|---|---|---|
| Response headers bytes | sum over `HeaderMap` at transport boundary | ✅ |
| Response body bytes (on-wire / compressed) | `Content-Length` response header when present | ⚠️ when server sends it |
| Response uncompressed bytes | `body_ref.size_bytes()` — reqwest already decompresses transparently | ✅ |
| Request headers bytes | we own the outgoing `HeaderMap` | ✅ |
| Request body bytes | `Bytes` payload length, or counter-wrapped stream for `Stream` payloads | ✅ |

Implementation note on compressed vs uncompressed: reqwest auto-decodes `gzip`/`brotli`/`deflate` (features enabled in `Cargo.toml`). We keep that behavior. `body_ref` bytes are decompressed bytes. For the on-wire "Body" number, use the `Content-Length` response header — it is the compressed size and is present for most real responses. When absent (chunked transfer, HEAD, etc.), render `—` for the on-wire "Body" and treat `body_ref.size_bytes()` as the single authoritative value. Do **not** disable reqwest's auto-decompress to compute both manually — it costs us gzip/brotli/deflate correctness work and three new deps for one row that is already largely covered by the header.

**Coverage: all rows except "Body" (on-wire) when `Content-Length` is missing — which is rare for real APIs.**

### 2.3 Response Time popover (`response_time.png`)

| Screenshot row | reqwest path | Verdict |
|---|---|---|
| Prepare | `Instant` checkpoint in `RequestExecutionService` | ✅ |
| Socket Initialization | no reqwest equivalent | ❌ **drop row** |
| DNS Lookup | custom `Resolve` via `ClientBuilder::dns_resolver(...)` with internal timing | ✅ |
| TCP Handshake | connector is opaque; cannot split from TLS | ❌ merged |
| SSL Handshake | connector is opaque; cannot split from TCP | ❌ merged |
| → **Connect (TCP + SSL)** | `connector_layer` wrapping `BoxedConnectorService`, wall-clock inside `Service::call` | ✅ single bar |
| Waiting (TTFB) | existing `ttfb_ms` | ✅ |
| Download | `Instant` between first byte and stream end | ✅ |
| Process | `Instant` between stream end and persist/notify | ✅ |

Row list rendered under this plan: **Prepare, DNS Lookup, Connect, Waiting (TTFB), Download, Process** (6 rows).

**Coverage: 6 of 8 rows from the screenshot, with TCP and SSL merged into one bar.** The 7th row ("Socket Initialization") has no real meaning on a Rust HTTP stack and is dropped, not rendered as `—`.

### 2.4 Network / Details popover (`details.png`)

| Field | Source | Verdict |
|---|---|---|
| HTTP Version | `response.version()` → `"HTTP/1.1"` / `"HTTP/2"` / `"HTTP/3"` | ✅ |
| Local Address | not exposed on reqwest `Response`; connector drops it | ❌ permanently `—` |
| Remote Address | `response.remote_addr()` | ✅ |
| TLS Protocol | not exposed — `TlsInfo` only carries peer cert DER | ❌ permanently `—` |
| Cipher Name | not exposed | ❌ permanently `—` |
| Certificate CN | DER from `TlsInfo::peer_certificate()` + `x509-parser` | ✅ (with opt-in) |
| Issuer CN | same | ✅ |
| Valid Until | same (`tbs_certificate.validity.not_after`) | ✅ |

Two deps gate the cert block:

1. `ClientBuilder::tls_info(true)` — negligible runtime cost.
2. `x509-parser` crate — a DER parser. Small, well-maintained, no transitive bloat.

**Coverage: 5 of 8 rows from the screenshot** (HTTP Version, Remote Address, Cert CN, Issuer CN, Valid Until). Local Address, TLS Protocol, Cipher Name are permanently `—` on reqwest.

### 2.5 Summary per popover

| Popover | Rendered rows / total | Real vs `—` / dropped |
|---|---|---|
| Status | 3 of 3 | all real |
| Size | 5 of 6 | one row is `—` only when the server omits `Content-Length` |
| Response Time | 6 of 8 | "Socket Initialization" dropped; TCP + SSL merged into one "Connect" bar |
| Network / Details | 5 of 8 | Local Address, TLS Protocol, Cipher Name permanently `—` |

Total reqwest ceiling under this plan: **19 of 25 screenshot rows render real data**; 3 rows show `—` permanently (the reqwest-ceiling fields); 1 row is dropped entirely ("Socket Initialization"); 1 row is `—` only when the server skips `Content-Length`.

---

## 3. Data Model Changes

### 3.1 `ResponseSummary` extensions

Every field below lands on `ResponseSummary` even when reqwest cannot populate it today — so the UI is transport-agnostic and a future transport fills the gaps without a schema change.

```rust
pub struct ResponseSummary {
    // existing fields …
    pub http_version:  Option<String>,        // "HTTP/2"
    pub local_addr:    Option<String>,        // reqwest: always None
    pub remote_addr:   Option<String>,
    pub tls:           Option<TlsSummary>,
    pub size:          ResponseSizeBreakdown,
    pub request_size:  RequestSizeBreakdown,
    pub phase_timings: PhaseTimings,
}

pub struct TlsSummary {
    pub protocol:       Option<String>,  // reqwest: None
    pub cipher:         Option<String>,  // reqwest: None
    pub certificate_cn: Option<String>,  // reqwest: via x509-parser
    pub issuer_cn:      Option<String>,
    pub valid_until:    Option<i64>,     // unix ms
}

pub struct ResponseSizeBreakdown {
    pub headers_bytes:       Option<u64>,  // sum over HeaderMap
    pub body_wire_bytes:     Option<u64>,  // Content-Length (compressed)
    pub body_decoded_bytes:  u64,          // body_ref size (decompressed)
}

pub struct RequestSizeBreakdown {
    pub headers_bytes: Option<u64>,
    pub body_bytes:    u64,
}

pub struct PhaseTimings {
    pub prepare_ms:  Option<u64>,
    pub dns_ms:      Option<u64>,
    pub connect_ms:  Option<u64>,  // TCP + TLS combined (reqwest)
    pub tcp_ms:      Option<u64>,  // reqwest: None
    pub tls_ms:      Option<u64>,  // reqwest: None
    pub ttfb_ms:     Option<u64>,
    pub download_ms: Option<u64>,
    pub process_ms:  Option<u64>,
}
```

### 3.2 `TransportResponse` extensions

```rust
pub struct TransportResponse {
    // existing fields …
    pub http_version:          Option<http::Version>,
    pub remote_addr:           Option<std::net::SocketAddr>,
    pub peer_cert_der:         Option<Vec<u8>>,  // from TlsInfo
    pub response_headers_size: Option<u64>,
    pub content_length:        Option<u64>,      // from Content-Length header
}
```

`ReqwestTransport` populates every `Option` it can; all others stay `None`.

### 3.3 Persistence

Extend the history metadata JSON blob with a `response_meta_v2` section holding every new field. Backwards-compatible (missing fields deserialize as `None`). No new SQL columns — these fields are purely presentational. Reopening a completed request restores every popover identically.

---

## 4. Transport Wiring

### 4.1 Custom DNS resolver → `dns_ms`

Implement `reqwest::dns::Resolve` as a thin wrapper around a base resolver (default system or `hickory-resolver` if we already want it elsewhere — for this plan, the default is enough):

```rust
struct TimedResolver { inner: Arc<dyn Resolve> }

impl Resolve for TimedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let inner = self.inner.clone();
        Box::pin(async move {
            let start = Instant::now();
            let addrs = inner.resolve(name).await?;
            PHASE_COLLECTOR.with(|c| c.record_dns(start.elapsed()));
            Ok(addrs)
        })
    }
}
```

`PHASE_COLLECTOR` is a per-operation collector keyed by operation ID (see §4.4). No `hickory-resolver` dep needed unless we pick it for other reasons.

### 4.2 `connector_layer` → `connect_ms`

Add a Tower layer via `ClientBuilder::connector_layer(...)` that wall-clocks the inner service's `call(Unnameable) -> Conn`. The layer sees the whole connect (including TCP + TLS) as one opaque step — that is exactly the `connect_ms` number we need, with DNS already subtracted since DNS ran in our `TimedResolver` before the connector was invoked.

```rust
struct TimedConnectLayer;
impl<S> Layer<S> for TimedConnectLayer { /* wraps service, wall-clocks Service::call */ }
```

### 4.3 `tls_info(true)` + `x509-parser` → cert block

- Enable `.tls_info(true)` on `ClientBuilder`.
- On response build, read `response.extensions().get::<reqwest::tls::TlsInfo>()`.
- Pull `peer_certificate() -> Option<&[u8]>` (DER).
- Parse with `x509_parser::parse_x509_certificate` and extract:
  - `subject()` → find `CN` RDN → `certificate_cn`
  - `issuer()` → find `CN` RDN → `issuer_cn`
  - `validity().not_after` → `valid_until` (unix ms)

All parsing happens once at transport boundary, not per-render.

### 4.4 Phase collector

One `PhaseTimings` builder per in-flight operation, stored on the `RequestExecutionService` keyed by `HistoryEntryId`. `Instant` checkpoints land in it from multiple sites (resolver, connector layer, service). On terminal state (Completed/Failed/Cancelled), the collector is consumed into `ResponseSummary::phase_timings`. Late callbacks from cancelled operations are dropped by operation ID, per CLAUDE.md conventions.

### 4.5 What we do NOT do on reqwest

- Do **not** disable `no_gzip()`/`no_brotli()`/`no_deflate()` to DIY decompression. Use `Content-Length` for the on-wire number instead. One row being `—` on non-`Content-Length` responses is cheaper than owning decompression correctness + three new deps.
- Do **not** hand-roll a custom rustls `ServerCertVerifier` to snoop protocol/cipher — reqwest does not surface `ClientConnection` state and a verifier cannot see the negotiated suite anyway. This is the Slice 7 transport project, not a crate addition.
- Do **not** attempt to extract local_addr from the connector — `Conn` is opaque.

---

## 5. New Dependencies

| Crate | Reason | Why it's worth it |
|---|---|---|
| `x509-parser` | decode DER from `TlsInfo::peer_certificate()` for the cert block | single biggest unlock in the Network popover; 3 real rows |
| (no others required) | — | — |

Explicitly *not* added:

- `hickory-resolver` — the default resolver wrapped with timing gives the same `dns_ms` without the dep. Add later if we want DoH/DNSSEC for unrelated reasons.
- `flate2` + `brotli` — see §4.5.
- `time` / `httpdate` — `time` is already in use per `docs/completed/phase-3.5.md`; no new dep.

---

## 6. UI Structure

(Unchanged from previous revision — keeping concise here.)

### 6.1 File layout

```
src/views/item_tabs/request_tab/response_panel/
    popovers/
        mod.rs              // hover state + trigger helper
        status.rs
        time.rs             // waterfall
        size.rs
        network.rs
```

### 6.2 Hover trigger

- `RequestTabView` gains `meta_hover: ResponseMetaHover` enum and a `meta_hover_close_task: Option<Task<()>>`.
- Each trigger wraps its token in `.on_hover(cx.listener(...))`.
- Hover-enter: cancel pending close, set variant, `cx.notify()`.
- Hover-leave: spawn a ~120 ms close task; clears variant if no other trigger/popover has taken hover.
- Popover body mirrors `.on_hover(...)` to keep open during pointer travel.
- Popover config: `.open(view.meta_hover == Self::VARIANT)` driven externally, `.overlay_closable(false)`, `.appearance(true)`, anchor `TopLeft` for left tokens / `TopRight` for network.
- Keyboard: focus-enter = hover-enter, blur = hover-leave.
- Enum equality ensures only one popover is open.

### 6.3 Waterfall

Hand-rolled `div`-based. Total bar width fixed (e.g. `px(360.)`). Each row: left label, middle bar (spacer + colored bar), right duration. Colors per phase reference theme tokens (`muted_foreground`, `warning`, `primary`, `destructive`, `success`). `gpui-component::chart::BarChart` is the wrong primitive (categorical X, numeric Y — not offset-based ranges).

### 6.4 Metadata bar

```
[label] | [status ▼] • [time ▼] • [size ▼]         [network ▼] | [save]
```

Hover-only triggers; `▼` is diagrammatic.

---

## 7. Rollout

### Slice 1 — Data model + persistence, no UI

- Extend `ResponseSummary`, `TransportResponse`, and the history metadata serializer.
- Backwards-compatible `response_meta_v2` section; round-trip test.
- `ReqwestTransport` populates the easy fields: `http_version`, `remote_addr`, response header size, content-length.

### Slice 2 — Status + Size popovers

- `popovers/mod.rs` trigger helper + hover state on `RequestTabView`.
- `status.rs` with Fluent keys per code / class.
- `size.rs` using `response_headers_size`, `content_length` (on-wire body), and `body_decoded_bytes` (uncompressed).
- Visual QA.

### Slice 3 — Phase-timing instrumentation

- `TimedResolver` + `ClientBuilder::dns_resolver(...)`.
- `TimedConnectLayer` + `ClientBuilder::connector_layer(...)`.
- Per-operation `PhaseTimings` collector.
- `Instant` checkpoints in service for `prepare_ms`, `download_ms`, `process_ms`.
- No UI in this slice — timings flow into `ResponseSummary::phase_timings`.

### Slice 4 — Response Time popover + waterfall

- `time.rs` with 6-row waterfall: Prepare, DNS, Connect, Waiting (TTFB), Download, Process.
- Each row shows `—` if its ms is `None`; shouldn't happen for these 6 rows under reqwest + Slice 3.
- Layout stable regardless of which phases are present.

### Slice 5 — TLS cert block

- Enable `ClientBuilder::tls_info(true)`.
- Add `x509-parser` dep.
- Parse leaf cert at transport boundary; fill `TlsSummary::{certificate_cn, issuer_cn, valid_until}`.

### Slice 6 — Network / Details popover

- `network.rs`.
- Row list (in order): HTTP Version, Remote Address, Certificate CN, Issuer CN, Valid Until.
- Omit Local Address, TLS Protocol, Cipher Name — do not render them as `—` rows, since they are permanently unreachable on reqwest and their absence is cleaner than their `—` presence.
- Data contract on `ResponseSummary` still has those slots — when a future transport fills them, a single `when` in the renderer makes them visible.

### Slice 7 — Higher-fidelity transport (out of scope for this plan)

Separate future plan. Fork reqwest, adopt isahc, or hand-roll on hyper + tokio-rustls to unlock TCP/TLS phase split, TLS protocol, cipher name, local address, and true on-wire sizes regardless of `Content-Length`. Until then, the three permanent-`—` fields stay hidden in the Network popover and the Connect bar stays single.

---

## 8. i18n Additions

New Fluent keys (both `en` and `zh-CN`):

```
request_tab_status_desc_200 = Request successful. The server has responded as required.
request_tab_status_desc_201 = …
request_tab_status_desc_2xx_generic = …
request_tab_status_desc_3xx_generic = …
request_tab_status_desc_4xx_generic = …
request_tab_status_desc_5xx_generic = …

request_tab_response_time_popover_title = Response Time
request_tab_response_time_phase_prepare = Prepare
request_tab_response_time_phase_dns = DNS Lookup
request_tab_response_time_phase_connect = Connect
request_tab_response_time_phase_ttfb = Waiting (TTFB)
request_tab_response_time_phase_download = Download
request_tab_response_time_phase_process = Process

request_tab_response_size_popover_response = Response Size
request_tab_response_size_popover_request = Request Size
request_tab_response_size_popover_headers = Headers
request_tab_response_size_popover_body = Body
request_tab_response_size_popover_uncompressed = Uncompressed

request_tab_response_details_popover_title = Network
request_tab_response_details_http_version = HTTP Version
request_tab_response_details_remote_addr = Remote Address
request_tab_response_details_cert_cn = Certificate CN
request_tab_response_details_issuer_cn = Issuer CN
request_tab_response_details_valid_until = Valid Until
```

---

## 9. Test Plan

Unit:

- `ResponseSummary` serde round-trip (all fields set / all `None`).
- `Content-Length` parser correctness on chunked vs length-bounded responses.
- Status-code description fallback to class-level key for unknown codes.
- Cert DER parsing: happy path, malformed DER, missing CN in subject, future `valid_until`.
- Phase collector drops late callbacks by operation ID.

Integration:

- Execute → complete round-trip against an HTTPS target with `Content-Length`: every real field set; every `—` field `None`.
- Reopen from history: popovers render identically without re-sending.
- Cancel mid-stream: cancelled response's phase collector is not written.
- TLS target without `tls_info`: cert block fields are `None` (enable flag off in test client).

Visual / manual:

- Hover-enter, hover-leave with grace delay, trigger-to-popover travel.
- Single-popover-at-a-time invariant.
- Keyboard focus opens, blur closes.
- Waterfall layout stable with 1 missing phase (e.g. synthetic `None` on `connect_ms`).

---

## 10. Open Questions

1. Do we render the Network popover's Certificate block as a visually-separated section (like the screenshot's divider-heavy layout) even when the cert fields are `None` (e.g. plaintext HTTP response)? Leaning: hide the cert block entirely when `TlsSummary::certificate_cn` is `None` — plaintext requests shouldn't pretend a cert was involved.
2. Should status descriptions live as a Rust match (compile-time) or Fluent-only (one lookup per open)? Leaning Fluent-only for locale parity; the cost is trivial since popovers open on hover, not on every render.
3. Is the "Body" row (on-wire) acceptable rendering `—` when `Content-Length` is missing, or should it fall back to "same as Uncompressed"? Leaning `—` — silently showing the decompressed size as "Body" is misleading.

---

## 11. Summary

Locked to reqwest 0.12. Uses every public hook reqwest provides — `dns_resolver`, `connector_layer`, `tls_info`, `Response::version`, `Response::remote_addr`, `Content-Length`, `HeaderMap` — plus one new dep (`x509-parser`) for the cert block. Renders real data in **19 of the 25 rows across all four screenshots**. The remaining 6 rows break down as: 3 permanently `—` (Local Address, TLS Protocol, Cipher Name — reqwest public-API ceilings), 1 dropped as not-a-real-metric ("Socket Initialization"), 1 merged (TCP + SSL → Connect), 1 conditionally `—` (on-wire Body when server omits `Content-Length`).

Every permanently-`—` slot is preserved on `ResponseSummary` so a future higher-fidelity transport (Slice 7) lights them up without UI changes.
