use anyhow::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassifiedError {
    DnsFailure { host: String },
    ConnectionRefused { host: String, port: u16 },
    ConnectionTimeout,
    TlsError { reason: String },
    RequestTimeout,
    TransportError { summary: String, detail: String },
}

pub fn classify_transport_error(error: &Error) -> ClassifiedError {
    let detail = error_chain_string(error);

    if let Some(reqwest_error) = error.downcast_ref::<reqwest::Error>() {
        let host = reqwest_error
            .url()
            .and_then(|url| url.host_str())
            .unwrap_or_default()
            .to_string();
        let port = reqwest_error
            .url()
            .and_then(|url| url.port_or_known_default())
            .unwrap_or(443);

        if reqwest_error.is_timeout() {
            if reqwest_error.is_connect() {
                return ClassifiedError::ConnectionTimeout;
            }
            return ClassifiedError::RequestTimeout;
        }

        if reqwest_error.is_connect() {
            if has_io_kind(error, std::io::ErrorKind::ConnectionRefused) {
                return ClassifiedError::ConnectionRefused { host, port };
            }

            let lowered = detail.to_ascii_lowercase();
            if lowered.contains("dns")
                || lowered.contains("name or service not known")
                || lowered.contains("failed to lookup address")
                || lowered.contains("could not resolve host")
            {
                return ClassifiedError::DnsFailure { host };
            }
        }
    }

    let lowered = detail.to_ascii_lowercase();
    if lowered.contains("certificate")
        || lowered.contains("tls")
        || lowered.contains("handshake")
        || lowered.contains("x509")
    {
        return ClassifiedError::TlsError { reason: detail };
    }

    ClassifiedError::TransportError {
        summary: "Transport error".to_string(),
        detail,
    }
}

fn has_io_kind(error: &Error, kind: std::io::ErrorKind) -> bool {
    error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<std::io::Error>())
        .any(|io| io.kind() == kind)
}

pub fn error_chain_string(error: &Error) -> String {
    error
        .chain()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join(": ")
}

#[cfg(test)]
mod tests {
    use super::{ClassifiedError, classify_transport_error};

    #[test]
    fn timeout_is_classified() {
        let timeout_err = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(1))
            .build()
            .expect("client")
            .get("https://example.com")
            .build()
            .expect("request")
            .try_clone()
            .expect("clone");
        let anyhow_err = anyhow::anyhow!("request send failed: {timeout_err:?}");
        let classified = classify_transport_error(&anyhow_err);
        match classified {
            ClassifiedError::TransportError { .. } | ClassifiedError::RequestTimeout => {}
            other => panic!("unexpected classification: {other:?}"),
        }
    }
}
