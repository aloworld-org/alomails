//! Property tests for the threading algorithm (RFC 8621 §3 / RFC 5322
//! References): reply chains converge to one thread; unrelated messages
//! never merge. Runs against real Postgres.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use std::collections::HashSet;

use alo_store::{AccountStore, MessageId, Page};

async fn thread_of(ts: &AccountStore, message: &MessageId) -> String {
    ts.message(message).await.unwrap().thread_id.to_string()
}

#[tokio::test]
async fn reply_chain_converges_to_one_thread() {
    let store = common::test_store().await;
    let (ts, _user, inbox) = common::fresh_account(&store, "chain").await;

    let root = common::deliver(&ts, &inbox, "<root@x>", &[], "Project kickoff").await;
    let mut messages = vec![root];
    let mut prev = "<root@x>".to_owned();
    for i in 0..12 {
        let mid = format!("<r{i}@x>");
        let m = common::deliver(
            &ts,
            &inbox,
            &mid,
            &[prev.as_str(), "<root@x>"],
            "Re: Project kickoff",
        )
        .await;
        messages.push(m);
        prev = mid;
    }

    let mut threads = HashSet::new();
    for m in &messages {
        threads.insert(thread_of(&ts, m).await);
    }
    assert_eq!(threads.len(), 1, "the whole reply chain is one thread");

    // And the thread enumerates every message.
    let thread_id = ts.message(&messages[0]).await.unwrap().thread_id;
    let members = ts
        .thread_messages(&thread_id, Page::first(100))
        .await
        .unwrap();
    assert_eq!(members.len(), messages.len());
}

#[tokio::test]
async fn unrelated_messages_never_merge() {
    let store = common::test_store().await;
    let (ts, _user, inbox) = common::fresh_account(&store, "distinct").await;

    let mut threads = HashSet::new();
    for i in 0..10 {
        let m = common::deliver(
            &ts,
            &inbox,
            &format!("<u{i}@x>"),
            &[],
            &format!("Unrelated subject {i}"),
        )
        .await;
        threads.insert(thread_of(&ts, &m).await);
    }
    assert_eq!(threads.len(), 10, "unrelated messages get distinct threads");
}

#[tokio::test]
async fn same_subject_without_references_does_not_merge() {
    // Documented interop choice: we thread on references, not subject
    // alone, so two independent "Re: Hi" with no References stay apart.
    let store = common::test_store().await;
    let (ts, _user, inbox) = common::fresh_account(&store, "subjonly").await;
    let a = common::deliver(&ts, &inbox, "<a@x>", &[], "Re: Hi").await;
    let b = common::deliver(&ts, &inbox, "<b@x>", &[], "Re: Hi").await;
    assert_ne!(thread_of(&ts, &a).await, thread_of(&ts, &b).await);
}

#[tokio::test]
async fn duplicate_message_id_joins_one_thread() {
    // A message the account both sends and receives lands twice with the
    // *same* Message-ID (the Sent copy and the delivered copy), and a
    // self-addressed or list-echoed message arrives twice likewise. Even
    // with no References to link them, both copies — and any reply that
    // references that Message-ID — must resolve to a single thread.
    let store = common::test_store().await;
    let (ts, _user, inbox) = common::fresh_account(&store, "dupmid").await;

    let sent_copy = common::deliver(&ts, &inbox, "<orig@x>", &[], "Lunch?").await;
    let inbox_copy = common::deliver(&ts, &inbox, "<orig@x>", &[], "Lunch?").await;
    let reply = common::deliver(&ts, &inbox, "<reply@x>", &["<orig@x>"], "Re: Lunch?").await;

    let t = thread_of(&ts, &sent_copy).await;
    assert_eq!(
        thread_of(&ts, &inbox_copy).await,
        t,
        "both copies share a thread"
    );
    assert_eq!(
        thread_of(&ts, &reply).await,
        t,
        "the reply joins that thread"
    );
}

#[tokio::test]
async fn randomized_forests_group_by_chain() {
    // A deterministic pseudo-random forest of reply chains, delivered in
    // a shuffled order. Every message in a chain must share one thread;
    // different chains must not collide.
    let store = common::test_store().await;
    let (ts, _user, inbox) = common::fresh_account(&store, "forest").await;

    // Build chains: chain c has roots <c-0@x> and replies <c-k@x> each
    // referencing <c-(k-1)@x>.
    let chains = 6;
    let per_chain = 5;
    let mut plan: Vec<(String, Vec<String>, usize)> = Vec::new(); // (msgid, refs, chain)
    for c in 0..chains {
        for k in 0..per_chain {
            let mid = format!("<{c}-{k}@x>");
            let refs = if k == 0 {
                vec![]
            } else {
                vec![format!("<{c}-{}@x>", k - 1)]
            };
            plan.push((mid, refs, c));
        }
    }
    // Interleave chains unpredictably while keeping each chain's internal
    // order (a parent always precedes its child — the algorithm threads
    // forward). A precomputed per-chain rank gives a *consistent* sort
    // key (mutating state inside the comparator would break Ord).
    let chain_rank: Vec<u64> = (0..chains)
        .map(|c| {
            (0x9E37_79B9_7F4A_7C15u64 ^ (c as u64))
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1)
        })
        .collect();
    let mut order: Vec<usize> = (0..plan.len()).collect();
    order.sort_by_key(|&i| (chain_rank[plan[i].2], i));

    let mut chain_thread: Vec<Option<String>> = vec![None; chains];
    let mut all_threads = HashSet::new();
    for &i in &order {
        let (mid, refs, c) = &plan[i];
        let refs_ref: Vec<&str> = refs.iter().map(String::as_str).collect();
        let m = common::deliver(&ts, &inbox, mid, &refs_ref, "subject").await;
        let t = thread_of(&ts, &m).await;
        match &chain_thread[*c] {
            Some(existing) => assert_eq!(existing, &t, "chain {c} split across threads"),
            None => chain_thread[*c] = Some(t.clone()),
        }
        all_threads.insert(t);
    }
    // Each chain is exactly one thread, and chains do not merge.
    assert_eq!(all_threads.len(), chains, "distinct chains stayed distinct");
}
