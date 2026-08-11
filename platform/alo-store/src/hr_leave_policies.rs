//! Leave policies — the kinds of time off a tenant grants, and the terms a
//! balance is folded from (alo HR, ADR 0035, wave B6.03a; `docs/design/hr.md`,
//! "Leave").
//!
//! # What a policy is, and what it is not
//!
//! A policy is a **rule**, not a number about a person: an entitlement per full
//! leave year at a full-time pattern, how that entitlement arrives over the
//! year, what carries into the next one, and whether an absence on it needs
//! deciding. Somebody's own figure is that rule scaled by their working pattern
//! and pro-rated by the days they were employed — computed by
//! [`crate::hr_leave_math`], never stored, so a balance can always be explained
//! by the policy and the employments that produced it.
//!
//! There is deliberately no balance column anywhere in this module. It is the
//! `qty_on_hand` rejection (B5.01) with somebody's holiday in it: one missed
//! decrement on a cancelled request and the number is wrong forever, with
//! nothing to reconcile it against.
//!
//! # Archived, rarely deleted
//!
//! A balance is only explicable beside the policy that produced it, so a policy
//! is retired by archiving. [`TenantStore::set_hr_leave_policy_archived`] takes
//! it out of the pickers and leaves every historical balance readable; a name
//! freed by archiving can be used again, because the uniqueness rule binds live
//! policies only.
//!
//! # The seed
//!
//! A tenant that presses nothing still has a workable annual policy:
//! [`TenantStore::ensure_hr_leave_policies`] creates one on first use, from the
//! statutory minimum of the tenant's country
//! ([`crate::hr_statutory_leave`]). A seeded entitlement is **a default, not
//! advice** — the screen that shows it says so, and it is editable from the
//! first minute. The alternative is a policy granting nothing, and a balance of
//! zero looks like an answer rather than an unanswered question.

use time::OffsetDateTime;

use crate::billing_field::{bounded, required};
use crate::error::{Result, StoreError};
use crate::hr_leave_math::{Accrual, ENTITLEMENT_MAX_MINUTES, LeaveYear};
use crate::hr_statutory_leave::seeded_entitlement_minutes;
use crate::id::{HrLeavePolicyId, UserId};
use crate::store::TenantStore;

/// The longest a policy's name may be: a label in a picker, not a paragraph.
pub const POLICY_NAME_MAX_CHARS: usize = 120;

/// The most months carried-over leave may survive before it lapses. Two years
/// is past every national rule alo has met (commonly 15 or 18 months), and a
/// larger figure is a typo rather than a policy.
pub const CARRYOVER_EXPIRY_MAX_MONTHS: i32 = 24;

/// The name a seeded annual policy gets when the caller offers none.
///
/// Callers with a locale — every HTTP route — pass the tenant's own word for it,
/// because a policy name is a string a person reads. This constant is the
/// fallback for a caller that has no locale at all (a test, a fixture, a
/// migration path), not a hardcoded product string.
pub const SEEDED_ANNUAL_POLICY_NAME: &str = "Annual leave";

/// What kind of time off a policy grants.
///
/// A closed vocabulary matched by the CHECK one layer down: a word no code knows
/// is a term nothing can compute with, and this one decides how the accrual, the
/// carryover and the approval rules are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveKind {
    /// Paid annual leave — the statutory entitlement, the one that accrues and
    /// carries over.
    Annual,
    /// Sickness absence. Usually recorded rather than approved, which is
    /// `requires_approval: false` rather than a special case in the code.
    Sick,
    /// Time off that is not paid and grants no entitlement.
    Unpaid,
    /// Everything else a tenant gives time off for: a moving day, a wedding,
    /// statutory family leave, a study day.
    OtherPaid,
}

impl LeaveKind {
    /// The stored word.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Annual => "annual",
            Self::Sick => "sick",
            Self::Unpaid => "unpaid",
            Self::OtherPaid => "other_paid",
        }
    }

    /// Reads a kind — from a request body or from a stored row.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "annual" => Ok(Self::Annual),
            "sick" => Ok(Self::Sick),
            "unpaid" => Ok(Self::Unpaid),
            "other_paid" => Ok(Self::OtherPaid),
            _ => Err(StoreError::Validation(
                "leave kind must be one of: annual, sick, unpaid, other_paid".to_owned(),
            )),
        }
    }
}

impl std::fmt::Display for LeaveKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The columns every read of a policy selects, in `PolicyRow` order.
const POLICY_COLS: &str = "id, name, kind, entitlement_minutes, accrual, leave_year_start_month, \
     leave_year_start_day, carryover_cap_minutes, carryover_expires_after_months, allow_negative, \
     requires_approval, paid, archived_at, created_by, created_at, updated_at";

/// The writable shape of a leave policy.
#[derive(Debug, Clone)]
pub struct NewLeavePolicy {
    /// The tenant's own word for it — "Vakantiedagen", "Congés payés".
    pub name: String,
    /// What kind of time off it grants.
    pub kind: LeaveKind,
    /// Minutes per full leave year **at a full-time pattern**. Zero is
    /// meaningful: an unpaid policy grants nothing and is bounded by approval.
    pub entitlement_minutes: i64,
    /// How the entitlement arrives over the year.
    pub accrual: Accrual,
    /// When the leave year begins.
    pub leave_year: LeaveYear,
    /// The most that may be carried into the next leave year; 0 = none.
    pub carryover_cap_minutes: i64,
    /// How long carried leave survives before it lapses; `None` = it does not.
    pub carryover_expires_after_months: Option<i32>,
    /// May an approval take a balance below zero?
    pub allow_negative: bool,
    /// Must an absence on this policy be decided by somebody? A sick policy is
    /// often recorded, not approved.
    pub requires_approval: bool,
    /// Is time off on this policy paid?
    pub paid: bool,
}

impl Default for NewLeavePolicy {
    /// Annual leave, monthly accrual, calendar leave year, nothing carried, paid
    /// and approved — the shape of the policy 90 % of tenants run, so a caller
    /// states only what differs.
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: LeaveKind::Annual,
            entitlement_minutes: 0,
            accrual: Accrual::Monthly,
            leave_year: LeaveYear::calendar(),
            carryover_cap_minutes: 0,
            carryover_expires_after_months: None,
            allow_negative: false,
            requires_approval: true,
            paid: true,
        }
    }
}

/// One stored leave policy.
#[derive(Debug, Clone)]
pub struct LeavePolicy {
    /// Opaque id, unique within the tenant.
    pub id: HrLeavePolicyId,
    /// The tenant's own word for it.
    pub name: String,
    /// What kind of time off it grants.
    pub kind: LeaveKind,
    /// Minutes per full leave year at a full-time pattern.
    pub entitlement_minutes: i64,
    /// How the entitlement arrives.
    pub accrual: Accrual,
    /// When the leave year begins.
    pub leave_year: LeaveYear,
    /// The carryover ceiling; 0 = none.
    pub carryover_cap_minutes: i64,
    /// How long carried leave survives; `None` = it does not lapse.
    pub carryover_expires_after_months: Option<i32>,
    /// May a balance go below zero on this policy?
    pub allow_negative: bool,
    /// Must an absence on it be decided?
    pub requires_approval: bool,
    /// Is it paid?
    pub paid: bool,
    /// When it was retired; `None` while the tenant runs it.
    pub archived_at: Option<OffsetDateTime>,
    /// Who created it.
    pub created_by: String,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time.
    pub updated_at: OffsetDateTime,
}

impl LeavePolicy {
    /// Whether the tenant still runs this policy.
    #[must_use]
    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }
}

/// A validated, normalised policy ready to be bound into a statement.
#[derive(Debug)]
struct Normalized {
    name: String,
    entitlement_minutes: i64,
    carryover_cap_minutes: i64,
    carryover_expires_after_months: Option<i32>,
}

/// Validates and normalises a policy. Pure — no database.
fn normalize(input: &NewLeavePolicy) -> Result<Normalized> {
    let name = required("leave policy name", &input.name, POLICY_NAME_MAX_CHARS)?;
    let entitlement = minutes_field("entitlement", input.entitlement_minutes)?;
    let carryover = minutes_field("carryover cap", input.carryover_cap_minutes)?;
    if carryover > entitlement && entitlement > 0 {
        return Err(StoreError::Validation(
            "carryover cap must not exceed the entitlement it carries".to_owned(),
        ));
    }
    if let Some(months) = input.carryover_expires_after_months {
        if !(1..=CARRYOVER_EXPIRY_MAX_MONTHS).contains(&months) {
            return Err(StoreError::Validation(format!(
                "carryover must expire between 1 and {CARRYOVER_EXPIRY_MAX_MONTHS} months after \
                 the leave year starts"
            )));
        }
        if carryover == 0 {
            return Err(StoreError::Validation(
                "a carryover expiry needs a carryover cap above zero".to_owned(),
            ));
        }
    }
    Ok(Normalized {
        name,
        entitlement_minutes: entitlement,
        carryover_cap_minutes: carryover,
        carryover_expires_after_months: input.carryover_expires_after_months,
    })
}

/// A minutes figure a policy may carry: never negative, never more than a year
/// of them.
fn minutes_field(field: &str, minutes: i64) -> Result<i64> {
    if !(0..=ENTITLEMENT_MAX_MINUTES).contains(&minutes) {
        return Err(StoreError::Validation(format!(
            "{field} must be between 0 and {ENTITLEMENT_MAX_MINUTES} minutes"
        )));
    }
    Ok(minutes)
}

/// Turns the policy table's uniqueness violation into an answer naming the
/// rule, and leaves every other database failure alone.
fn map_policy_conflict(error: sqlx::Error) -> StoreError {
    match error {
        sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23505") => {
            match db.constraint().unwrap_or_default() {
                "hr_leave_policies_name_unique" => {
                    StoreError::Conflict("a leave policy with this name already exists".to_owned())
                }
                _ => StoreError::Conflict("unique constraint".to_owned()),
            }
        }
        other => StoreError::Db(other),
    }
}

impl TenantStore {
    /// Creates a leave policy. **The HR door**: what a tenant grants is a
    /// tenant-wide rule, not something a manager sets for their own team.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a blank or over-long name, a figure outside
    /// 0..=[`ENTITLEMENT_MAX_MINUTES`] minutes, a carryover larger than the
    /// entitlement, or an expiry outside 1..=[`CARRYOVER_EXPIRY_MAX_MONTHS`]
    /// months; [`StoreError::Conflict`] when a live policy already has the name;
    /// [`StoreError::Db`] on failure.
    pub async fn create_hr_leave_policy(
        &self,
        input: &NewLeavePolicy,
        actor: &UserId,
    ) -> Result<HrLeavePolicyId> {
        let p = normalize(input)?;
        let id = HrLeavePolicyId::generate();
        sqlx::query(
            "INSERT INTO hr_leave_policies (tenant_id, id, name, kind, entitlement_minutes, \
                 accrual, leave_year_start_month, leave_year_start_day, carryover_cap_minutes, \
                 carryover_expires_after_months, allow_negative, requires_approval, paid, \
                 created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(&p.name)
        .bind(input.kind.as_str())
        .bind(p.entitlement_minutes)
        .bind(input.accrual.as_str())
        .bind(i16::from(input.leave_year.month()))
        .bind(i16::from(input.leave_year.day()))
        .bind(p.carryover_cap_minutes)
        .bind(p.carryover_expires_after_months)
        .bind(input.allow_negative)
        .bind(input.requires_approval)
        .bind(input.paid)
        .bind(actor.as_str())
        .execute(self.pool())
        .await
        .map_err(map_policy_conflict)?;
        Ok(id)
    }

    /// One policy of this tenant, or `None` — including when the id belongs to
    /// another tenant, which is indistinguishable by design.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the stored row carries a word this build
    /// does not know; [`StoreError::Db`] on failure.
    pub async fn hr_leave_policy(&self, id: &HrLeavePolicyId) -> Result<Option<LeavePolicy>> {
        let row = sqlx::query_as::<_, PolicyRow>(&format!(
            "SELECT {POLICY_COLS} FROM hr_leave_policies WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        row.map(PolicyRow::into_policy).transpose()
    }

    /// The tenant's policies, by name. Archived ones are excluded unless
    /// `include_archived`, and then sort after the live ones.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a stored word this build does not know;
    /// [`StoreError::Db`] on failure.
    pub async fn hr_leave_policies(&self, include_archived: bool) -> Result<Vec<LeavePolicy>> {
        let rows = sqlx::query_as::<_, PolicyRow>(&format!(
            "SELECT {POLICY_COLS} FROM hr_leave_policies \
              WHERE tenant_id = $1 AND ($2 OR archived_at IS NULL) \
              ORDER BY (archived_at IS NOT NULL), lower(name), id"
        ))
        .bind(self.tenant().as_str())
        .bind(include_archived)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(PolicyRow::into_policy).collect()
    }

    /// Replaces every writable field of a policy.
    ///
    /// **Editing a policy does not restate a balance that has already been
    /// taken.** A balance is folded from the policy as it is *now* for the leave
    /// year being asked about, which is why a tenant changing an entitlement
    /// mid-year sees this year's figures move — and why a policy that has served
    /// its purpose is archived and replaced rather than rewritten. The screen
    /// says so before it saves.
    ///
    /// An archived policy is not editable: it exists to explain balances already
    /// folded from it, and editing it would silently restate them. Restore it
    /// first.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the policy is not this tenant's;
    /// [`StoreError::Validation`] as for create; [`StoreError::Conflict`] when
    /// the policy is archived or another live policy has the name;
    /// [`StoreError::Db`] on failure.
    pub async fn update_hr_leave_policy(
        &self,
        id: &HrLeavePolicyId,
        input: &NewLeavePolicy,
    ) -> Result<()> {
        let p = normalize(input)?;
        let archived: Option<Option<OffsetDateTime>> = sqlx::query_scalar(
            "SELECT archived_at FROM hr_leave_policies WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        match archived {
            None => return Err(StoreError::NotFound),
            Some(Some(_)) => {
                return Err(StoreError::Conflict(
                    "an archived leave policy cannot be edited; restore it first".to_owned(),
                ));
            }
            Some(None) => {}
        }
        sqlx::query(
            "UPDATE hr_leave_policies SET name = $3, kind = $4, entitlement_minutes = $5, \
                 accrual = $6, leave_year_start_month = $7, leave_year_start_day = $8, \
                 carryover_cap_minutes = $9, carryover_expires_after_months = $10, \
                 allow_negative = $11, requires_approval = $12, paid = $13, updated_at = now() \
              WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(&p.name)
        .bind(input.kind.as_str())
        .bind(p.entitlement_minutes)
        .bind(input.accrual.as_str())
        .bind(i16::from(input.leave_year.month()))
        .bind(i16::from(input.leave_year.day()))
        .bind(p.carryover_cap_minutes)
        .bind(p.carryover_expires_after_months)
        .bind(input.allow_negative)
        .bind(input.requires_approval)
        .bind(input.paid)
        .execute(self.pool())
        .await
        .map_err(map_policy_conflict)?;
        Ok(())
    }

    /// Retires a policy, or brings it back.
    ///
    /// Archiving is how a policy is removed: a balance is only explicable beside
    /// the policy that produced it, so the row stays and leaves the pickers.
    /// Restoring is refused when the name has since been taken by another live
    /// policy — the uniqueness rule binds live policies, and two live
    /// "Vakantiedagen" would be two answers to "which balance is this?".
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the policy is not this tenant's;
    /// [`StoreError::Conflict`] when restoring would duplicate a live name;
    /// [`StoreError::Db`] on failure.
    pub async fn set_hr_leave_policy_archived(
        &self,
        id: &HrLeavePolicyId,
        archived: bool,
    ) -> Result<()> {
        let done = sqlx::query(
            "UPDATE hr_leave_policies \
                SET archived_at = CASE WHEN $3 THEN COALESCE(archived_at, now()) ELSE NULL END, \
                    updated_at = now() \
              WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(archived)
        .execute(self.pool())
        .await
        .map_err(map_policy_conflict)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// The tenant's live policies, seeding the first one if they have none.
    ///
    /// Called by every surface that needs a policy list, so a tenant who has
    /// pressed nothing still sees a workable annual policy rather than an empty
    /// screen with a plus button. The entitlement comes from the statutory
    /// minimum of the tenant's country as stated in their billing settings
    /// ([`crate::hr_statutory_leave`]) — **a default, not advice**, and editable
    /// from the first minute.
    ///
    /// `name` is the caller's word for annual leave in the reader's language; a
    /// blank one falls back to [`SEEDED_ANNUAL_POLICY_NAME`]. Idempotent: a
    /// tenant with any policy at all — even an archived one — is left alone, and
    /// two callers racing produce one policy, not two.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a stored word this build does not know;
    /// [`StoreError::Db`] on failure.
    pub async fn ensure_hr_leave_policies(
        &self,
        name: &str,
        actor: &UserId,
    ) -> Result<Vec<LeavePolicy>> {
        let any: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM hr_leave_policies WHERE tenant_id = $1)",
        )
        .bind(self.tenant().as_str())
        .fetch_one(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if !any {
            let country: Option<String> =
                sqlx::query_scalar("SELECT country FROM billing_settings WHERE tenant_id = $1")
                    .bind(self.tenant().as_str())
                    .fetch_optional(self.pool())
                    .await
                    .map_err(StoreError::Db)?;
            let name = bounded("leave policy name", name, POLICY_NAME_MAX_CHARS)?;
            let seed = NewLeavePolicy {
                name: if name.is_empty() {
                    SEEDED_ANNUAL_POLICY_NAME.to_owned()
                } else {
                    name
                },
                kind: LeaveKind::Annual,
                entitlement_minutes: seeded_entitlement_minutes(&country.unwrap_or_default()),
                ..NewLeavePolicy::default()
            };
            match self.create_hr_leave_policy(&seed, actor).await {
                Ok(_) => {}
                // Another caller seeded first. One policy is the outcome either
                // way, which is what "ensure" promises.
                Err(StoreError::Conflict(_)) => {}
                Err(other) => return Err(other),
            }
        }
        self.hr_leave_policies(false).await
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct PolicyRow {
    id: String,
    name: String,
    kind: String,
    entitlement_minutes: i64,
    accrual: String,
    leave_year_start_month: i16,
    leave_year_start_day: i16,
    carryover_cap_minutes: i64,
    carryover_expires_after_months: Option<i32>,
    allow_negative: bool,
    requires_approval: bool,
    paid: bool,
    archived_at: Option<OffsetDateTime>,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl PolicyRow {
    /// Fallible on purpose: a stored word or a stored leave-year start this
    /// build does not know is a schema disagreement, and answering with a
    /// guessed policy would be worse than answering with an error.
    fn into_policy(self) -> Result<LeavePolicy> {
        let month = u8::try_from(self.leave_year_start_month).map_err(|_| {
            StoreError::Validation("stored leave year start month is out of range".to_owned())
        })?;
        let day = u8::try_from(self.leave_year_start_day).map_err(|_| {
            StoreError::Validation("stored leave year start day is out of range".to_owned())
        })?;
        Ok(LeavePolicy {
            id: HrLeavePolicyId::new(self.id),
            name: self.name,
            kind: LeaveKind::parse(&self.kind)?,
            entitlement_minutes: self.entitlement_minutes,
            accrual: Accrual::parse(&self.accrual)?,
            leave_year: LeaveYear::new(month, day)?,
            carryover_cap_minutes: self.carryover_cap_minutes,
            carryover_expires_after_months: self.carryover_expires_after_months,
            allow_negative: self.allow_negative,
            requires_approval: self.requires_approval,
            paid: self.paid,
            archived_at: self.archived_at,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::hr_leave_math::LEAVE_YEAR_START_DAY_MAX;

    fn annual() -> NewLeavePolicy {
        NewLeavePolicy {
            name: "Vakantiedagen".to_owned(),
            entitlement_minutes: 20 * 480,
            ..Default::default()
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(message)) => message,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn the_vocabulary_is_closed_and_round_trips() {
        for kind in [
            LeaveKind::Annual,
            LeaveKind::Sick,
            LeaveKind::Unpaid,
            LeaveKind::OtherPaid,
        ] {
            assert_eq!(LeaveKind::parse(kind.as_str()).unwrap(), kind);
            assert_eq!(kind.to_string(), kind.as_str());
        }
        assert!(invalid(LeaveKind::parse("holiday")).contains("annual"));
    }

    #[test]
    fn a_policy_needs_a_name_and_a_plausible_entitlement() {
        assert!(normalize(&annual()).is_ok());
        let nameless = NewLeavePolicy {
            name: "   ".to_owned(),
            ..annual()
        };
        assert!(invalid(normalize(&nameless)).contains("leave policy name"));
        let negative = NewLeavePolicy {
            entitlement_minutes: -1,
            ..annual()
        };
        assert!(invalid(normalize(&negative)).contains("entitlement"));
        let absurd = NewLeavePolicy {
            entitlement_minutes: ENTITLEMENT_MAX_MINUTES + 1,
            ..annual()
        };
        assert!(invalid(normalize(&absurd)).contains("entitlement"));
        // Nothing granted is a real policy: unpaid leave, and sick leave in a
        // tenant that records it without a balance.
        let unpaid = NewLeavePolicy {
            kind: LeaveKind::Unpaid,
            entitlement_minutes: 0,
            paid: false,
            allow_negative: true,
            ..annual()
        };
        assert!(normalize(&unpaid).is_ok());
    }

    #[test]
    fn a_carryover_cannot_exceed_what_it_carries_or_expire_without_one() {
        let too_much = NewLeavePolicy {
            carryover_cap_minutes: 21 * 480,
            ..annual()
        };
        assert!(invalid(normalize(&too_much)).contains("carryover cap"));
        let capped = NewLeavePolicy {
            carryover_cap_minutes: 5 * 480,
            carryover_expires_after_months: Some(15),
            ..annual()
        };
        assert!(normalize(&capped).is_ok());
        let expiry_without_cap = NewLeavePolicy {
            carryover_expires_after_months: Some(15),
            ..annual()
        };
        assert!(invalid(normalize(&expiry_without_cap)).contains("carryover cap"));
        let too_long = NewLeavePolicy {
            carryover_cap_minutes: 5 * 480,
            carryover_expires_after_months: Some(CARRYOVER_EXPIRY_MAX_MONTHS + 1),
            ..annual()
        };
        assert!(invalid(normalize(&too_long)).contains("expire"));
        let too_soon = NewLeavePolicy {
            carryover_cap_minutes: 5 * 480,
            carryover_expires_after_months: Some(0),
            ..annual()
        };
        assert!(invalid(normalize(&too_soon)).contains("expire"));
    }

    #[test]
    fn a_leave_year_start_is_bounded_where_the_schema_bounds_it() {
        assert!(LeaveYear::new(4, 6).is_ok());
        assert!(LeaveYear::new(1, LEAVE_YEAR_START_DAY_MAX).is_ok());
        assert!(LeaveYear::new(1, LEAVE_YEAR_START_DAY_MAX + 1).is_err());
    }
}
