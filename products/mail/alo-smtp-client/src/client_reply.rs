//! Parsing of server replies on the *client* side of SMTP
//! (RFC 5321 §4.2): single-line `NNN text` and multiline
//! `NNN-text ... NNN text` forms, with the reply-class semantics the
//! queue's retry logic depends on (2xx success, 4xx transient, 5xx
//! permanent — choosing wrong causes silent mail loss or infinite
//! retries).

use tokio::io::AsyncBufRead;

use crate::line::{RawLine, read_raw_line};

/// Reply lines are bounded like command lines; multiline replies are
/// bounded in line count so a hostile server cannot feed us forever.
const MAX_REPLY_LINE: usize = 1024;
const MAX_REPLY_LINES: usize = 64;
const REPLY_FLOOD_CAP: usize = 128 * 1024;

/// A complete server reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerReply {
    /// Three-digit code of the final line.
    pub code: u16,
    /// All text lines, joined with `\n` (diagnostics/DSN use).
    pub text: String,
}

impl ServerReply {
    /// 2xx: requested action completed.
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.code)
    }

    /// 4xx: transient — retry later (RFC 5321 §4.2.1).
    pub fn is_transient(&self) -> bool {
        (400..500).contains(&self.code)
    }

    /// First line of the reply text, for compact diagnostics.
    pub fn first_line(&self) -> &str {
        self.text.lines().next().unwrap_or("")
    }

    /// Whether this (EHLO) reply advertises an ESMTP capability keyword, e.g.
    /// `STARTTLS` — the keyword is the first token of a line, case-insensitive
    /// (RFC 5321 §4.1.1.1).
    pub fn advertises(&self, keyword: &str) -> bool {
        self.text.lines().any(|line| {
            line.split_whitespace()
                .next()
                .is_some_and(|word| word.eq_ignore_ascii_case(keyword))
        })
    }
}

/// Why reading a reply failed.
#[derive(Debug, thiserror::Error)]
pub enum ReplyError {
    /// The server sent something that is not an SMTP reply.
    #[error("malformed reply from server: {reason}")]
    Malformed {
        /// What was wrong, for the retry diagnostic.
        reason: String,
    },
    /// Connection closed before a complete reply arrived.
    #[error("connection closed mid-reply")]
    Disconnected,
    /// Transport failure.
    #[error("I/O reading reply: {0}")]
    Io(#[from] std::io::Error),
}

/// Reads one complete (possibly multiline) reply.
///
/// # Errors
/// [`ReplyError`]; all variants are treated as transient by the queue
/// (the remote server misbehaving now may behave on retry).
pub async fn read_reply<R>(reader: &mut R) -> Result<ServerReply, ReplyError>
where
    R: AsyncBufRead + Unpin,
{
    let mut text = String::new();
    for _ in 0..MAX_REPLY_LINES {
        let line = match read_raw_line(reader, MAX_REPLY_LINE, REPLY_FLOOD_CAP).await? {
            RawLine::Line(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            RawLine::Eof => return Err(ReplyError::Disconnected),
            RawLine::TooLong { .. } | RawLine::Flooded => {
                return Err(ReplyError::Malformed {
                    reason: "reply line exceeded limits".to_owned(),
                });
            }
            RawLine::BadEol => {
                return Err(ReplyError::Malformed {
                    reason: "reply line not CRLF-terminated".to_owned(),
                });
            }
        };

        let (code, separator, rest) = split_reply_line(&line)?;
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(rest);
        // §4.2.1: "-" continues the reply, space (or nothing) ends it.
        if separator != '-' {
            return Ok(ServerReply { code, text });
        }
    }
    Err(ReplyError::Malformed {
        reason: format!("reply exceeded {MAX_REPLY_LINES} lines"),
    })
}

fn split_reply_line(line: &str) -> Result<(u16, char, &str), ReplyError> {
    let malformed = |reason: &str| ReplyError::Malformed {
        reason: format!("{reason}: {line:.80}"),
    };
    if line.len() < 3 || !line.is_char_boundary(3) {
        return Err(malformed("reply shorter than a code"));
    }
    let code: u16 = line[..3]
        .parse()
        .map_err(|_| malformed("reply does not start with a 3-digit code"))?;
    if !(200..600).contains(&code) {
        return Err(malformed("reply code out of range"));
    }
    match line[3..].chars().next() {
        None => Ok((code, ' ', "")),
        Some(sep @ (' ' | '-')) => Ok((code, sep, &line[4..])),
        Some(_) => Err(malformed("reply code not followed by space or hyphen")),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use tokio::io::BufReader;

    use super::*;

    async fn parse(input: &[u8]) -> Result<ServerReply, ReplyError> {
        let mut reader = BufReader::new(input);
        read_reply(&mut reader).await
    }

    #[tokio::test]
    async fn single_line_reply() {
        let reply = parse(b"250 OK\r\n").await.unwrap();
        assert_eq!(reply.code, 250);
        assert_eq!(reply.text, "OK");
        assert!(reply.is_success());
    }

    #[tokio::test]
    async fn multiline_ehlo_reply() {
        // The form every real EHLO response takes (§4.2.1).
        let reply = parse(b"250-mx.example greets us\r\n250-PIPELINING\r\n250 SIZE 10240000\r\n")
            .await
            .unwrap();
        assert_eq!(reply.code, 250);
        assert_eq!(
            reply.text,
            "mx.example greets us\nPIPELINING\nSIZE 10240000"
        );
    }

    #[tokio::test]
    async fn code_only_reply_is_valid() {
        let reply = parse(b"421\r\n").await.unwrap();
        assert_eq!(reply.code, 421);
        assert_eq!(reply.text, "");
    }

    #[tokio::test]
    async fn classes_map_to_retry_semantics() {
        assert!(parse(b"250 ok\r\n").await.unwrap().is_success());
        assert!(parse(b"451 try later\r\n").await.unwrap().is_transient());
        let permanent = parse(b"550 no such user\r\n").await.unwrap();
        assert!(!permanent.is_success() && !permanent.is_transient());
    }

    #[tokio::test]
    async fn garbage_is_malformed_not_a_panic() {
        assert!(matches!(
            parse(b"hello world\r\n").await,
            Err(ReplyError::Malformed { .. })
        ));
        assert!(matches!(
            parse(b"99 too small\r\n").await,
            Err(ReplyError::Malformed { .. })
        ));
        assert!(matches!(
            parse("ФФФ multibyte\r\n".as_bytes()).await,
            Err(ReplyError::Malformed { .. })
        ));
    }

    #[tokio::test]
    async fn disconnect_mid_reply_is_reported() {
        assert!(matches!(
            parse(b"250-partial\r\n").await,
            Err(ReplyError::Disconnected)
        ));
    }

    #[tokio::test]
    async fn endless_multiline_is_bounded() {
        let mut input = Vec::new();
        for _ in 0..100 {
            input.extend_from_slice(b"250-more\r\n");
        }
        assert!(matches!(
            parse(&input).await,
            Err(ReplyError::Malformed { .. })
        ));
    }
}
