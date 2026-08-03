//! STARTTLS on the cleartext (143-style) listener: LOGIN is refused before
//! TLS (no credentials in the clear), STARTTLS upgrades the connection, and
//! LOGIN then succeeds over the encrypted channel.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use common::*;
use rustls::pki_types::ServerName;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

/// Reads one CRLF line from a plaintext stream, byte at a time (safe: no
/// read-ahead past the line, so the later TLS handshake bytes are intact).
async fn plain_line(tcp: &mut TcpStream) -> String {
    let mut out = Vec::new();
    loop {
        let b = tcp.read_u8().await.unwrap();
        if b == b'\n' {
            break;
        }
        if b != b'\r' {
            out.push(b);
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[tokio::test]
async fn starttls_gates_login() {
    let store = test_store().await;
    let (_t, _u, email, pw) = make_user(&store, "starttls").await;
    let addr = spawn_imap_starttls(store.clone()).await;

    let mut tcp = TcpStream::connect(addr).await.unwrap();
    let greeting = plain_line(&mut tcp).await;
    assert!(greeting.starts_with("* OK"), "{greeting}");
    // The greeting advertises STARTTLS and LOGINDISABLED before TLS.
    assert!(greeting.contains("STARTTLS"), "{greeting}");
    assert!(greeting.contains("LOGINDISABLED"), "{greeting}");

    // LOGIN before STARTTLS is refused.
    tcp.write_all(format!("a LOGIN \"{email}\" \"{pw}\"\r\n").as_bytes())
        .await
        .unwrap();
    let no = plain_line(&mut tcp).await;
    assert!(no.starts_with("a NO"), "LOGIN allowed before TLS: {no}");

    // STARTTLS → OK, then upgrade.
    tcp.write_all(b"b STARTTLS\r\n").await.unwrap();
    let ok = plain_line(&mut tcp).await;
    assert!(ok.starts_with("b OK"), "{ok}");

    let connector = TlsConnector::from(Arc::new(danger::client_config()));
    let name = ServerName::try_from("localhost").unwrap();
    let tls = connector.connect(name, tcp).await.unwrap();

    // Over TLS, LOGIN now succeeds.
    let mut c = Client::attach(tls);
    c.write(format!("c LOGIN \"{email}\" \"{pw}\"\r\n").as_bytes())
        .await;
    let resp = c.read_until_tag("c").await;
    assert_ok(&resp);
    // And CAPABILITY no longer advertises LOGINDISABLED.
    c.write(b"d CAPABILITY\r\n").await;
    let caps = c.read_until_tag("d").await;
    assert!(!caps.join("\n").contains("LOGINDISABLED"), "{caps:?}");
    assert!(caps.join("\n").contains("AUTH=PLAIN"));
}
