//! A canned HTTP responder for tests: a one-shot loopback server that
//! reads a whole request and answers with a fixed body.
//!
//! It exists so the Rspamd-facing tests do not each hand-roll a stand-in
//! server. The subtlety it encapsulates is the drain: a socket closed
//! while unread request bytes are still queued makes the kernel send RST
//! instead of FIN (BSD/macOS in particular), and the client then observes
//! "connection reset by peer" instead of the response it was sent. So the
//! request is consumed in full — headers, then `Content-Length` bytes of
//! body — before anything is written back.
//!
//! Test-only support code, so it carries the same lint allowance as the
//! `#[cfg(test)]` modules that call it.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// Binds an ephemeral loopback port and serves exactly one request with
/// `body` as a `200 OK` JSON response, returning the address to point a
/// client at and the server task to await.
///
/// # Panics
/// If the loopback port cannot be bound, or the connection fails while
/// the response is being written — both are test-environment faults.
pub async fn serve_once(body: &'static [u8]) -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let addr = listener.local_addr().expect("loopback addr");
    let task = tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.expect("accept");
        drain_request(&mut sock).await;
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        sock.write_all(head.as_bytes()).await.expect("write head");
        sock.write_all(body).await.expect("write body");
        sock.flush().await.expect("flush");
    });
    (addr, task)
}

/// Reads one complete HTTP request: the head, then as many body bytes as
/// `Content-Length` announces. Stops early on EOF or a read error — the
/// caller still answers, which is what the tests assert on.
async fn drain_request(sock: &mut tokio::net::TcpStream) {
    let mut request = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let Ok(read) = sock.read(&mut buf).await else {
            return;
        };
        if read == 0 {
            return;
        }
        request.extend_from_slice(&buf[..read]);
        let Some(head_end) = request
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map(|at| at + 4)
        else {
            continue;
        };
        if request.len() >= head_end + content_length(&request[..head_end]) {
            return;
        }
    }
}

/// The `Content-Length` of a request head, or zero when absent or
/// unparsable (a bodyless request drains as soon as the head is in).
fn content_length(head: &[u8]) -> usize {
    String::from_utf8_lossy(head)
        .to_ascii_lowercase()
        .split("content-length:")
        .nth(1)
        .and_then(|rest| rest.split("\r\n").next())
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(0)
}
