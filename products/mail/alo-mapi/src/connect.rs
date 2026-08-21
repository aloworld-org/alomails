//! The `Connect` request type ([MS-OXCMAPIHTTP] §2.2.4.1) — the first thing
//! Outlook sends once Autodiscover has pointed it here.
//!
//! Both bodies are packed little-endian structures with variable-length string
//! fields, and the two string encodings differ: `UserDn` and `DnPrefix` are
//! null-terminated **ASCII**, while `DisplayName` is a null-terminated
//! **UTF-16LE** string. Mixing them up produces a body that decodes to
//! plausible-looking rubbish rather than an error, so each is parsed and
//! written by a named helper below.
//!
//! Request body (§2.2.4.1.1):
//!
//! | Field | Size |
//! |---|---|
//! | `UserDn` | variable, null-terminated ASCII |
//! | `Flags` | 4 |
//! | `DefaultCodePage` | 4 |
//! | `LcidSort` | 4 |
//! | `LcidString` | 4 |
//! | `AuxiliaryBufferSize` | 4 |
//! | `AuxiliaryBuffer` | `AuxiliaryBufferSize` |
//!
//! Success response body (§2.2.4.1.2):
//!
//! | Field | Size |
//! |---|---|
//! | `StatusCode` | 4, MUST be 0 |
//! | `ErrorCode` | 4 |
//! | `PollsMax` | 4 |
//! | `RetryCount` | 4 |
//! | `RetryDelay` | 4 |
//! | `DnPrefix` | variable, null-terminated ASCII |
//! | `DisplayName` | variable, null-terminated UTF-16LE |
//! | `AuxiliaryBufferSize` | 4 |
//! | `AuxiliaryBuffer` | `AuxiliaryBufferSize` |

/// The largest `Connect` body we will read.
///
/// The auxiliary buffer is caller-declared, so its length field is an
/// instruction from the network to allocate. Bounded here, and the bound is
/// checked against the bytes that actually arrived rather than trusted.
pub const MAX_CONNECT_BODY: usize = 128 * 1024;

/// A parsed `Connect` request body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectRequest {
    /// The distinguished name of the user asking to connect.
    pub user_dn: String,
    /// Connection flags (`ulFlags` in [MS-OXCRPC] §3.1.4.1).
    pub flags: u32,
    /// The code page requested for string properties.
    pub default_code_page: u32,
    /// The locale used for sorting.
    pub lcid_sort: u32,
    /// The locale used for everything but sorting.
    pub lcid_string: u32,
}

/// Why a `Connect` body could not be read. Every variant maps to
/// `InvalidRequestBody` on the wire — the distinction is for our logs, and the
/// client is told only that the body was invalid.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConnectError {
    /// The body ended in the middle of a field.
    #[error("body ended inside {field}")]
    Truncated {
        /// The field being read when the bytes ran out.
        field: &'static str,
    },
    /// A null-terminated string never terminated.
    #[error("unterminated string in {field}")]
    Unterminated {
        /// The field whose terminator is missing.
        field: &'static str,
    },
    /// The declared auxiliary buffer is longer than the bytes that arrived, or
    /// longer than we accept.
    #[error("auxiliary buffer of {declared} bytes is not present or not allowed")]
    AuxiliaryBuffer {
        /// The length the client claimed.
        declared: u64,
    },
}

/// Reads a null-terminated ASCII string, returning it and the rest.
///
/// Bytes above 0x7F are replaced rather than rejected: a DN is an identifier we
/// echo into logs and compare, never something we execute, and refusing the
/// whole connection over one high byte is a worse failure than a lossy name.
fn ascii_z<'a>(input: &'a [u8], field: &'static str) -> Result<(String, &'a [u8]), ConnectError> {
    let end = input
        .iter()
        .position(|&b| b == 0)
        .ok_or(ConnectError::Unterminated { field })?;
    let text = input[..end]
        .iter()
        .map(|&b| if b.is_ascii() { char::from(b) } else { '?' })
        .collect();
    Ok((text, &input[end + 1..]))
}

/// Reads a little-endian `u32`, returning it and the rest.
fn le_u32<'a>(input: &'a [u8], field: &'static str) -> Result<(u32, &'a [u8]), ConnectError> {
    let (head, rest) = input
        .split_at_checked(4)
        .ok_or(ConnectError::Truncated { field })?;
    let bytes: [u8; 4] = head
        .try_into()
        .map_err(|_| ConnectError::Truncated { field })?;
    Ok((u32::from_le_bytes(bytes), rest))
}

impl ConnectRequest {
    /// Parses a `Connect` request body ([MS-OXCMAPIHTTP] §2.2.4.1.1).
    ///
    /// # Errors
    /// [`ConnectError`] when the body is truncated, a string never terminates,
    /// or the declared auxiliary buffer is not actually present.
    pub fn parse(body: &[u8]) -> Result<Self, ConnectError> {
        if body.len() > MAX_CONNECT_BODY {
            return Err(ConnectError::AuxiliaryBuffer {
                declared: body.len() as u64,
            });
        }
        let (user_dn, rest) = ascii_z(body, "UserDn")?;
        let (flags, rest) = le_u32(rest, "Flags")?;
        let (default_code_page, rest) = le_u32(rest, "DefaultCodePage")?;
        let (lcid_sort, rest) = le_u32(rest, "LcidSort")?;
        let (lcid_string, rest) = le_u32(rest, "LcidString")?;
        let (aux_size, rest) = le_u32(rest, "AuxiliaryBufferSize")?;

        // The declared size must match the bytes that actually arrived. This is
        // the one field a hostile client controls independently of the payload,
        // and believing it is how a length field becomes an allocation bug.
        if u64::from(aux_size) != rest.len() as u64 {
            return Err(ConnectError::AuxiliaryBuffer {
                declared: u64::from(aux_size),
            });
        }

        Ok(Self {
            user_dn,
            flags,
            default_code_page,
            lcid_sort,
            lcid_string,
        })
    }
}

/// Builds a `Connect` success response body ([MS-OXCMAPIHTTP] §2.2.4.1.2).
///
/// `StatusCode` MUST be zero per the specification; `error_code` carries the
/// operation's own result. `polls_max`, `retry_count` and `retry_delay` are the
/// pacing the client will obey, so they are stated explicitly rather than
/// zeroed — a client told to retry zero times gives up on the first hiccup.
#[must_use]
pub fn success_body(
    error_code: u32,
    polls_max: u32,
    retry_count: u32,
    retry_delay: u32,
    dn_prefix: &str,
    display_name: &str,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + display_name.len() * 2);
    out.extend_from_slice(&0u32.to_le_bytes()); // StatusCode — MUST be 0.
    out.extend_from_slice(&error_code.to_le_bytes());
    out.extend_from_slice(&polls_max.to_le_bytes());
    out.extend_from_slice(&retry_count.to_le_bytes());
    out.extend_from_slice(&retry_delay.to_le_bytes());

    // DnPrefix: null-terminated ASCII.
    out.extend(dn_prefix.bytes().filter(u8::is_ascii));
    out.push(0);

    // DisplayName: null-terminated UTF-16LE — a different encoding to the
    // field directly above it, which is the trap this function exists to hide.
    for unit in display_name.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out.extend_from_slice(&[0, 0]);

    // We send no auxiliary payload, so the size is zero and the buffer empty.
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// A body assembled exactly as the specification lays the fields out.
    fn body(user_dn: &str, aux: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(user_dn.as_bytes());
        out.push(0);
        out.extend_from_slice(&0x0000_0001u32.to_le_bytes()); // Flags
        out.extend_from_slice(&65001u32.to_le_bytes()); // DefaultCodePage (UTF-8)
        out.extend_from_slice(&1033u32.to_le_bytes()); // LcidSort
        out.extend_from_slice(&2057u32.to_le_bytes()); // LcidString
        out.extend_from_slice(&(aux.len() as u32).to_le_bytes());
        out.extend_from_slice(aux);
        out
    }

    #[test]
    fn a_well_formed_connect_body_reads_back_field_for_field() {
        let raw = body("/o=alo/cn=disan", b"");
        let parsed = ConnectRequest::parse(&raw).expect("parsed");
        assert_eq!(
            parsed,
            ConnectRequest {
                user_dn: "/o=alo/cn=disan".to_owned(),
                flags: 1,
                default_code_page: 65001,
                lcid_sort: 1033,
                lcid_string: 2057,
            }
        );
    }

    #[test]
    fn an_auxiliary_buffer_is_accepted_when_it_is_really_there() {
        let raw = body("/o=alo/cn=x", &[1, 2, 3, 4, 5]);
        assert!(ConnectRequest::parse(&raw).is_ok());
    }

    /// The declared length is the one field a client controls independently of
    /// what it actually sent. Believing it is how a length field becomes an
    /// allocation bug, so a lie in either direction is refused.
    #[test]
    fn a_declared_auxiliary_size_that_does_not_match_the_bytes_is_refused() {
        // Claims five bytes of auxiliary payload, sends none.
        let mut lying = body("/o=alo/cn=x", b"");
        let len = lying.len();
        lying[len - 4..].copy_from_slice(&5u32.to_le_bytes());
        assert_eq!(
            ConnectRequest::parse(&lying),
            Err(ConnectError::AuxiliaryBuffer { declared: 5 })
        );

        // Claims none, sends five — equally a mismatch, and equally refused.
        let mut lying = body("/o=alo/cn=x", &[9, 9, 9, 9, 9]);
        let len = lying.len();
        lying[len - 9..len - 5].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            ConnectRequest::parse(&lying),
            Err(ConnectError::AuxiliaryBuffer { declared: 0 })
        );

        // And a size that would have us allocate four gigabytes.
        let mut huge = body("/o=alo/cn=x", b"");
        let len = huge.len();
        huge[len - 4..].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            ConnectRequest::parse(&huge),
            Err(ConnectError::AuxiliaryBuffer {
                declared: u64::from(u32::MAX)
            })
        );
    }

    #[test]
    fn a_truncated_body_names_the_field_it_died_in() {
        let full = body("/o=alo/cn=x", b"");
        // Every prefix that cuts a fixed field short is an error, never a
        // silently zero-filled struct.
        for cut in (12..full.len() - 1).step_by(3) {
            assert!(
                ConnectRequest::parse(&full[..cut]).is_err(),
                "accepted a body cut at {cut}"
            );
        }
        // A UserDn with no terminator at all.
        assert_eq!(
            ConnectRequest::parse(b"/o=alo/cn=never-ends"),
            Err(ConnectError::Unterminated { field: "UserDn" })
        );
    }

    #[test]
    fn an_empty_body_is_an_error_not_an_empty_connection() {
        assert!(ConnectRequest::parse(b"").is_err());
    }

    /// `DnPrefix` is ASCII and `DisplayName` is UTF-16LE, one after the other.
    /// Writing both the same way yields a body that decodes to plausible
    /// rubbish rather than failing, so the two encodings are asserted on bytes.
    #[test]
    fn the_success_body_uses_the_two_string_encodings_the_spec_asks_for() {
        let out = success_body(0, 60_000, 3, 1_000, "/o=alo", "Disan Ssebowa");

        // Five little-endian u32s first.
        assert_eq!(&out[0..4], &0u32.to_le_bytes(), "StatusCode MUST be zero");
        assert_eq!(&out[4..8], &0u32.to_le_bytes());
        assert_eq!(&out[8..12], &60_000u32.to_le_bytes());
        assert_eq!(&out[12..16], &3u32.to_le_bytes());
        assert_eq!(&out[16..20], &1_000u32.to_le_bytes());

        // DnPrefix: one byte per character, then a single NUL.
        let rest = &out[20..];
        assert!(rest.starts_with(b"/o=alo\0"), "{rest:?}");

        // DisplayName: two bytes per character, then a two-byte NUL. 'D' is
        // 0x44 0x00 in UTF-16LE — one byte per char here would be the bug.
        let name = &rest[b"/o=alo\0".len()..];
        assert_eq!(&name[0..2], &[0x44, 0x00], "DisplayName is not UTF-16LE");
        let expected: Vec<u8> = "Disan Ssebowa"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert!(name.starts_with(&expected));
        assert_eq!(
            &name[expected.len()..],
            &[0, 0, 0, 0, 0, 0],
            "terminator plus a zero AuxiliaryBufferSize"
        );
    }

    /// Non-ASCII names are the European case, not the edge case: the display
    /// name must survive UTF-16 intact even though the DN beside it cannot.
    #[test]
    fn a_display_name_carries_accents_intact() {
        let out = success_body(0, 0, 0, 0, "/o=alo", "Liège Müller");
        let expected: Vec<u8> = "Liège Müller"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert!(
            out.windows(expected.len()).any(|w| w == expected),
            "accented display name did not survive"
        );
    }
}
