//! The executors of alo Chat's verbs (ADR 0058) — what runs when the Chat
//! agent uses one of the intents `alo_ai::chat_intents` describes.
//!
//! Every executor runs through the asker's account door and answers with the
//! **record view** Chat's own routes serve ([`crate::chat`]'s `summary_json`,
//! `member_json`, `message_json`) — an agent grounds in exactly what the
//! sidebar and the feed draw, and there is no second summary of a room. A room
//! the asker cannot see does not exist here: a read reports it as not found,
//! indistinguishable on purpose from there being no such room, and never as
//! forbidden.
//!
//! The two writes only ever run from the asker's approval
//! ([`crate::agent::execute_tool`] holds that, not this module) and do what the
//! routes do: `post_message` posts **as the asker, in their own name**, so the
//! room is notified and a mentioned agent answers exactly as if they had typed
//! it; `create_room` makes them the owner of a new named room. The words posted
//! are the proposal's words — the preview showed them, and nothing rewrites
//! them on the way through.

use serde_json::{Value, json};

use alo_store::{ChannelVisibility, UserId};

use crate::agent_args::{string_arg, unprocessable};
use crate::agent_reads::room_named;
use crate::billing::map_store_err;
use crate::error::Problem;
use crate::state::{Account, AppState};

/// How many rooms a list read returns — a sidebar's worth, small enough to
/// sit inside the turn's result window.
const MAX_LISTED: usize = 12;

pub(crate) type Reply = Result<axum::Json<Value>, Problem>;

/// Every read's answer. Chat has no money to make readable, so the record
/// views pass through as they are.
fn ok(result: Value) -> Reply {
    Ok(axum::Json(json!({ "ok": true, "result": result })))
}

/// `my_rooms` — the caller's conversations, liveliest first, as the sidebar
/// sees them: unread counts, mentions, who a one-to-one is with, the last
/// thing said.
pub async fn execute_my_rooms(account: &Account) -> Reply {
    let summaries = account
        .acc
        .channel_summaries()
        .await
        .map_err(map_store_err)?;
    let mentions = account.acc.unread_mentions().await.unwrap_or_default();
    ok(json!({
        "kind": "chatRooms",
        "total": summaries.len(),
        "rooms": summaries
            .iter()
            .take(MAX_LISTED)
            .map(|s| crate::chat::summary_json(s, &mentions))
            .collect::<Vec<_>>(),
    }))
}

/// `unread_rooms` — only the conversations with something unread, so "what
/// did I miss" is counted from the read cursors rather than guessed.
pub async fn execute_unread_rooms(account: &Account) -> Reply {
    let summaries = account
        .acc
        .channel_summaries()
        .await
        .map_err(map_store_err)?;
    let mentions = account.acc.unread_mentions().await.unwrap_or_default();
    let unread: Vec<_> = summaries.iter().filter(|s| s.unread > 0).collect();
    ok(json!({
        "kind": "chatUnread",
        "unreadRooms": unread.len(),
        "rooms": unread
            .iter()
            .take(MAX_LISTED)
            .map(|s| crate::chat::summary_json(s, &mentions))
            .collect::<Vec<_>>(),
    }))
}

/// `room_members` — who is in one room, with addresses and roles.
///
/// # Errors
/// 422 when no room was named.
pub async fn execute_room_members(state: &AppState, account: &Account, args: &Value) -> Reply {
    let room = string_arg(args, "room").ok_or_else(|| unprocessable("room is required"))?;
    let Some(id) = room_named(account, &room).await? else {
        // Not an error: the model was told to say so rather than guess, and a
        // 404 here would tell the caller a private room exists.
        return ok(json!({ "kind": "chatMembers", "room": room, "found": false, "members": [] }));
    };
    let channel = account.acc.channel(&id).await.map_err(map_store_err)?;
    let members = account
        .acc
        .channel_members(&id)
        .await
        .map_err(map_store_err)?;
    let who: Vec<UserId> = members.iter().map(|m| m.user.clone()).collect();
    let emails = crate::chat::resolve_emails(state, account, &who).await;
    ok(json!({
        "kind": "chatMembers",
        "room": room,
        "found": true,
        "topic": channel.topic,
        "members": members
            .iter()
            .map(|m| crate::chat::member_json(m, &emails))
            .collect::<Vec<_>>(),
    }))
}

/// `post_message` — a write, run on the asker's approval: their words, their
/// name, a room they are a member of. The room is notified and a mentioned
/// agent answers, exactly as when the same words are typed into the feed.
///
/// # Errors
/// 422 when the room or the message is missing, or no room of that name is
/// the asker's to post in; the store's own refusals (not a member, archived
/// room, empty or over-long text) pass through.
pub async fn execute_post_message(state: &AppState, account: &Account, args: &Value) -> Reply {
    let room = string_arg(args, "room").ok_or_else(|| unprocessable("room is required"))?;
    let text = string_arg(args, "message").ok_or_else(|| unprocessable("message is required"))?;
    let Some(id) = room_named(account, &room).await? else {
        return Err(unprocessable(format!(
            "no room called {room} is yours to post in"
        )));
    };
    let message = account
        .acc
        .post_message(&id, &text, None)
        .await
        .map_err(map_store_err)?;
    let mentions = account
        .acc
        .mentions_for_channel(&message.channel, std::slice::from_ref(&message.id))
        .await
        .unwrap_or_default();
    let named: Vec<UserId> = mentions.values().flatten().cloned().collect();
    crate::chat::notify_room(state, account, &message.channel, &named).await;
    crate::chat_agent::answer_if_asked(
        state,
        account,
        &message.channel,
        &message.body,
        &message.id,
    );
    let emails =
        crate::chat::resolve_emails(state, account, std::slice::from_ref(&message.author)).await;
    ok(json!({
        "kind": "chatMessagePosted",
        "room": room,
        "message": crate::chat::message_json(&message, &emails),
    }))
}

/// `create_room` — a write, run on the asker's approval: a named room with
/// them as its owner. Nobody else is added here.
///
/// # Errors
/// 422 when no name was given or the visibility is neither public nor
/// private; the store's own refusals (name taken, over-long) pass through.
pub async fn execute_create_room(state: &AppState, account: &Account, args: &Value) -> Reply {
    let name = string_arg(args, "name").ok_or_else(|| unprocessable("name the room"))?;
    let name = name.trim_start_matches('#').trim().to_owned();
    if name.is_empty() {
        return Err(unprocessable("name the room"));
    }
    let visibility = match string_arg(args, "visibility")
        .map(|v| v.to_lowercase())
        .as_deref()
    {
        None | Some("public") => ChannelVisibility::Public,
        Some("private") => ChannelVisibility::Private,
        Some(other) => {
            return Err(unprocessable(format!(
                "a room is public or private, not {other}"
            )));
        }
    };
    let topic = string_arg(args, "topic");
    let id = account
        .acc
        .create_channel(&name, topic.as_deref(), visibility)
        .await
        .map_err(map_store_err)?;
    let channel = account.acc.channel(&id).await.map_err(map_store_err)?;
    crate::chat::notify_room(state, account, &id, &[]).await;
    ok(json!({
        "kind": "chatRoomCreated",
        "room": crate::chat::channel_json(&channel),
    }))
}

/// The module's verbs by name (A4.1c) — Chat's one row in the agent's
/// dispatcher list, `crate::agent::MODULES`. `None` is "not mine": the
/// dispatcher then asks the next module, so two modules never need to know of
/// each other. The two verbs the old tool set already had keep their
/// executors in [`crate::agent_reads`], and are reached from here so the
/// agent has one place to look.
pub(crate) fn dispatch<'a>(
    state: &'a AppState,
    account: &'a Account,
    tool: &'a str,
    args: &'a Value,
) -> Option<crate::agent::Dispatched<'a>> {
    let run: crate::agent::Dispatched<'a> = match tool {
        "my_rooms" => Box::pin(execute_my_rooms(account)),
        "unread_rooms" => Box::pin(execute_unread_rooms(account)),
        "room_members" => Box::pin(execute_room_members(state, account, args)),
        "catch_up_room" => Box::pin(crate::agent_reads::execute_catch_up_room(account, args)),
        "find_in_chat" => Box::pin(crate::agent_reads::execute_find_in_chat(account, args)),
        "post_message" => Box::pin(execute_post_message(state, account, args)),
        "create_room" => Box::pin(execute_create_room(state, account, args)),
        _ => return None,
    };
    Some(run)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use alo_ai::chat_intents::CHAT;

    /// Every `/chat/` route the router registers is the adapter of a verb or
    /// excluded with a reason — the coverage ADR 0058 makes structural.
    #[test]
    fn every_chat_route_is_a_verb_or_an_exclusion() {
        let router = include_str!("server.rs");
        let missing = CHAT.uncovered(router, "/chat/");
        assert!(
            missing.is_empty(),
            "routes with neither a verb nor a reason: {missing:?}"
        );
        // …and every verb's route exists, so an intent cannot name a route
        // the app does not have.
        let routes = alo_ai::routes_in(router, "/chat/");
        for intent in CHAT.intents {
            for route in intent.routes {
                assert!(
                    routes.contains(&(*route).to_owned()),
                    "{}: {route} is not a route",
                    intent.name
                );
            }
        }
    }

    #[test]
    fn every_verb_the_registry_offers_is_dispatched() {
        let dispatch = include_str!("chat_intents.rs");
        for intent in CHAT.intents {
            assert!(
                dispatch.contains(&format!("\"{}\" =>", intent.name)),
                "{} has no executor in the dispatch",
                intent.name
            );
        }
    }

    /// Chat's registration is one row in each list (A4.1c): the agent's
    /// dispatcher names this module once, and the two lists are the same
    /// length — every moved module has its dispatcher.
    #[test]
    fn the_module_is_one_row_in_each_list() {
        let agent = include_str!("agent.rs");
        assert_eq!(
            agent.matches("chat_intents::").count(),
            1,
            "agent.rs names Chat only in MODULES"
        );
        assert!(agent.contains("crate::chat_intents::dispatch"));
        assert_eq!(
            crate::agent::MODULES.len(),
            alo_ai::MOVED.len(),
            "a moved module without a dispatcher, or the reverse"
        );
    }
}
