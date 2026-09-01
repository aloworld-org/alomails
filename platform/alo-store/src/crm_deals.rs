//! CRM deals — the opportunities that move across a board, and the
//! append-only history of every move they made (alo CRM, ADR 0035, wave B2).
//!
//! A deal is **tenant-wide**, like the board it stands on: every member of the
//! tenant reads every deal, and the owner is a name on the record rather than
//! an access boundary (`docs/design/crm.md`, "Pipelines and stages").
//!
//! Three rules shape this file:
//!
//! - **A lifecycle change is its own call, never a field on the edit.**
//!   [`AccountStore::update_crm_deal`] cannot move a deal, reposition it, or
//!   close it; [`AccountStore::move_crm_deal`] does exactly that and nothing
//!   else. A stale edit form must not be able to win a deal.
//! - **The closing snapshot is written at the moment of the move.** `outcome`,
//!   `lost_reason` and `closed_at` are columns on the deal, not a join to the
//!   stage's flags, so re-flagging a column next year never rewrites last
//!   year's win rate — the same reason a billing line snapshots its price.
//! - **History is transactional.** Every stage change appends exactly one
//!   [`StageEvent`] in the same transaction as the move, and creating a deal
//!   writes the first one with no `from` stage. The audit log (best-effort,
//!   free text) answers "who changed this record"; these rows answer "what did
//!   this deal do", and neither replaces the other.
//!
//! Money is integer cents, always. The only `f64` here is `position`, which is
//! an ordering on a board and never a quantity.

use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::{bounded, currency, required};
use crate::error::{Result, StoreError};
use crate::id::{BillingCustomerId, ContactId, CrmDealId, CrmEventId, CrmPipelineId, CrmStageId};

/// A deal title is a line on a card: "Renewal — Acme GmbH", not a brief.
pub const DEAL_TITLE_MAX_CHARS: usize = 200;
/// The company, contact and lost-reason strings a deal carries itself. The
/// company and contact bounds match a customer's name, because winning a lead
/// creates a customer from exactly these fields.
pub const DEAL_PARTY_MAX_CHARS: usize = 200;
/// RFC 5321 caps a path at 256 octets; 320 is the everyday `local@domain`
/// ceiling and is what billing's customer email uses.
pub const DEAL_EMAIL_MAX_CHARS: usize = 320;
/// Where the opportunity came from — a word from the tenant's own vocabulary.
pub const DEAL_SOURCE_MAX_CHARS: usize = 60;

/// The most a deal may be worth, in integer cents: one billion euro.
///
/// An `i64`-safe ceiling no SME deal reaches, chosen so the pipeline report can
/// sum it: 10^11 cents × 10^4 deals is 10^15, comfortably inside `i64`. A
/// negative deal value is not a discount, it is a typo.
pub const DEAL_VALUE_MAX_CENTS: i64 = 100_000_000_000;

/// The columns every read of a deal selects, in `DealRow` order. Aliased `d`
/// because the list read joins the stage it stands in to order the board.
const DEAL_COLS: &str = "d.id, d.pipeline_id, d.stage_id, d.title, d.customer_id, d.contact_id, \
     d.company_name, d.contact_name, d.contact_email, d.value_cents, d.currency, \
     d.expected_close, d.owner_user_id, d.source, d.position, d.outcome, d.lost_reason, \
     d.closed_at, d.created_by, d.created_at, d.updated_at";

/// Where a deal stands, derived on read from its closing snapshot — the same
/// spirit as an invoice's `overdue`, and the value the list filter takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DealState {
    /// Still being worked: no outcome has been snapshotted.
    Open,
    /// Closed in a stage flagged `is_won`.
    Won,
    /// Closed in a stage flagged `is_lost`, with a reason.
    Lost,
}

impl DealState {
    /// The value the `outcome` column carries, and the one a filter sends.
    /// `open` is not stored — it is the absence of an outcome — but it reads
    /// back through `COALESCE`, so one word means one thing on both sides.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Won => "won",
            Self::Lost => "lost",
        }
    }

    /// Parses a state a caller sent, or `None` if it is not one we know. The
    /// route edge answers `None` with `422`: a filter that is not recognised
    /// must never widen silently into "everything".
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "won" => Some(Self::Won),
            "lost" => Some(Self::Lost),
            _ => None,
        }
    }

    /// Whether a deal in this state is closed, either way.
    pub fn is_closed(self) -> bool {
        !matches!(self, Self::Open)
    }
}

/// The writable shape of a deal, used for both create and update (an update is
/// a full replace — the route layer merges a partial `PATCH` onto the stored
/// record before calling).
///
/// The board is deliberately absent: which column a deal stands in, where it
/// sits in that column, and whether it is closed all belong to
/// [`AccountStore::move_crm_deal`].
#[derive(Debug, Clone)]
pub struct NewDeal {
    /// What the opportunity is. Required, non-blank.
    pub title: String,
    /// The company the tenant already invoices, or `None` while this is a
    /// lead.
    pub customer_id: Option<BillingCustomerId>,
    /// A pointer into an address book. **Contacts are per user**, so this may
    /// not resolve for a colleague reading the deal — never an error, never a
    /// blank deal, which is why the two fields below exist.
    pub contact_id: Option<ContactId>,
    /// The company, while the deal is still a lead.
    pub company_name: String,
    /// Who at the company is being spoken to.
    pub contact_name: String,
    /// Their email address — the field the thread suggestions (B2.05) match
    /// against.
    pub contact_email: String,
    /// What the deal is worth, in integer cents of `currency`.
    pub value_cents: i64,
    /// ISO 4217 code. The pipeline report groups by it rather than converting.
    pub currency: String,
    /// The day it is expected to close, or `None` when nobody has said.
    pub expected_close: Option<Date>,
    /// Whose deal it is, or `None` for the acting user. Must be a user of this
    /// tenant.
    pub owner_user_id: Option<String>,
    /// Where the opportunity came from; a tenant's own vocabulary.
    pub source: String,
}

impl Default for NewDeal {
    fn default() -> Self {
        Self {
            title: String::new(),
            customer_id: None,
            contact_id: None,
            company_name: String::new(),
            contact_name: String::new(),
            contact_email: String::new(),
            value_cents: 0,
            currency: crate::billing_field::DEFAULT_CURRENCY.to_owned(),
            expected_close: None,
            owner_user_id: None,
            source: String::new(),
        }
    }
}

/// A stored deal.
#[derive(Debug, Clone)]
pub struct Deal {
    /// Opaque id, unique within the tenant.
    pub id: CrmDealId,
    /// The board it is on.
    pub pipeline_id: CrmPipelineId,
    /// The column it stands in right now.
    pub stage_id: CrmStageId,
    /// What the opportunity is.
    pub title: String,
    /// The customer, once there is one.
    pub customer_id: Option<BillingCustomerId>,
    /// The address-book pointer, which may not resolve for every reader.
    pub contact_id: Option<ContactId>,
    /// The company, while the deal is still a lead.
    pub company_name: String,
    /// Who at the company is being spoken to.
    pub contact_name: String,
    /// Their email address.
    pub contact_email: String,
    /// What the deal is worth, in integer cents.
    pub value_cents: i64,
    /// ISO 4217 code, uppercase.
    pub currency: String,
    /// The day it is expected to close.
    pub expected_close: Option<Date>,
    /// Whose deal it is.
    pub owner_user_id: String,
    /// Where the opportunity came from.
    pub source: String,
    /// Fractional order within its column.
    pub position: f64,
    /// Why it is closed, `None` while open.
    pub lost_reason: Option<String>,
    /// When it closed, `None` while open.
    pub closed_at: Option<OffsetDateTime>,
    /// The user who created the record.
    pub created_by: String,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time.
    pub updated_at: OffsetDateTime,
    /// The stored outcome, read through [`Deal::state`].
    outcome: Option<DealState>,
}

impl Deal {
    /// Where the deal stands: `open` until it is moved into a flagged column.
    pub fn state(&self) -> DealState {
        self.outcome.unwrap_or(DealState::Open)
    }

    /// Whether the deal has been closed, either way.
    pub fn is_closed(&self) -> bool {
        self.state().is_closed()
    }

    /// The blank a fresh card carries, for the pure tests of this crate's CRM
    /// modules — the only way a sibling module can build a `Deal` at all, since
    /// `outcome` is private so that [`Deal::state`] is the one way to read it.
    /// Tests state the fields they care about and leave the rest alone.
    #[cfg(test)]
    pub(crate) fn blank(id: &str) -> Self {
        Self {
            id: CrmDealId::new(id),
            pipeline_id: CrmPipelineId::new("pip_test"),
            stage_id: CrmStageId::new("stg_test"),
            title: String::new(),
            customer_id: None,
            contact_id: None,
            company_name: String::new(),
            contact_name: String::new(),
            contact_email: String::new(),
            value_cents: 0,
            currency: crate::billing_field::DEFAULT_CURRENCY.to_owned(),
            expected_close: None,
            owner_user_id: "usr_test".to_owned(),
            source: String::new(),
            position: 1.0,
            lost_reason: None,
            closed_at: None,
            created_by: "usr_test".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            outcome: None,
        }
    }
}

/// One move a deal made, appended in the transaction that made it.
#[derive(Debug, Clone)]
pub struct StageEvent {
    /// Opaque id, unique within the tenant.
    pub id: CrmEventId,
    /// The deal that moved.
    pub deal_id: CrmDealId,
    /// Where it came from; `None` on the row written when it was created.
    pub from_stage_id: Option<CrmStageId>,
    /// Where it went.
    pub to_stage_id: CrmStageId,
    /// Who moved it.
    pub moved_by: String,
    /// When.
    pub moved_at: OffsetDateTime,
}

/// A move: the target column, optionally where in it, and — when the column is
/// flagged `is_lost` — why.
#[derive(Debug, Clone)]
pub struct StageMove {
    /// The column to land in. Must belong to the deal's own board.
    pub stage_id: CrmStageId,
    /// Where in the column, or `None` to append to the end of it. A move that
    /// names the column the deal is already in and no position is a no-op that
    /// still costs a statement, not an error.
    pub position: Option<f64>,
    /// Why the deal was lost. Required when the target column is flagged
    /// `is_lost`, refused otherwise — a reason on a won deal is a mistake, not
    /// a note.
    pub lost_reason: Option<String>,
}

impl StageMove {
    /// A plain move to a column, appending to its end.
    pub fn to(stage_id: CrmStageId) -> Self {
        Self {
            stage_id,
            position: None,
            lost_reason: None,
        }
    }
}

/// The list surface's filters. Every one of them is exact: an unrecognised
/// value is rejected at the route edge rather than silently widening the list,
/// because a sales manager reading "everything" when they asked for "mine" is
/// a wrong number on a screen.
#[derive(Debug, Clone, Default)]
pub struct DealFilter {
    /// Only deals on this board.
    pub pipeline_id: Option<CrmPipelineId>,
    /// Only deals in this column.
    pub stage_id: Option<CrmStageId>,
    /// Only deals owned by this user.
    pub owner_user_id: Option<String>,
    /// Only deals in this state.
    pub state: Option<DealState>,
}

/// A validated, normalised deal ready to be bound into a statement.
///
/// Visible to the crate because the lead import (B2.09) validates a whole file
/// **before** it opens the transaction it writes in: normalising resolves a
/// customer, a contact and an owner against the pool, and doing that while
/// holding a transaction would have a writer waiting on a second connection
/// it might not get.
#[derive(Debug)]
pub(crate) struct Normalized {
    title: String,
    customer_id: Option<String>,
    contact_id: Option<String>,
    company_name: String,
    contact_name: String,
    contact_email: String,
    value_cents: i64,
    currency: String,
    expected_close: Option<Date>,
    owner_user_id: String,
    source: String,
}

/// Validates a deal value in integer cents. Zero is legitimate and common: an
/// opportunity often exists before anybody has priced it.
fn value_cents(value: i64) -> Result<i64> {
    if !(0..=DEAL_VALUE_MAX_CENTS).contains(&value) {
        return Err(StoreError::Validation(format!(
            "deal value must be between 0 and {DEAL_VALUE_MAX_CENTS} cents"
        )));
    }
    Ok(value)
}

/// Validates the reason a deal was lost against the column it is moving into:
/// a losing column demands one, every other column refuses one.
fn lost_reason(reason: Option<&str>, target_is_lost: bool) -> Result<Option<String>> {
    let stated = match reason {
        Some(value) => {
            let value = bounded("lost reason", value, DEAL_PARTY_MAX_CHARS)?;
            (!value.is_empty()).then_some(value)
        }
        None => None,
    };
    match (target_is_lost, stated) {
        (true, Some(value)) => Ok(Some(value)),
        (true, None) => Err(StoreError::Validation(
            "a lost deal needs a reason".to_owned(),
        )),
        (false, Some(_)) => Err(StoreError::Validation(
            "a lost reason belongs only to a deal moved into a losing stage".to_owned(),
        )),
        (false, None) => Ok(None),
    }
}

/// Rejects a position that is not a real place in a column. `NaN` compares
/// false against everything, so one would make the column's order undefined
/// rather than merely wrong.
fn check_position(position: f64) -> Result<()> {
    if !position.is_finite() {
        return Err(StoreError::Validation(
            "position must be a finite number".to_owned(),
        ));
    }
    Ok(())
}

/// Translates a violation of the address-book foreign key into the same
/// [`StoreError::NotFound`] an unknown contact id gets. This is the race
/// window: the contact existed when we checked and was deleted before the
/// write landed.
fn map_contact_fk(error: sqlx::Error) -> StoreError {
    match error {
        sqlx::Error::Database(ref db) if db.constraint() == Some("crm_deals_contact_fk") => {
            StoreError::NotFound
        }
        other => StoreError::Db(other),
    }
}

/// Turns a stored outcome into a state, or reports corrupt data. An outcome
/// the code does not know is not user input — it is a row that should not
/// exist — so it is a decode failure rather than a guess, because guessing
/// here would mean reporting a lost deal as won.
fn parse_outcome(stored: Option<&str>) -> Result<Option<DealState>> {
    match stored {
        None => Ok(None),
        Some(value) => match DealState::parse(value) {
            Some(DealState::Won) => Ok(Some(DealState::Won)),
            Some(DealState::Lost) => Ok(Some(DealState::Lost)),
            _ => Err(StoreError::Db(sqlx::Error::Decode(
                "crm_deals.outcome is not a known outcome".into(),
            ))),
        },
    }
}

/// The stage a move resolved to, with the two facts the move decides against.
pub(crate) struct TargetStage {
    pub(crate) id: String,
    is_won: bool,
    is_lost: bool,
}

impl AccountStore {
    /// Creates a deal in a column of a board, and writes the first history row
    /// (`from` = nothing) in the same transaction — so "how long did this sit
    /// in Qualified" is answerable from row one.
    ///
    /// The card is appended to the end of the column. A deal is always created
    /// **open**, whatever the column's flags: closing is a move, and a deal
    /// that was never worked was never won.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a blank or over-long title, a value out of
    /// range, an unknown currency, a stage that is not this board's or is
    /// archived, or an owner who is not a user of this tenant;
    /// [`StoreError::NotFound`] when the board, the column, the customer or the
    /// contact is not this tenant's; [`StoreError::Db`] on failure.
    pub async fn create_crm_deal(
        &self,
        pipeline: &CrmPipelineId,
        stage: &CrmStageId,
        input: &NewDeal,
    ) -> Result<CrmDealId> {
        // Normalised before the transaction, never inside it: see `Normalized`.
        let d = self.normalize_deal(input).await?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Share the board with every other writer moving a card on it, and
        // exclude the archive/delete paths that need it to hold still.
        self.share_crm_pipeline(&mut tx, pipeline).await?;
        let id = self
            .insert_crm_deal_in(&mut tx, pipeline, stage, &d)
            .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// The write half of [`AccountStore::create_crm_deal`], inside a
    /// transaction the caller owns and under a board lock the caller has
    /// already taken.
    ///
    /// It exists for the lead import (B2.09), which writes a whole file of
    /// deals or none of them and therefore cannot use one transaction per
    /// card. Everything a created deal *is* — the appended position, the first
    /// history row — happens here, so an imported deal and a typed one are the
    /// same record made the same way. The validation is the caller's, done
    /// before the transaction was opened ([`AccountStore::normalize_deal`]).
    ///
    /// # Errors
    /// As [`AccountStore::create_crm_deal`].
    pub(crate) async fn insert_crm_deal_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        pipeline: &CrmPipelineId,
        stage: &CrmStageId,
        d: &Normalized,
    ) -> Result<CrmDealId> {
        let id = CrmDealId::generate();
        let target = self.resolve_target_stage(tx, pipeline, stage).await?;
        let position = next_position(tx, self.tenant.as_str(), &target.id).await?;
        sqlx::query(
            "INSERT INTO crm_deals (tenant_id, id, pipeline_id, stage_id, title, customer_id, \
             contact_id, company_name, contact_name, contact_email, value_cents, currency, \
             expected_close, owner_user_id, source, position, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(pipeline.as_str())
        .bind(&target.id)
        .bind(&d.title)
        .bind(&d.customer_id)
        .bind(&d.contact_id)
        .bind(&d.company_name)
        .bind(&d.contact_name)
        .bind(&d.contact_email)
        .bind(d.value_cents)
        .bind(&d.currency)
        .bind(d.expected_close)
        .bind(&d.owner_user_id)
        .bind(&d.source)
        .bind(position)
        .bind(self.user.as_str())
        .execute(&mut **tx)
        .await
        .map_err(map_contact_fk)?;
        self.append_stage_event(tx, &id, None, &target.id).await?;
        Ok(id)
    }

    /// One deal of the tenant, or `None` — including when the id belongs to
    /// another tenant (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure, or when the stored outcome is corrupt.
    pub async fn crm_deal(&self, id: &CrmDealId) -> Result<Option<Deal>> {
        let row = sqlx::query_as::<_, DealRow>(&format!(
            "SELECT {DEAL_COLS} FROM crm_deals d WHERE d.tenant_id = $1 AND d.id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(DealRow::into_deal).transpose()
    }

    /// The tenant's deals in **board order** — column by column, card by card —
    /// narrowed by whichever filters the caller stated.
    ///
    /// Every filter is exact and every one of them is optional; they compose,
    /// so "my open deals on the New Business board" is one read. A filter
    /// naming a board or a column of another tenant matches nothing, which is
    /// the same answer as a board that does not exist.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure, or when a stored outcome is corrupt.
    pub async fn crm_deals(&self, filter: &DealFilter) -> Result<Vec<Deal>> {
        let rows = sqlx::query_as::<_, DealRow>(&format!(
            "SELECT {DEAL_COLS} FROM crm_deals d \
             JOIN crm_stages s ON s.tenant_id = d.tenant_id AND s.id = d.stage_id \
             WHERE d.tenant_id = $1 \
               AND ($2::text IS NULL OR d.pipeline_id = $2) \
               AND ($3::text IS NULL OR d.stage_id = $3) \
               AND ($4::text IS NULL OR d.owner_user_id = $4) \
               AND ($5::text IS NULL OR COALESCE(d.outcome, 'open') = $5) \
             ORDER BY s.position, d.position, d.created_at, d.id"
        ))
        .bind(self.tenant.as_str())
        .bind(filter.pipeline_id.as_ref().map(CrmPipelineId::as_str))
        .bind(filter.stage_id.as_ref().map(CrmStageId::as_str))
        .bind(filter.owner_user_id.as_deref())
        .bind(filter.state.map(DealState::as_str))
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(DealRow::into_deal).collect()
    }

    /// Replaces every writable field of a deal. The board, the column, the
    /// place in it and the closing snapshot are **not** writable here: moving
    /// a deal writes history and can close it, so it must not happen because
    /// an editor submitted a stale form.
    ///
    /// # Errors
    /// [`StoreError::Validation`] as for create; [`StoreError::NotFound`] when
    /// the deal, the customer or the contact is not this tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn update_crm_deal(&self, id: &CrmDealId, input: &NewDeal) -> Result<()> {
        let d = self.normalize_deal(input).await?;
        let done = sqlx::query(
            "UPDATE crm_deals SET title = $3, customer_id = $4, contact_id = $5, \
             company_name = $6, contact_name = $7, contact_email = $8, value_cents = $9, \
             currency = $10, expected_close = $11, owner_user_id = $12, source = $13, \
             updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&d.title)
        .bind(&d.customer_id)
        .bind(&d.contact_id)
        .bind(&d.company_name)
        .bind(&d.contact_name)
        .bind(&d.contact_email)
        .bind(d.value_cents)
        .bind(&d.currency)
        .bind(d.expected_close)
        .bind(&d.owner_user_id)
        .bind(&d.source)
        .execute(&self.pool)
        .await
        .map_err(map_contact_fk)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Moves a deal to a column of its own board, and appends exactly one
    /// history row when the column actually changed.
    ///
    /// In one transaction it re-reads the deal under its row lock, checks the
    /// target column belongs to the **same board** (a board is not a place to
    /// lose a deal into another team's funnel), writes the column and the
    /// position, writes or clears the closing snapshot, and appends the event.
    ///
    /// A move **within** one column is a reposition: it writes no event,
    /// because a history row saying New → New answers no question and would
    /// spoil every velocity figure computed from these rows.
    ///
    /// Moving a closed deal back to an open column is allowed and clears the
    /// snapshot, leaving both events standing. A deal is our own private record
    /// of an opportunity — unlike a quote, whose terminal states are a document
    /// the customer holds — and pretending it cannot reopen just produces a
    /// second deal for the same customer and a win rate counted twice.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the column is not this deal's board's,
    /// is archived, the position is not finite, or the lost reason is missing
    /// (or given where it does not belong); [`StoreError::NotFound`] when the
    /// deal or the column is not this tenant's; [`StoreError::Db`] on failure.
    pub async fn move_crm_deal(&self, id: &CrmDealId, mv: &StageMove) -> Result<()> {
        if let Some(position) = mv.position {
            check_position(position)?;
        }
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let (pipeline, from_stage) = self.lock_crm_deal(&mut tx, id).await?;
        let pipeline = CrmPipelineId::new(pipeline);
        // The same shared board lock the create path takes: a move may not
        // slip past a stage or board being archived.
        self.share_crm_pipeline(&mut tx, &pipeline).await?;
        let target = self
            .resolve_target_stage(&mut tx, &pipeline, &mv.stage_id)
            .await?;
        let reason = lost_reason(mv.lost_reason.as_deref(), target.is_lost)?;
        let moved = target.id != from_stage;
        let position = match mv.position {
            Some(position) => position,
            // A card that changed column lands at the end of the new one; one
            // that stayed keeps its place rather than jumping to the bottom.
            None if moved => next_position(&mut tx, self.tenant.as_str(), &target.id).await?,
            None => sqlx::query_scalar::<_, f64>(
                "SELECT position FROM crm_deals WHERE tenant_id = $1 AND id = $2",
            )
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .fetch_one(&mut *tx)
            .await
            .map_err(StoreError::Db)?,
        };
        let outcome = if target.is_won {
            Some(DealState::Won.as_str())
        } else if target.is_lost {
            Some(DealState::Lost.as_str())
        } else {
            None
        };
        sqlx::query(
            // The CASE reads the OLD outcome, so `closed_at` is the moment the
            // deal entered the outcome it now has: cleared on a reopen, stamped
            // afresh when a won deal is later marked lost, and left alone when
            // the card merely moved within the same outcome.
            "UPDATE crm_deals SET stage_id = $3, position = $4, outcome = $5, lost_reason = $6, \
             closed_at = CASE WHEN $5::text IS NULL THEN NULL \
                              WHEN outcome IS DISTINCT FROM $5 THEN now() \
                              ELSE closed_at END, \
             updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&target.id)
        .bind(position)
        .bind(outcome)
        .bind(&reason)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if moved {
            self.append_stage_event(&mut tx, id, Some(&from_stage), &target.id)
                .await?;
        }
        tx.commit().await.map_err(StoreError::Db)
    }

    /// One deal's stage history, oldest first. Its first row is the creation,
    /// which carries no `from` stage.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the deal is not this tenant's (never an
    /// empty list, which would be an existence oracle);
    /// [`StoreError::Db`] on failure.
    pub async fn crm_deal_history(&self, id: &CrmDealId) -> Result<Vec<StageEvent>> {
        if self.crm_deal(id).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let rows = sqlx::query_as::<_, EventRow>(
            "SELECT id, deal_id, from_stage_id, to_stage_id, moved_by, moved_at \
             FROM crm_deal_stage_events WHERE tenant_id = $1 AND deal_id = $2 \
             ORDER BY moved_at, id",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(EventRow::into_event).collect())
    }

    /// Deletes a deal and its history outright.
    ///
    /// Deals are the one CRM record that is deleted rather than archived: it is
    /// our own private note of an opportunity, not a document anybody else
    /// holds, so a deal raised by mistake should leave no trace. A deal that
    /// was really worked is *lost*, which is a move, not a delete.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the deal is not this tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_crm_deal(&self, id: &CrmDealId) -> Result<()> {
        let has_project: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM crm_deal_projects WHERE tenant_id = $1 AND deal_id = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if has_project {
            return Err(StoreError::Conflict(
                "this deal has a delivery project and cannot be deleted".to_owned(),
            ));
        }
        let done = sqlx::query("DELETE FROM crm_deals WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Validates a deal and resolves everything it points at **within this
    /// tenant**: the customer must be one of ours and not archived, the
    /// contact one of ours, the owner a user of ours. A guessed id from another
    /// tenant is a `NotFound`, never a cross-tenant link.
    pub(crate) async fn normalize_deal(&self, input: &NewDeal) -> Result<Normalized> {
        let customer_id = match &input.customer_id {
            Some(id) => {
                let customer = self
                    .billing_customer(id)
                    .await?
                    .ok_or(StoreError::NotFound)?;
                if customer.is_archived() {
                    return Err(StoreError::Validation(
                        "the customer is archived; restore it before raising deals for it"
                            .to_owned(),
                    ));
                }
                Some(customer.id.as_str().to_owned())
            }
            None => None,
        };
        let contact_id = input.contact_id.as_ref().map(|c| c.as_str().to_owned());
        self.require_tenant_contact(contact_id.as_ref()).await?;
        let owner_user_id = match input.owner_user_id.as_deref() {
            Some(owner) => {
                self.require_tenant_user(owner).await?;
                owner.to_owned()
            }
            None => self.user.as_str().to_owned(),
        };
        Ok(Normalized {
            title: required("title", &input.title, DEAL_TITLE_MAX_CHARS)?,
            customer_id,
            contact_id,
            company_name: bounded("company name", &input.company_name, DEAL_PARTY_MAX_CHARS)?,
            contact_name: bounded("contact name", &input.contact_name, DEAL_PARTY_MAX_CHARS)?,
            contact_email: bounded("contact email", &input.contact_email, DEAL_EMAIL_MAX_CHARS)?,
            value_cents: value_cents(input.value_cents)?,
            currency: currency(&input.currency)?,
            expected_close: input.expected_close,
            owner_user_id,
            source: bounded("source", &input.source, DEAL_SOURCE_MAX_CHARS)?,
        })
    }

    /// Confirms an owner is a user of **this tenant**: an owner from another
    /// tenant is a name on a record the other tenant can never read, so it is
    /// refused rather than stored.
    async fn require_tenant_user(&self, user_id: &str) -> Result<()> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM users WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if exists {
            return Ok(());
        }
        Err(StoreError::Validation(
            "the owner must be a user of this tenant".to_owned(),
        ))
    }

    /// Takes a deal's row lock inside `tx` and returns the board and column it
    /// stands in, so a caller can decide and then write without another
    /// transaction slipping in between. Two movers of one card serialise here.
    async fn lock_crm_deal(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &CrmDealId,
    ) -> Result<(String, String)> {
        sqlx::query_as::<_, (String, String)>(
            "SELECT pipeline_id, stage_id FROM crm_deals \
             WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)
    }

    /// Resolves the column a card is going into: this tenant's, this board's,
    /// and not archived.
    ///
    /// A column of another board is a `Validation` rather than a `NotFound` —
    /// it is a real column of the tenant, and naming the rule ("that stage is
    /// not on this deal's pipeline") is the answer a user can act on.
    pub(crate) async fn resolve_target_stage(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        pipeline: &CrmPipelineId,
        stage: &CrmStageId,
    ) -> Result<TargetStage> {
        let row = sqlx::query_as::<_, (String, String, bool, bool, Option<OffsetDateTime>)>(
            "SELECT id, pipeline_id, is_won, is_lost, archived_at FROM crm_stages \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(stage.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)?;
        let (id, on_pipeline, is_won, is_lost, archived_at) = row;
        if on_pipeline != pipeline.as_str() {
            return Err(StoreError::Validation(
                "that stage belongs to another pipeline".to_owned(),
            ));
        }
        if archived_at.is_some() {
            return Err(StoreError::Validation(
                "that stage is archived; restore it before moving deals into it".to_owned(),
            ));
        }
        Ok(TargetStage {
            id,
            is_won,
            is_lost,
        })
    }

    /// Appends one history row inside the transaction that moved the deal.
    async fn append_stage_event(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        deal: &CrmDealId,
        from_stage: Option<&str>,
        to_stage: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO crm_deal_stage_events \
             (tenant_id, id, deal_id, from_stage_id, to_stage_id, moved_by) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(self.tenant.as_str())
        .bind(CrmEventId::generate().as_str())
        .bind(deal.as_str())
        .bind(from_stage)
        .bind(to_stage)
        .bind(self.user.as_str())
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }
}

/// The end of a column: one past its last card, so an append never collides
/// with the card that is already there.
async fn next_position(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    stage: &str,
) -> Result<f64> {
    sqlx::query_scalar::<_, f64>(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM crm_deals \
         WHERE tenant_id = $1 AND stage_id = $2",
    )
    .bind(tenant)
    .bind(stage)
    .fetch_one(&mut **tx)
    .await
    .map_err(StoreError::Db)
}

/// How many **open** deals stand in one column — the count that decides
/// whether it may be archived ([`AccountStore::set_crm_stage_archived`]).
/// Closed deals do not block: archiving a column is "no new work lands here",
/// not "this never happened".
pub(crate) async fn open_deals_in_stage(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    stage: &str,
) -> Result<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM crm_deals \
         WHERE tenant_id = $1 AND stage_id = $2 AND outcome IS NULL",
    )
    .bind(tenant)
    .bind(stage)
    .fetch_one(&mut **tx)
    .await
    .map_err(StoreError::Db)
}

/// How many **open** deals stand anywhere on one board — the count that
/// decides whether it may be archived.
pub(crate) async fn open_deals_in_pipeline(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    pipeline: &str,
) -> Result<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM crm_deals \
         WHERE tenant_id = $1 AND pipeline_id = $2 AND outcome IS NULL",
    )
    .bind(tenant)
    .bind(pipeline)
    .fetch_one(&mut **tx)
    .await
    .map_err(StoreError::Db)
}

/// Whether any deal stands in a column, or any history row has ever named it —
/// the question [`AccountStore::delete_crm_stage`] asks before deleting one.
/// The database refuses it too (both foreign keys are `RESTRICT`); this is what
/// turns that refusal into a sentence a user can act on.
pub(crate) async fn stage_is_spoken_for(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    stage: &str,
) -> Result<bool> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM crm_deals WHERE tenant_id = $1 AND stage_id = $2) \
             OR EXISTS(SELECT 1 FROM crm_deal_stage_events \
                       WHERE tenant_id = $1 AND (to_stage_id = $2 OR from_stage_id = $2))",
    )
    .bind(tenant)
    .bind(stage)
    .fetch_one(&mut **tx)
    .await
    .map_err(StoreError::Db)
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct DealRow {
    id: String,
    pipeline_id: String,
    stage_id: String,
    title: String,
    customer_id: Option<String>,
    contact_id: Option<String>,
    company_name: String,
    contact_name: String,
    contact_email: String,
    value_cents: i64,
    currency: String,
    expected_close: Option<Date>,
    owner_user_id: String,
    source: String,
    position: f64,
    outcome: Option<String>,
    lost_reason: Option<String>,
    closed_at: Option<OffsetDateTime>,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl DealRow {
    fn into_deal(self) -> Result<Deal> {
        Ok(Deal {
            id: CrmDealId::new(self.id),
            pipeline_id: CrmPipelineId::new(self.pipeline_id),
            stage_id: CrmStageId::new(self.stage_id),
            title: self.title,
            customer_id: self.customer_id.map(BillingCustomerId::new),
            contact_id: self.contact_id.map(ContactId::new),
            company_name: self.company_name,
            contact_name: self.contact_name,
            contact_email: self.contact_email,
            value_cents: self.value_cents,
            currency: self.currency,
            expected_close: self.expected_close,
            owner_user_id: self.owner_user_id,
            source: self.source,
            position: self.position,
            outcome: parse_outcome(self.outcome.as_deref())?,
            lost_reason: self.lost_reason,
            closed_at: self.closed_at,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct EventRow {
    id: String,
    deal_id: String,
    from_stage_id: Option<String>,
    to_stage_id: String,
    moved_by: String,
    moved_at: OffsetDateTime,
}

impl EventRow {
    fn into_event(self) -> StageEvent {
        StageEvent {
            id: CrmEventId::new(self.id),
            deal_id: CrmDealId::new(self.deal_id),
            from_stage_id: self.from_stage_id.map(CrmStageId::new),
            to_stage_id: CrmStageId::new(self.to_stage_id),
            moved_by: self.moved_by,
            moved_at: self.moved_at,
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
    fn a_deal_value_is_non_negative_and_capped() {
        // Zero is legitimate: an opportunity often exists before anybody has
        // priced it.
        for ok in [0, 1, 250_000, DEAL_VALUE_MAX_CENTS] {
            assert_eq!(value_cents(ok).unwrap_or(-1), ok);
        }
        for bad in [-1, DEAL_VALUE_MAX_CENTS + 1, i64::MIN, i64::MAX] {
            assert!(
                matches!(value_cents(bad), Err(StoreError::Validation(_))),
                "expected rejection: {bad}"
            );
        }
        assert!(invalid(value_cents(-1)).contains("deal value"));
    }

    #[test]
    fn the_value_cap_keeps_a_pipeline_report_inside_i64() {
        // The report sums a board. Ten thousand deals at the ceiling must
        // still be an i64 — otherwise a forecast could silently wrap.
        let deals: i64 = 10_000;
        assert!(deals.checked_mul(DEAL_VALUE_MAX_CENTS).is_some());
    }

    #[test]
    fn a_losing_column_demands_a_reason_and_every_other_refuses_one() {
        assert_eq!(
            lost_reason(Some("  Price  "), true).unwrap_or_default(),
            Some("Price".to_owned())
        );
        assert!(invalid(lost_reason(None, true)).contains("reason"));
        // Blank is "no reason", not a reason: a reason nobody enters is the
        // failure mode the rule exists to prevent.
        assert!(invalid(lost_reason(Some("   "), true)).contains("reason"));
        assert!(
            invalid(lost_reason(
                Some("x".repeat(DEAL_PARTY_MAX_CHARS + 1).as_str()),
                true
            ))
            .contains("at most")
        );
        assert_eq!(lost_reason(None, false).unwrap_or_default(), None);
        assert_eq!(lost_reason(Some("  "), false).unwrap_or_default(), None);
        assert!(invalid(lost_reason(Some("Price"), false)).contains("losing stage"));
    }

    #[test]
    fn a_position_must_be_a_real_place_in_the_column() {
        for ok in [0.0, 1.0, 1.5, -2.0, f64::MAX, f64::MIN] {
            assert!(check_position(ok).is_ok(), "expected valid: {ok}");
        }
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(invalid(check_position(bad)).contains("finite"));
        }
    }

    #[test]
    fn a_state_survives_the_round_trip_and_open_is_not_stored() {
        for state in [DealState::Open, DealState::Won, DealState::Lost] {
            assert_eq!(DealState::parse(state.as_str()), Some(state));
        }
        assert_eq!(DealState::parse("Won"), None, "the filter is exact");
        assert_eq!(DealState::parse(""), None);
        assert!(!DealState::Open.is_closed());
        assert!(DealState::Won.is_closed() && DealState::Lost.is_closed());
    }

    #[test]
    fn a_stored_outcome_is_won_lost_or_absent_and_never_guessed() {
        assert_eq!(parse_outcome(None).unwrap_or(Some(DealState::Won)), None);
        assert_eq!(
            parse_outcome(Some("won")).unwrap_or_default(),
            Some(DealState::Won)
        );
        assert_eq!(
            parse_outcome(Some("lost")).unwrap_or_default(),
            Some(DealState::Lost)
        );
        // 'open' is the absence of an outcome; a row that stores it is corrupt,
        // and so is anything else. Neither is guessed at.
        for corrupt in ["open", "OPEN", "abandoned", ""] {
            assert!(
                matches!(parse_outcome(Some(corrupt)), Err(StoreError::Db(_))),
                "expected a decode failure for {corrupt:?}"
            );
        }
    }

    #[test]
    fn the_state_reads_off_the_snapshot() {
        let deal = |outcome| Deal {
            id: CrmDealId::new("d"),
            pipeline_id: CrmPipelineId::new("p"),
            stage_id: CrmStageId::new("s"),
            title: "Renewal".to_owned(),
            customer_id: None,
            contact_id: None,
            company_name: String::new(),
            contact_name: String::new(),
            contact_email: String::new(),
            value_cents: 0,
            currency: "EUR".to_owned(),
            expected_close: None,
            owner_user_id: "u".to_owned(),
            source: String::new(),
            position: 1.0,
            lost_reason: None,
            closed_at: None,
            created_by: "u".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            outcome,
        };
        assert_eq!(deal(None).state(), DealState::Open);
        assert!(!deal(None).is_closed());
        assert_eq!(deal(Some(DealState::Won)).state(), DealState::Won);
        assert!(deal(Some(DealState::Lost)).is_closed());
    }

    #[test]
    fn a_default_deal_is_the_blank_a_ui_opens_with() {
        let d = NewDeal::default();
        assert_eq!(d.currency, "EUR");
        assert_eq!(d.value_cents, 0);
        assert!(d.customer_id.is_none() && d.contact_id.is_none());
        assert!(d.owner_user_id.is_none(), "the acting user owns it");
        assert!(d.expected_close.is_none());
    }

    #[test]
    fn a_plain_move_appends_to_the_target_column() {
        let mv = StageMove::to(CrmStageId::new("s2"));
        assert_eq!(mv.stage_id.as_str(), "s2");
        assert!(mv.position.is_none() && mv.lost_reason.is_none());
    }
}
