//! `RopOpenMessage` ([MS-OXCROPS] §2.2.6.1, [MS-OXCMSG] §2.2.3.1) — opening one
//! message so its properties can be read.
//!
//! Request, 23 bytes:
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | `0x03` |
//! | `LogonId` | 1 | |
//! | `InputHandleIndex` | 1 | the logon |
//! | `OutputHandleIndex` | 1 | where the message's handle goes |
//! | `CodePageId` | 2 | the code page for this message's strings |
//! | `FolderId` | 8 | the folder the message is in |
//! | `OpenModeFlags` | 1 | |
//! | `MessageId` | 8 | the message |
//!
//! Success response:
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | `0x03` |
//! | `OutputHandleIndex` | 1 | echoed |
//! | `ReturnValue` | 4 | `0x00000000` |
//! | `HasNamedProperties` | 1 | |
//! | `SubjectPrefix` | var | a `TypedString` |
//! | `NormalizedSubject` | var | a `TypedString` |
//! | `RecipientCount` | 2 | |
//! | `ColumnCount` | 2 | |
//! | `RecipientColumns` | var | `ColumnCount` property tags |
//! | `RowCount` | 1 | |
//! | `RecipientRows` | var | `RowCount` rows |
//!
//! ## The message is named, not handed over
//!
//! Both the folder and the message arrive as ids the *client* chose, so this
//! is a second place — after `RopLogon` — where a caller could try to name
//! somebody else's data. It cannot succeed, for a structural reason rather
//! than a check: the ids are resolved against the folder tree and the message
//! list built for **this session's authenticated account**, and a message that
//! is not in them does not exist as far as this code is concerned. There is no
//! path from a MID to the store that does not go through that list.
//!
//! ## Recipients are deliberately not here
//!
//! `RecipientCount`, `ColumnCount` and `RowCount` are all zero, and the
//! recipient table is empty. Building `OpenRecipientRow` structures properly
//! means resolving every recipient to an address-book entry, which is stage 6's
//! work — and a half-built recipient table would be worse than none, because a
//! client would draw it.
//!
//! The To and Cc lines a reader actually sees do **not** come from that table:
//! they are `PidTagDisplayTo` and `PidTagDisplayCc`, plain strings answered by
//! [`crate::properties`]. So a message opens with its recipients visible and
//! its recipient *table* empty, which is the honest split.

use crate::rop::RopError;

/// The `RopId` of `RopOpenMessage`.
pub const ROP_OPEN_MESSAGE: u8 = 0x03;

/// The fixed size of this request.
///
/// `1 + 1 + 1 + 1 + 2 + 8 + 1 + 8`. Worth adding up rather than eyeballing:
/// every field after a wrong total is read from the wrong offset, and the
/// symptom is a message id that names nothing rather than a parse failure.
pub const REQUEST_LEN: usize = 23;

/// `StringType` — no string is present ([MS-OXCDATA] §2.11.7).
pub const STRING_TYPE_NONE: u8 = 0x00;
/// `StringType` — the string is empty.
pub const STRING_TYPE_EMPTY: u8 = 0x01;
/// `StringType` — null-terminated Unicode, UTF-16LE, two zero bytes.
pub const STRING_TYPE_UNICODE: u8 = 0x04;

/// A parsed `RopOpenMessage` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenMessageRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the logon.
    pub input_handle_index: u8,
    /// The handle-table slot the message's handle goes into.
    pub output_handle_index: u8,
    /// The code page the client wants this message's strings in.
    ///
    /// Read and carried but not acted on: every string alo returns is
    /// `PtypString`, which is UTF-16LE by definition and carries no code page.
    /// Honouring this would mean transcoding to an 8-bit charset, which is a
    /// way to lose "Liège" and gain nothing.
    pub code_page_id: u16,
    /// The folder the message is in.
    pub folder_id: u64,
    /// Flags controlling access to the message.
    pub open_mode_flags: u8,
    /// The message to open.
    pub message_id: u64,
}

impl OpenMessageRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if fewer than [`REQUEST_LEN`] bytes remain, or
    /// the leading byte is not this operation.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let fixed = input.get(..REQUEST_LEN).ok_or(RopError::Truncated {
            part: "RopOpenMessage",
        })?;
        if fixed[0] != ROP_OPEN_MESSAGE {
            return Err(RopError::Truncated {
                part: "RopOpenMessage",
            });
        }
        let code_page_id = u16::from_le_bytes([fixed[4], fixed[5]]);
        let folder_id = u64::from_le_bytes([
            fixed[6], fixed[7], fixed[8], fixed[9], fixed[10], fixed[11], fixed[12], fixed[13],
        ]);
        let message_id = u64::from_le_bytes([
            fixed[15], fixed[16], fixed[17], fixed[18], fixed[19], fixed[20], fixed[21], fixed[22],
        ]);
        Ok((
            Self {
                logon_id: fixed[1],
                input_handle_index: fixed[2],
                output_handle_index: fixed[3],
                code_page_id,
                folder_id,
                open_mode_flags: fixed[14],
                message_id,
            },
            &input[REQUEST_LEN..],
        ))
    }
}

/// Writes a `TypedString` ([MS-OXCDATA] §2.11.7).
///
/// Always the Unicode form for a non-empty string, never the "reduced Unicode"
/// form: reducing is only legal when every character is below `0x100`, and a
/// European product's subjects routinely are not. Choosing the always-correct
/// encoding costs a byte per character and cannot be wrong.
pub fn write_typed_string(out: &mut Vec<u8>, value: Option<&str>) {
    match value {
        None => out.push(STRING_TYPE_NONE),
        Some("") => out.push(STRING_TYPE_EMPTY),
        Some(text) => {
            out.push(STRING_TYPE_UNICODE);
            for unit in text.encode_utf16() {
                out.extend_from_slice(&unit.to_le_bytes());
            }
            out.extend_from_slice(&[0, 0]);
        }
    }
}

/// Builds the success response for an opened message.
///
/// `subject_prefix` and `normalized_subject` follow [MS-OXCMSG] §3.2.5.2: the
/// prefix is the `Re:`/`Fw:` marker and the normalized subject is what is left.
/// alo stores one unfolded subject, so the prefix is absent (`StringType`
/// `0x00`) and the whole subject is the normalized one — which is what a client
/// concatenates anyway.
#[must_use]
pub fn success_body(
    output_handle_index: u8,
    has_named_properties: bool,
    normalized_subject: Option<&str>,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(32);
    out.push(ROP_OPEN_MESSAGE);
    out.push(output_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out.push(u8::from(has_named_properties));
    write_typed_string(&mut out, None); // SubjectPrefix
    write_typed_string(&mut out, normalized_subject);
    out.extend_from_slice(&0u16.to_le_bytes()); // RecipientCount
    out.extend_from_slice(&0u16.to_le_bytes()); // ColumnCount
    // No RecipientColumns, because ColumnCount is zero.
    out.push(0x00); // RowCount
    // No RecipientRows, because RowCount is zero.
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{
        OpenMessageRequest, REQUEST_LEN, ROP_OPEN_MESSAGE, STRING_TYPE_EMPTY, STRING_TYPE_NONE,
        STRING_TYPE_UNICODE, success_body, write_typed_string,
    };

    fn request_bytes(folder: u64, message: u64) -> Vec<u8> {
        let mut out = vec![ROP_OPEN_MESSAGE, 0x00, 0x00, 0x01];
        out.extend_from_slice(&1252u16.to_le_bytes());
        out.extend_from_slice(&folder.to_le_bytes());
        out.push(0x00);
        out.extend_from_slice(&message.to_le_bytes());
        out
    }

    #[test]
    fn a_request_carries_both_ids_at_their_own_offsets() {
        // Distinct values: an offset mistake that read one where the other
        // belongs would open a message in the wrong folder, or fail to find a
        // real one, and either would look like a store bug.
        let mut bytes = request_bytes(0x0001_0000_0000_0005, 0x0001_0000_00AB_CDEF);
        bytes.extend_from_slice(&[0xAA, 0xBB]);
        let (request, tail) = OpenMessageRequest::parse(&bytes).expect("parses");
        assert_eq!(request.folder_id, 0x0001_0000_0000_0005);
        assert_eq!(request.message_id, 0x0001_0000_00AB_CDEF);
        assert_eq!(request.code_page_id, 1252);
        assert_eq!(request.output_handle_index, 0x01);
        assert_eq!(tail, &[0xAA, 0xBB]);
    }

    #[test]
    fn the_request_is_exactly_twenty_two_bytes() {
        assert_eq!(request_bytes(1, 2).len(), REQUEST_LEN);
    }

    #[test]
    fn a_truncated_request_is_refused_rather_than_padded() {
        let bytes = request_bytes(1, 2);
        assert!(OpenMessageRequest::parse(&bytes[..REQUEST_LEN - 1]).is_err());
    }

    #[test]
    fn another_operations_bytes_are_not_this_one() {
        let mut bytes = request_bytes(1, 2);
        bytes[0] = 0x02; // RopOpenFolder
        assert!(OpenMessageRequest::parse(&bytes).is_err());
    }

    #[test]
    fn a_typed_string_distinguishes_absent_from_empty() {
        let mut absent = Vec::new();
        write_typed_string(&mut absent, None);
        assert_eq!(absent, vec![STRING_TYPE_NONE]);

        let mut empty = Vec::new();
        write_typed_string(&mut empty, Some(""));
        assert_eq!(empty, vec![STRING_TYPE_EMPTY]);
    }

    #[test]
    fn a_typed_string_is_utf16_with_a_two_byte_terminator() {
        let mut out = Vec::new();
        write_typed_string(&mut out, Some("Ré"));
        assert_eq!(out[0], STRING_TYPE_UNICODE);
        assert_eq!(&out[1..3], &[b'R', 0]);
        assert_eq!(&out[3..5], &0xE9u16.to_le_bytes()); // é
        assert_eq!(&out[5..7], &[0, 0]);
        assert_eq!(out.len(), 7);
    }

    #[test]
    fn an_accented_subject_is_never_reduced() {
        // The reduced form is only legal when every character is below 0x100,
        // and a client that read a reduced string containing a character above
        // it would render mojibake. Always-Unicode cannot be wrong.
        let mut out = Vec::new();
        write_typed_string(&mut out, Some("Ω"));
        assert_eq!(out[0], STRING_TYPE_UNICODE);
        assert_eq!(&out[1..3], &0x03A9u16.to_le_bytes());
    }

    #[test]
    fn the_response_has_no_recipient_table() {
        let body = success_body(0x01, false, Some("Rechnung"));
        assert_eq!(body[0], ROP_OPEN_MESSAGE);
        assert_eq!(body[1], 0x01);
        assert_eq!(&body[2..6], &0u32.to_le_bytes());
        assert_eq!(body[6], 0x00, "no named properties");
        assert_eq!(body[7], STRING_TYPE_NONE, "no subject prefix");
        assert_eq!(body[8], STRING_TYPE_UNICODE);

        // "Rechnung" is 8 characters: 16 bytes plus a 2-byte terminator.
        let after_subject = 9 + 16 + 2;
        assert_eq!(
            &body[after_subject..after_subject + 2],
            &0u16.to_le_bytes(),
            "RecipientCount"
        );
        assert_eq!(
            &body[after_subject + 2..after_subject + 4],
            &0u16.to_le_bytes(),
            "ColumnCount"
        );
        assert_eq!(body[after_subject + 4], 0x00, "RowCount");
        assert_eq!(body.len(), after_subject + 5, "nothing follows");
    }

    #[test]
    fn a_message_with_no_subject_is_absent_not_empty() {
        let body = success_body(0x01, false, None);
        assert_eq!(body[7], STRING_TYPE_NONE, "SubjectPrefix");
        assert_eq!(body[8], STRING_TYPE_NONE, "NormalizedSubject");
    }
}
