//! alo MAPI-over-HTTP: the transport native Outlook speaks to Exchange, served
//! on 443 and translated to the JMAP-native core (ADR 0051, [MS-OXCMAPIHTTP]).
//!
//! **Why this is a separate crate.** A half-built adapter for the largest
//! protocol in the product must not be able to destabilise the mail that
//! already works. Nothing here is reachable unless a deployment sets
//! `ALO_MAPI_HTTP_ENABLED`, and Autodiscover stays silent about MAPI until the
//! same switch is thrown.
//!
//! **What exists today** is the transport envelope and the session handshake:
//! `Connect` establishes a Session Context and `Disconnect` ends it. The
//! request types that carry actual mailbox work — `Execute`, which carries ROP
//! payloads, and `NotificationWait` — are later stages, and are answered
//! honestly as unsupported rather than with a plausible empty success.
//!
//! **What it will never be** is a second message store. MAPI is an edge
//! translator over the one store ([ADR 0001](../../../docs/decisions/0001-jmap-native-core.md));
//! a second source of truth is how this becomes unmaintainable.

pub mod attachments;
pub mod columns;
pub mod compose;
pub mod connect;
pub mod contents;
pub mod direct2;
pub mod dispatch;
pub mod execute;
pub mod fasttransfer;
pub mod folders;
pub mod hierarchy;
pub mod ics;
pub mod logon;
pub mod logon_response;
pub mod messages;
pub mod nspi;
pub mod openfolder;
pub mod openmessage;
pub mod properties;
pub mod release;
pub mod response;
pub mod rop;
pub mod router;
pub mod rows;
pub mod rpc;
pub mod session;
pub mod stream;
pub mod sync;

pub use connect::{ConnectError, ConnectRequest};
pub use response::{MapiResponse, ResponseCode};
pub use router::{MapiState, router};
pub use session::{SessionContext, SessionStore};

/// The request types this protocol defines on the mailbox endpoint
/// ([MS-OXCMAPIHTTP] §2.2.3.3.1).
///
/// Parsed from `X-RequestType`, which is case-insensitive in practice — real
/// clients have been observed varying it, and a case-sensitive match here would
/// reject a correct client for a reason nobody could see from the outside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestType {
    /// Establish a Session Context.
    Connect,
    /// Carry a ROP payload against an established context.
    Execute,
    /// End a Session Context.
    Disconnect,
    /// Hold open for server-initiated notifications (a later stage).
    NotificationWait,

    // ---- the address book endpoint ---------------------------------------
    //
    // These arrive on `/mapi/nspi` and are a different protocol from the three
    // above: their bodies carry no ROP layer at all. Only the envelope is
    // shared.
    /// Open an address book session.
    Bind,
    /// Close one.
    Unbind,
    /// Turn typed strings into recipients.
    ResolveNames,
}

impl RequestType {
    /// Reads an `X-RequestType` header value.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "connect" => Some(Self::Connect),
            "execute" => Some(Self::Execute),
            "disconnect" => Some(Self::Disconnect),
            "notificationwait" => Some(Self::NotificationWait),
            "bind" => Some(Self::Bind),
            "unbind" => Some(Self::Unbind),
            "resolvenames" => Some(Self::ResolveNames),
            _ => None,
        }
    }

    /// The spelling to echo in the response's `X-RequestType`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Connect => "Connect",
            Self::Execute => "Execute",
            Self::Disconnect => "Disconnect",
            Self::NotificationWait => "NotificationWait",
            Self::Bind => "Bind",
            Self::Unbind => "Unbind",
            Self::ResolveNames => "ResolveNames",
        }
    }

    /// Whether this stage of the adapter serves this request type.
    ///
    /// Kept explicit rather than implied by a `match` arm elsewhere: as stages
    /// land, this is the single line that opens each one, and the test beside
    /// it states exactly which are open today.
    #[must_use]
    pub const fn is_implemented(self) -> bool {
        matches!(
            self,
            Self::Connect
                | Self::Disconnect
                | Self::Execute
                | Self::Bind
                | Self::Unbind
                | Self::ResolveNames
        )
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn request_types_are_read_whatever_their_casing() {
        assert_eq!(RequestType::parse("Connect"), Some(RequestType::Connect));
        assert_eq!(RequestType::parse("connect"), Some(RequestType::Connect));
        assert_eq!(RequestType::parse("  CONNECT "), Some(RequestType::Connect));
        assert_eq!(
            RequestType::parse("NotificationWait"),
            Some(RequestType::NotificationWait)
        );
        assert_eq!(
            RequestType::parse("ResolveNames"),
            Some(RequestType::ResolveNames)
        );
        assert_eq!(RequestType::parse("bind"), Some(RequestType::Bind));
        assert_eq!(RequestType::parse("nonsense"), None);
        assert_eq!(RequestType::parse(""), None);
    }

    /// The response echoes the specification's spelling, not the client's, so a
    /// lowercased request does not produce a lowercased answer.
    #[test]
    fn the_response_echoes_the_canonical_spelling() {
        assert_eq!(RequestType::Connect.as_str(), "Connect");
        assert_eq!(RequestType::NotificationWait.as_str(), "NotificationWait");
    }

    /// States plainly which stage we are at. When `Execute` lands this test
    /// changes in the same commit that implements it — that is the point.
    #[test]
    fn the_request_types_served_today_are_stated_out_loud() {
        assert!(RequestType::Connect.is_implemented());
        assert!(RequestType::Disconnect.is_implemented());
        assert!(RequestType::Execute.is_implemented());
        assert!(RequestType::Bind.is_implemented());
        assert!(RequestType::Unbind.is_implemented());
        assert!(RequestType::ResolveNames.is_implemented());
        assert!(!RequestType::NotificationWait.is_implemented());
    }
}
