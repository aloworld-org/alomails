//! The running timer, on the wire of the real database (alo Projects, B3.04).
//!
//! Three claims the pure unit tests in `time_timer.rs` cannot make, because all
//! three are claims about the *table*:
//!
//! - **One clock per person, enforced by the primary key.** A second start is a
//!   `Conflict`, never an implicit stop and never a second row — and two
//!   simultaneous starts settle to exactly one running timer.
//! - **A stop is one transaction.** It clears the row and writes the hour
//!   together; two simultaneous stops produce exactly one entry and one
//!   `NotFound`.
//! - **A person's clock is their own.** Wrong tenant and wrong user both read as
//!   absence — the two denials `time_entries_tenancy.rs` proves for hours,
//!   proved again for the clock, because the door is what makes them true and
//!   each table has its own statements.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, NewCustomer, NewProjectClient, NewTask, ProjectId, StartTimer, Store, StoreError,
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

/// Asserts a result is a conflict naming the rule it disagrees with.
fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>, rule: &str) {
    match result {
        Err(StoreError::Conflict(msg)) => {
            assert!(msg.contains(rule), "expected {rule:?} in {msg:?}");
        }
        other => panic!("expected Conflict({rule:?}), got: {other:?}"),
    }
}

/// A day in the middle of a working week.
fn day(d: u8) -> Date {
    Date::from_calendar_date(2026, Month::August, d).expect("a real August day")
}

/// A tenant with one user, returning the account door plus the tenant id.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("timer-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@timer.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

/// A second user of an existing tenant, on their own account door.
async fn second_user(store: &Store, tenant: &TenantId, tag: &str) -> (AccountStore, UserId) {
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@timer.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user.clone()), user)
}

/// A team board that is client work at `rate_cents` an hour.
async fn engagement(account: &AccountStore, name: &str, rate_cents: Option<i64>) -> ProjectId {
    let customer = account
        .create_billing_customer(&NewCustomer {
            name: format!("{name} customer"),
            country: "de".to_owned(),
            currency: "eur".to_owned(),
            ..NewCustomer::default()
        })
        .await
        .unwrap();
    let project = account.create_task_project(name, None).await.unwrap();
    account
        .set_project_client(
            &project,
            &NewProjectClient {
                rate_cents,
                ..NewProjectClient::for_customer(customer)
            },
        )
        .await
        .unwrap();
    project
}

/// Direct pool access, for the assertions that must count rows rather than go
/// through the door's own predicate.
async fn pool() -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&common::database_url())
        .await
        .unwrap()
}

/// How many timers this tenant holds, counted around the store's own reads.
async fn timer_rows(pool: &sqlx::PgPool, tenant: &TenantId) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM time_timers WHERE tenant_id = $1")
        .bind(tenant.as_str())
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_clock_starts_is_visible_and_stops_into_an_hour() {
    let store = common::test_store().await;
    let (a, _t) = tenant_with_user(&store, "arc").await;
    let project = engagement(&a, "Portal rebuild", Some(9_500)).await;
    let task = a
        .create_task(
            &project,
            &NewTask {
                title: "Wire the login".to_owned(),
                ..NewTask::default()
            },
        )
        .await
        .unwrap();

    assert!(
        a.running_timer().await.unwrap().is_none(),
        "nothing runs until somebody starts it"
    );

    let started = a
        .start_timer(&StartTimer {
            task_id: Some(task.clone()),
            note: "  Login screen  ".to_owned(),
            ..StartTimer::on(project.clone())
        })
        .await
        .unwrap();
    assert_eq!(started.project_id, project);
    assert_eq!(started.task_id, Some(task.clone()));
    assert!(started.billable);
    assert_eq!(started.note, "Login screen", "trimmed on the way in");

    let running = a.running_timer().await.unwrap().unwrap();
    assert_eq!(running.started_at, started.started_at);
    assert_eq!(running.note, "Login screen");

    // ---- stop --------------------------------------------------------------
    let stopped = a.stop_timer(Some(day(3))).await.unwrap();
    assert_eq!(stopped.entry.project_id, project);
    assert_eq!(stopped.entry.task_id, Some(task));
    assert_eq!(stopped.entry.work_date, day(3), "the day the caller stated");
    assert_eq!(
        stopped.entry.minutes, 1,
        "a stint under a minute is one minute, never zero"
    );
    assert!(!stopped.capped);
    assert_eq!(stopped.elapsed_minutes, 1);
    assert_eq!(stopped.entry.note, "Login screen", "carried from the start");
    assert!(stopped.entry.billable);
    assert_eq!(
        stopped.entry.rate_cents,
        Some(9_500),
        "priced at the stop, from the engagement, exactly as a manual hour is"
    );
    assert_eq!(stopped.entry.currency.as_deref(), Some("EUR"));
    assert_eq!(
        stopped.entry.started_at,
        Some(started.started_at),
        "the clock's start is kept as provenance"
    );
    assert!(!stopped.entry.is_proposed());

    assert!(
        a.running_timer().await.unwrap().is_none(),
        "the stop cleared the clock"
    );
    let week = a.time_entries(day(3), day(9), None).await.unwrap();
    assert_eq!(week.len(), 1, "one stop, one hour");
    assert_eq!(week[0].id, stopped.entry.id);
}

#[tokio::test]
async fn an_unstated_day_falls_back_to_the_day_the_clock_started() {
    let store = common::test_store().await;
    let (a, _t) = tenant_with_user(&store, "fallback").await;
    let project = engagement(&a, "Data migration", None).await;
    let started = a.start_timer(&StartTimer::on(project)).await.unwrap();

    let stopped = a.stop_timer(None).await.unwrap();
    assert_eq!(
        stopped.entry.work_date,
        started.started_at.date(),
        "no day stated: the day the clock started, never the server's idea of now"
    );
    assert_eq!(
        stopped.entry.rate_cents, None,
        "an engagement with no rate leaves the hour unrated rather than priced at zero"
    );
    assert_eq!(stopped.entry.currency, None);
}

#[tokio::test]
async fn a_second_start_is_refused_and_never_a_second_row() {
    let store = common::test_store().await;
    let (a, tenant) = tenant_with_user(&store, "one").await;
    let pool = pool().await;
    let first = engagement(&a, "First board", Some(1_000)).await;
    let second = engagement(&a, "Second board", Some(2_000)).await;

    let started = a.start_timer(&StartTimer::on(first.clone())).await.unwrap();
    assert_conflict(
        a.start_timer(&StartTimer::on(second)).await,
        "a timer is already running",
    );
    assert_eq!(timer_rows(&pool, &tenant).await, 1);
    let running = a.running_timer().await.unwrap().unwrap();
    assert_eq!(
        running.project_id, first,
        "the refusal changed nothing — no implicit stop, no switched board"
    );
    assert_eq!(running.started_at, started.started_at);

    // The refusal wrote no hour either: a start that lost is not work.
    assert!(
        a.time_entries(day(1), day(28), None)
            .await
            .unwrap()
            .is_empty(),
        "a refused start writes nothing"
    );
}

#[tokio::test]
async fn simultaneous_starts_settle_to_exactly_one_clock() {
    let store = common::test_store().await;
    let (a, tenant) = tenant_with_user(&store, "race-start").await;
    let pool = pool().await;
    let project = engagement(&a, "Contended board", Some(5_000)).await;

    let attempts = (0..8).map(|_| {
        let door = a.clone();
        let board = project.clone();
        tokio::spawn(async move { door.start_timer(&StartTimer::on(board)).await })
    });
    let mut won = 0;
    let mut refused = 0;
    for attempt in attempts {
        match attempt.await.unwrap() {
            Ok(_) => won += 1,
            Err(StoreError::Conflict(_)) => refused += 1,
            Err(other) => panic!("a concurrent start failed with {other:?}"),
        }
    }
    assert_eq!(won, 1, "the primary key admits exactly one clock");
    assert_eq!(refused, 7);
    assert_eq!(timer_rows(&pool, &tenant).await, 1);
}

#[tokio::test]
async fn simultaneous_stops_write_exactly_one_hour() {
    let store = common::test_store().await;
    let (a, tenant) = tenant_with_user(&store, "race-stop").await;
    let pool = pool().await;
    let project = engagement(&a, "Contended stop", Some(5_000)).await;
    a.start_timer(&StartTimer::on(project)).await.unwrap();

    let attempts = (0..8).map(|_| {
        let door = a.clone();
        tokio::spawn(async move { door.stop_timer(Some(day(4))).await })
    });
    let mut wrote = 0;
    let mut absent = 0;
    for attempt in attempts {
        match attempt.await.unwrap() {
            Ok(_) => wrote += 1,
            Err(StoreError::NotFound) => absent += 1,
            Err(other) => panic!("a concurrent stop failed with {other:?}"),
        }
    }
    assert_eq!(wrote, 1, "the delete is the claim: one stop wins");
    assert_eq!(absent, 7);
    assert_eq!(timer_rows(&pool, &tenant).await, 0);
    assert_eq!(
        a.time_entries(day(4), day(4), None).await.unwrap().len(),
        1,
        "one clock, one hour — never a duplicate from the losers"
    );
}

#[tokio::test]
async fn stopping_nothing_is_a_clean_absence() {
    let store = common::test_store().await;
    let (a, tenant) = tenant_with_user(&store, "nostop").await;
    let pool = pool().await;
    assert_not_found(a.stop_timer(Some(day(3))).await);
    assert_eq!(timer_rows(&pool, &tenant).await, 0);
    assert!(
        a.time_entries(day(1), day(28), None)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_clock_may_only_run_on_a_board_the_person_can_open() {
    let store = common::test_store().await;
    let (a, tenant) = tenant_with_user(&store, "doors").await;
    let (b, _) = second_user(&store, &tenant, "colleague").await;

    // A colleague's personal board is not somewhere you may start work.
    let theirs = b.ensure_personal_project().await.unwrap();
    assert_not_found(a.start_timer(&StartTimer::on(theirs)).await);

    // An archived board is closed to new work. Nothing in the store archives a
    // project yet (Tasks has no such verb), so the state is planted directly —
    // the reader and its refusal are what this asserts.
    let archived = a.create_task_project("Finished", None).await.unwrap();
    sqlx::query("UPDATE task_projects SET archived = true WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_str())
        .bind(archived.as_str())
        .execute(&pool().await)
        .await
        .unwrap();
    assert_not_found(a.start_timer(&StartTimer::on(archived)).await);

    // A board that never existed reads the same.
    assert_not_found(
        a.start_timer(&StartTimer::on(ProjectId::new("no-such-board".to_owned())))
            .await,
    );

    // And one's own personal board is fine.
    let mine = a.ensure_personal_project().await.unwrap();
    let running = a.start_timer(&StartTimer::on(mine.clone())).await.unwrap();
    assert_eq!(running.project_id, mine);
}

#[tokio::test]
async fn a_named_task_must_live_on_the_clocks_own_board() {
    let store = common::test_store().await;
    let (a, _t) = tenant_with_user(&store, "tasks").await;
    let here = engagement(&a, "This board", Some(1_000)).await;
    let elsewhere = engagement(&a, "That board", Some(1_000)).await;
    let stray = a
        .create_task(
            &elsewhere,
            &NewTask {
                title: "Somebody else's work".to_owned(),
                ..NewTask::default()
            },
        )
        .await
        .unwrap();

    let refused = a
        .start_timer(&StartTimer {
            task_id: Some(stray),
            ..StartTimer::on(here.clone())
        })
        .await;
    match refused {
        Err(StoreError::Validation(msg)) => assert!(
            msg.contains("same project"),
            "expected the rule named, got {msg:?}"
        ),
        other => panic!("expected Validation, got {other:?}"),
    }
    assert!(
        a.running_timer().await.unwrap().is_none(),
        "a refused start leaves no clock behind — the person hears about it before an hour runs"
    );
}

#[tokio::test]
async fn a_note_longer_than_the_bound_is_refused_before_the_column_sees_it() {
    let store = common::test_store().await;
    let (a, _t) = tenant_with_user(&store, "bounds").await;
    let project = engagement(&a, "Bounded", Some(1_000)).await;
    let refused = a
        .start_timer(&StartTimer {
            note: "x".repeat(alo_store::TIME_NOTE_MAX + 1),
            ..StartTimer::on(project.clone())
        })
        .await;
    match refused {
        Err(StoreError::Validation(msg)) => assert!(msg.contains("note"), "{msg:?}"),
        other => panic!("expected Validation, got {other:?}"),
    }
    assert!(a.running_timer().await.unwrap().is_none());
    // The bound itself is inclusive.
    a.start_timer(&StartTimer {
        note: "x".repeat(alo_store::TIME_NOTE_MAX),
        ..StartTimer::on(project)
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn another_tenants_clock_is_absent_on_every_path() {
    let store = common::test_store().await;
    let (a, _t1) = tenant_with_user(&store, "mine").await;
    let (b, tenant_b) = tenant_with_user(&store, "theirs").await;
    let pool = pool().await;
    let theirs = engagement(&b, "Their engagement", Some(7_000)).await;
    b.start_timer(&StartTimer::on(theirs.clone()))
        .await
        .unwrap();

    assert!(
        a.running_timer().await.unwrap().is_none(),
        "their running clock is not ours to see"
    );
    assert_not_found(a.stop_timer(Some(day(3))).await);
    assert_not_found(a.start_timer(&StartTimer::on(theirs)).await);
    assert_eq!(
        timer_rows(&pool, &tenant_b).await,
        1,
        "and nothing we did touched their row"
    );
    assert!(b.running_timer().await.unwrap().is_some());
}

#[tokio::test]
async fn a_colleagues_clock_is_absent_too_inside_one_tenant() {
    let store = common::test_store().await;
    let (a, tenant) = tenant_with_user(&store, "same-a").await;
    let (b, _) = second_user(&store, &tenant, "same-b").await;
    let pool = pool().await;
    // One shared team board, which both may work on.
    let shared = engagement(&a, "Shared engagement", Some(8_000)).await;

    a.start_timer(&StartTimer::on(shared.clone()))
        .await
        .unwrap();
    assert!(
        b.running_timer().await.unwrap().is_none(),
        "a colleague's clock is not visible — not a refusal, an absence"
    );
    assert_not_found(b.stop_timer(Some(day(3))).await);

    // Each person's clock is their own, on the very same board.
    b.start_timer(&StartTimer::on(shared)).await.unwrap();
    assert_eq!(timer_rows(&pool, &tenant).await, 2);
    let stopped = b.stop_timer(Some(day(3))).await.unwrap();
    assert!(
        a.running_timer().await.unwrap().is_some(),
        "their stop left ours running"
    );
    assert!(
        a.time_entry(&stopped.entry.id).await.unwrap().is_none(),
        "and the hour it wrote is theirs, not ours"
    );
}

#[tokio::test]
async fn deleting_the_board_takes_the_clock_with_it() {
    let store = common::test_store().await;
    let (a, tenant) = tenant_with_user(&store, "cascade").await;
    let pool = pool().await;
    let project = engagement(&a, "Doomed", Some(1_000)).await;
    a.start_timer(&StartTimer::on(project.clone()))
        .await
        .unwrap();
    assert_eq!(timer_rows(&pool, &tenant).await, 1);

    sqlx::query("DELETE FROM task_projects WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_str())
        .bind(project.as_str())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        timer_rows(&pool, &tenant).await,
        0,
        "a clock on a board that no longer exists has nowhere to land its hour"
    );
    assert!(a.running_timer().await.unwrap().is_none());
    assert_not_found(a.stop_timer(Some(day(3))).await);
}
