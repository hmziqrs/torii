use std::{pin::Pin, sync::Arc, time::Instant};

use anyhow::{Context as _, Result, anyhow};
use base64::Engine as _;
use bytes::Bytes;
use futures::StreamExt as _;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::{
    domain::{
        ids::WorkspaceId,
        request::{AuthType, RequestItem},
        response::{BodyRef, ResponseBudgets, ResponseSummary},
    },
    infra::{blobs::BlobStore, secrets::SecretStoreRef},
    repos::history_repo::HistoryRepoRef,
};

// ---------------------------------------------------------------------------
// HttpTransport trait (for testability)
// ---------------------------------------------------------------------------

/// Abstraction over HTTP transport so tests can inject a MockTransport.
#[async_trait::async_trait]
pub trait HttpTransport: Send + Sync {
    async fn send(
        &self,
        method: http::Method,
        url: url::Url,
        headers: http::HeaderMap,
        body: Option<bytes::Bytes>,
        cancel: CancellationToken,
    ) -> Result<TransportResponse>;
}

pub struct TransportResponse {
    pub status_code: u16,
    pub status_text: String,
    pub headers: http::HeaderMap,
    pub media_type: Option<String>,
    pub body_stream: Pin<Box<dyn futures::Stream<Item = Result<Bytes>> + Send>>,
}

// ---------------------------------------------------------------------------
// ReqwestTransport (production)
// ---------------------------------------------------------------------------

pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::limited(10))
            .tls_built_in_root_certs(true)
            .no_proxy()
            .build()
            .context("failed to build reqwest client")?;
        Ok(Self { client })
    }
}

#[async_trait::async_trait]
impl HttpTransport for ReqwestTransport {
    async fn send(
        &self,
        method: http::Method,
        url: url::Url,
        headers: http::HeaderMap,
        body: Option<bytes::Bytes>,
        cancel: CancellationToken,
    ) -> Result<TransportResponse> {
        let mut builder = self.client.request(method, url.as_str());
        for (name, value) in &headers {
            builder = builder.header(name, value);
        }
        if let Some(body) = body {
            builder = builder.body(reqwest::Body::from(body));
        }

        let send_fut = builder.send();
        tokio::pin!(send_fut);
        let response = tokio::select! {
            _ = cancel.cancelled() => {
                return Err(anyhow!("request send cancelled"));
            }
            result = &mut send_fut => {
                result.context("request send failed")?
            }
        };

        let status_code = response.status().as_u16();
        let status_text = response
            .status()
            .canonical_reason()
            .unwrap_or("")
            .to_string();

        let resp_headers = response.headers().clone();
        let media_type = response
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(';').next().unwrap_or(s).trim().to_string());

        let stream = response
            .bytes_stream()
            .map(|result| result.map_err(|e| anyhow!("response stream error: {e}")));
        let boxed: Pin<Box<dyn futures::Stream<Item = Result<Bytes>> + Send>> = Box::pin(stream);

        Ok(TransportResponse {
            status_code,
            status_text,
            headers: resp_headers,
            media_type,
            body_stream: boxed,
        })
    }
}

// ---------------------------------------------------------------------------
// RequestExecutionService
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct RequestExecutionService {
    transport: Arc<dyn HttpTransport>,
    #[allow(dead_code)]
    history_repo: HistoryRepoRef,
    blob_store: Arc<BlobStore>,
    secret_store: SecretStoreRef,
}

impl RequestExecutionService {
    pub fn new(
        transport: Arc<dyn HttpTransport>,
        history_repo: HistoryRepoRef,
        blob_store: Arc<BlobStore>,
        secret_store: SecretStoreRef,
    ) -> Self {
        Self {
            transport,
            history_repo,
            blob_store,
            secret_store,
        }
    }

    /// Execute a request: resolve auth, build HTTP request, send, stream response.
    pub async fn execute(
        &self,
        request: &RequestItem,
        _workspace_id: WorkspaceId,
        cancel: CancellationToken,
    ) -> Result<ExecOutcome> {
        tracing::info!(
            method = %request.method,
            url = %request.url,
            "request execution starting"
        );

        // --- Preflight: parse URL ---
        let parsed_url = match url::Url::parse(&request.url) {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!(error = %e, "preflight rejected: invalid URL");
                return Ok(ExecOutcome::PreflightFailed(format!("invalid URL: {e}")));
            }
        };

        // --- Preflight: check for unresolved {{}} placeholders ---
        check_unresolved_placeholders(&request.url, "URL");
        for kv in &request.headers {
            if kv.enabled {
                check_unresolved_placeholders(&kv.value, &format!("header '{}'", kv.key));
            }
        }

        // --- Preflight: resolve auth secrets ---
        let auth_headers = match resolve_auth_headers(&request.auth, &self.secret_store) {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!(error = %e, "preflight rejected: auth resolution failed");
                return Ok(ExecOutcome::PreflightFailed(format!(
                    "auth resolution failed: {e}"
                )));
            }
        };

        // --- Build HTTP request ---
        let method = match http::Method::from_bytes(request.method.as_bytes()) {
            Ok(m) => m,
            Err(_) => {
                return Ok(ExecOutcome::PreflightFailed(format!(
                    "invalid HTTP method: '{}'",
                    request.method
                )));
            }
        };

        let mut header_map = http::HeaderMap::new();
        for kv in &request.headers {
            if kv.enabled && !kv.key.is_empty() {
                if let Ok(name) = http::header::HeaderName::from_bytes(kv.key.as_bytes()) {
                    if let Ok(value) = http::HeaderValue::from_str(&kv.value) {
                        header_map.append(name, value);
                    }
                }
            }
        }
        for (name, value) in auth_headers {
            header_map.append(name, value);
        }

        // --- Build body ---
        let body = build_request_body(request)?;

        // --- Send ---
        let dispatched_at = Instant::now();
        tracing::info!(method = %request.method, "request dispatched");
        let transport_response = match self
            .transport
            .send(method, parsed_url, header_map, body, cancel.clone())
            .await
        {
            Ok(r) => r,
            Err(e) => {
                if cancel.is_cancelled() {
                    tracing::info!("request cancelled before response");
                    return Ok(ExecOutcome::Cancelled { partial_size: None });
                }
                tracing::warn!(error = %e, "request send failed");
                return Ok(ExecOutcome::Failed(e.to_string()));
            }
        };

        let first_byte_elapsed = dispatched_at.elapsed();

        let status_code = transport_response.status_code;
        let status_text = transport_response.status_text;
        let media_type = transport_response.media_type;
        let resp_headers = transport_response.headers;

        let headers_json = serialize_headers(&resp_headers);

        // --- Stream response body into blob + preview ---
        let body_ref = self
            .stream_response_body(transport_response.body_stream, &media_type, cancel)
            .await?;

        let total_ms = Some(dispatched_at.elapsed().as_millis() as u64);
        let ttfb_ms = Some(first_byte_elapsed.as_millis() as u64);

        Ok(ExecOutcome::Completed(ResponseSummary {
            status_code,
            status_text,
            headers_json,
            media_type,
            body_ref,
            total_ms,
            ttfb_ms,
        }))
    }

    async fn stream_response_body(
        &self,
        mut stream: Pin<Box<dyn futures::Stream<Item = Result<Bytes>> + Send>>,
        media_type: &Option<String>,
        cancel: CancellationToken,
    ) -> Result<BodyRef> {
        tracing::info!("response body streaming started");
        let mut preview_buf = Vec::new();
        let mut total_written: u64 = 0;
        let mut exceeded_preview_cap = false;
        let cap = ResponseBudgets::PREVIEW_CAP_BYTES;

        let temp_path = self
            .blob_store
            .temp_dir()
            .join(format!("response-stream-{}", uuid::Uuid::now_v7()));
        let mut temp_file =
            std::fs::File::create(&temp_path).context("failed to create temp response file")?;
        use std::io::Write;

        while let Some(chunk_result) = stream.next().await {
            if cancel.is_cancelled() {
                tracing::info!(bytes_written = total_written, "response stream cancelled");
                drop(temp_file);
                match std::fs::remove_file(&temp_path) {
                    Ok(()) => tracing::debug!("cleaned up partial response blob"),
                    Err(e) => tracing::warn!(error = %e, "failed to clean up partial response blob"),
                }
                return Ok(BodyRef::Empty);
            }

            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    drop(temp_file);
                    let _ = std::fs::remove_file(&temp_path);
                    return Err(anyhow!("response stream error: {e}"));
                }
            };

            if let Err(e) = temp_file.write_all(&chunk) {
                drop(temp_file);
                let _ = std::fs::remove_file(&temp_path);
                return Err(anyhow!("response write error: {e}"));
            }

            total_written += chunk.len() as u64;

            if !exceeded_preview_cap {
                if preview_buf.len() + chunk.len() <= cap {
                    preview_buf.extend_from_slice(&chunk);
                } else {
                    let remaining = cap - preview_buf.len();
                    preview_buf.extend_from_slice(&chunk.slice(..remaining));
                    exceeded_preview_cap = true;
                }
            }
        }

        drop(temp_file);

        let file = std::fs::File::open(&temp_path).context("failed to reopen temp response")?;
        let blob_meta = self
            .blob_store
            .write_from_reader(file, media_type.as_deref())
            .context("failed to write response blob")?;

        let _ = std::fs::remove_file(&temp_path);

        if exceeded_preview_cap {
            Ok(BodyRef::DiskBlob {
                blob_id: blob_meta.hash,
                preview: Some(Bytes::from(preview_buf)),
                size_bytes: total_written,
            })
        } else {
            Ok(BodyRef::InMemoryPreview {
                bytes: Bytes::from(preview_buf),
                truncated: false,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Outcome enum
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum ExecOutcome {
    Completed(ResponseSummary),
    Failed(String),
    Cancelled { partial_size: Option<u64> },
    PreflightFailed(String),
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn check_unresolved_placeholders(value: &str, context: &str) {
    if value.contains("{{") && value.contains("}}") {
        warn!(
            context,
            value, "unresolved {{}} placeholder detected; will be sent literally"
        );
    }
}

fn resolve_auth_headers(
    auth: &AuthType,
    secret_store: &SecretStoreRef,
) -> Result<Vec<(http::header::HeaderName, http::HeaderValue)>> {
    let mut headers = Vec::new();
    match auth {
        AuthType::None => {}
        AuthType::Basic {
            username,
            password_secret_ref,
        } => {
            let password = match password_secret_ref {
                Some(key) => secret_store
                    .get_secret(key)?
                    .ok_or_else(|| anyhow!("basic auth password not found in secret store"))?,
                None => String::new(),
            };
            let encoded =
                base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
            headers.push((
                http::header::AUTHORIZATION,
                http::HeaderValue::from_str(&format!("Basic {encoded}"))?,
            ));
        }
        AuthType::Bearer { token_secret_ref } => {
            let token = match token_secret_ref {
                Some(key) => secret_store
                    .get_secret(key)?
                    .ok_or_else(|| anyhow!("bearer token not found in secret store"))?,
                None => String::new(),
            };
            headers.push((
                http::header::AUTHORIZATION,
                http::HeaderValue::from_str(&format!("Bearer {token}"))?,
            ));
        }
        AuthType::ApiKey {
            key_name,
            value_secret_ref,
            location,
        } => {
            let value = match value_secret_ref {
                Some(key) => secret_store
                    .get_secret(key)?
                    .ok_or_else(|| anyhow!("API key value not found in secret store"))?,
                None => String::new(),
            };
            match location {
                crate::domain::request::ApiKeyLocation::Header => {
                    let name = http::header::HeaderName::from_bytes(key_name.as_bytes())?;
                    headers.push((name, http::HeaderValue::from_str(&value)?));
                }
                crate::domain::request::ApiKeyLocation::Query => {
                    warn!("API key in query location not yet fully supported in execution");
                }
            }
        }
    }
    Ok(headers)
}

fn build_request_body(request: &RequestItem) -> Result<Option<Bytes>> {
    use crate::domain::request::BodyType;
    match &request.body {
        BodyType::None => Ok(None),
        BodyType::RawText { content } => Ok(Some(Bytes::from(content.clone()))),
        BodyType::RawJson { content } => Ok(Some(Bytes::from(content.clone()))),
        BodyType::UrlEncoded { entries } => {
            let pairs: Vec<(String, String)> = entries
                .iter()
                .filter(|e| e.enabled)
                .map(|e| (e.key.clone(), e.value.clone()))
                .collect();
            let encoded =
                serde_urlencoded::to_string(&pairs).context("failed to encode url-form body")?;
            Ok(Some(Bytes::from(encoded)))
        }
        BodyType::FormData { .. } => {
            warn!("multipart form-data body type not yet fully supported in execution");
            Ok(None)
        }
        BodyType::BinaryFile { .. } => {
            warn!("binary file body type not yet fully supported in execution");
            Ok(None)
        }
    }
}

fn serialize_headers(headers: &http::HeaderMap) -> Option<String> {
    if headers.is_empty() {
        return None;
    }
    let mut map = serde_json::Map::new();
    for (name, value) in headers {
        let key = name.as_str().to_string();
        let val = value.to_str().unwrap_or("<non-ascii>").to_string();
        map.entry(key)
            .and_modify(|existing| {
                if let serde_json::Value::String(s) = existing {
                    *s = format!("{s}, {val}");
                }
            })
            .or_insert(serde_json::Value::String(val));
    }
    Some(serde_json::Value::Object(map).to_string())
}
