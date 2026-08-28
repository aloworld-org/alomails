//! Tenancy proof for alo Projects client facts (Law 1: isolation is tested,
//! not assumed). An engagement is tenant-wide — a co-tenant reads the same
//! client facts — but an outsider tenant gets the clean `NotFound`/empty on
//! **every** path: read, list, list-by-customer, set and clear. Also covers
//! the arc the queue item requires (attach, read, replace, detach), the rules
//! that decide which project may be client work at all, the two different
//! denials a personal board gets depending on whose it is, the bounds, and
//! that deleting the project — or the tenant — takes the client facts with it.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, BillingCustomerId, NewCustomer, NewProjectClient, ProjectId, Store, StoreError,
    TenantId, UserId,
};
use time::{Date, Month};

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error.
fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

/// Asserts a result is a validation refusal naming the rule.
fn assert_invalid<T: std::fmt::Debug>(result: Result<T, StoreError>, rule: &str) {
    match result {
        Err(StoreError::Validation(msg)) => {
            assert!(msg.contains(rule), "expected {rule:?} in {msg:?}");
        }
        other => panic!("expected Validation({rule:?}), got: {other:?}"),
    }
}

/// A customer to bill the work to.
fn customer(name: &str, currency: &str) -> NewCustomer {
    NewCustomer {
        name: name.to_owned(),
        country: "de".to_owned(),
        currency: currency.to_owned(),
        ..NewCustomer::default()
    }
}

/// A tenant with one user, returning the account door plus the tenant id.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("proj-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@projects.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

/// A second user of an existing tenant, on their own account door.
async fn second_user(store: &Store, tenant: &TenantId, tag: &str) -> (AccountStore, UserId) {
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@projects.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user.clone()), user)
}

/// The full set of client facts, so the round trip covers every column.
fn full_facts(customer_id: &BillingCustomerId) -> NewProjectClient {
    NewProjectClient {
        currency: Some("chf".to_owned()),
        rate_cents: Some(9_500),
        budget_minutes: Some(48_000),
        budget_cents: Some(7_600_000),
        starts_on: Date::from_calendar_date(2026, Month::September, 1).ok(),
        ..NewProjectClient::for_customer(customer_id.clone())
    }
}

/// Direct pool access, for the assertions that must read rows rather than the
/// tenant-predicated API (a cascade is a claim about the table, not the view).
async fn pool() -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap()
}

#[tokio::test]
async fn a_project_becomes_client_work_and_back_again() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "arc").await;

    let acme = a
        .create_billing_customer(&customer("Acme GmbH", "eur"))
        .await
        .unwrap();
    let project = a.create_task_project("Portal rebuild", None).await.unwrap();

    // Before anything is attached the project is internal work, which is said
    // by absence and not by a sentinel.
    assert!(a.project_client(&project).await.unwrap().is_none());
    assert!(a.project_clients().await.unwrap().is_empty());

    // ---- attach ----------------------------------------------------------
    let facts = a
        .set_project_client(&project, &full_facts(&acme))
        .await
        .unwrap();
    assert_eq!(facts.project_id, project);
    assert_eq!(facts.customer_id, acme);
    assert_eq!(facts.currency, "CHF", "a stated currency wins, uppercased");
    assert_eq!(facts.rate_cents, Some(9_500));
    assert_eq!(facts.budget_minutes, Some(48_000));
    assert_eq!(facts.budget_cents, Some(7_600_000));
    assert_eq!(
        facts.starts_on,
        Date::from_calendar_date(2026, Month::September, 1).ok()
    );
    assert!(facts.is_priced());

    // Every column survives the round trip through the database.
    let read = a.project_client(&project).await.unwrap().unwrap();
    assert_eq!(read.currency, facts.currency);
    assert_eq!(read.rate_cents, facts.rate_cents);
    assert_eq!(read.budget_minutes, facts.budget_minutes);
    assert_eq!(read.budget_cents, facts.budget_cents);
    assert_eq!(read.starts_on, facts.starts_on);
    let listed = a.project_clients().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].project_id, project);
    assert_eq!(
        a.project_clients_for_customer(&acme).await.unwrap().len(),
        1
    );

    // ---- replace ---------------------------------------------------------
    // Idempotent by design: the same call with new facts replaces the record
    // whole, and unstated fields go back to absent rather than lingering.
    let globex = a
        .create_billing_customer(&customer("Globex SA", "usd"))
        .await
        .unwrap();
    let replaced = a
        .set_project_client(&project, &NewProjectClient::for_customer(globex.clone()))
        .await
        .unwrap();
    assert_eq!(replaced.customer_id, globex);
    assert_eq!(
        replaced.currency, "USD",
        "an unstated currency is the customer's own"
    );
    assert_eq!(replaced.rate_cents, None, "an unstated rate is cleared");
    assert_eq!(replaced.budget_minutes, None);
    assert_eq!(replaced.budget_cents, None);
    assert_eq!(replaced.starts_on, None);
    assert!(!replaced.is_priced(), "an unpriced engagement is legal");
    assert_eq!(
        replaced.created_at, facts.created_at,
        "when this became client work is still answerable after a replace"
    );
    assert!(replaced.updated_at >= facts.updated_at);
    assert_eq!(
        a.project_clients().await.unwrap().len(),
        1,
        "a replace is one engagement, never two"
    );
    assert!(
        a.project_clients_for_customer(&acme)
            .await
            .unwrap()
            .is_empty(),
        "the old customer no longer holds the engagement"
    );

    // ---- detach ----------------------------------------------------------
    a.clear_project_client(&project).await.unwrap();
    assert!(a.project_client(&project).await.unwrap().is_none());
    assert!(a.project_clients().await.unwrap().is_empty());
    // Detaching twice is a clean denial, not a silent success.
    assert_not_found(a.clear_project_client(&project).await);
    // The board itself is untouched: what was deleted is the claim that its
    // hours are billable to somebody, never the project.
    assert!(
        a.task_projects()
            .await
            .unwrap()
            .iter()
            .any(|p| p.id == project)
    );

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn project_creation_and_customer_link_are_atomic() {
    let store = common::test_store().await;
    let (a, tenant) = tenant_with_user(&store, "atomic-create").await;
    let acme = a
        .create_billing_customer(&customer("Acme GmbH", "eur"))
        .await
        .unwrap();

    let project = a
        .create_project(
            "Portal rebuild",
            None,
            Some(&NewProjectClient::for_customer(acme.clone())),
        )
        .await
        .unwrap();
    let linked = a.project_client(&project).await.unwrap().unwrap();
    assert_eq!(linked.customer_id, acme);
    assert_eq!(linked.currency, "EUR");

    let before = a.task_projects().await.unwrap().len();
    assert_not_found(
        a.create_project(
            "Must not survive",
            None,
            Some(&NewProjectClient::for_customer(BillingCustomerId::new(
                "missing-customer",
            ))),
        )
        .await,
    );
    let projects = a.task_projects().await.unwrap();
    assert_eq!(projects.len(), before);
    assert!(projects.iter().all(|item| item.name != "Must not survive"));

    store.delete_tenant(&tenant).await.unwrap();
}

#[tokio::test]
async fn another_tenant_can_never_read_or_write_our_engagement() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "iso-a").await;
    let (b, t2) = tenant_with_user(&store, "iso-b").await;

    let ours = a
        .create_billing_customer(&customer("Acme GmbH", "eur"))
        .await
        .unwrap();
    let our_project = a.create_task_project("Portal rebuild", None).await.unwrap();
    a.set_project_client(&our_project, &full_facts(&ours))
        .await
        .unwrap();

    // B holds their own customer and their own board, so the only thing they
    // are missing is the right to reach ours.
    let theirs = b
        .create_billing_customer(&customer("Initech BV", "eur"))
        .await
        .unwrap();
    let their_project = b.create_task_project("Their work", None).await.unwrap();

    // ---- B reaching A's engagement --------------------------------------
    assert!(
        b.project_client(&our_project).await.unwrap().is_none(),
        "our engagement is invisible, not merely unreadable"
    );
    assert!(b.project_clients().await.unwrap().is_empty());
    assert!(
        b.project_clients_for_customer(&ours)
            .await
            .unwrap()
            .is_empty()
    );
    assert_not_found(b.clear_project_client(&our_project).await);
    assert_not_found(
        b.set_project_client(
            &our_project,
            &NewProjectClient::for_customer(theirs.clone()),
        )
        .await,
    );
    assert_not_found(
        b.set_project_client(
            &their_project,
            &NewProjectClient::for_customer(ours.clone()),
        )
        .await,
        // Our customer on their own board: the link is refused, not made.
    );

    // ---- A reaching B's ---------------------------------------------------
    assert_not_found(
        a.set_project_client(
            &their_project,
            &NewProjectClient::for_customer(ours.clone()),
        )
        .await,
    );
    assert_not_found(
        a.set_project_client(&our_project, &NewProjectClient::for_customer(theirs))
            .await,
    );
    // An id that never existed answers exactly like a foreign one.
    assert_not_found(
        a.set_project_client(
            &ProjectId::generate(),
            &NewProjectClient::for_customer(ours.clone()),
        )
        .await,
    );
    assert_not_found(
        a.set_project_client(
            &our_project,
            &NewProjectClient::for_customer(BillingCustomerId::generate()),
        )
        .await,
    );

    // Nothing above moved our facts.
    let intact = a.project_client(&our_project).await.unwrap().unwrap();
    assert_eq!(intact.customer_id, ours);
    assert_eq!(intact.rate_cents, Some(9_500));
    assert!(b.project_clients().await.unwrap().is_empty());

    store.delete_tenant(&t1).await.unwrap();
    store.delete_tenant(&t2).await.unwrap();
}

#[tokio::test]
async fn client_facts_belong_to_a_team_board_only() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "kind-a").await;
    let (colleague, _) = second_user(&store, &t1, "kind-b").await;

    let acme = a
        .create_billing_customer(&customer("Acme GmbH", "eur"))
        .await
        .unwrap();
    let team = a.create_task_project("Portal rebuild", None).await.unwrap();
    let mine = a.ensure_personal_project().await.unwrap();
    let theirs = colleague.ensure_personal_project().await.unwrap();

    // A team board is the only board an engagement can live on: hours somebody
    // else approves and a customer is billed for are not private work.
    a.set_project_client(&team, &NewProjectClient::for_customer(acme.clone()))
        .await
        .unwrap();

    // My own personal board: I can already see it, so the honest answer is the
    // rule I broke.
    assert_invalid(
        a.set_project_client(&mine, &NewProjectClient::for_customer(acme.clone()))
            .await,
        "team project",
    );
    // A colleague's personal board: I may not see it at all, so it reads as
    // absent — naming the rule would confirm a row I have no right to know of.
    assert_not_found(
        a.set_project_client(&theirs, &NewProjectClient::for_customer(acme.clone()))
            .await,
    );

    // The engagement is tenant-wide: a co-tenant reads the same facts, because
    // everyone bills the same customers and works the same engagements.
    assert_eq!(
        colleague
            .project_client(&team)
            .await
            .unwrap()
            .unwrap()
            .customer_id,
        acme
    );
    assert_eq!(colleague.project_clients().await.unwrap().len(), 1);
    // …and a personal board never appears in the engagement list, whoever asks.
    assert!(
        a.project_clients()
            .await
            .unwrap()
            .iter()
            .all(|c| c.project_id != mine && c.project_id != theirs)
    );

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn an_archived_project_or_customer_cannot_take_on_new_work() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "arch").await;

    let acme = a
        .create_billing_customer(&customer("Acme GmbH", "eur"))
        .await
        .unwrap();
    let live = a.create_task_project("Portal rebuild", None).await.unwrap();
    let shelved = a.create_task_project("Old work", None).await.unwrap();

    // Archiving a project is not yet a store call (tasks.rs has no archive
    // function to reuse), so the fixture writes the column the reader checks.
    let pool = pool().await;
    sqlx::query("UPDATE task_projects SET archived = true WHERE tenant_id = $1 AND id = $2")
        .bind(t1.as_str())
        .bind(shelved.as_str())
        .execute(&pool)
        .await
        .unwrap();
    assert_invalid(
        a.set_project_client(&shelved, &NewProjectClient::for_customer(acme.clone()))
            .await,
        "archived",
    );

    // An archived customer is refused with the same care billing takes: an
    // engagement raised for a customer nobody bills any more is a document
    // waiting to be wrong.
    a.set_billing_customer_archived(&acme, true).await.unwrap();
    assert_invalid(
        a.set_project_client(&live, &NewProjectClient::for_customer(acme.clone()))
            .await,
        "archived",
    );
    a.set_billing_customer_archived(&acme, false).await.unwrap();
    a.set_project_client(&live, &NewProjectClient::for_customer(acme.clone()))
        .await
        .unwrap();

    // Archiving the customer afterwards does not retract an engagement that is
    // already running — the same rule an issued invoice lives by.
    a.set_billing_customer_archived(&acme, true).await.unwrap();
    assert_eq!(
        a.project_client(&live).await.unwrap().unwrap().customer_id,
        acme
    );

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn a_rate_or_budget_outside_its_bound_is_refused_before_the_column_sees_it() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "bounds").await;

    let acme = a
        .create_billing_customer(&customer("Acme GmbH", "eur"))
        .await
        .unwrap();
    let project = a.create_task_project("Portal rebuild", None).await.unwrap();

    let cases: [(NewProjectClient, &str); 6] = [
        (
            NewProjectClient {
                rate_cents: Some(-1),
                ..NewProjectClient::for_customer(acme.clone())
            },
            "hourly rate",
        ),
        (
            NewProjectClient {
                rate_cents: Some(alo_store::billing_field::UNIT_PRICE_MAX_CENTS + 1),
                ..NewProjectClient::for_customer(acme.clone())
            },
            "hourly rate",
        ),
        (
            NewProjectClient {
                budget_minutes: Some(-1),
                ..NewProjectClient::for_customer(acme.clone())
            },
            "budget hours",
        ),
        (
            NewProjectClient {
                budget_minutes: Some(alo_store::BUDGET_MINUTES_MAX + 1),
                ..NewProjectClient::for_customer(acme.clone())
            },
            "budget hours",
        ),
        (
            NewProjectClient {
                budget_cents: Some(alo_store::BUDGET_CENTS_MAX + 1),
                ..NewProjectClient::for_customer(acme.clone())
            },
            "budget amount",
        ),
        (
            NewProjectClient {
                currency: Some("EURO".to_owned()),
                ..NewProjectClient::for_customer(acme.clone())
            },
            "ISO 4217",
        ),
    ];
    for (input, rule) in cases {
        assert_invalid(a.set_project_client(&project, &input).await, rule);
    }
    // Not one of them wrote a row.
    assert!(a.project_client(&project).await.unwrap().is_none());

    // The ceilings themselves are inclusive — a bound that rejects its own
    // limit costs a real engagement.
    let at_the_edge = NewProjectClient {
        rate_cents: Some(alo_store::billing_field::UNIT_PRICE_MAX_CENTS),
        budget_minutes: Some(alo_store::BUDGET_MINUTES_MAX),
        budget_cents: Some(alo_store::BUDGET_CENTS_MAX),
        ..NewProjectClient::for_customer(acme)
    };
    let written = a.set_project_client(&project, &at_the_edge).await.unwrap();
    assert_eq!(written.budget_minutes, Some(alo_store::BUDGET_MINUTES_MAX));
    assert_eq!(written.budget_cents, Some(alo_store::BUDGET_CENTS_MAX));

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn deleting_the_project_or_the_tenant_takes_the_client_facts_with_it() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "purge").await;

    let acme = a
        .create_billing_customer(&customer("Acme GmbH", "eur"))
        .await
        .unwrap();
    let doomed = a.create_task_project("Short job", None).await.unwrap();
    let kept = a.create_task_project("Long job", None).await.unwrap();
    for project in [&doomed, &kept] {
        a.set_project_client(project, &NewProjectClient::for_customer(acme.clone()))
            .await
            .unwrap();
    }

    // Facts about a project that no longer exists are not facts about
    // anything: the board owns the engagement.
    let pool = pool().await;
    sqlx::query("DELETE FROM task_projects WHERE tenant_id = $1 AND id = $2")
        .bind(t1.as_str())
        .bind(doomed.as_str())
        .execute(&pool)
        .await
        .unwrap();
    assert!(a.project_client(&doomed).await.unwrap().is_none());
    assert_eq!(a.project_clients().await.unwrap().len(), 1);

    // Read the rows directly: the claim is that a tenant deletion cascaded
    // them away, not that they are hidden behind the tenant predicate.
    store.delete_tenant(&t1).await.unwrap();
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM project_clients WHERE tenant_id = $1")
            .bind(t1.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 0, "the tenant's engagements are purged with it");
}
