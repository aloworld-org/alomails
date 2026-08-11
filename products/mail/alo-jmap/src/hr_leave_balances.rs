//! What somebody has left, and who is away (alo HR, ADR 0035, wave B6.03b) —
//! over [`alo_store::hr_leave_balances`] and [`alo_store::hr_absences`].
//!
//! Two reads that look unrelated and are the same decision taken twice: **how
//! much of somebody's time off is anybody else's business.**
//!
//! - `GET /hr/leave-balances` is a *figure about a person* — how much they are
//!   owed, how much they have taken — and it goes to the person themselves,
//!   their manager (who has to decide their requests) and HR. Nobody else.
//! - `GET /hr/absences` is a *fact about a team* — who is not here on Thursday
//!   — and every member gets it, because that is what a workspace is for. It
//!   carries a name, an employee id and a day, and the store's query does not
//!   select the policy, the kind or the note, so there is nothing here to
//!   forget to strip.
//!
//! Both are reads, so neither is audited (`docs/design/hr.md` § Audit rejects a
//! general read-audit for HR: a trail of who looked at the absence layer is a
//! trail nobody reads and a fright everybody gets).
//!
//! The balance is returned **with its working** — entitlement, carried in,
//! accrued, taken, booked, pending — because a balance a person cannot
//! reproduce is a balance they will not trust, and because the alternative is a
//! support conversation for every leaver in Europe.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::Date;

use alo_store::hr_leave_math::days_tenths;
use alo_store::{AbsenceDay, HrEmployeeId, PolicyBalance};

use crate::billing::{iso_date, map_store_err, parse_iso_date};
use crate::billing_document::today;
use crate::error::Problem;
use crate::hr_leave_door::LeaveDoor;
use crate::hr_leave_policies::policy_json;
use crate::state::{AppState, authenticate};

/// One policy's balance, in minutes, with the days a screen shows beside them.
///
/// **Days are tenths of a day**, integers — `125` is 12.5 days. Money learned
/// this in B1 and leave has the same reason: a balance shown as `12.499999` is
/// a support ticket, and a float that rounds one way in the browser and another
/// on the server is a person told two different numbers about their own
/// holiday.
fn balance_json(entry: &PolicyBalance) -> Value {
    let balance = &entry.balance;
    let average = entry.average_day_minutes;
    json!({
        "policy": policy_json(&entry.policy),
        "entitlementMinutes": balance.entitlement_minutes,
        "carriedInMinutes": balance.carried_in_minutes,
        "accruedMinutes": balance.accrued_minutes,
        "takenMinutes": balance.taken_minutes,
        "bookedMinutes": balance.booked_minutes,
        "pendingMinutes": balance.pending_minutes,
        "remainingMinutes": balance.remaining_minutes,
        "averageDayMinutes": average,
        "entitlementDaysTenths": days_tenths(balance.entitlement_minutes, average),
        "takenDaysTenths": days_tenths(balance.taken_minutes, average),
        "bookedDaysTenths": days_tenths(balance.booked_minutes, average),
        "pendingDaysTenths": days_tenths(balance.pending_minutes, average),
        "remainingDaysTenths": days_tenths(balance.remaining_minutes, average),
    })
}

/// One day of the absence layer.
fn absence_json(day: &AbsenceDay) -> Value {
    json!({
        "day": iso_date(day.day),
        "people": day.people.iter().map(|person| json!({
            "employeeId": person.employee_id.as_str(),
            "name": person.name,
        })).collect::<Vec<_>>(),
    })
}

/// A day the caller stated, or the `422` that names the format.
fn stated_day(raw: &str, field: &str) -> Result<Date, Problem> {
    parse_iso_date(raw.trim()).ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{field} must be a date written YYYY-MM-DD"),
        )
    })
}

/// Query string of the balances route.
#[derive(Deserialize)]
pub struct BalanceQuery {
    /// Whose balance. Absent means the caller's own.
    #[serde(default, rename = "employeeId")]
    employee_id: Option<String>,
    /// The day to fold to — which also chooses the leave year. Absent means
    /// today, on the **server's** clock: whether a balance has accrued is a fact
    /// about the tenant's year, not about the reader's device.
    #[serde(default)]
    on: Option<String>,
}

/// `GET /hr/leave-balances[?employeeId=&on=]` → `{"employeeId":…,
/// "on":…, "balances":[…]}`.
///
/// One entry per live policy, each with the whole working behind it.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the person is not this
/// tenant's or not the caller's to read; `409` when the caller has no employee
/// record and named nobody; `422` on a date that is not `YYYY-MM-DD`.
pub async fn list_balances(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<BalanceQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let door = LeaveDoor::resolve(&account).await?;
    let employee = match q
        .employee_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        Some(stated) => HrEmployeeId::new(stated.to_owned()),
        None => door.require_me()?,
    };
    if !door.may_read(&employee) {
        // The same answer a stranger's id gets, so no refusal says who works
        // here.
        return Err(Problem::with(StatusCode::NOT_FOUND, "no such employee"));
    }
    let on = match q.on.as_deref() {
        None => today(),
        Some(raw) => stated_day(raw, "on")?,
    };
    let balances = state
        .store
        .for_tenant(account.tenant.clone())
        .hr_leave_balances(&employee, on)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "employeeId": employee.as_str(),
        "on": iso_date(on),
        "balances": balances.iter().map(balance_json).collect::<Vec<_>>(),
    })))
}

/// Query string of the absence layer.
#[derive(Deserialize)]
pub struct AbsenceQuery {
    from: String,
    to: String,
}

/// `GET /hr/absences?from=&to=` → `{"days":[…]}` — who is away on each day of
/// the window, days with nobody away omitted.
///
/// **Every member gets this**, and it is the module's one read about other
/// people: a name, an employee id, a day. The Agenda draws it as a layer behind
/// the week and month views, and the leave-request form draws the same layer
/// behind its date picker, so somebody asking for a week can see who else is
/// already off before they ask rather than after they are refused.
///
/// # Errors
/// `401` without a valid bearer token; `422` when the window ends before it
/// starts, is longer than a year, or is not written `YYYY-MM-DD`.
pub async fn list_absences(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<AbsenceQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let from = stated_day(&q.from, "from")?;
    let to = stated_day(&q.to, "to")?;
    let days = state
        .store
        .for_tenant(account.tenant.clone())
        .hr_absences(from, to)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "from": iso_date(from),
        "to": iso_date(to),
        "days": days.iter().map(absence_json).collect::<Vec<_>>(),
    })))
}
