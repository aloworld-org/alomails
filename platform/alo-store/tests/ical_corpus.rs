//! The iCalendar (RFC 5545) round-trip corpus (mail queue M3.1): every
//! fixture **parses → stores → serializes byte-stable**. A client `.ics` is
//! parsed (`ical::from_ics`), written through the real store (`put_event`,
//! the CalDAV PUT path), read back, and serialized (`ical::to_ics_at` — the
//! deterministic-`DTSTAMP` seam, since `DTSTAMP` is the moment of
//! serialization and derives from nothing in the event). The result must
//! equal the checked-in canonical bytes, and feeding the canonical form
//! through the same chain must reproduce it exactly — proving the
//! serializer is a fixed point: no drift in escaping, folding, or time math
//! across sync cycles.
//!
//! Slice scope (grows in M3.2 with recurrence/DST fixtures): plain timed
//! events, all-day events, and UTC / zoned (`TZID=`) / floating times.
//! Zoned wall-clock times are converted to the UTC instant via the IANA
//! database (jiff owns tz math); floating times are read as UTC — both
//! documented cuts in `docs/interop.md`, both pinned here.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{AccountStore, EventId, ical};
use common::{fresh_account, test_store};
use time::{Date, Month, OffsetDateTime, Time};

/// The pinned serialization instant for every canonical form below.
fn t0() -> OffsetDateTime {
    OffsetDateTime::new_utc(
        Date::from_calendar_date(2026, Month::January, 2).unwrap(),
        Time::from_hms(3, 4, 5).unwrap(),
    )
}

/// One corpus entry: a client-authored fixture and the canonical bytes the
/// server serves for it after parse → store → read-back.
struct Fixture {
    name: &'static str,
    /// The `.ics` as a client would PUT it (foreign PRODID, its own DTSTAMP).
    input: &'static str,
    /// The byte-exact serialization alo serves back (at the pinned DTSTAMP).
    canonical: &'static str,
}

const CORPUS: &[Fixture] = &[
    Fixture {
        name: "plain UTC timed event",
        input: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Apple Inc.//iOS 18.0//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-utc\r\nDTSTAMP:20260810T120000Z\r\n\
                DTSTART:20260815T090000Z\r\nDTEND:20260815T103000Z\r\n\
                SUMMARY:Design review\r\nLOCATION:Room 4\r\n\
                DESCRIPTION:Bring the mockups\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        canonical: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//alo//calendar//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-utc\r\nDTSTAMP:20260102T030405Z\r\n\
                DTSTART:20260815T090000Z\r\nDTEND:20260815T103000Z\r\n\
                SUMMARY:Design review\r\nLOCATION:Room 4\r\n\
                DESCRIPTION:Bring the mockups\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    },
    Fixture {
        name: "all-day event (VALUE=DATE)",
        input: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Mozilla.org//NONSGML Thunderbird//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-allday\r\nDTSTAMP:20260810T120000Z\r\n\
                DTSTART;VALUE=DATE:20261225\r\nDTEND;VALUE=DATE:20261227\r\n\
                SUMMARY:Office closed\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        canonical: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//alo//calendar//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-allday\r\nDTSTAMP:20260102T030405Z\r\n\
                DTSTART;VALUE=DATE:20261225\r\nDTEND;VALUE=DATE:20261227\r\n\
                SUMMARY:Office closed\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    },
    Fixture {
        // 14:00 Europe/Brussels on 2026-06-10 is CEST (UTC+2) → 12:00Z. The
        // zone name is resolved against the IANA database; the stored instant
        // is UTC, so the canonical form carries `Z` times.
        name: "zoned wall-clock time (TZID=Europe/Brussels)",
        input: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Zoned//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-zoned\r\nDTSTAMP:20260810T120000Z\r\n\
                DTSTART;TZID=Europe/Brussels:20260610T140000\r\n\
                DTEND;TZID=Europe/Brussels:20260610T153000\r\n\
                SUMMARY:Client visit\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        canonical: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//alo//calendar//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-zoned\r\nDTSTAMP:20260102T030405Z\r\n\
                DTSTART:20260610T120000Z\r\nDTEND:20260610T133000Z\r\n\
                SUMMARY:Client visit\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    },
    Fixture {
        // A floating time (no Z, no TZID) is read as UTC — the documented cut
        // (docs/interop.md); this pins that behaviour rather than assumes it.
        name: "floating time reads as UTC",
        input: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Floating//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-floating\r\nDTSTAMP:20260810T120000Z\r\n\
                DTSTART:20260405T083000\r\nDTEND:20260405T091500\r\n\
                SUMMARY:Morning run\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        canonical: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//alo//calendar//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-floating\r\nDTSTAMP:20260102T030405Z\r\n\
                DTSTART:20260405T083000Z\r\nDTEND:20260405T091500Z\r\n\
                SUMMARY:Morning run\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    },
    Fixture {
        // §3.3.11 text escaping: commas, semicolons, and newlines survive the
        // store byte-for-byte.
        name: "escaped text values",
        input: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Escapes//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-escapes\r\nDTSTAMP:20260810T120000Z\r\n\
                DTSTART:20260901T100000Z\r\nDTEND:20260901T110000Z\r\n\
                SUMMARY:Budget\\, review\\; part 1\r\nDESCRIPTION:Line one\\nLine two\r\n\
                END:VEVENT\r\nEND:VCALENDAR\r\n",
        canonical: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//alo//calendar//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-escapes\r\nDTSTAMP:20260102T030405Z\r\n\
                DTSTART:20260901T100000Z\r\nDTEND:20260901T110000Z\r\n\
                SUMMARY:Budget\\, review\\; part 1\r\nDESCRIPTION:Line one\\nLine two\r\n\
                END:VEVENT\r\nEND:VCALENDAR\r\n",
    },
];

/// Parse `ics`, write it through the real store onto the account's personal
/// calendar (the CalDAV PUT path), read it back, and serialize at the pinned
/// instant — the full chain a syncing client exercises.
async fn through_store(acc: &AccountStore, ics: &str) -> String {
    let mut event = ical::from_ics(ics, "corpus-fallback").expect("fixture parses");
    event.calendar_id = acc
        .ensure_personal_calendar()
        .await
        .expect("personal calendar");
    let id = EventId::new(event.id.as_str().to_owned());
    acc.put_event(&id, &event).await.expect("PUT stores");
    let stored = acc
        .event(&id)
        .await
        .expect("read back")
        .expect("stored event exists");
    ical::to_ics_at(&stored, t0())
}

#[tokio::test]
async fn corpus_round_trips_byte_stable_through_the_store() {
    let store = test_store().await;
    let (acc, _user, _inbox) = fresh_account(&store, "ical-corpus").await;

    for f in CORPUS {
        // Client bytes in → canonical bytes out.
        let first = through_store(&acc, f.input).await;
        assert_eq!(first, f.canonical, "canonical serialization: {}", f.name);
        // The canonical form is a fixed point: another full parse → store →
        // serialize cycle reproduces it byte-for-byte.
        let second = through_store(&acc, &first).await;
        assert_eq!(second, first, "byte-stable across cycles: {}", f.name);
    }
}

#[tokio::test]
async fn folded_lines_unfold_and_refold_stably() {
    // A description far past the 75-octet fold limit, with a multi-byte char
    // near a boundary; the folded serialization must be its own fixed point
    // and the unfolded text must survive the store unchanged.
    let store = test_store().await;
    let (acc, _user, _inbox) = fresh_account(&store, "ical-fold").await;

    let long = "Agenda point één: the quarterly figures need a second pass before \
                the board sees them — bring the annotated deck and the café list.";
    let input = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Fold//EN\r\n\
         BEGIN:VEVENT\r\nUID:corpus-fold\r\nDTSTAMP:20260810T120000Z\r\n\
         DTSTART:20260920T130000Z\r\nDTEND:20260920T140000Z\r\n\
         SUMMARY:Long agenda\r\nDESCRIPTION:{}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        long.replace(',', "\\,")
    );
    let first = through_store(&acc, &input).await;
    assert!(
        first.contains("\r\n "),
        "the long description folds: {first}"
    );
    let second = through_store(&acc, &first).await;
    assert_eq!(second, first, "folded form is a fixed point");
    let stored = acc
        .event(&EventId::new("corpus-fold".to_owned()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.description.as_deref(), Some(long), "text survives");
}
