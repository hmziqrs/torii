//! MockTransport — deterministic HTTP transport for integration tests.
//!
//! Controls timing (delays, drip-feeds bytes, fails on cue) so tests can
//! exercise cancel races, preview caps, and streaming without real network.

mod common;

use std::{
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
        request_repo::SqliteRequestRepository,
        workspace_repo::{SqliteWorkspaceRepository, WorkspaceRepository},
    },
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
    pub fn respond_with(&self, response: MockResponse) {
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
    pub fn captured(&self) -> Vec<CapturedRequest> {
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
        body: Option<bytes::Bytes>,
        cancel: CancellationToken,
    ) -> Result<TransportResponse> {
        let (response, captured) = {
            let mut inner = self.inner.lock().unwrap();
            let captured = CapturedRequest {
                method: method.to_string(),
                url: url.to_string(),
                headers: headers
                    .iter()
                    .map(|(n, v)| (n.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect(),
                body: body.map(|b| b.to_vec()),
            };
            inner.captured.push(captured);
            (
                inner.next_response.clone(),
                inner.captured.last().unwrap().clone(),
            )
        };

        // If programmed to fail, do so
        if let Some(delay) = response.fail_after {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mock_transport_simple_get() {
    let mock = MockTransport::new();
    mock.respond_ok(b"hello world");

    let (ws_repo, col_repo, _req_repo, _hist_repo, _blob, _secrets, exec) =
        build_test_services(mock.clone());

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let request = simple_request("GET", "https://api.test/echo", col.id);

    let cancel = CancellationToken::new();
    let outcome = exec.execute(&request, ws.id, cancel).await.unwrap();

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

#[tokio::test]
async fn mock_transport_captures_headers_and_body() {
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
    let _ = exec.execute(&request, ws.id, cancel).await.unwrap();

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

#[tokio::test]
async fn mock_transport_cancel_during_stream() {
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
    let handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        cancel_clone.cancel();
    });

    let outcome = exec.execute(&request, ws.id, cancel).await.unwrap();
    handle.await.unwrap();

    // Should be cancelled or completed (if it finished before cancel)
    match outcome {
        ExecOutcome::Cancelled { .. } | ExecOutcome::Completed { .. } => {}
        other => panic!("expected Cancelled or Completed, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_transport_preflight_invalid_url() {
    let mock = MockTransport::new();
    mock.respond_ok(b"");

    let (ws_repo, col_repo, _req_repo, _hist_repo, _blob, _secrets, exec) =
        build_test_services(mock.clone());

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let request = simple_request("GET", "not a valid url :::///", col.id);

    let cancel = CancellationToken::new();
    let outcome = exec.execute(&request, ws.id, cancel).await.unwrap();

    match outcome {
        ExecOutcome::PreflightFailed(msg) => {
            assert!(msg.contains("invalid URL"), "unexpected message: {msg}");
        }
        other => panic!("expected PreflightFailed, got {other:?}"),
    }
    // No request should have been sent
    assert_eq!(mock.request_count(), 0);
}

#[tokio::test]
async fn mock_transport_error_response() {
    let mock = MockTransport::new();
    mock.respond_error(Duration::from_millis(10));

    let (ws_repo, col_repo, _req_repo, _hist_repo, _blob, _secrets, exec) =
        build_test_services(mock);

    let ws = ws_repo.create("WS").unwrap();
    let col = col_repo.create(ws.id, "Col").unwrap();
    let request = simple_request("GET", "https://api.test/fail", col.id);

    let cancel = CancellationToken::new();
    let outcome = exec.execute(&request, ws.id, cancel).await.unwrap();

    match outcome {
        ExecOutcome::Failed(msg) => {
            assert!(msg.contains("mock transport error"));
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_transport_bearer_auth_injection() {
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
    let outcome = exec.execute(&request, ws.id, cancel).await.unwrap();

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

#[tokio::test]
async fn mock_transport_preview_cap_large_response() {
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
    let outcome = exec.execute(&request, ws.id, cancel).await.unwrap();

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

#[tokio::test]
async fn mock_transport_status_codes() {
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
        let outcome = exec.execute(&request, ws.id, cancel).await.unwrap();

        match outcome {
            ExecOutcome::Completed(summary) => {
                assert_eq!(summary.status_code, code);
                assert_eq!(summary.status_text, text);
            }
            other => panic!("expected Completed for {code}, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn mock_transport_empty_body() {
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
    let outcome = exec.execute(&request, ws.id, cancel).await.unwrap();

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
