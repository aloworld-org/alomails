//! Parsing a received RFC 5322 message into the fields the store
//! persists — including the `Authentication-Results` verdict (RFC 8601)
//! parsed back into queryable SPF/DKIM/DMARC results.

use alo_auth_mail::dkim::Message;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc2822;

use crate::thread;

/// Cap on the body text fed to the search index (a very large body must
/// not blow up the tsvector build).
const MAX_BODY_TEXT: usize = 1024 * 1024;

/// The fields extracted from a raw message for ingestion.
#[derive(Debug, Clone)]
pub struct ParsedMessage {
    /// `Message-ID` header, angle brackets included (for threading).
    pub message_id: Option<String>,
    /// Unfolded `Subject`.
    pub subject: String,
    /// Unfolded `From`.
    pub from_addr: String,
    /// Unfolded `To`.
    pub to_addrs: String,
    /// Whether the message carries an attachment (for the list paperclip).
    pub has_attachment: bool,
    /// Unfolded `Cc`.
    pub cc_addrs: String,
    /// Unfolded `Bcc` (present only on the sender's own copy — the wire message
    /// has it stripped, so a received message parses this as empty).
    pub bcc_addrs: String,
    /// Message-ids referenced via `In-Reply-To` + `References`.
    pub referenced_ids: Vec<String>,
    /// Parsed `Date`, when present and well-formed.
    pub sent_at: Option<OffsetDateTime>,
    /// Parsed Authentication-Results SPF result token.
    pub auth_spf: Option<String>,
    /// Parsed Authentication-Results DKIM result token.
    pub auth_dkim: Option<String>,
    /// Parsed Authentication-Results DMARC result token.
    pub auth_dmarc: Option<String>,
    /// The raw Authentication-Results header value (unfolded).
    pub auth_raw: Option<String>,
    /// Best-effort body text for the search index.
    pub body_text: String,
}

/// Parses a raw message into [`ParsedMessage`]. Never panics on
/// malformed input — missing/garbled headers yield empty/`None` fields.
pub fn parse(raw: &[u8]) -> ParsedMessage {
    let msg = Message::parse(raw);

    // Unfold, then RFC 2047-decode encoded-words so accented subjects and
    // display names are stored (and full-text-indexed) as readable UTF-8. The
    // addr-spec inside <…> is never an encoded-word, so addresses are intact.
    let decode_hdr = |v: &str| crate::rfc2047::decode(&unfold(v));
    let subject = header(&msg, "Subject").map(decode_hdr).unwrap_or_default();
    let from_addr = header(&msg, "From").map(decode_hdr).unwrap_or_default();
    let to_addrs = header(&msg, "To").map(decode_hdr).unwrap_or_default();
    let cc_addrs = header(&msg, "Cc").map(decode_hdr).unwrap_or_default();
    let bcc_addrs = header(&msg, "Bcc").map(decode_hdr).unwrap_or_default();

    let message_id = header(&msg, "Message-ID")
        .map(unfold)
        .and_then(|v| thread::extract_message_ids(&v).into_iter().next());

    let mut referenced_ids = Vec::new();
    for name in ["In-Reply-To", "References"] {
        if let Some(value) = header(&msg, name) {
            for id in thread::extract_message_ids(&unfold(value)) {
                if !referenced_ids.contains(&id) {
                    referenced_ids.push(id);
                }
            }
        }
    }

    let sent_at = header(&msg, "Date")
        .map(unfold)
        .and_then(|d| OffsetDateTime::parse(&d, &Rfc2822).ok());

    let auth_raw = header(&msg, "Authentication-Results").map(unfold);
    let (auth_spf, auth_dkim, auth_dmarc) = auth_raw
        .as_deref()
        .map(parse_authentication_results)
        .unwrap_or((None, None, None));

    let mut body_text = String::from_utf8_lossy(msg.body).into_owned();
    if body_text.len() > MAX_BODY_TEXT {
        // Truncate on a char boundary.
        let mut end = MAX_BODY_TEXT;
        while end > 0 && !body_text.is_char_boundary(end) {
            end -= 1;
        }
        body_text.truncate(end);
    }

    ParsedMessage {
        message_id,
        subject,
        from_addr,
        to_addrs,
        has_attachment: detect_attachment(raw),
        cc_addrs,
        bcc_addrs,
        referenced_ids,
        sent_at,
        auth_spf,
        auth_dkim,
        auth_dmarc,
        auth_raw,
        body_text,
    }
}

fn header<'a>(msg: &'a Message<'a>, name: &str) -> Option<&'a str> {
    msg.headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| *v)
}

/// Whether the raw message carries an attachment — a cheap heuristic used for
/// the list paperclip: any MIME part with `Content-Disposition: attachment`
/// (folding tolerated within a short window). No full MIME parse; false
/// positives from inline dispositions are deliberately excluded.
pub fn detect_attachment(raw: &[u8]) -> bool {
    fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
        if needle.is_empty() || hay.len() < needle.len() {
            return None;
        }
        hay.windows(needle.len()).position(|w| w == needle)
    }
    let lower = raw.to_ascii_lowercase();
    let cd = b"content-disposition:";
    let mut i = 0;
    while let Some(pos) = find(&lower[i..], cd) {
        let start = i + pos + cd.len();
        let end = (start + 64).min(lower.len());
        if find(&lower[start..end], b"attachment").is_some() {
            return true;
        }
        i = start;
    }
    false
}

/// Unfolds a header value (RFC 5322 §2.2.3): folding CRLFs are removed,
/// runs of whitespace collapsed to single spaces.
fn unfold(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut last_space = false;
    for c in value.chars() {
        if c == '\r' || c == '\n' || c == '\t' || c == ' ' {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(c);
            last_space = false;
        }
    }
    out.trim().to_owned()
}

/// Parses the `Authentication-Results` value (RFC 8601): the first
/// `;`-separated clause is the authserv-id; each later clause begins
/// `method=result`. Returns the first SPF/DKIM/DMARC result seen.
fn parse_authentication_results(value: &str) -> (Option<String>, Option<String>, Option<String>) {
    let (mut spf, mut dkim, mut dmarc) = (None, None, None);
    for clause in value.split(';') {
        let token = clause.split_whitespace().next().unwrap_or("");
        let Some((method, result)) = token.split_once('=') else {
            continue; // authserv-id or malformed clause
        };
        let result = result.to_owned();
        match method.to_ascii_lowercase().as_str() {
            "spf" if spf.is_none() => spf = Some(result),
            "dkim" if dkim.is_none() => dkim = Some(result),
            "dmarc" if dmarc.is_none() => dmarc = Some(result),
            _ => {}
        }
    }
    (spf, dkim, dmarc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_attachments() {
        let with = b"Content-Type: multipart/mixed; boundary=b\r\n\r\n--b\r\n\
            Content-Type: text/plain\r\n\r\nhi\r\n--b\r\n\
            Content-Type: application/zip\r\n\
            Content-Disposition: attachment; filename=\"report.zip\"\r\n\r\nPK..\r\n--b--\r\n";
        assert!(detect_attachment(with));

        // Folded disposition still counts.
        let folded = b"Content-Disposition:\r\n attachment; filename=x\r\n";
        assert!(detect_attachment(folded));

        // Plain message and an inline part do not.
        assert!(!detect_attachment(b"Subject: hi\r\n\r\njust text"));
        assert!(!detect_attachment(
            b"Content-Disposition: inline\r\nContent-Type: image/png\r\n"
        ));
    }

    const RAW: &[u8] = b"From: Alice <alice@example.com>\r\n\
To: Bob <bob@example.org>\r\n\
Subject: Re: Quarterly\r\n \tplan\r\n\
Message-ID: <msg-1@example.com>\r\n\
In-Reply-To: <msg-0@example.com>\r\n\
References: <root@example.com> <msg-0@example.com>\r\n\
Date: Mon, 27 Jul 2026 10:30:00 +0000\r\n\
Authentication-Results: mx.alo.test; spf=pass smtp.mailfrom=alice@example.com; \
dkim=pass header.d=example.com; dmarc=pass header.from=example.com\r\n\
\r\n\
Hello there, this is the body.\r\n";

    #[test]
    fn parses_all_fields() {
        let p = parse(RAW);
        assert_eq!(p.message_id.as_deref(), Some("<msg-1@example.com>"));
        assert_eq!(p.subject, "Re: Quarterly plan");
        assert_eq!(p.from_addr, "Alice <alice@example.com>");
        assert_eq!(p.to_addrs, "Bob <bob@example.org>");
        assert_eq!(
            p.referenced_ids,
            vec![
                "<msg-0@example.com>".to_owned(),
                "<root@example.com>".to_owned()
            ]
        );
        assert!(p.sent_at.is_some());
        assert_eq!(p.auth_spf.as_deref(), Some("pass"));
        assert_eq!(p.auth_dkim.as_deref(), Some("pass"));
        assert_eq!(p.auth_dmarc.as_deref(), Some("pass"));
        assert!(p.body_text.contains("Hello there"));
    }

    #[test]
    fn missing_headers_are_none_not_panic() {
        let p = parse(b"\r\nonly a body\r\n");
        assert!(p.message_id.is_none());
        assert_eq!(p.subject, "");
        assert!(p.auth_spf.is_none());
        assert!(p.sent_at.is_none());
    }

    #[test]
    fn authentication_results_authserv_id_is_not_a_method() {
        let (spf, dkim, dmarc) = parse_authentication_results("mx.alo.test; spf=fail; dmarc=fail");
        assert_eq!(spf.as_deref(), Some("fail"));
        assert!(dkim.is_none());
        assert_eq!(dmarc.as_deref(), Some("fail"));
    }

    #[test]
    fn decodes_rfc2047_subject_and_display_name() {
        let raw = b"From: =?UTF-8?B?SMOpbMOobmU=?= <helene@proceq.eu>\r\n\
Subject: =?ISO-8859-1?Q?caf=E9_pr=EAt?=\r\n\
\r\n\
body\r\n";
        let p = parse(raw);
        // Subject decoded to readable UTF-8; the address is untouched.
        assert_eq!(p.subject, "café prêt");
        assert_eq!(p.from_addr, "Hélène <helene@proceq.eu>");
    }
}
