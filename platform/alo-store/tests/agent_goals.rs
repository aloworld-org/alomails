//! Goals as objects (ADR 0058 §7, A8.3): the plan kept, progress that only
//! moves forward, one approval surface as a column, and — the mandatory half —
//! that a wrong tenant and a wrong colleague reach nothing through any of it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AgentGoalId, AgentProduct, ChannelVisibility, ChatProposalId, GoalEnd, GoalStatus, GoalStep,
    StoreError,
};

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

fn northstar_plan() -> Vec<GoalStep> {
    ["crm", "billing", "mail", "agenda"]
        .into_iter()
        .map(|agent| GoalStep {
            agent: agent.to_owned(),
            ask: format!("the {agent} part of closing Northstar"),
        })
        .collect()
}

/// The whole lifecycle, forward only: created working, waits behind exactly
/// one proposal, resumes past the approved step, and ends done — with the
/// plan unchanged from the day it was announced.
#[tokio::test]
async fn a_goal_walks_forward_through_wait_and_resume_to_done() {
    let store = common::test_store().await;
    let t = store.create_tenant("goal-life").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("anna@goal.test").await.unwrap();
    let a = store.for_account(t, ua);

    let room = a
        .create_channel("deals", None, ChannelVisibility::Public)
        .await
        .unwrap();
    let alo = a
        .create_agent("alo", "alo", None, AgentProduct::Workspace)
        .await
        .unwrap();

    let goal = a
        .create_goal(
            &room,
            &alo,
            "close the Northstar deal by Friday",
            &northstar_plan(),
        )
        .await
        .unwrap();
    assert_eq!(goal.status, GoalStatus::Working);
    assert_eq!(goal.cursor, 0);
    assert_eq!(goal.steps.len(), 4);
    assert!(goal.proposal.is_none());

    // Step one answered.
    a.advance_goal(&goal.id, 1).await.unwrap();
    // …but never backwards, and never past the plan.
    assert_validation(a.advance_goal(&goal.id, 1).await);
    assert_validation(a.advance_goal(&goal.id, 9).await);

    // Step two proposed: the goal waits behind that one proposal.
    let card = ChatProposalId::generate();
    a.goal_awaits(&goal.id, &card).await.unwrap();
    let held = a.goal(&goal.id).await.unwrap();
    assert_eq!(held.status, GoalStatus::Waiting);
    assert_eq!(held.proposal, Some(card.clone()));
    // Waiting is not working: nothing advances and nothing waits twice.
    assert_validation(a.advance_goal(&goal.id, 2).await);
    assert_validation(a.goal_awaits(&goal.id, &card).await);
    // The settled proposal finds its goal…
    let waiting = a.goal_waiting_on(&card).await.unwrap().unwrap();
    assert_eq!(waiting.id, goal.id);
    // …and an unrelated proposal finds nothing.
    assert!(
        a.goal_waiting_on(&ChatProposalId::generate())
            .await
            .unwrap()
            .is_none()
    );

    // Approved: the goal is handed back, past the step that proposed.
    let resumed = a.resume_goal(&goal.id).await.unwrap();
    assert_eq!(resumed.status, GoalStatus::Working);
    assert_eq!(resumed.cursor, 2);
    assert!(resumed.proposal.is_none());
    assert!(
        a.goal_waiting_on(&card).await.unwrap().is_none(),
        "a resumed goal waits on nothing"
    );

    // The rest runs; the goal ends done, and ended is ended.
    a.advance_goal(&goal.id, 3).await.unwrap();
    a.advance_goal(&goal.id, 4).await.unwrap();
    a.finish_goal(&goal.id, GoalEnd::Done, None).await.unwrap();
    let done = a.goal(&goal.id).await.unwrap();
    assert_eq!(done.status, GoalStatus::Done);
    assert_eq!(done.cursor, 4);
    assert_validation(a.finish_goal(&goal.id, GoalEnd::Stopped, None).await);
    assert_validation(a.advance_goal(&goal.id, 4).await);
    // The plan never drifted.
    assert_eq!(done.steps, northstar_plan());

    // The room lists it, newest first beside a second one.
    let second = a
        .create_goal(
            &room,
            &alo,
            "chase the overdue invoices",
            &northstar_plan()[..1],
        )
        .await
        .unwrap();
    let listed = a.channel_goals(&room).await.unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].id, second.id);
}

/// A goal can end from both live states — Stop works mid-run and mid-wait —
/// and a wait ended by a refusal leaves nothing waiting on the proposal.
#[tokio::test]
async fn a_goal_ends_from_working_and_from_waiting_alike() {
    let store = common::test_store().await;
    let t = store.create_tenant("goal-ends").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("anna@goalends.test").await.unwrap();
    let a = store.for_account(t, ua);
    let room = a
        .create_channel("deals", None, ChannelVisibility::Public)
        .await
        .unwrap();
    let alo = a
        .create_agent("alo", "alo", None, AgentProduct::Workspace)
        .await
        .unwrap();

    // Mid-run: Stop.
    let running = a
        .create_goal(&room, &alo, "first", &northstar_plan())
        .await
        .unwrap();
    a.finish_goal(&running.id, GoalEnd::Stopped, None)
        .await
        .unwrap();
    assert_eq!(
        a.goal(&running.id).await.unwrap().status,
        GoalStatus::Stopped
    );

    // Mid-wait: the proposal turned down. The note says so, and the proposal
    // column is cleared with the status — the two cannot disagree.
    let waiting = a
        .create_goal(&room, &alo, "second", &northstar_plan())
        .await
        .unwrap();
    let card = ChatProposalId::generate();
    a.goal_awaits(&waiting.id, &card).await.unwrap();
    a.finish_goal(&waiting.id, GoalEnd::Stopped, Some("turned down"))
        .await
        .unwrap();
    let ended = a.goal(&waiting.id).await.unwrap();
    assert_eq!(ended.status, GoalStatus::Stopped);
    assert_eq!(ended.note.as_deref(), Some("turned down"));
    assert!(ended.proposal.is_none());
    assert!(a.goal_waiting_on(&card).await.unwrap().is_none());
    // A goal that ended cannot be resumed.
    assert_validation(a.resume_goal(&waiting.id).await);

    // And the empty shapes are refused, not stored.
    assert_validation(a.create_goal(&room, &alo, "   ", &northstar_plan()).await);
    assert_validation(a.create_goal(&room, &alo, "no plan", &[]).await);
}

/// **The mandatory half.** A wrong tenant sees nothing and moves nothing; a
/// colleague in the same room reads the card but moves nothing; a room the
/// caller cannot see yields neither its goals nor a place to put one.
#[tokio::test]
async fn wrong_tenant_and_wrong_colleague_reach_nothing() {
    let store = common::test_store().await;
    let t = store.create_tenant("goal-iso").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("anna@goaliso.test").await.unwrap();
    let ub = ts.create_user("bram@goaliso.test").await.unwrap();
    let a = store.for_account(t.clone(), ua);
    let b = store.for_account(t, ub.clone());

    let t2 = store.create_tenant("goal-iso-2").await.unwrap();
    let uc = store
        .for_tenant(t2.clone())
        .create_user("eve@elsewhere.test")
        .await
        .unwrap();
    let c = store.for_account(t2, uc);

    let room = a
        .create_channel("deals", None, ChannelVisibility::Public)
        .await
        .unwrap();
    a.add_member(&room, &ub).await.unwrap();
    let alo = a
        .create_agent("alo", "alo", None, AgentProduct::Workspace)
        .await
        .unwrap();
    let goal = a
        .create_goal(&room, &alo, "close the Northstar deal", &northstar_plan())
        .await
        .unwrap();
    let card = ChatProposalId::generate();

    // The wrong tenant: the goal does not exist, in any verb.
    assert_not_found(c.goal(&goal.id).await);
    assert_not_found(c.channel_goals(&room).await);
    assert_not_found(c.advance_goal(&goal.id, 1).await);
    assert_not_found(c.goal_awaits(&goal.id, &card).await);
    assert_not_found(c.resume_goal(&goal.id).await);
    assert_not_found(c.finish_goal(&goal.id, GoalEnd::Stopped, None).await);
    assert_not_found(
        c.create_goal(&room, &alo, "steal the room", &northstar_plan())
            .await,
    );
    a.goal_awaits(&goal.id, &card).await.unwrap();
    assert!(c.goal_waiting_on(&card).await.unwrap().is_none());

    // The colleague: the room admits them to read — the card is part of the
    // conversation — and to nothing else. It was not their ask.
    assert_eq!(b.goal(&goal.id).await.unwrap().id, goal.id);
    assert_eq!(b.channel_goals(&room).await.unwrap().len(), 1);
    assert_forbidden(b.resume_goal(&goal.id).await);
    assert_forbidden(b.finish_goal(&goal.id, GoalEnd::Stopped, None).await);
    assert_forbidden(b.advance_goal(&goal.id, 1).await);
    // Even the waiting lookup is the asker's own: the colleague deciding is
    // already impossible at the proposal, and it finds no goal here either.
    assert!(b.goal_waiting_on(&card).await.unwrap().is_none());

    // A private room the asker is not in: its goals are not theirs to read,
    // and it is not a place they can put one.
    let closed = b
        .create_channel("closed", None, ChannelVisibility::Private)
        .await
        .unwrap();
    assert_not_found(a.channel_goals(&closed).await);
    assert_not_found(
        a.create_goal(&closed, &alo, "sneak in", &northstar_plan())
            .await,
    );
    let hidden = b
        .create_goal(&closed, &alo, "private work", &northstar_plan())
        .await
        .unwrap();
    assert_not_found(a.goal(&hidden.id).await);

    // An id never issued is the same answer as one that is not yours.
    assert_not_found(a.goal(&AgentGoalId::generate()).await);
}
