/// Phase 3.5 validation tests for pure helper functions and domain logic.
///
/// Covers: response header round-trip, cookie parsing, error classifier helpers,
/// domain model basics. The view-layer helper functions (params_from_url_query,
/// search_matches, format_bytes, etc.) are tested in the existing unit tests
/// inside `request_tab.rs` to avoid proc-macro expansion limits in the lib test
/// binary.
mod common;

use torii::domain::request::{AuthType, BodyType, KeyValuePair};
use torii::domain::response::{
    HeaderJsonFormat, ResponseHeaderRow, parse_response_header_rows, serialize_response_header_rows,
};

// ---------------------------------------------------------------------------
// Response header row round-trip
// ---------------------------------------------------------------------------

#[test]
fn header_rows_preserve_duplicate_names() {
    let rows = vec![
        ResponseHeaderRow {
            name: "set-cookie".into(),
            value: "a=1".into(),
        },
        ResponseHeaderRow {
            name: "set-cookie".into(),
            value: "b=2".into(),
        },
        ResponseHeaderRow {
            name: "content-type".into(),
            value: "text/html".into(),
        },
    ];
    let encoded = serialize_response_header_rows(&rows).unwrap();
    let (decoded, format) = parse_response_header_rows(Some(&encoded));
    assert_eq!(format, Some(HeaderJsonFormat::LosslessRows));
    assert_eq!(decoded.len(), 3);
    assert_eq!(decoded[0].name, "set-cookie");
    assert_eq!(decoded[0].value, "a=1");
    assert_eq!(decoded[1].name, "set-cookie");
    assert_eq!(decoded[1].value, "b=2");
    assert_eq!(decoded[2].name, "content-type");
    assert_eq!(decoded[2].value, "text/html");
}

#[test]
fn header_rows_legacy_object_map_fallback() {
    let raw = r#"{"content-type":"application/json","x-custom":"value"}"#;
    let (rows, format) = parse_response_header_rows(Some(raw));
    assert_eq!(format, Some(HeaderJsonFormat::LegacyObjectMap));
    assert_eq!(rows.len(), 2);
}

#[test]
fn header_rows_empty_input() {
    let (rows, format) = parse_response_header_rows(None);
    assert!(rows.is_empty());
    assert!(format.is_none());

    let (rows, format) = parse_response_header_rows(Some(""));
    assert!(rows.is_empty());
    assert!(format.is_none());
}

#[test]
fn header_rows_serialize_empty_returns_none() {
    assert!(serialize_response_header_rows(&[]).is_none());
}

// ---------------------------------------------------------------------------
// Cookie parsing (via the cookie crate, exercised through domain types)
// ---------------------------------------------------------------------------

#[test]
fn cookie_parse_full_attributes() {
    let value = "session=abc123; Domain=example.com; Path=/api; Secure; HttpOnly; SameSite=Strict";
    let cookie = cookie::Cookie::parse(value).expect("should parse");
    assert_eq!(cookie.name(), "session");
    assert_eq!(cookie.value(), "abc123");
    assert_eq!(cookie.domain(), Some("example.com"));
    assert_eq!(cookie.path(), Some("/api"));
    assert!(cookie.secure().unwrap_or(false));
    assert!(cookie.http_only().unwrap_or(false));
    assert_eq!(cookie.same_site(), Some(cookie::SameSite::Strict));
}

#[test]
fn cookie_parse_multiple_same_name() {
    let c1 = cookie::Cookie::parse("theme=dark").unwrap();
    let c2 = cookie::Cookie::parse("theme=light").unwrap();
    assert_eq!(c1.name(), c2.name());
    assert_ne!(c1.value(), c2.value());
}

#[test]
fn cookie_parse_empty_value_returns_error() {
    // The cookie crate returns an error for completely empty strings
    assert!(cookie::Cookie::parse("").is_err());
}

// ---------------------------------------------------------------------------
// Error classifier helpers
// ---------------------------------------------------------------------------

#[test]
fn error_chain_string_joins_causes() {
    let err = anyhow::anyhow!("outer").context("context");
    let chain = torii::services::error_classifier::error_chain_string(&err);
    assert!(chain.contains("context"));
    assert!(chain.contains("outer"));
}

#[test]
fn error_chain_string_single_error() {
    let err = anyhow::anyhow!("single error");
    let chain = torii::services::error_classifier::error_chain_string(&err);
    assert_eq!(chain, "single error");
}

// ---------------------------------------------------------------------------
// Domain model basics
// ---------------------------------------------------------------------------

#[test]
fn key_value_pair_builder() {
    let kv = KeyValuePair::new("key", "value");
    assert_eq!(kv.key, "key");
    assert_eq!(kv.value, "value");
    assert!(kv.enabled);

    let kv_disabled = KeyValuePair::new("key", "value").disabled();
    assert!(!kv_disabled.enabled);

    let kv_enabled = KeyValuePair::new("key", "value").disabled().enabled();
    assert!(kv_enabled.enabled);
}

#[test]
fn auth_type_default_is_none() {
    assert!(matches!(AuthType::default(), AuthType::None));
}

#[test]
fn body_type_default_is_none() {
    assert!(matches!(BodyType::default(), BodyType::None));
}

#[test]
fn body_ref_size_bytes() {
    use bytes::Bytes;
    use torii::domain::response::BodyRef;

    assert_eq!(BodyRef::Empty.size_bytes(), 0);

    let preview = BodyRef::InMemoryPreview {
        bytes: Bytes::from_static(b"hello"),
        truncated: false,
    };
    assert_eq!(preview.size_bytes(), 5);

    let blob = BodyRef::DiskBlob {
        blob_id: "abc".into(),
        preview: None,
        size_bytes: 1024,
    };
    assert_eq!(blob.size_bytes(), 1024);
}

#[test]
fn normalize_unix_ms_converts_seconds_to_millis() {
    use torii::domain::response::normalize_unix_ms;
    assert_eq!(normalize_unix_ms(1_700_000_000), 1_700_000_000_000);
    assert_eq!(normalize_unix_ms(1_700_000_000_000), 1_700_000_000_000);
    assert_eq!(normalize_unix_ms(-1), -1000);
}

#[test]
fn response_budgets_constants() {
    use torii::domain::response::ResponseBudgets;
    assert_eq!(ResponseBudgets::PREVIEW_CAP_BYTES, 2 * 1024 * 1024);
    assert_eq!(ResponseBudgets::PER_TAB_CAP_BYTES, 32 * 1024 * 1024);
}
