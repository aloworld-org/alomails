//! Building an RFC 5322 / MIME message for outgoing mail.
//!
//! European-correct by construction: non-ASCII display names and subjects are
//! emitted as RFC 2047 `B` encoded-words (each ≤75 chars), and a non-ASCII
//! body is base64 so transport is 7-bit clean on any path. Every header value
//! is CR/LF-sanitized before use — there is no header-injection path from a
//! composed field. Header lines are folded to ≤78 columns (RFC 5322 §2.1.1);
//! because non-ASCII is always encoded to ASCII first, folding is byte-safe.
//!
//! Structure: a `text/plain` body by default; a `multipart/alternative`
//! (text + HTML) when an HTML body is present; and a `multipart/mixed` wrapping
//! either of those plus one base64 part per attachment
//! (`Content-Disposition: attachment`).

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;

/// One address: an optional display name plus the addr-spec.
#[derive(Debug, Clone)]
pub struct Addr {
    pub name: Option<String>,
    pub email: String,
}

/// One outgoing attachment: its decoded bytes plus display name and MIME type.
pub struct Attachment {
    pub name: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// The fields of an outgoing message (text/plain, multipart/alternative with an
/// HTML body, and/or multipart/mixed when it carries attachments).
pub struct Outgoing {
    pub from: Addr,
    pub to: Vec<Addr>,
    pub cc: Vec<Addr>,
    /// Blind-carbon recipients. The `Bcc:` header is written here so the
    /// sender's own (Drafts/Sent) copy records them, but it is **stripped from
    /// the message before transmission** (see `submission::strip_bcc_header`) so
    /// recipients never learn who was blind-copied. Delivery uses the envelope.
    pub bcc: Vec<Addr>,
    pub subject: String,
    /// Parent message-ids (bare, no angle brackets) for `In-Reply-To`.
    pub in_reply_to: Vec<String>,
    /// The `References` chain (bare message-ids).
    pub references: Vec<String>,
    pub body_text: String,
    /// Optional HTML body. When present the message carries both a text/plain
    /// and a text/html part in a `multipart/alternative`.
    pub body_html: Option<String>,
    /// Attachments; when non-empty the message is built as multipart/mixed.
    pub attachments: Vec<Attachment>,
    /// The submission hostname, for the `Message-ID` domain if the submission
    /// pipeline does not add one. (It does — this is a belt-and-braces seed.)
    pub message_id_domain: String,
    /// A unique token seeding the `Message-ID` local part.
    ///
    /// Produce it with [`new_message_id_token`]. Anything derived from the
    /// sender, the subject or the time alone is not unique enough — see that
    /// function for what goes wrong.
    pub message_id_token: String,
}

/// A token for one message's `Message-ID`, unique across every message alo
/// ever sends.
///
/// RFC 5322 §3.6.4 requires a globally unique `Message-ID`, and receiving
/// servers and clients **deduplicate on it**. A token that repeats does not
/// produce a visible error: the second message is quietly discarded or threaded
/// into the first, which is silent mail loss — the worst thing this product can
/// do.
///
/// So it is random, from the same CSPRNG the store's ids use, with a timestamp
/// in front for readability in a header somebody may one day be reading while
/// debugging. The randomness is what makes it unique; the timestamp only makes
/// it legible, and two messages in the same nanosecond still differ.
#[must_use]
pub fn new_message_id_token() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("{nanos:x}.{}", crate::id::MessageId::generate().as_str())
}

/// Builds the full RFC 5322 message bytes (CRLF line endings).
pub fn build(msg: &Outgoing) -> Vec<u8> {
    let mut headers: Vec<String> = Vec::new();

    headers.push(fold(&format!("From: {}", format_addr(&msg.from))));
    if !msg.to.is_empty() {
        headers.push(fold(&format!("To: {}", format_addr_list(&msg.to))));
    }
    if !msg.cc.is_empty() {
        headers.push(fold(&format!("Cc: {}", format_addr_list(&msg.cc))));
    }
    if !msg.bcc.is_empty() {
        headers.push(fold(&format!("Bcc: {}", format_addr_list(&msg.bcc))));
    }
    headers.push(fold(&format!(
        "Subject: {}",
        encode_unstructured(&msg.subject)
    )));
    headers.push(format!(
        "Message-ID: <{}@{}>",
        sanitize(&msg.message_id_token),
        sanitize(&msg.message_id_domain)
    ));
    if let Some(first) = msg.in_reply_to.first() {
        headers.push(fold(&format!("In-Reply-To: {}", angle(first))));
    }
    if !msg.references.is_empty() {
        let refs = msg
            .references
            .iter()
            .map(|r| angle(r))
            .collect::<Vec<_>>()
            .join(" ");
        headers.push(fold(&format!("References: {refs}")));
    }
    headers.push("MIME-Version: 1.0".to_owned());

    let has_html = msg.body_html.as_ref().is_some_and(|h| !h.trim().is_empty());
    let mut out = Vec::with_capacity(msg.body_text.len() + 1024);

    // Structure by what the message carries:
    //   text only            → top-level text/plain
    //   text + html          → top-level multipart/alternative
    //   + attachments        → top-level multipart/mixed wrapping the above
    if msg.attachments.is_empty() {
        if has_html {
            let alt = format!("=_alt_{}", sanitize(&msg.message_id_token));
            headers.push(format!(
                "Content-Type: multipart/alternative; boundary=\"{alt}\""
            ));
            write_headers(&mut out, &headers);
            out.extend_from_slice(b"\r\n");
            write_alternative(
                &mut out,
                &alt,
                &msg.body_text,
                msg.body_html.as_deref().unwrap_or(""),
            );
        } else {
            let (cte, body) = encode_body(&msg.body_text);
            headers.push("Content-Type: text/plain; charset=utf-8".to_owned());
            headers.push(format!("Content-Transfer-Encoding: {cte}"));
            write_headers(&mut out, &headers);
            out.extend_from_slice(b"\r\n");
            out.extend_from_slice(&body);
            ensure_crlf(&mut out);
        }
        return out;
    }

    // multipart/mixed: the main body part (text/plain or multipart/alternative)
    // followed by one part per attachment. Boundaries are seeded from the unique
    // message-id token so they cannot collide with generated content.
    let mix = format!("=_mix_{}", sanitize(&msg.message_id_token));
    headers.push(format!("Content-Type: multipart/mixed; boundary=\"{mix}\""));
    write_headers(&mut out, &headers);
    out.extend_from_slice(b"\r\n");

    out.extend_from_slice(format!("--{mix}\r\n").as_bytes());
    if has_html {
        let alt = format!("=_alt_{}", sanitize(&msg.message_id_token));
        out.extend_from_slice(
            format!("Content-Type: multipart/alternative; boundary=\"{alt}\"\r\n\r\n").as_bytes(),
        );
        write_alternative(
            &mut out,
            &alt,
            &msg.body_text,
            msg.body_html.as_deref().unwrap_or(""),
        );
    } else {
        write_text_part(&mut out, &msg.body_text);
    }

    for att in &msg.attachments {
        out.extend_from_slice(format!("--{mix}\r\n").as_bytes());
        write_attachment_part(&mut out, att);
    }
    out.extend_from_slice(format!("--{mix}--\r\n").as_bytes());
    out
}

/// Append CRLF-terminated header lines to `out`.
fn write_headers(out: &mut Vec<u8>, headers: &[String]) {
    for h in headers {
        out.extend_from_slice(h.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
}

/// Ensure `out` ends with CRLF (part/message boundary hygiene).
fn ensure_crlf(out: &mut Vec<u8>) {
    if !out.ends_with(b"\r\n") {
        out.extend_from_slice(b"\r\n");
    }
}

/// A text/plain body part (headers + blank + encoded body + CRLF).
fn write_text_part(out: &mut Vec<u8>, text: &str) {
    let (cte, body) = encode_body(text);
    out.extend_from_slice(b"Content-Type: text/plain; charset=utf-8\r\n");
    out.extend_from_slice(format!("Content-Transfer-Encoding: {cte}\r\n\r\n").as_bytes());
    out.extend_from_slice(&body);
    ensure_crlf(out);
}

/// A text/html body part.
fn write_html_part(out: &mut Vec<u8>, html: &str) {
    let (cte, body) = encode_body(html);
    out.extend_from_slice(b"Content-Type: text/html; charset=utf-8\r\n");
    out.extend_from_slice(format!("Content-Transfer-Encoding: {cte}\r\n\r\n").as_bytes());
    out.extend_from_slice(&body);
    ensure_crlf(out);
}

/// A multipart/alternative block: the text/plain part then the text/html part
/// (least-to-most faithful, per RFC 2046 §5.1.4).
fn write_alternative(out: &mut Vec<u8>, boundary: &str, text: &str, html: &str) {
    out.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    write_text_part(out, text);
    out.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    write_html_part(out, html);
    out.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
}

/// A base64 attachment part with a quote-safe filename.
fn write_attachment_part(out: &mut Vec<u8>, att: &Attachment) {
    let name = header_param(&att.name);
    let ctype = sanitize(&att.content_type);
    out.extend_from_slice(format!("Content-Type: {ctype}; name=\"{name}\"\r\n").as_bytes());
    out.extend_from_slice(b"Content-Transfer-Encoding: base64\r\n");
    out.extend_from_slice(
        format!("Content-Disposition: attachment; filename=\"{name}\"\r\n\r\n").as_bytes(),
    );
    out.extend_from_slice(&base64_wrapped(&att.bytes));
}

/// Base64 of `bytes`, wrapped at 76 columns with CRLF (RFC 2045).
fn base64_wrapped(bytes: &[u8]) -> Vec<u8> {
    let b64 = B64.encode(bytes);
    let mut out = Vec::with_capacity(b64.len() + b64.len() / 76 * 2 + 2);
    for chunk in b64.as_bytes().chunks(76) {
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\r\n");
    }
    out
}

/// Sanitize a filename for a quoted header parameter: drop CR/LF and characters
/// that would break the quoted-string (`"` and `\`).
fn header_param(name: &str) -> String {
    name.replace(['\r', '\n', '"', '\\'], "")
}

/// Strips CR and LF from a header input (header-injection guard).
fn sanitize(s: &str) -> String {
    s.replace(['\r', '\n'], " ")
}

fn is_ascii_clean(s: &str) -> bool {
    s.bytes().all(|b| (0x20..=0x7e).contains(&b))
}

/// Wraps a bare message-id in angle brackets (idempotent).
fn angle(id: &str) -> String {
    let id = sanitize(id);
    let trimmed = id.trim().trim_start_matches('<').trim_end_matches('>');
    format!("<{trimmed}>")
}

/// An address as a header token: `email`, `"Display Name" <email>`, or with an
/// RFC 2047 encoded phrase when the name is non-ASCII.
pub fn format_addr(a: &Addr) -> String {
    let email = sanitize(&a.email);
    match &a.name {
        Some(name) if !name.trim().is_empty() => {
            let name = sanitize(name);
            if is_ascii_clean(&name) {
                let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{escaped}\" <{email}>")
            } else {
                format!("{} <{email}>", encoded_words(&name))
            }
        }
        _ => email,
    }
}

fn format_addr_list(list: &[Addr]) -> String {
    list.iter().map(format_addr).collect::<Vec<_>>().join(", ")
}

/// An unstructured header value (Subject): raw when ASCII, else encoded-words.
pub fn encode_unstructured(s: &str) -> String {
    let s = sanitize(s);
    if is_ascii_clean(&s) {
        s
    } else {
        encoded_words(&s)
    }
}

/// RFC 2047 `B` encoded-words for a UTF-8 string, each ≤75 chars, split on
/// character boundaries and joined by a folding space so the caller can place
/// them in a header.
fn encoded_words(s: &str) -> String {
    // Each encoded-word: "=?UTF-8?B?" + base64 + "?=". Keep base64 ≤ 60 chars
    // → ≤ 45 source bytes per word (well under the 75-char encoded-word limit).
    const MAX_BYTES: usize = 45;
    let mut words: Vec<String> = Vec::new();
    let mut chunk: Vec<u8> = Vec::new();
    for ch in s.chars() {
        let mut buf = [0u8; 4];
        let encoded = ch.encode_utf8(&mut buf).as_bytes();
        if chunk.len() + encoded.len() > MAX_BYTES && !chunk.is_empty() {
            words.push(format!("=?UTF-8?B?{}?=", B64.encode(&chunk)));
            chunk.clear();
        }
        chunk.extend_from_slice(encoded);
    }
    if !chunk.is_empty() {
        words.push(format!("=?UTF-8?B?{}?=", B64.encode(&chunk)));
    }
    // Encoded-words are separated by folding whitespace (a plain space here;
    // fold() will break the line if it grows too long).
    words.join(" ")
}

/// Chooses the body transfer encoding: 7bit for clean ASCII with short lines,
/// otherwise base64 (wrapped at 76). Line endings are normalized to CRLF.
fn encode_body(text: &str) -> (&'static str, Vec<u8>) {
    let normalized = normalize_crlf(text);
    let ascii = normalized.iter().all(|&b| b < 0x80);
    let short_lines = normalized
        .split(|&b| b == b'\n')
        .all(|line| line.len() <= 990);
    if ascii && short_lines {
        ("7bit", normalized)
    } else {
        let b64 = B64.encode(&normalized);
        let mut wrapped = Vec::with_capacity(b64.len() + b64.len() / 76 * 2 + 2);
        for chunk in b64.as_bytes().chunks(76) {
            wrapped.extend_from_slice(chunk);
            wrapped.extend_from_slice(b"\r\n");
        }
        ("base64", wrapped)
    }
}

fn normalize_crlf(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 16);
    let mut prev_cr = false;
    for &b in text.as_bytes() {
        match b {
            b'\n' => {
                if !prev_cr {
                    out.push(b'\r');
                }
                out.push(b'\n');
                prev_cr = false;
            }
            b'\r' => {
                out.push(b'\r');
                out.push(b'\n');
                prev_cr = true;
            }
            other => {
                out.push(other);
                prev_cr = false;
            }
        }
    }
    out
}

/// Folds an ASCII header line to ≤78 columns at spaces (RFC 5322 §2.2.3),
/// continuation lines beginning with a single space. Inputs are ASCII here
/// (non-ASCII was already encoded), so byte indexing is char-safe.
fn fold(header: &str) -> String {
    const LIMIT: usize = 78;
    if header.len() <= LIMIT {
        return header.to_owned();
    }
    let mut out = String::with_capacity(header.len() + 8);
    let mut line_len = 0usize;
    let mut last_space: Option<usize> = None;
    for ch in header.chars() {
        out.push(ch);
        line_len += 1;
        if ch == ' ' {
            last_space = Some(out.len() - 1);
        }
        if line_len > LIMIT
            && let Some(sp) = last_space
        {
            out.replace_range(sp..sp + 1, "\r\n ");
            line_len = out.len() - (sp + 3);
            last_space = None;
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Two messages must never share a `Message-ID`: receiving servers
    /// deduplicate on it, so a repeat is silent mail loss. A token derived from
    /// the sender or the subject looks fine in one message and collides in the
    /// second — this is what caught exactly that.
    #[test]
    fn message_id_tokens_never_repeat() {
        let tokens: std::collections::HashSet<String> =
            (0..256).map(|_| new_message_id_token()).collect();
        assert_eq!(tokens.len(), 256, "tokens repeated within one run");

        let seeded = tokens.iter().next().unwrap();
        assert!(
            !seeded.is_empty() && !seeded.contains(['<', '>', '@', ' ']),
            "token {seeded} is not safe in a Message-ID local part"
        );
    }

    fn addr(name: Option<&str>, email: &str) -> Addr {
        Addr {
            name: name.map(str::to_owned),
            email: email.to_owned(),
        }
    }

    fn base(subject: &str, body: &str) -> Outgoing {
        Outgoing {
            from: addr(Some("Disan"), "disan@namel3ss.com"),
            to: vec![addr(Some("Alice Ng"), "alice@example.eu")],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: subject.to_owned(),
            in_reply_to: Vec::new(),
            references: Vec::new(),
            body_text: body.to_owned(),
            body_html: None,
            attachments: Vec::new(),
            message_id_domain: "namel3ss.com".to_owned(),
            message_id_token: "abc123".to_owned(),
        }
    }

    fn text(msg: &Outgoing) -> String {
        String::from_utf8(build(msg)).unwrap()
    }

    #[test]
    fn ascii_message_is_plain_and_7bit() {
        let s = text(&base("Hello", "Hi there\nline two\n"));
        assert!(s.contains("From: \"Disan\" <disan@namel3ss.com>"));
        assert!(s.contains("To: \"Alice Ng\" <alice@example.eu>"));
        assert!(s.contains("Subject: Hello\r\n"));
        assert!(s.contains("Content-Transfer-Encoding: 7bit"));
        assert!(s.contains("\r\n\r\nHi there\r\nline two\r\n"));
    }

    #[test]
    fn non_ascii_subject_is_encoded_word() {
        let s = text(&base("Ründtür — café", "body"));
        assert!(
            s.contains("Subject: =?UTF-8?B?"),
            "subject not encoded: {s}"
        );
        assert!(!s.contains("Ründtür"));
    }

    #[test]
    fn non_ascii_body_is_base64() {
        let s = text(&base("s", "Voilà, c'est déjà prêt — café ☕"));
        assert!(s.contains("Content-Transfer-Encoding: base64"));
        assert!(!s.contains("Voilà"));
    }

    #[test]
    fn non_ascii_display_name_is_encoded() {
        let mut m = base("s", "b");
        m.to = vec![addr(Some("Hélène Fonck"), "helene@proceq.eu")];
        let s = text(&m);
        assert!(s.contains("=?UTF-8?B?") && s.contains("<helene@proceq.eu>"));
        assert!(!s.contains("Hélène"));
    }

    #[test]
    fn header_injection_is_neutralized() {
        let mut m = base("Hi\r\nBcc: evil@x", "b");
        m.to = vec![addr(Some("A\r\nX: y"), "a@x.eu")];
        let s = text(&m);
        // No injected header lines: the only Bcc/X: text is inline, folded away.
        assert!(!s.contains("\r\nBcc: evil@x"));
        assert!(!s.contains("\r\nX: y"));
    }

    #[test]
    fn reply_headers_present_and_bracketed() {
        let mut m = base("Re: hi", "b");
        m.in_reply_to = vec!["orig@a.eu".to_owned()];
        m.references = vec!["<root@a.eu>".to_owned(), "orig@a.eu".to_owned()];
        let s = text(&m);
        assert!(s.contains("In-Reply-To: <orig@a.eu>"));
        assert!(s.contains("References: <root@a.eu> <orig@a.eu>"));
    }

    #[test]
    fn long_recipient_list_folds_under_998() {
        let mut m = base("s", "b");
        m.to = (0..20)
            .map(|i| {
                addr(
                    Some(&format!("Person Number {i}")),
                    &format!("person{i}@example.eu"),
                )
            })
            .collect();
        let s = text(&m);
        for line in s.split("\r\n") {
            assert!(line.len() <= 998, "line exceeds 998: {}", line.len());
        }
    }

    #[test]
    fn plain_address_without_name() {
        let mut m = base("s", "b");
        m.to = vec![addr(None, "bare@example.eu")];
        let s = text(&m);
        assert!(s.contains("To: bare@example.eu\r\n"));
    }

    #[test]
    fn attachment_produces_multipart_with_base64_part() {
        let mut m = base("Report", "See attached.\n");
        m.attachments = vec![Attachment {
            name: "report.zip".to_owned(),
            content_type: "application/zip".to_owned(),
            bytes: vec![0x50, 0x4B, 0x03, 0x04, 0x0A], // "PK\x03\x04\n"
        }];
        let s = text(&m);
        assert!(s.contains("Content-Type: multipart/mixed; boundary=\"=_mix_abc123\""));
        // the text part
        assert!(s.contains("Content-Type: text/plain; charset=utf-8"));
        assert!(s.contains("See attached."));
        // the attachment part: type, disposition + filename, base64 body
        assert!(s.contains("Content-Type: application/zip; name=\"report.zip\""));
        assert!(s.contains("Content-Disposition: attachment; filename=\"report.zip\""));
        assert!(s.contains("Content-Transfer-Encoding: base64"));
        assert!(
            s.contains("UEsDBAo="),
            "attachment bytes are base64-encoded"
        );
        assert!(s.trim_end().ends_with("--=_mix_abc123--"));
    }

    #[test]
    fn html_body_produces_multipart_alternative() {
        let mut m = base("Hi", "plain version");
        m.body_html = Some("<p>rich <b>version</b></p>".to_owned());
        let s = text(&m);
        assert!(s.contains("Content-Type: multipart/alternative; boundary=\"=_alt_abc123\""));
        assert!(s.contains("Content-Type: text/plain; charset=utf-8"));
        assert!(s.contains("plain version"));
        assert!(s.contains("Content-Type: text/html; charset=utf-8"));
        assert!(s.contains("<p>rich <b>version</b></p>"));
        assert!(s.trim_end().ends_with("--=_alt_abc123--"));
    }

    #[test]
    fn html_plus_attachment_nests_alternative_in_mixed() {
        let mut m = base("Hi", "plain");
        m.body_html = Some("<b>rich</b>".to_owned());
        m.attachments = vec![Attachment {
            name: "f.txt".to_owned(),
            content_type: "text/plain".to_owned(),
            bytes: b"x".to_vec(),
        }];
        let s = text(&m);
        assert!(s.contains("Content-Type: multipart/mixed; boundary=\"=_mix_abc123\""));
        assert!(s.contains("Content-Type: multipart/alternative; boundary=\"=_alt_abc123\""));
        assert!(s.contains("Content-Type: text/html; charset=utf-8"));
        assert!(s.contains("Content-Disposition: attachment; filename=\"f.txt\""));
        assert!(s.trim_end().ends_with("--=_mix_abc123--"));
    }

    #[test]
    fn blank_html_stays_plain_text() {
        let mut m = base("Hi", "just text");
        m.body_html = Some("   ".to_owned());
        let s = text(&m);
        assert!(s.contains("Content-Type: text/plain; charset=utf-8"));
        assert!(!s.contains("multipart"));
    }

    #[test]
    fn attachment_filename_is_quote_safe() {
        let mut m = base("s", "b");
        m.attachments = vec![Attachment {
            name: "a\"b\\c.txt".to_owned(),
            content_type: "text/plain".to_owned(),
            bytes: b"x".to_vec(),
        }];
        let s = text(&m);
        assert!(
            s.contains("filename=\"abc.txt\""),
            "quotes/backslashes stripped"
        );
    }
}
