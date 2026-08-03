//! RFC 2047 "encoded-word" decoding for display header values — the `Subject`
//! and the display-name parts of `From`/`To`. An encoded-word
//! `=?charset?B|Q?text?=` is decoded to UTF-8 (any charset, via `encoding_rs`);
//! whitespace that separates two *adjacent* encoded-words is dropped (§6.2),
//! while whitespace between an encoded-word and ordinary text is kept.
//!
//! Best-effort and infallible: a malformed encoded-word is passed through
//! verbatim, matching the header parser this serves. The addr-spec of an
//! address (inside `<…>`) is never an encoded-word, so it is untouched.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;

/// Decodes every RFC 2047 encoded-word in a (already header-unfolded) value.
pub fn decode(input: &str) -> String {
    if !input.contains("=?") {
        return input.to_owned(); // fast path: nothing to decode
    }
    let mut out = String::with_capacity(input.len());
    // Whitespace held back so it can be dropped if it sits between two
    // adjacent encoded-words, or emitted otherwise.
    let mut pending_ws = String::new();
    let mut prev_encoded = false;
    let mut rest = input;

    while !rest.is_empty() {
        if let Some((decoded, consumed)) = try_encoded_word(rest) {
            if !prev_encoded {
                out.push_str(&pending_ws);
            }
            pending_ws.clear();
            out.push_str(&decoded);
            prev_encoded = true;
            rest = &rest[consumed..];
        } else {
            let Some(ch) = rest.chars().next() else { break };
            if ch.is_whitespace() {
                pending_ws.push(ch);
            } else {
                out.push_str(&pending_ws);
                pending_ws.clear();
                out.push(ch);
                prev_encoded = false;
            }
            rest = &rest[ch.len_utf8()..];
        }
    }
    out.push_str(&pending_ws);
    out
}

/// Attempts to read one encoded-word at the start of `s`. Returns the decoded
/// text and the number of bytes consumed, or `None` if `s` does not begin with
/// a well-formed encoded-word.
fn try_encoded_word(s: &str) -> Option<(String, usize)> {
    let body = s.strip_prefix("=?")?;
    let q1 = body.find('?')?;
    let charset = &body[..q1];
    let after = &body[q1 + 1..];
    // Encoding is a single ASCII char immediately followed by '?'.
    let enc = after.as_bytes().first().copied()?;
    if after.as_bytes().get(1) != Some(&b'?') {
        return None;
    }
    let text_area = &after[2..]; // safe: byte 1 is the ASCII '?'
    let end = text_area.find("?=")?;
    let text = &text_area[..end];
    // An encoded-word contains no whitespace and has a non-empty charset.
    if charset.is_empty() || text.chars().any(char::is_whitespace) {
        return None;
    }
    let raw = match enc {
        b'B' | b'b' => B64.decode(text).ok()?,
        b'Q' | b'q' => decode_q(text),
        _ => return None,
    };
    let decoded = decode_charset(charset, &raw);
    // "=?" + charset + "?" + enc + "?" + text + "?="
    let consumed = 2 + q1 + 1 + 2 + end + 2;
    Some((decoded, consumed))
}

/// RFC 2047 "Q" decoding: `_` is a space, `=XX` is a hex octet, else literal.
fn decode_q(text: &str) -> Vec<u8> {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'_' => {
                out.push(b' ');
                i += 1;
            }
            b'=' if i + 2 < b.len() => match (hex(b[i + 1]), hex(b[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push((h << 4) | l);
                    i += 3;
                }
                _ => {
                    out.push(b'=');
                    i += 1;
                }
            },
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    out
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

/// Decodes bytes in the named charset to a Rust string. Unknown labels fall
/// back to UTF-8 (lossy). `encoding_rs::Encoding::for_label` accepts the usual
/// aliases case-insensitively (utf-8, iso-8859-1/15, windows-1252, …).
fn decode_charset(charset: &str, bytes: &[u8]) -> String {
    let enc =
        encoding_rs::Encoding::for_label(charset.trim().as_bytes()).unwrap_or(encoding_rs::UTF_8);
    let (text, _, _) = enc.decode(bytes);
    text.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_untouched() {
        assert_eq!(decode("Re: Quarterly plan"), "Re: Quarterly plan");
    }

    #[test]
    fn base64_utf8() {
        // "café" = 63 61 66 C3 A9 → base64 Y2Fmw6k=
        assert_eq!(decode("=?UTF-8?B?Y2Fmw6k=?="), "café");
    }

    #[test]
    fn quoted_printable_latin1() {
        // 0xE9 is "é" in ISO-8859-1; "_" is a space.
        assert_eq!(decode("=?ISO-8859-1?Q?caf=E9_pr=EAt?="), "café prêt");
    }

    #[test]
    fn windows_1252_smart_quote() {
        // 0x92 is a right single quote in windows-1252 (U+2019).
        assert_eq!(decode("=?windows-1252?Q?it=92s?="), "it\u{2019}s");
    }

    #[test]
    fn adjacent_encoded_words_drop_separating_space() {
        // Two encoded-words separated by whitespace concatenate (§6.2).
        assert_eq!(decode("=?UTF-8?B?Y2Fm?= =?UTF-8?B?w6k=?="), "café");
    }

    #[test]
    fn mixed_text_and_encoded_word_keeps_spaces() {
        assert_eq!(decode("Fwd: =?UTF-8?B?Y2Fmw6k=?= today"), "Fwd: café today");
    }

    #[test]
    fn display_name_in_address_decodes_addr_untouched() {
        // "Hélène" = 48 C3A9 6C C3A8 6E 65 → base64 SMOpbMOobmU=
        assert_eq!(
            decode("=?UTF-8?B?SMOpbMOobmU=?= <helene@proceq.eu>"),
            "Hélène <helene@proceq.eu>"
        );
    }

    #[test]
    fn malformed_encoded_word_passes_through() {
        assert_eq!(
            decode("=?UTF-8?B?not valid base64 with spaces?="),
            "=?UTF-8?B?not valid base64 with spaces?="
        );
        assert_eq!(decode("=?UTF-8?"), "=?UTF-8?");
        assert_eq!(decode("plain =? not an ew"), "plain =? not an ew");
    }

    #[test]
    fn unknown_charset_falls_back_to_utf8() {
        // Bytes are valid UTF-8 for "café"; an unknown label still yields it.
        assert_eq!(decode("=?x-unknown?B?Y2Fmw6k=?="), "café");
    }
}
