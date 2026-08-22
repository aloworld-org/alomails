//! The messages a MAPI client sees in a folder, built from the tenant's own
//! mail.
//!
//! [`crate::folders::FolderView`] answers "what folders are there"; this
//! answers "what is in one of them". The two are separate because they are
//! loaded differently, and that difference is the whole design of this module.
//!
//! ## Why a folder's messages are loaded and the folder tree is not
//!
//! A mailbox has tens of folders and a client wants all of them at once, so
//! the tree is read whole on every `Execute`. A folder has as many messages as
//! somebody has ever received, and a client wants one folder's worth at a
//! time. Reading every message in every folder to answer a request about one
//! of them would make the cost of opening any folder scale with the size of
//! the whole mailbox.
//!
//! So this view holds messages for **only the folders a request actually
//! reaches**, and the router works out which those are before dispatching
//! (see [`crate::dispatch::wanted_contents`]). A folder that was not asked
//! about is simply absent, which is different from a folder that was asked
//! about and is empty — [`MessageView::rows`] distinguishes them, because
//! answering "no messages" for a folder nobody loaded would be a lie a client
//! caches.
//!
//! ## Message ids
//!
//! A MID is 64 bits and an alo message id is an opaque string, so a MID is a
//! stable hash of the string, in the same shape and for the same reason as a
//! folder id: a client caches these, so the same message must keep the same
//! MID across restarts. As with folders the hash is one-way, and a MID is
//! resolved back to a message by looking through the view rather than by
//! inverting it.
//!
//! Unlike folder ids there is no reserved range: no MID is advertised before
//! the store has been read, so nothing needs protecting from collision with a
//! fixed value.

use alo_store::{MailboxId, MapiMessageRow, MessageId};

use crate::folders::{FolderView, MAX_COUNTER};
use crate::rows::{filetime_from_unix_secs, mf};

/// The most messages one folder reports in a single `Execute`.
///
/// A client pages through a table with `RopQueryRows`, so this is the ceiling
/// on one *response*, not on a folder. It is deliberately larger than any
/// screenful and far smaller than a mailbox.
pub const MAX_MESSAGES: u32 = 2_000;

/// One message as a client sees it in a contents table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageEntry {
    /// The id this message is opened by.
    pub mid: u64,
    /// The store message behind it.
    pub message: MessageId,
    /// What the client displays as the subject.
    pub subject: String,
    /// Who it is from, as a display name.
    pub sender: String,
    /// When the store received it, as a `FILETIME`.
    pub delivery_time: u64,
    /// `PidTagMessageFlags` — the bits alo can answer.
    pub flags: u32,
    /// Size of the raw message in octets.
    pub size: u32,
    /// Whether it carries an attachment.
    pub has_attachment: bool,
}

/// The counter part of a message id, derived from the store's own id.
///
/// FNV-1a, the same hash [`crate::folders::mailbox_counter`] uses, folded into
/// the 48 bits a counter occupies. Distinct from zero because a MID of zero is
/// what an uninitialised field holds, and a client cannot tell those apart.
#[must_use]
pub fn message_counter(id: &MessageId) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in id.as_str().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    1 + (hash % MAX_COUNTER)
}

impl MessageEntry {
    /// Builds an entry from a store row.
    #[must_use]
    pub fn from_row(row: &MapiMessageRow) -> Self {
        let mut flags = mf::UNMODIFIED;
        if row.seen {
            flags |= mf::READ;
        }
        if row.has_attachment {
            flags |= mf::HAS_ATTACH;
        }
        Self {
            mid: crate::folders::fid(message_counter(&row.id)),
            message: row.id.clone(),
            subject: row.subject.clone(),
            // The display name a client shows beside a message. alo stores the
            // unfolded `From` header, which is already what a person reads;
            // splitting a display name out of it is [MS-OXCMAIL] §2.1.2 work
            // and belongs with the stage that parses addresses properly, not
            // with a half-parse here that would be wrong on quoted names.
            sender: row.from_addr.clone(),
            delivery_time: filetime_from_unix_secs(row.received_at.unix_timestamp()),
            flags,
            size: u32::try_from(row.size).unwrap_or(u32::MAX),
            has_attachment: row.has_attachment,
        }
    }
}

/// The messages loaded for this request, by folder.
///
/// Empty by default: a request that reaches no contents table loads nothing,
/// which is the common case for the handshake and the folder tree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageView {
    folders: Vec<(u64, Vec<MessageEntry>)>,
}

impl MessageView {
    /// A view holding nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the messages loaded for one folder.
    ///
    /// Replaces any earlier entry for the same folder, so a caller that loads
    /// twice cannot end up with a table that reads one list and counts another.
    pub fn insert(&mut self, folder_id: u64, rows: &[MapiMessageRow]) {
        let entries: Vec<MessageEntry> = rows.iter().map(MessageEntry::from_row).collect();
        match self
            .folders
            .iter_mut()
            .find(|(stored, _)| *stored == folder_id)
        {
            Some((_, existing)) => *existing = entries,
            None => self.folders.push((folder_id, entries)),
        }
    }

    /// The messages loaded for a folder, or `None` if it was not loaded.
    ///
    /// The distinction matters: `Some(&[])` is "this folder is empty", which a
    /// client may cache, and `None` is "nobody asked", which it must not.
    #[must_use]
    pub fn rows(&self, folder_id: u64) -> Option<&[MessageEntry]> {
        self.folders
            .iter()
            .find(|(stored, _)| *stored == folder_id)
            .map(|(_, entries)| entries.as_slice())
    }

    /// The store mailbox a folder id names, if the tree has one behind it.
    ///
    /// A protocol-only folder — one of the special folders with no alo mailbox
    /// under it — has no messages to load, and says so by returning `None`.
    #[must_use]
    pub fn mailbox_of(folders: &FolderView, folder_id: u64) -> Option<MailboxId> {
        folders
            .get(folder_id)
            .and_then(|entry| entry.mailbox.clone())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use alo_store::{MapiMessageRow, MessageId};
    use time::OffsetDateTime;

    use super::{MAX_MESSAGES, MessageEntry, MessageView, message_counter};
    use crate::folders::{MAX_COUNTER, counter_of};
    use crate::rows::mf;

    fn row(id: &str, seen: bool, attachment: bool) -> MapiMessageRow {
        MapiMessageRow {
            id: MessageId::new(id.to_owned()),
            subject: "Rechnung".to_owned(),
            from_addr: "Liège Müller <l@example.test>".to_owned(),
            // 2026-08-22T03:00:00Z
            received_at: OffsetDateTime::from_unix_timestamp(1_787_713_200).expect("time"),
            size: 4096,
            seen,
            has_attachment: attachment,
        }
    }

    #[test]
    fn a_mid_is_stable_and_never_zero() {
        let id = MessageId::new("msg-abc".to_owned());
        assert_eq!(message_counter(&id), message_counter(&id));
        assert!(message_counter(&id) > 0);
        assert!(message_counter(&id) <= MAX_COUNTER);
    }

    #[test]
    fn a_mid_is_a_well_formed_id_with_a_recoverable_counter() {
        let entry = MessageEntry::from_row(&row("msg-abc", false, false));
        // The same 64-bit shape a folder id has: replica plus counter. A MID
        // the client cannot round-trip is one it cannot ask us to open.
        assert!(counter_of(entry.mid).is_some());
    }

    #[test]
    fn ten_thousand_ids_do_not_collide() {
        // A collision would show two different messages under one MID, and the
        // client would open whichever the scan found first.
        let mut seen: Vec<u64> = (0..10_000)
            .map(|n| message_counter(&MessageId::new(format!("msg-{n}"))))
            .collect();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), before);
    }

    #[test]
    fn read_state_and_attachments_become_flag_bits() {
        let plain = MessageEntry::from_row(&row("a", false, false));
        assert_eq!(plain.flags & mf::READ, 0);
        assert_eq!(plain.flags & mf::HAS_ATTACH, 0);
        assert_eq!(plain.flags & mf::UNMODIFIED, mf::UNMODIFIED);

        let read = MessageEntry::from_row(&row("b", true, true));
        assert_eq!(read.flags & mf::READ, mf::READ);
        assert_eq!(read.flags & mf::HAS_ATTACH, mf::HAS_ATTACH);
        assert!(read.has_attachment);
    }

    #[test]
    fn the_delivery_time_is_a_filetime_not_a_unix_timestamp() {
        let entry = MessageEntry::from_row(&row("a", false, false));
        // 1601-based and in 100-ns ticks, so vastly larger than the seconds
        // value — the check that catches an epoch mistake, which otherwise
        // renders as a plausible-looking date in the wrong century.
        assert_eq!(
            entry.delivery_time,
            (1_787_713_200 + 11_644_473_600) * 10_000_000
        );
    }

    #[test]
    fn a_loaded_empty_folder_is_not_the_same_as_an_unloaded_one() {
        let mut view = MessageView::new();
        assert_eq!(view.rows(42), None);
        view.insert(42, &[]);
        assert_eq!(view.rows(42), Some([].as_slice()));
    }

    #[test]
    fn loading_a_folder_twice_replaces_rather_than_appends() {
        let mut view = MessageView::new();
        view.insert(7, &[row("a", false, false), row("b", false, false)]);
        view.insert(7, &[row("c", false, false)]);
        let rows = view.rows(7).expect("loaded");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].message.as_str(), "c");
    }

    #[test]
    fn a_utf8_subject_and_sender_survive_intact() {
        // A European product: these are test cases, not edge cases.
        let entry = MessageEntry::from_row(&row("a", false, false));
        assert_eq!(entry.subject, "Rechnung");
        assert!(entry.sender.contains("Liège Müller"));
    }

    #[test]
    fn a_full_page_survives_intact() {
        // Not an assertion about the constant — a check that a view built from
        // a whole page holds every row, which is what keeps one `Execute`
        // independent of how much mail somebody has.
        let rows: Vec<_> = (0..MAX_MESSAGES)
            .map(|n| row(&format!("m-{n}"), false, false))
            .collect();
        let mut view = MessageView::new();
        view.insert(1, &rows);
        assert_eq!(view.rows(1).expect("loaded").len(), MAX_MESSAGES as usize);
    }
}
