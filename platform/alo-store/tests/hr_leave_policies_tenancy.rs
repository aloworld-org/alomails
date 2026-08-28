//! Tenancy and lifecycle proofs for leave policies (alo HR, B6.03a — Law 1:
//! isolation is tested, not assumed).
//!
//! A policy decides what a person is owed, so an outsider reaching one would be
//! an outsider deciding somebody's holiday. Four things are proven here:
//!
//! - **wrong tenant** — tenant A's handle cannot read, list, edit, archive or
//!   restore tenant B's policy, and gets the clean `NotFound`/empty rather than
//!   data or a 500; nothing tenant A does changes tenant B's row;
//! - **the lifecycle** — create, read back every field, edit, archive, restore,
//!   and the rules that guard each (an archived policy is not editable; two
//!   live policies may not share a name; a freed name may be used again);
//! - **the seed** — a tenant who has pressed nothing gets one workable annual
//!   policy from the statutory minimum of their country, exactly once;
//! - **the arithmetic reads what was stored** — a balance folded through
//!   `hr_leave_math` from a stored policy is the figure the policy states, so
//!   the pure module and the schema agree about minutes.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::billing_settings::NewBillingSettings;
use alo_store::hr_employments::FULL_TIME_PATTERN;
use alo_store::hr_leave_math::{
    Accrual, ENTITLEMENT_MAX_MINUTES, LeaveYear, accrued_minutes, prorated_entitlement_minutes,
    scaled_entitlement_minutes, weekly_minutes,
};
use alo_store::hr_leave_policies::{
    CARRYOVER_EXPIRY_MAX_MONTHS, LeaveKind, NewLeavePolicy, SEEDED_ANNUAL_POLICY_NAME,
};
use alo_store::{HrLeavePolicyId, Store, StoreError, TenantStore, UserId};
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

fn conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(message)) => message,
        other => panic!("expected Conflict, got {other:?}"),
    }
}

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).expect("a real date")
}

/// A tenant with one user: the HR door and the user who acts through it.
async fn tenant_with_user(store: &Store, tag: &str) -> (TenantStore, UserId) {
    let tenant = store
        .create_tenant(&format!("hr-leave-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@people.test"))
        .await
        .unwrap();
    (store.for_tenant(tenant), user)
}

/// A fully-specified policy, so a round trip covers every column.
fn vakantiedagen() -> NewLeavePolicy {
    NewLeavePolicy {
        name: "Vakantiedagen".to_owned(),
        kind: LeaveKind::Annual,
        entitlement_minutes: 25 * 480,
        accrual: Accrual::Monthly,
        leave_year: LeaveYear::new(4, 6).expect("an April leave year"),
        carryover_cap_minutes: 5 * 480,
        carryover_expires_after_months: Some(15),
        allow_negative: false,
        requires_approval: true,
        paid: true,
    }
}

/// **Wrong tenant.** Every path an outsider could take to somebody else's leave
/// policy ends in the same clean denial, and their own list stays their own.
#[tokio::test]
async fn another_tenants_leave_policy_is_unreachable_by_every_path() {
    let store = common::test_store().await;
    let (hr_a, user_a) = tenant_with_user(&store, "own").await;
    let (hr_b, user_b) = tenant_with_user(&store, "other").await;

    let policy = hr_a
        .create_hr_leave_policy(&vakantiedagen(), &user_a)
        .await
        .unwrap();

    // Reads: nothing, and nothing that says the policy exists.
    assert!(hr_b.hr_leave_policy(&policy).await.unwrap().is_none());
    assert!(hr_b.hr_leave_policies(false).await.unwrap().is_empty());
    assert!(hr_b.hr_leave_policies(true).await.unwrap().is_empty());

    // Writes: refused, and the policy is untouched afterwards.
    assert_not_found(
        hr_b.update_hr_leave_policy(
            &policy,
            &NewLeavePolicy {
                entitlement_minutes: 0,
                ..vakantiedagen()
            },
        )
        .await,
    );
    assert_not_found(hr_b.set_hr_leave_policy_archived(&policy, true).await);
    assert_not_found(hr_b.set_hr_leave_policy_archived(&policy, false).await);

    let untouched = hr_a.hr_leave_policy(&policy).await.unwrap().unwrap();
    assert_eq!(untouched.entitlement_minutes, 25 * 480);
    assert!(!untouched.is_archived(), "an outsider changed nothing");

    // A guessed id that exists nowhere is the same answer as one that exists
    // next door: no existence oracle either way.
    let guessed = HrLeavePolicyId::new("hrlp_does_not_exist".to_owned());
    assert!(hr_a.hr_leave_policy(&guessed).await.unwrap().is_none());
    assert_not_found(hr_a.set_hr_leave_policy_archived(&guessed, true).await);
    assert_not_found(
        hr_a.update_hr_leave_policy(&guessed, &vakantiedagen())
            .await,
    );

    // And tenant B may run a policy of the very same name: uniqueness is
    // per-tenant, not global.
    let theirs = hr_b
        .create_hr_leave_policy(&vakantiedagen(), &user_b)
        .await
        .unwrap();
    assert_ne!(theirs.as_str(), policy.as_str());
    assert_eq!(hr_b.hr_leave_policies(false).await.unwrap().len(), 1);
    assert_eq!(hr_a.hr_leave_policies(false).await.unwrap().len(), 1);
}

/// Everything a policy carries survives the round trip, and comes back in the
/// canonical form the arithmetic reads.
#[tokio::test]
async fn a_policy_round_trips_every_field_it_carries() {
    let store = common::test_store().await;
    let (hr, user) = tenant_with_user(&store, "roundtrip").await;

    let id = hr
        .create_hr_leave_policy(&vakantiedagen(), &user)
        .await
        .unwrap();
    let stored = hr.hr_leave_policy(&id).await.unwrap().unwrap();
    assert_eq!(stored.name, "Vakantiedagen");
    assert_eq!(stored.kind, LeaveKind::Annual);
    assert_eq!(stored.entitlement_minutes, 12_000);
    assert_eq!(stored.accrual, Accrual::Monthly);
    assert_eq!(stored.leave_year.month(), 4);
    assert_eq!(stored.leave_year.day(), 6);
    assert_eq!(stored.carryover_cap_minutes, 2_400);
    assert_eq!(stored.carryover_expires_after_months, Some(15));
    assert!(!stored.allow_negative);
    assert!(stored.requires_approval);
    assert!(stored.paid);
    assert!(!stored.is_archived());
    assert_eq!(stored.created_by, user.as_str());

    // The other end of the vocabulary: a sick policy that is recorded rather
    // than approved, and unpaid leave that may go negative.
    let sick = hr
        .create_hr_leave_policy(
            &NewLeavePolicy {
                name: "Ziekteverzuim".to_owned(),
                kind: LeaveKind::Sick,
                entitlement_minutes: 0,
                accrual: Accrual::UpFront,
                requires_approval: false,
                ..NewLeavePolicy::default()
            },
            &user,
        )
        .await
        .unwrap();
    let sick = hr.hr_leave_policy(&sick).await.unwrap().unwrap();
    assert_eq!(sick.kind, LeaveKind::Sick);
    assert_eq!(sick.accrual, Accrual::UpFront);
    assert!(!sick.requires_approval);
    assert_eq!(sick.leave_year.month(), 1, "the calendar year by default");

    // A figure the schema refuses is refused before it reaches the schema.
    match hr
        .create_hr_leave_policy(
            &NewLeavePolicy {
                name: "Onmogelijk".to_owned(),
                entitlement_minutes: ENTITLEMENT_MAX_MINUTES + 1,
                ..NewLeavePolicy::default()
            },
            &user,
        )
        .await
    {
        Err(StoreError::Validation(message)) => assert!(message.contains("entitlement")),
        other => panic!("expected Validation, got {other:?}"),
    }
    match hr
        .create_hr_leave_policy(
            &NewLeavePolicy {
                name: "Onmogelijk".to_owned(),
                carryover_cap_minutes: 480,
                carryover_expires_after_months: Some(CARRYOVER_EXPIRY_MAX_MONTHS + 1),
                entitlement_minutes: 9_600,
                ..NewLeavePolicy::default()
            },
            &user,
        )
        .await
    {
        Err(StoreError::Validation(message)) => assert!(message.contains("expire")),
        other => panic!("expected Validation, got {other:?}"),
    }
    // Two live policies, both readable, sorted by name.
    let live = hr.hr_leave_policies(false).await.unwrap();
    assert_eq!(live.len(), 2);
    assert_eq!(live[0].name, "Vakantiedagen");
    assert_eq!(live[1].name, "Ziekteverzuim");
}

/// The lifecycle: edited while it is run, archived when it is not, and the
/// rules that guard each step.
#[tokio::test]
async fn a_policy_is_edited_while_live_and_archived_rather_than_deleted() {
    let store = common::test_store().await;
    let (hr, user) = tenant_with_user(&store, "lifecycle").await;

    let id = hr
        .create_hr_leave_policy(&vakantiedagen(), &user)
        .await
        .unwrap();

    // Two live policies may not share a name.
    assert!(
        conflict(hr.create_hr_leave_policy(&vakantiedagen(), &user).await)
            .contains("already exists")
    );
    // Case does not get around it.
    assert!(
        conflict(
            hr.create_hr_leave_policy(
                &NewLeavePolicy {
                    name: "VAKANTIEDAGEN".to_owned(),
                    ..vakantiedagen()
                },
                &user
            )
            .await
        )
        .contains("already exists")
    );

    // An ordinary edit: the tenant moves to an up-front grant and a January
    // leave year, and drops the carryover.
    hr.update_hr_leave_policy(
        &id,
        &NewLeavePolicy {
            name: "Vakantie".to_owned(),
            accrual: Accrual::UpFront,
            leave_year: LeaveYear::calendar(),
            carryover_cap_minutes: 0,
            carryover_expires_after_months: None,
            ..vakantiedagen()
        },
    )
    .await
    .unwrap();
    let edited = hr.hr_leave_policy(&id).await.unwrap().unwrap();
    assert_eq!(edited.name, "Vakantie");
    assert_eq!(edited.accrual, Accrual::UpFront);
    assert_eq!(edited.leave_year.month(), 1);
    assert_eq!(edited.carryover_cap_minutes, 0);
    assert_eq!(edited.carryover_expires_after_months, None);
    assert!(edited.updated_at >= edited.created_at);

    // Archived: out of the pickers, still readable, and the name is free again.
    hr.set_hr_leave_policy_archived(&id, true).await.unwrap();
    assert!(hr.hr_leave_policies(false).await.unwrap().is_empty());
    let all = hr.hr_leave_policies(true).await.unwrap();
    assert_eq!(all.len(), 1);
    assert!(all[0].is_archived());
    assert!(all[0].archived_at.is_some(), "and it carries when");

    // An archived policy explains balances already folded from it, so it is not
    // editable.
    assert!(
        conflict(
            hr.update_hr_leave_policy(
                &id,
                &NewLeavePolicy {
                    name: "Vakantie".to_owned(),
                    ..vakantiedagen()
                }
            )
            .await
        )
        .contains("restore it first")
    );

    // The freed name may be used again…
    let successor = hr
        .create_hr_leave_policy(
            &NewLeavePolicy {
                name: "Vakantie".to_owned(),
                entitlement_minutes: 26 * 480,
                ..NewLeavePolicy::default()
            },
            &user,
        )
        .await
        .unwrap();
    assert_eq!(hr.hr_leave_policies(false).await.unwrap().len(), 1);

    // …and restoring the old one is then refused, because two live policies of
    // one name are two answers to "which balance is this?".
    assert!(!conflict(hr.set_hr_leave_policy_archived(&id, false).await).is_empty());
    assert!(
        hr.hr_leave_policy(&id)
            .await
            .unwrap()
            .unwrap()
            .is_archived()
    );

    // Once the successor is out of the way, the restore lands, and archiving
    // twice does not restamp the date.
    hr.set_hr_leave_policy_archived(&successor, true)
        .await
        .unwrap();
    let archived_at = hr
        .hr_leave_policy(&successor)
        .await
        .unwrap()
        .unwrap()
        .archived_at;
    hr.set_hr_leave_policy_archived(&successor, true)
        .await
        .unwrap();
    assert_eq!(
        hr.hr_leave_policy(&successor)
            .await
            .unwrap()
            .unwrap()
            .archived_at,
        archived_at,
        "archiving is idempotent, not a re-stamp"
    );
    hr.set_hr_leave_policy_archived(&id, false).await.unwrap();
    let restored = hr.hr_leave_policy(&id).await.unwrap().unwrap();
    assert!(!restored.is_archived());
    assert_eq!(hr.hr_leave_policies(false).await.unwrap().len(), 1);
}

/// **The seed.** A tenant who has pressed nothing still has a workable annual
/// policy, from the statutory minimum of their country — once, whoever asks.
#[tokio::test]
async fn a_tenant_who_has_pressed_nothing_gets_one_workable_policy() {
    let store = common::test_store().await;
    let (hr, user) = tenant_with_user(&store, "seed-fr").await;

    // The tenant's country comes from the identity they invoice under.
    store
        .for_account(hr.tenant().clone(), user.clone())
        .save_billing_settings(&NewBillingSettings {
            legal_name: "Atelier Dupont SARL".to_owned(),
            country: "FR".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();

    let seeded = hr
        .ensure_hr_leave_policies("Congés payés", &user)
        .await
        .unwrap();
    assert_eq!(seeded.len(), 1);
    let policy = &seeded[0];
    assert_eq!(
        policy.name, "Congés payés",
        "the caller's language, not ours"
    );
    assert_eq!(policy.kind, LeaveKind::Annual);
    // France: 30 jours ouvrables = 25 days on a five-day week, at eight hours.
    assert_eq!(policy.entitlement_minutes, 25 * 480);
    assert_eq!(policy.accrual, Accrual::Monthly);
    assert_eq!(policy.leave_year.month(), 1);
    assert_eq!(policy.leave_year.day(), 1);
    assert_eq!(policy.carryover_cap_minutes, 0);
    assert!(policy.requires_approval);
    assert!(policy.paid);

    // Idempotent: asking again changes nothing, whatever name is offered.
    let again = hr
        .ensure_hr_leave_policies("Something else", &user)
        .await
        .unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].id.as_str(), policy.id.as_str());
    assert_eq!(again[0].name, "Congés payés");

    // A tenant whose only policy is archived is not re-seeded: they retired it
    // on purpose.
    hr.set_hr_leave_policy_archived(&policy.id, true)
        .await
        .unwrap();
    assert!(
        hr.ensure_hr_leave_policies("", &user)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(hr.hr_leave_policies(true).await.unwrap().len(), 1);
}

/// A tenant with no stated country still gets the European floor, under the
/// fallback name, rather than a policy granting nothing.
#[tokio::test]
async fn a_tenant_with_no_stated_country_gets_the_european_floor() {
    let store = common::test_store().await;
    let (hr, user) = tenant_with_user(&store, "seed-floor").await;

    let seeded = hr.ensure_hr_leave_policies("  ", &user).await.unwrap();
    assert_eq!(seeded.len(), 1);
    assert_eq!(seeded[0].name, SEEDED_ANNUAL_POLICY_NAME);
    // Directive 2003/88/EC Art. 7: four weeks.
    assert_eq!(seeded[0].entitlement_minutes, 20 * 480);
    assert_eq!(seeded[0].created_by, user.as_str());
}

/// The stored policy and the pure arithmetic agree: a balance folded from what
/// the schema returned is the figure the policy states, in minutes.
#[tokio::test]
async fn the_arithmetic_folds_what_the_schema_returned() {
    let store = common::test_store().await;
    let (hr, user) = tenant_with_user(&store, "fold").await;

    let id = hr
        .create_hr_leave_policy(
            &NewLeavePolicy {
                name: "Urlaub".to_owned(),
                entitlement_minutes: 20 * 480,
                accrual: Accrual::Monthly,
                ..NewLeavePolicy::default()
            },
            &user,
        )
        .await
        .unwrap();
    let policy = hr.hr_leave_policy(&id).await.unwrap().unwrap();

    // Somebody on a three-day week, employed from 1 July.
    let pattern = [480, 480, 480, 0, 0, 0, 0];
    let scaled = scaled_entitlement_minutes(
        policy.entitlement_minutes,
        weekly_minutes(&pattern),
        weekly_minutes(&FULL_TIME_PATTERN),
    );
    assert_eq!(scaled, 5_760, "three fifths of twenty eight-hour days");

    let year = policy.leave_year.window(day(2026, Month::September, 1));
    assert_eq!(year.0, day(2026, Month::January, 1));
    let entitlement =
        prorated_entitlement_minutes(scaled, year, Some(day(2026, Month::July, 1)), None);
    assert_eq!(entitlement, 5_760 - 5_760 * 181 / 365);

    // Monthly accrual over that year, on the last day of it, is all of it.
    assert_eq!(
        accrued_minutes(
            entitlement,
            policy.accrual,
            year.0,
            day(2026, Month::December, 31)
        ),
        entitlement
    );
    // And in June — six twelfths, from a policy nobody has edited since it was
    // written to Postgres.
    assert_eq!(
        accrued_minutes(
            entitlement,
            policy.accrual,
            year.0,
            day(2026, Month::June, 30)
        ),
        entitlement * 6 / 12
    );
}
