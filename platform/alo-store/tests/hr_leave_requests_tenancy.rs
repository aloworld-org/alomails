//! Tenancy, the state machine and the folds of leave (alo HR, B6.03b — Law 1:
//! isolation is tested, not assumed).
//!
//! Leave is somebody's time and somebody's money, so five things are proven
//! here against the real Postgres:
//!
//! - **wrong tenant** — tenant A's handle cannot read, list, edit, decide,
//!   withdraw or cancel tenant B's request, cannot see them in the absence
//!   layer, cannot fold a balance from them, and gets the clean
//!   `NotFound`/empty rather than data or a 500; nothing tenant A does changes
//!   tenant B's row;
//! - **the state machine** — every arrow of it, and the refusal of every
//!   transition it does not have (an approved request is not editable, decided
//!   leave is not decided twice, started leave is not cancelled);
//! - **the balance is applied** — an approval moves minutes out of *remaining*
//!   and a cancellation puts them back, folded from the requests rather than
//!   from a column;
//! - **the overdraft rule** — a policy that does not allow a negative balance
//!   refuses the approval that would cause one, naming the shortfall; one that
//!   allows it does not;
//! - **the absence layer** — who is away, without the note, the policy or the
//!   kind, and never on a day that person does not work.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::hr_employments::{FULL_TIME_PATTERN, NewEmployment};
use alo_store::hr_leave_math::{Accrual, LeaveYear};
use alo_store::hr_leave_policies::{LeaveKind, NewLeavePolicy};
use alo_store::hr_leave_requests::{LeaveRequestQuery, LeaveStatus, NewLeaveRequest};
use alo_store::{
    HrEmployeeId, HrLeavePolicyId, HrLeaveRequestId, NewEmployee, Store, StoreError, TenantStore,
    UserId,
};
use time::{Date, Month};

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).expect("a real date")
}

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

fn conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(message)) => message,
        other => panic!("expected Conflict, got {other:?}"),
    }
}

fn invalid<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(message)) => message,
        other => panic!("expected Validation, got {other:?}"),
    }
}

/// One tenant, one HR user, one full-time employee employed from 2025, and one
/// annual policy of 25 eight-hour days granted up front on the calendar year.
async fn workplace(
    store: &Store,
    tag: &str,
) -> (TenantStore, UserId, HrEmployeeId, HrLeavePolicyId) {
    let tenant = store
        .create_tenant(&format!("hr-leave-flow-{tag}"))
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
                given_name: "Inès".to_owned(),
                family_name: "Dupont".to_owned(),
                ..Default::default()
            },
            &user,
        )
        .await
        .unwrap();
    hr.append_hr_employment(
        &employee,
        &NewEmployment {
            job_title: "Vertegenwoordiger".to_owned(),
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

fn week(
    employee: &HrEmployeeId,
    policy: &HrLeavePolicyId,
    from: Date,
    to: Date,
) -> NewLeaveRequest {
    NewLeaveRequest {
        employee_id: employee.clone(),
        policy_id: policy.clone(),
        from_day: from,
        to_day: to,
        note: "Skiën".to_owned(),
    }
}

/// **Wrong tenant.** Every path an outsider could take to somebody else's leave
/// ends in the same clean denial, and their own reads stay their own.
#[tokio::test]
async fn another_tenants_leave_is_unreachable_by_every_path() {
    let store = common::test_store().await;
    let (hr_a, user_a, employee_a, policy_a) = workplace(&store, "own").await;
    let (hr_b, user_b, employee_b, policy_b) = workplace(&store, "other").await;

    let today = day(2026, Month::February, 1);
    let request = hr_a
        .create_hr_leave_request(
            &week(
                &employee_a,
                &policy_a,
                day(2026, Month::March, 2),
                day(2026, Month::March, 6),
            ),
            &user_a,
            today,
        )
        .await
        .unwrap();

    // Reads: nothing, and nothing that says the request exists.
    assert!(hr_b.hr_leave_request(&request).await.unwrap().is_none());
    assert!(
        hr_b.hr_leave_requests(&LeaveRequestQuery::default())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        hr_b.hr_leave_requests(&LeaveRequestQuery::for_employee(&employee_a))
            .await
            .unwrap()
            .is_empty(),
        "not even by naming their employee id"
    );

    // Writes: refused, every verb.
    assert_not_found(
        hr_b.update_hr_leave_request(
            &request,
            day(2026, Month::April, 6),
            day(2026, Month::April, 10),
            "",
        )
        .await,
    );
    assert_not_found(
        hr_b.decide_hr_leave_request(&request, true, &user_b, "", today)
            .await,
    );
    assert_not_found(hr_b.withdraw_hr_leave_request(&request, &user_b).await);
    assert_not_found(hr_b.cancel_hr_leave_request(&request, &user_b, today).await);

    // A cross-tenant request cannot even be written: the employee is not theirs.
    assert_not_found(
        hr_b.create_hr_leave_request(
            &week(
                &employee_a,
                &policy_b,
                day(2026, Month::May, 4),
                day(2026, Month::May, 8),
            ),
            &user_b,
            today,
        )
        .await,
    );
    // …nor is the policy.
    assert_not_found(
        hr_b.create_hr_leave_request(
            &week(
                &employee_b,
                &policy_a,
                day(2026, Month::May, 4),
                day(2026, Month::May, 8),
            ),
            &user_b,
            today,
        )
        .await,
    );

    // The absence layer is tenant-bound too: A approves, B sees nobody.
    hr_a.decide_hr_leave_request(&request, true, &user_a, "", today)
        .await
        .unwrap();
    let theirs = hr_a
        .hr_absences(day(2026, Month::March, 1), day(2026, Month::March, 31))
        .await
        .unwrap();
    assert_eq!(theirs.len(), 5, "five working days away");
    assert!(
        hr_b.hr_absences(day(2026, Month::March, 1), day(2026, Month::March, 31))
            .await
            .unwrap()
            .is_empty()
    );

    // And a balance folded by B about A's person is not a balance of zero — it
    // is the clean denial, because the person is not theirs to fold.
    assert_not_found(hr_b.hr_leave_balance(&employee_a, &policy_b, today).await);
    assert_not_found(hr_b.hr_leave_balances(&employee_a, today).await);

    // Nothing tenant B did changed tenant A's row.
    let untouched = hr_a.hr_leave_request(&request).await.unwrap().unwrap();
    assert_eq!(untouched.status, LeaveStatus::Approved);
    assert_eq!(untouched.from_day, day(2026, Month::March, 2));
    assert_eq!(untouched.cost.minutes, 5 * 480);

    // A guessed id is the same answer as one that exists next door.
    let guessed = HrLeaveRequestId::new("hrlr_does_not_exist".to_owned());
    assert!(hr_a.hr_leave_request(&guessed).await.unwrap().is_none());
    assert_not_found(hr_a.withdraw_hr_leave_request(&guessed, &user_a).await);
}

/// The state machine, arrow by arrow — and every transition it does not have.
#[tokio::test]
async fn the_state_machine_allows_its_arrows_and_no_others() {
    let store = common::test_store().await;
    let (hr, user, employee, policy) = workplace(&store, "machine").await;
    let today = day(2026, Month::February, 1);

    // requested → withdrawn.
    let first = hr
        .create_hr_leave_request(
            &week(
                &employee,
                &policy,
                day(2026, Month::March, 2),
                day(2026, Month::March, 6),
            ),
            &user,
            today,
        )
        .await
        .unwrap();
    let stored = hr.hr_leave_request(&first).await.unwrap().unwrap();
    assert_eq!(stored.status, LeaveStatus::Requested);
    assert_eq!(stored.cost.minutes, 5 * 480);
    assert_eq!(stored.cost.working_days, 5);
    assert_eq!(stored.employee_name, "Inès Dupont");
    assert_eq!(stored.policy_name, "Vakantiedagen");
    assert!(stored.decided_by.is_none());

    // Editable while nobody has decided.
    hr.update_hr_leave_request(
        &first,
        day(2026, Month::March, 2),
        day(2026, Month::March, 4),
        "Korter",
    )
    .await
    .unwrap();
    let edited = hr.hr_leave_request(&first).await.unwrap().unwrap();
    assert_eq!(edited.to_day, day(2026, Month::March, 4));
    assert_eq!(edited.cost.minutes, 3 * 480);
    assert_eq!(edited.note, "Korter");

    hr.withdraw_hr_leave_request(&first, &user).await.unwrap();
    let withdrawn = hr.hr_leave_request(&first).await.unwrap().unwrap();
    assert_eq!(withdrawn.status, LeaveStatus::Withdrawn);
    assert!(withdrawn.closed_by.is_some());
    assert!(withdrawn.closed_at.is_some());
    // …and a withdrawn request is not editable, decidable or withdrawable again.
    assert!(
        conflict(
            hr.update_hr_leave_request(
                &first,
                day(2026, Month::March, 2),
                day(2026, Month::March, 4),
                ""
            )
            .await
        )
        .contains("withdrawn")
    );
    assert!(
        conflict(
            hr.decide_hr_leave_request(&first, true, &user, "", today)
                .await
        )
        .contains("withdrawn")
    );
    assert!(conflict(hr.withdraw_hr_leave_request(&first, &user).await).contains("withdrawn"));

    // requested → rejected, with the note the manager wrote.
    let second = hr
        .create_hr_leave_request(
            &week(
                &employee,
                &policy,
                day(2026, Month::March, 2),
                day(2026, Month::March, 6),
            ),
            &user,
            today,
        )
        .await
        .unwrap();
    hr.decide_hr_leave_request(&second, false, &user, "Te druk die week", today)
        .await
        .unwrap();
    let rejected = hr.hr_leave_request(&second).await.unwrap().unwrap();
    assert_eq!(rejected.status, LeaveStatus::Rejected);
    assert_eq!(rejected.decision_note, "Te druk die week");
    assert_eq!(rejected.decided_by.as_deref(), Some(user.as_str()));
    assert!(rejected.decided_at.is_some());
    // A rejection frees the days again.
    let third = hr
        .create_hr_leave_request(
            &week(
                &employee,
                &policy,
                day(2026, Month::March, 2),
                day(2026, Month::March, 6),
            ),
            &user,
            today,
        )
        .await
        .unwrap();

    // requested → approved, and approved is not editable.
    hr.decide_hr_leave_request(&third, true, &user, "Veel plezier", today)
        .await
        .unwrap();
    assert!(
        conflict(
            hr.update_hr_leave_request(
                &third,
                day(2026, Month::March, 2),
                day(2026, Month::March, 3),
                ""
            )
            .await
        )
        .contains("cancel it and ask again")
    );
    assert!(
        conflict(
            hr.decide_hr_leave_request(&third, false, &user, "", today)
                .await
        )
        .contains("already approved")
    );
    assert!(conflict(hr.withdraw_hr_leave_request(&third, &user).await).contains("approved"));

    // approved → cancelled, while it has not started.
    hr.cancel_hr_leave_request(&third, &user, today)
        .await
        .unwrap();
    let cancelled = hr.hr_leave_request(&third).await.unwrap().unwrap();
    assert_eq!(cancelled.status, LeaveStatus::Cancelled);
    assert!(cancelled.closed_by.is_some());
    assert_eq!(
        cancelled.decided_by.as_deref(),
        Some(user.as_str()),
        "the approval that preceded it stays on the row"
    );

    // Approved leave that has begun is HR's to correct, not the employee's to
    // erase.
    let fourth = hr
        .create_hr_leave_request(
            &week(
                &employee,
                &policy,
                day(2026, Month::March, 2),
                day(2026, Month::March, 6),
            ),
            &user,
            today,
        )
        .await
        .unwrap();
    hr.decide_hr_leave_request(&fourth, true, &user, "", today)
        .await
        .unwrap();
    assert!(
        conflict(
            hr.cancel_hr_leave_request(&fourth, &user, day(2026, Month::March, 3))
                .await
        )
        .contains("2026-03-02")
    );
}

/// What the record refuses on its own terms: a backwards range, days outside
/// the employment, a weekend, and days another live request already covers.
#[tokio::test]
async fn a_request_is_refused_when_the_days_do_not_make_sense() {
    let store = common::test_store().await;
    let (hr, user, employee, policy) = workplace(&store, "refusals").await;
    let today = day(2026, Month::February, 1);

    let backwards = hr
        .create_hr_leave_request(
            &week(
                &employee,
                &policy,
                day(2026, Month::March, 6),
                day(2026, Month::March, 2),
            ),
            &user,
            today,
        )
        .await;
    assert!(invalid(backwards).contains("end on or after"));

    let before_the_job = hr
        .create_hr_leave_request(
            &week(
                &employee,
                &policy,
                day(2024, Month::March, 4),
                day(2024, Month::March, 6),
            ),
            &user,
            today,
        )
        .await;
    assert!(invalid(before_the_job).contains("2025-01-01"));

    // Saturday and Sunday, on a Monday-to-Friday pattern.
    let weekend = hr
        .create_hr_leave_request(
            &week(
                &employee,
                &policy,
                day(2026, Month::March, 7),
                day(2026, Month::March, 8),
            ),
            &user,
            today,
        )
        .await;
    assert!(invalid(weekend).contains("costs nothing"));

    // An over-long note is the caller's to fix.
    let shouting = hr
        .create_hr_leave_request(
            &NewLeaveRequest {
                note: "x".repeat(2_001),
                ..week(
                    &employee,
                    &policy,
                    day(2026, Month::March, 2),
                    day(2026, Month::March, 3),
                )
            },
            &user,
            today,
        )
        .await;
    assert!(invalid(shouting).contains("leave note"));

    // Overlap: a request, then another touching one of its days.
    let booked = hr
        .create_hr_leave_request(
            &week(
                &employee,
                &policy,
                day(2026, Month::March, 2),
                day(2026, Month::March, 6),
            ),
            &user,
            today,
        )
        .await
        .unwrap();
    let clash = hr
        .create_hr_leave_request(
            &week(
                &employee,
                &policy,
                day(2026, Month::March, 6),
                day(2026, Month::March, 11),
            ),
            &user,
            today,
        )
        .await;
    let message = conflict(clash);
    assert!(message.contains("2026-03-02"), "it names what covers them");
    assert!(message.contains("requested"));

    // A withdrawn request frees its days.
    hr.withdraw_hr_leave_request(&booked, &user).await.unwrap();
    hr.create_hr_leave_request(
        &week(
            &employee,
            &policy,
            day(2026, Month::March, 6),
            day(2026, Month::March, 11),
        ),
        &user,
        today,
    )
    .await
    .unwrap();
}

/// The balance is applied: an approval spends it, a cancellation gives it back,
/// and a policy that forbids a negative balance refuses the approval that would
/// cause one.
#[tokio::test]
async fn the_balance_is_applied_and_an_overdraft_is_refused() {
    let store = common::test_store().await;
    let (hr, user, employee, policy) = workplace(&store, "balance").await;
    let today = day(2026, Month::February, 1);

    let start = hr
        .hr_leave_balance(&employee, &policy, today)
        .await
        .unwrap();
    assert_eq!(start.entitlement_minutes, 25 * 480);
    assert_eq!(start.accrued_minutes, 25 * 480, "granted up front");
    assert_eq!(start.remaining_minutes, 25 * 480);

    let request = hr
        .create_hr_leave_request(
            &week(
                &employee,
                &policy,
                day(2026, Month::March, 2),
                day(2026, Month::March, 6),
            ),
            &user,
            today,
        )
        .await
        .unwrap();
    // Awaiting a decision: reported, never deducted.
    let pending = hr
        .hr_leave_balance(&employee, &policy, today)
        .await
        .unwrap();
    assert_eq!(pending.pending_minutes, 5 * 480);
    assert_eq!(pending.remaining_minutes, 25 * 480);

    hr.decide_hr_leave_request(&request, true, &user, "", today)
        .await
        .unwrap();
    let approved = hr
        .hr_leave_balance(&employee, &policy, today)
        .await
        .unwrap();
    assert_eq!(approved.pending_minutes, 0);
    assert_eq!(approved.booked_minutes, 5 * 480, "still ahead of today");
    assert_eq!(approved.taken_minutes, 0);
    assert_eq!(approved.remaining_minutes, 20 * 480);

    // The same approval, read after the days have passed, is taken rather than
    // booked — and the remainder is the same figure.
    let later = hr
        .hr_leave_balance(&employee, &policy, day(2026, Month::April, 1))
        .await
        .unwrap();
    assert_eq!(later.taken_minutes, 5 * 480);
    assert_eq!(later.booked_minutes, 0);
    assert_eq!(later.remaining_minutes, 20 * 480);

    // Cancelling gives the minutes back.
    hr.cancel_hr_leave_request(&request, &user, today)
        .await
        .unwrap();
    let cancelled = hr
        .hr_leave_balance(&employee, &policy, today)
        .await
        .unwrap();
    assert_eq!(cancelled.booked_minutes, 0);
    assert_eq!(cancelled.remaining_minutes, 25 * 480);

    // Now spend nearly all of it, and ask for more than is left.
    let big = hr
        .create_hr_leave_request(
            &week(
                &employee,
                &policy,
                day(2026, Month::April, 6),
                day(2026, Month::June, 26),
            ),
            &user,
            today,
        )
        .await
        .unwrap();
    let cost = hr
        .hr_leave_request(&big)
        .await
        .unwrap()
        .unwrap()
        .cost
        .minutes;
    assert!(cost > 25 * 480, "more than the year grants");
    let refusal = conflict(
        hr.decide_hr_leave_request(&big, true, &user, "", today)
            .await,
    );
    assert!(refusal.contains("more than the balance allows"));
    // Rejecting it is always allowed — a refusal never depends on the balance.
    hr.decide_hr_leave_request(&big, false, &user, "Te veel", today)
        .await
        .unwrap();

    // A policy that allows a negative balance takes the same request.
    let unpaid = hr
        .create_hr_leave_policy(
            &NewLeavePolicy {
                name: "Onbetaald verlof".to_owned(),
                kind: LeaveKind::Unpaid,
                entitlement_minutes: 0,
                allow_negative: true,
                paid: false,
                ..Default::default()
            },
            &user,
        )
        .await
        .unwrap();
    let long = hr
        .create_hr_leave_request(
            &week(
                &employee,
                &unpaid,
                day(2026, Month::April, 6),
                day(2026, Month::June, 26),
            ),
            &user,
            today,
        )
        .await
        .unwrap();
    hr.decide_hr_leave_request(&long, true, &user, "", today)
        .await
        .unwrap();
    let unpaid_balance = hr
        .hr_leave_balance(&employee, &unpaid, today)
        .await
        .unwrap();
    assert!(unpaid_balance.remaining_minutes < 0, "allowed to go under");
    // …and the annual policy's own balance is untouched by it.
    assert_eq!(
        hr.hr_leave_balance(&employee, &policy, today)
            .await
            .unwrap()
            .remaining_minutes,
        25 * 480
    );

    // Every live policy at once, for the screen that shows them side by side.
    let all = hr.hr_leave_balances(&employee, today).await.unwrap();
    assert_eq!(all.len(), 2);
    let annual = all
        .iter()
        .find(|entry| entry.policy.id == policy)
        .expect("the annual policy");
    assert_eq!(annual.balance.remaining_minutes, 25 * 480);
    assert_eq!(annual.average_day_minutes, 480);
}

/// A policy that records rather than decides lands approved, with the requester
/// named as the decider — so the record does not pretend somebody decided.
#[tokio::test]
async fn leave_that_needs_no_decision_is_approved_as_it_is_written() {
    let store = common::test_store().await;
    let (hr, user, employee, _) = workplace(&store, "sick").await;
    let today = day(2026, Month::February, 1);

    let sick = hr
        .create_hr_leave_policy(
            &NewLeavePolicy {
                name: "Ziekteverzuim".to_owned(),
                kind: LeaveKind::Sick,
                entitlement_minutes: 0,
                requires_approval: false,
                allow_negative: true,
                ..Default::default()
            },
            &user,
        )
        .await
        .unwrap();
    let recorded = hr
        .create_hr_leave_request(
            &week(
                &employee,
                &sick,
                day(2026, Month::January, 26),
                day(2026, Month::January, 27),
            ),
            &user,
            today,
        )
        .await
        .unwrap();
    let stored = hr.hr_leave_request(&recorded).await.unwrap().unwrap();
    assert_eq!(stored.status, LeaveStatus::Approved);
    assert_eq!(stored.decided_by.as_deref(), Some(user.as_str()));
    assert!(stored.decided_at.is_some());
    assert_eq!(stored.cost.minutes, 2 * 480);

    // An archived policy cannot be chosen for a new absence.
    hr.set_hr_leave_policy_archived(&sick, true).await.unwrap();
    assert!(
        conflict(
            hr.create_hr_leave_request(
                &week(
                    &employee,
                    &sick,
                    day(2026, Month::February, 9),
                    day(2026, Month::February, 10),
                ),
                &user,
                today,
            )
            .await
        )
        .contains("retired")
    );
    // …and the absence already recorded on it is still readable.
    assert!(hr.hr_leave_request(&recorded).await.unwrap().is_some());
}

/// The absence layer: who is away, on the days they actually work, and nothing
/// about why.
#[tokio::test]
async fn the_absence_layer_says_who_is_away_and_nothing_else() {
    let store = common::test_store().await;
    let (hr, user, employee, policy) = workplace(&store, "absences").await;
    let today = day(2026, Month::February, 1);

    // A colleague on a Monday-to-Wednesday pattern.
    let colleague = hr
        .create_hr_employee(
            &NewEmployee {
                given_name: "Jonas".to_owned(),
                family_name: "Peeters".to_owned(),
                ..Default::default()
            },
            &user,
        )
        .await
        .unwrap();
    hr.append_hr_employment(
        &colleague,
        &NewEmployment {
            started_on: day(2025, Month::January, 1),
            pattern_minutes: [480, 480, 480, 0, 0, 0, 0],
            ..Default::default()
        },
        &user,
    )
    .await
    .unwrap();

    // Both away the same week; one of them works only three days of it.
    let mut requests = Vec::new();
    for person in [&employee, &colleague] {
        let request = hr
            .create_hr_leave_request(
                &week(
                    person,
                    &policy,
                    day(2026, Month::March, 2),
                    day(2026, Month::March, 8),
                ),
                &user,
                today,
            )
            .await
            .unwrap();
        hr.decide_hr_leave_request(&request, true, &user, "", today)
            .await
            .unwrap();
        requests.push(request);
    }

    let days = hr
        .hr_absences(day(2026, Month::March, 1), day(2026, Month::March, 15))
        .await
        .unwrap();
    // Monday to Friday only — the weekend inside the range is nobody's absence.
    assert_eq!(days.len(), 5);
    assert_eq!(days[0].day, day(2026, Month::March, 2));
    assert_eq!(days[0].people.len(), 2, "both, on the Monday");
    let wednesday = &days[2];
    assert_eq!(wednesday.day, day(2026, Month::March, 4));
    assert_eq!(wednesday.people.len(), 2);
    let thursday = &days[3];
    assert_eq!(thursday.day, day(2026, Month::March, 5));
    assert_eq!(thursday.people.len(), 1, "the part-timer is not away then");
    assert_eq!(thursday.people[0].name, "Inès Dupont");

    // A window that ends before it starts, and one that is absurdly long, are
    // the caller's to fix.
    assert!(
        invalid(
            hr.hr_absences(day(2026, Month::March, 8), day(2026, Month::March, 1))
                .await
        )
        .contains("end on or after")
    );
    assert!(
        invalid(
            hr.hr_absences(day(2026, Month::January, 1), day(2027, Month::December, 31))
                .await
        )
        .contains("longer than")
    );

    // Somebody who has left the directory is not in the planning view.
    hr.set_hr_employee_archived(&colleague, true).await.unwrap();
    let after = hr
        .hr_absences(day(2026, Month::March, 1), day(2026, Month::March, 15))
        .await
        .unwrap();
    assert!(
        after.iter().all(|entry| entry
            .people
            .iter()
            .all(|person| person.name != "Jonas Peeters")),
        "a planning read is about the team there is"
    );

    // Leave given back leaves the layer in the same act (B7.03): the feed reads
    // `status = 'approved'` and nothing else, so the calendar drawn from it has
    // nothing to take back — there was never an event anywhere to delete.
    hr.cancel_hr_leave_request(&requests[0], &user, today)
        .await
        .unwrap();
    let gone = hr
        .hr_absences(day(2026, Month::March, 1), day(2026, Month::March, 15))
        .await
        .unwrap();
    assert!(
        gone.is_empty(),
        "nobody is away once the one live absence is cancelled"
    );
}

/// The queue reads: a person's own list, a manager's list of their people, and
/// HR's tenant-wide one — each answering exactly what it was asked for.
#[tokio::test]
async fn a_list_answers_exactly_the_people_it_was_asked_about() {
    let store = common::test_store().await;
    let (hr, user, employee, policy) = workplace(&store, "queues").await;
    let today = day(2026, Month::February, 1);

    let colleague = hr
        .create_hr_employee(
            &NewEmployee {
                given_name: "Jonas".to_owned(),
                family_name: "Peeters".to_owned(),
                ..Default::default()
            },
            &user,
        )
        .await
        .unwrap();
    hr.append_hr_employment(
        &colleague,
        &NewEmployment {
            started_on: day(2025, Month::January, 1),
            pattern_minutes: FULL_TIME_PATTERN,
            ..Default::default()
        },
        &user,
    )
    .await
    .unwrap();
    for person in [&employee, &colleague] {
        hr.create_hr_leave_request(
            &week(
                person,
                &policy,
                day(2026, Month::March, 2),
                day(2026, Month::March, 6),
            ),
            &user,
            today,
        )
        .await
        .unwrap();
    }

    let mine = hr
        .hr_leave_requests(&LeaveRequestQuery::for_employee(&employee))
        .await
        .unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].employee_id, employee);

    let everybody = hr
        .hr_leave_requests(&LeaveRequestQuery::default())
        .await
        .unwrap();
    assert_eq!(everybody.len(), 2);

    // A manager with no reports gets an empty queue, never the tenant's leave.
    let nobody = hr
        .hr_leave_requests(&LeaveRequestQuery {
            employees: Some(Vec::new()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(nobody.is_empty());

    // Narrowed by state and by window.
    let awaiting = hr
        .hr_leave_requests(&LeaveRequestQuery::default().with_statuses(&[LeaveStatus::Requested]))
        .await
        .unwrap();
    assert_eq!(awaiting.len(), 2);
    let elsewhere = hr
        .hr_leave_requests(
            &LeaveRequestQuery::default()
                .within(day(2026, Month::June, 1), day(2026, Month::June, 30)),
        )
        .await
        .unwrap();
    assert!(elsewhere.is_empty());
}
