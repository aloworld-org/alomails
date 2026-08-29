//! Rooms and resources against the real Postgres: the CRUD, the one rule
//! (a room is in one meeting at a time), and the tenant boundary around both.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::id::{CalendarId, EventId};
use alo_store::model::CalendarEvent;
use alo_store::{CalendarResource, StoreError};
use time::{Date, Month, OffsetDateTime, Time};

fn odt(y: i32, mo: u8, d: u8, h: u8, mi: u8) -> OffsetDateTime {
    OffsetDateTime::new_utc(
        Date::from_calendar_date(y, Month::try_from(mo).unwrap(), d).unwrap(),
        Time::from_hms(h, mi, 0).unwrap(),
    )
}

fn room(name: &str, email: &str) -> CalendarResource {
    CalendarResource {
        id: CalendarId::new(String::new()),
        name: name.to_owned(),
        email: email.to_owned(),
        location: Some("2nd floor".to_owned()),
        capacity: Some(8),
    }
}

fn meeting(cal: &CalendarId, start: OffsetDateTime, end: OffsetDateTime) -> CalendarEvent {
    CalendarEvent {
        id: EventId::new("placeholder"),
        calendar_id: cal.clone(),
        summary: "Meeting".to_owned(),
        description: None,
        location: None,
        starts_at: start,
        ends_at: end,
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

#[tokio::test]
async fn a_resource_round_trips_and_is_reachable_by_its_address() {
    let store = common::test_store().await;
    let (acc, _, _) = common::fresh_account(&store, "res-rt").await;

    assert!(acc.calendar_resources().await.unwrap().is_empty());
    let id = acc
        .create_calendar_resource(&room("Board room", "board@example.test"))
        .await
        .unwrap();

    let listed = acc.calendar_resources().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "Board room");
    assert_eq!(listed[0].capacity, Some(8));
    assert_eq!(listed[0].location.as_deref(), Some("2nd floor"));

    // The address is the handle a meeting names it by, case-insensitively.
    let found = acc
        .calendar_resource_by_email("BOARD@Example.Test")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.id.as_str(), id.as_str());

    acc.update_calendar_resource(
        &id,
        &CalendarResource {
            name: "Board room (big)".to_owned(),
            capacity: Some(12),
            location: None,
            ..room("x", "board@example.test")
        },
    )
    .await
    .unwrap();
    let after = acc.calendar_resource(&id).await.unwrap().unwrap();
    assert_eq!(after.name, "Board room (big)");
    assert_eq!(after.capacity, Some(12));
    assert_eq!(after.location, None);

    acc.delete_calendar_resource(&id).await.unwrap();
    assert!(acc.calendar_resource(&id).await.unwrap().is_none());
    assert!(matches!(
        acc.delete_calendar_resource(&id).await,
        Err(StoreError::NotFound)
    ));
}

#[tokio::test]
async fn an_address_names_one_thing_only() {
    let store = common::test_store().await;
    let (acc, _, _) = common::fresh_account(&store, "res-addr").await;

    acc.create_calendar_resource(&room("Board room", "board@example.test"))
        .await
        .unwrap();
    // A second room on the same address, in any case, is refused.
    assert!(matches!(
        acc.create_calendar_resource(&room("Other", "BOARD@example.test"))
            .await,
        Err(StoreError::Conflict(_))
    ));
    // …and so is a person's address: an attendee must mean one thing.
    assert!(matches!(
        acc.create_calendar_resource(&room("Me", "u-res-addr@example.test"))
            .await,
        Err(StoreError::Conflict(_))
    ));
    // A malformed address never reaches the table.
    assert!(matches!(
        acc.create_calendar_resource(&room("Nowhere", "board"))
            .await,
        Err(StoreError::Validation(_))
    ));
    assert_eq!(acc.calendar_resources().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_room_is_never_a_calendar_anyone_can_see_or_write() {
    let store = common::test_store().await;
    let (acc, _, _) = common::fresh_account(&store, "res-edit").await;

    let id = acc
        .create_calendar_resource(&room("Board room", "board@example.test"))
        .await
        .unwrap();
    // Not in the owner's own calendar list, so a room never lands in the week
    // grid with everybody else's bookings in it.
    let cals = acc.calendars().await.unwrap();
    assert!(cals.iter().all(|c| c.id.as_str() != id.as_str()));
    // And not writable — not even by the admin whose row created it.
    assert!(!acc.can_edit_calendar(&id).await.unwrap());
    let into_the_room = meeting(&id, odt(2026, 9, 2, 10, 0), odt(2026, 9, 2, 11, 0));
    assert!(matches!(
        acc.create_event(&into_the_room).await,
        Err(StoreError::NotFound)
    ));
}

#[tokio::test]
async fn a_room_is_in_one_meeting_at_a_time() {
    let store = common::test_store().await;
    let (acc, _, _) = common::fresh_account(&store, "res-clash").await;
    let personal = acc.ensure_personal_calendar().await.unwrap();
    let id = acc
        .create_calendar_resource(&room("Board room", "board@example.test"))
        .await
        .unwrap();

    // The first meeting takes it, 10:00–11:00.
    let first = EventId::generate();
    let event = meeting(&personal, odt(2026, 9, 2, 10, 0), odt(2026, 9, 2, 11, 0));
    acc.book_resources(&first, &event, std::slice::from_ref(&id))
        .await
        .unwrap();
    acc.create_event_at(&first, &event).await.unwrap();

    // A second, overlapping, is refused — and the refusal names the slot.
    let second = EventId::generate();
    let clashing = meeting(&personal, odt(2026, 9, 2, 10, 30), odt(2026, 9, 2, 11, 30));
    match acc
        .book_resources(&second, &clashing, std::slice::from_ref(&id))
        .await
    {
        Err(StoreError::Conflict(message)) => {
            assert!(message.contains("Board room"), "{message}");
            assert!(message.contains("2026-09-02T10:00:00Z"), "{message}");
        }
        other => panic!("expected a conflict, got {other:?}"),
    }
    // Nothing was held for the refused meeting.
    assert!(
        acc.resource_bookings_in_range(&id, odt(2026, 9, 2, 0, 0), odt(2026, 9, 3, 0, 0))
            .await
            .unwrap()
            .len()
            == 1
    );

    // Back to back is not a clash: 11:00–12:00 starts where the first ended.
    let third = EventId::generate();
    let after = meeting(&personal, odt(2026, 9, 2, 11, 0), odt(2026, 9, 2, 12, 0));
    acc.book_resources(&third, &after, std::slice::from_ref(&id))
        .await
        .unwrap();
    acc.create_event_at(&third, &after).await.unwrap();

    // Re-saving the first meeting unchanged is not a clash with itself.
    acc.book_resources(&first, &event, std::slice::from_ref(&id))
        .await
        .unwrap();

    // Dropping the room from the guest list releases it.
    acc.book_resources(&first, &event, &[]).await.unwrap();
    let held = acc
        .resource_bookings_in_range(&id, odt(2026, 9, 2, 0, 0), odt(2026, 9, 3, 0, 0))
        .await
        .unwrap();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].starts_at, odt(2026, 9, 2, 11, 0));
}

#[tokio::test]
async fn a_series_holds_the_room_at_every_occurrence_it_still_has() {
    let store = common::test_store().await;
    let (acc, _, _) = common::fresh_account(&store, "res-series").await;
    let personal = acc.ensure_personal_calendar().await.unwrap();
    let id = acc
        .create_calendar_resource(&room("Board room", "board@example.test"))
        .await
        .unwrap();

    // A weekly standup from Wednesday 2 September, 10:00–11:00.
    let series_id = EventId::generate();
    let series = CalendarEvent {
        recurrence: Some("FREQ=WEEKLY".to_owned()),
        ..meeting(&personal, odt(2026, 9, 2, 10, 0), odt(2026, 9, 2, 11, 0))
    };
    acc.book_resources(&series_id, &series, std::slice::from_ref(&id))
        .await
        .unwrap();
    acc.create_event_at(&series_id, &series).await.unwrap();

    // A one-off three weeks out collides with an occurrence nobody typed.
    let late = EventId::generate();
    let clashing = meeting(
        &personal,
        odt(2026, 9, 23, 10, 30),
        odt(2026, 9, 23, 11, 30),
    );
    assert!(matches!(
        acc.book_resources(&late, &clashing, std::slice::from_ref(&id))
            .await,
        Err(StoreError::Conflict(_))
    ));

    // Skip that week on the series (an EXDATE) and the slot is free again.
    acc.exclude_occurrence(&series_id, odt(2026, 9, 23, 10, 0))
        .await
        .unwrap();
    acc.book_resources(&late, &clashing, std::slice::from_ref(&id))
        .await
        .unwrap();
    acc.create_event_at(&late, &clashing).await.unwrap();

    // A moved occurrence is busy where it moved to, not where it was: move the
    // 30 September instance to 14:00 and 10:00 that day opens up.
    acc.override_occurrence(
        &series_id,
        odt(2026, 9, 30, 10, 0),
        &alo_store::model::OccurrenceOverride {
            summary: "Standup (moved)".to_owned(),
            description: None,
            location: None,
            starts_at: odt(2026, 9, 30, 14, 0),
            ends_at: odt(2026, 9, 30, 15, 0),
            all_day: false,
        },
    )
    .await
    .unwrap();
    let morning = EventId::generate();
    let at_ten = meeting(&personal, odt(2026, 9, 30, 10, 0), odt(2026, 9, 30, 11, 0));
    acc.book_resources(&morning, &at_ten, std::slice::from_ref(&id))
        .await
        .unwrap();
    // …while 14:00 that day is now taken.
    let afternoon = EventId::generate();
    let at_two = meeting(&personal, odt(2026, 9, 30, 14, 0), odt(2026, 9, 30, 15, 0));
    assert!(matches!(
        acc.book_resources(&afternoon, &at_two, std::slice::from_ref(&id))
            .await,
        Err(StoreError::Conflict(_))
    ));
}

#[tokio::test]
async fn deleting_the_meeting_gives_the_room_back() {
    let store = common::test_store().await;
    let (acc, _, _) = common::fresh_account(&store, "res-del").await;
    let personal = acc.ensure_personal_calendar().await.unwrap();
    let id = acc
        .create_calendar_resource(&room("Board room", "board@example.test"))
        .await
        .unwrap();

    let first = EventId::generate();
    let event = meeting(&personal, odt(2026, 9, 2, 10, 0), odt(2026, 9, 2, 11, 0));
    acc.book_resources(&first, &event, std::slice::from_ref(&id))
        .await
        .unwrap();
    acc.create_event_at(&first, &event).await.unwrap();
    // The room's own collection shows the booking, whoever made it.
    assert_eq!(acc.events_of_calendar(&id).await.unwrap().len(), 1);

    acc.delete_event(&first).await.unwrap();
    assert!(acc.events_of_calendar(&id).await.unwrap().is_empty());
    assert!(
        acc.resource_bookings_in_range(&id, odt(2026, 9, 2, 0, 0), odt(2026, 9, 3, 0, 0))
            .await
            .unwrap()
            .is_empty()
    );

    // And the slot is bookable again.
    let second = EventId::generate();
    acc.book_resources(&second, &event, std::slice::from_ref(&id))
        .await
        .unwrap();
}

#[tokio::test]
async fn one_tenants_rooms_are_invisible_and_unbookable_from_another() {
    let store = common::test_store().await;
    let t1 = store.create_tenant("t-res-iso-a").await.unwrap();
    let t2 = store.create_tenant("t-res-iso-b").await.unwrap();
    let u1 = store
        .for_tenant(t1.clone())
        .create_user("a@res-iso.test")
        .await
        .unwrap();
    let u2 = store
        .for_tenant(t2.clone())
        .create_user("b@res-iso.test")
        .await
        .unwrap();
    let a = store.for_account(t1.clone(), u1.clone());
    let b = store.for_account(t2.clone(), u2);
    // The forged door: tenant B holding tenant A's user id.
    let forged = store.for_account(t2.clone(), u1);

    let room_id = a
        .create_calendar_resource(&room("Board room", "board@res-iso.test"))
        .await
        .unwrap();
    let first = EventId::generate();
    let personal = a.ensure_personal_calendar().await.unwrap();
    let event = meeting(&personal, odt(2026, 9, 2, 10, 0), odt(2026, 9, 2, 11, 0));
    a.book_resources(&first, &event, std::slice::from_ref(&room_id))
        .await
        .unwrap();
    a.create_event_at(&first, &event).await.unwrap();

    for other in [&b, &forged] {
        assert!(other.calendar_resources().await.unwrap().is_empty());
        assert!(other.calendar_resource(&room_id).await.unwrap().is_none());
        assert!(
            other
                .calendar_resource_by_email("board@res-iso.test")
                .await
                .unwrap()
                .is_none()
        );
        // The other tenant may not book it, and may not read what it holds…
        let theirs = EventId::generate();
        assert!(matches!(
            other
                .book_resources(&theirs, &event, std::slice::from_ref(&room_id))
                .await,
            Err(StoreError::NotFound)
        ));
        assert!(
            other
                .resource_bookings_in_range(&room_id, odt(2026, 9, 2, 0, 0), odt(2026, 9, 3, 0, 0))
                .await
                .unwrap()
                .is_empty()
        );
        assert!(other.events_of_calendar(&room_id).await.unwrap().is_empty());
        // …nor rename or retire it.
        assert!(matches!(
            other
                .update_calendar_resource(&room_id, &room("Theirs", "theirs@res-iso.test"))
                .await,
            Err(StoreError::NotFound)
        ));
        assert!(matches!(
            other.delete_calendar_resource(&room_id).await,
            Err(StoreError::NotFound)
        ));
    }

    // Tenant A's room is untouched, and the same address is free next door.
    assert_eq!(a.calendar_resources().await.unwrap()[0].name, "Board room");
    b.create_calendar_resource(&room("Their room", "board@res-iso.test"))
        .await
        .unwrap();
}
