//! `RopQueryRows` ([MS-OXCROPS] §2.2.5.4) and the property values a row carries
//! ([MS-OXCDATA] §2.8, §2.11).
//!
//! Request:
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | `0x15` |
//! | `LogonId` | 1 | |
//! | `InputHandleIndex` | 1 | the table |
//! | `QueryRowsFlags` | 1 | |
//! | `ForwardRead` | 1 | a boolean |
//! | `RowCount` | 2 | how many rows are wanted |
//!
//! Success response: `RopId`, `InputHandleIndex`, `ReturnValue`, `Origin`,
//! `RowCount`, then that many `PropertyRow` structures.
//!
//! ## A row is positions, not names
//!
//! A `StandardPropertyRow` is a `0x00` flag byte followed by one value per
//! column, **in the order the columns were set**. Nothing in the row says which
//! property a value belongs to. That is why [`crate::columns`] stores the column
//! list on the table: it is the only thing that makes a row readable.
//!
//! The flag byte is the other half of that contract. `0x00` promises that every
//! value is present and without error; a row that cannot keep that promise must
//! be a `FlaggedPropertyRow` instead, where each value carries its own flag. So
//! a column we cannot answer is not something to fill with a zero — it changes
//! the shape of the whole row.
//!
//! ## Value encodings that are easy to get wrong
//!
//! * **`PtypBoolean` is one byte**, restricted to 0 or 1 — not the two bytes
//!   MAPI's `PT_BOOLEAN` occupies in other contexts. Two would shift every
//!   value after it in the row.
//! * **`PtypString` has no length prefix.** It is UTF-16LE "with terminating
//!   null character", and the terminator is what delimits it.
//! * **`PtypInteger64` is eight bytes little-endian**, which is how a folder id
//!   travels — the same 64 bits `RopOpenFolder` accepts back.

use crate::columns::PropertyTag;
use crate::rop::RopError;

/// The `RopId` of `RopQueryRows`.
pub const ROP_QUERY_ROWS: u8 = 0x15;

/// The fixed size of this request.
pub const REQUEST_LEN: usize = 7;

/// The most rows we will return in one response, whatever the client asks for.
///
/// The client's own `MaxRopOut` bounds the buffer, but a row is variable-sized,
/// so a count is the bound that can be applied before building any of them.
pub const MAX_ROWS: u16 = 512;

/// `Origin` — the cursor is at the start of the table ([MS-OXCTABL] §2.2.2.5.2).
pub const ORIGIN_BEGINNING: u8 = 0x00;
/// `Origin` — the cursor is at the end of the table.
pub const ORIGIN_END: u8 = 0x02;

/// Property types, as they appear in a tag ([MS-OXCDATA] §2.11.1).
pub mod ptyp {
    /// 4 bytes; a 32-bit integer.
    pub const INTEGER32: u16 = 0x0003;
    /// 4 bytes; an error code.
    pub const ERROR_CODE: u16 = 0x000A;
    /// **1 byte**, restricted to 0 or 1.
    pub const BOOLEAN: u16 = 0x000B;
    /// 8 bytes; a 64-bit integer.
    pub const INTEGER64: u16 = 0x0014;
    /// Variable; UTF-16LE with a terminating null and no length prefix.
    pub const STRING: u16 = 0x001F;
    /// 8 bytes; 100-nanosecond intervals since 1 January 1601 (a `FILETIME`).
    pub const TIME: u16 = 0x0040;
}

/// The property ids a folder or message row can answer ([MS-OXPROPS]).
pub mod pid {
    /// The folder's display name — `PtypString`.
    pub const DISPLAY_NAME: u16 = 0x3001;
    /// How many messages the folder holds — `PtypInteger32`.
    pub const CONTENT_COUNT: u16 = 0x3602;
    /// Whether the folder has children — `PtypBoolean`.
    pub const SUBFOLDERS: u16 = 0x360A;
    /// The folder's id — `PtypInteger64`.
    pub const FOLDER_ID: u16 = 0x6748;

    // ---- a message, as a contents-table row names it ----------------------

    /// The message's id — `PtypInteger64` ([MS-OXPROPS] §2.803).
    pub const MID: u16 = 0x674A;
    /// The message's subject — `PtypString` (§2.1035).
    pub const SUBJECT: u16 = 0x0037;
    /// The display name of whoever sent it — `PtypString` (§2.1006).
    pub const SENDER_NAME: u16 = 0x0C1A;
    /// When the server received it, in UTC — `PtypTime` (§2.791).
    pub const MESSAGE_DELIVERY_TIME: u16 = 0x0E06;
    /// The message's status bits — `PtypInteger32` (§2.793). See [`mf`].
    pub const MESSAGE_FLAGS: u16 = 0x0E07;
    /// Its size in bytes on the server — `PtypInteger32` (§2.798).
    pub const MESSAGE_SIZE: u16 = 0x0E08;
    /// Whether it has at least one attachment — `PtypBoolean` (§2.717).
    pub const HAS_ATTACHMENTS: u16 = 0x0E1B;
    /// What kind of item it is — `PtypString` (§2.789).
    pub const MESSAGE_CLASS: u16 = 0x001A;

    // ---- what an opened message adds --------------------------------------

    /// The body as plain text — `PtypString` (§2.618).
    pub const BODY: u16 = 0x1000;
    /// The `To` line, display names separated by semicolons — `PtypString`.
    pub const DISPLAY_TO: u16 = 0x0E04;
    /// The `Cc` line, likewise — `PtypString`.
    pub const DISPLAY_CC: u16 = 0x0E03;
    /// When the sender submitted it — `PtypTime` (§2.628).
    pub const CLIENT_SUBMIT_TIME: u16 = 0x0039;
    /// The `Message-ID` header — `PtypString`.
    pub const INTERNET_MESSAGE_ID: u16 = 0x1035;

    // ---- one attachment ---------------------------------------------------

    /// Its size in bytes — `PtypInteger32` (§2.573).
    pub const ATTACH_SIZE: u16 = 0x0E20;
    /// Its position within its message — `PtypInteger32` (§2.571).
    pub const ATTACH_NUMBER: u16 = 0x0E21;
    /// The bytes themselves — `PtypBinary` (§2.564).
    pub const ATTACH_DATA_BINARY: u16 = 0x3701;
    /// How the contents are reached — `PtypInteger32` (§2.601).
    pub const ATTACH_METHOD: u16 = 0x3705;
    /// Filename and extension — `PtypString` (§2.596).
    pub const ATTACH_LONG_FILENAME: u16 = 0x3707;
    /// Its content type — `PtypString` (§2.602).
    pub const ATTACH_MIME_TAG: u16 = 0x370E;
}

/// `PidTagMessageFlags` bits ([MS-OXCMSG] §2.2.1.6).
///
/// Only the bits alo can answer truthfully are here. The rest of the flag word
/// is real — `mfSubmitted`, `mfNotifyRead`, `mfFromMe` and the others — and is
/// deliberately absent rather than defaulted to zero, because a status bit a
/// client believes is worse than one it never saw.
pub mod mf {
    /// The message has been read.
    pub const READ: u32 = 0x0000_0001;
    /// Unmodified since it was delivered (or since first saved, if unsent).
    pub const UNMODIFIED: u32 = 0x0000_0002;
    /// Still being composed — a draft.
    pub const UNSENT: u32 = 0x0000_0008;
    /// Has at least one attachment; mirrors `PidTagHasAttachments`.
    pub const HAS_ATTACH: u32 = 0x0000_0010;
}

/// The message class alo reports for ordinary mail ([MS-OXCMSG] §2.2.1.3).
///
/// Everything alo keeps in a mailbox is a note. Calendar items and contacts are
/// separate objects that reach a client over CalDAV and CardDAV rather than as
/// MAPI message classes; when they become native MAPI classes this stops being
/// a constant.
pub const MESSAGE_CLASS_NOTE: &str = "IPM.Note";

/// The `FILETIME` epoch as a Unix timestamp: 1601-01-01 is this many seconds
/// *before* 1970-01-01.
///
/// Named rather than inlined because a wrong epoch does not fail — it silently
/// dates every message in the mailbox to the wrong century, and a client
/// renders that without complaint.
pub const FILETIME_EPOCH_OFFSET_SECS: i64 = 11_644_473_600;

/// A Unix timestamp in seconds as a `PtypTime` value.
///
/// Times before the `FILETIME` epoch, and times far enough beyond it to
/// overflow, clamp rather than wrap: a clamped date is visibly wrong, where a
/// wrapped one looks plausible and is not.
#[must_use]
pub fn filetime_from_unix_secs(secs: i64) -> u64 {
    let shifted = secs.saturating_add(FILETIME_EPOCH_OFFSET_SECS);
    u64::try_from(shifted)
        .unwrap_or(0)
        .saturating_mul(10_000_000)
}

/// A value a row can carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// A 32-bit integer.
    Integer32(u32),
    /// A 64-bit integer — a folder id, among other things.
    Integer64(u64),
    /// A boolean, written as one byte.
    Boolean(bool),
    /// A string, written UTF-16LE with a terminating null.
    String(String),
    /// A time, written as a `FILETIME` — see [`filetime_from_unix_secs`].
    Time(u64),
}

impl Value {
    /// The property type this value is written as.
    #[must_use]
    pub const fn property_type(&self) -> u16 {
        match self {
            Self::Integer32(_) => ptyp::INTEGER32,
            Self::Integer64(_) => ptyp::INTEGER64,
            Self::Boolean(_) => ptyp::BOOLEAN,
            Self::String(_) => ptyp::STRING,
            Self::Time(_) => ptyp::TIME,
        }
    }

    /// Appends this value's bytes.
    pub fn write(&self, out: &mut Vec<u8>) {
        match self {
            Self::Integer32(value) => out.extend_from_slice(&value.to_le_bytes()),
            Self::Integer64(value) => out.extend_from_slice(&value.to_le_bytes()),
            // One byte. Two would shift every value after it in the row.
            Self::Boolean(value) => out.push(u8::from(*value)),
            Self::String(value) => {
                for unit in value.encode_utf16() {
                    out.extend_from_slice(&unit.to_le_bytes());
                }
                out.extend_from_slice(&[0, 0]);
            }
            // Eight bytes little-endian, exactly like an Integer64 — the type
            // differs, the encoding does not.
            Self::Time(value) => out.extend_from_slice(&value.to_le_bytes()),
        }
    }
}

/// A parsed `RopQueryRows` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryRowsRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the table being read.
    pub input_handle_index: u8,
    /// Flags controlling the operation.
    pub flags: u8,
    /// Whether to read forwards from the cursor.
    pub forward_read: bool,
    /// How many rows the client wants.
    pub row_count: u16,
}

impl QueryRowsRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if fewer than [`REQUEST_LEN`] bytes remain, or
    /// the leading byte is not this operation.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let fixed = input.get(..REQUEST_LEN).ok_or(RopError::Truncated {
            part: "RopQueryRows",
        })?;
        if fixed[0] != ROP_QUERY_ROWS {
            return Err(RopError::Truncated {
                part: "RopQueryRows",
            });
        }
        Ok((
            Self {
                logon_id: fixed[1],
                input_handle_index: fixed[2],
                flags: fixed[3],
                forward_read: fixed[4] != 0,
                row_count: u16::from_le_bytes([fixed[5], fixed[6]]),
            },
            &input[REQUEST_LEN..],
        ))
    }
}

/// Writes one row's values for `columns`.
///
/// Returns `None` when any column cannot be answered — the caller must then
/// build a `FlaggedPropertyRow` rather than a standard one, because the `0x00`
/// flag is a promise that every value is present and without error.
#[must_use]
pub fn standard_row(
    columns: &[PropertyTag],
    value_of: &dyn Fn(PropertyTag) -> Option<Value>,
) -> Option<Vec<u8>> {
    let mut out = vec![0x00]; // Flag: all values present, no errors.
    for tag in columns {
        let value = value_of(*tag)?;
        // The value must be the type the client asked for. Writing a string
        // where an integer was requested would decode as the next columns'
        // bytes, so a mismatch is a refusal rather than a coercion.
        if value.property_type() != tag.property_type {
            return None;
        }
        value.write(&mut out);
    }
    Some(out)
}

/// `PropertyRow` — every value is present and correct ([MS-OXCDATA] §2.8.1).
pub const ROW_FLAG_STANDARD: u8 = 0x00;
/// `PropertyRow` — some value is missing or in error.
pub const ROW_FLAG_FLAGGED: u8 = 0x01;

/// `FlaggedPropertyValue` — the value follows ([MS-OXCDATA] §2.11.5).
pub const VALUE_FLAG_PRESENT: u8 = 0x00;
/// `FlaggedPropertyValue` — the value is absent and **nothing follows**.
pub const VALUE_FLAG_ABSENT: u8 = 0x01;

/// Builds a `FlaggedPropertyRow` answering `tags` positionally.
///
/// Unlike [`standard_row`], one unanswerable property does not refuse the whole
/// row. A client fetching a message asks for dozens of properties and fully
/// expects some of them not to exist — refusing everything because one was
/// absent would mean a message with no `Date` header could not be opened at
/// all. So each value carries its own flag, and an absent one is marked absent.
///
/// `size_limit` is the client's own ceiling on a value it will accept, with
/// zero meaning none. A value over it is marked **absent** rather than sent
/// anyway or truncated:
///
/// * Sending it anyway ignores a limit the client set for its own protection.
/// * Truncating produces a body that looks whole and is not — the one outcome
///   nobody downstream can detect.
///
/// Marking it absent is the honest third option, and it is what leads a client
/// to fetch the value as a stream instead. Until streams are served, a body
/// over the client's limit is one it will not display — a real limit, and a
/// visible one, rather than silent corruption.
///
/// Returns `None` only if a value's type disagrees with the tag that asked for
/// it, which is our bug rather than the client's.
#[must_use]
pub fn property_row(
    tags: &[PropertyTag],
    value_of: &dyn Fn(PropertyTag) -> Option<Value>,
    size_limit: usize,
) -> Option<Vec<u8>> {
    let mut values: Vec<(bool, Vec<u8>)> = Vec::with_capacity(tags.len());
    let mut any_absent = false;
    for tag in tags {
        match value_of(*tag) {
            Some(value) => {
                if value.property_type() != tag.property_type {
                    return None;
                }
                let mut bytes = Vec::new();
                value.write(&mut bytes);
                if size_limit > 0 && bytes.len() > size_limit {
                    any_absent = true;
                    values.push((false, Vec::new()));
                } else {
                    values.push((true, bytes));
                }
            }
            None => {
                any_absent = true;
                values.push((false, Vec::new()));
            }
        }
    }

    // A row with nothing missing is written in the standard form, which is
    // what a client parses fastest and what the tables already produce.
    if !any_absent {
        let mut out = vec![ROW_FLAG_STANDARD];
        for (_, bytes) in &values {
            out.extend_from_slice(bytes);
        }
        return Some(out);
    }

    let mut out = vec![ROW_FLAG_FLAGGED];
    for (present, bytes) in &values {
        if *present {
            out.push(VALUE_FLAG_PRESENT);
            out.extend_from_slice(bytes);
        } else {
            // Nothing follows an absent value — not a zero, not a placeholder.
            out.push(VALUE_FLAG_ABSENT);
        }
    }
    Some(out)
}

/// Builds a `RopQueryRows` success response.
#[must_use]
pub fn success_body(input_handle_index: u8, origin: u8, rows: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::with_capacity(9 + rows.iter().map(Vec::len).sum::<usize>());
    out.push(ROP_QUERY_ROWS);
    out.push(input_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out.push(origin);
    out.extend_from_slice(&u16::try_from(rows.len()).unwrap_or(u16::MAX).to_le_bytes());
    for row in rows {
        out.extend_from_slice(row);
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn request(count: u16) -> Vec<u8> {
        let mut out = vec![ROP_QUERY_ROWS, 0x00, 0x02, 0x00, 0x01];
        out.extend_from_slice(&count.to_le_bytes());
        out
    }

    #[test]
    fn a_request_reads_back_field_for_field() {
        let raw = request(32);
        let (query, rest) = QueryRowsRequest::parse(&raw).unwrap();
        assert_eq!(query.input_handle_index, 2);
        assert!(query.forward_read);
        assert_eq!(query.row_count, 32);
        assert!(rest.is_empty());
    }

    #[test]
    fn every_truncation_is_an_error() {
        let full = request(4);
        for cut in 0..full.len() {
            assert!(
                QueryRowsRequest::parse(&full[..cut]).is_err(),
                "accepted a request cut at {cut}"
            );
        }
    }

    /// A boolean is **one** byte. Two is what MAPI's `PT_BOOLEAN` occupies
    /// elsewhere, and using two here would shift every value after it.
    #[test]
    fn a_boolean_is_exactly_one_byte() {
        let mut out = Vec::new();
        Value::Boolean(true).write(&mut out);
        assert_eq!(out, vec![1]);

        out.clear();
        Value::Boolean(false).write(&mut out);
        assert_eq!(out, vec![0]);
    }

    /// A string is UTF-16LE with a terminating null and **no length prefix** —
    /// the terminator is what delimits it.
    #[test]
    fn a_string_is_utf16le_with_a_terminator_and_no_prefix() {
        let mut out = Vec::new();
        Value::String("Inbox".to_owned()).write(&mut out);
        let expected: Vec<u8> = "Inbox"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .chain([0, 0])
            .collect();
        assert_eq!(out, expected);
        assert_eq!(out.len(), 12, "5 characters, two bytes each, plus a NUL");

        // A European folder name survives intact — the case this protocol is
        // most often got wrong on.
        out.clear();
        Value::String("Gelöschte Elemente".to_owned()).write(&mut out);
        let expected: Vec<u8> = "Gelöschte Elemente"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .chain([0, 0])
            .collect();
        assert_eq!(out, expected);
    }

    #[test]
    fn integers_are_little_endian_and_their_stated_width() {
        let mut out = Vec::new();
        Value::Integer32(0x1234_5678).write(&mut out);
        assert_eq!(out, vec![0x78, 0x56, 0x34, 0x12]);

        out.clear();
        Value::Integer64(0x0102_0304_0506_0708).write(&mut out);
        assert_eq!(out, vec![0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]);
    }

    /// A row is the flag byte then values in **column order**, with nothing
    /// naming them. Reordering the columns reorders the bytes.
    #[test]
    fn a_row_writes_values_in_the_order_the_columns_were_set() {
        let name = PropertyTag {
            property_type: ptyp::STRING,
            property_id: pid::DISPLAY_NAME,
        };
        let count = PropertyTag {
            property_type: ptyp::INTEGER32,
            property_id: pid::CONTENT_COUNT,
        };
        let answer = |tag: PropertyTag| match tag.property_id {
            pid::DISPLAY_NAME => Some(Value::String("Inbox".to_owned())),
            pid::CONTENT_COUNT => Some(Value::Integer32(7)),
            _ => None,
        };

        let row = standard_row(&[name, count], &answer).unwrap();
        assert_eq!(row[0], 0x00, "all values present, no errors");
        let mut expected = vec![0x00];
        Value::String("Inbox".to_owned()).write(&mut expected);
        Value::Integer32(7).write(&mut expected);
        assert_eq!(row, expected);

        // The other order produces different bytes — which is the whole reason
        // the column list has to be remembered per table.
        let reversed = standard_row(&[count, name], &answer).unwrap();
        assert_ne!(reversed, row);
    }

    /// The `0x00` flag promises every value is present and without error, so a
    /// column we cannot answer means this is not a standard row at all.
    #[test]
    fn a_column_we_cannot_answer_is_not_a_standard_row() {
        let known = PropertyTag {
            property_type: ptyp::STRING,
            property_id: pid::DISPLAY_NAME,
        };
        let unknown = PropertyTag {
            property_type: ptyp::STRING,
            property_id: 0x1234,
        };
        let answer = |tag: PropertyTag| {
            (tag.property_id == pid::DISPLAY_NAME).then(|| Value::String("Inbox".to_owned()))
        };

        assert!(standard_row(&[known], &answer).is_some());
        assert!(
            standard_row(&[known, unknown], &answer).is_none(),
            "promised a complete row it could not fill"
        );
    }

    /// A value must be the type the client asked for. Writing a string where an
    /// integer was requested would be read as the following columns' bytes.
    #[test]
    fn a_value_of_the_wrong_type_is_refused_rather_than_coerced() {
        let asked_for_an_integer = PropertyTag {
            property_type: ptyp::INTEGER32,
            property_id: pid::DISPLAY_NAME,
        };
        let answer = |_: PropertyTag| Some(Value::String("Inbox".to_owned()));
        assert!(standard_row(&[asked_for_an_integer], &answer).is_none());
    }

    #[test]
    fn a_success_response_carries_its_rows_and_counts_them() {
        let rows = vec![vec![0x00, 0x01], vec![0x00, 0x02]];
        let body = success_body(2, ORIGIN_BEGINNING, &rows);
        assert_eq!(body[0], ROP_QUERY_ROWS);
        assert_eq!(body[1], 2);
        assert_eq!(&body[2..6], &0u32.to_le_bytes());
        assert_eq!(body[6], ORIGIN_BEGINNING);
        assert_eq!(&body[7..9], &2u16.to_le_bytes());
        assert_eq!(&body[9..], &[0x00, 0x01, 0x00, 0x02]);
    }

    #[test]
    fn an_empty_table_is_a_success_with_no_rows() {
        let body = success_body(2, ORIGIN_END, &[]);
        assert_eq!(body.len(), 9);
        assert_eq!(&body[7..9], &0u16.to_le_bytes());
    }
}
