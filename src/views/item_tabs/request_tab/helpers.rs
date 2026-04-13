use super::*;

pub(super) fn section_tab_button(
    id: &'static str,
    label: String,
    active: bool,
    cx: &App,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    if active {
        Button::new(id)
            .ghost()
            .label(label)
            .border_b_2()
            .border_color(cx.theme().primary)
            .font_weight(FontWeight::MEDIUM)
            .on_click(on_click)
    } else {
        Button::new(id)
            .ghost()
            .label(label)
            .text_color(cx.theme().muted_foreground)
            .on_click(on_click)
    }
}

pub(super) fn response_tab_button(
    id: &'static str,
    label: String,
    active: bool,
    cx: &App,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    if active {
        Button::new(id)
            .ghost()
            .label(label)
            .border_b_2()
            .border_color(cx.theme().primary)
            .font_weight(FontWeight::MEDIUM)
            .on_click(on_click)
    } else {
        Button::new(id)
            .ghost()
            .label(label)
            .text_color(cx.theme().muted_foreground)
            .on_click(on_click)
    }
}

#[derive(Debug, Clone)]
pub(super) struct CookieRow {
    pub(super) name: String,
    pub(super) value_preview: String,
    pub(super) domain: Option<String>,
    pub(super) path: Option<String>,
    pub(super) expires_or_max_age: Option<String>,
    pub(super) secure: bool,
    pub(super) http_only: bool,
    pub(super) same_site: Option<String>,
}

pub(super) fn status_code_color(status_code: u16) -> Hsla {
    match status_code {
        200..=299 => gpui::hsla(120. / 360., 0.7, 0.35, 1.),
        300..=399 => gpui::hsla(210. / 360., 0.8, 0.45, 1.),
        400..=499 => gpui::hsla(42. / 360., 0.9, 0.4, 1.),
        _ => gpui::hsla(0., 0.78, 0.45, 1.),
    }
}

pub(super) enum SaveSource {
    InMemory(Vec<u8>),
    Blob(String),
}

pub(super) fn response_body_preview_text(
    response: &crate::domain::response::ResponseSummary,
    loaded_full_body_text: &Option<String>,
) -> String {
    if let Some(full) = loaded_full_body_text {
        return full.clone();
    }

    match &response.body_ref {
        BodyRef::Empty => String::new(),
        BodyRef::InMemoryPreview { bytes, .. } => {
            render_preview_text(bytes.as_ref(), response.media_type.as_deref())
        }
        BodyRef::DiskBlob {
            preview,
            size_bytes,
            ..
        } => {
            let preview_text = preview
                .as_ref()
                .map(|b| render_preview_text(b.as_ref(), response.media_type.as_deref()))
                .unwrap_or_default();
            let preview_len = preview.as_ref().map(|b| b.len()).unwrap_or(0) as u64;
            if *size_bytes > preview_len {
                format!(
                    "{}\n{}",
                    preview_text,
                    es_fluent::localize("request_tab_response_truncated", None)
                )
            } else {
                preview_text
            }
        }
    }
}

pub(super) fn looks_like_image(media_type: Option<&str>) -> bool {
    matches!(media_type, Some(value) if value.to_ascii_lowercase().starts_with("image/"))
}

pub(super) fn looks_like_html(media_type: Option<&str>) -> bool {
    matches!(
        media_type,
        Some(value) if value.to_ascii_lowercase() == "text/html"
    )
}

pub(super) fn is_text_like_media_type(media_type: Option<&str>) -> bool {
    let Some(media_type) = media_type else {
        return true;
    };
    let media_type = media_type.to_ascii_lowercase();
    media_type.starts_with("text/")
        || matches!(
            media_type.as_str(),
            "application/json"
                | "application/xml"
                | "text/xml"
                | "text/html"
                | "application/javascript"
                | "application/x-www-form-urlencoded"
        )
}

pub(super) fn search_matches(text: &str, query: &str) -> Vec<usize> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let text_lower = text.to_ascii_lowercase();
    let query_lower = query.to_ascii_lowercase();
    let mut matches = Vec::new();
    let mut offset = 0;
    while let Some(found) = text_lower[offset..].find(&query_lower) {
        let absolute = offset + found;
        matches.push(absolute);
        offset = absolute + query_lower.len().max(1);
        if offset >= text.len() {
            break;
        }
    }
    matches
}

pub(super) fn suggested_file_name(media_type: Option<&str>) -> String {
    let ext = match media_type.map(|v| v.to_ascii_lowercase()) {
        Some(mt) if mt == "application/json" => "json",
        Some(mt) if mt == "application/xml" || mt == "text/xml" => "xml",
        Some(mt) if mt == "text/html" => "html",
        Some(mt) if mt.starts_with("text/") => "txt",
        Some(mt) if mt.starts_with("image/") => "img",
        _ => "bin",
    };
    format!("response.{ext}")
}

pub(super) fn standard_method_index(method: &str) -> Option<usize> {
    match method.to_ascii_uppercase().as_str() {
        "GET" => Some(0),
        "POST" => Some(1),
        "PUT" => Some(2),
        "PATCH" => Some(3),
        "DELETE" => Some(4),
        "HEAD" => Some(5),
        "OPTIONS" => Some(6),
        _ => None,
    }
}

pub(super) fn parse_set_cookie_rows(
    rows: &[crate::domain::response::ResponseHeaderRow],
) -> Vec<CookieRow> {
    let mut parsed = Vec::new();
    for row in rows {
        if !row.name.eq_ignore_ascii_case("set-cookie") {
            continue;
        }
        let Ok(cookie) = cookie::Cookie::parse(row.value.clone()) else {
            continue;
        };

        let raw_value = cookie.value().to_string();
        let value_preview = if raw_value.len() > 80 {
            format!("{}…", &raw_value[..80])
        } else {
            raw_value
        };
        let expires_or_max_age = cookie
            .max_age()
            .map(|d| format!("{}s", d.whole_seconds()))
            .or_else(|| {
                cookie.expires().and_then(|v| {
                    v.datetime().and_then(|dt| {
                        time::format_description::parse(
                            "[year]-[month]-[day] [hour]:[minute]:[second] UTC",
                        )
                        .ok()
                        .and_then(|fmt| dt.format(&fmt).ok())
                    })
                })
            });
        let same_site = cookie.same_site().map(|s| format!("{s:?}"));

        parsed.push(CookieRow {
            name: cookie.name().to_string(),
            value_preview,
            domain: cookie.domain().map(ToOwned::to_owned),
            path: cookie.path().map(ToOwned::to_owned),
            expires_or_max_age,
            secure: cookie.secure().unwrap_or(false),
            http_only: cookie.http_only().unwrap_or(false),
            same_site,
        });
    }
    parsed
}

pub(super) fn format_unix_ms(value: Option<i64>) -> String {
    let Some(value) = value else {
        return "—".to_string();
    };
    let ts = crate::domain::response::normalize_unix_ms(value);
    let nanos = ts.saturating_mul(1_000_000) as i128;
    let Ok(dt) = time::OffsetDateTime::from_unix_timestamp_nanos(nanos) else {
        return "—".to_string();
    };
    let Ok(fmt) = time::format_description::parse(
        "[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3] UTC",
    ) else {
        return "—".to_string();
    };
    dt.format(&fmt).unwrap_or_else(|_| "—".to_string())
}

pub(super) fn format_bytes(size: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    if size < 1024 {
        format!("{size} B")
    } else if (size as f64) < MB {
        format!("{:.1} KB", (size as f64) / KB)
    } else {
        format!("{:.2} MB", (size as f64) / MB)
    }
}

pub(super) fn params_from_url_query(url: &str) -> Vec<KeyValuePair> {
    let raw_query = if let Ok(parsed) = url::Url::parse(url) {
        parsed.query().map(ToOwned::to_owned)
    } else {
        url.split_once('?')
            .map(|(_, q)| q.split_once('#').map(|(qq, _)| qq).unwrap_or(q).to_string())
    };

    raw_query
        .map(|q| {
            url::form_urlencoded::parse(q.as_bytes())
                .map(|(k, v)| KeyValuePair::new(k.to_string(), v.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub(super) fn url_with_params(base_url: &str, params: &[KeyValuePair]) -> String {
    let enabled: Vec<(String, String)> = params
        .iter()
        .filter(|p| p.enabled && !p.key.trim().is_empty())
        .map(|p| (p.key.clone(), p.value.clone()))
        .collect();

    if let Ok(mut parsed) = url::Url::parse(base_url) {
        if enabled.is_empty() {
            parsed.set_query(None);
        } else {
            parsed
                .query_pairs_mut()
                .clear()
                .extend_pairs(enabled.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        }
        return parsed.to_string();
    }

    let (base, fragment) = match base_url.split_once('#') {
        Some((b, f)) => (b, Some(f)),
        None => (base_url, None),
    };
    let path_only = base.split_once('?').map(|(p, _)| p).unwrap_or(base);
    let query = if enabled.is_empty() {
        String::new()
    } else {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (k, v) in &enabled {
            serializer.append_pair(k, v);
        }
        serializer.finish()
    };

    match (query.is_empty(), fragment) {
        (true, Some(f)) => format!("{path_only}#{f}"),
        (true, None) => path_only.to_string(),
        (false, Some(f)) => format!("{path_only}?{query}#{f}"),
        (false, None) => format!("{path_only}?{query}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AuthKind {
    None,
    Basic,
    Bearer,
    ApiKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BodyKind {
    None,
    RawText,
    RawJson,
    UrlEncoded,
    FormData,
    BinaryFile,
}

pub(super) fn auth_kind_from_label(label: &str) -> AuthKind {
    match label {
        "Basic" => AuthKind::Basic,
        "Bearer" => AuthKind::Bearer,
        "API Key" => AuthKind::ApiKey,
        _ => AuthKind::None,
    }
}

pub(super) fn auth_type_index(auth: &AuthType) -> usize {
    match auth {
        AuthType::None => 0,
        AuthType::Basic { .. } => 1,
        AuthType::Bearer { .. } => 2,
        AuthType::ApiKey { .. } => 3,
    }
}

pub(super) fn api_key_location_index(location: ApiKeyLocation) -> usize {
    match location {
        ApiKeyLocation::Header => 0,
        ApiKeyLocation::Query => 1,
    }
}

pub(super) fn api_key_location_from_index(index: usize) -> ApiKeyLocation {
    match index {
        1 => ApiKeyLocation::Query,
        _ => ApiKeyLocation::Header,
    }
}

pub(super) fn body_kind_from_label(label: &str) -> BodyKind {
    match label {
        "Raw Text" => BodyKind::RawText,
        "Raw JSON" => BodyKind::RawJson,
        "URL Encoded" => BodyKind::UrlEncoded,
        "Form Data" => BodyKind::FormData,
        "Binary File" => BodyKind::BinaryFile,
        _ => BodyKind::None,
    }
}

pub(super) fn body_type_index(body: &BodyType) -> usize {
    match body {
        BodyType::None => 0,
        BodyType::RawText { .. } => 1,
        BodyType::RawJson { .. } => 2,
        BodyType::UrlEncoded { .. } => 3,
        BodyType::FormData { .. } => 4,
        BodyType::BinaryFile { .. } => 5,
    }
}

pub(super) fn latest_run_summary(exec_status: &ExecStatus) -> String {
    match exec_status {
        ExecStatus::Idle => es_fluent::localize("request_tab_latest_run_none", None).to_string(),
        ExecStatus::Sending => es_fluent::localize("request_tab_sending", None).to_string(),
        ExecStatus::Streaming => es_fluent::localize("request_tab_streaming", None).to_string(),
        ExecStatus::Completed { response } => {
            let status = format!("{} {}", response.status_code, response.status_text);
            if let Some(ms) = response.total_ms {
                format!("{status} • {ms} ms")
            } else {
                status
            }
        }
        ExecStatus::Failed { summary, .. } => format!(
            "{}: {}",
            es_fluent::localize("request_tab_response_failed", None),
            summary
        ),
        ExecStatus::Cancelled { partial_size } => match partial_size {
            Some(size) => format!(
                "{} ({size})",
                es_fluent::localize("request_tab_response_cancelled_with_bytes", None)
            ),
            None => es_fluent::localize("request_tab_response_cancelled", None).to_string(),
        },
    }
}

pub(super) fn classified_error_display(
    classified: Option<&ClassifiedError>,
    summary: &str,
) -> (String, String) {
    match classified {
        Some(ClassifiedError::DnsFailure { host }) => (
            es_fluent::localize("request_tab_error_dns_failure", None).to_string(),
            format!("Could not resolve host: {host}"),
        ),
        Some(ClassifiedError::ConnectionRefused { host, port }) => (
            es_fluent::localize("request_tab_error_connection_refused", None).to_string(),
            format!("Connection refused: {host}:{port}"),
        ),
        Some(ClassifiedError::ConnectionTimeout) => (
            es_fluent::localize("request_tab_error_connection_timeout", None).to_string(),
            summary.to_string(),
        ),
        Some(ClassifiedError::RequestTimeout) => (
            es_fluent::localize("request_tab_error_request_timeout", None).to_string(),
            summary.to_string(),
        ),
        Some(ClassifiedError::TlsError { reason }) => (
            es_fluent::localize("request_tab_error_tls_failure", None).to_string(),
            reason.clone(),
        ),
        Some(ClassifiedError::TransportError { summary, detail }) => {
            (summary.clone(), detail.clone())
        }
        None => (
            es_fluent::localize("request_tab_error_transport_generic", None).to_string(),
            summary.to_string(),
        ),
    }
}

pub(super) fn render_preview_text(bytes: &[u8], media_type: Option<&str>) -> String {
    let text = String::from_utf8_lossy(bytes).to_string();
    if matches!(media_type, Some(mt) if mt.eq_ignore_ascii_case("application/json")) {
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(value) => serde_json::to_string_pretty(&value).unwrap_or(text),
            Err(_) => text,
        }
    } else if matches!(
        media_type,
        Some(mt)
            if mt.eq_ignore_ascii_case("application/xml")
                || mt.eq_ignore_ascii_case("text/xml")
                || mt.eq_ignore_ascii_case("text/html")
    ) {
        pretty_print_xml_like(&text).unwrap_or(text)
    } else {
        text
    }
}

pub(super) fn pretty_print_xml_like(input: &str) -> Option<String> {
    use quick_xml::Writer;
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(input);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(event) => {
                if writer.write_event(event).is_err() {
                    return None;
                }
            }
            Err(_) => return None,
        }
        buf.clear();
    }

    String::from_utf8(writer.into_inner()).ok()
}

pub(super) fn truncate_for_tab_cap(bytes: Vec<u8>, max_bytes: usize) -> (Vec<u8>, bool) {
    if bytes.len() > max_bytes {
        (bytes[..max_bytes].to_vec(), true)
    } else {
        (bytes, false)
    }
}
