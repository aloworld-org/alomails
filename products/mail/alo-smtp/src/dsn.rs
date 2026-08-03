//! Delivery Status Notification (bounce) composition — the basics of
//! RFC 3464 (multipart/report), RFC 3463 (status codes), RFC 3834
//! (Auto-Submitted).
//!
//! DSNs are sent from the null reverse-path and are never generated
//! for messages that themselves arrived with a null reverse-path
//! (RFC 5321 §4.5.5 — the loop-prevention MUST).

use jiff::Zoned;

/// One permanently failed recipient, for the report.
#[derive(Debug, Clone)]
pub struct FailedRecipient {
    /// The recipient address as attempted.
    pub recipient: String,
    /// RFC 3463 status (e.g. `5.1.1`); mapped from the reply when the
    /// remote included one, else the bare class (`5.0.0`/`4.0.0`).
    pub status: String,
    /// Diagnostic line: `smtp; 550 no such user` or a local reason.
    pub diagnostic: String,
}

impl FailedRecipient {
    /// Builds the report entry from a remote SMTP reply.
    pub fn from_reply(recipient: &str, code: u16, first_line: &str) -> Self {
        Self {
            recipient: recipient.to_owned(),
            status: extract_status(code, first_line),
            diagnostic: format!("smtp; {code} {first_line}"),
        }
    }
}

/// Composes the full DSN message (headers + multipart/report body,
/// CRLF line endings) ready for spooling.
///
/// `original_sender` is the failed message's MAIL FROM (the DSN's
/// recipient); `original_headers` is the header block of the failed
/// message (everything up to the first empty line), returned per
/// RFC 3464 §2 as `text/rfc822-headers`.
pub fn compose(
    hostname: &str,
    original_sender: &str,
    original_id: &str,
    failed: &[FailedRecipient],
    original_headers: &str,
    when: &Zoned,
) -> String {
    let date = jiff::fmt::rfc2822::to_string(when).unwrap_or_else(|_| when.to_string());
    // The spool id is unique per message; suffixing keeps the
    // boundary and message-id unique and deterministic (testable).
    let boundary = format!("=_alo_dsn_{original_id}");

    // i18n: this human-readable prose is English-only. Bounce locale
    // is often unknowable (the sender's locale isn't in the envelope),
    // so localized DSN text is deferred (tracked in ROADMAP M2 notes);
    // the machine-readable message/delivery-status part below is what
    // clients actually parse and is locale-independent.
    let mut human = String::new();
    human.push_str("This is the mail system at host ");
    human.push_str(hostname);
    human.push_str(
        ".\r\n\r\nI'm sorry to have to inform you that your message could not\r\nbe delivered to one or more recipients.\r\n\r\n",
    );
    for f in failed {
        human.push_str(&format!("<{}>: {}\r\n", f.recipient, f.diagnostic));
    }

    let mut status_part = format!("Reporting-MTA: dns; {hostname}\r\n\r\n");
    for f in failed {
        status_part.push_str(&format!(
            "Final-Recipient: rfc822; {}\r\nAction: failed\r\nStatus: {}\r\nDiagnostic-Code: {}\r\n\r\n",
            f.recipient, f.status, f.diagnostic
        ));
    }

    format!(
        "Date: {date}\r\n\
         From: Mail Delivery System <MAILER-DAEMON@{hostname}>\r\n\
         To: <{original_sender}>\r\n\
         Subject: Undelivered Mail Returned to Sender\r\n\
         Message-ID: <dsn.{original_id}@{hostname}>\r\n\
         Auto-Submitted: auto-replied\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: multipart/report; report-type=delivery-status;\r\n\tboundary=\"{boundary}\"\r\n\
         \r\n\
         This is a MIME-encapsulated message.\r\n\
         \r\n\
         --{boundary}\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         \r\n\
         {human}\r\n\
         --{boundary}\r\n\
         Content-Type: message/delivery-status\r\n\
         \r\n\
         {status_part}\
         --{boundary}\r\n\
         Content-Type: text/rfc822-headers\r\n\
         \r\n\
         {original_headers}\r\n\
         --{boundary}--\r\n"
    )
}

/// Extracts the leading RFC 3463 enhanced status (`d.d.d`) from reply
/// text when present, else maps the reply class to `X.0.0`.
fn extract_status(code: u16, text: &str) -> String {
    if let Some(token) = text.split_whitespace().next()
        && is_enhanced_status(token)
    {
        return token.to_owned();
    }
    format!("{}.0.0", code / 100)
}

fn is_enhanced_status(token: &str) -> bool {
    let mut parts = token.split('.');
    let class_ok = parts
        .next()
        .is_some_and(|p| p == "2" || p == "4" || p == "5");
    let rest_ok = (0..2).all(|_| {
        parts
            .next()
            .is_some_and(|p| !p.is_empty() && p.len() <= 3 && p.bytes().all(|b| b.is_ascii_digit()))
    });
    class_ok && rest_ok && parts.next().is_none()
}

/// The header block of a message: everything before the first empty
/// line, for the `text/rfc822-headers` part.
pub fn header_block(message: &[u8]) -> String {
    let text = String::from_utf8_lossy(message);
    match text.split_once("\r\n\r\n") {
        Some((headers, _body)) => headers.to_owned(),
        None => text.into_owned(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn when() -> Zoned {
        "2026-07-26T12:00:00+00:00[UTC]".parse().unwrap()
    }

    #[test]
    fn dsn_has_report_structure_and_null_sender_semantics() {
        let failed = vec![FailedRecipient::from_reply(
            "alice@example.com",
            550,
            "5.1.1 no such user",
        )];
        let dsn = compose(
            "mx.alo.test",
            "bob@example.org",
            "123.4.5",
            &failed,
            "Subject: original\r\nFrom: bob@example.org",
            &when(),
        );
        assert!(dsn.contains("From: Mail Delivery System <MAILER-DAEMON@mx.alo.test>"));
        assert!(dsn.contains("To: <bob@example.org>"));
        assert!(dsn.contains("Auto-Submitted: auto-replied"));
        assert!(dsn.contains("Content-Type: multipart/report; report-type=delivery-status;"));
        assert!(dsn.contains("Reporting-MTA: dns; mx.alo.test"));
        assert!(dsn.contains("Final-Recipient: rfc822; alice@example.com"));
        assert!(dsn.contains("Action: failed"));
        assert!(dsn.contains("Status: 5.1.1"));
        assert!(dsn.contains("Diagnostic-Code: smtp; 550 5.1.1 no such user"));
        assert!(dsn.contains("Content-Type: text/rfc822-headers"));
        assert!(dsn.contains("Subject: original"));
        assert!(dsn.ends_with("--\r\n"));
    }

    #[test]
    fn status_extraction_prefers_enhanced_codes() {
        assert_eq!(extract_status(550, "5.7.1 blocked"), "5.7.1");
        assert_eq!(extract_status(550, "no such user"), "5.0.0");
        assert_eq!(extract_status(451, "greylisted"), "4.0.0");
        // Not enhanced codes:
        assert_eq!(extract_status(550, "5.7 short"), "5.0.0");
        assert_eq!(extract_status(550, "9.9.9 bogus class"), "5.0.0");
    }

    #[test]
    fn header_block_splits_at_first_empty_line() {
        assert_eq!(
            header_block(b"A: 1\r\nB: 2\r\n\r\nbody\r\n"),
            "A: 1\r\nB: 2"
        );
        assert_eq!(header_block(b"A: 1\r\n"), "A: 1\r\n");
    }
}
