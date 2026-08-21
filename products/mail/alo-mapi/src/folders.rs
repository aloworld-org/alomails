//! The folder tree a MAPI client sees, built from the tenant's own mailboxes.
//!
//! Until this module the adapter served thirteen fixed folders and refused to
//! answer a message count. This is where the store arrives: a client now sees
//! the folders a person actually made, with the names they gave them and the
//! number of messages in each.
//!
//! ## Two kinds of folder, one namespace
//!
//! MAPI requires thirteen **special folders** that a logon names by position
//! ([`SpecialFolder`]). Some of them are real places in an alo mailbox — the
//! inbox, sent mail, the bin — and the rest exist only because the protocol
//! expects them.
//!
//! So a folder id belongs to one of two ranges, and the split is what keeps the
//! two kinds from colliding:
//!
//! * **Counters 1..=13** are reserved for the special folders, in slot order.
//!   The logon advertises these before any mailbox has been read, and a client
//!   caches them, so they can never move.
//! * **Counters above that** are derived from a mailbox's own id by a stable
//!   hash, so the same mailbox keeps the same folder id across restarts —
//!   another thing a client caches and would be badly served by us changing.
//!
//! **A mailbox with a role adopts the special folder's id rather than its own.**
//! The logon has already told the client that folder id 5 is the Inbox; if the
//! real inbox also appeared under a hashed id, the client would draw two
//! inboxes and disagree with itself about which one new mail arrives in.
//!
//! ## What a hash cannot do
//!
//! The hash is one-way, so a folder id is resolved back to a mailbox by looking
//! through this view rather than by inverting it. That is a scan of the
//! tenant's own mailboxes — tens, not millions — and it is honest about what it
//! is. A persistent id column in the store would be better and is worth doing
//! when folder ids need to outlive a process, which they do not yet.

use alo_store::{Mailbox, MailboxId};

use crate::logon_response::SpecialFolder;

/// The replica id this deployment issues folder ids under.
pub const REPLICA_ID: u16 = 1;

/// Counters at or below this belong to the special folders.
pub const RESERVED_COUNTERS: u64 = 13;

/// The largest counter a folder id can hold (48 bits).
pub const MAX_COUNTER: u64 = (1 << 48) - 1;

/// The JMAP roles that are also MAPI special folders.
///
/// Only these three: MAPI's other twelve are protocol furniture, and alo has no
/// outbox at all — mail leaves when it is sent rather than waiting in a folder.
/// Drafts, Archive and Junk are real alo mailboxes with no MAPI slot, so they
/// appear as ordinary folders under their own names, which is what they are.
#[must_use]
pub fn special_for_role(role: &str) -> Option<SpecialFolder> {
    match role {
        "inbox" => Some(SpecialFolder::Inbox),
        "sent" => Some(SpecialFolder::SentItems),
        "trash" => Some(SpecialFolder::DeletedItems),
        _ => None,
    }
}

/// The 64-bit folder id for a special folder — its reserved counter.
#[must_use]
pub fn special_fid(folder: SpecialFolder) -> u64 {
    fid(folder.slot() as u64 + 1)
}

/// A folder id from a 48-bit counter and this deployment's replica.
#[must_use]
pub fn fid(counter: u64) -> u64 {
    let mut out = [0u8; 8];
    out[0..2].copy_from_slice(&REPLICA_ID.to_le_bytes());
    let counter = (counter & MAX_COUNTER).to_le_bytes();
    out[2..8].copy_from_slice(&counter[0..6]);
    u64::from_le_bytes(out)
}

/// The counter inside a folder id, if it is one of ours.
#[must_use]
pub fn counter_of(folder_id: u64) -> Option<u64> {
    let bytes = folder_id.to_le_bytes();
    if u16::from_le_bytes([bytes[0], bytes[1]]) != REPLICA_ID {
        return None;
    }
    Some(u64::from_le_bytes([
        bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], 0, 0,
    ]))
}

/// A stable counter for a mailbox, above the reserved range.
///
/// FNV-1a over the mailbox's opaque id: stable across processes, cheap, and
/// with no seed to lose. Not a cryptographic hash and not required to be —
/// nothing here is a secret, and a folder id is guessable by design (a client
/// is told every one of them).
///
/// Folded into the range above the reserved counters, so a mailbox can never
/// take a special folder's id.
#[must_use]
pub fn mailbox_counter(id: &MailboxId) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in id.as_str().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    let span = MAX_COUNTER - RESERVED_COUNTERS;
    RESERVED_COUNTERS + 1 + (hash % span)
}

/// One folder as a client sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderEntry {
    /// The id this folder is opened by.
    pub fid: u64,
    /// The folder this one hangs off, or `None` for the mailbox root.
    pub parent: Option<u64>,
    /// What the client displays.
    pub name: String,
    /// How many messages it holds.
    ///
    /// Zero for the protocol's own folders, and that is a measurement rather
    /// than a guess: the store has been read, no mailbox stands behind them, so
    /// there are no messages in them. Refusing here would only be honest while
    /// we had not looked — and it would refuse the whole row, taking the
    /// folders that *do* have counts down with it.
    pub total_messages: u32,
    /// The store mailbox behind this folder, if there is one.
    pub mailbox: Option<MailboxId>,
}

/// The whole folder tree for one mailbox, special folders included.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderView {
    entries: Vec<FolderEntry>,
}

impl FolderView {
    /// Builds the tree a client will see from the tenant's own mailboxes.
    ///
    /// The shape: the mailbox root holds the interpersonal-messages subtree and
    /// the protocol's own folders; the subtree holds the user's top-level
    /// mailboxes; and each mailbox holds its children, however deep they go.
    #[must_use]
    pub fn build(mailboxes: &[Mailbox]) -> Self {
        let mut entries = Vec::with_capacity(mailboxes.len() + SpecialFolder::ALL.len());

        // The special folders first, so their reserved ids exist even when no
        // mailbox claims them.
        let root = special_fid(SpecialFolder::Root);
        let subtree = special_fid(SpecialFolder::IpmSubtree);
        for folder in SpecialFolder::ALL {
            let parent = match folder {
                SpecialFolder::Root => None,
                SpecialFolder::IpmSubtree => Some(root),
                // The three that mirror a real mailbox hang where a reader
                // expects them; the rest are the root's protocol furniture.
                SpecialFolder::Inbox
                | SpecialFolder::SentItems
                | SpecialFolder::DeletedItems
                | SpecialFolder::Outbox => Some(subtree),
                _ => Some(root),
            };
            entries.push(FolderEntry {
                fid: special_fid(folder),
                parent,
                name: crate::hierarchy::display_name(folder).to_owned(),
                total_messages: 0,
                mailbox: None,
            });
        }

        // Then the store's own mailboxes. One that carries a role takes over
        // the matching special folder's entry rather than adding a second one.
        for mailbox in mailboxes {
            let special = mailbox.role.as_deref().and_then(special_for_role);
            let total = u32::try_from(mailbox.total_messages.max(0)).unwrap_or(u32::MAX);

            if let Some(folder) = special {
                let fid = special_fid(folder);
                if let Some(entry) = entries.iter_mut().find(|entry| entry.fid == fid) {
                    // The person's own name for it, not the protocol's.
                    entry.name.clone_from(&mailbox.name);
                    entry.total_messages = total;
                    entry.mailbox = Some(mailbox.id.clone());
                }
                continue;
            }

            entries.push(FolderEntry {
                fid: fid(mailbox_counter(&mailbox.id)),
                // A mailbox with no parent hangs off the subtree, which is the
                // root of everything a person sees.
                parent: Some(match &mailbox.parent_id {
                    Some(parent) => fid(mailbox_counter(parent)),
                    None => subtree,
                }),
                name: mailbox.name.clone(),
                total_messages: total,
                mailbox: Some(mailbox.id.clone()),
            });
        }

        // A child whose parent is a role-carrying mailbox must point at the
        // special folder's id, since that is where the parent actually lives.
        let moved: Vec<(u64, u64)> = mailboxes
            .iter()
            .filter_map(|mailbox| {
                let folder = mailbox.role.as_deref().and_then(special_for_role)?;
                Some((fid(mailbox_counter(&mailbox.id)), special_fid(folder)))
            })
            .collect();
        for entry in &mut entries {
            if let Some(parent) = entry.parent
                && let Some((_, to)) = moved.iter().find(|(from, _)| *from == parent)
            {
                entry.parent = Some(*to);
            }
        }

        Self { entries }
    }

    /// The folder a folder id names, if this view has one.
    #[must_use]
    pub fn get(&self, folder_id: u64) -> Option<&FolderEntry> {
        self.entries.iter().find(|entry| entry.fid == folder_id)
    }

    /// The children of a folder, in the order a client will draw them.
    #[must_use]
    pub fn children(&self, folder_id: u64) -> Vec<&FolderEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.parent == Some(folder_id))
            .collect()
    }

    /// Every folder in the view.
    #[must_use]
    pub fn entries(&self) -> &[FolderEntry] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn mailbox(
        id: &str,
        name: &str,
        role: Option<&str>,
        parent: Option<&str>,
        total: i64,
    ) -> Mailbox {
        Mailbox {
            id: MailboxId::new(id),
            parent_id: parent.map(MailboxId::new),
            name: name.to_owned(),
            role: role.map(ToOwned::to_owned),
            color: None,
            total_messages: total,
            unread_messages: 0,
        }
    }

    /// A mailbox with a role takes over the special folder's id rather than
    /// appearing beside it. Two inboxes is a client that disagrees with itself
    /// about where new mail lands.
    #[test]
    fn a_role_carrying_mailbox_becomes_the_special_folder() {
        let view = FolderView::build(&[mailbox("mb-1", "Postvak IN", Some("inbox"), None, 42)]);

        let inbox = view.get(special_fid(SpecialFolder::Inbox)).unwrap();
        assert_eq!(inbox.name, "Postvak IN", "the person's own name for it");
        assert_eq!(inbox.total_messages, 42);
        assert_eq!(inbox.mailbox, Some(MailboxId::new("mb-1")));

        // ...and it appears exactly once in the whole view.
        let named: Vec<_> = view
            .entries()
            .iter()
            .filter(|entry| entry.mailbox == Some(MailboxId::new("mb-1")))
            .collect();
        assert_eq!(named.len(), 1, "the inbox was listed twice");
    }

    /// A mailbox with no MAPI slot is an ordinary folder under its own name.
    #[test]
    fn a_mailbox_without_a_role_is_an_ordinary_folder() {
        let view = FolderView::build(&[
            mailbox("mb-1", "Inbox", Some("inbox"), None, 3),
            mailbox("mb-2", "Facturen", None, None, 7),
        ]);

        let subtree = view.children(special_fid(SpecialFolder::IpmSubtree));
        let names: Vec<&str> = subtree.iter().map(|entry| entry.name.as_str()).collect();
        assert!(names.contains(&"Facturen"), "{names:?}");
        assert!(names.contains(&"Inbox"), "{names:?}");

        let invoices = subtree
            .iter()
            .find(|entry| entry.name == "Facturen")
            .unwrap();
        assert_eq!(invoices.total_messages, 7);
        assert!(
            counter_of(invoices.fid).unwrap() > RESERVED_COUNTERS,
            "a store mailbox took a reserved folder id"
        );
    }

    /// Nesting survives: a child of a mailbox hangs off that mailbox, and a
    /// child of the *inbox* hangs off the special folder the inbox became.
    #[test]
    fn nesting_survives_and_follows_a_role_to_its_special_folder() {
        let view = FolderView::build(&[
            mailbox("mb-1", "Inbox", Some("inbox"), None, 0),
            mailbox("mb-2", "Projects", None, None, 0),
            mailbox("mb-3", "Alpha", None, Some("mb-2"), 5),
            mailbox("mb-4", "Newsletters", None, Some("mb-1"), 9),
        ]);

        // A child of an ordinary folder.
        let projects = view
            .entries()
            .iter()
            .find(|entry| entry.name == "Projects")
            .unwrap();
        let alpha = view.children(projects.fid);
        assert_eq!(alpha.len(), 1);
        assert_eq!(alpha[0].name, "Alpha");

        // A child of the inbox hangs off the *special* inbox id, because that
        // is where the parent actually lives in this view.
        let under_inbox = view.children(special_fid(SpecialFolder::Inbox));
        assert_eq!(under_inbox.len(), 1, "the child was orphaned");
        assert_eq!(under_inbox[0].name, "Newsletters");
    }

    /// The reserved range is what keeps the two kinds of folder apart. A
    /// mailbox id must never hash into a special folder's counter.
    #[test]
    fn a_mailbox_never_takes_a_reserved_counter() {
        for n in 0..2000 {
            let counter = mailbox_counter(&MailboxId::new(format!("mb-{n}")));
            assert!(
                counter > RESERVED_COUNTERS,
                "mb-{n} hashed into the reserved range"
            );
            assert!(counter <= MAX_COUNTER, "mb-{n} overflowed 48 bits");
        }
    }

    /// The same mailbox keeps the same folder id — a client caches these, and
    /// an id that moved would send it looking for a folder that no longer
    /// exists.
    #[test]
    fn a_folder_id_is_stable_for_the_same_mailbox() {
        let once = mailbox_counter(&MailboxId::new("mb-stable"));
        let again = mailbox_counter(&MailboxId::new("mb-stable"));
        assert_eq!(once, again);
        assert_ne!(once, mailbox_counter(&MailboxId::new("mb-other")));
    }

    /// An empty mailbox still has the protocol's folders: the tree is never
    /// empty, because a client needs somewhere to start.
    #[test]
    fn the_special_folders_exist_even_with_no_mailboxes() {
        let view = FolderView::build(&[]);
        assert_eq!(view.entries().len(), SpecialFolder::ALL.len());

        let root = special_fid(SpecialFolder::Root);
        assert!(view.get(root).is_some());
        assert_eq!(
            view.get(root).unwrap().parent,
            None,
            "the root has a parent"
        );
        assert!(!view.children(root).is_empty());

        // With no mailbox behind it the inbox reports zero, and that is a fact
        // rather than a placeholder: the store was read and holds no such
        // mailbox, so there are no messages in it.
        let inbox = view.get(special_fid(SpecialFolder::Inbox)).unwrap();
        assert_eq!(inbox.total_messages, 0);
        assert_eq!(inbox.mailbox, None);
    }

    /// A folder id from another replica is not ours, whatever its counter says.
    #[test]
    fn a_foreign_replica_is_not_one_of_our_folder_ids() {
        assert!(counter_of(fid(5)).is_some());
        let mut foreign = fid(5).to_le_bytes();
        foreign[0..2].copy_from_slice(&99u16.to_le_bytes());
        assert_eq!(counter_of(u64::from_le_bytes(foreign)), None);
    }

    /// Only the three that are real places in an alo mailbox map to a role.
    #[test]
    fn only_the_folders_alo_really_has_map_to_a_role() {
        assert_eq!(special_for_role("inbox"), Some(SpecialFolder::Inbox));
        assert_eq!(special_for_role("sent"), Some(SpecialFolder::SentItems));
        assert_eq!(special_for_role("trash"), Some(SpecialFolder::DeletedItems));
        // alo sends mail rather than queuing it, so there is no outbox to map.
        assert_eq!(special_for_role("drafts"), None);
        assert_eq!(special_for_role("junk"), None);
        assert_eq!(special_for_role("archive"), None);
    }
}
