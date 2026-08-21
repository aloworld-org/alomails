//! The `Execute` request type ([MS-OXCMAPIHTTP] §2.2.4.2) — the envelope that
//! carries ROP requests once a Session Context exists.
//!
//! Request body (§2.2.4.2.1):
//!
//! | Field | Size |
//! |---|---|
//! | `Flags` | 4 |
//! | `RopBufferSize` | 4 |
//! | `RopBuffer` | `RopBufferSize` |
//! | `MaxRopOut` | 4 |
//! | `AuxiliaryBufferSize` | 4 |
//! | `AuxiliaryBuffer` | `AuxiliaryBufferSize` |
//!
//! Success response body (§2.2.4.2.2):
//!
//! | Field | Size |
//! |---|---|
//! | `StatusCode` | 4, MUST be 0 |
//! | `ErrorCode` | 4 |
//! | `Flags` | 4, reserved, MUST be 0 |
//! | `RopBufferSize` | 4 |
//! | `RopBuffer` | `RopBufferSize` |
//! | `AuxiliaryBufferSize` | 4 |
//! | `AuxiliaryBuffer` | `AuxiliaryBufferSize` |
//!
//! `RopBuffer` is itself an extended buffer — see [`crate::rpc`] — so the ROP
//! payload is two layers down: this envelope, then the `RPC_HEADER_EXT` chain.
//!
//! **`MaxRopOut` is the client's ceiling on our answer, and it binds us.** A
//! response larger than the client asked for is not a large response, it is a
//! broken one: the client sized its buffer from that number.

/// The largest `Execute` body we will read, before the inner ROP buffer's own
/// ceiling applies.
pub const MAX_EXECUTE_BODY: usize = 8 * 1024 * 1024;

/// A parsed `Execute` request body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteRequest {
    /// How the server should build the ROP responses.
    pub flags: u32,
    /// The ROP request payload — still wrapped in its `RPC_HEADER_EXT` chain.
    pub rop_buffer: Vec<u8>,
    /// The largest ROP response buffer the client will accept.
    pub max_rop_out: u32,
}

/// Why an `Execute` body could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ExecuteError {
    /// The body ended in the middle of a field.
    #[error("body ended inside {field}")]
    Truncated {
        /// The field being read when the bytes ran out.
        field: &'static str,
    },
    /// A declared length does not match the bytes that arrived.
    #[error("{field} declares {declared} bytes that are not present")]
    Length {
        /// The field whose length was wrong.
        field: &'static str,
        /// The length the client claimed.
        declared: u64,
    },
}

/// Reads a little-endian `u32`, returning it and the rest.
fn le_u32<'a>(input: &'a [u8], field: &'static str) -> Result<(u32, &'a [u8]), ExecuteError> {
    let (head, rest) = input
        .split_at_checked(4)
        .ok_or(ExecuteError::Truncated { field })?;
    let bytes: [u8; 4] = head
        .try_into()
        .map_err(|_| ExecuteError::Truncated { field })?;
    Ok((u32::from_le_bytes(bytes), rest))
}

/// Takes a length-prefixed run of bytes whose length was already read.
///
/// The declared length is checked against what actually arrived rather than
/// used to allocate — the same rule as everywhere else a client tells us a size.
fn take<'a>(
    input: &'a [u8],
    len: u32,
    field: &'static str,
) -> Result<(&'a [u8], &'a [u8]), ExecuteError> {
    let len = usize::try_from(len).map_err(|_| ExecuteError::Length {
        field,
        declared: u64::from(len),
    })?;
    input.split_at_checked(len).ok_or(ExecuteError::Length {
        field,
        declared: len as u64,
    })
}

impl ExecuteRequest {
    /// Parses an `Execute` request body ([MS-OXCMAPIHTTP] §2.2.4.2.1).
    ///
    /// # Errors
    /// [`ExecuteError`] when the body is truncated or a declared length is not
    /// backed by the bytes that arrived.
    pub fn parse(body: &[u8]) -> Result<Self, ExecuteError> {
        if body.len() > MAX_EXECUTE_BODY {
            return Err(ExecuteError::Length {
                field: "body",
                declared: body.len() as u64,
            });
        }
        let (flags, rest) = le_u32(body, "Flags")?;
        let (rop_size, rest) = le_u32(rest, "RopBufferSize")?;
        let (rop_buffer, rest) = take(rest, rop_size, "RopBuffer")?;
        let (max_rop_out, rest) = le_u32(rest, "MaxRopOut")?;
        let (aux_size, rest) = le_u32(rest, "AuxiliaryBufferSize")?;
        let (_aux, rest) = take(rest, aux_size, "AuxiliaryBuffer")?;

        // Trailing bytes after every declared field mean the client and we
        // disagree about the shape of the body. Ignoring them would be reading
        // a message we do not actually understand.
        if !rest.is_empty() {
            return Err(ExecuteError::Length {
                field: "body",
                declared: rest.len() as u64,
            });
        }

        Ok(Self {
            flags,
            rop_buffer: rop_buffer.to_vec(),
            max_rop_out,
        })
    }
}

/// Builds an `Execute` success response body ([MS-OXCMAPIHTTP] §2.2.4.2.2).
///
/// `rop_buffer` is the already-framed extended buffer, not a bare ROP payload.
#[must_use]
pub fn success_body(error_code: u32, rop_buffer: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(20 + rop_buffer.len());
    out.extend_from_slice(&0u32.to_le_bytes()); // StatusCode — MUST be 0.
    out.extend_from_slice(&error_code.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // Flags — reserved, MUST be 0.
    // A cast that cannot lose: the caller's buffer is bounded well below 4GiB
    // by `crate::rpc::MAX_ROP_BUFFER`, and a saturating conversion here would
    // announce a length that does not match the bytes we then write.
    let size = u32::try_from(rop_buffer.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(rop_buffer);
    out.extend_from_slice(&0u32.to_le_bytes()); // AuxiliaryBufferSize.
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// A body laid out exactly as the specification orders the fields.
    fn body(rop: &[u8], aux: &[u8], max_rop_out: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&0u32.to_le_bytes()); // Flags
        out.extend_from_slice(&(rop.len() as u32).to_le_bytes());
        out.extend_from_slice(rop);
        out.extend_from_slice(&max_rop_out.to_le_bytes());
        out.extend_from_slice(&(aux.len() as u32).to_le_bytes());
        out.extend_from_slice(aux);
        out
    }

    #[test]
    fn a_well_formed_execute_body_reads_back_field_for_field() {
        let raw = body(b"rop-bytes", b"aux", 32_768);
        let parsed = ExecuteRequest::parse(&raw).unwrap();
        assert_eq!(parsed.flags, 0);
        assert_eq!(parsed.rop_buffer, b"rop-bytes");
        assert_eq!(parsed.max_rop_out, 32_768);
    }

    #[test]
    fn an_empty_rop_buffer_is_legal() {
        let raw = body(b"", b"", 8_192);
        let parsed = ExecuteRequest::parse(&raw).unwrap();
        assert!(parsed.rop_buffer.is_empty());
        assert_eq!(parsed.max_rop_out, 8_192);
    }

    /// The two length fields are what a hostile client controls independently
    /// of the payload. Both are checked against the bytes that arrived.
    #[test]
    fn a_declared_length_longer_than_the_body_is_refused() {
        let mut lying = body(b"rop", b"", 1024);
        lying[4..8].copy_from_slice(&9_000u32.to_le_bytes());
        assert!(matches!(
            ExecuteRequest::parse(&lying),
            Err(ExecuteError::Length {
                field: "RopBuffer",
                ..
            })
        ));

        // ...and the auxiliary length, which sits at the very end where a
        // short read is easiest to miss.
        let mut lying = body(b"rop", b"", 1024);
        let len = lying.len();
        lying[len - 4..].copy_from_slice(&500u32.to_le_bytes());
        assert!(matches!(
            ExecuteRequest::parse(&lying),
            Err(ExecuteError::Length {
                field: "AuxiliaryBuffer",
                ..
            })
        ));
    }

    /// Bytes after the last declared field mean we and the client disagree
    /// about the body's shape. Ignoring them would be acting on a message we do
    /// not actually understand.
    #[test]
    fn trailing_bytes_are_a_disagreement_not_padding() {
        let mut raw = body(b"rop", b"", 1024);
        raw.extend_from_slice(b"extra");
        assert!(matches!(
            ExecuteRequest::parse(&raw),
            Err(ExecuteError::Length { field: "body", .. })
        ));
    }

    #[test]
    fn every_truncation_is_an_error_never_a_short_read() {
        let full = body(b"rop-bytes", b"aux", 1024);
        for cut in 0..full.len() {
            assert!(
                ExecuteRequest::parse(&full[..cut]).is_err(),
                "accepted a body cut at {cut}"
            );
        }
    }

    /// The response's declared `RopBufferSize` must match the bytes that
    /// follow it, or the client reads the auxiliary size out of our ROP data.
    #[test]
    fn the_success_body_declares_the_length_it_actually_writes() {
        let rop = b"framed-rop-response";
        let out = success_body(0, rop);

        assert_eq!(&out[0..4], &0u32.to_le_bytes(), "StatusCode MUST be zero");
        assert_eq!(&out[4..8], &0u32.to_le_bytes(), "ErrorCode");
        assert_eq!(
            &out[8..12],
            &0u32.to_le_bytes(),
            "Flags reserved, MUST be 0"
        );

        let declared = u32::from_le_bytes(out[12..16].try_into().unwrap()) as usize;
        assert_eq!(declared, rop.len(), "declared a length it did not write");
        assert_eq!(&out[16..16 + declared], rop);
        // ...and the auxiliary size sits exactly where that length says it does.
        assert_eq!(&out[16 + declared..], &0u32.to_le_bytes());
    }

    /// An error is reported in `ErrorCode` with `StatusCode` still zero: the
    /// transport succeeded, the operation did not, and conflating the two
    /// makes a failed ROP look like a broken connection.
    #[test]
    fn an_operation_error_leaves_the_transport_status_at_zero() {
        let out = success_body(0x8004_0115, b"");
        assert_eq!(&out[0..4], &0u32.to_le_bytes());
        assert_eq!(&out[4..8], &0x8004_0115u32.to_le_bytes());
    }
}
