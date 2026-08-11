//! What somebody has left (alo HR, ADR 0035, wave B6.03b; `docs/design/hr.md`,
//! "The arithmetic, and the property it must have").
//!
//! **Nothing here is stored.** A balance is folded, every time it is asked for,
//! from four things that are: the policy, the employments that were in force,
//! the requests on the record, and the day being asked about. That is the whole
//! design — a `balance_minutes` column decremented on approval is the
//! `qty_on_hand` mistake (B5.01) with somebody's holiday in it, and the person
//! it goes wrong for is the one person guaranteed to check it by hand.
//!
//! The arithmetic itself is [`crate::hr_leave_math`], which has no database in
//! it. This module is the part that reads: it fills a
//! [`LeaveLedger`](crate::hr_leave_math::LeaveLedger) out of Postgres and calls
//! the same [`balance`] the tests pin, so **the figure a screen shows and the
//! figure an approval is checked against are one fold** rather than two
//! implementations that agree until they don't.
//!
//! # The five steps, in order
//!
//! 1. **Scale** the policy's full-year entitlement to this person's working
//!    pattern (a three-day week gets three fifths of it).
//! 2. **Pro-rate** it by the days they were employed inside the leave year, so a
//!    joiner and a leaver whose employments partition a year get two figures
//!    that sum to exactly one.
//! 3. **Accrue** to the day asked about — the whole entitlement up front, or a
//!    twelfth at each month start with the remainder carried.
//! 4. **Charge** every approved day inside the leave year at the pattern in
//!    force on that day: passed days are *taken*, days still ahead are *booked*.
//!    Requests nobody has decided are reported as *pending* and deducted from
//!    nothing.
//! 5. **Carry in** last year's remainder, capped by the policy and dropped once
//!    it has lapsed.
//!
//! **Carryover carries one year, not a chain.** Last year's remainder is folded
//! with nothing carried into *it*: a day granted in 2024, unused in 2025 and
//! still claimed in 2026 is exactly what the statutory expiry rules exist to
//! stop, and every member state alo has met caps carryover at 15 or 18 months.
//! A tenant who genuinely wants a longer chain is a design question with a
//! statutory answer, not a recursion depth.

use time::Date;

use crate::error::{Result, StoreError};
use crate::hr_employments::{Employment, FULL_TIME_PATTERN};
use crate::hr_leave_math::{
    Balance, LeaveLedger, accrued_minutes, average_working_day_minutes, balance,
    carried_in_minutes, carryover_expires_on, prorated_entitlement_minutes,
    scaled_entitlement_minutes, weekly_minutes,
};
use crate::hr_leave_policies::LeavePolicy;
use crate::hr_leave_requests::{LeaveRequest, LeaveRequestQuery, LeaveStatus};
use crate::id::{HrEmployeeId, HrLeavePolicyId};
use crate::store::TenantStore;

/// One policy and what this person has left on it, with the working that
/// produced the figure.
#[derive(Debug, Clone)]
pub struct PolicyBalance {
    /// The policy the balance is folded from. Returned beside the figure
    /// because a balance is only explicable next to the rule that produced it.
    pub policy: LeavePolicy,
    /// The figure, in minutes, every component visible.
    pub balance: Balance,
    /// Minutes in this person's average working day, on the pattern in force on
    /// the day asked about. It is the divisor a screen showing "12.5 days" must
    /// use, and it is computed here so the client never invents one.
    pub average_day_minutes: i32,
}

/// The employment in force on `day`, or the nearest one before it, or the first
/// one there is.
///
/// A balance asked about a day somebody was not employed on still needs a
/// pattern to scale by — the alternative is a zero entitlement for a leaver,
/// which reads as "you were owed nothing" rather than "you had left".
fn pattern_on(employments: &[Employment], day: Date) -> Option<&Employment> {
    employments
        .iter()
        .find(|employment| employment.covers(day))
        .or_else(|| {
            employments
                .iter()
                .filter(|employment| employment.started_on <= day)
                .max_by_key(|employment| employment.started_on)
        })
        .or_else(|| {
            employments
                .iter()
                .min_by_key(|employment| employment.started_on)
        })
}

/// Folds one leave year into a balance. **Pure** — the same shape the arithmetic
/// module has, so the whole of the fold above can be pinned without a fixture.
///
/// `requests` may carry any of this person's requests in any state; only the
/// live ones on this policy, on days inside `year`, are charged.
#[must_use]
pub fn fold_leave_year(
    policy: &LeavePolicy,
    employments: &[Employment],
    requests: &[LeaveRequest],
    year: (Date, Date),
    as_of: Date,
    carried_in: i64,
) -> Balance {
    let weekly = pattern_on(employments, as_of).map_or(0, Employment::weekly_minutes);
    let scaled = scaled_entitlement_minutes(
        policy.entitlement_minutes,
        weekly,
        weekly_minutes(&FULL_TIME_PATTERN),
    );
    let employed_from = employments.iter().map(|e| e.started_on).min();
    // Only a history with no open period has a last day; anybody still employed
    // is employed to the end of the year.
    let employed_to = if employments.iter().all(|e| e.ended_on.is_some()) {
        employments.iter().filter_map(|e| e.ended_on).max()
    } else {
        None
    };
    let entitlement = prorated_entitlement_minutes(scaled, year, employed_from, employed_to);

    let mut ledger = LeaveLedger {
        entitlement_minutes: entitlement,
        carried_in_minutes: carried_in,
        accrued_minutes: accrued_minutes(entitlement, policy.accrual, year.0, as_of),
        ..LeaveLedger::default()
    };
    for request in requests {
        if request.policy_id != policy.id || !request.status.is_live() {
            continue;
        }
        let from = request.from_day.max(year.0);
        let to = request.to_day.min(year.1);
        if to < from {
            continue;
        }
        // Charged day by day rather than as a range, because a request that
        // straddles the leave year's edge must charge each year its own part —
        // and because the pattern can change inside it.
        let mut day = from;
        while day <= to {
            let minutes = i64::from(
                employments
                    .iter()
                    .find(|employment| employment.covers(day))
                    .map_or(0, |employment| employment.minutes_on(day))
                    .max(0),
            );
            match request.status {
                LeaveStatus::Approved if day <= as_of => ledger.taken_minutes += minutes,
                LeaveStatus::Approved => ledger.booked_minutes += minutes,
                _ => ledger.pending_minutes += minutes,
            }
            match day.next_day() {
                Some(next) => day = next,
                None => break,
            }
        }
    }
    balance(&ledger)
}

impl TenantStore {
    /// What one person has left on one policy, as at `on`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the policy is not this tenant's;
    /// [`StoreError::Validation`] on a stored word this build does not know;
    /// [`StoreError::Db`] on failure.
    pub async fn hr_leave_balance(
        &self,
        employee: &HrEmployeeId,
        policy: &HrLeavePolicyId,
        on: Date,
    ) -> Result<Balance> {
        let policy = self
            .hr_leave_policy(policy)
            .await?
            .ok_or(StoreError::NotFound)?;
        let employments = self.hr_employments(employee).await?;
        let requests = self.leave_history(employee, &policy, on).await?;
        Ok(self.fold(&policy, &employments, &requests, on).balance)
    }

    /// What one person has left on **every live policy** the tenant runs, as at
    /// `on` — the read behind their own screen and behind a manager deciding
    /// their request.
    ///
    /// Policies with no live status are left out: a retired policy explains
    /// history, and a balance on it is not something anybody can spend.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a stored word this build does not know;
    /// [`StoreError::Db`] on failure.
    pub async fn hr_leave_balances(
        &self,
        employee: &HrEmployeeId,
        on: Date,
    ) -> Result<Vec<PolicyBalance>> {
        let policies = self.hr_leave_policies(false).await?;
        let employments = self.hr_employments(employee).await?;
        // One read of the requests for every policy, rather than one per policy:
        // the fold ignores the requests that are not its own.
        let earliest = policies
            .iter()
            .map(|policy| previous_year(policy, on).0)
            .min();
        let requests = match earliest {
            None => Vec::new(),
            Some(from) => {
                self.hr_leave_requests(
                    &LeaveRequestQuery::for_employee(employee)
                        .with_statuses(&[LeaveStatus::Requested, LeaveStatus::Approved])
                        .within(from, latest_year_end(&policies, on)),
                )
                .await?
            }
        };
        Ok(policies
            .into_iter()
            .map(|policy| self.fold(&policy, &employments, &requests, on))
            .collect())
    }

    /// The two folds — last year's remainder, then this year's — for one policy.
    fn fold(
        &self,
        policy: &LeavePolicy,
        employments: &[Employment],
        requests: &[LeaveRequest],
        on: Date,
    ) -> PolicyBalance {
        let year = policy.leave_year.window(on);
        let previous = previous_year(policy, on);
        // Last year is folded with nothing carried into it (see the module
        // docs): carryover carries one year, not a chain.
        let last = fold_leave_year(policy, employments, requests, previous, previous.1, 0);
        let carried = carried_in_minutes(
            last.remaining_minutes,
            policy.carryover_cap_minutes,
            carryover_expires_on(year.0, policy.carryover_expires_after_months),
            on,
        );
        PolicyBalance {
            balance: fold_leave_year(policy, employments, requests, year, on, carried),
            average_day_minutes: pattern_on(employments, on).map_or(0, |employment| {
                average_working_day_minutes(&employment.pattern_minutes)
            }),
            policy: policy.clone(),
        }
    }

    /// The live requests that could touch either leave year of one policy.
    async fn leave_history(
        &self,
        employee: &HrEmployeeId,
        policy: &LeavePolicy,
        on: Date,
    ) -> Result<Vec<LeaveRequest>> {
        let year = policy.leave_year.window(on);
        let previous = previous_year(policy, on);
        self.hr_leave_requests(
            &LeaveRequestQuery::for_employee(employee)
                .with_statuses(&[LeaveStatus::Requested, LeaveStatus::Approved])
                .within(previous.0, year.1),
        )
        .await
    }
}

/// The leave year before the one containing `on`.
fn previous_year(policy: &LeavePolicy, on: Date) -> (Date, Date) {
    let year = policy.leave_year.window(on);
    let last = year.0.previous_day().unwrap_or(year.0);
    policy.leave_year.window(last)
}

/// The furthest leave-year end any of these policies reaches on `on` — the
/// upper bound of the one request read that serves them all.
fn latest_year_end(policies: &[LeavePolicy], on: Date) -> Date {
    policies
        .iter()
        .map(|policy| policy.leave_year.window(on).1)
        .max()
        .unwrap_or(on)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::hr_employments::{ContractKind, PayPeriod};
    use crate::hr_leave_math::{Accrual, LeaveYear};
    use crate::hr_leave_policies::{LeaveKind, NewLeavePolicy};
    use crate::hr_leave_requests::LeaveStatus;
    use crate::id::{HrEmploymentId, HrLeaveRequestId};
    use time::{Month, OffsetDateTime};

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("a real date")
    }

    fn policy(input: NewLeavePolicy) -> LeavePolicy {
        LeavePolicy {
            id: HrLeavePolicyId::new("pol"),
            name: input.name,
            kind: input.kind,
            entitlement_minutes: input.entitlement_minutes,
            accrual: input.accrual,
            leave_year: input.leave_year,
            carryover_cap_minutes: input.carryover_cap_minutes,
            carryover_expires_after_months: input.carryover_expires_after_months,
            allow_negative: input.allow_negative,
            requires_approval: input.requires_approval,
            paid: input.paid,
            archived_at: None,
            created_by: String::new(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn annual() -> LeavePolicy {
        policy(NewLeavePolicy {
            name: "Annual leave".to_owned(),
            kind: LeaveKind::Annual,
            entitlement_minutes: 25 * 480,
            accrual: Accrual::UpFront,
            leave_year: LeaveYear::calendar(),
            ..NewLeavePolicy::default()
        })
    }

    fn employment(from: Date, to: Option<Date>, pattern: [i32; 7]) -> Employment {
        Employment {
            id: HrEmploymentId::new("emp"),
            employee_id: HrEmployeeId::new("person"),
            job_title: String::new(),
            team: String::new(),
            contract_kind: ContractKind::Permanent,
            started_on: from,
            ended_on: to,
            pattern_minutes: pattern,
            pay_amount_cents: None,
            pay_period: PayPeriod::Month,
            pay_currency: "EUR".to_owned(),
            created_by: String::new(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn request(from: Date, to: Date, status: LeaveStatus) -> LeaveRequest {
        LeaveRequest {
            id: HrLeaveRequestId::new("req"),
            employee_id: HrEmployeeId::new("person"),
            employee_name: String::new(),
            policy_id: HrLeavePolicyId::new("pol"),
            policy_name: String::new(),
            from_day: from,
            to_day: to,
            status,
            note: String::new(),
            requested_by: String::new(),
            decided_by: None,
            decided_at: None,
            decision_note: String::new(),
            closed_by: None,
            closed_at: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            cost: crate::hr_leave_math::RequestCost::default(),
        }
    }

    /// The identity the whole module exists to keep:
    /// `remaining = carried_in + accrued − taken − booked`, with pending beside
    /// it and never inside it.
    #[test]
    fn taken_is_behind_booked_is_ahead_and_pending_costs_nothing() {
        let terms = vec![employment(
            day(2020, Month::January, 1),
            None,
            FULL_TIME_PATTERN,
        )];
        let requests = vec![
            // Mon 2 – Fri 6 March: five days, already past.
            request(
                day(2026, Month::March, 2),
                day(2026, Month::March, 6),
                LeaveStatus::Approved,
            ),
            // Mon 6 – Tue 7 July: two days, still ahead.
            request(
                day(2026, Month::July, 6),
                day(2026, Month::July, 7),
                LeaveStatus::Approved,
            ),
            // Mon 3 – Wed 5 August: three days nobody has decided.
            request(
                day(2026, Month::August, 3),
                day(2026, Month::August, 5),
                LeaveStatus::Approved,
            ),
        ];
        let mut requests = requests;
        requests[2].status = LeaveStatus::Requested;

        let as_of = day(2026, Month::June, 1);
        let folded = fold_leave_year(
            &annual(),
            &terms,
            &requests,
            LeaveYear::calendar().window(as_of),
            as_of,
            0,
        );
        assert_eq!(folded.entitlement_minutes, 25 * 480);
        assert_eq!(folded.accrued_minutes, 25 * 480, "granted up front");
        assert_eq!(folded.taken_minutes, 5 * 480);
        assert_eq!(folded.booked_minutes, 2 * 480);
        assert_eq!(folded.pending_minutes, 3 * 480);
        assert_eq!(folded.remaining_minutes, (25 - 5 - 2) * 480);
    }

    /// A part-timer gets the policy scaled, a joiner gets it pro-rated, and the
    /// two together are one figure — not two rounding errors.
    #[test]
    fn a_part_time_joiner_gets_the_policy_scaled_and_prorated() {
        let terms = vec![employment(
            day(2026, Month::July, 1),
            None,
            [480, 480, 480, 0, 0, 0, 0],
        )];
        let as_of = day(2026, Month::December, 31);
        let folded = fold_leave_year(
            &annual(),
            &terms,
            &[],
            LeaveYear::calendar().window(as_of),
            as_of,
            0,
        );
        // Three fifths of 25 eight-hour days, then the 184 days of the year
        // they were employed for — cumulatively, which is what makes a joiner
        // and a leaver sum to exactly one year rather than to one year minus a
        // rounding error (`prorated_entitlement_minutes`).
        let scaled = 25 * 480 * 3 / 5;
        assert_eq!(
            folded.entitlement_minutes,
            scaled - scaled * 181 / 365,
            "the whole year, less the days before 1 July"
        );
        assert_eq!(folded.remaining_minutes, folded.entitlement_minutes);
    }

    /// A request straddling the leave year's edge charges each year its own
    /// part — the case a "sum the requests of this year" query gets wrong.
    #[test]
    fn a_request_across_the_year_boundary_is_split() {
        let terms = vec![employment(
            day(2020, Month::January, 1),
            None,
            FULL_TIME_PATTERN,
        )];
        // Mon 28 December 2026 – Fri 8 January 2027: four working days in 2026
        // (28, 29, 30, 31) and five in 2027 (1, 4, 5, 6, 7, 8 → 1 Jan is a
        // Friday, then Mon–Fri).
        let requests = vec![request(
            day(2026, Month::December, 28),
            day(2027, Month::January, 8),
            LeaveStatus::Approved,
        )];
        let old = fold_leave_year(
            &annual(),
            &terms,
            &requests,
            LeaveYear::calendar().window(day(2026, Month::December, 31)),
            day(2026, Month::December, 31),
            0,
        );
        let new = fold_leave_year(
            &annual(),
            &terms,
            &requests,
            LeaveYear::calendar().window(day(2027, Month::January, 31)),
            day(2027, Month::January, 31),
            0,
        );
        assert_eq!(old.taken_minutes, 4 * 480);
        assert_eq!(new.taken_minutes, 6 * 480);
    }

    /// Carryover is capped by the policy, and a policy that carries nothing
    /// carries nothing however much was left.
    #[test]
    fn carryover_is_capped_and_expires() {
        let year_start = day(2027, Month::January, 1);
        assert_eq!(
            carried_in_minutes(10 * 480, 5 * 480, None, year_start),
            5 * 480,
            "capped at five days"
        );
        assert_eq!(carried_in_minutes(10 * 480, 0, None, year_start), 0);
        let expires = carryover_expires_on(year_start, Some(3));
        assert_eq!(expires, Some(day(2027, Month::April, 1)));
        assert_eq!(
            carried_in_minutes(10 * 480, 5 * 480, expires, day(2027, Month::April, 1)),
            0,
            "lapsed on the day it expires"
        );
    }

    /// The pattern used to scale is the one in force on the day asked about, and
    /// a leaver is still scaled by the terms they had rather than by nothing.
    #[test]
    fn a_leaver_keeps_the_pattern_they_had() {
        let terms = vec![employment(
            day(2026, Month::January, 1),
            Some(day(2026, Month::June, 30)),
            FULL_TIME_PATTERN,
        )];
        let as_of = day(2026, Month::September, 1);
        let found = pattern_on(&terms, as_of).expect("the terms they had");
        assert_eq!(found.weekly_minutes(), 2_400);
        let folded = fold_leave_year(
            &annual(),
            &terms,
            &[],
            LeaveYear::calendar().window(as_of),
            as_of,
            0,
        );
        // Employed for 181 of the year's 365 days.
        assert_eq!(folded.entitlement_minutes, 25 * 480 * 181 / 365);
    }
}
