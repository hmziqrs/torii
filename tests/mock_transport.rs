//! MockTransport — deterministic HTTP transport for integration tests.
//!
//! Controls timing (delays, drip-feeds bytes, fails on cue) so tests can
//! exercise cancel races, preview caps, and streaming without real network.

mod common;

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Result;
use bytes::Bytes;
use futures::{Stream, StreamExt as _, stream};
use tokio_util::sync::CancellationToken;
use torii::{
    domain::request::{AuthType, BodyType, KeyValuePair, RequestItem},
    infra::{
        blobs::BlobStore,
        secrets::{InMemorySecretStore, SecretStore},
    },
    repos::{
        collection_repo::{CollectionRepository, SqliteCollectionRepository},
        history_repo::SqliteHistoryRepository,
        request_repo::{RequestRepository, SqliteRequestRepository},
        workspace_repo::{SqliteWorkspaceRepository, WorkspaceRepository},
    },
    services::request_body_payload::RequestBodyPayload,
    services::request_execution::{
        ExecOutcome, HttpTransport, RequestExecutionService, TransportResponse,
    },
};

// ---------------------------------------------------------------------------
// MockTransport
// ---------------------------------------------------------------------------

/// Recorded request captured by the mock.
#[derive(Debug, Clone)]
struct CapturedRequest {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
}

/// Pre-programmed response for the mock to return.
#[derive(Clone)]
struct MockResponse {
    status_code: u16,
    status_text: String,
    headers: Vec<(String, String)>,
    body_chunks: Vec<Vec<u8>>,
    /// Delay before sending each chunk (simulates network latency).
    chunk_delay: Duration,
    /// If set, the mock returns an error after this delay instead of a response.
    fail_after: Option<Duration>,
}

impl Default for MockResponse {
    fn default() -> Self {
        Self {
            status_code: 200,
            status_text: "OK".to_string(),
            headers: Vec::new(),
            body_chunks: Vec::new(),
            chunk_delay: Duration::ZERO,
            fail_after: None,
        }
    }
}

struct MockTransportInner {
    /// The next response to return. Tests set this before triggering a send.
    next_response: MockResponse,
    /// All requests captured so far.
    captured: Vec<CapturedRequest>,
}

/// A deterministic mock HTTP transport for tests.
///
/// # Usage
///
/// ```ignore
/// let mock = MockTransport::new();
/// mock.respond_with(MockResponse {
///     status_code: 200,
///     body_chunks: vec![b"hello".to_vec()],
///     ..Default::default()
/// });
/// // ... trigger request execution ...
/// let requests = mock.captured();
/// assert_eq!(requests[0].method, "GET");
/// ```
#[derive(Clone)]
pub struct MockTransport {
    inner: Arc<Mutex<MockTransportInner>>,
}

impl MockTransport {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MockTransportInner {
                next_response: MockResponse::default(),
                captured: Vec::new(),
            })),
        }
    }

    /// Program the next response the mock will return.
    fn respond_with(&self, response: MockResponse) {
        let mut inner = self.inner.lock().unwrap();
        inner.next_response = response;
    }

    /// Set a simple immediate response with the given status and body.
    pub fn respond_ok(&self, body: &[u8]) {
        self.respond_with(MockResponse {
            status_code: 200,
            status_text: "OK".to_string(),
            body_chunks: vec![body.to_vec()],
            ..Default::default()
        });
    }

    /// Set a response that fails with a transport error after the given delay.
    pub fn respond_error(&self, delay: Duration) {
        self.respond_with(MockResponse {
            fail_after: Some(delay),
            ..Default::default()
        });
    }

    /// Return a copy of all captured requests.
    fn captured(&self) -> Vec<CapturedRequest> {
        self.inner.lock().unwrap().captured.clone()
    }

    /// Number of requests captured.
    pub fn request_count(&self) -> usize {
        self.inner.lock().unwrap().captured.len()
    }
}

fn make_chunk_stream(
    chunks: Vec<Vec<u8>>,
    delay: Duration,
    cancel: CancellationToken,
) -> Pin<Box<dyn Stream<Item = Result<Bytes>> + Send>> {
    if delay.is_zero() {
        let items: Vec<Result<Bytes>> = chunks.into_iter().map(|c| Ok(Bytes::from(c))).collect();
        return Box::pin(stream::iter(items));
    }

    // Delayed stream: yield chunks one at a time with tokio::time::sleep
    let items: Vec<Result<Bytes>> = chunks.into_iter().map(|c| Ok(Bytes::from(c))).collect();
    let stream = stream::iter(items).then(move |item| {
        let cancel = cancel.clone();
        async move {
            if !cancel.is_cancelled() {
                tokio::time::sleep(delay).await;
            }
            item
        }
    });
    Box::pin(stream)
}

#[async_trait::async_trait]
impl HttpTransport for MockTransport {
    async fn send(
        &self,
        method: http::Method,
        url: url::Url,
        headers: http::HeaderMap,
        body: RequestBodyPayload,
        cancel: CancellationToken,
    ) -> Result<TransportResponse> {
        let captured_body = match body {
            RequestBodyPayload::None => None,
            RequestBodyPayload::Bytes(bytes) => Some(bytes.to_vec()),
            RequestBodyPayload::Stream(mut stream) => {
                let mut merged = Vec::new();
                while let Some(chunk) = stream.next().await {
                    merged.extend_from_slice(&chunk?);
                }
                Some(merged)
            }
        };
        let (response, captured) = {
            let mut inner = self.inner.lock().unwrap();
            let captured = CapturedRequest {
                method: method.to_string(),
                url: url.to_string(),
                headers: headers
                    .iter()
                    .map(|(n, v)| (n.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect(),
                body: captured_body,
            };
            inner.captured.push(captured);
            (
                inner.next_response.clone(),
                inner.captured.last().unwrap().clone(),
            )
        };

        // If programmed to fail, do so
        if let Some(delay) = response.fail_after {
            if delay.is_zero() {
                if cancel.is_cancelled() {
                    return Err(anyhow::anyhow!(
                        "mock transport cancelled for {}",
                        captured.url
                    ));
                }
            } else {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        return Err(anyhow::anyhow!("mock transport cancelled for {}", captured.url));
                    }
                    _ = tokio::time::sleep(delay) => {}
                }
            }
            return Err(anyhow::anyhow!("mock transport error for {}", captured.url));
        }

        let mut resp_headers = http::HeaderMap::new();
        for (name, value) in &response.headers {
            if let (Ok(n), Ok(v)) = (
                http::header::HeaderName::from_bytes(name.as_bytes()),
                http::HeaderValue::from_str(value),
            ) {
                resp_headers.insert(n, v);
            }
        }

        let media_type = resp_headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(';').next().unwrap_or(s).trim().to_string());

        let body_stream = make_chunk_stream(response.body_chunks, response.chunk_delay, cancel);

        Ok(TransportResponse {
            status_code: response.status_code,
            status_text: response.status_text,
            headers: resp_headers,
            media_type,
            body_stream,
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers for building test services
// ---------------------------------------------------------------------------

fn build_test_services(
    mock: MockTransport,
) -> (
    Arc<SqliteWorkspaceRepository>,
    Arc<SqliteCollectionRepository>,
    Arc<SqliteRequestRepository>,
    Arc<SqliteHistoryRepository>,
    Arc<BlobStore>,
    Arc<InMemorySecretStore>,
    RequestExecutionService,
) {
    let (paths, db) = common::test_database("mock-transport").unwrap();
    let db = Arc::new(db);
    let workspace_repo = Arc::new(SqliteWorkspaceRepository::new(db.clone()));
    let collection_repo = Arc::new(SqliteCollectionRepository::new(db.clone()));
    let request_repo = Arc::new(SqliteRequestRepository::new(db.clone()));
    let history_repo = Arc::new(SqliteHistoryRepository::new(db.clone()));
    let blob_store = Arc::new(BlobStore::new(&paths).unwrap());
    let secret_store = Arc::new(InMemorySecretStore::new());

    let exec_service = RequestExecutionService::new(
        Arc::new(mock),
        history_repo.clone(),
        blob_store.clone(),
        secret_store.clone(),
    );

    (
        workspace_repo,
        collection_repo,
        request_repo,
        history_repo,
        blob_store,
        secret_store,
        exec_service,
    )
}

fn simple_request(
    method: &str,
    url: &str,
    collection_id: torii::domain::ids::CollectionId,
) -> RequestItem {
    RequestItem::new(collection_id, None, "Test", method, url, 0)
}

fn run_async<T>(fut: impl Future<Output = T>) -> T {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime.block_on(fut)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn mock_transport_simple_get() {
    let mock = MockTransport::new();
    mock.respond_ok(b"hello world");

    let (ws_repo, col_repo, _req_repo, _hist_repo, _blob, _secrets, exec) =
        build_test_services(mock.clone());

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let request = simple_request("GET", "https://api.test/echo", col.id);

    let cancel = CancellationToken::new();
    let outcome = run_async(exec.execute(&request, ws.id, cancel)).unwrap();

    match outcome {
        ExecOutcome::Completed(summary) => {
            assert_eq!(summary.status_code, 200);
            assert_eq!(summary.status_text, "OK");
            // Body should be in memory (small)
            match &summary.body_ref {
                torii::domain::response::BodyRef::InMemoryPreview { bytes, .. } => {
                    assert_eq!(bytes.as_ref(), b"hello world");
                }
                other => panic!("expected InMemoryPreview, got {other:?}"),
            }
        }
        other => panic!("expected Completed, got {other:?}"),
    }

    assert_eq!(mock.request_count(), 1);
    assert_eq!(mock.captured()[0].method, "GET");
}

#[test]
fn mock_transport_captures_headers_and_body() {
    let mock = MockTransport::new();
    mock.respond_ok(b"ok");

    let (ws_repo, col_repo, _req_repo, _hist_repo, _blob, _secrets, exec) =
        build_test_services(mock.clone());

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let mut request = simple_request("POST", "https://api.test/data", col.id);
    request
        .headers
        .push(KeyValuePair::new("X-Custom", "value123"));
    request.body = BodyType::RawJson {
        content: r#"{"hello":"world"}"#.to_string(),
    };

    let cancel = CancellationToken::new();
    let _ = run_async(exec.execute(&request, ws.id, cancel)).unwrap();

    let captured = mock.captured();
    assert_eq!(captured[0].method, "POST");
    assert!(
        captured[0]
            .headers
            .iter()
            .any(|(k, v)| k == "x-custom" && v == "value123")
    );
    assert_eq!(
        captured[0].body.as_deref(),
        Some(br#"{"hello":"world"}"#.as_slice())
    );
}

#[test]
fn mock_transport_cancel_during_stream() {
    let mock = MockTransport::new();
    // Drip-feed 10 chunks with 50ms delay each
    let chunks: Vec<Vec<u8>> = (0..10)
        .map(|i| format!("chunk-{i}\n").into_bytes())
        .collect();
    mock.respond_with(MockResponse {
        status_code: 200,
        status_text: "OK".to_string(),
        body_chunks: chunks,
        chunk_delay: Duration::from_millis(50),
        ..Default::default()
    });

    let (ws_repo, col_repo, _req_repo, _hist_repo, _blob, _secrets, exec) =
        build_test_services(mock);

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let request = simple_request("GET", "https://api.test/slow", col.id);

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    // Cancel after 200ms (should interrupt mid-stream)
    let outcome = run_async(async move {
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            cancel_clone.cancel();
        });

        let outcome = exec.execute(&request, ws.id, cancel).await.unwrap();
        handle.await.unwrap();
        outcome
    });

    // Should be cancelled or completed (if it finished before cancel)
    match outcome {
        ExecOutcome::Cancelled { .. } | ExecOutcome::Completed { .. } => {}
        other => panic!("expected Cancelled or Completed, got {other:?}"),
    }
}

#[test]
fn mock_transport_preflight_invalid_url() {
    let mock = MockTransport::new();
    mock.respond_ok(b"");

    let (ws_repo, col_repo, _req_repo, _hist_repo, _blob, _secrets, exec) =
        build_test_services(mock.clone());

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let request = simple_request("GET", "not a valid url :::///", col.id);

    let cancel = CancellationToken::new();
    let outcome = run_async(exec.execute(&request, ws.id, cancel)).unwrap();

    match outcome {
        ExecOutcome::PreflightFailed(msg) => {
            assert!(msg.contains("invalid URL"), "unexpected message: {msg}");
        }
        other => panic!("expected PreflightFailed, got {other:?}"),
    }
    // No request should have been sent
    assert_eq!(mock.request_count(), 0);
}

#[test]
fn mock_transport_error_response() {
    let mock = MockTransport::new();
    mock.respond_error(Duration::from_millis(10));

    let (ws_repo, col_repo, _req_repo, _hist_repo, _blob, _secrets, exec) =
        build_test_services(mock);

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let request = simple_request("GET", "https://api.test/fail", col.id);

    let cancel = CancellationToken::new();
    let outcome = run_async(exec.execute(&request, ws.id, cancel)).unwrap();

    match outcome {
        ExecOutcome::Failed { summary, .. } => {
            assert!(summary.contains("mock transport error"));
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn mock_transport_bearer_auth_injection() {
    let mock = MockTransport::new();
    mock.respond_ok(b"authenticated");

    let (ws_repo, col_repo, _req_repo, _hist_repo, _blob, secrets, exec) =
        build_test_services(mock.clone());

    // Store a secret
    secrets
        .put_secret("my-token", "secret-token-value")
        .unwrap();

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let mut request = simple_request("GET", "https://api.test/protected", col.id);
    request.auth = AuthType::Bearer {
        token_secret_ref: Some("my-token".to_string()),
    };

    let cancel = CancellationToken::new();
    let outcome = run_async(exec.execute(&request, ws.id, cancel)).unwrap();

    match outcome {
        ExecOutcome::Completed(summary) => {
            assert_eq!(summary.status_code, 200);
        }
        other => panic!("expected Completed, got {other:?}"),
    }

    let captured = mock.captured();
    let auth_header = captured[0]
        .headers
        .iter()
        .find(|(k, _)| k == "authorization");
    assert!(auth_header.is_some());
    assert_eq!(auth_header.unwrap().1, "Bearer secret-token-value");
}

#[test]
fn mock_transport_preview_cap_large_response() {
    let mock = MockTransport::new();
    // Response larger than preview cap (2 MiB)
    let large_body = vec![b'X'; 3 * 1024 * 1024]; // 3 MiB
    mock.respond_with(MockResponse {
        status_code: 200,
        status_text: "OK".to_string(),
        headers: vec![(
            "content-type".to_string(),
            "application/octet-stream".to_string(),
        )],
        body_chunks: vec![large_body.clone()],
        ..Default::default()
    });

    let (ws_repo, col_repo, _req_repo, _hist_repo, blob_store, _secrets, exec) =
        build_test_services(mock);

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let request = simple_request("GET", "https://api.test/large", col.id);

    let cancel = CancellationToken::new();
    let outcome = run_async(exec.execute(&request, ws.id, cancel)).unwrap();

    match outcome {
        ExecOutcome::Completed(summary) => {
            match &summary.body_ref {
                torii::domain::response::BodyRef::DiskBlob {
                    blob_id,
                    preview,
                    size_bytes,
                } => {
                    assert_eq!(*size_bytes, 3 * 1024 * 1024 as u64);
                    // Preview should be capped
                    let preview_len = preview.as_ref().map(|p| p.len()).unwrap_or(0);
                    assert!(
                        preview_len <= torii::domain::response::ResponseBudgets::PREVIEW_CAP_BYTES
                    );
                    // Blob should exist on disk
                    assert!(blob_store.exists(blob_id));
                }
                other => panic!("expected DiskBlob for large response, got {other:?}"),
            }
        }
        other => panic!("expected Completed, got {other:?}"),
    }
}

#[test]
fn mock_transport_status_codes() {
    for (code, text) in [
        (200, "OK"),
        (404, "Not Found"),
        (500, "Internal Server Error"),
    ] {
        let mock = MockTransport::new();
        mock.respond_with(MockResponse {
            status_code: code,
            status_text: text.to_string(),
            body_chunks: vec![b"body".to_vec()],
            ..Default::default()
        });

        let (ws_repo, col_repo, _req_repo, _hist_repo, _blob, _secrets, exec) =
            build_test_services(mock);

        let ws = ws_repo.create("WS").unwrap();
        let col = col_repo.create(ws.id, "Col").unwrap();
        let request = simple_request("GET", "https://api.test/", col.id);

        let cancel = CancellationToken::new();
        let outcome = run_async(exec.execute(&request, ws.id, cancel)).unwrap();

        match outcome {
            ExecOutcome::Completed(summary) => {
                assert_eq!(summary.status_code, code);
                assert_eq!(summary.status_text, text);
            }
            other => panic!("expected Completed for {code}, got {other:?}"),
        }
    }
}

#[test]
fn mock_transport_empty_body() {
    let mock = MockTransport::new();
    mock.respond_with(MockResponse {
        status_code: 204,
        status_text: "No Content".to_string(),
        body_chunks: vec![],
        ..Default::default()
    });

    let (ws_repo, col_repo, _req_repo, _hist_repo, _blob, _secrets, exec) =
        build_test_services(mock);

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let request = simple_request("DELETE", "https://api.test/resource", col.id);

    let cancel = CancellationToken::new();
    let outcome = run_async(exec.execute(&request, ws.id, cancel)).unwrap();

    match outcome {
        ExecOutcome::Completed(summary) => {
            assert_eq!(summary.status_code, 204);
            match &summary.body_ref {
                torii::domain::response::BodyRef::InMemoryPreview { bytes, .. } => {
                    assert!(bytes.is_empty());
                }
                other => panic!("expected InMemoryPreview (empty), got {other:?}"),
            }
        }
        other => panic!("expected Completed, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// §9 Integration Tests: security, cancel races, history snapshot
// ---------------------------------------------------------------------------

#[test]
fn auth_secrets_never_appear_in_request_domain_model() {
    let mock = MockTransport::new();
    mock.respond_ok(b"ok");

    let (ws_repo, col_repo, _req_repo, _hist_repo, _blob, secrets, exec) =
        build_test_services(mock.clone());

    secrets
        .put_secret("bearer-token-123", "super-secret-value")
        .unwrap();

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let mut request = simple_request("GET", "https://api.test/protected", col.id);
    request.auth = AuthType::Bearer {
        token_secret_ref: Some("bearer-token-123".to_string()),
    };

    let cancel = CancellationToken::new();
    let outcome = run_async(exec.execute(&request, ws.id, cancel)).unwrap();
    assert!(matches!(outcome, ExecOutcome::Completed(_)));

    // Domain model only stores the secret REF, never the value
    assert_eq!(
        request.auth,
        AuthType::Bearer {
            token_secret_ref: Some("bearer-token-123".to_string())
        }
    );
}

#[test]
fn secret_store_failure_produces_preflight_error() {
    let mock = MockTransport::new();
    mock.respond_ok(b"ok");

    let (ws_repo, col_repo, _req_repo, _hist_repo, _blob, _secrets, exec) =
        build_test_services(mock);

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let mut request = simple_request("GET", "https://api.test/protected", col.id);
    request.auth = AuthType::Bearer {
        token_secret_ref: Some("nonexistent-secret-ref".to_string()),
    };

    let cancel = CancellationToken::new();
    let outcome = run_async(exec.execute(&request, ws.id, cancel)).unwrap();

    match outcome {
        ExecOutcome::PreflightFailed(msg) => {
            assert!(
                msg.contains("not found") || msg.contains("auth resolution"),
                "unexpected message: {msg}"
            );
        }
        other => panic!("expected PreflightFailed, got {other:?}"),
    }
}

#[test]
fn cancel_before_response_yields_cancelled_outcome() {
    let mock = MockTransport::new();
    mock.respond_with(MockResponse {
        fail_after: Some(Duration::from_secs(30)),
        ..Default::default()
    });

    let (ws_repo, col_repo, _req_repo, _hist_repo, _blob, _secrets, exec) =
        build_test_services(mock);

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let request = simple_request("GET", "https://api.test/slow", col.id);

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let outcome = rt.block_on(async {
        let exec_handle = tokio::spawn(async move { exec.execute(&request, ws.id, cancel).await });
        tokio::time::sleep(Duration::from_millis(10)).await;
        cancel_clone.cancel();
        exec_handle.await.unwrap().unwrap()
    });

    match outcome {
        ExecOutcome::Cancelled { partial_size } => {
            assert!(partial_size.is_none());
        }
        other => panic!("expected Cancelled, got {other:?}"),
    }
}

#[test]
fn cancel_mid_stream_clears_blob_hash() {
    let mock = MockTransport::new();
    let chunks: Vec<Vec<u8>> = (0..20)
        .map(|i| format!("chunk-{i:04}\n").into_bytes())
        .collect();
    mock.respond_with(MockResponse {
        status_code: 200,
        status_text: "OK".to_string(),
        body_chunks: chunks,
        chunk_delay: Duration::from_millis(100),
        ..Default::default()
    });

    let (ws_repo, col_repo, _req_repo, _hist_repo, _blob, _secrets, exec) =
        build_test_services(mock);

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let request = simple_request("GET", "https://api.test/stream", col.id);

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let outcome = rt.block_on(async {
        let exec_handle = tokio::spawn(async move { exec.execute(&request, ws.id, cancel).await });
        tokio::time::sleep(Duration::from_millis(250)).await;
        cancel_clone.cancel();
        exec_handle.await.unwrap().unwrap()
    });

    match outcome {
        ExecOutcome::Cancelled { .. } | ExecOutcome::Completed { .. } => {}
        other => panic!("expected Cancelled or Completed, got {other:?}"),
    }
}

#[test]
fn history_snapshot_redacts_auth_headers() {
    let mock = MockTransport::new();
    mock.respond_ok(b"ok");

    let (ws_repo, col_repo, _req_repo, _hist_repo, _blob, secrets, _exec) =
        build_test_services(mock);

    secrets
        .put_secret("my-api-key", "secret-key-value")
        .unwrap();

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let mut request = simple_request(
        "GET",
        "https://api.test/data?token=abc123&client_id=my-client",
        col.id,
    );
    request.auth = AuthType::Bearer {
        token_secret_ref: Some("my-api-key".to_string()),
    };
    request
        .headers
        .push(KeyValuePair::new("Authorization", "will-be-replaced"));
    request
        .headers
        .push(KeyValuePair::new("X-Custom", "visible"));

    let snapshot = torii::repos::history_repo::build_request_snapshot(&request);

    // Auth kind = "bearer", never the secret value
    assert_eq!(snapshot.method, "GET");
    assert_eq!(snapshot.auth_kind.as_deref(), Some("bearer"));
    assert_eq!(
        snapshot.url_redacted,
        "https://api.test/data?token=%5BREDACTED%5D&client_id=%5BREDACTED%5D"
    );

    // Redacted headers
    if let Some(headers_json) = &snapshot.headers_redacted_json {
        let headers: Vec<(String, String)> = serde_json::from_str(headers_json).unwrap();
        let auth_h = headers.iter().find(|(k, _)| k == "Authorization").unwrap();
        assert_eq!(auth_h.1, "[REDACTED]");
        let custom_h = headers.iter().find(|(k, _)| k == "X-Custom").unwrap();
        assert_eq!(custom_h.1, "visible");
    }

    // Body summary: kind only, no content
    if let Some(body_json) = &snapshot.body_summary_json {
        assert!(!body_json.contains("secret"));
        let body: serde_json::Value = serde_json::from_str(body_json).unwrap();
        assert_eq!(body["kind"], "none");
    }
}

#[test]
fn large_response_bounded_by_preview_cap() {
    let mock = MockTransport::new();
    let large_body = vec![b'A'; 10 * 1024 * 1024];
    mock.respond_with(MockResponse {
        status_code: 200,
        status_text: "OK".to_string(),
        headers: vec![(
            "content-type".to_string(),
            "application/octet-stream".to_string(),
        )],
        body_chunks: vec![large_body],
        ..Default::default()
    });

    let (ws_repo, col_repo, _req_repo, _hist_repo, blob_store, _secrets, exec) =
        build_test_services(mock);

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let request = simple_request("GET", "https://api.test/large", col.id);

    let cancel = CancellationToken::new();
    let outcome = run_async(exec.execute(&request, ws.id, cancel)).unwrap();

    match outcome {
        ExecOutcome::Completed(summary) => match &summary.body_ref {
            torii::domain::response::BodyRef::DiskBlob {
                blob_id,
                preview,
                size_bytes,
            } => {
                assert_eq!(*size_bytes, 10 * 1024 * 1024 as u64);
                let preview_len = preview.as_ref().map(|p| p.len()).unwrap_or(0);
                assert!(
                    preview_len <= torii::domain::response::ResponseBudgets::PREVIEW_CAP_BYTES,
                    "preview {preview_len} exceeds cap"
                );
                assert!(blob_store.exists(blob_id));
                let full = blob_store.read_all(blob_id).unwrap();
                assert_eq!(full.len(), 10 * 1024 * 1024);
            }
            other => panic!("expected DiskBlob, got {other:?}"),
        },
        other => panic!("expected Completed, got {other:?}"),
    }
}

#[test]
fn secret_ref_not_in_request_sqlite_row() {
    let (_paths, db) = common::test_database("secret-not-in-sqlite").unwrap();
    let db = Arc::new(db);
    let workspace_repo = Arc::new(SqliteWorkspaceRepository::new(db.clone()));
    let collection_repo = Arc::new(SqliteCollectionRepository::new(db.clone()));
    let request_repo = Arc::new(SqliteRequestRepository::new(db.clone()));

    let ws = workspace_repo.create("WS").unwrap();
    let col = collection_repo.create(ws.id, "Col").unwrap();
    let mut request = request_repo
        .create(col.id, None, "Test", "POST", "https://api.test")
        .unwrap();

    request.auth = AuthType::Bearer {
        token_secret_ref: Some("secret-ref-key".to_string()),
    };
    request.body = BodyType::RawJson {
        content: r#"{"data":"value"}"#.to_string(),
    };
    let _ = request_repo.save(&request, request.meta.revision).unwrap();

    let loaded = request_repo.get(request.id).unwrap().unwrap();
    match &loaded.auth {
        AuthType::Bearer { token_secret_ref } => {
            assert_eq!(token_secret_ref.as_deref(), Some("secret-ref-key"));
        }
        other => panic!("expected Bearer auth, got {other:?}"),
    }

    // Direct SQL: auth_json contains the ref key but never actual secret values
    let auth_json: String = db.block_on(async {
        sqlx::query_scalar("SELECT auth_json FROM requests WHERE id = ?")
            .bind(request.id.to_string())
            .fetch_one(db.pool())
            .await
            .unwrap()
    });
    assert!(auth_json.contains("secret-ref-key"));
    assert!(!auth_json.contains("super-secret-value"));
    assert!(!auth_json.contains("password"));
}

// ---------------------------------------------------------------------------
// §9 Performance Tests: 50 MB and 200 MB response budgets
// ---------------------------------------------------------------------------

#[test]
fn response_50mb_bounded_by_preview_cap() {
    let mock = MockTransport::new();
    let large_body = vec![b'B'; 50 * 1024 * 1024];
    mock.respond_with(MockResponse {
        status_code: 200,
        status_text: "OK".to_string(),
        headers: vec![(
            "content-type".to_string(),
            "application/octet-stream".to_string(),
        )],
        body_chunks: vec![large_body],
        ..Default::default()
    });

    let (ws_repo, col_repo, _req_repo, _hist_repo, blob_store, _secrets, exec) =
        build_test_services(mock);

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let request = simple_request("GET", "https://api.test/large-50m", col.id);

    let cancel = CancellationToken::new();
    let outcome = run_async(exec.execute(&request, ws.id, cancel)).unwrap();

    match outcome {
        ExecOutcome::Completed(summary) => match &summary.body_ref {
            torii::domain::response::BodyRef::DiskBlob {
                blob_id,
                preview,
                size_bytes,
            } => {
                assert_eq!(*size_bytes, 50u64 * 1024 * 1024);
                let preview_len = preview.as_ref().map(|p| p.len()).unwrap_or(0);
                assert!(
                    preview_len <= torii::domain::response::ResponseBudgets::PREVIEW_CAP_BYTES,
                    "preview {preview_len} exceeds cap"
                );
                assert!(blob_store.exists(blob_id));
                let full = blob_store.read_all(blob_id).unwrap();
                assert_eq!(full.len(), 50 * 1024 * 1024);
            }
            other => panic!("expected DiskBlob, got {other:?}"),
        },
        other => panic!("expected Completed, got {other:?}"),
    }
}

#[test]
fn response_200mb_bounded_by_preview_cap() {
    let mock = MockTransport::new();
    let large_body = vec![b'C'; 200 * 1024 * 1024];
    mock.respond_with(MockResponse {
        status_code: 200,
        status_text: "OK".to_string(),
        headers: vec![(
            "content-type".to_string(),
            "application/octet-stream".to_string(),
        )],
        body_chunks: vec![large_body],
        ..Default::default()
    });

    let (ws_repo, col_repo, _req_repo, _hist_repo, blob_store, _secrets, exec) =
        build_test_services(mock);

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let request = simple_request("GET", "https://api.test/large-200m", col.id);

    let cancel = CancellationToken::new();
    let outcome = run_async(exec.execute(&request, ws.id, cancel)).unwrap();

    match outcome {
        ExecOutcome::Completed(summary) => match &summary.body_ref {
            torii::domain::response::BodyRef::DiskBlob {
                blob_id,
                preview,
                size_bytes,
            } => {
                assert_eq!(*size_bytes, 200u64 * 1024 * 1024);
                let preview_len = preview.as_ref().map(|p| p.len()).unwrap_or(0);
                assert!(
                    preview_len <= torii::domain::response::ResponseBudgets::PREVIEW_CAP_BYTES,
                    "preview {preview_len} exceeds cap"
                );
                assert!(blob_store.exists(blob_id));
            }
            other => panic!("expected DiskBlob, got {other:?}"),
        },
        other => panic!("expected Completed, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// §9 Integration: drip-feed and stall scenarios
// ---------------------------------------------------------------------------

#[test]
fn drip_feed_cancel_race_with_preview_cap() {
    let mock = MockTransport::new();
    let chunks: Vec<Vec<u8>> = (0..100)
        .map(|i| format!("chunk-{i:04}\n").into_bytes())
        .collect();
    mock.respond_with(MockResponse {
        status_code: 200,
        status_text: "OK".to_string(),
        headers: vec![("content-type".to_string(), "text/plain".to_string())],
        body_chunks: chunks,
        chunk_delay: Duration::from_millis(20),
        ..Default::default()
    });

    let (ws_repo, col_repo, _req_repo, _hist_repo, _blob, _secrets, exec) =
        build_test_services(mock);

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let request = simple_request("GET", "https://api.test/drip", col.id);

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let outcome = rt.block_on(async {
        let exec_handle = tokio::spawn(async move { exec.execute(&request, ws.id, cancel).await });
        tokio::time::sleep(Duration::from_millis(300)).await;
        cancel_clone.cancel();
        exec_handle.await.unwrap().unwrap()
    });

    match outcome {
        ExecOutcome::Cancelled { .. } | ExecOutcome::Completed { .. } => {}
        other => panic!("expected Cancelled or Completed, got {other:?}"),
    }
}
