use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// Bounded response body reference — keeps only a preview in hot state.
#[derive(Debug, Clone)]
pub enum BodyRef {
    Empty,
    InMemoryPreview {
        bytes: Bytes,
        truncated: bool,
    },
    DiskBlob {
        blob_id: String,
        preview: Option<Bytes>,
        size_bytes: u64,
    },
}

impl BodyRef {
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub fn size_bytes(&self) -> u64 {
        match self {
            Self::Empty => 0,
            Self::InMemoryPreview { bytes, .. } => bytes.len() as u64,
            Self::DiskBlob { size_bytes, .. } => *size_bytes,
        }
    }
}

/// Summary of a response attached to the editor state.
#[derive(Debug, Clone)]
pub struct ResponseSummary {
    pub status_code: u16,
    pub status_text: String,
    pub headers_json: Option<String>,
    pub media_type: Option<String>,
    pub body_ref: BodyRef,
    pub total_ms: Option<u64>,
    pub ttfb_ms: Option<u64>,
    pub dispatched_at_unix_ms: Option<i64>,
    pub first_byte_at_unix_ms: Option<i64>,
    pub completed_at_unix_ms: Option<i64>,
    pub http_version: Option<String>,
    pub local_addr: Option<String>,
    pub remote_addr: Option<String>,
    pub tls: Option<TlsSummary>,
    pub size: ResponseSizeBreakdown,
    pub request_size: RequestSizeBreakdown,
    pub phase_timings: PhaseTimings,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TlsSummary {
    pub protocol: Option<String>,
    pub cipher: Option<String>,
    pub certificate_cn: Option<String>,
    pub issuer_cn: Option<String>,
    pub valid_until: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseSizeBreakdown {
    pub headers_bytes: Option<u64>,
    pub body_wire_bytes: Option<u64>,
    pub body_decoded_bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestSizeBreakdown {
    pub headers_bytes: Option<u64>,
    pub body_bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhaseTimings {
    pub prepare_ms: Option<u64>,
    pub dns_ms: Option<u64>,
    pub connect_ms: Option<u64>,
    pub tcp_ms: Option<u64>,
    pub tls_ms: Option<u64>,
    pub ttfb_ms: Option<u64>,
    pub download_ms: Option<u64>,
    pub process_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseMetaV2 {
    pub http_version: Option<String>,
    pub local_addr: Option<String>,
    pub remote_addr: Option<String>,
    pub tls: Option<TlsSummary>,
    pub size: ResponseSizeBreakdown,
    pub request_size: RequestSizeBreakdown,
    pub phase_timings: PhaseTimings,
}

impl ResponseSummary {
    pub fn meta_v2(&self) -> ResponseMetaV2 {
        ResponseMetaV2 {
            http_version: self.http_version.clone(),
            local_addr: self.local_addr.clone(),
            remote_addr: self.remote_addr.clone(),
            tls: self.tls.clone(),
            size: self.size.clone(),
            request_size: self.request_size.clone(),
            phase_timings: self.phase_timings.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseHeaderRow {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderJsonFormat {
    LosslessRows,
    LegacyObjectMap,
}

pub fn serialize_response_header_rows(rows: &[ResponseHeaderRow]) -> Option<String> {
    if rows.is_empty() {
        return None;
    }
    serde_json::to_string(rows).ok()
}

pub fn parse_response_header_rows(
    raw: Option<&str>,
) -> (Vec<ResponseHeaderRow>, Option<HeaderJsonFormat>) {
    let Some(raw) = raw else {
        return (Vec::new(), None);
    };

    if raw.trim().is_empty() {
        return (Vec::new(), None);
    }

    if let Ok(rows) = serde_json::from_str::<Vec<ResponseHeaderRow>>(raw) {
        return (rows, Some(HeaderJsonFormat::LosslessRows));
    }

    if let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(raw) {
        let mut rows = Vec::new();
        for (name, value) in map {
            match value {
                serde_json::Value::String(v) => rows.push(ResponseHeaderRow { name, value: v }),
                other => rows.push(ResponseHeaderRow {
                    name,
                    value: other.to_string(),
                }),
            }
        }
        return (rows, Some(HeaderJsonFormat::LegacyObjectMap));
    }

    (Vec::new(), None)
}

pub fn normalize_unix_ms(value: i64) -> i64 {
    if value.abs() < 1_000_000_000_000 {
        value.saturating_mul(1000)
    } else {
        value
    }
}

/// Response budget constants (Phase 3).
pub struct ResponseBudgets;

impl ResponseBudgets {
    /// Per-response in-memory preview cap: 2 MiB.
    pub const PREVIEW_CAP_BYTES: usize = 2 * 1024 * 1024;
    /// Per-tab total volatile response footprint cap: 32 MiB.
    pub const PER_TAB_CAP_BYTES: usize = 32 * 1024 * 1024;
}

#[cfg(test)]
mod tests {
    use super::{
        HeaderJsonFormat, PhaseTimings, RequestSizeBreakdown, ResponseHeaderRow, ResponseMetaV2,
        ResponseSizeBreakdown, TlsSummary, normalize_unix_ms, parse_response_header_rows,
        serialize_response_header_rows,
    };

    #[test]
    fn header_rows_roundtrip_lossless() {
        let rows = vec![
            ResponseHeaderRow {
                name: "set-cookie".to_string(),
                value: "a=1; Path=/".to_string(),
            },
            ResponseHeaderRow {
                name: "set-cookie".to_string(),
                value: "b=2; Path=/".to_string(),
            },
        ];
        let encoded = serialize_response_header_rows(&rows).expect("must encode");
        let (decoded, format) = parse_response_header_rows(Some(&encoded));
        assert_eq!(format, Some(HeaderJsonFormat::LosslessRows));
        assert_eq!(decoded, rows);
    }

    #[test]
    fn legacy_header_map_is_supported() {
        let raw = r#"{"content-type":"application/json","set-cookie":"a=1, b=2"}"#;
        let (rows, format) = parse_response_header_rows(Some(raw));
        assert_eq!(format, Some(HeaderJsonFormat::LegacyObjectMap));
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn normalize_unix_ms_converts_seconds() {
        assert_eq!(normalize_unix_ms(1_700_000_000), 1_700_000_000_000);
        assert_eq!(normalize_unix_ms(1_700_000_000_000), 1_700_000_000_000);
    }

    #[test]
    fn response_meta_v2_roundtrip() {
        let meta = ResponseMetaV2 {
            http_version: Some("HTTP/2".to_string()),
            local_addr: None,
            remote_addr: Some("1.2.3.4:443".to_string()),
            tls: Some(TlsSummary {
                protocol: None,
                cipher: None,
                certificate_cn: Some("example.com".to_string()),
                issuer_cn: Some("Example CA".to_string()),
                valid_until: Some(1_800_000_000_000),
            }),
            size: ResponseSizeBreakdown {
                headers_bytes: Some(320),
                body_wire_bytes: Some(1200),
                body_decoded_bytes: 2400,
            },
            request_size: RequestSizeBreakdown {
                headers_bytes: Some(180),
                body_bytes: 512,
            },
            phase_timings: PhaseTimings {
                prepare_ms: Some(1),
                dns_ms: Some(2),
                connect_ms: Some(3),
                tcp_ms: None,
                tls_ms: None,
                ttfb_ms: Some(4),
                download_ms: Some(5),
                process_ms: Some(6),
            },
        };

        let raw = serde_json::to_string(&meta).expect("serialize");
        let decoded: ResponseMetaV2 = serde_json::from_str(&raw).expect("deserialize");
        assert_eq!(decoded, meta);
    }
}
