//! The SMTP envelope: what the transaction said, as opposed to what
//! the message content says (RFC 5321 §2.3.1 buffer/envelope split).
//!
//! Serialized as the spool's JSON sidecar — this is the contract the
//! queue (M2) reads back and the store migration (M5) consumes.
//! Evolve it additively only.

use serde::{Deserialize, Serialize};

/// One accepted mail transaction's envelope plus trace metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    /// HELO/EHLO argument the client identified with.
    pub helo: String,
    /// Peer socket address the message arrived from.
    pub peer: String,
    /// `MAIL FROM` path in display form; `None` is the null path `<>`
    /// (bounce) — never conflate with an empty string.
    pub mail_from: Option<String>,
    /// Accepted `RCPT TO` paths in display form, in order.
    pub rcpt_to: Vec<String>,
    /// When the message was accepted, RFC 3339 (UTC).
    pub received_at: String,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn round_trips_through_json() {
        let envelope = Envelope {
            helo: "client.example".to_owned(),
            peer: "192.0.2.9:52061".to_owned(),
            mail_from: None,
            rcpt_to: vec!["alice@example.com".to_owned()],
            received_at: "2026-07-25T12:00:00Z".to_owned(),
        };
        let json = serde_json::to_string(&envelope).unwrap();
        let back: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, envelope);
        // The null sender must stay distinguishable after the trip.
        assert!(back.mail_from.is_none());
    }
}
