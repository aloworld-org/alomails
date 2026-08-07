//! CRM stages — the columns of a pipeline (alo CRM, ADR 0035, wave B2).
//!
//! A stage is a board column with an order and, optionally, one of two flags:
//! `is_won` or `is_lost`. **The flags are what make a column mean "closed"**,
//! not its name — a tenant may call the winning column "Signed", "Gewonnen" or
//! "Facturé", and renaming it is a rename rather than a schema change
//! (`docs/design/crm.md`). A pipeline may therefore hold at most one winning
//! and one losing column: a board with two "Won" columns has no win rate. That
//! rule is a partial unique index, so it holds under concurrency, and its
//! violation is mapped back to the typed [`StoreError::Validation`] the route
//! edge answers `422` with.
//!
//! Stages are **archived, never deleted**, with one deliberate exception: a
//! column created by mistake, which no deal and no history row has ever named,
//! can be deleted outright — as long as it is not the board's last one. Every
//! other retirement is an archive, because a deal closed last year must keep
//! pointing at the column it closed in.
//!
//! Ordering is the fractional `position` a task board carries (ADR 0022): a
//! `DOUBLE PRECISION` that is an ordering and never a quantity. No money
//! passes through this module.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_field::required;
use crate::crm_deals::{open_deals_in_stage, stage_is_spoken_for};
use crate::error::{Result, StoreError};
use crate::id::{CrmPipelineId, CrmStageId};

/// A stage name is a column header: a word or two.
pub const STAGE_NAME_MAX_CHARS: usize = 60;

/// The most columns one board may hold. Far past any real funnel — it is a
/// runaway guard, not a design opinion — but bounded, because an unbounded
/// board is an unbounded query on every read of it.
pub const STAGES_PER_PIPELINE_MAX: usize = 200;

/// The columns every read of a stage selects, in `StageRow` order.
const STAGE_COLS: &str = "id, pipeline_id, name, position, is_won, is_lost, archived_at, \
     created_at, updated_at";

/// The writable shape of a stage. Position is not here: appending is where a
/// new column goes, and reordering is [`AccountStore::move_crm_stage`] — a
/// board drag must not be able to rename a column, and saving an edit form
/// must not be able to reorder the board.
#[derive(Debug, Clone, Default)]
pub struct NewStage {
    /// The column header. Required, non-blank.
    pub name: String,
    /// Whether landing here means the deal was won.
    pub is_won: bool,
    /// Whether landing here means the deal was lost.
    pub is_lost: bool,
}

/// A stored stage.
#[derive(Debug, Clone)]
pub struct Stage {
    /// Opaque id, unique within the tenant.
    pub id: CrmStageId,
    /// The board this column belongs to.
    pub pipeline_id: CrmPipelineId,
    /// The column header.
    pub name: String,
    /// Fractional order within the board, ascending, left to right.
    pub position: f64,
    /// Whether landing here means the deal was won.
    pub is_won: bool,
    /// Whether landing here means the deal was lost.
    pub is_lost: bool,
    /// When the column was archived; `None` while active.
    pub archived_at: Option<OffsetDateTime>,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time.
    pub updated_at: OffsetDateTime,
}

impl Stage {
    /// Whether the column is archived — hidden from the board, still readable
    /// so a closed deal can name where it closed.
    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }

    /// Whether a deal that lands here is closed, either way.
    pub fn is_closed(&self) -> bool {
        self.is_won || self.is_lost
    }
}

/// Validates and trims one column header. Pure, and shared with the first-use
/// seed ([`crate::crm_pipelines`]) so a seeded name and a typed one are held
/// to exactly the same rule.
pub(crate) fn normalize_stage_name(value: &str) -> Result<String> {
    required("stage name", value, STAGE_NAME_MAX_CHARS)
}

/// Checks the win/loss flags of a set of columns that will share one board:
/// no column is both, and no board has two of either.
///
/// The database enforces the same three rules (a `CHECK` and two partial
/// unique indexes) and is the authority under concurrency; this is the pure
/// front door, so a caller building a board gets the rule named before any
/// row is written.
pub(crate) fn check_outcome_flags(flags: impl IntoIterator<Item = (bool, bool)>) -> Result<()> {
    let (mut won, mut lost) = (0_usize, 0_usize);
    for (is_won, is_lost) in flags {
        if is_won && is_lost {
            return Err(StoreError::Validation(
                "a stage cannot be both won and lost".to_owned(),
            ));
        }
        won += usize::from(is_won);
        lost += usize::from(is_lost);
    }
    if won > 1 {
        return Err(StoreError::Validation(
            "a pipeline may have at most one won stage".to_owned(),
        ));
    }
    if lost > 1 {
        return Err(StoreError::Validation(
            "a pipeline may have at most one lost stage".to_owned(),
        ));
    }
    Ok(())
}

/// Turns the board-level uniqueness violations into the typed
/// [`StoreError::Validation`] the route edge answers `422` with, naming which
/// flag was already taken, and leaves every other database failure alone.
fn map_flag_conflict(error: sqlx::Error) -> StoreError {
    let constraint = match error {
        sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23505") => {
            db.constraint().unwrap_or_default().to_owned()
        }
        other => return StoreError::Db(other),
    };
    match constraint.as_str() {
        "crm_stages_one_won" => {
            StoreError::Validation("a pipeline may have at most one won stage".to_owned())
        }
        "crm_stages_one_lost" => {
            StoreError::Validation("a pipeline may have at most one lost stage".to_owned())
        }
        _ => StoreError::Conflict("unique constraint".to_owned()),
    }
}

/// Writes one stage inside `tx`, at an explicit position. The single insert
/// both the public create and the first-use seed go through, so a seeded
/// column and a typed one are the same row.
///
/// # Errors
/// [`StoreError::Validation`] when the board already has a column with the
/// same outcome flag; [`StoreError::Db`] on failure.
pub(crate) async fn insert_stage(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    pipeline: &CrmPipelineId,
    name: &str,
    position: f64,
    is_won: bool,
    is_lost: bool,
) -> Result<CrmStageId> {
    let id = CrmStageId::generate();
    sqlx::query(
        "INSERT INTO crm_stages (tenant_id, id, pipeline_id, name, position, is_won, is_lost) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(tenant)
    .bind(id.as_str())
    .bind(pipeline.as_str())
    .bind(name)
    .bind(position)
    .bind(is_won)
    .bind(is_lost)
    .execute(&mut **tx)
    .await
    .map_err(map_flag_conflict)?;
    Ok(id)
}

/// Rejects a position that is not a real place on the board. `NaN` compares
/// false against everything, so a single one would make the board's order
/// undefined rather than merely wrong.
fn check_position(position: f64) -> Result<()> {
    if !position.is_finite() {
        return Err(StoreError::Validation(
            "position must be a finite number".to_owned(),
        ));
    }
    Ok(())
}

impl AccountStore {
    /// Appends a column to the right-hand end of a board.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a blank or over-long name, a column that
    /// is both won and lost, a board that already has that outcome, or a board
    /// at the stage cap; [`StoreError::NotFound`] when the board isn't the
    /// tenant's; [`StoreError::Db`] on failure.
    pub async fn create_crm_stage(
        &self,
        pipeline: &CrmPipelineId,
        input: &NewStage,
    ) -> Result<CrmStageId> {
        let name = normalize_stage_name(&input.name)?;
        check_outcome_flags([(input.is_won, input.is_lost)])?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Lock the board for the whole append: two columns added at once
        // serialise here instead of computing the same trailing position, and
        // the cap below cannot be walked past by a pair of racing writers.
        self.lock_crm_pipeline(&mut tx, pipeline).await?;
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM crm_stages WHERE tenant_id = $1 AND pipeline_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(pipeline.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if count >= STAGES_PER_PIPELINE_MAX as i64 {
            return Err(StoreError::Validation(format!(
                "a pipeline may hold at most {STAGES_PER_PIPELINE_MAX} stages"
            )));
        }
        // Append: one past the current right-hand column, archived ones
        // included — a restored column must not land on top of a live one.
        let position: f64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM crm_stages \
             WHERE tenant_id = $1 AND pipeline_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(pipeline.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let id = insert_stage(
            &mut tx,
            self.tenant.as_str(),
            pipeline,
            &name,
            position,
            input.is_won,
            input.is_lost,
        )
        .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// One board's columns, left to right. Archived columns are excluded
    /// unless `include_archived`, and then sort in among the others by
    /// position — an archived column keeps its place, because that is where
    /// the deals that closed in it sat.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the board isn't the tenant's (never an
    /// empty list, which would be an existence oracle);
    /// [`StoreError::Db`] on failure.
    pub async fn crm_stages(
        &self,
        pipeline: &CrmPipelineId,
        include_archived: bool,
    ) -> Result<Vec<Stage>> {
        if self.crm_pipeline(pipeline).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let rows = sqlx::query_as::<_, StageRow>(&format!(
            "SELECT {STAGE_COLS} FROM crm_stages \
             WHERE tenant_id = $1 AND pipeline_id = $2 AND ($3 OR archived_at IS NULL) \
             ORDER BY position, created_at, id"
        ))
        .bind(self.tenant.as_str())
        .bind(pipeline.as_str())
        .bind(include_archived)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(StageRow::into_stage).collect())
    }

    /// One column of the tenant, or `None` — including when the id belongs to
    /// another tenant (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn crm_stage(&self, id: &CrmStageId) -> Result<Option<Stage>> {
        let row = sqlx::query_as::<_, StageRow>(&format!(
            "SELECT {STAGE_COLS} FROM crm_stages WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(StageRow::into_stage))
    }

    /// Replaces a column's name and outcome flags. Its board and its place on
    /// the board are not writable here: a column cannot be moved to another
    /// pipeline at all (that would move every deal in it), and reordering is
    /// [`AccountStore::move_crm_stage`].
    ///
    /// # Errors
    /// [`StoreError::Validation`] as for create; [`StoreError::NotFound`] when
    /// the column isn't the tenant's; [`StoreError::Db`] on failure.
    pub async fn update_crm_stage(&self, id: &CrmStageId, input: &NewStage) -> Result<()> {
        let name = normalize_stage_name(&input.name)?;
        check_outcome_flags([(input.is_won, input.is_lost)])?;
        let done = sqlx::query(
            "UPDATE crm_stages SET name = $3, is_won = $4, is_lost = $5, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&name)
        .bind(input.is_won)
        .bind(input.is_lost)
        .execute(&self.pool)
        .await
        .map_err(map_flag_conflict)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Moves a column along its board — the one operation a drag performs.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the position is not a finite number;
    /// [`StoreError::NotFound`] when the column isn't the tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn move_crm_stage(&self, id: &CrmStageId, position: f64) -> Result<()> {
        check_position(position)?;
        let done = sqlx::query(
            "UPDATE crm_stages SET position = $3, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(position)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Archives or restores a column. Idempotent — archiving an archived
    /// column keeps the original archive time.
    ///
    /// An archived column keeps its outcome flag and its position: it is a
    /// column no new deal can be dropped into, not a column that never
    /// existed.
    ///
    /// Archiving is refused while the column still holds **open deals**
    /// (as built, B2.03): hiding a column that work is standing in would hide
    /// the work with it. Closed deals do not block — archiving a column says
    /// "no new work lands here", not "this never happened". The refusal is
    /// atomic against a concurrent move, which holds the same board row
    /// [shared](AccountStore::share_crm_pipeline).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the column isn't the tenant's;
    /// [`StoreError::Conflict`] when it still holds open deals;
    /// [`StoreError::Db`] on failure.
    pub async fn set_crm_stage_archived(&self, id: &CrmStageId, archived: bool) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let pipeline = self.stage_pipeline(&mut tx, id).await?;
        self.lock_crm_pipeline(&mut tx, &CrmPipelineId::new(pipeline))
            .await?;
        if archived {
            let open = open_deals_in_stage(&mut tx, self.tenant.as_str(), id.as_str()).await?;
            if open > 0 {
                return Err(StoreError::Conflict(format!(
                    "this stage still holds {open} open deal(s); move or close them first"
                )));
            }
        }
        let done = sqlx::query(
            "UPDATE crm_stages \
             SET archived_at = CASE WHEN $3 THEN COALESCE(archived_at, now()) ELSE NULL END, \
                 updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(archived)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        tx.commit().await.map_err(StoreError::Db)
    }

    /// Deletes a column outright — the escape hatch for one created by
    /// mistake. A board must keep at least one column, so the last one is
    /// refused; everything else is an archive.
    ///
    /// A column any deal stands in, or any history row has ever named, is
    /// refused too (as built, B2.03): the past named it, so it is archived
    /// rather than deleted. Both foreign keys are `RESTRICT`, so the database
    /// refuses it as well; this check is what turns that refusal into a
    /// sentence a user can act on.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the column isn't the tenant's;
    /// [`StoreError::Conflict`] when it is its board's last column, or a deal
    /// or a history row names it; [`StoreError::Db`] on failure.
    pub async fn delete_crm_stage(&self, id: &CrmStageId) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let pipeline = self.stage_pipeline(&mut tx, id).await?;
        // Lock the board so a concurrent delete of the other last-but-one
        // column cannot leave it with none.
        self.lock_crm_pipeline(&mut tx, &CrmPipelineId::new(pipeline.clone()))
            .await?;
        if stage_is_spoken_for(&mut tx, self.tenant.as_str(), id.as_str()).await? {
            return Err(StoreError::Conflict(
                "a deal has stood in this stage; archive it instead of deleting it".to_owned(),
            ));
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM crm_stages WHERE tenant_id = $1 AND pipeline_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(&pipeline)
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if count <= 1 {
            return Err(StoreError::Conflict(
                "a pipeline must keep at least one stage".to_owned(),
            ));
        }
        let done = sqlx::query("DELETE FROM crm_stages WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        tx.commit().await.map_err(StoreError::Db)
    }

    /// The board one column belongs to, read inside `tx`. A foreign or absent
    /// column id is the same `NotFound`.
    async fn stage_pipeline(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &CrmStageId,
    ) -> Result<String> {
        sqlx::query_scalar::<_, String>(
            "SELECT pipeline_id FROM crm_stages WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)
    }

    /// Takes the board's **exclusive** row lock inside `tx`, proving in the
    /// same breath that it is this tenant's. A foreign or absent id is the same
    /// `NotFound`.
    ///
    /// The board row is the module's one coordination point: everything that
    /// changes the *shape* of a board — adding, deleting or archiving a column,
    /// archiving the board — takes it exclusively, and everything that moves a
    /// card takes it [shared](AccountStore::share_crm_pipeline). Card moves
    /// therefore never block each other, and none of them can slip past a
    /// column being archived.
    pub(crate) async fn lock_crm_pipeline(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        pipeline: &CrmPipelineId,
    ) -> Result<()> {
        self.hold_crm_pipeline(tx, pipeline, "FOR UPDATE").await
    }

    /// Takes the board's **shared** row lock inside `tx` — what creating or
    /// moving a deal holds while it writes ([`crate::crm_deals`]).
    pub(crate) async fn share_crm_pipeline(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        pipeline: &CrmPipelineId,
    ) -> Result<()> {
        self.hold_crm_pipeline(tx, pipeline, "FOR SHARE").await
    }

    /// The one statement behind both locks. `mode` is a literal from this
    /// module and never caller input — no request value reaches this string.
    async fn hold_crm_pipeline(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        pipeline: &CrmPipelineId,
        mode: &'static str,
    ) -> Result<()> {
        sqlx::query_scalar::<_, String>(&format!(
            "SELECT id FROM crm_pipelines WHERE tenant_id = $1 AND id = $2 {mode}"
        ))
        .bind(self.tenant.as_str())
        .bind(pipeline.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)?;
        Ok(())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct StageRow {
    id: String,
    pipeline_id: String,
    name: String,
    position: f64,
    is_won: bool,
    is_lost: bool,
    archived_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl StageRow {
    fn into_stage(self) -> Stage {
        Stage {
            id: CrmStageId::new(self.id),
            pipeline_id: CrmPipelineId::new(self.pipeline_id),
            name: self.name,
            position: self.position,
            is_won: self.is_won,
            is_lost: self.is_lost,
            archived_at: self.archived_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn a_stage_name_is_required_trimmed_and_bounded() {
        assert_eq!(
            normalize_stage_name("  Qualified  ").unwrap_or_default(),
            "Qualified"
        );
        for blank in ["", "   ", "\t\n"] {
            assert!(invalid(normalize_stage_name(blank)).contains("stage name"));
        }
        assert!(normalize_stage_name(&"x".repeat(STAGE_NAME_MAX_CHARS)).is_ok());
        assert!(
            invalid(normalize_stage_name(&"x".repeat(STAGE_NAME_MAX_CHARS + 1)))
                .contains("at most")
        );
        // A column header in Greek is sixty characters, not thirty.
        assert!(normalize_stage_name(&"Ω".repeat(STAGE_NAME_MAX_CHARS)).is_ok());
    }

    #[test]
    fn a_stage_is_won_or_lost_or_neither_but_never_both() {
        assert!(check_outcome_flags([(false, false)]).is_ok());
        assert!(check_outcome_flags([(true, false)]).is_ok());
        assert!(check_outcome_flags([(false, true)]).is_ok());
        assert!(invalid(check_outcome_flags([(true, true)])).contains("both"));
    }

    #[test]
    fn a_board_carries_at_most_one_of_each_outcome() {
        let ordinary = [(false, false), (true, false), (false, true)];
        assert!(check_outcome_flags(ordinary).is_ok());
        assert!(
            invalid(check_outcome_flags([(true, false), (true, false)])).contains("one won stage")
        );
        assert!(
            invalid(check_outcome_flags([(false, true), (false, true)])).contains("one lost stage")
        );
    }

    #[test]
    fn a_position_must_be_a_real_place_on_the_board() {
        for ok in [0.0, 1.0, 1.5, -2.0, f64::MAX, f64::MIN] {
            assert!(check_position(ok).is_ok(), "expected valid: {ok}");
        }
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(invalid(check_position(bad)).contains("finite"));
        }
    }

    #[test]
    fn the_closed_flags_read_as_one_question() {
        let stage = |is_won, is_lost| Stage {
            id: CrmStageId::new("s"),
            pipeline_id: CrmPipelineId::new("p"),
            name: "Stage".to_owned(),
            position: 1.0,
            is_won,
            is_lost,
            archived_at: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        };
        assert!(!stage(false, false).is_closed());
        assert!(stage(true, false).is_closed());
        assert!(stage(false, true).is_closed());
        assert!(!stage(false, false).is_archived());
    }
}
