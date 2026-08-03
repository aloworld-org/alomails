//! Connection stream that can be plaintext or TLS, so a single
//! connection handler serves port 25 (plain, later STARTTLS), 587
//! (STARTTLS), and 465 (implicit TLS) without duplicating the session
//! loop.
//!
//! Both variants are `Unpin`, so the enum delegates `AsyncRead`/
//! `AsyncWrite` with plain `Pin::new` on the inner stream — no
//! pin-projection needed.

use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;

/// A connection that is either plaintext TCP or TLS over TCP.
pub enum SmtpStream {
    /// Plaintext (port 25, or 587 before STARTTLS).
    Plain(TcpStream),
    /// TLS (after STARTTLS, or implicit TLS on 465).
    Tls(Box<TlsStream<TcpStream>>),
    /// A spent placeholder, used only to move the real stream out of a
    /// `BufReader` during the STARTTLS swap (`std::mem::replace` needs
    /// a value). Reads report EOF and writes fail; it is never used
    /// for real I/O because the swap consumes the original immediately.
    Closed,
}

impl SmtpStream {
    /// Whether the connection is currently encrypted.
    pub fn is_tls(&self) -> bool {
        matches!(self, Self::Tls(_))
    }
}

impl AsyncRead for SmtpStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Self::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
            // EOF: the placeholder is never read in practice.
            Self::Closed => Poll::Ready(Ok(())),
        }
    }
}

impl AsyncWrite for SmtpStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Self::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
            Self::Closed => Poll::Ready(Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_flush(cx),
            Self::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
            Self::Closed => Poll::Ready(Ok(())),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Self::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
            Self::Closed => Poll::Ready(Ok(())),
        }
    }
}
