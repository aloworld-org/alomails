//! Onboarding and offboarding checklists — the shape of somebody's first week,
//! and of their last one (alo HR, ADR 0035, wave B6.05; `docs/design/hr.md`,
//! "Onboarding and offboarding checklists").
//!
//! # A template is a shape; an instance is a task board
//!
//! A template is an ordered list of steps, each with a title, an owner *by role*
//! and an offset in days from an anchor date. Nothing in this module records
//! that a step was done, because drawing a checklist for a person
//! ([`AccountStore::instantiate_hr_checklist`]) writes no row here at all: it
//! creates a real project in the Tasks module, with the steps as tasks —
//! assigned, dated, and linked back to the person by the source link every task
//! already carries (`source_kind = "hr_employee"`, ADR 0021).
//!
//! *Rejected: an `hr_checklist_items` table with its own status, assignee, due
//! date and comments.* That is a fifth board in a product that has one, and it
//! would need its own notifications, its own overdue view and its own mobile
//! screen. A step that arrives as a task arrives where its owner already looks.
//!
//! The consequence worth stating plainly: **an instance is a copy**. Editing a
//! template never changes a checklist already running, and deleting one leaves
//! every board it ever produced untouched. That is why templates are deleted
//! rather than archived — unlike a leave policy, a template explains nothing
//! after the fact.
//!
//! # Roles, resolved late
//!
//! A step's owner is `hr`, `manager`, `it` or `employee`, resolved to a person
//! by [`resolve_owner`] at the moment the checklist is drawn. *Manager* is
//! whoever that person reports to on that day; a user id stored on the template
//! would quietly assign three years of onboarding to somebody who left. Every
//! role falls back to the person drawing the checklist, who is then looking at
//! the one screen where a wrong assignment is obvious — and the resolution is
//! returned with the run, so they can see it rather than discover it.
//!
//! # What this module deliberately cannot do
//!
//! **Provision an account.** "Create the mailbox", "grant the Spaces", "hand
//! over the laptop" are steps a person does and ticks. An HR write that creates
//! accounts turns a badly-scoped HR role into a security incident, so the
//! capability is absent rather than guarded (`docs/design/hr.md`, "Cuts").

use time::{Date, Duration, OffsetDateTime, Time, UtcOffset};

use crate::account::AccountStore;
use crate::billing_field::{bounded, required};
use crate::error::{Result, StoreError};
use crate::id::{
    CommentId, HrChecklistStepId, HrChecklistTemplateId, HrEmployeeId, ProjectId, TaskId, UserId,
};
use crate::store::TenantStore;

/// The longest a template's name may be: a line in a picker, not a paragraph.
pub const TEMPLATE_NAME_MAX_CHARS: usize = 120;

/// The longest a step's title may be. It becomes a task card's title, so it is
/// bounded where a card is legible.
pub const STEP_TITLE_MAX_CHARS: usize = 200;

/// The longest a step's detail may be. It becomes the task's description — room
/// for what "prepare the workstation" means in this company, not for a policy.
pub const STEP_DETAIL_MAX_CHARS: usize = 2_000;

/// The most steps one template may carry. A checklist somebody reads top to
/// bottom on their first morning; past this it is a project, and Projects is
/// where a project belongs (B3).
pub const TEMPLATE_STEPS_MAX: usize = 60;

/// The furthest a step may sit from the anchor date, in days either direction.
/// A year of preparation is already implausible; more is a typo, and it would
/// land a task in a week nobody will look at.
pub const STEP_DAY_OFFSET_MAX: i32 = 365;

/// The source kind the tasks of a checklist carry (ADR 0021), so a person's
/// record can find every checklist ever drawn for them without a link table.
pub const CHECKLIST_SOURCE_KIND: &str = "hr_employee";

/// The board kind a checklist lands on: shared, because a checklist is work
/// spread across HR, a manager and whoever hands over the laptop, and a personal
/// board would be visible to exactly one of them.
const TEAM_KIND: &str = "team";

/// The board column a fresh step lands in, and the one that means it is done —
/// the Tasks module's own vocabulary, matched here so progress is counted the
/// way the board displays it.
const TODO_STATUS: &str = "todo";
/// The column whose cards are finished work.
const DONE_STATUS: &str = "done";

/// Which end of an employment a checklist describes.
///
/// A closed vocabulary matched by the CHECK one layer down, because the word
/// decides what the anchor date *means*: onboarding counts from the day somebody
/// starts, offboarding from their last day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecklistKind {
    /// Arriving: the days before and after somebody's first.
    Onboarding,
    /// Leaving: the days before and after somebody's last.
    Offboarding,
}

impl ChecklistKind {
    /// The stored word.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Onboarding => "onboarding",
            Self::Offboarding => "offboarding",
        }
    }

    /// Reads a kind — from a request body or from a stored row.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "onboarding" => Ok(Self::Onboarding),
            "offboarding" => Ok(Self::Offboarding),
            _ => Err(StoreError::Validation(
                "checklist kind must be one of: onboarding, offboarding".to_owned(),
            )),
        }
    }
}

impl std::fmt::Display for ChecklistKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Who does a step, stated as a role and resolved to a person at instantiation.
///
/// `It` has no counterpart in `tenant_user_roles` on purpose: in a company small
/// enough to be replacing Microsoft 365 with us, "IT" is frequently the same
/// person as "HR" and occasionally the founder. It is a *label on the work*,
/// resolved from what the caller states, and a tenant that grows an IT team
/// states a different user without a migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOwner {
    /// The HR side: the contract, the file, the payroll notification.
    Hr,
    /// Whoever the person reports to on the day the checklist is drawn.
    Manager,
    /// Whoever sets up accounts and hardware here.
    It,
    /// The arriving or leaving person themselves.
    Employee,
}

impl StepOwner {
    /// The stored word.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hr => "hr",
            Self::Manager => "manager",
            Self::It => "it",
            Self::Employee => "employee",
        }
    }

    /// Reads an owner role — from a request body or from a stored row.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "hr" => Ok(Self::Hr),
            "manager" => Ok(Self::Manager),
            "it" => Ok(Self::It),
            "employee" => Ok(Self::Employee),
            _ => Err(StoreError::Validation(
                "checklist step owner must be one of: hr, manager, it, employee".to_owned(),
            )),
        }
    }
}

impl std::fmt::Display for StepOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The writable shape of one step.
#[derive(Debug, Clone)]
pub struct NewChecklistStep {
    /// What the step is, as it will read on the task card.
    pub title: String,
    /// The longer form, which becomes the task's description. Blank is ordinary.
    pub detail: String,
    /// Who does it.
    pub owner: StepOwner,
    /// Whole days from the anchor date; negative is before it.
    pub day_offset: i32,
}

impl Default for NewChecklistStep {
    /// An HR step on the anchor day itself — the shape most steps have, so a
    /// caller states only what differs.
    fn default() -> Self {
        Self {
            title: String::new(),
            detail: String::new(),
            owner: StepOwner::Hr,
            day_offset: 0,
        }
    }
}

/// One stored step of a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistStep {
    /// Opaque id, unique within the tenant.
    pub id: HrChecklistStepId,
    /// What the step is.
    pub title: String,
    /// The longer form.
    pub detail: String,
    /// Who does it.
    pub owner: StepOwner,
    /// Whole days from the anchor date; negative is before it.
    pub day_offset: i32,
}

/// The writable shape of a template: a name, an end of the employment, and the
/// steps in the order somebody reads them.
#[derive(Debug, Clone)]
pub struct NewChecklistTemplate {
    /// The tenant's own word for it — "Nieuwe collega", "Arrivée".
    pub name: String,
    /// Which end of an employment it describes.
    pub kind: ChecklistKind,
    /// The steps, in order. A template with none is refused: an empty checklist
    /// instantiates into an empty board, which is a thing that looks done.
    pub steps: Vec<NewChecklistStep>,
}

/// One stored template, with its steps.
#[derive(Debug, Clone)]
pub struct ChecklistTemplate {
    /// Opaque id, unique within the tenant.
    pub id: HrChecklistTemplateId,
    /// The tenant's own word for it.
    pub name: String,
    /// Which end of an employment it describes.
    pub kind: ChecklistKind,
    /// The steps, in order.
    pub steps: Vec<ChecklistStep>,
    /// Who created it.
    pub created_by: String,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time.
    pub updated_at: OffsetDateTime,
}

/// The people a caller names for the roles a template mentions. Every one is
/// optional: an unstated role resolves by [`resolve_owner`]'s rules.
#[derive(Debug, Clone, Default)]
pub struct ChecklistOwners {
    /// Who acts as HR for this run.
    pub hr: Option<UserId>,
    /// Who acts as the manager, overriding the person's own manager link.
    pub manager: Option<UserId>,
    /// Who sets up accounts and hardware for this run.
    pub it: Option<UserId>,
    /// Who acts for the employee — rarely stated, because the employee is
    /// usually the employee.
    pub employee: Option<UserId>,
}

impl ChecklistOwners {
    /// The user the caller named for `owner`, if any.
    #[must_use]
    pub fn stated(&self, owner: StepOwner) -> Option<&UserId> {
        match owner {
            StepOwner::Hr => self.hr.as_ref(),
            StepOwner::Manager => self.manager.as_ref(),
            StepOwner::It => self.it.as_ref(),
            StepOwner::Employee => self.employee.as_ref(),
        }
    }

    /// Every user named, in role order — what the store validates before it
    /// writes anything.
    fn named(&self) -> Vec<(StepOwner, &UserId)> {
        [
            StepOwner::Hr,
            StepOwner::Manager,
            StepOwner::It,
            StepOwner::Employee,
        ]
        .into_iter()
        .filter_map(|owner| self.stated(owner).map(|user| (owner, user)))
        .collect()
    }
}

/// What the caller states when drawing a checklist for a person.
#[derive(Debug, Clone)]
pub struct NewChecklistRun {
    /// The shape to run.
    pub template_id: HrChecklistTemplateId,
    /// The day every step is dated from: the first day for an onboarding, the
    /// last day for an offboarding.
    pub anchor_on: Date,
    /// The board's name. Blank takes the template's name and the person's, which
    /// is what somebody scanning a board list needs to tell two runs apart.
    pub name: String,
    /// Who fills the roles this run.
    pub owners: ChecklistOwners,
}

/// One step as it landed: the task it became, who it went to, and when it is
/// due. Returned by the run so the person who drew the checklist can *see* the
/// resolution rather than discover it on somebody else's board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedStep {
    /// The task the step became.
    pub task_id: TaskId,
    /// The step's title, as it reads on the card.
    pub title: String,
    /// The role the template stated.
    pub owner: StepOwner,
    /// The person it resolved to.
    pub assignee: UserId,
    /// The day it is due.
    pub due_on: Date,
}

/// What one instantiation produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistRun {
    /// The board the steps landed on.
    pub project_id: ProjectId,
    /// The board's name.
    pub name: String,
    /// The template it was drawn from — still a template id, though the board no
    /// longer depends on it: an instance is a copy.
    pub template_id: HrChecklistTemplateId,
    /// Which end of the employment it describes.
    pub kind: ChecklistKind,
    /// The day the steps are dated from.
    pub anchor_on: Date,
    /// The steps, in template order.
    pub steps: Vec<PlannedStep>,
}

/// One checklist running for a person, as the person's record shows it.
///
/// Folded from the tasks themselves rather than stored: a "done" column would be
/// the `qty_on_hand` mistake again — one missed tick and a checklist claims work
/// that never happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistProgress {
    /// The board the steps are on.
    pub project_id: ProjectId,
    /// The board's name.
    pub name: String,
    /// How many steps it carries.
    pub total: i64,
    /// How many are finished.
    pub done: i64,
    /// The earliest step's day, if any step is dated.
    pub first_due_on: Option<Date>,
    /// The latest step's day.
    pub last_due_on: Option<Date>,
}

impl ChecklistProgress {
    /// Whether every step is finished.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.total > 0 && self.done == self.total
    }
}

/// Resolves a step's owner role to the person it goes to.
///
/// Pure, and the one piece of judgement in this module, so it is unit-tested
/// directly. The rules, in order:
///
/// - what the caller stated for the role always wins;
/// - `manager` then takes the person's own manager, `employee` takes the person;
/// - anything still unresolved goes to `actor` — the person drawing the
///   checklist.
///
/// The last rule is doing real work rather than being a shrug: on the day an
/// onboarding is drawn the arriving person usually has no login at all (that is
/// one of the steps), and their manager may be a record without an account. A
/// task assigned to nobody is a task nobody does; a task on the desk of the
/// person who just drew the checklist is one they can hand on in one gesture.
#[must_use]
pub fn resolve_owner(
    owner: StepOwner,
    stated: &ChecklistOwners,
    employee_user: Option<&UserId>,
    manager_user: Option<&UserId>,
    actor: &UserId,
) -> UserId {
    if let Some(user) = stated.stated(owner) {
        return user.clone();
    }
    let fallback = match owner {
        StepOwner::Manager => manager_user,
        StepOwner::Employee => employee_user,
        StepOwner::Hr | StepOwner::It => None,
    };
    fallback.unwrap_or(actor).clone()
}

/// The day a step falls on: the anchor moved by whole days, refusing rather than
/// wrapping at the ends of the calendar.
///
/// # Errors
/// [`StoreError::Validation`] when the offset moves the day off the calendar.
pub fn step_day(anchor: Date, day_offset: i32) -> Result<Date> {
    anchor
        .checked_add(Duration::days(i64::from(day_offset)))
        .ok_or_else(|| {
            StoreError::Validation(
                "this anchor date moves a step outside the calendar; choose a nearer date"
                    .to_owned(),
            )
        })
}

/// A day as the instant a task is due: that day's first moment, UTC — the
/// convention every date-to-instant conversion in this store already uses.
fn due_instant(day: Date) -> OffsetDateTime {
    day.with_time(Time::MIDNIGHT).assume_offset(UtcOffset::UTC)
}

/// A validated, normalised template ready to be bound into statements.
#[derive(Debug)]
struct Normalized {
    name: String,
    steps: Vec<NewChecklistStep>,
}

/// Validates and normalises a template. Pure — no database.
fn normalize(input: &NewChecklistTemplate) -> Result<Normalized> {
    let name = required(
        "checklist template name",
        &input.name,
        TEMPLATE_NAME_MAX_CHARS,
    )?;
    if input.steps.is_empty() {
        return Err(StoreError::Validation(
            "a checklist template needs at least one step".to_owned(),
        ));
    }
    if input.steps.len() > TEMPLATE_STEPS_MAX {
        return Err(StoreError::Validation(format!(
            "a checklist template carries at most {TEMPLATE_STEPS_MAX} steps; this one has {}",
            input.steps.len()
        )));
    }
    let steps = input
        .steps
        .iter()
        .map(|step| {
            if !(-STEP_DAY_OFFSET_MAX..=STEP_DAY_OFFSET_MAX).contains(&step.day_offset) {
                return Err(StoreError::Validation(format!(
                    "a step falls between {STEP_DAY_OFFSET_MAX} days before and \
                     {STEP_DAY_OFFSET_MAX} days after the date it is anchored to"
                )));
            }
            Ok(NewChecklistStep {
                title: required("checklist step title", &step.title, STEP_TITLE_MAX_CHARS)?,
                detail: bounded("checklist step detail", &step.detail, STEP_DETAIL_MAX_CHARS)?,
                owner: step.owner,
                day_offset: step.day_offset,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Normalized { name, steps })
}

/// Turns the template table's uniqueness violation into an answer naming the
/// rule, and leaves every other database failure alone.
fn map_template_conflict(error: sqlx::Error) -> StoreError {
    match error {
        sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23505") => {
            match db.constraint().unwrap_or_default() {
                "hr_checklist_templates_name_unique" => StoreError::Conflict(
                    "a checklist of this kind already has this name".to_owned(),
                ),
                _ => StoreError::Conflict("unique constraint".to_owned()),
            }
        }
        other => StoreError::Db(other),
    }
}

/// The columns every read of a template selects, in `TemplateRow` order.
const TEMPLATE_COLS: &str = "id, name, kind, created_by, created_at, updated_at";

/// Reads one template with its steps, tenant-bound.
///
/// A free function rather than a method because both doors need it: the HR door
/// ([`TenantStore`]) to show and edit templates, and the account door
/// ([`AccountStore`]) to run one — and a second copy of the read would be a
/// second chance to forget the tenant predicate.
async fn read_template(
    pool: &sqlx::PgPool,
    tenant: &str,
    id: &HrChecklistTemplateId,
) -> Result<Option<ChecklistTemplate>> {
    let row = sqlx::query_as::<_, TemplateRow>(&format!(
        "SELECT {TEMPLATE_COLS} FROM hr_checklist_templates WHERE tenant_id = $1 AND id = $2"
    ))
    .bind(tenant)
    .bind(id.as_str())
    .fetch_optional(pool)
    .await
    .map_err(StoreError::Db)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let steps = sqlx::query_as::<_, StepRow>(
        "SELECT id, title, detail, owner_role, day_offset FROM hr_checklist_steps \
          WHERE tenant_id = $1 AND template_id = $2 ORDER BY position, id",
    )
    .bind(tenant)
    .bind(id.as_str())
    .fetch_all(pool)
    .await
    .map_err(StoreError::Db)?;
    let steps = steps
        .into_iter()
        .map(StepRow::into_step)
        .collect::<Result<Vec<_>>>()?;
    row.into_template(steps).map(Some)
}

/// Writes a template's steps, in the order given. Used by create and by the
/// replace-the-lot edit, so the two can never disagree about ordering.
async fn write_steps(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    template: &HrChecklistTemplateId,
    steps: &[NewChecklistStep],
) -> Result<()> {
    if steps.is_empty() {
        return Ok(());
    }
    // The arrays are built in one pass so they stay the same length and the same
    // order: `unnest` pads a short array with NULLs, which would silently
    // misalign every row after the gap.
    let mut ids: Vec<String> = Vec::with_capacity(steps.len());
    let mut positions: Vec<i32> = Vec::with_capacity(steps.len());
    let mut titles: Vec<String> = Vec::with_capacity(steps.len());
    let mut details: Vec<String> = Vec::with_capacity(steps.len());
    let mut owners: Vec<String> = Vec::with_capacity(steps.len());
    let mut offsets: Vec<i32> = Vec::with_capacity(steps.len());
    for (index, step) in steps.iter().enumerate() {
        ids.push(HrChecklistStepId::generate().as_str().to_owned());
        positions.push(i32::try_from(index).unwrap_or(i32::MAX));
        titles.push(step.title.clone());
        details.push(step.detail.clone());
        owners.push(step.owner.as_str().to_owned());
        offsets.push(step.day_offset);
    }
    sqlx::query(
        "INSERT INTO hr_checklist_steps (tenant_id, id, template_id, position, title, detail, \
             owner_role, day_offset) \
         SELECT $1, s.id, $2, s.position, s.title, s.detail, s.owner_role, s.day_offset \
         FROM unnest($3::text[], $4::int[], $5::text[], $6::text[], $7::text[], $8::int[]) \
             AS s(id, position, title, detail, owner_role, day_offset)",
    )
    .bind(tenant)
    .bind(template.as_str())
    .bind(&ids)
    .bind(&positions)
    .bind(&titles)
    .bind(&details)
    .bind(&owners)
    .bind(&offsets)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    Ok(())
}

impl TenantStore {
    /// Creates a checklist template. **The HR door**: what a company does when
    /// somebody arrives is a company-wide shape, not one manager's habit.
    ///
    /// One transaction: a template either lands with its steps or does not land
    /// at all, because a template with half its steps is worse than none —
    /// nobody can tell which half is missing.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a blank or over-long name, no steps, more
    /// than [`TEMPLATE_STEPS_MAX`] of them, a blank or over-long step title, or
    /// an offset outside ±[`STEP_DAY_OFFSET_MAX`] days;
    /// [`StoreError::Conflict`] when a template of this kind already has the
    /// name; [`StoreError::Db`] on failure.
    pub async fn create_hr_checklist_template(
        &self,
        input: &NewChecklistTemplate,
        actor: &UserId,
    ) -> Result<HrChecklistTemplateId> {
        let template = normalize(input)?;
        let id = HrChecklistTemplateId::generate();
        let mut tx = self.pool().begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO hr_checklist_templates (tenant_id, id, name, kind, created_by) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(&template.name)
        .bind(input.kind.as_str())
        .bind(actor.as_str())
        .execute(&mut *tx)
        .await
        .map_err(map_template_conflict)?;
        write_steps(&mut tx, self.tenant().as_str(), &id, &template.steps).await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// One template of this tenant with its steps, or `None` — including when
    /// the id belongs to another tenant, which is indistinguishable by design.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when a stored row carries a word this build
    /// does not know; [`StoreError::Db`] on failure.
    pub async fn hr_checklist_template(
        &self,
        id: &HrChecklistTemplateId,
    ) -> Result<Option<ChecklistTemplate>> {
        read_template(self.pool(), self.tenant().as_str(), id).await
    }

    /// The tenant's templates with their steps: onboarding first, then by name.
    ///
    /// Read whole rather than paged. A company has a handful of these — the
    /// screen that lists them shows their steps, and two round trips per
    /// template would be the only reason to have written it any other way.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a stored word this build does not know;
    /// [`StoreError::Db`] on failure.
    pub async fn hr_checklist_templates(&self) -> Result<Vec<ChecklistTemplate>> {
        let rows = sqlx::query_as::<_, TemplateRow>(&format!(
            "SELECT {TEMPLATE_COLS} FROM hr_checklist_templates \
              WHERE tenant_id = $1 ORDER BY kind, lower(name), id"
        ))
        .bind(self.tenant().as_str())
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        let steps = sqlx::query_as::<_, TemplateStepRow>(
            "SELECT template_id, id, title, detail, owner_role, day_offset \
               FROM hr_checklist_steps WHERE tenant_id = $1 ORDER BY template_id, position, id",
        )
        .bind(self.tenant().as_str())
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter()
            .map(|row| {
                let mine = steps
                    .iter()
                    .filter(|step| step.template_id == row.id)
                    .map(TemplateStepRow::to_step)
                    .collect::<Result<Vec<_>>>()?;
                row.into_template(mine)
            })
            .collect()
    }

    /// Replaces a template's name and its steps.
    ///
    /// The steps are rewritten as a block: a checklist is a short ordered list,
    /// and a per-step diff would be a reordering protocol between two screens to
    /// save writing sixty rows nobody is racing over. **The kind is not
    /// editable** — turning an onboarding into an offboarding silently reverses
    /// what every offset in it means.
    ///
    /// Checklists already running are untouched, because an instance is a copy.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the template is not this tenant's;
    /// [`StoreError::Validation`] as for create; [`StoreError::Conflict`] when
    /// another template of the same kind has the name; [`StoreError::Db`] on
    /// failure.
    pub async fn update_hr_checklist_template(
        &self,
        id: &HrChecklistTemplateId,
        input: &NewChecklistTemplate,
    ) -> Result<()> {
        let template = normalize(input)?;
        let mut tx = self.pool().begin().await.map_err(StoreError::Db)?;
        let done = sqlx::query(
            "UPDATE hr_checklist_templates SET name = $3, updated_at = now() \
              WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(&template.name)
        .execute(&mut *tx)
        .await
        .map_err(map_template_conflict)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        sqlx::query("DELETE FROM hr_checklist_steps WHERE tenant_id = $1 AND template_id = $2")
            .bind(self.tenant().as_str())
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        write_steps(&mut tx, self.tenant().as_str(), id, &template.steps).await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Deletes a template and its steps.
    ///
    /// Deletion rather than archiving, and it is honest: a checklist already
    /// running is a copy on its own board, so nothing a person is doing depends
    /// on this row. (A leave policy is archived instead because a balance folded
    /// from it is only explicable beside it; a template explains nothing after
    /// the fact.)
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the template is not this tenant's, or is
    /// already gone — deleting twice is a clean denial, not a silent success;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_hr_checklist_template(&self, id: &HrChecklistTemplateId) -> Result<()> {
        let done =
            sqlx::query("DELETE FROM hr_checklist_templates WHERE tenant_id = $1 AND id = $2")
                .bind(self.tenant().as_str())
                .bind(id.as_str())
                .execute(self.pool())
                .await
                .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

/// The person a checklist is being drawn for, read once before the write.
struct Subject {
    display_name: String,
    user_id: Option<UserId>,
    manager_user_id: Option<UserId>,
}

impl AccountStore {
    /// Draws a checklist for a person: a real task board, with the template's
    /// steps as tasks — assigned, dated, and linked back to the person.
    ///
    /// One transaction: a run either lands whole or does not land at all. Every
    /// judgement is made *before* it opens — the person is read, the named users
    /// are checked against this tenant, the roles are resolved and the dates
    /// computed — so a refusal never leaves a half-built board somebody has to
    /// find and delete.
    ///
    /// The same template may be run for the same person twice. That is
    /// deliberate: a rehire is a real thing, a run cancelled by a moved start
    /// date is a real thing, and refusing the second one would mean an HR person
    /// deleting a board to be allowed to draw it again.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the template, the person, or a named user
    /// is not this tenant's; [`StoreError::Validation`] when the person is
    /// archived, when the board name is over-long, or when the anchor date moves
    /// a step off the calendar; [`StoreError::Db`] on failure.
    pub async fn instantiate_hr_checklist(
        &self,
        employee: &HrEmployeeId,
        run: &NewChecklistRun,
    ) -> Result<ChecklistRun> {
        let template = read_template(&self.pool, self.tenant.as_str(), &run.template_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        let subject = self.checklist_subject(employee).await?;
        self.require_tenant_users(&run.owners).await?;

        let name = bounded("checklist name", &run.name, TEMPLATE_NAME_MAX_CHARS)?;
        let name = if name.is_empty() {
            format!("{} — {}", template.name, subject.display_name)
        } else {
            name
        };

        // Ids are minted here, in one place, so the plan the caller is answered
        // with is the plan that was written.
        let mut planned: Vec<PlannedStep> = Vec::with_capacity(template.steps.len());
        for step in &template.steps {
            planned.push(PlannedStep {
                task_id: TaskId::generate(),
                title: step.title.clone(),
                owner: step.owner,
                assignee: resolve_owner(
                    step.owner,
                    &run.owners,
                    subject.user_id.as_ref(),
                    subject.manager_user_id.as_ref(),
                    &self.user,
                ),
                due_on: step_day(run.anchor_on, step.day_offset)?,
            });
        }

        let project_id = ProjectId::generate();
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO task_projects (tenant_id, id, name, kind, owner_user_id) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(self.tenant.as_str())
        .bind(project_id.as_str())
        .bind(&name)
        .bind(TEAM_KIND)
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        self.write_checklist_tasks(&mut tx, &project_id, employee, &template, &planned)
            .await?;
        tx.commit().await.map_err(StoreError::Db)?;

        Ok(ChecklistRun {
            project_id,
            name,
            template_id: template.id,
            kind: template.kind,
            anchor_on: run.anchor_on,
            steps: planned,
        })
    }

    /// The checklists ever drawn for a person, newest board first, each folded
    /// from its own tasks.
    ///
    /// Found through the source link the tasks carry (ADR 0021) rather than
    /// through a link table: the tasks *are* the checklist, so a board that lost
    /// its link row could never happen because there is no link row to lose.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn hr_employee_checklists(
        &self,
        employee: &HrEmployeeId,
    ) -> Result<Vec<ChecklistProgress>> {
        let rows = sqlx::query_as::<_, ProgressRow>(
            "SELECT t.project_id, p.name, count(*) AS total, \
                 count(*) FILTER (WHERE t.status = $4 OR t.completed_at IS NOT NULL) AS done, \
                 min(t.due_at) AS first_due_at, max(t.due_at) AS last_due_at \
               FROM tasks t \
               JOIN task_projects p ON p.tenant_id = t.tenant_id AND p.id = t.project_id \
              WHERE t.tenant_id = $1 AND t.source_kind = $2 AND t.source_id = $3 \
                AND t.state = 'active' AND p.kind = $5 \
              GROUP BY t.project_id, p.name \
              ORDER BY min(t.created_at) DESC, t.project_id",
        )
        .bind(self.tenant.as_str())
        .bind(CHECKLIST_SOURCE_KIND)
        .bind(employee.as_str())
        .bind(DONE_STATUS)
        .bind(TEAM_KIND)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(ProgressRow::into_progress).collect())
    }

    /// The person a checklist is being drawn for, with the two user links the
    /// role resolution needs.
    ///
    /// Refuses an archived record: archiving is the *last* act of an employment,
    /// after the offboarding checklist has been worked through, and drawing one
    /// for somebody already archived is either a mistake or work that will land
    /// on a record nobody opens again.
    async fn checklist_subject(&self, employee: &HrEmployeeId) -> Result<Subject> {
        let row = sqlx::query_as::<_, SubjectRow>(
            "SELECT e.given_name, e.family_name, e.preferred_name, e.user_id, e.archived_at, \
                 m.user_id AS manager_user_id \
               FROM hr_employees e \
               LEFT JOIN hr_employees m \
                 ON m.tenant_id = e.tenant_id AND m.id = e.manager_id AND m.archived_at IS NULL \
              WHERE e.tenant_id = $1 AND e.id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(employee.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)?;
        if row.archived_at.is_some() {
            return Err(StoreError::Validation(
                "this employee record is archived; restore it before drawing a checklist"
                    .to_owned(),
            ));
        }
        Ok(row.into_subject())
    }

    /// Checks every user the caller named is one of this tenant's.
    ///
    /// A foreign user id in an assignee column is not a disclosure — it names
    /// somebody who can never see the task — but it is a task nobody will ever
    /// do, filed as though somebody would. The denial is the ordinary
    /// [`StoreError::NotFound`]: whether a user id exists elsewhere is not this
    /// tenant's business.
    async fn require_tenant_users(&self, owners: &ChecklistOwners) -> Result<()> {
        for (_, user) in owners.named() {
            let known: Option<i32> =
                sqlx::query_scalar("SELECT 1 FROM users WHERE tenant_id = $1 AND id = $2")
                    .bind(self.tenant.as_str())
                    .bind(user.as_str())
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(StoreError::Db)?;
            if known.is_none() {
                return Err(StoreError::NotFound);
            }
        }
        Ok(())
    }

    /// Writes the run's tasks, their first activity line and their followers.
    ///
    /// One statement per table over an `unnest` of the plan rather than a
    /// statement per step: sixty steps is a plausible checklist, and sixty round
    /// trips inside one transaction is a lock held for no reason.
    async fn write_checklist_tasks(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        project: &ProjectId,
        employee: &HrEmployeeId,
        template: &ChecklistTemplate,
        planned: &[PlannedStep],
    ) -> Result<()> {
        // Every array is built in one pass over the template zipped with its
        // plan, so they stay the same length and the same order: `unnest` pads a
        // short array with NULLs, which would silently misalign every row after
        // the gap. Board order is the template's order, so the first morning's
        // steps sit at the top of the column rather than wherever a set returned
        // them.
        let mut ids: Vec<String> = Vec::with_capacity(planned.len());
        let mut titles: Vec<String> = Vec::with_capacity(planned.len());
        let mut details: Vec<Option<String>> = Vec::with_capacity(planned.len());
        let mut assignees: Vec<String> = Vec::with_capacity(planned.len());
        let mut due_at: Vec<OffsetDateTime> = Vec::with_capacity(planned.len());
        let mut positions: Vec<f64> = Vec::with_capacity(planned.len());
        for (index, (step, plan)) in template.steps.iter().zip(planned.iter()).enumerate() {
            ids.push(plan.task_id.as_str().to_owned());
            titles.push(plan.title.clone());
            details.push(if step.detail.is_empty() {
                None
            } else {
                Some(step.detail.clone())
            });
            assignees.push(plan.assignee.as_str().to_owned());
            due_at.push(due_instant(plan.due_on));
            positions.push(f64::from(u32::try_from(index).unwrap_or(u32::MAX)) + 1.0);
        }
        sqlx::query(
            "INSERT INTO tasks (tenant_id, id, project_id, title, description, status, position, \
                 assignee_user_id, due_at, priority, state, source_kind, source_id, created_by) \
             SELECT $1, s.id, $2, s.title, s.description, $3, s.position, s.assignee, s.due_at, \
                 'none', 'active', $4, $5, $6 \
             FROM unnest($7::text[], $8::text[], $9::text[], $10::text[], $11::float8[], \
                 $12::timestamptz[]) \
                 AS s(id, title, description, assignee, position, due_at)",
        )
        .bind(self.tenant.as_str())
        .bind(project.as_str())
        .bind(TODO_STATUS)
        .bind(CHECKLIST_SOURCE_KIND)
        .bind(employee.as_str())
        .bind(self.user.as_str())
        .bind(&ids)
        .bind(&titles)
        .bind(&details)
        .bind(&assignees)
        .bind(&positions)
        .bind(&due_at)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;

        // The task's own first history line, which says where it came from.
        let activity_ids: Vec<String> = planned
            .iter()
            .map(|_| CommentId::generate().as_str().to_owned())
            .collect();
        let detail = serde_json::json!({
            "checklist": template.kind.as_str(),
            "template": template.name,
        });
        sqlx::query(
            "INSERT INTO task_activity (tenant_id, id, task_id, actor_user_id, kind, detail) \
             SELECT $1, a.id, a.task_id, $2, 'created', $3 \
             FROM unnest($4::text[], $5::text[]) AS a(id, task_id)",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(sqlx::types::Json(detail))
        .bind(&activity_ids)
        .bind(&ids)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;

        // Both the assignee and the person who drew the checklist follow each
        // step: one is doing it, the other is answerable for the whole list.
        sqlx::query(
            "INSERT INTO task_followers (tenant_id, task_id, user_id) \
             SELECT DISTINCT $1, f.task_id, f.user_id \
             FROM unnest($2::text[] || $2::text[], $3::text[] || $4::text[]) AS f(task_id, user_id) \
             ON CONFLICT DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(&ids)
        .bind(&assignees)
        .bind(vec![self.user.as_str().to_owned(); ids.len()])
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct TemplateRow {
    id: String,
    name: String,
    kind: String,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl TemplateRow {
    /// Fallible on purpose: a stored word this build does not know is a schema
    /// disagreement, and answering with a guessed kind would be worse than
    /// answering with an error.
    fn into_template(self, steps: Vec<ChecklistStep>) -> Result<ChecklistTemplate> {
        Ok(ChecklistTemplate {
            id: HrChecklistTemplateId::new(self.id),
            name: self.name,
            kind: ChecklistKind::parse(&self.kind)?,
            steps,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct StepRow {
    id: String,
    title: String,
    detail: String,
    owner_role: String,
    day_offset: i32,
}

impl StepRow {
    fn into_step(self) -> Result<ChecklistStep> {
        Ok(ChecklistStep {
            id: HrChecklistStepId::new(self.id),
            title: self.title,
            detail: self.detail,
            owner: StepOwner::parse(&self.owner_role)?,
            day_offset: self.day_offset,
        })
    }
}

/// A step read across every template at once, for the list.
#[derive(sqlx::FromRow)]
struct TemplateStepRow {
    template_id: String,
    id: String,
    title: String,
    detail: String,
    owner_role: String,
    day_offset: i32,
}

impl TemplateStepRow {
    fn to_step(&self) -> Result<ChecklistStep> {
        Ok(ChecklistStep {
            id: HrChecklistStepId::new(self.id.clone()),
            title: self.title.clone(),
            detail: self.detail.clone(),
            owner: StepOwner::parse(&self.owner_role)?,
            day_offset: self.day_offset,
        })
    }
}

#[derive(sqlx::FromRow)]
struct SubjectRow {
    given_name: String,
    family_name: String,
    preferred_name: String,
    user_id: Option<String>,
    archived_at: Option<OffsetDateTime>,
    manager_user_id: Option<String>,
}

impl SubjectRow {
    fn into_subject(self) -> Subject {
        let first = if self.preferred_name.trim().is_empty() {
            self.given_name
        } else {
            self.preferred_name
        };
        let display_name = format!("{} {}", first.trim(), self.family_name.trim())
            .trim()
            .to_owned();
        Subject {
            display_name,
            user_id: self.user_id.map(UserId::new),
            manager_user_id: self.manager_user_id.map(UserId::new),
        }
    }
}

#[derive(sqlx::FromRow)]
struct ProgressRow {
    project_id: String,
    name: String,
    total: i64,
    done: i64,
    first_due_at: Option<OffsetDateTime>,
    last_due_at: Option<OffsetDateTime>,
}

impl ProgressRow {
    fn into_progress(self) -> ChecklistProgress {
        ChecklistProgress {
            project_id: ProjectId::new(self.project_id),
            name: self.name,
            total: self.total,
            done: self.done,
            first_due_on: self.first_due_at.map(OffsetDateTime::date),
            last_due_on: self.last_due_at.map(OffsetDateTime::date),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use time::Month;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("a real date")
    }

    fn step(title: &str, owner: StepOwner, day_offset: i32) -> NewChecklistStep {
        NewChecklistStep {
            title: title.to_owned(),
            owner,
            day_offset,
            ..Default::default()
        }
    }

    fn arrival() -> NewChecklistTemplate {
        NewChecklistTemplate {
            name: "Nieuwe collega".to_owned(),
            kind: ChecklistKind::Onboarding,
            steps: vec![
                step("Order the laptop", StepOwner::It, -5),
                step("Sign the contract", StepOwner::Hr, -1),
                step("First-day walkthrough", StepOwner::Manager, 0),
                step("Read the handbook", StepOwner::Employee, 1),
            ],
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(message)) => message,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn both_vocabularies_are_closed_and_round_trip() {
        for kind in [ChecklistKind::Onboarding, ChecklistKind::Offboarding] {
            assert_eq!(ChecklistKind::parse(kind.as_str()).unwrap(), kind);
            assert_eq!(kind.to_string(), kind.as_str());
        }
        assert!(invalid(ChecklistKind::parse("probation")).contains("onboarding"));
        for owner in [
            StepOwner::Hr,
            StepOwner::Manager,
            StepOwner::It,
            StepOwner::Employee,
        ] {
            assert_eq!(StepOwner::parse(owner.as_str()).unwrap(), owner);
            assert_eq!(owner.to_string(), owner.as_str());
        }
        assert!(invalid(StepOwner::parse("facilities")).contains("manager"));
    }

    #[test]
    fn a_template_needs_a_name_and_at_least_one_real_step() {
        assert!(normalize(&arrival()).is_ok());
        let nameless = NewChecklistTemplate {
            name: "  ".to_owned(),
            ..arrival()
        };
        assert!(invalid(normalize(&nameless)).contains("checklist template name"));
        let empty = NewChecklistTemplate {
            steps: Vec::new(),
            ..arrival()
        };
        assert!(invalid(normalize(&empty)).contains("at least one step"));
        let blank_step = NewChecklistTemplate {
            steps: vec![step("   ", StepOwner::Hr, 0)],
            ..arrival()
        };
        assert!(invalid(normalize(&blank_step)).contains("checklist step title"));
        let too_many = NewChecklistTemplate {
            steps: (0..=TEMPLATE_STEPS_MAX)
                .map(|n| step(&format!("Step {n}"), StepOwner::Hr, 0))
                .collect(),
            ..arrival()
        };
        assert!(invalid(normalize(&too_many)).contains("at most"));
    }

    #[test]
    fn an_offset_is_bounded_where_the_schema_bounds_it() {
        let far_future = NewChecklistTemplate {
            steps: vec![step(
                "Probation review",
                StepOwner::Manager,
                STEP_DAY_OFFSET_MAX,
            )],
            ..arrival()
        };
        assert!(normalize(&far_future).is_ok());
        let further = NewChecklistTemplate {
            steps: vec![step("Someday", StepOwner::Manager, STEP_DAY_OFFSET_MAX + 1)],
            ..arrival()
        };
        assert!(invalid(normalize(&further)).contains("days before"));
        let long_ago = NewChecklistTemplate {
            steps: vec![step("Someday", StepOwner::It, -STEP_DAY_OFFSET_MAX - 1)],
            ..arrival()
        };
        assert!(invalid(normalize(&long_ago)).contains("days after"));
    }

    #[test]
    fn a_step_falls_where_the_offset_puts_it() {
        let start = day(2026, Month::September, 1);
        assert_eq!(step_day(start, 0).unwrap(), start);
        assert_eq!(
            step_day(start, -5).unwrap(),
            day(2026, Month::August, 27),
            "five days before a Tuesday start is the Thursday before"
        );
        assert_eq!(step_day(start, 30).unwrap(), day(2026, Month::October, 1));
        // A leap day is an ordinary day to count from and an ordinary one to
        // land on, which is the whole reason offsets are days rather than months.
        assert_eq!(
            step_day(day(2028, Month::February, 29), 1).unwrap(),
            day(2028, Month::March, 1)
        );
        assert!(step_day(Date::MAX, 1).is_err());
        assert!(step_day(Date::MIN, -1).is_err());
    }

    #[test]
    fn a_stated_owner_always_wins_and_an_unstated_one_falls_back_in_order() {
        let actor = UserId::new("u-hr".to_owned());
        let employee = UserId::new("u-newcomer".to_owned());
        let manager = UserId::new("u-boss".to_owned());
        let named = UserId::new("u-it".to_owned());
        let stated = ChecklistOwners {
            it: Some(named.clone()),
            ..Default::default()
        };

        assert_eq!(
            resolve_owner(
                StepOwner::It,
                &stated,
                Some(&employee),
                Some(&manager),
                &actor
            ),
            named,
            "what the caller stated wins"
        );
        assert_eq!(
            resolve_owner(
                StepOwner::Manager,
                &stated,
                Some(&employee),
                Some(&manager),
                &actor
            ),
            manager
        );
        assert_eq!(
            resolve_owner(
                StepOwner::Employee,
                &stated,
                Some(&employee),
                Some(&manager),
                &actor
            ),
            employee
        );
        assert_eq!(
            resolve_owner(StepOwner::Hr, &stated, Some(&employee), None, &actor),
            actor,
            "HR is the person drawing the checklist unless somebody says otherwise"
        );
        // The onboarding case that matters: the newcomer has no login yet and
        // their manager is a record without an account.
        assert_eq!(
            resolve_owner(StepOwner::Employee, &Default::default(), None, None, &actor),
            actor
        );
        assert_eq!(
            resolve_owner(StepOwner::Manager, &Default::default(), None, None, &actor),
            actor
        );
    }

    #[test]
    fn a_run_is_only_complete_when_every_step_is() {
        let mut progress = ChecklistProgress {
            project_id: ProjectId::new("p-1".to_owned()),
            name: "Nieuwe collega — Ada Byron".to_owned(),
            total: 4,
            done: 3,
            first_due_on: None,
            last_due_on: None,
        };
        assert!(!progress.is_complete());
        progress.done = 4;
        assert!(progress.is_complete());
        // An empty board is not a finished one.
        progress.total = 0;
        progress.done = 0;
        assert!(!progress.is_complete());
    }
}
