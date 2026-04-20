use std::{
    pin::Pin,
    sync::{Arc, Mutex},
    time::Instant,
};

use anyhow::{Context as _, Result, anyhow};
use base64::Engine as _;
use bytes::Bytes;
use futures::StreamExt as _;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tower_layer::Layer;
use tower_service::Service;
use tracing::{info_span, warn};
use x509_parser::prelude::parse_x509_certificate;

use crate::{
    domain::{
        ids::{HistoryEntryId, RequestId, WorkspaceId},
        request::{ApiKeyLocation, AuthType, BodyType, RequestItem},
        response::{
            BodyRef, PhaseTimings, RequestSizeBreakdown, ResponseBudgets, ResponseHeaderRow,
            ResponseSizeBreakdown, ResponseSummary, TlsSummary, serialize_response_header_rows,
        },
    },
    infra::{blobs::BlobStore, secrets::SecretStoreRef},
    repos::history_repo::{HistoryRepoRef, build_request_snapshot},
    services::error_classifier::{ClassifiedError, classify_transport_error},
    services::request_body_payload::{RequestBodyPayload, build_request_body_payload},
    services::telemetry,
};

tokio::task_local! {
    static ACTIVE_PHASE_COLLECTOR: Arc<PhaseTimingCollector>;
}

#[async_trait::async_trait]
pub trait HttpTransport: Send + Sync {
    async fn send(
        &self,
        method: http::Method,
        url: url::Url,
        headers: http::HeaderMap,
        body: RequestBodyPayload,
        cancel: CancellationToken,
        phase_collector: Arc<PhaseTimingCollector>,
    ) -> Result<TransportResponse>;
}

pub struct TransportResponse {
    pub status_code: u16,
    pub status_text: String,
    pub headers: http::HeaderMap,
    pub media_type: Option<String>,
    pub http_version: Option<http::Version>,
    pub remote_addr: Option<std::net::SocketAddr>,
    pub peer_cert_der: Option<Vec<u8>>,
    pub response_headers_size: Option<u64>,
    pub content_length: Option<u64>,
    pub body_stream: Pin<Box<dyn futures::Stream<Item = Result<Bytes>> + Send>>,
}

#[derive(Default)]
pub struct PhaseTimingCollector {
    timings: Mutex<PhaseTimings>,
}

impl PhaseTimingCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> PhaseTimings {
        self.timings
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    fn record_dns(&self, ms: u64) {
        self.timings
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .dns_ms = Some(ms);
    }

    fn record_connect(&self, ms: u64) {
        self.timings
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .connect_ms = Some(ms);
    }
}

#[derive(Clone, Default)]
struct TimedResolver;

impl reqwest::dns::Resolve for TimedResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let start = Instant::now();
            let addrs = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
            let addrs: Vec<std::net::SocketAddr> = addrs.collect();
            let _ = ACTIVE_PHASE_COLLECTOR.try_with(|collector| {
                collector.record_dns(start.elapsed().as_millis() as u64);
            });
            Ok::<reqwest::dns::Addrs, Box<dyn std::error::Error + Send + Sync>>(Box::new(
                addrs.into_iter(),
            ))
        })
    }
}

#[derive(Clone, Default)]
struct TimedConnectLayer;

#[derive(Clone)]
struct TimedConnectService<S> {
    inner: S,
}

impl<S> Layer<S> for TimedConnectLayer {
    type Service = TimedConnectService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TimedConnectService { inner }
    }
}

impl<S, Req> Service<Req> for TimedConnectService<S>
where
    S: Service<Req> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future =
        Pin<Box<dyn futures::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let started = Instant::now();
        let fut = self.inner.call(req);
        Box::pin(async move {
            let result = fut.await;
            if result.is_ok() {
                let _ = ACTIVE_PHASE_COLLECTOR.try_with(|collector| {
                    collector.record_connect(started.elapsed().as_millis() as u64);
                });
            }
            result
        })
    }
}

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
            .tls_info(true)
            .dns_resolver2(TimedResolver)
            .connector_layer(TimedConnectLayer)
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
        body: RequestBodyPayload,
        cancel: CancellationToken,
        phase_collector: Arc<PhaseTimingCollector>,
    ) -> Result<TransportResponse> {
        let mut builder = self.client.request(method, url.as_str());
        for (name, value) in &headers {
            builder = builder.header(name, value);
        }
        match body {
            RequestBodyPayload::None => {}
            RequestBodyPayload::Bytes(body) => {
                builder = builder.body(reqwest::Body::from(body));
            }
            RequestBodyPayload::Stream(stream) => {
                builder = builder.body(reqwest::Body::wrap_stream(stream));
            }
        }

        let response = ACTIVE_PHASE_COLLECTOR
            .scope(phase_collector, async {
                let send_fut = builder.send();
                tokio::pin!(send_fut);
                tokio::select! {
                    _ = cancel.cancelled() => {
                        Err(anyhow!("request send cancelled"))
                    }
                    result = &mut send_fut => {
                        result.context("request send failed")
                    }
                }
            })
            .await?;

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
        let http_version = Some(response.version());
        let remote_addr = response.remote_addr();
        let response_headers_size = Some(header_map_size_bytes(&resp_headers));
        let content_length = parse_content_length(&resp_headers);
        let peer_cert_der = response
            .extensions()
            .get::<reqwest::tls::TlsInfo>()
            .and_then(|info| info.peer_certificate().map(|der| der.to_vec()));

        let stream = response
            .bytes_stream()
            .map(|result| result.map_err(|e| anyhow!("response stream error: {e}")));
        let boxed: Pin<Box<dyn futures::Stream<Item = Result<Bytes>> + Send>> = Box::pin(stream);

        Ok(TransportResponse {
            status_code,
            status_text,
            headers: resp_headers,
            media_type,
            http_version,
            remote_addr,
            peer_cert_der,
            response_headers_size,
            content_length,
            body_stream: boxed,
        })
    }
}

#[derive(Clone)]
pub struct RequestExecutionService {
    transport: Arc<dyn HttpTransport>,
    history_repo: HistoryRepoRef,
    blob_store: Arc<BlobStore>,
    secret_store: SecretStoreRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecProgressEvent {
    ResponseStreamingStarted,
}

#[derive(Debug)]
pub enum ExecOutcome {
    Completed(ResponseSummary),
    Failed {
        summary: String,
        classified: Option<ClassifiedError>,
    },
    Cancelled {
        partial_size: Option<u64>,
    },
    PreflightFailed(String),
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

    pub fn create_pending_history(
        &self,
        workspace_id: WorkspaceId,
        request_id: Option<RequestId>,
        request: &RequestItem,
    ) -> Result<crate::domain::history::HistoryEntry> {
        self.history_repo
            .create_pending(
                workspace_id,
                request_id,
                &request.method,
                &request.url,
                Some(build_request_snapshot(request)),
            )
            .map_err(|e| anyhow!("failed to create pending history row: {e}"))
    }

    pub fn finalize_history(&self, operation_id: HistoryEntryId, result: &Result<ExecOutcome>) {
        let _span = info_span!("response.persist", op_id = %operation_id).entered();
        match result {
            Ok(ExecOutcome::Completed(summary)) => {
                telemetry::inc_requests_completed();
                let headers_json = summary.headers_json.as_deref();
                let (blob_hash_owned, blob_size) = match &summary.body_ref {
                    BodyRef::DiskBlob {
                        blob_id,
                        size_bytes,
                        ..
                    } => (Some(blob_id.clone()), Some(*size_bytes as i64)),
                    BodyRef::InMemoryPreview { bytes, .. } => {
                        match self
                            .blob_store
                            .write_bytes(bytes, summary.media_type.as_deref())
                        {
                            Ok(meta) => (Some(meta.hash), Some(meta.size_bytes as i64)),
                            Err(_) => (None, None),
                        }
                    }
                    BodyRef::Empty => (None, None),
                };
                let meta_v2_json = serde_json::to_string(&summary.meta_v2()).ok();
                let _ = self.history_repo.finalize_completed(
                    operation_id,
                    summary.status_code as i64,
                    blob_hash_owned.as_deref(),
                    blob_size,
                    headers_json,
                    summary.media_type.as_deref(),
                    meta_v2_json.as_deref(),
                    summary.dispatched_at_unix_ms,
                    summary.first_byte_at_unix_ms,
                );
            }
            Ok(ExecOutcome::Failed { summary, .. }) => {
                telemetry::inc_requests_failed();
                let _ = self.history_repo.mark_failed(operation_id, summary);
            }
            Ok(ExecOutcome::Cancelled { partial_size }) => {
                telemetry::inc_requests_cancelled();
                let _ = self
                    .history_repo
                    .finalize_cancelled(operation_id, partial_size.map(|s| s as i64));
            }
            Ok(ExecOutcome::PreflightFailed(msg)) => {
                telemetry::inc_requests_failed();
                let _ = self.history_repo.mark_failed(operation_id, msg);
            }
            Err(e) => {
                telemetry::inc_requests_failed();
                let _ = self.history_repo.mark_failed(operation_id, &e.to_string());
            }
        }
    }

    pub async fn execute(
        &self,
        request: &RequestItem,
        workspace_id: WorkspaceId,
        cancel: CancellationToken,
    ) -> Result<ExecOutcome> {
        self.execute_with_progress(request, workspace_id, cancel, None)
            .await
    }

    pub async fn execute_with_progress(
        &self,
        request: &RequestItem,
        _workspace_id: WorkspaceId,
        cancel: CancellationToken,
        progress_tx: Option<mpsc::Sender<ExecProgressEvent>>,
    ) -> Result<ExecOutcome> {
        info_span!("request.send", method = %request.method, url = %request.url)
            .in_scope(|| tracing::info!("request lifecycle started"));
        let lifecycle_started = Instant::now();

        let parsed_url = match url::Url::parse(&request.url) {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!(error = %e, "preflight rejected: invalid URL");
                return Ok(ExecOutcome::PreflightFailed(format!("invalid URL: {e}")));
            }
        };

        check_unresolved_placeholders(&request.url, "URL");
        for kv in &request.params {
            if kv.enabled {
                check_unresolved_placeholders(&kv.value, &format!("query param '{}'", kv.key));
            }
        }
        for kv in &request.headers {
            if kv.enabled {
                check_unresolved_placeholders(&kv.value, &format!("header '{}'", kv.key));
            }
        }
        check_auth_placeholders(&request.auth);
        check_body_placeholders(&request.body);

        let resolved_auth = match resolve_auth(&request.auth, &self.secret_store) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "preflight rejected: auth resolution failed");
                return Ok(ExecOutcome::PreflightFailed(format!(
                    "auth resolution failed: {e}"
                )));
            }
        };

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
        for (name, value) in resolved_auth.headers {
            header_map.append(name, value);
        }

        let mut parsed_url = parsed_url;
        let enabled_params: Vec<(String, String)> = request
            .params
            .iter()
            .filter(|p| p.enabled && !p.key.trim().is_empty())
            .map(|p| (p.key.clone(), p.value.clone()))
            .collect();
        if !enabled_params.is_empty() {
            parsed_url
                .query_pairs_mut()
                .clear()
                .extend_pairs(enabled_params.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        }
        if !resolved_auth.query_pairs.is_empty() {
            parsed_url.query_pairs_mut().extend_pairs(
                resolved_auth
                    .query_pairs
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str())),
            );
        }

        let built_body = match build_request_body_payload(&request.body, &self.blob_store) {
            Ok(payload) => payload,
            Err(e) => {
                tracing::warn!(error = %e, "preflight rejected: request body invalid");
                return Ok(ExecOutcome::PreflightFailed(format!(
                    "request body invalid: {e}"
                )));
            }
        };
        if let Some(content_type) = built_body.content_type.as_deref() {
            if let Err(e) = ensure_content_type(&mut header_map, content_type) {
                tracing::warn!(error = %e, "preflight rejected: invalid content-type");
                return Ok(ExecOutcome::PreflightFailed(format!(
                    "invalid content-type: {e}"
                )));
            }
        }
        let request_headers_size = Some(header_map_size_bytes(&header_map));
        let request_body_size = built_body.body_bytes;

        let dispatched_at = Instant::now();
        let dispatched_at_unix_ms = now_unix_ms();
        let prepare_ms = Some(lifecycle_started.elapsed().as_millis() as u64);
        let phase_collector = Arc::new(PhaseTimingCollector::new());
        tracing::info!(method = %request.method, "request dispatched");
        let transport_response = match self
            .transport
            .send(
                method,
                parsed_url,
                header_map,
                built_body.payload,
                cancel.clone(),
                phase_collector.clone(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                if cancel.is_cancelled() {
                    tracing::info!("request cancelled before response");
                    return Ok(ExecOutcome::Cancelled { partial_size: None });
                }
                tracing::warn!(error = %e, "request send failed");
                return Ok(ExecOutcome::Failed {
                    summary: e.to_string(),
                    classified: Some(classify_transport_error(&e)),
                });
            }
        };

        let first_byte_elapsed = dispatched_at.elapsed();
        let first_byte_at_unix_ms = now_unix_ms();

        let status_code = transport_response.status_code;
        let status_text = transport_response.status_text;
        let media_type = transport_response.media_type;
        let resp_headers = transport_response.headers;
        let http_version = transport_response.http_version.map(http_version_to_string);
        let remote_addr = transport_response.remote_addr.map(|addr| addr.to_string());
        let response_headers_size = transport_response.response_headers_size;
        let content_length = transport_response.content_length;
        let tls = parse_tls_summary(transport_response.peer_cert_der.as_deref());
        let headers_json = serialize_headers(&resp_headers);

        if let Some(tx) = progress_tx.as_ref() {
            let _ = tx.try_send(ExecProgressEvent::ResponseStreamingStarted);
        }

        let download_started = Instant::now();
        let body_ref = self
            .stream_response_body(transport_response.body_stream, &media_type, cancel)
            .await?;
        let download_ms = Some(download_started.elapsed().as_millis() as u64);

        let process_started = Instant::now();
        let completed_at_unix_ms = now_unix_ms();
        let total_ms = Some(dispatched_at.elapsed().as_millis() as u64);
        let ttfb_ms = Some(first_byte_elapsed.as_millis() as u64);
        let decoded_body_size = body_ref.size_bytes();
        let process_ms = Some(process_started.elapsed().as_millis() as u64);
        let mut phase_timings = phase_collector.snapshot();
        phase_timings.prepare_ms = prepare_ms;
        phase_timings.ttfb_ms = ttfb_ms;
        phase_timings.download_ms = download_ms;
        phase_timings.process_ms = process_ms;

        Ok(ExecOutcome::Completed(ResponseSummary {
            status_code,
            status_text,
            headers_json,
            media_type,
            body_ref,
            total_ms,
            ttfb_ms,
            dispatched_at_unix_ms: Some(dispatched_at_unix_ms),
            first_byte_at_unix_ms: Some(first_byte_at_unix_ms),
            completed_at_unix_ms: Some(completed_at_unix_ms),
            http_version,
            local_addr: None,
            remote_addr,
            tls,
            size: ResponseSizeBreakdown {
                headers_bytes: response_headers_size,
                body_wire_bytes: content_length,
                body_decoded_bytes: decoded_body_size,
            },
            request_size: RequestSizeBreakdown {
                headers_bytes: request_headers_size,
                body_bytes: request_body_size,
            },
            phase_timings,
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
        let mut temp_file = tokio::fs::File::create(&temp_path)
            .await
            .context("failed to create temp response file")?;

        while let Some(chunk_result) = stream.next().await {
            if cancel.is_cancelled() {
                tracing::info!(bytes_written = total_written, "response stream cancelled");
                drop(temp_file);
                match tokio::fs::remove_file(&temp_path).await {
                    Ok(()) => tracing::debug!("cleaned up partial response blob"),
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to clean up partial response blob")
                    }
                }
                return Ok(BodyRef::Empty);
            }

            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    drop(temp_file);
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    return Err(anyhow!("response stream error: {e}"));
                }
            };

            if let Err(e) = temp_file.write_all(&chunk).await {
                drop(temp_file);
                let _ = tokio::fs::remove_file(&temp_path).await;
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

        let blob_store = self.blob_store.clone();
        let temp_path_for_blocking = temp_path.clone();
        let media_owned = media_type.clone();
        let blob_meta = tokio::task::spawn_blocking(move || -> Result<_> {
            let file = std::fs::File::open(&temp_path_for_blocking)
                .context("failed to reopen temp response")?;
            blob_store
                .write_from_reader(file, media_owned.as_deref())
                .context("failed to write response blob")
        })
        .await
        .map_err(|e| anyhow!("response persist task join error: {e}"))??;

        let _ = tokio::fs::remove_file(&temp_path).await;

        if exceeded_preview_cap {
            telemetry::inc_responses_truncated();
            telemetry::observe_preview_bytes(preview_buf.len());
            Ok(BodyRef::DiskBlob {
                blob_id: blob_meta.hash,
                preview: Some(Bytes::from(preview_buf)),
                size_bytes: total_written,
            })
        } else {
            telemetry::observe_preview_bytes(preview_buf.len());
            Ok(BodyRef::InMemoryPreview {
                bytes: Bytes::from(preview_buf),
                truncated: false,
            })
        }
    }
}

struct ResolvedAuth {
    headers: Vec<(http::header::HeaderName, http::HeaderValue)>,
    query_pairs: Vec<(String, String)>,
}

fn resolve_auth(auth: &AuthType, secret_store: &SecretStoreRef) -> Result<ResolvedAuth> {
    let mut headers = Vec::new();
    let mut query_pairs = Vec::new();
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
                ApiKeyLocation::Header => {
                    let name = http::header::HeaderName::from_bytes(key_name.as_bytes())?;
                    headers.push((name, http::HeaderValue::from_str(&value)?));
                }
                ApiKeyLocation::Query => {
                    query_pairs.push((key_name.clone(), value));
                }
            }
        }
    }

    Ok(ResolvedAuth {
        headers,
        query_pairs,
    })
}

fn ensure_content_type(headers: &mut http::HeaderMap, value: &str) -> Result<()> {
    if !headers.contains_key(http::header::CONTENT_TYPE) {
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_str(value)?,
        );
    }
    Ok(())
}

fn check_unresolved_placeholders(value: &str, context: &str) {
    if value.contains("{{") && value.contains("}}") {
        warn!(
            context,
            value, "unresolved {{}} placeholder detected; will be sent literally"
        );
    }
}

fn check_auth_placeholders(auth: &AuthType) {
    match auth {
        AuthType::None => {}
        AuthType::Basic { username, .. } => {
            check_unresolved_placeholders(username, "auth.basic.username");
        }
        AuthType::Bearer { .. } => {}
        AuthType::ApiKey { key_name, .. } => {
            check_unresolved_placeholders(key_name, "auth.api_key.name");
        }
    }
}

fn check_body_placeholders(body: &BodyType) {
    match body {
        BodyType::None => {}
        BodyType::RawText { content } | BodyType::RawJson { content } => {
            check_unresolved_placeholders(content, "body.raw");
        }
        BodyType::UrlEncoded { entries } => {
            for entry in entries.iter().filter(|e| e.enabled) {
                check_unresolved_placeholders(
                    &entry.value,
                    &format!("body.urlencoded '{}'", entry.key),
                );
            }
        }
        BodyType::FormData {
            text_fields,
            file_fields: _,
        } => {
            for field in text_fields.iter().filter(|f| f.enabled) {
                check_unresolved_placeholders(
                    &field.value,
                    &format!("body.form_data '{}'", field.key),
                );
            }
        }
        BodyType::BinaryFile { .. } => {}
    }
}

fn serialize_headers(headers: &http::HeaderMap) -> Option<String> {
    let rows = headers
        .iter()
        .map(|(name, value)| ResponseHeaderRow {
            name: name.as_str().to_string(),
            value: value.to_str().unwrap_or("<non-ascii>").to_string(),
        })
        .collect::<Vec<_>>();
    serialize_response_header_rows(&rows)
}

fn header_map_size_bytes(headers: &http::HeaderMap) -> u64 {
    headers
        .iter()
        .map(|(name, value)| {
            // "Name: Value\r\n"
            name.as_str().len() as u64 + 2 + value.as_bytes().len() as u64 + 2
        })
        .sum()
}

fn parse_content_length(headers: &http::HeaderMap) -> Option<u64> {
    let is_chunked = headers
        .get(http::header::TRANSFER_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(|value| {
            value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("chunked"))
        })
        .unwrap_or(false);
    if is_chunked {
        return None;
    }

    headers
        .get(http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
}

fn http_version_to_string(version: http::Version) -> String {
    match version {
        http::Version::HTTP_09 => "HTTP/0.9",
        http::Version::HTTP_10 => "HTTP/1.0",
        http::Version::HTTP_11 => "HTTP/1.1",
        http::Version::HTTP_2 => "HTTP/2",
        http::Version::HTTP_3 => "HTTP/3",
        _ => "HTTP/?",
    }
    .to_string()
}

fn parse_tls_summary(peer_cert_der: Option<&[u8]>) -> Option<TlsSummary> {
    let der = peer_cert_der?;
    let (_, cert) = parse_x509_certificate(der).ok()?;

    let certificate_cn = cert
        .subject()
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok())
        .map(|s| s.to_string());
    let issuer_cn = cert
        .issuer()
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok())
        .map(|s| s.to_string());
    let valid_until = cert.validity().not_after.timestamp().checked_mul(1000);

    Some(TlsSummary {
        protocol: None,
        cipher: None,
        certificate_cn,
        issuer_cn,
        valid_until,
    })
}

fn now_unix_ms() -> i64 {
    (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
}

#[cfg(test)]
mod tests {
    use super::{parse_content_length, parse_tls_summary};
    use base64::Engine as _;

    const CERT_WITH_CN_DER_B64: &str = "MIICqDCCAZACCQC+zNamvRmnrDANBgkqhkiG9w0BAQsFADAWMRQwEgYDVQQDDAtleGFtcGxlLmNvbTAeFw0yNjA0MTkyMTA0NTdaFw0zNjA0MTYyMTA0NTdaMBYxFDASBgNVBAMMC2V4YW1wbGUuY29tMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAn0XE3Gmpd3EJlmiza72fClQqEabt0IEW8VSPN4sKvGdHcckLX1W8nqy11M8cCc5PiVW6OgsZ1MYf8T0J7Hhu8AHBBubqRmecOahJI7NO9f/HiYSrF2pqGezwmvJFugNZmcBIty1qPN9HRewIzYVmahcLM+/FiTanD43KVYBt8+ABydlGr+Psp6IzbFcLsGRLnk86Iq3VddypXwL1ty4jjc+lzkPV+P1KsxM7Q8Vdebs0yaeUQFoW7o8GKuRhnYYs18tyJF96sSpEpfZIkffN14nT2ML7xLhl81bp3MXpQKl49u7J/Sg89xa9mM3t5hHtU2VdRv3Jjw8VXLr11KLDWwIDAQABMA0GCSqGSIb3DQEBCwUAA4IBAQAQDPh0tRj7Honl3JfQkDUUswaLzkkc/76zvempdE8Jk0H/iUwtXucoBPoQvFskXTnjIkvVf5dEGl24Y024qOlNkjroeMl4QCM9zD+29D8gx9Iceb+/3XtKis3aLggTd65uqxnrxuGkJxycLrEmMhM0HBGvp5p18JbDXQZKpkLlePwOCPsIqsO7D+3q1/YoLdMEmqj4OOGiMPagn6TEYJvtc9LhNOAt8anpklUwiCLcle59f532M5rbxeUxZo0yTjg0fOyri2tWPT5SBMliidUsMZgwvPhGdfMabVMufBESrCh/GGV1dcyfs/egcH2AeUSMBgvNz1GmxuszH9Ox4tBy";
    const CERT_NO_SUBJECT_CN_DER_B64: &str = "MIICpDCCAYwCCQDKL604yUhsqzANBgkqhkiG9w0BAQsFADAUMRIwEAYDVQQKDAlObyBDTiBPcmcwHhcNMjYwNDE5MjEwNDU4WhcNMzYwNDE2MjEwNDU4WjAUMRIwEAYDVQQKDAlObyBDTiBPcmcwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCg77X51JXFX3Utf8MeyHQk4rxVqz5Nw44JlV6PWLyGj0h3PApp36HYSPNq9Vy1Trape5jUFXu6qlP1GpUiE+NtNm3RXz66/KYXvyDykm9Um5tvYW9xrSc21TKBepnXgkd+gDdSysOYMugi3mYezn0tpZZEM+TXuhRJ+WeJSt02eU8NPHz7613aKiRq522vjeXeyliKD3X+h1CxfLRGKRrYJCSSD3TefwFUOk9d/qgEb2PitLrcwuY/1TAUIZS7+BOM+hV4R8zoo6xJ5Ma1g1iQGp9CmS29Xk4bfgr4uQI8SZuqNlEdjCOgUvwt0L2PxO+rjL7dXFY1Y4c18zdkJaaJAgMBAAEwDQYJKoZIhvcNAQELBQADggEBAJUHFEm87sNgs3QG4Ve+1B5egYr5KLb0musrgE5BymVlcX7Ns3nQ8vO2yOQsSU14Cl8mSjXTEDwYUlha4CICMqIbfsIqLD0rh9dvFXdHsRkIQZ03nGOwXJL9ae8R3ypoj0xGIx+LAIpEkcURmjpBvumeo2psSJJpoLCHhpAtZRUwPJBqH/WzY/8wBRL3OTloxhmQimaXvdmmW8vkFHxAEhEuwfYvKVvg5qwunhAYpPxSN3uOXaS5YjHHUwMjssuFXbFQhnbRM7Jvqxly/bldHYxaYNdc0hfSA0w4BtZdM+toX/JjDKh/7p7EbWTeSCXWIwh4RrbgqPmaL3VbtIr5IG4=";

    #[test]
    fn content_length_parser_handles_chunked_and_fixed_length() {
        let mut fixed = http::HeaderMap::new();
        fixed.insert(
            http::header::CONTENT_LENGTH,
            http::HeaderValue::from_static("512"),
        );
        assert_eq!(parse_content_length(&fixed), Some(512));

        let mut chunked = http::HeaderMap::new();
        chunked.insert(
            http::header::TRANSFER_ENCODING,
            http::HeaderValue::from_static("chunked"),
        );
        chunked.insert(
            http::header::CONTENT_LENGTH,
            http::HeaderValue::from_static("1024"),
        );
        assert_eq!(parse_content_length(&chunked), None);

        let mut invalid = http::HeaderMap::new();
        invalid.insert(
            http::header::CONTENT_LENGTH,
            http::HeaderValue::from_static("not-a-number"),
        );
        assert_eq!(parse_content_length(&invalid), None);
    }

    #[test]
    fn tls_summary_parsing_happy_path_and_future_validity() {
        let der = base64::engine::general_purpose::STANDARD
            .decode(CERT_WITH_CN_DER_B64)
            .expect("decode der");
        let tls = parse_tls_summary(Some(&der)).expect("parse tls summary");

        assert_eq!(tls.certificate_cn.as_deref(), Some("example.com"));
        assert_eq!(tls.issuer_cn.as_deref(), Some("example.com"));
        assert!(tls.valid_until.expect("valid_until") > 0);
        assert!(tls.valid_until.expect("valid_until") > super::now_unix_ms());
    }

    #[test]
    fn tls_summary_parsing_handles_malformed_der_and_missing_cn() {
        assert!(parse_tls_summary(Some(&[0x01, 0x02, 0x03])).is_none());

        let der = base64::engine::general_purpose::STANDARD
            .decode(CERT_NO_SUBJECT_CN_DER_B64)
            .expect("decode der");
        let tls = parse_tls_summary(Some(&der)).expect("parse tls summary");

        assert!(tls.certificate_cn.is_none());
        assert!(tls.valid_until.is_some());
    }
}
