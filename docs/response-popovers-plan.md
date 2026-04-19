# Response Metadata Popovers Plan

> Purpose: design the four click-to-open detail popovers that hang off the response metadata bar — Status, Response Time (waterfall), Response/Request Size, and Network/Details.
> Date: 2026-04-19
> Scope: Phase 3 REST response surface. Assets in `response_popovers/` (`status_code.png`, `response_time.png`, `res_req_size.png`, `details.png`) are the visual target.

---

## 1. Goal

The response panel currently renders a single-line metadata strip (`src/views/item_tabs/request_tab/response_panel.rs:83-118`) with plain `div` text for status, size, and time. The target UX replaces each of those tokens with a **hover-activated** trigger that opens a focused popover (no click required — the popover appears while the pointer rests on the token and dismisses when the pointer leaves both trigger and popover):

1. **Status popover** — status code, canonical reason, short description (`"Request successful. The server has responded as required."`).
2. **Response time popover** — total response time plus a waterfall chart of the execution phases (Prepare, Socket Init, DNS, TCP, SSL, Waiting/TTFB, Download, Process).
3. **Size popover** — response size (headers / body / uncompressed) and request size (headers / body).
4. **Network / details popover** — HTTP version, local/remote address, TLS protocol, cipher name, certificate CN, issuer CN, valid-until.

Constraints inherited from the current codebase:

- All visible strings go through `es_fluent::localize(...)` with entries in `i18n/en/torii.ftl` and `i18n/zh-CN/torii.ftl` (CLAUDE.md).
- The response panel must stay render-loop safe — no new `Vec<...>` allocations per frame (see `docs/diagnose/render-loop-audit.md:57`). Popover content is computed lazily, only when the popover is open.
- `reqwest` transport does not expose DNS / connect / TLS phase breakdown today (`docs/completed/phase-3.md`). Some rows will render as `—` placeholders until the transport evolves — this is consistent with the existing Timing tab policy (`docs/plan.md:439`).
- Response metadata must survive history restore — the extra fields must flow through `ResponseSummary` and its persistence path, not live only in the view.

---

## 2. Current State

Relevant code today:

- `src/domain/response.rs` — `ResponseSummary` carries `status_code`, `status_text`, `total_ms`, `ttfb_ms`, dispatch / first-byte / completed unix ms. It does **not** carry HTTP version, peer addresses, TLS info, phase timings, headers byte count, or request size.
- `src/services/request_execution.rs` — `TransportResponse` exposes status, headers, media type, and a body stream. No HTTP version, no peer metadata, no TLS info.
- `src/views/item_tabs/request_tab/response_panel.rs` — renders the metadata row directly with `div`; each token is inert.
- `src/views/item_tabs/request_tab/response_panel/chrome.rs` — response-tab strip; no popovers wired up.
- `src/views/item_tabs/request_tab/response_panel/tables.rs` — `TimingTableDelegate` already exists for the Timing tab; the new popover presents the same conceptual data in a waterfall chart, not a table.
- `gpui-component` 0.5.1 exposes `popover::{Popover, PopoverState}` (see `chart/`, `plot/` for primitives). `chart::BarChart` is a categorical X → numeric Y chart; it does **not** natively support offset-based horizontal bars of the Gantt / waterfall kind. A waterfall is simpler to hand-roll from `div` widths than to coax out of `BarChart`.

---

## 3. Data Model Changes

### 3.1 `ResponseSummary` extensions

Add optional fields so every popover has a single source of truth and each value is independently `None`-able when the transport cannot report it:

```rust
pub struct ResponseSummary {
    // existing fields …
    pub http_version: Option<String>,               // "HTTP/1.1" | "HTTP/2" | "HTTP/3"
    pub local_addr:   Option<String>,
    pub remote_addr:  Option<String>,
    pub tls:          Option<TlsSummary>,
    pub size:         ResponseSizeBreakdown,
    pub request_size: RequestSizeBreakdown,
    pub phase_timings: PhaseTimings,
}

pub struct TlsSummary {
    pub protocol:       Option<String>, // "TLSv1.3"
    pub cipher:         Option<String>, // "TLS_AES_128_GCM_SHA256"
    pub certificate_cn: Option<String>,
    pub issuer_cn:      Option<String>,
    pub valid_until:    Option<i64>,    // unix ms
}

pub struct ResponseSizeBreakdown {
    pub headers_bytes:      Option<u64>,
    pub body_bytes:         u64,          // already known via BodyRef::size_bytes
    pub uncompressed_bytes: Option<u64>,  // set when Content-Encoding decoded
}

pub struct RequestSizeBreakdown {
    pub headers_bytes: Option<u64>,
    pub body_bytes:    u64,
}

pub struct PhaseTimings {
    pub prepare_ms:    Option<u64>,
    pub socket_ms:     Option<u64>,
    pub dns_ms:        Option<u64>,
    pub tcp_ms:        Option<u64>,
    pub tls_ms:        Option<u64>,
    pub ttfb_ms:       Option<u64>,  // Waiting
    pub download_ms:   Option<u64>,
    pub process_ms:    Option<u64>,
}
```

Rules:

- All values default to `None` when unknown; the UI renders `—` for `None`. This matches the existing Timing tab policy.
- `ttfb_ms` continues to live on `ResponseSummary` directly (already persisted). `PhaseTimings::ttfb_ms` is a projection, not a new source of truth — the execution service writes both consistently.
- Request size is computed by the execution service at send time and attached before the response resolves. Body size must respect streaming bodies: when a request body is a stream (`RequestBodyPayload::Stream`), record the wire-sent byte count after the transport completes, not the in-memory payload size.

### 3.2 Transport surface

`TransportResponse` (`src/services/request_execution.rs:40`) gains optional fields the transport can populate when it can:

```rust
pub struct TransportResponse {
    // existing fields …
    pub http_version:     Option<http::Version>,
    pub local_addr:       Option<std::net::SocketAddr>,
    pub remote_addr:      Option<std::net::SocketAddr>,
    pub tls:              Option<TlsSummary>,
    pub response_headers_size: Option<u64>,
    pub uncompressed_body_size: Option<u64>,
}
```

Reqwest-path reality (for this slice):

- `http_version` — populated from `response.version()` (always available).
- `remote_addr` — populated from `response.remote_addr()` (available on reqwest).
- `local_addr` — `reqwest` does not expose it portably; leaves `None` for now.
- `tls` — only available when `reqwest::ClientBuilder::tls_info(true)` is enabled and the response carries `tls::TlsInfo`. See `request-console-plan.md:168` for the same trade-off discussion. Even then, only the leaf certificate bytes are exposed; protocol/cipher require TLS crate introspection. Default to `None` in the first slice.
- Phase timings — `reqwest` does not expose DNS / TCP / TLS phase split. `ttfb_ms` and `total_ms` are all we can honestly populate. Other phase fields stay `None` and the waterfall renders those rows as `—` placeholders (stable layout). This mirrors the choice made in `docs/completed/phase-3.5.md` for the Timing tab.

### 3.3 Persistence

The new fields must be restorable when the user reopens a completed request from history — the popovers should work identically whether the request was just sent or reopened weeks later.

Two options:

1. **Preferred:** extend the existing history metadata JSON blob (search `history_repo` for the response-metadata serializer) with a single `response_meta_v2` section containing the new fields. Backwards-compatible by default — missing fields deserialize as `None`.
2. **Alternative:** new columns on the history entry row. More rigid, more migrations, no clear query benefit.

Pick option 1. The fields are purely presentational and don't need to be queried.

---

## 4. UI Structure

### 4.1 File layout

Add a new sub-module tree:

```
src/views/item_tabs/request_tab/response_panel/
    popovers/
        mod.rs              // trigger helpers + shared styling
        status.rs           // Status code popover content
        time.rs             // Response time popover (+ waterfall component)
        size.rs             // Response/Request size popover
        network.rs          // Network / Details popover
```

`response_panel.rs` replaces the current inline header row with a call into `popovers::render_metadata_bar(view, &response, cx)`. Idle / Sending / Failed branches stay as they are — popovers are only relevant when `ExecStatus::Completed`.

### 4.2 Trigger pattern (hover)

These popovers are **hover-activated**, not click-activated. `gpui-component::Popover` is click-driven by default, so we drive the `open` prop explicitly from hover state on `RequestTabView` instead of using the built-in trigger.

State on `RequestTabView`:

```rust
#[derive(Default, Copy, Clone, PartialEq, Eq)]
enum ResponseMetaHover {
    #[default]
    None,
    Status,
    Time,
    Size,
    Network,
}

struct RequestTabView {
    // …
    meta_hover: ResponseMetaHover,
    meta_hover_close_task: Option<Task<()>>,
}
```

Per-trigger wiring:

- Wrap each token in a `div` with `.on_hover(cx.listener(|this, hovered, _, cx| { … }))`.
- **On hover-enter** (`hovered == true`): cancel any pending close task, set `meta_hover = <variant>`, `cx.notify()`.
- **On hover-leave** (`hovered == false`): spawn a short close task (`cx.spawn`, ~120 ms delay) that clears `meta_hover` to `None` if no other trigger/popover has taken the hover in the meantime. The delay is required so the pointer can travel from the trigger into the popover body without flicker.
- Popover content itself also carries `.on_hover(...)` with the same cancel-close behavior, so the popover stays open while the cursor is inside it.

Popover configuration:

- `.open(view.meta_hover == Self::VARIANT)` — driven by view state, not internal popover state.
- `.overlay_closable(false)` — there is no overlay to click; dismissal is pointer-leave, not click-outside.
- `.appearance(true)` — default card chrome, matches the screenshots.
- `.anchor(Corner::TopLeft)` for left-anchored triggers (status, time, size); `Corner::TopRight` for the right-anchored network trigger so it never drifts off-screen.
- The `trigger(...)` arg remains the visible token (e.g. the `200 OK` pill). Because we drive `open` externally, the trigger's own click handler is a no-op — it only has to render the visual.

The trigger element keeps its existing visual but gains a subtle hover background to signal it is interactive. Cursor stays default text — this is informational, not an action.

Because the same metadata can be rendered in multiple places (e.g. eventually a history-entry viewer), keep the popover **content** builders as pure functions taking `&ResponseSummary` + `&mut Window` + `&mut App`. The hover state and `open` wiring live on the view that owns the metadata bar.

Keyboard accessibility note: hover-only is a regression for keyboard users. Also bind focus — when a trigger receives keyboard focus, treat it as hover (same `meta_hover` variant) and dismiss on blur. This keeps the popover reachable via Tab without adding a click fallback.

### 4.3 Popover content

**Status popover** (`status.rs`):

- icon: green checkmark for 2xx, info for 1xx, redirect arrow for 3xx, warning for 4xx, red X for 5xx
- title: `"{code} {canonical_reason}"` — already stored
- description: short, locale-aware explanation looked up by status-code class. A small in-repo table (`status_descriptions.rs`) maps every well-known code to a Fluent key — `request_tab_status_desc_200`, `_201`, `_301`, `_400`, `_401`, `_404`, `_500`, etc. Unknown codes fall back to a class-level key (`_2xx_generic`, `_4xx_generic`).

**Response time popover** (`time.rs`):

- Header row: clock icon, `"Response Time"` label, right-aligned `"594.44 ms"` bold.
- Waterfall: 8 rows corresponding to `PhaseTimings`. Each row has: name (left), bar (center), duration (right).
- Each bar is a `div().h(px(14.)).rounded_sm().bg(color).w(px(width))` positioned inside a flex container with a left spacer `div` whose width encodes the cumulative start offset. Total width of the bar area is fixed (e.g. `px(360.)`) and offsets are computed as `(phase_start_ms / total_ms) * total_width`.
- Color scheme (matches screenshot; all reference theme tokens, not hard-coded hex):
  - Prepare → `muted_foreground` (dim)
  - Socket Init, DNS → amber (`warning`)
  - TCP, SSL → `primary` (blue)
  - Waiting (TTFB) → dashed outline in `destructive` to call out the biggest wait bucket (matches the red dashed box in the asset)
  - Download → `success` (green)
  - Process → `muted_foreground`
- When a phase's `*_ms` is `None`, show `—` instead of a number and render no bar (row still present for layout stability).
- Total line aligns with `response.total_ms`.

`gpui-component::chart::BarChart` is **not** used for the waterfall — it expects categorical X and numeric Y, not offset-based horizontal ranges. Hand-rolled `div` widths match the visual target and keep the popover allocation-free in the hot path. If later product needs include stacked-series timelines, revisit and build a `Stack` plot (`plot::shape::Stack`).

**Size popover** (`size.rs`):

- Two sections separated by `Divider::horizontal`:
  - Response Size (down arrow icon, blue tile): Headers, Body, Uncompressed subrows
  - Request Size (up arrow icon, amber tile): Headers, Body subrows
- Sub-row layout mirrors the asset: label left, value right, monospace-ish for the number.
- Use `format_bytes` already defined in `response_panel.rs` helpers.

**Network / details popover** (`network.rs`):

- Header: globe-lock icon + localized `"Network"` label.
- Body grouped into three blocks separated by `Divider::horizontal`:
  1. HTTP Version, Local Address, Remote Address
  2. TLS Protocol, Cipher Name
  3. Certificate CN, Issuer CN, Valid Until
- Trigger lives on the right side of the metadata bar, rendered as a small globe-lock icon button (see screenshot) rather than text. The adjacent `"Save Response"` button is unrelated to this plan.
- Any `None` value renders `—`. When `response.tls.is_none()`, the two TLS blocks collapse (single divider) rather than showing three `—`-only rows.

### 4.4 Metadata bar recomposition

Target final composition of the metadata row (left to right):

```
[response label] | [status pill ▼] • [time ▼] • [size ▼]   [network ▼] | [save response]
```

- `▼` indicates the token is a hover-activated popover trigger (nothing is actually clicked — the caret is diagrammatic).
- Separators (`•`) stay text; they are not triggers.
- The `network` button is right-aligned via a spacer `div().flex_1()`.
- Only one popover can be open at a time — setting `meta_hover` to a new variant implicitly closes the previous one because every popover's `open` prop is bound to equality against that single enum.

---

## 5. Execution-path Changes

1. `RequestExecutionService` grows a `PhaseTimings` collector tied to `Instant::now()` checkpoints it can observe:
   - `prepare_ms` — from builder-start to transport-start
   - `ttfb_ms` — already tracked
   - `download_ms` — from first byte to last byte
   - `process_ms` — from last byte to completion/persistence
   - `socket_ms | dns_ms | tcp_ms | tls_ms` — stay `None` until a richer transport lands (see `request-console-plan.md:5.2`)
2. Request-side byte accounting:
   - `RequestBodyPayload::Bytes(b)` → `body_bytes = b.len()`
   - `RequestBodyPayload::Stream(_)` → increment as the stream is polled; finalize after send completes
   - headers byte count is computed from the serialized `HeaderMap` the transport actually sent
3. Response headers byte count: sum over `(name.len() + 2 + value.len() + 2)` at the transport boundary; written into `ResponseSizeBreakdown::headers_bytes`.
4. Uncompressed body size: only set when reqwest reports decoded bytes — otherwise `None`. Optional future work.
5. Late responses after cancel continue to be dropped by operation ID, not by liveness (per CLAUDE.md "Conventions").

---

## 6. i18n Additions

New Fluent keys (both `en` and `zh-CN`):

```
request_tab_response_status_popover_desc = …
request_tab_status_desc_200 = Request successful. The server has responded as required.
request_tab_status_desc_201 = …
request_tab_status_desc_2xx_generic = …
request_tab_status_desc_3xx_generic = …
request_tab_status_desc_4xx_generic = …
request_tab_status_desc_5xx_generic = …

request_tab_response_time_popover_title = Response Time
request_tab_response_time_phase_prepare = Prepare
request_tab_response_time_phase_socket = Socket Initialization
request_tab_response_time_phase_dns = DNS Lookup
request_tab_response_time_phase_tcp = TCP Handshake
request_tab_response_time_phase_tls = SSL Handshake
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
request_tab_response_details_local_addr = Local Address
request_tab_response_details_remote_addr = Remote Address
request_tab_response_details_tls_protocol = TLS Protocol
request_tab_response_details_cipher = Cipher Name
request_tab_response_details_cert_cn = Certificate CN
request_tab_response_details_issuer_cn = Issuer CN
request_tab_response_details_valid_until = Valid Until
```

Canonical `status_text` (from `http` crate) stays as today — the new `status_desc_*` keys are *additional* user-friendly context, not a replacement.

---

## 7. Rollout

### Slice 1 — Data model + persistence (no UI)

- Extend `ResponseSummary`, `TransportResponse` with new fields.
- Wire reqwest-known fields (`http_version`, `remote_addr`, response header size, body bytes breakdown).
- Extend history metadata serializer with `response_meta_v2`; round-trip test.
- Keep existing UI untouched.

### Slice 2 — Status + Size popovers

- Build `popovers/mod.rs` trigger helper.
- Implement `status.rs` and `size.rs` — these depend only on fields we already have after Slice 1.
- Add Fluent keys.
- Snapshot/visual QA: open, close, hover, focus-trap, Escape dismisses.

### Slice 3 — Response time popover + waterfall

- Implement `time.rs` with the hand-rolled waterfall.
- Use whatever phase data is available; missing phases render `—` + empty bar.
- Keep the widget allocation-free per frame — recompute layout only inside the `content(...)` closure when the popover is open.

### Slice 4 — Network / details popover

- Implement `network.rs`.
- When TLS data is unavailable (likely in first delivery), hide TLS blocks rather than rendering three `—` rows.
- When `tls_info(true)` work lands later, surface protocol / cipher / certificate fields without any further view changes.

### Slice 5 — Richer phase timings (optional, later)

- Only if transport evolves (see `request-console-plan.md §5.2`).
- No view changes required — the waterfall already handles `None` → `—`.

---

## 8. Test Plan

Unit:

- `ResponseSummary` serde round-trip (both with all fields set and all-`None`).
- Status-code description resolver falls back to class-level key for unknown codes.
- `format_bytes` / phase-width layout math is deterministic at zero / missing totals.

Integration (`tests/`):

- End-to-end execute → complete → assert new fields on `ResponseSummary` for an HTTP/2 target.
- Re-open the same request from history: popovers render the same values without re-sending.
- Graceful degradation: mock transport that reports only `total_ms` — waterfall still renders, all other phase rows show `—`.

Manual / visual:

- Hovering each trigger opens the correct popover; moving away dismisses after the grace delay.
- Moving the pointer from trigger into popover body keeps it open (no mid-travel flicker).
- Moving between two adjacent triggers swaps popovers without a visible gap.
- Only one popover is open at a time.
- Escape dismisses while any popover is focused via keyboard.
- Triggers are keyboard-reachable via Tab; focus opens the popover, blur closes it.

---

## 9. Open Questions

1. Should the status popover description table be a compiled-in Rust match, or Fluent keys only (with the description defined per-locale)? Leaning Fluent-only for localization parity, at the cost of one lookup per popover open.
2. Do we need a dedicated Phase 3 setting to enable `reqwest::ClientBuilder::tls_info(true)` by default, or behind a preference flag? Enabling it globally has a small perf cost on every request.
3. Should "Save Response" (already adjacent to the metadata bar) move *inside* the network popover as a "Download certificate chain" action as well? Out of scope for this plan.
4. For the waterfall, do we want interactive hover tooltips on each bar showing exact start/end ms? Nice-to-have; deferred to a follow-up if user feedback demands it.

---

## 10. Summary

Four popovers hang off the response metadata bar; each has a clear, stable `None`-friendly data contract on `ResponseSummary`. Slices 1 and 2 are low-risk and deliverable against the current reqwest transport. The waterfall is a hand-rolled `div` layout, not `gpui-component::chart::BarChart`, because the shape (offset-based horizontal spans) does not fit the chart primitive. Everything respects the existing i18n rule, the render-loop audit, and the Phase 3 transport-fidelity boundary already agreed in `request-console-plan.md`.
