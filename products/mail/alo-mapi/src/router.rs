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
use crate::dispatch::{dispatch, wanted_contents};
use crate::execute::{ExecuteRequest, success_body as execute_success_body};
use crate::folders::FolderView;
use crate::logon_response::LogonTime;
use crate::messages::{MessageBody, MessageView};
use crate::response::{MapiResponse, ResponseCode};
use crate::rop::RopBuffer;
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
            let wanted = {
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
                    &folders,
                    &input,
                    logon_time(now),
                )
            };

            // One query per folder actually reached, as the caller's own
            // account — the same door the folder tree came through, so a view
            // of somebody else's mail is not something this code can produce.
            let mut messages = MessageView::new();
            // A message's own folder must be loaded before its MID can be
            // resolved, so the two lists are read as one.
            let folder_ids: Vec<u64> = wanted
                .folders
                .iter()
                .copied()
                .chain(wanted.messages.iter().map(|(folder, _)| *folder))
                .collect();
            let mut seen: Vec<u64> = Vec::new();
            for folder_id in folder_ids {
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
            for (folder_id, mid) in &wanted.messages {
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
                messages.insert_body(
                    *mid,
                    MessageBody {
                        // A message with no plain-text alternative reads as
                        // an empty body rather than a refusal: the message
                        // exists and opens, and what it has to show over this
                        // protocol is nothing until HTML bodies are served.
                        text: parsed.text.unwrap_or_default(),
                        display_to: meta.to_addrs,
                        display_cc: meta.cc_addrs,
                        submit_time: meta
                            .sent_at
                            .map(|at| crate::rows::filetime_from_unix_secs(at.unix_timestamp())),
                        internet_message_id: meta.message_id_hdr,
                    },
                );
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
                    &folders,
                    &messages,
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
        RequestType::NotificationWait => MapiResponse::new(
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

/// The address book endpoint (`/mapi/nspi`) — a later stage.
///
/// It answers rather than 404s because Autodiscover names it: a client that
/// finds nothing here would retry until it timed out, where a stated refusal is
/// something it can report at once.
async fn nspi(headers: HeaderMap) -> Response {
    let request_id = header_str(&headers, "X-RequestId").unwrap_or_default();
    let request_type = header_str(&headers, "X-RequestType").unwrap_or_default();
    MapiResponse::new(request_type, request_id, ResponseCode::EndpointDisabled)
        .with_client_info(header_str(&headers, "X-ClientInfo"))
        .into_response()
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
