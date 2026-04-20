# Response Transport Fork Plan (Slice 7: Items 1, 2, 4)

> Date: 2026-04-20  
> Scope: Define a concrete fork/transport plan for:
> 1. split connect into **TCP** and **TLS** timings,
> 2. expose negotiated **TLS protocol** and **cipher**,
> 4. true **on-wire** body size when `Content-Length` is missing.  
>
> Note: Item `3` from the earlier discussion (local socket address) is now implemented in current reqwest transport using response extensions (`HttpInfo.local_addr()`), so it is not part of this fork plan.

---

## 1. Goal

Add transport-grade observability that reqwest public APIs cannot fully provide:

- `tcp_ms` and `tls_ms` as separate phases (not merged `connect_ms`)
- negotiated TLS protocol and cipher suite
- accurate per-response wire-size metrics across:
  - HTTP/1.1 chunked transfer
  - compression/content-encoding
  - HTTP/2 and HTTP/3 framing

These fields already exist or can be added as `Option` fields in response metadata; this plan covers how to populate them reliably.

---

## 2. Current Limits (Why Fork/Custom Transport Is Needed)

Under reqwest public API:

- connect timing is observable only as one opaque step (`connector_layer`)
- `TlsInfo` exposes peer certificate only; no protocol/cipher
- wire body size is not reliably exposed for chunked/compressed/h2/h3 paths

Therefore, items `1`, `2`, and `4` require either:

1. a reqwest fork/patch, or
2. a custom hyper/tokio-rustls transport path.

---

## 3. Recommended Strategy

## 3.1 Phase A (Lowest disruption): Reqwest fork patch

Patch reqwest transport internals to emit additional connection metadata.

Deliverables:

- split connect telemetry:
  - `tcp_started_at`/`tcp_done_at`
  - `tls_started_at`/`tls_done_at`
- TLS negotiated details (rustls backend):
  - protocol version
  - cipher suite
- response extension metadata carrying the above, consumed by Torii transport layer

Why first:

- minimal blast radius in Torii
- preserves current request API and behavior
- fast validation path for UI and persistence changes

Risks:

- fork maintenance burden across reqwest updates
- native-tls backend parity may be partial

## 3.2 Phase B (Higher fidelity): Dedicated instrumented transport

Build a custom client stack on hyper + tokio-rustls (and optional h2/h3-specific hooks).

Deliverables:

- explicit phase boundaries for DNS/TCP/TLS/TTFB/download
- negotiated TLS details as first-class metadata
- wire-size counters by metric class:
  - representation bytes
  - transfer/frame bytes
  - encrypted transport bytes (optional advanced mode)

Why second:

- best long-term correctness and control
- avoids permanent reqwest fork dependency

Risks:

- largest implementation scope
- need feature-parity work (redirects, proxies, decompression policy, pooling behavior)

---

## 4. Workstreams

## 4.1 Item 1: Split TCP and TLS timings

### Reqwest fork path

- instrument connector internals where TCP and TLS occur as separate awaited steps
- persist:
  - `phase_timings.tcp_ms`
  - `phase_timings.tls_ms`
- keep merged `connect_ms` for backward compatibility (`tcp_ms + tls_ms`)

### Custom transport path

- own the connector stack and timestamp each boundary directly
- add connection-reuse semantics (`reused_connection = true`) to avoid misleading zeroes

Acceptance:

- HTTPS new connections show non-empty `tcp_ms` and `tls_ms`
- connection reuse does not fabricate handshake values

## 4.2 Item 2: TLS protocol and cipher

### Reqwest fork path

- extract from rustls session state after handshake
- map to normalized strings (e.g. `TLS1.3`, `TLS_AES_128_GCM_SHA256`)
- persist into `TlsSummary { protocol, cipher }`

### Custom transport path

- read directly from rustls connection object and attach to response metadata

Acceptance:

- TLS responses populate protocol+cipher
- plaintext HTTP leaves them `None`

## 4.3 Item 4: True on-wire body size

Define three separate metrics to avoid ambiguity:

- `body_decoded_bytes` (already present)
- `body_coded_bytes` (content-encoded payload bytes)
- `body_wire_bytes` (payload + transfer/framing; protocol-specific)

### Reqwest fork path

- instrument body decode/buffer layers where feasible
- caution: full per-response accuracy for h2/h3 multiplexed framing remains limited

### Custom transport path (recommended for correctness)

- collect bytes at framing layer per stream/response:
  - h1 chunk framing
  - h2 DATA/frame accounting
  - h3 stream/frame accounting

Acceptance:

- h1 chunked responses produce non-`None` wire-size without relying on `Content-Length`
- compressed responses keep decoded/coded/wire metrics distinct

---

## 5. Data Contract Changes

Add/confirm the following on response metadata:

- `PhaseTimings { tcp_ms: Option<u64>, tls_ms: Option<u64> }`
- `TlsSummary { protocol: Option<String>, cipher: Option<String> }`
- `ResponseSizeBreakdown` extension:
  - `body_coded_bytes: Option<u64>` (new)
  - `body_wire_bytes: Option<u64>` (existing semantic clarified)

All new fields remain optional and backward compatible.

---

## 6. Test Plan

Unit:

- TCP/TLS phase math and fallback semantics
- rustls protocol/cipher extraction mapping
- size-accounting logic for chunked/compressed/h2/h3 sample traces

Integration:

- HTTPS endpoint with known TLS version/cipher
- h1 chunked without `Content-Length`
- compressed response with decompression enabled/disabled modes
- reuse vs fresh connection behavior

Manual:

- popovers render new fields with `—` fallback when absent
- persisted history reopen matches live response metadata

---

## 7. Rollout

1. Land metadata schema and UI rendering guards (no behavior change).  
2. Implement reqwest-fork instrumentation for items 1+2.  
3. Add partial wire metrics and explicit metric labels (decoded/coded/wire).  
4. Evaluate accuracy gaps; if unacceptable, execute custom transport Phase B.

---

## 8. Decision Record

Current decision:

- implement item `3` now in mainline (done),
- isolate `1`, `2`, `4` behind fork/transport work because reqwest public API is insufficient for reliable coverage.
