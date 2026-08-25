//! JMAP error objects: request-level *problem details* (RFC 8620 §3.6.1)
//! and method-level error values (§3.6.2). Internal detail never leaks
//! into a client-visible `detail`/`description`.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

/// A request-level problem — the whole HTTP request fails with a JSON
/// problem object and a status code.
#[derive(Debug)]
pub struct Problem {
    /// HTTP status.
    pub status: StatusCode,
    /// The problem `type` URI.
    pub type_uri: &'static str,
    /// Optional human detail (never internal error text).
    pub detail: Option<String>,
    /// Optional machine-readable context, merged into the problem body.
    ///
    /// For the refusal a client has to *act* on rather than merely show: the
    /// first caller is the lead import (B2.09), whose `422` carries the
    /// per-row report naming which line broke which rule — a refusal a person
    /// cannot act on is the one thing an importer must never answer. It is
    /// server-authored context, never an echo of the request, and it is
    /// deliberately not part of the JMAP envelope's own error shapes.
    pub extra: Option<Value>,
}

impl Problem {
    fn new(status: StatusCode, type_uri: &'static str) -> Self {
        Self {
            status,
            type_uri,
            detail: None,
            extra: None,
        }
    }

    /// With a human-readable detail.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// The request body was not valid JSON (§3.6.1).
    pub fn not_json() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "urn:ietf:params:jmap:error:notJSON",
        )
    }

    /// The body was valid JSON but not a valid JMAP Request object.
    pub fn not_request() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "urn:ietf:params:jmap:error:notRequest",
        )
    }

    /// A `using` capability the server does not support.
    pub fn unknown_capability() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "urn:ietf:params:jmap:error:unknownCapability",
        )
    }

    /// A request-level limit was exceeded (§3.6.1 `limit`).
    pub fn limit(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "urn:ietf:params:jmap:error:limit").detail(detail)
    }

    /// Missing/invalid bearer token → 401.
    pub fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "about:blank").detail("missing or invalid bearer token")
    }

    /// The account in the URL/args is not the authenticated one → 404
    /// (no cross-account existence oracle).
    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "about:blank")
    }

    /// The request object exceeded `maxSizeRequestObject` → 400.
    pub fn too_large() -> Self {
        Self::new(StatusCode::BAD_REQUEST, "urn:ietf:params:jmap:error:limit")
            .detail("request object too large")
    }

    /// An internal failure → 500, with no leaked detail.
    pub fn server_error() -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "about:blank")
    }

    /// A problem with an explicit status and a safe, client-visible detail
    /// (never internal error text). For endpoints outside the JMAP envelope.
    pub fn with(status: StatusCode, detail: impl Into<String>) -> Self {
        Self::new(status, "about:blank").detail(detail)
    }

    /// With machine-readable context merged into the problem body.
    ///
    /// Only the members of a JSON **object** are merged, and never over the
    /// problem's own `type`, `status` or `detail`: context added to a refusal
    /// must not be able to change what the refusal says.
    pub fn with_extra(mut self, extra: Value) -> Self {
        self.extra = Some(extra);
        self
    }
}

/// A store failure as the problem it is on the wire.
///
/// The same mapping [`crate::billing::map_store_err`] has always made — that
/// function is now this impl, so there is exactly one table — and it exists as
/// a `From` because the store's own transactional flows take the caller's error
/// type (`E: From<StoreError>`) so that a callback of ours can run inside their
/// transaction ([`alo_store::inv_po_send`]).
///
/// Anything that is not one of the four typed refusals is a `500` **with no
/// detail**: a `Db` error's text is internal, and internal text is not
/// something we hand a client.
impl From<alo_store::StoreError> for Problem {
    fn from(error: alo_store::StoreError) -> Self {
        use alo_store::StoreError;
        match error {
            StoreError::NotFound => Self::with(StatusCode::NOT_FOUND, "not found"),
            StoreError::Forbidden => Self::with(StatusCode::FORBIDDEN, "insufficient role"),
            StoreError::Validation(message) => {
                Self::with(StatusCode::UNPROCESSABLE_ENTITY, message)
            }
            StoreError::Conflict(message) => Self::with(StatusCode::CONFLICT, message),
            // The four above are the caller's to fix, and the wire says so.
            // Everything else is ours, and a 500 deliberately tells the caller
            // nothing — so it gets written down here instead. Dropping it left
            // the log silent for precisely the failures worth investigating.
            other => {
                tracing::warn!(cause = %other.log_cause(), "store failure returned as 500");
                Self::server_error()
            }
        }
    }
}

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let mut body = json!({ "type": self.type_uri, "status": self.status.as_u16() });
        if let Some(Value::Object(extra)) = self.extra {
            for (key, value) in extra {
                if !matches!(key.as_str(), "type" | "status" | "detail") {
                    body[key] = value;
                }
            }
        }
        if let Some(detail) = &self.detail {
            body["detail"] = json!(detail);
        }
        let mut resp = (self.status, Json(body)).into_response();
        if self.status == StatusCode::UNAUTHORIZED {
            resp.headers_mut().insert(
                axum::http::header::WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_static("Bearer"),
            );
        }
        resp
    }
}

/// A method-level error value (§3.6.2), placed as
/// `["error", <this>, callId]` in `methodResponses`.
pub fn method_error(type_: &str) -> Value {
    json!({ "type": type_ })
}

/// A method error carrying a description.
pub fn method_error_desc(type_: &str, description: &str) -> Value {
    json!({ "type": type_, "description": description })
}
