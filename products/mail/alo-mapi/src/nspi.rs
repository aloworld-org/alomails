//! The address book endpoint ([MS-OXCMAPIHTTP] §2.2.5, [MS-OXNSPI]) — turning
//! a typed name into a recipient.
//!
//! This is a **different protocol from the rest of this crate.** `/mapi/emsmdb`
//! carries ROP buffers; `/mapi/nspi` carries its own request types, named in
//! `X-RequestType` exactly as the mailbox endpoint's are, with bodies of their
//! own shape and no ROP layer underneath. The envelope is shared — the same
//! `PROCESSING`/`DONE` framing, the same `X-ResponseCode` — and nothing else is.
//!
//! ## What is served
//!
//! `Bind`, `Unbind` and `ResolveNames`. That is stage 6's milestone stated as
//! client behaviour: somebody types a colleague's name into the To line and it
//! resolves. Browsing the whole directory (`QueryRows`, `GetSpecialTable`,
//! `SeekEntries`) is a bigger surface and a later stage — it is *refused*
//! rather than half-answered, because a client shown a truncated directory has
//! no way to tell it is truncated.
//!
//! ## Bind
//!
//! Request: `Flags` (4), `HasState` (1), `State` (36, when `HasState`),
//! `AuxiliaryBufferSize` (4), `AuxiliaryBuffer`.
//!
//! Response: `StatusCode` (4, always zero), `ErrorCode` (4), `ServerGuid` (16),
//! `AuxiliaryBufferSize` (4), `AuxiliaryBuffer`.
//!
//! **`StatusCode` is not the error channel.** It is always `0x00000000`; the
//! outcome travels in `ErrorCode`. Two adjacent four-byte fields where only the
//! second carries meaning is exactly the pair to get the wrong way round, and
//! doing so reports every success as a failure or every failure as a success.
//!
//! ## ResolveNames
//!
//! Request: `Reserved` (4), `HasState` (1) + `State` (36),
//! `HasPropertyTags` (1) + a `LargePropertyTagArray`, `HasNames` (1) +
//! `NameCount` (4) + that many null-terminated UTF-16LE strings, then the
//! auxiliary buffer.
//!
//! Response: `StatusCode`, `ErrorCode`, `CodePage`, `HasMinimalIds` (1) +
//! `MinimalIdCount` (4) + that many ids, `HasRowsAndCols` (1) + the property
//! tags + `RowCount` (4) + that many `AddressBookPropertyRow`s, then the
//! auxiliary buffer.
//!
//! The ids array carries the **outcome** of resolving each name, in the order
//! the names were given: [`MID_UNRESOLVED`], [`MID_AMBIGUOUS`] or
//! [`MID_RESOLVED`]. Rows are returned only for the resolved ones, so the row
//! count is normally smaller than the name count, and a client pairs them by
//! walking the outcomes in order.
//!
//! ## Why an ambiguous name is not silently picked
//!
//! When a typed string matches two people, the answer is [`MID_AMBIGUOUS`] and
//! no row. Choosing one would put a colleague's address on a message somebody
//! believed was going elsewhere — the single worst thing an address book can
//! do, and it fails silently at exactly the moment nobody re-reads the To line.

use crate::columns::PropertyTag;
use crate::rop::RopError;
use crate::rows::Value;

/// The size of a `STAT` structure ([MS-OXNSPI] §2.2.8): nine `DWORD`s.
pub const STAT_LEN: usize = 36;

/// A `MinimalEntryID` is a single `DWORD`.
pub const MINIMAL_ENTRY_ID_LEN: usize = 4;

/// ANR outcome — the string matched nothing ([MS-OXNSPI] §2.2.9.1.1).
pub const MID_UNRESOLVED: u32 = 0x0000_0000;
/// ANR outcome — the string matched more than one person.
pub const MID_AMBIGUOUS: u32 = 0x0000_0001;
/// ANR outcome — the string matched exactly one person.
pub const MID_RESOLVED: u32 = 0x0000_0002;

/// `ErrorCode` — the operation succeeded (`Success`, [MS-OXNSPI] §2.2.1.2).
pub const NSPI_SUCCESS: u32 = 0x0000_0000;
/// `ErrorCode` — the server does not implement this call (`NotSupported`).
pub const NSPI_NOT_SUPPORTED: u32 = 0x8004_0102;

/// The code page a response declares for its strings.
///
/// Every string alo returns is `PtypString`, which is UTF-16LE and carries no
/// code page. This is the value that means "Unicode" to a client that insists
/// on being told one.
pub const CODE_PAGE_UNICODE: u32 = 1200;

/// The most names one `ResolveNames` request may carry.
///
/// The count is client-declared, and each name costs a directory lookup.
pub const MAX_NAMES: u32 = 256;

/// The most property tags one request may name.
pub const MAX_TAGS: u32 = 1024;

/// A parsed `Bind` request body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindRequest {
    /// Authentication flags. Everything but `fAnonymousLogin` is ignored by
    /// the specification, and alo authenticates with HTTP Basic before any of
    /// this is read, so the field is carried and not acted on.
    pub flags: u32,
    /// The client's table state, when it sent one.
    pub state: Option<[u8; STAT_LEN]>,
}

impl BindRequest {
    /// Parses the body.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if a declared field runs past the end.
    pub fn parse(input: &[u8]) -> Result<Self, RopError> {
        let mut at = 0usize;
        let flags = read_u32(input, &mut at)?;
        let has_state = read_u8(input, &mut at)?;
        let state = if has_state != 0 {
            Some(read_stat(input, &mut at)?)
        } else {
            None
        };
        // The auxiliary buffer is read to prove the body is well formed and
        // then discarded: it carries client performance telemetry
        // ([MS-OXCRPC] §3.1.4.1.2), which alo neither needs nor stores.
        let aux_len = read_u32(input, &mut at)?;
        skip(input, &mut at, aux_len)?;
        Ok(Self { flags, state })
    }
}

/// Builds a `Bind` success response body.
#[must_use]
pub fn bind_success_body(server_guid: [u8; 16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(28);
    out.extend_from_slice(&0u32.to_le_bytes()); // StatusCode: always zero.
    out.extend_from_slice(&NSPI_SUCCESS.to_le_bytes());
    out.extend_from_slice(&server_guid);
    out.extend_from_slice(&0u32.to_le_bytes()); // AuxiliaryBufferSize
    out
}

/// A parsed `Unbind` request body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnbindRequest;

impl UnbindRequest {
    /// Parses the body.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if a declared field runs past the end.
    pub fn parse(input: &[u8]) -> Result<Self, RopError> {
        let mut at = 0usize;
        let _reserved = read_u32(input, &mut at)?;
        let aux_len = read_u32(input, &mut at)?;
        skip(input, &mut at, aux_len)?;
        Ok(Self)
    }
}

/// Builds an `Unbind` success response body.
///
/// `ErrorCode` here is `Success` even though the session it names may not have
/// existed: unbinding something already gone is not a failure a client can act
/// on, and telling it otherwise only produces a retry loop.
#[must_use]
pub fn unbind_success_body() -> Vec<u8> {
    let mut out = Vec::with_capacity(12);
    out.extend_from_slice(&0u32.to_le_bytes()); // StatusCode: always zero.
    out.extend_from_slice(&NSPI_SUCCESS.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // AuxiliaryBufferSize
    out
}

/// A parsed `ResolveNames` request body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveNamesRequest {
    /// The properties the client wants back for each resolved name.
    pub property_tags: Vec<PropertyTag>,
    /// The strings somebody typed.
    pub names: Vec<String>,
}

impl ResolveNamesRequest {
    /// Parses the body.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if a declared field runs past the end, or a
    /// declared count is beyond [`MAX_NAMES`] or [`MAX_TAGS`].
    pub fn parse(input: &[u8]) -> Result<Self, RopError> {
        let mut at = 0usize;
        let _reserved = read_u32(input, &mut at)?;
        if read_u8(input, &mut at)? != 0 {
            let _state = read_stat(input, &mut at)?;
        }

        let mut property_tags = Vec::new();
        if read_u8(input, &mut at)? != 0 {
            let count = read_u32(input, &mut at)?;
            if count > MAX_TAGS {
                return Err(truncated());
            }
            for _ in 0..count {
                let raw = read_exact::<4>(input, &mut at)?;
                property_tags.push(PropertyTag::from_bytes(raw));
            }
        }

        let mut names = Vec::new();
        if read_u8(input, &mut at)? != 0 {
            let count = read_u32(input, &mut at)?;
            if count > MAX_NAMES {
                return Err(truncated());
            }
            for _ in 0..count {
                names.push(read_utf16_z(input, &mut at)?);
            }
        }

        let aux_len = read_u32(input, &mut at)?;
        skip(input, &mut at, aux_len)?;
        Ok(Self {
            property_tags,
            names,
        })
    }
}

/// One resolved directory entry, ready to be written as a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// What a client displays.
    pub display_name: String,
    /// The SMTP address.
    pub email: String,
}

/// What resolving one typed string produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Nothing matched.
    Unresolved,
    /// More than one person matched, so none is chosen.
    Ambiguous,
    /// Exactly one person matched.
    Resolved(Box<Entry>),
}

/// Builds a `ResolveNames` success response body.
///
/// `outcomes` is in the order the names were given; rows follow for the
/// resolved ones only, in the same order.
#[must_use]
pub fn resolve_names_success_body(
    tags: &[PropertyTag],
    outcomes: &[Resolution],
    value_of: &dyn Fn(&Entry, PropertyTag) -> Option<Value>,
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0u32.to_le_bytes()); // StatusCode: always zero.
    out.extend_from_slice(&NSPI_SUCCESS.to_le_bytes());
    out.extend_from_slice(&CODE_PAGE_UNICODE.to_le_bytes());

    // One outcome per name, in order.
    out.push(0xFF); // HasMinimalIds
    out.extend_from_slice(&u32::try_from(outcomes.len()).unwrap_or(0).to_le_bytes());
    for outcome in outcomes {
        let id = match outcome {
            Resolution::Unresolved => MID_UNRESOLVED,
            Resolution::Ambiguous => MID_AMBIGUOUS,
            Resolution::Resolved(_) => MID_RESOLVED,
        };
        out.extend_from_slice(&id.to_le_bytes());
    }

    // Then the columns, and a row for each resolved name.
    out.push(0xFF); // HasRowsAndCols
    out.extend_from_slice(&u32::try_from(tags.len()).unwrap_or(0).to_le_bytes());
    for tag in tags {
        out.extend_from_slice(&tag.to_bytes());
    }
    let resolved: Vec<&Entry> = outcomes
        .iter()
        .filter_map(|outcome| match outcome {
            Resolution::Resolved(entry) => Some(entry.as_ref()),
            _ => None,
        })
        .collect();
    out.extend_from_slice(&u32::try_from(resolved.len()).unwrap_or(0).to_le_bytes());
    for entry in resolved {
        write_address_book_row(&mut out, tags, entry, value_of);
    }

    out.extend_from_slice(&0u32.to_le_bytes()); // AuxiliaryBufferSize
    out
}

/// Writes one `AddressBookPropertyRow` ([MS-OXCMAPIHTTP] §2.2.1.7).
///
/// The flag byte is `0x00` when every value is present, exactly as a ROP
/// property row's is. A string value is preceded by its own `HasValue` byte —
/// `0xFF` present, `0x00` absent — which is the difference from a ROP row and
/// the thing most easily left out: without it every string in the row is read
/// one byte early.
fn write_address_book_row(
    out: &mut Vec<u8>,
    tags: &[PropertyTag],
    entry: &Entry,
    value_of: &dyn Fn(&Entry, PropertyTag) -> Option<Value>,
) {
    let values: Vec<Option<Value>> = tags.iter().map(|tag| value_of(entry, *tag)).collect();
    let complete = values.iter().all(Option::is_some);
    out.push(u8::from(!complete));
    for (tag, value) in tags.iter().zip(values) {
        let is_string = matches!(
            tag.property_type,
            crate::rows::ptyp::STRING | crate::rows::ptyp::STRING8
        );
        match value {
            Some(value) => {
                if !complete {
                    out.push(0x00); // FlaggedPropertyValue: present.
                }
                if is_string {
                    out.push(0xFF); // HasValue: the value follows.
                }
                value.write(out);
            }
            None => {
                if complete {
                    // Unreachable: `complete` is false when any value is None.
                    continue;
                }
                out.push(0x01); // FlaggedPropertyValue: absent, nothing follows.
            }
        }
    }
}

// ---- little readers, all bounds-checked -----------------------------------

fn truncated() -> RopError {
    RopError::Truncated {
        part: "address book request body",
    }
}

fn read_u8(input: &[u8], at: &mut usize) -> Result<u8, RopError> {
    let byte = *input.get(*at).ok_or_else(truncated)?;
    *at += 1;
    Ok(byte)
}

fn read_u32(input: &[u8], at: &mut usize) -> Result<u32, RopError> {
    Ok(u32::from_le_bytes(read_exact::<4>(input, at)?))
}

fn read_exact<const N: usize>(input: &[u8], at: &mut usize) -> Result<[u8; N], RopError> {
    let slice = input.get(*at..*at + N).ok_or_else(truncated)?;
    let mut out = [0u8; N];
    out.copy_from_slice(slice);
    *at += N;
    Ok(out)
}

fn read_stat(input: &[u8], at: &mut usize) -> Result<[u8; STAT_LEN], RopError> {
    read_exact::<STAT_LEN>(input, at)
}

fn skip(input: &[u8], at: &mut usize, count: u32) -> Result<(), RopError> {
    let count = usize::try_from(count).map_err(|_| truncated())?;
    if input.len() < *at + count {
        return Err(truncated());
    }
    *at += count;
    Ok(())
}

/// Reads a null-terminated UTF-16LE string.
fn read_utf16_z(input: &[u8], at: &mut usize) -> Result<String, RopError> {
    let mut units = Vec::new();
    loop {
        let pair = read_exact::<2>(input, at)?;
        let unit = u16::from_le_bytes(pair);
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    String::from_utf16(&units).map_err(|_| truncated())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{
        BindRequest, CODE_PAGE_UNICODE, Entry, MAX_NAMES, MID_AMBIGUOUS, MID_RESOLVED,
        MID_UNRESOLVED, NSPI_SUCCESS, Resolution, ResolveNamesRequest, STAT_LEN, UnbindRequest,
        bind_success_body, resolve_names_success_body, unbind_success_body,
    };
    use crate::columns::PropertyTag;
    use crate::rows::{Value, ptyp};

    fn utf16z(text: &str) -> Vec<u8> {
        let mut out = Vec::new();
        for unit in text.encode_utf16() {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
        out
    }

    #[test]
    fn a_bind_body_without_state_parses() {
        let mut body = 0x20u32.to_le_bytes().to_vec();
        body.push(0x00); // HasState
        body.extend_from_slice(&0u32.to_le_bytes()); // AuxiliaryBufferSize
        let request = BindRequest::parse(&body).expect("parses");
        assert_eq!(request.flags, 0x20);
        assert_eq!(request.state, None);
    }

    #[test]
    fn a_bind_body_with_state_consumes_exactly_thirty_six_bytes() {
        // The one number in this body that shifts everything after it.
        let mut body = 0u32.to_le_bytes().to_vec();
        body.push(0x01); // HasState
        body.extend_from_slice(&[0x5A; STAT_LEN]);
        body.extend_from_slice(&3u32.to_le_bytes()); // AuxiliaryBufferSize
        body.extend_from_slice(&[1, 2, 3]);
        let request = BindRequest::parse(&body).expect("parses");
        assert_eq!(request.state, Some([0x5A; STAT_LEN]));
    }

    #[test]
    fn a_body_whose_auxiliary_buffer_runs_past_the_end_is_refused() {
        let mut body = 0u32.to_le_bytes().to_vec();
        body.push(0x00);
        body.extend_from_slice(&99u32.to_le_bytes()); // claims 99 bytes
        body.extend_from_slice(&[1, 2, 3]); // has three
        assert!(BindRequest::parse(&body).is_err());
    }

    #[test]
    fn a_bind_response_puts_the_outcome_in_errorcode_not_statuscode() {
        // StatusCode is always zero; the outcome is the *second* field. Getting
        // these round the wrong way reports every success as a failure.
        let body = bind_success_body([0xAB; 16]);
        assert_eq!(&body[0..4], &0u32.to_le_bytes(), "StatusCode");
        assert_eq!(&body[4..8], &NSPI_SUCCESS.to_le_bytes(), "ErrorCode");
        assert_eq!(&body[8..24], &[0xAB; 16], "ServerGuid");
        assert_eq!(&body[24..28], &0u32.to_le_bytes(), "AuxiliaryBufferSize");
        assert_eq!(body.len(), 28);
    }

    #[test]
    fn an_unbind_body_parses_and_answers() {
        let mut body = 0u32.to_le_bytes().to_vec();
        body.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(UnbindRequest::parse(&body), Ok(UnbindRequest));
        let out = unbind_success_body();
        assert_eq!(&out[4..8], &NSPI_SUCCESS.to_le_bytes());
        assert_eq!(out.len(), 12);
    }

    fn resolve_body(names: &[&str], tags: &[PropertyTag]) -> Vec<u8> {
        let mut body = 0u32.to_le_bytes().to_vec();
        body.push(0x00); // HasState
        body.push(0x01); // HasPropertyTags
        body.extend_from_slice(&u32::try_from(tags.len()).unwrap().to_le_bytes());
        for tag in tags {
            body.extend_from_slice(&tag.to_bytes());
        }
        body.push(0x01); // HasNames
        body.extend_from_slice(&u32::try_from(names.len()).unwrap().to_le_bytes());
        for name in names {
            body.extend_from_slice(&utf16z(name));
        }
        body.extend_from_slice(&0u32.to_le_bytes());
        body
    }

    #[test]
    fn a_resolve_body_reads_its_names_and_tags() {
        let tags = [PropertyTag {
            property_type: ptyp::STRING,
            property_id: 0x3001,
        }];
        let body = resolve_body(&["Müller", "liège"], &tags);
        let request = ResolveNamesRequest::parse(&body).expect("parses");
        assert_eq!(request.names, vec!["Müller".to_owned(), "liège".to_owned()]);
        assert_eq!(request.property_tags, tags.to_vec());
    }

    #[test]
    fn an_absurd_name_count_is_refused_before_any_allocation() {
        let mut body = 0u32.to_le_bytes().to_vec();
        body.push(0x00);
        body.push(0x00);
        body.push(0x01);
        body.extend_from_slice(&(MAX_NAMES + 1).to_le_bytes());
        assert!(ResolveNamesRequest::parse(&body).is_err());
    }

    #[test]
    fn a_name_with_no_terminator_is_refused_rather_than_run_off_the_end() {
        let mut body = 0u32.to_le_bytes().to_vec();
        body.push(0x00);
        body.push(0x00);
        body.push(0x01);
        body.extend_from_slice(&1u32.to_le_bytes());
        body.extend_from_slice(&[b'A', 0x00]); // 'A' and then nothing
        assert!(ResolveNamesRequest::parse(&body).is_err());
    }

    fn entry(name: &str, email: &str) -> Entry {
        Entry {
            display_name: name.to_owned(),
            email: email.to_owned(),
        }
    }

    fn answer(entry: &Entry, tag: PropertyTag) -> Option<Value> {
        match tag.property_id {
            0x3001 => Some(Value::String(entry.display_name.clone())),
            0x3003 => Some(Value::String(entry.email.clone())),
            _ => None,
        }
    }

    #[test]
    fn the_outcomes_come_back_in_the_order_the_names_were_given() {
        let tags = [PropertyTag {
            property_type: ptyp::STRING,
            property_id: 0x3001,
        }];
        let outcomes = [
            Resolution::Unresolved,
            Resolution::Resolved(Box::new(entry("Disan", "disan@alo.test"))),
            Resolution::Ambiguous,
        ];
        let body = resolve_names_success_body(&tags, &outcomes, &answer);

        assert_eq!(&body[0..4], &0u32.to_le_bytes(), "StatusCode");
        assert_eq!(&body[4..8], &NSPI_SUCCESS.to_le_bytes(), "ErrorCode");
        assert_eq!(&body[8..12], &CODE_PAGE_UNICODE.to_le_bytes());
        assert_eq!(body[12], 0xFF, "HasMinimalIds");
        assert_eq!(u32::from_le_bytes(body[13..17].try_into().unwrap()), 3);
        assert_eq!(
            u32::from_le_bytes(body[17..21].try_into().unwrap()),
            MID_UNRESOLVED
        );
        assert_eq!(
            u32::from_le_bytes(body[21..25].try_into().unwrap()),
            MID_RESOLVED
        );
        assert_eq!(
            u32::from_le_bytes(body[25..29].try_into().unwrap()),
            MID_AMBIGUOUS
        );

        // One row, for the one resolved name.
        assert_eq!(body[29], 0xFF, "HasRowsAndCols");
        assert_eq!(u32::from_le_bytes(body[30..34].try_into().unwrap()), 1);
        let after_tags = 34 + 4;
        assert_eq!(
            u32::from_le_bytes(body[after_tags..after_tags + 4].try_into().unwrap()),
            1,
            "RowCount"
        );
        let row = &body[after_tags + 4..];
        assert_eq!(row[0], 0x00, "every value present");
        assert_eq!(row[1], 0xFF, "HasValue before a string");
        let units: Vec<u16> = row[2..2 + 10]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(String::from_utf16(&units).unwrap(), "Disan");
    }

    #[test]
    fn an_ambiguous_name_returns_no_row_at_all() {
        // Picking one would put a colleague's address on a message somebody
        // believed was going elsewhere, and nobody re-reads the To line.
        let tags = [PropertyTag {
            property_type: ptyp::STRING,
            property_id: 0x3001,
        }];
        let body = resolve_names_success_body(&tags, &[Resolution::Ambiguous], &answer);
        let after_tags = 21 + 1 + 4 + 4;
        assert_eq!(
            u32::from_le_bytes(body[after_tags..after_tags + 4].try_into().unwrap()),
            0,
            "a row was returned for an ambiguous name"
        );
    }

    #[test]
    fn no_names_is_a_valid_request_and_a_valid_answer() {
        let body = resolve_names_success_body(&[], &[], &answer);
        assert_eq!(u32::from_le_bytes(body[13..17].try_into().unwrap()), 0);
    }
}
