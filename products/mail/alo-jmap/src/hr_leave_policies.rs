//! The leave a tenant grants, over HTTP (alo HR, ADR 0035, wave B6.03b) — over
//! [`alo_store::hr_leave_policies`].
//!
//! Three decisions this file makes, each of them about a door rather than about
//! a field:
//!
//! - **Reading the policies is every member's.** The design's route table says
//!   HR, and building the request form proved it wrong: somebody asking for
//!   time off has to choose what kind, and a picker they may not read is a form
//!   they cannot fill in. What a company grants is a *rule it publishes to its
//!   staff*, not a secret — so the list is readable by anybody with a login and
//!   writable only through the HR door. `includeArchived` stays HR's, because a
//!   retired policy is history rather than a choice.
//! - **Writing is `require_hr`.** What a tenant grants is a tenant-wide rule,
//!   never something a manager sets for their own team.
//! - **`DELETE` is not implemented; archiving is** (the B6.03a decision, now
//!   binding: a request can be on a policy, so "delete only while nothing has
//!   ever been on it" would refuse almost always and confuse otherwise). The
//!   verb is `POST /hr/leave-policies/{id}/archive`, the shape
//!   `/hr/employees/{id}/archive` and `/billing/customers/{id}/archive`
//!   already have.
//!
//! A policy carries no personal data, so — unusually for this module —
//! everything here is safe to name in an error. Nothing is logged all the same.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::hr_leave_math::{Accrual, LeaveYear};
use alo_store::hr_leave_policies::{LeaveKind, NewLeavePolicy, SEEDED_ANNUAL_POLICY_NAME};
use alo_store::{HrLeavePolicyId, LeavePolicy, TenantStore};

use crate::billing::{flag, iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// One policy as JSON — every term a balance is folded from, so a screen can
/// show the working beside the figure.
pub(crate) fn policy_json(policy: &LeavePolicy) -> Value {
    json!({
        "id": policy.id.as_str(),
        "name": policy.name,
        "kind": policy.kind.as_str(),
        "entitlementMinutes": policy.entitlement_minutes,
        "accrual": policy.accrual.as_str(),
        "leaveYearStartMonth": policy.leave_year.month(),
        "leaveYearStartDay": policy.leave_year.day(),
        "carryoverCapMinutes": policy.carryover_cap_minutes,
        "carryoverExpiresAfterMonths": policy.carryover_expires_after_months,
        "allowNegative": policy.allow_negative,
        "requiresApproval": policy.requires_approval,
        "paid": policy.paid,
        "archived": policy.is_archived(),
        "archivedAt": policy.archived_at.map(iso),
        "createdAt": iso(policy.created_at),
        "updatedAt": iso(policy.updated_at),
    })
}

/// The writable fields of a policy. Absent fields keep what is stored, so a
/// `PATCH` carrying one field changes one field.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PolicyBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    entitlement_minutes: Option<i64>,
    #[serde(default)]
    accrual: Option<String>,
    #[serde(default)]
    leave_year_start_month: Option<u8>,
    #[serde(default)]
    leave_year_start_day: Option<u8>,
    #[serde(default)]
    carryover_cap_minutes: Option<i64>,
    /// `null` clears the expiry; absent leaves it alone. Spelled as a nested
    /// option for the same reason the employee record spells its private fields
    /// that way — otherwise an expiry entered by mistake could never come off.
    #[serde(default, deserialize_with = "crate::billing::absent_or_null")]
    carryover_expires_after_months: Option<Option<i32>>,
    #[serde(default)]
    allow_negative: Option<bool>,
    #[serde(default)]
    requires_approval: Option<bool>,
    #[serde(default)]
    paid: Option<bool>,
}

impl PolicyBody {
    /// Merges the stated fields onto `base`.
    fn apply(self, base: NewLeavePolicy) -> Result<NewLeavePolicy, Problem> {
        let month = self
            .leave_year_start_month
            .unwrap_or(base.leave_year.month());
        let day = self.leave_year_start_day.unwrap_or(base.leave_year.day());
        Ok(NewLeavePolicy {
            name: self.name.unwrap_or(base.name),
            kind: match self.kind.as_deref() {
                None => base.kind,
                Some(word) => LeaveKind::parse(word).map_err(map_store_err)?,
            },
            entitlement_minutes: self.entitlement_minutes.unwrap_or(base.entitlement_minutes),
            accrual: match self.accrual.as_deref() {
                None => base.accrual,
                Some(word) => Accrual::parse(word).map_err(map_store_err)?,
            },
            leave_year: LeaveYear::new(month, day).map_err(map_store_err)?,
            carryover_cap_minutes: self
                .carryover_cap_minutes
                .unwrap_or(base.carryover_cap_minutes),
            carryover_expires_after_months: match self.carryover_expires_after_months {
                None => base.carryover_expires_after_months,
                Some(stated) => stated,
            },
            allow_negative: self.allow_negative.unwrap_or(base.allow_negative),
            requires_approval: self.requires_approval.unwrap_or(base.requires_approval),
            paid: self.paid.unwrap_or(base.paid),
        })
    }
}

/// The stored policy as writable input — the base a `PATCH` merges onto.
fn editable(policy: &LeavePolicy) -> NewLeavePolicy {
    NewLeavePolicy {
        name: policy.name.clone(),
        kind: policy.kind,
        entitlement_minutes: policy.entitlement_minutes,
        accrual: policy.accrual,
        leave_year: policy.leave_year,
        carryover_cap_minutes: policy.carryover_cap_minutes,
        carryover_expires_after_months: policy.carryover_expires_after_months,
        allow_negative: policy.allow_negative,
        requires_approval: policy.requires_approval,
        paid: policy.paid,
    }
}

/// Loads one of the tenant's policies, or the `404` an id from another tenant
/// gets.
pub(crate) async fn load(hr: &TenantStore, id: &HrLeavePolicyId) -> Result<LeavePolicy, Problem> {
    hr.hr_leave_policy(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such leave policy"))
}

/// Query string of the list route.
#[derive(Deserialize)]
pub struct ListQuery {
    /// `includeArchived=1` also returns retired policies — **HR only**, and
    /// silently ignored for anybody else.
    #[serde(default, rename = "includeArchived")]
    include_archived: Option<String>,
}

/// `GET /hr/leave-policies[?includeArchived=1]` → `{"policies":[…], "hr":bool}`
/// — what this tenant grants.
///
/// **Seeds on first read.** A tenant who has pressed nothing gets one workable
/// annual policy from the statutory minimum of their country rather than an
/// empty screen with a plus button, and the client says plainly that it is a
/// starting point to edit (`docs/design/hr.md`, "Policies"). The name is passed
/// in the reader's language by the client; a request that offers none gets the
/// store's fallback.
///
/// # Errors
/// `401` without a valid bearer token.
pub async fn list_policies(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let is_hr = account.require_hr().is_ok();
    let hr = state.store.for_tenant(account.tenant.clone());
    let policies = if is_hr && flag(q.include_archived.as_deref()) {
        hr.hr_leave_policies(true).await
    } else {
        // Seeding on a member's read is deliberate: the first person to ask for
        // leave in a new tenant must find something to ask against, and waiting
        // for an HR user to open a screen would make that person's first act a
        // dead end.
        hr.ensure_hr_leave_policies(SEEDED_ANNUAL_POLICY_NAME, &account.user)
            .await
    }
    .map_err(map_store_err)?;
    Ok(Json(json!({
        "policies": policies.iter().map(policy_json).collect::<Vec<_>>(),
        "hr": is_hr,
    })))
}

/// `POST /hr/leave-policies` `{name, kind, entitlementMinutes, …}` →
/// `{"policy":{…}}` — **HR only**: a new kind of time off.
///
/// # Errors
/// `401`/`403` per the HR door; `409` when a live policy already has the name;
/// `422` on a figure or a word the caller can fix.
pub async fn create_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let req: PolicyBody = parse_body(&body)?;
    let input = req.apply(NewLeavePolicy::default())?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = hr
        .create_hr_leave_policy(&input, &account.user)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "policy": policy_json(&load(&hr, &id).await?) }),
    ))
}

/// `GET /hr/leave-policies/{id}` → `{"policy":{…}}` — one rule, in full.
///
/// Every member may read it: a balance is only explicable beside the policy that
/// produced it, and an employee is entitled to both halves of their own figure.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the id is not this tenant's.
pub async fn get_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let policy = load(&hr, &HrLeavePolicyId::new(id)).await?;
    Ok(Json(json!({ "policy": policy_json(&policy) })))
}

/// `PATCH /hr/leave-policies/{id}` `{any writable field}` → `{"policy":{…}}` —
/// **HR only**.
///
/// Editing a policy does not restate leave already taken: a balance is folded
/// from the policy as it is *now* for the year being asked about, which is why
/// a tenant changing an entitlement mid-year sees this year's figures move. The
/// screen says so before it saves, and a policy that has served its purpose is
/// archived and replaced rather than rewritten.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the policy is not this tenant's;
/// `409` when it is archived or the name is taken; `422` on a figure the caller
/// can fix.
pub async fn update_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let req: PolicyBody = parse_body(&body)?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = HrLeavePolicyId::new(id);
    let stored = load(&hr, &id).await?;
    let input = req.apply(editable(&stored))?;
    hr.update_hr_leave_policy(&id, &input)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "policy": policy_json(&load(&hr, &id).await?) }),
    ))
}

/// The body of the archive verb.
#[derive(Deserialize)]
struct ArchiveBody {
    /// `false` restores a policy retired by mistake. An **empty** body archives,
    /// because the route's name is already the intent.
    archived: bool,
}

/// `POST /hr/leave-policies/{id}/archive` `{"archived":true}` →
/// `{"policy":{…}}` — **HR only**: this is how a policy is removed.
///
/// A balance is only explicable beside the policy that produced it, so the row
/// stays and leaves the pickers. Restoring is refused while another live policy
/// has taken the name.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the policy is not this tenant's;
/// `409` when restoring would duplicate a live name.
pub async fn archive_policy(
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
    let id = HrLeavePolicyId::new(id);
    hr.set_hr_leave_policy_archived(&id, req.archived)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "policy": policy_json(&load(&hr, &id).await?) }),
    ))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn body(value: Value) -> PolicyBody {
        serde_json::from_value(value).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn stored() -> NewLeavePolicy {
        NewLeavePolicy {
            name: "Vakantiedagen".to_owned(),
            entitlement_minutes: 25 * 480,
            carryover_cap_minutes: 5 * 480,
            carryover_expires_after_months: Some(15),
            ..Default::default()
        }
    }

    #[test]
    fn an_empty_patch_changes_nothing() {
        let merged = body(json!({}))
            .apply(stored())
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(merged.name, "Vakantiedagen");
        assert_eq!(merged.entitlement_minutes, 25 * 480);
        assert_eq!(merged.carryover_expires_after_months, Some(15));
    }

    #[test]
    fn an_explicit_null_clears_the_expiry_and_absence_does_not() {
        let cleared = body(json!({ "carryoverExpiresAfterMonths": null }))
            .apply(stored())
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(cleared.carryover_expires_after_months, None);
        let kept = body(json!({ "name": "Vakantie" }))
            .apply(stored())
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(kept.carryover_expires_after_months, Some(15));
        assert_eq!(kept.name, "Vakantie");
    }

    #[test]
    fn a_word_this_build_does_not_know_is_refused() {
        assert!(
            body(json!({ "kind": "sabbatical" }))
                .apply(stored())
                .is_err()
        );
        assert!(
            body(json!({ "accrual": "weekly" }))
                .apply(stored())
                .is_err()
        );
        // 29 February is a leave-year start three years in four cannot build.
        assert!(
            body(json!({ "leaveYearStartMonth": 2, "leaveYearStartDay": 29 }))
                .apply(stored())
                .is_err()
        );
        let april = body(json!({ "leaveYearStartMonth": 4, "leaveYearStartDay": 6 }))
            .apply(stored())
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(april.leave_year.month(), 4);
        assert_eq!(april.leave_year.day(), 6);
    }
}
