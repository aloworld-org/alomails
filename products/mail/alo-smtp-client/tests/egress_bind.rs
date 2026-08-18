//! The outbound client leaves by the source address it was given (ADR 0044 §1).
//!
//! Proving a source address was *chosen* is harder than it looks: a test that
//! binds a loopback address and sees a loopback peer proves nothing, because the
//! kernel would have picked the same one. So the proof here is the other way
//! round — pin an address this host does **not** hold, and the connection must
//! fail. If the bind were quietly ignored, the connection would succeed, and
//! that is exactly the failure mode that matters: mail leaving by the
//! transactional address under a campaign identity whose SPF record ends in
//! `-all`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use alo_smtp_client::client::{DeliveryError, OutboundSession, TlsRequirement};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

/// An address from TEST-NET-1 (RFC 5737), which no host holds: binding it must
/// fail rather than silently fall back to whatever the kernel would pick.
const NOT_OURS: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));

/// A mock that greets, answers EHLO, and accepts a QUIT — enough to prove the
/// connection was made. Serves `connections` callers, then stops.
async fn plain_mock(connections: usize) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        for _ in 0..connections {
            let Ok((stream, _peer)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut reader = BufReader::new(stream);
                reader
                    .get_mut()
                    .write_all(b"220 plain.local ESMTP\r\n")
                    .await
                    .unwrap();
                let mut line = String::new();
                while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                    let reply: &[u8] = if line.to_ascii_uppercase().starts_with("QUIT") {
                        b"221 2.0.0 bye\r\n"
                    } else {
                        b"250 plain.local\r\n"
                    };
                    reader.get_mut().write_all(reply).await.unwrap();
                    line.clear();
                }
            });
        }
    });
    addr
}

#[tokio::test]
async fn a_pinned_source_address_is_applied_rather_than_advisory() {
    let addr = plain_mock(2).await;

    // The control: no pinned address, the kernel chooses, and delivery works.
    // Without this the test below would pass just as happily against a mock
    // that was never listening.
    let session = OutboundSession::connect(
        "plain.local",
        &[addr.ip()],
        addr.port(),
        "client.local",
        TlsRequirement::Opportunistic,
        None,
    )
    .await;
    assert!(
        session.is_ok(),
        "the unpinned path must still connect: {:?}",
        session.err()
    );

    // The same destination, pinned to an address this host does not hold. A
    // bind that was ignored would connect exactly like the control above.
    let result = OutboundSession::connect(
        "plain.local",
        &[addr.ip()],
        addr.port(),
        "client.local",
        TlsRequirement::Opportunistic,
        Some(NOT_OURS),
    )
    .await;
    match result {
        Err(DeliveryError::Connect { host, .. }) => assert_eq!(host, "plain.local"),
        Err(other) => panic!("expected a connect failure, got {other:?}"),
        Ok(_) => panic!(
            "connecting from an address this host does not hold must fail — \
             a pinned egress address that is merely advisory sends campaign mail \
             from the transactional IP"
        ),
    }
}

#[tokio::test]
async fn a_source_of_the_wrong_family_fails_the_attempt_rather_than_falling_back() {
    let addr = plain_mock(1).await;
    // An IPv6 source cannot reach an IPv4 destination. The tempting behaviour
    // is to shrug and let the kernel choose; that would deliver the message
    // from an address the identity's SPF record does not authorise.
    let result = OutboundSession::connect(
        "plain.local",
        &[addr.ip()],
        addr.port(),
        "client.local",
        TlsRequirement::Opportunistic,
        Some(IpAddr::V6(Ipv6Addr::LOCALHOST)),
    )
    .await;
    match result {
        Err(DeliveryError::Connect { reason, .. }) => assert!(
            reason.contains("IP family"),
            "the reason must say why, for the operator reading the log: {reason}"
        ),
        Err(other) => panic!("expected a connect failure, got {other:?}"),
        Ok(_) => panic!("a mismatched egress family must not connect"),
    }
}

#[tokio::test]
async fn the_smarthost_path_honours_the_same_pin() {
    let addr = plain_mock(1).await;
    // The smarthost route is the self-hosted mode and the test seam; an egress
    // address that only worked on the MX route would be a hole nobody notices
    // until a deployment uses a relay.
    let result = OutboundSession::connect_addr(addr, "client.local", Some(NOT_OURS)).await;
    assert!(
        matches!(result, Err(DeliveryError::Connect { .. })),
        "the smarthost path must honour a pinned source address too"
    );
}
