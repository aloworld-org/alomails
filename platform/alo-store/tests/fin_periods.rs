//! Fiscal periods and the soft close (alo Finance, ADR 0035, wave B4.10) —
//! the lifecycle, the derived lock date, the rule it puts on the journal, and
//! the tenancy proof (Law 1: isolation is tested, not assumed).
//!
//! Four things are asserted here that nothing else can assert:
//!
//! - **A close shuts the journal.** Not a flag on a screen: an entry dated into
//!   a closed period is a typed conflict from
//!   [`AccountStore::post_fin_entry`], the one door the books have.
//! - **The close is soft.** Reopening with a reason lets exactly that entry in,
//!   and closing again shuts it once more.
//! - **Closed periods stay a contiguous prefix**, because the lock date is a
//!   maximum: closing out of order or reopening out of order is refused rather
//!   than allowed to make "closed through X" a lie.
//! - **A tenant's close is theirs alone**: tenant B posts freely into the dates
//!   tenant A has shut, cannot see A's periods, and cannot close or reopen one.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    Account, AccountRole, AccountStore, CHART, ChartName, ChartSeed, EntryKind, EntrySource,
    FinPeriodId, FxSnapshot, NewEntry, NewPosting, PeriodStatus, SourceEvent, SourceKind, Store,
    StoreError, TenantId,
};
use time::{Date, Month};

/// Asserts a result is a typed conflict whose message names the rule.
fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>, expect: &str) {
    match result {
        Err(StoreError::Conflict(msg)) => {
            assert!(
                msg.contains(expect),
                "conflict {msg:?} should name {expect:?}"
            );
        }
        other => panic!("expected Conflict naming {expect:?}, got: {other:?}"),
    }
}

/// Asserts a result is a typed validation refusal whose message names the rule.
fn assert_invalid<T: std::fmt::Debug>(result: Result<T, StoreError>, expect: &str) {
    match result {
        Err(StoreError::Validation(msg)) => {
            assert!(
                msg.contains(expect),
                "message {msg:?} should name {expect:?}"
            );
        }
        other => panic!("expected Validation naming {expect:?}, got: {other:?}"),
    }
}

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).expect("a real day")
}

/// The four quarters of 2026, as an accountant states them.
fn q1() -> (Date, Date) {
    (day(2026, Month::January, 1), day(2026, Month::March, 31))
}
fn q2() -> (Date, Date) {
    (day(2026, Month::April, 1), day(2026, Month::June, 30))
}

fn seed(tag: &str) -> ChartSeed {
    ChartSeed {
        names: CHART
            .iter()
            .map(|account| ChartName {
                code: account.code.to_owned(),
                name: format!("{tag} {}", account.code),
            })
            .collect(),
    }
}

/// A tenant with one user and a seeded chart.
async fn tenant_with_chart(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store
        .create_tenant(&format!("periods-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@periods.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    account
        .fin_accounts_or_seed(&seed(tag), false)
        .await
        .unwrap();
    (account, tenant)
}

async fn role(account: &AccountStore, role: AccountRole) -> Account {
    account
        .fin_account_for_role(role)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("no account for {}", role.as_str()))
}

/// €121.00 invoiced at 21 % on `on`, as a distinct document each time.
async fn invoice_entry(account: &AccountStore, source_id: &str, on: Date) -> NewEntry {
    let ar = role(account, AccountRole::Ar).await;
    let revenue = role(account, AccountRole::Revenue).await;
    let vat = role(account, AccountRole::VatOutput).await;
    NewEntry {
        entry_date: on,
        kind: EntryKind::Invoice,
        source: Some(EntrySource {
            kind: SourceKind::Invoice,
            id: source_id.to_owned(),
            event: SourceEvent::Issue,
        }),
        memo: "an invoice".to_owned(),
        reverses_entry_id: None,
        attachment_node_id: None,
        currency: "EUR".to_owned(),
        fx: FxSnapshot::identity("EUR", on),
        postings: vec![
            NewPosting::new(ar.id.clone(), 12_100, 12_100),
            NewPosting::new(revenue.id.clone(), -10_000, -10_000),
            NewPosting {
                vat_rate_bp: Some(2100),
                ..NewPosting::new(vat.id.clone(), -2_100, -2_100)
            },
        ],
    }
}

#[tokio::test]
async fn a_period_is_defined_closed_and_reopened_and_the_lock_follows() {
    let store = common::test_store().await;
    let (a, _t) = tenant_with_chart(&store, "life").await;

    // ---- nothing is closed, so nothing is locked -------------------------
    assert!(a.fin_periods().await.unwrap().is_empty());
    assert_eq!(a.fin_lock_date().await.unwrap(), None);

    // ---- defining periods ------------------------------------------------
    let first = a.create_fin_period(q1().0, q1().1).await.unwrap();
    let second = a.create_fin_period(q2().0, q2().1).await.unwrap();
    assert_eq!(first.status, PeriodStatus::Open);
    assert!(first.closed_by.is_none() && first.closed_at.is_none());
    assert_eq!(first.note, "");

    let listed = a.fin_periods().await.unwrap();
    assert_eq!(listed.len(), 2, "oldest first");
    assert_eq!(listed[0].from_date, q1().0);
    assert_eq!(listed[1].from_date, q2().0);

    // Neighbours touch; one shared day is an overlap.
    assert_conflict(
        a.create_fin_period(day(2026, Month::March, 31), day(2026, Month::April, 30))
            .await,
        "overlaps 2026-01-01 – 2026-03-31",
    );
    assert_conflict(
        a.create_fin_period(q1().0, q1().1).await,
        "overlaps 2026-01-01 – 2026-03-31",
    );
    assert_invalid(
        a.create_fin_period(q2().1, q2().0).await,
        "on or after the day it starts",
    );
    // 2026-01-01 through 2027-01-01 is 366 days — a common year plus its
    // anniversary — and legal; one day more is a mistyped year.
    assert_invalid(
        a.create_fin_period(day(2026, Month::January, 1), day(2027, Month::January, 2))
            .await,
        "at most 366 days; that one covers 367",
    );

    // ---- an entry lands while everything is open -------------------------
    let inside_q1 = day(2026, Month::February, 9);
    a.post_fin_entry(&invoice_entry(&a, "inv-open", inside_q1).await)
        .await
        .expect("open books take an entry");

    // ---- closing out of order is refused ---------------------------------
    assert_conflict(
        a.close_fin_period(&second.id, "").await,
        "close the periods in order",
    );

    // ---- the close, and what it does to the journal ----------------------
    let closed = a
        .close_fin_period(&first.id, "  filed with the VAT return  ")
        .await
        .unwrap();
    assert_eq!(closed.status, PeriodStatus::Closed);
    assert_eq!(closed.note, "filed with the VAT return", "trimmed");
    assert!(closed.closed_by.is_some(), "who closed it is the state");
    let closed_on = closed.closed_at.expect("when they closed it").date();
    assert_eq!(a.fin_lock_date().await.unwrap(), Some(q1().1));

    let refused = a
        .post_fin_entry(&invoice_entry(&a, "inv-locked", inside_q1).await)
        .await;
    match refused {
        Err(StoreError::Conflict(ref msg)) => {
            assert!(msg.contains("closed through 2026-03-31"), "{msg}");
            assert!(msg.contains("dated 2026-02-09"), "{msg}");
            assert!(msg.contains("2026-01-01 – 2026-03-31"), "{msg}");
            assert!(msg.contains(&format!("closed on {closed_on}")), "{msg}");
        }
        other => panic!("expected the closed-period conflict, got: {other:?}"),
    }
    // The last day of the period is inside it; the first day after it is not.
    assert!(
        a.post_fin_entry(&invoice_entry(&a, "inv-edge-in", q1().1).await)
            .await
            .is_err(),
        "the lock date itself is shut"
    );
    a.post_fin_entry(&invoice_entry(&a, "inv-edge-out", q2().0).await)
        .await
        .expect("the day after the lock date is open");

    // Nothing of the refused entry was written: the source is still free.
    assert!(
        a.fin_entry_for_source(&EntrySource {
            kind: SourceKind::Invoice,
            id: "inv-locked".to_owned(),
            event: SourceEvent::Issue,
        })
        .await
        .unwrap()
        .is_none(),
        "a refused posting leaves no half-entry behind"
    );

    // ---- a period cannot be defined inside shut books --------------------
    assert_conflict(
        a.create_fin_period(day(2025, Month::October, 1), day(2025, Month::December, 31))
            .await,
        "would sit inside them",
    );

    // ---- the close is soft ----------------------------------------------
    assert_invalid(
        a.reopen_fin_period(&first.id, "   ").await,
        "say why this period is being reopened",
    );
    let reopened = a
        .reopen_fin_period(&first.id, "the January rent invoice arrived late")
        .await
        .unwrap();
    assert_eq!(reopened.status, PeriodStatus::Open);
    assert!(
        reopened.closed_by.is_none() && reopened.closed_at.is_none(),
        "the close is cleared whole"
    );
    assert_eq!(reopened.note, "the January rent invoice arrived late");
    assert_eq!(a.fin_lock_date().await.unwrap(), None);
    assert_conflict(
        a.reopen_fin_period(&first.id, "again").await,
        "is not closed",
    );

    let landed = a
        .post_fin_entry(&invoice_entry(&a, "inv-locked", inside_q1).await)
        .await
        .expect("the reopened period takes the entry that was refused");
    assert_eq!(
        a.fin_entry(&landed).await.unwrap().unwrap().entry_date,
        inside_q1
    );

    // ---- and it shuts again ---------------------------------------------
    a.close_fin_period(&first.id, "refiled").await.unwrap();
    assert_eq!(a.fin_lock_date().await.unwrap(), Some(q1().1));
    assert_conflict(a.close_fin_period(&first.id, "").await, "is already closed");

    // ---- reopening out of order is refused -------------------------------
    a.close_fin_period(&second.id, "").await.unwrap();
    assert_eq!(a.fin_lock_date().await.unwrap(), Some(q2().1));
    assert_conflict(
        a.reopen_fin_period(&first.id, "an old receipt turned up")
            .await,
        "reopen the periods newest first",
    );
    a.reopen_fin_period(&second.id, "an old receipt turned up")
        .await
        .unwrap();
    a.reopen_fin_period(&first.id, "an old receipt turned up")
        .await
        .expect("newest first, and now it is");
}

#[tokio::test]
async fn one_tenants_close_never_reaches_another() {
    let store = common::test_store().await;
    let (a, _ta) = tenant_with_chart(&store, "owner").await;
    let (b, _tb) = tenant_with_chart(&store, "outsider").await;

    let owned = a.create_fin_period(q1().0, q1().1).await.unwrap();
    a.create_fin_period(q2().0, q2().1).await.unwrap();
    a.close_fin_period(&owned.id, "filed").await.unwrap();

    // ---- the read side ---------------------------------------------------
    assert!(
        b.fin_periods().await.unwrap().is_empty(),
        "another tenant's periods are not listed"
    );
    assert!(
        b.fin_period(&owned.id).await.unwrap().is_none(),
        "and not readable by id"
    );
    assert_eq!(
        b.fin_lock_date().await.unwrap(),
        None,
        "A's close does not lock B's books"
    );

    // ---- the write side --------------------------------------------------
    match b.close_fin_period(&owned.id, "not yours").await {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound closing a foreign period, got: {other:?}"),
    }
    match b.reopen_fin_period(&owned.id, "not yours").await {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound reopening a foreign period, got: {other:?}"),
    }
    match b
        .close_fin_period(&FinPeriodId::new("no-such-id"), "")
        .await
    {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound for an unknown id, got: {other:?}"),
    }
    // A's period is untouched by either attempt.
    let still = a.fin_period(&owned.id).await.unwrap().unwrap();
    assert_eq!(still.status, PeriodStatus::Closed);
    assert_eq!(still.note, "filed");

    // ---- the journal, which is the point of the lock ---------------------
    let inside = day(2026, Month::February, 9);
    assert!(
        a.post_fin_entry(&invoice_entry(&a, "a-inv", inside).await)
            .await
            .is_err(),
        "A's own books are shut"
    );
    b.post_fin_entry(&invoice_entry(&b, "b-inv", inside).await)
        .await
        .expect("B posts freely into the dates A has closed");

    // B may define the very same dates: two tenants' fiscal years are their own.
    b.create_fin_period(q1().0, q1().1)
        .await
        .expect("the same quarter, in another tenant");
    // And A's later period is still A's to close, after its own predecessor.
    assert_eq!(a.fin_periods().await.unwrap().len(), 2);
    assert_eq!(b.fin_periods().await.unwrap().len(), 1);
}
