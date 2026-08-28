//! Every tool an agent ran, read or write (ADR 0047 §4).
//!
//! Two properties, and both are the point of the table existing: a read leaves
//! a record even though nobody approved it, and a record is **one person's**
//! — never another tenant's, and never a colleague's, however exactly its id
//! is guessed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AgentProduct, ChannelVisibility, ChatAgentId, ChatChannelId, NewAgentToolRun, StoreError,
};
use serde_json::json;

/// A read runs inside the turn with nobody's approval, so the log is the only
/// place it shows up — and the agent's record has to count it, or eleven
/// tools' worth of work becomes invisible.
#[tokio::test]
async fn a_read_leaves_a_record_even_though_nobody_approved_it() {
    let store = common::test_store().await;
    let t = store.create_tenant("toolrun-t").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("anna@toolrun.test").await.unwrap();
    let a = store.for_account(t.clone(), ua.clone());

    let agent = a
        .create_agent("inventory", "Inventory", None, AgentProduct::Inventory)
        .await
        .unwrap();
    let room = a
        .create_channel("stock", None, ChannelVisibility::Public)
        .await
        .unwrap();
    a.add_agent_to_channel(&room, &agent).await.unwrap();

    // Nothing yet: an agent that has done nothing reports nothing.
    assert!(a.agent_tool_runs(50).await.unwrap().is_empty());
    assert_eq!(
        a.agent_records()
            .await
            .unwrap()
            .get(agent.as_str())
            .map(|r| r.reads),
        None
    );

    let args = json!({ "product": "X100" });
    a.record_tool_run(&NewAgentToolRun {
        agent: Some(&agent),
        channel: Some(&room),
        tool: "stock_answer",
        effect: "read",
        args: &args,
        ok: true,
    })
    .await
    .unwrap();

    let runs = a.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].tool, "stock_answer");
    assert_eq!(runs[0].effect, "read");
    assert_eq!(runs[0].args["product"], "X100");
    assert!(runs[0].ok);
    // It ran through Anna's door, so it is recorded against Anna — an agent
    // has no access of its own to record.
    assert_eq!(runs[0].asked_by.as_str(), ua.as_str());
    assert_eq!(
        runs[0].agent.as_ref().map(ChatAgentId::as_str),
        Some(agent.as_str())
    );
    assert_eq!(
        runs[0].channel.as_ref().map(ChatChannelId::as_str),
        Some(room.as_str())
    );

    // …and it shows in the record, beside the answers and the approved actions.
    let records = a.agent_records().await.unwrap();
    assert_eq!(records.get(agent.as_str()).unwrap().reads, 1);
    assert_eq!(records.get(agent.as_str()).unwrap().answers, 0);

    // A refused write is recorded too, and is NOT counted as a read: an audit
    // that only kept the successes would hide exactly the interesting rows.
    a.record_tool_run(&NewAgentToolRun {
        agent: Some(&agent),
        channel: Some(&room),
        tool: "send_email",
        effect: "write",
        args: &args,
        ok: false,
    })
    .await
    .unwrap();
    assert_eq!(a.agent_tool_runs(50).await.unwrap().len(), 2);
    assert_eq!(
        a.agent_records()
            .await
            .unwrap()
            .get(agent.as_str())
            .unwrap()
            .reads,
        1,
        "a refused write is not a read"
    );

    // The palette's assistant is not a row in `chat_agents`, so its runs carry
    // no agent and belong to nobody's per-agent tally.
    a.record_tool_run(&NewAgentToolRun {
        agent: None,
        channel: None,
        tool: "find_file",
        effect: "read",
        args: &args,
        ok: true,
    })
    .await
    .unwrap();
    assert_eq!(a.agent_tool_runs(50).await.unwrap().len(), 3);
    assert_eq!(
        a.agent_records()
            .await
            .unwrap()
            .get(agent.as_str())
            .unwrap()
            .reads,
        1
    );

    // An effect that is neither is refused rather than stored: the column is
    // read back as a fact about what happened.
    assert!(matches!(
        a.record_tool_run(&NewAgentToolRun {
            agent: None,
            channel: None,
            tool: "stock_answer",
            effect: "peek",
            args: &args,
            ok: true,
        })
        .await,
        Err(StoreError::Validation(_))
    ));
}

/// The directory's per-agent window (A3.3): one agent's runs, most recent
/// first, and **only that agent's** — an entry that quietly included the
/// neighbouring agent's work would describe a reach it does not have.
#[tokio::test]
async fn one_agents_runs_are_that_agents_and_the_newest_come_first() {
    let store = common::test_store().await;
    let t = store.create_tenant("toolrun-one").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("anna@one.test").await.unwrap();
    let a = store.for_account(t.clone(), ua.clone());

    let inventory = a
        .create_agent("inventory", "Inventory", None, AgentProduct::Inventory)
        .await
        .unwrap();
    let agenda = a
        .create_agent("agenda", "Agenda", None, AgentProduct::Agenda)
        .await
        .unwrap();

    let args = json!({ "product": "X100" });
    for tool in ["stock_answer", "reorder_proposals"] {
        a.record_tool_run(&NewAgentToolRun {
            agent: Some(&inventory),
            channel: None,
            tool,
            effect: if tool == "stock_answer" {
                "read"
            } else {
                "write"
            },
            args: &args,
            ok: true,
        })
        .await
        .unwrap();
    }
    a.record_tool_run(&NewAgentToolRun {
        agent: Some(&agenda),
        channel: None,
        tool: "whats_on",
        effect: "read",
        args: &args,
        ok: true,
    })
    .await
    .unwrap();

    let runs = a.agent_tool_runs_for(&inventory, 20).await.unwrap();
    assert_eq!(runs.len(), 2, "{runs:?}");
    // Newest first, so a directory entry opens on what just happened.
    assert_eq!(runs[0].tool, "reorder_proposals");
    assert_eq!(runs[0].effect, "write");
    assert_eq!(runs[1].tool, "stock_answer");
    assert!(
        runs.iter().all(|r| r.tool != "whats_on"),
        "another agent's run is in this one's window: {runs:?}"
    );
    assert_eq!(a.agent_tool_runs_for(&agenda, 20).await.unwrap().len(), 1);

    // The window is a window: asking for one gets the newest one.
    let newest = a.agent_tool_runs_for(&inventory, 1).await.unwrap();
    assert_eq!(newest.len(), 1);
    assert_eq!(newest[0].tool, "reorder_proposals");

    // An agent that has run nothing reports nothing rather than failing, and so
    // does an id that was never issued — the refusal that says which it is
    // belongs to the route, not here.
    let quiet = a
        .create_agent("mail", "Mail", None, AgentProduct::Mail)
        .await
        .unwrap();
    assert!(a.agent_tool_runs_for(&quiet, 20).await.unwrap().is_empty());
    assert!(
        a.agent_tool_runs_for(&ChatAgentId::new("never-issued".to_owned()), 20)
            .await
            .unwrap()
            .is_empty()
    );
}

/// Constitution law #1. A run says which diary was opened and which room was
/// read on whose behalf; another tenant must not see one, and neither must a
/// colleague in the same tenant.
#[tokio::test]
async fn a_run_is_never_another_tenants_and_never_a_colleagues() {
    let store = common::test_store().await;
    let t = store.create_tenant("toolrun-iso").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("anna@iso.test").await.unwrap();
    let ub = ts.create_user("ben@iso.test").await.unwrap();
    let a = store.for_account(t.clone(), ua.clone());
    let b = store.for_account(t.clone(), ub.clone());

    let t2 = store.create_tenant("toolrun-iso2").await.unwrap();
    let uc = store
        .for_tenant(t2.clone())
        .create_user("carol@iso2.test")
        .await
        .unwrap();
    let c = store.for_account(t2.clone(), uc.clone());

    let agent = a
        .create_agent("agenda", "Agenda", None, AgentProduct::Agenda)
        .await
        .unwrap();
    let args = json!({ "from": "2026-08-14" });
    a.record_tool_run(&NewAgentToolRun {
        agent: Some(&agent),
        channel: None,
        tool: "whats_on",
        effect: "read",
        args: &args,
        ok: true,
    })
    .await
    .unwrap();

    assert_eq!(a.agent_tool_runs(50).await.unwrap().len(), 1);

    // Ben is in the same tenant and can see the same agent. He cannot see
    // which diary it opened for Anna: whose diary was read is Anna's business.
    assert!(
        b.agent_tool_runs(50).await.unwrap().is_empty(),
        "a colleague must not read another person's tool runs"
    );
    // …including through the directory's per-agent window, with the agent id
    // he legitimately knows because he can see the agent itself (A3.3).
    assert!(
        b.agent_tool_runs_for(&agent, 50).await.unwrap().is_empty(),
        "a colleague read another person's runs through the directory"
    );
    assert_eq!(
        b.agent_records()
            .await
            .unwrap()
            .get(agent.as_str())
            .map(|r| r.reads),
        None,
        "not even as a tally"
    );

    // Another tenant sees nothing at all, agent id and tool name guessed
    // exactly right.
    assert!(c.agent_tool_runs(50).await.unwrap().is_empty());
    assert!(c.agent_tool_runs_for(&agent, 50).await.unwrap().is_empty());
    assert!(c.agent_records().await.unwrap().is_empty());
    // …and writing one in tenant two does not reach into tenant one.
    c.record_tool_run(&NewAgentToolRun {
        agent: Some(&agent),
        channel: None,
        tool: "whats_on",
        effect: "read",
        args: &args,
        ok: true,
    })
    .await
    .unwrap();
    assert_eq!(
        a.agent_tool_runs(50).await.unwrap().len(),
        1,
        "another tenant's row must not appear here"
    );
    assert_eq!(c.agent_tool_runs(50).await.unwrap().len(), 1);
    // The agent id is the same string in both tenants, and the per-agent window
    // still separates them: the tenant is the first thing the query asks about,
    // not something the id is trusted to imply.
    assert_eq!(a.agent_tool_runs_for(&agent, 50).await.unwrap().len(), 1);
    assert_eq!(c.agent_tool_runs_for(&agent, 50).await.unwrap().len(), 1);
}
