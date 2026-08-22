//! `RopRelease` ([MS-OXCROPS] §2.2.15.3) — the client saying it is finished
//! with a server object.
//!
//! Request, 3 bytes:
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | `0x01` |
//! | `LogonId` | 1 | |
//! | `InputHandleIndex` | 1 | the object being released |
//!
//! **There is no response.** The specification defines a request buffer and no
//! success or failure buffer, so a `RopRelease` in a list contributes nothing
//! to the output — not even an acknowledgement. Emitting one would misalign
//! every response after it, which is the kind of fault that shows up as a
//! client rendering the wrong thing rather than as an error.
//!
//! ## Why releasing matters here
//!
//! A client opens a message, reads it, and releases the handle before opening
//! the next. A session that never released would accumulate one object per
//! message a person clicked, for as long as the session lived — a slow leak
//! whose size is set by how much mail somebody reads. So this is not an
//! optional courtesy to implement later; it is what keeps the object table the
//! size of what is actually open.
//!
//! Handles are **not reused** after release. The next object gets a fresh
//! number, so a stale handle from a client that released twice, or released
//! and then used, names nothing rather than naming whatever took its place.

use crate::rop::RopError;

/// The `RopId` of `RopRelease`.
pub const ROP_RELEASE: u8 = 0x01;

/// The fixed size of this request.
pub const REQUEST_LEN: usize = 3;

/// A parsed `RopRelease` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReleaseRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the object to release.
    pub input_handle_index: u8,
}

impl ReleaseRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if fewer than [`REQUEST_LEN`] bytes remain, or
    /// the leading byte is not this operation.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let fixed = input
            .get(..REQUEST_LEN)
            .ok_or(RopError::Truncated { part: "RopRelease" })?;
        if fixed[0] != ROP_RELEASE {
            return Err(RopError::Truncated { part: "RopRelease" });
        }
        Ok((
            Self {
                logon_id: fixed[1],
                input_handle_index: fixed[2],
            },
            &input[REQUEST_LEN..],
        ))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{REQUEST_LEN, ROP_RELEASE, ReleaseRequest};

    #[test]
    fn a_request_is_three_bytes_and_leaves_the_rest() {
        let bytes = [ROP_RELEASE, 0x00, 0x02, 0xAA];
        let (request, tail) = ReleaseRequest::parse(&bytes).expect("parses");
        assert_eq!(request.logon_id, 0x00);
        assert_eq!(request.input_handle_index, 0x02);
        assert_eq!(tail, &[0xAA]);
        assert_eq!(REQUEST_LEN, 3);
    }

    #[test]
    fn a_truncated_request_is_refused() {
        assert!(ReleaseRequest::parse(&[ROP_RELEASE, 0x00]).is_err());
    }

    #[test]
    fn another_operations_bytes_are_not_this_one() {
        assert!(ReleaseRequest::parse(&[0x02, 0x00, 0x02]).is_err());
    }
}
