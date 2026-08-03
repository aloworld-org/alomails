//! `Received:` trace header construction, RFC 5321 §4.4.
//!
//! Every server that accepts a message MUST stamp one, and MUST NOT
//! alter earlier trace headers. Time is a parameter so tests are
//! deterministic.

use jiff::Zoned;

/// Builds the `Received:` header line (CRLF-terminated, folded with
/// tabs per RFC 5322 §2.2.3) to prepend to the message content.
/// `protocol` is the RFC 3848 WITH value: `ESMTP` after EHLO, `SMTP`
/// after HELO.
pub fn stamp(
    helo: &str,
    peer_ip: &str,
    hostname: &str,
    protocol: &str,
    id: &str,
    when: &Zoned,
) -> String {
    // RFC 5322 §3.3 date-time via RFC 2822 formatting.
    let date = jiff::fmt::rfc2822::to_string(when).unwrap_or_else(|_| when.to_string());
    format!(
        "Received: from {helo} ([{peer_ip}])\r\n\tby {hostname} with {protocol} id {id};\r\n\t{date}\r\n"
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn header_has_required_clauses_and_crlf_folding() {
        let when: Zoned = "2026-07-25T10:30:00+00:00[UTC]".parse().unwrap();
        let header = stamp(
            "client.example",
            "192.0.2.9",
            "mx.alo.test",
            "ESMTP",
            "1234.5.6",
            &when,
        );
        assert!(header.starts_with("Received: from client.example ([192.0.2.9])"));
        assert!(header.contains("by mx.alo.test with ESMTP id 1234.5.6;"));
        // RFC 3848: HELO sessions are stamped SMTP, not ESMTP.
        let helo_header = stamp(
            "c.example",
            "192.0.2.9",
            "mx.alo.test",
            "SMTP",
            "1.2.3",
            &when,
        );
        assert!(helo_header.contains("with SMTP id"));
        assert!(header.contains("Sat, 25 Jul 2026"));
        assert!(header.ends_with("\r\n"));
        // Folded continuation lines must start with whitespace.
        for continuation in header.split("\r\n").skip(1).filter(|l| !l.is_empty()) {
            assert!(continuation.starts_with('\t'));
        }
    }
}
