//! The working-hours half of Agenda's scheduling surface: reading and setting
//! the caller's own schedule. Authenticated and scoped through the account
//! door, like every `/calendar/*` route; the *use* of the schedule — the
//! `outsideHours` span kind beside `busy` — is served by
//! [`crate::calendar::free_busy`].
//!
//! - `GET /calendar/working-hours` — the caller's schedule (the Mon–Fri
//!   09:00–17:00 default until they set one).
//! - `PUT /calendar/working-hours` — replace it.
//!
//! On the wire the window is wall-clock (`"start": "09:00"`), days are ISO
//! weekday numbers (Monday = 1 … Sunday = 7), and `zone` is an IANA name or
//! `null` for "my own zone" (the person's profile zone, else UTC).

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::WorkingHours;

use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// `GET /calendar/working-hours` →
/// `{"days":[1..7],"start":"HH:MM","end":"HH:MM","zone":name|null}`.
pub async fn get_working_hours(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let hours = account
        .acc
        .working_hours()
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(hours_json(&hours)))
}

#[derive(Deserialize)]
struct WorkingHoursBody {
    /// ISO weekday numbers, Monday = 1 … Sunday = 7. Order and duplicates
    /// don't matter — the schedule is the *set*.
    days: Vec<u8>,
    /// Wall-clock `"HH:MM"`.
    start: String,
    /// Wall-clock `"HH:MM"` (or `"24:00"`), strictly after `start`.
    end: String,
    /// IANA zone name, or absent/`null` for the person's own zone.
    #[serde(default)]
    zone: Option<String>,
}

/// `PUT /calendar/working-hours` — replace the caller's schedule.
pub async fn put_working_hours(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: WorkingHoursBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let mut days: u8 = 0;
    for day in &req.days {
        if !(1..=7).contains(day) {
            return Err(Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "days are ISO weekday numbers, 1 (Monday) to 7 (Sunday)",
            ));
        }
        days |= 1 << (day - 1);
    }
    let hours = WorkingHours {
        days,
        start_minute: parse_hhmm(&req.start)?,
        end_minute: parse_hhmm(&req.end)?,
        zone: req.zone,
    };
    // The store re-checks the invariants (window order, zone known) and its
    // `Validation` message is the verbatim, actionable refusal → 422.
    account
        .acc
        .set_working_hours(&hours)
        .await
        .map_err(Problem::from)?;
    Ok(Json(hours_json(&hours)))
}

/// A schedule as the wire serves it.
fn hours_json(hours: &WorkingHours) -> Value {
    let days: Vec<u8> = (1u8..=7)
        .filter(|day| hours.days & (1 << (day - 1)) != 0)
        .collect();
    json!({
        "days": days,
        "start": format_hhmm(hours.start_minute),
        "end": format_hhmm(hours.end_minute),
        "zone": hours.zone,
    })
}

/// `"HH:MM"` → minutes after midnight. Accepts `"24:00"` (end-of-day), so a
/// round-the-clock schedule can be written; everything else is 00:00–23:59.
fn parse_hhmm(s: &str) -> Result<u16, Problem> {
    let invalid = || {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{s:?} is not a time of day (expected \"HH:MM\")"),
        )
    };
    let (h, m) = s.split_once(':').ok_or_else(invalid)?;
    let h: u16 = h.parse().map_err(|_| invalid())?;
    let m: u16 = m.parse().map_err(|_| invalid())?;
    let minutes = h * 60 + m;
    if h > 24 || m > 59 || minutes > 1440 {
        return Err(invalid());
    }
    Ok(minutes)
}

/// Minutes after midnight → `"HH:MM"`.
fn format_hhmm(minutes: u16) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn hhmm_round_trips_and_rejects_nonsense() {
        assert_eq!(parse_hhmm("09:00").unwrap(), 540);
        assert_eq!(parse_hhmm("23:59").unwrap(), 1439);
        assert_eq!(parse_hhmm("24:00").unwrap(), 1440);
        for bad in ["", "9", "09:60", "25:00", "24:01", "nine", "09:0a"] {
            assert!(parse_hhmm(bad).is_err(), "{bad:?} accepted");
        }
        assert_eq!(format_hhmm(540), "09:00");
        assert_eq!(format_hhmm(1440), "24:00");
    }
}
