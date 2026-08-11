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
//! The chart itself — [`fold_org_chart`], the fold `GET /hr/org` returns
//! (B6.02b) — is a **pure function over the directory projection**, which is
//! how it carries no private field: the only type it can see is
//! [`DirectoryEntry`], and no home address is on that type to leak.

use std::collections::HashMap;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::hr_employees::DirectoryEntry;
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

/// One person in the chart, with the people who report to them beneath.
///
/// The fields are the directory's public ones and nothing else — a name, what
/// they do, which team, who they report to. `docs/design/hr.md` ("The org
/// chart") makes this the one HR read every member gets, and a company where
/// you cannot find out who your colleague's manager is has an org chart in a
/// filing cabinet.
///
/// There is no private field on [`DirectoryEntry`] to put here even by mistake,
/// which is the point of folding the chart from that projection rather than
/// from the record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrgNode {
    /// The employee record this node is.
    pub id: HrEmployeeId,
    /// What they are called — the preferred name when there is one.
    pub name: String,
    /// Their job title, from the employment in force.
    pub job_title: String,
    /// Their team, from the employment in force.
    pub team: String,
    /// Who they report to, when the chart shows that person. `None` at a root.
    pub manager_id: Option<HrEmployeeId>,
    /// Their direct reports, in the directory's order (family name, then given
    /// name) — so the chart reads the same way twice.
    pub reports: Vec<OrgNode>,
}

/// Folds a directory into the reporting tree: roots first, each with its
/// reports beneath it, in directory order throughout.
///
/// Three properties, each one a test below rather than a claim:
///
/// - **Nobody is lost.** A person whose manager is not in the set — archived
///   last week, or never recorded — is a root, not an absence. A chart that
///   silently dropped a branch would be a chart somebody plans headcount from.
/// - **It terminates whatever the data says.** The cycle refusal upstream
///   ([`TenantStore::assert_manager_link_sound`]) means a cycle cannot be
///   written; this fold does not *rely* on that. It walks with an explicit
///   stack and a visited set, so a row that somehow closed a loop (a restore
///   from an older dump, a repair run by hand in psql) yields a chart with that
///   person at a root instead of hanging a request forever.
/// - **It is pure.** No database, no clock, no tenant — which is why the tests
///   for it are unit tests, and why the only shape it can return is the one the
///   directory door already allows the caller to see.
#[must_use]
pub fn fold_org_chart(entries: Vec<DirectoryEntry>) -> Vec<OrgNode> {
    let index: HashMap<String, usize> = entries
        .iter()
        .enumerate()
        .map(|(at, entry)| (entry.id.as_str().to_owned(), at))
        .collect();

    // Children by their manager's position, and the roots: everybody whose
    // manager is absent from this set (nobody recorded, or archived out of it).
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); entries.len()];
    let mut roots: Vec<usize> = Vec::new();
    for (at, entry) in entries.iter().enumerate() {
        match entry
            .manager_id
            .as_ref()
            .and_then(|manager| index.get(manager.as_str()))
        {
            Some(&manager) if manager != at => children[manager].push(at),
            _ => roots.push(at),
        }
    }

    // Post-order with an explicit stack: a node is assembled only once every
    // one of its reports has been. `visited` is what makes a looped row
    // terminate — it is entered once and never expanded again.
    let mut built: Vec<Option<OrgNode>> = vec![None; entries.len()];
    let mut visited = vec![false; entries.len()];
    let mut stack: Vec<(usize, bool)> = Vec::new();
    let mut chart: Vec<OrgNode> = Vec::with_capacity(roots.len());
    // Anything a loop kept out of the walk is added afterwards, at a root, so
    // "nobody is lost" holds for broken data too.
    let walk: Vec<usize> = roots.iter().copied().chain(0..entries.len()).collect();
    for start in walk {
        if visited[start] {
            continue;
        }
        stack.push((start, false));
        while let Some((at, expanded)) = stack.pop() {
            if expanded {
                let reports: Vec<OrgNode> = children[at]
                    .iter()
                    .filter_map(|&child| built[child].take())
                    .collect();
                built[at] = Some(node_of(&entries[at], reports));
                continue;
            }
            if visited[at] {
                continue;
            }
            visited[at] = true;
            stack.push((at, true));
            for &child in &children[at] {
                if !visited[child] {
                    stack.push((child, false));
                }
            }
        }
        if let Some(node) = built[start].take() {
            chart.push(node);
        }
    }
    chart
}

/// One directory row as a childless chart node.
fn node_of(entry: &DirectoryEntry, reports: Vec<OrgNode>) -> OrgNode {
    OrgNode {
        id: entry.id.clone(),
        name: entry.display_name(),
        job_title: entry.job_title.clone(),
        team: entry.team.clone(),
        manager_id: entry.manager_id.clone(),
        reports,
    }
}

impl AccountStore {
    /// **The directory door's chart**: this tenant's active people folded into
    /// the reporting tree, public fields only.
    ///
    /// Every member gets it, archived people are not in it (they are not in the
    /// directory this folds), and there is no argument by which a caller could
    /// ask for another tenant's — the handle carries the tenant.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn hr_org_chart(&self) -> Result<Vec<OrgNode>> {
        Ok(fold_org_chart(self.hr_directory().await?))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{OrgNode, fold_org_chart};
    use crate::hr_employees::DirectoryEntry;
    use crate::id::HrEmployeeId;

    /// A directory row: an id, a name, and who they report to. The remaining
    /// public fields play no part in the fold.
    fn row(id: &str, family: &str, manager: Option<&str>) -> DirectoryEntry {
        DirectoryEntry {
            id: HrEmployeeId::new(id.to_owned()),
            given_name: "A".to_owned(),
            family_name: family.to_owned(),
            preferred_name: String::new(),
            work_email: None,
            work_phone: String::new(),
            manager_id: manager.map(|m| HrEmployeeId::new(m.to_owned())),
            photo_node_id: None,
            job_title: String::new(),
            team: String::new(),
            started_on: None,
            archived: false,
        }
    }

    /// The ids of a level, in the order the fold put them.
    fn ids(nodes: &[OrgNode]) -> Vec<&str> {
        nodes.iter().map(|n| n.id.as_str()).collect()
    }

    /// Everybody in a chart, however deep — what "nobody is lost" is asserted
    /// with.
    fn everybody(nodes: &[OrgNode]) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        let mut stack: Vec<&OrgNode> = nodes.iter().collect();
        while let Some(node) = stack.pop() {
            found.push(node.id.as_str().to_owned());
            stack.extend(node.reports.iter());
        }
        found.sort();
        found
    }

    #[test]
    fn a_three_level_chart_folds_into_one_root() {
        // ceo → (head → junior), and a second report of the head.
        let chart = fold_org_chart(vec![
            row("ceo", "Aalders", None),
            row("head", "Bakker", Some("ceo")),
            row("junior", "Claes", Some("head")),
            row("second", "Daems", Some("head")),
        ]);
        assert_eq!(ids(&chart), vec!["ceo"]);
        assert_eq!(ids(&chart[0].reports), vec!["head"]);
        assert_eq!(ids(&chart[0].reports[0].reports), vec!["junior", "second"]);
        assert_eq!(
            chart[0].reports[0]
                .manager_id
                .as_ref()
                .map(HrEmployeeId::as_str),
            Some("ceo")
        );
    }

    #[test]
    fn somebody_whose_manager_left_the_directory_is_a_root_not_an_absence() {
        // The manager is archived, so they are not in the directory at all.
        let chart = fold_org_chart(vec![
            row("ceo", "Aalders", None),
            row("orphan", "Bakker", Some("archived-manager")),
        ]);
        assert_eq!(ids(&chart), vec!["ceo", "orphan"]);
        assert_eq!(everybody(&chart), vec!["ceo", "orphan"]);
    }

    #[test]
    fn a_looped_row_terminates_and_still_shows_everyone() {
        // Unwritable through the store (the cycle is refused on write), but the
        // fold must not hang on a row a repair-by-hand could produce.
        let chart = fold_org_chart(vec![
            row("one", "Aalders", Some("two")),
            row("two", "Bakker", Some("one")),
            row("three", "Claes", None),
        ]);
        assert_eq!(everybody(&chart), vec!["one", "three", "two"]);
    }

    #[test]
    fn somebody_recorded_as_their_own_manager_is_a_root() {
        let chart = fold_org_chart(vec![row("solo", "Aalders", Some("solo"))]);
        assert_eq!(ids(&chart), vec!["solo"]);
        assert!(chart[0].reports.is_empty());
    }

    #[test]
    fn an_empty_directory_is_an_empty_chart() {
        assert!(fold_org_chart(Vec::new()).is_empty());
    }
}
