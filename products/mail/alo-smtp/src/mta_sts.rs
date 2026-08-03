//! MTA-STS policy endpoint — serves the rendered policy at
//! `GET /.well-known/mta-sts.txt` (RFC 8461 §3.2).
//!
//! Plaintext HTTP on a local port; RFC 8461 mandates HTTPS with a
//! WebPKI-valid certificate on `mta-sts.<domain>`, which the deploy
//! TLS-terminating proxy provides (see `docs/interop.md`). The policy
//! bytes come from `alo-auth-mail::mta_sts`, so the served policy and
//! the running server never disagree.

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// The one path this endpoint answers (RFC 8461 §3.2).
const POLICY_PATH: &str = "/.well-known/mta-sts.txt";
/// Cap on the request bytes read before routing (requests are tiny; a
/// client cannot make us buffer unboundedly).
const MAX_REQUEST: usize = 8192;
/// A whole request must arrive within this window; a peer that opens a
/// socket and dribbles (or sends nothing) is dropped rather than pinning
/// a task (slow-loris guard).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Spawns the MTA-STS responder on an already-bound listener. `policy`
/// is the pre-rendered policy document (RFC 8461 §3.2).
pub fn spawn(listener: TcpListener, policy: Arc<str>) {
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _peer)) => {
                    let policy = Arc::clone(&policy);
                    tokio::spawn(async move {
                        if let Err(error) = handle(stream, &policy).await {
                            tracing::debug!(%error, "mta-sts request errored");
                        }
                    });
                }
                Err(error) => {
                    tracing::warn!(%error, "mta-sts accept failed");
                }
            }
        }
    });
}

/// Reads one request, routes it, and writes the response. The read is
/// bounded by [`REQUEST_TIMEOUT`]; a stalled peer is dropped.
async fn handle(mut stream: TcpStream, policy: &str) -> std::io::Result<()> {
    let request_line =
        match tokio::time::timeout(REQUEST_TIMEOUT, read_request_line(&mut stream)).await {
            Ok(result) => result?,
            Err(_) => return Ok(()), // timed out; drop the connection
        };
    let response = build_response(&request_line, policy);
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await
}

/// Reads until the request line (first CRLF) or the request cap.
async fn read_request_line(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(pos) = buf.windows(2).position(|w| w == b"\r\n") {
            buf.truncate(pos);
            break;
        }
        if buf.len() >= MAX_REQUEST {
            break;
        }
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Builds the full HTTP response for a request line (`METHOD PATH
/// VERSION`). Pure and testable: the served policy is static, request
/// input is never reflected into the body.
fn build_response(request_line: &str, policy: &str) -> String {
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");
    // Strip a query string if present.
    let path = path.split('?').next().unwrap_or(path);

    if method != "GET" {
        return http_response(
            405,
            "Method Not Allowed",
            "text/plain",
            "method not allowed\n",
        );
    }
    if path != POLICY_PATH {
        return http_response(404, "Not Found", "text/plain", "not found\n");
    }
    http_response(200, "OK", "text/plain; charset=utf-8", policy)
}

fn http_response(code: u16, reason: &str, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {code} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        body.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLICY: &str =
        "version: STSv1\r\nmode: enforce\r\nmx: mx.example.com\r\nmax_age: 604800\r\n";

    #[test]
    fn serves_policy_on_the_well_known_path() {
        let resp = build_response("GET /.well-known/mta-sts.txt HTTP/1.1", POLICY);
        assert!(resp.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(resp.contains("Content-Type: text/plain; charset=utf-8\r\n"));
        assert!(resp.ends_with(POLICY));
        assert!(resp.contains(&format!("Content-Length: {}\r\n", POLICY.len())));
    }

    #[test]
    fn tolerates_a_query_string() {
        let resp = build_response("GET /.well-known/mta-sts.txt?x=1 HTTP/1.1", POLICY);
        assert!(resp.starts_with("HTTP/1.1 200 OK"));
    }

    #[test]
    fn other_paths_are_404() {
        let resp = build_response("GET /secret HTTP/1.1", POLICY);
        assert!(resp.starts_with("HTTP/1.1 404 Not Found"));
        assert!(!resp.contains("STSv1"));
    }

    #[test]
    fn non_get_is_405() {
        let resp = build_response("POST /.well-known/mta-sts.txt HTTP/1.1", POLICY);
        assert!(resp.starts_with("HTTP/1.1 405 Method Not Allowed"));
    }

    #[test]
    fn garbage_request_is_404_not_panic() {
        let resp = build_response("", POLICY);
        assert!(resp.starts_with("HTTP/1.1 405"));
    }
}
