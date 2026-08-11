//! Tenancy and arithmetic proofs for public-holiday calendars (alo HR, B6.04 —
//! Law 1: isolation is tested, not assumed).
//!
//! Which calendar a company observes decides what a week off costs its staff, so
//! four things are proven here against the real Postgres:
//!
//! - **wrong tenant** — tenant A's choice is invisible to tenant B, nothing
//!   tenant A writes changes what tenant B's leave costs, and each folds its own
//!   arithmetic;
//! - **the seed** — a company that has pressed nothing observes the calendar of
//!   the country it invoices under, exactly once, and a country the seed does
//!   not carry produces an explicit empty choice rather than a missing one;
//! - **the choice's rules** — an unknown calendar, too many calendars and a
//!   default nothing observes are each refused by name, and observing none is a
//!   real answer that survives a re-read;
//! - **the arithmetic** — a real leave request over Christmas costs four days on
//!   a Belgian calendar and five on none, the balance charges exactly what the
//!   request said it would, and a year the seed has not been reviewed for is
//!   refused rather than folded as if that country had no holidays.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::billing_settings::NewBillingSettings;
use alo_store::hr_employments::{FULL_TIME_PATTERN, NewEmployment};
use alo_store::hr_leave_math::{Accrual, LeaveYear};
use alo_store::hr_leave_policies::{LeaveKind, NewLeavePolicy};
use alo_store::hr_leave_requests::{LeaveStatus, NewLeaveRequest};
use alo_store::{
    HrEmployeeId, HrLeavePolicyId, NewEmployee, Store, StoreError, TenantStore, UserId,
};
use time::{Date, Month};

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).expect("a real date")
}

fn invalid<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(message)) => message,
        other => panic!("expected Validation, got {other:?}"),
    }
}

/// One tenant, one user, one full-time employee, one annual policy — the same
/// workplace the leave suite builds, because holidays are only observable
/// through what leave costs.
async fn workplace(
    store: &Store,
    tag: &str,
) -> (TenantStore, UserId, HrEmployeeId, HrLeavePolicyId) {
    let tenant = store
        .create_tenant(&format!("hr-holidays-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@people.test"))
        .await
        .unwrap();
    let hr = store.for_tenant(tenant);
    let employee = hr
        .create_hr_employee(
            &NewEmployee {
                user_id: Some(user.clone()),
                given_name: "Jonas".to_owned(),
                family_name: "Peeters".to_owned(),
                ..Default::default()
            },
            &user,
        )
        .await
        .unwrap();
    hr.append_hr_employment(
        &employee,
        &NewEmployment {
            job_title: "Ontwikkelaar".to_owned(),
            started_on: day(2025, Month::January, 1),
            pattern_minutes: FULL_TIME_PATTERN,
            ..Default::default()
        },
        &user,
    )
    .await
    .unwrap();
    let policy = hr
        .create_hr_leave_policy(
            &NewLeavePolicy {
                name: "Vakantiedagen".to_owned(),
                kind: LeaveKind::Annual,
                entitlement_minutes: 25 * 480,
                accrual: Accrual::UpFront,
                leave_year: LeaveYear::calendar(),
                ..Default::default()
            },
            &user,
        )
        .await
        .unwrap();
    (hr, user, employee, policy)
}

/// **Wrong tenant.** One company's calendar is not another's, and neither can
/// see or change the other's choice — or the arithmetic that follows from it.
#[tokio::test]
async fn another_tenants_holiday_choice_is_neither_visible_nor_binding() {
    let store = common::test_store().await;
    let (hr_a, user_a, employee_a, policy_a) = workplace(&store, "own").await;
    let (hr_b, user_b, employee_b, policy_b) = workplace(&store, "other").await;

    hr_a.set_hr_holiday_selection(&["BE".to_owned()], Some("BE"), &user_a)
        .await
        .unwrap();

    // Tenant B has made no choice, and tenant A's is not it.
    assert!(hr_b.hr_holiday_selection().await.unwrap().is_none());
    assert!(hr_b.hr_holidays().await.unwrap().calendar_code().is_none());
    assert_eq!(
        hr_a.hr_holidays().await.unwrap().calendar_code(),
        Some("BE")
    );

    // The same week costs each company a different number of days, and neither
    // figure moves when the other company changes its mind.
    let from = day(2026, Month::December, 21);
    let to = day(2026, Month::December, 25);
    let ask = |employee: &HrEmployeeId, policy: &HrLeavePolicyId| NewLeaveRequest {
        employee_id: employee.clone(),
        policy_id: policy.clone(),
        from_day: from,
        to_day: to,
        note: String::new(),
    };
    let in_a = hr_a
        .create_hr_leave_request(&ask(&employee_a, &policy_a), &user_a, from)
        .await
        .unwrap();
    let in_b = hr_b
        .create_hr_leave_request(&ask(&employee_b, &policy_b), &user_b, from)
        .await
        .unwrap();
    let cost_a = hr_a
        .hr_leave_request(&in_a)
        .await
        .unwrap()
        .expect("A's request")
        .cost;
    let cost_b = hr_b
        .hr_leave_request(&in_b)
        .await
        .unwrap()
        .expect("B's request")
        .cost;
    assert_eq!(cost_a.minutes, 4 * 480, "Christmas Day is free in Belgium");
    assert_eq!(cost_a.holiday_minutes, 480);
    assert_eq!(cost_b.minutes, 5 * 480, "B observes nothing");
    assert_eq!(cost_b.holiday_minutes, 0);

    // Tenant B choosing Poland does not restate tenant A's figure.
    hr_b.set_hr_holiday_selection(&["PL".to_owned()], None, &user_b)
        .await
        .unwrap();
    assert_eq!(
        hr_a.hr_leave_request(&in_a)
            .await
            .unwrap()
            .expect("A's request")
            .cost
            .minutes,
        4 * 480
    );
    // Poland's Christmas Eve (from 2025) and Christmas Day both fall in B's
    // week, so B's own figure moves to three days — from B's own choice.
    assert_eq!(
        hr_b.hr_leave_request(&in_b)
            .await
            .unwrap()
            .expect("B's request")
            .cost
            .minutes,
        3 * 480
    );
    // And each company's other reads are its own.
    assert_eq!(
        hr_a.hr_holiday_selection()
            .await
            .unwrap()
            .expect("A's choice")
            .calendars,
        vec!["BE".to_owned()]
    );
    assert_eq!(
        hr_b.hr_holiday_selection()
            .await
            .unwrap()
            .expect("B's choice")
            .calendars,
        vec!["PL".to_owned()]
    );
}

/// A company that has pressed nothing observes the calendar of the country it
/// invoices under — and a country the seed does not carry gets an explicit
/// empty choice rather than a missing one.
#[tokio::test]
async fn a_company_that_has_pressed_nothing_observes_its_own_country() {
    let store = common::test_store().await;
    let (hr, user, _, _) = workplace(&store, "seed-be").await;
    store
        .for_account(hr.tenant().clone(), user.clone())
        .save_billing_settings(&NewBillingSettings {
            legal_name: "Peeters BV".to_owned(),
            country: "BE".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();

    let seeded = hr.ensure_hr_holiday_selection(&user).await.unwrap();
    assert_eq!(seeded.calendars, vec!["BE".to_owned()]);
    assert_eq!(seeded.default_calendar.as_deref(), Some("BE"));
    assert_eq!(seeded.chosen_by, user.as_str());
    // Idempotent: seeding twice is one choice, and a choice already made is
    // never overwritten by the seed.
    hr.set_hr_holiday_selection(&[], None, &user).await.unwrap();
    let again = hr.ensure_hr_holiday_selection(&user).await.unwrap();
    assert!(
        again.calendars.is_empty(),
        "observing none is a choice, not an absence"
    );
    assert!(again.default_calendar.is_none());

    // A country the seed does not carry: an explicit empty choice.
    let (other, user_other, _, _) = workplace(&store, "seed-zz").await;
    store
        .for_account(other.tenant().clone(), user_other.clone())
        .save_billing_settings(&NewBillingSettings {
            legal_name: "Elsewhere Ltd".to_owned(),
            country: "US".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    let empty = other
        .ensure_hr_holiday_selection(&user_other)
        .await
        .unwrap();
    assert!(empty.calendars.is_empty());
    assert!(other.hr_holidays().await.unwrap().calendar_code().is_none());
}

/// The rules of the choice: an unknown calendar, too many, and a default nothing
/// observes are each refused by name; a replacement replaces the whole choice.
#[tokio::test]
async fn a_choice_is_refused_by_name_or_replaced_whole() {
    let store = common::test_store().await;
    let (hr, user, _, _) = workplace(&store, "choice").await;

    let unknown = invalid(
        hr.set_hr_holiday_selection(&["BE".to_owned(), "UK".to_owned()], None, &user)
            .await,
    );
    assert!(unknown.contains("UK"), "{unknown}");
    assert!(
        unknown.contains("BE"),
        "the known list is offered: {unknown}"
    );
    assert!(
        hr.hr_holiday_selection().await.unwrap().is_none(),
        "a refused choice writes nothing"
    );

    let stray = invalid(
        hr.set_hr_holiday_selection(&["BE".to_owned()], Some("NL"), &user)
            .await,
    );
    assert!(stray.contains("NL"), "{stray}");

    let too_many: Vec<String> = [
        "AT", "BE", "DE", "DK", "ES", "FI", "FR", "IE", "IT", "LU", "MT",
    ]
    .iter()
    .map(|code| (*code).to_owned())
    .collect();
    assert!(invalid(hr.set_hr_holiday_selection(&too_many, None, &user).await).contains("10"),);

    // Two calendars, the second the default; then one, and the first is it.
    let two = hr
        .set_hr_holiday_selection(&["be".to_owned(), " nl ".to_owned()], Some("nl"), &user)
        .await
        .unwrap();
    assert_eq!(two.calendars, vec!["BE".to_owned(), "NL".to_owned()]);
    assert_eq!(two.default_calendar.as_deref(), Some("NL"));
    let one = hr
        .set_hr_holiday_selection(&["FR".to_owned()], None, &user)
        .await
        .unwrap();
    assert_eq!(one.calendars, vec!["FR".to_owned()]);
    assert_eq!(
        one.default_calendar.as_deref(),
        Some("FR"),
        "the only calendar is the default"
    );
    assert_eq!(
        hr.hr_holidays().await.unwrap().calendar_code(),
        Some("FR"),
        "the arithmetic follows the choice immediately"
    );
}

/// The whole point: a holiday inside leave costs nothing, the balance charges
/// exactly what the request said, and a year the seed has not been reviewed for
/// is refused rather than folded as if the country had no holidays.
#[tokio::test]
async fn a_holiday_inside_leave_costs_nothing_and_the_balance_agrees() {
    let store = common::test_store().await;
    let (hr, user, employee, policy) = workplace(&store, "arithmetic").await;
    hr.set_hr_holiday_selection(&["BE".to_owned()], Some("BE"), &user)
        .await
        .unwrap();

    // Monday 21 to Friday 25 December 2026 — Christmas Day is the Friday.
    let from = day(2026, Month::December, 21);
    let to = day(2026, Month::December, 25);
    let id = hr
        .create_hr_leave_request(
            &NewLeaveRequest {
                employee_id: employee.clone(),
                policy_id: policy.clone(),
                from_day: from,
                to_day: to,
                note: "Kerst".to_owned(),
            },
            &user,
            from,
        )
        .await
        .unwrap();
    let request = hr
        .hr_leave_request(&id)
        .await
        .unwrap()
        .expect("the request");
    assert_eq!(request.cost.minutes, 4 * 480);
    assert_eq!(request.cost.working_days, 4);
    assert_eq!(request.cost.holiday_minutes, 480);

    // Approved, the balance charges the same four days — one fold, not two.
    hr.decide_hr_leave_request(&id, true, &user, "", from)
        .await
        .unwrap();
    let balance = hr
        .hr_leave_balance(&employee, &policy, day(2026, Month::December, 31))
        .await
        .unwrap();
    assert_eq!(balance.taken_minutes, 4 * 480);
    assert_eq!(balance.remaining_minutes, (25 - 4) * 480);
    assert_eq!(
        hr.hr_leave_request(&id).await.unwrap().expect("it").status,
        LeaveStatus::Approved
    );

    // Dropping the calendar recomputes the same week at five days: nothing is
    // stored, so the arithmetic follows the choice.
    hr.set_hr_holiday_selection(&[], None, &user).await.unwrap();
    assert_eq!(
        hr.hr_leave_request(&id)
            .await
            .unwrap()
            .expect("it")
            .cost
            .minutes,
        5 * 480
    );
    assert_eq!(
        hr.hr_leave_balance(&employee, &policy, day(2026, Month::December, 31))
            .await
            .unwrap()
            .taken_minutes,
        5 * 480
    );

    // A year the seed has not been reviewed for is refused, naming the gap —
    // but only for a company that observes a calendar at all.
    hr.set_hr_holiday_selection(&["BE".to_owned()], Some("BE"), &user)
        .await
        .unwrap();
    let ahead = invalid(
        hr.create_hr_leave_request(
            &NewLeaveRequest {
                employee_id: employee.clone(),
                policy_id: policy.clone(),
                from_day: day(2040, Month::March, 2),
                to_day: day(2040, Month::March, 6),
                note: String::new(),
            },
            &user,
            from,
        )
        .await,
    );
    assert!(ahead.contains("2040"), "{ahead}");
    assert!(ahead.contains("2035"), "{ahead}");
    hr.set_hr_holiday_selection(&[], None, &user).await.unwrap();
    hr.create_hr_leave_request(
        &NewLeaveRequest {
            employee_id: employee,
            policy_id: policy,
            from_day: day(2040, Month::March, 2),
            to_day: day(2040, Month::March, 6),
            note: String::new(),
        },
        &user,
        from,
    )
    .await
    .expect("a company observing nothing has no gap to fall into");
}
