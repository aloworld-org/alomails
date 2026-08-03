//! DKIM canonicalization (RFC 6376 §3.4): `simple` and `relaxed` for
//! both the header and the body. Canonicalization normalizes the bytes
//! that are hashed so benign transit mutations do not break the
//! signature; getting it exactly right is the whole game.

/// The canonicalization algorithm for one half (header or body).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Canon {
    /// `simple`: essentially verbatim (§3.4.1, §3.4.3).
    Simple,
    /// `relaxed`: whitespace/case normalization (§3.4.2, §3.4.4).
    Relaxed,
}

impl Canon {
    /// Parses the `c=` half token.
    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "simple" => Some(Self::Simple),
            "relaxed" => Some(Self::Relaxed),
            _ => None,
        }
    }
}

/// Canonicalizes one header (`name`, `raw_value` as it appeared on the
/// wire including folding but excluding the trailing CRLF) and returns
/// the bytes to hash, terminated with CRLF.
///
/// `relaxed` (§3.4.2): lowercase the name; unfold; collapse runs of
/// WSP to a single SP; strip trailing WSP; remove WSP around the colon.
/// `simple` (§3.4.1): the header verbatim plus CRLF.
pub fn header(canon: Canon, name: &str, raw_value: &str) -> Vec<u8> {
    match canon {
        Canon::Simple => {
            let mut out = Vec::with_capacity(name.len() + raw_value.len() + 4);
            out.extend_from_slice(name.as_bytes());
            out.push(b':');
            out.extend_from_slice(raw_value.as_bytes());
            out.extend_from_slice(b"\r\n");
            out
        }
        Canon::Relaxed => {
            let name = name.trim().to_ascii_lowercase();
            let value = relax_value(raw_value);
            let mut out = Vec::with_capacity(name.len() + value.len() + 3);
            out.extend_from_slice(name.as_bytes());
            out.push(b':');
            out.extend_from_slice(value.as_bytes());
            out.extend_from_slice(b"\r\n");
            out
        }
    }
}

/// Relaxed value normalization: unfold (remove CRLF), collapse WSP runs
/// to one SP, and trim leading/trailing WSP (§3.4.2).
fn relax_value(raw: &str) -> String {
    // Unfold: a folded header has CRLF followed by WSP; removing CRLF
    // leaves the WSP, which the collapse step handles.
    let unfolded: String = raw.chars().filter(|&c| c != '\r' && c != '\n').collect();
    let mut out = String::with_capacity(unfolded.len());
    let mut in_wsp = false;
    for c in unfolded.chars() {
        if c == ' ' || c == '\t' {
            in_wsp = true;
        } else {
            if in_wsp && !out.is_empty() {
                out.push(' ');
            }
            in_wsp = false;
            out.push(c);
        }
    }
    out
}

/// Canonicalizes the message body and returns the bytes to hash.
/// `simple` (§3.4.3): remove trailing empty lines; a body of only
/// empty lines becomes a single CRLF. `relaxed` (§3.4.4): reduce WSP
/// runs within a line to one SP, strip trailing WSP per line, remove
/// trailing empty lines. Both then ensure the body ends with CRLF.
///
/// `body` is the raw message body (after the header/body separator),
/// with CRLF line endings.
pub fn body(canon: Canon, body: &[u8]) -> Vec<u8> {
    // Split into lines on CRLF, preserving that the final segment may
    // lack a terminator.
    let text = body;
    let mut lines: Vec<&[u8]> = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i + 1 < text.len() {
        if text[i] == b'\r' && text[i + 1] == b'\n' {
            lines.push(&text[start..i]);
            i += 2;
            start = i;
        } else {
            i += 1;
        }
    }
    // Trailing bytes with no final CRLF form a last (unterminated) line.
    let tail = &text[start..];

    let mut canon_lines: Vec<Vec<u8>> = lines
        .iter()
        .map(|line| canon_body_line(canon, line))
        .collect();
    if !tail.is_empty() {
        canon_lines.push(canon_body_line(canon, tail));
    }

    // Remove trailing empty lines (both algorithms, §3.4.3/§3.4.4).
    while matches!(canon_lines.last(), Some(l) if l.is_empty()) {
        canon_lines.pop();
    }

    let mut out = Vec::with_capacity(body.len() + 2);
    for line in &canon_lines {
        out.extend_from_slice(line);
        out.extend_from_slice(b"\r\n");
    }
    // A completely empty body canonicalizes to a single CRLF (§3.4.3).
    if out.is_empty() {
        out.extend_from_slice(b"\r\n");
    }
    out
}

fn canon_body_line(canon: Canon, line: &[u8]) -> Vec<u8> {
    match canon {
        Canon::Simple => line.to_vec(),
        Canon::Relaxed => {
            // Collapse WSP runs to one SP, strip trailing WSP.
            let mut out = Vec::with_capacity(line.len());
            let mut in_wsp = false;
            for &b in line {
                if b == b' ' || b == b'\t' {
                    in_wsp = true;
                } else {
                    if in_wsp {
                        out.push(b' ');
                    }
                    in_wsp = false;
                    out.push(b);
                }
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relaxed_header_examples_rfc6376_3_4_5() {
        // A:X → a:X ; B : Y\t\tZ (folded) → b:Y Z
        assert_eq!(header(Canon::Relaxed, "A", "X"), b"a:X\r\n");
        assert_eq!(
            header(Canon::Relaxed, "B", " Y\t\r\n\tZ  "),
            b"b:Y Z\r\n".to_vec()
        );
    }

    #[test]
    fn simple_header_is_verbatim() {
        assert_eq!(header(Canon::Simple, "A", " X "), b"A: X \r\n".to_vec());
    }

    #[test]
    fn relaxed_body_collapses_and_trims() {
        // " C \r\nD \t E\r\n\r\n\r\n" → "C\r\nD E\r\n" (trailing blanks gone,
        // internal WSP collapsed, trailing WSP stripped).
        let out = body(Canon::Relaxed, b" C \r\nD \t E\r\n\r\n\r\n");
        assert_eq!(out, b" C\r\nD E\r\n".to_vec());
    }

    #[test]
    fn simple_empty_body_is_single_crlf() {
        assert_eq!(body(Canon::Simple, b""), b"\r\n".to_vec());
        assert_eq!(body(Canon::Simple, b"\r\n\r\n"), b"\r\n".to_vec());
    }

    #[test]
    fn simple_body_removes_trailing_empty_lines_only() {
        assert_eq!(
            body(Canon::Simple, b"line1\r\nline2\r\n\r\n"),
            b"line1\r\nline2\r\n".to_vec()
        );
    }
}
