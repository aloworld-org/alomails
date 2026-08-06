//! Rspamd spam-scoring client — consulted at DATA over Rspamd's
//! `POST /checkv2` HTTP endpoint (the pinned engine runs as a container;
//! we integrate, never patch it).
//!
//! Purpose-built rather than pulling a full HTTP client: the call is one
//! `POST` to a localhost/container controller over plaintext, so a small
//! `Connection: close` + read-to-EOF client with `serde_json` for the
//! body keeps the dependency surface — and the fail-closed/timeout
//! policy — ours (see `docs/design/spam-filtering-and-mta-sts.md`).
//!
//! Every failure (unreachable, bad status, unparseable/timeout) is a
//! typed error; the caller maps it to a **fail-closed** 451 so a scanner
//! outage never silently disables filtering.

use std::time::Duration;

use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Cap on the response we will read from the scanner (its JSON verdict
/// is small; bound it so a misbehaving endpoint cannot exhaust memory).
const MAX_RESPONSE: usize = 1024 * 1024;

/// The Rspamd action (RFC-less; Rspamd's documented action set), most to
/// least severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RspamdAction {
    /// Refuse the message (spam).
    Reject,
    /// Defer now, accept on retry (greylisting).
    Greylist,
    /// Temporary reject — sender should retry.
    SoftReject,
    /// Accept but mark (spam header).
    AddHeader,
    /// Accept but rewrite the subject.
    RewriteSubject,
    /// Accept, clean.
    NoAction,
}

impl RspamdAction {
    fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "reject" => Some(Self::Reject),
            "greylist" => Some(Self::Greylist),
            "soft reject" => Some(Self::SoftReject),
            "add header" => Some(Self::AddHeader),
            "rewrite subject" => Some(Self::RewriteSubject),
            "no action" => Some(Self::NoAction),
            _ => None,
        }
    }

    /// The `x-spam` result token for Authentication-Results: `yes` when
    /// Rspamd took any spam action, else `no`.
    pub fn spam_token(self) -> &'static str {
        match self {
            Self::NoAction => "no",
            _ => "yes",
        }
    }
}

/// A scan verdict.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RspamdVerdict {
    /// The action Rspamd chose.
    pub action: RspamdAction,
    /// The spam score.
    pub score: f64,
}

/// Envelope context passed to Rspamd as request metadata headers.
pub struct RspamdMeta<'a> {
    /// Connecting client IP.
    pub ip: &'a str,
    /// HELO/EHLO identity.
    pub helo: &'a str,
    /// MAIL FROM (envelope sender); `None` for the null path.
    pub mail_from: Option<&'a str>,
    /// Accepted recipients.
    pub recipients: &'a [String],
    /// Our hostname (`MTA-Name`).
    pub mta_name: &'a str,
}

/// Why a scan could not produce a verdict. The caller treats every
/// variant as fail-closed (451).
#[derive(Debug, thiserror::Error)]
pub enum RspamdError {
    /// The scanner could not be reached / the socket failed.
    #[error("rspamd connection failed: {0}")]
    Connect(String),
    /// The call exceeded the configured timeout.
    #[error("rspamd call timed out")]
    Timeout,
    /// The scanner answered with a non-200 status.
    #[error("rspamd returned status {0}")]
    BadStatus(u16),
    /// The response could not be parsed into a verdict.
    #[error("rspamd response could not be parsed: {0}")]
    Parse(String),
}

/// A configured Rspamd controller endpoint.
#[derive(Debug, Clone)]
pub struct RspamdClient {
    host: String,
    port: u16,
    timeout: Duration,
}

impl RspamdClient {
    /// Parses `http://host:port` (plaintext only — the controller is a
    /// local/container endpoint behind our network boundary).
    ///
    /// # Errors
    /// A human-readable message when the URL is not a bare
    /// `http://host:port`.
    pub fn from_url(url: &str, timeout: Duration) -> Result<Self, String> {
        let rest = url
            .strip_prefix("http://")
            .ok_or_else(|| format!("rspamd url must start with http:// (got {url})"))?;
        let rest = rest.trim_end_matches('/');
        let (host, port) = rest
            .rsplit_once(':')
            .ok_or_else(|| format!("rspamd url must be http://host:port (got {url})"))?;
        if host.is_empty() {
            return Err(format!("rspamd url has no host (got {url})"));
        }
        let port: u16 = port
            .parse()
            .map_err(|_| format!("rspamd url port is not a number (got {url})"))?;
        Ok(Self {
            host: host.to_owned(),
            port,
            timeout,
        })
    }

    /// Consults Rspamd for `raw_message`. Applies the configured timeout
    /// to the whole exchange.
    ///
    /// # Errors
    /// [`RspamdError`] on any transport/parse failure — the caller
    /// fail-closes (451).
    pub async fn check(
        &self,
        meta: &RspamdMeta<'_>,
        raw_message: &[u8],
    ) -> Result<RspamdVerdict, RspamdError> {
        let fut = self.check_inner(meta, raw_message);
        match tokio::time::timeout(self.timeout, fut).await {
            Ok(result) => result,
            Err(_) => Err(RspamdError::Timeout),
        }
    }

    async fn check_inner(
        &self,
        meta: &RspamdMeta<'_>,
        raw_message: &[u8],
    ) -> Result<RspamdVerdict, RspamdError> {
        let mut stream = TcpStream::connect((self.host.as_str(), self.port))
            .await
            .map_err(|e| RspamdError::Connect(e.to_string()))?;

        let head = self.request_head(meta, raw_message.len());
        stream
            .write_all(head.as_bytes())
            .await
            .map_err(|e| RspamdError::Connect(e.to_string()))?;
        stream
            .write_all(raw_message)
            .await
            .map_err(|e| RspamdError::Connect(e.to_string()))?;
        stream
            .flush()
            .await
            .map_err(|e| RspamdError::Connect(e.to_string()))?;

        // `Connection: close` → read to EOF (bounded).
        let mut response = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = stream
                .read(&mut buf)
                .await
                .map_err(|e| RspamdError::Connect(e.to_string()))?;
            if n == 0 {
                break;
            }
            if response.len() + n > MAX_RESPONSE {
                return Err(RspamdError::Parse("response exceeds cap".to_owned()));
            }
            response.extend_from_slice(&buf[..n]);
        }
        parse_response(&response)
    }

    /// Builds the request head (request line + headers + blank line).
    /// Attacker-controlled metadata is CR/LF-stripped so it cannot inject
    /// extra HTTP headers into the scanner request.
    fn request_head(&self, meta: &RspamdMeta<'_>, body_len: usize) -> String {
        let mut head = String::new();
        head.push_str("POST /checkv2 HTTP/1.1\r\n");
        head.push_str(&format!("Host: {}:{}\r\n", self.host, self.port));
        head.push_str("Connection: close\r\n");
        head.push_str(&format!("Content-Length: {body_len}\r\n"));
        head.push_str(&format!("IP: {}\r\n", sanitize_header(meta.ip)));
        head.push_str(&format!("Helo: {}\r\n", sanitize_header(meta.helo)));
        if let Some(from) = meta.mail_from {
            head.push_str(&format!("From: {}\r\n", sanitize_header(from)));
        }
        for rcpt in meta.recipients {
            head.push_str(&format!("Rcpt: {}\r\n", sanitize_header(rcpt)));
        }
        head.push_str(&format!("MTA-Name: {}\r\n", sanitize_header(meta.mta_name)));
        head.push_str("\r\n");
        head
    }
}

/// Strips control characters (including CR/LF) from an HTTP header value.
fn sanitize_header(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(998)
        .collect()
}

/// The subset of Rspamd's `/checkv2` JSON we consume.
#[derive(Debug, Deserialize)]
struct RspamdResponse {
    action: String,
    #[serde(default)]
    score: f64,
}

/// Splits an HTTP/1.x response, checks the status, and parses the JSON
/// body into a verdict.
fn parse_response(bytes: &[u8]) -> Result<RspamdVerdict, RspamdError> {
    let sep = find_double_crlf(bytes)
        .ok_or_else(|| RspamdError::Parse("no header/body separator".to_owned()))?;
    let head = &bytes[..sep];
    let body = &bytes[sep + 4..];

    // Status line: `HTTP/1.1 200 OK`.
    let status_line = head
        .split(|&b| b == b'\r' || b == b'\n')
        .next()
        .unwrap_or(head);
    let status_line = std::str::from_utf8(status_line)
        .map_err(|_| RspamdError::Parse("status line not UTF-8".to_owned()))?;
    let code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| RspamdError::Parse("no status code".to_owned()))?;
    if code != 200 {
        return Err(RspamdError::BadStatus(code));
    }

    let parsed: RspamdResponse =
        serde_json::from_slice(body).map_err(|e| RspamdError::Parse(e.to_string()))?;
    let action = RspamdAction::parse(&parsed.action)
        .ok_or_else(|| RspamdError::Parse(format!("unknown action {:?}", parsed.action)))?;
    Ok(RspamdVerdict {
        action,
        score: parsed.score,
    })
}

fn find_double_crlf(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|w| w == b"\r\n\r\n")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn from_url_parses_and_rejects() {
        let c = RspamdClient::from_url("http://127.0.0.1:11333", Duration::from_secs(5)).unwrap();
        assert_eq!(c.host, "127.0.0.1");
        assert_eq!(c.port, 11333);
        // trailing slash tolerated
        assert!(RspamdClient::from_url("http://host:80/", Duration::from_secs(5)).is_ok());
        assert!(RspamdClient::from_url("https://host:443", Duration::from_secs(5)).is_err());
        assert!(RspamdClient::from_url("host:11333", Duration::from_secs(5)).is_err());
        assert!(RspamdClient::from_url("http://host", Duration::from_secs(5)).is_err());
    }

    #[test]
    fn action_and_spam_token() {
        assert_eq!(
            RspamdAction::parse("no action"),
            Some(RspamdAction::NoAction)
        );
        assert_eq!(
            RspamdAction::parse("SOFT REJECT"),
            Some(RspamdAction::SoftReject)
        );
        assert_eq!(RspamdAction::parse("reject"), Some(RspamdAction::Reject));
        assert_eq!(RspamdAction::parse("nonsense"), None);
        assert_eq!(RspamdAction::NoAction.spam_token(), "no");
        assert_eq!(RspamdAction::AddHeader.spam_token(), "yes");
        assert_eq!(RspamdAction::Reject.spam_token(), "yes");
    }

    #[test]
    fn parses_a_checkv2_response() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n\
            {\"action\":\"reject\",\"score\":15.0,\"required_score\":12.0}";
        let v = parse_response(raw).unwrap();
        assert_eq!(v.action, RspamdAction::Reject);
        assert!((v.score - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn non_200_and_bad_json_are_errors() {
        let bad_status = b"HTTP/1.1 500 Internal Server Error\r\n\r\n{}";
        assert!(matches!(
            parse_response(bad_status),
            Err(RspamdError::BadStatus(500))
        ));
        let bad_json = b"HTTP/1.1 200 OK\r\n\r\nnot json";
        assert!(matches!(
            parse_response(bad_json),
            Err(RspamdError::Parse(_))
        ));
        let unknown = b"HTTP/1.1 200 OK\r\n\r\n{\"action\":\"detonate\"}";
        assert!(matches!(
            parse_response(unknown),
            Err(RspamdError::Parse(_))
        ));
    }

    #[test]
    fn sanitize_strips_crlf_injection() {
        assert_eq!(sanitize_header("evil\r\nInjected: 1"), "evilInjected: 1");
    }

    #[tokio::test]
    async fn check_talks_to_a_loopback_endpoint() {
        // A canned Rspamd stand-in: reads the request, returns a verdict.
        let (addr, server) =
            crate::canned_http::serve_once(b"{\"action\":\"add header\",\"score\":6.7}").await;

        let client =
            RspamdClient::from_url(&format!("http://{addr}"), Duration::from_secs(5)).unwrap();
        let meta = RspamdMeta {
            ip: "192.0.2.1",
            helo: "client.example",
            mail_from: Some("spammer@evil.example"),
            recipients: &["bob@example.com".to_owned()],
            mta_name: "mx.alo.test",
        };
        let verdict = client
            .check(&meta, b"From: a@b\r\n\r\nhi\r\n")
            .await
            .unwrap();
        assert_eq!(verdict.action, RspamdAction::AddHeader);
        assert!((verdict.score - 6.7).abs() < 1e-9);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn unreachable_endpoint_is_connect_error() {
        // Nothing listening on this port → connect error (caller 451s).
        let client =
            RspamdClient::from_url("http://127.0.0.1:1", Duration::from_millis(500)).unwrap();
        let meta = RspamdMeta {
            ip: "192.0.2.1",
            helo: "h",
            mail_from: None,
            recipients: &[],
            mta_name: "mx",
        };
        let err = client.check(&meta, b"x").await.unwrap_err();
        assert!(matches!(
            err,
            RspamdError::Connect(_) | RspamdError::Timeout
        ));
    }
}
