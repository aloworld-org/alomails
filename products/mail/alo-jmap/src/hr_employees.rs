//! The employee record's HTTP surface (alo HR, ADR 0035, wave B6.02b) — the
//! directory, the record, and the archive — over [`alo_store::hr_employees`].
//!
//! `/hr` is a **new top-level prefix**: the production Caddyfile needs it added
//! at the next deploy (the standing human action `/billing`, `/crm`, `/audit`,
//! `/insights`, `/projects`, `/finance` and `/inventory` already carry), and it
//! joins `API_PATHS` in `web/vite.config.ts` here so a browser call reaches the
//! API instead of the dev SPA — the lesson S1.11, BI1.04, B3.04 and B4.05b each
//! paid for once (`docs/design/hr.md` § Routes).
//!
//! It shares [`crate::billing`]'s conventions — the store door, `Problem`
//! errors, no validation duplicated from the store, days as `YYYY-MM-DD` — and
//! adds the one this module exists for:
//!
//! > **The door decides the projection, not a filter at the edge.**
//!
//! `GET /hr/employees` and `GET /hr/employees/{id}` are readable by every
//! member and by HR, and they do not answer the same shape. A member gets
//! [`alo_store::DirectoryEntry`] — a type with no private field on it, so no
//! careless line in this file can leak a home address — and HR gets the whole
//! record, which is the only place a national identifier or a bank account is
//! returned about somebody else. The choice is made by calling a *different
//! store function*, never by deleting keys from a JSON object, because a
//! deletion somebody forgets is the leak this module exists to prevent.
//!
//! Everything a person's record holds is personal data, so **nothing here is
//! logged**: not a name, not an address, not a pay figure, not a staff number.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::Date;

use alo_store::hr_employments::{ContractKind, Employment, NewEmployment, PATTERN_DAYS, PayPeriod};
use alo_store::{
    DirectoryEntry, DriveNodeId, Employee, HrEmployeeId, NewEmployee, TenantStore, UserId,
};

use crate::billing::{
    absent_or_null, blank_to_none, flag, iso, iso_date, map_store_err, parse_body, parse_iso_date,
};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// One person as the directory shows them: the public fields, and nothing that
/// is not on [`DirectoryEntry`].
pub(crate) fn directory_json(e: &DirectoryEntry) -> Value {
    json!({
        "id": e.id.as_str(),
        "name": e.display_name(),
        "givenName": e.given_name,
        "familyName": e.family_name,
        "preferredName": e.preferred_name,
        "workEmail": e.work_email,
        "workPhone": e.work_phone,
        "managerId": e.manager_id.as_ref().map(HrEmployeeId::as_str),
        "photoNodeId": e.photo_node_id.as_ref().map(DriveNodeId::as_str),
        "jobTitle": e.job_title,
        "team": e.team,
        "startedOn": e.started_on.map(iso_date),
        "archived": e.archived,
    })
}

/// One whole record — **HR's read, and a person's read of themselves**.
///
/// Everything on it is here, private fields included: a subject-access answer
/// that omitted the address we hold would be a worse answer than none. The gate
/// is on the routes that call this, and the two that do are the HR door and the
/// own door.
pub(crate) fn employee_json(e: &Employee) -> Value {
    json!({
        "id": e.id.as_str(),
        "name": e.display_name(),
        "userId": e.user_id.as_ref().map(UserId::as_str),
        "staffNumber": e.staff_number,
        "givenName": e.given_name,
        "familyName": e.family_name,
        "preferredName": e.preferred_name,
        "workEmail": e.work_email,
        "workPhone": e.work_phone,
        "personalEmail": e.personal_email,
        "personalPhone": e.personal_phone,
        "dateOfBirth": e.date_of_birth.map(iso_date),
        "addressLine1": e.address_line1,
        "addressLine2": e.address_line2,
        "postalCode": e.postal_code,
        "city": e.city,
        "region": e.region,
        "country": e.country,
        "nationalId": e.national_id,
        "iban": e.iban,
        "emergencyName": e.emergency_name,
        "emergencyPhone": e.emergency_phone,
        "managerId": e.manager_id.as_ref().map(HrEmployeeId::as_str),
        "photoNodeId": e.photo_node_id.as_ref().map(DriveNodeId::as_str),
        "archived": e.is_archived(),
        "archivedAt": e.archived_at.map(iso),
        "createdAt": iso(e.created_at),
        "updatedAt": iso(e.updated_at),
    })
}

/// One period of employment as JSON.
///
/// `weeklyMinutes` is included although it is derivable, because it is the
/// figure a leave balance is scaled by and the client must never be the thing
/// that computes it — the same rule money has everywhere in this suite.
pub(crate) fn employment_json(e: &Employment) -> Value {
    json!({
        "id": e.id.as_str(),
        "jobTitle": e.job_title,
        "team": e.team,
        "contractKind": e.contract_kind.as_str(),
        "startedOn": iso_date(e.started_on),
        "endedOn": e.ended_on.map(iso_date),
        "patternMinutes": e.pattern_minutes,
        "weeklyMinutes": e.weekly_minutes(),
        "payAmountCents": e.pay_amount_cents,
        "payPeriod": e.pay_period.as_str(),
        "payCurrency": e.pay_currency,
        "open": e.is_open(),
        "createdAt": iso(e.created_at),
    })
}

/// The stored record as writable input — the base a `PATCH` merges onto, so an
/// absent field means "leave it alone" and a form that carries half a record
/// cannot blank the other half.
fn editable(e: &Employee) -> NewEmployee {
    NewEmployee {
        user_id: e.user_id.clone(),
        staff_number: e.staff_number.clone(),
        given_name: e.given_name.clone(),
        family_name: e.family_name.clone(),
        preferred_name: e.preferred_name.clone(),
        work_email: e.work_email.clone(),
        work_phone: e.work_phone.clone(),
        personal_email: e.personal_email.clone(),
        personal_phone: e.personal_phone.clone(),
        date_of_birth: e.date_of_birth,
        address_line1: e.address_line1.clone(),
        address_line2: e.address_line2.clone(),
        postal_code: e.postal_code.clone(),
        city: e.city.clone(),
        region: e.region.clone(),
        country: e.country.clone(),
        national_id: e.national_id.clone(),
        iban: e.iban.clone(),
        emergency_name: e.emergency_name.clone(),
        emergency_phone: e.emergency_phone.clone(),
        manager_id: e.manager_id.clone(),
        photo_node_id: e.photo_node_id.clone(),
    }
}

/// The writable fields of a person.
///
/// The nullable ones are `Option<Option<…>>` ([`absent_or_null`]): absent
/// leaves the stored value alone and an explicit `null` clears it. Without the
/// distinction a national identifier entered by mistake could never be taken
/// off a record again — and "we cannot remove it" is not an answer to give
/// about somebody's personal data.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmployeeBody {
    #[serde(default, deserialize_with = "absent_or_null")]
    user_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "absent_or_null")]
    staff_number: Option<Option<String>>,
    #[serde(default)]
    given_name: Option<String>,
    #[serde(default)]
    family_name: Option<String>,
    #[serde(default)]
    preferred_name: Option<String>,
    #[serde(default, deserialize_with = "absent_or_null")]
    work_email: Option<Option<String>>,
    #[serde(default)]
    work_phone: Option<String>,
    #[serde(default, deserialize_with = "absent_or_null")]
    personal_email: Option<Option<String>>,
    #[serde(default)]
    personal_phone: Option<String>,
    #[serde(default, deserialize_with = "absent_or_null")]
    date_of_birth: Option<Option<String>>,
    #[serde(default)]
    address_line1: Option<String>,
    #[serde(default)]
    address_line2: Option<String>,
    #[serde(default)]
    postal_code: Option<String>,
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    region: Option<String>,
    #[serde(default)]
    country: Option<String>,
    #[serde(default, deserialize_with = "absent_or_null")]
    national_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "absent_or_null")]
    iban: Option<Option<String>>,
    #[serde(default)]
    emergency_name: Option<String>,
    #[serde(default)]
    emergency_phone: Option<String>,
    #[serde(default, deserialize_with = "absent_or_null")]
    manager_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "absent_or_null")]
    photo_node_id: Option<Option<String>>,
    /// The terms they start on. Read on **create only** — see
    /// [`create_employee`].
    #[serde(default)]
    employment: Option<EmploymentBody>,
}

/// A nullable field's new value: absent keeps `base`, `null` clears it.
fn merge_optional(stated: Option<Option<String>>, base: Option<String>) -> Option<String> {
    match stated {
        None => base,
        Some(value) => blank_to_none(value),
    }
}

/// A nullable day: absent keeps `base`, `null` clears it, a value must be
/// exactly `YYYY-MM-DD` (never a timestamp, which would silently shift a date
/// of birth across midnight in some zone).
fn merge_optional_day(
    stated: Option<Option<String>>,
    base: Option<Date>,
    field: &str,
) -> Result<Option<Date>, Problem> {
    match stated {
        None => Ok(base),
        Some(None) => Ok(None),
        Some(Some(raw)) if raw.trim().is_empty() => Ok(None),
        Some(Some(raw)) => parse_iso_date(raw.trim()).map(Some).ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("{field} must be a date written YYYY-MM-DD"),
            )
        }),
    }
}

impl EmployeeBody {
    /// Merges the stated fields onto `base`, leaving the rest as they were.
    fn apply(self, base: NewEmployee) -> Result<NewEmployee, Problem> {
        Ok(NewEmployee {
            user_id: merge_optional(self.user_id, base.user_id.map(|u| u.as_str().to_owned()))
                .map(UserId::new),
            staff_number: merge_optional(self.staff_number, base.staff_number),
            given_name: self.given_name.unwrap_or(base.given_name),
            family_name: self.family_name.unwrap_or(base.family_name),
            preferred_name: self.preferred_name.unwrap_or(base.preferred_name),
            work_email: merge_optional(self.work_email, base.work_email),
            work_phone: self.work_phone.unwrap_or(base.work_phone),
            personal_email: merge_optional(self.personal_email, base.personal_email),
            personal_phone: self.personal_phone.unwrap_or(base.personal_phone),
            date_of_birth: merge_optional_day(
                self.date_of_birth,
                base.date_of_birth,
                "dateOfBirth",
            )?,
            address_line1: self.address_line1.unwrap_or(base.address_line1),
            address_line2: self.address_line2.unwrap_or(base.address_line2),
            postal_code: self.postal_code.unwrap_or(base.postal_code),
            city: self.city.unwrap_or(base.city),
            region: self.region.unwrap_or(base.region),
            country: self.country.unwrap_or(base.country),
            national_id: merge_optional(self.national_id, base.national_id),
            iban: merge_optional(self.iban, base.iban),
            emergency_name: self.emergency_name.unwrap_or(base.emergency_name),
            emergency_phone: self.emergency_phone.unwrap_or(base.emergency_phone),
            manager_id: merge_optional(
                self.manager_id,
                base.manager_id.map(|m| m.as_str().to_owned()),
            )
            .map(HrEmployeeId::new),
            photo_node_id: merge_optional(
                self.photo_node_id,
                base.photo_node_id.map(|n| n.as_str().to_owned()),
            )
            .map(DriveNodeId::new),
        })
    }
}

/// The terms a person starts on.
///
/// `startedOn` is required and never defaulted to today: a start date is a fact
/// about a contract, and a silent "now" would make every balance folded from it
/// wrong in a way nobody notices until somebody counts their own days.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmploymentBody {
    #[serde(default)]
    job_title: Option<String>,
    #[serde(default)]
    team: Option<String>,
    #[serde(default)]
    contract_kind: Option<String>,
    started_on: String,
    #[serde(default)]
    ended_on: Option<String>,
    #[serde(default)]
    pattern_minutes: Option<Vec<i32>>,
    #[serde(default)]
    pay_amount_cents: Option<i64>,
    #[serde(default)]
    pay_period: Option<String>,
    #[serde(default)]
    pay_currency: Option<String>,
}

impl EmploymentBody {
    /// The store's input, with every word parsed by the store's own vocabulary
    /// so a spelling this build does not know is the caller's `422` rather than
    /// a row nothing can compute with.
    fn parse(self) -> Result<NewEmployment, Problem> {
        let base = NewEmployment::default();
        let started_on = parse_iso_date(self.started_on.trim()).ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "startedOn must be a date written YYYY-MM-DD",
            )
        })?;
        let ended_on = match self.ended_on.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(raw) => Some(parse_iso_date(raw).ok_or_else(|| {
                Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "endedOn must be a date written YYYY-MM-DD",
                )
            })?),
        };
        let pattern_minutes = match self.pattern_minutes {
            None => base.pattern_minutes,
            Some(stated) => <[i32; PATTERN_DAYS]>::try_from(stated.as_slice()).map_err(|_| {
                Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "patternMinutes must be seven numbers, Monday to Sunday",
                )
            })?,
        };
        Ok(NewEmployment {
            job_title: self.job_title.unwrap_or(base.job_title),
            team: self.team.unwrap_or(base.team),
            contract_kind: match self.contract_kind.as_deref() {
                None => base.contract_kind,
                Some(word) => ContractKind::parse(word).map_err(map_store_err)?,
            },
            started_on,
            ended_on,
            pattern_minutes,
            pay_amount_cents: self.pay_amount_cents,
            pay_period: match self.pay_period.as_deref() {
                None => base.pay_period,
                Some(word) => PayPeriod::parse(word).map_err(map_store_err)?,
            },
            pay_currency: self.pay_currency.unwrap_or(base.pay_currency),
        })
    }
}

/// Loads one of the tenant's employees through the HR door, or the `404` an id
/// from another tenant gets.
pub(crate) async fn load(hr: &TenantStore, id: &HrEmployeeId) -> Result<Employee, Problem> {
    hr.hr_employee(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such employee"))
}

/// Query string of the directory route.
#[derive(Deserialize)]
pub struct ListQuery {
    /// `includeArchived=1` also returns people who have left — **HR only**, and
    /// silently ignored for anybody else, because the directory a member reads
    /// is the people who are here.
    #[serde(default, rename = "includeArchived")]
    include_archived: Option<String>,
}

/// `GET /hr/employees[?includeArchived=1]` → `{"employees":[…]}` — the people
/// of this tenant, family name first.
///
/// **Every member gets this**, and it is the same projection either way: a type
/// with no private field on it. HR's read differs by one thing only — it can
/// include the people who have left.
///
/// # Errors
/// `401` without a valid bearer token.
pub async fn list_employees(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let is_hr = account.require_hr().is_ok();
    let people = if is_hr {
        state
            .store
            .for_tenant(account.tenant.clone())
            .hr_directory(flag(q.include_archived.as_deref()))
            .await
    } else {
        account.acc.hr_directory().await
    }
    .map_err(map_store_err)?;
    Ok(Json(json!({
        "employees": people.iter().map(directory_json).collect::<Vec<_>>(),
        "hr": is_hr,
    })))
}

/// `POST /hr/employees` `{givenName, familyName, …, employment?}` →
/// `{"employee":{…}, "employments":[…]}` — **HR only**: somebody joined.
///
/// The optional `employment` block is the terms they start on, written in the
/// same act: a person with no terms has no working pattern, so every leave
/// figure about them would be unanswerable until somebody remembered a second
/// call. It is read on create only — changing terms **appends** a period rather
/// than editing one, and a `PATCH` that appended would let a stale form restate
/// somebody's pay by resubmitting.
///
/// # Errors
/// `401`/`403` per the HR door; `409` when the staff number or the login is
/// already claimed; `422` on a field the caller can fix; `404` when the named
/// manager, user or photo node is not this tenant's.
pub async fn create_employee(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let mut req: EmployeeBody = parse_body(&body)?;
    let terms = req
        .employment
        .take()
        .map(EmploymentBody::parse)
        .transpose()?;
    let input = req.apply(NewEmployee::default())?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = hr
        .create_hr_employee(&input, &account.user)
        .await
        .map_err(map_store_err)?;
    if let Some(terms) = terms {
        hr.append_hr_employment(&id, &terms, &account.user)
            .await
            .map_err(map_store_err)?;
    }
    record(&hr, &id).await
}

/// `GET /hr/employees/{id}` → the record **the caller's door allows**.
///
/// HR (and a tenant admin) get the whole record and its employment history.
/// Every other member gets the same person as the directory shows them —
/// `{"employee":{public fields}}` — because a colleague's name, job title and
/// manager are what a workspace is for, and their address is not.
///
/// A person reading **their own** record through this route gets the directory
/// projection too; `GET /hr/me` is the own door and answers with everything.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the id is not this tenant's
/// (or, for a member, not in the directory) — never a `403`, which would
/// confirm the record exists.
pub async fn get_employee(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = HrEmployeeId::new(id);
    if account.require_hr().is_err() {
        let entry = account
            .acc
            .hr_directory()
            .await
            .map_err(map_store_err)?
            .into_iter()
            .find(|e| e.id.as_str() == id.as_str())
            .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such employee"))?;
        return Ok(Json(json!({ "employee": directory_json(&entry) })));
    }
    let hr = state.store.for_tenant(account.tenant.clone());
    record(&hr, &id).await
}

/// `PATCH /hr/employees/{id}` `{any writable field}` → `{"employee":{…}}` —
/// **HR only**: a new address, a corrected name, a different manager.
///
/// Merged onto the stored record, so an absent field is left alone and a
/// `null` clears a nullable one. Archiving is not a field here: it is
/// [`archive_employee`], for the reason `/billing/customers/{id}/archive`
/// established — an ordinary edit must never drop somebody out of the directory
/// because a stale form carried a flag.
///
/// The terms are not writable here either (see [`create_employee`]).
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the record, the named manager, the
/// user or the photo node is not this tenant's; `409` on a claimed staff number
/// or login; `422` on a field the caller can fix, including a manager link that
/// would close a cycle.
pub async fn update_employee(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let req: EmployeeBody = parse_body(&body)?;
    if req.employment.is_some() {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "terms are appended, not edited: this route changes the person only",
        ));
    }
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = HrEmployeeId::new(id);
    let stored = load(&hr, &id).await?;
    let input = req.apply(editable(&stored))?;
    hr.update_hr_employee(&id, &input)
        .await
        .map_err(map_store_err)?;
    record(&hr, &id).await
}

/// The body of the archive verb.
#[derive(Deserialize)]
struct ArchiveBody {
    /// `false` restores somebody archived by mistake. Required when a body is
    /// sent; an **empty** body archives, because the route's name is already
    /// the intent.
    archived: bool,
}

/// `POST /hr/employees/{id}/archive` `{"archived":true}` →
/// `{"employee":{…}}` — **HR only**: somebody left.
///
/// Archiving is the only removal HR performs. The record leaves the directory
/// and the org chart and stays readable through this door, because employment
/// records carry statutory retention in every member state; an erasure once a
/// retention period has genuinely expired is an admin's deliberate act taken
/// with legal advice, never something a route does.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the record is not this tenant's;
/// `409` naming how many direct reports must be reassigned first — the chart
/// hides archived people, so archiving a manager would silently cut a branch
/// off it and leave their reports with nobody to decide their leave.
pub async fn archive_employee(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let req: ArchiveBody = parse_body(if body.is_empty() {
        br#"{"archived":true}"#
    } else {
        &body
    })?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = HrEmployeeId::new(id);
    hr.set_hr_employee_archived(&id, req.archived)
        .await
        .map_err(map_store_err)?;
    record(&hr, &id).await
}

/// The HR door's answer about one person: the whole record and the terms
/// beneath it, which is what every write here returns so a client never has to
/// ask again for what it just changed.
pub(crate) async fn record(hr: &TenantStore, id: &HrEmployeeId) -> Result<Json<Value>, Problem> {
    let employee = load(hr, id).await?;
    let employments = hr.hr_employments(id).await.map_err(map_store_err)?;
    Ok(Json(json!({
        "employee": employee_json(&employee),
        "employments": employments.iter().map(employment_json).collect::<Vec<_>>(),
    })))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn body(value: Value) -> EmployeeBody {
        serde_json::from_value(value).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn stored() -> NewEmployee {
        NewEmployee {
            given_name: "Inès".to_owned(),
            family_name: "Dupont".to_owned(),
            city: "Bruxelles".to_owned(),
            national_id: Some("79061234567".to_owned()),
            ..Default::default()
        }
    }

    #[test]
    fn an_empty_patch_changes_nothing() {
        let merged = body(json!({}))
            .apply(stored())
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(merged.given_name, "Inès");
        assert_eq!(merged.city, "Bruxelles");
        assert_eq!(merged.national_id.as_deref(), Some("79061234567"));
    }

    #[test]
    fn an_explicit_null_clears_a_private_field_and_absence_does_not() {
        let cleared = body(json!({ "nationalId": null }))
            .apply(stored())
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(cleared.national_id, None, "a person can have it removed");

        let untouched = body(json!({ "city": "Antwerpen" }))
            .apply(stored())
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(untouched.national_id.as_deref(), Some("79061234567"));
        assert_eq!(untouched.city, "Antwerpen");
    }

    #[test]
    fn a_blank_string_means_the_same_as_null() {
        let merged = body(json!({ "nationalId": "   " }))
            .apply(stored())
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(merged.national_id, None);
    }

    #[test]
    fn a_date_of_birth_must_be_a_day_not_a_timestamp() {
        let refused = body(json!({ "dateOfBirth": "1988-03-02T00:00:00Z" })).apply(stored());
        assert!(refused.is_err(), "a timestamp is the caller's 422");

        let accepted = body(json!({ "dateOfBirth": "1988-03-02" }))
            .apply(stored())
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert!(accepted.date_of_birth.is_some());
    }

    #[test]
    fn a_working_pattern_is_seven_days_or_nothing() {
        let terms: EmploymentBody = serde_json::from_value(json!({
            "startedOn": "2026-01-01",
            "patternMinutes": [480, 480, 480, 480, 0, 0],
        }))
        .expect("body parses");
        assert!(terms.parse().is_err(), "six days is not a week");

        let terms: EmploymentBody = serde_json::from_value(json!({
            "startedOn": "2026-01-01",
            "patternMinutes": [480, 480, 480, 480, 300, 0, 0],
            "contractKind": "part_time",
        }))
        .expect("body parses");
        let parsed = terms.parse().unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(parsed.pattern_minutes[4], 300);
        assert_eq!(parsed.contract_kind, ContractKind::PartTime);
    }

    #[test]
    fn a_contract_kind_this_build_does_not_know_is_refused() {
        let terms: EmploymentBody = serde_json::from_value(json!({
            "startedOn": "2026-01-01",
            "contractKind": "zero_hours",
        }))
        .expect("body parses");
        assert!(terms.parse().is_err(), "an unknown word is a 422");
    }
}
