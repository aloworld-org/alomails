//! The two parts of a campaign, assembled as `multipart/alternative` (alo
//! Campaigns, ADR 0044, wave C3.3).
//!
//! Queue item C3.3: *a plain-text alternative from the same blocks, **assembled
//! as `multipart/alternative`**.* [`crate::campaign_text`] writes the text part
//! and [`crate::campaign_html`] the HTML one; this module is the envelope that
//! makes them two readings of one letter rather than two mails.
//!
//! ## Nothing here sends, and that is not a gap
//!
//! ADR 0044 §1 requires a separate sending identity on its own IP, which is a
//! purchase this loop cannot make (C2). So what is built here is the **MIME
//! entity**: a `Content-Type` value with its boundary, and a body holding both
//! parts. The message headers a send adds around it — `From`, `To`,
//! `Message-ID`, `Date`, `MIME-Version`, RFC 8058's `List-Unsubscribe` — belong
//! to the sender, because every one of them is a fact about a send rather than
//! about a letter. Handing this entity to a submission path is one function
//! call on the day there is one.
//!
//! This is also why the assembly lives here and not in `alo-jmap`'s `mime.rs`,
//! which is the transactional composer: that builds a whole RFC 5322 message
//! from an `Outgoing` with a sender, recipients and a message id, and a
//! campaign has none of the three. Its `multipart/alternative` is the same
//! structure and the same RFC; what differs is everything around it, and the
//! encoding — see below.
//!
//! ## Quoted-printable, rather than base64 or nothing at all
//!
//! Both parts are quoted-printable, and each of the three alternatives is worse
//! for a reason worth writing down:
//!
//! - **8-bit as-is** would be an invalid message. RFC 5322 §2.1.1 caps a line
//!   at 998 octets and a campaign paragraph is capped at 5 000 characters, so
//!   one long paragraph is enough to produce a line no MTA is obliged to carry
//!   — and the HTML part, whose every block is one long line of markup and
//!   inline CSS, passes that cap on an ordinary letter.
//! - **Base64** is line-safe but turns the letter into an opaque blob: a
//!   content filter that cannot read the text part scores what it cannot see,
//!   and the operator debugging a delivery complaint cannot read the mail
//!   either. Bulk mail is exactly where both of those matter.
//! - **Quoted-printable** keeps a European letter legible on the wire —
//!   `Genève` is `Gen=C3=A8ve` and everything ASCII is itself — while bounding
//!   every line. It is what every serious sender uses for this, for these
//!   reasons.
//!
//! The encoder is deliberately strict about the two rules a naive one gets
//! wrong, because both corrupt a letter silently: **whitespace never sits at
//! the end of an encoded line** (a decoder may strip it, so an encoded trailing
//! space is written `=20`), and a line beginning `From ` has its `F` encoded so
//! an mbox-based archive on the path cannot mangle it into `>From `.
//!
//! ## The boundary is derived from the letter, not from a clock
//!
//! A random boundary would make the assembled message unpinnable, and a
//! golden file is the only thing that catches an encoding regression before a
//! recipient does. So the boundary is `sha256` of the two encoded parts:
//! deterministic, and unguessable-in-advance in the only way that matters here
//! — it cannot collide with content that was hashed to produce it.
//! [`unique_boundary`] then walks it forward on the (unreachable, and
//! therefore tested directly) chance that it appears in a part anyway. RFC 2046
//! §5.1.1 is unambiguous that a boundary appearing in a part destroys the
//! message, and "unreachable" is not a thing to leave resting on an argument.

use sha2::{Digest, Sha256};

use crate::campaign_html::{CampaignLetter, render_campaign_html};
use crate::campaign_text::render_campaign_text;
use crate::error::Result;

/// The column a quoted-printable line is broken at.
///
/// RFC 2045 §6.7 allows 76 including the trailing `=`. 72 is used instead so
/// that re-encoding a trailing space at a break (one character becoming three)
/// cannot push a line past the limit — the fix-up is applied *after* the break
/// point has been chosen, so the headroom has to exist in advance.
const QP_BREAK_COLUMN: usize = 72;

/// The hard cap a quoted-printable line may never reach, RFC 2045 §6.7.
pub const CAMPAIGN_MIME_LINE_MAX: usize = 76;

/// The prefix of every boundary this module produces. `=_` cannot occur in
/// quoted-printable output — a `=` there is always followed by two hex digits
/// or by the end of the line — which is what makes a collision impossible
/// rather than merely unlikely.
const BOUNDARY_PREFIX: &str = "=_alo_";

/// How many hex characters of the digest name the boundary.
///
/// 24 is 96 bits, which is far past what a token needs whose only job is to be
/// absent from two strings it was hashed from — and it is what keeps
/// `Content-Type: multipart/alternative; boundary="…"` at exactly the 78 octets
/// RFC 5322 §2.1.1 recommends a header line stay inside. A sender that has to
/// fold a `boundary` parameter meets the one class of MTA that then reassembles
/// it wrongly, and the whole message is unparseable rather than merely ugly.
/// [`tests::the_content_type_fits_a_header_line_nobody_has_to_fold`] holds it.
const BOUNDARY_DIGEST_CHARS: usize = 24;

/// The header line length RFC 5322 §2.1.1 recommends staying inside — the
/// number [`CampaignMessage::content_type`] is built to fit, and the one a
/// sender writing the header around it has to respect.
pub const CAMPAIGN_HEADER_LINE_MAX: usize = 78;

/// A campaign compiled into the two parts a mail carries, plus the entity that
/// holds them.
///
/// [`text`](Self::text) and [`html`](Self::html) are the parts *before*
/// encoding — kept because the preview screen (C3.6) shows exactly those, and
/// re-deriving them there would be a second rendering that could disagree with
/// the one that was actually assembled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CampaignMessage {
    /// The `Content-Type` header **value**, boundary included:
    /// `multipart/alternative; boundary="=_alo_…"`. The sender writes
    /// the header itself, beside the `MIME-Version: 1.0` that belongs to a
    /// message rather than to a part.
    pub content_type: String,
    /// The entity body: the text part, the HTML part, and the closing
    /// delimiter. CRLF throughout, and pure ASCII — see
    /// [`CampaignMessage::is_seven_bit_clean`].
    pub body: String,
    /// The plain-text alternative, as [`crate::campaign_text`] rendered it.
    pub text: String,
    /// The HTML alternative, as [`crate::campaign_html`] rendered it.
    pub html: String,
}

impl CampaignMessage {
    /// Whether the assembled body can travel a path that never negotiated
    /// 8BITMIME — true by construction, and asserted rather than assumed.
    pub fn is_seven_bit_clean(&self) -> bool {
        self.body.is_ascii()
    }
}

/// Compiles a campaign into the `multipart/alternative` entity of a mail.
///
/// The text part comes first. That is not house style: RFC 2046 §5.1.4 orders
/// the alternatives least-faithful first, and a client showing the *last* part
/// it understands is the whole mechanism — a mail with the HTML first is a mail
/// that arrives as markup in a text-only client.
///
/// # Errors
/// [`crate::error::StoreError::Validation`] when the body would not pass the
/// write gate, from whichever renderer reaches it first. Both apply the same
/// gate, so a letter can never be assembled with one legal part and one
/// illegal one.
pub fn render_campaign_message(letter: &CampaignLetter<'_>) -> Result<CampaignMessage> {
    let text = render_campaign_text(letter.content)?;
    let html = render_campaign_html(letter)?;

    let encoded_text = encode_quoted_printable(&text);
    let encoded_html = encode_quoted_printable(&html);
    let boundary = boundary_for(&[&encoded_text, &encoded_html]);

    let mut body = String::with_capacity(encoded_text.len() + encoded_html.len() + 512);
    push_part(&mut body, &boundary, "text/plain", &encoded_text);
    push_part(&mut body, &boundary, "text/html", &encoded_html);
    body.push_str("--");
    body.push_str(&boundary);
    body.push_str("--\r\n");

    Ok(CampaignMessage {
        content_type: format!("multipart/alternative; boundary=\"{boundary}\""),
        body,
        text,
        html,
    })
}

/// One part: its delimiter, its two headers, the blank line that ends them, and
/// the encoded body.
///
/// The body is followed by a CRLF of its own, which RFC 2046 §5.1.1 counts as
/// belonging to the delimiter rather than to the content — so a part whose text
/// ends without a newline does not acquire one, and a part whose text ends with
/// one does not lose it.
fn push_part(out: &mut String, boundary: &str, content_type: &str, encoded: &str) {
    out.push_str("--");
    out.push_str(boundary);
    out.push_str("\r\n");
    out.push_str("Content-Type: ");
    out.push_str(content_type);
    out.push_str("; charset=utf-8\r\n");
    out.push_str("Content-Transfer-Encoding: quoted-printable\r\n");
    out.push_str("\r\n");
    out.push_str(encoded);
    out.push_str("\r\n");
}

/// Derives the boundary from the parts it will separate. See the module docs.
fn boundary_for(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update(part.as_bytes());
        // A separator, so two parts cannot be re-cut into the same digest.
        digest.update([0x00]);
    }
    let hex = format!("{:x}", digest.finalize());
    let base = format!(
        "{BOUNDARY_PREFIX}{}",
        &hex[..BOUNDARY_DIGEST_CHARS.min(hex.len())]
    );
    unique_boundary(&base, parts)
}

/// Walks a candidate boundary forward until it appears in none of the parts.
///
/// Unreachable with the encoder above — quoted-printable cannot produce `=_` —
/// and written anyway, because RFC 2046 §5.1.1 makes a boundary that occurs
/// inside a part into a message that ends early, and a future part that is not
/// quoted-printable would otherwise break it silently. A walk that did fire
/// would lengthen the header past the 78 octets above, which is a header a
/// sender folds; that is the right trade, since an unfoldable header is worse
/// than a truncated message only in theory.
fn unique_boundary(base: &str, parts: &[&str]) -> String {
    let mut candidate = base.to_owned();
    let mut nonce = 0_u32;
    while parts.iter().any(|part| part.contains(&candidate)) {
        nonce += 1;
        candidate = format!("{base}_{nonce}");
    }
    candidate
}

/// Encodes a part as quoted-printable (RFC 2045 §6.7) with CRLF hard breaks.
///
/// The input's line endings are normalised first: a part carrying a bare LF
/// would be a body whose lines are not lines to an MTA.
fn encode_quoted_printable(text: &str) -> String {
    let normalised = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(normalised.len() + normalised.len() / 8);
    for (index, line) in normalised.split('\n').enumerate() {
        if index > 0 {
            out.push_str("\r\n");
        }
        encode_line(line, &mut out);
    }
    out
}

/// One input line, with soft breaks inserted so no output line reaches
/// [`CAMPAIGN_MIME_LINE_MAX`].
fn encode_line(line: &str, out: &mut String) {
    let mut column = 0_usize;
    for token in line_tokens(line) {
        if column > 0 && column + token.len() > QP_BREAK_COLUMN {
            // Whitespace must not sit at the end of an encoded line: a decoder
            // is entitled to strip it, and the letter would arrive re-flowed.
            if out.ends_with(' ') {
                out.pop();
                out.push_str("=20");
            } else if out.ends_with('\t') {
                out.pop();
                out.push_str("=09");
            }
            out.push_str("=\r\n");
            column = 0;
        }
        out.push_str(&token);
        column += token.len();
    }
}

/// One line as the atoms it encodes to — each is one character of output or an
/// `=XX` triple, and a soft break may fall between two of them but never inside
/// one.
fn line_tokens(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let mut tokens = Vec::with_capacity(bytes.len());
    // A line beginning `From ` is rewritten to `>From ` by anything that stores
    // mail as mbox. Encoding the F costs three characters and cannot be
    // mangled.
    let escape_from = line.starts_with("From ");
    for (index, byte) in bytes.iter().enumerate() {
        let last = index + 1 == bytes.len();
        let token = match byte {
            b'=' => "=3D".to_owned(),
            b' ' | b'\t' if last => format!("={byte:02X}"),
            b' ' => " ".to_owned(),
            b'\t' => "\t".to_owned(),
            b'F' if index == 0 && escape_from => "=46".to_owned(),
            0x21..=0x7E => char::from(*byte).to_string(),
            other => format!("={other:02X}"),
        };
        tokens.push(token);
    }
    tokens
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::campaign_content::{CampaignBlock, CampaignContent, TableBlock};
    use crate::error::StoreError;
    use serde_json::json;

    fn body(blocks: serde_json::Value) -> CampaignContent {
        CampaignContent::from_value(json!({ "schema_version": 1, "blocks": blocks }))
            .expect("the fixture body is valid")
    }

    fn letter() -> CampaignContent {
        body(json!([
            { "type": "heading", "id": "h1", "level": 1, "text": "Prijzen vanaf maandag" },
            { "type": "paragraph", "id": "p1", "text": "Beste klant,\nalles hieronder is per liter — Genève inbegrepen." },
            { "type": "table", "id": "t1", "rows": [["Product", "Prijs"], ["Olijfolie", "12,50 €"]] },
            { "type": "code", "id": "c1", "code": "curl https://api.alo.test/orders", "language": "bash" },
        ]))
    }

    fn message(content: &CampaignContent) -> CampaignMessage {
        render_campaign_message(&CampaignLetter {
            subject: "Prijzen vanaf maandag",
            preheader: Some("Olijfolie 12,50 €"),
            content,
        })
        .expect("a validated body assembles")
    }

    /// A quoted-printable decoder written from RFC 2045 §6.7, so the tests
    /// check the encoder against the standard rather than against itself.
    ///
    /// A line ending in a lone `=` is a soft break and joins the next one; an
    /// `=XX` triple is one byte; everything else is itself.
    fn decode_quoted_printable(encoded: &str) -> String {
        let lines: Vec<&str> = encoded.split("\r\n").collect();
        let mut out: Vec<u8> = Vec::with_capacity(encoded.len());
        for (index, line) in lines.iter().enumerate() {
            let (content, soft) = match line.strip_suffix('=') {
                Some(rest) => (rest, true),
                None => (*line, false),
            };
            let bytes = content.as_bytes();
            let mut at = 0;
            while at < bytes.len() {
                if bytes[at] == b'=' {
                    let hex = std::str::from_utf8(&bytes[at + 1..at + 3])
                        .expect("an =XX triple is ascii");
                    out.push(u8::from_str_radix(hex, 16).expect("an =XX triple is hex"));
                    at += 3;
                } else {
                    out.push(bytes[at]);
                    at += 1;
                }
            }
            if !soft && index + 1 < lines.len() {
                out.extend_from_slice(b"\r\n");
            }
        }
        String::from_utf8(out).expect("quoted-printable decodes to the utf-8 input")
    }

    /// The order is the mechanism, not a preference: a client shows the last
    /// part it understands, so the text part has to come first (RFC 2046
    /// §5.1.4).
    #[test]
    fn the_text_part_comes_first_because_that_is_what_makes_it_the_fallback() {
        let assembled = message(&letter());
        let text_at = assembled
            .body
            .find("Content-Type: text/plain")
            .expect("a text part");
        let html_at = assembled
            .body
            .find("Content-Type: text/html")
            .expect("an HTML part");
        assert!(
            text_at < html_at,
            "the HTML part must not shadow the text one: {}",
            assembled.body
        );
    }

    /// The item's own sentence: *a campaign with no text part is scored as spam
    /// by filters older than this project.* Every campaign has one, including
    /// the empty draft somebody sends by accident.
    #[test]
    fn every_campaign_carries_both_parts_including_an_empty_one() {
        for content in [letter(), CampaignContent::empty()] {
            let assembled = message(&content);
            assert!(
                assembled
                    .content_type
                    .starts_with("multipart/alternative; boundary=\""),
                "{}",
                assembled.content_type
            );
            assert_eq!(
                assembled
                    .body
                    .matches("Content-Type: text/plain; charset=utf-8")
                    .count(),
                1
            );
            assert_eq!(
                assembled
                    .body
                    .matches("Content-Type: text/html; charset=utf-8")
                    .count(),
                1
            );
            assert_eq!(
                assembled
                    .body
                    .matches("Content-Transfer-Encoding: quoted-printable")
                    .count(),
                2
            );
        }
    }

    /// The encoding is reversible, which is the only property that matters
    /// about it — checked with a decoder written from the RFC rather than by
    /// re-running the encoder.
    #[test]
    fn both_parts_decode_back_to_exactly_what_was_rendered() {
        let assembled = message(&letter());
        let boundary = assembled
            .content_type
            .split("boundary=\"")
            .nth(1)
            .and_then(|rest| rest.strip_suffix('"'))
            .expect("the boundary is in the content type");

        let mut parts: Vec<String> = Vec::new();
        for chunk in assembled.body.split(&format!("--{boundary}")).skip(1) {
            if chunk.starts_with("--") {
                break; // the closing delimiter
            }
            let payload = chunk
                .split_once("\r\n\r\n")
                .expect("a part has headers and a body")
                .1;
            let payload = payload.strip_suffix("\r\n").unwrap_or(payload);
            parts.push(decode_quoted_printable(payload));
        }
        assert_eq!(parts.len(), 2, "two alternatives");
        assert_eq!(parts[0].replace("\r\n", "\n"), assembled.text);
        assert_eq!(parts[1].replace("\r\n", "\n"), assembled.html);
        // And the letter really did carry the characters that make the
        // encoding necessary.
        assert!(assembled.text.contains("Genève"), "{}", assembled.text);
        assert!(assembled.body.contains("Gen=C3=A8ve"), "{}", assembled.body);
    }

    /// RFC 5322 §2.1.1 caps a line at 998 octets and RFC 2045 §6.7 caps a
    /// quoted-printable line at 76. A campaign paragraph may be 5 000
    /// characters, and the HTML part writes a block as one line of markup, so
    /// this is the ordinary case rather than an edge one.
    #[test]
    fn no_line_of_the_assembled_message_can_break_an_mta() {
        // The longest paragraph the model allows, near enough: 4 992 of 5 000
        // characters, in one block, as a real letter's sign-off never is and a
        // pasted press release routinely is.
        let long = "Beste klant, ".repeat(384);
        let content = body(json!([
            { "type": "paragraph", "id": "p1", "text": long },
            { "type": "code", "id": "c1", "code": "x".repeat(3_000), "language": "text" },
        ]));
        let assembled = message(&content);
        for line in assembled.body.split("\r\n") {
            assert!(
                line.len() <= CAMPAIGN_MIME_LINE_MAX,
                "a line of {} octets: {line:?}",
                line.len()
            );
        }
        // A bare LF is not a line ending in a message.
        assert!(
            !assembled.body.replace("\r\n", "").contains('\n'),
            "a bare line feed survived"
        );
    }

    /// Quoted-printable exists to make a body 7-bit clean; a part that was not
    /// would need an 8BITMIME path that a bulk send cannot assume it has.
    #[test]
    fn the_assembled_body_survives_a_seven_bit_path() {
        let assembled = message(&letter());
        assert!(assembled.is_seven_bit_clean(), "{}", assembled.body);
        assert!(assembled.body.is_ascii());
    }

    /// A trailing space is stripped by decoders that follow the RFC, so an
    /// encoder that leaves one has written a letter that arrives re-flowed.
    #[test]
    fn whitespace_never_sits_at_the_end_of_an_encoded_line() {
        // A line whose only content is spaces, and one that ends in a space
        // exactly where the soft break falls.
        let padded = format!("{} trailing ", "word ".repeat(20));
        let encoded = encode_quoted_printable(&format!("kept   \n{padded}\nend"));
        for line in encoded.split("\r\n") {
            let visible = line.strip_suffix('=').unwrap_or(line);
            assert!(
                !visible.ends_with(' ') && !visible.ends_with('\t'),
                "an encoded line ends in whitespace: {line:?}"
            );
        }
        // Only the *last* space of a run is encoded — the ones before it are
        // ordinary characters and encoding them would triple a letter's size
        // for nothing.
        assert!(encoded.contains("kept  =20\r\n"), "{encoded}");
        assert_eq!(
            decode_quoted_printable(&encoded).replace("\r\n", "\n"),
            format!("kept   \n{padded}\nend")
        );
    }

    /// An mbox archive on the path rewrites a line that begins `From ` into
    /// `>From `, which changes the letter. Three characters prevent it.
    #[test]
    fn a_line_that_begins_from_cannot_be_mangled_by_an_archive() {
        let encoded = encode_quoted_printable("From maandag geldt de nieuwe prijs\nFromage");
        assert!(encoded.starts_with("=46rom maandag"), "{encoded}");
        assert!(encoded.contains("\r\nFromage"), "{encoded}");
        assert_eq!(
            decode_quoted_printable(&encoded).replace("\r\n", "\n"),
            "From maandag geldt de nieuwe prijs\nFromage"
        );
    }

    /// The property the golden file rests on. A random boundary would make the
    /// assembled message unpinnable, and a golden is the only thing that
    /// catches an encoding regression before a recipient does.
    #[test]
    fn the_same_letter_assembles_to_the_same_bytes_every_time() {
        let content = letter();
        assert_eq!(message(&content), message(&content));
        // And a body rebuilt from its own stored JSON assembles identically.
        let stored = content.to_json().expect("serialises");
        let reloaded = CampaignContent::parse(&stored).expect("re-reads");
        assert_eq!(message(&reloaded), message(&content));
    }

    /// RFC 2046 §5.1.1: a boundary that occurs inside a part ends the message
    /// early. The encoder makes that impossible — `=_` is not producible — and
    /// the walk that would fix it anyway is tested here rather than left as an
    /// argument.
    #[test]
    fn a_boundary_never_appears_inside_the_parts_it_separates() {
        let hostile = body(json!([
            { "type": "paragraph", "id": "p1", "text": "=_alo_0123456789abcdef01234567" },
            { "type": "code", "id": "c1", "code": "--=_alo_ffffffffffffffffffffffff", "language": "text" },
        ]));
        let assembled = message(&hostile);
        let boundary = assembled
            .content_type
            .split("boundary=\"")
            .nth(1)
            .and_then(|rest| rest.strip_suffix('"'))
            .expect("the boundary is in the content type");
        // Three delimiters and nothing else: two openers and the closer.
        assert_eq!(
            assembled.body.matches(boundary).count(),
            3,
            "{}",
            assembled.body
        );

        // And the walk itself, exercised directly on a part that does contain
        // the candidate.
        let taken = unique_boundary("=_alo_beef", &["... =_alo_beef ..."]);
        assert_eq!(taken, "=_alo_beef_1");
        assert_eq!(
            unique_boundary("=_alo_beef", &["=_alo_beef", "=_alo_beef_1"]),
            "=_alo_beef_2"
        );
    }

    /// A `boundary` parameter that has to be folded across two lines is where
    /// the least forgiving MTAs on the path stop agreeing about what the
    /// message is, so the header it goes in fits one line by construction.
    #[test]
    fn the_content_type_fits_a_header_line_nobody_has_to_fold() {
        for content in [letter(), CampaignContent::empty()] {
            let header = format!("Content-Type: {}", message(&content).content_type);
            assert!(
                header.len() <= CAMPAIGN_HEADER_LINE_MAX,
                "a header of {} octets: {header}",
                header.len()
            );
        }
    }

    /// Two different letters do not share a boundary, and the same letter's
    /// boundary is not a name anybody chose — it is the digest of what it
    /// separates.
    #[test]
    fn the_boundary_is_derived_from_the_parts_rather_than_from_a_name() {
        let other = body(json!([
            { "type": "paragraph", "id": "p1", "text": "Iets anders" },
        ]));
        assert_ne!(
            message(&letter()).content_type,
            message(&other).content_type
        );
        assert!(
            message(&other).content_type.contains(BOUNDARY_PREFIX),
            "the boundary must be recognisably ours"
        );
    }

    /// One gate, two parts: a letter can never be assembled with one legal half
    /// and one illegal one.
    #[test]
    fn a_body_that_never_passed_the_write_gate_is_refused_rather_than_assembled() {
        let ragged = CampaignContent {
            schema_version: 1,
            blocks: vec![CampaignBlock::Table(TableBlock {
                id: "t1".to_owned(),
                rows: vec![vec!["a".to_owned(), "b".to_owned()], vec!["a".to_owned()]],
            })],
        };
        match render_campaign_message(&CampaignLetter {
            subject: "s",
            preheader: None,
            content: &ragged,
        }) {
            Err(StoreError::Validation(detail)) => assert!(detail.contains("columns"), "{detail}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// The parts are kept beside the entity because C3.6's preview shows
    /// exactly them; re-rendering there would be a second opinion about one
    /// letter.
    #[test]
    fn the_message_carries_the_parts_it_encoded() {
        let content = letter();
        let assembled = message(&content);
        assert_eq!(
            assembled.text,
            render_campaign_text(&content).expect("renders")
        );
        assert_eq!(
            assembled.html,
            render_campaign_html(&CampaignLetter {
                subject: "Prijzen vanaf maandag",
                preheader: Some("Olijfolie 12,50 €"),
                content: &content,
            })
            .expect("renders")
        );
        // The preheader reaches the HTML part and deliberately not the text
        // one — there is nothing to hide behind in a text part.
        assert!(assembled.html.contains("Olijfolie 12,50 €"));
        assert!(!assembled.text.contains("Olijfolie 12,50 €"));
    }
}
