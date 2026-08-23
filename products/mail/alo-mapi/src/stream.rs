//! `RopOpenStream` and `RopReadStream` ([MS-OXCROPS] §2.2.9.1–2, [MS-OXCPRPT]
//! §2.2.14–15) — reading one property in pieces instead of all at once.
//!
//! This is how a client reads a value too large to come back inside a property
//! row. [`crate::properties`] marks such a value absent rather than truncating
//! it, and a client's response to that is to open a stream on the same property
//! and read it in chunks — which is what makes a long message body readable at
//! all.
//!
//! `RopOpenStream` request, 9 bytes:
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | `0x2B` |
//! | `LogonId` | 1 | |
//! | `InputHandleIndex` | 1 | the object holding the property |
//! | `OutputHandleIndex` | 1 | where the stream's handle goes |
//! | `PropertyTag` | 4 | which property to stream |
//! | `OpenModeFlags` | 1 | |
//!
//! Success response, 10 bytes: `RopId`, `OutputHandleIndex`, `ReturnValue` (4),
//! `StreamSize` (4).
//!
//! `RopReadStream` request, **5 or 9 bytes**:
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | `0x2C` |
//! | `LogonId` | 1 | |
//! | `InputHandleIndex` | 1 | the stream |
//! | `ByteCount` | 2 | how much to read — unless it is `0xBABE` |
//! | `MaximumByteCount` | 4 | present **only** when `ByteCount` is `0xBABE` |
//!
//! ## The `0xBABE` sentinel
//!
//! `ByteCount` is two bytes, so it cannot ask for more than 65 535. When a
//! client wants more it writes the sentinel `0xBABE` and a four-byte count
//! follows. **This makes the request variable-length**, and a parser that
//! assumed five bytes would read the next operation's first four bytes as a
//! count and then resume the walk in the middle of an operation — the exact
//! failure the dispatcher's "stop rather than guess" rule exists to avoid,
//! except that here it would not even look like a failure.
//!
//! Response: `RopId`, `InputHandleIndex`, `ReturnValue` (4), `DataSize` (2),
//! then that many bytes. `DataSize` is two bytes whatever the request asked
//! for, so one read can return at most 65 535 bytes and a client loops.
//!
//! ## What a stream is here
//!
//! A stream object remembers *which* property of which message it reads and
//! how far it has got — not the bytes themselves. The bytes come from the same
//! loaded message the rest of this crate reads, on every request, so a session
//! holding a stream open across many requests costs a cursor rather than a copy
//! of somebody's mail.
//!
//! Only reading is served. `ReadWrite` and `Create` are refused rather than
//! silently downgraded to read-only: a client that believed it had a writable
//! stream would send writes that vanished.

use crate::columns::PropertyTag;
use crate::rop::RopError;

/// The `RopId` of `RopOpenStream`.
pub const ROP_OPEN_STREAM: u8 = 0x2B;
/// The `RopId` of `RopReadStream`.
pub const ROP_READ_STREAM: u8 = 0x2C;

/// The fixed size of a `RopOpenStream` request.
pub const OPEN_REQUEST_LEN: usize = 9;
/// The size of a `RopOpenStream` success response.
pub const OPEN_RESPONSE_LEN: usize = 10;

/// The size of a `RopReadStream` request in its short form.
pub const READ_REQUEST_LEN: usize = 5;
/// The size of a `RopReadStream` request when the sentinel is used.
pub const READ_REQUEST_LEN_EXTENDED: usize = 9;

/// `ByteCount` value meaning "the real count is in `MaximumByteCount`".
pub const BYTE_COUNT_EXTENDED: u16 = 0xBABE;

/// `OpenModeFlags` — read-only ([MS-OXCPRPT] §2.2.14.1).
pub const OPEN_MODE_READ_ONLY: u8 = 0x00;
/// `OpenModeFlags` — read/write.
pub const OPEN_MODE_READ_WRITE: u8 = 0x01;
/// `OpenModeFlags` — create, discarding any current value.
pub const OPEN_MODE_CREATE: u8 = 0x02;

/// The most bytes one `RopReadStream` response can carry.
///
/// `DataSize` is two bytes, so this is the protocol's ceiling rather than a
/// choice of ours: a client asking for more gets this much and reads again.
pub const MAX_READ: usize = u16::MAX as usize;

/// A parsed `RopOpenStream` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenStreamRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the object with the property.
    pub input_handle_index: u8,
    /// The handle-table slot the stream's handle goes into.
    pub output_handle_index: u8,
    /// Which property to stream.
    pub property_tag: PropertyTag,
    /// How the stream is to be opened.
    pub open_mode_flags: u8,
}

impl OpenStreamRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if fewer than [`OPEN_REQUEST_LEN`] bytes remain,
    /// or the leading byte is not this operation.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let fixed = input.get(..OPEN_REQUEST_LEN).ok_or(RopError::Truncated {
            part: "RopOpenStream",
        })?;
        if fixed[0] != ROP_OPEN_STREAM {
            return Err(RopError::Truncated {
                part: "RopOpenStream",
            });
        }
        Ok((
            Self {
                logon_id: fixed[1],
                input_handle_index: fixed[2],
                output_handle_index: fixed[3],
                property_tag: PropertyTag::from_bytes([fixed[4], fixed[5], fixed[6], fixed[7]]),
                open_mode_flags: fixed[8],
            },
            &input[OPEN_REQUEST_LEN..],
        ))
    }

    /// Whether this asks for anything beyond reading.
    #[must_use]
    pub const fn wants_to_write(&self) -> bool {
        self.open_mode_flags != OPEN_MODE_READ_ONLY
    }
}

/// Builds the `RopOpenStream` success response.
#[must_use]
pub fn open_success_body(output_handle_index: u8, stream_size: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(OPEN_RESPONSE_LEN);
    out.push(ROP_OPEN_STREAM);
    out.push(output_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out.extend_from_slice(&stream_size.to_le_bytes());
    out
}

/// A parsed `RopReadStream` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadStreamRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the stream.
    pub input_handle_index: u8,
    /// How many bytes the client will accept.
    pub wanted: u32,
}

impl ReadStreamRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// Handles both forms: five bytes normally, and nine when `ByteCount` is
    /// the `0xBABE` sentinel.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if the request is short for the form it
    /// declares, or the leading byte is not this operation.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let fixed = input.get(..READ_REQUEST_LEN).ok_or(RopError::Truncated {
            part: "RopReadStream",
        })?;
        if fixed[0] != ROP_READ_STREAM {
            return Err(RopError::Truncated {
                part: "RopReadStream",
            });
        }
        let byte_count = u16::from_le_bytes([fixed[3], fixed[4]]);
        if byte_count != BYTE_COUNT_EXTENDED {
            return Ok((
                Self {
                    logon_id: fixed[1],
                    input_handle_index: fixed[2],
                    wanted: u32::from(byte_count),
                },
                &input[READ_REQUEST_LEN..],
            ));
        }

        // The sentinel: four more bytes follow, and the operation is longer.
        let extended = input
            .get(..READ_REQUEST_LEN_EXTENDED)
            .ok_or(RopError::Truncated {
                part: "RopReadStream",
            })?;
        Ok((
            Self {
                logon_id: fixed[1],
                input_handle_index: fixed[2],
                wanted: u32::from_le_bytes([extended[5], extended[6], extended[7], extended[8]]),
            },
            &input[READ_REQUEST_LEN_EXTENDED..],
        ))
    }
}

/// Builds the `RopReadStream` response around `data`.
///
/// The caller has already bounded `data` by [`MAX_READ`]; anything longer than
/// `DataSize` can express would be a length the client cannot read back.
#[must_use]
pub fn read_success_body(input_handle_index: u8, data: &[u8]) -> Vec<u8> {
    let size = u16::try_from(data.len()).unwrap_or(u16::MAX);
    let taken = &data[..usize::from(size)];
    let mut out = Vec::with_capacity(8 + taken.len());
    out.push(ROP_READ_STREAM);
    out.push(input_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(taken);
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{
        BYTE_COUNT_EXTENDED, MAX_READ, OPEN_MODE_CREATE, OPEN_MODE_READ_ONLY, OPEN_MODE_READ_WRITE,
        OPEN_REQUEST_LEN, OPEN_RESPONSE_LEN, OpenStreamRequest, ROP_OPEN_STREAM, ROP_READ_STREAM,
        ReadStreamRequest, open_success_body, read_success_body,
    };
    use crate::columns::PropertyTag;

    fn open_bytes(tag: PropertyTag, mode: u8) -> Vec<u8> {
        let mut out = vec![ROP_OPEN_STREAM, 0x00, 0x01, 0x02];
        out.extend_from_slice(&tag.to_bytes());
        out.push(mode);
        out
    }

    #[test]
    fn an_open_request_reads_its_property_tag() {
        let tag = PropertyTag {
            property_type: 0x001F,
            property_id: 0x1000,
        };
        let mut bytes = open_bytes(tag, OPEN_MODE_READ_ONLY);
        bytes.extend_from_slice(&[0xAA]);
        let (request, tail) = OpenStreamRequest::parse(&bytes).expect("parses");
        assert_eq!(request.property_tag, tag);
        assert_eq!(request.input_handle_index, 0x01);
        assert_eq!(request.output_handle_index, 0x02);
        assert!(!request.wants_to_write());
        assert_eq!(tail, &[0xAA]);
        assert_eq!(OPEN_REQUEST_LEN, 9);
    }

    #[test]
    fn writable_modes_are_recognised_as_writes() {
        // Not silently downgraded to read-only: a client that thought it had a
        // writable stream would send writes that went nowhere.
        for mode in [OPEN_MODE_READ_WRITE, OPEN_MODE_CREATE] {
            let bytes = open_bytes(
                PropertyTag {
                    property_type: 0x001F,
                    property_id: 0x1000,
                },
                mode,
            );
            let (request, _) = OpenStreamRequest::parse(&bytes).expect("parses");
            assert!(request.wants_to_write(), "mode {mode:#04x}");
        }
    }

    #[test]
    fn an_open_response_carries_the_stream_size() {
        let body = open_success_body(0x02, 70_000);
        assert_eq!(body.len(), OPEN_RESPONSE_LEN);
        assert_eq!(body[0], ROP_OPEN_STREAM);
        assert_eq!(body[1], 0x02);
        assert_eq!(&body[2..6], &0u32.to_le_bytes());
        assert_eq!(&body[6..10], &70_000u32.to_le_bytes());
    }

    #[test]
    fn a_short_read_request_is_five_bytes() {
        let mut bytes = vec![ROP_READ_STREAM, 0x00, 0x02];
        bytes.extend_from_slice(&4096u16.to_le_bytes());
        bytes.push(0xAA);
        let (request, tail) = ReadStreamRequest::parse(&bytes).expect("parses");
        assert_eq!(request.wanted, 4096);
        assert_eq!(request.input_handle_index, 0x02);
        assert_eq!(tail, &[0xAA], "the walk resumes right after five bytes");
    }

    #[test]
    fn the_sentinel_makes_the_request_four_bytes_longer() {
        // **The trap.** A parser fixed at five bytes would take the next
        // operation's first bytes as part of this one and resume the walk
        // mid-operation — which does not look like a failure, it looks like
        // the client asking for something else entirely.
        let mut bytes = vec![ROP_READ_STREAM, 0x00, 0x02];
        bytes.extend_from_slice(&BYTE_COUNT_EXTENDED.to_le_bytes());
        bytes.extend_from_slice(&200_000u32.to_le_bytes());
        bytes.push(0xAA);
        let (request, tail) = ReadStreamRequest::parse(&bytes).expect("parses");
        assert_eq!(request.wanted, 200_000);
        assert_eq!(tail, &[0xAA], "the walk resumes after nine bytes");
    }

    #[test]
    fn a_sentinel_with_no_count_after_it_is_refused() {
        let mut bytes = vec![ROP_READ_STREAM, 0x00, 0x02];
        bytes.extend_from_slice(&BYTE_COUNT_EXTENDED.to_le_bytes());
        bytes.extend_from_slice(&[0x01, 0x02]); // only two of the four
        assert!(ReadStreamRequest::parse(&bytes).is_err());
    }

    #[test]
    fn a_read_response_never_claims_more_than_datasize_can_hold() {
        // `DataSize` is two bytes, so a response cannot describe more than
        // 65 535 bytes however much the caller passed.
        let data = vec![0x41u8; MAX_READ + 10];
        let body = read_success_body(0x02, &data);
        let size = u16::from_le_bytes(body[6..8].try_into().unwrap());
        assert_eq!(usize::from(size), MAX_READ);
        assert_eq!(body.len(), 8 + MAX_READ);
    }

    #[test]
    fn a_read_response_at_the_end_of_a_stream_is_empty_and_successful() {
        // Not an error: a client reads until it gets nothing back, and an
        // error here would look like the stream had broken.
        let body = read_success_body(0x02, &[]);
        assert_eq!(body[0], ROP_READ_STREAM);
        assert_eq!(&body[2..6], &0u32.to_le_bytes());
        assert_eq!(u16::from_le_bytes(body[6..8].try_into().unwrap()), 0);
        assert_eq!(body.len(), 8);
    }

    #[test]
    fn another_operations_bytes_are_neither_of_these() {
        assert!(ReadStreamRequest::parse(&[ROP_OPEN_STREAM, 0, 1, 0, 0]).is_err());
        let mut bytes = open_bytes(
            PropertyTag {
                property_type: 0x001F,
                property_id: 0x1000,
            },
            OPEN_MODE_READ_ONLY,
        );
        bytes[0] = ROP_READ_STREAM;
        assert!(OpenStreamRequest::parse(&bytes).is_err());
    }
}
