//! Bounded CRLF line reading shared by the command loop and the DATA
//! reader. Limits are enforced *during* the read, never after
//! buffering (protocol skill non-negotiable); what each outcome means
//! (reply, abort, close) is the caller's policy, not this module's.

use tokio::io::AsyncBufRead;
use tokio::io::AsyncBufReadExt;

/// Outcome of reading one line under limits.
#[derive(Debug, PartialEq, Eq)]
pub enum RawLine {
    /// A complete CRLF-terminated line, terminator stripped.
    Line(Vec<u8>),
    /// Line exceeded `max_len`; the excess up to the next LF was
    /// drained so the stream stays parseable. `octets` is what was
    /// actually consumed, so callers can charge flood budgets with
    /// real numbers (an over-long line can consume far more than
    /// `max_len` before its LF arrives).
    TooLong {
        /// Total octets consumed for this line, including the drain.
        octets: usize,
    },
    /// The line ended in bare LF or contained a stray CR: the classic
    /// SMTP-smuggling shapes (RFC 5321 §2.3.8 requires CRLF; protocol
    /// skill: reject, do not guess, when ambiguity has security
    /// consequences).
    BadEol,
    /// More than `flood_cap` octets arrived without any LF.
    Flooded,
    /// Peer closed the connection (an unterminated tail is EOF too:
    /// a partial line is never delivered).
    Eof,
}

/// Reads one line of at most `max_len` octets (including CRLF),
/// draining over-long lines and detecting CRLF violations.
pub async fn read_raw_line<R>(
    reader: &mut R,
    max_len: usize,
    flood_cap: usize,
) -> std::io::Result<RawLine>
where
    R: AsyncBufRead + Unpin,
{
    let mut line: Vec<u8> = Vec::with_capacity(128);
    let mut overflowed = false;
    let mut drained: usize = 0;
    let mut consumed_total: usize = 0;

    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok(RawLine::Eof);
        }

        if let Some(newline_at) = available.iter().position(|&b| b == b'\n') {
            if !overflowed {
                line.extend_from_slice(&available[..=newline_at]);
            }
            reader.consume(newline_at + 1);
            consumed_total += newline_at + 1;

            if overflowed || line.len() > max_len {
                return Ok(RawLine::TooLong {
                    octets: consumed_total,
                });
            }
            if line.len() < 2 || line[line.len() - 2] != b'\r' {
                return Ok(RawLine::BadEol);
            }
            line.truncate(line.len() - 2);
            if line.contains(&b'\r') {
                return Ok(RawLine::BadEol);
            }
            return Ok(RawLine::Line(line));
        }

        let chunk = available.len();
        if !overflowed {
            line.extend_from_slice(available);
            if line.len() > max_len {
                overflowed = true;
                line.clear();
            }
        }
        reader.consume(chunk);
        consumed_total += chunk;
        drained += chunk;
        if drained > flood_cap {
            return Ok(RawLine::Flooded);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use tokio::io::BufReader;

    use super::*;

    async fn read_all(input: &[u8], max_len: usize) -> Vec<RawLine> {
        let mut reader = BufReader::new(input);
        let mut out = Vec::new();
        loop {
            let outcome = read_raw_line(&mut reader, max_len, 64 * 1024)
                .await
                .unwrap();
            let eof = outcome == RawLine::Eof;
            out.push(outcome);
            if eof {
                return out;
            }
        }
    }

    #[tokio::test]
    async fn crlf_line_round_trips() {
        let out = read_all(b"EHLO x\r\n", 512).await;
        assert_eq!(out[0], RawLine::Line(b"EHLO x".to_vec()));
    }

    #[tokio::test]
    async fn bare_lf_is_bad_eol() {
        let out = read_all(b"EHLO x\n", 512).await;
        assert_eq!(out[0], RawLine::BadEol);
    }

    #[tokio::test]
    async fn stray_cr_is_bad_eol() {
        let out = read_all(b"EH\rLO x\r\n", 512).await;
        assert_eq!(out[0], RawLine::BadEol);
    }

    #[tokio::test]
    async fn too_long_drains_recovers_and_reports_real_octets() {
        let mut input = vec![b'X'; 600];
        input.extend_from_slice(b"\r\nQUIT\r\n");
        let out = read_all(&input, 512).await;
        // 600 X + CRLF = 602 octets actually consumed — the flood
        // accounting must see the real number, not the cap.
        assert_eq!(out[0], RawLine::TooLong { octets: 602 });
        assert_eq!(out[1], RawLine::Line(b"QUIT".to_vec()));
    }

    #[tokio::test]
    async fn flood_without_newline_is_flooded() {
        let input = vec![b'X'; 70 * 1024];
        let mut reader = BufReader::new(input.as_slice());
        let outcome = read_raw_line(&mut reader, 512, 64 * 1024).await.unwrap();
        assert_eq!(outcome, RawLine::Flooded);
    }

    #[tokio::test]
    async fn unterminated_tail_is_eof() {
        let out = read_all(b"EHLO x", 512).await;
        assert_eq!(out[0], RawLine::Eof);
    }
}
