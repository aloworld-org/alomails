//! The `RopSynchronizationUploadStateStream` family ([MS-OXCFXICS] §2.2.3.2.2)
//! — how a client tells the server what its replica already holds.
//!
//! Downloading is only half of incremental synchronisation. Before asking what
//! changed, a client uploads its *state*: the ids it holds and the change
//! numbers it has seen. The server answers with the difference.
//!
//! State arrives in three operations, in order and per property:
//!
//! | ROP | Id | |
//! |---|---|---|
//! | `...UploadStateStreamBegin` | `0x75` | names the property and its size |
//! | `...UploadStateStreamContinue` | `0x76` | one piece of the bytes, repeatable |
//! | `...UploadStateStreamEnd` | `0x77` | the property is complete |
//!
//! Only four properties may be uploaded ([MS-OXCFXICS] §2.2.3.2.2.1.1):
//! `MetaTagIdsetGiven`, `MetaTagCnsetSeen`, `MetaTagCnsetSeenFAI` and
//! `MetaTagCnsetRead`. Anything else is refused rather than stored under a tag
//! nothing will ever read.
//!
//! ## This is the untrusted edge
//!
//! Every byte here is chosen by the client, including the declared size. So the
//! size is bounded before anything is allocated, the accumulated bytes are
//! checked against what was declared, and the sets are only parsed once the
//! property is complete. A client that declares one size and sends another has
//! contradicted itself, and the upload is refused rather than reconciled.

use crate::ics::{IcsError, IdSet};
use crate::rop::RopError;

/// The `RopId` of `RopSynchronizationUploadStateStreamBegin`.
pub const ROP_UPLOAD_STATE_BEGIN: u8 = 0x75;
/// The `RopId` of `RopSynchronizationUploadStateStreamContinue`.
pub const ROP_UPLOAD_STATE_CONTINUE: u8 = 0x76;
/// The `RopId` of `RopSynchronizationUploadStateStreamEnd`.
pub const ROP_UPLOAD_STATE_END: u8 = 0x77;

/// The size of any of these operations' success responses.
pub const RESPONSE_LEN: usize = 6;

/// The most state bytes we will hold for one property.
///
/// A state set for a large mailbox compresses to a few kilobytes; a megabyte is
/// far beyond anything real and still cheap to refuse. The declared size is
/// client-chosen, so it is checked against this before a buffer is reserved.
pub const MAX_STATE_BYTES: usize = 1024 * 1024;

/// The four properties a client may upload ([MS-OXCFXICS] §2.2.3.2.2.1.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateProperty {
    /// The ids the client's replica holds.
    IdsetGiven,
    /// Change numbers already seen for normal messages.
    CnsetSeen,
    /// Change numbers already seen for folder-associated messages.
    CnsetSeenFai,
    /// Change numbers whose read state the client has.
    CnsetRead,
}

impl StateProperty {
    /// Recognises a property tag, accepting only the four that are valid here.
    ///
    /// `MetaTagIdsetGiven` is accepted in **both** encodings: [MS-OXCFXICS]
    /// §2.2.1.3 declares it `PtypInteger32` while its value is a
    /// variable-length set, so a client may reasonably send either. See
    /// `docs/interop.md`.
    #[must_use]
    pub fn from_tag(tag: u32) -> Option<Self> {
        use crate::ics::meta;
        match tag {
            meta::IDSET_GIVEN | meta::IDSET_GIVEN_AS_DECLARED => Some(Self::IdsetGiven),
            meta::CNSET_SEEN => Some(Self::CnsetSeen),
            meta::CNSET_SEEN_FAI => Some(Self::CnsetSeenFai),
            meta::CNSET_READ => Some(Self::CnsetRead),
            _ => None,
        }
    }
}

/// A parsed `RopSynchronizationUploadStateStreamBegin` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadBeginRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the synchronisation context.
    pub input_handle_index: u8,
    /// The property being uploaded, as sent.
    pub state_property: u32,
    /// How many bytes the client says it will send.
    pub declared_size: u32,
}

impl UploadBeginRequest {
    /// The recognised property, or `None` if this is not one a client may send.
    #[must_use]
    pub fn property(&self) -> Option<StateProperty> {
        StateProperty::from_tag(self.state_property)
    }

    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    ///
    /// [`RopError::Truncated`] if the buffer ends inside a field, the leading
    /// byte is not this operation, or the declared size exceeds
    /// [`MAX_STATE_BYTES`].
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        const LEN: usize = 11;
        let fixed = input.get(..LEN).ok_or(RopError::Truncated {
            part: "RopSynchronizationUploadStateStreamBegin",
        })?;
        if fixed[0] != ROP_UPLOAD_STATE_BEGIN {
            return Err(RopError::Truncated {
                part: "RopSynchronizationUploadStateStreamBegin",
            });
        }

        let state_property = u32::from_le_bytes([fixed[3], fixed[4], fixed[5], fixed[6]]);
        let declared_size = u32::from_le_bytes([fixed[7], fixed[8], fixed[9], fixed[10]]);

        // Bounded before any buffer is reserved: the number is the client's.
        if declared_size as usize > MAX_STATE_BYTES {
            return Err(RopError::Truncated {
                part: "RopSynchronizationUploadStateStreamBegin TransferBufferSize",
            });
        }

        Ok((
            Self {
                logon_id: fixed[1],
                input_handle_index: fixed[2],
                state_property,
                declared_size,
            },
            &input[LEN..],
        ))
    }
}

/// A parsed `RopSynchronizationUploadStateStreamContinue` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadContinueRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the synchronisation context.
    pub input_handle_index: u8,
    /// This piece of the state.
    pub data: Vec<u8>,
}

impl UploadContinueRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    ///
    /// [`RopError::Truncated`] if the buffer ends inside a field, the leading
    /// byte is not this operation, the declared size is zero (which
    /// [MS-OXCFXICS] §2.2.3.2.2.2.1 forbids), or it exceeds
    /// [`MAX_STATE_BYTES`].
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        const LEN: usize = 7;
        let fixed = input.get(..LEN).ok_or(RopError::Truncated {
            part: "RopSynchronizationUploadStateStreamContinue",
        })?;
        if fixed[0] != ROP_UPLOAD_STATE_CONTINUE {
            return Err(RopError::Truncated {
                part: "RopSynchronizationUploadStateStreamContinue",
            });
        }

        let size = u32::from_le_bytes([fixed[3], fixed[4], fixed[5], fixed[6]]) as usize;
        // "MUST NOT be set to 0x00000000" — a zero-length piece would make no
        // progress, and a client sending them would loop forever.
        if size == 0 || size > MAX_STATE_BYTES {
            return Err(RopError::Truncated {
                part: "RopSynchronizationUploadStateStreamContinue StreamDataSize",
            });
        }

        let data = input
            .get(LEN..LEN + size)
            .ok_or(RopError::Truncated {
                part: "RopSynchronizationUploadStateStreamContinue StreamData",
            })?
            .to_vec();

        Ok((
            Self {
                logon_id: fixed[1],
                input_handle_index: fixed[2],
                data,
            },
            &input[LEN + size..],
        ))
    }
}

/// A parsed `RopSynchronizationUploadStateStreamEnd` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadEndRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the synchronisation context.
    pub input_handle_index: u8,
}

impl UploadEndRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    ///
    /// [`RopError::Truncated`] if the buffer ends inside a field or the leading
    /// byte is not this operation.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        const LEN: usize = 3;
        let fixed = input.get(..LEN).ok_or(RopError::Truncated {
            part: "RopSynchronizationUploadStateStreamEnd",
        })?;
        if fixed[0] != ROP_UPLOAD_STATE_END {
            return Err(RopError::Truncated {
                part: "RopSynchronizationUploadStateStreamEnd",
            });
        }
        Ok((
            Self {
                logon_id: fixed[1],
                input_handle_index: fixed[2],
            },
            &input[LEN..],
        ))
    }
}

/// Builds the six-byte success response these three operations share.
#[must_use]
pub fn upload_success_body(rop_id: u8, input_handle_index: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(RESPONSE_LEN);
    out.push(rop_id);
    out.push(input_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out
}

/// What a client says its replica already holds.
///
/// Empty means "nothing" — a first synchronisation — which is the correct
/// reading and produces a full download rather than an error.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncState {
    /// The ids in the client's replica.
    pub idset_given: IdSet,
    /// Change numbers seen for normal messages.
    pub cnset_seen: IdSet,
    /// Change numbers seen for folder-associated messages.
    pub cnset_seen_fai: IdSet,
    /// Change numbers whose read state the client holds.
    pub cnset_read: IdSet,
}

/// One property's upload, in progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateUpload {
    property: StateProperty,
    declared: usize,
    buffer: Vec<u8>,
}

/// What can go wrong receiving a state upload.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UploadError {
    /// A property tag that may not be uploaded.
    #[error("property {tag:#010X} is not an uploadable ICS state property")]
    UnknownProperty {
        /// The tag the client sent.
        tag: u32,
    },
    /// More bytes arrived than the client said it would send.
    #[error("state upload sent {sent} bytes after declaring {declared}")]
    Overrun {
        /// How much has now arrived.
        sent: usize,
        /// What was promised.
        declared: usize,
    },
    /// The set inside the upload did not parse.
    #[error("state upload did not contain a valid set: {0}")]
    Set(#[from] IcsError),
}

impl StateUpload {
    /// Begins an upload of `property`, expecting `declared` bytes.
    ///
    /// # Errors
    ///
    /// [`UploadError::UnknownProperty`] if the tag is not one of the four a
    /// client may upload.
    pub fn begin(request: &UploadBeginRequest) -> Result<Self, UploadError> {
        let property = request.property().ok_or(UploadError::UnknownProperty {
            tag: request.state_property,
        })?;
        let declared = request.declared_size as usize;
        Ok(Self {
            property,
            declared,
            // Reserving what was declared is safe: `UploadBeginRequest::parse`
            // has already refused anything past `MAX_STATE_BYTES`.
            buffer: Vec::with_capacity(declared.min(MAX_STATE_BYTES)),
        })
    }

    /// Adds one piece of the stream.
    ///
    /// # Errors
    ///
    /// [`UploadError::Overrun`] if this piece would take the total past what
    /// the client declared — a client that contradicts its own header has told
    /// us nothing dependable about where the set ends.
    pub fn extend(&mut self, data: &[u8]) -> Result<(), UploadError> {
        let sent = self.buffer.len() + data.len();
        if sent > self.declared || sent > MAX_STATE_BYTES {
            return Err(UploadError::Overrun {
                sent,
                declared: self.declared,
            });
        }
        self.buffer.extend_from_slice(data);
        Ok(())
    }

    /// Completes the upload, parsing the set into `state`.
    ///
    /// A short upload — fewer bytes than declared — is accepted, because the
    /// declared size is a hint about allocation rather than a promise the
    /// specification makes the client keep, and the set itself is
    /// self-delimiting.
    ///
    /// # Errors
    ///
    /// [`UploadError::Set`] if the bytes are not a well-formed `IDSET`.
    pub fn finish(self, state: &mut SyncState) -> Result<StateProperty, UploadError> {
        let parsed = IdSet::parse(&self.buffer)?;
        match self.property {
            StateProperty::IdsetGiven => state.idset_given = parsed,
            StateProperty::CnsetSeen => state.cnset_seen = parsed,
            StateProperty::CnsetSeenFai => state.cnset_seen_fai = parsed,
            StateProperty::CnsetRead => state.cnset_read = parsed,
        }
        Ok(self.property)
    }

    /// Which property is being uploaded.
    #[must_use]
    pub fn property(&self) -> StateProperty {
        self.property
    }

    /// How many bytes have arrived.
    #[must_use]
    pub fn received(&self) -> usize {
        self.buffer.len()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::ics::{GlobRange, meta};

    fn begin(tag: u32, size: u32) -> Vec<u8> {
        let mut out = vec![ROP_UPLOAD_STATE_BEGIN, 0x00, 0x01];
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out
    }

    fn continue_with(data: &[u8]) -> Vec<u8> {
        let mut out = vec![ROP_UPLOAD_STATE_CONTINUE, 0x00, 0x01];
        out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn the_three_operations_parse_their_fields() {
        let raw = begin(meta::CNSET_SEEN, 40);
        let (b, rest) = UploadBeginRequest::parse(&raw).unwrap();
        assert!(rest.is_empty());
        assert_eq!(b.input_handle_index, 0x01);
        assert_eq!(b.declared_size, 40);
        assert_eq!(b.property(), Some(StateProperty::CnsetSeen));

        let raw = continue_with(&[1, 2, 3]);
        let (c, rest) = UploadContinueRequest::parse(&raw).unwrap();
        assert!(rest.is_empty());
        assert_eq!(c.data, vec![1, 2, 3]);

        let (e, rest) = UploadEndRequest::parse(&[ROP_UPLOAD_STATE_END, 0x00, 0x01, 0xFF]).unwrap();
        assert_eq!(e.input_handle_index, 0x01);
        assert_eq!(rest, &[0xFF], "the next operation was consumed");
    }

    /// A whole state property, uploaded in pieces the way a client sends it.
    #[test]
    fn a_state_property_reassembles_from_its_pieces() {
        let replica = [0x0A_u8; 16];
        let original = IdSet::single(replica, vec![GlobRange::new(1024, 1030)]);
        let bytes = original.serialize();

        let (b, _) =
            UploadBeginRequest::parse(&begin(meta::CNSET_SEEN, bytes.len() as u32)).unwrap();
        let mut upload = StateUpload::begin(&b).unwrap();
        for piece in bytes.chunks(3) {
            let raw = continue_with(piece);
            let (c, _) = UploadContinueRequest::parse(&raw).unwrap();
            upload.extend(&c.data).unwrap();
        }
        assert_eq!(upload.received(), bytes.len());

        let mut state = SyncState::default();
        assert_eq!(upload.finish(&mut state).unwrap(), StateProperty::CnsetSeen);
        assert_eq!(state.cnset_seen, original);
        // The other three are untouched.
        assert_eq!(state.idset_given, IdSet::new());
    }

    /// Both encodings of `MetaTagIdsetGiven` are accepted, because the
    /// specification declares one and the grammar requires the other.
    #[test]
    fn both_forms_of_idset_given_are_accepted() {
        assert_eq!(
            StateProperty::from_tag(meta::IDSET_GIVEN),
            Some(StateProperty::IdsetGiven)
        );
        assert_eq!(
            StateProperty::from_tag(meta::IDSET_GIVEN_AS_DECLARED),
            Some(StateProperty::IdsetGiven)
        );
    }

    /// A property that is not one of the four is refused, not stored somewhere
    /// nothing will read it.
    #[test]
    fn an_unuploadable_property_is_refused() {
        assert_eq!(StateProperty::from_tag(0x0037_001F), None);
        let raw = begin(0x0037_001F, 4);
        let (b, _) = UploadBeginRequest::parse(&raw).unwrap();
        assert!(matches!(
            StateUpload::begin(&b),
            Err(UploadError::UnknownProperty { tag: 0x0037_001F })
        ));
    }

    /// A client that sends more than it declared has contradicted itself.
    #[test]
    fn sending_more_than_was_declared_is_refused() {
        let raw = begin(meta::CNSET_SEEN, 4);
        let (b, _) = UploadBeginRequest::parse(&raw).unwrap();
        let mut upload = StateUpload::begin(&b).unwrap();
        upload.extend(&[1, 2, 3]).unwrap();
        assert_eq!(
            upload.extend(&[4, 5]),
            Err(UploadError::Overrun {
                sent: 5,
                declared: 4
            })
        );
    }

    /// The declared size is bounded before a buffer is reserved for it.
    #[test]
    fn an_absurd_declared_size_is_refused_before_allocating() {
        let buf = begin(meta::CNSET_SEEN, u32::MAX);
        assert!(UploadBeginRequest::parse(&buf).is_err());
    }

    /// §2.2.3.2.2.2.1: a continue of zero bytes is forbidden, and would make no
    /// progress if allowed.
    #[test]
    fn a_zero_length_continue_is_refused() {
        let mut buf = vec![ROP_UPLOAD_STATE_CONTINUE, 0x00, 0x01];
        buf.extend_from_slice(&0u32.to_le_bytes());
        assert!(UploadContinueRequest::parse(&buf).is_err());
    }

    /// A continue whose data runs past the buffer is truncated, not padded.
    #[test]
    fn a_truncated_continue_is_refused() {
        let mut buf = vec![ROP_UPLOAD_STATE_CONTINUE, 0x00, 0x01];
        buf.extend_from_slice(&100u32.to_le_bytes());
        buf.extend_from_slice(&[1, 2, 3]);
        assert!(UploadContinueRequest::parse(&buf).is_err());
    }

    /// A malformed set is refused at the end rather than stored half-read.
    #[test]
    fn a_malformed_set_is_refused_on_completion() {
        let raw = begin(meta::CNSET_SEEN, 20);
        let (b, _) = UploadBeginRequest::parse(&raw).unwrap();
        let mut upload = StateUpload::begin(&b).unwrap();
        // Sixteen bytes of GUID then a command byte that means nothing.
        upload.extend(&[0x01; 16]).unwrap();
        upload.extend(&[0x77]).unwrap();

        let mut state = SyncState::default();
        assert!(matches!(
            upload.finish(&mut state),
            Err(UploadError::Set(_))
        ));
        assert_eq!(state, SyncState::default(), "a bad set changed the state");
    }

    /// An empty state is a first synchronisation, not an error.
    #[test]
    fn an_empty_upload_means_the_client_holds_nothing() {
        let raw = begin(meta::IDSET_GIVEN, 0);
        let (b, _) = UploadBeginRequest::parse(&raw).unwrap();
        let upload = StateUpload::begin(&b).unwrap();
        let mut state = SyncState::default();
        assert_eq!(
            upload.finish(&mut state).unwrap(),
            StateProperty::IdsetGiven
        );
        assert_eq!(state.idset_given, IdSet::new());
    }

    /// Every response is the same six bytes with its own operation's id.
    #[test]
    fn the_responses_carry_their_own_operation_id() {
        for rop in [
            ROP_UPLOAD_STATE_BEGIN,
            ROP_UPLOAD_STATE_CONTINUE,
            ROP_UPLOAD_STATE_END,
        ] {
            let body = upload_success_body(rop, 0x01);
            assert_eq!(body, vec![rop, 0x01, 0x00, 0x00, 0x00, 0x00]);
            assert_eq!(body.len(), RESPONSE_LEN);
        }
    }
}
