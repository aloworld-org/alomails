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

use crate::logon::LogonRequest;
use crate::logon_response::{
    Fid, LogonResponse, LogonTime, RESPONSE_OWNER_RIGHT, RESPONSE_SEND_AS_RIGHT, ROP_LOGON,
    SpecialFolder,
};
use crate::openfolder::{OpenFolderRequest, ROP_OPEN_FOLDER, success_body as open_folder_success};
use crate::rop::{RopBuffer, RopHeader};
use crate::session::SessionContext;

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
        /// Which special folder this is.
        folder: SpecialFolder,
    },
}

/// The special folder a folder id names, if we issued it.
///
/// Folder ids we did not issue resolve to nothing. The replica must match and
/// the counter must be one this deployment hands out — a client that invents an
/// id gets `ecNotFound`, not a folder, and certainly not somebody else's.
#[must_use]
pub fn folder_for_id(folder_id: u64) -> Option<SpecialFolder> {
    let replica = u16::try_from(folder_id & 0xFFFF).ok()?;
    if replica != REPLICA_ID {
        return None;
    }
    let counter = folder_id >> 16;
    // Counters are issued as the slot number plus one, so zero is not one of
    // ours and neither is anything past the last folder.
    let slot = usize::try_from(counter.checked_sub(1)?).ok()?;
    SpecialFolder::ALL.get(slot).copied()
}

/// The server objects one Session Context holds.
///
/// Handles are allocated per session and never reused within it: a handle that
/// came back after its object was released would let a stale client reach
/// whatever took its place.
#[derive(Debug, Default)]
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

    /// The object a handle names, if this session has one.
    #[must_use]
    pub fn get(&self, handle: u32) -> Option<&ServerObject> {
        self.objects
            .iter()
            .find(|(stored, _)| *stored == handle)
            .map(|(_, object)| object)
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
    input: &RopBuffer,
    now: LogonTime,
) -> Dispatched {
    let mut responses = Vec::new();
    let mut handles = input.handles.clone();
    let mut rest: &[u8] = &input.rops;

    while !rest.is_empty() {
        let Ok(header) = RopHeader::parse(rest) else {
            // Fewer than three bytes left: the list is malformed rather than
            // finished, and there is no operation here to answer.
            return Dispatched {
                responses,
                handles,
                complete: false,
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

                // A folder id we did not issue is not a folder. Answered as
                // "not found" rather than opened as something nearby.
                let Some(folder) = folder_for_id(request.folder_id) else {
                    responses.extend(failure(
                        ROP_OPEN_FOLDER,
                        request.output_handle_index,
                        error::NOT_FOUND,
                    ));
                    continue;
                };

                let handle = objects.insert(ServerObject::Folder { folder });
                let index = usize::from(request.output_handle_index);
                if handles.len() <= index {
                    handles.resize(index + 1, crate::rop::HANDLE_UNSET);
                }
                handles[index] = handle;

                // No rules yet, and never ghosted: we are the only replica of
                // everything we serve.
                responses.extend(open_folder_success(request.output_handle_index, false));
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
                };
            }
        }
    }

    Dispatched {
        responses,
        handles,
        complete: true,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::logon::{LOGON_PRIVATE, OPEN_PUBLIC};
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
        let fid = Fid::new(REPLICA_ID, folder.slot() as u64 + 1).unwrap();
        u64::from_le_bytes(fid.to_bytes())
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

        let out = dispatch(&ctx, "/o=alo", &mut objects, &buffer, LogonTime::default());
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
                folder: SpecialFolder::Inbox
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

        let out = dispatch(&ctx, "/o=alo", &mut objects, &buffer, LogonTime::default());
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
            let out = dispatch(&ctx, "/o=alo", &mut objects, &buffer, LogonTime::default());
            let folder_response = &out.responses[166..];
            assert_eq!(
                u32::from_le_bytes(folder_response[2..6].try_into().unwrap()),
                error::NOT_FOUND,
                "opened an invented folder id {invented:#x}"
            );
        }
    }

    /// Every id the logon hands out resolves back to the folder it names —
    /// the round trip a client actually performs.
    #[test]
    fn every_folder_id_the_logon_issues_resolves_back() {
        for folder in SpecialFolder::ALL {
            assert_eq!(
                folder_for_id(fid_of(folder)),
                Some(folder),
                "{folder:?} did not resolve"
            );
        }
    }

    #[test]
    fn a_caller_may_open_their_own_mailbox() {
        let ctx = context("disan@alo.test");
        let mut objects = ObjectTable::new();
        let buffer = logon_buffer("/o=alo/cn=disan", LOGON_PRIVATE, 0);

        let out = dispatch(&ctx, "/o=alo", &mut objects, &buffer, LogonTime::default());
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
        let out = dispatch(&ctx, "/o=alo", &mut objects, &buffer, LogonTime::default());
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
            let out = dispatch(&ctx, "/o=alo", &mut objects, &buffer, LogonTime::default());
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
        let out = dispatch(&ctx, "/o=alo", &mut objects, &buffer, LogonTime::default());
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
            // RopRelease, then a logon that must NOT be reached.
            rops: vec![0x01, 0x00, 0x00, ROP_LOGON, 0x00, 0x00],
            handles: vec![7],
        };

        let out = dispatch(&ctx, "/o=alo", &mut objects, &buffer, LogonTime::default());
        assert!(!out.complete, "claimed to have finished the list");
        assert_eq!(out.responses.len(), 6, "one failure, and nothing after it");
        assert_eq!(out.responses[0], 0x01, "answered the operation it saw");
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
        let out = dispatch(&ctx, "/o=alo", &mut objects, &buffer, LogonTime::default());
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
        let out = dispatch(&ctx, "/o=alo", &mut objects, &buffer, LogonTime::default());
        assert!(out.complete);
        assert!(out.responses.is_empty());
        assert_eq!(out.handles, vec![1, 2], "the table is returned unchanged");
    }
}
