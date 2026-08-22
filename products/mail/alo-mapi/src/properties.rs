//! `RopGetPropertiesSpecific` ([MS-OXCROPS] §2.2.8.3, [MS-OXCPRPT] §2.2.2) —
//! asking an open object for a named list of its properties.
//!
//! This is how a client reads a message: `RopOpenMessage` gets a handle, and
//! this asks that handle for the subject, the body, the To line and whatever
//! else it wants to display.
//!
//! Request:
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | `0x07` |
//! | `LogonId` | 1 | |
//! | `InputHandleIndex` | 1 | the object being asked |
//! | `PropertySizeLimit` | 2 | the largest value the client will accept |
//! | `WantUnicode` | 2 | a Boolean, **two bytes** |
//! | `PropertyTagCount` | 2 | |
//! | `PropertyTags` | var | that many 4-byte tags |
//!
//! Success response:
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | `0x07` |
//! | `InputHandleIndex` | 1 | echoed **from the request's input index** |
//! | `ReturnValue` | 4 | `0x00000000` |
//! | `RowData` | var | one `PropertyRow` |
//!
//! Two things here are easy to get wrong and silent when wrong:
//!
//! * **`WantUnicode` is two bytes, not one.** A one-byte read leaves the
//!   parser one byte behind for the rest of the buffer, and the tag count then
//!   comes out of the middle of a tag.
//! * The response echoes the **input** handle index. Every table operation in
//!   this crate echoes an *output* index, so the habit is the wrong one here.
//!
//! ## `PropertySizeLimit`
//!
//! The client states the largest value it will accept, and zero means no
//! limit. A value over the limit must not simply be sent anyway — a client
//! that asked for at most 8 KB and receives a 2 MB body has had its own
//! protection ignored. Over-long values are returned as `PtypErrorCode` with
//! `ecPropSizeLimit`, which is the specification's way of saying "this exists
//! and is too big for what you asked" — a client then fetches it as a stream.
//! Streams are a later stage; until then a body over the limit is reported as
//! too large rather than truncated, because a truncated body that looks whole
//! is the one outcome nobody can detect.

use crate::columns::PropertyTag;
use crate::rop::RopError;

/// The `RopId` of `RopGetPropertiesSpecific`.
pub const ROP_GET_PROPERTIES_SPECIFIC: u8 = 0x07;

/// The size of this request before its tag list.
pub const REQUEST_HEAD_LEN: usize = 9;

/// The most property tags one request may name.
///
/// A ceiling on work a caller can ask for in one operation. Far above what any
/// client sends — Outlook asks for a few dozen — and low enough that a crafted
/// buffer cannot make us build an enormous row.
pub const MAX_TAGS: u16 = 1024;

/// A parsed `RopGetPropertiesSpecific` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetPropertiesRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the object being asked.
    pub input_handle_index: u8,
    /// The largest value the client will accept; zero means no limit.
    pub property_size_limit: u16,
    /// Whether the client wants strings in Unicode.
    ///
    /// Read and carried, and alo answers `PtypString` either way: the only
    /// alternative is `PtypString8` in some code page, which is how "Liège"
    /// becomes "Li?ge". A client that asked for 8-bit strings and receives
    /// Unicode reads them correctly; the reverse is not true.
    pub want_unicode: bool,
    /// The properties asked for, in order. A row answers them positionally.
    pub tags: Vec<PropertyTag>,
}

impl GetPropertiesRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if the head or the tag list is short, the
    /// leading byte is not this operation, or more than [`MAX_TAGS`] tags are
    /// named.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let head = input.get(..REQUEST_HEAD_LEN).ok_or(RopError::Truncated {
            part: "RopGetPropertiesSpecific",
        })?;
        if head[0] != ROP_GET_PROPERTIES_SPECIFIC {
            return Err(RopError::Truncated {
                part: "RopGetPropertiesSpecific",
            });
        }
        let count = u16::from_le_bytes([head[7], head[8]]);
        if count > MAX_TAGS {
            return Err(RopError::Truncated {
                part: "RopGetPropertiesSpecific",
            });
        }
        let bytes = usize::from(count) * PropertyTag::LEN;
        let list = input
            .get(REQUEST_HEAD_LEN..REQUEST_HEAD_LEN + bytes)
            .ok_or(RopError::Truncated {
                part: "RopGetPropertiesSpecific",
            })?;
        let tags = list
            .chunks_exact(PropertyTag::LEN)
            .map(|raw| PropertyTag::from_bytes([raw[0], raw[1], raw[2], raw[3]]))
            .collect();
        Ok((
            Self {
                logon_id: head[1],
                input_handle_index: head[2],
                property_size_limit: u16::from_le_bytes([head[3], head[4]]),
                // Two bytes. A one-byte read here puts every later field one
                // byte out and the tag count lands inside a tag.
                want_unicode: u16::from_le_bytes([head[5], head[6]]) != 0,
                tags,
            },
            &input[REQUEST_HEAD_LEN + bytes..],
        ))
    }
}

/// Builds the success response around an already-built `PropertyRow`.
///
/// The index echoed is the request's **input** handle index, not an output one:
/// this operation reads an object rather than creating one.
#[must_use]
pub fn success_body(input_handle_index: u8, row: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + row.len());
    out.push(ROP_GET_PROPERTIES_SPECIFIC);
    out.push(input_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out.extend_from_slice(row);
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{
        GetPropertiesRequest, MAX_TAGS, REQUEST_HEAD_LEN, ROP_GET_PROPERTIES_SPECIFIC, success_body,
    };
    use crate::columns::PropertyTag;

    fn request_bytes(limit: u16, unicode: u16, tags: &[PropertyTag]) -> Vec<u8> {
        let mut out = vec![ROP_GET_PROPERTIES_SPECIFIC, 0x00, 0x01];
        out.extend_from_slice(&limit.to_le_bytes());
        out.extend_from_slice(&unicode.to_le_bytes());
        out.extend_from_slice(&u16::try_from(tags.len()).unwrap().to_le_bytes());
        for tag in tags {
            out.extend_from_slice(&tag.to_bytes());
        }
        out
    }

    fn tag(property_type: u16, property_id: u16) -> PropertyTag {
        PropertyTag {
            property_type,
            property_id,
        }
    }

    #[test]
    fn a_request_reads_its_tags_in_order() {
        let wanted = [tag(0x001F, 0x0037), tag(0x001F, 0x1000)];
        let mut bytes = request_bytes(8192, 1, &wanted);
        bytes.extend_from_slice(&[0xAA, 0xBB]);
        let (request, tail) = GetPropertiesRequest::parse(&bytes).expect("parses");
        assert_eq!(request.input_handle_index, 0x01);
        assert_eq!(request.property_size_limit, 8192);
        assert!(request.want_unicode);
        assert_eq!(request.tags, wanted.to_vec());
        assert_eq!(tail, &[0xAA, 0xBB]);
    }

    #[test]
    fn want_unicode_is_two_bytes_wide() {
        // The trap: a one-byte read would leave the parser one byte behind and
        // the tag count would come out of the middle of a tag. Setting only
        // the *high* byte proves both bytes are consumed.
        let wanted = [tag(0x001F, 0x0037)];
        let bytes = request_bytes(0, 0x0100, &wanted);
        let (request, tail) = GetPropertiesRequest::parse(&bytes).expect("parses");
        assert!(request.want_unicode);
        assert_eq!(request.tags, wanted.to_vec());
        assert!(tail.is_empty(), "the tag list ended where it should");
    }

    #[test]
    fn no_tags_is_a_valid_request() {
        let bytes = request_bytes(0, 0, &[]);
        let (request, tail) = GetPropertiesRequest::parse(&bytes).expect("parses");
        assert!(request.tags.is_empty());
        assert!(tail.is_empty());
        assert_eq!(bytes.len(), REQUEST_HEAD_LEN);
    }

    #[test]
    fn a_tag_list_shorter_than_its_count_is_refused() {
        // Not padded with zeros: the missing tags would be answered as
        // property zero, and the row would be a shape the client cannot read.
        let mut bytes = request_bytes(0, 1, &[tag(0x001F, 0x0037)]);
        bytes.truncate(bytes.len() - 1);
        assert!(GetPropertiesRequest::parse(&bytes).is_err());
    }

    #[test]
    fn an_absurd_tag_count_is_refused_before_any_allocation() {
        let mut bytes = vec![ROP_GET_PROPERTIES_SPECIFIC, 0x00, 0x01];
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&(MAX_TAGS + 1).to_le_bytes());
        assert!(GetPropertiesRequest::parse(&bytes).is_err());
    }

    #[test]
    fn another_operations_bytes_are_not_this_one() {
        let mut bytes = request_bytes(0, 1, &[]);
        bytes[0] = 0x12; // RopSetColumns
        assert!(GetPropertiesRequest::parse(&bytes).is_err());
    }

    #[test]
    fn the_response_echoes_the_input_handle_index() {
        // Not an output index: this operation reads an object, it does not
        // create one, and every other table operation here echoes an output
        // index — which makes this the easy one to get wrong.
        let body = success_body(0x03, &[0x00, 0x01, 0x02]);
        assert_eq!(body[0], ROP_GET_PROPERTIES_SPECIFIC);
        assert_eq!(body[1], 0x03);
        assert_eq!(&body[2..6], &0u32.to_le_bytes());
        assert_eq!(&body[6..], &[0x00, 0x01, 0x02]);
    }
}
