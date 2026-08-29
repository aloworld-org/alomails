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
//! Since AS.2 every zoned canonical form also carries the served `VTIMEZONE`
//! (one per referenced zone, observances bounded to the object's span).
//! Zoned wall-clock times are stored as UTC instants beside their IANA zone
//! (jiff owns tz math) and served back in `;TZID=` wall-clock form; floating
//! times are read as UTC — documented in `docs/interop.md`, pinned here.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common::{fresh_account, test_store};
use alo_store::{AccountStore, EventId, ical};
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
        // Serving adds the VTIMEZONE for the referenced zone (AS.2): the one
        // rule in force across the event — CEST, entered at the 2026-03-29
        // switch (01:00Z = 02:00 in the prior +0100 offset).
        canonical: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//alo//calendar//EN\r\n\
                BEGIN:VTIMEZONE\r\nTZID:Europe/Brussels\r\n\
                BEGIN:DAYLIGHT\r\nDTSTART:20260329T020000\r\n\
                TZOFFSETFROM:+0100\r\nTZOFFSETTO:+0200\r\nTZNAME:CEST\r\nEND:DAYLIGHT\r\n\
                END:VTIMEZONE\r\n\
                BEGIN:VEVENT\r\nUID:corpus-zoned\r\nDTSTAMP:20260102T030405Z\r\n\
                DTSTART;TZID=Europe/Brussels:20260610T140000\r\n\
                DTEND;TZID=Europe/Brussels:20260610T153000\r\n\
                SUMMARY:Client visit\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    },
    Fixture {
        // A fixed-offset zone (Etc/GMT-2 is UTC+2 — POSIX sign inversion —
        // with no transitions ever): its VTIMEZONE is a single STANDARD
        // observance holding since forever, the epoch by convention.
        name: "fixed-offset zone (TZID=Etc/GMT-2)",
        input: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Fixed//EN\r\n\
                BEGIN:VEVENT\r\nUID:corpus-fixed-zone\r\nDTSTAMP:20260810T120000Z\r\n\
                DTSTART;TZID=Etc/GMT-2:20260610T140000\r\n\
                DTEND;TZID=Etc/GMT-2:20260610T153000\r\n\
                SUMMARY:Fixed-zone call\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        canonical: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//alo//calendar//EN\r\n\
                BEGIN:VTIMEZONE\r\nTZID:Etc/GMT-2\r\n\
                BEGIN:STANDARD\r\nDTSTART:19700101T000000\r\n\
                TZOFFSETFROM:+0200\r\nTZOFFSETTO:+0200\r\nTZNAME:+02\r\nEND:STANDARD\r\n\
                END:VTIMEZONE\r\n\
                BEGIN:VEVENT\r\nUID:corpus-fixed-zone\r\nDTSTAMP:20260102T030405Z\r\n\
                DTSTART;TZID=Etc/GMT-2:20260610T140000\r\n\
                DTEND;TZID=Etc/GMT-2:20260610T153000\r\n\
                SUMMARY:Fixed-zone call\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
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
        // The served VTIMEZONE covers the series' span plus a year (the rule
        // is COUNT-bounded, treated like open-ended): the CEST rule in force,
        // the 2026-10-25 switch the series crosses, and the two switches of
        // the following year.
        canonical: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//alo//calendar//EN\r\n\
                BEGIN:VTIMEZONE\r\nTZID:Europe/Brussels\r\n\
                BEGIN:DAYLIGHT\r\nDTSTART:20260329T020000\r\n\
                TZOFFSETFROM:+0100\r\nTZOFFSETTO:+0200\r\nTZNAME:CEST\r\nEND:DAYLIGHT\r\n\
                BEGIN:STANDARD\r\nDTSTART:20261025T030000\r\n\
                TZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\nTZNAME:CET\r\nEND:STANDARD\r\n\
                BEGIN:DAYLIGHT\r\nDTSTART:20270328T020000\r\n\
                TZOFFSETFROM:+0100\r\nTZOFFSETTO:+0200\r\nTZNAME:CEST\r\nEND:DAYLIGHT\r\n\
                BEGIN:STANDARD\r\nDTSTART:20271031T030000\r\n\
                TZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\nTZNAME:CET\r\nEND:STANDARD\r\n\
                END:VTIMEZONE\r\n\
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

/// A multi-`VEVENT` fixture: parse the whole series (`from_ics_series`),
/// write it through the real store (`put_event` + `replace_overrides` — the
/// CalDAV PUT path), read back master and overrides, and serialize at the
/// pinned instant.
async fn series_through_store(acc: &AccountStore, ics: &str) -> String {
    let mut series = ical::from_ics_series(ics, "corpus-fallback").expect("fixture parses");
    series.master.calendar_id = acc
        .ensure_personal_calendar()
        .await
        .expect("personal calendar");
    let id = EventId::new(series.master.id.as_str().to_owned());
    acc.put_event(&id, &series.master)
        .await
        .expect("PUT stores");
    acc.replace_overrides(&id, &series.overrides)
        .await
        .expect("overrides reconcile");
    let stored = acc
        .event(&id)
        .await
        .expect("read back")
        .expect("stored event exists");
    let ovs = acc.override_occurrences(&id).await.expect("overrides read");
    ical::to_ics_series_at(&stored, &ovs, t0())
}

/// Phone-originated per-occurrence edits (AS.1): what Apple Calendar and
/// DAVx⁵ actually PUT — a `VTIMEZONE` block plus master and `RECURRENCE-ID`
/// instances — parses, stores, and serves back byte-stable; a
/// `STATUS:CANCELLED` instance collapses into an `EXDATE` on the master.
#[tokio::test]
async fn client_series_with_overrides_round_trips_byte_stable() {
    let store = test_store().await;
    let (acc, _user, _inbox) = fresh_account(&store, "ical-series").await;

    let series_fixtures = [
        Fixture {
            // Apple-style: VTIMEZONE shipped (ignored — the IANA name is the
            // definition), zoned master + one moved instance.
            name: "Apple two-VEVENT series with a moved occurrence",
            input: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Apple Inc.//iOS 18.0//EN\r\n\
                    BEGIN:VTIMEZONE\r\nTZID:Europe/Brussels\r\n\
                    BEGIN:DAYLIGHT\r\nTZOFFSETFROM:+0100\r\n\
                    RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU\r\nDTSTART:19810329T020000\r\n\
                    TZNAME:CEST\r\nTZOFFSETTO:+0200\r\nEND:DAYLIGHT\r\n\
                    BEGIN:STANDARD\r\nTZOFFSETFROM:+0200\r\n\
                    RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\r\nDTSTART:19961027T030000\r\n\
                    TZNAME:CET\r\nTZOFFSETTO:+0100\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n\
                    BEGIN:VEVENT\r\nUID:corpus-series-ov\r\nDTSTAMP:20260810T120000Z\r\n\
                    DTSTART;TZID=Europe/Brussels:20260907T090000\r\n\
                    DTEND;TZID=Europe/Brussels:20260907T093000\r\n\
                    RRULE:FREQ=WEEKLY\r\nSUMMARY:Standup\r\nEND:VEVENT\r\n\
                    BEGIN:VEVENT\r\nUID:corpus-series-ov\r\nDTSTAMP:20260810T120000Z\r\n\
                    RECURRENCE-ID;TZID=Europe/Brussels:20260914T090000\r\n\
                    DTSTART;TZID=Europe/Brussels:20260914T150000\r\n\
                    DTEND;TZID=Europe/Brussels:20260914T153000\r\n\
                    SUMMARY:Standup (moved)\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            // The served form defines the zone itself (AS.2) — the client's
            // shipped VTIMEZONE is not echoed; alo's own bounded one is
            // emitted: the open-ended weekly extends the span a year, so the
            // rule in force plus the next two switches appear.
            canonical: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//alo//calendar//EN\r\n\
                    BEGIN:VTIMEZONE\r\nTZID:Europe/Brussels\r\n\
                    BEGIN:DAYLIGHT\r\nDTSTART:20260329T020000\r\n\
                    TZOFFSETFROM:+0100\r\nTZOFFSETTO:+0200\r\nTZNAME:CEST\r\nEND:DAYLIGHT\r\n\
                    BEGIN:STANDARD\r\nDTSTART:20261025T030000\r\n\
                    TZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\nTZNAME:CET\r\nEND:STANDARD\r\n\
                    BEGIN:DAYLIGHT\r\nDTSTART:20270328T020000\r\n\
                    TZOFFSETFROM:+0100\r\nTZOFFSETTO:+0200\r\nTZNAME:CEST\r\nEND:DAYLIGHT\r\n\
                    END:VTIMEZONE\r\n\
                    BEGIN:VEVENT\r\nUID:corpus-series-ov\r\nDTSTAMP:20260102T030405Z\r\n\
                    DTSTART;TZID=Europe/Brussels:20260907T090000\r\n\
                    DTEND;TZID=Europe/Brussels:20260907T093000\r\n\
                    RRULE:FREQ=WEEKLY\r\nSUMMARY:Standup\r\nEND:VEVENT\r\n\
                    BEGIN:VEVENT\r\nUID:corpus-series-ov\r\nDTSTAMP:20260102T030405Z\r\n\
                    DTSTART;TZID=Europe/Brussels:20260914T150000\r\n\
                    DTEND;TZID=Europe/Brussels:20260914T153000\r\n\
                    RECURRENCE-ID;TZID=Europe/Brussels:20260914T090000\r\n\
                    SUMMARY:Standup (moved)\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        },
        Fixture {
            // DAVx⁵-style: a cancelled instance (STATUS:CANCELLED at its
            // RECURRENCE-ID) collapses into an EXDATE on the master.
            name: "DAVx5 cancelled instance becomes an EXDATE",
            input: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:+//IDN bitfire.at//DAVx5\r\n\
                    BEGIN:VEVENT\r\nUID:corpus-series-cancel\r\nDTSTAMP:20260810T120000Z\r\n\
                    DTSTART:20260907T090000Z\r\nDTEND:20260907T093000Z\r\n\
                    RRULE:FREQ=WEEKLY\r\nSUMMARY:Standup\r\nEND:VEVENT\r\n\
                    BEGIN:VEVENT\r\nUID:corpus-series-cancel\r\nDTSTAMP:20260810T120000Z\r\n\
                    RECURRENCE-ID:20260921T090000Z\r\nDTSTART:20260921T090000Z\r\n\
                    DTEND:20260921T093000Z\r\nSTATUS:CANCELLED\r\nSUMMARY:Standup\r\n\
                    END:VEVENT\r\nEND:VCALENDAR\r\n",
            canonical: "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//alo//calendar//EN\r\n\
                    BEGIN:VEVENT\r\nUID:corpus-series-cancel\r\nDTSTAMP:20260102T030405Z\r\n\
                    DTSTART:20260907T090000Z\r\nDTEND:20260907T093000Z\r\n\
                    RRULE:FREQ=WEEKLY\r\nEXDATE:20260921T090000Z\r\n\
                    SUMMARY:Standup\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        },
    ];
    for f in &series_fixtures {
        let first = series_through_store(&acc, f.input).await;
        assert_eq!(first, f.canonical, "canonical serialization: {}", f.name);
        let second = series_through_store(&acc, &first).await;
        assert_eq!(second, first, "byte-stable across cycles: {}", f.name);
    }

    // A re-PUT that no longer carries the override removes it (RFC 4791 §4.1:
    // the PUT body is the whole resource).
    let trimmed = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Apple Inc.//iOS 18.0//EN\r\n\
                   BEGIN:VEVENT\r\nUID:corpus-series-ov\r\nDTSTAMP:20260810T120000Z\r\n\
                   DTSTART;TZID=Europe/Brussels:20260907T090000\r\n\
                   DTEND;TZID=Europe/Brussels:20260907T093000\r\n\
                   RRULE:FREQ=WEEKLY\r\nSUMMARY:Standup\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let served = series_through_store(&acc, trimmed).await;
    assert!(
        !served.contains("RECURRENCE-ID"),
        "the dropped override is gone: {served}"
    );
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
