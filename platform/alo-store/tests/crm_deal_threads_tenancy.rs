//! Tenancy proof for the deal ↔ mail link (Law 1: isolation is tested, not
//! assumed) — the sharpest privacy boundary alo CRM has.
//!
//! A deal is **tenant-wide** while mail is **per user**, so this suite proves
//! two different denials, not one:
//!
//! - the tenant boundary, as everywhere else: another tenant's deal, and
//!   another tenant's conversation, are the clean `NotFound` on every path;
//! - the **mailbox** boundary inside one tenant: a colleague who does not hold a
//!   conversation cannot link it, sees it only as a linked subject with the name
//!   of whoever did, and never appears in anybody else's suggestions.
//!
//! It also proves the arc the queue item requires — link, idempotent re-link,
//! list, unlink, the per-deal cap — and that suggestions propose and never
//! write, with the free-mail rule that keeps private mail out of a record the
//! whole company reads.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::crm_deal_threads::DEAL_THREADS_MAX;
use alo_store::crm_thread_match::MatchReason;
use alo_store::{
    AccountStore, CrmDealId, CrmStageId, DealFilter, MailboxId, NewCustomer, NewDeal, Page,
    PipelineSeed, StageSeed, Store, StoreError, TenantId, ThreadId, UserId,
};

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error.
fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

/// A tenant with one user, returning the account door, the tenant, the user and
/// their inbox.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId, UserId, MailboxId) {
    let tenant = store.create_tenant(&format!("crmt-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@crmt.test"))
        .await
        .unwrap();
    let acc = store.for_account(tenant.clone(), user.clone());
    let inbox = acc.inbox().await.unwrap();
    (acc, tenant, user, inbox)
}

/// A second user of an existing tenant, with their own inbox — the colleague
/// every mailbox-boundary assertion below is made against.
async fn colleague(store: &Store, tenant: &TenantId, tag: &str) -> (AccountStore, MailboxId) {
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@crmt.test"))
        .await
        .unwrap();
    let acc = store.for_account(tenant.clone(), user);
    let inbox = acc.inbox().await.unwrap();
    (acc, inbox)
}

fn stage_seed(name: &str, is_won: bool, is_lost: bool) -> StageSeed {
    StageSeed {
        name: name.to_owned(),
        is_won,
        is_lost,
    }
}

fn sales_seed() -> PipelineSeed {
    PipelineSeed {
        name: "Sales".to_owned(),
        stages: vec![
            stage_seed("New", false, false),
            stage_seed("Qualified", false, false),
            stage_seed("Won", true, false),
            stage_seed("Lost", false, true),
        ],
    }
}

/// A deal on a freshly seeded board, with the contact address the suggestions
/// match on.
async fn deal_with_contact(acc: &AccountStore, title: &str, contact: &str) -> CrmDealId {
    let boards = acc.crm_pipelines_or_seed(&sales_seed()).await.unwrap();
    let board = boards[0].id.clone();
    let stages: Vec<CrmStageId> = acc
        .crm_stages(&board, false)
        .await
        .unwrap()
        .into_iter()
        .map(|s| s.id)
        .collect();
    acc.create_crm_deal(
        &board,
        &stages[0],
        &NewDeal {
            title: title.to_owned(),
            contact_email: contact.to_owned(),
            ..Default::default()
        },
    )
    .await
    .unwrap()
}

/// Delivers one message into an account and answers the conversation it landed
/// in — the only way a user comes to "hold" a thread.
async fn conversation(
    acc: &AccountStore,
    inbox: &MailboxId,
    tag: &str,
    from: &str,
    to: &str,
    subject: &str,
) -> ThreadId {
    let raw = format!(
        "From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\n\
         Message-ID: <{tag}@crmt.test>\r\n\r\nbody of {tag}\r\n"
    );
    let message = acc.ingest(inbox, raw.as_bytes()).await.unwrap();
    acc.message(&message).await.unwrap().thread_id
}

#[tokio::test]
async fn creating_from_mail_commits_the_deal_and_thread_link_together() {
    let store = common::test_store().await;
    let (account, _, _, inbox) = tenant_with_user(&store, "mail-create").await;
    let boards = account.crm_pipelines_or_seed(&sales_seed()).await.unwrap();
    let pipeline = boards[0].id.clone();
    let stage = account.crm_stages(&pipeline, false).await.unwrap()[0]
        .id
        .clone();
    let thread = conversation(
        &account,
        &inbox,
        "mail-create",
        "Ada <ada@acme.test>",
        "mail-create@crmt.test",
        "New website",
    )
    .await;

    let deal = account
        .create_crm_deal_from_thread(
            &pipeline,
            &stage,
            &NewDeal {
                title: "New website".to_owned(),
                contact_name: "Ada".to_owned(),
                contact_email: "ada@acme.test".to_owned(),
                source: "Email".to_owned(),
                ..Default::default()
            },
            &thread,
        )
        .await
        .unwrap();

    assert_eq!(
        account.crm_deal(&deal).await.unwrap().unwrap().source,
        "Email"
    );
    let links = account.crm_deal_threads(&deal).await.unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].thread_id, thread);
}

#[tokio::test]
async fn creating_from_an_unreadable_thread_writes_no_deal() {
    let store = common::test_store().await;
    let (owner, _, _, owner_inbox) = tenant_with_user(&store, "mail-owner").await;
    let (caller, _, _, _) = tenant_with_user(&store, "mail-caller").await;
    let foreign_thread = conversation(
        &owner,
        &owner_inbox,
        "foreign-create",
        "Ada <ada@acme.test>",
        "mail-owner@crmt.test",
        "Private lead",
    )
    .await;
    let boards = caller.crm_pipelines_or_seed(&sales_seed()).await.unwrap();
    let pipeline = boards[0].id.clone();
    let stage = caller.crm_stages(&pipeline, false).await.unwrap()[0]
        .id
        .clone();

    assert_not_found(
        caller
            .create_crm_deal_from_thread(
                &pipeline,
                &stage,
                &NewDeal {
                    title: "Must not exist".to_owned(),
                    ..Default::default()
                },
                &foreign_thread,
            )
            .await,
    );
    assert!(
        caller
            .crm_deals(&DealFilter::default())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_link_round_trips_and_never_crosses_a_tenant() {
    let store = common::test_store().await;
    let (a, ta, ua, a_inbox) = tenant_with_user(&store, "a").await;
    let (b, _tb, _ub, b_inbox) = tenant_with_user(&store, "b").await;

    let deal = deal_with_contact(&a, "Renewal — Acme GmbH", "ada@acme.test").await;
    let thread = conversation(
        &a,
        &a_inbox,
        "arc1",
        "Ada <ada@acme.test>",
        "a@crmt.test",
        "Renewal 2027",
    )
    .await;

    // ---- link, and link again ------------------------------------------
    assert!(a.link_crm_deal_thread(&deal, &thread).await.unwrap());
    assert!(
        !a.link_crm_deal_thread(&deal, &thread).await.unwrap(),
        "linking twice is the same link, not an error"
    );

    let linked = a.crm_deal_threads(&deal).await.unwrap();
    assert_eq!(linked.len(), 1);
    assert_eq!(linked[0].thread_id.as_str(), thread.as_str());
    assert_eq!(linked[0].subject, "Renewal 2027");
    assert!(linked[0].readable, "the linker holds the conversation");
    assert_eq!(linked[0].linked_by, ua.to_string());

    // ---- the neighbour's door -------------------------------------------
    let b_deal = deal_with_contact(&b, "Theirs", "someone@other.test").await;
    let b_thread = conversation(
        &b,
        &b_inbox,
        "arc2",
        "Someone <someone@other.test>",
        "b@crmt.test",
        "Theirs",
    )
    .await;

    // B cannot read, write or remove anything on A's deal — and the answer is
    // the same one an id that never existed gets.
    assert_not_found(b.crm_deal_threads(&deal).await);
    assert_not_found(b.link_crm_deal_thread(&deal, &b_thread).await);
    assert_not_found(b.unlink_crm_deal_thread(&deal, &thread).await);
    assert_not_found(b.suggest_crm_deal_threads(&deal, 10).await);
    let invented = CrmDealId::new("crd_nope");
    assert_not_found(b.crm_deal_threads(&invented).await);
    assert_not_found(b.link_crm_deal_thread(&invented, &b_thread).await);

    // And A cannot reach across the other way: B's conversation is not A's to
    // link, identically to a conversation that does not exist at all.
    assert_not_found(a.link_crm_deal_thread(&deal, &b_thread).await);
    assert_not_found(
        a.link_crm_deal_thread(&deal, &ThreadId::new("thr_nope"))
            .await,
    );

    // A's link is untouched by any of it.
    assert_eq!(a.crm_deal_threads(&deal).await.unwrap().len(), 1);
    assert!(b.crm_deal_threads(&b_deal).await.unwrap().is_empty());

    // ---- unlink ----------------------------------------------------------
    a.unlink_crm_deal_thread(&deal, &thread).await.unwrap();
    assert!(a.crm_deal_threads(&deal).await.unwrap().is_empty());
    assert_not_found(a.unlink_crm_deal_thread(&deal, &thread).await);
    // Unlinking destroyed nothing: the conversation is still the user's.
    assert!(
        a.link_crm_deal_thread(&deal, &thread).await.unwrap(),
        "the mail was never held by the link"
    );

    // Deleting the deal takes its links with it and leaves the mail alone.
    a.delete_crm_deal(&deal).await.unwrap();
    assert_not_found(a.crm_deal_threads(&deal).await);
    assert!(
        !a.thread_messages(&thread, Page::first(10))
            .await
            .unwrap()
            .is_empty(),
        "the mail outlives the deal that pointed at it"
    );
    let _ = ta;
}

#[tokio::test]
async fn a_colleague_who_does_not_hold_a_conversation_can_see_it_linked_but_not_open_it() {
    let store = common::test_store().await;
    let (a, tenant, ua, a_inbox) = tenant_with_user(&store, "hold").await;
    let (c, _c_inbox) = colleague(&store, &tenant, "held-not").await;
    let (d, d_inbox) = colleague(&store, &tenant, "held-too").await;

    let deal = deal_with_contact(&a, "Renewal", "ada@acme.test").await;
    let thread = conversation(
        &a,
        &a_inbox,
        "hold1",
        "Ada <ada@acme.test>",
        "hold@crmt.test",
        "Renewal 2027",
    )
    .await;
    a.link_crm_deal_thread(&deal, &thread).await.unwrap();

    // The colleague reads the DEAL — it is tenant-wide — and sees that a
    // conversation is linked, what it is called, and who linked it. They cannot
    // open it, and the store says so rather than leaving a silent gap.
    let seen = c.crm_deal_threads(&deal).await.unwrap();
    assert_eq!(seen.len(), 1);
    assert!(!seen[0].readable, "this colleague does not hold it");
    assert_eq!(
        seen[0].linked_by,
        ua.to_string(),
        "the answer is 'ask them'"
    );
    // The base subject is the one field that crosses, and it crosses as the
    // normalised label the thread row carries — never a body, never an address.
    assert_eq!(seen[0].subject, "renewal 2027");
    assert!(
        c.thread_messages(&thread, Page::first(10))
            .await
            .unwrap()
            .is_empty(),
        "the link is not a key to the mailbox"
    );

    // A colleague who cannot open it still cannot LINK one either: linking
    // requires the linker's own door.
    let their_deal = deal_with_contact(&c, "Theirs", "ada@acme.test").await;
    assert_not_found(c.link_crm_deal_thread(&their_deal, &thread).await);

    // Their OWN copy of the same conversation is a different thread row —
    // `AccountStore::resolve_thread` threads per user, so two colleagues on one
    // email hold two threads. Linking is therefore per copy, which is exactly
    // why `readable` is computed per reader and never assumed.
    let their_copy = conversation(
        &d,
        &d_inbox,
        "hold1-copy",
        "Ada <ada@acme.test>",
        "held-too@crmt.test",
        "Renewal 2027",
    )
    .await;
    assert_ne!(their_copy.as_str(), thread.as_str());
    let seen = d.crm_deal_threads(&deal).await.unwrap();
    assert!(
        !seen[0].readable,
        "a colleague's own copy is not the linked one"
    );
    // …and that copy is theirs to link to the same tenant-wide deal, where it
    // reads back as their own.
    assert!(d.link_crm_deal_thread(&deal, &their_copy).await.unwrap());
    let seen = d.crm_deal_threads(&deal).await.unwrap();
    let mine = seen
        .iter()
        .find(|t| t.thread_id.as_str() == their_copy.as_str())
        .unwrap();
    assert!(mine.readable);
    assert_eq!(mine.subject, "Renewal 2027", "their own copy names it");
    d.unlink_crm_deal_thread(&deal, &their_copy).await.unwrap();

    // And any member of the tenant may remove the link, including the colleague
    // who cannot open it — otherwise a link left by someone who has left the
    // company would be permanent.
    c.unlink_crm_deal_thread(&deal, &thread).await.unwrap();
    assert!(a.crm_deal_threads(&deal).await.unwrap().is_empty());
}

#[tokio::test]
async fn suggestions_propose_from_the_callers_own_mail_and_link_nothing() {
    let store = common::test_store().await;
    let (a, tenant, _ua, inbox) = tenant_with_user(&store, "sug").await;
    let (c, c_inbox) = colleague(&store, &tenant, "sug-mate").await;

    let deal = deal_with_contact(&a, "Renewal — Acme GmbH", "Ada <Ada@Acme.test>").await;

    let contact = conversation(
        &a,
        &inbox,
        "sug-contact",
        "Ada <ada@acme.test>",
        "sug@crmt.test",
        "Renewal 2027",
    )
    .await;
    let colleague_of_contact = conversation(
        &a,
        &inbox,
        "sug-domain",
        "Bob <bob@acme.test>",
        "sug@crmt.test",
        "Procurement",
    )
    .await;
    let outbound = conversation(
        &a,
        &inbox,
        "sug-outbound",
        "sug@crmt.test",
        "Ada <ada@acme.test>",
        "Our proposal",
    )
    .await;
    let unrelated = conversation(
        &a,
        &inbox,
        "sug-none",
        "friend@gmail.com",
        "sug@crmt.test",
        "Lunch",
    )
    .await;

    let proposed = a.suggest_crm_deal_threads(&deal, 10).await.unwrap();
    let ids: Vec<&str> = proposed.iter().map(|s| s.thread_id.as_str()).collect();
    assert!(ids.contains(&contact.as_str()));
    assert!(
        ids.contains(&outbound.as_str()),
        "a conversation WE started with the contact is the commonest sales thread"
    );
    assert!(ids.contains(&colleague_of_contact.as_str()));
    assert!(
        !ids.contains(&unrelated.as_str()),
        "nothing matched it, so nothing proposed it"
    );

    // Address matches rank above the domain match, whatever their order in the
    // mailbox, and each carries the reason a user reviews it by.
    let domain_rank = ids
        .iter()
        .position(|id| *id == colleague_of_contact.as_str())
        .unwrap();
    assert_eq!(domain_rank, ids.len() - 1);
    let by_id = |id: &ThreadId| {
        proposed
            .iter()
            .find(|s| s.thread_id.as_str() == id.as_str())
            .unwrap()
    };
    assert_eq!(by_id(&contact).reason, MatchReason::Address);
    assert_eq!(by_id(&contact).matched_address, "ada@acme.test");
    assert_eq!(by_id(&colleague_of_contact).reason, MatchReason::Domain);
    assert_eq!(
        by_id(&colleague_of_contact).matched_address,
        "bob@acme.test"
    );

    // Proposing wrote nothing at all.
    assert!(a.crm_deal_threads(&deal).await.unwrap().is_empty());

    // A conversation already linked is not proposed again.
    a.link_crm_deal_thread(&deal, &contact).await.unwrap();
    let proposed = a.suggest_crm_deal_threads(&deal, 10).await.unwrap();
    assert!(
        !proposed
            .iter()
            .any(|s| s.thread_id.as_str() == contact.as_str())
    );

    // The colleague reads the same tenant-wide deal and is proposed THEIR OWN
    // mail — none, here, because they hold none of these conversations.
    assert!(
        c.suggest_crm_deal_threads(&deal, 10)
            .await
            .unwrap()
            .is_empty()
    );
    let theirs = conversation(
        &c,
        &c_inbox,
        "sug-mate-own",
        "Ada <ada@acme.test>",
        "sug-mate@crmt.test",
        "Their own thread",
    )
    .await;
    let proposed = c.suggest_crm_deal_threads(&deal, 10).await.unwrap();
    assert_eq!(proposed.len(), 1);
    assert_eq!(proposed[0].thread_id.as_str(), theirs.as_str());

    // The limit is a page size, and a deal with no address to match on proposes
    // nothing rather than everything recent.
    assert_eq!(a.suggest_crm_deal_threads(&deal, 1).await.unwrap().len(), 1);
    let blank = deal_with_contact(&a, "A lead with no address", "  ").await;
    assert!(
        a.suggest_crm_deal_threads(&blank, 10)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_free_mail_customer_matches_only_on_the_exact_address() {
    let store = common::test_store().await;
    let (a, _t, _u, inbox) = tenant_with_user(&store, "free").await;

    // The deal's contact is at a free-mail domain — half of European SMEs.
    let deal = deal_with_contact(&a, "Lead — Ada", "ada@gmail.com").await;
    let theirs = conversation(
        &a,
        &inbox,
        "free-theirs",
        "Ada <ada@gmail.com>",
        "free@crmt.test",
        "About your offer",
    )
    .await;
    let private = conversation(
        &a,
        &inbox,
        "free-private",
        "Mum <mum@gmail.com>",
        "free@crmt.test",
        "Sunday",
    )
    .await;

    let proposed = a.suggest_crm_deal_threads(&deal, 10).await.unwrap();
    let ids: Vec<&str> = proposed.iter().map(|s| s.thread_id.as_str()).collect();
    assert_eq!(ids, vec![theirs.as_str()]);
    assert!(
        !ids.contains(&private.as_str()),
        "domain-matching a free-mail domain would attach private mail to a \
         record the whole company reads"
    );
}

#[tokio::test]
async fn the_customers_own_address_is_matched_too() {
    let store = common::test_store().await;
    let (a, _t, _u, inbox) = tenant_with_user(&store, "cust").await;

    let customer = a
        .create_billing_customer(&NewCustomer {
            name: "Acme GmbH".to_owned(),
            country: "DE".to_owned(),
            email: Some("billing@acme-invoices.test".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();
    let boards = a.crm_pipelines_or_seed(&sales_seed()).await.unwrap();
    let stages = a.crm_stages(&boards[0].id, false).await.unwrap();
    let deal = a
        .create_crm_deal(
            &boards[0].id,
            &stages[0].id,
            &NewDeal {
                title: "Renewal".to_owned(),
                customer_id: Some(customer.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let thread = conversation(
        &a,
        &inbox,
        "cust-1",
        "Accounts <accounts@acme-invoices.test>",
        "cust@crmt.test",
        "Your invoice",
    )
    .await;
    let proposed = a.suggest_crm_deal_threads(&deal, 10).await.unwrap();
    assert_eq!(proposed.len(), 1);
    assert_eq!(proposed[0].thread_id.as_str(), thread.as_str());
    assert_eq!(proposed[0].reason, MatchReason::Domain);
}

#[tokio::test]
async fn a_deal_holds_a_bounded_number_of_conversations() {
    let store = common::test_store().await;
    let (a, _t, _u, inbox) = tenant_with_user(&store, "cap").await;
    let deal = deal_with_contact(&a, "A very talkative deal", "ada@acme.test").await;

    let mut first: Option<ThreadId> = None;
    for n in 0..DEAL_THREADS_MAX {
        let thread = conversation(
            &a,
            &inbox,
            &format!("cap-{n}"),
            "Ada <ada@acme.test>",
            "cap@crmt.test",
            &format!("Thread {n}"),
        )
        .await;
        assert!(a.link_crm_deal_thread(&deal, &thread).await.unwrap());
        first.get_or_insert(thread);
    }
    assert_eq!(
        a.crm_deal_threads(&deal).await.unwrap().len(),
        usize::try_from(DEAL_THREADS_MAX).unwrap()
    );

    let one_more = conversation(
        &a,
        &inbox,
        "cap-over",
        "Ada <ada@acme.test>",
        "cap@crmt.test",
        "One too many",
    )
    .await;
    match a.link_crm_deal_thread(&deal, &one_more).await {
        Err(StoreError::Conflict(msg)) => {
            assert!(msg.contains("at most"), "{msg}");
        }
        other => panic!("expected Conflict, got: {other:?}"),
    }

    // A full deal still answers "yes, that one is linked" for a link it already
    // holds — the idempotent path is checked before the cap.
    let held = first.unwrap();
    assert!(!a.link_crm_deal_thread(&deal, &held).await.unwrap());

    // And freeing one place lets the next conversation in.
    a.unlink_crm_deal_thread(&deal, &held).await.unwrap();
    assert!(a.link_crm_deal_thread(&deal, &one_more).await.unwrap());
}
