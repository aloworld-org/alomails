//! Tenancy proof for the profitability report (B3.08, Law 1: isolation is
//! tested, not assumed), and the arithmetic it reports, proven against the real
//! Postgres rather than against the fold's unit tests alone.
//!
//! Three claims, in the order they matter:
//!
//! - **Wrong tenant.** Tenant A's engagement is never in tenant B's report, and
//!   asking for it by id is the clean `NotFound` — the same denial an id that
//!   never existed gets, so the report is not an existence oracle.
//! - **The report is a project aggregate.** A colleague's hours on a shared
//!   engagement are counted, and the answer names nobody: the type has no
//!   per-person field to ask with.
//! - **The figures are the ones the design promises.** The period bounds the
//!   work, the budget is consumed to date, unrated hours are counted and never
//!   priced, currencies are grouped and never converted, and the money is the
//!   cents a billing line would carry.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, BillingCustomerId, NewCustomer, NewProjectClient, NewTimeEntry, ProjectId, Store,
    StoreError, TenantId, UserId, profitability_totals,
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

/// A day in August 2026 — the period every test below reports on.
fn day(d: u8) -> Date {
    Date::from_calendar_date(2026, Month::August, d).expect("a real August day")
}

/// A day in the July before it — work that is to date, but not in the period.
fn july(d: u8) -> Date {
    Date::from_calendar_date(2026, Month::July, d).expect("a real July day")
}

/// A tenant with one user, returning the account door plus the tenant id.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("profit-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@profit.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

/// A second user of an existing tenant, on their own account door.
async fn second_user(store: &Store, tenant: &TenantId, tag: &str) -> (AccountStore, UserId) {
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@profit.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user.clone()), user)
}

/// A customer to bill the work to.
async fn customer(acc: &AccountStore, name: &str, currency: &str) -> BillingCustomerId {
    acc.create_billing_customer(&NewCustomer {
        name: name.to_owned(),
        country: "de".to_owned(),
        currency: currency.to_owned(),
        ..NewCustomer::default()
    })
    .await
    .unwrap()
}

/// An engagement: a team board with client facts — a rate, and both budgets.
async fn engagement(
    acc: &AccountStore,
    name: &str,
    customer_id: &BillingCustomerId,
    rate_cents: Option<i64>,
) -> ProjectId {
    let project = acc.create_task_project(name, None).await.unwrap();
    acc.set_project_client(
        &project,
        &NewProjectClient {
            rate_cents,
            // 100 hours, €10 000 — round numbers, so a wrong proportion is
            // visible by eye in a failure message.
            budget_minutes: Some(6_000),
            budget_cents: Some(1_000_000),
            ..NewProjectClient::for_customer(customer_id.clone())
        },
    )
    .await
    .unwrap();
    project
}

/// The report row for one engagement, or `None` when it is not in the answer.
async fn row(acc: &AccountStore, project: &ProjectId) -> Option<alo_store::ProjectProfitability> {
    acc.project_profitability(day(1), day(31), None)
        .await
        .unwrap()
        .projects
        .into_iter()
        .find(|p| &p.project_id == project)
}

/// Direct pool access. Marking an hour as carried onto a document is the
/// handoff's job (B3.06) and takes an approved week to get there; this suite is
/// about the *report*, so the state it needs is planted rather than re-earned —
/// the same shortcut `project_hours_tenancy` takes.
async fn pool() -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap()
}

#[tokio::test]
async fn another_tenants_engagement_is_never_in_the_answer() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "wrong-a").await;
    let (b, t2) = tenant_with_user(&store, "wrong-b").await;

    let their_customer = customer(&a, "Their client", "EUR").await;
    let theirs = engagement(&a, "Their engagement", &their_customer, Some(9_500)).await;
    a.log_time(&NewTimeEntry::worked(theirs.clone(), day(3), 480))
        .await
        .unwrap();

    let our_customer = customer(&b, "Our client", "EUR").await;
    let ours = engagement(&b, "Our engagement", &our_customer, Some(8_000)).await;
    b.log_time(&NewTimeEntry::worked(ours.clone(), day(3), 60))
        .await
        .unwrap();

    // In the list: the neighbour's engagement simply is not there, and eight of
    // their hours move none of our figures.
    let mine = b
        .project_profitability(day(1), day(31), None)
        .await
        .unwrap();
    assert_eq!(mine.projects.len(), 1);
    assert_eq!(mine.projects[0].project_id, ours);
    assert_eq!(mine.projects[0].minutes, 60);

    // By id: a clean denial, never data and never a 500 — and identical to the
    // answer an id that never existed anywhere gets.
    assert_not_found(
        b.project_profitability(day(1), day(31), Some(&theirs))
            .await,
    );
    assert_not_found(a.project_profitability(day(1), day(31), Some(&ours)).await);
    assert_not_found(
        b.project_profitability(day(1), day(31), Some(&ProjectId::new("no-such-project")))
            .await,
    );

    store.delete_tenant(&t1).await.unwrap();
    store.delete_tenant(&t2).await.unwrap();
}

#[tokio::test]
async fn internal_work_is_not_an_engagement_and_has_no_profitability() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "internal").await;

    // A team board nobody has made client work: no customer, no rate, no
    // budget — every column of this report would be empty, so it is not a row.
    let internal = a
        .create_task_project("Internal tooling", None)
        .await
        .unwrap();
    a.log_time(&NewTimeEntry::worked(internal.clone(), day(4), 120))
        .await
        .unwrap();
    // …and neither is somebody's private board, which the report reaches
    // through the same visibility predicate the engagement list does.
    let personal = a.ensure_personal_project().await.unwrap();
    a.log_time(&NewTimeEntry::worked(personal.clone(), day(4), 30))
        .await
        .unwrap();

    let report = a
        .project_profitability(day(1), day(31), None)
        .await
        .unwrap();
    assert!(report.projects.is_empty(), "{:?}", report.projects);
    // Asking for one by id is the same denial a neighbour's id gets: the report
    // never explains which of the two reasons applies.
    assert_not_found(
        a.project_profitability(day(1), day(31), Some(&internal))
            .await,
    );

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn the_period_bounds_the_work_and_the_budget_is_consumed_to_date() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "period").await;
    let client = customer(&a, "Sunrise", "EUR").await;
    let project = engagement(&a, "Sunrise portal", &client, Some(9_500)).await;

    // Ten hours in July, two in August, and one after the period ends.
    a.log_time(&NewTimeEntry::worked(project.clone(), july(6), 600))
        .await
        .unwrap();
    a.log_time(&NewTimeEntry::worked(project.clone(), day(5), 90))
        .await
        .unwrap();
    a.log_time(&NewTimeEntry::worked(project.clone(), day(12), 30))
        .await
        .unwrap();
    a.log_time(&NewTimeEntry::worked(
        project.clone(),
        Date::from_calendar_date(2026, Month::September, 2).unwrap(),
        480,
    ))
    .await
    .unwrap();

    let august = row(&a, &project).await.unwrap();
    assert_eq!(august.minutes, 120, "August only");
    assert_eq!(august.billable_minutes, 120);
    assert_eq!(august.unrated_minutes, 0);
    assert_eq!(august.by_currency.len(), 1);
    assert_eq!(august.by_currency[0].currency, "EUR");
    // Two hours at €95.00 — the cents a billing line would carry.
    assert_eq!(august.by_currency[0].net_cents, 19_000);
    assert_eq!(august.by_currency[0].billed_minutes, 0);

    // To date is everything up to and including the last day of the period —
    // July's ten hours included, September's eight excluded, so a closed
    // quarter re-read next year answers the same figures.
    assert_eq!(august.to_date_minutes, 720);
    assert_eq!(august.to_date_net_cents, 114_000);
    assert_eq!(august.hours_consumption_bp(), Some(1_200));
    assert_eq!(august.budget_consumption_bp(), Some(1_140));
    assert_eq!(august.budget_remaining_cents(), Some(886_000));

    // A period that ends before it starts is refused rather than answered with
    // nothing, which would read as an engagement nobody worked.
    match a.project_profitability(day(31), day(1), None).await {
        Err(StoreError::Validation(msg)) => assert!(msg.contains("ends before"), "{msg}"),
        other => panic!("expected a refusal, got {other:?}"),
    }

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn an_engagement_nobody_worked_is_a_row_of_zeroes_and_a_suggestion_is_not_an_hour() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "quiet").await;
    let client = customer(&a, "Quiet", "EUR").await;
    let project = engagement(&a, "Quiet engagement", &client, Some(9_500)).await;

    let untouched = row(&a, &project).await.unwrap();
    assert_eq!(untouched.minutes, 0);
    assert_eq!(untouched.to_date_minutes, 0);
    assert!(
        untouched.by_currency.is_empty(),
        "no money at all, not zero"
    );
    assert_eq!(untouched.budget_minutes, Some(6_000));
    assert_eq!(untouched.budget_consumption_bp(), Some(0));
    assert_eq!(untouched.customer_id, client);

    // An agent's suggestion is not work until a human accepts it (ADR 0023): a
    // budget bar that filled up with proposals would report on hours nobody
    // worked.
    let proposal = a
        .log_time(&NewTimeEntry {
            proposed: true,
            ..NewTimeEntry::worked(project.clone(), day(6), 300)
        })
        .await
        .unwrap();
    assert_eq!(row(&a, &project).await.unwrap().minutes, 0);

    a.accept_time_entry(&proposal.id).await.unwrap();
    let accepted = row(&a, &project).await.unwrap();
    assert_eq!(accepted.minutes, 300);
    assert_eq!(accepted.by_currency[0].net_cents, 47_500);

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn unrated_hours_are_counted_never_priced_and_billed_hours_are_named_apart() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "unrated").await;
    let client = customer(&a, "Unpriced", "EUR").await;

    // An engagement nobody has priced: legal and normal, and its hours are
    // countable but not billable.
    let project = engagement(&a, "Discovery", &client, None).await;
    a.log_time(&NewTimeEntry::worked(project.clone(), day(4), 60))
        .await
        .unwrap();
    a.log_time(&NewTimeEntry {
        billable: false,
        ..NewTimeEntry::worked(project.clone(), day(5), 45)
    })
    .await
    .unwrap();

    let unpriced = row(&a, &project).await.unwrap();
    assert_eq!(unpriced.minutes, 105);
    assert_eq!(unpriced.billable_minutes, 60);
    assert_eq!(unpriced.unrated_minutes, 60, "the gap somebody must close");
    assert!(unpriced.by_currency.is_empty(), "never valued at zero");
    assert_eq!(unpriced.to_date_net_cents, 0);
    assert_eq!(
        unpriced.budget_consumption_bp(),
        Some(0),
        "an unpriced engagement consumes no money budget"
    );

    // Pricing it does not restate the hours already logged: a rate is
    // snapshotted onto an entry when it is written.
    a.set_project_client(
        &project,
        &NewProjectClient {
            rate_cents: Some(9_500),
            budget_minutes: Some(6_000),
            budget_cents: Some(1_000_000),
            ..NewProjectClient::for_customer(client.clone())
        },
    )
    .await
    .unwrap();
    let later = a
        .log_time(&NewTimeEntry::worked(project.clone(), day(10), 120))
        .await
        .unwrap();
    let priced = row(&a, &project).await.unwrap();
    assert_eq!(priced.unrated_minutes, 60, "the old hour keeps its silence");
    assert_eq!(priced.by_currency[0].billable_minutes, 120);
    assert_eq!(priced.by_currency[0].net_cents, 19_000);
    assert_eq!(priced.by_currency[0].unbilled_net_cents(), 19_000);

    // What is already on a document is named beside the value, never inside it.
    let planted = pool().await;
    sqlx::query(
        "UPDATE time_entries SET invoice_id = 'inv-planted', billed_at = now() \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(t1.as_str())
    .bind(later.id.as_str())
    .execute(&planted)
    .await
    .unwrap();
    let billed = row(&a, &project).await.unwrap();
    assert_eq!(
        billed.by_currency[0].net_cents, 19_000,
        "billing earns nothing new"
    );
    assert_eq!(billed.by_currency[0].billed_minutes, 120);
    assert_eq!(billed.by_currency[0].billed_net_cents, 19_000);
    assert_eq!(billed.by_currency[0].unbilled_net_cents(), 0);

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn two_currencies_are_two_rows_and_the_budget_only_knows_its_own() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "currency").await;
    let client = customer(&a, "Transatlantic", "EUR").await;
    let project = engagement(&a, "Transatlantic rollout", &client, Some(9_500)).await;

    a.log_time(&NewTimeEntry::worked(project.clone(), day(3), 60))
        .await
        .unwrap();
    // An hour priced in dollars: stated on the entry, snapshotted with its rate.
    a.log_time(&NewTimeEntry {
        rate_cents: Some(10_000),
        currency: Some("USD".to_owned()),
        ..NewTimeEntry::worked(project.clone(), day(4), 120)
    })
    .await
    .unwrap();

    let mixed = row(&a, &project).await.unwrap();
    assert_eq!(
        mixed
            .by_currency
            .iter()
            .map(|m| m.currency.as_str())
            .collect::<Vec<_>>(),
        ["EUR", "USD"]
    );
    assert_eq!(mixed.by_currency[0].net_cents, 9_500);
    assert_eq!(mixed.by_currency[1].net_cents, 20_000);
    // The money budget is stated in the engagement's currency, so the dollars
    // are reported and never converted into it.
    assert_eq!(mixed.currency, "EUR");
    assert_eq!(mixed.to_date_net_cents, 9_500);
    assert_eq!(mixed.budget_consumption_bp(), Some(95));

    // The report's totals keep the same rule across engagements.
    let totals = profitability_totals(
        &a.project_profitability(day(1), day(31), None)
            .await
            .unwrap()
            .projects,
    );
    assert_eq!(totals.minutes, 180);
    assert_eq!(totals.by_currency.len(), 2);
    assert_eq!(totals.by_currency[0].net_cents, 9_500);
    assert_eq!(totals.by_currency[1].net_cents, 20_000);

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn a_colleagues_hours_are_counted_and_the_answer_names_nobody() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "shared").await;
    let (b, _) = second_user(&store, &t1, "shared-colleague").await;
    let client = customer(&a, "Shared", "EUR").await;
    let project = engagement(&a, "Shared engagement", &client, Some(9_500)).await;

    a.log_time(&NewTimeEntry::worked(project.clone(), day(3), 60))
        .await
        .unwrap();
    b.log_time(&NewTimeEntry::worked(project.clone(), day(4), 120))
        .await
        .unwrap();

    // One engagement, two people, one row — and the colleague reads exactly the
    // same figures, which is what makes a project aggregate shareable at all.
    let mine = row(&a, &project).await.unwrap();
    assert_eq!(mine.minutes, 180);
    assert_eq!(mine.by_currency[0].net_cents, 28_500);
    assert_eq!(row(&b, &project).await.unwrap(), mine);

    // There is nowhere in the answer to ask who worked when: the type carries
    // no per-person field, which is the guarantee rather than a filter somebody
    // has to remember. What can still be asked is one's own hours, through the
    // door that carries a user.
    assert_eq!(
        b.time_entries(day(1), day(31), Some(&project))
            .await
            .unwrap()
            .len(),
        1,
        "the personal door still shows only the reader's own entry"
    );

    store.delete_tenant(&t1).await.unwrap();
}
