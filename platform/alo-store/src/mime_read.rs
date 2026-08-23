//! Reading inbound MIME. Extracts the display body (plain text + HTML) and
//! the attachment list from a raw RFC 5322 message, decoding transfer
//! encodings (base64/quoted-printable) and charsets along the way. This is
//! delegated to `mail-parser` — reading arbitrary real-world mail correctly
//! (nested multipart, encoded words, charsets) is a parser's job, not a
//! hand-rolled split on the first blank line.

use mail_parser::{MessageParser, MimeHeaders};

/// One attachment surfaced to JMAP: its position among the message's
/// attachments (used to build the composite download blob id), a display
/// name, MIME type, and decoded size in bytes.
pub struct Attachment {
    pub index: usize,
    pub name: String,
    pub content_type: String,
    pub size: usize,
    /// The part's `Content-ID` without angle brackets, if any — an HTML body's
    /// `cid:` reference resolves to the inline part with the matching id.
    pub content_id: Option<String>,
    /// Whether the part is `Content-Disposition: inline` (an embedded image),
    /// as opposed to a downloadable attachment.
    pub inline: bool,
}

/// Which header a recipient came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipientKind {
    /// `To`.
    To,
    /// `Cc`.
    Cc,
    /// `Bcc` — present only on the sender's own copy.
    Bcc,
}

/// One addressee, with the display name separated from the address.
///
/// Split by the parser rather than by hand: `"Müller, Anna" <a@x.test>` is one
/// address whose display name contains a comma, and a hand-rolled split on
/// commas turns it into two people who do not exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipient {
    /// Which header it came from.
    pub kind: RecipientKind,
    /// What a reader sees; falls back to the address when there is no name.
    pub display_name: String,
    /// The address itself.
    pub email: String,
}

/// The reading view of a parsed message.
pub struct Parsed {
    pub text: Option<String>,
    /// The HTML body — **which may have been generated from the plain text.**
    ///
    /// `mail-parser` renders a `text/html` view for a message that has none,
    /// so a plain-text message comes back with
    /// `<html><body>…<br/></body></html>` here. That is useful for a reading
    /// surface, and wrong for anything that has to say what the *sender* sent:
    /// see [`Parsed::html_is_original`].
    pub html: Option<String>,
    /// Whether [`Parsed::html`] came from a real `text/html` part.
    ///
    /// `false` means the parser generated it. A protocol that offers a client a
    /// choice of bodies has to know the difference — otherwise every plain-text
    /// message is displayed as generated markup, because a client that is
    /// offered HTML prefers it.
    pub html_is_original: bool,
    pub attachments: Vec<Attachment>,
    /// The message's List-Unsubscribe options, if it carries any.
    pub unsubscribe: Option<Unsubscribe>,
    /// Everyone the message was addressed to, in header order.
    pub recipients: Vec<Recipient>,
}

/// A message's unsubscribe options (RFC 2369 `List-Unsubscribe`, plus RFC 8058
/// one-click). `http`/`mailto` are the first URI of each scheme; `one_click` is
/// true only when there is an https URI and a `List-Unsubscribe-Post:
/// List-Unsubscribe=One-Click` header — i.e. the sender supports silent,
/// single-request unsubscribe.
pub struct Unsubscribe {
    pub http: Option<String>,
    pub mailto: Option<String>,
    pub one_click: bool,
}

/// The `<URI>` tokens of a List-Unsubscribe header, with any folding whitespace
/// inside a URI removed. Text outside the angle brackets is ignored.
fn bracketed_uris(raw: &str) -> Vec<String> {
    raw.split('<')
        .filter_map(|seg| seg.split_once('>'))
        .map(|(uri, _)| uri.split_whitespace().collect::<String>())
        .filter(|u| !u.is_empty())
        .collect()
}

/// Parse a message's unsubscribe options from its headers. `None` when there is
/// no usable `List-Unsubscribe`.
fn parse_unsubscribe(message: &mail_parser::Message) -> Option<Unsubscribe> {
    let raw = message.header_raw("List-Unsubscribe")?;
    let uris = bracketed_uris(raw);
    let mailto = uris.iter().find(|u| u.starts_with("mailto:")).cloned();
    // Prefer an https URI (required for one-click) over a bare http one.
    let http = uris
        .iter()
        .find(|u| u.starts_with("https://"))
        .or_else(|| uris.iter().find(|u| u.starts_with("http://")))
        .cloned();
    if http.is_none() && mailto.is_none() {
        return None;
    }
    let one_click = http.as_deref().is_some_and(|h| h.starts_with("https://"))
        && message
            .header_raw("List-Unsubscribe-Post")
            .is_some_and(|p| p.to_ascii_lowercase().contains("one-click"));
    Some(Unsubscribe {
        http,
        mailto,
        one_click,
    })
}

fn content_type_of(part: &mail_parser::MessagePart) -> String {
    match part.content_type() {
        Some(ct) => match ct.subtype() {
            Some(sub) => format!("{}/{}", ct.ctype(), sub),
            None => ct.ctype().to_owned(),
        },
        None => "application/octet-stream".to_owned(),
    }
}

/// The part's `Content-ID` with surrounding angle brackets and whitespace
/// stripped — the token an HTML `cid:` URL references.
fn content_id_of(part: &mail_parser::MessagePart) -> Option<String> {
    part.content_id()
        .map(|c| {
            c.trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_owned()
        })
        .filter(|c| !c.is_empty())
}

/// Whether the part is `Content-Disposition: inline` (an embedded image), not a
/// downloadable file.
fn is_inline(part: &mail_parser::MessagePart) -> bool {
    part.content_disposition()
        .is_some_and(|d| d.ctype().eq_ignore_ascii_case("inline"))
}

fn name_of(part: &mail_parser::MessagePart, index: usize) -> String {
    part.attachment_name()
        .map(str::to_owned)
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| format!("attachment-{}", index + 1))
}

/// Parse a raw message into its text/HTML body and attachment list. A message
/// that fails to parse yields an empty view (no body, no attachments) rather
/// than an error — the caller still has headers to show.
pub fn parse(raw: &[u8]) -> Parsed {
    let Some(message) = MessageParser::default().parse(raw) else {
        return Parsed {
            text: None,
            html: None,
            html_is_original: false,
            attachments: Vec::new(),
            unsubscribe: None,
            recipients: Vec::new(),
        };
    };
    let text = message.body_text(0).map(|c| c.into_owned());
    let html = message.body_html(0).map(|c| c.into_owned());
    // Whether any part actually declared itself HTML, rather than the parser
    // having rendered one for us.
    let html_is_original = message.parts.iter().any(|part| {
        matches!(part.content_type(), Some(ct)
            if ct.ctype().eq_ignore_ascii_case("text")
                && ct.subtype().is_some_and(|sub| sub.eq_ignore_ascii_case("html")))
    });
    let attachments = message
        .attachments()
        .enumerate()
        .map(|(index, part)| Attachment {
            index,
            name: name_of(part, index),
            content_type: content_type_of(part),
            size: part.contents().len(),
            content_id: content_id_of(part),
            inline: is_inline(part),
        })
        .collect();
    let unsubscribe = parse_unsubscribe(&message);
    let mut recipients = Vec::new();
    collect_recipients(message.to(), RecipientKind::To, &mut recipients);
    collect_recipients(message.cc(), RecipientKind::Cc, &mut recipients);
    collect_recipients(message.bcc(), RecipientKind::Bcc, &mut recipients);
    Parsed {
        text,
        html,
        html_is_original,
        attachments,
        unsubscribe,
        recipients,
    }
}

/// Adds one header's addresses to `out`.
///
/// An entry with no address at all is skipped: a display name nobody can send
/// to is not a recipient, and showing one implies a message could reach them.
fn collect_recipients(
    header: Option<&mail_parser::Address<'_>>,
    kind: RecipientKind,
    out: &mut Vec<Recipient>,
) {
    let Some(header) = header else {
        return;
    };
    for address in header.iter() {
        let Some(email) = address.address() else {
            continue;
        };
        let email = email.trim();
        if email.is_empty() {
            continue;
        }
        let display_name = address
            .name()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(email)
            .to_owned();
        out.push(Recipient {
            kind,
            display_name,
            email: email.to_owned(),
        });
    }
}

/// The decoded bytes of the message's iCalendar part (`text/calendar`), if it
/// has one — the payload of an iMIP invitation/reply/cancel. Walks every MIME
/// part (not just `attachments()`) so it also finds a calendar part that is the
/// message's sole body. Returns the first such part; the caller reads the
/// `METHOD` from the iCalendar body (authoritative per RFC 6047) to tell a
/// REQUEST from a REPLY. `None` for ordinary mail.
pub fn calendar_part(raw: &[u8]) -> Option<Vec<u8>> {
    let message = MessageParser::default().parse(raw)?;
    message
        .parts
        .iter()
        .find(|p| content_type_of(p).eq_ignore_ascii_case("text/calendar"))
        .map(|p| p.contents().to_vec())
}

/// The decoded bytes of the `index`-th attachment, plus its MIME type and
/// display name — for the download route. `None` if the message doesn't parse
/// or the index is out of range.
pub fn attachment_bytes(raw: &[u8], index: usize) -> Option<(Vec<u8>, String, String)> {
    let message = MessageParser::default().parse(raw)?;
    let part = message.attachments().nth(index)?;
    Some((
        part.contents().to_vec(),
        content_type_of(part),
        name_of(part, index),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // A multipart/mixed message with a text part and a base64 zip attachment —
    // the shape of a DMARC aggregate report, which previously rendered as raw
    // base64 in the body. "UEsDBAo=" decodes to the bytes PK\x03\x04\n.
    const MSG: &[u8] = concat!(
        "From: a@example.com\r\n",
        "To: b@example.com\r\n",
        "Subject: Report\r\n",
        "MIME-Version: 1.0\r\n",
        "Content-Type: multipart/mixed; boundary=\"b\"\r\n",
        "\r\n",
        "--b\r\n",
        "Content-Type: text/plain; charset=utf-8\r\n",
        "\r\n",
        "Please find the report attached.\r\n",
        "--b\r\n",
        "Content-Type: application/zip; name=\"report.zip\"\r\n",
        "Content-Transfer-Encoding: base64\r\n",
        "Content-Disposition: attachment; filename=\"report.zip\"\r\n",
        "\r\n",
        "UEsDBAo=\r\n",
        "--b--\r\n",
    )
    .as_bytes();

    // An iMIP invitation in the exact shape send_invitations produces:
    // multipart/alternative { text/plain, text/calendar; method=REQUEST }.
    const INVITE: &[u8] = concat!(
        "From: organizer@example.com\r\n",
        "To: guest@example.com\r\n",
        "Subject: Invitation: Kickoff\r\n",
        "MIME-Version: 1.0\r\n",
        "Content-Type: multipart/alternative; boundary=\"=_alo\"\r\n",
        "\r\n",
        "--=_alo\r\n",
        "Content-Type: text/plain; charset=utf-8\r\n",
        "\r\n",
        "You're invited to Kickoff.\r\n",
        "--=_alo\r\n",
        "Content-Type: text/calendar; charset=utf-8; method=REQUEST\r\n",
        "\r\n",
        "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:evt-1\r\n",
        "SUMMARY:Kickoff\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        "--=_alo--\r\n",
    )
    .as_bytes();

    #[test]
    fn surfaces_the_calendar_part_of_an_invitation() {
        let ics = calendar_part(INVITE).expect("the text/calendar part is reachable");
        let text = String::from_utf8(ics).unwrap();
        assert!(text.contains("METHOD:REQUEST"));
        assert!(text.contains("UID:evt-1"));
    }

    #[test]
    fn ordinary_mail_has_no_calendar_part() {
        assert!(calendar_part(MSG).is_none());
    }

    #[test]
    fn extracts_text_body_not_the_attachment() {
        let parsed = parse(MSG);
        let text = parsed.text.expect("a text body");
        assert!(text.contains("Please find the report attached."));
        assert!(
            !text.contains("UEsDBA"),
            "base64 must not leak into the body"
        );
    }

    #[test]
    fn lists_the_attachment_with_name_and_type() {
        let parsed = parse(MSG);
        assert_eq!(parsed.attachments.len(), 1);
        let a = &parsed.attachments[0];
        assert_eq!(a.name, "report.zip");
        assert_eq!(a.content_type, "application/zip");
        assert_eq!(a.index, 0);
        assert_eq!(a.content_id, None, "a file attachment has no Content-ID");
        assert!(!a.inline, "Content-Disposition: attachment ⇒ not inline");
    }

    #[test]
    fn inline_image_exposes_cid_and_disposition() {
        let raw = concat!(
            "From: a@example.com\r\n",
            "Subject: Newsletter\r\n",
            "MIME-Version: 1.0\r\n",
            "Content-Type: multipart/related; boundary=\"r\"\r\n",
            "\r\n",
            "--r\r\n",
            "Content-Type: text/html; charset=utf-8\r\n",
            "\r\n",
            "<p>Logo: <img src=\"cid:logo@shop\"></p>\r\n",
            "--r\r\n",
            "Content-Type: image/png\r\n",
            "Content-Transfer-Encoding: base64\r\n",
            "Content-ID: <logo@shop>\r\n",
            "Content-Disposition: inline\r\n",
            "\r\n",
            "iVBORw0KGgo=\r\n",
            "--r--\r\n",
        )
        .as_bytes();
        let parsed = parse(raw);
        assert!(parsed.html.expect("html body").contains("cid:logo@shop"));
        assert_eq!(parsed.attachments.len(), 1, "the inline image is a part");
        let img = &parsed.attachments[0];
        assert_eq!(
            img.content_id.as_deref(),
            Some("logo@shop"),
            "cid without <>"
        );
        assert!(img.inline, "Content-Disposition: inline");
        assert_eq!(img.content_type, "image/png");
    }

    #[test]
    fn attachment_bytes_are_transfer_decoded() {
        let (bytes, ctype, name) = attachment_bytes(MSG, 0).expect("attachment 0");
        assert_eq!(bytes, vec![0x50, 0x4B, 0x03, 0x04, 0x0A]); // "PK\x03\x04\n"
        assert_eq!(ctype, "application/zip");
        assert_eq!(name, "report.zip");
        assert!(attachment_bytes(MSG, 1).is_none(), "no second attachment");
    }

    /// Build a minimal message carrying the given extra header lines.
    fn msg_with(headers: &str) -> Vec<u8> {
        format!("From: news@shop.example\r\nSubject: Sale\r\n{headers}\r\nHello\r\n").into_bytes()
    }

    #[test]
    fn one_click_unsubscribe_is_recognized() {
        let raw = msg_with(
            "List-Unsubscribe: <https://shop.example/u?id=9>, <mailto:unsub@shop.example>\r\n\
             List-Unsubscribe-Post: List-Unsubscribe=One-Click\r\n",
        );
        let u = parse(&raw).unsubscribe.expect("unsubscribe present");
        assert_eq!(u.http.as_deref(), Some("https://shop.example/u?id=9"));
        assert_eq!(u.mailto.as_deref(), Some("mailto:unsub@shop.example"));
        assert!(u.one_click, "https URL + One-Click post header ⇒ one-click");
    }

    #[test]
    fn mailto_only_is_not_one_click() {
        let raw = msg_with("List-Unsubscribe: <mailto:unsub@shop.example?subject=stop>\r\n");
        let u = parse(&raw).unsubscribe.expect("unsubscribe present");
        assert_eq!(
            u.mailto.as_deref(),
            Some("mailto:unsub@shop.example?subject=stop")
        );
        assert!(u.http.is_none());
        assert!(!u.one_click, "no https URL ⇒ never one-click");
    }

    #[test]
    fn http_without_post_header_is_not_one_click() {
        // A browsing unsubscribe link, but no RFC 8058 One-Click support.
        let raw = msg_with("List-Unsubscribe: <https://shop.example/u?id=9>\r\n");
        let u = parse(&raw).unsubscribe.expect("unsubscribe present");
        assert_eq!(u.http.as_deref(), Some("https://shop.example/u?id=9"));
        assert!(!u.one_click);
    }

    #[test]
    fn no_list_unsubscribe_header_yields_none() {
        assert!(parse(&msg_with("")).unsubscribe.is_none());
    }
}
