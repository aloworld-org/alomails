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
//! Corpus scope: plain timed events, all-day events, UTC / zoned (`TZID=`) /
//! floating times, and (since M3.2) recurrence — weekly-with-exceptions,
//! monthly-by-day, RDATE extras, and a Europe/Brussels DST-crossing series.
//! Zoned wall-clock times are stored as UTC instants beside their IANA zone
//! (jiff owns tz math) and served back in `;TZID=` wall-clock form; floating
//! times are read as UTC — documented in `docs/interop.md`, pinned here.
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
        // 14:00 Europe/Brussels on 2026-06-10 is CEST (UTC+2): stored as the
        // UTC instant 12:00Z with the zone kept beside it, and served back in
        // the zone's wall-clock (`;TZID=`) so a client's recurrence expansion
        // follows local time. (Until M3.2 the canonical form flattened this
        // to UTC, which was DST-wrong for recurring events.)
        name: "zoned wall-clock time (TZID=Europe/Brussels)",
        input: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Zoned//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-zoned\r\nDTSTAMP:20260810T120000Z\r\n\
                DTSTART;TZID=Europe/Brussels:20260610T140000\r\n\
                DTEND;TZID=Europe/Brussels:20260610T153000\r\n\
                SUMMARY:Client visit\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        canonical: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//alo//calendar//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-zoned\r\nDTSTAMP:20260102T030405Z\r\n\
                DTSTART;TZID=Europe/Brussels:20260610T140000\r\n\
                DTEND;TZID=Europe/Brussels:20260610T153000\r\n\
                SUMMARY:Client visit\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    },
    Fixture {
        // A weekly series with two cancelled instances; the comma-separated
        // client EXDATE splits into one canonical line per instant.
        name: "weekly with exceptions (RRULE + EXDATE)",
        input: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Apple Inc.//iOS 18.0//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-weekly-ex\r\nDTSTAMP:20260810T120000Z\r\n\
                DTSTART:20260803T090000Z\r\nDTEND:20260803T093000Z\r\n\
                RRULE:FREQ=WEEKLY\r\nEXDATE:20260810T090000Z,20260824T090000Z\r\n\
                SUMMARY:Standup\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        canonical: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//alo//calendar//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-weekly-ex\r\nDTSTAMP:20260102T030405Z\r\n\
                DTSTART:20260803T090000Z\r\nDTEND:20260803T093000Z\r\n\
                RRULE:FREQ=WEEKLY\r\nEXDATE:20260810T090000Z\r\n\
                EXDATE:20260824T090000Z\r\n\
                SUMMARY:Standup\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    },
    Fixture {
        // Monthly by weekday ordinal (the 2nd Tuesday), plus an RDATE extra.
        name: "monthly by day (BYDAY=2TU) with an RDATE",
        input: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Mozilla.org//NONSGML Thunderbird//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-monthly-byday\r\nDTSTAMP:20260810T120000Z\r\n\
                DTSTART:20260908T130000Z\r\nDTEND:20260908T140000Z\r\n\
                RRULE:FREQ=MONTHLY;BYDAY=2TU\r\nRDATE:20260918T130000Z\r\n\
                SUMMARY:Board sync\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        canonical: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//alo//calendar//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-monthly-byday\r\nDTSTAMP:20260102T030405Z\r\n\
                DTSTART:20260908T130000Z\r\nDTEND:20260908T140000Z\r\n\
                RRULE:FREQ=MONTHLY;BYDAY=2TU\r\nRDATE:20260918T130000Z\r\n\
                SUMMARY:Board sync\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    },
    Fixture {
        // A recurring Brussels series crossing the 2026-10-25 end of DST: the
        // wall-clock form survives the store byte-for-byte, including a
        // post-switch EXDATE (09:00 local is then 08:00Z at rest) and a
        // zoned RDATE — the DST-crossing fixture the M3.2 queue item names.
        name: "Europe/Brussels DST-crossing recurring series",
        input: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//DST//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-dst-series\r\nDTSTAMP:20260810T120000Z\r\n\
                DTSTART;TZID=Europe/Brussels:20261019T090000\r\n\
                DTEND;TZID=Europe/Brussels:20261019T093000\r\n\
                RRULE:FREQ=WEEKLY;COUNT=6\r\n\
                EXDATE;TZID=Europe/Brussels:20261102T090000\r\n\
                RDATE;TZID=Europe/Brussels:20261022T090000\r\n\
                SUMMARY:Monday review\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        canonical: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//alo//calendar//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-dst-series\r\nDTSTAMP:20260102T030405Z\r\n\
                DTSTART;TZID=Europe/Brussels:20261019T090000\r\n\
                DTEND;TZID=Europe/Brussels:20261019T093000\r\n\
                RRULE:FREQ=WEEKLY;COUNT=6\r\n\
                RDATE;TZID=Europe/Brussels:20261022T090000\r\n\
                EXDATE;TZID=Europe/Brussels:20261102T090000\r\n\
                SUMMARY:Monday review\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
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
