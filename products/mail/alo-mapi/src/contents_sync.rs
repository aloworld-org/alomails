//! The content synchronisation stream ([MS-OXCFXICS] §2.2.4.2) — what a folder
//! looks like when a client is keeping its own copy of it.
//!
//! This is the payload `RopFastTransferSourceGetBuffer` hands over. Everything
//! else in stage 8 is framing; this is the part that says which messages
//! changed, what they now contain, and where the client's state should stand
//! afterwards.
//!
//! ## The shape, from the grammar
//!
//! ```text
//! contentsSync      = [progressTotal]
//!                     *( [progressPerMessage] messageChange )
//!                     [deletions]
//!                     [readStateChanges]
//!                     state
//!                     IncrSyncEnd
//!
//! messageChangeFull = IncrSyncChg messageChangeHeader
//!                     IncrSyncMessage propList
//!                     messageChildren
//! messageChildren   = [MetaTagFXDelProp] [*recipient]
//!                     [MetaTagFXDelProp] [*attachment]
//! recipient         = StartRecip propList EndToRecip
//! state             = IncrSyncStateBegin propList IncrSyncStateEnd
//! ```
//!
//! The order is not a style: a client reads this as a stream and acts on each
//! marker as it arrives. `state` before `IncrSyncEnd` is what lets the client
//! record where it got to, and a stream that ends without it leaves the client
//! repeating the same download forever.
//!
//! ## The header is fixed, and its contents are prescribed
//!
//! [MS-OXCFXICS] §2.2.4.3.19 gives `messageChangeHeader` five required
//! properties in **fixed position**, three conditional ones, and prohibits
//! everything else:
//!
//! | Property | Tag | |
//! |---|---|---|
//! | `PidTagSourceKey` | `0x65E0` | required, fixed position |
//! | `PidTagLastModificationTime` | `0x3008` | required, fixed position |
//! | `PidTagChangeKey` | `0x65E2` | required, fixed position |
//! | `PidTagPredecessorChangeList` | `0x65E3` | required, fixed position |
//! | `PidTagAssociated` | `0x67AA` | required, fixed position |
//! | `PidTagMid` | `0x674A` | iff the `Eid` extra flag |
//! | `PidTagMessageSize` | `0x0E08` | iff the `MessageSize` extra flag |
//! | `PidTagChangeNumber` | `0x67A4` | iff the `CN` extra flag |
//!
//! Every id above is from [MS-OXPROPS]; the [MS-OXCFXICS] pages give only the
//! data types.
//!
//! ## The counter's byte order, and why it is derived rather than quoted
//!
//! `PidTagSourceKey` is an `XID` ([MS-OXCFXICS] §2.2.2.2): a 16-byte namespace
//! GUID and a `LocalId` of one to eight bytes. For a message that `XID` is a
//! `GID` — the replica GUID and the 48-bit counter — but [MS-OXCDATA] §2.2.1.3
//! describes `GlobalCounter` only as "6 bytes; an unsigned integer" and never
//! states its byte order.
//!
//! It is settled anyway, structurally. [MS-OXCFXICS] §2.2.2.4.2 says a
//! `REPLGUID` combined with the `GLOBCNT` values in a `GLOBSET` "produces a set
//! of `GID` structures" — so an `IDSET` *is* a compressed set of these same
//! identifiers. The two encodings must therefore agree, and a `GLOBSET` is
//! unambiguously most-significant-first, because its stack holds the values'
//! shared **high-order** bytes.
//!
//! So the counter here is written by [`crate::ics::globcnt_to_bytes`], the one
//! function that already makes that choice, and a test pins the agreement.
//! Recorded in `docs/interop.md` as derived from the specification rather than
//! confirmed against a real client.

use crate::fasttransfer::{Writer, marker};
use crate::folders::counter_of;
use crate::ics::GlobRange;
use crate::ics::{IdSet, globcnt_to_bytes, meta};
use crate::messages::MessageEntry;
use crate::rows::pid;
use crate::sync::SyncConfigureRequest;

/// `PidTagSourceKey` ([MS-OXPROPS] §2.1024) — the object's own identifier.
pub const PID_TAG_SOURCE_KEY: u16 = 0x65E0;
/// `PidTagLastModificationTime` ([MS-OXPROPS] §2.766).
pub const PID_TAG_LAST_MODIFICATION_TIME: u16 = 0x3008;
/// `PidTagChangeKey` ([MS-OXPROPS] §2.631) — identifies the last change.
pub const PID_TAG_CHANGE_KEY: u16 = 0x65E2;
/// `PidTagPredecessorChangeList` ([MS-OXPROPS] §2.869) — for conflict detection.
pub const PID_TAG_PREDECESSOR_CHANGE_LIST: u16 = 0x65E3;
/// `PidTagAssociated` ([MS-OXPROPS] §2.584) — whether this is an FAI message.
pub const PID_TAG_ASSOCIATED: u16 = 0x67AA;
/// `PidTagMid` ([MS-OXPROPS] §2.807) — the message id.
pub const PID_TAG_MID: u16 = 0x674A;
/// `PidTagMessageSize` ([MS-OXPROPS] §2.797).
pub const PID_TAG_MESSAGE_SIZE: u16 = 0x0E08;
/// `PidTagChangeNumber` ([MS-OXPROPS] §2.632).
pub const PID_TAG_CHANGE_NUMBER: u16 = 0x67A4;

/// An `XID` ([MS-OXCFXICS] §2.2.2.2): a namespace and an id within it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Xid {
    /// The namespace the local id belongs to — for us, the replica GUID.
    pub namespace: [u8; 16],
    /// One to eight bytes identifying the object in that namespace.
    pub local_id: Vec<u8>,
}

impl Xid {
    /// The `XID` naming a message or folder by its counter.
    ///
    /// The counter is written most-significant-first; see the module
    /// documentation for why that is not a coin toss.
    #[must_use]
    pub fn for_counter(replica: [u8; 16], counter: u64) -> Self {
        Self {
            namespace: replica,
            local_id: globcnt_to_bytes(counter).to_vec(),
        }
    }

    /// The bytes of the `XID` itself.
    #[must_use]
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 + self.local_id.len());
        out.extend_from_slice(&self.namespace);
        out.extend_from_slice(&self.local_id);
        out
    }

    /// The bytes of a `SizedXid` ([MS-OXCFXICS] §2.2.2.3.1): a one-byte length
    /// then the `XID`.
    #[must_use]
    pub fn serialize_sized(&self) -> Vec<u8> {
        let xid = self.serialize();
        let mut out = Vec::with_capacity(1 + xid.len());
        out.push(u8::try_from(xid.len()).unwrap_or(u8::MAX));
        out.extend_from_slice(&xid);
        out
    }
}

/// A `PredecessorChangeList` ([MS-OXCFXICS] §2.2.2.3): every `XID` that has
/// been folded into the object's current version, each length-prefixed.
#[must_use]
pub fn predecessor_change_list(xids: &[Xid]) -> Vec<u8> {
    let mut out = Vec::new();
    for xid in xids {
        out.extend_from_slice(&xid.serialize_sized());
    }
    out
}

/// One message the client is being told about.
#[derive(Debug, Clone)]
pub struct MessageChange {
    /// The message's own identifier.
    pub source_key: Xid,
    /// When it last changed, as a `FILETIME`.
    pub last_modified: u64,
    /// The identifier of that last change.
    pub change_key: Xid,
    /// The changes already folded in, for conflict detection.
    pub predecessors: Vec<Xid>,
    /// Whether this is a folder-associated message rather than mail.
    pub associated: bool,
    /// The message id, sent when the client asked for it.
    pub mid: u64,
    /// The size in octets, sent when the client asked for it.
    pub message_size: u32,
    /// The change number, sent when the client asked for it.
    pub change_number: u64,
    /// The message's own properties, already serialised as a `propList`.
    pub content: Vec<u8>,
    /// Each recipient's properties, already serialised as a `propList`.
    pub recipients: Vec<Vec<u8>>,
}

/// Where the client's state should stand once the stream has been read.
#[derive(Debug, Clone, Default)]
pub struct FinalState {
    /// The ids the client will then hold.
    pub idset_given: IdSet,
    /// The change numbers it will then have seen.
    pub cnset_seen: IdSet,
    /// The same, for folder-associated messages.
    pub cnset_seen_fai: IdSet,
    /// The change numbers whose read state it will then hold.
    pub cnset_read: IdSet,
}

/// Builds a content synchronisation stream.
///
/// `request` decides what is included: a client that did not ask for deletions
/// is not told about them, and one that did not ask for progress gets none.
/// Sending more than was asked for is not a protocol error and is still wrong —
/// the client stores whatever it is handed.
#[must_use]
pub fn build(
    request: &SyncConfigureRequest,
    changes: &[MessageChange],
    deleted: &IdSet,
    read: &IdSet,
    unread: &IdSet,
    state: &FinalState,
) -> Vec<u8> {
    let mut w = Writer::new();

    if request.wants_progress() {
        // progressTotal = IncrSyncProgressMode propList
        w.marker(marker::INCR_SYNC_PROGRESS_MODE);
        let total: u32 = changes
            .iter()
            .map(|change| change.message_size)
            .fold(0, u32::saturating_add);
        w.int32_tag(crate::fasttransfer::join_tag(0x0003, 0x4074), 0);
        w.int32_tag(
            crate::fasttransfer::join_tag(0x0003, 0x4075),
            i32::try_from(total).unwrap_or(i32::MAX),
        );
    }

    for change in changes {
        if request.wants_progress() {
            w.marker(marker::INCR_SYNC_PROGRESS_PER_MSG);
            w.int32(
                0x4075,
                i32::try_from(change.message_size).unwrap_or(i32::MAX),
            );
        }

        // messageChangeFull = IncrSyncChg messageChangeHeader
        //                     IncrSyncMessage propList messageChildren
        w.marker(marker::INCR_SYNC_CHG);
        write_header(&mut w, request, change);

        w.marker(marker::INCR_SYNC_MESSAGE);
        w.raw(&change.content);

        for recipient in &change.recipients {
            w.marker(marker::START_RECIP);
            w.raw(recipient);
            w.marker(marker::END_TO_RECIP);
        }
    }

    if request.wants_deletions() && !deleted.entries.is_empty() {
        w.marker(marker::INCR_SYNC_DEL);
        w.binary_tag(meta::IDSET_DELETED, &deleted.serialize());
    }

    if request.wants_read_state() && !(read.entries.is_empty() && unread.entries.is_empty()) {
        w.marker(marker::INCR_SYNC_READ);
        if !read.entries.is_empty() {
            w.binary_tag(meta::IDSET_READ, &read.serialize());
        }
        if !unread.entries.is_empty() {
            w.binary_tag(meta::IDSET_UNREAD, &unread.serialize());
        }
    }

    // state = IncrSyncStateBegin propList IncrSyncStateEnd. Without this the
    // client cannot record where it got to, and repeats the whole download on
    // every connection — a stream that works once and never converges.
    w.marker(marker::INCR_SYNC_STATE_BEGIN);
    w.binary_tag(meta::IDSET_GIVEN, &state.idset_given.serialize());
    w.binary_tag(meta::CNSET_SEEN, &state.cnset_seen.serialize());
    w.binary_tag(meta::CNSET_SEEN_FAI, &state.cnset_seen_fai.serialize());
    w.binary_tag(meta::CNSET_READ, &state.cnset_read.serialize());
    w.marker(marker::INCR_SYNC_STATE_END);

    w.marker(marker::INCR_SYNC_END);
    w.finish()
}

/// Writes `messageChangeHeader` — five required properties in the order
/// [MS-OXCFXICS] §2.2.4.3.19 fixes, then whatever the extra flags asked for.
fn write_header(w: &mut Writer, request: &SyncConfigureRequest, change: &MessageChange) {
    w.binary(PID_TAG_SOURCE_KEY, &change.source_key.serialize());
    w.time(PID_TAG_LAST_MODIFICATION_TIME, change.last_modified);
    w.binary(PID_TAG_CHANGE_KEY, &change.change_key.serialize());
    w.binary(
        PID_TAG_PREDECESSOR_CHANGE_LIST,
        &predecessor_change_list(&change.predecessors),
    );
    w.boolean(PID_TAG_ASSOCIATED, change.associated);

    // "MUST be present if and only if" — so each of these is absent unless
    // asked for, not merely harmless to include.
    if request.wants_eid() {
        w.int64(PID_TAG_MID, i64::from_le_bytes(change.mid.to_le_bytes()));
    }
    if request.wants_message_size() {
        w.int32(
            PID_TAG_MESSAGE_SIZE,
            i32::try_from(change.message_size).unwrap_or(i32::MAX),
        );
    }
    if request.wants_change_number() {
        w.int64(
            PID_TAG_CHANGE_NUMBER,
            i64::from_le_bytes(change.change_number.to_le_bytes()),
        );
    }
}

/// Builds the stream for a folder from the rows already loaded for it.
///
/// Returns the bytes and how many messages they describe, which is what the
/// progress fields count.
///
/// ## What is left out, and why that is the whole point
///
/// A message whose change number the client has already seen is **not sent**.
/// That is the difference between incremental synchronisation and downloading
/// the mailbox again every time a client connects, and it is the only reason
/// the state a client uploads is worth reading.
///
/// The comparison is against change numbers rather than ids: a message the
/// client holds may still have changed since, and it is the change that decides
/// whether it travels, not the identity.
#[must_use]
pub fn from_entries(
    request: &SyncConfigureRequest,
    replica: [u8; 16],
    entries: &[MessageEntry],
    seen: &IdSet,
) -> (Vec<u8>, u16) {
    let mut changes = Vec::new();
    let mut given = Vec::new();
    let mut numbers = Vec::new();

    for entry in entries {
        // A message with no counter is one whose id we could not read; sending
        // it under a made-up identifier is worse than leaving it out, because
        // the client would cache the wrong one.
        let Some(counter) = counter_of(entry.mid) else {
            continue;
        };

        // Every message in scope belongs in the final state, whether or not it
        // travels: the state describes what the client will hold afterwards,
        // and a message it already has is still one it holds.
        given.push(GlobRange::single(counter));
        if entry.change_number > 0 {
            numbers.push(GlobRange::single(entry.change_number));
        }

        if seen.contains(&replica, entry.change_number) {
            continue;
        }

        let change_key = Xid::for_counter(replica, entry.change_number);
        changes.push(MessageChange {
            source_key: Xid::for_counter(replica, counter),
            last_modified: entry.delivery_time,
            // The last change was ours, so the key is an XID built from the
            // change number ([MS-OXCFXICS] §2.2.1.2.7).
            change_key: change_key.clone(),
            // One entry, being the change that produced this version. A longer
            // list only arises once a client imports changes of its own, which
            // is upload — not built yet, and an invented history would be a
            // claim about conflicts that never happened.
            predecessors: vec![change_key],
            associated: false,
            mid: entry.mid,
            message_size: entry.size,
            change_number: entry.change_number,
            content: content_of(entry),
            // Recipients come from a message's parsed body, which a contents
            // listing has not read. They arrive with the slice that opens each
            // message rather than being guessed from the display line.
            recipients: Vec::new(),
        });
    }

    let state = FinalState {
        idset_given: IdSet::single(replica, given),
        cnset_seen: IdSet::single(replica, numbers),
        cnset_seen_fai: IdSet::new(),
        cnset_read: IdSet::new(),
    };

    let steps = u16::try_from(changes.len()).unwrap_or(u16::MAX);
    let stream = build(
        request,
        &changes,
        &IdSet::new(),
        &IdSet::new(),
        &IdSet::new(),
        &state,
    );
    (stream, steps)
}

/// The properties a client needs to show a message in a list, serialised as a
/// `propList`.
///
/// The same set a contents table carries, for the same reason: this is what a
/// folder view is drawn from. A body is not here — it belongs to opening the
/// message, and putting every body in the folder's stream would make the first
/// synchronisation the size of the mailbox.
fn content_of(entry: &MessageEntry) -> Vec<u8> {
    let mut w = Writer::new();
    w.string(pid::SUBJECT, &entry.subject);
    w.string(pid::SENDER_NAME, &entry.sender);
    w.time(pid::MESSAGE_DELIVERY_TIME, entry.delivery_time);
    w.int32(
        pid::MESSAGE_FLAGS,
        i32::from_le_bytes(entry.flags.to_le_bytes()),
    );
    w.int32(
        pid::MESSAGE_SIZE,
        i32::try_from(entry.size).unwrap_or(i32::MAX),
    );
    w.boolean(pid::HAS_ATTACHMENTS, entry.has_attachment);
    // Without this a client has no idea what kind of item it is holding and
    // will not render it as mail.
    w.string(pid::MESSAGE_CLASS, "IPM.Note");
    w.finish()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::fasttransfer::{element_end, safe_split, split_tag};
    use crate::ics::GlobRange;
    use crate::sync::{SyncConfigureRequest, extra_flags, send_options, sync_flags};

    const REPLICA: [u8; 16] = [0x0A; 16];

    fn configure(flags: u16, extra: u32) -> SyncConfigureRequest {
        let mut buf = vec![0x70, 0x00, 0x01, 0x02, 0x01];
        buf.push(send_options::UNICODE);
        buf.extend_from_slice(&(flags | sync_flags::UNICODE).to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&extra.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        SyncConfigureRequest::parse(&buf).unwrap().0
    }

    fn a_message(counter: u64, cn: u64) -> MessageChange {
        let mut content = Writer::new();
        content.string(0x0037, "Rechnung für August");
        MessageChange {
            source_key: Xid::for_counter(REPLICA, counter),
            last_modified: 132_000_000_000_000_000,
            change_key: Xid::for_counter(REPLICA, cn),
            predecessors: vec![Xid::for_counter(REPLICA, cn)],
            associated: false,
            mid: counter,
            message_size: 2048,
            change_number: cn,
            content: content.finish(),
            recipients: Vec::new(),
        }
    }

    /// Reads the markers out of a stream, in order, ignoring properties.
    fn markers_of(stream: &[u8]) -> Vec<u32> {
        let mut found = Vec::new();
        let mut at = 0_usize;
        while at < stream.len() {
            let tag =
                u32::from_le_bytes([stream[at], stream[at + 1], stream[at + 2], stream[at + 3]]);
            if marker::is_marker(tag) {
                found.push(tag);
            }
            at = element_end(stream, at).unwrap();
        }
        found
    }

    /// The stream follows the grammar's order, and ends with state then
    /// IncrSyncEnd — the two elements that let a client converge.
    #[test]
    fn the_stream_follows_the_grammar() {
        let request = configure(sync_flags::NORMAL, 0);
        let stream = build(
            &request,
            &[a_message(1024, 5000)],
            &IdSet::new(),
            &IdSet::new(),
            &IdSet::new(),
            &FinalState::default(),
        );

        assert_eq!(
            markers_of(&stream),
            vec![
                marker::INCR_SYNC_CHG,
                marker::INCR_SYNC_MESSAGE,
                marker::INCR_SYNC_STATE_BEGIN,
                marker::INCR_SYNC_STATE_END,
                marker::INCR_SYNC_END,
            ]
        );
    }

    /// A stream without its state would make the client repeat the whole
    /// download on every connection, so its absence is worth its own test.
    #[test]
    fn the_stream_always_carries_a_final_state() {
        let request = configure(sync_flags::NORMAL, 0);
        let stream = build(
            &request,
            &[],
            &IdSet::new(),
            &IdSet::new(),
            &IdSet::new(),
            &FinalState::default(),
        );
        let markers = markers_of(&stream);
        assert!(markers.contains(&marker::INCR_SYNC_STATE_BEGIN));
        assert_eq!(markers.last(), Some(&marker::INCR_SYNC_END));
    }

    /// The five required header properties appear in the fixed order
    /// §2.2.4.3.19 gives, immediately after IncrSyncChg.
    #[test]
    fn the_header_properties_come_in_their_fixed_order() {
        let request = configure(sync_flags::NORMAL, 0);
        let stream = build(
            &request,
            &[a_message(1024, 5000)],
            &IdSet::new(),
            &IdSet::new(),
            &IdSet::new(),
            &FinalState::default(),
        );

        // Collect property ids in order, skipping markers.
        let mut ids = Vec::new();
        let mut at = 0_usize;
        while at < stream.len() {
            let tag =
                u32::from_le_bytes([stream[at], stream[at + 1], stream[at + 2], stream[at + 3]]);
            if !marker::is_marker(tag) {
                ids.push(split_tag(tag).1);
            }
            at = element_end(&stream, at).unwrap();
        }

        assert_eq!(
            &ids[..5],
            &[
                PID_TAG_SOURCE_KEY,
                PID_TAG_LAST_MODIFICATION_TIME,
                PID_TAG_CHANGE_KEY,
                PID_TAG_PREDECESSOR_CHANGE_LIST,
                PID_TAG_ASSOCIATED,
            ]
        );
    }

    /// "MUST be present if and only if" — the conditional header properties
    /// are absent unless the client's extra flags asked for them.
    #[test]
    fn conditional_header_properties_follow_the_extra_flags() {
        let bare = build(
            &configure(sync_flags::NORMAL, 0),
            &[a_message(1024, 5000)],
            &IdSet::new(),
            &IdSet::new(),
            &IdSet::new(),
            &FinalState::default(),
        );
        let all = build(
            &configure(
                sync_flags::NORMAL,
                extra_flags::EID | extra_flags::MESSAGE_SIZE | extra_flags::CN,
            ),
            &[a_message(1024, 5000)],
            &IdSet::new(),
            &IdSet::new(),
            &IdSet::new(),
            &FinalState::default(),
        );

        let has = |stream: &[u8], id: u16, ptype: u16| {
            let tag = crate::fasttransfer::join_tag(ptype, id);
            stream
                .windows(4)
                .any(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]) == tag)
        };

        assert!(!has(&bare, PID_TAG_MID, 0x0014), "Mid sent unasked");
        assert!(
            !has(&bare, PID_TAG_CHANGE_NUMBER, 0x0014),
            "CN sent unasked"
        );
        assert!(has(&all, PID_TAG_MID, 0x0014));
        assert!(has(&all, PID_TAG_CHANGE_NUMBER, 0x0014));
        assert!(has(&all, PID_TAG_MESSAGE_SIZE, 0x0003));
    }

    /// A client that did not ask for deletions is not told about them.
    #[test]
    fn deletions_and_read_state_follow_what_was_asked_for() {
        let deleted = IdSet::single(REPLICA, vec![GlobRange::single(99)]);
        let read = IdSet::single(REPLICA, vec![GlobRange::single(7)]);

        let opted_out = build(
            &configure(sync_flags::NORMAL | sync_flags::NO_DELETIONS, 0),
            &[],
            &deleted,
            &read,
            &IdSet::new(),
            &FinalState::default(),
        );
        assert!(!markers_of(&opted_out).contains(&marker::INCR_SYNC_DEL));
        assert!(!markers_of(&opted_out).contains(&marker::INCR_SYNC_READ));

        let asked = build(
            &configure(sync_flags::NORMAL | sync_flags::READ_STATE, 0),
            &[],
            &deleted,
            &read,
            &IdSet::new(),
            &FinalState::default(),
        );
        let markers = markers_of(&asked);
        assert!(markers.contains(&marker::INCR_SYNC_DEL));
        assert!(markers.contains(&marker::INCR_SYNC_READ));
    }

    /// Recipients are wrapped in their own markers, one pair each.
    #[test]
    fn each_recipient_is_wrapped_in_its_own_markers() {
        let mut change = a_message(1024, 5000);
        for name in ["Anna", "Bob"] {
            let mut w = Writer::new();
            w.string(0x3001, name);
            change.recipients.push(w.finish());
        }

        let stream = build(
            &configure(sync_flags::NORMAL, 0),
            &[change],
            &IdSet::new(),
            &IdSet::new(),
            &IdSet::new(),
            &FinalState::default(),
        );
        let markers = markers_of(&stream);
        assert_eq!(
            markers
                .iter()
                .filter(|m| **m == marker::START_RECIP)
                .count(),
            2
        );
        assert_eq!(
            markers
                .iter()
                .filter(|m| **m == marker::END_TO_RECIP)
                .count(),
            2
        );
    }

    /// The derivation the module documents: the counter bytes inside a
    /// SourceKey must be the same bytes a GLOBSET would encode for that value,
    /// because an IDSET is a compressed set of these very identifiers.
    ///
    /// If this ever fails, a client's own id set and the SourceKeys we send it
    /// describe different messages — silently.
    #[test]
    fn a_source_keys_counter_matches_what_a_globset_encodes() {
        let counter = 0x0000_1234_5678_u64;
        let xid = Xid::for_counter(REPLICA, counter);

        assert_eq!(xid.local_id, globcnt_to_bytes(counter).to_vec());
        assert_eq!(xid.local_id.len(), 6);
        assert_eq!(&xid.serialize()[..16], &REPLICA);
        // Most significant byte first.
        assert_eq!(xid.local_id, vec![0x00, 0x00, 0x12, 0x34, 0x56, 0x78]);

        // And it survives the set round-trip that names the same message.
        let set = IdSet::single(REPLICA, vec![GlobRange::single(counter)]);
        let back = IdSet::parse(&set.serialize()).unwrap();
        assert!(back.contains(&REPLICA, counter));
    }

    /// A SizedXid carries its own length, and a predecessor list is just those
    /// laid end to end.
    #[test]
    fn a_predecessor_list_is_length_prefixed_xids() {
        let one = Xid::for_counter(REPLICA, 1);
        let two = Xid::for_counter(REPLICA, 2);
        let list = predecessor_change_list(&[one.clone(), two.clone()]);

        assert_eq!(list[0], 22, "16 GUID + 6 counter");
        assert_eq!(&list[1..23], one.serialize().as_slice());
        assert_eq!(list[23], 22);
        assert_eq!(&list[24..46], two.serialize().as_slice());
        assert_eq!(list.len(), 46);
        assert!(predecessor_change_list(&[]).is_empty());
    }

    /// Whatever we build must be something the chunker can carry — the two
    /// halves of stage 8 have to agree about the format.
    #[test]
    fn the_stream_survives_being_chunked() {
        let changes: Vec<MessageChange> = (0..8).map(|n| a_message(1024 + n, 5000 + n)).collect();
        let stream = build(
            &configure(sync_flags::NORMAL, extra_flags::EID | extra_flags::CN),
            &changes,
            &IdSet::new(),
            &IdSet::new(),
            &IdSet::new(),
            &FinalState::default(),
        );

        let mut rebuilt = Vec::new();
        let mut sent = 0_usize;
        while sent < stream.len() {
            let next = safe_split(&stream, sent, 64).unwrap();
            assert!(next > sent, "chunking stalled at {sent}");
            rebuilt.extend_from_slice(&stream[sent..next]);
            sent = next;
        }
        assert_eq!(rebuilt, stream);
    }

    fn entry(counter: u64, change_number: u64, subject: &str) -> MessageEntry {
        MessageEntry {
            mid: crate::folders::fid(counter),
            message: alo_store::MessageId::new(format!("m-{counter}")),
            subject: subject.to_owned(),
            sender: "Müller <m@example.test>".to_owned(),
            delivery_time: 132_000_000_000_000_000,
            flags: 0,
            size: 2048,
            has_attachment: false,
            change_number,
        }
    }

    /// A first synchronisation: the client holds nothing, so everything travels
    /// and the final state names all of it.
    #[test]
    fn a_client_that_holds_nothing_is_sent_everything() {
        let request = configure(sync_flags::NORMAL, extra_flags::CN);
        let entries = vec![
            entry(1024, 7, "Rechnung"),
            entry(1025, 8, "Angebot"),
            entry(1026, 9, "Liefertermin"),
        ];

        let (stream, steps) = from_entries(&request, REPLICA, &entries, &IdSet::new());
        assert_eq!(steps, 3);
        assert_eq!(
            markers_of(&stream)
                .iter()
                .filter(|m| **m == marker::INCR_SYNC_CHG)
                .count(),
            3
        );
    }

    /// The point of the whole exercise: a message whose change the client has
    /// already seen does not travel again.
    ///
    /// Without this, every connection re-downloads the mailbox and the state a
    /// client uploads is decoration.
    #[test]
    fn a_change_the_client_has_seen_is_not_sent_again() {
        let request = configure(sync_flags::NORMAL, extra_flags::CN);
        let entries = vec![
            entry(1024, 7, "Rechnung"),
            entry(1025, 8, "Angebot"),
            entry(1026, 9, "Liefertermin"),
        ];

        // The client reports having seen changes 7 and 8.
        let seen = IdSet::single(REPLICA, vec![GlobRange::new(7, 8)]);
        let (stream, steps) = from_entries(&request, REPLICA, &entries, &seen);

        assert_eq!(steps, 1, "only the unseen change should travel");
        assert_eq!(
            markers_of(&stream)
                .iter()
                .filter(|m| **m == marker::INCR_SYNC_CHG)
                .count(),
            1
        );

        // ...and it is the right one.
        let text: Vec<u16> = "Liefertermin".encode_utf16().collect();
        let needle: Vec<u8> = text.iter().flat_map(|u| u.to_le_bytes()).collect();
        assert!(
            stream.windows(needle.len()).any(|w| w == needle),
            "the message that changed was not the one sent"
        );
    }

    /// A message that does not travel is still part of what the client will
    /// hold, so the final state has to name it — otherwise the next
    /// synchronisation would think the client had lost it.
    #[test]
    fn the_final_state_names_messages_that_did_not_travel() {
        let request = configure(sync_flags::NORMAL, extra_flags::CN);
        let entries = vec![entry(1024, 7, "Rechnung"), entry(1025, 8, "Angebot")];
        let seen = IdSet::single(REPLICA, vec![GlobRange::single(7)]);

        let (stream, steps) = from_entries(&request, REPLICA, &entries, &seen);
        assert_eq!(steps, 1);

        // The state must name *both* messages, not just the one that travelled.
        // Walk to `MetaTagIdsetGiven` and decode the set it carries, rather
        // than hunting for bytes: an id set is compressed, so the run
        // 1024..=1025 does not appear literally anywhere in the stream.
        let mut at = 0_usize;
        let mut given: Option<IdSet> = None;
        while at < stream.len() {
            let tag =
                u32::from_le_bytes([stream[at], stream[at + 1], stream[at + 2], stream[at + 3]]);
            if tag == meta::IDSET_GIVEN {
                // type, id, then a 32-bit length, then the set.
                let len = u32::from_le_bytes([
                    stream[at + 4],
                    stream[at + 5],
                    stream[at + 6],
                    stream[at + 7],
                ]) as usize;
                given = Some(IdSet::parse(&stream[at + 8..at + 8 + len]).unwrap());
                break;
            }
            at = element_end(&stream, at).unwrap();
        }

        let given = given.expect("the stream carried no MetaTagIdsetGiven");
        assert!(
            given.contains(&REPLICA, 1024),
            "the message that did travel is missing from the state"
        );
        assert!(
            given.contains(&REPLICA, 1025),
            "a message the client already had was dropped from the state, so the \
             next synchronisation would think it had been lost"
        );
    }

    /// Everything the builder produces must survive the chunker, or the two
    /// halves of stage 8 disagree about the format.
    #[test]
    fn a_built_stream_still_chunks() {
        let request = configure(sync_flags::NORMAL, extra_flags::EID | extra_flags::CN);
        let entries: Vec<MessageEntry> = (0..12)
            .map(|n| entry(1024 + n, 100 + n, "Rechnung für August"))
            .collect();

        let (stream, _) = from_entries(&request, REPLICA, &entries, &IdSet::new());
        let mut sent = 0_usize;
        while sent < stream.len() {
            let next = safe_split(&stream, sent, 96).unwrap();
            assert!(next > sent, "chunking stalled at {sent}");
            sent = next;
        }
        assert_eq!(sent, stream.len());
    }
}
