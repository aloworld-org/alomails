//! The DATA phase reader: collects message content after the 354
//! reply, RFC 5321 §4.1.1.4.
//!
//! Responsibilities: end-of-data detection (`CRLF.CRLF`),
//! dot-unstuffing (§4.5.2), the message size limit enforced *during*
//! the read, and rejection of bare-LF/CR line endings inside content —
//! the primary SMTP-smuggling vector, so those close the connection
//! rather than guess.

use tokio::io::AsyncBufRead;

use crate::line::{RawLine, read_raw_line};

/// Data lines may reach 1000 octets per RFC 5321 §4.5.3.1.6; real
/// mail (long HTML lines) routinely exceeds it, and receivers are
/// told to handle that gracefully. We accept up to a defensive cap
/// and reject beyond it (see docs/interop.md "Policies").
const DATA_LINE_LIMIT: usize = 8192;

/// Cap on octets drained while looking for the terminator after the
/// message is already over-size or over-long; beyond this the peer is
/// flooding and the connection must close.
const DRAIN_CAP: usize = 8 * 1024 * 1024;

/// Why DATA collection failed.
#[derive(Debug, thiserror::Error)]
pub enum DataError {
    /// Message exceeded the size limit; input was drained to the
    /// terminator, the session survives. Maps to 552.
    #[error("message exceeds the size limit")]
    TooLarge,
    /// A content line exceeded [`DATA_LINE_LIMIT`]; drained to the
    /// terminator, the session survives. Maps to 500.
    #[error("message line too long")]
    LineTooLong,
    /// Bare LF or stray CR inside content — smuggling shape; the
    /// caller replies 500 and CLOSES the connection.
    #[error("bare line ending inside message content")]
    BareLineEnding,
    /// Peer flooded past every cap without terminating; close.
    #[error("peer flooded the DATA channel")]
    Flooded,
    /// Peer disconnected before `CRLF.CRLF`; the partial message is
    /// discarded (RFC 5321 §4.1.1.4 — only a complete message counts).
    #[error("connection closed before end of data")]
    UnexpectedEof,
    /// Transport failure.
    #[error("I/O during DATA: {0}")]
    Io(#[from] std::io::Error),
}

/// Reads message content until `CRLF.CRLF`, returning the unstuffed
/// bytes (CRLF line endings preserved).
///
/// # Errors
/// See [`DataError`]; `TooLarge`/`LineTooLong` leave the stream
/// positioned after the terminator so the session can continue.
pub async fn read_message<R>(reader: &mut R, max_size: usize) -> Result<Vec<u8>, DataError>
where
    R: AsyncBufRead + Unpin,
{
    let mut message: Vec<u8> = Vec::with_capacity(4096);
    // Set when the message can no longer be accepted but we still
    // drain to the terminator to keep the session parseable.
    let mut failure: Option<DataError> = None;
    let mut drained: usize = 0;

    loop {
        let outcome = read_raw_line(reader, DATA_LINE_LIMIT, DRAIN_CAP).await?;
        let line = match outcome {
            RawLine::Line(line) => line,
            RawLine::TooLong { octets } => {
                failure.get_or_insert(DataError::LineTooLong);
                // Charge what was actually consumed (an over-long line
                // can be far larger than the line limit before its LF).
                drained += octets;
                if drained > DRAIN_CAP {
                    return Err(DataError::Flooded);
                }
                continue;
            }
            RawLine::BadEol => return Err(DataError::BareLineEnding),
            RawLine::Flooded => return Err(DataError::Flooded),
            RawLine::Eof => return Err(DataError::UnexpectedEof),
        };

        // End-of-data: a line that is exactly one dot (§4.1.1.4).
        if line == b"." {
            return match failure {
                Some(error) => Err(error),
                None => Ok(message),
            };
        }

        // Dot-unstuffing (§4.5.2): a leading dot doubling the first
        // octet is transparency, strip it.
        let content: &[u8] = if line.first() == Some(&b'.') {
            &line[1..]
        } else {
            &line
        };

        if failure.is_none() {
            // +2 for the CRLF this line contributes.
            if message.len() + content.len() + 2 > max_size {
                failure = Some(DataError::TooLarge);
                message.clear();
            } else {
                message.extend_from_slice(content);
                message.extend_from_slice(b"\r\n");
            }
        }
        drained += line.len();
        if drained > max_size.saturating_mul(2).max(DRAIN_CAP) {
            return Err(DataError::Flooded);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use tokio::io::BufReader;

    use super::*;

    async fn read(input: &[u8], max: usize) -> Result<Vec<u8>, DataError> {
        let mut reader = BufReader::new(input);
        read_message(&mut reader, max).await
    }

    #[tokio::test]
    async fn simple_message_collected_with_crlf_preserved() {
        let out = read(b"Subject: hi\r\n\r\nbody line\r\n.\r\n", 1024)
            .await
            .unwrap();
        assert_eq!(out, b"Subject: hi\r\n\r\nbody line\r\n");
    }

    #[tokio::test]
    async fn dot_stuffed_lines_are_unstuffed() {
        // RFC 5321 §4.5.2: "..x" on the wire is ".x" in content.
        let out = read(b"..leading dot\r\n...two dots\r\n.\r\n", 1024)
            .await
            .unwrap();
        assert_eq!(out, b".leading dot\r\n..two dots\r\n");
    }

    #[tokio::test]
    async fn lone_dot_line_ends_immediately_empty_message_ok() {
        let out = read(b".\r\n", 1024).await.unwrap();
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn oversize_is_rejected_but_drained_to_terminator() {
        let mut input = Vec::new();
        for _ in 0..100 {
            input.extend_from_slice(b"0123456789012345678901234567890123456789\r\n");
        }
        input.extend_from_slice(b".\r\nQUIT\r\n");
        let mut reader = BufReader::new(input.as_slice());
        let err = read_message(&mut reader, 512).await.unwrap_err();
        assert!(matches!(err, DataError::TooLarge));
        // The stream must be positioned after the terminator.
        let next = crate::line::read_raw_line(&mut reader, 512, 1024)
            .await
            .unwrap();
        assert_eq!(next, RawLine::Line(b"QUIT".to_vec()));
    }

    #[tokio::test]
    async fn bare_lf_inside_data_is_fatal() {
        let err = read(b"good line\r\nsmuggled\n.\r\n", 1024)
            .await
            .unwrap_err();
        assert!(matches!(err, DataError::BareLineEnding));
    }

    #[tokio::test]
    async fn eof_before_terminator_discards_message() {
        let err = read(b"partial\r\n", 1024).await.unwrap_err();
        assert!(matches!(err, DataError::UnexpectedEof));
    }
}
