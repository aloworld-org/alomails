//! `RopOpenFolder` ([MS-OXCROPS] §2.2.4.1, [MS-OXCFOLD] §2.2.1.1) — opening one
//! of the folders a logon named.
//!
//! Request, 13 bytes:
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | `0x02` |
//! | `LogonId` | 1 | |
//! | `InputHandleIndex` | 1 | the logon this folder is opened through |
//! | `OutputHandleIndex` | 1 | where the folder's handle goes |
//! | `FolderId` | 8 | which folder |
//! | `OpenModeFlags` | 1 | |
//!
//! Success response, 8 bytes when the folder is not ghosted:
//!
//! | Field | Size |
//! |---|---|
//! | `RopId` | 1 |
//! | `OutputHandleIndex` | 1 |
//! | `ReturnValue` | 4 |
//! | `HasRules` | 1 |
//! | `IsGhosted` | 1 |
//!
//! **`IsGhosted` decides whether the response has three more fields.** When it
//! is non-zero, `ServerCount`, `CheapServerCount` and a list of server names
//! follow; when it is zero they are absent entirely — not zero-valued, absent.
//! We are the only replica of everything we serve, so it is always zero here
//! and the response is always eight bytes. A server that wrote the optional
//! fields anyway would produce a response the client reads as eight bytes
//! followed by the start of some other operation.
//!
//! **This operation is the first that requires a handle rather than issuing
//! one.** The input handle names the logon the folder is opened through, and
//! that handle only exists inside the Session Context that created it — which
//! is what keeps one caller's folder handles unreachable from another's.

use crate::rop::RopError;

/// The `RopId` of `RopOpenFolder`.
pub const ROP_OPEN_FOLDER: u8 = 0x02;

/// The fixed size of this request.
pub const REQUEST_LEN: usize = 13;

/// The size of a success response for a folder that is not ghosted.
pub const RESPONSE_LEN: usize = 8;

/// A parsed `RopOpenFolder` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenFolderRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the logon to open the folder through.
    pub input_handle_index: u8,
    /// The handle-table slot the opened folder's handle goes into.
    pub output_handle_index: u8,
    /// The folder being opened, as the 64-bit value on the wire.
    pub folder_id: u64,
    /// Flags controlling how the folder is opened.
    pub open_mode_flags: u8,
}

impl OpenFolderRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if fewer than [`REQUEST_LEN`] bytes remain, or
    /// the leading byte is not this operation.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let fixed = input.get(..REQUEST_LEN).ok_or(RopError::Truncated {
            part: "RopOpenFolder",
        })?;
        if fixed[0] != ROP_OPEN_FOLDER {
            return Err(RopError::Truncated {
                part: "RopOpenFolder",
            });
        }
        let folder_id = u64::from_le_bytes([
            fixed[4], fixed[5], fixed[6], fixed[7], fixed[8], fixed[9], fixed[10], fixed[11],
        ]);
        Ok((
            Self {
                logon_id: fixed[1],
                input_handle_index: fixed[2],
                output_handle_index: fixed[3],
                folder_id,
                open_mode_flags: fixed[12],
            },
            &input[REQUEST_LEN..],
        ))
    }
}

/// Builds a `RopOpenFolder` success response for a folder we hold ourselves.
///
/// `IsGhosted` is zero and the optional fields are therefore **absent**, which
/// is why this is always [`RESPONSE_LEN`] bytes.
#[must_use]
pub fn success_body(output_handle_index: u8, has_rules: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(RESPONSE_LEN);
    out.push(ROP_OPEN_FOLDER);
    out.push(output_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out.push(u8::from(has_rules));
    out.push(0); // IsGhosted: we are the only replica.
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn request(folder_id: u64) -> Vec<u8> {
        let mut out = vec![ROP_OPEN_FOLDER, 0x00, 0x00, 0x01];
        out.extend_from_slice(&folder_id.to_le_bytes());
        out.push(0x00);
        out
    }

    #[test]
    fn a_request_reads_back_field_for_field() {
        let raw = request(0x0000_0005_0000_0001);
        let (open, rest) = OpenFolderRequest::parse(&raw).unwrap();
        assert_eq!(open.logon_id, 0);
        assert_eq!(open.input_handle_index, 0);
        assert_eq!(open.output_handle_index, 1);
        assert_eq!(open.folder_id, 0x0000_0005_0000_0001);
        assert_eq!(open.open_mode_flags, 0);
        assert!(rest.is_empty());
    }

    #[test]
    fn the_remainder_is_handed_back_for_the_next_operation() {
        let mut raw = request(1);
        raw.extend_from_slice(&[0x01, 0x00, 0x00]);
        let (_, rest) = OpenFolderRequest::parse(&raw).unwrap();
        assert_eq!(rest, &[0x01, 0x00, 0x00]);
    }

    #[test]
    fn every_truncation_is_an_error() {
        let full = request(1);
        for cut in 0..full.len() {
            assert!(
                OpenFolderRequest::parse(&full[..cut]).is_err(),
                "accepted a request cut at {cut}"
            );
        }
    }

    #[test]
    fn another_rop_id_is_not_an_open_folder() {
        let mut raw = request(1);
        raw[0] = 0xFE;
        assert!(OpenFolderRequest::parse(&raw).is_err());
    }

    /// Eight bytes, and no more. `IsGhosted` of zero means the three optional
    /// fields are absent rather than zero-valued — a response that wrote them
    /// anyway would be read as eight bytes followed by the start of some other
    /// operation.
    #[test]
    fn a_success_response_is_eight_bytes_with_the_optional_fields_absent() {
        let body = success_body(1, false);
        assert_eq!(body.len(), RESPONSE_LEN);
        assert_eq!(body[0], ROP_OPEN_FOLDER);
        assert_eq!(body[1], 1, "the request's output index is echoed");
        assert_eq!(&body[2..6], &0u32.to_le_bytes());
        assert_eq!(body[6], 0, "HasRules");
        assert_eq!(body[7], 0, "IsGhosted — and so nothing follows it");

        let with_rules = success_body(2, true);
        assert_eq!(with_rules[6], 1);
        assert_eq!(with_rules.len(), RESPONSE_LEN, "rules do not add fields");
    }
}
