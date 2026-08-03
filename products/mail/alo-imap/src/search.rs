//! SEARCH key parsing and evaluation (RFC 9051 §6.4.4). A criteria list
//! parses into a [`SearchKey`] tree; each candidate message is tested with
//! its cheap metadata ([`ImapSearchRow`]) plus, only when a BODY/TEXT/
//! header key demands it, its raw bytes. Charset is limited to
//! US-ASCII/UTF-8 (declared in the design note).

use alo_store::ImapSearchRow;
use time::{Date, Month};

use crate::flags;

/// A parsed SEARCH key tree.
#[derive(Debug, Clone)]
pub enum SearchKey {
    /// Always matches.
    All,
    /// Message sequence set (resolved against the view by the caller).
    SeqSet(String),
    /// `UID` set.
    UidSet(String),
    /// Message bears keyword.
    Keyword(String),
    /// Message lacks keyword.
    Unkeyword(String),
    /// Substring in `From`.
    From(String),
    /// Substring in `To`.
    To(String),
    /// Substring in `Subject`.
    Subject(String),
    /// Substring anywhere in the header block (also CC/BCC/HEADER).
    HeaderContains(String),
    /// Substring in a specific header field.
    Header(String, String),
    /// Substring in the body.
    Body(String),
    /// Substring anywhere (header or body).
    Text(String),
    /// INTERNALDATE strictly before.
    Before(Date),
    /// INTERNALDATE on the day.
    On(Date),
    /// INTERNALDATE on/after.
    Since(Date),
    /// `Date:` header before/on/since.
    SentBefore(Date),
    /// See [`SearchKey::SentBefore`].
    SentOn(Date),
    /// See [`SearchKey::SentBefore`].
    SentSince(Date),
    /// Larger than N octets.
    Larger(i64),
    /// Smaller than N octets.
    Smaller(i64),
    /// Negation.
    Not(Box<SearchKey>),
    /// Disjunction.
    Or(Box<SearchKey>, Box<SearchKey>),
    /// Conjunction (an implicit list, or a parenthesized one).
    And(Vec<SearchKey>),
}

impl SearchKey {
    /// Whether evaluating this tree needs the raw message bytes.
    pub fn needs_bytes(&self) -> bool {
        match self {
            SearchKey::Body(_)
            | SearchKey::Text(_)
            | SearchKey::HeaderContains(_)
            | SearchKey::Header(_, _) => true,
            SearchKey::Not(k) => k.needs_bytes(),
            SearchKey::Or(a, b) => a.needs_bytes() || b.needs_bytes(),
            SearchKey::And(v) => v.iter().any(SearchKey::needs_bytes),
            _ => false,
        }
    }
}

/// Parses a whitespace/paren-tokenized criteria list into an ANDed
/// [`SearchKey`]. Returns `None` on a syntax error.
pub fn parse(tokens: &[String]) -> Option<SearchKey> {
    let mut pos = 0;
    let mut keys = Vec::new();
    while pos < tokens.len() {
        keys.push(parse_key(tokens, &mut pos)?);
    }
    if keys.is_empty() {
        Some(SearchKey::All)
    } else if keys.len() == 1 {
        Some(keys.pop()?)
    } else {
        Some(SearchKey::And(keys))
    }
}

fn parse_key(tokens: &[String], pos: &mut usize) -> Option<SearchKey> {
    let tok = tokens.get(*pos)?.to_ascii_uppercase();
    *pos += 1;
    let arg = |pos: &mut usize| -> Option<String> {
        let a = tokens.get(*pos)?.clone();
        *pos += 1;
        Some(a)
    };
    Some(match tok.as_str() {
        "ALL" => SearchKey::All,
        "ANSWERED" => SearchKey::Keyword("$answered".into()),
        "UNANSWERED" => SearchKey::Unkeyword("$answered".into()),
        "DELETED" => SearchKey::Keyword(flags::DELETED.into()),
        "UNDELETED" => SearchKey::Unkeyword(flags::DELETED.into()),
        "DRAFT" => SearchKey::Keyword("$draft".into()),
        "UNDRAFT" => SearchKey::Unkeyword("$draft".into()),
        "FLAGGED" => SearchKey::Keyword("$flagged".into()),
        "UNFLAGGED" => SearchKey::Unkeyword("$flagged".into()),
        "SEEN" => SearchKey::Keyword("$seen".into()),
        "UNSEEN" => SearchKey::Unkeyword("$seen".into()),
        // We never set \Recent, so NEW/RECENT match nothing, OLD matches all.
        "NEW" | "RECENT" => SearchKey::Not(Box::new(SearchKey::All)),
        "OLD" => SearchKey::All,
        "KEYWORD" => SearchKey::Keyword(flags::imap_to_keyword(&arg(pos)?)?),
        "UNKEYWORD" => SearchKey::Unkeyword(flags::imap_to_keyword(&arg(pos)?)?),
        "FROM" => SearchKey::From(arg(pos)?),
        "TO" => SearchKey::To(arg(pos)?),
        "CC" | "BCC" => SearchKey::HeaderContains(arg(pos)?),
        "SUBJECT" => SearchKey::Subject(arg(pos)?),
        "BODY" => SearchKey::Body(arg(pos)?),
        "TEXT" => SearchKey::Text(arg(pos)?),
        "HEADER" => {
            let name = arg(pos)?;
            let val = arg(pos)?;
            SearchKey::Header(name, val)
        }
        "BEFORE" => SearchKey::Before(parse_date(&arg(pos)?)?),
        "ON" => SearchKey::On(parse_date(&arg(pos)?)?),
        "SINCE" => SearchKey::Since(parse_date(&arg(pos)?)?),
        "SENTBEFORE" => SearchKey::SentBefore(parse_date(&arg(pos)?)?),
        "SENTON" => SearchKey::SentOn(parse_date(&arg(pos)?)?),
        "SENTSINCE" => SearchKey::SentSince(parse_date(&arg(pos)?)?),
        "LARGER" => SearchKey::Larger(arg(pos)?.parse().ok()?),
        "SMALLER" => SearchKey::Smaller(arg(pos)?.parse().ok()?),
        "UID" => SearchKey::UidSet(arg(pos)?),
        "NOT" => SearchKey::Not(Box::new(parse_key(tokens, pos)?)),
        "OR" => {
            let a = parse_key(tokens, pos)?;
            let b = parse_key(tokens, pos)?;
            SearchKey::Or(Box::new(a), Box::new(b))
        }
        "(" => {
            let mut inner = Vec::new();
            while tokens.get(*pos).map(String::as_str) != Some(")") {
                inner.push(parse_key(tokens, pos)?);
            }
            *pos += 1; // consume ")"
            SearchKey::And(inner)
        }
        // A bare token is a sequence set.
        other => SearchKey::SeqSet(other.to_owned()),
    })
}

/// Evaluates a key against one message. `seq` is the message's 1-based
/// sequence number in the current view; `bytes` is its raw content when
/// available (needed by BODY/TEXT/HEADER keys). Sequence/UID sets are
/// pre-resolved by the caller into `seq_hit`/`uid_hit` closures.
#[allow(clippy::too_many_arguments)]
pub fn eval(
    key: &SearchKey,
    row: &ImapSearchRow,
    seq: u64,
    bytes: Option<&[u8]>,
    seq_match: &dyn Fn(&str, u64) -> bool,
    uid_match: &dyn Fn(&str, i64) -> bool,
) -> bool {
    let ci = |hay: &str, needle: &str| hay.to_lowercase().contains(&needle.to_lowercase());
    match key {
        SearchKey::All => true,
        SearchKey::SeqSet(set) => seq_match(set, seq),
        SearchKey::UidSet(set) => uid_match(set, row.uid),
        SearchKey::Keyword(k) => row.flags.iter().any(|f| f == k),
        SearchKey::Unkeyword(k) => !row.flags.iter().any(|f| f == k),
        SearchKey::From(s) => ci(&row.from_addr, s),
        SearchKey::To(s) => ci(&row.to_addrs, s),
        SearchKey::Subject(s) => ci(&row.subject, s),
        SearchKey::HeaderContains(s) => bytes.is_some_and(|b| header_contains(b, s)),
        SearchKey::Header(name, val) => bytes.is_some_and(|b| header_field_contains(b, name, val)),
        SearchKey::Body(s) => bytes.is_some_and(|b| body_contains(b, s)),
        SearchKey::Text(s) => bytes.is_some_and(|b| {
            String::from_utf8_lossy(b)
                .to_lowercase()
                .contains(&s.to_lowercase())
        }),
        SearchKey::Before(d) => row.received_at.date() < *d,
        SearchKey::On(d) => row.received_at.date() == *d,
        SearchKey::Since(d) => row.received_at.date() >= *d,
        SearchKey::SentBefore(d) => row.sent_at.map(|t| t.date() < *d).unwrap_or(false),
        SearchKey::SentOn(d) => row.sent_at.map(|t| t.date() == *d).unwrap_or(false),
        SearchKey::SentSince(d) => row.sent_at.map(|t| t.date() >= *d).unwrap_or(false),
        SearchKey::Larger(n) => row.size > *n,
        SearchKey::Smaller(n) => row.size < *n,
        SearchKey::Not(k) => !eval(k, row, seq, bytes, seq_match, uid_match),
        SearchKey::Or(a, b) => {
            eval(a, row, seq, bytes, seq_match, uid_match)
                || eval(b, row, seq, bytes, seq_match, uid_match)
        }
        SearchKey::And(v) => v
            .iter()
            .all(|k| eval(k, row, seq, bytes, seq_match, uid_match)),
    }
}

fn header_contains(raw: &[u8], needle: &str) -> bool {
    let (h, _, _) = crate::fetch::split_header_body(raw);
    String::from_utf8_lossy(h)
        .to_lowercase()
        .contains(&needle.to_lowercase())
}

fn body_contains(raw: &[u8], needle: &str) -> bool {
    let (_, b, _) = crate::fetch::split_header_body(raw);
    String::from_utf8_lossy(b)
        .to_lowercase()
        .contains(&needle.to_lowercase())
}

fn header_field_contains(raw: &[u8], name: &str, needle: &str) -> bool {
    let (h, _, _) = crate::fetch::split_header_body(raw);
    for f in crate::fetch::parse_fields(h) {
        if f.name.eq_ignore_ascii_case(name)
            && f.value.to_lowercase().contains(&needle.to_lowercase())
        {
            return true;
        }
    }
    false
}

/// Parses an IMAP date `d-Mon-yyyy` (e.g. `1-Feb-2024`).
fn parse_date(s: &str) -> Option<Date> {
    let s = s.trim_matches('"');
    let mut it = s.split('-');
    let day: u8 = it.next()?.parse().ok()?;
    let month = month_from(it.next()?)?;
    let year: i32 = it.next()?.parse().ok()?;
    Date::from_calendar_date(year, month, day).ok()
}

fn month_from(m: &str) -> Option<Month> {
    Some(match m.to_ascii_lowercase().as_str() {
        "jan" => Month::January,
        "feb" => Month::February,
        "mar" => Month::March,
        "apr" => Month::April,
        "may" => Month::May,
        "jun" => Month::June,
        "jul" => Month::July,
        "aug" => Month::August,
        "sep" => Month::September,
        "oct" => Month::October,
        "nov" => Month::November,
        "dec" => Month::December,
        _ => return None,
    })
}

/// Tokenizes a SEARCH argument line into words and parentheses, honoring
/// quoted strings (which may contain spaces).
pub fn tokenize(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    for c in input.chars() {
        match c {
            '"' => {
                in_quote = !in_quote;
                // On close, emit the quoted token (even if empty); on open,
                // flush any preceding unquoted token.
                if !in_quote || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            '(' | ')' if !in_quote => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                out.push(c.to_string());
            }
            ' ' if !in_quote => {
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_flags() {
        let toks = tokenize("UNSEEN SUBJECT \"quarterly report\"");
        let key = parse(&toks).unwrap();
        match key {
            SearchKey::And(v) => {
                assert!(matches!(v[0], SearchKey::Unkeyword(_)));
                assert!(matches!(&v[1], SearchKey::Subject(s) if s == "quarterly report"));
            }
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn or_and_not() {
        let toks = tokenize("OR FROM alice NOT SEEN");
        let key = parse(&toks).unwrap();
        assert!(!key.needs_bytes());
        assert!(matches!(key, SearchKey::And(_) | SearchKey::Or(_, _)));
    }

    #[test]
    fn date_parsing() {
        assert_eq!(
            parse_date("1-Feb-2024"),
            Date::from_calendar_date(2024, Month::February, 1).ok()
        );
    }
}
