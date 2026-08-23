//! `RopSynchronizationConfigure` ([MS-OXCROPS] §2.2.13.1, [MS-OXCFXICS]
//! §2.2.3.2.1.1) — the operation that opens a synchronisation download context.
//!
//! This is the door to cached mode. A client names a folder, says whether it
//! wants that folder's *contents* or the folder *hierarchy* beneath it, and
//! declares what it can understand; the server answers with a handle that later
//! `RopFastTransferSourceGetBuffer` calls draw a FastTransfer stream from.
//!
//! ## Request
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | `0x70` |
//! | `LogonId` | 1 | |
//! | `InputHandleIndex` | 1 | the folder |
//! | `OutputHandleIndex` | 1 | where the new context's handle goes |
//! | `SynchronizationType` | 1 | contents or hierarchy |
//! | `SendOptions` | 1 | what representations the client accepts |
//! | `SynchronizationFlags` | 2 | what to include |
//! | `RestrictionDataSize` | 2 | |
//! | `RestrictionData` | variable | contents only |
//! | `SynchronizationExtraFlags` | 4 | which header properties to include |
//! | `PropertyTagCount` | 2 | |
//! | `PropertyTags` | 4 × count | included or excluded, per `OnlySpecifiedProperties` |
//!
//! Success response, 6 bytes: `RopId`, `OutputHandleIndex`, `ReturnValue`. The
//! download context itself is the output server object, which lives in the
//! handle table rather than in the response bytes ([MS-OXCFXICS] §2.2.3.2.1.1.2).
//!
//! **On the prologue.** [MS-OXCROPS] §2.2.13.1 defers the field table to
//! [MS-OXCFXICS], which defers it back, and neither renders it. The four-byte
//! prologue above is the one every object-creating ROP in this crate already
//! uses — `RopOpenFolder`, `RopCreateMessage`, `RopGetContentsTable` — so it is
//! taken from the shape the protocol is consistent about rather than guessed.
//!
//! ## What the flags actually decide
//!
//! `SynchronizationFlags` is where a client says whether it wants deletions,
//! read-state changes, FAI messages, and normal messages at all. A server that
//! ignored them would send a correct stream describing more than the client
//! asked for, which is not a protocol error and is still wrong: the client
//! stores what it is told.

use crate::columns::PropertyTag;
use crate::rop::RopError;

/// The `RopId` of `RopSynchronizationConfigure`.
pub const ROP_SYNCHRONIZATION_CONFIGURE: u8 = 0x70;

/// The bytes before `RestrictionData`.
const FIXED_PREFIX_LEN: usize = 10;

/// The size of a success response.
pub const RESPONSE_LEN: usize = 6;

/// The most property tags we will accept in one configure.
///
/// Client-declared and four bytes each; bounding it bounds the allocation and
/// the per-message filtering every later item in the stream pays for.
pub const MAX_PROPERTY_TAGS: usize = 512;

/// The largest restriction we will accept.
///
/// A restriction narrows a content synchronisation. It is client-supplied and
/// parsed later, so its size is bounded before any of it is read.
pub const MAX_RESTRICTION_LEN: usize = 64 * 1024;

/// What is being synchronised ([MS-OXCFXICS] §2.2.3.2.1.1.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncType {
    /// The messages in one folder.
    Contents,
    /// The folders beneath one folder.
    Hierarchy,
}

impl SyncType {
    /// The value on the wire.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Contents => 0x01,
            Self::Hierarchy => 0x02,
        }
    }

    /// Reads the wire value, refusing anything the specification does not name.
    fn parse(value: u8) -> Result<Self, RopError> {
        match value {
            0x01 => Ok(Self::Contents),
            0x02 => Ok(Self::Hierarchy),
            _ => Err(RopError::Truncated {
                part: "RopSynchronizationConfigure SynchronizationType",
            }),
        }
    }
}

/// `SynchronizationFlags` ([MS-OXCFXICS] §2.2.3.2.1.1.1).
pub mod sync_flags {
    /// Strings are Unicode. Must agree with `SendOptions`.
    pub const UNICODE: u16 = 0x0001;
    /// Do not send information about deletions.
    pub const NO_DELETIONS: u16 = 0x0002;
    /// Do not report messages that left the synchronisation scope.
    pub const IGNORE_NO_LONGER_IN_SCOPE: u16 = 0x0004;
    /// Include changes to read state.
    pub const READ_STATE: u16 = 0x0008;
    /// Include folder-associated messages.
    pub const FAI: u16 = 0x0010;
    /// Include ordinary messages.
    pub const NORMAL: u16 = 0x0020;
    /// `PropertyTags` lists what to include rather than what to leave out.
    pub const ONLY_SPECIFIED_PROPERTIES: u16 = 0x0080;
    /// Ignore stored source keys when producing changes.
    pub const NO_FOREIGN_IDENTIFIERS: u16 = 0x0100;
    /// Must be zero when sent.
    pub const RESERVED: u16 = 0x1000;
    /// Send bodies in their original format rather than RTF.
    pub const BEST_BODY: u16 = 0x2000;
    /// Do not apply the property filter to FAI messages.
    pub const IGNORE_SPECIFIED_ON_FAI: u16 = 0x4000;
    /// Inject progress information into the stream.
    pub const PROGRESS: u16 = 0x8000;
}

/// `SynchronizationExtraFlags` ([MS-OXCFXICS] §2.2.3.2.1.1.1).
pub mod extra_flags {
    /// Include `PidTagFolderId` or `PidTagMid` in the change header.
    pub const EID: u32 = 0x0000_0001;
    /// Include `PidTagMessageSize` in the message change header.
    pub const MESSAGE_SIZE: u32 = 0x0000_0002;
    /// Include `PidTagChangeNumber` in the message change header.
    pub const CN: u32 = 0x0000_0004;
    /// Order messages by delivery time.
    pub const ORDER_BY_DELIVERY_TIME: u32 = 0x0000_0008;
}

/// `SendOptions` ([MS-OXCFXICS] §2.2.3.1.1.1.1).
pub mod send_options {
    /// Strings are output as Unicode.
    pub const UNICODE: u8 = 0x01;
    /// The client understands code-page property types.
    pub const USE_CPID: u8 = 0x02;
    /// The stream is destined for another server, not for local storage.
    pub const FOR_UPLOAD: u8 = 0x03;
    /// The client supports recovery mode.
    pub const RECOVER_MODE: u8 = 0x04;
    /// Strings are output as Unicode regardless of the connection code page.
    pub const FORCE_UNICODE: u8 = 0x08;
    /// The client supports partial item downloads.
    pub const PARTIAL_ITEM: u8 = 0x10;
}

/// A parsed `RopSynchronizationConfigure` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncConfigureRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the folder.
    pub input_handle_index: u8,
    /// The handle-table slot the download context's handle goes into.
    pub output_handle_index: u8,
    /// Contents or hierarchy.
    pub sync_type: SyncType,
    /// What representations the client accepts.
    pub send_options: u8,
    /// What to include.
    pub sync_flags: u16,
    /// The restriction narrowing a content synchronisation, if any.
    pub restriction: Vec<u8>,
    /// Which header properties to include.
    pub extra_flags: u32,
    /// Properties to include or exclude, per `ONLY_SPECIFIED_PROPERTIES`.
    pub property_tags: Vec<PropertyTag>,
}

impl SyncConfigureRequest {
    /// Whether the client asked for ordinary messages.
    #[must_use]
    pub fn wants_normal(&self) -> bool {
        self.sync_flags & sync_flags::NORMAL != 0
    }

    /// Whether the client asked for folder-associated messages.
    #[must_use]
    pub fn wants_fai(&self) -> bool {
        self.sync_flags & sync_flags::FAI != 0
    }

    /// Whether the client wants to hear about deletions.
    ///
    /// Note the inversion: the flag names its *absence*, so a client that sets
    /// nothing gets deletions.
    #[must_use]
    pub fn wants_deletions(&self) -> bool {
        self.sync_flags & sync_flags::NO_DELETIONS == 0
    }

    /// Whether the client wants read-state changes.
    #[must_use]
    pub fn wants_read_state(&self) -> bool {
        self.sync_flags & sync_flags::READ_STATE != 0
    }

    /// Whether the client wants progress information in the stream.
    #[must_use]
    pub fn wants_progress(&self) -> bool {
        self.sync_flags & sync_flags::PROGRESS != 0
    }

    /// Whether `PropertyTags` is an include list rather than an exclude list.
    #[must_use]
    pub fn only_specified_properties(&self) -> bool {
        self.sync_flags & sync_flags::ONLY_SPECIFIED_PROPERTIES != 0
    }

    /// Whether the change header should carry the item's id.
    #[must_use]
    pub fn wants_eid(&self) -> bool {
        self.extra_flags & extra_flags::EID != 0
    }

    /// Whether the change header should carry the change number.
    #[must_use]
    pub fn wants_change_number(&self) -> bool {
        self.extra_flags & extra_flags::CN != 0
    }

    /// Whether the change header should carry the message size.
    #[must_use]
    pub fn wants_message_size(&self) -> bool {
        self.extra_flags & extra_flags::MESSAGE_SIZE != 0
    }

    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    ///
    /// [`RopError::Truncated`] if the buffer ends inside a field, if the
    /// operation byte is not this one, if `SynchronizationType` is not a value
    /// the specification names, or if a client-declared length exceeds the
    /// bounds above.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let fixed = input.get(..FIXED_PREFIX_LEN).ok_or(RopError::Truncated {
            part: "RopSynchronizationConfigure",
        })?;
        if fixed[0] != ROP_SYNCHRONIZATION_CONFIGURE {
            return Err(RopError::Truncated {
                part: "RopSynchronizationConfigure",
            });
        }

        let sync_type = SyncType::parse(fixed[4])?;
        let send_options = fixed[5];
        let sync_flags = u16::from_le_bytes([fixed[6], fixed[7]]);
        let restriction_len = usize::from(u16::from_le_bytes([fixed[8], fixed[9]]));

        if restriction_len > MAX_RESTRICTION_LEN {
            return Err(RopError::Truncated {
                part: "RopSynchronizationConfigure RestrictionData",
            });
        }
        // §2.2.3.2.1.1.1: for a hierarchy synchronisation the size "MUST be set
        // to 0x0000". A restriction there would narrow nothing and says the
        // client and server disagree about what is being synchronised, so it is
        // refused rather than ignored.
        if matches!(sync_type, SyncType::Hierarchy) && restriction_len != 0 {
            return Err(RopError::Truncated {
                part: "RopSynchronizationConfigure hierarchy restriction",
            });
        }

        let mut at = FIXED_PREFIX_LEN;
        let restriction = input
            .get(at..at + restriction_len)
            .ok_or(RopError::Truncated {
                part: "RopSynchronizationConfigure RestrictionData",
            })?
            .to_vec();
        at += restriction_len;

        let tail = input.get(at..at + 6).ok_or(RopError::Truncated {
            part: "RopSynchronizationConfigure SynchronizationExtraFlags",
        })?;
        let extra_flags = u32::from_le_bytes([tail[0], tail[1], tail[2], tail[3]]);
        let tag_count = usize::from(u16::from_le_bytes([tail[4], tail[5]]));
        at += 6;

        if tag_count > MAX_PROPERTY_TAGS {
            return Err(RopError::Truncated {
                part: "RopSynchronizationConfigure PropertyTagCount",
            });
        }

        let tag_bytes = tag_count.checked_mul(4).ok_or(RopError::Truncated {
            part: "RopSynchronizationConfigure PropertyTags",
        })?;
        let raw = input.get(at..at + tag_bytes).ok_or(RopError::Truncated {
            part: "RopSynchronizationConfigure PropertyTags",
        })?;
        at += tag_bytes;

        let property_tags = raw
            .chunks_exact(4)
            .map(|tag| PropertyTag {
                property_type: u16::from_le_bytes([tag[0], tag[1]]),
                property_id: u16::from_le_bytes([tag[2], tag[3]]),
            })
            .collect();

        Ok((
            Self {
                logon_id: fixed[1],
                input_handle_index: fixed[2],
                output_handle_index: fixed[3],
                sync_type,
                send_options,
                sync_flags,
                restriction,
                extra_flags,
                property_tags,
            },
            &input[at..],
        ))
    }

    /// Whether the client's two Unicode declarations agree.
    ///
    /// §2.2.3.2.1.1.1 requires the `Unicode` flag of `SynchronizationFlags` to
    /// "match the value of the Unicode flag from the `SendOptions` field". They
    /// are two statements of one fact, and a client that disagrees with itself
    /// has told us nothing reliable about what it can read — so the caller can
    /// refuse rather than pick one.
    #[must_use]
    pub fn unicode_is_consistent(&self) -> bool {
        let by_flags = self.sync_flags & sync_flags::UNICODE != 0;
        let by_options = self.send_options & send_options::UNICODE != 0;
        by_flags == by_options
    }
}

/// Builds the `RopSynchronizationConfigure` success response.
///
/// The download context is the output server object and lives in the handle
/// table; nothing about it appears in these bytes.
#[must_use]
pub fn configure_success_body(output_handle_index: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(RESPONSE_LEN);
    out.push(ROP_SYNCHRONIZATION_CONFIGURE);
    out.push(output_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Builds a request the way a client would.
    fn request(
        sync_type: u8,
        sync_flags: u16,
        restriction: &[u8],
        extra: u32,
        tags: &[(u16, u16)],
    ) -> Vec<u8> {
        let mut out = vec![ROP_SYNCHRONIZATION_CONFIGURE, 0x00, 0x01, 0x02];
        out.push(sync_type);
        out.push(send_options::UNICODE);
        out.extend_from_slice(&sync_flags.to_le_bytes());
        out.extend_from_slice(&u16::try_from(restriction.len()).unwrap().to_le_bytes());
        out.extend_from_slice(restriction);
        out.extend_from_slice(&extra.to_le_bytes());
        out.extend_from_slice(&u16::try_from(tags.len()).unwrap().to_le_bytes());
        for (property_type, property_id) in tags {
            out.extend_from_slice(&property_type.to_le_bytes());
            out.extend_from_slice(&property_id.to_le_bytes());
        }
        out
    }

    #[test]
    fn a_contents_configure_parses_every_field() {
        let flags = sync_flags::UNICODE | sync_flags::NORMAL | sync_flags::READ_STATE;
        let buf = request(
            0x01,
            flags,
            &[0x02, 0x00],
            extra_flags::EID | extra_flags::CN,
            &[(0x001F, 0x0037)],
        );

        let (parsed, rest) = SyncConfigureRequest::parse(&buf).unwrap();
        assert!(rest.is_empty(), "left {} bytes unread", rest.len());
        assert_eq!(parsed.logon_id, 0x00);
        assert_eq!(parsed.input_handle_index, 0x01);
        assert_eq!(parsed.output_handle_index, 0x02);
        assert_eq!(parsed.sync_type, SyncType::Contents);
        assert_eq!(parsed.restriction, vec![0x02, 0x00]);
        assert_eq!(parsed.property_tags.len(), 1);
        assert_eq!(parsed.property_tags[0].property_id, 0x0037);
        assert!(parsed.wants_normal());
        assert!(parsed.wants_read_state());
        assert!(parsed.wants_eid());
        assert!(parsed.wants_change_number());
        assert!(!parsed.wants_message_size());
        assert!(parsed.unicode_is_consistent());
    }

    /// The absence of a flag is what asks for deletions, so a client that sets
    /// nothing must still be told about them.
    #[test]
    fn deletions_are_wanted_unless_the_client_opts_out() {
        let (with, _) = SyncConfigureRequest::parse(&request(0x01, 0, &[], 0, &[])).unwrap();
        assert!(with.wants_deletions(), "a bare request lost its deletions");

        let buf = request(0x01, sync_flags::NO_DELETIONS, &[], 0, &[]);
        let (without, _) = SyncConfigureRequest::parse(&buf).unwrap();
        assert!(!without.wants_deletions());
    }

    /// A hierarchy synchronisation carries no restriction — the specification
    /// makes the size zero, and a non-zero one means the two ends disagree
    /// about what is being synchronised.
    #[test]
    fn a_hierarchy_configure_refuses_a_restriction() {
        let buf = request(0x02, sync_flags::UNICODE, &[0x02, 0x00], 0, &[]);
        assert!(SyncConfigureRequest::parse(&buf).is_err());

        let clean = request(0x02, sync_flags::UNICODE, &[], 0, &[]);
        let (parsed, _) = SyncConfigureRequest::parse(&clean).unwrap();
        assert_eq!(parsed.sync_type, SyncType::Hierarchy);
    }

    /// A client that contradicts itself about Unicode is detectable, so the
    /// caller can refuse rather than pick one of the two answers.
    #[test]
    fn contradictory_unicode_declarations_are_visible() {
        // SynchronizationFlags says Unicode, SendOptions (set by the helper)
        // also does — consistent.
        let (ok, _) =
            SyncConfigureRequest::parse(&request(0x01, sync_flags::UNICODE, &[], 0, &[])).unwrap();
        assert!(ok.unicode_is_consistent());

        // Now drop the flag but leave SendOptions claiming Unicode.
        let (bad, _) = SyncConfigureRequest::parse(&request(0x01, 0, &[], 0, &[])).unwrap();
        assert!(!bad.unicode_is_consistent());
    }

    /// Unknown synchronisation types are refused, not treated as contents.
    #[test]
    fn an_unknown_synchronization_type_is_refused() {
        for kind in [0x00, 0x03, 0xFF] {
            let buf = request(kind, sync_flags::UNICODE, &[], 0, &[]);
            assert!(
                SyncConfigureRequest::parse(&buf).is_err(),
                "type {kind:#04X} was accepted"
            );
        }
    }

    /// Client-declared lengths are bounded before anything is allocated.
    #[test]
    fn client_declared_lengths_are_bounded() {
        // A tag count far beyond the buffer.
        let mut buf = vec![ROP_SYNCHRONIZATION_CONFIGURE, 0x00, 0x01, 0x02, 0x01];
        buf.push(send_options::UNICODE);
        buf.extend_from_slice(&sync_flags::UNICODE.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // no restriction
        buf.extend_from_slice(&0u32.to_le_bytes()); // extra flags
        buf.extend_from_slice(&0xFFFFu16.to_le_bytes()); // absurd tag count
        assert!(SyncConfigureRequest::parse(&buf).is_err());

        // And a truncated buffer stops rather than reading past the end.
        assert!(SyncConfigureRequest::parse(&[ROP_SYNCHRONIZATION_CONFIGURE, 0x00]).is_err());
        assert!(SyncConfigureRequest::parse(&[]).is_err());
    }

    /// A different operation's bytes must not parse as this one.
    #[test]
    fn another_operation_is_not_mistaken_for_this_one() {
        let mut buf = request(0x01, sync_flags::UNICODE, &[], 0, &[]);
        buf[0] = 0x71;
        assert!(SyncConfigureRequest::parse(&buf).is_err());
    }

    /// The response is the six bytes the specification gives, and says nothing
    /// about the context itself.
    #[test]
    fn the_success_response_is_six_bytes() {
        let body = configure_success_body(0x02);
        assert_eq!(body.len(), RESPONSE_LEN);
        assert_eq!(
            body,
            vec![ROP_SYNCHRONIZATION_CONFIGURE, 0x02, 0x00, 0x00, 0x00, 0x00]
        );
    }

    /// Trailing bytes belong to the next operation in the buffer, not to this
    /// one — ROPs are pipelined, so over-reading corrupts the whole batch.
    #[test]
    fn parsing_leaves_the_following_operation_alone() {
        let mut buf = request(0x01, sync_flags::UNICODE, &[], 0, &[(0x0003, 0x3602)]);
        buf.extend_from_slice(&[0x01, 0x00, 0x00]); // a RopRelease behind it
        let (_, rest) = SyncConfigureRequest::parse(&buf).unwrap();
        assert_eq!(rest, &[0x01, 0x00, 0x00]);
    }
}
