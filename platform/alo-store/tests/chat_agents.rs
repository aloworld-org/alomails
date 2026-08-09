//! Agents as chat participants: identity without authority
//! (ADR 0034 §chat, `docs/design/chat-agents.md`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{ChannelVisibility, ChatAgentId, ProposalState, StoreError};
use serde_json::json;

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

/// An agent posts under its own name and acts with nobody's authority. Its
/// messages record the person whose reach produced them, and it appears only
/// in rooms it was put in.
#[tokio::test]
async fn an_agent_posts_as_itself_and_acts_as_the_person_who_asked() {
    let store = common::test_store().await;
    let t = store.create_tenant("agent-t").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("anna@agent.test").await.unwrap();
    let ub = ts.create_user("ben@agent.test").await.unwrap();
    let a = store.for_account(t.clone(), ua.clone());
    let b = store.for_account(t.clone(), ub.clone());

    let alo = a
        .create_agent("alo", "alo", Some("asks and answers"))
        .await
        .unwrap();

    // A handle is claimed once per tenant, and must be typeable.
    assert!(a.create_agent("alo", "another", None).await.is_err());
    assert!(a.create_agent("has space", "x", None).await.is_err());
    assert!(a.create_agent("", "x", None).await.is_err());
    // '@' is stripped rather than refused: people type it.
    let helper = a.create_agent("@helper", "Helper", None).await.unwrap();
    assert_eq!(
        a.agent(&helper).await.unwrap().handle,
        "helper",
        "a handle is stored without its '@'"
    );

    let room = a
        .create_channel("planning", None, ChannelVisibility::Public)
        .await
        .unwrap();

    // Being defined in the tenant is not permission to appear in a room.
    assert_not_found(a.post_as_agent(&room, &alo, "hello?", None).await);
    assert!(a.channel_agents(&room).await.unwrap().is_empty());

    a.add_agent_to_channel(&room, &alo).await.unwrap();
    assert_eq!(a.channel_agents(&room).await.unwrap().len(), 1);

    // Now it can speak — as itself, on Anna's behalf.
    let said = a
        .post_as_agent(&room, &alo, "Here is what I found.", None)
        .await
        .unwrap();
    assert_eq!(
        said.author.as_str(),
        alo.as_str(),
        "posted under its own id"
    );

    // An agent that does not exist here cannot be summoned.
    assert_not_found(
        a.post_as_agent(&room, &ChatAgentId::new("no-such".to_owned()), "hi", None)
            .await,
    );

    // Ben may READ this public room, so he sees which agents are in it —
    // reading a room includes seeing who is in it. But reading is not
    // membership, so he cannot make one speak.
    assert_eq!(b.channel_agents(&room).await.unwrap().len(), 1);
    assert_not_found(b.post_as_agent(&room, &alo, "hi", None).await);

    // A private room he is not in discloses nothing at all, agents included.
    let closed = a
        .create_channel("closed", None, ChannelVisibility::Private)
        .await
        .unwrap();
    a.add_agent_to_channel(&closed, &alo).await.unwrap();
    assert_not_found(b.channel_agents(&closed).await);
    assert_not_found(b.post_as_agent(&closed, &alo, "hi", None).await);
}

/// A proposal is decided by the person whose words caused it, exactly once,
/// and everyone else is refused with a permission rather than a silence.
#[tokio::test]
async fn only_the_asker_may_approve_what_an_agent_proposed() {
    let store = common::test_store().await;
    let t = store.create_tenant("propose-t").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("anna@propose.test").await.unwrap();
    let ub = ts.create_user("ben@propose.test").await.unwrap();
    let a = store.for_account(t.clone(), ua.clone());
    let b = store.for_account(t.clone(), ub.clone());

    let t2 = store.create_tenant("propose-t2").await.unwrap();
    let uc = store
        .for_tenant(t2.clone())
        .create_user("stranger@propose.test")
        .await
        .unwrap();
    let c = store.for_account(t2, uc);

    let alo = a.create_agent("alo", "alo", None).await.unwrap();
    let room = a
        .create_channel("planning", None, ChannelVisibility::Public)
        .await
        .unwrap();
    a.add_member(&room, &ub).await.unwrap();
    a.add_agent_to_channel(&room, &alo).await.unwrap();

    // Anna asks; the agent answers and proposes something.
    a.post_message(&room, "@alo make a task for the review", None)
        .await
        .unwrap();
    let answer = a
        .post_as_agent(&room, &alo, "I can create that task.", None)
        .await
        .unwrap();
    let proposal = a
        .propose_action(
            &answer.id,
            "create_task",
            &json!({ "title": "Review the plan" }),
        )
        .await
        .unwrap();

    // Ben is in the room and can SEE it — a proposal is not a secret.
    let seen = b.proposal(&proposal).await.unwrap();
    assert_eq!(seen.state, ProposalState::Pending);
    assert_eq!(seen.asked_by.as_str(), ua.as_str());
    assert_eq!(seen.tool, "create_task");

    // ...but it was computed through Anna's access, so only Anna may decide.
    // Forbidden, not NotFound: the secret is the permission, not the thing.
    assert_forbidden(b.decide_proposal(&proposal, true).await);

    // Another tenant cannot even see it.
    assert_not_found(c.proposal(&proposal).await);

    // Anna decides, once.
    let decided = a.decide_proposal(&proposal, true).await.unwrap();
    assert_eq!(decided.state, ProposalState::Approved);
    assert_eq!(
        decided.decided_by.map(|u| u.as_str().to_owned()),
        Some(ua.as_str().to_owned())
    );

    // A second tap is refused rather than run again.
    assert!(a.decide_proposal(&proposal, true).await.is_err());
    assert!(a.decide_proposal(&proposal, false).await.is_err());

    // And it draws on the page in one read.
    let page = a
        .proposals_for_channel(&room, std::slice::from_ref(&answer.id))
        .await
        .unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(
        page.get(answer.id.as_str()).map(|p| p.state),
        Some(ProposalState::Approved)
    );
}

/// Turning one down is a decision too, and it does not run.
#[tokio::test]
async fn a_discarded_proposal_stays_discarded() {
    let store = common::test_store().await;
    let t = store.create_tenant("discard-t").await.unwrap();
    let ua = store
        .for_tenant(t.clone())
        .create_user("anna@discard.test")
        .await
        .unwrap();
    let a = store.for_account(t, ua);

    let alo = a.create_agent("alo", "alo", None).await.unwrap();
    let room = a
        .create_channel("planning", None, ChannelVisibility::Public)
        .await
        .unwrap();
    a.add_agent_to_channel(&room, &alo).await.unwrap();
    let answer = a
        .post_as_agent(&room, &alo, "I could do that.", None)
        .await
        .unwrap();
    let proposal = a
        .propose_action(&answer.id, "create_task", &json!({ "title": "x" }))
        .await
        .unwrap();

    let decided = a.decide_proposal(&proposal, false).await.unwrap();
    assert_eq!(decided.state, ProposalState::Discarded);
    assert!(a.decide_proposal(&proposal, true).await.is_err());
}

/// Search sees exactly what reading sees. It is the likeliest place for a
/// private room to leak, because a naive implementation searches everything
/// and filters afterwards — and a filter is something someone eventually
/// forgets.
#[tokio::test]
async fn search_finds_only_what_the_reader_could_already_read() {
    let store = common::test_store().await;
    let t = store.create_tenant("search-t").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("anna@search.test").await.unwrap();
    let ub = ts.create_user("ben@search.test").await.unwrap();
    let a = store.for_account(t.clone(), ua);
    let b = store.for_account(t.clone(), ub.clone());

    let t2 = store.create_tenant("search-t2").await.unwrap();
    let uc = store
        .for_tenant(t2.clone())
        .create_user("stranger@search.test")
        .await
        .unwrap();
    let c = store.for_account(t2, uc);

    let open = a
        .create_channel("open", None, ChannelVisibility::Public)
        .await
        .unwrap();
    let closed = a
        .create_channel("closed", None, ChannelVisibility::Private)
        .await
        .unwrap();
    a.post_message(&open, "the pelican budget is approved", None)
        .await
        .unwrap();
    a.post_message(&closed, "the pelican salary review", None)
        .await
        .unwrap();

    // Anna is in both, so she finds both.
    assert_eq!(
        a.search_messages("pelican", None, 50).await.unwrap().len(),
        2
    );

    // Ben may read the live public room and is not in the private one. He
    // finds one — and never learns the other sentence exists.
    let bens = b.search_messages("pelican", None, 50).await.unwrap();
    assert_eq!(bens.len(), 1);
    assert!(bens[0].body.contains("budget"));

    // Another tenant finds nothing, however well they guess the words.
    assert!(
        c.search_messages("pelican", None, 50)
            .await
            .unwrap()
            .is_empty()
    );

    // Narrowing to one room, and a room that is not the caller's to see
    // simply yields nothing rather than an error that confirms it exists.
    assert_eq!(
        a.search_messages("pelican", Some(&open), 50)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        b.search_messages("pelican", Some(&closed), 50)
            .await
            .unwrap()
            .is_empty()
    );

    // Withdrawn words are not findable: the body is gone, so a hit would show
    // nothing.
    let said = a.post_message(&open, "pelican typo", None).await.unwrap();
    assert_eq!(a.search_messages("typo", None, 50).await.unwrap().len(), 1);
    a.delete_message(&said.id).await.unwrap();
    assert!(
        a.search_messages("typo", None, 50)
            .await
            .unwrap()
            .is_empty()
    );

    // An empty question asks nothing rather than returning everything.
    assert!(a.search_messages("   ", None, 50).await.unwrap().is_empty());
}

/// Finding a colleague is a search, never a listing. The distinction is the
/// whole privacy argument for having it at all: it answers "is there a Ben
/// here?" and refuses "give me everyone".
#[tokio::test]
async fn people_can_be_searched_for_but_never_listed() {
    let store = common::test_store().await;
    let t = store.create_tenant("people-t").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("anna@people.test").await.unwrap();
    ts.create_user("ben@people.test").await.unwrap();
    ts.create_user("bernard@people.test").await.unwrap();
    let a = store.for_account(t.clone(), ua.clone());

    let t2 = store.create_tenant("people-t2").await.unwrap();
    store
        .for_tenant(t2.clone())
        .create_user("ben@other.test")
        .await
        .unwrap();

    // A real search finds the people it should.
    let found = a.find_people("ber", 25).await.unwrap();
    assert_eq!(
        found.iter().map(|(_, e)| e.as_str()).collect::<Vec<_>>(),
        vec!["bernard@people.test"]
    );

    // Nothing to search for returns nothing — this is the line between a
    // finder and an export of the staff directory.
    assert!(a.find_people("", 25).await.unwrap().is_empty());
    assert!(
        a.find_people("b", 25).await.unwrap().is_empty(),
        "one letter is a listing in disguise"
    );
    assert!(a.find_people("   ", 25).await.unwrap().is_empty());

    // A wildcard is searched for literally, not honoured — otherwise "%" would
    // hand back everybody.
    assert!(a.find_people("%%", 25).await.unwrap().is_empty());
    assert!(a.find_people("__", 25).await.unwrap().is_empty());

    // Never yourself: a list of people to talk to should not offer you you.
    let all = a.find_people("people.test", 25).await.unwrap();
    assert!(!all.iter().any(|(u, _)| u.as_str() == ua.as_str()));
    assert_eq!(all.len(), 2, "ben and bernard, not anna");

    // Never across a tenant, however exactly the address is guessed. Anna is
    // in people-t; "ben@other.test" is real, and is not hers to find.
    assert!(
        a.find_people("ben@other.test", 25)
            .await
            .unwrap()
            .is_empty(),
        "another tenant's person is not findable even by exact address"
    );
    assert!(
        a.find_people("other.test", 25).await.unwrap().is_empty(),
        "nor by their domain"
    );
}
