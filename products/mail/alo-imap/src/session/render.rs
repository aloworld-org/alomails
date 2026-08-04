//! FETCH item parsing and per-message response rendering (RFC 9051 §7.5.2).

use alo_store::MessageId;
use time::OffsetDateTime;
use time::format_description::BorrowedFormatItem;
use time::macros::format_description;

use crate::fetch::{self, Section};
use crate::flags;

/// One requested FETCH data item.
#[derive(Debug, Clone)]
pub(super) enum FetchItem {
    Flags,
    Uid,
    InternalDate,
    Rfc822Size,
    Envelope,
    BodyStructure,
    /// `BODY` — the non-extensible body structure.
    BodyNonExtensible,
    Rfc822,
    Rfc822Header,
    Rfc822Text,
    /// `BODY[...]` / `BODY.PEEK[...]` with an optional `<offset.count>`.
    Section {
        peek: bool,
        section: Section,
        partial: Option<(usize, usize)>,
        /// The section text as the client wrote it, echoed in the response.
        label: String,
    },
}

/// What a message needs fetched to satisfy an item set.
pub(super) struct Needs {
    pub bytes: bool,
    pub mark_seen: bool,
}

/// Parses the FETCH item list (after the sequence set). Accepts a single
/// item, a parenthesized list, or a macro (ALL/FAST/FULL).
pub(super) fn parse_items(input: &str) -> Option<Vec<FetchItem>> {
    let trimmed = input.trim();
    let inner = trimmed
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(trimmed);
    let upper = inner.trim().to_ascii_uppercase();
    match upper.as_str() {
        "ALL" => {
            return Some(vec![
                FetchItem::Flags,
                FetchItem::InternalDate,
                FetchItem::Rfc822Size,
                FetchItem::Envelope,
            ]);
        }
        "FAST" => {
            return Some(vec![
                FetchItem::Flags,
                FetchItem::InternalDate,
                FetchItem::Rfc822Size,
            ]);
        }
        "FULL" => {
            return Some(vec![
                FetchItem::Flags,
                FetchItem::InternalDate,
                FetchItem::Rfc822Size,
                FetchItem::Envelope,
                FetchItem::BodyNonExtensible,
            ]);
        }
        _ => {}
    }
    let mut items = Vec::new();
    for tok in split_items(inner) {
        items.push(parse_one(&tok)?);
    }
    if items.is_empty() { None } else { Some(items) }
}

/// Splits an item list on spaces that are not inside `[...]` or `(...)`.
fn split_items(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    for c in input.chars() {
        match c {
            '[' | '(' => {
                depth += 1;
                cur.push(c);
            }
            ']' | ')' => {
                depth -= 1;
                cur.push(c);
            }
            ' ' if depth == 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn parse_one(tok: &str) -> Option<FetchItem> {
    let upper = tok.to_ascii_uppercase();
    Some(match upper.as_str() {
        "FLAGS" => FetchItem::Flags,
        "UID" => FetchItem::Uid,
        "INTERNALDATE" => FetchItem::InternalDate,
        "RFC822.SIZE" => FetchItem::Rfc822Size,
        "ENVELOPE" => FetchItem::Envelope,
        "BODYSTRUCTURE" => FetchItem::BodyStructure,
        "BODY" => FetchItem::BodyNonExtensible,
        "RFC822" => FetchItem::Rfc822,
        "RFC822.HEADER" => FetchItem::Rfc822Header,
        "RFC822.TEXT" => FetchItem::Rfc822Text,
        _ => {
            // BODY[...] / BODY.PEEK[...] possibly with <partial>.
            let (peek, rest) = if let Some(r) = upper.strip_prefix("BODY.PEEK") {
                (true, r)
            } else {
                (false, upper.strip_prefix("BODY")?)
            };
            // `rest` (uppercased) begins with `[`. Use the original-case tok
            // for HEADER.FIELDS names.
            let orig_rest = &tok[tok.len() - rest.len()..];
            let lb = orig_rest.find('[')?;
            let rb = orig_rest.rfind(']')?;
            let section_str = &orig_rest[lb + 1..rb];
            let after = &orig_rest[rb + 1..];
            let partial = parse_partial(after)?;
            let section = parse_section(section_str)?;
            let label = format!("[{section_str}]");
            FetchItem::Section {
                peek,
                section,
                partial,
                label,
            }
        }
    })
}

fn parse_partial(after: &str) -> Option<Option<(usize, usize)>> {
    let after = after.trim();
    if after.is_empty() {
        return Some(None);
    }
    let inner = after.strip_prefix('<')?.strip_suffix('>')?;
    let (off, count) = inner.split_once('.')?;
    Some(Some((off.parse().ok()?, count.parse().ok()?)))
}

fn parse_section(s: &str) -> Option<Section> {
    let up = s.trim().to_ascii_uppercase();
    if up.is_empty() {
        return Some(Section::Full);
    }
    if up == "HEADER" {
        return Some(Section::Header);
    }
    if up == "TEXT" {
        return Some(Section::Text);
    }
    if let Some(rest) = up.strip_prefix("HEADER.FIELDS.NOT") {
        return Some(Section::HeaderFieldsNot(field_names(rest, s)));
    }
    if let Some(rest) = up.strip_prefix("HEADER.FIELDS") {
        return Some(Section::HeaderFields(field_names(rest, s)));
    }
    // Numbered part: digits separated by dots, optional trailing keyword.
    let parts: Vec<&str> = up.split('.').collect();
    let mut path = Vec::new();
    let mut suffix = None;
    for (i, seg) in parts.iter().enumerate() {
        if let Ok(n) = seg.parse::<usize>() {
            path.push(n);
        } else if i == parts.len() - 1 {
            suffix = Some(*seg);
        } else {
            return None;
        }
    }
    if path.is_empty() {
        return None;
    }
    Some(match suffix {
        None => Section::Part(path),
        Some("MIME") => Section::PartMime(path),
        Some("HEADER") => Section::PartHeader(path),
        Some("TEXT") => Section::PartText(path),
        _ => return None,
    })
}

/// Extracts the field-name list from a `HEADER.FIELDS (a b c)` section,
/// using the original-case source for the names.
fn field_names(_upper_rest: &str, orig: &str) -> Vec<String> {
    let start = orig.find('(');
    let end = orig.rfind(')');
    match (start, end) {
        (Some(a), Some(b)) if a < b => orig[a + 1..b]
            .split_whitespace()
            .map(|s| s.to_owned())
            .collect(),
        _ => Vec::new(),
    }
}

/// Computes whether an item set needs the raw bytes and/or marks `\Seen`.
pub(super) fn needs(items: &[FetchItem]) -> Needs {
    let mut bytes = false;
    let mut mark_seen = false;
    for it in items {
        match it {
            FetchItem::Envelope
            | FetchItem::BodyStructure
            | FetchItem::BodyNonExtensible
            | FetchItem::Rfc822
            | FetchItem::Rfc822Header
            | FetchItem::Rfc822Text => bytes = true,
            FetchItem::Section { peek, .. } => {
                bytes = true;
                if !peek {
                    mark_seen = true;
                }
            }
            _ => {}
        }
    }
    Needs { bytes, mark_seen }
}

const INTERNALDATE_FMT: &[BorrowedFormatItem<'_>] = format_description!(
    "[day padding:zero]-[month repr:short]-[year] [hour]:[minute]:[second] [offset_hour sign:mandatory][offset_minute]"
);

/// Renders one message's FETCH response line (a byte buffer, since body
/// sections are literals). `_message` anchors the row to its id for logs.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_fetch(
    seq: usize,
    uid: i64,
    flags_kw: &[String],
    internaldate: OffsetDateTime,
    size: i64,
    raw: Option<&[u8]>,
    items: &[FetchItem],
    force_uid: bool,
    _message: &MessageId,
) -> Vec<u8> {
    let mut parts: Vec<Vec<u8>> = Vec::new();
    let mut saw_uid = false;
    for it in items {
        match it {
            FetchItem::Flags => {
                parts.push(format!("FLAGS ({})", flags::render_flags(flags_kw)).into_bytes());
            }
            FetchItem::Uid => {
                saw_uid = true;
                parts.push(format!("UID {uid}").into_bytes());
            }
            FetchItem::InternalDate => {
                let s = internaldate.format(INTERNALDATE_FMT).unwrap_or_default();
                parts.push(format!("INTERNALDATE \"{s}\"").into_bytes());
            }
            FetchItem::Rfc822Size => parts.push(format!("RFC822.SIZE {size}").into_bytes()),
            FetchItem::Envelope => {
                let hdr = raw.map(|r| fetch::split_header_body(r).0).unwrap_or(&[]);
                parts.push(format!("ENVELOPE {}", fetch::envelope(hdr)).into_bytes());
            }
            FetchItem::BodyStructure => {
                let bs = raw
                    .map(|r| fetch::body_structure(r, true))
                    .unwrap_or_else(|| "NIL".into());
                parts.push(format!("BODYSTRUCTURE {bs}").into_bytes());
            }
            FetchItem::BodyNonExtensible => {
                let bs = raw
                    .map(|r| fetch::body_structure(r, false))
                    .unwrap_or_else(|| "NIL".into());
                parts.push(format!("BODY {bs}").into_bytes());
            }
            FetchItem::Rfc822 => push_literal(&mut parts, "RFC822", raw.unwrap_or(&[])),
            FetchItem::Rfc822Header => {
                let b = raw
                    .and_then(|r| fetch::section_bytes(r, &Section::Header))
                    .unwrap_or_default();
                push_literal(&mut parts, "RFC822.HEADER", &b);
            }
            FetchItem::Rfc822Text => {
                let b = raw
                    .and_then(|r| fetch::section_bytes(r, &Section::Text))
                    .unwrap_or_default();
                push_literal(&mut parts, "RFC822.TEXT", &b);
            }
            FetchItem::Section {
                section,
                partial,
                label,
                ..
            } => {
                let bytes = raw
                    .and_then(|r| fetch::section_bytes(r, section))
                    .unwrap_or_default();
                let (bytes, tag) = match partial {
                    Some((off, count)) => (
                        fetch::apply_partial(bytes, Some((*off, *count))),
                        format!("BODY{label}<{off}>"),
                    ),
                    None => (bytes, format!("BODY{label}")),
                };
                push_literal(&mut parts, &tag, &bytes);
            }
        }
    }
    if force_uid && !saw_uid {
        parts.push(format!("UID {uid}").into_bytes());
    }
    let mut out = format!("* {seq} FETCH (").into_bytes();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            out.push(b' ');
        }
        out.extend_from_slice(part);
    }
    out.extend_from_slice(b")\r\n");
    out
}

/// Appends an item whose value is a literal: `NAME {n}\r\n<bytes>`.
fn push_literal(parts: &mut Vec<Vec<u8>>, name: &str, bytes: &[u8]) {
    let mut p = format!("{name} {{{}}}\r\n", bytes.len()).into_bytes();
    p.extend_from_slice(bytes);
    parts.push(p);
}
