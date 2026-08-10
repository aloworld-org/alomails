//! The billing HTTP edge (alo Billing, ADR 0035, wave B1) — the conventions
//! every `/billing/*` route module shares, so customers, products, and the
//! invoices, quotes and payments that follow answer a caller the same way
//! rather than each inventing its own dialect.
//!
//! Four things live here and nothing else: the store-error → HTTP map
//! (`docs/design/billing.md` § Errors), body parsing that never echoes the
//! request back, the RFC 3339 stamp every billing resource carries, and the
//! small helpers a partial `PATCH` and a boolean query flag need.

use axum::http::StatusCode;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use time::format_description::well_known::{Iso8601, Rfc3339};
use time::{Date, OffsetDateTime};

use alo_store::StoreError;

use crate::error::Problem;

/// Maps a store error onto the HTTP answer `docs/design/billing.md` promises.
///
/// The two that matter for billing: a record that is absent **or another
/// tenant's** is the same `404` (no existence oracle across tenants), and a
/// field the caller can fix is a `422` carrying the rule it broke. Those
/// [`StoreError::Validation`] messages are authored by the store and name the
/// violated rule only — they never echo a stored value, so they are safe to
/// return verbatim. Everything else, database failures included, is an opaque
/// `500`.
pub fn map_store_err(error: StoreError) -> Problem {
    Problem::from(error)
}

/// Parses a JSON request body, mapping every failure to a plain `400` with a
/// fixed detail.
///
/// Deliberately not the JMAP `notJSON` problem type: these are ordinary REST
/// resources, not JMAP envelopes. And deliberately not serde's own message —
/// it quotes the offending input, which on a billing route can be customer
/// data, and error text is not a place we put customer data.
pub fn parse_body<T: DeserializeOwned>(body: &[u8]) -> Result<T, Problem> {
    serde_json::from_slice(body)
        .map_err(|_| Problem::with(StatusCode::BAD_REQUEST, "malformed request body"))
}

/// Formats a timestamp the way every billing resource reports one.
pub fn iso(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

/// Formats a calendar date — an issue date, a due date — as `YYYY-MM-DD`.
///
/// A billing date is a **day**, not an instant: an invoice issued in Warsaw is
/// dated the day the tenant issued it, and giving it a time and a zone would
/// invite a client to shift it across midnight. Kept separate from [`iso`] for
/// exactly that reason.
pub fn iso_date(d: Date) -> String {
    d.format(&Iso8601::DATE).unwrap_or_default()
}

/// Reads a billing date written `YYYY-MM-DD` — the mirror of [`iso_date`] —
/// or `None` when the text is not exactly that.
///
/// The shape is checked before parsing because `Iso8601::DATE` on its own is
/// **too forgiving for money**: it accepts `2026-08-07T10:00:00Z` and quietly
/// keeps the day, so a client sending its own local midnight would have a
/// payment silently dated to whichever side of it their zone falls. It also
/// accepts `20260807`, a second spelling of a value that appears on documents
/// and in exports. One spelling in, one spelling out, and anything else is the
/// caller's `422`.
pub fn parse_iso_date(raw: &str) -> Option<Date> {
    let bytes = raw.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(i, b)| matches!(i, 4 | 7) || b.is_ascii_digit())
    {
        return None;
    }
    Date::parse(raw, &Iso8601::DATE).ok()
}

/// Reads an **instant** a caller wrote as a full RFC 3339 timestamp, normalised
/// to UTC — the mirror of [`iso`] — or `None` when the text is not one.
///
/// The counterpart of [`parse_iso_date`], and deliberately its opposite: a day
/// must never be written as a timestamp, and an instant must never be written as
/// a bare day. A call logged at 16:05 in Warsaw happened at one moment, so the
/// zone is part of what the caller states; `2026-08-07` states no moment at all
/// and is the caller's `422` rather than a silent midnight in whichever zone the
/// server happens to run in.
pub fn parse_rfc3339(raw: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(raw.trim(), &Rfc3339)
        .ok()
        .map(|t| t.to_offset(time::UtcOffset::UTC))
}

/// Deserializes a field that may be absent, `null`, or a value, keeping the
/// three cases apart: `None` = absent (a `PATCH` leaves the stored value
/// alone), `Some(None)` = explicit `null` (clear it), `Some(Some(v))` = set it.
///
/// A plain `Option<T>` collapses the first two, which would leave a `PATCH`
/// unable to clear a nullable field — a VAT id entered by mistake could never
/// be taken off a customer again. Used with `#[serde(default,
/// deserialize_with = "…")]` on an `Option<Option<T>>` field.
pub fn absent_or_null<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Normalises an optional string field written by a client: blank (or
/// whitespace-only) is the same as absent, because a form that clears a text
/// box sends `""` and means `null`.
pub fn blank_to_none(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

/// Reads an optional boolean query flag: `1`, `true` or `yes` in any case turn
/// it on; anything else — including an absent or unparseable value — leaves it
/// off.
///
/// Forgiving on purpose. A list filter is not worth failing a request over,
/// and axum's own `Query<bool>` rejection would answer in a shape that is not
/// our [`Problem`], which is exactly the inconsistency this module exists to
/// prevent.
pub fn flag(value: Option<&str>) -> bool {
    matches!(
        value.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("1" | "true" | "yes")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_reads_the_common_spellings_of_yes() {
        for on in ["1", "true", "TRUE", "True", "yes", " true "] {
            assert!(flag(Some(on)), "expected on: {on:?}");
        }
        for off in [None, Some(""), Some("0"), Some("false"), Some("maybe")] {
            assert!(!flag(off), "expected off: {off:?}");
        }
    }

    #[test]
    fn a_billing_date_is_a_plain_day() {
        let d = Date::from_calendar_date(2026, time::Month::January, 9).unwrap_or_else(|e| {
            panic!("{e}");
        });
        assert_eq!(iso_date(d), "2026-01-09");
        assert_eq!(parse_iso_date("2026-01-09"), Some(d), "and back again");
    }

    #[test]
    fn a_date_that_is_not_a_plain_day_is_refused_never_truncated() {
        // The timestamp is the one that matters: `Iso8601::DATE` would accept
        // it and keep the day, so a client's local midnight would silently land
        // a payment on the wrong side of it.
        for bad in [
            "2026-08-07T10:00:00Z",
            "2026-08-07 ",
            "20260807",
            "2026-8-7",
            "07/08/2026",
            "2026-13-01",
            "2026-02-30",
            "yesterday",
            "",
            "202X-08-07",
        ] {
            assert_eq!(parse_iso_date(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn blank_is_the_same_as_absent() {
        assert_eq!(blank_to_none(None), None);
        assert_eq!(blank_to_none(Some(String::new())), None);
        assert_eq!(blank_to_none(Some("  \t ".to_owned())), None);
        assert_eq!(
            blank_to_none(Some(" DE811907980 ".to_owned())),
            Some(" DE811907980 ".to_owned())
        );
    }

    #[test]
    fn absent_null_and_value_stay_three_different_things() {
        #[derive(Deserialize)]
        struct Body {
            #[serde(default, deserialize_with = "absent_or_null")]
            vat_id: Option<Option<String>>,
        }
        let absent: Body = serde_json::from_str("{}").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(absent.vat_id, None);
        let cleared: Body =
            serde_json::from_str(r#"{"vat_id":null}"#).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(cleared.vat_id, Some(None));
        let set: Body =
            serde_json::from_str(r#"{"vat_id":"DE811907980"}"#).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(set.vat_id, Some(Some("DE811907980".to_owned())));
    }

    #[test]
    fn the_error_map_is_the_one_the_design_note_publishes() {
        let status = |e: StoreError| map_store_err(e).status;
        assert_eq!(status(StoreError::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(status(StoreError::Forbidden), StatusCode::FORBIDDEN);
        assert_eq!(
            status(StoreError::Validation("bad vat id".to_owned())),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            status(StoreError::Conflict("already issued".to_owned())),
            StatusCode::CONFLICT
        );
        assert_eq!(
            status(StoreError::OverQuota),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn a_validation_message_reaches_the_caller_but_a_database_error_does_not() {
        let problem = map_store_err(StoreError::Validation("country must be…".to_owned()));
        assert_eq!(problem.detail.as_deref(), Some("country must be…"));
        let problem = map_store_err(StoreError::Db(sqlx::Error::PoolClosed));
        assert_eq!(problem.detail, None);
    }

    #[test]
    fn a_malformed_body_never_quotes_the_body() {
        #[derive(Deserialize)]
        struct Body {
            #[allow(dead_code)]
            name: String,
        }
        let Err(problem) = parse_body::<Body>(br#"{"name": 7, "secret": "acme-gmbh"}"#) else {
            panic!("a wrongly typed field must be rejected");
        };
        assert_eq!(problem.status, StatusCode::BAD_REQUEST);
        assert_eq!(problem.detail.as_deref(), Some("malformed request body"));
    }
}
