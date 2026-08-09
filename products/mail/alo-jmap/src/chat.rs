//! alo Chat surface (ADR 0038): the authenticated `/chat/*` routes — rooms,
//! membership, messages and read state.
//!
//! Every handler resolves the caller with [`authenticate`] and reaches data
//! only through the account door, so a room or message from another tenant
//! simply does not resolve. The store's own language is preserved on the wire
//! (`docs/design/chat.md`): a room the caller may not see is **404, never
//! 403** — its existence is not disclosed — while a room they *can* see but
//! lack the role for (renaming, archiving, removing someone, editing another
//! person's words) is a plain 403, because there the secret is the permission,
//! not the room. Rule violations — empty or over-long text, an archived room,
//! a reply to something that is not a top-level message here, a DM with
//! oneself — are 422 carrying the store's own message, which the UI shows
//! verbatim (UX law 8).
//!
//! Members and message authors carry their opaque user id **and** the email
//! address it belongs to, the way tasks carries `assigneeId` beside
//! `assignee`: the id is what a client sends back, the address is what a
//! person recognises. The address is `null` when the id no longer resolves —
//! someone left the tenant — because a feed must still render when an author
//! is gone. There is no display-name column in this schema yet; when there is,
//! it is added beside the address, not in place of it.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use alo_store::{
    ChannelVisibility, ChatChannel, ChatChannelId, ChatChannelSummary, ChatFeedMessage, ChatMember,
    ChatMessage, ChatMessageId, MESSAGE_PAGE_DEFAULT, StoreError, UserId,
};

use crate::error::Problem;
use crate::push;
use crate::state::{Account, AppState, authenticate};

/// The store's vocabulary on the wire: not-found stays not-found (a room the
/// caller may not see), forbidden stays forbidden (a room they see without the
/// role), and every rule violation is a 422 whose detail is the store's own
/// sentence.
fn map_store_err(e: StoreError) -> Problem {
    match e {
        StoreError::NotFound => Problem::with(StatusCode::NOT_FOUND, "not found"),
        StoreError::Forbidden => Problem::with(StatusCode::FORBIDDEN, "forbidden"),
        StoreError::Conflict(msg) | StoreError::Validation(msg) => {
            Problem::with(StatusCode::UNPROCESSABLE_ENTITY, msg)
        }
        _ => Problem::server_error(),
    }
}

/// Tell everyone in a room that chat changed, plus anyone named in `also` (the
/// person just removed, who must see the room leave their sidebar).
///
/// Best-effort throughout: a write that succeeded is never reported as failed
/// because a live notification could not be sent.
async fn notify_room(
    state: &AppState,
    account: &Account,
    channel: &ChatChannelId,
    also: &[UserId],
) {
    let mut users: Vec<UserId> = account
        .acc
        .channel_members(channel)
        .await
        .map(|members| members.into_iter().map(|m| m.user).collect())
        .unwrap_or_default();
    users.extend(also.iter().cloned());
    push::notify_chat(state, &account.tenant, &users).await;
}

fn iso(at: OffsetDateTime) -> String {
    at.format(&Rfc3339).unwrap_or_default()
}

fn channel_json(c: &ChatChannel) -> Value {
    json!({
        "id": c.id.as_str(),
        "kind": c.kind.as_str(),
        "name": c.name,
        "topic": c.topic,
        "visibility": c.visibility.as_str(),
        "createdBy": c.created_by.as_str(),
        "createdAt": iso(c.created_at),
        "archivedAt": c.archived_at.map(iso),
    })
}

fn summary_json(s: &ChatChannelSummary) -> Value {
    let mut value = channel_json(&s.channel);
    if let Some(object) = value.as_object_mut() {
        object.insert("unread".to_owned(), json!(s.unread));
        object.insert("lastReadSeq".to_owned(), json!(s.last_read_seq));
        object.insert("lastSeq".to_owned(), json!(s.last_seq));
        object.insert("lastAt".to_owned(), json!(s.last_at.map(iso)));
    }
    value
}

/// Email addresses for the people a payload names, keyed by user id.
///
/// One query for the whole page (`emails_of`), not one per line: fifty
/// messages from five people cost five names, and a loop over `email_of`
/// would cost fifty round trips to say the same thing.
///
/// Best-effort by design — if the lookup fails, the payload still goes out
/// with ids and no addresses. A feed that renders unlabelled is a poor screen;
/// a feed that 500s because the directory hiccuped is a broken one.
async fn resolve_emails(
    state: &AppState,
    account: &Account,
    users: &[UserId],
) -> HashMap<String, String> {
    if users.is_empty() {
        return HashMap::new();
    }
    let ts = state.store.for_tenant(account.tenant.clone());
    ts.emails_of(users).await.unwrap_or_default()
}

fn member_json(m: &ChatMember, emails: &HashMap<String, String>) -> Value {
    json!({
        "user": m.user.as_str(),
        "email": emails.get(m.user.as_str()),
        "role": m.role.as_str(),
        "joinedAt": iso(m.joined_at),
        "lastReadSeq": m.last_read_seq,
        "muted": m.muted,
    })
}

/// A feed line: the message, plus the thread hanging under it. `replyCount`
/// is what lets a client draw "3 replies" without fetching the thread, and
/// `lastReplyAt` is when that thread last moved.
fn feed_message_json(f: &ChatFeedMessage, emails: &HashMap<String, String>) -> Value {
    let mut value = message_json(&f.message, emails);
    if let Some(object) = value.as_object_mut() {
        object.insert("replyCount".to_owned(), json!(f.reply_count));
        object.insert("lastReplyAt".to_owned(), json!(f.last_reply_at.map(iso)));
    }
    value
}

fn message_json(m: &ChatMessage, emails: &HashMap<String, String>) -> Value {
    json!({
        "id": m.id.as_str(),
        "channel": m.channel.as_str(),
        "seq": m.seq,
        "author": m.author.as_str(),
        "authorEmail": emails.get(m.author.as_str()),
        "body": m.body,
        "kind": m.kind.as_str(),
        "threadRootSeq": m.thread_root_seq,
        "createdAt": iso(m.created_at),
        "editedAt": m.edited_at.map(iso),
        "deletedAt": m.deleted_at.map(iso),
    })
}

/// `GET /chat/channels` → the caller's rooms with unread counts and last
/// activity, liveliest first — everything a sidebar draws.
///
/// # Errors
/// 401 unauthenticated.
pub async fn list_channels(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let summaries = account
        .acc
        .channel_summaries()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "channels": summaries.iter().map(summary_json).collect::<Vec<_>>()
    })))
}

/// `GET /chat/channels/joinable` → the live public channels of the tenant the
/// caller has not joined (the "browse channels" list).
///
/// # Errors
/// 401 unauthenticated.
pub async fn list_joinable(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let channels = account
        .acc
        .joinable_channels()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "channels": channels.iter().map(channel_json).collect::<Vec<_>>()
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewChannelBody {
    /// `"channel"` (default) or `"dm"`.
    kind: Option<String>,
    name: Option<String>,
    topic: Option<String>,
    /// `"public"` (default) or `"private"`; ignored for a DM, which is always
    /// private.
    visibility: Option<String>,
    /// The other person, when opening a DM.
    with: Option<String>,
}

/// `POST /chat/channels` → create a named room `{name, topic?, visibility?}`,
/// or open a DM `{kind:"dm", with}` (idempotent: the same pair always returns
/// the same room).
///
/// # Errors
/// 422 for a missing/invalid name, an unknown visibility, a DM without a
/// partner or with oneself; 404 when the DM partner is not of this tenant.
pub async fn create_channel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<NewChannelBody>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = if body.kind.as_deref() == Some("dm") {
        let with = body.with.as_deref().unwrap_or("").trim();
        if with.is_empty() {
            return Err(Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "a direct message needs someone to talk to",
            ));
        }
        account
            .acc
            .open_dm(&UserId::new(with.to_owned()))
            .await
            .map_err(map_store_err)?
    } else {
        let name = body.name.as_deref().unwrap_or("");
        let visibility = match body.visibility.as_deref() {
            None | Some("public") => ChannelVisibility::Public,
            Some("private") => ChannelVisibility::Private,
            Some(other) => {
                return Err(Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    format!("unknown visibility {other}"),
                ));
            }
        };
        account
            .acc
            .create_channel(name, body.topic.as_deref(), visibility)
            .await
            .map_err(map_store_err)?
    };
    let channel = account.acc.channel(&id).await.map_err(map_store_err)?;
    notify_room(&state, &account, &id, &[]).await;
    Ok(Json(channel_json(&channel)))
}

/// `GET /chat/channels/{id}` → one room with its members and the caller's own
/// role in it (`null` when they are only a reader of a public room).
///
/// # Errors
/// 404 when the room is not the caller's to see.
pub async fn get_channel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = ChatChannelId::new(id);
    let channel = account.acc.channel(&id).await.map_err(map_store_err)?;
    let members = account
        .acc
        .channel_members(&id)
        .await
        .map_err(map_store_err)?;
    let role = account.acc.channel_role(&id).await.map_err(map_store_err)?;
    let who: Vec<UserId> = members.iter().map(|m| m.user.clone()).collect();
    let emails = resolve_emails(&state, &account, &who).await;
    let mut value = channel_json(&channel);
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "members".to_owned(),
            json!(
                members
                    .iter()
                    .map(|m| member_json(m, &emails))
                    .collect::<Vec<_>>()
            ),
        );
        object.insert("myRole".to_owned(), json!(role.map(|r| r.as_str())));
    }
    Ok(Json(value))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPatchBody {
    name: Option<String>,
    topic: Option<String>,
}

/// `PATCH /chat/channels/{id}` `{name?, topic?}` → rename and/or retopic a
/// named room. Owners only; a `null` field is left alone.
///
/// # Errors
/// 404 not visible or not a member, 403 not an owner, 422 for a bad name or a
/// DM (which has no name to change).
pub async fn patch_channel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<ChannelPatchBody>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = ChatChannelId::new(id);
    account
        .acc
        .rename_channel(&id, body.name.as_deref(), body.topic.as_deref())
        .await
        .map_err(map_store_err)?;
    notify_room(&state, &account, &id, &[]).await;
    let channel = account.acc.channel(&id).await.map_err(map_store_err)?;
    Ok(Json(channel_json(&channel)))
}

/// `POST /chat/channels/{id}/archive` → take a room out of the lists and free
/// its name; its history stays readable to members. Owners only.
///
/// # Errors
/// 404 not visible or not a member, 403 not an owner, 422 for a DM.
pub async fn archive_channel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = ChatChannelId::new(id);
    account
        .acc
        .archive_channel(&id)
        .await
        .map_err(map_store_err)?;
    notify_room(&state, &account, &id, &[]).await;
    let channel = account.acc.channel(&id).await.map_err(map_store_err)?;
    Ok(Json(channel_json(&channel)))
}

/// `POST /chat/channels/{id}/join` → join a live public channel. Joining twice
/// is not an error.
///
/// # Errors
/// 404 when there is no such joinable channel (a private one included — its
/// existence is not disclosed).
pub async fn join_channel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = ChatChannelId::new(id);
    account.acc.join_channel(&id).await.map_err(map_store_err)?;
    notify_room(&state, &account, &id, &[]).await;
    let channel = account.acc.channel(&id).await.map_err(map_store_err)?;
    Ok(Json(channel_json(&channel)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberBody {
    user: String,
}

/// `POST /chat/channels/{id}/members` `{user}` → add someone to a room the
/// caller belongs to.
///
/// # Errors
/// 404 when the room is not the caller's, or the person is not of this
/// tenant; 422 for a DM, whose two people are fixed when it is opened.
pub async fn add_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<MemberBody>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = ChatChannelId::new(id);
    account
        .acc
        .add_member(&id, &UserId::new(body.user))
        .await
        .map_err(map_store_err)?;
    notify_room(&state, &account, &id, &[]).await;
    let members = account
        .acc
        .channel_members(&id)
        .await
        .map_err(map_store_err)?;
    let who: Vec<UserId> = members.iter().map(|m| m.user.clone()).collect();
    let emails = resolve_emails(&state, &account, &who).await;
    Ok(Json(json!({
        "members": members
            .iter()
            .map(|m| member_json(m, &emails))
            .collect::<Vec<_>>()
    })))
}

/// `DELETE /chat/channels/{id}/members/{user}` → leave a room, or (as its
/// owner) remove someone else.
///
/// # Errors
/// 404 not a member, 403 removing someone else without being an owner, 422
/// for a DM.
pub async fn remove_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, user)): Path<(String, String)>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = ChatChannelId::new(id);
    // Read the room's people first: if the caller is the one leaving, the room
    // stops being theirs to look at the moment they are out of it.
    let before: Vec<UserId> = account
        .acc
        .channel_members(&id)
        .await
        .map(|members| members.into_iter().map(|m| m.user).collect())
        .unwrap_or_default();
    account
        .acc
        .remove_member(&id, &UserId::new(user))
        .await
        .map_err(map_store_err)?;
    push::notify_chat(&state, &account.tenant, &before).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryQuery {
    /// Walk backwards from this sequence (exclusive).
    before: Option<i64>,
    limit: Option<i64>,
}

/// `GET /chat/channels/{id}/messages?before=&limit=` → a page of history,
/// newest first. Pass the oldest `seq` you hold as `before` to page back.
///
/// # Errors
/// 404 when the room is not the caller's to see.
pub async fn list_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let messages = account
        .acc
        .messages(
            &ChatChannelId::new(id),
            query.before,
            query.limit.unwrap_or(MESSAGE_PAGE_DEFAULT),
        )
        .await
        .map_err(map_store_err)?;
    let who: Vec<UserId> = messages.iter().map(|m| m.message.author.clone()).collect();
    let emails = resolve_emails(&state, &account, &who).await;
    Ok(Json(json!({
        "messages": messages
            .iter()
            .map(|m| feed_message_json(m, &emails))
            .collect::<Vec<_>>()
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMessageBody {
    body: String,
    /// The `seq` of the message being replied to, for a threaded reply.
    thread_root_seq: Option<i64>,
}

/// `POST /chat/channels/{id}/messages` `{body, threadRootSeq?}` → say
/// something. Membership is required: reading a public room does not make the
/// caller a participant in it.
///
/// # Errors
/// 404 not visible or not a member, 422 for empty/over-long text, an archived
/// room, or a reply to something that is not a top-level message here.
pub async fn post_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<NewMessageBody>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let message = account
        .acc
        .post_message(&ChatChannelId::new(id), &body.body, body.thread_root_seq)
        .await
        .map_err(map_store_err)?;
    notify_room(&state, &account, &message.channel, &[]).await;
    let emails = resolve_emails(&state, &account, std::slice::from_ref(&message.author)).await;
    Ok(Json(message_json(&message, &emails)))
}

/// `GET /chat/channels/{id}/threads/{seq}` → the replies gathered under one
/// top-level message, oldest first.
///
/// # Errors
/// 404 when the room is not the caller's to see.
pub async fn list_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, seq)): Path<(String, i64)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let messages = account
        .acc
        .thread_replies(&ChatChannelId::new(id), seq)
        .await
        .map_err(map_store_err)?;
    let who: Vec<UserId> = messages.iter().map(|m| m.author.clone()).collect();
    let emails = resolve_emails(&state, &account, &who).await;
    Ok(Json(json!({
        "messages": messages
            .iter()
            .map(|m| message_json(m, &emails))
            .collect::<Vec<_>>()
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditMessageBody {
    body: String,
}

/// `PATCH /chat/messages/{id}` `{body}` → rewrite one's own message. The
/// sequence, and so everyone's read state, is untouched.
///
/// # Errors
/// 404 not visible, 403 someone else's, 422 for bad text or a withdrawn
/// message.
pub async fn edit_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<EditMessageBody>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let message = account
        .acc
        .edit_message(&ChatMessageId::new(id), &body.body)
        .await
        .map_err(map_store_err)?;
    notify_room(&state, &account, &message.channel, &[]).await;
    let emails = resolve_emails(&state, &account, std::slice::from_ref(&message.author)).await;
    Ok(Json(message_json(&message, &emails)))
}

/// `DELETE /chat/messages/{id}` → withdraw one's own message. The words go;
/// the position stays, so nobody's read state shifts.
///
/// # Errors
/// 404 not visible, 403 someone else's.
pub async fn delete_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = ChatMessageId::new(id);
    let message = account.acc.chat_message(&id).await.map_err(map_store_err)?;
    account
        .acc
        .delete_message(&id)
        .await
        .map_err(map_store_err)?;
    notify_room(&state, &account, &message.channel, &[]).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadBody {
    seq: i64,
}

/// `POST /chat/channels/{id}/read` `{seq}` → move the caller's read cursor
/// forward. It never moves backwards and never past what the room has said.
///
/// # Errors
/// 404 when the caller is not a member of the room.
pub async fn mark_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<ReadBody>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .mark_read(&ChatChannelId::new(id), body.seq)
        .await
        .map_err(map_store_err)?;
    // A read cursor is personal: only this person's other devices need it.
    push::notify_chat(&state, &account.tenant, std::slice::from_ref(&account.user)).await;
    Ok(StatusCode::NO_CONTENT)
}
