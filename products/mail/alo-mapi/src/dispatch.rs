//! Dispatching a ROP list: walking the requests, deciding what each may do, and
//! assembling the responses ([MS-OXCROPS] §3).
//!
//! **This is where the tenancy decision lives.** Everything before it moved
//! bytes; this is the first module that answers "may this caller open that
//! mailbox", and it answers from the Session Context — the identity the caller
//! actually proved — never from anything the request says about itself.
//!
//! ## Walking a list we only partly understand
//!
//! ROP requests are packed end to end with no length prefix and no separator:
//! the only way to find the second is to fully parse the first, because each
//! operation's body has its own shape. So an operation we cannot parse is not
//! one we can skip — it is the end of what this buffer can tell us.
//!
//! When that happens the dispatcher answers the operation it could not perform
//! with `NotImplemented` and stops. The alternative — guessing a length to
//! resume at — would parse whatever follows as some other operation entirely,
//! and act on it. A short answer is recoverable; acting on a misread request is
//! not.

use crate::attachments::{
    AttachmentTableRequest, OpenAttachmentRequest, ROP_GET_ATTACHMENT_TABLE, ROP_OPEN_ATTACHMENT,
    open_success_body as open_attachment_success, table_success_body as attachment_table_success,
};
use crate::columns::{
    PropertyTag, ROP_SET_COLUMNS, SetColumnsRequest, success_body as set_columns_success,
};
use crate::contents::{
    ContentsTableRequest, ROP_GET_CONTENTS_TABLE, success_body as contents_table_success,
};
use crate::folders::FolderView;
use crate::hierarchy::{
    HierarchyTableRequest, ROP_GET_HIERARCHY_TABLE, success_body as hierarchy_table_success,
};
use crate::logon::LogonRequest;
use crate::logon_response::{
    Fid, LogonResponse, LogonTime, RESPONSE_OWNER_RIGHT, RESPONSE_SEND_AS_RIGHT, ROP_LOGON,
    SpecialFolder,
};
use crate::messages::MessageView;
use crate::openfolder::{OpenFolderRequest, ROP_OPEN_FOLDER, success_body as open_folder_success};
use crate::openmessage::{
    OpenMessageRequest, ROP_OPEN_MESSAGE, success_body as open_message_success,
};
use crate::properties::{
    GetPropertiesRequest, ROP_GET_PROPERTIES_SPECIFIC, success_body as get_properties_success,
};
use crate::release::{ROP_RELEASE, ReleaseRequest};
use crate::rop::{RopBuffer, RopHeader};
use crate::rows::{
    MESSAGE_CLASS_NOTE, ORIGIN_BEGINNING, ORIGIN_END, QueryRowsRequest, ROP_QUERY_ROWS, Value, pid,
    standard_row, success_body as query_rows_success,
};
use crate::session::SessionContext;
use crate::stream::{
    MAX_READ, OpenStreamRequest, ROP_OPEN_STREAM, ROP_READ_STREAM, ReadStreamRequest,
    open_success_body as open_stream_success, read_success_body as read_stream_success,
};

/// Error codes as they travel in ROP responses ([MS-OXCDATA] §2.4).
pub mod error {
    /// The operation succeeded.
    pub const SUCCESS: u32 = 0x0000_0000;
    /// The caller does not have sufficient access rights (`ecAccessDenied`).
    pub const ACCESS_DENIED: u32 = 0x8007_0005;
    /// The server does not implement this method call.
    pub const NOT_IMPLEMENTED: u32 = 0x8004_0FFF;
    /// A client was unable to log on to the server (`ecLoginFailure`).
    pub const LOGON_FAILED: u32 = 0x8004_0111;
    /// The requested object could not be found (`ecNotFound`).
    pub const NOT_FOUND: u32 = 0x8004_010F;
    /// A reference to an object that is destroyed or not viable
    /// (`ecInvalidObject`).
    pub const INVALID_OBJECT: u32 = 0x8004_0108;
}

/// The replica id this deployment issues folder ids under.
///
/// One namespace per deployment for now. Per-mailbox replicas are what
/// `USE_PER_MDB_REPLID_MAPPING` asks for, and they arrive with the stage that
/// serves real folders — inventing the scheme now would be a guess baked into
/// identifiers clients cache.
pub const REPLICA_ID: u16 = 1;

/// A server object a client holds a handle to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerObject {
    /// A logon: an open mailbox, and the root of everything else a client does.
    Logon {
        /// The id the client asked this logon be known by.
        logon_id: u8,
        /// The address whose mailbox is open.
        login: String,
    },
    /// An open folder, reached through a logon.
    Folder {
        /// The id of the folder that is open.
        folder_id: u64,
    },
    /// A table of a folder's children, opened on that folder.
    HierarchyTable {
        /// The folder whose children this table lists.
        folder_id: u64,
        /// The columns every row of this table will carry, in order.
        ///
        /// Empty until a `RopSetColumns` names them. A row's values are matched
        /// to properties **by position** against this list, so it is the
        /// table's schema rather than a preference.
        columns: Vec<PropertyTag>,
        /// How many rows the client has already read.
        ///
        /// A table has a cursor, and `RopQueryRows` advances it: a client that
        /// asks twice gets the next rows, not the same ones again.
        cursor: usize,
    },
    /// An open message, reached through a logon.
    Message {
        /// The folder the message was opened from.
        ///
        /// Kept because a MID is only meaningful inside a folder: the lookup
        /// that turns one into a store message searches that folder's loaded
        /// rows, and a message object that forgot where it came from could not
        /// be re-resolved on a later request.
        folder_id: u64,
        /// The id the client opened it by.
        mid: u64,
    },
    /// An open stream over one property of one message.
    ///
    /// Holds a cursor, not the bytes. The value is re-read from the loaded
    /// message on every request, so a session that keeps a stream open across
    /// many requests costs a position rather than a copy of somebody's mail.
    Stream {
        /// The folder the message was opened from.
        folder_id: u64,
        /// The message whose property this streams.
        mid: u64,
        /// The attachment, when the stream reads one of its files rather than
        /// a property of the message itself.
        attachment: Option<u32>,
        /// Which property.
        property_id: u16,
        /// How many bytes the client has already read.
        position: usize,
    },
    /// A table of a message's attachments, opened on that message.
    AttachmentTable {
        /// The folder the message was opened from.
        folder_id: u64,
        /// The message whose attachments this lists.
        mid: u64,
        /// The columns every row will carry, in order.
        columns: Vec<PropertyTag>,
        /// How many rows the client has already read.
        cursor: usize,
    },
    /// One open attachment of a message.
    Attachment {
        /// The folder the message was opened from.
        folder_id: u64,
        /// The message it hangs off.
        mid: u64,
        /// Its `PidTagAttachNumber`.
        number: u32,
    },
    /// A table of a folder's messages, opened on that folder.
    ///
    /// Deliberately its own variant rather than a flag on
    /// [`ServerObject::HierarchyTable`]: the two tables share their shape on
    /// the wire and nothing else. A child folder and a message answer
    /// different properties out of different sources, so a single variant
    /// would put a discriminant in every place either one is read.
    ContentsTable {
        /// The folder whose messages this table lists.
        folder_id: u64,
        /// The columns every row of this table will carry, in order.
        columns: Vec<PropertyTag>,
        /// How many rows the client has already read.
        cursor: usize,
        /// Whether this table was opened for associated (FAI) messages.
        ///
        /// alo keeps none, so such a table is empty. It is remembered rather
        /// than discarded because the emptiness has to be *stable*: a client
        /// that opens an associated table and reads it twice must not be told
        /// a different story the second time.
        associated: bool,
    },
}

/// The server objects one Session Context holds.
///
/// Handles are allocated per session and never reused within it: a handle that
/// came back after its object was released would let a stale client reach
/// whatever took its place.
///
/// `Clone` exists for one caller: [`wanted_contents`] dispatches against a copy
/// to learn which folders a buffer is about to read, and must not leave the
/// session's real table advanced by that rehearsal.
#[derive(Debug, Default, Clone)]
pub struct ObjectTable {
    objects: Vec<(u32, ServerObject)>,
    next: u32,
}

impl ObjectTable {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            objects: Vec::new(),
            next: 1,
        }
    }

    /// Stores an object and returns its handle.
    ///
    /// Handles start at one because `0xFFFFFFFF` means "unset" on the wire and
    /// zero is the value an uninitialised table entry most often holds — a
    /// handle that collided with either would be indistinguishable from an
    /// absent one.
    pub fn insert(&mut self, object: ServerObject) -> u32 {
        let handle = self.next;
        self.next = self.next.saturating_add(1);
        self.objects.push((handle, object));
        handle
    }

    /// The object a handle names, mutably, if this session has one.
    pub fn get_mut(&mut self, handle: u32) -> Option<&mut ServerObject> {
        self.objects
            .iter_mut()
            .find(|(stored, _)| *stored == handle)
            .map(|(_, object)| object)
    }

    /// The object a handle names, if this session has one.
    #[must_use]
    pub fn get(&self, handle: u32) -> Option<&ServerObject> {
        self.objects
            .iter()
            .find(|(stored, _)| *stored == handle)
            .map(|(_, object)| object)
    }

    /// Forgets the object a handle names, if this session has one.
    ///
    /// The handle is **not** made available again: `next` only ever goes up, so
    /// a client that released a handle and then used it names nothing rather
    /// than naming whatever was created afterwards.
    pub fn remove(&mut self, handle: u32) -> bool {
        let before = self.objects.len();
        self.objects.retain(|(stored, _)| *stored != handle);
        self.objects.len() != before
    }

    /// How many objects are open.
    #[must_use]
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Whether no object is open.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}

/// The distinguished name this deployment would issue for a mailbox.
///
/// Derived from the address rather than looked up, so it is the same string
/// every time and can be compared without a round trip. The local part is
/// lowercased because a DN comparison that was case-sensitive would refuse a
/// client that echoed back its own name differently.
#[must_use]
pub fn mailbox_dn(prefix: &str, login: &str) -> String {
    let local = login.split('@').next().unwrap_or(login).to_lowercase();
    format!("{}/cn={local}", prefix.trim_end_matches('/'))
}

/// Whether `essdn` names the mailbox this session may open.
///
/// **The security decision of this module.** An empty `Essdn` means "my own
/// mailbox", which is what a client sends when it has nothing better; anything
/// else must match the DN we would have issued for the authenticated address.
/// The comparison is against [`SessionContext::login`] — who the caller proved
/// to be — and never against the DN they announced at `Connect`, which is a
/// string they chose.
#[must_use]
pub fn may_open(ctx: &SessionContext, prefix: &str, essdn: &str) -> bool {
    if essdn.is_empty() {
        return true;
    }
    essdn.eq_ignore_ascii_case(&mailbox_dn(prefix, &ctx.login))
}

/// One message property, answered from the loaded message.
///
/// The single source for what a message says: [`ROP_GET_PROPERTIES_SPECIFIC`]
/// builds a row from it and [`ROP_OPEN_STREAM`] streams it. Two lookups would
/// eventually disagree, and a client that read a body one way and streamed it
/// another would see the difference.
fn message_value(
    entry: &crate::messages::MessageEntry,
    body: &crate::messages::MessageBody,
    property_id: u16,
) -> Option<Value> {
    match property_id {
        pid::MID => Some(Value::Integer64(entry.mid)),
        pid::SUBJECT => Some(Value::String(entry.subject.clone())),
        pid::SENDER_NAME => Some(Value::String(entry.sender.clone())),
        pid::MESSAGE_DELIVERY_TIME => Some(Value::Time(entry.delivery_time)),
        pid::MESSAGE_FLAGS => Some(Value::Integer32(entry.flags)),
        pid::MESSAGE_SIZE => Some(Value::Integer32(entry.size)),
        pid::HAS_ATTACHMENTS => Some(Value::Boolean(entry.has_attachment)),
        pid::MESSAGE_CLASS => Some(Value::String(MESSAGE_CLASS_NOTE.to_owned())),
        pid::BODY => Some(Value::String(body.text.clone())),
        pid::DISPLAY_TO => Some(Value::String(body.display_to.clone())),
        pid::DISPLAY_CC => Some(Value::String(body.display_cc.clone())),
        // A message with no `Date` header has no submit time, and a zero here
        // would date it to 1601. Refusing the column is the honest answer.
        pid::CLIENT_SUBMIT_TIME => body.submit_time.map(Value::Time),
        pid::INTERNET_MESSAGE_ID => body
            .internet_message_id
            .as_ref()
            .map(|id| Value::String(id.clone())),
        _ => None,
    }
}

/// Which of the three tables a `RopQueryRows` is reading.
///
/// They share a shape on the wire and nothing else: the rows come from three
/// different places and answer three different property sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableKind {
    /// A folder's children.
    Hierarchy,
    /// A folder's messages.
    Contents,
    /// One message's attachments.
    Attachments {
        /// The message they hang off.
        mid: u64,
    },
}

/// One attachment property, answered from the loaded message.
fn attachment_value(
    attachment: &crate::messages::AttachmentEntry,
    property_id: u16,
) -> Option<Value> {
    match property_id {
        pid::ATTACH_NUMBER => Some(Value::Integer32(attachment.number)),
        pid::ATTACH_SIZE => Some(Value::Integer32(attachment.size)),
        pid::ATTACH_LONG_FILENAME => Some(Value::String(attachment.filename.clone())),
        pid::ATTACH_MIME_TAG => Some(Value::String(attachment.mime_type.clone())),
        // Everything alo attaches is carried in the message itself.
        pid::ATTACH_METHOD => Some(Value::Integer32(crate::attachments::ATTACH_BY_VALUE)),
        // `PidTagAttachDataBinary` is deliberately not answered in a row. It is
        // `PtypBinary`, whose count width is the one thing in this protocol
        // this crate has not resolved, and any real file exceeds a client's
        // property-size limit anyway — so it is read as a stream, where there
        // is no count field to get wrong.
        _ => None,
    }
}

/// The bytes a stream carries.
///
/// A stream reads either one property of a message or the contents of one of
/// its attachments; `attachment` picks which. An attachment's
/// `PidTagAttachDataBinary` is served raw — a stream has no count field, which
/// is precisely why the `PtypBinary` width question never arises for a file.
///
/// A string streams as its UTF-16LE content **without** the terminating null a
/// property row carries: the stream's size is the size of the value, and a
/// client that appended two zero bytes to a body would render a stray
/// character at the end of every long message. This is the one encoding choice
/// here not pinned by a specification sentence, so it is written down rather
/// than assumed — and it is what a real client should be checked against.
fn stream_source(
    messages: &MessageView,
    folder_id: u64,
    mid: u64,
    attachment: Option<u32>,
    property_id: u16,
) -> Option<Vec<u8>> {
    if let Some(number) = attachment {
        let attachment = messages.attachment(mid, number)?;
        return match property_id {
            pid::ATTACH_DATA_BINARY => attachment.data.clone(),
            other => {
                let mut out = Vec::new();
                attachment_value(attachment, other)?.write(&mut out);
                Some(out)
            }
        };
    }
    let entry = messages.entry(folder_id, mid)?;
    let body = messages.body(mid)?;
    match message_value(entry, body, property_id)? {
        Value::String(text) => {
            let mut out = Vec::with_capacity(text.len() * 2);
            for unit in text.encode_utf16() {
                out.extend_from_slice(&unit.to_le_bytes());
            }
            Some(out)
        }
        other => {
            let mut out = Vec::new();
            other.write(&mut out);
            Some(out)
        }
    }
}

/// A failure response for an operation that returns only a status
/// ([MS-OXCROPS] §2.2.3.1.3): `RopId`, the handle index, and the code.
fn failure(rop_id: u8, handle_index: u8, code: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(6);
    out.push(rop_id);
    out.push(handle_index);
    out.extend_from_slice(&code.to_le_bytes());
    out
}

/// What a dispatch produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dispatched {
    /// The ROP responses, ready to be framed as a ROP output buffer.
    pub responses: Vec<u8>,
    /// The handle table to return with them.
    pub handles: Vec<u32>,
    /// Whether the list was walked to its end. `false` means an operation could
    /// not be parsed and the rest of the buffer was not read.
    pub complete: bool,
    /// The folders whose messages this buffer needed.
    ///
    /// Populated whether or not the messages were available, which is what
    /// makes [`wanted_contents`] work: a rehearsal against an empty
    /// [`MessageView`] still reports what it *would* have read.
    pub contents_folders: Vec<u64>,
    /// The messages this buffer opened, as `(folder id, MID)`.
    ///
    /// Same purpose and same rule: recorded during the rehearsal so the router
    /// knows which bodies to fetch, and the folder each one names is loaded
    /// first, because a MID can only be resolved inside its folder's rows.
    pub opened_messages: Vec<(u64, u64)>,
    /// The attachments this buffer opened, as `(folder id, MID, number)`.
    ///
    /// Separate from the messages because the costs differ by orders of
    /// magnitude: listing a message's attachments reads names and sizes out of
    /// a parse already done, and opening one means decoding a file.
    pub opened_attachments: Vec<(u64, u64, u32)>,
}

/// The folders whose messages a buffer is about to read.
///
/// The router has to load messages **before** dispatching, because dispatch
/// runs under a lock and awaits nothing — but which folder a client is about to
/// read is only knowable by walking the buffer, and the handle it names is
/// often opened by an earlier operation in that same buffer (Outlook sends
/// `RopOpenFolder`, `RopGetContentsTable`, `RopSetColumns` and `RopQueryRows`
/// together). A scan that did not simulate handle allocation would miss
/// exactly the common case.
///
/// So this rehearses the whole dispatch against a **copy** of the object table
/// and reports what it reached. The responses are thrown away; only the list of
/// things to load is kept. Rehearsing costs a second walk of a buffer that is at
/// most tens of kilobytes, and it cannot drift from the real walk because it
/// *is* the real walk.
///
/// `messages` is what has been loaded so far, which is why the router calls this
/// more than once: each pass can see one layer further than the last. With
/// nothing loaded a rehearsal cannot get past `RopOpenMessage`, so it never
/// reaches the `RopOpenAttachment` behind it.
#[must_use]
pub fn wanted_contents(
    ctx: &SessionContext,
    prefix: &str,
    objects: &ObjectTable,
    folders: &FolderView,
    messages: &MessageView,
    input: &RopBuffer,
    now: LogonTime,
) -> Wanted {
    let mut rehearsal = objects.clone();
    let out = dispatch(ctx, prefix, &mut rehearsal, folders, messages, input, now);
    Wanted {
        folders: out.contents_folders,
        messages: out.opened_messages,
        attachments: out.opened_attachments,
    }
}

/// What a rehearsal learned a buffer is about to read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Wanted {
    /// Folders whose message rows are needed.
    pub folders: Vec<u64>,
    /// Messages whose content is needed, as `(folder id, MID)`.
    pub messages: Vec<(u64, u64)>,
    /// Attachments whose bytes are needed, as `(folder id, MID, number)`.
    pub attachments: Vec<(u64, u64, u32)>,
}

/// Walks a ROP input buffer and answers each request in turn.
///
/// `now` is passed rather than read so a response's bytes are testable; see
/// [`LogonTime`].
#[must_use]
pub fn dispatch(
    ctx: &SessionContext,
    prefix: &str,
    objects: &mut ObjectTable,
    folders: &FolderView,
    messages: &MessageView,
    input: &RopBuffer,
    now: LogonTime,
) -> Dispatched {
    let mut responses = Vec::new();
    let mut handles = input.handles.clone();
    let mut rest: &[u8] = &input.rops;
    let mut contents_folders: Vec<u64> = Vec::new();
    let mut opened_messages: Vec<(u64, u64)> = Vec::new();
    let mut opened_attachments: Vec<(u64, u64, u32)> = Vec::new();

    while !rest.is_empty() {
        let Ok(header) = RopHeader::parse(rest) else {
            // Fewer than three bytes left: the list is malformed rather than
            // finished, and there is no operation here to answer.
            return Dispatched {
                responses,
                handles,
                complete: false,
                contents_folders,
                opened_messages,
                opened_attachments,
            };
        };

        match header.rop_id {
            ROP_LOGON => {
                let Ok((request, tail)) = LogonRequest::parse(rest) else {
                    responses.extend(failure(
                        ROP_LOGON,
                        header.input_handle_index,
                        error::LOGON_FAILED,
                    ));
                    return Dispatched {
                        responses,
                        handles,
                        complete: false,
                        contents_folders,
                        opened_messages,
                        opened_attachments,
                    };
                };
                rest = tail;

                // Public folders are a later stage. Refused by name rather than
                // answered as though the mailbox were empty.
                if request.wants_public() || !request.is_private() {
                    responses.extend(failure(
                        ROP_LOGON,
                        request.output_handle_index,
                        error::NOT_IMPLEMENTED,
                    ));
                    continue;
                }

                // **The tenancy decision.** A caller may open the mailbox they
                // authenticated as, and no other — an authenticated user naming
                // somebody else's DN is refused, not served.
                if !may_open(ctx, prefix, &request.essdn) {
                    tracing::warn!(
                        logon_id = request.logon_id,
                        "mapi: refused a logon to another mailbox"
                    );
                    responses.extend(failure(
                        ROP_LOGON,
                        request.output_handle_index,
                        error::ACCESS_DENIED,
                    ));
                    continue;
                }

                // Administrative access is never granted here. The request is
                // answered for the caller's own mailbox with ordinary rights
                // rather than refused, because that is what the caller is
                // entitled to and what they asked for beyond it is simply not
                // given.
                let handle = objects.insert(ServerObject::Logon {
                    logon_id: request.logon_id,
                    login: ctx.login.clone(),
                });

                let index = usize::from(request.output_handle_index);
                if handles.len() <= index {
                    handles.resize(index + 1, crate::rop::HANDLE_UNSET);
                }
                handles[index] = handle;

                // Folder ids are positional and stable per deployment. Real
                // folders arrive with the stage that serves them; what matters
                // here is that the slots are filled in the order a client reads
                // them, and that the same folder keeps the same id.
                let folder_ids = SpecialFolder::ALL.map(|folder| {
                    Fid::new(REPLICA_ID, folder.slot() as u64 + 1).unwrap_or(Fid {
                        replica: REPLICA_ID,
                        counter: 0,
                    })
                });

                let response = LogonResponse {
                    output_handle_index: request.output_handle_index,
                    // Echoed unchanged, as the specification requires.
                    logon_flags: request.logon_flags,
                    folder_ids,
                    response_flags: RESPONSE_OWNER_RIGHT | RESPONSE_SEND_AS_RIGHT,
                    mailbox_guid: [0; 16],
                    replica_id: REPLICA_ID,
                    replica_guid: [0; 16],
                    logon_time: now,
                    gwart_time: 0,
                };
                responses.extend(response.to_bytes());
            }

            ROP_OPEN_FOLDER => {
                let Ok((request, tail)) = OpenFolderRequest::parse(rest) else {
                    responses.extend(failure(
                        ROP_OPEN_FOLDER,
                        header.input_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    return Dispatched {
                        responses,
                        handles,
                        complete: false,
                        contents_folders,
                        opened_messages,
                        opened_attachments,
                    };
                };
                rest = tail;

                // **A folder is opened through a logon, and the logon must be
                // one this session holds.** The handle table is per Session
                // Context, so a handle from another caller's session simply is
                // not here — which is what keeps folder handles unreachable
                // across sessions without a second check.
                let logon = handles
                    .get(usize::from(request.input_handle_index))
                    .copied()
                    .filter(|handle| *handle != crate::rop::HANDLE_UNSET)
                    .and_then(|handle| objects.get(handle));
                if !matches!(logon, Some(ServerObject::Logon { .. })) {
                    responses.extend(failure(
                        ROP_OPEN_FOLDER,
                        request.output_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    continue;
                }

                // A folder id this mailbox does not contain is not a
                // folder. The view is built from this caller's own mailboxes,
                // so an id belonging to somebody else is simply absent from it
                // — the refusal is structural rather than a check.
                let Some(_) = folders.get(request.folder_id) else {
                    responses.extend(failure(
                        ROP_OPEN_FOLDER,
                        request.output_handle_index,
                        error::NOT_FOUND,
                    ));
                    continue;
                };

                let handle = objects.insert(ServerObject::Folder {
                    folder_id: request.folder_id,
                });
                let index = usize::from(request.output_handle_index);
                if handles.len() <= index {
                    handles.resize(index + 1, crate::rop::HANDLE_UNSET);
                }
                handles[index] = handle;

                // No rules yet, and never ghosted: we are the only replica of
                // everything we serve.
                responses.extend(open_folder_success(request.output_handle_index, false));
            }

            ROP_GET_HIERARCHY_TABLE => {
                let Ok((request, tail)) = HierarchyTableRequest::parse(rest) else {
                    responses.extend(failure(
                        ROP_GET_HIERARCHY_TABLE,
                        header.input_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    return Dispatched {
                        responses,
                        handles,
                        complete: false,
                        contents_folders,
                        opened_messages,
                        opened_attachments,
                    };
                };
                rest = tail;

                // A hierarchy table is opened **on a folder**, so the input
                // handle must name one this session holds. A logon handle is
                // not a folder, and neither is a table — asking the wrong kind
                // of object for its children is refused rather than answered
                // with an empty one.
                let opened = handles
                    .get(usize::from(request.input_handle_index))
                    .copied()
                    .filter(|handle| *handle != crate::rop::HANDLE_UNSET)
                    .and_then(|handle| objects.get(handle));
                let Some(ServerObject::Folder { folder_id }) = opened else {
                    responses.extend(failure(
                        ROP_GET_HIERARCHY_TABLE,
                        request.output_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    continue;
                };
                let folder_id = *folder_id;

                let handle = objects.insert(ServerObject::HierarchyTable {
                    folder_id,
                    columns: Vec::new(),
                    cursor: 0,
                });
                let index = usize::from(request.output_handle_index);
                if handles.len() <= index {
                    handles.resize(index + 1, crate::rop::HANDLE_UNSET);
                }
                handles[index] = handle;

                let rows = u32::try_from(folders.children(folder_id).len()).unwrap_or(0);
                responses.extend(hierarchy_table_success(request.output_handle_index, rows));
            }

            ROP_GET_CONTENTS_TABLE => {
                let Ok((request, tail)) = ContentsTableRequest::parse(rest) else {
                    responses.extend(failure(
                        ROP_GET_CONTENTS_TABLE,
                        header.input_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    return Dispatched {
                        responses,
                        handles,
                        complete: false,
                        contents_folders,
                        opened_messages,
                        opened_attachments,
                    };
                };
                rest = tail;

                // Opened **on a folder**, exactly as a hierarchy table is: a
                // logon handle is not a folder and neither is another table.
                // Asking the wrong kind of object for its messages is refused
                // rather than answered with an empty table, which a client
                // would cache as "this folder has no mail in it".
                let opened = handles
                    .get(usize::from(request.input_handle_index))
                    .copied()
                    .filter(|handle| *handle != crate::rop::HANDLE_UNSET)
                    .and_then(|handle| objects.get(handle));
                let Some(ServerObject::Folder { folder_id }) = opened else {
                    responses.extend(failure(
                        ROP_GET_CONTENTS_TABLE,
                        request.output_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    continue;
                };
                let folder_id = *folder_id;

                // Recorded even when the messages are already loaded: this list
                // is what the rehearsal in `wanted_contents` exists to collect,
                // and it runs against an empty view where nothing is loaded.
                // An associated table needs nothing read — alo has no FAI
                // messages — so asking for one loads nothing.
                if !request.associated() && !contents_folders.contains(&folder_id) {
                    contents_folders.push(folder_id);
                }

                let handle = objects.insert(ServerObject::ContentsTable {
                    folder_id,
                    columns: Vec::new(),
                    cursor: 0,
                    associated: request.associated(),
                });
                let index = usize::from(request.output_handle_index);
                if handles.len() <= index {
                    handles.resize(index + 1, crate::rop::HANDLE_UNSET);
                }
                handles[index] = handle;

                // The count is what has actually been read for this folder. A
                // folder nobody loaded reports zero here and its `RopQueryRows`
                // refuses rather than returning an empty page — the count and
                // the rows come from the same place, so they cannot disagree.
                let loaded = if request.associated() {
                    0
                } else {
                    messages.rows(folder_id).map_or(0, <[_]>::len)
                };
                let rows = u32::try_from(loaded).unwrap_or(u32::MAX);
                responses.extend(contents_table_success(request.output_handle_index, rows));
            }

            ROP_SET_COLUMNS => {
                let Ok((request, tail)) = SetColumnsRequest::parse(rest) else {
                    responses.extend(failure(
                        ROP_SET_COLUMNS,
                        header.input_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    return Dispatched {
                        responses,
                        handles,
                        complete: false,
                        contents_folders,
                        opened_messages,
                        opened_attachments,
                    };
                };
                rest = tail;

                // Columns are set **on a table**. A folder or a logon is not
                // one, and configuring the wrong kind of object is refused
                // rather than quietly ignored — a client that believed its
                // columns were set would misread every row that followed.
                let handle = handles
                    .get(usize::from(request.input_handle_index))
                    .copied()
                    .filter(|handle| *handle != crate::rop::HANDLE_UNSET);
                let is_table =
                    handle
                        .and_then(|handle| objects.get(handle))
                        .is_some_and(|object| {
                            matches!(
                                object,
                                ServerObject::HierarchyTable { .. }
                                    | ServerObject::ContentsTable { .. }
                                    | ServerObject::AttachmentTable { .. }
                            )
                        });
                if !is_table {
                    responses.extend(failure(
                        ROP_SET_COLUMNS,
                        request.input_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    continue;
                }

                // Every kind of table carries a column list, and
                // `RopSetColumns` means the same thing to each — it is the
                // *rows* that differ.
                if let Some(handle) = handle
                    && let Some(
                        ServerObject::HierarchyTable { columns, .. }
                        | ServerObject::ContentsTable { columns, .. }
                        | ServerObject::AttachmentTable { columns, .. },
                    ) = objects.get_mut(handle)
                {
                    columns.clone_from(&request.columns);
                }
                responses.extend(set_columns_success(request.input_handle_index));
            }

            ROP_OPEN_MESSAGE => {
                let Ok((request, tail)) = OpenMessageRequest::parse(rest) else {
                    responses.extend(failure(
                        ROP_OPEN_MESSAGE,
                        header.input_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    return Dispatched {
                        responses,
                        handles,
                        complete: false,
                        contents_folders,
                        opened_messages,
                        opened_attachments,
                    };
                };
                rest = tail;

                // Opened through a **logon**, which is what makes the mailbox
                // this reads from the authenticated one. The folder and message
                // ids that follow are the client's; the account they are looked
                // up in is not.
                let opened = handles
                    .get(usize::from(request.input_handle_index))
                    .copied()
                    .filter(|handle| *handle != crate::rop::HANDLE_UNSET)
                    .and_then(|handle| objects.get(handle));
                let Some(ServerObject::Logon { .. }) = opened else {
                    responses.extend(failure(
                        ROP_OPEN_MESSAGE,
                        request.output_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    continue;
                };

                // The folder must be one this session's own tree has. A folder
                // id naming nothing here is `ecNotFound`, the same answer a
                // folder that genuinely does not exist gets — so a caller
                // probing ids learns nothing from the difference.
                if folders.get(request.folder_id).is_none() {
                    responses.extend(failure(
                        ROP_OPEN_MESSAGE,
                        request.output_handle_index,
                        error::NOT_FOUND,
                    ));
                    continue;
                }

                // Recorded for the rehearsal, whose whole job is to reach here
                // with nothing loaded and report what it would have needed.
                let wanted = (request.folder_id, request.message_id);
                if !opened_messages.contains(&wanted) {
                    opened_messages.push(wanted);
                }

                // The MID is resolved inside that folder's loaded rows — the
                // only route from a client-supplied id to a stored message.
                let Some(entry) = messages.entry(request.folder_id, request.message_id) else {
                    // Either the folder was not loaded (the rehearsal) or the
                    // MID names no message of this account's. Both are
                    // `ecNotFound`: a message we cannot show is a message that
                    // does not exist as far as this caller is concerned.
                    responses.extend(failure(
                        ROP_OPEN_MESSAGE,
                        request.output_handle_index,
                        error::NOT_FOUND,
                    ));
                    continue;
                };
                let subject = entry.subject.clone();

                let handle = objects.insert(ServerObject::Message {
                    folder_id: request.folder_id,
                    mid: request.message_id,
                });
                let index = usize::from(request.output_handle_index);
                if handles.len() <= index {
                    handles.resize(index + 1, crate::rop::HANDLE_UNSET);
                }
                handles[index] = handle;

                // No named properties: alo stores none. Saying so truthfully
                // saves the client a `RopGetNamesFromPropertyIds` round trip.
                let subject = if subject.is_empty() {
                    None
                } else {
                    Some(subject.as_str())
                };
                responses.extend(open_message_success(
                    request.output_handle_index,
                    false,
                    subject,
                ));
            }

            ROP_GET_PROPERTIES_SPECIFIC => {
                let Ok((request, tail)) = GetPropertiesRequest::parse(rest) else {
                    responses.extend(failure(
                        ROP_GET_PROPERTIES_SPECIFIC,
                        header.input_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    return Dispatched {
                        responses,
                        handles,
                        complete: false,
                        contents_folders,
                        opened_messages,
                        opened_attachments,
                    };
                };
                rest = tail;

                let opened = handles
                    .get(usize::from(request.input_handle_index))
                    .copied()
                    .filter(|handle| *handle != crate::rop::HANDLE_UNSET)
                    .and_then(|handle| objects.get(handle));
                // Only a message answers properties so far. A folder and a
                // logon have properties too, and they are a later stage; being
                // asked for them is refused rather than answered with a row of
                // invented values.
                let Some(ServerObject::Message { folder_id, mid }) = opened else {
                    responses.extend(failure(
                        ROP_GET_PROPERTIES_SPECIFIC,
                        request.input_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    continue;
                };
                let (folder_id, mid) = (*folder_id, *mid);

                let (Some(entry), Some(body)) =
                    (messages.entry(folder_id, mid), messages.body(mid))
                else {
                    // The message was opened but its content was never loaded.
                    // Answering a blank row would show a client an empty
                    // message, which is indistinguishable from mail that really
                    // is empty.
                    responses.extend(failure(
                        ROP_GET_PROPERTIES_SPECIFIC,
                        request.input_handle_index,
                        error::NOT_FOUND,
                    ));
                    continue;
                };

                let answer = |tag: PropertyTag| -> Option<Value> {
                    message_value(entry, body, tag.property_id)
                };

                // The client's own ceiling on a value it will accept, honoured
                // rather than ignored: a client that asked for at most 8 KB and
                // is handed a 2 MB body has had its protection taken away.
                let limit = usize::from(request.property_size_limit);
                match crate::rows::property_row(&request.tags, &answer, limit) {
                    Some(row) => {
                        responses.extend(get_properties_success(request.input_handle_index, &row));
                    }
                    None => {
                        responses.extend(failure(
                            ROP_GET_PROPERTIES_SPECIFIC,
                            request.input_handle_index,
                            error::NOT_IMPLEMENTED,
                        ));
                    }
                }
            }

            ROP_GET_ATTACHMENT_TABLE => {
                let Ok((request, tail)) = AttachmentTableRequest::parse(rest) else {
                    responses.extend(failure(
                        ROP_GET_ATTACHMENT_TABLE,
                        header.input_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    return Dispatched {
                        responses,
                        handles,
                        complete: false,
                        contents_folders,
                        opened_messages,
                        opened_attachments,
                    };
                };
                rest = tail;

                let opened = handles
                    .get(usize::from(request.input_handle_index))
                    .copied()
                    .filter(|handle| *handle != crate::rop::HANDLE_UNSET)
                    .and_then(|handle| objects.get(handle));
                let Some(ServerObject::Message { folder_id, mid }) = opened else {
                    responses.extend(failure(
                        ROP_GET_ATTACHMENT_TABLE,
                        request.output_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    continue;
                };
                let (folder_id, mid) = (*folder_id, *mid);

                let wanted = (folder_id, mid);
                if !opened_messages.contains(&wanted) {
                    opened_messages.push(wanted);
                }
                let Some(body) = messages.body(mid) else {
                    responses.extend(failure(
                        ROP_GET_ATTACHMENT_TABLE,
                        request.output_handle_index,
                        error::NOT_FOUND,
                    ));
                    continue;
                };
                let rows = u32::try_from(body.attachments.len()).unwrap_or(u32::MAX);

                let handle = objects.insert(ServerObject::AttachmentTable {
                    folder_id,
                    mid,
                    columns: Vec::new(),
                    cursor: 0,
                });
                let index = usize::from(request.output_handle_index);
                if handles.len() <= index {
                    handles.resize(index + 1, crate::rop::HANDLE_UNSET);
                }
                handles[index] = handle;
                responses.extend(attachment_table_success(request.output_handle_index, rows));
            }

            ROP_OPEN_ATTACHMENT => {
                let Ok((request, tail)) = OpenAttachmentRequest::parse(rest) else {
                    responses.extend(failure(
                        ROP_OPEN_ATTACHMENT,
                        header.input_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    return Dispatched {
                        responses,
                        handles,
                        complete: false,
                        contents_folders,
                        opened_messages,
                        opened_attachments,
                    };
                };
                rest = tail;

                // Opened on the message it belongs to: an attachment number
                // means nothing outside its own message, so there is no way to
                // name one without first holding the message.
                let opened = handles
                    .get(usize::from(request.input_handle_index))
                    .copied()
                    .filter(|handle| *handle != crate::rop::HANDLE_UNSET)
                    .and_then(|handle| objects.get(handle));
                let Some(ServerObject::Message { folder_id, mid }) = opened else {
                    responses.extend(failure(
                        ROP_OPEN_ATTACHMENT,
                        request.output_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    continue;
                };
                let (folder_id, mid) = (*folder_id, *mid);

                let wanted_message = (folder_id, mid);
                if !opened_messages.contains(&wanted_message) {
                    opened_messages.push(wanted_message);
                }
                // Opening an attachment is what makes its bytes worth fetching;
                // listing a message's attachments does not.
                let wanted = (folder_id, mid, request.attachment_id);
                if !opened_attachments.contains(&wanted) {
                    opened_attachments.push(wanted);
                }

                if messages.attachment(mid, request.attachment_id).is_none() {
                    responses.extend(failure(
                        ROP_OPEN_ATTACHMENT,
                        request.output_handle_index,
                        error::NOT_FOUND,
                    ));
                    continue;
                }

                let handle = objects.insert(ServerObject::Attachment {
                    folder_id,
                    mid,
                    number: request.attachment_id,
                });
                let index = usize::from(request.output_handle_index);
                if handles.len() <= index {
                    handles.resize(index + 1, crate::rop::HANDLE_UNSET);
                }
                handles[index] = handle;
                responses.extend(open_attachment_success(request.output_handle_index));
            }

            ROP_OPEN_STREAM => {
                let Ok((request, tail)) = OpenStreamRequest::parse(rest) else {
                    responses.extend(failure(
                        ROP_OPEN_STREAM,
                        header.input_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    return Dispatched {
                        responses,
                        handles,
                        complete: false,
                        contents_folders,
                        opened_messages,
                        opened_attachments,
                    };
                };
                rest = tail;

                // Opened on a message. Folders and logons have streamable
                // properties too, and they are a later stage.
                let opened = handles
                    .get(usize::from(request.input_handle_index))
                    .copied()
                    .filter(|handle| *handle != crate::rop::HANDLE_UNSET)
                    .and_then(|handle| objects.get(handle));
                // A stream reads a property of a message, or the bytes of one
                // of its attachments — the two objects a client streams from.
                let (folder_id, mid, attachment) = match opened {
                    Some(ServerObject::Message { folder_id, mid }) => (*folder_id, *mid, None),
                    Some(ServerObject::Attachment {
                        folder_id,
                        mid,
                        number,
                    }) => (*folder_id, *mid, Some(*number)),
                    _ => {
                        responses.extend(failure(
                            ROP_OPEN_STREAM,
                            request.output_handle_index,
                            error::INVALID_OBJECT,
                        ));
                        continue;
                    }
                };

                // Nothing here writes. Refused rather than quietly opened
                // read-only: a client holding what it believes is a writable
                // stream would send changes that went nowhere.
                if request.wants_to_write() {
                    responses.extend(failure(
                        ROP_OPEN_STREAM,
                        request.output_handle_index,
                        error::NOT_IMPLEMENTED,
                    ));
                    continue;
                }

                // Streaming needs the message loaded, exactly as opening it
                // did — and during the rehearsal it is not, which is what
                // tells the router to fetch it.
                let wanted = (folder_id, mid);
                if !opened_messages.contains(&wanted) {
                    opened_messages.push(wanted);
                }
                let Some(bytes) = stream_source(
                    messages,
                    folder_id,
                    mid,
                    attachment,
                    request.property_tag.property_id,
                ) else {
                    // A property this message does not have. `ecNotFound` is
                    // the truthful answer; an empty stream would say the
                    // property exists and is blank.
                    responses.extend(failure(
                        ROP_OPEN_STREAM,
                        request.output_handle_index,
                        error::NOT_FOUND,
                    ));
                    continue;
                };
                let size = u32::try_from(bytes.len()).unwrap_or(u32::MAX);

                let handle = objects.insert(ServerObject::Stream {
                    folder_id,
                    mid,
                    attachment,
                    property_id: request.property_tag.property_id,
                    position: 0,
                });
                let index = usize::from(request.output_handle_index);
                if handles.len() <= index {
                    handles.resize(index + 1, crate::rop::HANDLE_UNSET);
                }
                handles[index] = handle;
                responses.extend(open_stream_success(request.output_handle_index, size));
            }

            ROP_READ_STREAM => {
                let Ok((request, tail)) = ReadStreamRequest::parse(rest) else {
                    responses.extend(failure(
                        ROP_READ_STREAM,
                        header.input_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    return Dispatched {
                        responses,
                        handles,
                        complete: false,
                        contents_folders,
                        opened_messages,
                        opened_attachments,
                    };
                };
                rest = tail;

                let handle = handles
                    .get(usize::from(request.input_handle_index))
                    .copied()
                    .filter(|handle| *handle != crate::rop::HANDLE_UNSET);
                let Some(ServerObject::Stream {
                    folder_id,
                    mid,
                    attachment,
                    property_id,
                    position,
                }) = handle.and_then(|handle| objects.get(handle))
                else {
                    responses.extend(failure(
                        ROP_READ_STREAM,
                        request.input_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    continue;
                };
                let (folder_id, mid, attachment, property_id, position) =
                    (*folder_id, *mid, *attachment, *property_id, *position);

                // The stream holds a cursor, not the bytes, so a read in a
                // later request needs the message loaded again.
                let wanted = (folder_id, mid);
                if !opened_messages.contains(&wanted) {
                    opened_messages.push(wanted);
                }
                let Some(bytes) = stream_source(messages, folder_id, mid, attachment, property_id)
                else {
                    responses.extend(failure(
                        ROP_READ_STREAM,
                        request.input_handle_index,
                        error::NOT_FOUND,
                    ));
                    continue;
                };

                // Bounded three ways: what is left, what the client asked for,
                // and what `DataSize` can describe. A read past the end returns
                // nothing and succeeds — that is how a client knows it is done,
                // and an error there would look like a broken stream.
                let start = position.min(bytes.len());
                let take = usize::try_from(request.wanted)
                    .unwrap_or(MAX_READ)
                    .min(MAX_READ)
                    .min(bytes.len() - start);
                let chunk = &bytes[start..start + take];

                if let Some(handle) = handle
                    && let Some(ServerObject::Stream { position, .. }) = objects.get_mut(handle)
                {
                    *position = start + take;
                }
                responses.extend(read_stream_success(request.input_handle_index, chunk));
            }

            ROP_RELEASE => {
                let Ok((request, tail)) = ReleaseRequest::parse(rest) else {
                    responses.extend(failure(
                        ROP_RELEASE,
                        header.input_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    return Dispatched {
                        responses,
                        handles,
                        complete: false,
                        contents_folders,
                        opened_messages,
                        opened_attachments,
                    };
                };
                rest = tail;

                // **No response, ever.** The specification defines a request
                // buffer for this operation and no response buffer, so emitting
                // anything here would shift every response after it — a fault
                // that surfaces as a client rendering the wrong thing rather
                // than as an error. Releasing a handle that names nothing is
                // silently fine for the same reason: there is nowhere to say so.
                if let Some(handle) = handles
                    .get(usize::from(request.input_handle_index))
                    .copied()
                    .filter(|handle| *handle != crate::rop::HANDLE_UNSET)
                {
                    objects.remove(handle);
                }
            }

            ROP_QUERY_ROWS => {
                let Ok((request, tail)) = QueryRowsRequest::parse(rest) else {
                    responses.extend(failure(
                        ROP_QUERY_ROWS,
                        header.input_handle_index,
                        error::INVALID_OBJECT,
                    ));
                    return Dispatched {
                        responses,
                        handles,
                        complete: false,
                        contents_folders,
                        opened_messages,
                        opened_attachments,
                    };
                };
                rest = tail;

                let handle = handles
                    .get(usize::from(request.input_handle_index))
                    .copied()
                    .filter(|handle| *handle != crate::rop::HANDLE_UNSET);

                // Which table this is decides where the rows come from. Both
                // carry a folder id, a column list and a cursor; a hierarchy
                // table reads the folder's children out of the tree, and a
                // contents table reads its messages out of what was loaded.
                let opened = handle.and_then(|handle| objects.get(handle));
                let (folder_id, columns, cursor, kind, associated) = match opened {
                    Some(ServerObject::HierarchyTable {
                        folder_id,
                        columns,
                        cursor,
                    }) => (
                        *folder_id,
                        columns.clone(),
                        *cursor,
                        TableKind::Hierarchy,
                        false,
                    ),
                    Some(ServerObject::ContentsTable {
                        folder_id,
                        columns,
                        cursor,
                        associated,
                    }) => (
                        *folder_id,
                        columns.clone(),
                        *cursor,
                        TableKind::Contents,
                        *associated,
                    ),
                    Some(ServerObject::AttachmentTable {
                        folder_id,
                        mid,
                        columns,
                        cursor,
                    }) => (
                        *folder_id,
                        columns.clone(),
                        *cursor,
                        TableKind::Attachments { mid: *mid },
                        false,
                    ),
                    _ => {
                        responses.extend(failure(
                            ROP_QUERY_ROWS,
                            request.input_handle_index,
                            error::INVALID_OBJECT,
                        ));
                        continue;
                    }
                };

                // The rows this table could ever return, before the cursor and
                // the client's count are applied.
                let total = match kind {
                    TableKind::Hierarchy => folders.children(folder_id).len(),
                    // alo keeps no FAI messages, so an associated table is
                    // empty and stays empty. Not a refusal: an empty answer
                    // here is the truth, and a client caching it caches truth.
                    TableKind::Contents if associated => 0,
                    TableKind::Contents => match messages.rows(folder_id) {
                        Some(rows) => rows.len(),
                        None => {
                            // The folder's messages were never loaded, so there
                            // is nothing truthful to say about them. Returning
                            // an empty page would tell a client the folder is
                            // empty, which it then caches.
                            responses.extend(failure(
                                ROP_QUERY_ROWS,
                                request.input_handle_index,
                                error::NOT_FOUND,
                            ));
                            continue;
                        }
                    },
                    TableKind::Attachments { mid } => match messages.body(mid) {
                        Some(body) => body.attachments.len(),
                        None => {
                            responses.extend(failure(
                                ROP_QUERY_ROWS,
                                request.input_handle_index,
                                error::NOT_FOUND,
                            ));
                            continue;
                        }
                    },
                };

                // Read forwards from the cursor, bounded by what the client
                // asked for and by our own ceiling — a row is variable-sized,
                // so the count is the bound that can be applied before any of
                // them is built.
                let wanted = usize::from(request.row_count.min(crate::rows::MAX_ROWS));
                // Backward reads are not served yet. An empty answer at the end
                // of the table is a truthful "nothing further this way", not a
                // claim that the folder is empty.
                let span = if request.forward_read {
                    (cursor.min(total), (cursor + wanted).min(total))
                } else {
                    (0, 0)
                };

                let mut rows = Vec::with_capacity(span.1.saturating_sub(span.0));
                let mut refused = false;
                if let TableKind::Attachments { mid } = kind {
                    let empty = Vec::new();
                    let list = messages.body(mid).map_or(&empty, |body| &body.attachments);
                    for attachment in &list[span.0..span.1] {
                        let answer = |tag: PropertyTag| -> Option<Value> {
                            attachment_value(attachment, tag.property_id)
                        };
                        match standard_row(&columns, &answer) {
                            Some(row) => rows.push(row),
                            None => {
                                refused = true;
                                break;
                            }
                        }
                    }
                } else if matches!(kind, TableKind::Contents) {
                    let entries = if associated {
                        &[][..]
                    } else {
                        messages.rows(folder_id).unwrap_or(&[])
                    };
                    for message in &entries[span.0..span.1] {
                        let answer = |tag: PropertyTag| -> Option<Value> {
                            match tag.property_id {
                                pid::MID => Some(Value::Integer64(message.mid)),
                                pid::SUBJECT => Some(Value::String(message.subject.clone())),
                                pid::SENDER_NAME => Some(Value::String(message.sender.clone())),
                                pid::MESSAGE_DELIVERY_TIME => {
                                    Some(Value::Time(message.delivery_time))
                                }
                                pid::MESSAGE_FLAGS => Some(Value::Integer32(message.flags)),
                                pid::MESSAGE_SIZE => Some(Value::Integer32(message.size)),
                                pid::HAS_ATTACHMENTS => {
                                    Some(Value::Boolean(message.has_attachment))
                                }
                                // Everything alo keeps in a mailbox is a note.
                                pid::MESSAGE_CLASS => {
                                    Some(Value::String(MESSAGE_CLASS_NOTE.to_owned()))
                                }
                                _ => None,
                            }
                        };
                        match standard_row(&columns, &answer) {
                            Some(row) => rows.push(row),
                            None => {
                                refused = true;
                                break;
                            }
                        }
                    }
                } else {
                    let all = folders.children(folder_id);
                    for child in &all[span.0..span.1] {
                        let child = *child;
                        let has_children = !folders.children(child.fid).is_empty();
                        let answer = move |tag: PropertyTag| -> Option<Value> {
                            match tag.property_id {
                                pid::DISPLAY_NAME => Some(Value::String(child.name.clone())),
                                pid::FOLDER_ID => Some(Value::Integer64(child.fid)),
                                pid::SUBFOLDERS => Some(Value::Boolean(has_children)),
                                // Every folder in the view can answer this: a
                                // real mailbox reports what it holds, and a
                                // protocol folder reports zero because the
                                // store was read and no mailbox stands behind
                                // it.
                                pid::CONTENT_COUNT => Some(Value::Integer32(child.total_messages)),
                                _ => None,
                            }
                        };
                        match standard_row(&columns, &answer) {
                            Some(row) => rows.push(row),
                            None => {
                                refused = true;
                                break;
                            }
                        }
                    }
                }

                // A column we cannot answer makes this not a standard row, and
                // flagged rows are a later stage. Refusing is honest; filling
                // the gap with a zero would be a value the client believes.
                if refused {
                    responses.extend(failure(
                        ROP_QUERY_ROWS,
                        request.input_handle_index,
                        error::NOT_IMPLEMENTED,
                    ));
                    continue;
                }

                let advanced = cursor + rows.len();
                if let Some(handle) = handle
                    && let Some(
                        ServerObject::HierarchyTable { cursor, .. }
                        | ServerObject::ContentsTable { cursor, .. }
                        | ServerObject::AttachmentTable { cursor, .. },
                    ) = objects.get_mut(handle)
                {
                    *cursor = advanced;
                }

                let origin = if advanced >= total {
                    ORIGIN_END
                } else {
                    ORIGIN_BEGINNING
                };
                responses.extend(query_rows_success(
                    request.input_handle_index,
                    origin,
                    &rows,
                ));
            }

            // An operation we do not implement. We answer it honestly and stop:
            // its body length is unknown, so whatever follows cannot be found
            // without guessing, and a guess would have us act on a misread
            // request.
            other => {
                responses.extend(failure(
                    other,
                    header.input_handle_index,
                    error::NOT_IMPLEMENTED,
                ));
                return Dispatched {
                    responses,
                    handles,
                    complete: false,
                    contents_folders,
                    opened_messages,
                    opened_attachments,
                };
            }
        }
    }

    Dispatched {
        responses,
        handles,
        complete: true,
        contents_folders,
        opened_messages,
        opened_attachments,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::logon::{LOGON_PRIVATE, OPEN_PUBLIC};
    use alo_store::MapiMessageRow;
    use alo_store::{TenantId, UserId};
    use time::OffsetDateTime;

    fn context(login: &str) -> SessionContext {
        SessionContext {
            tenant: TenantId::generate(),
            user: UserId::generate(),
            user_dn: "/o=alo/cn=whatever-they-claimed".to_owned(),
            login: login.to_owned(),
            expires_at: OffsetDateTime::UNIX_EPOCH,
            objects: std::sync::Arc::new(std::sync::Mutex::new(ObjectTable::new())),
        }
    }

    /// A ROP input buffer carrying one logon for `essdn`.
    fn logon_buffer(essdn: &str, logon_flags: u8, open_flags: u32) -> RopBuffer {
        let mut rop = vec![ROP_LOGON, 0x00, 0x00, logon_flags];
        rop.extend_from_slice(&open_flags.to_le_bytes());
        rop.extend_from_slice(&0u32.to_le_bytes());
        let mut name = essdn.as_bytes().to_vec();
        name.push(0);
        rop.extend_from_slice(&u16::try_from(name.len()).unwrap().to_le_bytes());
        rop.extend_from_slice(&name);
        RopBuffer {
            rops: rop,
            handles: vec![crate::rop::HANDLE_UNSET],
        }
    }

    fn code_of(responses: &[u8]) -> u32 {
        u32::from_le_bytes([responses[2], responses[3], responses[4], responses[5]])
    }

    /// A `RopOpenFolder` request for `folder_id` through the logon at handle
    /// index 0, storing the folder at index 1.
    fn open_folder_rop(folder_id: u64) -> Vec<u8> {
        let mut out = vec![ROP_OPEN_FOLDER, 0x00, 0x00, 0x01];
        out.extend_from_slice(&folder_id.to_le_bytes());
        out.push(0x00);
        out
    }

    /// The folder id this deployment issues for a special folder.
    fn fid_of(folder: SpecialFolder) -> u64 {
        crate::folders::special_fid(folder)
    }

    /// A folder view for a mailbox with the three folders alo really has, plus
    /// one the person made themselves — so the tests exercise a real tree
    /// rather than the protocol's furniture alone.
    fn view() -> FolderView {
        FolderView::build(&[
            mailbox("mb-inbox", "Inbox", Some("inbox"), None, 12),
            mailbox("mb-sent", "Sent Items", Some("sent"), None, 3),
            mailbox("mb-trash", "Deleted Items", Some("trash"), None, 0),
            mailbox("mb-facturen", "Facturen", None, None, 7),
        ])
    }

    fn mailbox(
        id: &str,
        name: &str,
        role: Option<&str>,
        parent: Option<&str>,
        total: i64,
    ) -> alo_store::Mailbox {
        alo_store::Mailbox {
            id: alo_store::MailboxId::new(id),
            parent_id: parent.map(alo_store::MailboxId::new),
            name: name.to_owned(),
            role: role.map(ToOwned::to_owned),
            color: None,
            total_messages: total,
            unread_messages: 0,
        }
    }

    /// Logon and open the Inbox in one buffer — which is what a client does,
    /// naming the handle the logon has not created yet by its index.
    #[test]
    fn a_folder_opens_through_a_logon_in_the_same_buffer() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();

        let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
        rops.extend(open_folder_rop(fid_of(SpecialFolder::Inbox)));
        let buffer = RopBuffer {
            rops,
            handles: vec![crate::rop::HANDLE_UNSET, crate::rop::HANDLE_UNSET],
        };

        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        assert!(out.complete, "the walk stopped early");
        // 166 bytes of logon, then 8 of open-folder.
        assert_eq!(out.responses.len(), 166 + 8);
        let folder_response = &out.responses[166..];
        assert_eq!(folder_response[0], ROP_OPEN_FOLDER);
        assert_eq!(
            u32::from_le_bytes(folder_response[2..6].try_into().unwrap()),
            error::SUCCESS
        );
        assert_eq!(folder_response[7], 0, "IsGhosted: we are the only replica");

        // Both objects exist, and the folder went into the slot named for it.
        assert_eq!(objects.len(), 2);
        assert_eq!(
            objects.get(out.handles[1]),
            Some(&ServerObject::Folder {
                folder_id: fid_of(SpecialFolder::Inbox)
            })
        );
    }

    /// **A folder cannot be opened without a logon.** The handle table is per
    /// Session Context, so a handle from anywhere else is simply not in it.
    #[test]
    fn opening_a_folder_without_a_logon_handle_is_refused() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();
        let buffer = RopBuffer {
            rops: open_folder_rop(fid_of(SpecialFolder::Inbox)),
            // A handle that names nothing in this session.
            handles: vec![9999, crate::rop::HANDLE_UNSET],
        };

        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        assert_eq!(out.responses.len(), 6, "a failure response");
        assert_eq!(code_of(&out.responses), error::INVALID_OBJECT);
        assert!(objects.is_empty(), "a folder was opened anyway");
    }

    /// A folder id we never issued is not a folder — answered "not found"
    /// rather than opened as whatever happens to be nearby.
    #[test]
    fn a_folder_id_we_did_not_issue_is_not_found() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();

        for invented in [
            0u64,                                         // counter zero: never issued
            fid_of(SpecialFolder::Shortcuts) + (1 << 16), // one past the last
            0x0000_0005_0000_0002,                        // a different replica
            u64::MAX,
        ] {
            let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
            rops.extend(open_folder_rop(invented));
            let buffer = RopBuffer {
                rops,
                handles: vec![crate::rop::HANDLE_UNSET, crate::rop::HANDLE_UNSET],
            };
            let out = dispatch(
                &ctx,
                "/o=alo",
                &mut objects,
                &view(),
                &MessageView::new(),
                &buffer,
                LogonTime::default(),
            );
            let folder_response = &out.responses[166..];
            assert_eq!(
                u32::from_le_bytes(folder_response[2..6].try_into().unwrap()),
                error::NOT_FOUND,
                "opened an invented folder id {invented:#x}"
            );
        }
    }

    /// Every id the logon hands out resolves to a folder in the view — the
    /// round trip a client actually performs, now through the real tree.
    #[test]
    fn every_folder_id_the_logon_issues_resolves_back() {
        let view = view();
        for folder in SpecialFolder::ALL {
            assert!(
                view.get(fid_of(folder)).is_some(),
                "{folder:?} did not resolve"
            );
        }
    }

    /// A `RopGetHierarchyTable` on the folder at handle index 1, storing the
    /// table at index 2.
    fn hierarchy_rop() -> Vec<u8> {
        vec![ROP_GET_HIERARCHY_TABLE, 0x00, 0x01, 0x02, 0x00]
    }

    /// The whole chain a client performs to draw a folder tree: log on, open
    /// the interpersonal-messages subtree, ask it for its children — all in one
    /// buffer, each operation naming a handle the previous one created.
    #[test]
    fn a_client_can_log_on_open_a_folder_and_count_its_children_in_one_buffer() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();

        let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
        rops.extend(open_folder_rop(fid_of(SpecialFolder::IpmSubtree)));
        rops.extend(hierarchy_rop());
        let buffer = RopBuffer {
            rops,
            handles: vec![crate::rop::HANDLE_UNSET; 3],
        };

        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        assert!(out.complete, "the walk stopped early");
        assert_eq!(out.responses.len(), 166 + 8 + 10);

        let table = &out.responses[174..];
        assert_eq!(table[0], ROP_GET_HIERARCHY_TABLE);
        assert_eq!(
            u32::from_le_bytes(table[2..6].try_into().unwrap()),
            error::SUCCESS
        );
        assert_eq!(
            u32::from_le_bytes(table[6..10].try_into().unwrap()),
            5,
            "Inbox, Outbox, Sent Items, Deleted Items and the folder they made"
        );

        assert_eq!(
            objects.get(out.handles[2]),
            Some(&ServerObject::HierarchyTable {
                folder_id: fid_of(SpecialFolder::IpmSubtree),
                // No columns yet: a table starts without a schema, and the
                // rows it could return are undefined until one is set.
                columns: Vec::new(),
                cursor: 0,
            })
        );
    }

    /// A hierarchy table is opened on a folder. A **logon** handle is not a
    /// folder, and asking the wrong kind of object for its children is refused
    /// rather than answered with an empty table.
    #[test]
    fn a_hierarchy_table_cannot_be_opened_on_a_logon() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();

        let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
        // Point the table request at index 0 — the logon, not a folder.
        rops.extend(vec![ROP_GET_HIERARCHY_TABLE, 0x00, 0x00, 0x02, 0x00]);
        let buffer = RopBuffer {
            rops,
            handles: vec![crate::rop::HANDLE_UNSET; 3],
        };

        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        let table = &out.responses[166..];
        assert_eq!(table.len(), 6, "a failure response");
        assert_eq!(
            u32::from_le_bytes(table[2..6].try_into().unwrap()),
            error::INVALID_OBJECT
        );
        assert_eq!(objects.len(), 1, "only the logon should exist");
    }

    /// A leaf folder reports no children — an empty table, not an error.
    #[test]
    fn a_leaf_folder_reports_an_empty_hierarchy() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();

        let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
        rops.extend(open_folder_rop(fid_of(SpecialFolder::Inbox)));
        rops.extend(hierarchy_rop());
        let buffer = RopBuffer {
            rops,
            handles: vec![crate::rop::HANDLE_UNSET; 3],
        };

        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        let table = &out.responses[174..];
        assert_eq!(
            u32::from_le_bytes(table[2..6].try_into().unwrap()),
            error::SUCCESS,
            "an empty folder is not an error"
        );
        assert_eq!(u32::from_le_bytes(table[6..10].try_into().unwrap()), 0);
    }

    /// A `RopSetColumns` on the table at handle index 2.
    fn set_columns_rop(tags: &[PropertyTag]) -> Vec<u8> {
        let mut out = vec![ROP_SET_COLUMNS, 0x00, 0x02, 0x00];
        out.extend_from_slice(&u16::try_from(tags.len()).unwrap().to_le_bytes());
        for tag in tags {
            out.extend_from_slice(&tag.to_bytes());
        }
        out
    }

    /// The full chain a client performs before it can read a folder tree:
    /// log on, open the subtree, take its hierarchy table, and tell the table
    /// which columns its rows will carry — four operations, one buffer.
    #[test]
    fn a_client_can_set_the_columns_of_a_hierarchy_table() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();
        let wanted = [
            PropertyTag {
                property_type: 0x001F,
                property_id: 0x3001,
            },
            PropertyTag {
                property_type: 0x0003,
                property_id: 0x3602,
            },
        ];

        let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
        rops.extend(open_folder_rop(fid_of(SpecialFolder::IpmSubtree)));
        rops.extend(hierarchy_rop());
        rops.extend(set_columns_rop(&wanted));
        let buffer = RopBuffer {
            rops,
            handles: vec![crate::rop::HANDLE_UNSET; 3],
        };

        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        assert!(out.complete, "the walk stopped early");
        assert_eq!(out.responses.len(), 166 + 8 + 10 + 7);

        let set = &out.responses[184..];
        assert_eq!(set[0], ROP_SET_COLUMNS);
        assert_eq!(
            u32::from_le_bytes(set[2..6].try_into().unwrap()),
            error::SUCCESS
        );

        // The table remembers them, in order — it is the schema of every row.
        assert_eq!(
            objects.get(out.handles[2]),
            Some(&ServerObject::HierarchyTable {
                folder_id: fid_of(SpecialFolder::IpmSubtree),
                columns: wanted.to_vec(),
                cursor: 0,
            })
        );
    }

    /// Columns are set on a table. A **folder** is not one, and configuring
    /// the wrong kind of object is refused rather than quietly ignored — a
    /// client that believed its columns were set would misread every row.
    #[test]
    fn columns_cannot_be_set_on_a_folder() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();

        let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
        rops.extend(open_folder_rop(fid_of(SpecialFolder::IpmSubtree)));
        // Index 1 is the folder, not a table.
        let mut set = vec![ROP_SET_COLUMNS, 0x00, 0x01, 0x00];
        set.extend_from_slice(&0u16.to_le_bytes());
        rops.extend(set);
        let buffer = RopBuffer {
            rops,
            handles: vec![crate::rop::HANDLE_UNSET; 3],
        };

        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        let response = &out.responses[174..];
        assert_eq!(response.len(), 6, "a failure response");
        assert_eq!(
            u32::from_le_bytes(response[2..6].try_into().unwrap()),
            error::INVALID_OBJECT
        );
    }

    /// A `RopQueryRows` on the table at index 2.
    fn query_rows_rop(count: u16) -> Vec<u8> {
        let mut out = vec![ROP_QUERY_ROWS, 0x00, 0x02, 0x00, 0x01];
        out.extend_from_slice(&count.to_le_bytes());
        out
    }

    /// **The whole of stage 3 in one buffer**: log on, open the subtree, take
    /// its hierarchy table, set the columns, and read the rows. This is what
    /// Outlook does to draw a folder tree.
    #[test]
    fn a_client_can_read_a_folder_tree_in_one_buffer() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();
        let name = PropertyTag {
            property_type: crate::rows::ptyp::STRING,
            property_id: pid::DISPLAY_NAME,
        };
        let id = PropertyTag {
            property_type: crate::rows::ptyp::INTEGER64,
            property_id: pid::FOLDER_ID,
        };

        let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
        rops.extend(open_folder_rop(fid_of(SpecialFolder::IpmSubtree)));
        rops.extend(hierarchy_rop());
        rops.extend(set_columns_rop(&[name, id]));
        rops.extend(query_rows_rop(50));
        let buffer = RopBuffer {
            rops,
            handles: vec![crate::rop::HANDLE_UNSET; 3],
        };

        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        assert!(out.complete, "the walk stopped early");

        let query = &out.responses[191..];
        assert_eq!(query[0], ROP_QUERY_ROWS);
        assert_eq!(
            u32::from_le_bytes(query[2..6].try_into().unwrap()),
            error::SUCCESS
        );
        assert_eq!(query[6], ORIGIN_END, "the whole table was read");
        assert_eq!(
            u16::from_le_bytes(query[7..9].try_into().unwrap()),
            5,
            "Inbox, Outbox, Sent Items, Deleted Items, and Facturen — the \
             folder this person made, which is the point of reading the store"
        );

        // The first row: flag byte, then "Inbox" in UTF-16LE, then its id.
        let rows = &query[9..];
        assert_eq!(rows[0], 0x00, "a standard row");
        let expected_name: Vec<u8> = "Inbox"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .chain([0, 0])
            .collect();
        assert_eq!(&rows[1..1 + expected_name.len()], &expected_name[..]);
        let at = 1 + expected_name.len();
        assert_eq!(
            u64::from_le_bytes(rows[at..at + 8].try_into().unwrap()),
            fid_of(SpecialFolder::Inbox),
            "the id a client would use to open the Inbox"
        );
    }

    /// The cursor advances: asking twice returns the next rows, not the same
    /// ones. A table that reset every read would loop a client forever.
    #[test]
    fn reading_twice_advances_the_cursor() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();
        let name = PropertyTag {
            property_type: crate::rows::ptyp::STRING,
            property_id: pid::DISPLAY_NAME,
        };

        let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
        rops.extend(open_folder_rop(fid_of(SpecialFolder::IpmSubtree)));
        rops.extend(hierarchy_rop());
        rops.extend(set_columns_rop(&[name]));
        rops.extend(query_rows_rop(2));
        rops.extend(query_rows_rop(2));
        rops.extend(query_rows_rop(2));
        let buffer = RopBuffer {
            rops,
            handles: vec![crate::rop::HANDLE_UNSET; 3],
        };

        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        assert!(out.complete);

        // Walk the three query responses, counting rows and reading names.
        let mut at = 191;
        let mut names = Vec::new();
        for expected in [2u16, 2, 1] {
            let query = &out.responses[at..];
            let count = u16::from_le_bytes(query[7..9].try_into().unwrap());
            assert_eq!(count, expected, "row count at offset {at}");
            let mut cursor = 9;
            for _ in 0..count {
                cursor += 1; // the flag byte
                let start = cursor;
                while query[cursor] != 0 || query[cursor + 1] != 0 {
                    cursor += 2;
                }
                let units: Vec<u16> = query[start..cursor]
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                names.push(String::from_utf16(&units).unwrap());
                cursor += 2; // the terminator
            }
            at += cursor;
        }
        // The order is the view's: the protocol's folders first, then the
        // mailboxes the person owns.
        assert_eq!(
            names.len(),
            5,
            "the cursor did not advance, or advanced wrongly"
        );
        assert!(names.contains(&"Inbox".to_owned()), "{names:?}");
        assert!(names.contains(&"Facturen".to_owned()), "{names:?}");
    }

    /// A property we cannot answer is refused rather than filled with a zero.
    ///
    /// Counts are answerable now that the store is read, so this asks for a
    /// property the adapter genuinely does not serve. The `0x00` flag on a
    /// standard row promises every value is present and without error, so a
    /// gap changes the shape of the row — it is not something to paper over
    /// with a plausible default the client would believe.
    #[test]
    fn a_property_we_cannot_answer_is_refused_not_invented() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();
        // A property this adapter does not serve — not a count, which it now
        // answers for every folder in the view.
        let unserved = PropertyTag {
            property_type: crate::rows::ptyp::INTEGER32,
            property_id: 0x0E08,
        };

        let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
        // The root's children include Views and Shortcuts, which no mailbox
        // stands behind.
        rops.extend(open_folder_rop(fid_of(SpecialFolder::Root)));
        rops.extend(hierarchy_rop());
        rops.extend(set_columns_rop(&[unserved]));
        rops.extend(query_rows_rop(10));
        let buffer = RopBuffer {
            rops,
            handles: vec![crate::rop::HANDLE_UNSET; 3],
        };

        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        let query = &out.responses[191..];
        assert_eq!(query.len(), 6, "a failure response");
        assert_eq!(
            u32::from_le_bytes(query[2..6].try_into().unwrap()),
            error::NOT_IMPLEMENTED
        );
    }

    #[test]
    fn a_caller_may_open_their_own_mailbox() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();
        let buffer = logon_buffer("/o=alo/cn=disan", LOGON_PRIVATE, 0);

        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        assert!(out.complete);
        assert_eq!(code_of(&out.responses), error::SUCCESS);
        assert_eq!(out.responses.len(), 166, "a full logon response");

        // A handle was allocated and written into the slot the request named.
        assert_eq!(objects.len(), 1);
        assert_ne!(out.handles[0], crate::rop::HANDLE_UNSET);
        assert!(objects.get(out.handles[0]).is_some());
    }

    /// An empty `Essdn` means "my own mailbox", which is what a client sends
    /// when it has nothing better to say.
    #[test]
    fn an_empty_essdn_opens_the_callers_own_mailbox() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();
        let buffer = logon_buffer("", LOGON_PRIVATE, 0);
        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        assert_eq!(code_of(&out.responses), error::SUCCESS);
    }

    /// **The wrong-mailbox test.** A caller who authenticated perfectly well
    /// cannot name somebody else's mailbox — the decision is made from who they
    /// proved to be, not from what the request claims.
    #[test]
    fn an_authenticated_caller_cannot_open_another_mailbox() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();

        for other in [
            "/o=alo/cn=kevin",
            "/o=alo/cn=admin",
            "/o=other/cn=disan",
            "/o=alo/cn=disan/cn=extra",
        ] {
            let buffer = logon_buffer(other, LOGON_PRIVATE, 0);
            let out = dispatch(
                &ctx,
                "/o=alo",
                &mut objects,
                &view(),
                &MessageView::new(),
                &buffer,
                LogonTime::default(),
            );
            assert_eq!(
                code_of(&out.responses),
                error::ACCESS_DENIED,
                "opened {other}"
            );
            // Nothing was created for a refused logon.
            assert!(objects.is_empty(), "an object survived a refusal");
            assert_eq!(out.responses.len(), 6, "a failure is six bytes");
        }
    }

    /// The comparison ignores case, because a client echoing its own name back
    /// differently is not an attacker.
    #[test]
    fn the_distinguished_name_comparison_ignores_case() {
        let ctx = context("Disan@Alo.Test");
        assert!(may_open(&ctx, "/o=alo", "/O=ALO/CN=DISAN"));
        assert!(may_open(&ctx, "/o=alo", "/o=alo/cn=disan"));
        assert!(!may_open(&ctx, "/o=alo", "/o=alo/cn=disanx"));
    }

    /// Public-folder logons are a later stage, refused by name rather than
    /// answered as though the store were empty.
    #[test]
    fn a_public_folder_logon_is_refused_as_unimplemented() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();
        let buffer = logon_buffer("", 0, OPEN_PUBLIC);
        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        assert_eq!(code_of(&out.responses), error::NOT_IMPLEMENTED);
    }

    /// An operation we cannot parse ends the walk. Its body length is unknown,
    /// so resuming would mean guessing where the next request starts — and
    /// acting on whatever that guess produced.
    #[test]
    fn an_unimplemented_operation_is_answered_and_stops_the_walk() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();
        let buffer = RopBuffer {
            // `RopGetPropertiesAll` (0x08), which is not built, then a logon
            // that must NOT be reached. This was `RopRelease` until releasing
            // became a real operation — the example has to be one we genuinely
            // do not implement, or the test proves nothing about stopping.
            rops: vec![0x08, 0x00, 0x00, ROP_LOGON, 0x00, 0x00],
            handles: vec![7],
        };

        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        assert!(!out.complete, "claimed to have finished the list");
        assert_eq!(out.responses.len(), 6, "one failure, and nothing after it");
        assert_eq!(out.responses[0], 0x08, "answered the operation it saw");
        assert_eq!(code_of(&out.responses), error::NOT_IMPLEMENTED);
        assert!(objects.is_empty(), "the unreached logon was performed");
    }

    /// Handles start at one: zero and `0xFFFFFFFF` both mean "no object" in
    /// places a client reads, so a handle equal to either is indistinguishable
    /// from an absent one.
    #[test]
    fn handles_are_never_zero_or_the_unset_marker() {
        let mut objects = ObjectTable::new();
        for _ in 0..64 {
            let handle = objects.insert(ServerObject::Logon {
                logon_id: 0,
                login: "x@alo.test".to_owned(),
            });
            assert_ne!(handle, 0);
            assert_ne!(handle, crate::rop::HANDLE_UNSET);
        }
        assert_eq!(objects.len(), 64);
    }

    #[test]
    fn a_truncated_operation_ends_the_walk_without_inventing_a_response() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();
        let buffer = RopBuffer {
            rops: vec![ROP_LOGON, 0x00],
            handles: vec![],
        };
        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        assert!(!out.complete);
        assert!(
            out.responses.is_empty(),
            "answered an operation it never read"
        );
    }

    #[test]
    fn an_empty_list_is_a_complete_walk() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();
        let buffer = RopBuffer {
            rops: Vec::new(),
            handles: vec![1, 2],
        };
        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        assert!(out.complete);
        assert!(out.responses.is_empty());
        assert_eq!(out.handles, vec![1, 2], "the table is returned unchanged");
    }

    // ---- stage 4: the messages in a folder --------------------------------

    /// A `RopGetContentsTable` on the folder at handle index 1, storing the
    /// table at index 2 — the same slots the hierarchy tests use, so the two
    /// chains are directly comparable.
    fn contents_rop(table_flags: u8) -> Vec<u8> {
        vec![ROP_GET_CONTENTS_TABLE, 0x00, 0x01, 0x02, table_flags]
    }

    fn message_row(id: &str, subject: &str, seen: bool, attachment: bool) -> MapiMessageRow {
        MapiMessageRow {
            id: alo_store::MessageId::new(id.to_owned()),
            subject: subject.to_owned(),
            from_addr: "Müller <m@example.test>".to_owned(),
            received_at: time::OffsetDateTime::from_unix_timestamp(1_787_713_200).unwrap(),
            size: 2048,
            seen,
            has_attachment: attachment,
        }
    }

    /// The messages loaded for the folder a test is about to read.
    fn loaded(folder_id: u64, rows: &[MapiMessageRow]) -> MessageView {
        let mut view = MessageView::new();
        view.insert(folder_id, rows);
        view
    }

    /// The columns Outlook asks a message list for: who it is from, what it is
    /// about, when it arrived, and whether it has been read.
    fn message_columns() -> Vec<PropertyTag> {
        vec![
            PropertyTag {
                property_type: crate::rows::ptyp::INTEGER64,
                property_id: pid::MID,
            },
            PropertyTag {
                property_type: crate::rows::ptyp::STRING,
                property_id: pid::SUBJECT,
            },
            PropertyTag {
                property_type: crate::rows::ptyp::STRING,
                property_id: pid::SENDER_NAME,
            },
            PropertyTag {
                property_type: crate::rows::ptyp::TIME,
                property_id: pid::MESSAGE_DELIVERY_TIME,
            },
            PropertyTag {
                property_type: crate::rows::ptyp::INTEGER32,
                property_id: pid::MESSAGE_FLAGS,
            },
        ]
    }

    /// **The whole of stage 4 in one buffer**: log on, open the inbox, take its
    /// contents table, set the columns, and read the rows. This is what Outlook
    /// does to list the messages in a folder.
    #[test]
    fn a_client_can_read_the_messages_in_a_folder_in_one_buffer() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();
        let inbox = fid_of(SpecialFolder::Inbox);
        let messages = loaded(
            inbox,
            &[
                message_row("m-1", "Rechnung", false, false),
                message_row("m-2", "Liège", true, true),
            ],
        );

        let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
        rops.extend(open_folder_rop(inbox));
        rops.extend(contents_rop(0x00));
        rops.extend(set_columns_rop(&message_columns()));
        rops.extend(query_rows_rop(50));
        let buffer = RopBuffer {
            rops,
            handles: vec![crate::rop::HANDLE_UNSET; 3],
        };

        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &messages,
            &buffer,
            LogonTime::default(),
        );
        assert!(out.complete, "the walk stopped early");

        // The contents-table response reports what the folder holds.
        let table = &out.responses[174..184];
        assert_eq!(table[0], ROP_GET_CONTENTS_TABLE);
        assert_eq!(
            u32::from_le_bytes(table[2..6].try_into().unwrap()),
            error::SUCCESS
        );
        assert_eq!(u32::from_le_bytes(table[6..10].try_into().unwrap()), 2);

        // Then the rows themselves.
        let rows = &out.responses[191..];
        assert_eq!(rows[0], ROP_QUERY_ROWS);
        assert_eq!(
            u32::from_le_bytes(rows[2..6].try_into().unwrap()),
            error::SUCCESS
        );
        assert_eq!(rows[6], ORIGIN_END, "both messages were read");
        assert_eq!(u16::from_le_bytes(rows[7..9].try_into().unwrap()), 2);

        // First row: flag byte, then MID, subject, sender, time, flags.
        let first = &rows[9..];
        assert_eq!(first[0], 0x00, "a standard row, every value present");
        let mid = u64::from_le_bytes(first[1..9].try_into().unwrap());
        assert!(mid != 0, "a MID a client can ask us to open");

        let subject = utf16_at(first, 9);
        assert_eq!(subject.0, "Rechnung");
        let sender = utf16_at(first, subject.1);
        assert!(sender.0.contains("Müller"), "{}", sender.0);

        let time = u64::from_le_bytes(first[sender.1..sender.1 + 8].try_into().unwrap());
        assert_eq!(time, (1_787_713_200 + 11_644_473_600) * 10_000_000);
        let flags = u32::from_le_bytes(first[sender.1 + 8..sender.1 + 12].try_into().unwrap());
        assert_eq!(flags & crate::rows::mf::READ, 0, "unread");
    }

    /// Reads a UTF-16LE null-terminated string at `at`, returning it with the
    /// offset just past its terminator.
    fn utf16_at(bytes: &[u8], at: usize) -> (String, usize) {
        let mut units = Vec::new();
        let mut i = at;
        loop {
            let unit = u16::from_le_bytes([bytes[i], bytes[i + 1]]);
            i += 2;
            if unit == 0 {
                break;
            }
            units.push(unit);
        }
        (String::from_utf16(&units).expect("utf-16"), i)
    }

    /// A contents table is opened **on a folder**. A logon is not one, and
    /// asking the wrong kind of object for its messages is refused rather than
    /// answered with an empty table — which a client would cache as "this
    /// folder has no mail in it".
    #[test]
    fn a_contents_table_cannot_be_opened_on_a_logon() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();

        let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
        // Index 0 is the logon, not a folder.
        rops.extend(vec![ROP_GET_CONTENTS_TABLE, 0x00, 0x00, 0x02, 0x00]);
        let buffer = RopBuffer {
            rops,
            handles: vec![crate::rop::HANDLE_UNSET; 3],
        };

        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        let response = &out.responses[166..];
        assert_eq!(response.len(), 6, "a failure response");
        assert_eq!(response[0], ROP_GET_CONTENTS_TABLE);
        assert_eq!(
            u32::from_le_bytes(response[2..6].try_into().unwrap()),
            error::INVALID_OBJECT
        );
    }

    /// A folder whose messages were never loaded is refused rather than
    /// reported empty. An empty page here is a claim a client caches, and it
    /// would be a claim we had not checked.
    #[test]
    fn an_unloaded_folder_refuses_rather_than_claiming_to_be_empty() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();

        let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
        rops.extend(open_folder_rop(fid_of(SpecialFolder::Inbox)));
        rops.extend(contents_rop(0x00));
        rops.extend(set_columns_rop(&message_columns()));
        rops.extend(query_rows_rop(50));
        let buffer = RopBuffer {
            rops,
            handles: vec![crate::rop::HANDLE_UNSET; 3],
        };

        // Nothing loaded — the state the rehearsal runs in.
        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        assert!(out.complete);
        let rows = &out.responses[191..];
        assert_eq!(rows.len(), 6, "a failure response");
        assert_eq!(
            u32::from_le_bytes(rows[2..6].try_into().unwrap()),
            error::NOT_FOUND
        );
    }

    /// The rehearsal reports the folder a buffer is about to read, even though
    /// it runs with nothing loaded — which is the whole point of it, and the
    /// only way the router can know what to fetch before dispatching.
    #[test]
    fn the_rehearsal_names_the_folder_a_buffer_will_read() {
        let ctx = context("disan@alo.test");
        let objects = ObjectTable::new();
        let inbox = fid_of(SpecialFolder::Inbox);

        let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
        rops.extend(open_folder_rop(inbox));
        rops.extend(contents_rop(0x00));
        let buffer = RopBuffer {
            rops,
            handles: vec![crate::rop::HANDLE_UNSET; 3],
        };

        let wanted = wanted_contents(
            &ctx,
            "/o=alo",
            &objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        assert_eq!(wanted.folders, vec![inbox]);
        assert!(wanted.messages.is_empty(), "nothing was opened");
        // And the session's own table is untouched by the rehearsal.
        assert!(objects.is_empty());
    }

    /// A buffer that opens no contents table asks for nothing, so drawing a
    /// folder tree costs no message reads at all.
    #[test]
    fn a_buffer_that_reads_no_messages_asks_for_none() {
        let ctx = context("disan@alo.test");
        let objects = ObjectTable::new();

        let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
        rops.extend(open_folder_rop(fid_of(SpecialFolder::IpmSubtree)));
        rops.extend(hierarchy_rop());
        let buffer = RopBuffer {
            rops,
            handles: vec![crate::rop::HANDLE_UNSET; 3],
        };

        assert!(
            wanted_contents(
                &ctx,
                "/o=alo",
                &objects,
                &view(),
                &MessageView::new(),
                &buffer,
                LogonTime::default(),
            )
            .folders
            .is_empty()
        );
    }

    /// An associated (FAI) table is empty and needs nothing read: alo keeps no
    /// FAI messages, so the truthful answer costs no query.
    #[test]
    fn an_associated_table_is_empty_and_loads_nothing() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();

        let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
        rops.extend(open_folder_rop(fid_of(SpecialFolder::Inbox)));
        rops.extend(contents_rop(crate::contents::TABLE_FLAG_ASSOCIATED));
        rops.extend(set_columns_rop(&message_columns()));
        rops.extend(query_rows_rop(50));
        let buffer = RopBuffer {
            rops,
            handles: vec![crate::rop::HANDLE_UNSET; 3],
        };

        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &MessageView::new(),
            &buffer,
            LogonTime::default(),
        );
        assert!(out.complete);
        assert!(
            out.contents_folders.is_empty(),
            "an associated table needs no message read"
        );
        // Success with no rows, not a refusal: the emptiness is the truth.
        let rows = &out.responses[191..];
        assert_eq!(
            u32::from_le_bytes(rows[2..6].try_into().unwrap()),
            error::SUCCESS
        );
        assert_eq!(u16::from_le_bytes(rows[7..9].try_into().unwrap()), 0);
    }

    /// The cursor advances, so a client paging through a folder gets the next
    /// messages rather than the same ones again.
    #[test]
    fn reading_twice_advances_through_the_messages() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();
        let inbox = fid_of(SpecialFolder::Inbox);
        let messages = loaded(
            inbox,
            &[
                message_row("m-1", "one", false, false),
                message_row("m-2", "two", false, false),
                message_row("m-3", "three", false, false),
            ],
        );
        let columns = vec![PropertyTag {
            property_type: crate::rows::ptyp::STRING,
            property_id: pid::SUBJECT,
        }];

        let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
        rops.extend(open_folder_rop(inbox));
        rops.extend(contents_rop(0x00));
        rops.extend(set_columns_rop(&columns));
        rops.extend(query_rows_rop(2));
        rops.extend(query_rows_rop(2));
        let buffer = RopBuffer {
            rops,
            handles: vec![crate::rop::HANDLE_UNSET; 3],
        };

        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &messages,
            &buffer,
            LogonTime::default(),
        );
        assert!(out.complete);

        // First read: two rows, and not at the end.
        let first = &out.responses[191..];
        assert_eq!(first[6], ORIGIN_BEGINNING);
        assert_eq!(u16::from_le_bytes(first[7..9].try_into().unwrap()), 2);
        let one = utf16_at(first, 10);
        assert_eq!(one.0, "one");
        let two = utf16_at(first, one.1 + 1);
        assert_eq!(two.0, "two");

        // Second read: the remaining one, and now at the end.
        let second = &first[9 + (one.1 - 10) + 1 + (two.1 - one.1 - 1) + 1..];
        assert_eq!(second[0], ROP_QUERY_ROWS);
        assert_eq!(second[6], ORIGIN_END);
        assert_eq!(u16::from_le_bytes(second[7..9].try_into().unwrap()), 1);
        assert_eq!(utf16_at(second, 10).0, "three");
    }

    /// A column we cannot answer refuses the read rather than filling the gap.
    /// A zero in a column a client asked for is a value it believes.
    #[test]
    fn a_message_column_we_cannot_answer_is_refused() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();
        let inbox = fid_of(SpecialFolder::Inbox);
        let messages = loaded(inbox, &[message_row("m-1", "one", false, false)]);
        // PidTagBody: a real property, and not one a contents table carries.
        let columns = vec![PropertyTag {
            property_type: crate::rows::ptyp::STRING,
            property_id: 0x1000,
        }];

        let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
        rops.extend(open_folder_rop(inbox));
        rops.extend(contents_rop(0x00));
        rops.extend(set_columns_rop(&columns));
        rops.extend(query_rows_rop(50));
        let buffer = RopBuffer {
            rops,
            handles: vec![crate::rop::HANDLE_UNSET; 3],
        };

        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &messages,
            &buffer,
            LogonTime::default(),
        );
        let rows = &out.responses[191..];
        assert_eq!(rows.len(), 6, "a failure response");
        assert_eq!(
            u32::from_le_bytes(rows[2..6].try_into().unwrap()),
            error::NOT_IMPLEMENTED
        );
    }

    /// A hierarchy table and a contents table opened on the same folder are
    /// different tables with independent cursors. Sharing one would make
    /// reading the folder list disturb the message list.
    #[test]
    fn the_two_kinds_of_table_do_not_share_a_cursor() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();
        let subtree = fid_of(SpecialFolder::IpmSubtree);
        let messages = loaded(subtree, &[message_row("m-1", "one", false, false)]);

        let mut rops = logon_buffer("", LOGON_PRIVATE, 0).rops;
        rops.extend(open_folder_rop(subtree));
        rops.extend(hierarchy_rop());
        rops.extend(contents_rop(0x00));
        let buffer = RopBuffer {
            rops,
            handles: vec![crate::rop::HANDLE_UNSET; 4],
        };

        let out = dispatch(
            &ctx,
            "/o=alo",
            &mut objects,
            &view(),
            &messages,
            &buffer,
            LogonTime::default(),
        );
        assert!(out.complete);
        // Both tables landed in slot 2 in turn; the second replaced the handle
        // there, and both objects exist independently in the table.
        assert!(matches!(
            objects.get(out.handles[2]),
            Some(ServerObject::ContentsTable { .. })
        ));
        assert_eq!(objects.len(), 4, "logon, folder, and two tables");
    }
}
