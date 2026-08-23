//! FastTransfer streams ([MS-OXCFXICS] §2.2.4) — the byte format that carries
//! a mailbox's contents to a client that wants to keep its own copy.
//!
//! Everything Outlook does in cached mode arrives through this one format: the
//! server serialises folders, messages, recipients and attachments into a flat
//! stream of *markers* and *property values*, and the client replays it into
//! its local store. Stage 8 of [ADR 0051] is built on this module.
//!
//! ## The stream is markers and properties, nothing else
//!
//! ```text
//! stream    = 1*element
//! element   = marker / propValue
//! marker    = PtypInteger32 <from the table in 2.2.4.1.4>
//! ```
//!
//! A marker is a bare 32-bit value and never carries data ([MS-OXCFXICS]
//! §2.2.4.2); it only says "a message starts here", "the folder ends here". A
//! `propValue` carries a property's type, its identity, and its value. Structure
//! comes entirely from the order the two are emitted in, which is why the
//! grammar in §2.2.4.2 is worth reading before changing anything here.
//!
//! ## The length field is 32 bits — and that is *not* what ROP buffers do
//!
//! ```text
//! propValue  = fixedPropType propInfo fixedSizeValue
//! propValue =/ varPropType propInfo length varSizeValue
//! length     = PtypInteger32 <MUST be greater than 0>
//! ```
//!
//! [MS-OXCDATA] §2.11.1 says byte counts for `PtypBinary` are **16 bits wide in
//! the context of ROP buffers**, and 32 bits in other contexts. A FastTransfer
//! stream is one of those other contexts: §2.2.4.1 defines `length` as
//! `PtypInteger32`, for every variable-size value, `PtypBinary` included.
//!
//! This is the single most dangerous difference in the module. The rest of this
//! crate writes property values into ROP buffers with 16-bit binary counts, and
//! reusing that writer here produces a stream that is wrong from its first
//! attachment onward — with no error anywhere, because a client reading a
//! 32-bit length off a 16-bit field simply resynchronises onto garbage. The two
//! writers stay separate for that reason; see [`Writer`].
//!
//! ## Serialisation is [MS-OXCDATA], with exactly two exceptions
//!
//! Per [MS-OXCFXICS] §2.2.4.1.3, values serialise as [MS-OXCDATA] specifies,
//! except:
//!
//! * **`PtypBoolean` is 2 bytes here**, not 1 — `01 00` for true, `00 00` for
//!   false.
//! * **Strings keep their terminating null**, and it counts toward the length.
//!
//! Little-endian throughout.
//!
//! [ADR 0051]: ../../../../docs/decisions/0051-native-outlook-without-manual-setup.md

use crate::columns::PropertyTag;

/// Markers ([MS-OXCFXICS] §2.2.4.1.4).
///
/// Each is a 32-bit value written literally into the stream. They are laid out
/// as property tags whose ids fall in a reserved range, so they can never
/// collide with a real property — but they are not properties and never take a
/// value.
pub mod marker {
    /// Start of a folder's data.
    pub const START_TOP_FLD: u32 = 0x4009_0003;
    /// Start of a subfolder's data.
    pub const START_SUB_FLD: u32 = 0x400A_0003;
    /// End of a folder or subfolder.
    pub const END_FOLDER: u32 = 0x400B_0003;

    /// Start of a normal message.
    pub const START_MESSAGE: u32 = 0x400C_0003;
    /// Start of an FAI (folder-associated) message.
    pub const START_FAI_MSG: u32 = 0x4010_0003;
    /// End of a message.
    pub const END_MESSAGE: u32 = 0x400D_0003;

    /// Start of an embedded message.
    pub const START_EMBED: u32 = 0x4001_0003;
    /// End of an embedded message.
    pub const END_EMBED: u32 = 0x4002_0003;

    /// Start of one recipient.
    pub const START_RECIP: u32 = 0x4003_0003;
    /// End of one recipient.
    pub const END_TO_RECIP: u32 = 0x4004_0003;

    /// Start of one attachment.
    pub const NEW_ATTACH: u32 = 0x4000_0003;
    /// End of one attachment.
    pub const END_ATTACH: u32 = 0x400E_0003;

    /// Start of the ICS information for a changed item.
    pub const INCR_SYNC_CHG: u32 = 0x4012_0003;
    /// Start of the property-group mapping for a partially changed message.
    pub const INCR_SYNC_CHG_PARTIAL: u32 = 0x407D_0003;
    /// Start of the deleted-item data.
    pub const INCR_SYNC_DEL: u32 = 0x4013_0003;
    /// End of the serialised ICS data.
    pub const INCR_SYNC_END: u32 = 0x4014_0003;
    /// Start of the read/unread state changes.
    pub const INCR_SYNC_READ: u32 = 0x402F_0003;
    /// Start of the post-synchronisation state.
    pub const INCR_SYNC_STATE_BEGIN: u32 = 0x403A_0003;
    /// End of the post-synchronisation state.
    pub const INCR_SYNC_STATE_END: u32 = 0x403B_0003;
    /// Start of the total-size progress information.
    pub const INCR_SYNC_PROGRESS_MODE: u32 = 0x4074_000B;
    /// Start of the per-message progress information.
    pub const INCR_SYNC_PROGRESS_PER_MSG: u32 = 0x4075_000B;
    /// Start of a message's data within an ICS download.
    pub const INCR_SYNC_MESSAGE: u32 = 0x4015_0003;
    /// Start of the property-group mapping information.
    pub const INCR_SYNC_GROUP_INFO: u32 = 0x407B_0102;

    /// Start of error data.
    pub const FX_ERROR_INFO: u32 = 0x4018_0003;

    /// Whether `value` is one of the markers above.
    ///
    /// A reader needs this to tell a marker from a property tag, since both
    /// occupy the same four bytes in the same position. Producers do not, but
    /// tests do — a marker constant mistyped into the range of a real property
    /// would otherwise be caught only by a client that stops working.
    #[must_use]
    pub fn is_marker(value: u32) -> bool {
        ALL.contains(&value)
    }

    /// Every marker, for lookup and for tests.
    pub const ALL: [u32; 23] = [
        START_TOP_FLD,
        START_SUB_FLD,
        END_FOLDER,
        START_MESSAGE,
        START_FAI_MSG,
        END_MESSAGE,
        START_EMBED,
        END_EMBED,
        START_RECIP,
        END_TO_RECIP,
        NEW_ATTACH,
        END_ATTACH,
        INCR_SYNC_CHG,
        INCR_SYNC_CHG_PARTIAL,
        INCR_SYNC_DEL,
        INCR_SYNC_END,
        INCR_SYNC_READ,
        INCR_SYNC_STATE_BEGIN,
        INCR_SYNC_STATE_END,
        INCR_SYNC_PROGRESS_MODE,
        INCR_SYNC_PROGRESS_PER_MSG,
        INCR_SYNC_MESSAGE,
        INCR_SYNC_GROUP_INFO,
    ];
}

/// Property types this module can serialise ([MS-OXCDATA] §2.11.1).
pub mod ptyp {
    /// 16-bit integer.
    pub const INTEGER16: u16 = 0x0002;
    /// 32-bit integer.
    pub const INTEGER32: u16 = 0x0003;
    /// 64-bit integer.
    pub const INTEGER64: u16 = 0x0014;
    /// Boolean — **two** bytes in a FastTransfer stream.
    pub const BOOLEAN: u16 = 0x000B;
    /// 64-bit `FILETIME`.
    pub const TIME: u16 = 0x0040;
    /// UTF-16LE string with a terminating null.
    pub const STRING: u16 = 0x001F;
    /// Byte string.
    pub const STRING8: u16 = 0x001E;
    /// Arbitrary bytes.
    pub const BINARY: u16 = 0x0102;
    /// A GUID.
    pub const GUID: u16 = 0x0048;
    /// Multi-valued binary.
    pub const MULTIPLE_BINARY: u16 = 0x1102;
    /// Multi-valued string.
    pub const MULTIPLE_STRING: u16 = 0x101F;
    /// Multi-valued 32-bit integer.
    pub const MULTIPLE_INTEGER32: u16 = 0x1003;
}

/// The lowest property id that denotes a named property ([MS-OXCFXICS]
/// §2.2.4.1: `namedPropId` is "greater or equal to 0x8000").
pub const NAMED_PROP_ID_FLOOR: u16 = 0x8000;

/// How a named property is identified within its property set
/// ([MS-OXCFXICS] §2.2.4.1, `namedPropInfo`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamedPropKind {
    /// Identified by a numeric dispatch id — `%x00 dispid`.
    Dispatch(u32),
    /// Identified by a name — `%x01 name`.
    Name(String),
}

/// A named property's full identity: the set it belongs to, and which member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedProp {
    /// The property set GUID, serialised in little-endian `Data1`/`2`/`3` form.
    pub property_set: [u8; 16],
    /// Which member of that set.
    pub kind: NamedPropKind,
}

/// Builds a FastTransfer stream.
///
/// Deliberately separate from the ROP-buffer writers elsewhere in this crate:
/// the length discipline differs (see the module documentation), and a shared
/// writer would make the difference invisible at the call site.
#[derive(Debug, Default)]
pub struct Writer {
    out: Vec<u8>,
}

impl Writer {
    /// A new, empty stream.
    #[must_use]
    pub fn new() -> Self {
        Self { out: Vec::new() }
    }

    /// The bytes written so far.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.out
    }

    /// Consumes the writer and returns the stream.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        self.out
    }

    /// How many bytes have been written.
    #[must_use]
    pub fn len(&self) -> usize {
        self.out.len()
    }

    /// Whether nothing has been written yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.out.is_empty()
    }

    /// Writes a marker ([MS-OXCFXICS] §2.2.4.1.4) — four bytes, no value.
    pub fn marker(&mut self, marker: u32) {
        self.out.extend_from_slice(&marker.to_le_bytes());
    }

    /// Writes the `propType propInfo` prefix shared by every property value.
    ///
    /// Type first, then id — the same order [MS-OXCDATA] §2.9 gives for a
    /// property tag, and the same order the rest of this crate uses. A named
    /// property id (`>= 0x8000`) is followed by the set GUID and the member.
    fn prop_def(&mut self, property_type: u16, property_id: u16, named: Option<&NamedProp>) {
        self.out.extend_from_slice(&property_type.to_le_bytes());
        self.out.extend_from_slice(&property_id.to_le_bytes());
        if property_id < NAMED_PROP_ID_FLOOR {
            return;
        }
        // A named id without its `namedPropInfo` would desynchronise every
        // later element, so an id in the named range always writes one.
        let Some(named) = named else {
            return;
        };
        self.out.extend_from_slice(&named.property_set);
        match &named.kind {
            NamedPropKind::Dispatch(dispid) => {
                self.out.push(0x00);
                self.out.extend_from_slice(&dispid.to_le_bytes());
            }
            NamedPropKind::Name(name) => {
                self.out.push(0x01);
                self.out.extend_from_slice(&utf16_with_null(name));
            }
        }
    }

    /// Writes the 32-bit `length` field ([MS-OXCFXICS] §2.2.4.1).
    ///
    /// Four bytes, never two. See the module documentation for why that
    /// distinction is the most dangerous one here.
    fn length(&mut self, len: usize) {
        let len = u32::try_from(len).unwrap_or(u32::MAX);
        self.out.extend_from_slice(&len.to_le_bytes());
    }

    /// A 16-bit integer property.
    pub fn int16(&mut self, property_id: u16, value: i16) {
        self.prop_def(ptyp::INTEGER16, property_id, None);
        self.out.extend_from_slice(&value.to_le_bytes());
    }

    /// A 32-bit integer property.
    pub fn int32(&mut self, property_id: u16, value: i32) {
        self.prop_def(ptyp::INTEGER32, property_id, None);
        self.out.extend_from_slice(&value.to_le_bytes());
    }

    /// A 64-bit integer property.
    pub fn int64(&mut self, property_id: u16, value: i64) {
        self.prop_def(ptyp::INTEGER64, property_id, None);
        self.out.extend_from_slice(&value.to_le_bytes());
    }

    /// A boolean property — **two** bytes ([MS-OXCFXICS] §2.2.4.1.3).
    pub fn boolean(&mut self, property_id: u16, value: bool) {
        self.prop_def(ptyp::BOOLEAN, property_id, None);
        self.out.extend_from_slice(&u16::from(value).to_le_bytes());
    }

    /// A `FILETIME` property — 100-nanosecond intervals since 1601-01-01 UTC.
    pub fn time(&mut self, property_id: u16, filetime: u64) {
        self.prop_def(ptyp::TIME, property_id, None);
        self.out.extend_from_slice(&filetime.to_le_bytes());
    }

    /// A GUID property.
    pub fn guid(&mut self, property_id: u16, guid: [u8; 16]) {
        self.prop_def(ptyp::GUID, property_id, None);
        self.out.extend_from_slice(&guid);
    }

    /// A Unicode string property.
    ///
    /// The terminating null is written and counted, per [MS-OXCFXICS]
    /// §2.2.4.1.3: readers "MUST check that the last ... 2 bytes ... are indeed
    /// zeros before truncating them", which only works if we send them.
    pub fn string(&mut self, property_id: u16, value: &str) {
        self.prop_def(ptyp::STRING, property_id, None);
        let encoded = utf16_with_null(value);
        self.length(encoded.len());
        self.out.extend_from_slice(&encoded);
    }

    /// A binary property.
    ///
    /// The length is 32 bits here even though the same property in a ROP buffer
    /// takes 16 ([MS-OXCDATA] §2.11.1).
    pub fn binary(&mut self, property_id: u16, value: &[u8]) {
        self.prop_def(ptyp::BINARY, property_id, None);
        self.length(value.len());
        self.out.extend_from_slice(value);
    }

    /// A binary property carrying a tag whose type is already known.
    ///
    /// Used by the ICS state, whose meta-properties are binary but are named by
    /// full 32-bit tags rather than by bare ids.
    pub fn binary_tag(&mut self, tag: u32, value: &[u8]) {
        let (property_type, property_id) = split_tag(tag);
        self.prop_def(property_type, property_id, None);
        self.length(value.len());
        self.out.extend_from_slice(value);
    }

    /// A 32-bit integer carrying a full tag rather than a bare id.
    pub fn int32_tag(&mut self, tag: u32, value: i32) {
        let (property_type, property_id) = split_tag(tag);
        self.prop_def(property_type, property_id, None);
        self.out.extend_from_slice(&value.to_le_bytes());
    }

    /// A named property whose value is a string.
    pub fn named_string(&mut self, property_id: u16, named: &NamedProp, value: &str) {
        self.prop_def(ptyp::STRING, property_id, Some(named));
        let encoded = utf16_with_null(value);
        self.length(encoded.len());
        self.out.extend_from_slice(&encoded);
    }

    /// A multi-valued binary property.
    ///
    /// The first `length` is the **element count**, not a byte count, and each
    /// element then carries its own 32-bit length — `mvPropType propInfo length
    /// *( fixedSizeValue / length varSizeValue )` ([MS-OXCFXICS] §2.2.4.1).
    pub fn multi_binary(&mut self, property_id: u16, values: &[Vec<u8>]) {
        self.prop_def(ptyp::MULTIPLE_BINARY, property_id, None);
        self.length(values.len());
        for value in values {
            self.length(value.len());
            self.out.extend_from_slice(value);
        }
    }

    /// A multi-valued string property.
    pub fn multi_string(&mut self, property_id: u16, values: &[String]) {
        self.prop_def(ptyp::MULTIPLE_STRING, property_id, None);
        self.length(values.len());
        for value in values {
            let encoded = utf16_with_null(value);
            self.length(encoded.len());
            self.out.extend_from_slice(&encoded);
        }
    }

    /// A multi-valued 32-bit integer property.
    ///
    /// The elements are fixed-size, so only the count is written — no per-value
    /// length.
    pub fn multi_int32(&mut self, property_id: u16, values: &[i32]) {
        self.prop_def(ptyp::MULTIPLE_INTEGER32, property_id, None);
        self.length(values.len());
        for value in values {
            self.out.extend_from_slice(&value.to_le_bytes());
        }
    }

    /// Appends an already-serialised fragment.
    pub fn raw(&mut self, bytes: &[u8]) {
        self.out.extend_from_slice(bytes);
    }
}

/// Splits a 32-bit property tag into its type and id.
///
/// A tag is `id << 16 | type`, so the type is the low half — the same layout
/// the wire uses, written type-first.
#[must_use]
pub fn split_tag(tag: u32) -> (u16, u16) {
    #[allow(clippy::cast_possible_truncation)]
    let property_type = (tag & 0xFFFF) as u16;
    #[allow(clippy::cast_possible_truncation)]
    let property_id = (tag >> 16) as u16;
    (property_type, property_id)
}

/// Joins a type and id into a 32-bit property tag.
#[must_use]
pub fn join_tag(property_type: u16, property_id: u16) -> u32 {
    (u32::from(property_id) << 16) | u32::from(property_type)
}

/// A property tag as this crate's ROP layer models it, as a 32-bit tag.
#[must_use]
pub fn tag_of(tag: PropertyTag) -> u32 {
    join_tag(tag.property_type, tag.property_id)
}

/// UTF-16LE bytes with the terminating null included.
#[must_use]
fn utf16_with_null(value: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len() * 2 + 2);
    for unit in value.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out.extend_from_slice(&[0x00, 0x00]);
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The one difference that would silently corrupt every stream: a binary
    /// value's length is four bytes here, where a ROP buffer writes two.
    #[test]
    fn binary_length_is_thirty_two_bits() {
        let mut w = Writer::new();
        w.binary(0x0FF9, &[0xAA, 0xBB, 0xCC]);

        // type, id, then a 4-byte length, then the bytes.
        assert_eq!(
            w.as_bytes(),
            &[
                0x02, 0x01, // PtypBinary
                0xF9, 0x0F, // property id
                0x03, 0x00, 0x00, 0x00, // length: 32 bits
                0xAA, 0xBB, 0xCC,
            ]
        );
        assert_eq!(w.len(), 11, "a 16-bit length would make this 9");
    }

    /// [MS-OXCFXICS] §2.2.4.1.3: two bytes, not the one [MS-OXCDATA] gives.
    #[test]
    fn boolean_is_two_bytes() {
        let mut w = Writer::new();
        w.boolean(0x0E07, true);
        assert_eq!(w.as_bytes(), &[0x0B, 0x00, 0x07, 0x0E, 0x01, 0x00]);

        let mut w = Writer::new();
        w.boolean(0x0E07, false);
        assert_eq!(w.as_bytes(), &[0x0B, 0x00, 0x07, 0x0E, 0x00, 0x00]);
    }

    /// The terminating null is written *and* counted — a reader is required to
    /// find zeros in the last two bytes.
    #[test]
    fn string_keeps_and_counts_its_terminating_null() {
        let mut w = Writer::new();
        w.string(0x0037, "Hi");

        assert_eq!(
            w.as_bytes(),
            &[
                0x1F, 0x00, // PtypString
                0x37, 0x00, // PidTagSubject
                0x06, 0x00, 0x00, 0x00, // 2 chars + null = 6 bytes
                b'H', 0x00, b'i', 0x00, 0x00, 0x00,
            ]
        );
    }

    /// Non-ASCII is a European product's normal case, not an edge case.
    #[test]
    fn string_is_utf16_little_endian() {
        let mut w = Writer::new();
        w.string(0x0037, "Liège");
        let bytes = w.as_bytes();

        let len = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        assert_eq!(len, 12, "5 characters plus the null, two bytes each");
        // 'è' is U+00E8, little-endian.
        assert!(
            bytes.windows(2).any(|pair| pair == [0xE8, 0x00]),
            "{bytes:02X?}"
        );
    }

    /// A multi-valued property writes the element count first, then a length
    /// per element — not one byte count for the whole thing.
    #[test]
    fn multi_binary_counts_elements_then_each_length() {
        let mut w = Writer::new();
        w.multi_binary(0x1013, &[vec![0x01], vec![0x02, 0x03]]);

        assert_eq!(
            w.as_bytes(),
            &[
                0x02, 0x11, // PtypMultipleBinary
                0x13, 0x10, // property id
                0x02, 0x00, 0x00, 0x00, // two elements
                0x01, 0x00, 0x00, 0x00, 0x01, // first: 1 byte
                0x02, 0x00, 0x00, 0x00, 0x02, 0x03, // second: 2 bytes
            ]
        );
    }

    /// Fixed-size elements carry no per-element length.
    #[test]
    fn multi_int32_writes_only_the_count() {
        let mut w = Writer::new();
        w.multi_int32(0x1014, &[7, 9]);

        assert_eq!(
            w.as_bytes(),
            &[
                0x03, 0x10, // PtypMultipleInteger32
                0x14, 0x10, // property id
                0x02, 0x00, 0x00, 0x00, // two elements
                0x07, 0x00, 0x00, 0x00, //
                0x09, 0x00, 0x00, 0x00,
            ]
        );
    }

    /// A marker is four bytes and carries nothing.
    #[test]
    fn markers_are_bare_four_byte_values() {
        let mut w = Writer::new();
        w.marker(marker::START_MESSAGE);
        w.marker(marker::END_MESSAGE);
        assert_eq!(
            w.as_bytes(),
            &[0x03, 0x00, 0x0C, 0x40, 0x03, 0x00, 0x0D, 0x40]
        );
    }

    /// Every marker must be distinct — a duplicated constant would make two
    /// structural events indistinguishable, and the stream would parse into the
    /// wrong shape rather than fail.
    #[test]
    fn markers_are_all_distinct() {
        let unique: std::collections::HashSet<u32> = marker::ALL.iter().copied().collect();
        assert_eq!(unique.len(), marker::ALL.len(), "a marker value repeats");
    }

    /// Markers live in a reserved id range so they can never be mistaken for a
    /// property ([MS-OXCFXICS] §2.2.4.1.4).
    #[test]
    fn markers_sit_in_the_reserved_id_range() {
        for value in marker::ALL {
            let (_, id) = split_tag(value);
            assert!(
                (0x4000..=0x40FF).contains(&id),
                "{value:#010X} has id {id:#06X}, outside the reserved range"
            );
            assert!(marker::is_marker(value));
        }
        assert!(
            !marker::is_marker(0x0037_001F),
            "PidTagSubject is not a marker"
        );
    }

    /// A named property carries its set and member after the id; without them a
    /// reader loses its place for the rest of the stream.
    #[test]
    fn named_property_carries_its_set_and_dispatch_id() {
        let named = NamedProp {
            property_set: [0x11; 16],
            kind: NamedPropKind::Dispatch(0x8205),
        };
        let mut w = Writer::new();
        w.named_string(0x8001, &named, "x");

        let bytes = w.as_bytes();
        assert_eq!(&bytes[0..2], &[0x1F, 0x00], "type");
        assert_eq!(&bytes[2..4], &[0x01, 0x80], "named id");
        assert_eq!(&bytes[4..20], &[0x11; 16], "property set");
        assert_eq!(bytes[20], 0x00, "dispatch-id form");
        assert_eq!(&bytes[21..25], &0x8205_u32.to_le_bytes(), "dispid");
    }

    /// A tag is id-over-type; getting this backwards yields a valid-looking tag
    /// for a property nobody asked about.
    #[test]
    fn tags_split_and_join_consistently() {
        assert_eq!(split_tag(0x0037_001F), (0x001F, 0x0037));
        assert_eq!(join_tag(0x001F, 0x0037), 0x0037_001F);
        assert_eq!(split_tag(marker::START_TOP_FLD), (0x0003, 0x4009));
    }
}
