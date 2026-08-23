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

/// Where a FastTransfer stream may legally be cut.
///
/// [MS-OXCFXICS] §2.2.4.1: "If a split is required, the stream MUST be split
/// either between two atoms or at any point inside a `varSizeValue` lexeme. A
/// stream MUST NOT be split within a single atom."
///
/// An atom is a marker, a `propDef`, a fixed-size value, or a length. So the
/// gaps between elements are always legal, and the interior of a variable-size
/// value is legal too — which matters, because a single attachment can be
/// larger than the 64 KiB a response buffer can carry, and only the second rule
/// lets it move at all.
/// Where the chunk starting at `from` may end, taking at most `limit` bytes.
///
/// Returns an absolute offset into `stream`, always **greater than `from`**
/// whenever any legal cut exists, so a caller walking the stream cannot stall.
///
/// `from` is required rather than implied by slicing, because a chunk may end
/// *inside* a value: the remainder is then a run of raw bytes with no element
/// header, and re-scanning it as if it were a fresh stream would read the
/// message's own contents as property tags. Only a scan from the true start
/// knows where the structure is.
///
/// Returns `from` itself when nothing legal fits — the caller must treat that
/// as "this element cannot travel in a buffer this small" and fail the
/// download, not as an empty chunk, because handing back zero bytes forever is
/// how a client hangs instead of erroring.
///
/// # Errors
///
/// Returns [`FtError`] if the stream is not well formed, since a scanner that
/// guessed at a malformed stream would cut inside an atom and produce a chunk
/// the client silently misreads.
pub fn safe_split(stream: &[u8], from: usize, limit: usize) -> Result<usize, FtError> {
    let ceiling = from.saturating_add(limit).min(stream.len());
    if ceiling >= stream.len() {
        return Ok(stream.len());
    }

    let mut best = from;
    let mut at = 0_usize;

    while at < stream.len() {
        if at > ceiling {
            break;
        }
        // The gap before this element is a legal cut, if it is ahead of us.
        if at > best {
            best = at;
        }

        let element = measure(stream, at)?;
        if let Some((start, end)) = element.value_span {
            // Inside a variable-size value every offset is legal, so take the
            // ceiling itself — this is the rule that lets one attachment
            // larger than a whole buffer move at all.
            if start <= ceiling && ceiling <= end && ceiling > from {
                return Ok(ceiling);
            }
        }
        at = element.end;
    }

    if at <= ceiling && at > best {
        best = at;
    }
    Ok(best)
}

/// One element's extent, and where its variable-size value lies if it has one.
struct Element {
    /// One past the element's last byte.
    end: usize,
    /// The half-open range of the raw value bytes, for a variable-size value.
    value_span: Option<(usize, usize)>,
}

/// Measures the element beginning at `at`.
fn measure(stream: &[u8], at: usize) -> Result<Element, FtError> {
    let tag = read_u32(stream, at, "element tag")?;
    if marker::is_marker(tag) {
        return Ok(Element {
            end: at + 4,
            value_span: None,
        });
    }

    let (property_type, property_id) = split_tag(tag);
    let mut cursor = at + 4;

    // A named property carries its set and member before the value; skipping it
    // wrongly would make the value bytes start in the wrong place.
    if property_id >= NAMED_PROP_ID_FLOOR {
        cursor += 16; // the property set GUID
        let kind = *stream.get(cursor).ok_or(FtError::Truncated {
            part: "namedPropInfo kind",
        })?;
        cursor += 1;
        match kind {
            0x00 => cursor += 4, // dispid
            0x01 => cursor = utf16z_end(stream, cursor)?,
            _ => {
                return Err(FtError::Malformed {
                    part: "namedPropInfo kind",
                });
            }
        }
    }

    if let Some(width) = fixed_width(property_type) {
        return Ok(Element {
            end: cursor + width,
            value_span: None,
        });
    }

    if is_multi_valued(property_type) {
        let count = read_u32(stream, cursor, "multi-value count")? as usize;
        cursor += 4;
        let element_type = property_type & 0x0FFF;
        for _ in 0..count {
            if let Some(width) = fixed_width(element_type) {
                cursor += width;
            } else {
                let len = read_u32(stream, cursor, "multi-value element length")? as usize;
                cursor = cursor
                    .checked_add(4 + len)
                    .ok_or(FtError::Malformed { part: "length" })?;
            }
        }
        // The interior of a multi-valued property is a sequence of atoms and
        // values rather than one value, so only its ends are offered here.
        return Ok(Element {
            end: cursor,
            value_span: None,
        });
    }

    // A plain variable-size value: length, then that many bytes.
    let len = read_u32(stream, cursor, "value length")? as usize;
    let start = cursor + 4;
    let end = start
        .checked_add(len)
        .ok_or(FtError::Malformed { part: "length" })?;
    if end > stream.len() {
        return Err(FtError::Truncated { part: "value" });
    }
    Ok(Element {
        end,
        value_span: Some((start, end)),
    })
}

/// The size of a fixed-width value, or `None` if the type is variable.
fn fixed_width(property_type: u16) -> Option<usize> {
    match property_type {
        ptyp::INTEGER16 => Some(2),
        // A boolean is two bytes in a FastTransfer stream, not one.
        ptyp::BOOLEAN => Some(2),
        ptyp::INTEGER32 => Some(4),
        ptyp::INTEGER64 | ptyp::TIME => Some(8),
        ptyp::GUID => Some(16),
        _ => None,
    }
}

/// Whether the type is one of the multi-valued families.
fn is_multi_valued(property_type: u16) -> bool {
    property_type & 0x1000 != 0
}

/// Reads a little-endian `u32`.
fn read_u32(stream: &[u8], at: usize, part: &'static str) -> Result<u32, FtError> {
    let bytes = stream.get(at..at + 4).ok_or(FtError::Truncated { part })?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// One past the terminating null of a UTF-16 string.
fn utf16z_end(stream: &[u8], mut at: usize) -> Result<usize, FtError> {
    loop {
        let pair = stream.get(at..at + 2).ok_or(FtError::Truncated {
            part: "named property name",
        })?;
        at += 2;
        if pair == [0x00, 0x00] {
            return Ok(at);
        }
    }
}

/// What can go wrong scanning a stream.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FtError {
    /// The stream ended inside an element.
    #[error("FastTransfer stream truncated in {part}")]
    Truncated {
        /// Which part ran out.
        part: &'static str,
    },
    /// The stream is structurally impossible.
    #[error("FastTransfer stream malformed at {part}")]
    Malformed {
        /// Which part did not make sense.
        part: &'static str,
    },
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

    /// A limit past the end takes the whole stream.
    #[test]
    fn a_limit_beyond_the_stream_takes_all_of_it() {
        let mut w = Writer::new();
        w.marker(marker::START_MESSAGE);
        w.int32(0x0E07, 1);
        let stream = w.finish();
        assert_eq!(
            safe_split(&stream, 0, stream.len() + 100).unwrap(),
            stream.len()
        );
        assert_eq!(safe_split(&stream, 0, stream.len()).unwrap(), stream.len());
    }

    /// A cut must land between elements, never inside a marker or a fixed value
    /// — an atom split leaves the client reading a property that is not there.
    #[test]
    fn a_cut_lands_between_elements_not_inside_an_atom() {
        let mut w = Writer::new();
        w.marker(marker::START_MESSAGE); // 4 bytes: 0..4
        w.int32(0x0E07, 1); // 8 bytes: 4..12
        w.marker(marker::END_MESSAGE); // 4 bytes: 12..16
        let stream = w.finish();
        assert_eq!(stream.len(), 16);

        // Mid-marker and mid-value limits fall back to the last clean boundary.
        assert_eq!(safe_split(&stream, 0, 2).unwrap(), 0);
        assert_eq!(safe_split(&stream, 0, 4).unwrap(), 4);
        assert_eq!(safe_split(&stream, 0, 7).unwrap(), 4, "cut inside an int32");
        assert_eq!(safe_split(&stream, 0, 11).unwrap(), 4);
        assert_eq!(safe_split(&stream, 0, 12).unwrap(), 12);
        assert_eq!(
            safe_split(&stream, 0, 15).unwrap(),
            12,
            "cut inside a marker"
        );
    }

    /// The rule that makes large attachments possible: inside a variable-size
    /// value, any offset is legal, so the cut is taken exactly at the limit.
    #[test]
    fn a_cut_inside_a_variable_size_value_is_taken_at_the_limit() {
        let mut w = Writer::new();
        w.binary(0x3701, &vec![0xAB; 1000]);
        let stream = w.finish();
        // 2 type + 2 id + 4 length = 8 bytes of header, then 1000 bytes.
        assert_eq!(stream.len(), 1008);

        // Inside the header, fall back to the start.
        assert_eq!(safe_split(&stream, 0, 5).unwrap(), 0);
        // Anywhere in the value itself is legal.
        assert_eq!(safe_split(&stream, 0, 8).unwrap(), 8);
        assert_eq!(safe_split(&stream, 0, 500).unwrap(), 500);
        assert_eq!(safe_split(&stream, 0, 1007).unwrap(), 1007);
    }

    /// A value far larger than any single buffer must still be able to move,
    /// which is the whole reason the second split rule exists.
    #[test]
    fn an_attachment_larger_than_a_buffer_still_moves() {
        let mut w = Writer::new();
        w.binary(0x3701, &vec![0x5A; 200_000]);
        let stream = w.finish();

        // Walk it out in 64 KiB bites, exactly as GetBuffer would.
        let mut sent = 0_usize;
        let mut rounds = 0;
        while sent < stream.len() {
            let next = safe_split(&stream, sent, 65_535).unwrap();
            assert!(next > sent, "made no progress at offset {sent}");
            sent = next;
            rounds += 1;
            assert!(rounds < 100, "did not converge");
        }
        assert_eq!(sent, stream.len());
    }

    /// A string's bytes are a variable-size value too, so the same rule holds.
    #[test]
    fn a_string_value_splits_like_any_other_variable_value() {
        let mut w = Writer::new();
        w.string(0x0037, "Grüße aus Liège");
        let stream = w.finish();
        assert_eq!(safe_split(&stream, 0, 10).unwrap(), 10);
        assert_eq!(safe_split(&stream, 0, 3).unwrap(), 0);
    }

    /// Multi-valued properties are a sequence of atoms, so only their ends are
    /// offered — cutting inside one would leave a half-read element count.
    #[test]
    fn a_multi_valued_property_is_cut_only_at_its_ends() {
        let mut w = Writer::new();
        w.marker(marker::START_RECIP);
        w.multi_int32(0x1014, &[1, 2, 3]);
        w.marker(marker::END_TO_RECIP);
        let stream = w.finish();

        // 4 (marker) + 8 (header+count) + 12 (three values) = 24, then 4.
        assert_eq!(stream.len(), 28);
        assert_eq!(safe_split(&stream, 0, 4).unwrap(), 4);
        assert_eq!(
            safe_split(&stream, 0, 14).unwrap(),
            4,
            "inside the value list"
        );
        assert_eq!(safe_split(&stream, 0, 24).unwrap(), 24);
    }

    /// A named property's set and member sit between the tag and the value, so
    /// a scanner that skipped them would mistake the GUID for a length.
    #[test]
    fn a_named_property_is_measured_past_its_set_and_member() {
        let named = NamedProp {
            property_set: [0x77; 16],
            kind: NamedPropKind::Dispatch(0x8205),
        };
        let mut w = Writer::new();
        w.named_string(0x8001, &named, "hello");
        w.marker(marker::END_MESSAGE);
        let stream = w.finish();

        // The last element is a 4-byte marker, so the boundary before it is
        // len - 4. A scanner that lost its place could not find it.
        let boundary = stream.len() - 4;
        assert_eq!(safe_split(&stream, 0, boundary).unwrap(), boundary);
        assert_eq!(safe_split(&stream, 0, stream.len() - 1).unwrap(), boundary);
    }

    /// A malformed stream is refused rather than cut at a guess — a wrong cut
    /// produces a chunk the client misreads without any error.
    #[test]
    fn a_malformed_stream_is_refused() {
        // A binary property whose length runs past the buffer.
        let mut bad = vec![0x02, 0x01, 0x01, 0x37];
        bad.extend_from_slice(&9999u32.to_le_bytes());
        bad.extend_from_slice(&[0x01, 0x02]);
        assert!(safe_split(&bad, 0, 4).is_err());

        // A tag that stops halfway.
        assert!(safe_split(&[0x02, 0x01], 0, 1).is_err());
    }
}
