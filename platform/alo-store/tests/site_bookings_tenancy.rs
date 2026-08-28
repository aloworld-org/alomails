//! Tenant, site, and Agenda-source boundaries for Sites booking services.
//!
//! The store's own unit tests cover the week and the questions in isolation;
//! what this suite proves is what only a database can: that a service is
//! reachable from its own tenant and its own site and from nowhere else, that
//! the availability source is resolved through the Agenda seam on every write,
//! and that a calendar which later goes away leaves the service visible rather
//! than vanishing with it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, CalendarId, SITE_BOOKING_MAX_PER_SITE, SiteBookingField, SiteBookingFieldKind,
    SiteBookingInput, SiteBookingWindow, SiteId, StoreError,
};

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

fn validation_of<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(said)) => said,
        other => panic!("expected Validation, got {other:?}"),
    }
}

fn subdomain(tag: &str) -> String {
    format!(
        "{tag}-{}",
        SiteId::generate()
            .as_str()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(12)
            .collect::<String>()
            .to_ascii_lowercase()
    )
}

fn window(weekday: i32, start_minute: i32, end_minute: i32) -> SiteBookingWindow {
    SiteBookingWindow {
        weekday,
        start_minute,
        end_minute,
    }
}

fn input<'a>(
    name: &'a str,
    calendar: &'a CalendarId,
    hours: &'a [SiteBookingWindow],
    fields: &'a [SiteBookingField],
) -> SiteBookingInput<'a> {
    SiteBookingInput {
        name,
        description: None,
        calendar,
        time_zone: "Europe/Brussels",
        duration_minutes: 30,
        buffer_minutes: 10,
        notice_minutes: 120,
        horizon_days: 30,
        location: None,
        hours,
        fields,
        active: true,
    }
}

/// A tenant, a user, a site, and that user's personal calendar.
async fn site_with_calendar(tag: &str, email: &str) -> (AccountStore, SiteId, CalendarId) {
    let store = common::test_store().await;
    let tenant = store.create_tenant(tag).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(email)
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let site = account
        .create_site("Studio", &subdomain(tag))
        .await
        .unwrap();
    let calendar = account.ensure_personal_calendar().await.unwrap();
    (account, site, calendar)
}

#[tokio::test]
async fn a_booking_service_round_trips_its_week_its_questions_and_its_source() {
    let (account, site, calendar) =
        site_with_calendar("site-booking-roundtrip", "owner@site-bookings.test").await;

    // Typed out of order and with a choice question: what comes back is the
    // sorted week and the trimmed answers.
    let hours = [
        window(3, 540, 720),
        window(1, 540, 720),
        window(1, 780, 1_020),
    ];
    let fields = [
        SiteBookingField {
            key: "phone".to_owned(),
            label: "  Phone number  ".to_owned(),
            kind: SiteBookingFieldKind::Phone,
            required: true,
            options: Vec::new(),
        },
        SiteBookingField {
            key: "treatment".to_owned(),
            label: "Which treatment?".to_owned(),
            kind: SiteBookingFieldKind::Choice,
            required: false,
            options: vec![" Cut ".to_owned(), "Colour".to_owned()],
        },
    ];
    let created = account
        .create_site_booking(
            &site,
            &SiteBookingInput {
                description: Some("  Half an hour, in the studio.  "),
                location: Some("Second floor"),
                ..input("Consultation", &calendar, &hours, &fields)
            },
        )
        .await
        .unwrap();

    let stored = account
        .site_booking(&site, &created)
        .await
        .unwrap()
        .expect("the service the tenant just created");
    assert_eq!(stored.name, "Consultation");
    assert_eq!(
        stored.description.as_deref(),
        Some("Half an hour, in the studio.")
    );
    assert_eq!(stored.calendar.as_str(), calendar.as_str());
    assert_eq!(stored.time_zone, "Europe/Brussels");
    assert_eq!(stored.duration_minutes, 30);
    assert_eq!(stored.buffer_minutes, 10);
    assert_eq!(stored.notice_minutes, 120);
    assert_eq!(stored.horizon_days, 30);
    assert!(stored.active);
    assert_eq!(
        stored.hours,
        vec![
            window(1, 540, 720),
            window(1, 780, 1_020),
            window(3, 540, 720)
        ]
    );
    assert_eq!(stored.fields[0].label, "Phone number");
    assert!(stored.fields[0].required);
    assert_eq!(stored.fields[1].options, vec!["Cut", "Colour"]);

    // The source resolves through the seam, with the account's own access.
    let source = account
        .site_availability_source(&stored.calendar)
        .await
        .unwrap()
        .expect("the calendar it was bound to");
    assert!(source.writable);

    // A replace is whole: the questions are gone because they were not sent.
    account
        .update_site_booking(
            &site,
            &created,
            &SiteBookingInput {
                active: false,
                ..input("Long consultation", &calendar, &hours, &[])
            },
        )
        .await
        .unwrap();
    let replaced = account
        .site_booking(&site, &created)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(replaced.name, "Long consultation");
    assert!(replaced.fields.is_empty());
    assert!(!replaced.active);
    assert_eq!(replaced.created_at, stored.created_at);

    let listed = account.site_bookings(&site).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id.as_str(), created.as_str());

    account.delete_site_booking(&site, &created).await.unwrap();
    assert!(
        account
            .site_booking(&site, &created)
            .await
            .unwrap()
            .is_none()
    );
    assert_not_found(account.delete_site_booking(&site, &created).await);
}

#[tokio::test]
async fn another_tenants_booking_service_and_another_tenants_calendar_are_out_of_reach() {
    let (owner, owner_site, owner_calendar) =
        site_with_calendar("site-booking-owner", "owner@site-booking-tenancy.test").await;
    let (stranger, stranger_site, stranger_calendar) = site_with_calendar(
        "site-booking-stranger",
        "stranger@site-booking-tenancy.test",
    )
    .await;

    let hours = [window(2, 540, 720)];
    let booking = owner
        .create_site_booking(&owner_site, &input("Viewing", &owner_calendar, &hours, &[]))
        .await
        .unwrap();

    // The stranger's tenant cannot see it on its own site or on the owner's.
    assert!(
        stranger
            .site_booking(&stranger_site, &booking)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        stranger
            .site_booking(&owner_site, &booking)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        stranger
            .site_bookings(&owner_site)
            .await
            .unwrap()
            .is_empty()
    );
    assert_not_found(
        stranger
            .update_site_booking(
                &owner_site,
                &booking,
                &input("Stolen", &stranger_calendar, &hours, &[]),
            )
            .await,
    );
    assert_not_found(stranger.delete_site_booking(&owner_site, &booking).await);

    // And the owner's calendar is not an availability source anyone else can
    // bind: it does not resolve, so it is a NotFound, not a leak.
    assert!(
        stranger
            .site_availability_source(&owner_calendar)
            .await
            .unwrap()
            .is_none()
    );
    assert_not_found(
        stranger
            .create_site_booking(
                &stranger_site,
                &input("Viewing", &owner_calendar, &hours, &[]),
            )
            .await,
    );

    // The owner's own service survived every one of those attempts intact.
    let intact = owner
        .site_booking(&owner_site, &booking)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(intact.name, "Viewing");
    assert_eq!(intact.calendar.as_str(), owner_calendar.as_str());
}

#[tokio::test]
async fn a_calendar_shared_for_reading_only_cannot_be_booked_into() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("site-booking-readonly").await.unwrap();
    let tenants = store.for_tenant(tenant.clone());
    let owner_user = tenants
        .create_user("owner@site-booking-share.test")
        .await
        .unwrap();
    let colleague_user = tenants
        .create_user("colleague@site-booking-share.test")
        .await
        .unwrap();
    let owner = store.for_account(tenant.clone(), owner_user);
    let colleague = store.for_account(tenant, colleague_user.clone());

    let shared = owner
        .create_calendar("Consulting room", None)
        .await
        .unwrap();
    owner
        .grant_calendar(&shared, "user", colleague_user.as_str(), "viewer")
        .await
        .unwrap();
    let site = colleague
        .create_site("Practice", &subdomain("readonly"))
        .await
        .unwrap();

    // Visible — so the picker can explain it — but refused as a source, by
    // name, because a booking has to be able to write the appointment.
    let source = colleague
        .site_availability_source(&shared)
        .await
        .unwrap()
        .expect("a read-only share is still visible");
    assert!(!source.writable);
    let said = validation_of(
        colleague
            .create_site_booking(
                &site,
                &input("Session", &shared, &[window(1, 540, 720)], &[]),
            )
            .await,
    );
    assert!(said.contains("Consulting room"), "{said}");
    assert!(colleague.site_bookings(&site).await.unwrap().is_empty());

    // Raised to editor, the same write is accepted.
    owner
        .grant_calendar(&shared, "user", colleague_user.as_str(), "editor")
        .await
        .unwrap();
    colleague
        .create_site_booking(
            &site,
            &input("Session", &shared, &[window(1, 540, 720)], &[]),
        )
        .await
        .unwrap();
    assert_eq!(colleague.site_bookings(&site).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_source_that_goes_away_leaves_the_service_visible_and_unresolved() {
    let (account, site, _personal) =
        site_with_calendar("site-booking-lost", "owner@site-booking-lost.test").await;
    let calendar = account.create_calendar("Fittings", None).await.unwrap();
    let booking = account
        .create_site_booking(
            &site,
            &input("Fitting", &calendar, &[window(5, 600, 720)], &[]),
        )
        .await
        .unwrap();

    account.delete_calendar(&calendar).await.unwrap();

    // The service is still the owner's — nothing Agenda does deletes Sites
    // data — but its source no longer resolves, which is what the editor
    // shows as a broken connection.
    let stored = account
        .site_booking(&site, &booking)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.calendar.as_str(), calendar.as_str());
    assert!(
        account
            .site_availability_source(&stored.calendar)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn the_write_gate_refuses_a_broken_shape_without_storing_anything() {
    let (account, site, calendar) =
        site_with_calendar("site-booking-gate", "owner@site-booking-gate.test").await;
    let hours = [window(1, 540, 720)];

    let blank = validation_of(
        account
            .create_site_booking(&site, &input("   ", &calendar, &hours, &[]))
            .await,
    );
    assert!(blank.contains("must not be empty"), "{blank}");

    let zone = validation_of(
        account
            .create_site_booking(
                &site,
                &SiteBookingInput {
                    time_zone: "Middle/Earth",
                    ..input("Consultation", &calendar, &hours, &[])
                },
            )
            .await,
    );
    assert!(zone.contains("Europe/Brussels"), "{zone}");

    let overlap = validation_of(
        account
            .create_site_booking(
                &site,
                &input(
                    "Consultation",
                    &calendar,
                    &[window(1, 540, 720), window(1, 600, 780)],
                    &[],
                ),
            )
            .await,
    );
    assert!(overlap.contains("overlap"), "{overlap}");

    let duration = validation_of(
        account
            .create_site_booking(
                &site,
                &SiteBookingInput {
                    duration_minutes: 0,
                    ..input("Consultation", &calendar, &hours, &[])
                },
            )
            .await,
    );
    assert!(duration.contains("appointment length"), "{duration}");

    // A calendar id that is not anyone's is the same answer as another
    // tenant's: nothing that would tell a caller which of the two it was.
    assert_not_found(
        account
            .create_site_booking(
                &site,
                &input("Consultation", &CalendarId::new("cal-nobody"), &hours, &[]),
            )
            .await,
    );

    assert!(account.site_bookings(&site).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_site_may_offer_only_so_many_services() {
    let (account, site, calendar) =
        site_with_calendar("site-booking-cap", "owner@site-booking-cap.test").await;
    let hours = [window(1, 540, 720)];
    for index in 0..SITE_BOOKING_MAX_PER_SITE {
        account
            .create_site_booking(
                &site,
                &input(&format!("Service {index}"), &calendar, &hours, &[]),
            )
            .await
            .unwrap();
    }
    match account
        .create_site_booking(&site, &input("One too many", &calendar, &hours, &[]))
        .await
    {
        Err(StoreError::Conflict(said)) => {
            assert!(
                said.contains(&SITE_BOOKING_MAX_PER_SITE.to_string()),
                "{said}"
            );
        }
        other => panic!("expected Conflict, got {other:?}"),
    }
    assert_eq!(
        account.site_bookings(&site).await.unwrap().len() as i64,
        SITE_BOOKING_MAX_PER_SITE
    );
}
