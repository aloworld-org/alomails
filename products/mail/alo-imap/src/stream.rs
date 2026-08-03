//! A connection that is plaintext or TLS, so one session loop serves
//! implicit-TLS ports (993/995) and the STARTTLS port (143) without
//! duplication. Mirrors `alo-smtp`'s `stream.rs` (the shared-transport
//! question is answered in the design note). No `unsafe`: both variants
//! are `Unpin`, so we delegate with plain `Pin::new`.

use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;

/// A connection that is either plaintext TCP or TLS over TCP.
pub enum ImapStream {
    /// Plaintext (143 before STARTTLS).
    Plain(TcpStream),
    /// TLS (implicit on 993/995, or after STARTTLS on 143).
    Tls(Box<TlsStream<TcpStream>>),
    /// A spent placeholder used only to move the real stream out during
    /// the STARTTLS swap (`std::mem::replace` needs a value). Never used
    /// for real I/O.
    Closed,
}

impl ImapStream {
    /// Whether the connection is currently encrypted.
    pub fn is_tls(&self) -> bool {
        matches!(self, Self::Tls(_))
    }
}

impl AsyncRead for ImapStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Self::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
            Self::Closed => Poll::Ready(Ok(())),
        }
    }
}

impl AsyncWrite for ImapStream {
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
