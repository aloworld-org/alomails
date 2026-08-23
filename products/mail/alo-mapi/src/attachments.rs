//! `RopGetAttachmentTable` and `RopOpenAttachment` ([MS-OXCROPS] §2.2.6.17,
//! §2.2.6.12; [MS-OXCMSG] §2.2.3.17, §2.2.3.12) — the files hanging off a
//! message.
//!
//! `RopGetAttachmentTable` request, 5 bytes — the same shape as every other
//! "give me a table" operation:
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | `0x21` |
//! | `LogonId` | 1 | |
//! | `InputHandleIndex` | 1 | the message |
//! | `OutputHandleIndex` | 1 | where the table's handle goes |
//! | `TableFlags` | 1 | |
//!
//! `RopOpenAttachment` request, 9 bytes:
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | `0x22` |
//! | `LogonId` | 1 | |
//! | `InputHandleIndex` | 1 | the message |
//! | `OutputHandleIndex` | 1 | where the attachment's handle goes |
//! | `OpenAttachmentFlags` | 1 | |
//! | `AttachmentID` | 4 | the `PidTagAttachNumber` of the one wanted |
//!
//! Both success responses are the ordinary shapes: the table's is `RopId`,
//! `OutputHandleIndex`, `ReturnValue`, `RowCount`; the attachment's is `RopId`,
//! `OutputHandleIndex`, `ReturnValue`.
//!
//! ## How the bytes come back
//!
//! `PidTagAttachDataBinary` is `PtypBinary`, whose byte count is 16 bits inside
//! a ROP buffer and 32 bits in the MAPI/HTTP structures ([MS-OXCDATA] §2.11.1)
//! — a discrepancy this crate has deliberately not guessed at. It does not have
//! to: an attachment of any real size exceeds the client's own
//! `PropertySizeLimit`, so the row marks it absent and the client opens a
//! **stream** on it, and a stream carries raw bytes with no count field at all.
//!
//! That is not a workaround. It is what a client does with a large property
//! anyway, and it means the one ambiguous encoding in the specification never
//! has to be resolved to serve a file.
//!
//! ## `AttachmentID` is a position, not a handle
//!
//! The id is `PidTagAttachNumber`, which identifies an attachment *within its
//! message*. alo numbers them by their order in the parsed MIME, so the number
//! is stable for a given message and means nothing outside it — which is also
//! why opening an attachment requires the message handle it belongs to.

use crate::rop::RopError;

/// The `RopId` of `RopGetAttachmentTable`.
pub const ROP_GET_ATTACHMENT_TABLE: u8 = 0x21;
/// The `RopId` of `RopOpenAttachment`.
pub const ROP_OPEN_ATTACHMENT: u8 = 0x22;

/// The fixed size of a `RopGetAttachmentTable` request.
pub const TABLE_REQUEST_LEN: usize = 5;
/// The size of a `RopGetAttachmentTable` success response.
pub const TABLE_RESPONSE_LEN: usize = 10;

/// The fixed size of a `RopOpenAttachment` request.
pub const OPEN_REQUEST_LEN: usize = 9;
/// The size of a `RopOpenAttachment` success response.
pub const OPEN_RESPONSE_LEN: usize = 6;

/// `PidTagAttachMethod` — the data is in `PidTagAttachDataBinary`
/// ([MS-OXCMSG] §2.2.2.9).
pub const ATTACH_BY_VALUE: u32 = 0x0000_0001;

/// A parsed `RopGetAttachmentTable` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachmentTableRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the message.
    pub input_handle_index: u8,
    /// The handle-table slot the table's handle goes into.
    pub output_handle_index: u8,
    /// Flags controlling the kind of table.
    pub table_flags: u8,
}

impl AttachmentTableRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if fewer than [`TABLE_REQUEST_LEN`] bytes
    /// remain, or the leading byte is not this operation.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let fixed = input.get(..TABLE_REQUEST_LEN).ok_or(RopError::Truncated {
            part: "RopGetAttachmentTable",
        })?;
        if fixed[0] != ROP_GET_ATTACHMENT_TABLE {
            return Err(RopError::Truncated {
                part: "RopGetAttachmentTable",
            });
        }
        Ok((
            Self {
                logon_id: fixed[1],
                input_handle_index: fixed[2],
                output_handle_index: fixed[3],
                table_flags: fixed[4],
            },
            &input[TABLE_REQUEST_LEN..],
        ))
    }
}

/// Builds the `RopGetAttachmentTable` success response.
#[must_use]
pub fn table_success_body(output_handle_index: u8, row_count: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(TABLE_RESPONSE_LEN);
    out.push(ROP_GET_ATTACHMENT_TABLE);
    out.push(output_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out.extend_from_slice(&row_count.to_le_bytes());
    out
}

/// A parsed `RopOpenAttachment` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenAttachmentRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the message.
    pub input_handle_index: u8,
    /// The handle-table slot the attachment's handle goes into.
    pub output_handle_index: u8,
    /// Flags for opening.
    pub open_flags: u8,
    /// The `PidTagAttachNumber` of the attachment wanted.
    pub attachment_id: u32,
}

impl OpenAttachmentRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if fewer than [`OPEN_REQUEST_LEN`] bytes remain,
    /// or the leading byte is not this operation.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let fixed = input.get(..OPEN_REQUEST_LEN).ok_or(RopError::Truncated {
            part: "RopOpenAttachment",
        })?;
        if fixed[0] != ROP_OPEN_ATTACHMENT {
            return Err(RopError::Truncated {
                part: "RopOpenAttachment",
            });
        }
        Ok((
            Self {
                logon_id: fixed[1],
                input_handle_index: fixed[2],
                output_handle_index: fixed[3],
                open_flags: fixed[4],
                attachment_id: u32::from_le_bytes([fixed[5], fixed[6], fixed[7], fixed[8]]),
            },
            &input[OPEN_REQUEST_LEN..],
        ))
    }
}

/// Builds the `RopOpenAttachment` success response.
#[must_use]
pub fn open_success_body(output_handle_index: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(OPEN_RESPONSE_LEN);
    out.push(ROP_OPEN_ATTACHMENT);
    out.push(output_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{
        AttachmentTableRequest, OPEN_REQUEST_LEN, OPEN_RESPONSE_LEN, OpenAttachmentRequest,
        ROP_GET_ATTACHMENT_TABLE, ROP_OPEN_ATTACHMENT, TABLE_REQUEST_LEN, TABLE_RESPONSE_LEN,
        open_success_body, table_success_body,
    };

    #[test]
    fn a_table_request_is_five_bytes_and_leaves_the_rest() {
        let bytes = [ROP_GET_ATTACHMENT_TABLE, 0x00, 0x01, 0x02, 0x00, 0xAA];
        let (request, tail) = AttachmentTableRequest::parse(&bytes).expect("parses");
        assert_eq!(request.input_handle_index, 0x01);
        assert_eq!(request.output_handle_index, 0x02);
        assert_eq!(tail, &[0xAA]);
        assert_eq!(TABLE_REQUEST_LEN, 5);
    }

    #[test]
    fn an_open_request_reads_its_attachment_number() {
        let mut bytes = vec![ROP_OPEN_ATTACHMENT, 0x00, 0x01, 0x02, 0x00];
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.push(0xAA);
        let (request, tail) = OpenAttachmentRequest::parse(&bytes).expect("parses");
        assert_eq!(request.attachment_id, 3);
        assert_eq!(request.output_handle_index, 0x02);
        assert_eq!(tail, &[0xAA]);
        assert_eq!(OPEN_REQUEST_LEN, 9);
    }

    #[test]
    fn the_two_operations_are_told_apart_by_their_leading_byte() {
        // Their requests differ in length, so mistaking one for the other
        // would resume the walk at the wrong offset rather than fail.
        let bytes = [ROP_GET_ATTACHMENT_TABLE, 0x00, 0x01, 0x02, 0x00, 0, 0, 0, 0];
        assert!(OpenAttachmentRequest::parse(&bytes).is_err());
        let mut bytes = vec![ROP_OPEN_ATTACHMENT, 0x00, 0x01, 0x02, 0x00];
        bytes.extend_from_slice(&0u32.to_le_bytes());
        assert!(AttachmentTableRequest::parse(&bytes).is_err());
    }

    #[test]
    fn a_truncated_open_request_is_refused_rather_than_padded() {
        let mut bytes = vec![ROP_OPEN_ATTACHMENT, 0x00, 0x01, 0x02, 0x00];
        bytes.extend_from_slice(&[0x01, 0x02]); // two of the four id bytes
        assert!(OpenAttachmentRequest::parse(&bytes).is_err());
    }

    #[test]
    fn the_responses_are_their_documented_shapes() {
        let table = table_success_body(0x02, 2);
        assert_eq!(table.len(), TABLE_RESPONSE_LEN);
        assert_eq!(table[0], ROP_GET_ATTACHMENT_TABLE);
        assert_eq!(&table[6..10], &2u32.to_le_bytes());

        let open = open_success_body(0x03);
        assert_eq!(open.len(), OPEN_RESPONSE_LEN);
        assert_eq!(open[0], ROP_OPEN_ATTACHMENT);
        assert_eq!(open[1], 0x03);
        assert_eq!(&open[2..6], &0u32.to_le_bytes());
    }
}
