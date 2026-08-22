//! `RopGetContentsTable` ([MS-OXCROPS] §2.2.4.14, [MS-OXCFOLD] §2.2.1.14) —
//! asking a folder for a table of the messages in it.
//!
//! Request, 5 bytes:
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | `0x05` |
//! | `LogonId` | 1 | |
//! | `InputHandleIndex` | 1 | the folder whose messages are wanted |
//! | `OutputHandleIndex` | 1 | where the table's handle goes |
//! | `TableFlags` | 1 | |
//!
//! Success response, 10 bytes:
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | `0x05` |
//! | `OutputHandleIndex` | 1 | echoed from the request |
//! | `ReturnValue` | 4 | `0x00000000` |
//! | `RowCount` | 4 | how many messages the folder holds |
//!
//! Byte-for-byte the shape of `RopGetHierarchyTable`, which is why the two
//! modules read alike. They are kept apart because what a *row* is differs
//! entirely — a child folder against a message — and merging them would push a
//! discriminant into every place either is used.
//!
//! ## What `TableFlags` asks for, and what is answered
//!
//! [MS-OXCFOLD] §2.2.1.14.1 defines the flags. Two matter here and neither is
//! honoured yet:
//!
//! * `Associated` (`0x02`) asks for the folder's FAI messages — configuration
//!   items a client stores in a mailbox, not mail. alo keeps none, so the
//!   truthful answer is an empty table, which is what an unfiltered read of a
//!   mailbox with no FAI messages already produces.
//! * `DeferredErrors` (`0x08`) permits returning success before the table is
//!   built. Nothing here is deferred — the rows are already in memory by the
//!   time this runs — so the permission is simply unused.
//!
//! The remaining flags concern notifications and conversation views, which are
//! later stages. A flag we do not implement is *ignored* rather than refused,
//! deliberately: Outlook sets bits opportunistically and refusing an unknown
//! one would fail a request whose answer is unaffected by it.

use crate::rop::RopError;

/// The `RopId` of `RopGetContentsTable`.
pub const ROP_GET_CONTENTS_TABLE: u8 = 0x05;

/// The fixed size of this request.
pub const REQUEST_LEN: usize = 5;

/// The size of a success response.
pub const RESPONSE_LEN: usize = 10;

/// `TableFlags` — the table lists the folder's FAI messages rather than its
/// mail ([MS-OXCFOLD] §2.2.1.14.1).
pub const TABLE_FLAG_ASSOCIATED: u8 = 0x02;

/// A parsed `RopGetContentsTable` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentsTableRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the folder whose messages are wanted.
    pub input_handle_index: u8,
    /// The handle-table slot the table's handle goes into.
    pub output_handle_index: u8,
    /// Flags controlling the kind of table.
    pub table_flags: u8,
}

impl ContentsTableRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if fewer than [`REQUEST_LEN`] bytes remain, or
    /// the leading byte is not this operation.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let fixed = input.get(..REQUEST_LEN).ok_or(RopError::Truncated {
            part: "RopGetContentsTable",
        })?;
        if fixed[0] != ROP_GET_CONTENTS_TABLE {
            return Err(RopError::Truncated {
                part: "RopGetContentsTable",
            });
        }
        Ok((
            Self {
                logon_id: fixed[1],
                input_handle_index: fixed[2],
                output_handle_index: fixed[3],
                table_flags: fixed[4],
            },
            &input[REQUEST_LEN..],
        ))
    }

    /// Whether this asks for the folder's associated (FAI) messages.
    #[must_use]
    pub const fn associated(&self) -> bool {
        self.table_flags & TABLE_FLAG_ASSOCIATED != 0
    }
}

/// Builds the success response for a table holding `row_count` messages.
#[must_use]
pub fn success_body(output_handle_index: u8, row_count: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(RESPONSE_LEN);
    out.push(ROP_GET_CONTENTS_TABLE);
    out.push(output_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out.extend_from_slice(&row_count.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{
        ContentsTableRequest, RESPONSE_LEN, ROP_GET_CONTENTS_TABLE, TABLE_FLAG_ASSOCIATED,
        success_body,
    };

    #[test]
    fn a_request_is_five_bytes_and_leaves_the_rest() {
        let bytes = [ROP_GET_CONTENTS_TABLE, 0x00, 0x01, 0x02, 0x00, 0xAA, 0xBB];
        let (request, tail) = ContentsTableRequest::parse(&bytes).expect("parses");
        assert_eq!(request.logon_id, 0x00);
        assert_eq!(request.input_handle_index, 0x01);
        assert_eq!(request.output_handle_index, 0x02);
        assert_eq!(request.table_flags, 0x00);
        assert!(!request.associated());
        assert_eq!(tail, &[0xAA, 0xBB]);
    }

    #[test]
    fn the_associated_flag_is_read_from_its_own_bit() {
        let bytes = [
            ROP_GET_CONTENTS_TABLE,
            0x00,
            0x01,
            0x02,
            TABLE_FLAG_ASSOCIATED,
        ];
        let (request, _) = ContentsTableRequest::parse(&bytes).expect("parses");
        assert!(request.associated());

        // A flag we do not implement must not be mistaken for this one.
        let bytes = [ROP_GET_CONTENTS_TABLE, 0x00, 0x01, 0x02, 0x08];
        let (request, _) = ContentsTableRequest::parse(&bytes).expect("parses");
        assert!(!request.associated());
    }

    #[test]
    fn another_operations_bytes_are_not_this_one() {
        // 0x04 is RopGetHierarchyTable, whose request is the same size and
        // shape. Only the leading byte tells them apart, so it is checked.
        let bytes = [0x04, 0x00, 0x01, 0x02, 0x00];
        assert!(ContentsTableRequest::parse(&bytes).is_err());
    }

    #[test]
    fn a_truncated_request_is_refused_rather_than_padded() {
        let bytes = [ROP_GET_CONTENTS_TABLE, 0x00, 0x01, 0x02];
        assert!(ContentsTableRequest::parse(&bytes).is_err());
    }

    #[test]
    fn the_response_carries_the_row_count_little_endian() {
        let body = success_body(0x02, 258);
        assert_eq!(body.len(), RESPONSE_LEN);
        assert_eq!(body[0], ROP_GET_CONTENTS_TABLE);
        assert_eq!(body[1], 0x02);
        assert_eq!(&body[2..6], &0u32.to_le_bytes());
        assert_eq!(&body[6..10], &[0x02, 0x01, 0x00, 0x00]);
    }
}
