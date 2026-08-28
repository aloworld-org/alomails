//! Tenancy proof for a deal's log and its next steps (Law 1: isolation is
//! tested, not assumed) — B2.06.
//!
//! Two records, two different boundaries, proven separately:
//!
//! - **the activity log** is tenant-wide like the deal it hangs on, so the
//!   denial to prove is the tenant one — another tenant's deal and another
//!   tenant's entry are the clean `NotFound` on every path — plus the one rule
//!   inside the tenant: only the author may delete, and a colleague who tries
//!   reads `Forbidden`, never a lie about the row's existence;
//! - **a next step is a real task**, so it inherits the tasks module's own
//!   visibility: a next step filed on a colleague's personal project is theirs,
//!   and appears for somebody else only when it is assigned to them. A deal
//!   being tenant-wide does not widen a task by one row.
//!
//! It also proves the arc the queue item requires — write, read back newest
//! first, delete; create a next step that really is a task with the deal as its
//! source, and that shows its due date in the deal.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use time::{Duration, OffsetDateTime};

use alo_store::crm_activities::{ACTIVITY_BODY_MAX_CHARS, DEAL_ACTIVITIES_MAX};
use alo_store::{
    AccountStore, ActivityKind, CrmActivityId, CrmDealId, DEAL_SOURCE_KIND, NewActivity, NewDeal,
    NewTask, PipelineSeed, StageSeed, Store, StoreError, TenantId, UserId,
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

/// A tenant with one user, returning the account door, the tenant and the user.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId, UserId) {
    let tenant = store.create_tenant(&format!("crma-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@crma.test"))
        .await
        .unwrap();
    let acc = store.for_account(tenant.clone(), user.clone());
    (acc, tenant, user)
}

/// A second user of an existing tenant — the colleague every rule inside the
/// tenant is asserted against.
async fn colleague(store: &Store, tenant: &TenantId, tag: &str) -> (AccountStore, UserId) {
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@crma.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user.clone()), user)
}

fn sales_seed() -> PipelineSeed {
    PipelineSeed {
        name: "Sales".to_owned(),
        stages: vec![
            StageSeed {
                name: "New".to_owned(),
                is_won: false,
                is_lost: false,
            },
            StageSeed {
                name: "Won".to_owned(),
                is_won: true,
                is_lost: false,
            },
        ],
    }
}

/// A deal on a freshly seeded board.
async fn deal(acc: &AccountStore, title: &str) -> CrmDealId {
    let boards = acc.crm_pipelines_or_seed(&sales_seed()).await.unwrap();
    let stages = acc.crm_stages(&boards[0].id, false).await.unwrap();
    acc.create_crm_deal(
        &boards[0].id,
        &stages[0].id,
        &NewDeal {
            title: title.to_owned(),
            ..Default::default()
        },
    )
    .await
    .unwrap()
}

fn note(body: &str) -> NewActivity {
    NewActivity {
        body: body.to_owned(),
        ..Default::default()
    }
}

// ---- the log -----------------------------------------------------------------

#[tokio::test]
async fn a_deals_log_round_trips_and_never_crosses_a_tenant() {
    let store = common::test_store().await;
    let (a, _ta, ua) = tenant_with_user(&store, "log-a").await;
    let (b, _tb, _ub) = tenant_with_user(&store, "log-b").await;

    let deal_a = deal(&a, "Renewal — Acme GmbH").await;
    assert!(a.crm_activities(&deal_a).await.unwrap().is_empty());

    // Written once, with the moment it happened rather than the moment it was
    // typed up.
    let called_at = OffsetDateTime::now_utc() - Duration::hours(3);
    let call = a
        .add_crm_activity(
            &deal_a,
            &NewActivity {
                kind: ActivityKind::Call,
                body: "Ada wants the renewal quoted for 40 seats.".to_owned(),
                happened_at: Some(called_at),
            },
        )
        .await
        .unwrap();
    let written = a
        .add_crm_activity(&deal_a, &note("Sent the deck."))
        .await
        .unwrap();

    // Newest first: the note typed second happened now, the call three hours
    // ago, so the order is by WHEN IT HAPPENED and not by when it was written.
    let log = a.crm_activities(&deal_a).await.unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].id.as_str(), written.as_str());
    assert_eq!(log[1].id.as_str(), call.as_str());
    assert_eq!(log[1].kind, ActivityKind::Call);
    assert_eq!(log[1].author_user_id, ua.to_string());
    assert_eq!(log[1].deal_id.as_str(), deal_a.as_str());
    assert!(
        (log[1].happened_at - called_at).abs() < Duration::seconds(1),
        "the call keeps the hour it took place"
    );
    assert!(
        log[1].created_at > log[1].happened_at,
        "and still knows when it was entered"
    );

    // ---- the neighbour's door -------------------------------------------
    let deal_b = deal(&b, "Theirs").await;
    assert_not_found(b.crm_activities(&deal_a).await);
    assert_not_found(b.add_crm_activity(&deal_a, &note("mine now")).await);
    assert_not_found(b.delete_crm_activity(&call).await);
    // Identical answers for an id that never existed: no existence oracle.
    let invented = CrmDealId::new("crd_nope");
    assert_not_found(b.crm_activities(&invented).await);
    assert_not_found(b.add_crm_activity(&invented, &note("x")).await);
    assert_not_found(b.delete_crm_activity(&CrmActivityId::new("cra_nope")).await);
    assert!(b.crm_activities(&deal_b).await.unwrap().is_empty());
    assert_eq!(a.crm_activities(&deal_a).await.unwrap().len(), 2);

    // ---- delete, and the record's end ------------------------------------
    a.delete_crm_activity(&written).await.unwrap();
    assert_eq!(a.crm_activities(&deal_a).await.unwrap().len(), 1);
    assert_not_found(a.delete_crm_activity(&written).await);

    // Deleting the deal takes its log with it.
    a.delete_crm_deal(&deal_a).await.unwrap();
    assert_not_found(a.crm_activities(&deal_a).await);
    assert_not_found(a.delete_crm_activity(&call).await);
}

#[tokio::test]
async fn only_the_author_may_delete_and_a_colleague_is_told_so_plainly() {
    let store = common::test_store().await;
    let (a, tenant, ua) = tenant_with_user(&store, "author").await;
    let (c, _uc) = colleague(&store, &tenant, "author-mate").await;

    let deal_a = deal(&a, "Renewal").await;
    let mine = a
        .add_crm_activity(&deal_a, &note("Called Ada, she is in."))
        .await
        .unwrap();

    // The colleague reads the log — it is tenant-wide, like the deal.
    let seen = c.crm_activities(&deal_a).await.unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].author_user_id, ua.to_string());

    // …and may write their own entry against the same deal.
    let theirs = c
        .add_crm_activity(&deal_a, &note("Met their CTO at the fair."))
        .await
        .unwrap();

    // But they may not delete somebody else's, and the answer names the reason
    // rather than pretending the row is not there: they are looking at it.
    match c.delete_crm_activity(&mine).await {
        Err(StoreError::Forbidden) => {}
        other => panic!("expected Forbidden, got: {other:?}"),
    }
    match a.delete_crm_activity(&theirs).await {
        Err(StoreError::Forbidden) => {}
        other => panic!("expected Forbidden, got: {other:?}"),
    }
    assert_eq!(a.crm_activities(&deal_a).await.unwrap().len(), 2);

    // Each may remove their own.
    c.delete_crm_activity(&theirs).await.unwrap();
    a.delete_crm_activity(&mine).await.unwrap();
    assert!(a.crm_activities(&deal_a).await.unwrap().is_empty());
}

#[tokio::test]
async fn an_entry_records_something_and_a_log_is_bounded() {
    let store = common::test_store().await;
    let (a, _t, _u) = tenant_with_user(&store, "bounds").await;
    let deal_a = deal(&a, "A very chatty deal").await;

    // An empty note records nothing; an essay is not a note.
    for blank in ["", "   ", "\t\n"] {
        match a.add_crm_activity(&deal_a, &note(blank)).await {
            Err(StoreError::Validation(msg)) => assert!(msg.contains("must not be empty"), "{msg}"),
            other => panic!("accepted {blank:?}: {other:?}"),
        }
    }
    let essay = "x".repeat(ACTIVITY_BODY_MAX_CHARS + 1);
    match a.add_crm_activity(&deal_a, &note(&essay)).await {
        Err(StoreError::Validation(msg)) => assert!(msg.contains("at most"), "{msg}"),
        other => panic!("accepted an essay: {other:?}"),
    }
    // The bound itself is fine, and the body is stored trimmed.
    let ok = a
        .add_crm_activity(&deal_a, &note("  Ada rang back.  "))
        .await
        .unwrap();
    let log = a.crm_activities(&deal_a).await.unwrap();
    assert_eq!(log[0].id.as_str(), ok.as_str());
    assert_eq!(log[0].body, "Ada rang back.");
    assert_eq!(log[0].kind, ActivityKind::Note, "the default is a note");
}

#[tokio::test]
async fn a_deals_log_holds_a_bounded_number_of_entries() {
    let store = common::test_store().await;
    let (a, _t, _u) = tenant_with_user(&store, "cap").await;
    let deal_a = deal(&a, "The talkative one").await;

    for n in 0..DEAL_ACTIVITIES_MAX {
        a.add_crm_activity(&deal_a, &note(&format!("entry {n}")))
            .await
            .unwrap();
    }
    assert_eq!(
        a.crm_activities(&deal_a).await.unwrap().len(),
        usize::try_from(DEAL_ACTIVITIES_MAX).unwrap()
    );
    match a.add_crm_activity(&deal_a, &note("one too many")).await {
        Err(StoreError::Conflict(msg)) => assert!(msg.contains("at most"), "{msg}"),
        other => panic!("expected Conflict, got: {other:?}"),
    }
}

// ---- next steps ---------------------------------------------------------------

#[tokio::test]
async fn a_next_step_is_a_real_task_linked_back_to_its_deal() {
    let store = common::test_store().await;
    let (a, _ta, _ua) = tenant_with_user(&store, "step-a").await;
    let (b, _tb, _ub) = tenant_with_user(&store, "step-b").await;

    let deal_a = deal(&a, "Renewal — Acme GmbH").await;
    assert!(a.crm_deal_next_steps(&deal_a).await.unwrap().is_empty());

    let due = OffsetDateTime::now_utc() + Duration::days(7);
    let step = a
        .create_crm_deal_next_step(
            &deal_a,
            None,
            &NewTask {
                title: "Send the renewal quote".to_owned(),
                due_at: Some(due),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // It really is a task, in the tasks module, with the deal as its source —
    // so the same row is the one the user sees in their own morning list.
    let task = a.task(&step).await.unwrap().expect("the task exists");
    assert_eq!(task.title, "Send the renewal quote");
    assert_eq!(task.source_kind.as_deref(), Some(DEAL_SOURCE_KIND));
    assert_eq!(task.source_id.as_deref(), Some(deal_a.as_str()));
    assert_eq!(
        task.state, "active",
        "a person decided, so it is not a proposal"
    );
    assert_eq!(
        task.project_id.as_str(),
        a.ensure_personal_project().await.unwrap().as_str(),
        "it belongs to the person who will do it"
    );

    // …and the deal shows it, with the date it is due.
    let steps = a.crm_deal_next_steps(&deal_a).await.unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].id.as_str(), step.as_str());
    assert!(
        (steps[0].due_at.unwrap_or(OffsetDateTime::UNIX_EPOCH) - due).abs() < Duration::seconds(1)
    );

    // A caller cannot point a "next step" at a record it did not come from: the
    // source is written by the store, whatever the input said.
    let forged = a
        .create_crm_deal_next_step(
            &deal_a,
            None,
            &NewTask {
                title: "Forged".to_owned(),
                source_kind: Some("email".to_owned()),
                source_id: Some("msg_1".to_owned()),
                state: Some("proposed".to_owned()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let task = a.task(&forged).await.unwrap().expect("the task exists");
    assert_eq!(task.source_kind.as_deref(), Some(DEAL_SOURCE_KIND));
    assert_eq!(task.source_id.as_deref(), Some(deal_a.as_str()));
    assert_eq!(task.state, "active");

    // ---- the neighbour's door -------------------------------------------
    assert_not_found(b.crm_deal_next_steps(&deal_a).await);
    assert_not_found(
        b.create_crm_deal_next_step(
            &deal_a,
            None,
            &NewTask {
                title: "Theirs to do".to_owned(),
                ..Default::default()
            },
        )
        .await,
    );
    let invented = CrmDealId::new("crd_nope");
    assert_not_found(b.crm_deal_next_steps(&invented).await);
    // B's own board, holding B's own next step, is untouched by any of it.
    let deal_b = deal(&b, "Theirs").await;
    assert!(b.crm_deal_next_steps(&deal_b).await.unwrap().is_empty());
    assert_eq!(a.crm_deal_next_steps(&deal_a).await.unwrap().len(), 2);

    // A next step filed on a project the caller cannot see is refused — the
    // tasks module's own rule, not one CRM invents.
    let their_project = b.ensure_personal_project().await.unwrap();
    assert_not_found(
        a.create_crm_deal_next_step(
            &deal_a,
            Some(&their_project),
            &NewTask {
                title: "Into their list".to_owned(),
                ..Default::default()
            },
        )
        .await,
    );
}

#[tokio::test]
async fn a_next_step_is_only_as_visible_as_the_task_it_is() {
    let store = common::test_store().await;
    let (a, tenant, _ua) = tenant_with_user(&store, "vis").await;
    let (c, uc) = colleague(&store, &tenant, "vis-mate").await;

    let deal_a = deal(&a, "Renewal").await;
    // Mine, in my own personal project: my business, on a deal we both read.
    a.create_crm_deal_next_step(
        &deal_a,
        None,
        &NewTask {
            title: "Draft the quote".to_owned(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    // Theirs to do, still filed in my personal project — assigned, so they see
    // it wherever it lives.
    let assigned = a
        .create_crm_deal_next_step(
            &deal_a,
            None,
            &NewTask {
                title: "Chase the PO".to_owned(),
                assignee: Some(uc.as_str().to_owned()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    // Ours, on a team project: everyone's.
    let team = a.create_task_project("Sales team", None).await.unwrap();
    let shared = a
        .create_crm_deal_next_step(
            &deal_a,
            Some(&team),
            &NewTask {
                title: "Book the demo".to_owned(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // The deal is tenant-wide; a personal project is not, and reading the deal
    // does not widen it by one row.
    let mine = a.crm_deal_next_steps(&deal_a).await.unwrap();
    assert_eq!(mine.len(), 3);
    let theirs: Vec<String> = c
        .crm_deal_next_steps(&deal_a)
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.id.as_str().to_owned())
        .collect();
    assert_eq!(theirs.len(), 2, "the private one is not theirs to read");
    assert!(theirs.contains(&assigned.as_str().to_owned()));
    assert!(theirs.contains(&shared.as_str().to_owned()));
}
