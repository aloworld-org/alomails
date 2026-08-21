//! The common MAPI-over-HTTP response envelope ([MS-OXCMAPIHTTP] §2.2.2.2).
//!
//! Every response on every endpoint has the same shape, and getting it wrong is
//! invisible: Outlook does not report a malformed envelope, it simply stops
//! talking. So the envelope lives in one place, is built one way, and is
//! covered by tests that read the bytes rather than the intent.
//!
//! The shape, quoting the specification's own layout:
//!
//! ```text
//! HTTP/1.1 200 OK
//! Content-Length: <length of META-TAGS, ADDITIONAL HEADERS and RESPONSE BODY>
//! Content-Type: application/mapi-http
//! X-RequestType: <?>
//! X-ResponseCode: <?>
//! X-RequestId: <?>
//! X-ServerApplication: <server version>
//!
//! PROCESSING<CRLF>
//! DONE<CRLF>
//! X-ResponseCode: 0<CRLF>
//! X-ElapsedTime: <milliseconds><CRLF>
//! X-StartTime: <date/time><CRLF>
//! <CRLF>
//! <RESPONSE BODY>
//! ```
//!
//! Two properties of that layout are easy to lose and expensive to debug:
//!
//! * **`X-ResponseCode` appears twice** — once as a real HTTP header and again
//!   inside the body's additional headers. They are not redundant to the
//!   client, and both are written here from the same value so they cannot drift.
//! * **The body is `\r\n`-framed text followed by raw little-endian binary.**
//!   The blank line is the boundary. A `\n` anywhere in the framing makes the
//!   whole response unparseable.
//!
//! The HTTP status is `200 OK` even for most failures: the specification
//! reserves non-200 for authentication (`401`), redirection (`302`), and
//! genuinely exceptional conditions (`5xx`). A failure is carried in
//! [`ResponseCode`], not in the status line.

use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// The media type every request and response on this protocol carries.
pub const MAPI_CONTENT_TYPE: &str = "application/mapi-http";

/// What this server reports as its version ([MS-OXCMAPIHTTP] §2.2.3.3.6).
pub const SERVER_APPLICATION: &str = "alo/1.0";

/// `X-ResponseCode` ([MS-OXCMAPIHTTP] §2.2.3.3.3) — the result of a request
/// from the transport's point of view, carried on a `200 OK`.
///
/// Zero means the client should parse the response body for the request it
/// issued; anything else means the body carries diagnostic information
/// instead. The numbers are the specification's and must not be renumbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ResponseCode {
    /// The request was properly formatted and accepted.
    Success = 0,
    /// The request produced an unknown failure.
    UnknownFailure = 1,
    /// The request has an invalid verb.
    InvalidVerb = 2,
    /// The request has an invalid path.
    InvalidPath = 3,
    /// The request has an invalid header.
    InvalidHeader = 4,
    /// The request has an invalid `X-RequestType` header.
    InvalidRequestType = 5,
    /// The request has an invalid session context cookie.
    InvalidContextCookie = 6,
    /// The request has a missing required header.
    MissingHeader = 7,
    /// The request is anonymous, but anonymous requests are not accepted.
    AnonymousNotAllowed = 8,
    /// The request is too large.
    TooLarge = 9,
    /// The Session Context is not found.
    ContextNotFound = 10,
    /// The client has no privileges to the Session Context.
    NoPrivilege = 11,
    /// The request body is invalid.
    InvalidRequestBody = 12,
    /// The request is missing a required cookie.
    MissingCookie = 13,
    /// One request at a time per Session Context was violated.
    InvalidSequence = 15,
    /// The endpoint is disabled.
    EndpointDisabled = 16,
    /// The endpoint is shutting down.
    EndpointShuttingDown = 18,
}

impl ResponseCode {
    /// The number that goes on the wire.
    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }
}

/// A response under construction: the envelope plus an optional binary body.
pub struct MapiResponse {
    request_type: String,
    request_id: String,
    client_info: Option<String>,
    code: ResponseCode,
    body: Vec<u8>,
    cookies: Vec<String>,
    extra: Vec<(&'static str, String)>,
}

impl MapiResponse {
    /// A response to `request_type`, echoing the client's `X-RequestId`.
    ///
    /// The request id is echoed rather than generated: the client uses it to
    /// match a response to the request it made, and inventing one strands it.
    #[must_use]
    pub fn new(request_type: &str, request_id: &str, code: ResponseCode) -> Self {
        Self {
            request_type: request_type.to_owned(),
            request_id: request_id.to_owned(),
            client_info: None,
            code,
            body: Vec::new(),
            cookies: Vec::new(),
            extra: Vec::new(),
        }
    }

    /// Adds one of the protocol's own headers — `X-PendingPeriod`,
    /// `X-ExpirationInfo` and the like.
    ///
    /// The name is `&'static str` on purpose: these are protocol constants, and
    /// taking an owned name here would open a path for a caller to write a
    /// header name out of a client-supplied string.
    #[must_use]
    pub fn with_header(mut self, name: &'static str, value: String) -> Self {
        self.extra.push((name, value));
        self
    }

    /// Echoes the client's `X-ClientInfo` back, as the specification's examples
    /// show the server doing.
    #[must_use]
    pub fn with_client_info(mut self, info: Option<&str>) -> Self {
        self.client_info = info.map(ToOwned::to_owned);
        self
    }

    /// The request-type-specific binary body, appended after the framing.
    #[must_use]
    pub fn with_body(mut self, body: Vec<u8>) -> Self {
        self.body = body;
        self
    }

    /// Adds a `Set-Cookie`. Session-context cookies are opaque and random —
    /// never a mailbox id, which would let one client name another's context.
    #[must_use]
    pub fn with_cookie(mut self, cookie: String) -> Self {
        self.cookies.push(cookie);
        self
    }

    /// The framed payload: meta-tags, the additional headers, a blank line, and
    /// then the binary body.
    ///
    /// `PROCESSING` then `DONE` with no `PENDING` between them: `PENDING` is for
    /// a long-running request that must keep the connection warm, and nothing
    /// here is long-running yet. Both are CRLF-terminated, as is every framing
    /// line — the blank line that ends the headers is the body boundary.
    fn payload(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(96 + self.body.len());
        out.extend_from_slice(b"PROCESSING\r\n");
        out.extend_from_slice(b"DONE\r\n");
        out.extend_from_slice(format!("X-ResponseCode: {}\r\n", self.code.code()).as_bytes());
        // Elapsed and start times are diagnostic only. They are reported as
        // zero rather than measured: a real clock here would make every
        // response byte-unstable and every test a snapshot of a stopwatch.
        out.extend_from_slice(b"X-ElapsedTime: 0\r\n");
        out.extend_from_slice(b"X-StartTime: 0\r\n");
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&self.body);
        out
    }
}

impl IntoResponse for MapiResponse {
    fn into_response(self) -> Response {
        let payload = self.payload();
        let mut response = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, MAPI_CONTENT_TYPE)
            .header("X-RequestType", header_value(&self.request_type))
            .header("X-ResponseCode", self.code.code())
            .header("X-RequestId", header_value(&self.request_id))
            .header("X-ServerApplication", SERVER_APPLICATION);
        if let Some(info) = &self.client_info {
            response = response.header("X-ClientInfo", header_value(info));
        }
        for (name, value) in &self.extra {
            response = response.header(*name, header_value(value));
        }
        let mut response = match response.body(axum::body::Body::from(payload)) {
            Ok(response) => response,
            // Unreachable in practice: every value above is either a constant
            // or sanitised by `header_value`. Answering with a bare 500 keeps
            // this infallible rather than panicking inside a request.
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        for cookie in &self.cookies {
            if let Ok(value) = HeaderValue::from_str(cookie) {
                response
                    .headers_mut()
                    .append(HeaderName::from_static("set-cookie"), value);
            }
        }
        response
    }
}

/// A header value that cannot break the header block.
///
/// Client-supplied strings (`X-RequestId`, `X-ClientInfo`) are echoed back, so
/// they are values we do not control being written into headers — exactly the
/// shape of header injection. Anything outside printable ASCII is dropped
/// rather than escaped: these are opaque identifiers, and a client that sent a
/// control character was not going to match on it anyway.
fn header_value(raw: &str) -> HeaderValue {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(256)
        .collect();
    HeaderValue::from_str(&cleaned).unwrap_or_else(|_| HeaderValue::from_static(""))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use axum::body::to_bytes;

    async fn bytes_of(response: Response) -> Vec<u8> {
        to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("body")
            .to_vec()
    }

    /// The framing is `\r\n` throughout and the blank line separates the
    /// headers from the binary body. A stray `\n` makes the whole response
    /// unparseable, and Outlook reports that by going quiet rather than
    /// complaining — so it is asserted on the bytes.
    #[tokio::test]
    async fn envelope_is_crlf_framed_with_a_blank_line_before_the_body() {
        let response = MapiResponse::new("Connect", "req-1", ResponseCode::Success)
            .with_body(vec![0xDE, 0xAD, 0xBE, 0xEF])
            .into_response();
        let body = bytes_of(response).await;

        let framing = b"PROCESSING\r\nDONE\r\nX-ResponseCode: 0\r\nX-ElapsedTime: 0\r\nX-StartTime: 0\r\n\r\n";
        assert!(
            body.starts_with(framing),
            "framing was {:?}",
            String::from_utf8_lossy(&body[..body.len().min(120)])
        );
        assert_eq!(&body[framing.len()..], &[0xDE, 0xAD, 0xBE, 0xEF]);
        // No bare newline anywhere in the framing.
        let head = &body[..framing.len()];
        for (index, window) in head.windows(2).enumerate() {
            if window[1] == b'\n' {
                assert_eq!(window[0], b'\r', "bare LF at byte {index}");
            }
        }
    }

    /// The code appears as an HTTP header *and* inside the body, and the two
    /// are the same number. They are written from one value so they cannot
    /// drift apart as later stages add failure paths.
    #[tokio::test]
    async fn the_response_code_is_stated_identically_in_both_places() {
        let response =
            MapiResponse::new("Connect", "req-2", ResponseCode::InvalidRequestBody).into_response();
        let header = response
            .headers()
            .get("X-ResponseCode")
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned)
            .expect("header present");
        let body = String::from_utf8(bytes_of(response).await).expect("utf8 framing");

        assert_eq!(header, "12");
        assert!(body.contains("X-ResponseCode: 12\r\n"), "{body:?}");
    }

    /// A failure is a `200 OK` carrying a code, not an HTTP error status. The
    /// specification reserves non-200 for auth, redirection and exceptional
    /// conditions; answering 400 here would make Outlook give up instead of
    /// reading the diagnosis.
    #[tokio::test]
    async fn failures_are_two_hundred_with_a_code() {
        for code in [
            ResponseCode::MissingHeader,
            ResponseCode::InvalidRequestType,
            ResponseCode::ContextNotFound,
        ] {
            let response = MapiResponse::new("Connect", "r", code).into_response();
            assert_eq!(response.status(), StatusCode::OK, "{code:?}");
            assert_eq!(
                response.headers().get(header::CONTENT_TYPE).unwrap(),
                MAPI_CONTENT_TYPE
            );
        }
    }

    /// The request id is echoed, because the client matches responses by it.
    /// It is also client-supplied text going into a header, so a value that
    /// tries to end the header block early is stripped rather than reflected.
    #[tokio::test]
    async fn a_client_supplied_id_cannot_forge_a_header() {
        let response = MapiResponse::new(
            "Connect",
            "abc\r\nX-ResponseCode: 0\r\nX-Injected: yes",
            ResponseCode::Success,
        )
        .with_client_info(Some("info\r\nX-Also-Injected: yes"))
        .into_response();

        assert!(response.headers().get("X-Injected").is_none());
        assert!(response.headers().get("X-Also-Injected").is_none());
        let echoed = response.headers().get("X-RequestId").unwrap();
        assert!(!echoed.to_str().unwrap().contains('\r'));
        // Exactly one response code header, not the two an injection would add.
        assert_eq!(
            response.headers().get_all("X-ResponseCode").iter().count(),
            1
        );
    }

    /// The numbers are the specification's; renumbering them silently breaks
    /// every client, so they are pinned.
    #[test]
    fn response_codes_match_the_specification() {
        assert_eq!(ResponseCode::Success.code(), 0);
        assert_eq!(ResponseCode::InvalidRequestType.code(), 5);
        assert_eq!(ResponseCode::MissingHeader.code(), 7);
        assert_eq!(ResponseCode::AnonymousNotAllowed.code(), 8);
        assert_eq!(ResponseCode::ContextNotFound.code(), 10);
        assert_eq!(ResponseCode::InvalidRequestBody.code(), 12);
        assert_eq!(ResponseCode::MissingCookie.code(), 13);
        // 14 is reserved and 17 is a client-side condition, so neither is here;
        // the gaps are deliberate rather than an oversight.
        assert_eq!(ResponseCode::InvalidSequence.code(), 15);
        assert_eq!(ResponseCode::EndpointShuttingDown.code(), 18);
    }
}
