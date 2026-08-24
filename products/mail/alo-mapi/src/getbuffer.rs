//! `RopFastTransferSourceGetBuffer` ([MS-OXCROPS] §2.2.12.4, [MS-OXCFXICS]
//! §2.2.3.1.1.5) — the operation a client repeats until its mailbox has arrived.
//!
//! A download context produced by `RopSynchronizationConfigure` holds a
//! FastTransfer stream. This hands that stream over a chunk at a time: the
//! client asks for at most so many bytes, the server answers with a piece and
//! says whether more is coming.
//!
//! ## Request
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | `0x4E` |
//! | `LogonId` | 1 | |
//! | `InputHandleIndex` | 1 | the download context |
//! | `BufferSize` | 2 | the most the client wants back |
//! | `MaximumBufferSize` | 2 | present **only** when `BufferSize` is `0xBABE` |
//!
//! `0xBABE` is a sentinel, not a size: it means "you choose, but no more than
//! `MaximumBufferSize`" ([MS-OXCFXICS] §2.2.3.1.1.5.1). Reading it as a literal
//! 47,806 bytes would work by accident on most requests and then desynchronise
//! the buffer, because the two extra bytes that follow it would be parsed as
//! the next operation.
//!
//! ## Response
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | |
//! | `InputHandleIndex` | 1 | |
//! | `ReturnValue` | 4 | |
//! | `TransferStatus` | 2 | |
//! | `InProgressCount` | 2 | progress display only |
//! | `TotalStepCount` | 2 | progress display only |
//! | `Reserved` | 1 | zero |
//! | `TransferBufferSize` | 2 | |
//! | `TransferBuffer` | variable | the chunk |
//!
//! `TransferBufferSize` is 16 bits, so no single chunk can exceed 64 KiB — a
//! ceiling the whole chunking design exists to live within.

use std::sync::Arc;

use crate::fasttransfer::{FtError, safe_split};
use crate::rop::RopError;

/// The `RopId` of `RopFastTransferSourceGetBuffer`.
pub const ROP_FAST_TRANSFER_SOURCE_GET_BUFFER: u8 = 0x4E;

/// The `BufferSize` value meaning "server's choice" ([MS-OXCFXICS]
/// §2.2.3.1.1.5.1). Followed by a `MaximumBufferSize`, and only then.
pub const BUFFER_SIZE_SENTINEL: u16 = 0xBABE;

/// Bytes before the optional `MaximumBufferSize`.
const REQUEST_FIXED_LEN: usize = 5;

/// Bytes of response before `TransferBuffer`.
pub const RESPONSE_HEADER_LEN: usize = 15;

/// The largest chunk that can be described, since `TransferBufferSize` is
/// 16 bits.
pub const MAX_CHUNK: usize = u16::MAX as usize;

/// What we hand back when the client leaves the size to us and names no
/// ceiling of its own.
pub const DEFAULT_CHUNK: usize = 32 * 1024;

/// How a download stands after producing a chunk ([MS-OXCFXICS]
/// §2.2.3.1.1.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferStatus {
    /// A non-recoverable error; `ReturnValue` carries the reason.
    Error,
    /// The stream was split and more is available.
    Partial,
    /// The stream was split because the chunk would not fit.
    NoRoom,
    /// That was the last of it.
    Done,
}

impl TransferStatus {
    /// The value on the wire.
    #[must_use]
    pub fn as_u16(self) -> u16 {
        match self {
            Self::Error => 0x0000,
            Self::Partial => 0x0001,
            Self::NoRoom => 0x0002,
            Self::Done => 0x0003,
        }
    }
}

/// A parsed `RopFastTransferSourceGetBuffer` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetBufferRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the download context.
    pub input_handle_index: u8,
    /// What the client asked for, before the sentinel is resolved.
    pub buffer_size: u16,
    /// The ceiling the client set when it left the size to us.
    pub maximum_buffer_size: Option<u16>,
}

impl GetBufferRequest {
    /// How many bytes this request actually permits.
    ///
    /// Resolves the sentinel, and clamps to what `TransferBufferSize` can
    /// describe — a client asking for more than 64 KiB cannot be answered with
    /// more than 64 KiB whatever it says.
    #[must_use]
    pub fn limit(&self) -> usize {
        let asked = if self.buffer_size == BUFFER_SIZE_SENTINEL {
            self.maximum_buffer_size
                .map_or(DEFAULT_CHUNK, |max| usize::from(max).min(DEFAULT_CHUNK))
        } else {
            usize::from(self.buffer_size)
        };
        asked.min(MAX_CHUNK)
    }

    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    ///
    /// [`RopError::Truncated`] if the buffer ends inside a field or the leading
    /// byte is not this operation.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let fixed = input.get(..REQUEST_FIXED_LEN).ok_or(RopError::Truncated {
            part: "RopFastTransferSourceGetBuffer",
        })?;
        if fixed[0] != ROP_FAST_TRANSFER_SOURCE_GET_BUFFER {
            return Err(RopError::Truncated {
                part: "RopFastTransferSourceGetBuffer",
            });
        }

        let buffer_size = u16::from_le_bytes([fixed[3], fixed[4]]);
        let mut at = REQUEST_FIXED_LEN;

        // Present if and only if the sentinel was sent. Reading it when it is
        // absent — or skipping it when it is there — leaves the rest of the ROP
        // buffer shifted by two bytes, and every later operation garbage.
        let maximum_buffer_size = if buffer_size == BUFFER_SIZE_SENTINEL {
            let extra = input.get(at..at + 2).ok_or(RopError::Truncated {
                part: "RopFastTransferSourceGetBuffer MaximumBufferSize",
            })?;
            at += 2;
            Some(u16::from_le_bytes([extra[0], extra[1]]))
        } else {
            None
        };

        Ok((
            Self {
                logon_id: fixed[1],
                input_handle_index: fixed[2],
                buffer_size,
                maximum_buffer_size,
            },
            &input[at..],
        ))
    }
}

/// A stream being handed out a chunk at a time.
///
/// Holds the whole serialised stream and how far the client has taken. The
/// position is absolute so that [`safe_split`] can see the structure from the
/// beginning even when the previous chunk stopped inside a value.
///
/// **The bytes are shared and the position is not.** A download lives in the
/// session's object table, which the router *clones* before rehearsing a ROP
/// buffer — up to three times per request. Copying a mailbox-sized stream that
/// often would dominate the cost of every synchronising request, so the stream
/// sits behind an [`Arc`] and only the cursor is duplicated. That is also the
/// behaviour the rehearsal needs: the clone advances, the original does not,
/// and the real dispatch afterwards starts from where the client actually left
/// off.
#[derive(Debug, Clone)]
pub struct Download {
    stream: Arc<[u8]>,
    sent: usize,
    /// How many items the stream describes, for the progress fields.
    total_steps: u16,
    /// How many have been passed so far.
    steps_done: u16,
}

impl Download {
    /// A download of `stream`, describing `total_steps` items.
    #[must_use]
    pub fn new(stream: Vec<u8>, total_steps: u16) -> Self {
        Self {
            stream: Arc::from(stream.into_boxed_slice()),
            sent: 0,
            total_steps,
            steps_done: 0,
        }
    }

    /// A download sharing another's bytes, from the beginning.
    ///
    /// Used when a client configures the same scope twice: the stream is
    /// already built, and rebuilding it would read the whole folder again.
    #[must_use]
    pub fn restart(&self) -> Self {
        Self {
            stream: Arc::clone(&self.stream),
            sent: 0,
            total_steps: self.total_steps,
            steps_done: 0,
        }
    }

    /// Whether everything has been handed over.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.sent >= self.stream.len()
    }

    /// How many bytes remain.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.stream.len().saturating_sub(self.sent)
    }

    /// Takes the next chunk of at most `limit` bytes.
    ///
    /// Returns the chunk and the status that goes with it. Advances only on
    /// success, so a caller that fails to send the response can ask again.
    ///
    /// # Errors
    ///
    /// Returns [`FtError`] if the stream is malformed, or if a single element
    /// is too large to travel in `limit` bytes — the latter cannot happen for a
    /// stream this crate produced, since any value large enough may be split
    /// inside, but it is reported rather than looping on an empty chunk.
    pub fn next_chunk(&mut self, limit: usize) -> Result<(Vec<u8>, TransferStatus), FtError> {
        if self.is_finished() {
            return Ok((Vec::new(), TransferStatus::Done));
        }

        let end = safe_split(&self.stream, self.sent, limit.max(1))?;
        if end <= self.sent {
            return Err(FtError::Malformed {
                part: "an element larger than the client's buffer",
            });
        }

        let chunk = self.stream[self.sent..end].to_vec();
        self.sent = end;

        // Progress is display-only, so it is derived from bytes rather than
        // tracked per item: an approximate bar is what the field is for, and a
        // second bookkeeping path is a second thing to get wrong.
        if self.total_steps > 0 {
            let fraction = (self.sent as u128 * u128::from(self.total_steps))
                / (self.stream.len().max(1) as u128);
            self.steps_done = u16::try_from(fraction).unwrap_or(self.total_steps);
        }

        let status = if self.is_finished() {
            TransferStatus::Done
        } else {
            TransferStatus::Partial
        };
        Ok((chunk, status))
    }

    /// The progress pair for the response: done, and total.
    #[must_use]
    pub fn progress(&self) -> (u16, u16) {
        (self.steps_done, self.total_steps)
    }
}

/// Builds the `RopFastTransferSourceGetBuffer` success response.
///
/// # Panics
///
/// Never: the chunk is clamped to what `TransferBufferSize` can describe.
#[must_use]
pub fn get_buffer_body(
    input_handle_index: u8,
    status: TransferStatus,
    progress: (u16, u16),
    chunk: &[u8],
) -> Vec<u8> {
    let chunk = &chunk[..chunk.len().min(MAX_CHUNK)];
    let mut out = Vec::with_capacity(RESPONSE_HEADER_LEN + chunk.len());
    out.push(ROP_FAST_TRANSFER_SOURCE_GET_BUFFER);
    out.push(input_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out.extend_from_slice(&status.as_u16().to_le_bytes());
    out.extend_from_slice(&progress.0.to_le_bytes());
    out.extend_from_slice(&progress.1.to_le_bytes());
    out.push(0x00); // Reserved.
    let size = u16::try_from(chunk.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(chunk);
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::fasttransfer::{Writer, marker};

    #[test]
    fn a_plain_request_carries_no_maximum() {
        let buf = vec![ROP_FAST_TRANSFER_SOURCE_GET_BUFFER, 0x00, 0x01, 0x00, 0x40];
        let (parsed, rest) = GetBufferRequest::parse(&buf).unwrap();
        assert!(rest.is_empty());
        assert_eq!(parsed.input_handle_index, 0x01);
        assert_eq!(parsed.buffer_size, 0x4000);
        assert_eq!(parsed.maximum_buffer_size, None);
        assert_eq!(parsed.limit(), 0x4000);
    }

    /// The sentinel is a word meaning "you choose", not a size. Treating it as
    /// one would also leave two unread bytes that the next operation would be
    /// parsed from.
    #[test]
    fn the_sentinel_pulls_in_a_maximum_and_is_not_a_size() {
        let mut buf = vec![ROP_FAST_TRANSFER_SOURCE_GET_BUFFER, 0x00, 0x01];
        buf.extend_from_slice(&BUFFER_SIZE_SENTINEL.to_le_bytes());
        buf.extend_from_slice(&4096u16.to_le_bytes());
        buf.extend_from_slice(&[0x01, 0x00, 0x00]); // a RopRelease behind it

        let (parsed, rest) = GetBufferRequest::parse(&buf).unwrap();
        assert_eq!(parsed.maximum_buffer_size, Some(4096));
        assert_eq!(parsed.limit(), 4096, "the sentinel was taken as a size");
        assert_eq!(rest, &[0x01, 0x00, 0x00], "the next operation was consumed");
    }

    /// A sentinel with no maximum behind it is truncated, not a default.
    #[test]
    fn a_sentinel_without_its_maximum_is_refused() {
        let mut buf = vec![ROP_FAST_TRANSFER_SOURCE_GET_BUFFER, 0x00, 0x01];
        buf.extend_from_slice(&BUFFER_SIZE_SENTINEL.to_le_bytes());
        assert!(GetBufferRequest::parse(&buf).is_err());
    }

    /// `TransferBufferSize` is 16 bits, so nothing larger can be described.
    #[test]
    fn the_limit_is_clamped_to_what_the_size_field_can_say() {
        let mut buf = vec![ROP_FAST_TRANSFER_SOURCE_GET_BUFFER, 0x00, 0x01];
        buf.extend_from_slice(&BUFFER_SIZE_SENTINEL.to_le_bytes());
        buf.extend_from_slice(&u16::MAX.to_le_bytes());
        let (parsed, _) = GetBufferRequest::parse(&buf).unwrap();
        assert!(parsed.limit() <= MAX_CHUNK);
    }

    /// A stream smaller than one buffer arrives whole and says so.
    #[test]
    fn a_short_stream_is_done_in_one_chunk() {
        let mut w = Writer::new();
        w.marker(marker::START_MESSAGE);
        w.string(0x0037, "Rechnung");
        w.marker(marker::END_MESSAGE);
        let stream = w.finish();

        let mut download = Download::new(stream.clone(), 1);
        let (chunk, status) = download.next_chunk(4096).unwrap();
        assert_eq!(chunk, stream);
        assert_eq!(status, TransferStatus::Done);
        assert!(download.is_finished());

        // Asking again is harmless and still Done.
        let (empty, status) = download.next_chunk(4096).unwrap();
        assert!(empty.is_empty());
        assert_eq!(status, TransferStatus::Done);
    }

    /// A long stream comes in pieces that reassemble to exactly the original,
    /// with Partial until the last one.
    #[test]
    fn a_long_stream_reassembles_byte_for_byte() {
        let mut w = Writer::new();
        w.marker(marker::START_MESSAGE);
        w.binary(0x3701, &vec![0x5A; 150_000]);
        w.marker(marker::END_MESSAGE);
        let stream = w.finish();

        let mut download = Download::new(stream.clone(), 3);
        let mut rebuilt = Vec::new();
        let mut statuses = Vec::new();
        for _ in 0..100 {
            let (chunk, status) = download.next_chunk(16_384).unwrap();
            rebuilt.extend_from_slice(&chunk);
            statuses.push(status);
            if status == TransferStatus::Done {
                break;
            }
        }

        assert_eq!(rebuilt, stream, "the stream did not survive chunking");
        assert_eq!(statuses.last(), Some(&TransferStatus::Done));
        assert!(
            statuses[..statuses.len() - 1]
                .iter()
                .all(|s| *s == TransferStatus::Partial),
            "an intermediate chunk did not say more was coming"
        );
    }

    /// Progress only ever moves forward and never overshoots the total.
    #[test]
    fn progress_is_monotonic_and_bounded() {
        let mut w = Writer::new();
        w.binary(0x3701, &vec![0x11; 100_000]);
        let stream = w.finish();

        let mut download = Download::new(stream, 10);
        let mut last = 0;
        loop {
            let (_, status) = download.next_chunk(8192).unwrap();
            let (done, total) = download.progress();
            assert_eq!(total, 10);
            assert!(done >= last, "progress went backwards: {last} then {done}");
            assert!(done <= total, "progress overshot: {done} of {total}");
            last = done;
            if status == TransferStatus::Done {
                break;
            }
        }
        assert_eq!(last, 10, "progress did not reach the total");
    }

    /// The response is the header the specification gives, then the chunk.
    #[test]
    fn the_response_lays_its_fields_out_in_order() {
        let body = get_buffer_body(0x01, TransferStatus::Partial, (2, 7), &[0xAA, 0xBB]);
        assert_eq!(
            body,
            vec![
                ROP_FAST_TRANSFER_SOURCE_GET_BUFFER,
                0x01, // InputHandleIndex
                0x00,
                0x00,
                0x00,
                0x00, // ReturnValue
                0x01,
                0x00, // TransferStatus: Partial
                0x02,
                0x00, // InProgressCount
                0x07,
                0x00, // TotalStepCount
                0x00, // Reserved
                0x02,
                0x00, // TransferBufferSize
                0xAA,
                0xBB,
            ]
        );
        assert_eq!(body.len(), RESPONSE_HEADER_LEN + 2);
    }

    /// The four statuses are the four values the specification names.
    #[test]
    fn transfer_status_values_match_the_specification() {
        assert_eq!(TransferStatus::Error.as_u16(), 0x0000);
        assert_eq!(TransferStatus::Partial.as_u16(), 0x0001);
        assert_eq!(TransferStatus::NoRoom.as_u16(), 0x0002);
        assert_eq!(TransferStatus::Done.as_u16(), 0x0003);
    }

    /// A different operation's bytes must not parse as this one.
    #[test]
    fn another_operation_is_not_mistaken_for_this_one() {
        let buf = vec![0x4F, 0x00, 0x01, 0x00, 0x40];
        assert!(GetBufferRequest::parse(&buf).is_err());
        assert!(GetBufferRequest::parse(&[]).is_err());
    }

    /// The property the whole rehearsal depends on: a clone advances on its
    /// own and leaves the original where it was.
    ///
    /// The router clones the object table, rehearses a ROP buffer against the
    /// clone to learn what it needs to load, then throws the clone away and
    /// dispatches for real. If a rehearsal's chunk advanced the true cursor,
    /// the client would silently lose that many bytes out of the middle of its
    /// mailbox — no error, just a gap.
    #[test]
    fn a_clone_advances_without_moving_the_original() {
        let mut w = Writer::new();
        w.binary(0x3701, &vec![0x2C; 40_000]);
        let original = Download::new(w.finish(), 1);

        let mut rehearsal = original.clone();
        let (chunk, status) = rehearsal.next_chunk(1024).unwrap();
        assert_eq!(chunk.len(), 1024);
        assert_eq!(status, TransferStatus::Partial);

        assert_eq!(
            original.remaining(),
            original.remaining(),
            "sanity: remaining is stable"
        );
        assert!(
            rehearsal.remaining() < original.remaining(),
            "the clone did not advance"
        );
        assert_eq!(
            original.remaining(),
            rehearsal.remaining() + 1024,
            "the rehearsal moved the original's cursor"
        );

        // And the real dispatch afterwards starts from the beginning.
        let mut real = original.clone();
        let (first, _) = real.next_chunk(1024).unwrap();
        assert_eq!(
            first, chunk,
            "the real chunk differs from the rehearsed one"
        );
    }

    /// Restarting shares the bytes but rewinds the cursor, so re-configuring
    /// the same scope does not re-read the folder.
    #[test]
    fn restart_rewinds_without_rebuilding() {
        let mut w = Writer::new();
        w.marker(marker::START_MESSAGE);
        w.string(0x0037, "Rechnung");
        w.marker(marker::END_MESSAGE);
        let mut download = Download::new(w.finish(), 1);

        let (whole, status) = download.next_chunk(4096).unwrap();
        assert_eq!(status, TransferStatus::Done);
        assert!(download.is_finished());

        let mut again = download.restart();
        assert!(!again.is_finished());
        let (repeat, status) = again.next_chunk(4096).unwrap();
        assert_eq!(repeat, whole, "restart produced a different stream");
        assert_eq!(status, TransferStatus::Done);
    }
}
