//! A deal's next step (alo CRM, ADR 0035, wave B2) — the bridge between a deal
//! and the tasks module, and deliberately nothing more.
//!
//! **A next step is a Task.** It is created in the existing tasks store with
//! `source_kind = "deal"` and `source_id = <deal id>` — the additive third value
//! beside `email` and `event`, which is exactly what ADR 0021's source link
//! exists for — and read back through the same link
//! (`docs/design/crm.md`, "Activities and next steps").
//!
//! **Rejected: a `next_step` column, or a CRM-private to-do table.** Two to-do
//! lists in one workspace is how a CRM becomes the system nobody updates: the
//! task that matters ends up in the list the user actually opens every morning
//! and the CRM's copy rots. So this file owns no table. It owns two rules:
//!
//! - a next step lands in a project the user picks, defaulting to their
//!   **personal** project, because the next step belongs to the person who will
//!   do it; and
//! - the source link is written by us and never by the caller, so a "next step"
//!   always really points at the deal it was raised from.
//!
//! The consequence of the first rule is honest and worth stating out loud: a
//! deal is tenant-wide, a personal project is not, so a colleague reading the
//! same deal sees the next steps they own or were assigned — not everyone's.
//! It is the same asymmetry a linked conversation has, and for the same reason.

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{CrmDealId, ProjectId, TaskId};
use crate::tasks::{NewTask, Task};

/// The `tasks.source_kind` value a deal's next step carries.
///
/// One constant, used by the writer and the reader, so the link can never be
/// spelled two ways.
pub const DEAL_SOURCE_KIND: &str = "deal";

impl AccountStore {
    /// Creates a next step for a deal: a real task, linked back to it.
    ///
    /// `project` is where the user filed it; `None` means their own personal
    /// project, ensured to exist. Whatever `input` says about a source link is
    /// **overwritten** with this deal — a next step that points somewhere else
    /// is not a next step.
    ///
    /// The task is created `active`, never `proposed`: this is a person
    /// deciding, not the agent suggesting (the propose-then-approve path of
    /// ADR 0023 stays where it is, in the tasks store).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the deal is not this tenant's, or the
    /// project is not one the caller can see; [`StoreError::Db`] on failure.
    pub async fn create_crm_deal_next_step(
        &self,
        deal: &CrmDealId,
        project: Option<&ProjectId>,
        input: &NewTask,
    ) -> Result<TaskId> {
        if self.crm_deal(deal).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let project = match project {
            Some(project) => project.clone(),
            None => self.ensure_personal_project().await?,
        };
        let new = NewTask {
            source_kind: Some(DEAL_SOURCE_KIND.to_owned()),
            source_id: Some(deal.as_str().to_owned()),
            state: None,
            ..input.clone()
        };
        self.create_task(&project, &new).await
    }

    /// The deal's next steps: its linked tasks, as **this** reader may see them
    /// — unfinished first, then by due date, so the drawer's answer to "what is
    /// due on this deal" is the first line of it.
    ///
    /// A next step filed in a colleague's personal project is theirs; it appears
    /// here for somebody else only when it is assigned to them
    /// ([`AccountStore::tasks_for_source`]).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the deal is not this tenant's — never an
    /// empty list, which would be an existence oracle;
    /// [`StoreError::Db`] on failure.
    pub async fn crm_deal_next_steps(&self, deal: &CrmDealId) -> Result<Vec<Task>> {
        if self.crm_deal(deal).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        self.tasks_for_source(DEAL_SOURCE_KIND, deal.as_str()).await
    }
}
