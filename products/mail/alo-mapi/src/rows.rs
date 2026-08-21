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
}

/// The property ids a folder row can answer ([MS-OXPROPS]).
pub mod pid {
    /// The folder's display name — `PtypString`.
    pub const DISPLAY_NAME: u16 = 0x3001;
    /// How many messages the folder holds — `PtypInteger32`.
    pub const CONTENT_COUNT: u16 = 0x3602;
    /// Whether the folder has children — `PtypBoolean`.
    pub const SUBFOLDERS: u16 = 0x360A;
    /// The folder's id — `PtypInteger64`.
    pub const FOLDER_ID: u16 = 0x6748;
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
