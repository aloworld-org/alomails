//! The MAPI-over-HTTP endpoints ([MS-OXCMAPIHTTP] §3.1) and the authentication
//! in front of them.
//!
//! Two endpoints exist in the protocol: the **mailbox** endpoint (`/mapi/emsmdb`)
//! and the **address book** endpoint (`/mapi/nspi`). Autodiscover names both,
//! so both must answer — the address book is a later stage and says so rather
//! than timing out, which is what an unrouted path would do.
//!
//! **Authentication is HTTP Basic, and therefore TLS is not optional.** The
//! protocol carries a mailbox password on every connection; Caddy terminates
//! TLS in front of this and the session cookie is marked `Secure`, but the
//! deployment note is worth stating where the code is: exposing this endpoint
//! without TLS hands out credentials.
//!
//! Credentials are checked with [`alo_identity::Identity::authenticate_legacy`],
//! the same door SMTP AUTH and IMAP `LOGIN` use, and for the same reason: a
//! Basic exchange has nowhere to prompt for a second factor. That door fails
//! closed for accounts with TOTP enabled and applies per-username backoff
//! across connections, so this endpoint cannot become the soft way in that
//! bypasses two-factor everywhere else.

use std::sync::Arc;

use alo_identity::Identity;
use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use time::OffsetDateTime;

use crate::connect::{ConnectRequest, MAX_CONNECT_BODY, success_body};
use crate::dispatch::{Sources, dispatch, wanted_contents};
use crate::execute::{ExecuteRequest, success_body as execute_success_body};
use crate::folders::FolderView;
use crate::logon_response::LogonTime;
use crate::messages::{AttachmentEntry, MessageBody, MessageView};
use crate::nspi;
use crate::response::{MapiResponse, ResponseCode};
use crate::rop::RopBuffer;
use crate::rows::Value;
use crate::rpc::{read_extended_buffer, write_extended_buffer};
use crate::session::{SEQUENCE_COOKIE, SESSION_COOKIE, SessionStore, cookie_value, set_cookie};
use crate::{RequestType, session};

/// How long the client should wait between polls, in milliseconds
/// (`X-PendingPeriod`, [MS-OXCMAPIHTTP] §2.2.3.3.5).
const PENDING_PERIOD_MS: u32 = 15_000;

/// The most folders one mailbox may present.
///
/// A ceiling on a page read, not on what a person may own: a mailbox with more
/// folders than this shows the first of them rather than failing, and the
/// number is far above any real mailbox. Without it a single `Execute` could
/// pull an unbounded result set into memory on every call.
const MAX_FOLDERS: i64 = 10_000;

/// The most messages read from one folder in a single `Execute`.
///
/// A client pages a table with `RopQueryRows`, so this bounds one response
/// rather than a folder. Mirrors [`crate::messages::MAX_MESSAGES`], as an
/// `i64` because that is what a store page takes.
const MAX_MESSAGES: i64 = crate::messages::MAX_MESSAGES as i64;

/// How many times a buffer is rehearsed before it is dispatched for real.
///
/// Three, because that is the depth of the object graph a rehearsal walks:
/// a folder's messages, then one message's content, then one attachment's
/// bytes. Each pass can see one layer further than the last, and a fourth
/// would have nothing new to find.
const MAX_REHEARSALS: usize = 3;

/// The largest address book request body accepted.
///
/// These bodies carry a handful of typed names and a short property list; a
/// megabyte of them is not a client we need to serve.
const MAX_ADDRESS_BOOK_BODY: usize = 256 * 1024;

/// The most directory entries one typed name may match before it is called
/// ambiguous.
///
/// Two is enough to know a name is ambiguous, but a slightly larger number
/// keeps the query's cost visible in one place and leaves room for a later
/// stage to *show* the choices rather than only report that there are some.
const MAX_RESOLVE_MATCHES: i64 = 16;

/// The GUID this deployment's address book answers `Bind` with.
///
/// A Minimal Entry ID is only meaningful against the server GUID that issued
/// it ([MS-OXNSPI] §2.2.9.1), so this is the value that scopes them. Fixed
/// rather than generated per process: a client caches entry ids, and a GUID
/// that changed on restart would silently invalidate every one of them.
const ADDRESS_BOOK_GUID: [u8; 16] = [
    0x61, 0x6C, 0x6F, 0x61, 0x64, 0x64, 0x72, 0x62, 0x6F, 0x6F, 0x6B, 0x00, 0x00, 0x00, 0x00, 0x01,
];

/// What the client is told about retrying, mirroring Exchange's own pacing.
/// A client told to retry zero times gives up on the first hiccup.
const POLLS_MAX_MS: u32 = 60_000;
const RETRY_COUNT: u32 = 3;
const RETRY_DELAY_MS: u32 = 1_000;

/// What this deployment serves the endpoints with.
#[derive(Clone)]
pub struct MapiState {
    /// The store the folder tree is read from.
    pub store: std::sync::Arc<alo_store::Store>,
    /// The credential door — the same one SMTP and IMAP use.
    pub identity: Identity,
    /// Live Session Contexts.
    pub sessions: Arc<SessionStore>,
    /// The DN prefix handed to clients for building recipients.
    pub dn_prefix: String,
    /// The deployment's trusted submission listener, when sending is
    /// configured. `None` means this deployment does not send, and a client
    /// that tries is refused rather than left waiting.
    pub submission_addr: Option<String>,
}

/// The `/mapi/*` routes.
///
/// `POST` only: the protocol defines no other verb on these paths, and axum
/// answers anything else with `405` before a handler runs.
pub fn router(state: MapiState) -> Router {
    Router::new()
        .route("/mapi/emsmdb", post(emsmdb))
        .route("/mapi/emsmdb/", post(emsmdb))
        .route("/mapi/nspi", post(nspi))
        .route("/mapi/nspi/", post(nspi))
        .with_state(state)
}

/// A `401` that tells the client how to authenticate.
///
/// The specification reserves non-200 for exactly this, and the challenge is
/// what makes Outlook prompt rather than fail. The realm is a fixed string: it
/// is displayed to the user and must never carry anything caller-supplied.
fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, r#"Basic realm="alo""#)],
    )
        .into_response()
}

/// The username and password from an HTTP Basic `Authorization` header.
///
/// Returns `None` for anything that is not well-formed Basic: a missing header,
/// another scheme, invalid base64, or no colon. Every one of those is answered
/// identically with a challenge, so nothing here distinguishes "malformed" from
/// "wrong" for a caller probing the endpoint.
fn basic_credentials(headers: &HeaderMap) -> Option<(String, String)> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let encoded = raw
        .strip_prefix("Basic ")
        .or_else(|| raw.strip_prefix("basic "))?;
    let decoded = BASE64.decode(encoded.trim()).ok()?;
    let text = String::from_utf8(decoded).ok()?;
    // Split on the FIRST colon: a password may contain colons, a username
    // may not ([RFC 7617] §2).
    let (user, password) = text.split_once(':')?;
    Some((user.to_owned(), password.to_owned()))
}

/// One header as a string, if it is present and is text at all.
fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// The mailbox endpoint (`/mapi/emsmdb`).
async fn emsmdb(State(state): State<MapiState>, headers: HeaderMap, body: Bytes) -> Response {
    // `X-RequestId` is echoed on every answer, including the failures below, so
    // it is read before anything can reject the request.
    let request_id = header_str(&headers, "X-RequestId").unwrap_or_default();
    let client_info = header_str(&headers, "X-ClientInfo");

    let Some(raw_type) = header_str(&headers, "X-RequestType") else {
        return MapiResponse::new("", request_id, ResponseCode::MissingHeader)
            .with_client_info(client_info)
            .into_response();
    };
    let Some(request_type) = RequestType::parse(raw_type) else {
        return MapiResponse::new("", request_id, ResponseCode::InvalidRequestType)
            .with_client_info(client_info)
            .into_response();
    };

    // Credentials before anything reads the body: an unauthenticated caller
    // learns nothing about what we would have done with it.
    let Some((username, password)) = basic_credentials(&headers) else {
        return unauthorized();
    };
    let principal = match state
        .identity
        .authenticate_legacy(&username, &password)
        .await
    {
        Ok(Some(principal)) => principal,
        // Wrong password, unknown user, and a 2FA account that cannot answer a
        // Basic exchange are one answer: a challenge, with nothing to tell them
        // apart. `authenticate_legacy` already makes them indistinguishable in
        // time as well.
        Ok(None) => return unauthorized(),
        Err(error) => {
            // No username, no password, no mailbox in the log line.
            tracing::warn!(%error, "mapi: credential lookup failed");
            return MapiResponse::new(
                request_type.as_str(),
                request_id,
                ResponseCode::UnknownFailure,
            )
            .with_client_info(client_info)
            .into_response();
        }
    };

    let now = OffsetDateTime::now_utc();
    match request_type {
        RequestType::Connect => {
            if body.len() > MAX_CONNECT_BODY {
                return MapiResponse::new(
                    request_type.as_str(),
                    request_id,
                    ResponseCode::TooLarge,
                )
                .with_client_info(client_info)
                .into_response();
            }
            let request = match ConnectRequest::parse(&body) {
                Ok(request) => request,
                Err(error) => {
                    // The reason is ours to keep; the client is told only that
                    // the body was invalid, which is all the protocol defines.
                    tracing::debug!(%error, "mapi: malformed Connect body");
                    return MapiResponse::new(
                        request_type.as_str(),
                        request_id,
                        ResponseCode::InvalidRequestBody,
                    )
                    .with_client_info(client_info)
                    .into_response();
                }
            };

            let Some(token) = state.sessions.establish(
                principal.tenant.clone(),
                principal.user.clone(),
                request.user_dn.clone(),
                username.clone(),
                now,
            ) else {
                // The table is full, or the CSPRNG failed. Either way we do not
                // hand back a session we cannot stand behind.
                tracing::warn!("mapi: refused a Connect, no session context available");
                return MapiResponse::new(
                    request_type.as_str(),
                    request_id,
                    ResponseCode::UnknownFailure,
                )
                .with_client_info(client_info)
                .into_response();
            };

            // The display name is the address, not a looked-up profile field:
            // this stage authenticates and hands back a context, and inventing
            // a nicer name would be data we have not actually read.
            let body = success_body(
                0,
                POLLS_MAX_MS,
                RETRY_COUNT,
                RETRY_DELAY_MS,
                &state.dn_prefix,
                &username,
            );
            MapiResponse::new(request_type.as_str(), request_id, ResponseCode::Success)
                .with_client_info(client_info)
                .with_cookie(set_cookie(SESSION_COOKIE, &token))
                // The sequence cookie is per-context and starts at one; the
                // client returns it so requests within a context can be ordered.
                .with_cookie(set_cookie(SEQUENCE_COOKIE, "1"))
                .with_header("X-PendingPeriod", PENDING_PERIOD_MS.to_string())
                .with_header(
                    "X-ExpirationInfo",
                    (session::CONTEXT_LIFETIME.whole_milliseconds()).to_string(),
                )
                .with_body(body)
                .into_response()
        }

        RequestType::Disconnect => {
            let Some(cookie) = header_str(&headers, "cookie") else {
                return MapiResponse::new(
                    request_type.as_str(),
                    request_id,
                    ResponseCode::MissingCookie,
                )
                .with_client_info(client_info)
                .into_response();
            };
            let Some(token) = cookie_value(cookie, SESSION_COOKIE) else {
                return MapiResponse::new(
                    request_type.as_str(),
                    request_id,
                    ResponseCode::MissingCookie,
                )
                .with_client_info(client_info)
                .into_response();
            };

            // **A context may only be ended by the tenant that owns it.**
            // Without this check a valid credential from tenant A could end
            // tenant B's session by quoting its cookie — the cookie is
            // unguessable, but "unguessable" is not an authorisation model.
            match state.sessions.touch(&token, now) {
                Some(context) if context.tenant == principal.tenant => {
                    state.sessions.end(&token);
                    MapiResponse::new(request_type.as_str(), request_id, ResponseCode::Success)
                        .with_client_info(client_info)
                        .with_body(disconnect_body())
                        .into_response()
                }
                Some(_) => {
                    MapiResponse::new(request_type.as_str(), request_id, ResponseCode::NoPrivilege)
                        .with_client_info(client_info)
                        .into_response()
                }
                // Already gone. A client retrying a Disconnect it completed is
                // behaving correctly, so this is a success, not an error.
                None => MapiResponse::new(request_type.as_str(), request_id, ResponseCode::Success)
                    .with_client_info(client_info)
                    .with_body(disconnect_body())
                    .into_response(),
            }
        }

        RequestType::Execute => {
            // Every operation happens inside a Session Context, so the cookie is
            // required before the body is even read.
            let context = match header_str(&headers, "cookie")
                .and_then(|cookie| cookie_value(cookie, SESSION_COOKIE))
                .and_then(|token| state.sessions.touch(&token, now))
            {
                Some(context) => context,
                None => {
                    return MapiResponse::new(
                        request_type.as_str(),
                        request_id,
                        ResponseCode::ContextNotFound,
                    )
                    .with_client_info(client_info)
                    .into_response();
                }
            };

            // **The same boundary `Disconnect` enforces.** A valid credential
            // from another tenant must not act inside this context, however it
            // came by the cookie.
            if context.tenant != principal.tenant {
                return MapiResponse::new(
                    request_type.as_str(),
                    request_id,
                    ResponseCode::NoPrivilege,
                )
                .with_client_info(client_info)
                .into_response();
            }

            let request = match ExecuteRequest::parse(&body) {
                Ok(request) => request,
                Err(error) => {
                    tracing::debug!(%error, "mapi: malformed Execute body");
                    return MapiResponse::new(
                        request_type.as_str(),
                        request_id,
                        ResponseCode::InvalidRequestBody,
                    )
                    .with_client_info(client_info)
                    .into_response();
                }
            };

            // Two layers down to the operations: the extended-buffer chain,
            // then the ROP container inside it.
            let payload = match read_extended_buffer(&request.rop_buffer) {
                Ok(payload) => payload,
                Err(error) => {
                    tracing::debug!(%error, "mapi: unreadable ROP payload");
                    return MapiResponse::new(
                        request_type.as_str(),
                        request_id,
                        ResponseCode::InvalidRequestBody,
                    )
                    .with_client_info(client_info)
                    .into_response();
                }
            };
            let input = match RopBuffer::parse(&payload) {
                Ok(input) => input,
                Err(error) => {
                    tracing::debug!(%error, "mapi: unreadable ROP buffer");
                    return MapiResponse::new(
                        request_type.as_str(),
                        request_id,
                        ResponseCode::InvalidRequestBody,
                    )
                    .with_client_info(client_info)
                    .into_response();
                }
            };

            // **The folder tree is read before the dispatch, not during it.**
            // Dispatching happens under a lock and must not await, so the
            // store is consulted here and the result handed in as a snapshot.
            // One query per `Execute`, taken as the caller's own account — a
            // view built from somebody else's mailboxes is not something this
            // code can accidentally produce, because the account door is the
            // only way in.
            let account = state
                .store
                .for_account(context.tenant.clone(), context.user.clone());
            let folders = match account.mailboxes(alo_store::Page::first(MAX_FOLDERS)).await {
                Ok(mailboxes) => FolderView::build(&mailboxes),
                Err(error) => {
                    // No mailbox names and no addresses in the log line.
                    tracing::warn!(%error, "mapi: could not read the folder tree");
                    return MapiResponse::new(
                        request_type.as_str(),
                        request_id,
                        ResponseCode::UnknownFailure,
                    )
                    .with_client_info(client_info)
                    .into_response();
                }
            };

            // **Which folders' messages this buffer needs, worked out before
            // any of them is read.** Dispatch runs under a lock and awaits
            // nothing, so a folder's messages have to be in hand before it
            // starts — but the folder a client is about to read is named by a
            // handle that the same buffer often opens (Outlook sends
            // `RopOpenFolder`, `RopGetContentsTable`, `RopSetColumns` and
            // `RopQueryRows` together). So the buffer is rehearsed against a
            // copy of the object table, and only the folder list is kept.
            // **Rehearse, load, repeat.** One pass is not enough, because what
            // a buffer needs is discovered in layers: a rehearsal with nothing
            // loaded sees the folder a contents table will read, but it cannot
            // get past `RopOpenMessage` — with no messages loaded that
            // operation fails, so no message object exists, so the
            // `RopOpenAttachment` behind it is never reached and its file is
            // never fetched. Loading the message and rehearsing again gets one
            // layer further.
            //
            // The depth is fixed by the object graph — folder, then message,
            // then attachment — so this converges in three passes and the loop
            // is bounded at [`MAX_REHEARSALS`] rather than run to a fixed
            // point. A buffer that somehow wanted a fourth layer gets the three
            // it asked for and an honest `ecNotFound` for the rest, which is
            // the same answer it would get for anything else we could not
            // reach.
            let mut messages = MessageView::new();
            let mut seen: Vec<u64> = Vec::new();
            let mut loaded_messages: Vec<(u64, u64)> = Vec::new();
            let mut loaded_attachments: Vec<(u64, u64, u32)> = Vec::new();
            // The last rehearsal's findings, kept past the loop: the writes
            // below need the draft content it discovered, and only the final
            // pass has seen the whole buffer.
            let mut wanted = crate::dispatch::Wanted::default();
            // Drafts already written in this request, and the ids they got.
            let mut saved: Vec<(u32, u64, String)> = Vec::new();
            // Synchronisation streams built for this request (ADR 0051, stage
            // 8). Empty for now, and truthfully so: a folder is only offered
            // for synchronisation once a message carries a change number, and
            // a stream built without one would look like it worked and then
            // never show the client an update. So a configure opens a context
            // and the first GetBuffer answers Done — a complete conversation
            // about nothing, rather than a half-built one about something.
            let sync_streams = crate::dispatch_sync::SyncStreams::new();

            for _ in 0..MAX_REHEARSALS {
                wanted = {
                    let Ok(objects) = context.objects.lock() else {
                        tracing::error!("mapi: session object table is poisoned");
                        return MapiResponse::new(
                            request_type.as_str(),
                            request_id,
                            ResponseCode::UnknownFailure,
                        )
                        .with_client_info(client_info)
                        .into_response();
                    };
                    wanted_contents(
                        &context,
                        &state.dn_prefix,
                        &objects,
                        Sources {
                            folders: &folders,
                            messages: &messages,
                            written: &saved,
                            sync_streams: Some(&sync_streams),
                        },
                        &input,
                        logon_time(now),
                    )
                };

                // Nothing new to fetch: the rehearsal has stopped discovering,
                // and another pass would read the same things again.
                let fresh_folders: Vec<u64> = wanted
                    .folders
                    .iter()
                    .copied()
                    .chain(wanted.messages.iter().map(|(folder, _)| *folder))
                    .filter(|folder| !seen.contains(folder))
                    .collect();
                let fresh_messages: Vec<(u64, u64)> = wanted
                    .messages
                    .iter()
                    .copied()
                    .filter(|it| !loaded_messages.contains(it))
                    .collect();
                let fresh_attachments: Vec<(u64, u64, u32)> = wanted
                    .attachments
                    .iter()
                    .copied()
                    .filter(|it| !loaded_attachments.contains(it))
                    .collect();
                let fresh_saves: Vec<u32> = wanted
                    .saves
                    .iter()
                    .copied()
                    .filter(|handle| !saved.iter().any(|(done, _, _)| done == handle))
                    .collect();
                if fresh_folders.is_empty()
                    && fresh_messages.is_empty()
                    && fresh_attachments.is_empty()
                    && fresh_saves.is_empty()
                {
                    break;
                }

                // **The one write inside the loop.** A draft has to reach the
                // store before the pass that discovers its send can happen —
                // a message is sent from what was stored, so until it is
                // stored there is nothing to send. Guarded by `saved`, so a
                // later pass rehearsing the same buffer does not write twice.
                for handle in fresh_saves {
                    let Some(crate::dispatch::ServerObject::Draft {
                        folder_id,
                        properties,
                        recipients,
                        ..
                    }) = wanted.rehearsed.get(handle)
                    else {
                        continue;
                    };
                    let Some(mailbox) = MessageView::mailbox_of(&folders, *folder_id) else {
                        tracing::debug!("mapi: a draft named a folder with no mailbox behind it");
                        continue;
                    };
                    let Some(from) = account_address(&state, &principal).await else {
                        tracing::warn!("mapi: no address for the composing account");
                        return MapiResponse::new(
                            request_type.as_str(),
                            request_id,
                            ResponseCode::UnknownFailure,
                        )
                        .with_client_info(client_info)
                        .into_response();
                    };
                    let outgoing =
                        draft_to_outgoing(&from, properties, recipients, &state.dn_prefix);
                    let raw = alo_store::mime_write::build(&outgoing);
                    let Ok(id) = account.ingest(&mailbox, &raw).await else {
                        tracing::warn!("mapi: could not save a draft");
                        return MapiResponse::new(
                            request_type.as_str(),
                            request_id,
                            ResponseCode::UnknownFailure,
                        )
                        .with_client_info(client_info)
                        .into_response();
                    };
                    if let Err(error) = account.set_keyword(&id, "$draft", true).await {
                        tracing::warn!(%error, "mapi: could not mark a draft");
                    }
                    let mid = crate::folders::fid(crate::messages::message_counter(&id));
                    saved.push((handle, mid, id.as_str().to_owned()));
                }

                // One query per folder actually reached, as the caller's own
                // account — the same door the folder tree came through, so a view
                // of somebody else's mail is not something this code can produce.
                for folder_id in fresh_folders {
                    if seen.contains(&folder_id) {
                        continue;
                    }
                    seen.push(folder_id);
                    let Some(mailbox) = MessageView::mailbox_of(&folders, folder_id) else {
                        // A special folder with no alo mailbox behind it. It holds
                        // nothing, and that is a measurement: the tree was read and
                        // no mailbox stands there.
                        messages.insert(folder_id, &[]);
                        continue;
                    };
                    match account
                        .mapi_mailbox_rows(&mailbox, alo_store::Page::first(MAX_MESSAGES))
                        .await
                    {
                        Ok(rows) => messages.insert(folder_id, &rows),
                        Err(error) => {
                            // No subjects, addresses or mailbox names in the log.
                            tracing::warn!(%error, "mapi: could not read a folder's messages");
                            return MapiResponse::new(
                                request_type.as_str(),
                                request_id,
                                ResponseCode::UnknownFailure,
                            )
                            .with_client_info(client_info)
                            .into_response();
                        }
                    }
                }

                // The content of each message a buffer opens: one blob fetch and
                // one MIME parse each, and only for messages actually opened.
                for (folder_id, mid) in &fresh_messages {
                    loaded_messages.push((*folder_id, *mid));
                    let Some(entry) = messages.entry(*folder_id, *mid) else {
                        // The MID names no message of this account's. Nothing is
                        // loaded, and the dispatch answers `ecNotFound` — the same
                        // answer a message that never existed gets.
                        continue;
                    };
                    let id = entry.message.clone();
                    let (Ok(meta), Ok(raw)) =
                        (account.message(&id).await, account.message_bytes(&id).await)
                    else {
                        tracing::warn!("mapi: could not read a message this account owns");
                        return MapiResponse::new(
                            request_type.as_str(),
                            request_id,
                            ResponseCode::UnknownFailure,
                        )
                        .with_client_info(client_info)
                        .into_response();
                    };
                    let parsed = alo_store::mime_read::parse(&raw);
                    // Names and sizes only. Decoding a file is what costs, and it
                    // happens when somebody opens that file, not when they open
                    // the message it came with.
                    let attachments: Vec<AttachmentEntry> = parsed
                        .attachments
                        .iter()
                        .map(|part| AttachmentEntry {
                            number: u32::try_from(part.index).unwrap_or(u32::MAX),
                            filename: part.name.clone(),
                            mime_type: part.content_type.clone(),
                            size: u32::try_from(part.size).unwrap_or(u32::MAX),
                            data: None,
                        })
                        .collect();
                    messages.insert_body(
                        *mid,
                        MessageBody {
                            // A message with no plain-text alternative reads as
                            // an empty plain body. No longer a gap: the HTML
                            // alternative beside it is served too, and a client
                            // asks for whichever of the two it prefers.
                            text: parsed.text.unwrap_or_default(),
                            // Only a body the sender actually sent. The
                            // parser renders HTML for a plain-text message,
                            // and offering that to a client which prefers HTML
                            // would show generated markup in place of what was
                            // written.
                            html: parsed.html.filter(|_| parsed.html_is_original),
                            display_to: meta.to_addrs,
                            display_cc: meta.cc_addrs,
                            submit_time: meta.sent_at.map(|at| {
                                crate::rows::filetime_from_unix_secs(at.unix_timestamp())
                            }),
                            internet_message_id: meta.message_id_hdr,
                            attachments,
                            recipients: parsed
                                .recipients
                                .iter()
                                .map(|person| crate::openmessage::RecipientEntry {
                                    recipient_type: match person.kind {
                                        alo_store::mime_read::RecipientKind::To => {
                                            crate::openmessage::RECIPIENT_TYPE_TO
                                        }
                                        alo_store::mime_read::RecipientKind::Cc => {
                                            crate::openmessage::RECIPIENT_TYPE_CC
                                        }
                                        alo_store::mime_read::RecipientKind::Bcc => {
                                            crate::openmessage::RECIPIENT_TYPE_BCC
                                        }
                                    },
                                    display_name: person.display_name.clone(),
                                    email: person.email.clone(),
                                })
                                .collect(),
                        },
                    );
                }

                // The bytes of each attachment a buffer actually opened. Decoded
                // one at a time, and only these: a message with six files whose
                // reader opened one costs one decode.
                for (folder_id, mid, number) in &fresh_attachments {
                    loaded_attachments.push((*folder_id, *mid, *number));
                    let Some(entry) = messages.entry(*folder_id, *mid) else {
                        continue;
                    };
                    let id = entry.message.clone();
                    let Ok(raw) = account.message_bytes(&id).await else {
                        tracing::warn!("mapi: could not read a message this account owns");
                        return MapiResponse::new(
                            request_type.as_str(),
                            request_id,
                            ResponseCode::UnknownFailure,
                        )
                        .with_client_info(client_info)
                        .into_response();
                    };
                    let index = usize::try_from(*number).unwrap_or(usize::MAX);
                    if let Some((bytes, _, _)) = alo_store::mime_read::attachment_bytes(&raw, index)
                    {
                        messages.insert_attachment_data(*mid, *number, bytes);
                    }
                    // An attachment number naming nothing loads nothing, and the
                    // dispatch answers `ecNotFound` — the same answer a file that
                    // never existed gets.
                }
            }

            // **Sending.** The saves happened inside the rehearsal loop, because
            // a draft has to be stored before the pass that discovers its send
            // can run. Sending is different: nothing downstream depends on it,
            // so it happens once, here, after the buffer is fully understood.
            //
            // The send-as check, the `Bcc` strip and the filing into
            // Sent all live in `alo-submit`, which is the same code the JMAP
            // path uses — a second copy of the check binding a message's
            // visible `From:` to this account would be a second place for it to
            // be wrong.
            for (_, stored) in &wanted.submits {
                let id = alo_store::MessageId::new(stored.clone());
                let Ok(raw) = account.message_bytes(&id).await else {
                    tracing::warn!("mapi: could not read a draft being sent");
                    return MapiResponse::new(
                        request_type.as_str(),
                        request_id,
                        ResponseCode::UnknownFailure,
                    )
                    .with_client_info(client_info)
                    .into_response();
                };
                match send_draft(&state, &principal, &account, &id, &raw).await {
                    Ok(()) => {}
                    Err(reason) => {
                        // No addresses and no subject in the log line, and the
                        // client is told the `Execute` failed rather than why:
                        // "you may not send as that person" is information a
                        // caller probing identities would use.
                        tracing::warn!(reason, "mapi: submission refused");
                        return MapiResponse::new(
                            request_type.as_str(),
                            request_id,
                            ResponseCode::UnknownFailure,
                        )
                        .with_client_info(client_info)
                        .into_response();
                    }
                }
            }

            let dispatched = {
                // The lock is held only across the dispatch, which awaits
                // nothing — a poisoned lock means another request panicked
                // mid-dispatch, and continuing on a half-updated table would be
                // worse than refusing this one.
                let Ok(mut objects) = context.objects.lock() else {
                    tracing::error!("mapi: session object table is poisoned");
                    return MapiResponse::new(
                        request_type.as_str(),
                        request_id,
                        ResponseCode::UnknownFailure,
                    )
                    .with_client_info(client_info)
                    .into_response();
                };
                dispatch(
                    &context,
                    &state.dn_prefix,
                    &mut objects,
                    Sources {
                        folders: &folders,
                        messages: &messages,
                        written: &saved,
                        sync_streams: Some(&sync_streams),
                    },
                    &input,
                    logon_time(now),
                )
            };

            let output = RopBuffer {
                rops: dispatched.responses,
                handles: dispatched.handles,
            };
            let Ok(framed) = output.to_bytes() else {
                tracing::error!("mapi: ROP responses do not fit a ROP buffer");
                return MapiResponse::new(
                    request_type.as_str(),
                    request_id,
                    ResponseCode::UnknownFailure,
                )
                .with_client_info(client_info)
                .into_response();
            };
            let Ok(wrapped) = write_extended_buffer(&framed) else {
                tracing::error!("mapi: ROP buffer does not fit an extended buffer");
                return MapiResponse::new(
                    request_type.as_str(),
                    request_id,
                    ResponseCode::UnknownFailure,
                )
                .with_client_info(client_info)
                .into_response();
            };

            // `MaxRopOut` is the client's ceiling and it binds us: the client
            // sized its receive buffer from that number, so overrunning it is
            // not a large answer but a broken one. The protocol's proper reply
            // is `RopBackoff` (a later stage); until then this refuses rather
            // than overruns, and says so in the log.
            if wrapped.len() as u64 > u64::from(request.max_rop_out) {
                tracing::warn!(
                    produced = wrapped.len(),
                    max_rop_out = request.max_rop_out,
                    "mapi: response exceeds the client's ceiling"
                );
                return MapiResponse::new(
                    request_type.as_str(),
                    request_id,
                    ResponseCode::TooLarge,
                )
                .with_client_info(client_info)
                .into_response();
            }

            MapiResponse::new(request_type.as_str(), request_id, ResponseCode::Success)
                .with_client_info(client_info)
                .with_body(execute_success_body(0, &wrapped))
                .into_response()
        }

        // Recognised, authenticated, and honestly refused. Answering it with an
        // empty success would have Outlook wait on notifications that never
        // come — a worse failure than a refusal it can report.
        //
        // The address book's request types belong to the *other* endpoint. A
        // client that sent one here has the wrong URL, and refusing beats
        // answering: the two endpoints hold different session state, and a
        // `Bind` answered from the mailbox endpoint would leave the client
        // holding an address book session this endpoint knows nothing about.
        RequestType::NotificationWait
        | RequestType::Bind
        | RequestType::Unbind
        | RequestType::ResolveNames => MapiResponse::new(
            request_type.as_str(),
            request_id,
            ResponseCode::EndpointDisabled,
        )
        .with_client_info(client_info)
        .into_response(),
    }
}

/// The wall-clock components a logon reports, from an instant.
///
/// `time`'s weekday numbers Monday as one; [MS-OXCROPS] §2.2.3.1.2.1 numbers
/// Sunday as zero, so the conversion is explicit rather than a cast that would
/// be wrong by one for six days of the week.
fn logon_time(now: OffsetDateTime) -> LogonTime {
    LogonTime {
        seconds: now.second(),
        minutes: now.minute(),
        hour: now.hour(),
        day_of_week: now.weekday().number_days_from_sunday(),
        day: now.day(),
        month: u8::from(now.month()),
        year: u16::try_from(now.year()).unwrap_or(0),
    }
}

/// The `Disconnect` success response body ([MS-OXCMAPIHTTP] §2.2.4.3.2):
/// `StatusCode`, `ErrorCode`, and an empty auxiliary buffer.
fn disconnect_body() -> Vec<u8> {
    let mut out = Vec::with_capacity(12);
    out.extend_from_slice(&0u32.to_le_bytes()); // StatusCode — MUST be 0.
    out.extend_from_slice(&0u32.to_le_bytes()); // ErrorCode.
    out.extend_from_slice(&0u32.to_le_bytes()); // AuxiliaryBufferSize.
    out
}

/// The canonical address of the account composing a message.
async fn account_address(state: &MapiState, principal: &alo_identity::Principal) -> Option<String> {
    state
        .store
        .for_tenant(principal.tenant.clone())
        .email_of(&principal.user)
        .await
        .ok()
        .flatten()
}

/// Turns a draft's accumulated properties and recipients into a message.
///
/// The `From` is the account's own canonical address and is **not** taken from
/// anything the client set. A client may put whatever it likes in
/// `PidTagSenderEmailAddress`; the address a recipient reads is decided here,
/// and checked again by `alo-submit` before the message leaves. Two checks
/// rather than one because this one is about what gets written and that one is
/// about what gets sent, and they are reached by different paths.
fn draft_to_outgoing(
    from: &str,
    properties: &[(u16, crate::rows::Value)],
    recipients: &[crate::openmessage::RecipientEntry],
    domain_hint: &str,
) -> alo_store::mime_write::Outgoing {
    let text_of = |id: u16| -> Option<String> {
        properties.iter().find_map(|(pid, value)| {
            if *pid != id {
                return None;
            }
            match value {
                crate::rows::Value::String(text) => Some(text.clone()),
                crate::rows::Value::Binary(bytes) => {
                    Some(String::from_utf8_lossy(bytes).into_owned())
                }
                _ => None,
            }
        })
    };
    let pick = |kind: u8| -> Vec<alo_store::mime_write::Addr> {
        recipients
            .iter()
            .filter(|person| person.recipient_type == kind)
            .map(|person| alo_store::mime_write::Addr {
                name: Some(person.display_name.clone()),
                email: person.email.clone(),
            })
            .collect()
    };

    alo_store::mime_write::Outgoing {
        from: alo_store::mime_write::Addr {
            name: None,
            email: from.to_owned(),
        },
        to: pick(crate::openmessage::RECIPIENT_TYPE_TO),
        cc: pick(crate::openmessage::RECIPIENT_TYPE_CC),
        bcc: pick(crate::openmessage::RECIPIENT_TYPE_BCC),
        subject: text_of(crate::rows::pid::SUBJECT).unwrap_or_default(),
        in_reply_to: Vec::new(),
        references: Vec::new(),
        body_text: text_of(crate::rows::pid::BODY).unwrap_or_default(),
        body_html: text_of(crate::rows::pid::HTML),
        attachments: Vec::new(),
        message_id_domain: from.rsplit('@').next().unwrap_or(domain_hint).to_owned(),
        message_id_token: alo_store::mime_write::new_message_id_token(),
    }
}

/// Sends a stored draft, through the same door JMAP sends through.
///
/// # Errors
/// A short reason for the server's own log. Never returned to the client:
/// distinguishing "you may not send as that person" from "the relay refused"
/// tells a caller probing identities which addresses exist.
async fn send_draft(
    state: &MapiState,
    principal: &alo_identity::Principal,
    account: &alo_store::AccountStore,
    id: &alo_store::MessageId,
    raw: &[u8],
) -> Result<(), &'static str> {
    let Some(listener) = state.submission_addr.as_deref() else {
        return Err("no submission listener is configured");
    };

    // The addresses this account may send as: its own, plus its aliases.
    let ts = state.store.for_tenant(principal.tenant.clone());
    let canonical = ts
        .email_of(&principal.user)
        .await
        .map_err(|_| "sender lookup failed")?
        .ok_or("no address for this account")?;
    let mut permitted = vec![canonical.to_lowercase()];
    if let Ok(aliases) = ts.aliases_of(&principal.user).await {
        permitted.extend(aliases.into_iter().map(|alias| alias.to_lowercase()));
    }

    // **The anti-spoof check, on the header a recipient reads.** The envelope
    // is not what anybody looks at, so binding only that would leave the
    // visible author free.
    let from = alo_submit::extract_from_addr(raw).ok_or("the draft has no From address")?;
    if !permitted.contains(&from) {
        return Err("the From address is not one this account owns");
    }

    let rcpts: Vec<String> = recipients_of(raw)
        .into_iter()
        .filter(|address| alo_submit::valid_addr(address))
        .collect();
    if rcpts.is_empty() {
        return Err("the draft has no deliverable recipient");
    }

    // `Bcc:` is stripped from the bytes that travel; blind recipients are
    // reached through the envelope above, and the stored copy keeps the header.
    let wire = alo_submit::strip_bcc_header(raw);
    alo_submit::submit(listener, "alo-mapi", &from, &rcpts, &wire)
        .await
        .map_err(|_| "the relay refused the message")?;
    alo_submit::post_send(account, id).await;
    Ok(())
}

/// Every address a stored draft is addressed to, from its own headers.
///
/// Read back out of the message rather than carried from the session, so the
/// envelope and the message that was actually stored cannot disagree about who
/// it is for.
fn recipients_of(raw: &[u8]) -> Vec<String> {
    alo_store::mime_read::parse(raw)
        .recipients
        .into_iter()
        .map(|person| person.email)
        .collect()
}

/// The address book endpoint (`/mapi/nspi`).
///
/// Serves `Bind`, `Unbind` and `ResolveNames`: enough for somebody to type a
/// colleague's name into the To line and have it become an address. Browsing
/// the directory is a later stage and is refused rather than half-answered,
/// because a client shown a truncated directory cannot tell it is truncated.
///
/// **Authenticated exactly as the mailbox endpoint is**, and for the same
/// reason: this reads the tenant's own people and the caller's own contacts, so
/// it is somebody's data and not a public list.
async fn nspi(State(state): State<MapiState>, headers: HeaderMap, body: Bytes) -> Response {
    let request_id = header_str(&headers, "X-RequestId").unwrap_or_default();
    let client_info = header_str(&headers, "X-ClientInfo");

    let Some(raw_type) = header_str(&headers, "X-RequestType") else {
        return MapiResponse::new("", request_id, ResponseCode::MissingHeader)
            .with_client_info(client_info)
            .into_response();
    };
    let Some(request_type) = RequestType::parse(raw_type) else {
        return MapiResponse::new("", request_id, ResponseCode::InvalidRequestType)
            .with_client_info(client_info)
            .into_response();
    };

    // Credentials before the body is read, exactly as the mailbox endpoint
    // does: an unauthenticated caller learns nothing about what we would have
    // done with it, and above all learns nobody's name.
    let Some((username, password)) = basic_credentials(&headers) else {
        return unauthorized();
    };
    let principal = match state
        .identity
        .authenticate_legacy(&username, &password)
        .await
    {
        Ok(Some(principal)) => principal,
        Ok(None) => return unauthorized(),
        Err(error) => {
            tracing::warn!(%error, "mapi: credential lookup failed");
            return MapiResponse::new(
                request_type.as_str(),
                request_id,
                ResponseCode::UnknownFailure,
            )
            .with_client_info(client_info)
            .into_response();
        }
    };

    if body.len() > MAX_ADDRESS_BOOK_BODY {
        return MapiResponse::new(request_type.as_str(), request_id, ResponseCode::TooLarge)
            .with_client_info(client_info)
            .into_response();
    }

    match request_type {
        // A bind carries no state worth keeping: this endpoint holds no
        // per-session table position, because it serves no table to page
        // through. The GUID is the deployment's, and it is what makes the
        // Minimal Entry IDs this server issues recognisably its own.
        RequestType::Bind => {
            if nspi::BindRequest::parse(&body).is_err() {
                return MapiResponse::new(
                    request_type.as_str(),
                    request_id,
                    ResponseCode::InvalidRequestBody,
                )
                .with_client_info(client_info)
                .into_response();
            }
            MapiResponse::new(request_type.as_str(), request_id, ResponseCode::Success)
                .with_client_info(client_info)
                .with_body(nspi::bind_success_body(ADDRESS_BOOK_GUID))
                .into_response()
        }

        RequestType::Unbind => {
            if nspi::UnbindRequest::parse(&body).is_err() {
                return MapiResponse::new(
                    request_type.as_str(),
                    request_id,
                    ResponseCode::InvalidRequestBody,
                )
                .with_client_info(client_info)
                .into_response();
            }
            MapiResponse::new(request_type.as_str(), request_id, ResponseCode::Success)
                .with_client_info(client_info)
                .with_body(nspi::unbind_success_body())
                .into_response()
        }

        RequestType::ResolveNames => {
            let Ok(request) = nspi::ResolveNamesRequest::parse(&body) else {
                return MapiResponse::new(
                    request_type.as_str(),
                    request_id,
                    ResponseCode::InvalidRequestBody,
                )
                .with_client_info(client_info)
                .into_response();
            };

            // Every lookup goes through the caller's own account door, so a
            // name can only resolve to somebody in their tenant or in their own
            // contacts. There is no unscoped directory to reach.
            let account = state
                .store
                .for_account(principal.tenant.clone(), principal.user.clone());

            let mut outcomes = Vec::with_capacity(request.names.len());
            for name in &request.names {
                let matches = match account.mapi_resolve(name, MAX_RESOLVE_MATCHES).await {
                    Ok(matches) => matches,
                    Err(error) => {
                        // No names and no addresses in the log line.
                        tracing::warn!(%error, "mapi: address book lookup failed");
                        return MapiResponse::new(
                            request_type.as_str(),
                            request_id,
                            ResponseCode::UnknownFailure,
                        )
                        .with_client_info(client_info)
                        .into_response();
                    }
                };
                outcomes.push(match matches.len() {
                    0 => nspi::Resolution::Unresolved,
                    1 => {
                        let found = &matches[0];
                        nspi::Resolution::Resolved(Box::new(nspi::Entry {
                            display_name: found.display_name.clone(),
                            email: found.email.clone(),
                        }))
                    }
                    // More than one, and none is chosen. Picking would put a
                    // colleague's address on a message somebody believed was
                    // going elsewhere.
                    _ => nspi::Resolution::Ambiguous,
                });
            }

            let body = nspi::resolve_names_success_body(
                &request.property_tags,
                &outcomes,
                &address_book_value,
            );
            MapiResponse::new(request_type.as_str(), request_id, ResponseCode::Success)
                .with_client_info(client_info)
                .with_body(body)
                .into_response()
        }

        // Everything else this endpoint might be asked — browsing the
        // directory, fetching a template, comparing positions — is a later
        // stage, and the mailbox endpoint's own request types do not belong
        // here at all.
        _ => MapiResponse::new(
            request_type.as_str(),
            request_id,
            ResponseCode::EndpointDisabled,
        )
        .with_client_info(client_info)
        .into_response(),
    }
}

/// One property of one directory entry.
///
/// Deliberately few: a display name, the address in the two spellings a client
/// asks for it, and the address type. Everything else Outlook might ask of a
/// directory entry — a phone number, an office, a manager, a display type —
/// alo does not know about a colleague, and a blank string in a field somebody
/// reads as fact is worse than the field being absent. The flagged row carries
/// the absence honestly.
fn address_book_value(entry: &nspi::Entry, tag: crate::columns::PropertyTag) -> Option<Value> {
    match tag.property_id {
        crate::rows::pid::DISPLAY_NAME => Some(Value::String(entry.display_name.clone())),
        crate::rows::pid::EMAIL_ADDRESS | crate::rows::pid::SMTP_ADDRESS => {
            Some(Value::String(entry.email.clone()))
        }
        crate::rows::pid::ADDRESS_TYPE => {
            Some(Value::String(crate::rows::ADDRESS_TYPE_SMTP.to_owned()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn basic_credentials_are_read_from_a_well_formed_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!("Basic {}", BASE64.encode("disan@alo.test:s3cret"))
                .parse()
                .unwrap(),
        );
        assert_eq!(
            basic_credentials(&headers),
            Some(("disan@alo.test".to_owned(), "s3cret".to_owned()))
        );
    }

    /// A password may contain colons and a username may not ([RFC 7617] §2), so
    /// the split is on the first colon. Splitting on the last would silently
    /// mangle any password containing one.
    #[test]
    fn a_password_may_contain_colons() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!("Basic {}", BASE64.encode("user:a:b:c"))
                .parse()
                .unwrap(),
        );
        assert_eq!(
            basic_credentials(&headers),
            Some(("user".to_owned(), "a:b:c".to_owned()))
        );
    }

    /// Every malformed shape is `None`, so the endpoint answers all of them
    /// with the same challenge and tells a prober nothing.
    #[test]
    fn anything_that_is_not_basic_is_no_credential_at_all() {
        for raw in [
            "",
            "Bearer abc",
            "Basic",
            "Basic !!!not-base64!!!",
            // Valid base64, but no colon: not a credential.
            "Basic dXNlcm5hbWU=",
        ] {
            let mut headers = HeaderMap::new();
            if !raw.is_empty() {
                headers.insert(header::AUTHORIZATION, raw.parse().unwrap());
            }
            assert_eq!(basic_credentials(&headers), None, "accepted {raw:?}");
        }
    }

    #[test]
    fn the_challenge_is_a_401_with_a_fixed_realm() {
        let response = unauthorized();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers().get(header::WWW_AUTHENTICATE).unwrap(),
            r#"Basic realm="alo""#
        );
    }

    #[test]
    fn the_disconnect_body_is_three_little_endian_zeroes() {
        assert_eq!(disconnect_body(), vec![0u8; 12]);
    }
}
