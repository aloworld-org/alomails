//! The availability seam against a real database (ADR 0040 §4): what Agenda
//! will say to an anonymous caller — spans, and nothing else — and what a
//! published site offers appointments for at site level, without a
//! page-carried service id.
//!
//! The slot arithmetic is unit-tested in `site_booking_slots` and the booking
//! flow itself in `site_bookings_public`; this suite proves the seam — the
//! expansion riding Agenda's own reads, the calendar/owner/tenant scoping of
//! the busy answer, and the publish scoping of the site-level list.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, CalendarAvailability, CalendarEvent, CalendarId, EventId, SiteBookingId,
    SiteBookingInput, SiteBookingWindow, SiteId, SitePublicStore, Store,
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

fn event(calendar: &CalendarId, summary: &str, starts_at: OffsetDateTime) -> CalendarEvent {
    CalendarEvent {
        id: EventId::generate(),
        calendar_id: calendar.clone(),
        summary: summary.to_owned(),
        description: Some("Notes nobody outside may read".to_owned()),
        location: None,
        starts_at,
        ends_at: starts_at + time::Duration::minutes(30),
        all_day: false,
        recurrence: None,
        attendees: Vec::new(),
        exdates: Vec::new(),
        timezone: None,
        rdates: Vec::new(),
        recurrence_id: None,
        reminder_minutes: None,
        attendee_status: Vec::new(),
    }
}

/// One tenant with an owner account, a site, and the doors the anonymous
/// service would hold.
struct Owned {
    account: AccountStore,
    store: Store,
    blobs: alo_store::BlobStore,
    site: SiteId,
    subdomain: String,
    calendar: CalendarId,
    public: SitePublicStore,
    pool: sqlx::PgPool,
}

async fn owned_site(tag: &str) -> Owned {
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
    let calendar = account.ensure_personal_calendar().await.unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&common::database_url())
        .await
        .unwrap();
    Owned {
        account,
        store,
        blobs: blobs.clone(),
        site,
        subdomain: site_subdomain,
        calendar,
        public: SitePublicStore::new(pool.clone(), blobs),
        pool,
    }
}

impl Owned {
    /// One bookable half-hour service on `calendar`: Wednesday 09:00–11:00 in
    /// Brussels, no buffer or notice — four slots on the test Wednesday.
    async fn service(&self, name: &str, calendar: &CalendarId, active: bool) -> SiteBookingId {
        let hours = [window(3, 540, 660)];
        self.account
            .create_site_booking(
                &self.site,
                &SiteBookingInput {
                    name,
                    description: None,
                    calendar,
                    time_zone: "Europe/Brussels",
                    duration_minutes: 30,
                    buffer_minutes: 0,
                    notice_minutes: 0,
                    horizon_days: 365,
                    location: None,
                    hours: &hours,
                    fields: &[],
                    active,
                },
            )
            .await
            .unwrap()
    }

    /// Puts one booking section per service on the home page and publishes —
    /// only referenced services are frozen into a publish.
    async fn publish_with(&self, services: &[&SiteBookingId]) {
        let home = match self.account.site_pages(&self.site).await.unwrap().first() {
            Some(page) => page.id.clone(),
            None => self
                .account
                .create_site_page(&self.site, "Home", "", true)
                .await
                .unwrap(),
        };
        let sections: Vec<serde_json::Value> = services
            .iter()
            .map(|booking| {
                json!({
                    "type": "booking",
                    "booking_id": booking.as_str(),
                    "heading": "Come and talk to us"
                })
            })
            .collect();
        self.account
            .set_page_sections(
                &self.site,
                &home,
                json!({ "schema_version": 1, "sections": sections }),
            )
            .await
            .unwrap();
        self.account.publish_site(&self.site).await.unwrap();
    }

    /// The availability door an anonymous path would open for this owner.
    fn availability(&self) -> CalendarAvailability {
        CalendarAvailability::open(
            self.pool.clone(),
            self.blobs.clone(),
            self.account.tenant().clone(),
            self.account.user().clone(),
        )
    }
}

#[tokio::test]
async fn agenda_answers_spans_only_and_expands_its_own_recurrence() {
    let owned = owned_site("seam-spans").await;
    owned
        .account
        .create_event(&event(
            &owned.calendar,
            "Confidential supplier call",
            utc(16, 10, 0),
        ))
        .await
        .unwrap();
    let mut daily = event(&owned.calendar, "Morning stand-up", utc(14, 9, 0));
    daily.recurrence = Some("FREQ=DAILY;COUNT=3".to_owned());
    owned.account.create_event(&daily).await.unwrap();

    let mut spans = owned
        .availability()
        .busy_spans(&owned.calendar, utc(14, 0, 0), utc(17, 0, 0))
        .await
        .unwrap();
    spans.sort_by_key(|span| span.from);
    let times: Vec<(OffsetDateTime, OffsetDateTime)> =
        spans.into_iter().map(|span| (span.from, span.to)).collect();
    assert_eq!(
        times,
        vec![
            (utc(14, 9, 0), utc(14, 9, 30)),
            (utc(15, 9, 0), utc(15, 9, 30)),
            (utc(16, 9, 0), utc(16, 9, 30)),
            (utc(16, 10, 0), utc(16, 10, 30)),
        ],
        "three expanded occurrences and the plain event, as spans"
    );
    // Nothing else to assert about contents: `CalendarBusySpan` has no field a
    // summary, guest or note could travel in. The boundary is the type.
}

#[tokio::test]
async fn busy_spans_are_scoped_to_the_calendar_its_owner_and_their_tenant() {
    let owned = owned_site("seam-scope").await;
    let side = owned
        .account
        .create_calendar("Side projects", None)
        .await
        .unwrap();
    owned
        .account
        .create_event(&event(
            &owned.calendar,
            "On the asked calendar",
            utc(16, 9, 0),
        ))
        .await
        .unwrap();
    owned
        .account
        .create_event(&event(&side, "On the other calendar", utc(16, 9, 0)))
        .await
        .unwrap();

    let spans = owned
        .availability()
        .busy_spans(&owned.calendar, utc(16, 0, 0), utc(17, 0, 0))
        .await
        .unwrap();
    assert_eq!(spans.len(), 1, "the sibling calendar's event stays its own");

    // Another user of the same tenant, never granted the calendar: nothing.
    let colleague = owned
        .store
        .for_tenant(owned.account.tenant().clone())
        .create_user("colleague@seam-scope.test")
        .await
        .unwrap();
    let colleague_door = CalendarAvailability::open(
        owned.pool.clone(),
        owned.blobs.clone(),
        owned.account.tenant().clone(),
        colleague,
    );
    assert!(
        colleague_door
            .busy_spans(&owned.calendar, utc(16, 0, 0), utc(17, 0, 0))
            .await
            .unwrap()
            .is_empty(),
        "an ungranted colleague learns nothing, not even that the calendar exists"
    );

    // Another tenant entirely, naming the first tenant's calendar id: nothing.
    let stranger = owned_site("seam-scope-b").await;
    assert!(
        stranger
            .availability()
            .busy_spans(&owned.calendar, utc(16, 0, 0), utc(17, 0, 0))
            .await
            .unwrap()
            .is_empty(),
        "a foreign tenant's calendar id is indistinguishable from a free week"
    );
}

#[tokio::test]
async fn published_availability_offers_what_the_current_publish_switched_on() {
    let owned = owned_site("seam-list").await;
    let consultation = owned.service("Consultation", &owned.calendar, true).await;
    let asleep = owned.service("Asleep", &owned.calendar, false).await;
    owned.publish_with(&[&consultation, &asleep]).await;

    let site = owned
        .public
        .resolve_published(&owned.subdomain)
        .await
        .unwrap()
        .expect("the site is live");
    let offered = owned.public.published_availability(&site).await.unwrap();
    assert_eq!(offered.len(), 1, "the switched-off service is not offered");
    assert_eq!(offered[0].published.name, "Consultation");
    assert_eq!(offered[0].published.duration_minutes, 30);

    // The listed service is the same resolved shape the booking flow books
    // through: it can answer slots straight away.
    let slots = owned
        .public
        .public_booking_slots(&offered[0], wednesday(), asking_at())
        .await
        .unwrap();
    assert_eq!(slots.len(), 4, "the published week offers its four slots");

    // A service added after the publish stays invisible until a republish.
    let side = owned
        .account
        .create_calendar("Second room", None)
        .await
        .unwrap();
    let newer = owned.service("Walk-in", &side, true).await;
    let offered = owned.public.published_availability(&site).await.unwrap();
    assert_eq!(offered.len(), 1, "a draft-only service is not offered");

    owned.publish_with(&[&consultation, &asleep, &newer]).await;
    let site = owned
        .public
        .resolve_published(&owned.subdomain)
        .await
        .unwrap()
        .expect("still live");
    let names: Vec<String> = owned
        .public
        .published_availability(&site)
        .await
        .unwrap()
        .into_iter()
        .map(|service| service.published.name)
        .collect();
    assert_eq!(
        names,
        vec!["Consultation", "Walk-in"],
        "name order, active only"
    );

    // A calendar deleted since the publish takes its service off the list:
    // nothing bookable, rather than an empty week.
    owned.account.delete_calendar(&side).await.unwrap();
    let names: Vec<String> = owned
        .public
        .published_availability(&site)
        .await
        .unwrap()
        .into_iter()
        .map(|service| service.published.name)
        .collect();
    assert_eq!(names, vec!["Consultation"]);
}

#[tokio::test]
async fn published_availability_is_scoped_to_the_site_and_its_tenant() {
    let ours = owned_site("seam-tenancy-a").await;
    let consultation = ours.service("Consultation", &ours.calendar, true).await;
    ours.publish_with(&[&consultation]).await;

    let theirs = owned_site("seam-tenancy-b").await;
    let massage = theirs.service("Massage", &theirs.calendar, true).await;
    theirs.publish_with(&[&massage]).await;

    // Each resolved site lists exactly its own tenant's services — a
    // `PublishedSite` only ever comes out of the Host resolvers, so there is
    // no way to even ask for another tenant's list; what this proves is that
    // the rows behind two sites never bleed into one another.
    let our_site = ours
        .public
        .resolve_published(&ours.subdomain)
        .await
        .unwrap()
        .unwrap();
    let their_site = theirs
        .public
        .resolve_published(&theirs.subdomain)
        .await
        .unwrap()
        .unwrap();
    let our_list = ours.public.published_availability(&our_site).await.unwrap();
    let their_list = theirs
        .public
        .published_availability(&their_site)
        .await
        .unwrap();
    assert_eq!(our_list.len(), 1);
    assert_eq!(
        our_list[0].published.booking_id.as_str(),
        consultation.as_str()
    );
    assert_eq!(their_list.len(), 1);
    assert_eq!(
        their_list[0].published.booking_id.as_str(),
        massage.as_str()
    );
}
