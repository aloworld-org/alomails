//! The write path: composing a message and sending it ([MS-OXCROPS] §2.2.6,
//! §2.2.7, §2.2.8.6).
//!
//! Everything else in this crate reads. This is the first module that changes
//! anything, and the five operations here are one arc:
//!
//! | | | |
//! |---|---|---|
//! | `RopCreateMessage` | `0x06` | make a draft in a folder |
//! | `RopSetProperties` | `0x0A` | give it a subject and a body |
//! | `RopModifyRecipients` | `0x0E` | say who it is for |
//! | `RopSaveChangesMessage` | `0x0C` | store it |
//! | `RopSubmitMessage` | `0x32` | send it |
//!
//! ## The draft lives in the session until it is saved
//!
//! A client sets properties one `Execute` at a time and may send several before
//! saving. Writing each of those to the store as it arrives would leave a
//! half-composed message in somebody's Drafts folder every time Outlook paused,
//! so the properties and recipients accumulate against the session's message
//! object and reach the store once, at `RopSaveChangesMessage`.
//!
//! ## Reading a `RecipientRow` is the mirror of writing one
//!
//! [`crate::openmessage`] writes these; this parses them. The same warning
//! applies in reverse and is worse here, because the bytes are the client's: a
//! `RecipientFlags` bitfield says which fields follow, there is no length to
//! re-synchronise on, and a row read one field out of step yields an address
//! made of somebody's display name. Every optional field is therefore read
//! strictly from its flag, and a row whose declared size does not match what
//! its flags describe is refused rather than salvaged.
//!
//! ## `PropertyValueSize` counts itself, almost
//!
//! `RopSetProperties` carries `PropertyValueSize`, which is "the number of
//! bytes used for the `PropertyValueCount` field **and** the `PropertyValues`
//! field" — so the values occupy `PropertyValueSize - 2`. Reading it as the
//! size of the values alone overshoots by two bytes and lands the walk inside
//! the next operation.

use crate::columns::PropertyTag;
use crate::openmessage::{
    RECIPIENT_FLAG_DISPLAY_NAME, RECIPIENT_FLAG_EMAIL, RECIPIENT_FLAG_UNICODE, RecipientEntry,
};
use crate::rop::RopError;
use crate::rows::{Value, ptyp};

/// The `RopId` of `RopCreateMessage`.
pub const ROP_CREATE_MESSAGE: u8 = 0x06;
/// The `RopId` of `RopSetProperties`.
pub const ROP_SET_PROPERTIES: u8 = 0x0A;
/// The `RopId` of `RopSaveChangesMessage`.
pub const ROP_SAVE_CHANGES_MESSAGE: u8 = 0x0C;
/// The `RopId` of `RopModifyRecipients`.
pub const ROP_MODIFY_RECIPIENTS: u8 = 0x0E;
/// The `RopId` of `RopSubmitMessage`.
pub const ROP_SUBMIT_MESSAGE: u8 = 0x32;

/// The fixed size of a `RopCreateMessage` request.
///
/// `1 + 1 + 1 + 1 + 2 + 8 + 1`.
pub const CREATE_REQUEST_LEN: usize = 15;
/// The fixed size of a `RopSaveChangesMessage` request.
pub const SAVE_REQUEST_LEN: usize = 5;
/// The fixed size of a `RopSubmitMessage` request.
pub const SUBMIT_REQUEST_LEN: usize = 4;
/// The size of a `RopSetProperties` request before its value list.
pub const SET_PROPERTIES_HEAD_LEN: usize = 7;
/// The size of a `RopModifyRecipients` request before its column list.
pub const MODIFY_RECIPIENTS_HEAD_LEN: usize = 5;

/// The most properties one `RopSetProperties` may carry.
pub const MAX_PROPERTIES: u16 = 1024;
/// The most recipients one `RopModifyRecipients` may carry.
///
/// The same ceiling the JMAP submission path applies, and for the same reason:
/// a message addressed to more people than this is a mailing list, which is a
/// different product with different rules.
pub const MAX_RECIPIENTS: u16 = 100;

/// A parsed `RopCreateMessage` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreateMessageRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the logon.
    pub input_handle_index: u8,
    /// The handle-table slot the new message's handle goes into.
    pub output_handle_index: u8,
    /// The code page the client wants for this message's strings.
    pub code_page_id: u16,
    /// The folder to create it in.
    pub folder_id: u64,
    /// Whether it is an FAI message rather than mail.
    pub associated: bool,
}

impl CreateMessageRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if fewer than [`CREATE_REQUEST_LEN`] bytes
    /// remain, or the leading byte is not this operation.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let fixed = input.get(..CREATE_REQUEST_LEN).ok_or(RopError::Truncated {
            part: "RopCreateMessage",
        })?;
        if fixed[0] != ROP_CREATE_MESSAGE {
            return Err(RopError::Truncated {
                part: "RopCreateMessage",
            });
        }
        Ok((
            Self {
                logon_id: fixed[1],
                input_handle_index: fixed[2],
                output_handle_index: fixed[3],
                code_page_id: u16::from_le_bytes([fixed[4], fixed[5]]),
                folder_id: u64::from_le_bytes([
                    fixed[6], fixed[7], fixed[8], fixed[9], fixed[10], fixed[11], fixed[12],
                    fixed[13],
                ]),
                associated: fixed[14] != 0,
            },
            &input[CREATE_REQUEST_LEN..],
        ))
    }
}

/// Builds the `RopCreateMessage` success response.
///
/// The message id is omitted (`HasMessageId` zero) until the draft is saved:
/// alo assigns one when the message reaches the store, and a client told an id
/// before then would hold one that names nothing.
#[must_use]
pub fn create_success_body(output_handle_index: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(7);
    out.push(ROP_CREATE_MESSAGE);
    out.push(output_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out.push(0x00); // HasMessageId
    out
}

/// A parsed `RopSetProperties` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetPropertiesRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the object being changed.
    pub input_handle_index: u8,
    /// The properties to set, in the order given.
    pub values: Vec<(PropertyTag, Value)>,
}

impl SetPropertiesRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if a declared length runs past the end, more
    /// than [`MAX_PROPERTIES`] are named, or a value's type is one this
    /// adapter cannot read.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let head = input.get(..SET_PROPERTIES_HEAD_LEN).ok_or(truncated())?;
        if head[0] != ROP_SET_PROPERTIES {
            return Err(truncated());
        }
        let declared = usize::from(u16::from_le_bytes([head[3], head[4]]));
        let count = u16::from_le_bytes([head[5], head[6]]);
        if count > MAX_PROPERTIES {
            return Err(truncated());
        }
        // `PropertyValueSize` counts the two-byte count *and* the values, so
        // the values themselves are two bytes shorter. Reading it as the size
        // of the values alone overshoots into the next operation.
        let values_len = declared.checked_sub(2).ok_or_else(truncated)?;
        let body = input
            .get(SET_PROPERTIES_HEAD_LEN..SET_PROPERTIES_HEAD_LEN + values_len)
            .ok_or_else(truncated)?;

        let mut at = 0usize;
        let mut values = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            let raw = read_exact::<4>(body, &mut at)?;
            let tag = PropertyTag::from_bytes(raw);
            let value = read_value(body, &mut at, tag.property_type)?;
            values.push((tag, value));
        }
        Ok((
            Self {
                logon_id: head[1],
                input_handle_index: head[2],
                values,
            },
            &input[SET_PROPERTIES_HEAD_LEN + values_len..],
        ))
    }
}

/// Builds the `RopSetProperties` success response.
///
/// `PropertyProblemCount` is zero: a property this adapter does not store is
/// refused at the operation level rather than reported as a per-property
/// problem, because a client that saw zero problems and then found its subject
/// missing would have no way to tell what happened.
#[must_use]
pub fn set_properties_success_body(input_handle_index: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(8);
    out.push(ROP_SET_PROPERTIES);
    out.push(input_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out.extend_from_slice(&0u16.to_le_bytes()); // PropertyProblemCount
    out
}

/// A parsed `RopSaveChangesMessage` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveChangesRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The slot the response refers to.
    ///
    /// **Not** the input handle: this operation is the one in the crate whose
    /// request names the response slot *before* the input slot, and reading
    /// them the usual way round makes a client's save appear to apply to a
    /// different object.
    pub response_handle_index: u8,
    /// The handle-table slot holding the message.
    pub input_handle_index: u8,
    /// Flags controlling the save.
    pub save_flags: u8,
}

impl SaveChangesRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if fewer than [`SAVE_REQUEST_LEN`] bytes remain,
    /// or the leading byte is not this operation.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let fixed = input.get(..SAVE_REQUEST_LEN).ok_or(RopError::Truncated {
            part: "RopSaveChangesMessage",
        })?;
        if fixed[0] != ROP_SAVE_CHANGES_MESSAGE {
            return Err(RopError::Truncated {
                part: "RopSaveChangesMessage",
            });
        }
        Ok((
            Self {
                logon_id: fixed[1],
                response_handle_index: fixed[2],
                input_handle_index: fixed[3],
                save_flags: fixed[4],
            },
            &input[SAVE_REQUEST_LEN..],
        ))
    }
}

/// Builds the `RopSaveChangesMessage` success response.
#[must_use]
pub fn save_success_body(response_handle_index: u8, message_id: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(14);
    out.push(ROP_SAVE_CHANGES_MESSAGE);
    out.push(response_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out.push(response_handle_index); // InputHandleIndex, echoed
    out.extend_from_slice(&message_id.to_le_bytes());
    out
}

/// A parsed `RopSubmitMessage` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubmitMessageRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the message to send.
    pub input_handle_index: u8,
    /// Flags controlling the submission.
    pub submit_flags: u8,
}

impl SubmitMessageRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if fewer than [`SUBMIT_REQUEST_LEN`] bytes
    /// remain, or the leading byte is not this operation.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let fixed = input.get(..SUBMIT_REQUEST_LEN).ok_or(RopError::Truncated {
            part: "RopSubmitMessage",
        })?;
        if fixed[0] != ROP_SUBMIT_MESSAGE {
            return Err(RopError::Truncated {
                part: "RopSubmitMessage",
            });
        }
        Ok((
            Self {
                logon_id: fixed[1],
                input_handle_index: fixed[2],
                submit_flags: fixed[3],
            },
            &input[SUBMIT_REQUEST_LEN..],
        ))
    }
}

/// A parsed `RopModifyRecipients` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModifyRecipientsRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the message.
    pub input_handle_index: u8,
    /// The recipients, in the order given.
    pub recipients: Vec<RecipientEntry>,
}

impl ModifyRecipientsRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if a declared length runs past the end, more
    /// than [`MAX_RECIPIENTS`] rows are given, or a row's declared size does
    /// not match what its flags describe.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let head = input
            .get(..MODIFY_RECIPIENTS_HEAD_LEN)
            .ok_or_else(truncated)?;
        if head[0] != ROP_MODIFY_RECIPIENTS {
            return Err(truncated());
        }
        let mut at = MODIFY_RECIPIENTS_HEAD_LEN;

        // The column list describes the *extra* property row each recipient
        // may carry. alo reads a recipient's address and name from the named
        // fields, so the columns are read to find the end of them and then
        // discarded — but they must be read, because the rows behind them
        // cannot be found otherwise.
        let column_count = u16::from_le_bytes([head[3], head[4]]);
        let columns: usize = usize::from(column_count);
        at += columns * PropertyTag::LEN;
        if input.len() < at + 2 {
            return Err(truncated());
        }

        let row_count = u16::from_le_bytes([input[at], input[at + 1]]);
        at += 2;
        if row_count > MAX_RECIPIENTS {
            return Err(truncated());
        }

        let mut recipients = Vec::with_capacity(usize::from(row_count));
        for _ in 0..row_count {
            // ModifyRecipientRow: RowId (4), RecipientType (1),
            // RecipientRowSize (2), then a RecipientRow of that size.
            let _row_id = read_exact::<4>(input, &mut at)?;
            let recipient_type = read_u8(input, &mut at)?;
            let size = usize::from(u16::from_le_bytes(read_exact::<2>(input, &mut at)?));
            if size == 0 {
                // A row with no body: the client is removing this recipient.
                // Nothing to add, and skipping it is not an error.
                continue;
            }
            let row = input.get(at..at + size).ok_or_else(truncated)?;
            at += size;
            if let Some(entry) = parse_recipient_row(row, recipient_type)? {
                recipients.push(entry);
            }
        }
        Ok((
            Self {
                logon_id: head[1],
                input_handle_index: head[2],
                recipients,
            },
            &input[at..],
        ))
    }
}

/// Reads one `RecipientRow` ([MS-OXCDATA] §2.8.3.2) strictly from its flags.
///
/// Returns `None` for a row that carries no address — a recipient a message
/// cannot be sent to is not one worth recording, and inventing an address for
/// it would be worse.
///
/// # Errors
/// [`RopError::Truncated`] if a field the flags promise runs past the row.
fn parse_recipient_row(row: &[u8], recipient_type: u8) -> Result<Option<RecipientEntry>, RopError> {
    let mut at = 0usize;
    let flags = u16::from_le_bytes(read_exact::<2>(row, &mut at)?);
    let address_type = flags & 0x0007;
    let unicode = flags & RECIPIENT_FLAG_UNICODE != 0;

    // X500DN (0x1) puts three more fields here; the two personal-distribution
    // list types put an EntryId and a SearchKey. Reading them is what keeps the
    // walk aligned even when the row is not one alo can use.
    if address_type == 0x0001 {
        let _address_prefix_used = read_u8(row, &mut at)?;
        let _display_type = read_u8(row, &mut at)?;
        let _x500dn = read_asciiz(row, &mut at)?;
    } else if address_type == 0x0006 || address_type == 0x0007 {
        let entry_id = usize::from(u16::from_le_bytes(read_exact::<2>(row, &mut at)?));
        at = at.checked_add(entry_id).ok_or_else(truncated)?;
        let search_key = usize::from(u16::from_le_bytes(read_exact::<2>(row, &mut at)?));
        at = at.checked_add(search_key).ok_or_else(truncated)?;
        if row.len() < at {
            return Err(truncated());
        }
        // A distribution list is not an address alo can send to.
        return Ok(None);
    }

    // `O` (0x8000) adds an AddressType string, and only for the NoType kind.
    if address_type == 0x0000 && flags & 0x8000 != 0 {
        let _address_type = read_asciiz(row, &mut at)?;
    }

    let email = if flags & RECIPIENT_FLAG_EMAIL != 0 {
        Some(read_string(row, &mut at, unicode)?)
    } else {
        None
    };
    let display_name = if flags & RECIPIENT_FLAG_DISPLAY_NAME != 0 {
        Some(read_string(row, &mut at, unicode)?)
    } else {
        None
    };
    // `I` (0x0400) and `T` (0x0020) add two more names. Read so the row's
    // declared size can be checked, then discarded: alo shows one name.
    if flags & 0x0400 != 0 {
        let _simple = read_string(row, &mut at, unicode)?;
    }
    if flags & 0x0020 != 0 {
        let _transmittable = read_string(row, &mut at, unicode)?;
    }

    let Some(email) = email.filter(|address| !address.trim().is_empty()) else {
        return Ok(None);
    };
    let display_name = display_name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| email.clone());
    Ok(Some(RecipientEntry {
        recipient_type,
        display_name,
        email,
    }))
}

/// Builds the `RopModifyRecipients` success response.
#[must_use]
pub fn modify_recipients_success_body(input_handle_index: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(6);
    out.push(ROP_MODIFY_RECIPIENTS);
    out.push(input_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out
}

/// Builds the `RopSubmitMessage` success response.
#[must_use]
pub fn submit_success_body(input_handle_index: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(6);
    out.push(ROP_SUBMIT_MESSAGE);
    out.push(input_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out
}

// ---- value reading --------------------------------------------------------

/// Reads one `PropertyValue` of the given type ([MS-OXCDATA] §2.11.2.1).
///
/// Only the types a composed message actually uses are read. An unrecognised
/// type is an error rather than a skip, because a `PropertyValue` has no length
/// of its own: without knowing the type there is no way to find where it ends,
/// so guessing would misread everything after it.
fn read_value(input: &[u8], at: &mut usize, property_type: u16) -> Result<Value, RopError> {
    match property_type {
        ptyp::INTEGER32 => Ok(Value::Integer32(u32::from_le_bytes(read_exact::<4>(
            input, at,
        )?))),
        ptyp::INTEGER64 => Ok(Value::Integer64(u64::from_le_bytes(read_exact::<8>(
            input, at,
        )?))),
        ptyp::BOOLEAN => Ok(Value::Boolean(read_u8(input, at)? != 0)),
        ptyp::TIME => Ok(Value::Time(u64::from_le_bytes(read_exact::<8>(input, at)?))),
        ptyp::STRING => Ok(Value::String(read_utf16z(input, at)?)),
        ptyp::STRING8 => Ok(Value::String(read_asciiz(input, at)?)),
        ptyp::BINARY => {
            // 16 bits here: this is a ROP buffer ([MS-OXCDATA] §2.11.1).
            let count = usize::from(u16::from_le_bytes(read_exact::<2>(input, at)?));
            let bytes = input.get(*at..*at + count).ok_or_else(truncated)?;
            *at += count;
            Ok(Value::Binary(bytes.to_vec()))
        }
        _ => Err(truncated()),
    }
}

fn truncated() -> RopError {
    RopError::Truncated {
        part: "compose request",
    }
}

fn read_u8(input: &[u8], at: &mut usize) -> Result<u8, RopError> {
    let byte = *input.get(*at).ok_or_else(truncated)?;
    *at += 1;
    Ok(byte)
}

fn read_exact<const N: usize>(input: &[u8], at: &mut usize) -> Result<[u8; N], RopError> {
    let slice = input.get(*at..*at + N).ok_or_else(truncated)?;
    let mut out = [0u8; N];
    out.copy_from_slice(slice);
    *at += N;
    Ok(out)
}

/// Reads a string in whichever width the row's `U` flag declared.
fn read_string(input: &[u8], at: &mut usize, unicode: bool) -> Result<String, RopError> {
    if unicode {
        read_utf16z(input, at)
    } else {
        read_asciiz(input, at)
    }
}

/// Reads a null-terminated UTF-16LE string.
fn read_utf16z(input: &[u8], at: &mut usize) -> Result<String, RopError> {
    let mut units = Vec::new();
    loop {
        let unit = u16::from_le_bytes(read_exact::<2>(input, at)?);
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    String::from_utf16(&units).map_err(|_| truncated())
}

/// Reads a null-terminated 8-bit string.
///
/// Decoded as UTF-8 where it is valid and lossily where it is not. The row's
/// code page is the client's, and refusing a name because it is in some
/// eight-bit charset would refuse the message rather than the name.
fn read_asciiz(input: &[u8], at: &mut usize) -> Result<String, RopError> {
    let start = *at;
    loop {
        let byte = read_u8(input, at)?;
        if byte == 0 {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&input[start..*at - 1]).into_owned())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{
        CREATE_REQUEST_LEN, CreateMessageRequest, MAX_RECIPIENTS, ModifyRecipientsRequest,
        ROP_CREATE_MESSAGE, ROP_MODIFY_RECIPIENTS, ROP_SAVE_CHANGES_MESSAGE, ROP_SET_PROPERTIES,
        ROP_SUBMIT_MESSAGE, SaveChangesRequest, SetPropertiesRequest, SubmitMessageRequest,
        create_success_body, save_success_body,
    };
    use crate::columns::PropertyTag;
    use crate::openmessage::{
        RECIPIENT_FLAG_DISPLAY_NAME, RECIPIENT_FLAG_EMAIL, RECIPIENT_FLAG_TYPE_SMTP,
        RECIPIENT_FLAG_UNICODE, RECIPIENT_TYPE_TO,
    };
    use crate::rows::{Value, pid, ptyp};

    fn utf16z(text: &str) -> Vec<u8> {
        let mut out = Vec::new();
        for unit in text.encode_utf16() {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
        out
    }

    #[test]
    fn a_create_request_reads_its_folder() {
        let mut bytes = vec![ROP_CREATE_MESSAGE, 0x00, 0x00, 0x01];
        bytes.extend_from_slice(&1252u16.to_le_bytes());
        bytes.extend_from_slice(&0x0001_0000_0000_0005u64.to_le_bytes());
        bytes.push(0x00);
        bytes.push(0xAA);
        let (request, tail) = CreateMessageRequest::parse(&bytes).expect("parses");
        assert_eq!(request.folder_id, 0x0001_0000_0000_0005);
        assert_eq!(request.output_handle_index, 0x01);
        assert!(!request.associated);
        assert_eq!(tail, &[0xAA]);
        assert_eq!(CREATE_REQUEST_LEN, 15);
    }

    #[test]
    fn a_create_response_promises_no_message_id_yet() {
        // alo assigns one when the draft reaches the store; a client told an id
        // before then would hold one that names nothing.
        let body = create_success_body(0x01);
        assert_eq!(body[0], ROP_CREATE_MESSAGE);
        assert_eq!(&body[2..6], &0u32.to_le_bytes());
        assert_eq!(body[6], 0x00, "HasMessageId");
        assert_eq!(body.len(), 7);
    }

    fn set_properties_bytes(values: &[(PropertyTag, Vec<u8>)]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&u16::try_from(values.len()).unwrap().to_le_bytes());
        for (tag, encoded) in values {
            body.extend_from_slice(&tag.to_bytes());
            body.extend_from_slice(encoded);
        }
        let mut out = vec![ROP_SET_PROPERTIES, 0x00, 0x01];
        out.extend_from_slice(&u16::try_from(body.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn set_properties_reads_a_subject_and_a_body() {
        let subject = PropertyTag {
            property_type: ptyp::STRING,
            property_id: pid::SUBJECT,
        };
        let text = PropertyTag {
            property_type: ptyp::STRING,
            property_id: pid::BODY,
        };
        let mut bytes = set_properties_bytes(&[
            (subject, utf16z("Rechnung")),
            (text, utf16z("Grüße aus Liège")),
        ]);
        bytes.push(0xAA);
        let (request, tail) = SetPropertiesRequest::parse(&bytes).expect("parses");
        assert_eq!(request.values.len(), 2);
        assert_eq!(
            request.values[0],
            (subject, Value::String("Rechnung".to_owned()))
        );
        assert_eq!(
            request.values[1],
            (text, Value::String("Grüße aus Liège".to_owned()))
        );
        assert_eq!(tail, &[0xAA], "the walk resumed in the right place");
    }

    #[test]
    fn property_value_size_counts_the_count_field_too() {
        // The trap: `PropertyValueSize` covers `PropertyValueCount` *and* the
        // values, so the values are two bytes shorter than it says. Reading it
        // as the values' own size overshoots into the next operation — which is
        // exactly what the trailing byte here would be swallowed by.
        let tag = PropertyTag {
            property_type: ptyp::INTEGER32,
            property_id: pid::MESSAGE_FLAGS,
        };
        let mut bytes = set_properties_bytes(&[(tag, 8u32.to_le_bytes().to_vec())]);
        bytes.extend_from_slice(&[ROP_SUBMIT_MESSAGE, 0x00, 0x01, 0x00]);
        let (request, tail) = SetPropertiesRequest::parse(&bytes).expect("parses");
        assert_eq!(request.values.len(), 1);
        assert_eq!(
            tail,
            &[ROP_SUBMIT_MESSAGE, 0x00, 0x01, 0x00],
            "the next operation was consumed"
        );
    }

    #[test]
    fn a_value_of_a_type_we_cannot_read_is_refused_not_skipped() {
        // A PropertyValue has no length of its own, so a type we do not know is
        // a type whose end we cannot find. Guessing misreads everything after.
        let tag = PropertyTag {
            property_type: 0x000D, // PtypObject
            property_id: 0x0E21,
        };
        let bytes = set_properties_bytes(&[(tag, vec![0x01, 0x02])]);
        assert!(SetPropertiesRequest::parse(&bytes).is_err());
    }

    fn recipient_row(email: &str, name: &str) -> Vec<u8> {
        let flags = RECIPIENT_FLAG_TYPE_SMTP
            | RECIPIENT_FLAG_EMAIL
            | RECIPIENT_FLAG_DISPLAY_NAME
            | RECIPIENT_FLAG_UNICODE;
        let mut row = flags.to_le_bytes().to_vec();
        row.extend_from_slice(&utf16z(email));
        row.extend_from_slice(&utf16z(name));
        row.extend_from_slice(&0u16.to_le_bytes()); // RecipientColumnCount
        row
    }

    fn modify_bytes(rows: &[(u8, Vec<u8>)]) -> Vec<u8> {
        let mut out = vec![ROP_MODIFY_RECIPIENTS, 0x00, 0x01];
        out.extend_from_slice(&0u16.to_le_bytes()); // ColumnCount
        out.extend_from_slice(&u16::try_from(rows.len()).unwrap().to_le_bytes());
        for (index, (kind, row)) in rows.iter().enumerate() {
            out.extend_from_slice(&u32::try_from(index).unwrap().to_le_bytes());
            out.push(*kind);
            out.extend_from_slice(&u16::try_from(row.len()).unwrap().to_le_bytes());
            out.extend_from_slice(row);
        }
        out
    }

    #[test]
    fn recipients_are_read_strictly_from_their_flags() {
        let mut bytes = modify_bytes(&[
            (
                RECIPIENT_TYPE_TO,
                recipient_row("anna@x.test", "Anna Müller"),
            ),
            (0x02, recipient_row("office@x.test", "Liège Office")),
        ]);
        bytes.push(0xAA);
        let (request, tail) = ModifyRecipientsRequest::parse(&bytes).expect("parses");
        assert_eq!(request.recipients.len(), 2);
        assert_eq!(request.recipients[0].email, "anna@x.test");
        assert_eq!(request.recipients[0].display_name, "Anna Müller");
        assert_eq!(request.recipients[0].recipient_type, RECIPIENT_TYPE_TO);
        assert_eq!(request.recipients[1].recipient_type, 0x02);
        assert_eq!(tail, &[0xAA], "the walk resumed after the last row");
    }

    #[test]
    fn a_recipient_with_no_address_is_dropped_rather_than_invented() {
        let flags = RECIPIENT_FLAG_TYPE_SMTP | RECIPIENT_FLAG_DISPLAY_NAME | RECIPIENT_FLAG_UNICODE;
        let mut row = flags.to_le_bytes().to_vec();
        row.extend_from_slice(&utf16z("Somebody"));
        row.extend_from_slice(&0u16.to_le_bytes());
        let bytes = modify_bytes(&[(RECIPIENT_TYPE_TO, row)]);
        let (request, _) = ModifyRecipientsRequest::parse(&bytes).expect("parses");
        assert!(
            request.recipients.is_empty(),
            "a name with no address became a recipient"
        );
    }

    #[test]
    fn a_row_of_size_zero_removes_a_recipient_and_is_not_an_error() {
        let bytes = modify_bytes(&[(RECIPIENT_TYPE_TO, Vec::new())]);
        let (request, _) = ModifyRecipientsRequest::parse(&bytes).expect("parses");
        assert!(request.recipients.is_empty());
    }

    #[test]
    fn an_absurd_recipient_count_is_refused_before_any_allocation() {
        let mut out = vec![ROP_MODIFY_RECIPIENTS, 0x00, 0x01];
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(MAX_RECIPIENTS + 1).to_le_bytes());
        assert!(ModifyRecipientsRequest::parse(&out).is_err());
    }

    #[test]
    fn a_row_that_runs_past_its_declared_size_is_refused() {
        let mut out = vec![ROP_MODIFY_RECIPIENTS, 0x00, 0x01];
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // RowId
        out.push(RECIPIENT_TYPE_TO);
        out.extend_from_slice(&99u16.to_le_bytes()); // claims 99 bytes
        out.extend_from_slice(&[0x00, 0x00]); // has two
        assert!(ModifyRecipientsRequest::parse(&out).is_err());
    }

    #[test]
    fn save_changes_reads_the_response_slot_before_the_input_slot() {
        // The one operation here whose request names them in that order.
        // Reading them the usual way round makes a save appear to apply to a
        // different object than the one the client saved.
        let bytes = [ROP_SAVE_CHANGES_MESSAGE, 0x00, 0x02, 0x01, 0x0C, 0xAA];
        let (request, tail) = SaveChangesRequest::parse(&bytes).expect("parses");
        assert_eq!(request.response_handle_index, 0x02);
        assert_eq!(request.input_handle_index, 0x01);
        assert_eq!(request.save_flags, 0x0C);
        assert_eq!(tail, &[0xAA]);
    }

    #[test]
    fn a_save_response_carries_the_message_id() {
        let body = save_success_body(0x02, 0x0001_0000_00AB_CDEF);
        assert_eq!(body[0], ROP_SAVE_CHANGES_MESSAGE);
        assert_eq!(body[1], 0x02);
        assert_eq!(&body[2..6], &0u32.to_le_bytes());
        assert_eq!(body[6], 0x02);
        assert_eq!(
            u64::from_le_bytes(body[7..15].try_into().unwrap()),
            0x0001_0000_00AB_CDEF
        );
    }

    #[test]
    fn a_submit_request_is_four_bytes() {
        let bytes = [ROP_SUBMIT_MESSAGE, 0x00, 0x01, 0x00, 0xAA];
        let (request, tail) = SubmitMessageRequest::parse(&bytes).expect("parses");
        assert_eq!(request.input_handle_index, 0x01);
        assert_eq!(request.submit_flags, 0x00);
        assert_eq!(tail, &[0xAA]);
    }

    #[test]
    fn each_operation_refuses_another_operations_bytes() {
        let create = [ROP_CREATE_MESSAGE; CREATE_REQUEST_LEN];
        assert!(SubmitMessageRequest::parse(&create).is_err());
        assert!(SaveChangesRequest::parse(&create).is_err());
        assert!(SetPropertiesRequest::parse(&create).is_err());
        assert!(ModifyRecipientsRequest::parse(&create).is_err());
    }
}
