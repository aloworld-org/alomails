//! SMTP address and path parsing, RFC 5321 §4.1.2.
//!
//! Covers dot-string and quoted-string local parts, domains, IPv4 and
//! IPv6 address literals, source routes (accepted and ignored per
//! Appendix C), the null reverse-path, and the bare `<postmaster>`
//! forward-path (§4.1.1.3).
//!
//! SMTPUTF8-readiness: every function takes `allow_utf8`; with it set,
//! non-ASCII octets are legal in local parts and domains (RFC 6531).
//! Nothing enables it yet — the extension is advertised in a later
//! milestone — but the types and parser will not need to change.

use std::fmt;
use std::net::{Ipv4Addr, Ipv6Addr};

/// Maximum local-part octets (RFC 5321 §4.5.3.1.1).
const MAX_LOCAL_PART: usize = 64;
/// Maximum domain octets (RFC 5321 §4.5.3.1.2).
const MAX_DOMAIN: usize = 255;
/// Maximum path octets including punctuation (RFC 5321 §4.5.3.1.3).
const MAX_PATH: usize = 256;

/// The host half of a mailbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Host {
    /// A domain name, stored as received (case preserved for display,
    /// compared case-insensitively per §2.4).
    Domain(String),
    /// An IPv4 address literal `[192.0.2.1]`.
    Ipv4(Ipv4Addr),
    /// An IPv6 address literal `[IPv6:2001:db8::1]`.
    Ipv6(Ipv6Addr),
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(d) => f.write_str(d),
            Self::Ipv4(a) => write!(f, "[{a}]"),
            Self::Ipv6(a) => write!(f, "[IPv6:{a}]"),
        }
    }
}

/// A parsed `local-part@host` mailbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mailbox {
    /// Local part, unquoted/unescaped form (case preserved: the local
    /// part is case-sensitive per §2.4).
    pub local_part: String,
    /// Domain or address literal.
    pub host: Host,
}

impl fmt::Display for Mailbox {
    /// Re-quotes the local part when it is not a valid dot-string, so
    /// the display form is always a valid SMTP mailbox.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if is_dot_string(&self.local_part, true) {
            write!(f, "{}@{}", self.local_part, self.host)
        } else {
            f.write_str("\"")?;
            for c in self.local_part.chars() {
                if c == '"' || c == '\\' {
                    f.write_str("\\")?;
                }
                write!(f, "{c}")?;
            }
            write!(f, "\"@{}", self.host)
        }
    }
}

/// `MAIL FROM` argument: a mailbox or the null path `<>` (bounces).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReversePath {
    /// The null reverse-path `<>` (§3.6.3, §4.5.5).
    Null,
    /// An ordinary sender mailbox.
    Mailbox(Mailbox),
}

impl fmt::Display for ReversePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => f.write_str("<>"),
            Self::Mailbox(m) => write!(f, "<{m}>"),
        }
    }
}

/// `RCPT TO` argument: a mailbox or the domain-less `<postmaster>`
/// special case every SMTP server must accept (§4.1.1.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardPath {
    /// `<postmaster>` with no domain.
    Postmaster,
    /// An ordinary recipient mailbox.
    Mailbox(Mailbox),
}

impl fmt::Display for ForwardPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Postmaster => f.write_str("<postmaster>"),
            Self::Mailbox(m) => write!(f, "<{m}>"),
        }
    }
}

/// Why an address failed to parse; every variant maps to a 501 reply.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AddressError {
    /// Path did not start with `<` and end with `>`.
    #[error("path must be enclosed in angle brackets")]
    NoAngleBrackets,
    /// Path exceeded 256 octets (§4.5.3.1.3).
    #[error("path exceeds {MAX_PATH} octets")]
    PathTooLong,
    /// Local part missing, malformed, or over 64 octets.
    #[error("invalid local part")]
    BadLocalPart,
    /// Domain missing, malformed, or over 255 octets.
    #[error("invalid domain")]
    BadDomain,
    /// Address literal not a valid IPv4 or `IPv6:` literal. General
    /// address literals (§4.1.3 tagged forms) are deliberately
    /// rejected: nothing routable can be done with them.
    #[error("invalid address literal")]
    BadAddressLiteral,
    /// Non-ASCII octets present without SMTPUTF8 (RFC 6531) enabled.
    #[error("non-ASCII address requires SMTPUTF8")]
    NonAsciiWithoutSmtputf8,
}

/// Parses a `MAIL FROM` path argument (including `<>`).
///
/// # Errors
/// [`AddressError`] on any syntax violation; maps to 501.
pub fn parse_reverse_path(raw: &str, allow_utf8: bool) -> Result<ReversePath, AddressError> {
    let inner = strip_angles(raw)?;
    if inner.is_empty() {
        return Ok(ReversePath::Null);
    }
    Ok(ReversePath::Mailbox(parse_mailbox_with_route(
        inner, allow_utf8,
    )?))
}

/// Parses a `RCPT TO` path argument (including `<postmaster>`).
///
/// # Errors
/// [`AddressError`] on any syntax violation; maps to 501.
pub fn parse_forward_path(raw: &str, allow_utf8: bool) -> Result<ForwardPath, AddressError> {
    let inner = strip_angles(raw)?;
    if inner.eq_ignore_ascii_case("postmaster") {
        return Ok(ForwardPath::Postmaster);
    }
    Ok(ForwardPath::Mailbox(parse_mailbox_with_route(
        inner, allow_utf8,
    )?))
}

fn strip_angles(raw: &str) -> Result<&str, AddressError> {
    if raw.len() > MAX_PATH {
        return Err(AddressError::PathTooLong);
    }
    raw.strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .ok_or(AddressError::NoAngleBrackets)
}

/// Handles the optional source route (`@a,@b:` prefix): accepted and
/// ignored, per RFC 5321 Appendix C.
fn parse_mailbox_with_route(inner: &str, allow_utf8: bool) -> Result<Mailbox, AddressError> {
    let mailbox_part = if inner.starts_with('@') {
        let colon = inner.find(':').ok_or(AddressError::BadDomain)?;
        let (route, rest) = inner.split_at(colon);
        // Validate the route domains even though we discard them.
        for hop in route.split(',') {
            let hop = hop.strip_prefix('@').ok_or(AddressError::BadDomain)?;
            validate_domain(hop, allow_utf8)?;
        }
        &rest[1..]
    } else {
        inner
    };
    parse_mailbox(mailbox_part, allow_utf8)
}

/// Parses `local-part@host`.
fn parse_mailbox(s: &str, allow_utf8: bool) -> Result<Mailbox, AddressError> {
    let (local_raw, host_raw) = split_at_local_part(s)?;

    let local_part = if local_raw.starts_with('"') {
        parse_quoted_local(local_raw, allow_utf8)?
    } else {
        if !is_dot_string(local_raw, false) {
            if !allow_utf8 && !local_raw.is_ascii() {
                return Err(AddressError::NonAsciiWithoutSmtputf8);
            }
            if !(allow_utf8 && is_dot_string(local_raw, true)) {
                return Err(AddressError::BadLocalPart);
            }
        }
        local_raw.to_owned()
    };
    if local_part.is_empty() || local_raw.len() > MAX_LOCAL_PART {
        return Err(AddressError::BadLocalPart);
    }

    let host = if host_raw.starts_with('[') {
        parse_address_literal(host_raw)?
    } else {
        validate_domain(host_raw, allow_utf8)?;
        Host::Domain(host_raw.to_owned())
    };

    Ok(Mailbox { local_part, host })
}

/// Splits on the `@` separating local part and host, honouring quoting
/// (an `@` inside a quoted local part is content, not the separator).
fn split_at_local_part(s: &str) -> Result<(&str, &str), AddressError> {
    if s.starts_with('"') {
        // Find the closing quote, skipping backslash escapes.
        let bytes = s.as_bytes();
        let mut i = 1;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b'"' => {
                    let local = &s[..=i];
                    let rest = &s[i + 1..];
                    let host = rest.strip_prefix('@').ok_or(AddressError::BadLocalPart)?;
                    return Ok((local, host));
                }
                _ => i += 1,
            }
        }
        Err(AddressError::BadLocalPart)
    } else {
        let at = s.rfind('@').ok_or(AddressError::BadLocalPart)?;
        Ok((&s[..at], &s[at + 1..]))
    }
}

/// Unescapes a quoted-string local part (§4.1.2 Quoted-string).
fn parse_quoted_local(raw: &str, allow_utf8: bool) -> Result<String, AddressError> {
    let inner = raw
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or(AddressError::BadLocalPart)?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // quoted-pairSMTP: %d32-126 after the backslash.
                let escaped = chars.next().ok_or(AddressError::BadLocalPart)?;
                if !escaped.is_ascii() || (escaped as u32) < 32 || (escaped as u32) > 126 {
                    return Err(AddressError::BadLocalPart);
                }
                out.push(escaped);
            }
            '"' => return Err(AddressError::BadLocalPart),
            c if c.is_ascii() => {
                // qtextSMTP: %d32-33 / %d35-91 / %d93-126 ('"' and '\'
                // handled above; reject controls).
                let v = c as u32;
                if !(32..=126).contains(&v) {
                    return Err(AddressError::BadLocalPart);
                }
                out.push(c);
            }
            c if allow_utf8 => out.push(c),
            _ => return Err(AddressError::NonAsciiWithoutSmtputf8),
        }
    }
    Ok(out)
}

/// atext of RFC 5322 §3.2.3 (the printable ASCII specials SMTP allows
/// unquoted), optionally extended with non-ASCII for SMTPUTF8.
fn is_atext(c: char, allow_utf8: bool) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-/=?^_`{|}~".contains(c) || (allow_utf8 && !c.is_ascii())
}

/// `Dot-string = Atom *("." Atom)` — no leading/trailing/double dots.
fn is_dot_string(s: &str, allow_utf8: bool) -> bool {
    !s.is_empty()
        && !s.starts_with('.')
        && !s.ends_with('.')
        && !s.contains("..")
        && s.chars().all(|c| c == '.' || is_atext(c, allow_utf8))
}

/// Validates `Domain = sub-domain *("." sub-domain)` with LDH labels
/// (§4.1.2), each label ≤ 63 octets, whole domain ≤ 255.
fn validate_domain(s: &str, allow_utf8: bool) -> Result<(), AddressError> {
    if s.is_empty() || s.len() > MAX_DOMAIN {
        return Err(AddressError::BadDomain);
    }
    if !allow_utf8 && !s.is_ascii() {
        return Err(AddressError::NonAsciiWithoutSmtputf8);
    }
    for label in s.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(AddressError::BadDomain);
        }
        let first_ok = label
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || (allow_utf8 && !c.is_ascii()));
        let last_ok = label
            .chars()
            .last()
            .is_some_and(|c| c.is_ascii_alphanumeric() || (allow_utf8 && !c.is_ascii()));
        let body_ok = label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || (allow_utf8 && !c.is_ascii()));
        if !(first_ok && last_ok && body_ok) {
            return Err(AddressError::BadDomain);
        }
    }
    Ok(())
}

/// Parses `[IPv4]` and `[IPv6:...]` literals (§4.1.3). IPv4 support is
/// mandatory, IPv6 recommended; general (tagged) literals are rejected.
fn parse_address_literal(s: &str) -> Result<Host, AddressError> {
    let inner = s
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or(AddressError::BadAddressLiteral)?;
    if let Some(v6) = inner
        .strip_prefix("IPv6:")
        .or_else(|| inner.strip_prefix("ipv6:"))
    {
        return v6
            .parse::<Ipv6Addr>()
            .map(Host::Ipv6)
            .map_err(|_| AddressError::BadAddressLiteral);
    }
    inner
        .parse::<Ipv4Addr>()
        .map(Host::Ipv4)
        .map_err(|_| AddressError::BadAddressLiteral)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn mailbox(raw: &str) -> Mailbox {
        match parse_forward_path(raw, false).unwrap() {
            ForwardPath::Mailbox(m) => m,
            ForwardPath::Postmaster => panic!("expected mailbox"),
        }
    }

    #[test]
    fn simple_mailbox_parses() {
        let m = mailbox("<user@example.com>");
        assert_eq!(m.local_part, "user");
        assert_eq!(m.host, Host::Domain("example.com".to_owned()));
    }

    #[test]
    fn plus_tag_and_specials_in_dot_string() {
        let m = mailbox("<user+tag@example.com>");
        assert_eq!(m.local_part, "user+tag");
        let m = mailbox("<o'brien=x{y}@example.com>");
        assert_eq!(m.local_part, "o'brien=x{y}");
    }

    #[test]
    fn quoted_local_part_with_space_and_at() {
        let m = mailbox("<\"john smith\"@example.com>");
        assert_eq!(m.local_part, "john smith");
        let m = mailbox("<\"a@b\"@example.com>");
        assert_eq!(m.local_part, "a@b");
    }

    #[test]
    fn quoted_pair_unescapes() {
        let m = mailbox("<\"quote\\\"d\\\\slash\"@example.com>");
        assert_eq!(m.local_part, "quote\"d\\slash");
    }

    #[test]
    fn display_requotes_when_needed() {
        // RFC 5321 §4.1.2: display form must be a valid mailbox again.
        let m = mailbox("<\"john smith\"@example.com>");
        assert_eq!(m.to_string(), "\"john smith\"@example.com");
        let round = mailbox(&format!("<{m}>"));
        assert_eq!(round.local_part, "john smith");
    }

    #[test]
    fn null_reverse_path_is_null() {
        assert_eq!(parse_reverse_path("<>", false).unwrap(), ReversePath::Null);
    }

    #[test]
    fn postmaster_without_domain_is_special() {
        // RFC 5321 §4.1.1.3
        assert_eq!(
            parse_forward_path("<Postmaster>", false).unwrap(),
            ForwardPath::Postmaster
        );
    }

    #[test]
    fn source_route_is_accepted_and_ignored() {
        // RFC 5321 Appendix C
        let m = mailbox("<@relay1.example,@relay2.example:user@example.com>");
        assert_eq!(m.local_part, "user");
        assert_eq!(m.host, Host::Domain("example.com".to_owned()));
    }

    #[test]
    fn ipv4_and_ipv6_literals_parse() {
        // RFC 5321 §4.1.3
        let m = mailbox("<user@[192.0.2.1]>");
        assert_eq!(m.host, Host::Ipv4("192.0.2.1".parse().unwrap()));
        let m = mailbox("<user@[IPv6:2001:db8::1]>");
        assert_eq!(m.host, Host::Ipv6("2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn general_address_literal_is_rejected() {
        assert_eq!(
            parse_forward_path("<user@[tag:content]>", false),
            Err(AddressError::BadAddressLiteral)
        );
    }

    #[test]
    fn syntax_violations_are_rejected() {
        for bad in [
            "user@example.com",       // no angle brackets
            "<user>",                 // no domain
            "<@example.com>",         // no local part
            "<.user@example.com>",    // leading dot
            "<us..er@example.com>",   // double dot
            "<user@-bad.example>",    // label starts with hyphen
            "<user@example..com>",    // empty label
            "<user@[999.0.2.1]>",     // bad IPv4
            "<\"unterminated@e.com>", // unterminated quote
        ] {
            assert!(
                parse_forward_path(bad, false).is_err(),
                "should reject: {bad}"
            );
        }
    }

    #[test]
    fn length_limits_enforced() {
        // §4.5.3.1.1: local part > 64 octets
        let long_local = format!("<{}@example.com>", "a".repeat(65));
        assert!(parse_forward_path(&long_local, false).is_err());
        // §4.5.3.1.3: whole path > 256 octets
        let long_path = format!("<user@{}.example.com>", "a".repeat(260));
        assert_eq!(
            parse_forward_path(&long_path, false),
            Err(AddressError::PathTooLong)
        );
    }

    #[test]
    fn utf8_rejected_without_smtputf8_accepted_with() {
        // SMTPUTF8-readiness (RFC 6531): same parser, flag flipped.
        assert_eq!(
            parse_forward_path("<müller@example.com>", false),
            Err(AddressError::NonAsciiWithoutSmtputf8)
        );
        let m = match parse_forward_path("<müller@münchen.example>", true).unwrap() {
            ForwardPath::Mailbox(m) => m,
            ForwardPath::Postmaster => panic!(),
        };
        assert_eq!(m.local_part, "müller");
    }
}
