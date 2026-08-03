//! Command reading and argument parsing (RFC 9051 §4, §9). A command is
//! read as a list of [`Seg`]ments — runs of ASCII command text with binary
//! **literals** (`{n}` / `{n+}`) materialized between them — so a literal's
//! arbitrary bytes never re-enter text tokenization. [`Parser`] walks the
//! segments to pull atoms, quoted strings, astrings, numbers, and
//! parenthesized lists. Sizes are bounded before allocation (protocol
//! rule: bound the wire, don't buffer first).

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

/// One piece of a read command: a run of command text, or a literal's raw
/// bytes.
#[derive(Debug, Clone)]
pub enum Seg {
    /// ASCII command text (no CRLF).
    Text(String),
    /// A literal argument's raw bytes.
    Lit(Vec<u8>),
}

/// The outcome of trying to read one command.
#[derive(Debug)]
pub enum ReadOutcome {
    /// A complete command as segments.
    Line(Vec<Seg>),
    /// Peer closed cleanly.
    Eof,
    /// A command line exceeded the configured limit.
    TooLong,
    /// A literal declared a size over the configured ceiling.
    LiteralTooLarge,
    /// A bare LF (no CR) — rejected (RFC 9051 requires CRLF).
    BareNewline,
    /// A malformed literal marker.
    BadLiteral,
}

/// Reads one full command, handling synchronizing (`{n}`) and
/// non-synchronizing (`{n+}`, LITERAL+) literals. For a synchronizing
/// literal within the ceiling it writes the `+ ` continuation before
/// reading the bytes.
pub async fn read_command<S>(
    reader: &mut BufReader<S>,
    max_line: usize,
    max_literal: usize,
) -> std::io::Result<ReadOutcome>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut segs: Vec<Seg> = Vec::new();
    let mut text = String::new();
    // Bound the *cumulative* literal bytes in one command, not just each
    // literal, so a command made entirely of literal continuations cannot
    // force unbounded accumulation before dispatch.
    let mut literal_total: usize = 0;
    loop {
        let mut line = Vec::new();
        match read_line_bounded(reader, max_line, &mut line).await? {
            LineEnd::Eof if line.is_empty() && segs.is_empty() && text.is_empty() => {
                return Ok(ReadOutcome::Eof);
            }
            LineEnd::Eof => return Ok(ReadOutcome::Eof),
            LineEnd::TooLong => return Ok(ReadOutcome::TooLong),
            LineEnd::BareNewline => return Ok(ReadOutcome::BareNewline),
            LineEnd::Crlf => {}
        }
        // `line` excludes the trailing CRLF. A trailing `{n}`/`{n+}` makes
        // this a literal continuation; otherwise the command is complete.
        match trailing_literal(&line) {
            None => {
                let Ok(s) = String::from_utf8(line) else {
                    // Non-UTF-8 in command text (outside a literal) is a
                    // protocol error; treat as a bad line.
                    return Ok(ReadOutcome::BadLiteral);
                };
                text.push_str(&s);
                segs.push(Seg::Text(text));
                return Ok(ReadOutcome::Line(segs));
            }
            Some(Err(())) => return Ok(ReadOutcome::BadLiteral),
            Some(Ok((head, len, non_sync))) => {
                literal_total = literal_total.saturating_add(len);
                if len > max_literal || literal_total > max_literal {
                    return Ok(ReadOutcome::LiteralTooLarge);
                }
                let Ok(head) = String::from_utf8(head) else {
                    return Ok(ReadOutcome::BadLiteral);
                };
                text.push_str(&head);
                segs.push(Seg::Text(std::mem::take(&mut text)));
                if !non_sync {
                    reader.get_mut().write_all(b"+ OK\r\n").await?;
                    reader.get_mut().flush().await?;
                }
                let mut buf = vec![0u8; len];
                tokio::io::AsyncReadExt::read_exact(reader, &mut buf).await?;
                segs.push(Seg::Lit(buf));
                // Continue reading the rest of the command on the next line.
            }
        }
    }
}

/// How a read line terminated.
enum LineEnd {
    Crlf,
    BareNewline,
    Eof,
    TooLong,
}

/// Reads bytes up to and including `\n`, bounded by `max` octets (not
/// counting CRLF). Returns the line content in `out` without the line
/// ending.
async fn read_line_bounded<S>(
    reader: &mut BufReader<S>,
    max: usize,
    out: &mut Vec<u8>,
) -> std::io::Result<LineEnd>
where
    S: AsyncRead + Unpin,
{
    loop {
        let available = match reader.fill_buf().await {
            Ok(b) => {
                if b.is_empty() {
                    return Ok(LineEnd::Eof);
                }
                b
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(LineEnd::Eof),
            Err(e) => return Err(e),
        };
        if let Some(nl) = available.iter().position(|&b| b == b'\n') {
            out.extend_from_slice(&available[..nl]);
            reader.consume(nl + 1);
            // Strip a trailing CR; a lone LF is a bare newline.
            if out.last() == Some(&b'\r') {
                out.pop();
                return Ok(LineEnd::Crlf);
            }
            return Ok(LineEnd::BareNewline);
        }
        let take = available.len();
        out.extend_from_slice(available);
        reader.consume(take);
        if out.len() > max {
            // Over the line ceiling: stop accumulating and report. The
            // session replies BAD; a following partial line yields another
            // BAD until the client resynchronizes (bounded, no unbounded
            // buffering).
            return Ok(LineEnd::TooLong);
        }
    }
}

/// If `line` ends with a literal marker `{n}` or `{n+}`, returns the head
/// before it, the length, and whether it is non-synchronizing. `Err` on a
/// malformed marker.
#[allow(clippy::type_complexity)]
fn trailing_literal(line: &[u8]) -> Option<Result<(Vec<u8>, usize, bool), ()>> {
    if line.last() != Some(&b'}') {
        return None;
    }
    let open = line.iter().rposition(|&b| b == b'{')?;
    let inner = &line[open + 1..line.len() - 1];
    if inner.is_empty() {
        return Some(Err(()));
    }
    let (digits, non_sync) = if inner.last() == Some(&b'+') {
        (&inner[..inner.len() - 1], true)
    } else {
        (inner, false)
    };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return Some(Err(()));
    }
    let Ok(s) = std::str::from_utf8(digits) else {
        return Some(Err(()));
    };
    let Ok(len) = s.parse::<usize>() else {
        return Some(Err(()));
    };
    Some(Ok((line[..open].to_vec(), len, non_sync)))
}

/// A value that is either text (atom/quoted) or a literal's bytes.
#[derive(Debug, Clone)]
pub enum AString {
    /// Text value (atom or dequoted quoted-string).
    Str(String),
    /// Literal bytes.
    Bytes(Vec<u8>),
}

impl AString {
    /// The value as a UTF-8 string (lossy for a binary literal). Command
    /// arguments that must be strings (mailbox names, user/pass) use this.
    pub fn as_string(&self) -> String {
        match self {
            AString::Str(s) => s.clone(),
            AString::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        }
    }
    /// The raw bytes (for APPEND message data).
    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            AString::Str(s) => s.into_bytes(),
            AString::Bytes(b) => b,
        }
    }
}

/// Walks a read command's segments, pulling tokens. Text segments are a
/// char cursor; a literal segment is consumed whole as one astring value.
pub struct Parser {
    segs: Vec<Seg>,
    seg: usize,
    pos: usize,
}

impl Parser {
    /// Wraps a read command.
    pub fn new(segs: Vec<Seg>) -> Self {
        Self {
            segs,
            seg: 0,
            pos: 0,
        }
    }

    /// True when no more input remains.
    pub fn at_end(&mut self) -> bool {
        self.skip_to_content();
        self.seg >= self.segs.len()
    }

    /// Advance past a text segment fully consumed, so `seg` points at the
    /// next segment with content (text or literal).
    fn skip_to_content(&mut self) {
        while self.seg < self.segs.len() {
            match &self.segs[self.seg] {
                Seg::Text(t) if self.pos >= t.len() => {
                    self.seg += 1;
                    self.pos = 0;
                }
                _ => break,
            }
        }
    }

    fn cur_text(&self) -> Option<&str> {
        match self.segs.get(self.seg) {
            Some(Seg::Text(t)) => Some(&t[self.pos.min(t.len())..]),
            _ => None,
        }
    }

    /// Skips spaces in the current text segment.
    pub fn skip_sp(&mut self) {
        while let Some(t) = self.cur_text() {
            let trimmed = t.len() - t.trim_start_matches(' ').len();
            if trimmed == 0 {
                break;
            }
            self.pos += trimmed;
        }
    }

    /// Peeks the next char of the current text segment (not a literal).
    pub fn peek(&mut self) -> Option<char> {
        self.skip_to_content();
        self.cur_text().and_then(|t| t.chars().next())
    }

    /// Consumes the next char if it equals `c`.
    pub fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    /// Reads an atom: a run of non-delimiter chars. Empty if the cursor is
    /// at a delimiter or literal.
    pub fn read_atom(&mut self) -> String {
        self.skip_to_content();
        let Some(t) = self.cur_text() else {
            return String::new();
        };
        let end = t.find(is_atom_delimiter).unwrap_or(t.len());
        let atom = t[..end].to_owned();
        self.pos += end;
        atom
    }

    /// Reads an astring (atom, quoted string, or literal).
    pub fn read_astring(&mut self) -> Option<AString> {
        self.skip_to_content();
        match self.segs.get(self.seg) {
            Some(Seg::Lit(_)) => {
                let Seg::Lit(b) = self.segs[self.seg].clone() else {
                    return None;
                };
                self.seg += 1;
                self.pos = 0;
                Some(AString::Bytes(b))
            }
            Some(Seg::Text(_)) => {
                if self.peek() == Some('"') {
                    self.read_quoted().map(AString::Str)
                } else {
                    let atom = self.read_atom();
                    if atom.is_empty() {
                        None
                    } else if atom.eq_ignore_ascii_case("NIL") {
                        Some(AString::Str(String::new()))
                    } else {
                        Some(AString::Str(atom))
                    }
                }
            }
            None => None,
        }
    }

    /// Reads a quoted string (leading `"` already peeked), handling `\\`
    /// and `\"` escapes.
    pub fn read_quoted(&mut self) -> Option<String> {
        if !self.eat('"') {
            return None;
        }
        let t = self.cur_text()?;
        let mut out = String::new();
        let mut chars = t.char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                '\\' => {
                    let (_, next) = chars.next()?;
                    out.push(next);
                }
                '"' => {
                    self.pos += i + c.len_utf8();
                    return Some(out);
                }
                _ => out.push(c),
            }
        }
        None // unterminated
    }

    /// Reads a decimal number.
    pub fn read_number(&mut self) -> Option<u64> {
        let atom = self.read_atom();
        atom.parse::<u64>().ok()
    }

    /// Reads the remainder of the current text segment, trimmed.
    pub fn rest(&mut self) -> String {
        self.skip_to_content();
        let Some(t) = self.cur_text() else {
            return String::new();
        };
        let out = t.trim().to_owned();
        self.pos += t.len();
        out
    }
}

/// Whether `c` ends an atom (space or a special).
fn is_atom_delimiter(c: char) -> bool {
    matches!(c, ' ' | '(' | ')' | '{' | '"' | ']') || c.is_control()
}

/// A parsed sequence set: ranges of numbers, with `*` = the highest
/// existing (message number or UID), represented as [`u64::MAX`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceSet(pub Vec<(u64, u64)>);

/// The `*` sentinel — the highest message number / UID present.
pub const STAR: u64 = u64::MAX;

impl SequenceSet {
    /// Parses `1`, `1:5`, `1,3,5`, `1:*`, `*:4` (RFC 9051 §9 sequence-set).
    pub fn parse(input: &str) -> Option<Self> {
        if input.is_empty() {
            return None;
        }
        let mut ranges = Vec::new();
        for part in input.split(',') {
            let (a, b) = match part.split_once(':') {
                Some((lo, hi)) => (parse_seqno(lo)?, parse_seqno(hi)?),
                None => {
                    let n = parse_seqno(part)?;
                    (n, n)
                }
            };
            // Normalize so lo <= hi (RFC allows reversed ranges).
            ranges.push((a.min(b), a.max(b)));
        }
        Some(SequenceSet(ranges))
    }

    /// Resolves the set against a slice of `n` items, returning the 1-based
    /// indices covered (deduplicated, ascending). `*` maps to `n`.
    pub fn resolve_indices(&self, n: usize) -> Vec<usize> {
        if n == 0 {
            return Vec::new();
        }
        let cap = n as u64;
        let mut hit = vec![false; n];
        for &(lo, hi) in &self.0 {
            let lo = if lo == STAR { cap } else { lo.min(cap) };
            let hi = if hi == STAR { cap } else { hi.min(cap) };
            let (lo, hi) = (lo.min(hi), lo.max(hi));
            if lo == 0 {
                continue;
            }
            for i in lo..=hi {
                if i >= 1 && i <= cap {
                    hit[(i - 1) as usize] = true;
                }
            }
        }
        (0..n).filter(|&i| hit[i]).collect()
    }

    /// Resolves the set as UID ranges, returning the UIDs from `uids` that
    /// fall in any range. `*` matches the largest existing UID.
    pub fn resolve_uids(&self, uids: &[i64]) -> Vec<i64> {
        if uids.is_empty() {
            return Vec::new();
        }
        let max_uid = *uids.iter().max().unwrap_or(&0) as u64;
        uids.iter()
            .copied()
            .filter(|&uid| {
                let u = uid as u64;
                self.0.iter().any(|&(lo, hi)| {
                    let lo = if lo == STAR { max_uid } else { lo };
                    let hi = if hi == STAR { max_uid } else { hi };
                    let (lo, hi) = (lo.min(hi), lo.max(hi));
                    u >= lo && u <= hi
                })
            })
            .collect()
    }
}

fn parse_seqno(s: &str) -> Option<u64> {
    if s == "*" {
        Some(STAR)
    } else {
        let n = s.parse::<u64>().ok()?;
        if n == 0 { None } else { Some(n) }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn sequence_set_parses_and_resolves() {
        let s = SequenceSet::parse("1:3,5,7:*").unwrap();
        assert_eq!(s.resolve_indices(8), vec![0, 1, 2, 4, 6, 7]);
        // `*` alone is the last.
        assert_eq!(SequenceSet::parse("*").unwrap().resolve_indices(4), vec![3]);
        // reversed range is normalized.
        assert_eq!(
            SequenceSet::parse("4:2").unwrap().resolve_indices(9),
            vec![1, 2, 3]
        );
        // zero is invalid.
        assert!(SequenceSet::parse("0").is_none());
    }

    #[test]
    fn uid_resolution_uses_actual_uids() {
        let uids = [2i64, 5, 9, 12];
        // 5:* covers 5,9,12.
        assert_eq!(
            SequenceSet::parse("5:*").unwrap().resolve_uids(&uids),
            vec![5, 9, 12]
        );
        // a gap UID range still selects only existing UIDs.
        assert_eq!(
            SequenceSet::parse("3:8").unwrap().resolve_uids(&uids),
            vec![5]
        );
    }

    #[test]
    fn trailing_literal_detection() {
        assert!(matches!(
            trailing_literal(b"a LOGIN {4}"),
            Some(Ok((_, 4, false)))
        ));
        assert!(matches!(
            trailing_literal(b"a LOGIN {4+}"),
            Some(Ok((_, 4, true)))
        ));
        assert!(trailing_literal(b"a NOOP").is_none());
        assert!(matches!(trailing_literal(b"a X {}"), Some(Err(()))));
    }

    #[test]
    fn parser_reads_atoms_and_quoted() {
        let mut p = Parser::new(vec![Seg::Text("LOGIN \"al ice\" secret".to_owned())]);
        assert_eq!(p.read_atom(), "LOGIN");
        p.skip_sp();
        assert_eq!(p.read_quoted().unwrap(), "al ice");
        p.skip_sp();
        assert_eq!(p.read_atom(), "secret");
        assert!(p.at_end());
    }

    #[test]
    fn parser_reads_literal_astring() {
        let mut p = Parser::new(vec![
            Seg::Text("LOGIN ".to_owned()),
            Seg::Lit(b"alice".to_vec()),
            Seg::Text(" ".to_owned()),
            Seg::Lit(b"s3cret".to_vec()),
        ]);
        assert_eq!(p.read_atom(), "LOGIN");
        p.skip_sp();
        assert_eq!(p.read_astring().unwrap().as_string(), "alice");
        p.skip_sp();
        assert_eq!(p.read_astring().unwrap().as_string(), "s3cret");
    }
}
