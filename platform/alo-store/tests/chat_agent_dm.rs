//! A one-to-one with an agent (ADR 0048): a third room kind, one human, one
//! agent, private to the person who opened it.
//!
//! The properties worth a suite are the ones a name alone would not give you:
//! that opening it twice is the same room and opening it as two people is two
//! rooms, that it is refused by every path that assumes a channel, and — the
//! mandatory half — that a wrong tenant and a wrong colleague both reach
//! nothing through it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{AgentProduct, ChannelKind, ChatAgentId, ChatChannelId, StoreError};

fn assert_not_found<T: std::fmt::Debug>(r: Result<T, StoreError>) {
    assert!(
        matches!(r, Err(StoreError::NotFound)),
        "expected NotFound, got {r:?}"
    );
}

fn assert_forbidden<T: std::fmt::Debug>(r: Result<T, StoreError>) {
    assert!(
        matches!(r, Err(StoreError::Forbidden)),
        "expected Forbidden, got {r:?}"
    );
}

fn assert_validation<T: std::fmt::Debug>(r: Result<T, StoreError>) {
    assert!(
        matches!(r, Err(StoreError::Validation(_))),
        "expected Validation, got {r:?}"
    );
}

/// Opening it is idempotent, it holds exactly one human and one agent, and it
/// is the shape the room list already knows how to carry.
#[tokio::test]
async fn an_agent_dm_is_opened_once_and_holds_one_person_and_one_agent() {
    let store = common::test_store().await;
    let t = store.create_tenant("agentdm-open").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("anna@agentdm.test").await.unwrap();
    let a = store.for_account(t.clone(), ua.clone());

    let mail = a
        .create_agent(
            "mail",
            "Mail",
            Some("your correspondence"),
            AgentProduct::Mail,
        )
        .await
        .unwrap();

    // Nothing exists until it is asked for: a tenant with a dozen agents does
    // not get a dozen empty rooms.
    assert!(a.agent_dm(&mail).await.unwrap().is_none());

    let room = a.open_agent_dm(&mail).await.unwrap();
    assert_eq!(
        a.open_agent_dm(&mail).await.unwrap().as_str(),
        room.as_str(),
        "opening the same conversation twice is the same room"
    );
    assert_eq!(
        a.agent_dm(&mail).await.unwrap().unwrap().as_str(),
        room.as_str()
    );

    let read = a.channel(&room).await.unwrap();
    assert_eq!(read.kind, ChannelKind::AgentDm);
    assert!(read.name.is_none(), "a one-to-one has no name");
    assert_eq!(
        read.agent.as_ref().map(ChatAgentId::as_str),
        Some(mail.as_str()),
        "the room says which agent it is with"
    );

    // One human in `chat_members`, one agent in `chat_agent_members`.
    let people = a.channel_members(&room).await.unwrap();
    assert_eq!(people.len(), 1);
    assert_eq!(people[0].user.as_str(), ua.as_str());
    let agents = a.channel_agents(&room).await.unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].id.as_str(), mail.as_str());

    // It is in the caller's own room list, labelled by who it is with…
    let summaries = a.channel_summaries().await.unwrap();
    let mine = summaries
        .iter()
        .find(|s| s.channel.id.as_str() == room.as_str())
        .expect("an agent DM is listed beside the rest");
    assert_eq!(mine.counterpart.as_deref(), Some("@mail"));
    assert!(
        a.channels()
            .await
            .unwrap()
            .iter()
            .any(|c| c.id.as_str() == room.as_str())
    );
    // …and nowhere in discovery: a one-to-one is not a room to be browsed.
    assert!(
        a.joinable_channels()
            .await
            .unwrap()
            .iter()
            .all(|c| c.id.as_str() != room.as_str())
    );
}

/// The agent answers there exactly as it does in a channel: it may speak,
/// because it is in the room, and what it proposes still waits for its human.
#[tokio::test]
async fn an_agent_speaks_in_its_own_one_to_one_and_still_only_proposes() {
    let store = common::test_store().await;
    let t = store.create_tenant("agentdm-speak").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("anna@agentdm.test").await.unwrap();
    let a = store.for_account(t.clone(), ua.clone());

    let tasks = a
        .create_agent("tasks", "Tasks", None, AgentProduct::Tasks)
        .await
        .unwrap();
    let other = a
        .create_agent("drive", "Drive", None, AgentProduct::Drive)
        .await
        .unwrap();
    let room = a.open_agent_dm(&tasks).await.unwrap();

    a.post_message(&room, "what is on my plate?", None)
        .await
        .unwrap();
    let said = a
        .post_as_agent(&room, &tasks, "Three things are due today.", None)
        .await
        .unwrap();
    assert!(said.author_is_agent);
    assert_eq!(
        said.on_behalf_of.as_ref().map(|u| u.as_str()),
        Some(ua.as_str())
    );

    // An agent that is not this room's counterpart is not in the room, so it
    // cannot speak in it — being defined in the tenant is not permission to
    // appear in somebody's one-to-one with someone else.
    assert_not_found(a.post_as_agent(&room, &other, "hello?", None).await);

    // A proposal here is a proposal like any other: pending until its asker
    // taps it (ADR 0047 is unchanged by ADR 0048).
    let proposal = a
        .propose_action(
            &said.id,
            "create_task",
            &serde_json::json!({ "title": "call the supplier" }),
        )
        .await
        .unwrap();
    let held = a.proposal(&proposal).await.unwrap();
    assert_eq!(held.asked_by.as_str(), ua.as_str());
    assert_eq!(held.state, alo_store::ProposalState::Pending);
}

/// It cannot become a channel by accretion, and none of the channel verbs
/// apply to it — the same refusals a human DM already gives.
#[tokio::test]
async fn a_one_to_one_with_an_agent_stays_one_to_one() {
    let store = common::test_store().await;
    let t = store.create_tenant("agentdm-shape").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("anna@agentdm.test").await.unwrap();
    let ub = ts.create_user("ben@agentdm.test").await.unwrap();
    let a = store.for_account(t.clone(), ua.clone());

    let crm = a
        .create_agent("crm", "CRM", None, AgentProduct::Crm)
        .await
        .unwrap();
    let room = a.open_agent_dm(&crm).await.unwrap();

    assert_validation(a.add_member(&room, &ub).await);
    assert_validation(a.remove_member(&room, &ua).await);
    // Renaming and archiving are refused one step earlier, by ownership: the
    // one person in a one-to-one is a plain member, because there is nothing
    // here to own. That is exactly how a human DM already answers, and it is
    // the refusal being asserted rather than the message.
    assert_forbidden(a.rename_channel(&room, Some("agent"), None).await);
    assert_forbidden(a.archive_channel(&room).await);

    // The agent is reached through the room, not through the words: there is
    // no handle to type in a one-to-one.
    assert_eq!(
        a.channel_agent_counterpart(&room)
            .await
            .unwrap()
            .as_ref()
            .map(ChatAgentId::as_str),
        Some(crm.as_str())
    );
    // …and a named room has no counterpart to answer, so the same question
    // there returns nothing and the handle stays the only trigger.
    let named = a
        .create_channel("planning", None, alo_store::ChannelVisibility::Public)
        .await
        .unwrap();
    assert!(a.channel_agent_counterpart(&named).await.unwrap().is_none());
}

/// The isolation half, which is the one that matters: a colleague in the same
/// tenant and a stranger in another both reach nothing through an agent DM,
/// and two people asking the same agent get two separate conversations.
#[tokio::test]
async fn an_agent_dm_is_never_a_colleagues_and_never_another_tenants() {
    let store = common::test_store().await;
    let t = store.create_tenant("agentdm-iso-a").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("anna@agentdm.test").await.unwrap();
    let ub = ts.create_user("ben@agentdm.test").await.unwrap();
    let a = store.for_account(t.clone(), ua.clone());
    let b = store.for_account(t.clone(), ub.clone());

    let other_t = store.create_tenant("agentdm-iso-b").await.unwrap();
    let other_ts = store.for_tenant(other_t.clone());
    let ud = other_ts.create_user("dana@elsewhere.test").await.unwrap();
    let d = store.for_account(other_t.clone(), ud.clone());

    let hr = a
        .create_agent("hr", "HR", None, AgentProduct::Hr)
        .await
        .unwrap();
    let mine = a.open_agent_dm(&hr).await.unwrap();
    a.post_message(&mine, "what is my leave balance?", None)
        .await
        .unwrap();

    // A colleague of the same tenant: the room is not theirs to see at all,
    // and asking the same agent opens a room of their own.
    assert_not_found(b.channel(&mine).await);
    assert_not_found(b.channel_members(&mine).await);
    assert_not_found(b.channel_agents(&mine).await);
    assert_not_found(b.messages(&mine, None, 50).await);
    assert!(b.agent_dm(&hr).await.unwrap().is_none());
    let theirs = b.open_agent_dm(&hr).await.unwrap();
    assert_ne!(
        theirs.as_str(),
        mine.as_str(),
        "one room per person per agent"
    );
    assert!(
        b.channels()
            .await
            .unwrap()
            .iter()
            .all(|c| c.id.as_str() != mine.as_str())
    );
    assert!(
        b.channel_summaries()
            .await
            .unwrap()
            .iter()
            .all(|s| s.channel.id.as_str() != mine.as_str())
    );

    // Another tenant: the agent does not exist for them, so neither does the
    // room — and naming either id reaches nothing.
    assert_not_found(d.agent(&hr).await);
    assert_not_found(d.open_agent_dm(&hr).await);
    assert!(d.agent_dm(&hr).await.unwrap().is_none());
    assert_not_found(d.channel(&mine).await);
    assert_not_found(d.messages(&mine, None, 50).await);
    assert!(d.channels().await.unwrap().is_empty());

    // And an id from another tenant's table is not a shortcut either.
    let their_agent = d
        .create_agent("hr", "HR", None, AgentProduct::Hr)
        .await
        .unwrap();
    assert_not_found(a.open_agent_dm(&their_agent).await);
    assert_not_found(
        a.channel(&ChatChannelId::new("no-such-room".to_owned()))
            .await,
    );
}
