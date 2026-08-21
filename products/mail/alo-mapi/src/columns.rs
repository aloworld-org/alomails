//! `RopSetColumns` ([MS-OXCROPS] §2.2.5.1, [MS-OXCTABL] §2.2.2.2) — choosing
//! which properties a table's rows will carry.
//!
//! Request:
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | `0x12` |
//! | `LogonId` | 1 | |
//! | `InputHandleIndex` | 1 | the table |
//! | `SetColumnsFlags` | 1 | |
//! | `PropertyTagCount` | 2 | |
//! | `PropertyTags` | 4 × count | |
//!
//! Success response, 7 bytes: `RopId`, `InputHandleIndex`, `ReturnValue`,
//! `TableStatus`.
//!
//! ## Why this operation exists before rows do
//!
//! A row carries no names. `RopQueryRows` returns a `StandardPropertyRow` whose
//! values are laid out **in the order the columns were set**, with nothing in
//! the row saying which property each value belongs to — the client matches
//! them up by position against the list it sent here.
//!
//! So the column list is not a hint about what to include; it is the schema of
//! every row that follows, and it has to be remembered per table. A server that
//! returned values in its own preferred order would produce rows that decode
//! without error into the wrong fields.
//!
//! **A property tag is type-then-id**, both 16-bit and little-endian
//! ([MS-OXCDATA] §2.9). The type coming first is easy to get backwards, and a
//! reversed tag is a valid-looking tag for a property nobody asked about.

use crate::rop::RopError;

/// The `RopId` of `RopSetColumns`.
pub const ROP_SET_COLUMNS: u8 = 0x12;

/// The size of a success response.
pub const RESPONSE_LEN: usize = 7;

/// The most columns we will accept for one table.
///
/// The count is client-declared and each tag is four bytes, so this bounds both
/// the allocation and the per-row work every later `RopQueryRows` will do.
pub const MAX_COLUMNS: usize = 512;

/// `TableStatus` — the table is complete and not being rebuilt
/// ([MS-OXCTABL] §2.2.2.1.3).
pub const TABLE_STATUS_COMPLETE: u8 = 0x00;

/// A property tag: the data type of a value and the property it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertyTag {
    /// The data type of the value ([MS-OXCDATA] §2.11.1).
    pub property_type: u16,
    /// The property this tag names.
    pub property_id: u16,
}

impl PropertyTag {
    /// The four bytes of this tag: **type first**, then id, both little-endian.
    #[must_use]
    pub fn to_bytes(self) -> [u8; 4] {
        let mut out = [0u8; 4];
        out[0..2].copy_from_slice(&self.property_type.to_le_bytes());
        out[2..4].copy_from_slice(&self.property_id.to_le_bytes());
        out
    }

    /// Reads a tag from exactly four bytes.
    #[must_use]
    pub fn from_bytes(raw: [u8; 4]) -> Self {
        Self {
            property_type: u16::from_le_bytes([raw[0], raw[1]]),
            property_id: u16::from_le_bytes([raw[2], raw[3]]),
        }
    }
}

/// A parsed `RopSetColumns` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetColumnsRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the table being configured.
    pub input_handle_index: u8,
    /// Flags controlling the operation.
    pub flags: u8,
    /// The columns, in the order every later row will present them.
    pub columns: Vec<PropertyTag>,
}

impl SetColumnsRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if the buffer ends inside a field, and
    /// [`RopError::RopSize`] if the declared column count is not backed by the
    /// bytes that arrived or exceeds [`MAX_COLUMNS`].
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let fixed = input.get(..6).ok_or(RopError::Truncated {
            part: "RopSetColumns",
        })?;
        if fixed[0] != ROP_SET_COLUMNS {
            return Err(RopError::Truncated {
                part: "RopSetColumns",
            });
        }
        let count = usize::from(u16::from_le_bytes([fixed[4], fixed[5]]));
        if count > MAX_COLUMNS {
            return Err(RopError::RopSize {
                size: u16::try_from(count).unwrap_or(u16::MAX),
                available: input.len(),
            });
        }
        let tags_len = count * 4;
        let raw = input.get(6..6 + tags_len).ok_or(RopError::Truncated {
            part: "PropertyTags",
        })?;

        let columns = raw
            .chunks_exact(4)
            .map(|chunk| PropertyTag::from_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();

        Ok((
            Self {
                logon_id: fixed[1],
                input_handle_index: fixed[2],
                flags: fixed[3],
                columns,
            },
            &input[6 + tags_len..],
        ))
    }
}

/// Builds a `RopSetColumns` success response.
///
/// `TableStatus` reports the table complete: ours are built from a fixed
/// hierarchy and are never asynchronously repopulated, so there is no state in
/// which a client should be told to wait.
#[must_use]
pub fn success_body(input_handle_index: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(RESPONSE_LEN);
    out.push(ROP_SET_COLUMNS);
    out.push(input_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out.push(TABLE_STATUS_COMPLETE);
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn request(tags: &[PropertyTag]) -> Vec<u8> {
        let mut out = vec![ROP_SET_COLUMNS, 0x00, 0x01, 0x00];
        out.extend_from_slice(&u16::try_from(tags.len()).unwrap().to_le_bytes());
        for tag in tags {
            out.extend_from_slice(&tag.to_bytes());
        }
        out
    }

    /// A tag is **type first, then id**. Reversed, it is still a valid-looking
    /// tag — for a property nobody asked about — so the order is pinned on
    /// bytes rather than left to a round trip through our own encoder.
    #[test]
    fn a_property_tag_is_type_then_id() {
        let tag = PropertyTag {
            property_type: 0x001F,
            property_id: 0x3001,
        };
        assert_eq!(tag.to_bytes(), [0x1F, 0x00, 0x01, 0x30]);
        assert_eq!(PropertyTag::from_bytes([0x1F, 0x00, 0x01, 0x30]), tag);

        // The reversed bytes are a different tag, not the same one.
        assert_ne!(PropertyTag::from_bytes([0x01, 0x30, 0x1F, 0x00]), tag);
    }

    /// The column order is the schema of every row that follows, so it must
    /// survive parsing exactly — a set that came back sorted or deduplicated
    /// would put values in the wrong fields of every later row.
    #[test]
    fn the_column_order_is_preserved_exactly() {
        let tags = [
            PropertyTag {
                property_type: 0x001F,
                property_id: 0x3001,
            },
            PropertyTag {
                property_type: 0x0003,
                property_id: 0x3602,
            },
            PropertyTag {
                property_type: 0x000B,
                property_id: 0x360A,
            },
            // Deliberately a repeat: a client may ask for the same property
            // twice, and the row must carry it twice.
            PropertyTag {
                property_type: 0x001F,
                property_id: 0x3001,
            },
        ];
        let raw = request(&tags);
        let (set, rest) = SetColumnsRequest::parse(&raw).unwrap();
        assert_eq!(set.columns, tags, "the order or the repeats changed");
        assert_eq!(set.input_handle_index, 1);
        assert!(rest.is_empty());
    }

    #[test]
    fn an_empty_column_set_is_legal() {
        let raw = request(&[]);
        let (set, rest) = SetColumnsRequest::parse(&raw).unwrap();
        assert!(set.columns.is_empty());
        assert!(rest.is_empty());
    }

    #[test]
    fn the_remainder_is_handed_back_for_the_next_operation() {
        let mut raw = request(&[PropertyTag {
            property_type: 0x001F,
            property_id: 0x3001,
        }]);
        raw.extend_from_slice(&[0x15, 0x00, 0x01]);
        let (_, rest) = SetColumnsRequest::parse(&raw).unwrap();
        assert_eq!(rest, &[0x15, 0x00, 0x01]);
    }

    /// The count is client-declared, so it is checked against the bytes that
    /// arrived and against a ceiling of our own — every column costs work on
    /// every later row, not just four bytes here.
    #[test]
    fn a_column_count_longer_than_the_buffer_is_refused() {
        let mut lying = request(&[PropertyTag {
            property_type: 0x001F,
            property_id: 0x3001,
        }]);
        lying[4..6].copy_from_slice(&50u16.to_le_bytes());
        assert!(SetColumnsRequest::parse(&lying).is_err());

        let mut huge = request(&[]);
        huge[4..6].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(matches!(
            SetColumnsRequest::parse(&huge),
            Err(RopError::RopSize { .. })
        ));
    }

    #[test]
    fn every_truncation_is_an_error() {
        let full = request(&[
            PropertyTag {
                property_type: 0x001F,
                property_id: 0x3001,
            },
            PropertyTag {
                property_type: 0x0003,
                property_id: 0x3602,
            },
        ]);
        for cut in 0..full.len() {
            assert!(
                SetColumnsRequest::parse(&full[..cut]).is_err(),
                "accepted a request cut at {cut}"
            );
        }
    }

    #[test]
    fn a_success_response_is_seven_bytes() {
        let body = success_body(1);
        assert_eq!(body.len(), RESPONSE_LEN);
        assert_eq!(body[0], ROP_SET_COLUMNS);
        assert_eq!(body[1], 1, "the input index is echoed");
        assert_eq!(&body[2..6], &0u32.to_le_bytes());
        assert_eq!(body[6], TABLE_STATUS_COMPLETE);
    }
}
