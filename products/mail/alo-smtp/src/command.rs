//! Parsing of SMTP command lines (RFC 5321 §4.1.1).
//!
//! Parsing is separated from the session state machine so each can be
//! tested alone: this module decides *what the client said*, never
//! what to do about it. ESMTP parameters are extracted verbatim —
//! whether any are acceptable is session policy (none are, until
//! extensions are advertised in a later milestone).

use crate::address::{self, AddressError, ForwardPath, ReversePath};

/// A syntactically valid command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `EHLO <domain>` (§4.1.1.1).
    Ehlo {
        /// The client-supplied domain or address literal, verbatim.
        client: String,
    },
    /// `HELO <domain>` (§4.1.1.1) — pre-ESMTP clients still send it.
    Helo {
        /// The client-supplied domain, verbatim.
        client: String,
    },
    /// `MAIL FROM:<path> [params]` (§4.1.1.2).
    Mail {
        /// Sender, or null for bounces.
        reverse_path: ReversePath,
        /// Raw ESMTP parameters after the path, if any.
        params: Option<String>,
    },
    /// `RCPT TO:<path> [params]` (§4.1.1.3).
    Rcpt {
        /// Recipient.
        forward_path: ForwardPath,
        /// Raw ESMTP parameters after the path, if any.
        params: Option<String>,
    },
    /// `DATA` (§4.1.1.4).
    Data,
    /// `STARTTLS` (RFC 3207 §4) — takes no argument.
    StartTls,
    /// `AUTH <mechanism> [initial-response]` (RFC 4954 §4).
    Auth {
        /// Mechanism token as received (e.g. `PLAIN`, `LOGIN`).
        mechanism: String,
        /// Optional base64 initial response, or `=` for an empty one.
        initial: Option<String>,
    },
    /// `RSET` (§4.1.1.5).
    Rset,
    /// `VRFY <string>` (§4.1.1.6).
    Vrfy,
    /// `NOOP` (§4.1.1.9) — an optional argument is permitted.
    Noop,
    /// `QUIT` (§4.1.1.10).
    Quit,
    /// A verb we recognize but do not implement (EXPN, HELP) — 502.
    NotImplemented {
        /// The verb, uppercased.
        verb: String,
    },
    /// A verb we do not recognize — 500.
    Unknown {
        /// The verb, uppercased, for logging — never echoed.
        verb: String,
    },
}

/// A command line that could not be accepted as written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    /// Empty command line.
    #[error("empty command line")]
    Empty,
    /// A verb that requires an argument arrived without one.
    #[error("{verb} requires an argument")]
    MissingParameter {
        /// The verb in question.
        verb: String,
    },
    /// Arguments malformed or present where none are allowed.
    #[error("{verb} received unexpected or malformed arguments")]
    BadParameter {
        /// The verb in question.
        verb: String,
    },
    /// The path in MAIL/RCPT failed address parsing — maps to 501.
    #[error("bad address: {0}")]
    BadAddress(#[from] AddressError),
}

/// Parses one command line (already stripped of its CRLF). Verbs are
/// case-insensitive (§2.4). `allow_utf8` propagates SMTPUTF8 state to
/// address parsing (always false until the extension is advertised).
///
/// # Errors
/// [`CommandError`] when the line is empty or arguments do not fit
/// the verb; the session maps these to 500/501 replies.
pub fn parse(line: &str, allow_utf8: bool) -> Result<Command, CommandError> {
    if line.is_empty() {
        return Err(CommandError::Empty);
    }

    let (verb, argument) = match line.split_once(' ') {
        Some((v, rest)) => (v, Some(rest)),
        None => (line, None),
    };
    let verb_upper = verb.to_ascii_uppercase();

    match verb_upper.as_str() {
        "EHLO" => single_token(argument, verb_upper).map(|client| Command::Ehlo { client }),
        "HELO" => single_token(argument, verb_upper).map(|client| Command::Helo { client }),
        "MAIL" => {
            let (path_raw, params) = keyword_path(argument, "FROM:", &verb_upper)?;
            let reverse_path = address::parse_reverse_path(&path_raw, allow_utf8)?;
            Ok(Command::Mail {
                reverse_path,
                params,
            })
        }
        "RCPT" => {
            let (path_raw, params) = keyword_path(argument, "TO:", &verb_upper)?;
            let forward_path = address::parse_forward_path(&path_raw, allow_utf8)?;
            Ok(Command::Rcpt {
                forward_path,
                params,
            })
        }
        "DATA" => no_argument(argument, verb_upper, Command::Data),
        "STARTTLS" => no_argument(argument, verb_upper, Command::StartTls),
        "AUTH" => parse_auth(argument, verb_upper),
        "RSET" => no_argument(argument, verb_upper, Command::Rset),
        "QUIT" => no_argument(argument, verb_upper, Command::Quit),
        // NOOP may carry a string that is simply ignored (§4.1.1.9).
        "NOOP" => Ok(Command::Noop),
        // VRFY requires an argument (§4.1.1.6); we never disclose
        // whether it names a user, so the argument itself is unused.
        "VRFY" => match argument {
            None | Some("") => Err(CommandError::MissingParameter { verb: verb_upper }),
            Some(_) => Ok(Command::Vrfy),
        },
        "EXPN" | "HELP" => Ok(Command::NotImplemented { verb: verb_upper }),
        _ => Ok(Command::Unknown { verb: verb_upper }),
    }
}

/// EHLO/HELO: exactly one token of printable ASCII (§4.1.1.1 expects
/// a Domain or address-literal; control octets would otherwise flow
/// verbatim into the `Received:` stamp and the spool sidecar —
/// attacker-controlled binary in the message trace). SMTPUTF8 (M3)
/// will widen this to U-labels.
fn single_token(argument: Option<&str>, verb: String) -> Result<String, CommandError> {
    match argument {
        None | Some("") => Err(CommandError::MissingParameter { verb }),
        Some(client) if !client.chars().all(|c| c.is_ascii_graphic()) => {
            Err(CommandError::BadParameter { verb })
        }
        Some(client) => Ok(client.to_owned()),
    }
}

/// `AUTH mechanism [initial-response]` (RFC 4954 §4). The initial
/// response, when present, is one token (base64 or `=` for empty).
fn parse_auth(argument: Option<&str>, verb: String) -> Result<Command, CommandError> {
    let arg = match argument {
        None | Some("") => return Err(CommandError::MissingParameter { verb }),
        Some(arg) => arg,
    };
    let mut parts = arg.split(' ');
    let mechanism = parts.next().unwrap_or("").to_owned();
    if mechanism.is_empty() {
        return Err(CommandError::MissingParameter { verb });
    }
    let initial = parts.next().map(str::to_owned);
    // At most one initial-response token (§4).
    if parts.next().is_some() {
        return Err(CommandError::BadParameter { verb });
    }
    Ok(Command::Auth { mechanism, initial })
}

/// DATA/RSET/QUIT/STARTTLS admit no arguments.
fn no_argument(
    argument: Option<&str>,
    verb: String,
    command: Command,
) -> Result<Command, CommandError> {
    match argument {
        None => Ok(command),
        Some(_) => Err(CommandError::BadParameter { verb }),
    }
}

/// Extracts `<path>` and trailing params from `FROM:<path> params` /
/// `TO:<path> params`. RFC 5321 puts no space after the colon; one
/// optional space is tolerated (widely sent, no security ambiguity).
fn keyword_path(
    argument: Option<&str>,
    keyword: &str,
    verb: &str,
) -> Result<(String, Option<String>), CommandError> {
    let arg = argument.ok_or_else(|| CommandError::MissingParameter {
        verb: verb.to_owned(),
    })?;
    // `get` (not slicing) so a multi-byte char straddling the keyword
    // boundary is a clean 501, never a char-boundary panic — this is
    // a public API and SMTPUTF8 (M3) will feed it non-ASCII lines.
    let (keyword_part, rest) = match (arg.get(..keyword.len()), arg.get(keyword.len()..)) {
        (Some(k), Some(r)) => (k, r),
        _ => {
            return Err(CommandError::BadParameter {
                verb: verb.to_owned(),
            });
        }
    };
    if !keyword_part.eq_ignore_ascii_case(keyword) {
        return Err(CommandError::BadParameter {
            verb: verb.to_owned(),
        });
    }
    let rest = rest.trim_start_matches(' ');
    let (path, tail) = split_path(rest, verb)?;
    let params = tail.trim();
    Ok((
        path,
        if params.is_empty() {
            None
        } else {
            Some(params.to_owned())
        },
    ))
}

/// Splits `<...>` from what follows, honouring quoted local parts —
/// `>` is legal qtextSMTP, so naive `find('>')` would mis-split
/// `<"a>b"@example.com>`.
fn split_path<'a>(s: &'a str, verb: &str) -> Result<(String, &'a str), CommandError> {
    if !s.starts_with('<') {
        return Err(CommandError::BadParameter {
            verb: verb.to_owned(),
        });
    }
    let bytes = s.as_bytes();
    let mut in_quote = false;
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_quote => i += 1,
            b'"' => in_quote = !in_quote,
            b'>' if !in_quote => {
                return Ok((s[..=i].to_owned(), &s[i + 1..]));
            }
            _ => {}
        }
        i += 1;
    }
    Err(CommandError::BadParameter {
        verb: verb.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use crate::address::{ForwardPath, Host, ReversePath};

    use super::*;

    #[test]
    fn ehlo_helo_parse_and_are_case_insensitive() {
        assert!(matches!(
            parse("ehlo client.example", false).unwrap(),
            Command::Ehlo { client } if client == "client.example"
        ));
        assert!(matches!(
            parse("HeLo client.example", false).unwrap(),
            Command::Helo { client } if client == "client.example"
        ));
    }

    #[test]
    fn mail_from_parses_path_and_null_path() {
        let cmd = parse("MAIL FROM:<bob@example.org>", false).unwrap();
        assert!(matches!(
            cmd,
            Command::Mail { reverse_path: ReversePath::Mailbox(ref m), params: None }
                if m.local_part == "bob"
        ));
        let cmd = parse("MAIL FROM:<>", false).unwrap();
        assert!(matches!(
            cmd,
            Command::Mail {
                reverse_path: ReversePath::Null,
                params: None
            }
        ));
    }

    #[test]
    fn mail_from_tolerates_one_space_after_colon() {
        let cmd = parse("MAIL FROM: <bob@example.org>", false).unwrap();
        assert!(matches!(cmd, Command::Mail { .. }));
    }

    #[test]
    fn esmtp_params_are_extracted_verbatim() {
        let cmd = parse("MAIL FROM:<bob@example.org> SIZE=1000 BODY=8BITMIME", false).unwrap();
        assert!(matches!(
            cmd,
            Command::Mail { params: Some(ref p), .. } if p == "SIZE=1000 BODY=8BITMIME"
        ));
    }

    #[test]
    fn rcpt_to_parses_including_quoted_gt_in_local_part() {
        let cmd = parse("RCPT TO:<\"a>b\"@example.com>", false).unwrap();
        assert!(matches!(
            cmd,
            Command::Rcpt { forward_path: ForwardPath::Mailbox(ref m), params: None }
                if m.local_part == "a>b" && m.host == Host::Domain("example.com".to_owned())
        ));
    }

    #[test]
    fn bad_addresses_surface_as_bad_address() {
        assert!(matches!(
            parse("MAIL FROM:<not-an-address>", false),
            Err(CommandError::BadAddress(_))
        ));
        assert!(matches!(
            parse("RCPT TO:bob@example.org", false),
            Err(CommandError::BadParameter { .. })
        ));
    }

    #[test]
    fn wrong_keyword_is_bad_parameter() {
        assert!(matches!(
            parse("MAIL TO:<bob@example.org>", false),
            Err(CommandError::BadParameter { .. })
        ));
    }

    #[test]
    fn multibyte_argument_at_keyword_boundary_is_501_not_a_panic() {
        // Regression (review finding): `MAIL ФФФ` used to hit a
        // char-boundary panic in the keyword slice.
        assert!(matches!(
            parse("MAIL ФФФ", false),
            Err(CommandError::BadParameter { .. })
        ));
        assert!(matches!(
            parse("MAIL ФФФ", true),
            Err(CommandError::BadParameter { .. })
        ));
        assert!(matches!(
            parse("RCPT д", true),
            Err(CommandError::BadParameter { .. })
        ));
    }

    #[test]
    fn helo_with_control_octets_is_501() {
        // Control octets must never reach the Received: stamp.
        assert!(matches!(
            parse("EHLO a\u{0}b", false),
            Err(CommandError::BadParameter { .. })
        ));
        assert!(matches!(
            parse("HELO a\tb", false),
            Err(CommandError::BadParameter { .. })
        ));
    }

    #[test]
    fn bare_verbs_and_argument_rules() {
        assert!(matches!(parse("DATA", false).unwrap(), Command::Data));
        assert!(matches!(parse("RSET", false).unwrap(), Command::Rset));
        assert!(matches!(parse("QUIT", false).unwrap(), Command::Quit));
        assert!(matches!(parse("NOOP", false).unwrap(), Command::Noop));
        assert!(matches!(parse("NOOP ping", false).unwrap(), Command::Noop));
        assert!(matches!(
            parse("DATA now", false),
            Err(CommandError::BadParameter { .. })
        ));
        assert!(matches!(
            parse("VRFY", false),
            Err(CommandError::MissingParameter { .. })
        ));
        assert!(matches!(parse("VRFY alice", false).unwrap(), Command::Vrfy));
    }

    #[test]
    fn recognized_but_unimplemented_verbs() {
        assert!(matches!(
            parse("HELP", false).unwrap(),
            Command::NotImplemented { ref verb } if verb == "HELP"
        ));
        assert!(matches!(
            parse("EXPN list", false).unwrap(),
            Command::NotImplemented { ref verb } if verb == "EXPN"
        ));
    }

    #[test]
    fn unknown_verb_is_unknown() {
        assert!(matches!(
            parse("XYZZY", false).unwrap(),
            Command::Unknown { ref verb } if verb == "XYZZY"
        ));
    }

    #[test]
    fn starttls_parses_and_rejects_arguments() {
        assert!(matches!(
            parse("STARTTLS", false).unwrap(),
            Command::StartTls
        ));
        assert!(matches!(
            parse("starttls", false).unwrap(),
            Command::StartTls
        ));
        assert!(matches!(
            parse("STARTTLS now", false),
            Err(CommandError::BadParameter { .. })
        ));
    }

    #[test]
    fn auth_parses_mechanism_and_optional_initial_response() {
        assert!(matches!(
            parse("AUTH LOGIN", false).unwrap(),
            Command::Auth { ref mechanism, initial: None } if mechanism == "LOGIN"
        ));
        assert!(matches!(
            parse("AUTH PLAIN dGVzdA==", false).unwrap(),
            Command::Auth { ref mechanism, initial: Some(ref ir) }
                if mechanism == "PLAIN" && ir == "dGVzdA=="
        ));
        // Empty initial response marker `=` (RFC 4954 §4).
        assert!(matches!(
            parse("AUTH PLAIN =", false).unwrap(),
            Command::Auth { initial: Some(ref ir), .. } if ir == "="
        ));
        assert!(matches!(
            parse("AUTH", false),
            Err(CommandError::MissingParameter { .. })
        ));
        assert!(matches!(
            parse("AUTH PLAIN a b", false),
            Err(CommandError::BadParameter { .. })
        ));
    }
}
