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
//! ## The recipient table
//!
//! Each recipient is an `OpenRecipientRow`: `RecipientType` (1), `CodePageId`
//! (2), `Reserved` (2), `RecipientRowSize` (2), then a `RecipientRow`
//! ([MS-OXCDATA] §2.8.3.2) of that many bytes.
//!
//! **A `RecipientRow` is a bitfield followed by exactly the fields the bitfield
//! claims.** `RecipientFlags` says which optional fields follow, and there is
//! no length to re-synchronise on — a flag set without its field, or a field
//! written without its flag, silently shifts everything after it and the client
//! reads one recipient's address as the next one's name. So the flags and the
//! writes are built together in [`write_recipient_row`], from one description
//! of the row, rather than in two places that have to agree.
//!
//! alo writes `SMTP` recipients: address type in the low three bits, plus `E`
//! (an address follows), `D` (a display name follows) and `U` (both are
//! UTF-16LE). Not `X500DN` — that type obliges an `AddressPrefixUsed`, a
//! `DisplayType` and an X.500 distinguished name, and alo has no X.500
//! namespace to name one in.
//!
//! `RecipientColumnCount` is zero and no extra property row follows: everything
//! a client needs about these people is in the named fields above it.

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

/// `RecipientType` — a primary (`To`) recipient ([MS-OXCMSG] §2.2.3.1.2).
pub const RECIPIENT_TYPE_TO: u8 = 0x01;
/// `RecipientType` — a carbon-copy recipient.
pub const RECIPIENT_TYPE_CC: u8 = 0x02;
/// `RecipientType` — a blind carbon-copy recipient.
pub const RECIPIENT_TYPE_BCC: u8 = 0x03;

/// `RecipientFlags` — the address type is SMTP ([MS-OXCDATA] §2.8.3.1).
pub const RECIPIENT_FLAG_TYPE_SMTP: u16 = 0x0003;
/// `RecipientFlags` `E` — an `EmailAddress` field follows.
pub const RECIPIENT_FLAG_EMAIL: u16 = 0x0008;
/// `RecipientFlags` `D` — a `DisplayName` field follows.
pub const RECIPIENT_FLAG_DISPLAY_NAME: u16 = 0x0010;
/// `RecipientFlags` `U` — the strings are UTF-16LE with a two-byte terminator.
pub const RECIPIENT_FLAG_UNICODE: u16 = 0x0200;

/// One addressee, as a row will carry them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipientEntry {
    /// Which header they came from.
    pub recipient_type: u8,
    /// What a reader sees.
    pub display_name: String,
    /// Where a message would go.
    pub email: String,
}

/// Writes one `RecipientRow` ([MS-OXCDATA] §2.8.3.2).
///
/// The flags and the fields are produced together on purpose: the bitfield is
/// the only thing that says what follows it, and a row whose flags and body
/// disagree cannot be detected by the client — it just reads the next
/// recipient's bytes as this one's.
fn write_recipient_row(out: &mut Vec<u8>, entry: &RecipientEntry) {
    let flags = RECIPIENT_FLAG_TYPE_SMTP
        | RECIPIENT_FLAG_EMAIL
        | RECIPIENT_FLAG_DISPLAY_NAME
        | RECIPIENT_FLAG_UNICODE;
    out.extend_from_slice(&flags.to_le_bytes());
    // No `AddressPrefixUsed`, `DisplayType` or `X500DN`: those belong to the
    // X500DN address type, which this is not. No `EntryId`/`SearchKey` either
    // — those are the personal-distribution-list types.
    write_utf16z(out, &entry.email); // because E is set
    write_utf16z(out, &entry.display_name); // because D is set
    out.extend_from_slice(&0u16.to_le_bytes()); // RecipientColumnCount
}

/// Writes one `OpenRecipientRow` ([MS-OXCROPS] §2.2.6.1.2.1).
fn write_open_recipient_row(out: &mut Vec<u8>, entry: &RecipientEntry) {
    out.push(entry.recipient_type);
    // The code page for this recipient's strings. Zero, and honestly so: the
    // `U` flag says they are Unicode, which carries no code page.
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // Reserved, MUST be zero.

    // The row is built first so its length can be written in front of it —
    // the one field here that cannot be computed by counting the spec's
    // columns, because the strings vary.
    let mut row = Vec::new();
    write_recipient_row(&mut row, entry);
    out.extend_from_slice(&u16::try_from(row.len()).unwrap_or(u16::MAX).to_le_bytes());
    out.extend_from_slice(&row);
}

/// Writes a null-terminated UTF-16LE string.
fn write_utf16z(out: &mut Vec<u8>, text: &str) {
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out.extend_from_slice(&[0, 0]);
}

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
    recipients: &[RecipientEntry],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.push(ROP_OPEN_MESSAGE);
    out.push(output_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out.push(u8::from(has_named_properties));
    write_typed_string(&mut out, None); // SubjectPrefix
    write_typed_string(&mut out, normalized_subject);

    // `RowCount` is one byte, so at most 255 rows can be described however
    // many recipients the message has. `RecipientCount` still reports the
    // truth — the specification requires only that `RowCount` be no greater —
    // so a client with a 300-address message is told there are 300 and handed
    // the first 255, rather than being told a smaller number that is wrong.
    let rows = recipients.len().min(usize::from(u8::MAX));
    out.extend_from_slice(
        &u16::try_from(recipients.len())
            .unwrap_or(u16::MAX)
            .to_le_bytes(),
    );
    out.extend_from_slice(&0u16.to_le_bytes()); // ColumnCount
    // No RecipientColumns, because ColumnCount is zero.
    out.push(u8::try_from(rows).unwrap_or(u8::MAX)); // RowCount
    for entry in &recipients[..rows] {
        write_open_recipient_row(&mut out, entry);
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{
        OpenMessageRequest, RECIPIENT_FLAG_DISPLAY_NAME, RECIPIENT_FLAG_EMAIL,
        RECIPIENT_FLAG_TYPE_SMTP, RECIPIENT_FLAG_UNICODE, RECIPIENT_TYPE_BCC, RECIPIENT_TYPE_CC,
        RECIPIENT_TYPE_TO, REQUEST_LEN, ROP_OPEN_MESSAGE, RecipientEntry, STRING_TYPE_EMPTY,
        STRING_TYPE_NONE, STRING_TYPE_UNICODE, success_body, write_typed_string,
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
        let body = success_body(0x01, false, Some("Rechnung"), &[]);
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
        let body = success_body(0x01, false, None, &[]);
        assert_eq!(body[7], STRING_TYPE_NONE, "SubjectPrefix");
        assert_eq!(body[8], STRING_TYPE_NONE, "NormalizedSubject");
    }

    fn to(name: &str, email: &str) -> RecipientEntry {
        RecipientEntry {
            recipient_type: RECIPIENT_TYPE_TO,
            display_name: name.to_owned(),
            email: email.to_owned(),
        }
    }

    /// The row's flags and its fields have to agree, and nothing in the wire
    /// format can detect it when they do not — the client simply reads the next
    /// recipient's bytes as this one's. So this walks the row the way a client
    /// would: read the flags, then read exactly what they promise.
    #[test]
    fn a_recipient_row_carries_exactly_what_its_flags_promise() {
        let body = success_body(0x01, false, Some("Hallo"), &[to("Anna Müller", "a@x.test")]);

        // Past the header, the flag byte and the two typed strings.
        let mut at = 7; // RopId, index, ReturnValue, HasNamedProperties
        assert_eq!(body[at], STRING_TYPE_NONE, "SubjectPrefix");
        at += 1;
        assert_eq!(body[at], STRING_TYPE_UNICODE, "NormalizedSubject");
        at += 1 + "Hallo".encode_utf16().count() * 2 + 2;

        assert_eq!(
            u16::from_le_bytes(body[at..at + 2].try_into().unwrap()),
            1,
            "RecipientCount"
        );
        assert_eq!(
            u16::from_le_bytes(body[at + 2..at + 4].try_into().unwrap()),
            0,
            "ColumnCount"
        );
        assert_eq!(body[at + 4], 1, "RowCount");
        at += 5;

        // The OpenRecipientRow.
        assert_eq!(body[at], RECIPIENT_TYPE_TO);
        assert_eq!(
            u16::from_le_bytes(body[at + 1..at + 3].try_into().unwrap()),
            0,
            "CodePageId"
        );
        assert_eq!(
            u16::from_le_bytes(body[at + 3..at + 5].try_into().unwrap()),
            0,
            "Reserved MUST be zero"
        );
        let row_size = usize::from(u16::from_le_bytes(body[at + 5..at + 7].try_into().unwrap()));
        at += 7;
        let row = &body[at..at + row_size];
        assert_eq!(
            body.len(),
            at + row_size,
            "the declared row size is the truth"
        );

        // The RecipientRow, read as a client reads it.
        let flags = u16::from_le_bytes(row[0..2].try_into().unwrap());
        assert_eq!(flags & 0x0007, RECIPIENT_FLAG_TYPE_SMTP, "address type");
        assert_ne!(flags & RECIPIENT_FLAG_EMAIL, 0);
        assert_ne!(flags & RECIPIENT_FLAG_DISPLAY_NAME, 0);
        assert_ne!(flags & RECIPIENT_FLAG_UNICODE, 0);
        // No X500DN type, so none of its obligatory fields are here.
        assert_eq!(flags & 0x8000, 0, "the O flag would add an AddressType");

        let mut at = 2;
        assert_eq!(read_utf16z(row, &mut at), "a@x.test", "EmailAddress");
        assert_eq!(read_utf16z(row, &mut at), "Anna Müller", "DisplayName");
        assert_eq!(
            u16::from_le_bytes(row[at..at + 2].try_into().unwrap()),
            0,
            "RecipientColumnCount"
        );
        assert_eq!(at + 2, row.len(), "nothing unaccounted for in the row");
    }

    /// Reads a null-terminated UTF-16LE string, advancing `at` past it.
    fn read_utf16z(bytes: &[u8], at: &mut usize) -> String {
        let start = *at;
        while bytes[*at] != 0 || bytes[*at + 1] != 0 {
            *at += 2;
        }
        let units: Vec<u16> = bytes[start..*at]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        *at += 2;
        String::from_utf16(&units).expect("utf-16")
    }

    #[test]
    fn every_recipient_kind_keeps_its_own_type_byte() {
        // To, Cc and Bcc are 0x01/0x02/0x03. Confusing them puts somebody in
        // the wrong line of a message a reader is looking at.
        let people = [
            to("A", "a@x.test"),
            RecipientEntry {
                recipient_type: RECIPIENT_TYPE_CC,
                display_name: "B".to_owned(),
                email: "b@x.test".to_owned(),
            },
            RecipientEntry {
                recipient_type: RECIPIENT_TYPE_BCC,
                display_name: "C".to_owned(),
                email: "c@x.test".to_owned(),
            },
        ];
        let body = success_body(0x01, false, None, &people);
        // Header, no prefix, no subject, then counts.
        let mut at = 7 + 1 + 1;
        assert_eq!(u16::from_le_bytes(body[at..at + 2].try_into().unwrap()), 3);
        assert_eq!(body[at + 4], 3, "RowCount");
        at += 5;
        for expected in [RECIPIENT_TYPE_TO, RECIPIENT_TYPE_CC, RECIPIENT_TYPE_BCC] {
            assert_eq!(body[at], expected);
            let size = usize::from(u16::from_le_bytes(body[at + 5..at + 7].try_into().unwrap()));
            at += 7 + size;
        }
        assert_eq!(at, body.len());
    }

    /// `RowCount` is one byte. A message with more recipients than it can
    /// describe reports the true `RecipientCount` and hands back as many rows
    /// as the field allows — a smaller count would be a number that is wrong.
    #[test]
    fn more_recipients_than_a_row_count_can_hold_still_counts_them_truthfully() {
        let people: Vec<RecipientEntry> = (0..300)
            .map(|n| to(&format!("P{n}"), &format!("p{n}@x.test")))
            .collect();
        let body = success_body(0x01, false, None, &people);
        let at = 7 + 1 + 1;
        assert_eq!(
            u16::from_le_bytes(body[at..at + 2].try_into().unwrap()),
            300,
            "RecipientCount tells the truth"
        );
        assert_eq!(body[at + 4], 255, "RowCount is what one byte can hold");
    }

    #[test]
    fn a_message_with_no_recipients_writes_no_rows() {
        let body = success_body(0x01, false, None, &[]);
        let at = 7 + 1 + 1;
        assert_eq!(u16::from_le_bytes(body[at..at + 2].try_into().unwrap()), 0);
        assert_eq!(body[at + 4], 0);
        assert_eq!(body.len(), at + 5, "nothing follows");
    }
}
