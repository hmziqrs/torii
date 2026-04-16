use gpui::{FontWeight, Hsla, IntoElement, div, hsla, prelude::*};

// ---------------------------------------------------------------------------
// Protocol type — the "kind" of request, not the HTTP verb
// ---------------------------------------------------------------------------

/// The network protocol used by a request.
/// GraphQL is intentionally absent — it is HTTP at the transport level and
/// only differs in UI/workflow, not in how the connection works.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestProtocol {
    Http,
    WebSocket,
    Grpc,
}

impl RequestProtocol {
    /// Derive protocol from a method string stored on the request.
    /// HTTP verbs → Http; reserved strings identify other protocols.
    pub fn from_method(method: &str) -> Self {
        match method.to_ascii_uppercase().as_str() {
            "WS" | "WEBSOCKET" => Self::WebSocket,
            "GRPC" | "GRPC_UNARY" | "GRPC_STREAMING" => Self::Grpc,
            _ => Self::Http,
        }
    }

    /// Short display label shown in badges.
    pub fn label(self) -> &'static str {
        match self {
            Self::Http => "HTTP",
            Self::WebSocket => "WS",
            Self::Grpc => "gRPC",
        }
    }

    /// Semantic color for the protocol.
    pub fn color(self) -> Hsla {
        match self {
            Self::Http => hsla(217. / 360., 0.91, 0.60, 1.), // blue
            Self::WebSocket => hsla(142. / 360., 0.71, 0.45, 1.), // green
            Self::Grpc => hsla(262. / 360., 0.83, 0.58, 1.), // purple
        }
    }
}

/// Compact colored label for the request protocol (HTTP, WS, gRPC, GQL).
/// Use in breadcrumbs, sidebar rows, history lists, and any other place
/// the protocol needs to be identified at a glance.
pub fn protocol_badge(protocol: RequestProtocol) -> impl IntoElement {
    div()
        .text_color(protocol.color())
        .font_weight(FontWeight::BOLD)
        .text_xs()
        .child(protocol.label())
}

// ---------------------------------------------------------------------------
// HTTP method colors — for use in method selectors, dropdowns, etc.
// ---------------------------------------------------------------------------

/// Semantic color for an HTTP verb (GET, POST, …).
/// Use in method selectors and dropdowns — not in breadcrumbs.
pub fn method_color(method: &str) -> Hsla {
    match method.to_ascii_uppercase().as_str() {
        "GET" => hsla(142. / 360., 0.71, 0.45, 1.),
        "POST" => hsla(33. / 360., 0.95, 0.50, 1.),
        "PUT" => hsla(217. / 360., 0.91, 0.60, 1.),
        "PATCH" => hsla(262. / 360., 0.83, 0.58, 1.),
        "DELETE" => hsla(0. / 360., 0.84, 0.60, 1.),
        "HEAD" => hsla(188. / 360., 0.78, 0.41, 1.),
        "OPTIONS" => hsla(44. / 360., 0.90, 0.51, 1.),
        _ => hsla(0., 0., 0.55, 1.),
    }
}

/// Compact colored HTTP verb label (GET, POST, …).
/// Use in method selectors and dropdowns — not in breadcrumbs.
pub fn method_badge(method: &str) -> impl IntoElement {
    let color = method_color(method);
    div()
        .text_color(color)
        .font_weight(FontWeight::BOLD)
        .text_xs()
        .child(method.to_ascii_uppercase())
}
