//! Tenancy and door proofs for the employee record and the employments under
//! it (alo HR, B6.02a — Law 1: isolation is tested, not assumed).
//!
//! `docs/design/hr.md` ("Tenancy") makes **four** tests mandatory before this
//! item is done, two more than any previous wave, and each has a test here:
//!
//! - **wrong tenant** — tenant A's handle cannot read, list, edit, archive or
//!   append terms to tenant B's employee, and gets the clean `NotFound`/empty
//!   rather than data or a 500;
//! - **wrong user** — a colleague's record is not reachable through the own
//!   door, inside the same tenant;
//! - **wrong role** — the projection a non-HR caller can obtain carries **no**
//!   private field, asserted by enumerating the values rather than by trusting
//!   the projection;
//! - **wrong manager** — the reporting line is a tree: the cycle a chart could
//!   close is refused on write, on a three-level chart.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::hr_employments::{ContractKind, FULL_TIME_PATTERN, NewEmployment, PayPeriod};
use alo_store::{
    AccountStore, HrEmployeeId, NewEmployee, Store, StoreError, TenantId, TenantStore, UserId,
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

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).expect("a real date")
}

/// The private values planted in the record below. Every one of them is a
/// string a directory response must never contain.
const PRIVATE_VALUES: [&str; 8] = [
    "Rue du Marché 14",
    "bte 3",
    "Bruxelles",
    "79061234567",
    "BE68539007547034",
    "ines.privee@example.test",
    "+32 470 00 00 00",
    "Papa Dupont",
];

/// A fully-specified employee, so a round trip covers every column — and so the
/// private-field test has something real to look for.
fn ines(user: Option<UserId>) -> NewEmployee {
    NewEmployee {
        user_id: user,
        staff_number: Some("E-0042".to_owned()),
        given_name: "Inès".to_owned(),
        family_name: "Dupont".to_owned(),
        preferred_name: "Nes".to_owned(),
        work_email: Some("ines.dupont@example.test".to_owned()),
        work_phone: "+32 2 555 00 11".to_owned(),
        personal_email: Some("ines.privee@example.test".to_owned()),
        personal_phone: "+32 470 00 00 00".to_owned(),
        date_of_birth: Some(day(1988, Month::March, 2)),
        address_line1: "Rue du Marché 14".to_owned(),
        address_line2: "bte 3".to_owned(),
        postal_code: "1000".to_owned(),
        city: "Bruxelles".to_owned(),
        region: "Bruxelles-Capitale".to_owned(),
        country: "be".to_owned(),
        national_id: Some("79061234567".to_owned()),
        iban: Some("BE68 5390 0754 7034".to_owned()),
        emergency_name: "Papa Dupont".to_owned(),
        emergency_phone: "+32 2 555 99 88".to_owned(),
        ..Default::default()
    }
}

fn terms(started_on: Date) -> NewEmployment {
    NewEmployment {
        job_title: "Vertegenwoordiger".to_owned(),
        team: "Sales".to_owned(),
        contract_kind: ContractKind::Permanent,
        started_on,
        pattern_minutes: FULL_TIME_PATTERN,
        pay_amount_cents: Some(320_000),
        pay_period: PayPeriod::Month,
        ..Default::default()
    }
}

/// A tenant with one user: the tenant door (HR's), the account door (theirs),
/// and both ids.
async fn tenant_with_user(
    store: &Store,
    tag: &str,
) -> (TenantStore, AccountStore, TenantId, UserId) {
    let tenant = store.create_tenant(&format!("hr-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@people.test"))
        .await
        .unwrap();
    (
        store.for_tenant(tenant.clone()),
        store.for_account(tenant.clone(), user.clone()),
        tenant,
        user,
    )
}

/// **Wrong tenant.** Every path an outsider could take to somebody else's
/// employee record ends in the same clean denial.
#[tokio::test]
async fn another_tenants_employee_is_unreachable_by_every_path() {
    let store = common::test_store().await;
    let (hr_a, _acc_a, _tenant_a, user_a) = tenant_with_user(&store, "own").await;
    let (hr_b, acc_b, _tenant_b, user_b) = tenant_with_user(&store, "other").await;

    let employee = hr_a.create_hr_employee(&ines(None), &user_a).await.unwrap();
    hr_a.append_hr_employment(&employee, &terms(day(2026, Month::January, 1)), &user_a)
        .await
        .unwrap();

    // Reads: nothing, and nothing that says the record exists.
    assert!(hr_b.hr_employee(&employee).await.unwrap().is_none());
    assert!(hr_b.hr_directory(true).await.unwrap().is_empty());
    assert!(acc_b.hr_directory().await.unwrap().is_empty());
    assert!(acc_b.my_hr_employee().await.unwrap().is_none());
    assert!(acc_b.my_hr_employments().await.unwrap().is_empty());
    assert_not_found(hr_b.hr_employments(&employee).await);
    assert_not_found(
        hr_b.hr_employment_on(&employee, day(2026, Month::June, 1))
            .await,
    );

    // Writes: refused, and the record is untouched afterwards.
    assert_not_found(hr_b.update_hr_employee(&employee, &ines(None)).await);
    assert_not_found(hr_b.set_hr_employee_archived(&employee, true).await);
    assert_not_found(
        hr_b.append_hr_employment(&employee, &terms(day(2027, Month::January, 1)), &user_b)
            .await,
    );
    assert_not_found(
        hr_b.end_hr_employment(&employee, day(2026, Month::June, 30))
            .await,
    );

    let still_there = hr_a.hr_employee(&employee).await.unwrap().unwrap();
    assert!(!still_there.is_archived(), "an outsider changed nothing");
    assert_eq!(still_there.national_id.as_deref(), Some("79061234567"));
    assert_eq!(hr_a.hr_employments(&employee).await.unwrap().len(), 1);

    // A manager link cannot cross the boundary either: tenant B naming tenant
    // A's employee as a manager is the same denial, not a foreign edge.
    let b_employee = hr_b
        .create_hr_employee(
            &NewEmployee {
                given_name: "Bram".to_owned(),
                family_name: "Peeters".to_owned(),
                ..Default::default()
            },
            &user_b,
        )
        .await
        .unwrap();
    assert_not_found(
        hr_b.update_hr_employee(
            &b_employee,
            &NewEmployee {
                given_name: "Bram".to_owned(),
                family_name: "Peeters".to_owned(),
                manager_id: Some(employee.clone()),
                ..Default::default()
            },
        )
        .await,
    );
}

/// **Wrong user.** Inside one tenant, the own door answers with the caller's
/// own record and there is no argument that could ask for a colleague's.
#[tokio::test]
async fn the_own_door_answers_only_about_its_own_caller() {
    let store = common::test_store().await;
    let (hr, acc_ines, tenant, user_ines) = tenant_with_user(&store, "self").await;
    let colleague = store
        .for_tenant(tenant.clone())
        .create_user("bram@people.test")
        .await
        .unwrap();
    let acc_bram = store.for_account(tenant.clone(), colleague.clone());

    let ines_id = hr
        .create_hr_employee(&ines(Some(user_ines.clone())), &user_ines)
        .await
        .unwrap();
    hr.append_hr_employment(&ines_id, &terms(day(2026, Month::January, 1)), &user_ines)
        .await
        .unwrap();
    let bram_id = hr
        .create_hr_employee(
            &NewEmployee {
                user_id: Some(colleague.clone()),
                given_name: "Bram".to_owned(),
                family_name: "Peeters".to_owned(),
                ..Default::default()
            },
            &user_ines,
        )
        .await
        .unwrap();

    let mine = acc_ines.my_hr_employee().await.unwrap().unwrap();
    assert_eq!(mine.id.as_str(), ines_id.as_str());
    assert_eq!(mine.national_id.as_deref(), Some("79061234567"));
    assert_eq!(mine.display_name(), "Nes Dupont", "called what she asked");

    // Bram's door answers about Bram — with nothing of hers, including the
    // employment history and its pay figure.
    let his = acc_bram.my_hr_employee().await.unwrap().unwrap();
    assert_eq!(his.id.as_str(), bram_id.as_str());
    assert_eq!(his.national_id, None);
    assert!(acc_bram.my_hr_employments().await.unwrap().is_empty());
    let hers = acc_ines.my_hr_employments().await.unwrap();
    assert_eq!(hers.len(), 1);
    assert_eq!(hers[0].pay_amount_cents, Some(320_000));

    // A user with no employee record is ordinary, not an error.
    let stranger = store
        .for_tenant(tenant.clone())
        .create_user("admin@people.test")
        .await
        .unwrap();
    let acc_stranger = store.for_account(tenant, stranger);
    assert!(acc_stranger.my_hr_employee().await.unwrap().is_none());
}

/// **Wrong role.** The projection every member can obtain carries no private
/// field — asserted by looking for each planted value, because a test that
/// trusted the projection would pass on the day somebody widened it.
#[tokio::test]
async fn no_private_field_reaches_the_directory_either_door() {
    let store = common::test_store().await;
    let (hr, acc, tenant, user) = tenant_with_user(&store, "dir").await;
    let colleague = store
        .for_tenant(tenant.clone())
        .create_user("bram@people.test")
        .await
        .unwrap();
    let acc_colleague = store.for_account(tenant, colleague);

    let employee = hr
        .create_hr_employee(&ines(Some(user.clone())), &user)
        .await
        .unwrap();
    hr.append_hr_employment(&employee, &terms(day(2026, Month::January, 1)), &user)
        .await
        .unwrap();

    for entries in [
        acc.hr_directory().await.unwrap(),
        acc_colleague.hr_directory().await.unwrap(),
        hr.hr_directory(true).await.unwrap(),
    ] {
        assert_eq!(entries.len(), 1);
        let rendered = format!("{entries:?}");
        for private in PRIVATE_VALUES {
            assert!(
                !rendered.contains(private),
                "a private value reached the directory: {private}"
            );
        }
        // What it DOES carry: the facts a colleague may look up, including the
        // job title and team, which come from the employment in force.
        let entry = &entries[0];
        assert_eq!(entry.display_name(), "Nes Dupont");
        assert_eq!(entry.job_title, "Vertegenwoordiger");
        assert_eq!(entry.team, "Sales");
        assert_eq!(entry.started_on, Some(day(2026, Month::January, 1)));
        assert_eq!(
            entry.work_email.as_deref(),
            Some("ines.dupont@example.test")
        );
    }

    // And the record read behind the HR door does carry them — the point of
    // the door, and proof the values were really stored.
    let record = hr.hr_employee(&employee).await.unwrap().unwrap();
    assert_eq!(record.address_line1, "Rue du Marché 14");
    assert_eq!(record.iban.as_deref(), Some("BE68539007547034"));
    assert_eq!(record.date_of_birth, Some(day(1988, Month::March, 2)));
    assert_eq!(record.country, "BE");
}

/// **Wrong manager.** The reporting line is a tree, proven on a three-level
/// chart: neither the direct link back, nor the one through the middle, nor
/// self-management can be written.
#[tokio::test]
async fn a_reporting_line_can_never_close_a_cycle() {
    let store = common::test_store().await;
    let (hr, _acc, _tenant, user) = tenant_with_user(&store, "org").await;

    let named = |name: &str, manager: Option<HrEmployeeId>| NewEmployee {
        given_name: name.to_owned(),
        family_name: "Chart".to_owned(),
        manager_id: manager,
        ..Default::default()
    };
    let top = hr
        .create_hr_employee(&named("Top", None), &user)
        .await
        .unwrap();
    let middle = hr
        .create_hr_employee(&named("Middle", Some(top.clone())), &user)
        .await
        .unwrap();
    let leaf = hr
        .create_hr_employee(&named("Leaf", Some(middle.clone())), &user)
        .await
        .unwrap();

    let cycle = |result: Result<(), StoreError>, expect: &str| match result {
        Err(StoreError::Validation(message)) => assert!(
            message.contains(expect),
            "expected a message naming {expect}: {message}"
        ),
        other => panic!("expected Validation, got {other:?}"),
    };

    // Top reporting to the leaf closes the loop through the middle.
    cycle(
        hr.update_hr_employee(&top, &named("Top", Some(leaf.clone())))
            .await,
        top.as_str(),
    );
    // The shorter loop, and the shortest of all.
    cycle(
        hr.update_hr_employee(&top, &named("Top", Some(middle.clone())))
            .await,
        top.as_str(),
    );
    cycle(
        hr.update_hr_employee(&top, &named("Top", Some(top.clone())))
            .await,
        "own manager",
    );
    // Nothing was written by any of the refusals.
    let unchanged = hr.hr_employee(&top).await.unwrap().unwrap();
    assert_eq!(unchanged.manager_id, None);

    // A manager id that is not this tenant's employee at all is a not-found,
    // never a dangling edge.
    assert_not_found(
        hr.update_hr_employee(
            &leaf,
            &named("Leaf", Some(HrEmployeeId::new("nope".to_owned()))),
        )
        .await,
    );

    // The links that ARE a tree stay writable: moving the leaf up a level.
    hr.update_hr_employee(&leaf, &named("Leaf", Some(top.clone())))
        .await
        .unwrap();
    let moved = hr.hr_employee(&leaf).await.unwrap().unwrap();
    assert_eq!(moved.manager_id, Some(top.clone()));
}

/// The claims that are one-per-tenant say which claim clashed, and never quote
/// what the other record holds.
#[tokio::test]
async fn a_staff_number_and_a_login_are_claimed_once_each() {
    let store = common::test_store().await;
    let (hr, _acc, tenant, user) = tenant_with_user(&store, "claims").await;
    hr.create_hr_employee(&ines(Some(user.clone())), &user)
        .await
        .unwrap();

    let second_user = store
        .for_tenant(tenant.clone())
        .create_user("bram@people.test")
        .await
        .unwrap();
    let same_number = NewEmployee {
        user_id: Some(second_user.clone()),
        staff_number: Some("E-0042".to_owned()),
        given_name: "Bram".to_owned(),
        family_name: "Peeters".to_owned(),
        ..Default::default()
    };
    match hr.create_hr_employee(&same_number, &user).await {
        Err(StoreError::Conflict(message)) => {
            assert!(message.contains("staff number"), "{message}");
            assert!(!message.contains("Dupont"), "never names the holder");
        }
        other => panic!("expected Conflict, got {other:?}"),
    }

    let same_login = NewEmployee {
        user_id: Some(user.clone()),
        given_name: "Bram".to_owned(),
        family_name: "Peeters".to_owned(),
        ..Default::default()
    };
    match hr.create_hr_employee(&same_login, &user).await {
        Err(StoreError::Conflict(message)) => assert!(message.contains("employee record")),
        other => panic!("expected Conflict, got {other:?}"),
    }

    // A user of another tenant can never become this tenant's employee.
    let (_hr_other, _acc_other, _tenant_other, outsider) =
        tenant_with_user(&store, "claims-other").await;
    assert_not_found(
        hr.create_hr_employee(
            &NewEmployee {
                user_id: Some(outsider),
                given_name: "Nobody".to_owned(),
                family_name: "Here".to_owned(),
                ..Default::default()
            },
            &user,
        )
        .await,
    );
    // Two people without a staff number and without a login are ordinary.
    for name in ["Anke", "Joris"] {
        hr.create_hr_employee(
            &NewEmployee {
                given_name: name.to_owned(),
                family_name: "Seasonal".to_owned(),
                ..Default::default()
            },
            &user,
        )
        .await
        .unwrap();
    }
}

/// Terms are appended: the running period closes the day before the next one
/// starts, and a date-bound read gets the terms that were in force **then**.
#[tokio::test]
async fn employment_history_is_appended_and_answers_by_date() {
    let store = common::test_store().await;
    let (hr, _acc, _tenant, user) = tenant_with_user(&store, "terms").await;
    let employee = hr.create_hr_employee(&ines(None), &user).await.unwrap();

    hr.append_hr_employment(&employee, &terms(day(2024, Month::April, 1)), &user)
        .await
        .unwrap();
    // A move to three days a week, from the first of July.
    let part_time = NewEmployment {
        job_title: "Vertegenwoordiger".to_owned(),
        team: "Sales".to_owned(),
        contract_kind: ContractKind::PartTime,
        started_on: day(2026, Month::July, 1),
        pattern_minutes: [480, 480, 480, 0, 0, 0, 0],
        pay_amount_cents: Some(192_000),
        ..Default::default()
    };
    hr.append_hr_employment(&employee, &part_time, &user)
        .await
        .unwrap();

    let history = hr.hr_employments(&employee).await.unwrap();
    assert_eq!(history.len(), 2, "appended, not edited");
    assert!(history[0].is_open(), "newest first, and it is the open one");
    assert_eq!(history[1].ended_on, Some(day(2026, Month::June, 30)));
    assert_eq!(history[1].pay_amount_cents, Some(320_000), "history kept");

    // The whole point: the terms in force THEN, not the ones in force now.
    let before = hr
        .hr_employment_on(&employee, day(2026, Month::June, 30))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(before.weekly_minutes(), 2_400, "a five-day week");
    let after = hr
        .hr_employment_on(&employee, day(2026, Month::July, 1))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.weekly_minutes(), 1_440, "a three-day week");
    assert_eq!(after.contract_kind, ContractKind::PartTime);
    assert!(
        hr.hr_employment_on(&employee, day(2020, Month::January, 1))
            .await
            .unwrap()
            .is_none(),
        "nobody is employed before they started"
    );

    // Back-dating a change is refused: it would restate balances already folded
    // from the terms it replaced.
    match hr
        .append_hr_employment(&employee, &terms(day(2025, Month::January, 1)), &user)
        .await
    {
        Err(StoreError::Validation(message)) => assert!(message.contains("start after")),
        other => panic!("expected Validation, got {other:?}"),
    }

    // A leaver: the running period ends, and a second attempt has nothing left
    // to end rather than re-stamping a date payroll may already have used.
    hr.end_hr_employment(&employee, day(2026, Month::December, 31))
        .await
        .unwrap();
    assert!(
        hr.hr_employment_on(&employee, day(2027, Month::January, 2))
            .await
            .unwrap()
            .is_none()
    );
    assert_not_found(
        hr.end_hr_employment(&employee, day(2027, Month::January, 31))
            .await,
    );
}

/// Archiving is the only removal, it refuses to cut a branch off the chart, and
/// what it hides from the directory it keeps for HR.
#[tokio::test]
async fn archiving_hides_a_person_without_losing_them() {
    let store = common::test_store().await;
    let (hr, acc, _tenant, user) = tenant_with_user(&store, "archive").await;

    let manager = hr
        .create_hr_employee(
            &NewEmployee {
                given_name: "Marta".to_owned(),
                family_name: "Baas".to_owned(),
                ..Default::default()
            },
            &user,
        )
        .await
        .unwrap();
    let report = hr
        .create_hr_employee(
            &NewEmployee {
                given_name: "Bram".to_owned(),
                family_name: "Peeters".to_owned(),
                manager_id: Some(manager.clone()),
                ..Default::default()
            },
            &user,
        )
        .await
        .unwrap();

    // Somebody with reports cannot be archived out from under them.
    match hr.set_hr_employee_archived(&manager, true).await {
        Err(StoreError::Conflict(message)) => {
            assert!(message.contains('1'), "names the count: {message}");
            assert!(!message.contains("Peeters"), "never names the report");
        }
        other => panic!("expected Conflict, got {other:?}"),
    }

    // Reassign, then archive.
    hr.update_hr_employee(
        &report,
        &NewEmployee {
            given_name: "Bram".to_owned(),
            family_name: "Peeters".to_owned(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    hr.set_hr_employee_archived(&manager, true).await.unwrap();
    hr.set_hr_employee_archived(&manager, true)
        .await
        .expect("idempotent");

    let members_see: Vec<String> = acc
        .hr_directory()
        .await
        .unwrap()
        .into_iter()
        .map(|entry| entry.display_name())
        .collect();
    assert_eq!(members_see, vec!["Bram Peeters".to_owned()], "she is gone");
    let hr_sees = hr.hr_directory(true).await.unwrap();
    assert_eq!(hr_sees.len(), 2, "and still readable through the HR door");
    assert!(
        hr_sees.iter().any(|entry| entry.archived),
        "marked as archived, because retention outlives the employment"
    );
    assert!(
        hr.hr_employee(&manager)
            .await
            .unwrap()
            .unwrap()
            .is_archived(),
        "the record itself is intact"
    );

    // Restoring is the same switch the other way.
    hr.set_hr_employee_archived(&manager, false).await.unwrap();
    assert_eq!(acc.hr_directory().await.unwrap().len(), 2);
}
