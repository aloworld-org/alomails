//! RFC 6409 Message Submission header fixups applied by the MSA
//! (submission role only), §8.
//!
//! We apply the safe, non-destructive subset: add a `Date:` and a
//! `Message-ID:` when the submitted message lacks them (§8.1, §8.2).
//! We deliberately do NOT rewrite `From`/`Sender` or canonicalize
//! addresses (recorded out-of-scope in the M3 design note) — those
//! change author-visible content and belong with a policy decision.

use jiff::Zoned;

/// Applies submission fixups to `message` (a full RFC 5322 message
/// whose header block ends at the first blank line). `id` seeds a
/// generated Message-ID; `when`/`hostname` seed a generated Date and
/// the Message-ID domain. Returns the possibly-modified message.
///
/// Idempotent: headers already present are never duplicated.
pub fn apply_fixups(message: &[u8], hostname: &str, id: &str, when: &Zoned) -> Vec<u8> {
    let headers_lower = header_block(message).to_ascii_lowercase();

    let mut added = Vec::new();
    if !has_header(&headers_lower, "date:") {
        let date = jiff::fmt::rfc2822::to_string(when).unwrap_or_else(|_| when.to_string());
        added.push(format!("Date: {date}\r\n"));
    }
    if !has_header(&headers_lower, "message-id:") {
        added.push(format!("Message-ID: <{id}@{hostname}>\r\n"));
    }

    if added.is_empty() {
        return message.to_vec();
    }

    // Prepend the new headers, then the whole original message intact
    // (header order within the section is insignificant, RFC 5322
    // §3.6, so prepending is safe and never touches the body).
    let mut out = Vec::with_capacity(message.len() + 128);
    for header in &added {
        out.extend_from_slice(header.as_bytes());
    }
    out.extend_from_slice(message);
    out
}

/// The header block of a message: everything before the `CRLF CRLF`
/// separator, or the whole message when there is no blank line.
fn header_block(message: &[u8]) -> String {
    match message.windows(4).position(|w| w == b"\r\n\r\n") {
        Some(pos) => String::from_utf8_lossy(&message[..pos]).into_owned(),
        None => String::from_utf8_lossy(message).into_owned(),
    }
}

/// Header presence check against a lowercased header block: the field
/// name must start a line (`^name:` or after a CRLF).
fn has_header(headers_lower: &str, name_lower: &str) -> bool {
    if headers_lower.starts_with(name_lower) {
        return true;
    }
    headers_lower
        .match_indices(name_lower)
        .any(|(i, _)| i >= 2 && &headers_lower[i - 2..i] == "\r\n")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn when() -> Zoned {
        "2026-07-27T09:00:00+00:00[UTC]".parse().unwrap()
    }

    fn text(v: &[u8]) -> String {
        String::from_utf8(v.to_vec()).unwrap()
    }

    #[test]
    fn adds_missing_date_and_message_id() {
        let msg = b"Subject: hi\r\nFrom: bob@alo.test\r\n\r\nbody\r\n";
        let out = apply_fixups(msg, "mx.alo.test", "abc.1", &when());
        let s = text(&out);
        assert!(s.contains("Date: Mon, 27 Jul 2026"));
        assert!(s.contains("Message-ID: <abc.1@mx.alo.test>"));
        assert!(s.contains("Subject: hi"));
        assert!(s.ends_with("body\r\n"));
    }

    #[test]
    fn preserves_existing_date_and_message_id() {
        let msg = b"Date: Sun, 01 Jan 2023 00:00:00 +0000\r\nMessage-ID: <orig@x>\r\nSubject: hi\r\n\r\nbody\r\n";
        let out = apply_fixups(msg, "mx.alo.test", "abc.1", &when());
        let s = text(&out);
        // No duplication; original values intact.
        assert_eq!(s.matches("Date:").count(), 1);
        assert_eq!(s.matches("Message-ID:").count(), 1);
        assert!(s.contains("<orig@x>"));
        assert_eq!(out, msg);
    }

    #[test]
    fn case_insensitive_header_detection() {
        let msg = b"DATE: Sun, 01 Jan 2023 00:00:00 +0000\r\nmessage-id: <orig@x>\r\n\r\nbody\r\n";
        let out = apply_fixups(msg, "mx.alo.test", "abc.1", &when());
        assert_eq!(out, msg, "existing headers detected regardless of case");
    }

    #[test]
    fn substring_in_value_does_not_count_as_header() {
        // "Message-ID:" appearing in a value must not suppress the fixup.
        let msg = b"Subject: my Message-ID: is missing\r\n\r\nbody\r\n";
        let out = apply_fixups(msg, "mx.alo.test", "abc.1", &when());
        let s = text(&out);
        assert!(s.contains("Message-ID: <abc.1@mx.alo.test>"));
    }

    #[test]
    fn headers_only_message_still_gets_fixups() {
        let msg = b"Subject: no body\r\n";
        let out = apply_fixups(msg, "mx.alo.test", "abc.1", &when());
        let s = text(&out);
        assert!(s.contains("Date:"));
        assert!(s.contains("Message-ID:"));
        assert!(s.contains("Subject: no body"));
    }
}
