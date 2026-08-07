//! CRM pipelines — the boards a tenant's deals move across (alo CRM,
//! ADR 0035, wave B2), reached through the account door like every other
//! business record.
//!
//! A pipeline is **tenant-wide**, and a tenant may have several: New
//! Business, Renewals, one per sales team. "Per-team" is satisfied by
//! several boards, not by an access boundary — there is deliberately no
//! per-pipeline permission in B2, because roles are cross-cutting and
//! half-building them here from the narrowest of their callers would settle
//! that design by accident (`docs/design/crm.md`). Every member of a tenant
//! sees every board.
//!
//! A board is **archived, never deleted**: a deal won last year must always
//! be able to name the pipeline it was won on. The one name a tenant's active
//! boards cannot share is each other's — two tabs called "Sales" mean nothing
//! to the person reading them, and that uniqueness is also what makes
//! [`AccountStore::crm_pipelines_or_seed`] race-free without a lock.
//!
//! The columns of a board live in [`crate::crm_stages`]; this file owns the
//! board itself and the first-use seed that creates one.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_field::{bounded, required};
use crate::crm_deals::open_deals_in_pipeline;
use crate::crm_stages::{STAGES_PER_PIPELINE_MAX, insert_stage};
use crate::error::{Result, StoreError};
use crate::id::CrmPipelineId;

/// A pipeline name is a board tab, not a sentence.
pub const PIPELINE_NAME_MAX_CHARS: usize = 120;
/// Room for "what this board is for, and who works it" and no more.
pub const PIPELINE_DESCRIPTION_MAX_CHARS: usize = 1_000;

/// The columns every read of a pipeline selects, in `PipelineRow` order.
const PIPELINE_COLS: &str =
    "id, name, description, archived_at, created_by, created_at, updated_at";

/// The writable shape of a pipeline, used for both create and update (an
/// update is a full replace — the route layer merges a partial `PATCH` onto
/// the stored record before calling).
#[derive(Debug, Clone, Default)]
pub struct NewPipeline {
    /// The board's label. Required, non-blank, unique among the tenant's
    /// active boards.
    pub name: String,
    /// What the board is for; empty is normal.
    pub description: String,
}

/// A stored pipeline.
#[derive(Debug, Clone)]
pub struct Pipeline {
    /// Opaque id, unique within the tenant.
    pub id: CrmPipelineId,
    /// The board's label.
    pub name: String,
    /// What the board is for; empty when unstated.
    pub description: String,
    /// When the board was archived; `None` while active.
    pub archived_at: Option<OffsetDateTime>,
    /// The user who created the record (the seed names the first reader).
    pub created_by: String,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time.
    pub updated_at: OffsetDateTime,
}

impl Pipeline {
    /// Whether the board is archived — hidden from the pickers, still
    /// readable so a closed deal can name where it closed.
    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }
}

/// One column of the board a tenant is seeded with. The **names arrive from
/// the caller**, never from this module: they are shown to a user, so they
/// come through the route edge's i18n catalogue in the requesting user's
/// language (`docs/design/crm.md`, "Seeding") and are ordinary user data from
/// that moment on.
#[derive(Debug, Clone)]
pub struct StageSeed {
    /// The column header, already translated.
    pub name: String,
    /// Whether landing here means the deal was won.
    pub is_won: bool,
    /// Whether landing here means the deal was lost.
    pub is_lost: bool,
}

/// The board a tenant gets on its first read of the module.
#[derive(Debug, Clone)]
pub struct PipelineSeed {
    /// The board's label, already translated.
    pub name: String,
    /// Its columns, left to right.
    pub stages: Vec<StageSeed>,
}

/// A validated, normalised pipeline ready to be bound into a statement.
#[derive(Debug)]
struct Normalized {
    name: String,
    description: String,
}

/// Validates and normalises a whole pipeline. Pure — no database, so the
/// rules are unit-tested directly.
fn normalize(input: &NewPipeline) -> Result<Normalized> {
    Ok(Normalized {
        name: required("name", &input.name, PIPELINE_NAME_MAX_CHARS)?,
        description: bounded(
            "description",
            &input.description,
            PIPELINE_DESCRIPTION_MAX_CHARS,
        )?,
    })
}

/// The seed, checked as strictly as anything a user types: it is our own
/// input, so a bad one is a bug, and a bug that writes a broken board is
/// worse than one that refuses to.
fn normalize_seed(seed: &PipelineSeed) -> Result<(Normalized, Vec<StageSeed>)> {
    let pipeline = normalize(&NewPipeline {
        name: seed.name.clone(),
        description: String::new(),
    })?;
    if seed.stages.is_empty() {
        return Err(StoreError::Validation(
            "a pipeline must be seeded with at least one stage".to_owned(),
        ));
    }
    if seed.stages.len() > STAGES_PER_PIPELINE_MAX {
        return Err(StoreError::Validation(format!(
            "a pipeline may hold at most {STAGES_PER_PIPELINE_MAX} stages"
        )));
    }
    let stages = seed
        .stages
        .iter()
        .map(|stage| {
            Ok(StageSeed {
                name: crate::crm_stages::normalize_stage_name(&stage.name)?,
                is_won: stage.is_won,
                is_lost: stage.is_lost,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    crate::crm_stages::check_outcome_flags(stages.iter().map(|s| (s.is_won, s.is_lost)))?;
    Ok((pipeline, stages))
}

/// Turns the unique-name violation into the [`StoreError::Conflict`] the
/// route edge answers `409` with, and leaves every other database failure
/// alone. The message never echoes the name — it is the caller's own input,
/// but a rule reads better than a value and the habit is worth keeping.
fn map_name_conflict(error: sqlx::Error) -> StoreError {
    match error {
        sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23505") => {
            StoreError::Conflict("a pipeline with that name already exists".to_owned())
        }
        other => StoreError::Db(other),
    }
}

impl AccountStore {
    /// Creates an active pipeline with no stages. A board a user builds by
    /// hand starts empty; the one a tenant is *given* comes from
    /// [`AccountStore::crm_pipelines_or_seed`].
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a blank or over-long name or an
    /// over-long description; [`StoreError::Conflict`] when an active board of
    /// the tenant already carries the name; [`StoreError::Db`] on failure.
    pub async fn create_crm_pipeline(&self, input: &NewPipeline) -> Result<CrmPipelineId> {
        let p = normalize(input)?;
        let id = CrmPipelineId::generate();
        sqlx::query(
            "INSERT INTO crm_pipelines (tenant_id, id, name, description, created_by) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&p.name)
        .bind(&p.description)
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(map_name_conflict)?;
        Ok(id)
    }

    /// The tenant's boards in name order. Archived boards are excluded unless
    /// `include_archived`, and then sort after the active ones.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn crm_pipelines(&self, include_archived: bool) -> Result<Vec<Pipeline>> {
        let rows = sqlx::query_as::<_, PipelineRow>(&format!(
            "SELECT {PIPELINE_COLS} FROM crm_pipelines \
             WHERE tenant_id = $1 AND ($2 OR archived_at IS NULL) \
             ORDER BY (archived_at IS NOT NULL), lower(name), id"
        ))
        .bind(self.tenant.as_str())
        .bind(include_archived)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(PipelineRow::into_pipeline).collect())
    }

    /// The tenant's active boards, **seeding the default one on first use**.
    ///
    /// A tenant that never opens CRM has no rows at all; the first read
    /// creates one board with the columns the caller hands in, in the
    /// requesting user's language. Seeding happens only when the tenant has no
    /// pipeline whatsoever — an archived board still counts, so a tenant that
    /// archived everything is not handed a new "Sales" every morning.
    ///
    /// Two first reads at once do not produce two boards: the loser of the
    /// race hits the active-name uniqueness and simply reads back what the
    /// winner wrote.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the seed itself is malformed (a blank
    /// name, no stages, two winning columns); [`StoreError::Db`] on failure.
    pub async fn crm_pipelines_or_seed(&self, seed: &PipelineSeed) -> Result<Vec<Pipeline>> {
        let (pipeline, stages) = normalize_seed(seed)?;
        if !self.tenant_has_any_crm_pipeline().await? {
            match self.seed_crm_pipeline(&pipeline, &stages).await {
                // A concurrent first read won: its board is the tenant's.
                Ok(()) | Err(StoreError::Conflict(_)) => {}
                Err(other) => return Err(other),
            }
        }
        self.crm_pipelines(false).await
    }

    /// Whether the tenant has ever had a board, archived ones included.
    async fn tenant_has_any_crm_pipeline(&self) -> Result<bool> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM crm_pipelines WHERE tenant_id = $1)",
        )
        .bind(self.tenant.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)
    }

    /// Writes the seeded board and its columns in **one transaction**: a
    /// tenant is never left holding a board with half its columns.
    async fn seed_crm_pipeline(&self, pipeline: &Normalized, stages: &[StageSeed]) -> Result<()> {
        let id = CrmPipelineId::generate();
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO crm_pipelines (tenant_id, id, name, description, created_by) \
             VALUES ($1, $2, $3, '', $4)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&pipeline.name)
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(map_name_conflict)?;
        for (index, stage) in stages.iter().enumerate() {
            // Positions 1, 2, 3 … leave room below the first column for a
            // fractional insert, exactly as a task board does.
            let position = index as f64 + 1.0;
            insert_stage(
                &mut tx,
                self.tenant.as_str(),
                &id,
                &stage.name,
                position,
                stage.is_won,
                stage.is_lost,
            )
            .await?;
        }
        tx.commit().await.map_err(StoreError::Db)
    }

    /// One board of the tenant, or `None` — including when the id belongs to
    /// another tenant (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn crm_pipeline(&self, id: &CrmPipelineId) -> Result<Option<Pipeline>> {
        let row = sqlx::query_as::<_, PipelineRow>(&format!(
            "SELECT {PIPELINE_COLS} FROM crm_pipelines WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(PipelineRow::into_pipeline))
    }

    /// Replaces every writable field of a board. Archiving is a separate
    /// operation ([`AccountStore::set_crm_pipeline_archived`]) so renaming a
    /// board can never make it disappear from the tabs by accident.
    ///
    /// # Errors
    /// [`StoreError::Validation`] as for create; [`StoreError::NotFound`] when
    /// the board isn't the tenant's; [`StoreError::Conflict`] when the new name
    /// is another active board's; [`StoreError::Db`] on failure.
    pub async fn update_crm_pipeline(&self, id: &CrmPipelineId, input: &NewPipeline) -> Result<()> {
        let p = normalize(input)?;
        let done = sqlx::query(
            "UPDATE crm_pipelines SET name = $3, description = $4, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&p.name)
        .bind(&p.description)
        .execute(&self.pool)
        .await
        .map_err(map_name_conflict)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Archives or restores a board. Archiving is the only removal there is,
    /// and it is idempotent — archiving an archived board keeps the original
    /// archive time.
    ///
    /// Its stages go with it in the pickers (they are read per board) and stay
    /// exactly where they are in the data, so every closed deal keeps pointing
    /// at the column it closed in.
    ///
    /// Archiving is refused while the board still holds **open deals** (as
    /// built, B2.03): a board that disappears from the tabs with live work on
    /// it takes the work with it. Closed deals do not block — a board full of
    /// won and lost deals is exactly the board a tenant retires.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the board isn't the tenant's;
    /// [`StoreError::Conflict`] when it still holds open deals, or when
    /// restoring would collide with an active board of the same name;
    /// [`StoreError::Db`] on failure.
    pub async fn set_crm_pipeline_archived(
        &self,
        id: &CrmPipelineId,
        archived: bool,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // The exclusive board lock every shape change takes, which a
        // concurrent card move holds shared — so no deal can arrive on the
        // board between the count and the archive.
        self.lock_crm_pipeline(&mut tx, id).await?;
        if archived {
            let open = open_deals_in_pipeline(&mut tx, self.tenant.as_str(), id.as_str()).await?;
            if open > 0 {
                return Err(StoreError::Conflict(format!(
                    "this pipeline still holds {open} open deal(s); move or close them first"
                )));
            }
        }
        let done = sqlx::query(
            "UPDATE crm_pipelines \
             SET archived_at = CASE WHEN $3 THEN COALESCE(archived_at, now()) ELSE NULL END, \
                 updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(archived)
        .execute(&mut *tx)
        .await
        .map_err(map_name_conflict)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        tx.commit().await.map_err(StoreError::Db)
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct PipelineRow {
    id: String,
    name: String,
    description: String,
    archived_at: Option<OffsetDateTime>,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl PipelineRow {
    fn into_pipeline(self) -> Pipeline {
        Pipeline {
            id: CrmPipelineId::new(self.id),
            name: self.name,
            description: self.description,
            archived_at: self.archived_at,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn renewals() -> NewPipeline {
        NewPipeline {
            name: "Renewals".to_owned(),
            description: "Contracts up for renewal this year".to_owned(),
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    fn seed() -> PipelineSeed {
        PipelineSeed {
            name: "Sales".to_owned(),
            stages: vec![
                StageSeed {
                    name: "New".to_owned(),
                    is_won: false,
                    is_lost: false,
                },
                StageSeed {
                    name: "Won".to_owned(),
                    is_won: true,
                    is_lost: false,
                },
                StageSeed {
                    name: "Lost".to_owned(),
                    is_won: false,
                    is_lost: true,
                },
            ],
        }
    }

    #[test]
    fn normalize_trims_and_keeps_the_description_optional() {
        let p = normalize(&NewPipeline {
            name: "  Renewals  ".to_owned(),
            description: "  ".to_owned(),
        })
        .unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(p.name, "Renewals");
        assert_eq!(p.description, "");
    }

    #[test]
    fn name_is_required_and_bounded() {
        for blank in ["", "   ", "\t\n"] {
            let input = NewPipeline {
                name: blank.to_owned(),
                ..renewals()
            };
            assert!(invalid(normalize(&input)).contains("name"));
        }
        let input = NewPipeline {
            name: "x".repeat(PIPELINE_NAME_MAX_CHARS + 1),
            ..renewals()
        };
        assert!(invalid(normalize(&input)).contains("at most"));
        let input = NewPipeline {
            name: "x".repeat(PIPELINE_NAME_MAX_CHARS),
            ..renewals()
        };
        assert!(normalize(&input).is_ok(), "the bound is inclusive");
    }

    #[test]
    fn description_is_bounded() {
        let input = NewPipeline {
            description: "x".repeat(PIPELINE_DESCRIPTION_MAX_CHARS + 1),
            ..renewals()
        };
        assert!(invalid(normalize(&input)).contains("description"));
    }

    #[test]
    fn a_seed_needs_a_name_and_at_least_one_stage() {
        assert!(normalize_seed(&seed()).is_ok());
        let nameless = PipelineSeed {
            name: "  ".to_owned(),
            ..seed()
        };
        assert!(invalid(normalize_seed(&nameless)).contains("name"));
        let empty = PipelineSeed {
            stages: Vec::new(),
            ..seed()
        };
        assert!(invalid(normalize_seed(&empty)).contains("at least one stage"));
    }

    #[test]
    fn a_seed_may_not_carry_two_winning_or_two_losing_columns() {
        let mut two_wins = seed();
        two_wins.stages[0].is_won = true;
        assert!(invalid(normalize_seed(&two_wins)).contains("won"));
        let mut two_losses = seed();
        two_losses.stages[0].is_lost = true;
        assert!(invalid(normalize_seed(&two_losses)).contains("lost"));
        let mut both = seed();
        both.stages[0].is_won = true;
        both.stages[0].is_lost = true;
        assert!(normalize_seed(&both).is_err());
    }

    #[test]
    fn a_seed_stage_name_is_trimmed_and_bounded() {
        let mut padded = seed();
        padded.stages[0].name = "  New  ".to_owned();
        let (_, stages) = normalize_seed(&padded).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(stages[0].name, "New");
        let mut blank = seed();
        blank.stages[0].name = "   ".to_owned();
        assert!(invalid(normalize_seed(&blank)).contains("stage name"));
    }

    #[test]
    fn a_seed_may_not_exceed_the_stage_cap() {
        let over = PipelineSeed {
            name: "Sales".to_owned(),
            stages: (0..=STAGES_PER_PIPELINE_MAX)
                .map(|i| StageSeed {
                    name: format!("Stage {i}"),
                    is_won: false,
                    is_lost: false,
                })
                .collect(),
        };
        assert!(invalid(normalize_seed(&over)).contains("at most"));
    }
}
