//! The org chart's one invariant: **who reports to whom is a tree** (alo HR,
//! ADR 0035, wave B6.02a; `docs/design/hr.md`, "The org chart, and the cycle it
//! must refuse").
//!
//! A cycle is refused **on write**, not detected on read. A chart that can be
//! cyclic is a chart whose every reader — the renderer, the approvals
//! narrowing, the absence layer, the payroll grouping — must defend itself
//! against an infinite walk forever, and the reader that forgets hangs a
//! request. Refusing the write once means every reader afterwards may assume a
//! tree.
//!
//! Depth is bounded at [`ORG_CHART_MAX_DEPTH`], far past any real organisation.
//! The bound is not a second cycle check (the walk already terminates); it stops
//! a pathological chain from turning an ordinary read into a long one.
//!
//! **The refusal names ids, not people.** `docs/design/hr.md`'s error table
//! requires it of the staff-number clash for a reason that applies at least as
//! strongly here: the caller is holding one record and proposing a link, and a
//! message that answered with a colleague's name would tell them something the
//! refusal was not asked to disclose. The ids are already in their hands.
//!
//! The chart itself — the fold into a tree that `GET /hr/org` returns — arrives
//! with the routes in B6.02b. This file exists now because the rule it holds is
//! a property of the **write** path, and the write path ships first.

use crate::error::{Result, StoreError};
use crate::id::HrEmployeeId;
use crate::store::TenantStore;

/// How deep a reporting line may be. Sixteen levels is a very large company;
/// sixty-four is past anything real, which is the point — the bound exists to
/// stop a pathological chain, not to have an opinion about org design.
pub const ORG_CHART_MAX_DEPTH: usize = 64;

impl TenantStore {
    /// Proves a proposed manager link is sound before it is written: the
    /// manager is **this tenant's** employee, and following the reporting line
    /// up from them never arrives back at `employee`.
    ///
    /// `None` passes — somebody has to be at the top, and a person whose
    /// manager has not been recorded yet is ordinary.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the proposed manager is not this tenant's
    /// employee — the same answer an id that was never issued gets, so the
    /// refusal is not an existence oracle across tenants.
    /// [`StoreError::Validation`] when the link would close a cycle (naming
    /// both record ids, never their names) or when the line is already deeper
    /// than [`ORG_CHART_MAX_DEPTH`]. [`StoreError::Db`] on failure.
    pub(crate) async fn assert_manager_link_sound(
        &self,
        employee: &HrEmployeeId,
        manager: Option<&HrEmployeeId>,
    ) -> Result<()> {
        let Some(manager) = manager else {
            return Ok(());
        };
        if manager.as_str() == employee.as_str() {
            return Err(StoreError::Validation(format!(
                "employee {employee} cannot be their own manager"
            )));
        }
        // Walk up from the proposed manager. Each step is one indexed primary
        // key lookup; the walk is bounded twice over — by the depth limit, and
        // by the tree the previous writes have already guaranteed.
        let mut at = manager.as_str().to_owned();
        for _ in 0..ORG_CHART_MAX_DEPTH {
            let row: Option<(Option<String>,)> = sqlx::query_as(
                "SELECT manager_id FROM hr_employees WHERE tenant_id = $1 AND id = $2",
            )
            .bind(self.tenant().as_str())
            .bind(&at)
            .fetch_optional(self.pool())
            .await
            .map_err(StoreError::Db)?;
            // The first miss is the proposed manager not existing here; a later
            // one cannot happen (the FK holds the links), and answering
            // `NotFound` either way is the safe direction.
            let Some((next,)) = row else {
                return Err(StoreError::NotFound);
            };
            let Some(next) = next else { return Ok(()) };
            if next == employee.as_str() {
                return Err(StoreError::Validation(format!(
                    "manager refused: employee {employee} would report to themselves \
                     through {at}"
                )));
            }
            at = next;
        }
        Err(StoreError::Validation(format!(
            "reporting line is deeper than {ORG_CHART_MAX_DEPTH} levels"
        )))
    }
}
