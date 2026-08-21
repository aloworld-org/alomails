//! `RopLogon` ([MS-OXCROPS] §2.2.3.1, [MS-OXCSTOR] §2.2.1.1) — the operation
//! that opens a mailbox, and the prerequisite for every other ROP.
//!
//! Request layout ([MS-OXCROPS] §2.2.3.1.1):
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | `0xFE` |
//! | `LogonId` | 1 | the id the client wants this logon known by |
//! | `OutputHandleIndex` | 1 | where to store the resulting handle |
//! | `LogonFlags` | 1 | `Private` `0x01`, and three the server ignores |
//! | `OpenFlags` | 4 | |
//! | `StoreState` | 4 | unused, MUST be `0x00000000` |
//! | `EssdnSize` | 2 | includes the terminating NUL |
//! | `Essdn` | `EssdnSize` | null-terminated ASCII |
//!
//! **The third byte is an *output* index here.** Most ROPs spell it
//! `InputHandleIndex` — the object they act on — but a logon has no input
//! object: it *creates* one, and this names the handle-table slot to put it in.
//! [`crate::rop::RopHeader`] reads that byte generically, so this module names
//! it correctly rather than inheriting a word that would mislead.
//!
//! **`Essdn` is a claim, not a credential.** It is the legacy distinguished
//! name of the mailbox the client wants, and it arrives from a client that has
//! already authenticated as somebody. Which mailbox that somebody may open is
//! decided against the Session Context, never against this string — a logon
//! that trusted `Essdn` would let any authenticated user name any mailbox.

use crate::rop::RopError;

/// The `RopId` of `RopLogon` ([MS-OXCROPS] §2.2.3.1.1).
pub const ROP_LOGON: u8 = 0xFE;

/// `LogonFlags` — logon to a private mailbox rather than public folders.
pub const LOGON_PRIVATE: u8 = 0x01;

/// `OpenFlags` — a request for administrative access to the mailbox.
pub const OPEN_USE_ADMIN_PRIVILEGE: u32 = 0x0000_0001;
/// `OpenFlags` — a request to open the public folders store.
pub const OPEN_PUBLIC: u32 = 0x0000_0002;

/// The longest `Essdn` we will read. Distinguished names are bounded in
/// practice; the field's own 16-bit length is not a reason to accept 64KiB of
/// it per logon.
pub const MAX_ESSDN: usize = 1024;

/// A parsed `RopLogon` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogonRequest {
    /// The id the client wants this logon known by. Reusing an active id
    /// releases the logon it names and replaces it.
    pub logon_id: u8,
    /// The handle-table slot the new logon's handle goes into.
    pub output_handle_index: u8,
    /// Flags controlling the logon.
    pub logon_flags: u8,
    /// Further flags controlling the logon.
    pub open_flags: u32,
    /// The mailbox the client is asking for, without its terminating NUL.
    ///
    /// A claim to be checked, never an authority in itself.
    pub essdn: String,
}

impl LogonRequest {
    /// Whether this is a private-mailbox logon rather than public folders.
    #[must_use]
    pub const fn is_private(&self) -> bool {
        self.logon_flags & LOGON_PRIVATE != 0
    }

    /// Whether the client asked for administrative access.
    ///
    /// Worth reading even though we do not grant it: a logon that quietly
    /// ignored the request would leave a client believing it has privileges it
    /// does not have, and acting accordingly.
    #[must_use]
    pub const fn wants_admin(&self) -> bool {
        self.open_flags & OPEN_USE_ADMIN_PRIVILEGE != 0
    }

    /// Whether the client asked for the public folders store.
    #[must_use]
    pub const fn wants_public(&self) -> bool {
        self.open_flags & OPEN_PUBLIC != 0
    }

    /// Parses a `RopLogon` request, including its three-byte common header.
    ///
    /// Returns the request and the bytes that follow it, so a dispatcher can
    /// continue along the ROP list — only an operation that knows its own
    /// layout can say where the next one starts.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if the buffer ends inside a field, and
    /// [`RopError::RopSize`] if `EssdnSize` is longer than the bytes that
    /// arrived or longer than [`MAX_ESSDN`].
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let fixed = input
            .get(..14)
            .ok_or(RopError::Truncated { part: "RopLogon" })?;
        if fixed[0] != ROP_LOGON {
            return Err(RopError::Truncated { part: "RopLogon" });
        }
        let logon_id = fixed[1];
        let output_handle_index = fixed[2];
        let logon_flags = fixed[3];
        let open_flags = u32::from_le_bytes([fixed[4], fixed[5], fixed[6], fixed[7]]);
        // StoreState (bytes 8..12) is specified as unused and MUST be zero. It
        // is read past rather than checked: refusing a client that set a field
        // the server is told to ignore would be strictness with nothing behind
        // it.
        let essdn_size = usize::from(u16::from_le_bytes([fixed[12], fixed[13]]));

        if essdn_size > MAX_ESSDN {
            return Err(RopError::RopSize {
                size: u16::try_from(essdn_size).unwrap_or(u16::MAX),
                available: input.len(),
            });
        }
        let raw = input
            .get(14..14 + essdn_size)
            .ok_or(RopError::Truncated { part: "Essdn" })?;

        // The size includes the terminating NUL, so the name is everything
        // before the first one. A size of zero is a legal empty Essdn.
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        let essdn = raw[..end]
            .iter()
            .map(|&b| if b.is_ascii() { char::from(b) } else { '?' })
            .collect();

        Ok((
            Self {
                logon_id,
                output_handle_index,
                logon_flags,
                open_flags,
                essdn,
            },
            &input[14 + essdn_size..],
        ))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// A request laid out exactly as [MS-OXCROPS] §2.2.3.1.1 orders the fields.
    fn request(essdn: &str, logon_flags: u8, open_flags: u32) -> Vec<u8> {
        let mut out = vec![ROP_LOGON, 0x00, 0x00, logon_flags];
        out.extend_from_slice(&open_flags.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // StoreState — MUST be 0.
        let mut name = essdn.as_bytes().to_vec();
        name.push(0); // the size includes the terminating NUL
        out.extend_from_slice(&u16::try_from(name.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&name);
        out
    }

    #[test]
    fn a_private_logon_reads_back_field_for_field() {
        let dn = "/o=alo/ou=exchange/cn=recipients/cn=disan";
        let raw = request(dn, LOGON_PRIVATE, 0x0100_0000);
        let (logon, rest) = LogonRequest::parse(&raw).unwrap();

        assert_eq!(logon.logon_id, 0);
        assert_eq!(logon.output_handle_index, 0);
        assert_eq!(logon.essdn, dn, "the NUL is not part of the name");
        assert!(logon.is_private());
        assert!(!logon.wants_public());
        assert!(!logon.wants_admin());
        assert!(rest.is_empty(), "consumed exactly its own bytes");
    }

    /// The parser returns what follows, so a dispatcher can walk the list. Only
    /// an operation that knows its own layout can say where the next begins.
    #[test]
    fn the_remainder_is_handed_back_for_the_next_operation() {
        let mut raw = request("/o=alo/cn=x", LOGON_PRIVATE, 0);
        raw.extend_from_slice(&[0x01, 0x00, 0x00]); // a RopRelease behind it
        let (_, rest) = LogonRequest::parse(&raw).unwrap();
        assert_eq!(rest, &[0x01, 0x00, 0x00]);
    }

    /// An administrative request is read rather than ignored. We do not grant
    /// it, but a logon that silently dropped the flag would leave the client
    /// believing it has privileges it does not have.
    #[test]
    fn an_administrative_request_is_visible_to_the_caller() {
        let raw = request("/o=alo/cn=x", LOGON_PRIVATE, OPEN_USE_ADMIN_PRIVILEGE);
        let (logon, _) = LogonRequest::parse(&raw).unwrap();
        assert!(logon.wants_admin());
    }

    #[test]
    fn a_public_folder_logon_is_distinguishable_from_a_private_one() {
        let raw = request("/o=alo/cn=pf", 0, OPEN_PUBLIC);
        let (logon, _) = LogonRequest::parse(&raw).unwrap();
        assert!(!logon.is_private());
        assert!(logon.wants_public());
    }

    #[test]
    fn an_empty_essdn_is_legal() {
        let mut raw = vec![ROP_LOGON, 0x00, 0x00, LOGON_PRIVATE];
        raw.extend_from_slice(&0u32.to_le_bytes());
        raw.extend_from_slice(&0u32.to_le_bytes());
        raw.extend_from_slice(&0u16.to_le_bytes()); // EssdnSize of zero
        let (logon, rest) = LogonRequest::parse(&raw).unwrap();
        assert_eq!(logon.essdn, "");
        assert!(rest.is_empty());
    }

    /// `EssdnSize` is a client-declared length, so it is checked against the
    /// bytes that arrived and against a ceiling of our own — the field's 16-bit
    /// width is not a reason to accept 64KiB of distinguished name per logon.
    #[test]
    fn a_declared_essdn_longer_than_the_buffer_is_refused() {
        let mut lying = request("/o=alo/cn=x", LOGON_PRIVATE, 0);
        lying[12..14].copy_from_slice(&9_000u16.to_le_bytes());
        assert!(LogonRequest::parse(&lying).is_err());

        let mut huge = request("/o=alo/cn=x", LOGON_PRIVATE, 0);
        huge[12..14].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(matches!(
            LogonRequest::parse(&huge),
            Err(RopError::RopSize { .. })
        ));
    }

    #[test]
    fn every_truncation_is_an_error() {
        let full = request("/o=alo/cn=x", LOGON_PRIVATE, 0);
        for cut in 0..full.len() {
            assert!(
                LogonRequest::parse(&full[..cut]).is_err(),
                "accepted a request cut at {cut}"
            );
        }
    }

    /// A different operation is not a logon, however well-formed the rest looks.
    #[test]
    fn another_rop_id_is_not_a_logon() {
        let mut raw = request("/o=alo/cn=x", LOGON_PRIVATE, 0);
        raw[0] = 0x01; // RopRelease
        assert!(LogonRequest::parse(&raw).is_err());
    }
}
