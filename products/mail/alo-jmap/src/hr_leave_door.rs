//! Who may see and decide whose leave (alo HR, ADR 0035, wave B6.03b;
//! `docs/design/hr.md`, "Who approves").
//!
//! The store refuses on the *record's* rules — a decided request is not decided
//! twice, an overdraft is not approved — and would refuse the same way whoever
//! asked. **This file is the other half**: the rules that are about the person
//! asking, resolved once, in one place, so that no route spells them itself and
//! none of them can drift.
//!
//! Three relationships, and nothing else:
//!
//! - **mine** — the request is about the employee record linked to my login.
//! - **my team** — the person's `manager_id` is my employee record. One level:
//!   a manager's manager is *not* an approver (the chain is a cut recorded in
//!   the design note; escalation today is HR, who can decide anything).
//! - **HR** — the tenant admin, or somebody holding the HR role
//!   ([`crate::state::Account::require_hr`]).
//!
//! Two sharp cases, decided in the design note rather than by whichever screen
//! shipped first:
//!
//! - **A manager may not approve their own leave** — `409`, naming that it must
//!   go to their own manager or to HR. An **admin** may, because a one-person
//!   tenant has nobody else, and the audit entry records that it was
//!   self-approved.
//! - **A refusal about somebody else's record is a `404`, not a `403`.** A `403`
//!   would confirm that the request exists and whose it is; a stranger and a
//!   stranger's colleague must get the same answer.

use axum::http::StatusCode;

use alo_store::{HrEmployeeId, TenantStore};

use crate::billing::map_store_err;
use crate::error::Problem;
use crate::state::Account;

/// What the caller is, in leave terms: their own record, the people who report
/// to them, and whether they hold the HR door.
pub(crate) struct LeaveDoor {
    /// The employee record linked to this login, when there is one. `None` for
    /// a contractor with a mailbox or an admin who is not on the payroll — an
    /// ordinary state, not an error.
    pub me: Option<HrEmployeeId>,
    /// The people whose `manager_id` is [`LeaveDoor::me`]. Direct reports only.
    pub reports: Vec<HrEmployeeId>,
    /// Admin, or the HR role.
    pub is_hr: bool,
    /// Tenant admin — the one caller who may decide their own leave.
    pub is_admin: bool,
}

impl LeaveDoor {
    /// Resolves the caller once. Two reads, both of them ones the client is
    /// already entitled to make.
    pub(crate) async fn resolve(account: &Account) -> Result<Self, Problem> {
        let me = account
            .acc
            .my_hr_employee()
            .await
            .map_err(map_store_err)?
            .map(|employee| employee.id);
        let reports = match &me {
            None => Vec::new(),
            Some(mine) => account
                .acc
                .hr_directory()
                .await
                .map_err(map_store_err)?
                .into_iter()
                .filter(|entry| entry.manager_id.as_ref() == Some(mine))
                .map(|entry| entry.id)
                .collect(),
        };
        Ok(Self {
            me,
            reports,
            is_hr: account.require_hr().is_ok(),
            is_admin: account.is_admin,
        })
    }

    /// Whether `employee` is the caller themselves.
    pub(crate) fn is_me(&self, employee: &HrEmployeeId) -> bool {
        self.me.as_ref() == Some(employee)
    }

    /// Whether `employee` reports to the caller.
    pub(crate) fn manages(&self, employee: &HrEmployeeId) -> bool {
        self.reports.contains(employee)
    }

    /// Whether the caller may see this person's leave at all: their own, their
    /// team's, or — for HR — anybody's.
    pub(crate) fn may_read(&self, employee: &HrEmployeeId) -> bool {
        self.is_hr || self.is_me(employee) || self.manages(employee)
    }

    /// The `404` a request about somebody the caller may not read gets. Never a
    /// `403`: the answer must not confirm that the record exists.
    pub(crate) fn require_read(&self, employee: &HrEmployeeId) -> Result<(), Problem> {
        if self.may_read(employee) {
            Ok(())
        } else {
            Err(Problem::with(
                StatusCode::NOT_FOUND,
                "no such leave request",
            ))
        }
    }

    /// The caller's own employee record, or the `409` that says the workspace
    /// does not know who they are on the payroll.
    ///
    /// Not a `403`: nothing has been refused. The tenant simply has no employee
    /// record linked to this login yet, and the fix is HR's rather than the
    /// caller's — so the sentence says exactly that.
    pub(crate) fn require_me(&self) -> Result<HrEmployeeId, Problem> {
        self.me.clone().ok_or_else(|| {
            Problem::with(
                StatusCode::CONFLICT,
                "this login is not linked to an employee record; ask HR to link it before booking \
                 leave",
            )
        })
    }

    /// Whether the caller may decide this person's request, and the refusal that
    /// says why not.
    ///
    /// # Errors
    /// `409` when it is their own leave and they are not an admin — naming where
    /// it must go instead; `404` when the person is not theirs to decide about.
    pub(crate) fn require_decide(&self, employee: &HrEmployeeId) -> Result<(), Problem> {
        if self.is_me(employee) && !self.is_admin {
            return Err(Problem::with(
                StatusCode::CONFLICT,
                "leave cannot be approved by the person taking it; it goes to their manager or to \
                 HR",
            ));
        }
        if self.is_hr || self.is_me(employee) || self.manages(employee) {
            return Ok(());
        }
        Err(Problem::with(
            StatusCode::NOT_FOUND,
            "no such leave request",
        ))
    }

    /// The employee ids a `scope` resolves to: `None` for HR's tenant-wide read,
    /// otherwise the exact list the store will answer about.
    ///
    /// # Errors
    /// `403` when somebody who is not HR asks for `all`; `422` on a word this
    /// build does not know.
    pub(crate) fn scope(&self, scope: Option<&str>) -> Result<Option<Vec<HrEmployeeId>>, Problem> {
        match scope.unwrap_or("mine").trim() {
            "mine" => Ok(Some(self.me.clone().into_iter().collect())),
            "team" => {
                // The manager's own leave is not in their team queue: a queue is
                // what is waiting for *them* to decide.
                Ok(Some(self.reports.clone()))
            }
            "all" if self.is_hr => Ok(None),
            "all" => Err(Problem::with(
                StatusCode::FORBIDDEN,
                "admin or hr only sees every person's leave",
            )),
            other => Err(Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("scope must be one of: mine, team, all (got {other})"),
            )),
        }
    }
}

/// The employee a write is about: the caller's own record by default, somebody
/// else's only through the HR door.
///
/// HR filing leave for another person is the ordinary way an absence is recorded
/// for somebody who has no login at all — a warehouse hand, a seasonal picker —
/// and it is the only reason this takes an id.
///
/// # Errors
/// `403` when a caller who is not HR names somebody else; `409` when they name
/// nobody and have no employee record; `404` when the id is not this tenant's.
pub(crate) async fn subject_of_write(
    hr: &TenantStore,
    door: &LeaveDoor,
    stated: Option<&str>,
) -> Result<HrEmployeeId, Problem> {
    let Some(stated) = stated.map(str::trim).filter(|id| !id.is_empty()) else {
        return door.require_me();
    };
    let stated = HrEmployeeId::new(stated.to_owned());
    if !door.is_me(&stated) && !door.is_hr {
        return Err(Problem::with(
            StatusCode::FORBIDDEN,
            "only HR may book leave for somebody else",
        ));
    }
    // Proved to be this tenant's before anything is written about them, so a
    // guessed id from another tenant is a 404 rather than a store error.
    hr.hr_employee(&stated)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such employee"))?;
    Ok(stated)
}
