//! Dispatch for the synchronisation operations (ADR 0051, stage 8).
//!
//! Kept out of [`crate::dispatch`] deliberately. That module is already the
//! largest in the crate and its one reason to change is the ordinary
//! folder-and-message conversation; synchronisation is a second conversation
//! with its own object, its own lifecycle and its own reasons to change, so it
//! gets its own file (CLAUDE.md, law 3).
//!
//! ## How this fits the rehearsal
//!
//! Dispatch runs under a lock, touches no store, and is **rehearsed**: the
//! router clones the object table, walks the buffer against it to learn what to
//! load, throws the clone away, loads, and dispatches for real. So nothing here
//! reads a database or writes one. A `RopSynchronizationConfigure` records
//! *that a stream is wanted* in [`SyncWant`]; the router builds it and offers it
//! back through [`SyncStreams`] on the next pass.
//!
//! That is the same shape `RopGetContentsTable` already uses for a folder's
//! messages, and it is why a configure against an empty [`SyncStreams`] still
//! succeeds — it has to, or the rehearsal would stop before reaching the
//! operations behind it.

use crate::dispatch::{ObjectTable, ServerObject, error};
use crate::getbuffer::{
    Download, GetBufferRequest, ROP_FAST_TRANSFER_SOURCE_GET_BUFFER, TransferStatus,
    get_buffer_body,
};
use crate::rop::{HANDLE_UNSET, RopHeader};
use crate::sync::{
    ROP_SYNCHRONIZATION_CONFIGURE, SyncConfigureRequest, SyncType, configure_success_body,
};
use crate::upload::{
    ROP_UPLOAD_STATE_BEGIN, ROP_UPLOAD_STATE_CONTINUE, ROP_UPLOAD_STATE_END, StateUpload,
    UploadBeginRequest, UploadContinueRequest, UploadEndRequest, upload_success_body,
};

/// A synchronisation stream this buffer asked for.
///
/// Recorded whether or not the stream was available, so a rehearsal against an
/// empty [`SyncStreams`] still reports what it *would* have read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncWant {
    /// The folder being synchronised.
    pub folder_id: u64,
    /// Its contents, or the hierarchy beneath it.
    pub sync_type: SyncType,
}

/// The streams the router has built for this request.
#[derive(Debug, Default, Clone)]
pub struct SyncStreams {
    built: Vec<(SyncWant, Download)>,
}

impl SyncStreams {
    /// No streams — what a rehearsal runs against.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a stream the router built.
    pub fn insert(&mut self, want: SyncWant, download: Download) {
        self.built.retain(|(existing, _)| *existing != want);
        self.built.push((want, download));
    }

    /// The stream for `want`, rewound to its start.
    ///
    /// Rewound rather than moved, because one request may configure the same
    /// scope more than once and each context begins at the beginning.
    #[must_use]
    pub fn get(&self, want: &SyncWant) -> Option<Download> {
        self.built
            .iter()
            .find(|(existing, _)| existing == want)
            .map(|(_, download)| download.restart())
    }

    /// Whether nothing has been built.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.built.is_empty()
    }
}

/// One synchronisation operation's answer.
pub struct SyncOutcome {
    /// The bytes to append to the response.
    pub response: Vec<u8>,
    /// How many bytes of the request buffer this operation used.
    pub consumed: usize,
}

/// Answers a synchronisation operation, or returns `None` if this is not one.
///
/// Returning `None` rather than an error is what lets the caller keep its own
/// match exhaustive over the operations it owns: this module claims five
/// opcodes and declines everything else.
#[must_use]
pub fn try_dispatch(
    header: &RopHeader,
    rest: &[u8],
    objects: &mut ObjectTable,
    handles: &mut Vec<u32>,
    streams: &SyncStreams,
    wants: &mut Vec<SyncWant>,
) -> Option<SyncOutcome> {
    match header.rop_id {
        ROP_SYNCHRONIZATION_CONFIGURE => {
            Some(configure(header, rest, objects, handles, streams, wants))
        }
        ROP_FAST_TRANSFER_SOURCE_GET_BUFFER => Some(get_buffer(header, rest, objects, handles)),
        ROP_UPLOAD_STATE_BEGIN => Some(upload_begin(header, rest, objects, handles)),
        ROP_UPLOAD_STATE_CONTINUE => Some(upload_continue(header, rest, objects, handles)),
        ROP_UPLOAD_STATE_END => Some(upload_end(header, rest, objects, handles)),
        _ => None,
    }
}

/// `RopSynchronizationConfigure` — opens a download context on a folder.
fn configure(
    header: &RopHeader,
    rest: &[u8],
    objects: &mut ObjectTable,
    handles: &mut Vec<u32>,
    streams: &SyncStreams,
    wants: &mut Vec<SyncWant>,
) -> SyncOutcome {
    let Ok((request, tail)) = SyncConfigureRequest::parse(rest) else {
        return refuse(
            ROP_SYNCHRONIZATION_CONFIGURE,
            header.input_handle_index,
            error::INVALID_OBJECT,
            rest.len(),
        );
    };
    let consumed = rest.len() - tail.len();

    // A client that contradicts itself about Unicode has told us nothing
    // dependable about what it can read, and §2.2.3.2.1.1.1 requires the two
    // declarations to match. Guessing which one it meant would produce a stream
    // it may not be able to decode.
    if !request.unicode_is_consistent() {
        return refuse(
            ROP_SYNCHRONIZATION_CONFIGURE,
            request.output_handle_index,
            error::NOT_IMPLEMENTED,
            consumed,
        );
    }

    // Configured **on a folder**, like a contents table. A logon handle is not
    // a folder, and answering anyway would hand back a context describing
    // nothing that the client would then synchronise from.
    let opened = handles
        .get(usize::from(request.input_handle_index))
        .copied()
        .filter(|handle| *handle != HANDLE_UNSET)
        .and_then(|handle| objects.get(handle));
    let Some(ServerObject::Folder { folder_id }) = opened else {
        return refuse(
            ROP_SYNCHRONIZATION_CONFIGURE,
            request.output_handle_index,
            error::INVALID_OBJECT,
            consumed,
        );
    };
    let folder_id = *folder_id;

    let want = SyncWant {
        folder_id,
        sync_type: request.sync_type,
    };
    if !wants.contains(&want) {
        wants.push(want);
    }

    // Absent during a rehearsal, present on the pass that matters. A context
    // with no stream answers its first GetBuffer as finished rather than
    // failing, so the rehearsal walks the whole buffer.
    let download = streams.get(&want);
    let output_handle_index = request.output_handle_index;
    let handle = objects.insert(ServerObject::SyncContext {
        folder_id,
        request: Box::new(request),
        download,
        upload: None,
        state: crate::upload::SyncState::default(),
    });
    assign(handles, output_handle_index, handle);

    SyncOutcome {
        response: configure_success_body(output_handle_index),
        consumed,
    }
}

/// `RopFastTransferSourceGetBuffer` — hands over the next chunk.
fn get_buffer(
    header: &RopHeader,
    rest: &[u8],
    objects: &mut ObjectTable,
    handles: &[u32],
) -> SyncOutcome {
    let Ok((request, tail)) = GetBufferRequest::parse(rest) else {
        return refuse(
            ROP_FAST_TRANSFER_SOURCE_GET_BUFFER,
            header.input_handle_index,
            error::INVALID_OBJECT,
            rest.len(),
        );
    };
    let consumed = rest.len() - tail.len();
    let index = request.input_handle_index;

    let Some(ServerObject::SyncContext { download, .. }) = resolve_mut(objects, handles, index)
    else {
        return refuse(
            ROP_FAST_TRANSFER_SOURCE_GET_BUFFER,
            index,
            error::INVALID_OBJECT,
            consumed,
        );
    };

    let Some(download) = download.as_mut() else {
        // Nothing built for this scope. Saying "done" with an empty chunk is
        // the honest answer during a rehearsal and cannot mislead a real
        // client, which only reaches here if the router found nothing to send.
        return SyncOutcome {
            response: get_buffer_body(index, TransferStatus::Done, (0, 0), &[]),
            consumed,
        };
    };

    match download.next_chunk(request.limit()) {
        Ok((chunk, status)) => SyncOutcome {
            response: get_buffer_body(index, status, download.progress(), &chunk),
            consumed,
        },
        // A stream we built that we cannot chunk is our bug, not the client's
        // — report it rather than looping on an empty buffer.
        Err(_) => SyncOutcome {
            response: get_buffer_body(index, TransferStatus::Error, (0, 0), &[]),
            consumed,
        },
    }
}

/// `RopSynchronizationUploadStateStreamBegin`.
fn upload_begin(
    header: &RopHeader,
    rest: &[u8],
    objects: &mut ObjectTable,
    handles: &[u32],
) -> SyncOutcome {
    let Ok((request, tail)) = UploadBeginRequest::parse(rest) else {
        return refuse(
            ROP_UPLOAD_STATE_BEGIN,
            header.input_handle_index,
            error::INVALID_OBJECT,
            rest.len(),
        );
    };
    let consumed = rest.len() - tail.len();
    let index = request.input_handle_index;

    let Some(ServerObject::SyncContext { upload, .. }) = resolve_mut(objects, handles, index)
    else {
        return refuse(
            ROP_UPLOAD_STATE_BEGIN,
            index,
            error::INVALID_OBJECT,
            consumed,
        );
    };

    match StateUpload::begin(&request) {
        Ok(started) => {
            *upload = Some(started);
            SyncOutcome {
                response: upload_success_body(ROP_UPLOAD_STATE_BEGIN, index),
                consumed,
            }
        }
        // A property that may not be uploaded, refused rather than stored
        // somewhere nothing will read it.
        Err(_) => refuse(
            ROP_UPLOAD_STATE_BEGIN,
            index,
            error::NOT_IMPLEMENTED,
            consumed,
        ),
    }
}

/// `RopSynchronizationUploadStateStreamContinue`.
fn upload_continue(
    header: &RopHeader,
    rest: &[u8],
    objects: &mut ObjectTable,
    handles: &[u32],
) -> SyncOutcome {
    let Ok((request, tail)) = UploadContinueRequest::parse(rest) else {
        return refuse(
            ROP_UPLOAD_STATE_CONTINUE,
            header.input_handle_index,
            error::INVALID_OBJECT,
            rest.len(),
        );
    };
    let consumed = rest.len() - tail.len();
    let index = request.input_handle_index;

    let Some(ServerObject::SyncContext { upload, .. }) = resolve_mut(objects, handles, index)
    else {
        return refuse(
            ROP_UPLOAD_STATE_CONTINUE,
            index,
            error::INVALID_OBJECT,
            consumed,
        );
    };

    // A continue with no begin behind it has nowhere to go; the client has
    // skipped a step rather than sent something we can use.
    let Some(active) = upload.as_mut() else {
        return refuse(
            ROP_UPLOAD_STATE_CONTINUE,
            index,
            error::INVALID_OBJECT,
            consumed,
        );
    };

    match active.extend(&request.data) {
        Ok(()) => SyncOutcome {
            response: upload_success_body(ROP_UPLOAD_STATE_CONTINUE, index),
            consumed,
        },
        Err(_) => {
            // The client contradicted its own declared size. Drop the partial
            // upload rather than keep bytes we cannot trust the shape of.
            *upload = None;
            refuse(
                ROP_UPLOAD_STATE_CONTINUE,
                index,
                error::NOT_IMPLEMENTED,
                consumed,
            )
        }
    }
}

/// `RopSynchronizationUploadStateStreamEnd`.
fn upload_end(
    header: &RopHeader,
    rest: &[u8],
    objects: &mut ObjectTable,
    handles: &[u32],
) -> SyncOutcome {
    let Ok((request, tail)) = UploadEndRequest::parse(rest) else {
        return refuse(
            ROP_UPLOAD_STATE_END,
            header.input_handle_index,
            error::INVALID_OBJECT,
            rest.len(),
        );
    };
    let consumed = rest.len() - tail.len();
    let index = request.input_handle_index;

    let Some(ServerObject::SyncContext { upload, state, .. }) =
        resolve_mut(objects, handles, index)
    else {
        return refuse(ROP_UPLOAD_STATE_END, index, error::INVALID_OBJECT, consumed);
    };

    let Some(active) = upload.take() else {
        return refuse(ROP_UPLOAD_STATE_END, index, error::INVALID_OBJECT, consumed);
    };

    // Kept, not discarded: the router reads this off the rehearsed table to
    // decide what the client still needs. A malformed set leaves the state as
    // it was rather than half-applied, so a client that garbles one property
    // does not silently lose the ones it got right.
    match active.finish(state) {
        Ok(_) => SyncOutcome {
            response: upload_success_body(ROP_UPLOAD_STATE_END, index),
            consumed,
        },
        Err(_) => refuse(
            ROP_UPLOAD_STATE_END,
            index,
            error::NOT_IMPLEMENTED,
            consumed,
        ),
    }
}

/// The object a handle index names, mutably.
fn resolve_mut<'a>(
    objects: &'a mut ObjectTable,
    handles: &[u32],
    index: u8,
) -> Option<&'a mut ServerObject> {
    let handle = handles
        .get(usize::from(index))
        .copied()
        .filter(|handle| *handle != HANDLE_UNSET)?;
    objects.get_mut(handle)
}

/// Puts `handle` in the table at `index`, growing it if the client named a slot
/// past the end.
fn assign(handles: &mut Vec<u32>, index: u8, handle: u32) {
    let index = usize::from(index);
    if handles.len() <= index {
        handles.resize(index + 1, HANDLE_UNSET);
    }
    handles[index] = handle;
}

/// A failure response, and how much of the buffer it accounts for.
fn refuse(rop_id: u8, handle_index: u8, code: u32, consumed: usize) -> SyncOutcome {
    let mut response = Vec::with_capacity(6);
    response.push(rop_id);
    response.push(handle_index);
    response.extend_from_slice(&code.to_le_bytes());
    SyncOutcome { response, consumed }
}
