//! Tenancy proof for alo Finance's mileage — the rate table and the journeys it
//! turns into claims (Law 1: isolation is tested, not assumed) — plus the arc a
//! journey walks and the rules that decide what it is worth.
//!
//! There are **two** isolation questions here, with different answers, which is
//! the same split `fin_expenses_tenancy.rs` documents for claims:
//!
//! - The **rate table** is tenant-wide configuration: a co-tenant reads the same
//!   rates. Another tenant reads their own (empty) table, and replacing theirs
//!   leaves this one byte-identical.
//! - A **journey** is personal data about one employee — it places a named
//!   person at an address on a date — so a *colleague inside the same tenant* is
//!   as blind to it as another tenant entirely. Absent, never `Forbidden`.
//!
//! And one question that is neither: **the rate a journey was paid at is
//! history**. Replacing the table must not restate a claim that has already been
//! made, which is the property the suite ends on.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountRole, AccountStore, CHART, ChartName, ChartSeed, ExpenseMethod, ExpenseStatus,
    FinAccountId, FinCategoryId, FinMileageId, KM_MAX_MILLI, NewExpenseCategory, NewMileage,
    NewMileageRate, RATE_MAX_CENTS_PER_KM, RATES_MAX, Store, StoreError, TenantId,
};
use time::{Date, Month};

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

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

fn assert_invalid<T: std::fmt::Debug>(result: Result<T, StoreError>, expect: &str) {
    match result {
        Err(StoreError::Validation(msg)) => {
            assert!(
                msg.contains(expect),
                "validation {msg:?} should name {expect:?}"
            );
        }
        other => panic!("expected Validation naming {expect:?}, got: {other:?}"),
    }
}

/// The chart seed as the HTTP edge hands it in, tagged per tenant so a leak
/// would show itself.
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
async fn tenant_with_chart(store: &Store, tag: &str) -> (AccountStore, TenantId, FinAccountId) {
    let tenant = store.create_tenant(&format!("mil-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@mileage.test"))
        .await
        .unwrap();
    let door = store.for_account(tenant.clone(), user);
    door.fin_accounts_or_seed(&seed(tag), false).await.unwrap();
    let account = door
        .fin_account_for_role(AccountRole::ExpenseDefault)
        .await
        .unwrap()
        .expect("the seeded chart holds expense_default")
        .id;
    (door, tenant, account)
}

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).unwrap()
}

/// The German shape of the table: thirty cents until 2026, thirty-eight after.
fn german_table() -> Vec<NewMileageRate> {
    vec![
        NewMileageRate {
            effective_from: day(2025, Month::January, 1),
            cents_per_km: 30,
            note: "board decision".to_owned(),
        },
        NewMileageRate {
            effective_from: day(2026, Month::January, 1),
            cents_per_km: 38,
            note: String::new(),
        },
    ]
}

/// Berlin → München and back, on a day in 2026: 250 km at 38 c = €95.00.
fn journey() -> NewMileage {
    NewMileage {
        from_place: "Berlin".to_owned(),
        to_place: "München".to_owned(),
        reason: "Kundentermin".to_owned(),
        ..NewMileage::driven(day(2026, Month::March, 14), 250_000)
    }
}

// ---- the rate table ---------------------------------------------------------

#[tokio::test]
async fn the_rate_table_is_tenant_wide_and_never_cross_tenant() {
    let store = common::test_store().await;
    let (a, t1, _a_account) = tenant_with_chart(&store, "rates-a").await;
    let colleague_id = store
        .for_tenant(t1.clone())
        .create_user("colleague@mileage.test")
        .await
        .unwrap();
    let colleague = store.for_account(t1.clone(), colleague_id);
    let (b, _t2, _b_account) = tenant_with_chart(&store, "rates-b").await;

    // It ships empty: we publish nobody's rate for them.
    assert!(a.fin_mileage_rates().await.unwrap().is_empty());

    let written = a.replace_fin_mileage_rates(&german_table()).await.unwrap();
    assert_eq!(written.len(), 2);
    assert_eq!(written[0].cents_per_km, 38, "newest period first");
    assert_eq!(written[0].effective_from, day(2026, Month::January, 1));
    assert_eq!(written[1].note, "board decision");
    // A co-tenant reads the same configuration; another tenant reads their own.
    assert_eq!(colleague.fin_mileage_rates().await.unwrap().len(), 2);
    assert!(b.fin_mileage_rates().await.unwrap().is_empty());

    // Replacing is a whole-table write, so B's replace touches nothing of A's.
    b.replace_fin_mileage_rates(&[NewMileageRate {
        effective_from: day(2026, Month::January, 1),
        cents_per_km: 25,
        note: String::new(),
    }])
    .await
    .unwrap();
    let after = a.fin_mileage_rates().await.unwrap();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0].cents_per_km, 38, "A's table is byte-identical");
    assert_eq!(b.fin_mileage_rates().await.unwrap()[0].cents_per_km, 25);

    // ---- validated whole, or not written at all -------------------------
    let before = a.fin_mileage_rates().await.unwrap();
    for (bad, expect) in [
        (
            vec![NewMileageRate {
                cents_per_km: 0,
                ..german_table()[0].clone()
            }],
            "per kilometre",
        ),
        (
            vec![NewMileageRate {
                cents_per_km: RATE_MAX_CENTS_PER_KM + 1,
                ..german_table()[0].clone()
            }],
            "per kilometre",
        ),
        (
            vec![german_table()[0].clone(), german_table()[0].clone()],
            "same day",
        ),
        (
            (0..=RATES_MAX)
                .map(|index| NewMileageRate {
                    effective_from: day(2000, Month::January, 1)
                        .saturating_add(time::Duration::days(index as i64)),
                    cents_per_km: 30,
                    note: String::new(),
                })
                .collect(),
            "at most",
        ),
    ] {
        assert_invalid(a.replace_fin_mileage_rates(&bad).await, expect);
    }
    assert_eq!(
        a.fin_mileage_rates().await.unwrap(),
        before,
        "a refused replace leaves the table exactly as it was"
    );

    // An empty table is legal and is how a tenant stops paying mileage.
    assert!(a.replace_fin_mileage_rates(&[]).await.unwrap().is_empty());
    assert!(a.fin_mileage_rates().await.unwrap().is_empty());
}

// ---- journeys ---------------------------------------------------------------

#[tokio::test]
async fn a_journey_is_reachable_only_by_the_person_who_drove_it() {
    let store = common::test_store().await;
    let (a, t1, a_account) = tenant_with_chart(&store, "trip-a").await;
    let colleague_id = store
        .for_tenant(t1.clone())
        .create_user("colleague@mileage.test")
        .await
        .unwrap();
    let colleague = store.for_account(t1.clone(), colleague_id);
    let (b, _t2, _b_account) = tenant_with_chart(&store, "trip-b").await;

    a.replace_fin_mileage_rates(&german_table()).await.unwrap();
    b.replace_fin_mileage_rates(&german_table()).await.unwrap();
    let travel = a
        .create_fin_category(&NewExpenseCategory {
            name: "Reisekosten".to_owned(),
            account_id: a_account,
            default_vat_rate_bp: None,
        })
        .await
        .unwrap();
    let claimed = a
        .log_mileage(&NewMileage {
            category_id: Some(travel.clone()),
            ..journey()
        })
        .await
        .unwrap();

    // ---- what the traveller sees ----------------------------------------
    assert_eq!(claimed.journey.km_milli, 250_000);
    assert_eq!(claimed.journey.rate_cents_per_km, 38, "2026's rate");
    assert_eq!(claimed.journey.from_place, "Berlin");
    assert_eq!(claimed.expense.gross_cents, 9500, "250 km × 38 c");
    assert_eq!(claimed.expense.vat_cents, 0, "an allowance carries no VAT");
    assert_eq!(claimed.expense.vat_rate_bp, None);
    assert_eq!(claimed.expense.net_cents(), 9500);
    assert_eq!(claimed.expense.method, ExpenseMethod::Personal);
    assert!(claimed.expense.method.owes_the_employee());
    assert_eq!(claimed.expense.status, ExpenseStatus::Draft);
    assert_eq!(claimed.expense.currency, "EUR", "the accounting currency");
    assert_eq!(
        claimed.expense.description, "Kundentermin",
        "the traveller's own words, not a sentence we composed"
    );
    assert_eq!(claimed.expense.category_id, Some(travel));
    assert_eq!(claimed.expense.spent_on, claimed.journey.travelled_on);
    assert_eq!(
        a.fin_mileage(&claimed.journey.id)
            .await
            .unwrap()
            .unwrap()
            .expense
            .id,
        claimed.expense.id
    );
    let mine = a
        .fin_mileages(day(2026, Month::March, 1), day(2026, Month::March, 31))
        .await
        .unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].journey.id, claimed.journey.id);
    // The claim is an ordinary claim, so it is in the ordinary list too.
    assert_eq!(
        a.expenses(
            day(2026, Month::March, 1),
            day(2026, Month::March, 31),
            None
        )
        .await
        .unwrap()
        .len(),
        1
    );

    // ---- what a colleague of the same tenant sees: nothing --------------
    assert!(
        colleague
            .fin_mileage(&claimed.journey.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        colleague
            .fin_mileages(day(2026, Month::March, 1), day(2026, Month::March, 31))
            .await
            .unwrap()
            .is_empty()
    );
    assert_not_found(colleague.delete_fin_mileage(&claimed.journey.id).await);
    // …nor through the claim, which is the same personal door.
    assert!(
        colleague
            .expense(&claimed.expense.id)
            .await
            .unwrap()
            .is_none()
    );

    // ---- what another tenant sees: the same nothing ---------------------
    assert!(b.fin_mileage(&claimed.journey.id).await.unwrap().is_none());
    assert!(
        b.fin_mileages(day(2026, Month::March, 1), day(2026, Month::March, 31))
            .await
            .unwrap()
            .is_empty()
    );
    assert_not_found(b.delete_fin_mileage(&claimed.journey.id).await);

    // A's journey and its claim are untouched after every one of those.
    let after = a.fin_mileage(&claimed.journey.id).await.unwrap().unwrap();
    assert_eq!(after.journey.km_milli, 250_000);
    assert_eq!(after.expense.gross_cents, 9500);
    assert_eq!(after.expense.updated_at, claimed.expense.updated_at);
}

#[tokio::test]
async fn a_journey_takes_the_rate_in_force_on_the_day_it_was_driven() {
    let store = common::test_store().await;
    let (a, _t1, _a_account) = tenant_with_chart(&store, "rate-on").await;

    // No table at all: refused, naming the day, never an allowance at a rate
    // nobody published.
    assert_invalid(a.log_mileage(&journey()).await, "no mileage rate");

    a.replace_fin_mileage_rates(&german_table()).await.unwrap();

    // December books at last year's rate, January at this year's — the whole
    // reason the table is effective-dated.
    let december = a
        .log_mileage(&NewMileage {
            travelled_on: day(2025, Month::December, 31),
            ..journey()
        })
        .await
        .unwrap();
    assert_eq!(december.journey.rate_cents_per_km, 30);
    assert_eq!(december.expense.gross_cents, 7500, "250 km × 30 c");
    let january = a
        .log_mileage(&NewMileage {
            travelled_on: day(2026, Month::January, 1),
            ..journey()
        })
        .await
        .unwrap();
    assert_eq!(january.journey.rate_cents_per_km, 38);
    assert_eq!(january.expense.gross_cents, 9500);

    // Before the table starts there is no rate, and no rate is a refusal.
    assert_invalid(
        a.log_mileage(&NewMileage {
            travelled_on: day(2024, Month::December, 31),
            ..journey()
        })
        .await,
        "no mileage rate",
    );

    // ---- the rate is history, not a reference ---------------------------
    a.replace_fin_mileage_rates(&[NewMileageRate {
        effective_from: day(2025, Month::January, 1),
        cents_per_km: 1,
        note: "a mistake somebody made in April".to_owned(),
    }])
    .await
    .unwrap();
    let unchanged = a.fin_mileage(&january.journey.id).await.unwrap().unwrap();
    assert_eq!(
        unchanged.journey.rate_cents_per_km, 38,
        "correcting the table must not restate what was already claimed"
    );
    assert_eq!(unchanged.expense.gross_cents, 9500);

    // ---- what the distance has to be ------------------------------------
    for bad in [0, -1, KM_MAX_MILLI + 1] {
        assert_invalid(
            a.log_mileage(&NewMileage {
                km_milli: bad,
                ..journey()
            })
            .await,
            "distance must be between",
        );
    }
    // A journey worth less than half a cent at the rate in force is refused
    // rather than rounded up to a cent nobody can explain. Thirteen metres at
    // one cent per kilometre is 0.013 c.
    assert_invalid(
        a.log_mileage(&NewMileage {
            km_milli: 13,
            ..journey()
        })
        .await,
        "less than a cent",
    );
    // A journey pointing at a category that is not the caller's is absent,
    // never a refusal that confirms it exists.
    assert_not_found(
        a.log_mileage(&NewMileage {
            category_id: Some(FinCategoryId::new("no-such-category".to_owned())),
            ..journey()
        })
        .await,
    );
}

#[tokio::test]
async fn a_journey_is_withdrawn_with_its_claim_and_frozen_with_it() {
    let store = common::test_store().await;
    let (a, tenant, _a_account) = tenant_with_chart(&store, "arc").await;
    a.replace_fin_mileage_rates(&german_table()).await.unwrap();

    // ---- delete takes both rows ------------------------------------------
    let scratch = a.log_mileage(&journey()).await.unwrap();
    a.delete_fin_mileage(&scratch.journey.id).await.unwrap();
    assert!(a.fin_mileage(&scratch.journey.id).await.unwrap().is_none());
    assert!(a.expense(&scratch.expense.id).await.unwrap().is_none());
    assert_not_found(a.delete_fin_mileage(&scratch.journey.id).await);
    // An id that never existed reads exactly like one that has been removed.
    assert!(
        a.fin_mileage(&FinMileageId::new("no-such-journey".to_owned()))
            .await
            .unwrap()
            .is_none()
    );

    // ---- deleting the CLAIM takes the journey with it (the cascade) ------
    let by_claim = a.log_mileage(&journey()).await.unwrap();
    a.delete_expense(&by_claim.expense.id).await.unwrap();
    assert!(
        a.fin_mileage(&by_claim.journey.id).await.unwrap().is_none(),
        "a journey explaining an amount that is gone would point at nothing"
    );

    // ---- handed in, and therefore frozen ---------------------------------
    let claimed = a.log_mileage(&journey()).await.unwrap();
    a.submit_expense(&claimed.expense.id).await.unwrap();
    assert_conflict(
        a.delete_fin_mileage(&claimed.journey.id).await,
        "withdraw it first",
    );
    assert!(
        a.fin_mileage(&claimed.journey.id).await.unwrap().is_some(),
        "the refusal changed nothing"
    );
    // Taking the claim back out of the queue makes the journey the
    // traveller's again — there is one flow, and it is the claim's.
    a.withdraw_expense(&claimed.expense.id).await.unwrap();
    a.delete_fin_mileage(&claimed.journey.id).await.unwrap();
    assert!(a.fin_mileage(&claimed.journey.id).await.unwrap().is_none());

    // ---- the period read, and its one rule -------------------------------
    let march = a.log_mileage(&journey()).await.unwrap();
    assert_eq!(
        a.fin_mileages(day(2026, Month::March, 14), day(2026, Month::March, 14))
            .await
            .unwrap()
            .len(),
        1,
        "both ends are included"
    );
    assert!(
        a.fin_mileages(day(2026, Month::March, 15), day(2026, Month::March, 31))
            .await
            .unwrap()
            .is_empty()
    );
    assert_invalid(
        a.fin_mileages(day(2026, Month::March, 31), day(2026, Month::March, 1))
            .await,
        "before its start",
    );
    assert_eq!(march.journey.rate_cents_per_km, 38);

    // 0106's lesson for the two new foreign keys: a tenant whose journeys point
    // at claims that point at categories can still be dropped.
    store.delete_tenant(&tenant).await.unwrap();
    assert!(
        a.fin_mileages(day(2026, Month::January, 1), day(2026, Month::December, 31))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(a.fin_mileage_rates().await.unwrap().is_empty());
}
