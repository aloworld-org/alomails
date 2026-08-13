//! The public booking flow against a real database: what a publish freezes,
//! what a visitor is offered, what happens when two of them want the same
//! quarter of an hour, and what a stranger can reach (nothing).
//!
//! The slot arithmetic itself is unit-tested in `site_booking_slots`; this
//! suite proves the parts only Postgres can settle — the freeze, the calendar
//! subtraction, the race, and the tenant boundary.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    AccountStore, BookingRequest, CalendarEvent, CalendarId, EventId, PublicBookingService,
    SiteBookingField, SiteBookingFieldKind, SiteBookingInput, SiteBookingWindow, SiteId,
    SitePublicStore, StoreError,
};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use time::{Date, Month, OffsetDateTime};

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

/// A Wednesday, in a year the tests can stand anywhere in.
fn wednesday() -> Date {
    Date::from_calendar_date(2026, Month::September, 16).unwrap()
}

/// Long before that Wednesday: nothing is inside anyone's notice period.
fn asking_at() -> OffsetDateTime {
    Date::from_calendar_date(2026, Month::September, 1)
        .unwrap()
        .with_hms(8, 0, 0)
        .unwrap()
        .assume_utc()
}

fn utc(day: u8, hour: u8, minute: u8) -> OffsetDateTime {
    Date::from_calendar_date(2026, Month::September, day)
        .unwrap()
        .with_hms(hour, minute, 0)
        .unwrap()
        .assume_utc()
}

/// One tenant with a published site offering one bookable service, plus the
/// public door the anonymous service would use.
struct Published {
    account: AccountStore,
    site: SiteId,
    calendar: CalendarId,
    booking_id: String,
    subdomain: String,
    public: SitePublicStore,
    /// A pool of this test's own, for the raw reads that check what actually
    /// landed in the ledger.
    pool: sqlx::PgPool,
}

async fn published_service(tag: &str, fields: &[SiteBookingField], active: bool) -> Published {
    let (store, blobs) = common::test_store_with_blobs().await;
    let tenant = store.create_tenant(tag).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("owner@{tag}.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let site_subdomain = subdomain(tag);
    let site = account
        .create_site("Studio", &site_subdomain)
        .await
        .unwrap();
    let home = account
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    let calendar = account.ensure_personal_calendar().await.unwrap();
    // Wednesday 09:00–11:00 in Brussels, half an hour each, no buffer: four
    // slots, at 07:00, 07:30, 08:00 and 08:30 UTC.
    let hours = [window(3, 540, 660)];
    let booking = account
        .create_site_booking(
            &site,
            &SiteBookingInput {
                name: "Consultation",
                description: Some("Half an hour, in the studio."),
                calendar: &calendar,
                time_zone: "Europe/Brussels",
                duration_minutes: 30,
                buffer_minutes: 0,
                notice_minutes: 0,
                horizon_days: 365,
                location: Some("Second floor"),
                hours: &hours,
                fields,
                active,
            },
        )
        .await
        .unwrap();
    account
        .set_page_sections(
            &site,
            &home,
            json!({
                "schema_version": 1,
                "sections": [{
                    "type": "booking",
                    "booking_id": booking.as_str(),
                    "heading": "Come and talk to us"
                }]
            }),
        )
        .await
        .unwrap();
    account.publish_site(&site).await.unwrap();
    let public_pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&common::database_url())
        .await
        .unwrap();
    Published {
        account,
        site,
        calendar,
        booking_id: booking.as_str().to_owned(),
        subdomain: site_subdomain,
        public: SitePublicStore::new(public_pool.clone(), blobs),
        pool: public_pool,
    }
}

async fn resolve(published: &Published) -> PublicBookingService {
    published
        .public
        .public_booking(&published.booking_id)
        .await
        .unwrap()
        .expect("the published service resolves")
}

fn visitor<'a>(starts_at: OffsetDateTime, answers: &'a [(String, String)]) -> BookingRequest<'a> {
    BookingRequest {
        starts_at,
        visitor_name: "Ada Lovelace",
        visitor_email: "ada@example.test",
        answers,
    }
}

#[tokio::test]
async fn publishing_freezes_the_service_and_the_public_door_offers_its_week() {
    let published = published_service("site-booking-publish", &[], true).await;

    // What the publish froze is what the editor set, and it is what the public
    // page reads — not the editable row, which may already say something else.
    let frozen = published
        .public
        .published_bookings(
            &published
                .public
                .resolve_published(&published.subdomain)
                .await
                .unwrap()
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(frozen.len(), 1);
    assert_eq!(frozen[0].name, "Consultation");
    assert_eq!(frozen[0].duration_minutes, 30);
    assert_eq!(frozen[0].hours, vec![window(3, 540, 660)]);

    let service = resolve(&published).await;
    let slots = published
        .public
        .public_booking_slots(&service, wednesday(), asking_at())
        .await
        .unwrap();
    assert_eq!(
        slots.iter().map(|slot| slot.starts_at).collect::<Vec<_>>(),
        vec![utc(16, 7, 0), utc(16, 7, 30), utc(16, 8, 0), utc(16, 8, 30)]
    );

    // Shortening the appointment in the editor does not change the page: a
    // published week is what was promised, until the owner publishes again.
    let hours = [window(3, 540, 660)];
    let editable = published
        .account
        .site_bookings(&published.site)
        .await
        .unwrap();
    published
        .account
        .update_site_booking(
            &published.site,
            &editable[0].id,
            &SiteBookingInput {
                name: "Consultation",
                description: None,
                calendar: &published.calendar,
                time_zone: "Europe/Brussels",
                duration_minutes: 60,
                buffer_minutes: 0,
                notice_minutes: 0,
                horizon_days: 365,
                location: None,
                hours: &hours,
                fields: &[],
                active: true,
            },
        )
        .await
        .unwrap();
    let after = resolve(&published).await;
    assert_eq!(
        after.published.duration_minutes, 30,
        "the publish is immutable"
    );
}

#[tokio::test]
async fn the_owners_calendar_takes_slots_away_and_a_booking_takes_the_next_one() {
    let published = published_service("site-booking-busy", &[], true).await;
    // The owner has a meeting at 09:30–10:00 local (07:30–08:00 UTC).
    published
        .account
        .create_event(&CalendarEvent {
            id: EventId::generate(),
            calendar_id: published.calendar.clone(),
            summary: "Supplier call".to_owned(),
            description: None,
            location: None,
            starts_at: utc(16, 7, 30),
            ends_at: utc(16, 8, 0),
            all_day: false,
            recurrence: None,
            attendees: Vec::new(),
            exdates: Vec::new(),
            recurrence_id: None,
            reminder_minutes: None,
            attendee_status: Vec::new(),
        })
        .await
        .unwrap();

    let service = resolve(&published).await;
    let slots = published
        .public
        .public_booking_slots(&service, wednesday(), asking_at())
        .await
        .unwrap();
    assert_eq!(
        slots.iter().map(|slot| slot.starts_at).collect::<Vec<_>>(),
        vec![utc(16, 7, 0), utc(16, 8, 0), utc(16, 8, 30)],
        "the meeting's half hour is gone, and only that one"
    );

    // A visitor takes the first free time.
    let reserved = published
        .public
        .reserve_public_booking(&service, &visitor(utc(16, 7, 0), &[]), asking_at())
        .await
        .unwrap()
        .expect("the service is bookable");
    assert_eq!(reserved.starts_at, utc(16, 7, 0));
    assert_eq!(reserved.ends_at, utc(16, 7, 30));
    assert_eq!(reserved.time_zone, "Europe/Brussels");

    // It is gone from what the next visitor is offered…
    let after = published
        .public
        .public_booking_slots(&service, wednesday(), asking_at())
        .await
        .unwrap();
    assert_eq!(
        after.iter().map(|slot| slot.starts_at).collect::<Vec<_>>(),
        vec![utc(16, 8, 0), utc(16, 8, 30)]
    );

    // …and it is in the owner's calendar, as an event they can see and move.
    let events = published
        .account
        .events_in_range(utc(16, 0, 0), utc(17, 0, 0))
        .await
        .unwrap();
    let booked = events
        .iter()
        .find(|event| event.starts_at == utc(16, 7, 0))
        .expect("the appointment reached the calendar");
    assert!(booked.summary.contains("Consultation"), "{booked:?}");
    assert!(booked.summary.contains("Ada Lovelace"), "{booked:?}");
    assert!(
        booked
            .description
            .as_deref()
            .is_some_and(|text| text.contains("ada@example.test")),
        "the owner can answer the visitor: {booked:?}"
    );
}

#[tokio::test]
async fn two_visitors_wanting_one_slot_produce_exactly_one_appointment() {
    let published = published_service("site-booking-race", &[], true).await;
    let service = resolve(&published).await;

    // Six visitors press *book* on the same quarter of an hour at once.
    let mut racing = Vec::new();
    for _ in 0..6 {
        let public = published.public.clone();
        let service = service.clone();
        racing.push(tokio::spawn(async move {
            public
                .reserve_public_booking(&service, &visitor(utc(16, 7, 0), &[]), asking_at())
                .await
        }));
    }
    let mut won = 0;
    let mut lost = 0;
    for handle in racing {
        match handle.await.unwrap() {
            Ok(Some(_)) => won += 1,
            Err(StoreError::Conflict(said)) => {
                assert!(said.contains("taken") || said.contains("free"), "{said}");
                lost += 1;
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }
    assert_eq!(won, 1, "exactly one visitor may have the slot");
    assert_eq!(lost, 5);

    // And the ledger agrees: one live appointment, one calendar event.
    let slots = published
        .public
        .public_booking_slots(&service, wednesday(), asking_at())
        .await
        .unwrap();
    assert_eq!(slots.len(), 3, "only the taken slot disappeared");
    let events = published
        .account
        .events_in_range(utc(16, 0, 0), utc(17, 0, 0))
        .await
        .unwrap();
    assert_eq!(events.len(), 1, "one appointment, one event");
}

#[tokio::test]
async fn a_time_that_is_not_offered_is_refused_however_it_is_asked_for() {
    let published = published_service("site-booking-refuse", &[], true).await;
    let service = resolve(&published).await;

    // Outside the published window.
    let outside = published
        .public
        .reserve_public_booking(&service, &visitor(utc(16, 12, 0), &[]), asking_at())
        .await;
    assert!(
        matches!(outside, Err(StoreError::Conflict(_))),
        "{outside:?}"
    );
    // Inside the window but not on the grid the page offered.
    let off_grid = published
        .public
        .reserve_public_booking(&service, &visitor(utc(16, 7, 10), &[]), asking_at())
        .await;
    assert!(
        matches!(off_grid, Err(StoreError::Conflict(_))),
        "{off_grid:?}"
    );
    // A day the owner is closed.
    let closed = published
        .public
        .reserve_public_booking(&service, &visitor(utc(15, 7, 0), &[]), asking_at())
        .await;
    assert!(matches!(closed, Err(StoreError::Conflict(_))), "{closed:?}");
    // And a visitor without a usable address is refused before any of that.
    let nameless = published
        .public
        .reserve_public_booking(
            &service,
            &BookingRequest {
                starts_at: utc(16, 7, 0),
                visitor_name: "  ",
                visitor_email: "ada@example.test",
                answers: &[],
            },
            asking_at(),
        )
        .await;
    assert!(
        matches!(nameless, Err(StoreError::Validation(_))),
        "{nameless:?}"
    );
}

#[tokio::test]
async fn the_services_own_questions_are_asked_answered_and_stored() {
    let fields = [
        SiteBookingField {
            key: "phone".to_owned(),
            label: "Phone number".to_owned(),
            kind: SiteBookingFieldKind::Phone,
            required: true,
            options: Vec::new(),
        },
        SiteBookingField {
            key: "treatment".to_owned(),
            label: "Which treatment?".to_owned(),
            kind: SiteBookingFieldKind::Choice,
            required: false,
            options: vec!["Cut".to_owned(), "Colour".to_owned()],
        },
    ];
    let published = published_service("site-booking-answers", &fields, true).await;
    let service = resolve(&published).await;
    assert_eq!(service.published.fields.len(), 2);

    // The required question must be answered…
    let unanswered = published
        .public
        .reserve_public_booking(&service, &visitor(utc(16, 7, 0), &[]), asking_at())
        .await;
    match unanswered {
        Err(StoreError::Validation(said)) => assert!(said.contains("Phone number"), "{said}"),
        other => panic!("expected a named refusal, got {other:?}"),
    }
    // …and a choice must be one of the offered answers.
    let invented = published
        .public
        .reserve_public_booking(
            &service,
            &visitor(
                utc(16, 7, 0),
                &[
                    ("phone".to_owned(), "+32 2 555 01".to_owned()),
                    ("treatment".to_owned(), "Shave".to_owned()),
                ],
            ),
            asking_at(),
        )
        .await;
    match invented {
        Err(StoreError::Validation(said)) => assert!(said.contains("Which treatment?"), "{said}"),
        other => panic!("expected a named refusal, got {other:?}"),
    }

    published
        .public
        .reserve_public_booking(
            &service,
            &visitor(
                utc(16, 7, 0),
                &[
                    ("phone".to_owned(), " +32 2 555 01 ".to_owned()),
                    ("treatment".to_owned(), "Colour".to_owned()),
                ],
            ),
            asking_at(),
        )
        .await
        .unwrap()
        .expect("the answered booking is taken");

    // The answers reached the row, labelled as the visitor read them.
    let stored: (String, String, serde_json::Value) = sqlx::query_as(
        "SELECT visitor_name, visitor_email, answers FROM site_booking_appointments \
         WHERE tenant_id = $1 AND site_id = $2",
    )
    .bind(published.account.tenant().as_str())
    .bind(published.site.as_str())
    .fetch_one(&published.pool)
    .await
    .unwrap();
    assert_eq!(stored.0, "Ada Lovelace");
    assert_eq!(stored.1, "ada@example.test");
    assert_eq!(
        stored.2,
        json!([
            {"key": "phone", "label": "Phone number", "value": "+32 2 555 01"},
            {"key": "treatment", "label": "Which treatment?", "value": "Colour"}
        ])
    );
}

#[tokio::test]
async fn nothing_is_bookable_that_is_not_published_live_and_switched_on() {
    // A service published switched off is one absence like any other: it does
    // not resolve, so neither the day page nor the reservation can reach it.
    // The published page still shows it — with the sentence that says it takes
    // no bookings — because that snapshot is read by a different door.
    let asleep = published_service("site-booking-asleep", &[], false).await;
    assert!(
        asleep
            .public
            .public_booking(&asleep.booking_id)
            .await
            .unwrap()
            .is_none(),
        "a service switched off is not bookable"
    );
    let live_page = asleep
        .public
        .resolve_published(&asleep.subdomain)
        .await
        .unwrap()
        .unwrap();
    let shown = asleep.public.published_bookings(&live_page).await.unwrap();
    assert_eq!(shown.len(), 1);
    assert!(!shown[0].active, "the page can say it is closed");

    // An id that is not a service at all, and one shaped like an attack, are
    // the same clean absence.
    for id in ["", "nope", "b\'; drop table sites; --", &"x".repeat(200)] {
        assert!(asleep.public.public_booking(id).await.unwrap().is_none());
    }

    // A live service stops resolving the moment its site is unpublished.
    let live = published_service("site-booking-live", &[], true).await;
    assert!(
        live.public
            .public_booking(&live.booking_id)
            .await
            .unwrap()
            .is_some()
    );
    live.account.unpublish_site(&live.site).await.unwrap();
    assert!(
        live.public
            .public_booking(&live.booking_id)
            .await
            .unwrap()
            .is_none(),
        "an unpublished site offers nothing"
    );
}

#[tokio::test]
async fn another_tenant_can_neither_see_nor_take_a_booking() {
    let ours = published_service("site-booking-mine", &[], true).await;
    let theirs = published_service("site-booking-yours", &[], true).await;

    // The rival's editor door cannot reach our service, our site, or our
    // publish's snapshots — by id, from their own tenant.
    let our_bookings = ours.account.site_bookings(&ours.site).await.unwrap();
    assert!(
        theirs
            .account
            .site_booking(&ours.site, &our_bookings[0].id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        theirs
            .account
            .site_bookings(&ours.site)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        theirs
            .account
            .site_booking_preview(&ours.site, &our_bookings[0].id)
            .await,
        Err(StoreError::NotFound)
    ));

    // A booking taken on our site lands in our tenant and nowhere near theirs.
    let service = resolve(&ours).await;
    ours.public
        .reserve_public_booking(&service, &visitor(utc(16, 7, 0), &[]), asking_at())
        .await
        .unwrap()
        .unwrap();
    let mine: i64 =
        sqlx::query_scalar("SELECT count(*) FROM site_booking_appointments WHERE tenant_id = $1")
            .bind(ours.account.tenant().as_str())
            .fetch_one(&ours.pool)
            .await
            .unwrap();
    let yours: i64 =
        sqlx::query_scalar("SELECT count(*) FROM site_booking_appointments WHERE tenant_id = $1")
            .bind(theirs.account.tenant().as_str())
            .fetch_one(&ours.pool)
            .await
            .unwrap();
    assert_eq!((mine, yours), (1, 0));

    // And the rival's calendar is untouched: the event went to our owner.
    assert!(
        theirs
            .account
            .events_in_range(utc(16, 0, 0), utc(17, 0, 0))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn an_appointment_stores_nothing_about_the_visitors_connection() {
    let published = published_service("site-booking-privacy", &[], true).await;
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_name = 'site_booking_appointments'",
    )
    .fetch_all(&published.pool)
    .await
    .unwrap();
    for forbidden in [
        "ip",
        "ip_address",
        "user_agent",
        "referrer",
        "referer",
        "session",
        "fingerprint",
    ] {
        assert!(
            !columns.iter().any(|column| column == forbidden),
            "the appointment ledger must not carry {forbidden}"
        );
    }
    // What it does carry is what the owner needs to keep the appointment.
    for expected in ["visitor_name", "visitor_email", "starts_at", "answers"] {
        assert!(
            columns.iter().any(|column| column == expected),
            "missing {expected}"
        );
    }
}
