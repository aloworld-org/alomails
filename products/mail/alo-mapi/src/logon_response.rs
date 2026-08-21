//! The `RopLogon` success response for a private mailbox
//! ([MS-OXCROPS] §2.2.3.1.2, [MS-OXCSTOR] §2.2.1.1.3).
//!
//! 166 bytes, in this order:
//!
//! | Field | Size |
//! |---|---|
//! | `RopId` | 1 (`0xFE`) |
//! | `OutputHandleIndex` | 1 |
//! | `ReturnValue` | 4 (`0` for success) |
//! | `LogonFlags` | 1 |
//! | `FolderIds` | 104 — thirteen 64-bit folder ids |
//! | `ResponseFlags` | 1 |
//! | `MailboxGuid` | 16 |
//! | `ReplId` | 2 |
//! | `ReplGuid` | 16 |
//! | `LogonTime` | 8 |
//! | `GwartTime` | 8 |
//! | `StoreState` | 4 (`0`) |
//!
//! **`LogonFlags` is echoed, not decided.** [MS-OXCSTOR] §2.2.1.1.3: "the
//! server returns these flags unchanged from the `LogonFlags` field of the
//! `RopLogon` request".
//!
//! **The thirteen folders are positional.** There is no tag beside them — a
//! client reads meaning from the slot, so an id in the wrong position is a
//! working response that points Outlook at the wrong folder. [`SpecialFolder`]
//! exists so the order is stated once, in the specification's own sequence, and
//! the array is built from it rather than by hand.
//!
//! **`ResponseFlags` bit `0x01` MUST be set**, and it is the bit that means
//! nothing: "this bit MUST be set and MUST be ignored by the client". The three
//! that carry meaning are owner, send-as, and out-of-office.

/// The `RopId` this response carries, matching the request.
pub const ROP_LOGON: u8 = 0xFE;

/// The exact size of this response ([MS-OXCROPS] §2.2.3.1.2).
pub const LOGON_RESPONSE_LEN: usize = 166;

/// `ResponseFlags` — MUST be set, and carries no meaning.
pub const RESPONSE_RESERVED: u8 = 0x01;
/// `ResponseFlags` — the user has owner permission on the mailbox.
pub const RESPONSE_OWNER_RIGHT: u8 = 0x02;
/// `ResponseFlags` — the user may send mail from the mailbox.
pub const RESPONSE_SEND_AS_RIGHT: u8 = 0x04;
/// `ResponseFlags` — the mailbox has Out of Office set.
pub const RESPONSE_OOF: u8 = 0x10;

/// The thirteen special folders a logon reports, **in the order the wire
/// expects them** ([MS-OXCSTOR] §2.2.1.1.3).
///
/// The discriminants are the array positions, and the order is the
/// specification's. Nothing on the wire names these folders, so a value in the
/// wrong slot is not an error the client reports — it is a client that opens
/// the wrong folder and behaves oddly ever after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum SpecialFolder {
    /// The mailbox root; every other folder here descends from it.
    Root = 0,
    /// Deferred Action.
    DeferredAction = 1,
    /// Spooler Queue.
    SpoolerQueue = 2,
    /// The interpersonal-messages subtree — the root of what a user sees.
    IpmSubtree = 3,
    /// Inbox.
    Inbox = 4,
    /// Outbox.
    Outbox = 5,
    /// Sent Items.
    SentItems = 6,
    /// Deleted Items.
    DeletedItems = 7,
    /// Common Views.
    CommonViews = 8,
    /// Schedule.
    Schedule = 9,
    /// Search.
    Search = 10,
    /// Views.
    Views = 11,
    /// Shortcuts.
    Shortcuts = 12,
}

impl SpecialFolder {
    /// Every special folder, in wire order.
    pub const ALL: [Self; 13] = [
        Self::Root,
        Self::DeferredAction,
        Self::SpoolerQueue,
        Self::IpmSubtree,
        Self::Inbox,
        Self::Outbox,
        Self::SentItems,
        Self::DeletedItems,
        Self::CommonViews,
        Self::Schedule,
        Self::Search,
        Self::Views,
        Self::Shortcuts,
    ];

    /// This folder's slot in the `FolderIds` array.
    #[must_use]
    pub const fn slot(self) -> usize {
        self as usize
    }
}

/// A folder id ([MS-OXCDATA] §2.2.1.1): a 16-bit replica id followed by a
/// 48-bit global counter, written little-endian as one 64-bit value.
///
/// The counter is 48 bits, so a value that would overflow it is not a folder id
/// — it is truncated silently by a naive shift, which would make two different
/// folders share an identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fid {
    /// The replica id namespace this counter belongs to.
    pub replica: u16,
    /// The 48-bit counter within that namespace.
    pub counter: u64,
}

impl Fid {
    /// The largest counter a folder id can hold.
    pub const MAX_COUNTER: u64 = (1 << 48) - 1;

    /// A folder id, or `None` if the counter does not fit 48 bits.
    #[must_use]
    pub const fn new(replica: u16, counter: u64) -> Option<Self> {
        if counter > Self::MAX_COUNTER {
            return None;
        }
        Some(Self { replica, counter })
    }

    /// The eight bytes of this id: the replica id first, then six bytes of
    /// counter, all little-endian.
    #[must_use]
    pub fn to_bytes(self) -> [u8; 8] {
        let mut out = [0u8; 8];
        out[0..2].copy_from_slice(&self.replica.to_le_bytes());
        let counter = self.counter.to_le_bytes();
        out[2..8].copy_from_slice(&counter[0..6]);
        out
    }
}

/// The wall-clock components a logon reports ([MS-OXCROPS] §2.2.3.1.2.1).
///
/// Taken as values rather than read from a clock here: a response that reaches
/// for the system time is one no test can assert byte-for-byte, and this is a
/// structure whose exact bytes are the whole point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LogonTime {
    /// Seconds.
    pub seconds: u8,
    /// Minutes.
    pub minutes: u8,
    /// Hour.
    pub hour: u8,
    /// Day of week, Sunday being zero.
    pub day_of_week: u8,
    /// Day of the month.
    pub day: u8,
    /// Month, January being one.
    pub month: u8,
    /// Year.
    pub year: u16,
}

impl LogonTime {
    /// The eight bytes of this structure.
    #[must_use]
    pub fn to_bytes(self) -> [u8; 8] {
        let mut out = [0u8; 8];
        out[0] = self.seconds;
        out[1] = self.minutes;
        out[2] = self.hour;
        out[3] = self.day_of_week;
        out[4] = self.day;
        out[5] = self.month;
        out[6..8].copy_from_slice(&self.year.to_le_bytes());
        out
    }
}

/// A `RopLogon` success response for a private mailbox.
#[derive(Debug, Clone)]
pub struct LogonResponse {
    /// The handle-table slot the request named for its output object.
    pub output_handle_index: u8,
    /// The logon flags, echoed unchanged from the request.
    pub logon_flags: u8,
    /// The thirteen special folders, in [`SpecialFolder`] order.
    pub folder_ids: [Fid; 13],
    /// What the client is told about the mailbox's state.
    pub response_flags: u8,
    /// The mailbox's identifying GUID.
    pub mailbox_guid: [u8; 16],
    /// The replica id for this logon.
    pub replica_id: u16,
    /// The GUID the replica id maps to.
    pub replica_guid: [u8; 16],
    /// When the logon happened.
    pub logon_time: LogonTime,
    /// A value that changes whenever the address routing table does. The client
    /// only ever compares it to the last one it saw.
    pub gwart_time: u64,
}

impl LogonResponse {
    /// Serialises the response ([MS-OXCROPS] §2.2.3.1.2).
    ///
    /// Always exactly [`LOGON_RESPONSE_LEN`] bytes — there is no variable-length
    /// field in it, so a response of any other size is a bug in this function.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(LOGON_RESPONSE_LEN);
        out.push(ROP_LOGON);
        out.push(self.output_handle_index);
        out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
        out.push(self.logon_flags);
        for fid in &self.folder_ids {
            out.extend_from_slice(&fid.to_bytes());
        }
        // The reserved bit is forced on rather than left to the caller: the
        // specification says it MUST be set, and a caller that forgot would
        // produce a response that is wrong in a way nothing here would notice.
        out.push(self.response_flags | RESPONSE_RESERVED);
        out.extend_from_slice(&self.mailbox_guid);
        out.extend_from_slice(&self.replica_id.to_le_bytes());
        out.extend_from_slice(&self.replica_guid);
        out.extend_from_slice(&self.logon_time.to_bytes());
        out.extend_from_slice(&self.gwart_time.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // StoreState: MUST be 0.
        debug_assert_eq!(out.len(), LOGON_RESPONSE_LEN);
        out
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn response() -> LogonResponse {
        // Each folder gets its slot number as a counter, so a misordered array
        // is visible in the bytes rather than hidden behind identical ids.
        let folder_ids = SpecialFolder::ALL
            .map(|folder| Fid::new(1, folder.slot() as u64 + 1).expect("a small counter fits"));
        LogonResponse {
            output_handle_index: 0,
            logon_flags: 0x01,
            folder_ids,
            response_flags: RESPONSE_OWNER_RIGHT | RESPONSE_SEND_AS_RIGHT,
            mailbox_guid: [0xAA; 16],
            replica_id: 1,
            replica_guid: [0xBB; 16],
            logon_time: LogonTime {
                seconds: 30,
                minutes: 45,
                hour: 13,
                day_of_week: 5,
                day: 21,
                month: 8,
                year: 2026,
            },
            gwart_time: 0,
        }
    }

    /// Every field at the offset the specification puts it. The response has no
    /// variable-length part, so each one can be named exactly — and an
    /// off-by-one anywhere shifts everything after it, which is precisely the
    /// error Outlook does not report.
    #[test]
    fn every_field_lands_at_the_offset_the_specification_gives_it() {
        let bytes = response().to_bytes();
        assert_eq!(bytes.len(), LOGON_RESPONSE_LEN, "166 bytes exactly");

        assert_eq!(bytes[0], ROP_LOGON, "RopId");
        assert_eq!(bytes[1], 0, "OutputHandleIndex");
        assert_eq!(&bytes[2..6], &0u32.to_le_bytes(), "ReturnValue is success");
        assert_eq!(bytes[6], 0x01, "LogonFlags echoed");
        // FolderIds: 104 bytes, thirteen ids of eight.
        assert_eq!(bytes[7..111].len(), 104);
        assert_eq!(bytes[111] & RESPONSE_RESERVED, RESPONSE_RESERVED);
        assert_eq!(&bytes[112..128], &[0xAA; 16], "MailboxGuid");
        assert_eq!(&bytes[128..130], &1u16.to_le_bytes(), "ReplId");
        assert_eq!(&bytes[130..146], &[0xBB; 16], "ReplGuid");
        assert_eq!(&bytes[146..154], &response().logon_time.to_bytes());
        assert_eq!(&bytes[154..162], &0u64.to_le_bytes(), "GwartTime");
        assert_eq!(
            &bytes[162..166],
            &0u32.to_le_bytes(),
            "StoreState MUST be 0"
        );
    }

    /// The folders are positional and nothing on the wire names them, so the
    /// order is pinned against the specification's own list. A value in the
    /// wrong slot is a working response that opens the wrong folder.
    #[test]
    fn the_thirteen_folders_are_written_in_the_specifications_order() {
        let bytes = response().to_bytes();
        let ids = &bytes[7..111];

        for folder in SpecialFolder::ALL {
            let at = folder.slot() * 8;
            let fid = &ids[at..at + 8];
            let counter =
                u64::from_le_bytes([fid[2], fid[3], fid[4], fid[5], fid[6], fid[7], 0, 0]);
            assert_eq!(
                counter,
                folder.slot() as u64 + 1,
                "{folder:?} is not in slot {}",
                folder.slot()
            );
        }

        // And the order itself, stated once so a reordering of the enum is
        // caught here rather than by a confused mail client.
        assert_eq!(SpecialFolder::Root.slot(), 0);
        assert_eq!(SpecialFolder::DeferredAction.slot(), 1);
        assert_eq!(SpecialFolder::SpoolerQueue.slot(), 2);
        assert_eq!(SpecialFolder::IpmSubtree.slot(), 3);
        assert_eq!(SpecialFolder::Inbox.slot(), 4);
        assert_eq!(SpecialFolder::Outbox.slot(), 5);
        assert_eq!(SpecialFolder::SentItems.slot(), 6);
        assert_eq!(SpecialFolder::DeletedItems.slot(), 7);
        assert_eq!(SpecialFolder::CommonViews.slot(), 8);
        assert_eq!(SpecialFolder::Schedule.slot(), 9);
        assert_eq!(SpecialFolder::Search.slot(), 10);
        assert_eq!(SpecialFolder::Views.slot(), 11);
        assert_eq!(SpecialFolder::Shortcuts.slot(), 12);
    }

    /// The reserved bit MUST be set. It is forced on here rather than trusted
    /// to the caller, because a caller that forgot produces a response that is
    /// wrong in a way nothing else would catch.
    #[test]
    fn the_reserved_response_flag_is_always_set() {
        let mut logon = response();
        logon.response_flags = 0;
        assert_eq!(logon.to_bytes()[111] & RESPONSE_RESERVED, RESPONSE_RESERVED);

        logon.response_flags = RESPONSE_OOF;
        let flags = logon.to_bytes()[111];
        assert_eq!(flags & RESPONSE_OOF, RESPONSE_OOF, "OOF survived");
        assert_eq!(flags & RESPONSE_RESERVED, RESPONSE_RESERVED);
    }

    /// A folder id is a replica id and a 48-bit counter. A counter that does not
    /// fit is refused rather than truncated — truncation would give two
    /// different folders the same identifier, which a client has no way to
    /// notice.
    #[test]
    fn a_folder_id_is_a_replica_and_a_forty_eight_bit_counter() {
        let fid = Fid::new(0x1234, 0x0000_5678_9ABC).unwrap();
        let bytes = fid.to_bytes();
        assert_eq!(&bytes[0..2], &0x1234u16.to_le_bytes(), "replica id first");
        assert_eq!(&bytes[2..8], &[0xBC, 0x9A, 0x78, 0x56, 0x00, 0x00]);

        assert!(Fid::new(1, Fid::MAX_COUNTER).is_some());
        assert!(
            Fid::new(1, Fid::MAX_COUNTER + 1).is_none(),
            "a counter that cannot fit was accepted"
        );
    }

    #[test]
    fn the_logon_time_fields_are_in_the_order_the_spec_gives() {
        let time = LogonTime {
            seconds: 1,
            minutes: 2,
            hour: 3,
            day_of_week: 4,
            day: 5,
            month: 6,
            year: 2026,
        };
        let bytes = time.to_bytes();
        assert_eq!(&bytes[0..6], &[1, 2, 3, 4, 5, 6]);
        assert_eq!(&bytes[6..8], &2026u16.to_le_bytes());
    }
}
