//! Tokenizer and recursive-descent parser for RFC 5228 (plus the
//! supported extensions). Every hard limit — script size, nesting depth,
//! test-list length, string-literal size — is checked **during** parse, so
//! a hostile script is rejected before an AST exists. `require` is
//! enforced: a feature from an un-required (or unsupported) extension is a
//! compile error, per §3.2.

use std::collections::HashSet;

use crate::ast::*;
use crate::error::{CompileError, Result};

/// Hard parse/eval limits — all security controls.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Max script source size in bytes.
    pub max_script: usize,
    /// Max block/test nesting depth.
    pub max_depth: usize,
    /// Max entries in a single test-list (`allof`/`anyof`).
    pub max_test_list: usize,
    /// Max single string-literal length.
    pub max_string: usize,
    /// Max evaluation instruction budget (used by the evaluator).
    pub max_instructions: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_script: 64 * 1024,
            max_depth: 15,
            max_test_list: 64,
            max_string: 16 * 1024,
            max_instructions: 100_000,
        }
    }
}

/// Extensions this engine implements (accepted in `require`).
const SUPPORTED: &[&str] = &[
    "fileinto",
    "envelope",
    "vacation",
    "subaddress",
    "imap4flags",
    "comparator-i;ascii-numeric",
    "comparator-i;ascii-casemap",
    "comparator-i;octet",
];

/// A compiled script: its top-level commands plus the extensions it
/// declared. The evaluator carries [`Limits`] for the instruction budget.
#[derive(Debug, Clone)]
pub struct Script {
    /// Top-level commands, in order.
    pub commands: Vec<Command>,
    /// Extensions `require`d (for evaluator feature checks).
    pub requires: HashSet<String>,
    /// The limits the script was compiled under.
    pub limits: Limits,
}

/// Compiles script `source` under `limits`.
///
/// # Errors
/// [`CompileError`] for any syntax error, missing/unsupported `require`, or
/// exceeded hard limit.
pub fn compile(source: &str, limits: Limits) -> Result<Script> {
    if source.len() > limits.max_script {
        return Err(CompileError::LimitExceeded(format!(
            "script exceeds {} bytes",
            limits.max_script
        )));
    }
    let tokens = tokenize(source, &limits)?;
    let mut p = Parser {
        toks: &tokens,
        pos: 0,
        limits,
        requires: HashSet::new(),
        seen_command: false,
    };
    let commands = p.parse_commands(0)?;
    if p.pos != tokens.len() {
        return Err(CompileError::Syntax("trailing tokens after script".into()));
    }
    Ok(Script {
        commands,
        requires: p.requires,
        limits,
    })
}

// ---- tokenizer ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Ident(String),
    Tag(String), // without the leading ':'
    Number(u64),
    Str(String),
    LBracket,
    RBracket,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Semicolon,
}

fn tokenize(src: &str, limits: &Limits) -> Result<Vec<Tok>> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut toks = Vec::new();
    while i < b.len() {
        let c = b[i];
        match c {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b'#' => {
                // Hash comment to end of line.
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'*' => {
                // Bracketed comment.
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                if i + 1 >= b.len() {
                    return Err(CompileError::Syntax("unterminated comment".into()));
                }
                i += 2;
            }
            b'[' => {
                toks.push(Tok::LBracket);
                i += 1;
            }
            b']' => {
                toks.push(Tok::RBracket);
                i += 1;
            }
            b'(' => {
                toks.push(Tok::LParen);
                i += 1;
            }
            b')' => {
                toks.push(Tok::RParen);
                i += 1;
            }
            b'{' => {
                toks.push(Tok::LBrace);
                i += 1;
            }
            b'}' => {
                toks.push(Tok::RBrace);
                i += 1;
            }
            b',' => {
                toks.push(Tok::Comma);
                i += 1;
            }
            b';' => {
                toks.push(Tok::Semicolon);
                i += 1;
            }
            b'"' => {
                let (s, next) = lex_quoted(b, i, limits)?;
                toks.push(Tok::Str(s));
                i = next;
            }
            b':' => {
                // Tag.
                let start = i + 1;
                let mut j = start;
                while j < b.len() && is_ident_byte(b[j]) {
                    j += 1;
                }
                if j == start {
                    return Err(CompileError::Syntax("empty tag ':'".into()));
                }
                toks.push(Tok::Tag(ascii_string(&b[start..j])?));
                i = j;
            }
            b'0'..=b'9' => {
                let (n, next) = lex_number(b, i)?;
                toks.push(Tok::Number(n));
                i = next;
            }
            _ if is_ident_start(c) => {
                let start = i;
                let mut j = i;
                while j < b.len() && is_ident_byte(b[j]) {
                    j += 1;
                }
                let ident = ascii_string(&b[start..j])?;
                // A multi-line string `text:` (only after certain args) —
                // recognized specially where it follows the `text:` marker.
                if ident == "text" && j < b.len() && b[j] == b':' {
                    let (s, next) = lex_multiline(b, j + 1, limits)?;
                    toks.push(Tok::Str(s));
                    i = next;
                } else {
                    toks.push(Tok::Ident(ident));
                    i = j;
                }
            }
            other => {
                return Err(CompileError::Syntax(format!(
                    "unexpected character 0x{other:02x}"
                )));
            }
        }
    }
    Ok(toks)
}

fn lex_quoted(b: &[u8], start: usize, limits: &Limits) -> Result<(String, usize)> {
    let mut i = start + 1;
    let mut out = Vec::new();
    while i < b.len() {
        match b[i] {
            b'"' => {
                if out.len() > limits.max_string {
                    return Err(CompileError::LimitExceeded(
                        "string literal too large".into(),
                    ));
                }
                return Ok((ascii_string(&out)?, i + 1));
            }
            b'\\' if i + 1 < b.len() => {
                // Only \\ and \" are special; other \x is literal x.
                out.push(b[i + 1]);
                i += 2;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    Err(CompileError::Syntax("unterminated string".into()))
}

/// Lexes a multi-line string: `text:` CRLF ... CRLF `.` CRLF, with dot-
/// unstuffing (a line of `..` becomes `.`).
fn lex_multiline(b: &[u8], start: usize, limits: &Limits) -> Result<(String, usize)> {
    // Skip the rest of the `text:` line (optional whitespace/comment) up to \n.
    let mut i = start;
    while i < b.len() && b[i] != b'\n' {
        i += 1;
    }
    if i < b.len() {
        i += 1; // past the newline
    }
    let mut out = Vec::new();
    loop {
        // Read one line.
        let line_start = i;
        while i < b.len() && b[i] != b'\n' {
            i += 1;
        }
        let mut line = &b[line_start..i];
        if line.last() == Some(&b'\r') {
            line = &line[..line.len() - 1];
        }
        if i < b.len() {
            i += 1; // consume newline
        }
        if line == b"." {
            if out.len() > limits.max_string {
                return Err(CompileError::LimitExceeded(
                    "string literal too large".into(),
                ));
            }
            return Ok((String::from_utf8_lossy(&out).into_owned(), i));
        }
        // Dot-unstuffing: a leading ".." → ".".
        let content = if line.first() == Some(&b'.') && line.get(1) == Some(&b'.') {
            &line[1..]
        } else {
            line
        };
        out.extend_from_slice(content);
        out.push(b'\n');
        if i >= b.len() {
            return Err(CompileError::Syntax(
                "unterminated multi-line string".into(),
            ));
        }
    }
}

fn lex_number(b: &[u8], start: usize) -> Result<(u64, usize)> {
    let mut i = start;
    let mut n: u64 = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        n = n
            .checked_mul(10)
            .and_then(|x| x.checked_add(u64::from(b[i] - b'0')))
            .ok_or_else(|| CompileError::Syntax("number too large".into()))?;
        i += 1;
    }
    let mult: u64 = match b.get(i).map(u8::to_ascii_uppercase) {
        Some(b'K') => {
            i += 1;
            1024
        }
        Some(b'M') => {
            i += 1;
            1024 * 1024
        }
        Some(b'G') => {
            i += 1;
            1024 * 1024 * 1024
        }
        _ => 1,
    };
    let n = n
        .checked_mul(mult)
        .ok_or_else(|| CompileError::Syntax("number too large".into()))?;
    Ok((n, i))
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}
fn is_ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}
fn ascii_string(bytes: &[u8]) -> Result<String> {
    // Sieve identifiers/tags are ASCII; strings may be UTF-8 (lossy ok).
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

// ---- parser ---------------------------------------------------------------

struct Parser<'a> {
    toks: &'a [Tok],
    pos: usize,
    limits: Limits,
    requires: HashSet<String>,
    seen_command: bool,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn bump(&mut self) -> Option<&Tok> {
        let t = self.toks.get(self.pos);
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
    fn expect(&mut self, t: &Tok, what: &str) -> Result<()> {
        if self.peek() == Some(t) {
            self.pos += 1;
            Ok(())
        } else {
            Err(CompileError::Syntax(format!("expected {what}")))
        }
    }

    /// Ensures `ext` was `require`d; else a compile error.
    fn need(&self, ext: &str) -> Result<()> {
        if self.requires.contains(ext) {
            Ok(())
        } else {
            Err(CompileError::MissingRequire(ext.to_owned()))
        }
    }

    fn parse_commands(&mut self, depth: usize) -> Result<Vec<Command>> {
        if depth > self.limits.max_depth {
            return Err(CompileError::LimitExceeded("nesting too deep".into()));
        }
        let mut cmds = Vec::new();
        while let Some(tok) = self.peek() {
            if *tok == Tok::RBrace {
                break;
            }
            let cmd = self.parse_command(depth)?;
            cmds.push(cmd);
        }
        Ok(cmds)
    }

    fn parse_command(&mut self, depth: usize) -> Result<Command> {
        let name = match self.bump() {
            Some(Tok::Ident(s)) => s.clone(),
            _ => return Err(CompileError::Syntax("expected a command".into())),
        };
        let is_require = name == "require";
        if is_require && self.seen_command {
            return Err(CompileError::Syntax(
                "require must precede all other commands".into(),
            ));
        }
        if !is_require {
            self.seen_command = true;
        }
        match name.as_str() {
            "require" => self.parse_require(),
            "if" => self.parse_if(depth),
            "elsif" | "else" => Err(CompileError::Syntax(format!("unexpected `{name}`"))),
            "stop" => {
                self.semi()?;
                Ok(Command::Stop)
            }
            "keep" => {
                let flags = self.opt_flags()?;
                self.semi()?;
                Ok(Command::Keep(flags))
            }
            "discard" => {
                self.semi()?;
                Ok(Command::Discard)
            }
            "fileinto" => {
                self.need("fileinto")?;
                let flags = self.opt_flags()?;
                let mailbox = self.string()?;
                self.semi()?;
                Ok(Command::FileInto { flags, mailbox })
            }
            "redirect" => {
                let address = self.string()?;
                self.semi()?;
                Ok(Command::Redirect(address))
            }
            "vacation" => {
                self.need("vacation")?;
                let v = self.parse_vacation()?;
                self.semi()?;
                Ok(Command::Vacation(v))
            }
            "setflag" | "addflag" | "removeflag" => {
                self.need("imap4flags")?;
                let flags = self.string_list()?;
                self.semi()?;
                Ok(match name.as_str() {
                    "setflag" => Command::SetFlag(flags),
                    "addflag" => Command::AddFlag(flags),
                    _ => Command::RemoveFlag(flags),
                })
            }
            other => Err(CompileError::Syntax(format!("unknown command `{other}`"))),
        }
    }

    fn parse_require(&mut self) -> Result<Command> {
        let exts = self.string_list()?;
        self.semi()?;
        for e in &exts {
            if !SUPPORTED.contains(&e.as_str()) {
                return Err(CompileError::UnsupportedExtension(e.clone()));
            }
            self.requires.insert(e.clone());
        }
        Ok(Command::Require(exts))
    }

    fn parse_if(&mut self, depth: usize) -> Result<Command> {
        let mut branches = Vec::new();
        let test = self.parse_test(depth)?;
        let block = self.block(depth)?;
        branches.push((test, block));
        let mut otherwise = None;
        loop {
            match self.peek() {
                Some(Tok::Ident(s)) if s == "elsif" => {
                    self.pos += 1;
                    let t = self.parse_test(depth)?;
                    let b = self.block(depth)?;
                    branches.push((t, b));
                }
                Some(Tok::Ident(s)) if s == "else" => {
                    self.pos += 1;
                    otherwise = Some(self.block(depth)?);
                    break;
                }
                _ => break,
            }
        }
        Ok(Command::If {
            branches,
            otherwise,
        })
    }

    fn block(&mut self, depth: usize) -> Result<Vec<Command>> {
        self.expect(&Tok::LBrace, "`{`")?;
        let cmds = self.parse_commands(depth + 1)?;
        self.expect(&Tok::RBrace, "`}`")?;
        Ok(cmds)
    }

    fn parse_test(&mut self, depth: usize) -> Result<Test> {
        if depth > self.limits.max_depth {
            return Err(CompileError::LimitExceeded("test nesting too deep".into()));
        }
        let name = match self.bump() {
            Some(Tok::Ident(s)) => s.clone(),
            _ => return Err(CompileError::Syntax("expected a test".into())),
        };
        match name.as_str() {
            "true" => Ok(Test::True),
            "false" => Ok(Test::False),
            "not" => Ok(Test::Not(Box::new(self.parse_test(depth + 1)?))),
            "allof" => Ok(Test::AllOf(self.test_list(depth)?)),
            "anyof" => Ok(Test::AnyOf(self.test_list(depth)?)),
            "exists" => Ok(Test::Exists(self.string_list()?)),
            "size" => self.parse_size(),
            "header" => self.parse_header(),
            "address" => self.parse_address(),
            "envelope" => {
                self.need("envelope")?;
                self.parse_envelope()
            }
            other => Err(CompileError::Syntax(format!("unknown test `{other}`"))),
        }
    }

    fn test_list(&mut self, depth: usize) -> Result<Vec<Test>> {
        self.expect(&Tok::LParen, "`(`")?;
        let mut tests = Vec::new();
        if self.peek() != Some(&Tok::RParen) {
            loop {
                if tests.len() >= self.limits.max_test_list {
                    return Err(CompileError::LimitExceeded("test-list too long".into()));
                }
                tests.push(self.parse_test(depth + 1)?);
                match self.peek() {
                    Some(Tok::Comma) => {
                        self.pos += 1;
                    }
                    _ => break,
                }
            }
        }
        self.expect(&Tok::RParen, "`)`")?;
        Ok(tests)
    }

    fn parse_size(&mut self) -> Result<Test> {
        let over = match self.bump() {
            Some(Tok::Tag(t)) if t == "over" => true,
            Some(Tok::Tag(t)) if t == "under" => false,
            _ => return Err(CompileError::Syntax("size needs :over or :under".into())),
        };
        let limit = match self.bump() {
            Some(Tok::Number(n)) => *n,
            _ => return Err(CompileError::Syntax("size needs a number".into())),
        };
        Ok(Test::Size { over, limit })
    }

    fn parse_header(&mut self) -> Result<Test> {
        let (comparator, match_type, _part) = self.match_args(false)?;
        let headers = self.string_list()?;
        let keys = self.string_list()?;
        Ok(Test::Header {
            comparator,
            match_type,
            headers,
            keys,
        })
    }

    fn parse_address(&mut self) -> Result<Test> {
        let (comparator, match_type, part) = self.match_args(true)?;
        let headers = self.string_list()?;
        let keys = self.string_list()?;
        Ok(Test::Address {
            comparator,
            match_type,
            part,
            headers,
            keys,
        })
    }

    fn parse_envelope(&mut self) -> Result<Test> {
        let (comparator, match_type, part) = self.match_args(true)?;
        let fields = self.string_list()?;
        let keys = self.string_list()?;
        Ok(Test::Envelope {
            comparator,
            match_type,
            part,
            fields,
            keys,
        })
    }

    /// Parses the optional `:comparator`, match-type, and (if `address`)
    /// address-part tagged arguments in any order.
    fn match_args(&mut self, address: bool) -> Result<(Comparator, MatchType, AddressPart)> {
        let mut comparator = Comparator::default();
        let mut match_type = MatchType::default();
        let mut part = AddressPart::default();
        while let Some(Tok::Tag(t)) = self.peek() {
            match t.as_str() {
                "is" => match_type = MatchType::Is,
                "contains" => match_type = MatchType::Contains,
                "matches" => match_type = MatchType::Matches,
                "comparator" => {
                    self.pos += 1;
                    let name = self.string()?;
                    comparator = match name.as_str() {
                        "i;ascii-casemap" => Comparator::AsciiCasemap,
                        "i;octet" => Comparator::Octet,
                        "i;ascii-numeric" => {
                            self.need("comparator-i;ascii-numeric")?;
                            Comparator::AsciiNumeric
                        }
                        other => {
                            return Err(CompileError::Syntax(format!(
                                "unknown comparator `{other}`"
                            )));
                        }
                    };
                    continue; // already consumed the tag + value
                }
                "all" if address => part = AddressPart::All,
                "localpart" if address => part = AddressPart::LocalPart,
                "domain" if address => part = AddressPart::Domain,
                "user" if address => {
                    self.need("subaddress")?;
                    part = AddressPart::User;
                }
                "detail" if address => {
                    self.need("subaddress")?;
                    part = AddressPart::Detail;
                }
                _ => break, // not a match/address tag; stop
            }
            self.pos += 1;
        }
        Ok((comparator, match_type, part))
    }

    fn parse_vacation(&mut self) -> Result<Vacation> {
        let mut v = Vacation::default();
        while let Some(Tok::Tag(t)) = self.peek() {
            let tag = t.clone();
            self.pos += 1;
            match tag.as_str() {
                "days" => {
                    v.days = Some(match self.bump() {
                        Some(Tok::Number(n)) => u32::try_from(*n).unwrap_or(u32::MAX),
                        _ => return Err(CompileError::Syntax(":days needs a number".into())),
                    });
                }
                "subject" => v.subject = Some(self.string()?),
                "from" => v.from = Some(self.string()?),
                "handle" => v.handle = Some(self.string()?),
                "addresses" => v.addresses = self.string_list()?,
                "mime" => v.mime = true,
                other => {
                    return Err(CompileError::Syntax(format!(
                        "unknown vacation tag :{other}"
                    )));
                }
            }
        }
        v.reason = self.string()?;
        Ok(v)
    }

    /// Parses an optional `:flags flag-list` tagged argument (imap4flags).
    fn opt_flags(&mut self) -> Result<FlagArg> {
        if let Some(Tok::Tag(t)) = self.peek()
            && t == "flags"
        {
            self.need("imap4flags")?;
            self.pos += 1;
            return Ok(Some(self.string_list()?));
        }
        Ok(None)
    }

    fn semi(&mut self) -> Result<()> {
        self.expect(&Tok::Semicolon, "`;`")
    }

    /// A single string (a bare string or a one-element list is not allowed
    /// here — RFC uses `string` where a list is not).
    fn string(&mut self) -> Result<String> {
        match self.bump() {
            Some(Tok::Str(s)) => Ok(s.clone()),
            _ => Err(CompileError::Syntax("expected a string".into())),
        }
    }

    /// A string-list: either `"x"` or `["a","b",...]`.
    fn string_list(&mut self) -> Result<Vec<String>> {
        match self.peek() {
            Some(Tok::Str(_)) => {
                let Some(Tok::Str(s)) = self.bump() else {
                    unreachable!()
                };
                Ok(vec![s.clone()])
            }
            Some(Tok::LBracket) => {
                self.pos += 1;
                let mut out = Vec::new();
                if self.peek() != Some(&Tok::RBracket) {
                    loop {
                        out.push(self.string()?);
                        match self.peek() {
                            Some(Tok::Comma) => {
                                self.pos += 1;
                            }
                            _ => break,
                        }
                    }
                }
                self.expect(&Tok::RBracket, "`]`")?;
                Ok(out)
            }
            _ => Err(CompileError::Syntax(
                "expected a string or string-list".into(),
            )),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn ok(src: &str) -> Script {
        compile(src, Limits::default()).expect("compile")
    }

    #[test]
    fn parses_rfc_example() {
        let s = ok(r#"
            require ["fileinto"];
            if header :contains "from" "coyote" {
                discard;
            } elsif header :contains ["subject"] ["$$$"] {
                discard;
            } else {
                fileinto "INBOX";
            }
        "#);
        assert_eq!(s.commands.len(), 2); // require + if
    }

    #[test]
    fn require_is_enforced() {
        // fileinto without require → MissingRequire.
        let e = compile("fileinto \"X\";", Limits::default()).unwrap_err();
        assert!(matches!(e, CompileError::MissingRequire(_)));
    }

    #[test]
    fn unsupported_extension_rejected() {
        let e = compile("require [\"reject\"];", Limits::default()).unwrap_err();
        assert!(matches!(e, CompileError::UnsupportedExtension(_)));
    }

    #[test]
    fn require_after_command_rejected() {
        let e = compile("keep; require [\"fileinto\"];", Limits::default()).unwrap_err();
        assert!(matches!(e, CompileError::Syntax(_)));
    }

    #[test]
    fn depth_limit_enforced() {
        let limits = Limits {
            max_depth: 2,
            ..Limits::default()
        };
        let deep = "if true { if true { if true { if true { keep; } } } }";
        assert!(matches!(
            compile(deep, limits),
            Err(CompileError::LimitExceeded(_))
        ));
    }

    #[test]
    fn test_list_limit_enforced() {
        let limits = Limits {
            max_test_list: 2,
            ..Limits::default()
        };
        let src = "if anyof (true, true, true) { keep; }";
        assert!(matches!(
            compile(src, limits),
            Err(CompileError::LimitExceeded(_))
        ));
    }

    #[test]
    fn number_quantifiers() {
        let s = ok("if size :over 1K { discard; }");
        if let Command::If { branches, .. } = &s.commands[0] {
            assert_eq!(
                branches[0].0,
                Test::Size {
                    over: true,
                    limit: 1024
                }
            );
        } else {
            panic!("expected if");
        }
    }

    #[test]
    fn multiline_string() {
        let s = ok("require [\"vacation\"];\nvacation text:\nHello\nWorld\n.\n;");
        if let Command::Vacation(v) = &s.commands[1] {
            assert_eq!(v.reason, "Hello\nWorld\n");
        } else {
            panic!("expected vacation");
        }
    }

    #[test]
    fn oversized_script_rejected() {
        let limits = Limits {
            max_script: 64,
            ..Limits::default()
        };
        let big = "keep; ".repeat(100);
        assert!(matches!(
            compile(&big, limits),
            Err(CompileError::LimitExceeded(_))
        ));
    }

    #[test]
    fn oversized_string_literal_rejected() {
        let limits = Limits {
            max_string: 16,
            ..Limits::default()
        };
        let src = format!("if header :is \"x\" \"{}\" {{ keep; }}", "a".repeat(64));
        assert!(matches!(
            compile(&src, limits),
            Err(CompileError::LimitExceeded(_))
        ));
    }

    #[test]
    fn subaddress_needs_require() {
        let e = compile(
            "if address :detail \"to\" \"x\" { keep; }",
            Limits::default(),
        )
        .unwrap_err();
        assert!(matches!(e, CompileError::MissingRequire(_)));
    }
}
