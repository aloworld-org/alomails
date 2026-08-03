//! ClamAV malware scanning at DATA time — the clamd `INSTREAM`
//! protocol over TCP, mirroring the Rspamd integration's shape: the
//! pinned engine runs as a container behind our API; a `FOUND` verdict
//! rejects the message (550), and a scanner outage **fails closed**
//! (451 defer) — an unscanned message is never spooled while scanning
//! is configured.
//!
//! Protocol (clamd(8)): send `zINSTREAM\0`, then the message as
//! `<u32 big-endian length><bytes>` chunks, terminated by a zero-length
//! chunk; clamd answers one NUL-terminated line — `stream: OK` or
//! `stream: <Signature> FOUND`. Messages beyond [`MAX_SCAN_BYTES`] pass
//! unscanned with a log line (every scanning engine caps stream size;
//! clamd's own default cap is in the same range and oversized streams
//! would error the session instead).

use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Messages larger than this are not streamed to clamd (see module
/// docs) — they are accepted as unscanned, loudly logged.
pub const MAX_SCAN_BYTES: usize = 20 * 1024 * 1024;

/// INSTREAM chunk size. clamd accepts arbitrary chunking; 64 KiB keeps
/// syscalls low without large buffers.
const CHUNK: usize = 64 * 1024;

/// The scan verdict for one message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClamVerdict {
    /// No signature matched.
    Clean,
    /// A signature matched; carries the (sanitized) signature name for
    /// the reply/log.
    Infected(String),
}

/// A clamd TCP client. Cheap to clone-by-config; each scan opens a
/// fresh connection (clamd sessions are per-command here, and a fresh
/// connection sidesteps half-closed pools on clamd reloads).
#[derive(Debug)]
pub struct ClamavClient {
    addr: SocketAddr,
    /// Display form for logs (host:port as configured).
    label: String,
    timeout: Duration,
}

impl ClamavClient {
    /// Builds a client for `addr` (`host:port`). The host is resolved
    /// at scan time via tokio's resolver, so a container restart with
    /// a new IP needs no reconfiguration — `addr` here is validated
    /// only for shape.
    ///
    /// # Errors
    /// A human-readable message when `addr` is not `host:port`.
    pub fn from_addr(addr: &str, timeout: Duration) -> Result<Self, String> {
        let label = addr.trim().to_owned();
        let (host, port) = label
            .rsplit_once(':')
            .ok_or_else(|| format!("{label}: expected host:port"))?;
        if host.is_empty() {
            return Err(format!("{label}: empty host"));
        }
        let port: u16 = port.parse().map_err(|_| format!("{label}: invalid port"))?;
        // Pre-resolve numeric forms now; names resolve per scan.
        let addr = match host.parse::<std::net::IpAddr>() {
            Ok(ip) => SocketAddr::new(ip, port),
            // A DNS name: keep a placeholder; `scan` resolves the label.
            Err(_) => SocketAddr::new(std::net::IpAddr::from([0, 0, 0, 0]), port),
        };
        Ok(Self {
            addr,
            label,
            timeout,
        })
    }

    /// Streams `raw_message` to clamd and returns the verdict.
    ///
    /// # Errors
    /// A human-readable transport/protocol error — the caller treats
    /// any error as "scanner unavailable" and fails closed.
    pub async fn scan(&self, raw_message: &[u8]) -> Result<ClamVerdict, String> {
        if raw_message.len() > MAX_SCAN_BYTES {
            tracing::warn!(
                size = raw_message.len(),
                cap = MAX_SCAN_BYTES,
                "message exceeds the malware-scan cap; accepting unscanned"
            );
            return Ok(ClamVerdict::Clean);
        }
        tokio::time::timeout(self.timeout, self.scan_inner(raw_message))
            .await
            .map_err(|_elapsed| format!("clamd {}: scan timed out", self.label))?
    }

    async fn scan_inner(&self, raw_message: &[u8]) -> Result<ClamVerdict, String> {
        let mut stream = self.connect().await?;
        stream
            .write_all(b"zINSTREAM\0")
            .await
            .map_err(|e| format!("clamd {}: write failed: {e}", self.label))?;
        for chunk in raw_message.chunks(CHUNK) {
            let len = (chunk.len() as u32).to_be_bytes();
            stream
                .write_all(&len)
                .await
                .map_err(|e| format!("clamd {}: write failed: {e}", self.label))?;
            stream
                .write_all(chunk)
                .await
                .map_err(|e| format!("clamd {}: write failed: {e}", self.label))?;
        }
        stream
            .write_all(&0u32.to_be_bytes())
            .await
            .map_err(|e| format!("clamd {}: write failed: {e}", self.label))?;
        stream
            .flush()
            .await
            .map_err(|e| format!("clamd {}: flush failed: {e}", self.label))?;

        // One NUL-terminated reply line, bounded (a verdict is short).
        let mut reply = Vec::with_capacity(128);
        let mut byte = [0u8; 1];
        loop {
            let n = stream
                .read(&mut byte)
                .await
                .map_err(|e| format!("clamd {}: read failed: {e}", self.label))?;
            if n == 0 || byte[0] == 0 {
                break;
            }
            reply.push(byte[0]);
            if reply.len() > 512 {
                return Err(format!("clamd {}: oversized reply", self.label));
            }
        }
        parse_verdict(&String::from_utf8_lossy(&reply)).ok_or_else(|| {
            format!(
                "clamd {}: unrecognised reply: {}",
                self.label,
                String::from_utf8_lossy(&reply)
            )
        })
    }

    async fn connect(&self) -> Result<TcpStream, String> {
        // Numeric address: connect directly. Name: resolve fresh.
        if !self.addr.ip().is_unspecified() {
            return TcpStream::connect(self.addr)
                .await
                .map_err(|e| format!("clamd {}: connect failed: {e}", self.label));
        }
        TcpStream::connect(&self.label)
            .await
            .map_err(|e| format!("clamd {}: connect failed: {e}", self.label))
    }
}

/// Parses clamd's verdict line: `stream: OK`, `stream: <Name> FOUND`,
/// or an `ERROR` (→ `None`, treated as scanner failure). The signature
/// name is sanitized to the characters signatures actually use before
/// it can reach a reply or log line.
fn parse_verdict(line: &str) -> Option<ClamVerdict> {
    let line = line.trim();
    let rest = line.strip_prefix("stream:")?.trim();
    if rest == "OK" {
        return Some(ClamVerdict::Clean);
    }
    if let Some(name) = rest.strip_suffix("FOUND") {
        let sanitized: String = name
            .trim()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':'))
            .take(120)
            .collect();
        return Some(ClamVerdict::Infected(sanitized));
    }
    None // "ERROR" or anything else → scanner failure, fail closed
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    /// A mock clamd: reads the INSTREAM exchange, replies `verdict`.
    async fn mock_clamd(verdict: &'static [u8]) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            // Command: zINSTREAM\0
            let mut cmd = [0u8; 10];
            sock.read_exact(&mut cmd).await.unwrap();
            assert_eq!(&cmd, b"zINSTREAM\0");
            // Chunks until the zero-length terminator.
            let mut body = Vec::new();
            loop {
                let mut len = [0u8; 4];
                sock.read_exact(&mut len).await.unwrap();
                let len = u32::from_be_bytes(len) as usize;
                if len == 0 {
                    break;
                }
                let mut chunk = vec![0u8; len];
                sock.read_exact(&mut chunk).await.unwrap();
                body.extend_from_slice(&chunk);
            }
            assert!(!body.is_empty(), "message bytes must be streamed");
            sock.write_all(verdict).await.unwrap();
        });
        addr
    }

    fn client(addr: SocketAddr) -> ClamavClient {
        ClamavClient::from_addr(&addr.to_string(), Duration::from_secs(5)).unwrap()
    }

    #[tokio::test]
    async fn clean_stream_is_clean() {
        let addr = mock_clamd(b"stream: OK\0").await;
        let verdict = client(addr)
            .scan(b"From: a@b\r\n\r\nhello\r\n")
            .await
            .unwrap();
        assert_eq!(verdict, ClamVerdict::Clean);
    }

    #[tokio::test]
    async fn found_maps_to_infected_with_sanitized_name() {
        let addr = mock_clamd(b"stream: Eicar-Signature FOUND\0").await;
        let verdict = client(addr).scan(b"From: a@b\r\n\r\nx\r\n").await.unwrap();
        assert_eq!(verdict, ClamVerdict::Infected("Eicar-Signature".into()));
    }

    #[tokio::test]
    async fn error_reply_is_a_scanner_failure() {
        let addr = mock_clamd(b"INSTREAM size limit exceeded. ERROR\0").await;
        let err = client(addr)
            .scan(b"From: a@b\r\n\r\nx\r\n")
            .await
            .unwrap_err();
        assert!(err.contains("unrecognised reply"), "{err}");
    }

    #[tokio::test]
    async fn unreachable_clamd_is_an_error_not_a_verdict() {
        let c = ClamavClient::from_addr("127.0.0.1:1", Duration::from_millis(400)).unwrap();
        assert!(c.scan(b"x").await.is_err());
    }

    #[tokio::test]
    async fn oversized_message_passes_unscanned() {
        // No listener at all: an oversized message must not even try.
        let c = ClamavClient::from_addr("127.0.0.1:1", Duration::from_millis(400)).unwrap();
        let big = vec![b'a'; MAX_SCAN_BYTES + 1];
        assert_eq!(c.scan(&big).await.unwrap(), ClamVerdict::Clean);
    }

    #[test]
    fn addr_validation() {
        assert!(ClamavClient::from_addr("clamav:3310", Duration::from_secs(1)).is_ok());
        assert!(ClamavClient::from_addr("no-port", Duration::from_secs(1)).is_err());
        assert!(ClamavClient::from_addr(":3310", Duration::from_secs(1)).is_err());
        assert!(ClamavClient::from_addr("h:notaport", Duration::from_secs(1)).is_err());
    }

    #[test]
    fn verdict_parsing_sanitizes_names() {
        assert_eq!(parse_verdict("stream: OK"), Some(ClamVerdict::Clean));
        assert_eq!(
            parse_verdict("stream: Win.Test.EICAR_HDB-1 FOUND"),
            Some(ClamVerdict::Infected("Win.Test.EICAR_HDB-1".into()))
        );
        // Structural characters an attacker-influenced name could carry
        // never reach the SMTP reply.
        assert_eq!(
            parse_verdict("stream: Evil\r\n550 injected FOUND"),
            Some(ClamVerdict::Infected("Evil550injected".into()))
        );
        assert_eq!(parse_verdict("stream: whatever ERROR"), None);
        assert_eq!(parse_verdict("garbage"), None);
    }
}
