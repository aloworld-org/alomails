//! ROP buffers ([MS-OXCROPS] §2.2.1) — the list of operations and the handle
//! table they address each other through.
//!
//! ```text
//! | RopSize (2) | RopsList (RopSize - 2) | ServerObjectHandleTable (rest) |
//! ```
//!
//! Two things about that layout catch people out, and both are stated in the
//! specification rather than implied:
//!
//! * **`RopSize` counts itself.** It "specifies the size of both this field and
//!   the `RopsList` field", so the operations occupy `RopSize - 2` bytes. Read
//!   as the length of the list alone, every buffer is two bytes long in the
//!   wrong direction and the handle table is misparsed with it.
//! * **The handle table is whatever remains.** It has no count of its own: "the
//!   size of this field is equal to the number of bytes of data remaining in
//!   the ROP input/output buffer after the `RopsList` field". So a trailing
//!   byte that is not a whole number of 32-bit handles is a malformed buffer,
//!   not a handle table with a remainder.
//!
//! Operations reference server objects **by index into that table**, not by
//! handle, which is what lets a client pipeline: one ROP can name the index a
//! previous ROP in the same buffer wrote its result into.
//!
//! Every ROP request begins with the same three bytes — `RopId`, `LogonId`,
//! `InputHandleIndex` — and continues with a body whose shape depends on the
//! `RopId`. This module reads the container and that common header; the bodies
//! belong to the modules that implement each operation.

/// The value a client writes into a handle-table entry that an operation is
/// expected to fill in ([MS-OXCROPS] §3.1.4.1).
pub const HANDLE_UNSET: u32 = 0xFFFF_FFFF;

/// The most operations we will read from one buffer.
///
/// A bound on work, not on bytes: each ROP is only three bytes of header, so a
/// buffer within every size limit can still ask for an enormous number of them.
pub const MAX_ROPS: usize = 4096;

/// Why a ROP buffer could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RopError {
    /// The buffer ended in the middle of a field.
    #[error("ROP buffer truncated in {part}")]
    Truncated {
        /// Which part ran out.
        part: &'static str,
    },
    /// `RopSize` is smaller than the field itself, or larger than the buffer.
    #[error("RopSize of {size} does not fit a buffer of {available} bytes")]
    RopSize {
        /// What the client declared.
        size: u16,
        /// What actually arrived.
        available: usize,
    },
    /// The bytes after the ROP list are not a whole number of handles.
    #[error("handle table has {remainder} bytes left over")]
    HandleTable {
        /// The bytes that do not complete a handle.
        remainder: usize,
    },
    /// The buffer asks for more operations than we will process.
    #[error("ROP buffer contains more than {MAX_ROPS} operations")]
    TooManyRops,
}

/// The three bytes every ROP request begins with ([MS-OXCROPS] §2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RopHeader {
    /// Which operation this is.
    pub rop_id: u8,
    /// Which logon the operation belongs to.
    pub logon_id: u8,
    /// The handle-table index this operation reads its input object from.
    pub input_handle_index: u8,
}

impl RopHeader {
    /// Reads the common header from the front of a ROP request.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if fewer than three bytes remain.
    pub fn parse(input: &[u8]) -> Result<Self, RopError> {
        let head = input.get(..3).ok_or(RopError::Truncated { part: "ROP" })?;
        Ok(Self {
            rop_id: head[0],
            logon_id: head[1],
            input_handle_index: head[2],
        })
    }
}

/// A parsed ROP buffer: the raw operation list and the handle table.
///
/// The operations are kept as bytes rather than decoded here. Each `RopId` has
/// its own body format, and only a dispatcher that knows the operation can say
/// where one request ends and the next begins — so splitting the list is the
/// dispatcher's job, not the container's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RopBuffer {
    /// The concatenated ROP requests, `RopSize - 2` bytes of them.
    pub rops: Vec<u8>,
    /// The server object handles the operations address by index.
    pub handles: Vec<u32>,
}

impl RopBuffer {
    /// Parses a ROP input buffer ([MS-OXCROPS] §2.2.1).
    ///
    /// # Errors
    /// [`RopError`] if the buffer is truncated, `RopSize` is impossible, or the
    /// trailing bytes are not a whole number of handles.
    pub fn parse(input: &[u8]) -> Result<Self, RopError> {
        let head = input
            .get(..2)
            .ok_or(RopError::Truncated { part: "RopSize" })?;
        let rop_size = u16::from_le_bytes([head[0], head[1]]);

        // `RopSize` counts itself, so anything below two is not a length at
        // all — and a buffer of exactly two is a legal empty list.
        if usize::from(rop_size) < 2 || usize::from(rop_size) > input.len() {
            return Err(RopError::RopSize {
                size: rop_size,
                available: input.len(),
            });
        }
        let list_len = usize::from(rop_size) - 2;
        let rops = input
            .get(2..2 + list_len)
            .ok_or(RopError::Truncated { part: "RopsList" })?;

        // Three bytes is the smallest a ROP can be, so this bounds the work a
        // single buffer can ask for without reading the operations themselves.
        if list_len / 3 > MAX_ROPS {
            return Err(RopError::TooManyRops);
        }

        let table = input.get(2 + list_len..).ok_or(RopError::Truncated {
            part: "handle table",
        })?;
        if table.len() % 4 != 0 {
            return Err(RopError::HandleTable {
                remainder: table.len() % 4,
            });
        }
        let handles = table
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();

        Ok(Self {
            rops: rops.to_vec(),
            handles,
        })
    }

    /// The handle at `index`, if the table has one there.
    ///
    /// Returns `None` for an index past the end **and** for an entry the client
    /// left unset — an operation that names either has not given us a server
    /// object, and treating `0xFFFFFFFF` as a handle would look one up.
    #[must_use]
    pub fn handle(&self, index: u8) -> Option<u32> {
        match self.handles.get(usize::from(index)) {
            Some(&HANDLE_UNSET) | None => None,
            Some(&handle) => Some(handle),
        }
    }

    /// Serialises a ROP output buffer.
    ///
    /// # Errors
    /// [`RopError::RopSize`] if the response list is too long to describe in
    /// the 16-bit `RopSize` field.
    pub fn to_bytes(&self) -> Result<Vec<u8>, RopError> {
        let size = u16::try_from(self.rops.len() + 2).map_err(|_| RopError::RopSize {
            size: u16::MAX,
            available: self.rops.len(),
        })?;
        let mut out = Vec::with_capacity(2 + self.rops.len() + self.handles.len() * 4);
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&self.rops);
        for handle in &self.handles {
            out.extend_from_slice(&handle.to_le_bytes());
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// The specification's own worked example ([MS-OXCROPS] §4.2): two
    /// `RopRelease` requests and two handles, byte for byte.
    ///
    /// Using the specification's bytes rather than our own encoder's is the
    /// point — a round trip through code we also wrote would agree with itself
    /// even if both halves misread the layout.
    const SPEC_EXAMPLE: &[u8] = &[
        0x08, 0x00, // RopSize — counts itself, so the list is 6 bytes
        0x01, 0x00, 0x00, // RopRelease, logon 0, input handle index 0
        0x01, 0x00, 0x01, // RopRelease, logon 0, input handle index 1
        0x6F, 0x00, 0x00, 0x00, // handle 0
        0x6E, 0x00, 0x00, 0x00, // handle 1
    ];

    #[test]
    fn the_specifications_worked_example_parses_byte_for_byte() {
        let buffer = RopBuffer::parse(SPEC_EXAMPLE).unwrap();

        // Six bytes of operations: RopSize counts itself, and reading it as the
        // length of the list alone is the mistake this pins.
        assert_eq!(buffer.rops.len(), 6);
        assert_eq!(buffer.handles, vec![0x6F, 0x6E]);

        let first = RopHeader::parse(&buffer.rops).unwrap();
        assert_eq!(first.rop_id, 0x01, "RopRelease");
        assert_eq!(first.logon_id, 0);
        assert_eq!(first.input_handle_index, 0);

        let second = RopHeader::parse(&buffer.rops[3..]).unwrap();
        assert_eq!(second.rop_id, 0x01);
        assert_eq!(second.input_handle_index, 1);

        // The indices resolve to the handles the example names.
        assert_eq!(buffer.handle(first.input_handle_index), Some(0x6F));
        assert_eq!(buffer.handle(second.input_handle_index), Some(0x6E));
    }

    /// What we write, the parser reads back — and the example's own bytes come
    /// out unchanged, which is the stronger statement.
    #[test]
    fn a_buffer_round_trips_to_the_same_bytes() {
        let buffer = RopBuffer::parse(SPEC_EXAMPLE).unwrap();
        assert_eq!(buffer.to_bytes().unwrap(), SPEC_EXAMPLE);
    }

    /// An empty list is legal: `RopSize` of exactly two, no operations, and a
    /// handle table that may still carry entries.
    #[test]
    fn an_empty_rop_list_is_legal() {
        let raw = [0x02, 0x00, 0x01, 0x00, 0x00, 0x00];
        let buffer = RopBuffer::parse(&raw).unwrap();
        assert!(buffer.rops.is_empty());
        assert_eq!(buffer.handles, vec![1]);
    }

    /// `RopSize` below two is not a length, and above the buffer is a lie.
    /// Both are refused rather than clamped — clamping would parse some other
    /// buffer than the one that arrived.
    #[test]
    fn an_impossible_rop_size_is_refused() {
        for size in [0u16, 1] {
            let mut raw = SPEC_EXAMPLE.to_vec();
            raw[0..2].copy_from_slice(&size.to_le_bytes());
            assert!(
                matches!(RopBuffer::parse(&raw), Err(RopError::RopSize { .. })),
                "accepted RopSize of {size}"
            );
        }
        let mut raw = SPEC_EXAMPLE.to_vec();
        raw[0..2].copy_from_slice(&9_000u16.to_le_bytes());
        assert!(matches!(
            RopBuffer::parse(&raw),
            Err(RopError::RopSize { .. })
        ));
    }

    /// The handle table has no length of its own — it is whatever remains — so
    /// bytes that do not complete a 32-bit handle mean the buffer is malformed,
    /// not that the table has a remainder to ignore.
    #[test]
    fn a_handle_table_that_is_not_whole_handles_is_refused() {
        for extra in 1..4 {
            let mut raw = SPEC_EXAMPLE.to_vec();
            raw.extend(std::iter::repeat_n(0u8, extra));
            assert_eq!(
                RopBuffer::parse(&raw),
                Err(RopError::HandleTable { remainder: extra }),
                "accepted {extra} trailing byte(s)"
            );
        }
    }

    /// An index past the table, and an entry the client left unset, are both
    /// "no object" — treating `0xFFFFFFFF` as a handle would send us looking
    /// one up.
    #[test]
    fn an_unset_or_missing_handle_is_not_an_object() {
        let raw = [
            0x05, 0x00, // RopSize: one 3-byte ROP
            0x01, 0x00, 0x00, // the ROP
            0xFF, 0xFF, 0xFF, 0xFF, // an entry the client left for us to fill
        ];
        let buffer = RopBuffer::parse(&raw).unwrap();
        assert_eq!(buffer.handles, vec![HANDLE_UNSET]);
        assert_eq!(buffer.handle(0), None, "unset read as a handle");
        assert_eq!(buffer.handle(1), None, "past the end read as a handle");
        assert_eq!(buffer.handle(255), None);
    }

    #[test]
    fn every_truncation_is_an_error() {
        for cut in 0..SPEC_EXAMPLE.len() {
            // Cuts that land on a whole-handle boundary are legal shorter
            // buffers, not truncations; everything else must fail.
            let parsed = RopBuffer::parse(&SPEC_EXAMPLE[..cut]);
            let legal = cut == 8 || cut == 12;
            assert_eq!(
                parsed.is_ok(),
                legal,
                "buffer cut at {cut} parsed as {parsed:?}"
            );
        }
    }

    #[test]
    fn a_header_needs_all_three_of_its_bytes() {
        assert!(RopHeader::parse(&[]).is_err());
        assert!(RopHeader::parse(&[0x01]).is_err());
        assert!(RopHeader::parse(&[0x01, 0x00]).is_err());
        assert!(RopHeader::parse(&[0x01, 0x00, 0x00]).is_ok());
    }
}
