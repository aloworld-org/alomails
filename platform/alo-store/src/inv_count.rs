//! The stocktake: opening a count, the sheet it snapshots, and what a person
//! actually found (alo Inventory, ADR 0035, wave B5.08a;
//! `docs/design/inventory.md`, "Stocktake").
//!
//! A count is a **worksheet**, not a record of stock. Opening one for a location
//! writes down what the ledger says is there at that moment — one line per
//! stocked product with something on that shelf — and a person then works down
//! the sheet putting a found quantity against each line. Nothing about the
//! ledger changes here: applying the count (B5.08b) writes ordinary adjustment
//! movements, so "where did the other four go" keeps the answer it has had since
//! B5.04a.
//!
//! Four decisions this module makes, all of them refusals to guess:
//!
//! - **The snapshot is a reading, not an authority.** `expected_qty_milli` is
//!   what the ledger said when the sheet was opened. It is kept so the sheet is
//!   printable and so the counter can see what they were meant to find — but it
//!   is **not** what the variance is computed against when the count is applied.
//!   A warehouse does not stop while it is counted, so [`CountLine`] also states
//!   what is on the shelf *now* and flags the lines where the two differ
//!   ([`CountLine::moved_since`]): those are the ones an apply will skip, and
//!   the person re-counts a few items rather than losing a shipment.
//! - **An uncounted line is uncounted, not zero.** [`CountLine::counted_qty_milli`]
//!   is `None` until somebody looks, because "nobody got to this shelf" and
//!   "there are none left" are opposite facts and confusing them writes off
//!   everything nobody reached. Recording a count is undoable — sending no
//!   quantity clears the line back to uncounted, which is undo rather than
//!   confirm (`docs/design/ux-principles.md`).
//! - **One open count per location.** Two people counting one shelf at the same
//!   time produce two truths, and applying both would adjust the same variance
//!   twice. A second open count for a place is a [`StoreError::Conflict`].
//! - **A line may be added by scanning something the sheet did not expect.**
//!   Its `expected` is the on-hand at the moment it was added — zero for a
//!   product that has never been on that shelf — which is exactly the surplus
//!   case a stocktake exists to catch.
//!
//! Quantities are milli-units, as everywhere else in the suite. No float touches
//! this module, and no money is quoted by it at all: what a variance is *worth*
//! is a question for the movements it becomes.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_line::QTY_MAX_MILLI;
use crate::error::{Result, StoreError};
use crate::id::{BillingProductId, InvCountId, InvLocationId};
use crate::inv_locations::LocationKind;
use crate::inv_moves::MOVE_NOTE_MAX_CHARS;

/// The most counts one read returns. A stocktake list is a history screen, and
/// a tenant who counts a shelf every week for four years has more rows than
/// anybody wants in one response.
pub const COUNTS_PAGE_MAX: i64 = 200;

/// Where a stocktake has got to.
///
/// Both closed states are terminal. That is what stops one afternoon's variance
/// being written into the ledger twice: a count that has been applied cannot be
/// applied again, and one that was walked away from is kept as the sheet it was
/// rather than reopened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountStatus {
    /// Being counted. The only state in which lines may be written.
    Open,
    /// The variances became adjustment movements (B5.08b).
    Applied,
    /// Walked away from: the sheet is kept, the ledger untouched.
    Cancelled,
}

/// Every state, in lifecycle order — the vocabulary a refusal lists.
pub const COUNT_STATUSES: [CountStatus; 3] = [
    CountStatus::Open,
    CountStatus::Applied,
    CountStatus::Cancelled,
];

impl CountStatus {
    /// The stored word — the database value and the wire form, one spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Applied => "applied",
            Self::Cancelled => "cancelled",
        }
    }

    /// Reads the stored word back.
    ///
    /// # Errors
    /// [`StoreError::Validation`] listing every state — the message a caller
    /// needs to fix a status filter without reading our source.
    pub fn parse(value: &str) -> Result<Self> {
        COUNT_STATUSES
            .into_iter()
            .find(|status| status.as_str() == value.trim())
            .ok_or_else(|| {
                StoreError::Validation(format!(
                    "status must be {}",
                    COUNT_STATUSES
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })
    }

    /// Whether the count is still being worked on — and so whether its lines,
    /// its note and its fate can still be changed.
    #[must_use]
    pub fn is_open(self) -> bool {
        matches!(self, Self::Open)
    }
}

/// What opening a stocktake states: which place, and why.
#[derive(Debug, Clone)]
pub struct NewCount {
    /// The place to count. One of this tenant's, and a real `stock` location: a
    /// count of the `supplier` counterparty would be a count of a number that
    /// is negative by construction, and `transit` holds goods that are by
    /// definition not anywhere to be counted.
    pub location_id: InvLocationId,
    /// What the person wrote about it ("Tuesday, back shelves"). Bounded, and
    /// never logged.
    pub note: String,
}

/// One stocktake, with enough of its shape to be listed on a screen.
#[derive(Debug, Clone)]
pub struct Count {
    /// Opaque id, unique within the tenant.
    pub id: InvCountId,
    /// The place counted.
    pub location_id: InvLocationId,
    /// That location's code.
    pub location_code: String,
    /// That location's name.
    pub location_name: String,
    /// Where the count has got to.
    pub status: CountStatus,
    /// What the person wrote about it.
    pub note: String,
    /// How many lines the sheet has.
    pub line_count: i64,
    /// How many of them somebody has actually counted.
    pub counted_count: i64,
    /// How many counted lines disagree with their snapshot — the headline
    /// number of the sheet, and a *provisional* one: what an apply will act on
    /// is recomputed against on-hand at that moment (B5.08b).
    pub variance_count: i64,
    /// The user who opened the count.
    pub created_by: String,
    /// When it was opened.
    pub created_at: OffsetDateTime,
    /// When it was last touched.
    pub updated_at: OffsetDateTime,
    /// When it stopped being open; `None` while open.
    pub closed_at: Option<OffsetDateTime>,
    /// Who closed it; `None` while open.
    pub closed_by: Option<String>,
}

/// One row of the count sheet: an item, what was expected, what was found, and
/// whether the shelf moved underneath the counter.
#[derive(Debug, Clone)]
pub struct CountLine {
    /// The product on the row.
    pub product_id: BillingProductId,
    /// Its name in the catalog today.
    pub product_name: String,
    /// Its SKU; empty when the tenant has not given it one.
    pub sku: String,
    /// Its barcode; empty for the plenty of stock that genuinely has none. Here
    /// because the sheet is worked with a scanner (B5.09c).
    pub barcode: String,
    /// Its unit label; empty for a unitless item.
    pub unit: String,
    /// What the ledger said was here when the line joined the sheet, in
    /// milli-units. A reading, not an authority.
    pub expected_qty_milli: i64,
    /// What was found, in milli-units. `None` until somebody looks.
    pub counted_qty_milli: Option<i64>,
    /// `counted − expected`, in milli-units; `None` while uncounted. Positive is
    /// a surplus, negative a loss.
    pub variance_qty_milli: Option<i64>,
    /// What is on that shelf **now**, in milli-units — read at the moment of
    /// this call, not at the snapshot.
    pub on_hand_qty_milli: i64,
    /// Whether stock moved since the line joined the sheet. An apply skips these
    /// rows rather than writing a difference that would erase the movement.
    pub moved_since: bool,
    /// What the counter wrote about this row.
    pub note: String,
    /// When it was counted; `None` while uncounted.
    pub counted_at: Option<OffsetDateTime>,
    /// Who counted it; `None` while uncounted.
    pub counted_by: Option<String>,
}

/// What a counter records against one row.
#[derive(Debug, Clone, Default)]
pub struct CountEntry {
    /// What was found, in milli-units. `None` clears the row back to uncounted —
    /// the undo of a mis-scan, and not the same as counting zero.
    pub counted_qty_milli: Option<i64>,
    /// What the counter wrote about it. Bounded, and never logged.
    pub note: String,
}

/// Which stocktakes to read.
#[derive(Debug, Clone, Default)]
pub struct CountFilter {
    /// One place, across every time it has been counted.
    pub location_id: Option<InvLocationId>,
    /// One state — `open` is the "what is being counted right now" question.
    pub status: Option<CountStatus>,
    /// How many at most, newest first. Clamped to [`COUNTS_PAGE_MAX`]; `None` is
    /// the cap.
    pub limit: Option<i64>,
}

/// What a counted row says happened to the stock: found minus expected.
///
/// `None` while the row is uncounted, because no claim has been made about it.
/// Saturating, so a pair of extreme numbers cannot overflow into a variance of
/// the wrong sign.
#[must_use]
pub fn variance_qty_milli(counted_qty_milli: Option<i64>, expected_qty_milli: i64) -> Option<i64> {
    counted_qty_milli.map(|counted| counted.saturating_sub(expected_qty_milli))
}

/// Validates a found quantity. Pure — no database, so the rule is unit-tested
/// directly.
///
/// Zero is legitimate and important: "I looked, and there are none" is the
/// finding a stocktake most needs to be able to state. Negative is not: a shelf
/// cannot hold minus four of anything, and a count is a statement about a shelf.
fn check_counted(counted_qty_milli: Option<i64>) -> Result<()> {
    let Some(counted) = counted_qty_milli else {
        return Ok(());
    };
    if !(0..=QTY_MAX_MILLI).contains(&counted) {
        return Err(StoreError::Validation(format!(
            "a counted quantity must be between 0 and {QTY_MAX_MILLI} milli-units"
        )));
    }
    Ok(())
}

/// Validates a note the same way every other free-text field in the module is.
fn check_note(note: &str) -> Result<String> {
    if note.chars().count() > MOVE_NOTE_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "note must be at most {MOVE_NOTE_MAX_CHARS} characters"
        )));
    }
    Ok(note.trim().to_owned())
}

/// Turns the one unique violation this module can produce into the refusal a
/// caller could have predicted, rather than a `500`.
fn map_open_conflict(error: sqlx::Error) -> StoreError {
    match error {
        sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23505") => {
            StoreError::Conflict(
                "this location already has a count open; finish or cancel it first".to_owned(),
            )
        }
        other => StoreError::Db(other),
    }
}

/// The columns every read of a count selects, in `CountRow` order.
const COUNT_COLS: &str = "c.id, c.location_id, l.code AS location_code, l.name AS location_name, \
     c.status, c.note, t.line_count, t.counted_count, t.variance_count, \
     c.created_by, c.created_at, c.updated_at, c.closed_at, c.closed_by";

/// The joins behind [`COUNT_COLS`]: the location a count is about, and the
/// three tallies of its sheet, in one statement rather than one read per row.
const COUNT_FROM: &str = "FROM inv_counts c \
     JOIN inv_locations l ON l.tenant_id = c.tenant_id AND l.id = c.location_id \
     LEFT JOIN LATERAL ( \
         SELECT count(*) AS line_count, \
             count(cl.counted_qty_milli) AS counted_count, \
             count(*) FILTER ( \
                 WHERE cl.counted_qty_milli IS NOT NULL \
                   AND cl.counted_qty_milli <> cl.expected_qty_milli) AS variance_count \
         FROM inv_count_lines cl \
         WHERE cl.tenant_id = c.tenant_id AND cl.count_id = c.id) t ON TRUE";

/// The columns every read of a sheet line selects, in `LineRow` order. The
/// on-hand comes from the cached balance for the count's own location, read at
/// the moment of the call — that is what makes [`CountLine::moved_since`] a fact
/// rather than a memory.
const LINE_COLS: &str = "cl.product_id, p.name AS product_name, p.sku, p.barcode, p.unit, \
     cl.expected_qty_milli, cl.counted_qty_milli, COALESCE(st.qty_milli, 0) AS on_hand_qty_milli, \
     cl.note, cl.counted_at, cl.counted_by";

/// The joins behind [`LINE_COLS`].
const LINE_FROM: &str = "FROM inv_count_lines cl \
     JOIN inv_counts c ON c.tenant_id = cl.tenant_id AND c.id = cl.count_id \
     JOIN billing_products p ON p.tenant_id = cl.tenant_id AND p.id = cl.product_id \
     LEFT JOIN inv_stock st ON st.tenant_id = cl.tenant_id \
         AND st.product_id = cl.product_id AND st.location_id = c.location_id";

impl AccountStore {
    /// Opens a stocktake for one location and snapshots what the ledger says is
    /// on it.
    ///
    /// The sheet starts with one line per **stocked product with something on
    /// that shelf**. A product with nothing there is deliberately absent: the
    /// sheet would otherwise be the whole catalog, and the only finding such a
    /// row could carry — a surplus — is exactly the case
    /// [`AccountStore::set_inv_count_line`] adds a line for. Archived products
    /// are included when they have stock: what is on the shelf is on the shelf,
    /// whatever the catalog thinks of it.
    ///
    /// The snapshot and the count row are written in **one transaction**: a
    /// tenant never holds a count whose sheet is half a shelf.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the location is not this tenant's;
    /// [`StoreError::Validation`] on a location that is not a real shelf or an
    /// over-long note; [`StoreError::Conflict`] when the location is archived or
    /// already has an open count; [`StoreError::Db`] on failure.
    pub async fn open_inv_count(&self, input: &NewCount) -> Result<InvCountId> {
        let note = check_note(&input.note)?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let location = self
            .require_tenant_location(&mut tx, &input.location_id)
            .await?;
        if location.kind != LocationKind::Stock {
            return Err(StoreError::Validation(
                "a stocktake counts a real stock location".to_owned(),
            ));
        }
        if location.is_archived() {
            return Err(StoreError::Conflict(format!(
                "{} is archived and cannot be counted",
                location.code
            )));
        }
        let id = InvCountId::generate();
        sqlx::query(
            "INSERT INTO inv_counts (tenant_id, id, location_id, note, created_by) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(location.id.as_str())
        .bind(&note)
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(map_open_conflict)?;
        sqlx::query(
            "INSERT INTO inv_count_lines (tenant_id, count_id, product_id, expected_qty_milli) \
             SELECT $1, $2, s.product_id, s.qty_milli FROM inv_stock s \
             JOIN billing_products p ON p.tenant_id = s.tenant_id AND p.id = s.product_id \
             WHERE s.tenant_id = $1 AND s.location_id = $3 AND s.qty_milli > 0 AND p.stocked",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(location.id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// One stocktake by id, or `None` when it is not this tenant's — never a
    /// refusal that would confirm another tenant's row exists.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Validation`] if a stored
    /// status is not one this version knows.
    pub async fn inv_count(&self, id: &InvCountId) -> Result<Option<Count>> {
        let row = sqlx::query_as::<_, CountRow>(&format!(
            "SELECT {COUNT_COLS} {COUNT_FROM} WHERE c.tenant_id = $1 AND c.id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(CountRow::into_count).transpose()
    }

    /// The tenant's stocktakes, newest first.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure. An unknown or foreign location id narrows
    /// to an empty list, never a refusal that would confirm it exists.
    pub async fn inv_counts(&self, filter: &CountFilter) -> Result<Vec<Count>> {
        let limit = filter
            .limit
            .unwrap_or(COUNTS_PAGE_MAX)
            .clamp(0, COUNTS_PAGE_MAX);
        let rows = sqlx::query_as::<_, CountRow>(&format!(
            "SELECT {COUNT_COLS} {COUNT_FROM} WHERE c.tenant_id = $1 \
               AND ($2::text IS NULL OR c.location_id = $2) \
               AND ($3::text IS NULL OR c.status = $3) \
             ORDER BY c.created_at DESC, c.id LIMIT $4"
        ))
        .bind(self.tenant.as_str())
        .bind(filter.location_id.as_ref().map(InvLocationId::as_str))
        .bind(filter.status.map(CountStatus::as_str))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(CountRow::into_count).collect()
    }

    /// The sheet: every line of one count, in product-name order, each with what
    /// is on the shelf now beside what was expected.
    ///
    /// An id that is not this tenant's reads as an empty sheet, for the reason
    /// every other list does: existence is never disclosed. A caller that needs
    /// to tell "no such count" from "a count with no lines" reads the count
    /// itself first, which is what the HTTP layer does.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_count_sheet(&self, id: &InvCountId) -> Result<Vec<CountLine>> {
        let rows = sqlx::query_as::<_, LineRow>(&format!(
            "SELECT {LINE_COLS} {LINE_FROM} \
             WHERE cl.tenant_id = $1 AND cl.count_id = $2 \
             ORDER BY lower(p.name), p.id"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(LineRow::into_line).collect())
    }

    /// Records what was found against one product — or clears the row back to
    /// uncounted when `entry.counted_qty_milli` is `None`.
    ///
    /// Idempotent by (count, product), so a scanner that fires twice records one
    /// row rather than two, and a re-count overwrites rather than accumulates.
    /// A product the sheet did not expect **joins** it here, with its `expected`
    /// read from the shelf at this moment — the surplus case a stocktake exists
    /// to catch.
    ///
    /// The snapshot of a line that is already on the sheet is never rewritten:
    /// what was expected was expected, and moving it would quietly erase the
    /// variance the count is about.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the count or the product is not this
    /// tenant's; [`StoreError::Validation`] on a negative or over-bound
    /// quantity, an over-long note, or a service product;
    /// [`StoreError::Conflict`] when the count is no longer open;
    /// [`StoreError::Db`] on failure.
    pub async fn set_inv_count_line(
        &self,
        count_id: &InvCountId,
        product_id: &BillingProductId,
        entry: &CountEntry,
    ) -> Result<CountLine> {
        check_counted(entry.counted_qty_milli)?;
        let note = check_note(&entry.note)?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Locked for the length of the write: the status decides whether the
        // line may be written at all, and a cancel racing a scanner must not be
        // able to land between the check and the insert.
        let held: Option<(String, String)> = sqlx::query_as(
            "SELECT status, location_id FROM inv_counts \
             WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(count_id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let (status, location_id) = held.ok_or(StoreError::NotFound)?;
        let status = CountStatus::parse(&status)?;
        if !status.is_open() {
            return Err(StoreError::Conflict(format!(
                "this count is {} and can no longer be counted",
                status.as_str()
            )));
        }
        let product: Option<(String, bool)> = sqlx::query_as(
            "SELECT name, stocked FROM billing_products WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(product_id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let (product_name, stocked) = product.ok_or(StoreError::NotFound)?;
        if !stocked {
            return Err(StoreError::Validation(format!(
                "{product_name} is not a stocked product, so it cannot be counted"
            )));
        }
        // What the shelf holds right now — the `expected` of a line joining the
        // sheet here, and ignored for one that is already on it.
        let on_hand: Option<i64> = sqlx::query_scalar(
            "SELECT qty_milli FROM inv_stock \
             WHERE tenant_id = $1 AND product_id = $2 AND location_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(product_id.as_str())
        .bind(&location_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let counted_at = entry.counted_qty_milli.map(|_| OffsetDateTime::now_utc());
        let counted_by = entry
            .counted_qty_milli
            .map(|_| self.user.as_str().to_owned());
        sqlx::query(
            "INSERT INTO inv_count_lines (tenant_id, count_id, product_id, expected_qty_milli, \
                 counted_qty_milli, counted_at, counted_by, note) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
             ON CONFLICT (tenant_id, count_id, product_id) DO UPDATE \
                 SET counted_qty_milli = EXCLUDED.counted_qty_milli, \
                     counted_at = EXCLUDED.counted_at, counted_by = EXCLUDED.counted_by, \
                     note = EXCLUDED.note",
        )
        .bind(self.tenant.as_str())
        .bind(count_id.as_str())
        .bind(product_id.as_str())
        .bind(on_hand.unwrap_or(0).max(0))
        .bind(entry.counted_qty_milli)
        .bind(counted_at)
        .bind(counted_by)
        .bind(&note)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        self.touch_inv_count(&mut tx, count_id).await?;
        let line = sqlx::query_as::<_, LineRow>(&format!(
            "SELECT {LINE_COLS} {LINE_FROM} \
             WHERE cl.tenant_id = $1 AND cl.count_id = $2 AND cl.product_id = $3"
        ))
        .bind(self.tenant.as_str())
        .bind(count_id.as_str())
        .bind(product_id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(line.into_line())
    }

    /// Changes what the person wrote about a count. Only while it is open: a
    /// closed sheet is a record of what happened, and records are not edited.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the count is not this tenant's;
    /// [`StoreError::Validation`] on an over-long note;
    /// [`StoreError::Conflict`] when the count is closed; [`StoreError::Db`] on
    /// failure.
    pub async fn update_inv_count_note(&self, id: &InvCountId, note: &str) -> Result<()> {
        let note = check_note(note)?;
        let done = sqlx::query(
            "UPDATE inv_counts SET note = $3, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND status = 'open'",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&note)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(self.why_not_open(id).await);
        }
        Ok(())
    }

    /// Walks away from a count: the sheet is kept exactly as it was, and the
    /// ledger is untouched.
    ///
    /// Terminal — a cancelled count is not reopened, because the shelf has moved
    /// on since and the honest thing to do is count it again. Cancelling frees
    /// the location for a new count.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the count is not this tenant's;
    /// [`StoreError::Conflict`] when it is already closed; [`StoreError::Db`] on
    /// failure.
    pub async fn cancel_inv_count(&self, id: &InvCountId) -> Result<()> {
        let done = sqlx::query(
            "UPDATE inv_counts SET status = 'cancelled', closed_at = now(), closed_by = $3, \
                 updated_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND status = 'open'",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(self.why_not_open(id).await);
        }
        Ok(())
    }

    /// Marks a count touched, so a list ordered by activity is honest about a
    /// sheet somebody is working down right now.
    async fn touch_inv_count(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &InvCountId,
    ) -> Result<()> {
        sqlx::query("UPDATE inv_counts SET updated_at = now() WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&mut **tx)
            .await
            .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Why a write that required an open count changed nothing: because there is
    /// no such count for this tenant, or because it is closed. Called only on
    /// the failing path, so the happy path stays one statement.
    async fn why_not_open(&self, id: &InvCountId) -> StoreError {
        match self.inv_count(id).await {
            Ok(Some(count)) => StoreError::Conflict(format!(
                "this count is {} and can no longer be changed",
                count.status.as_str()
            )),
            Ok(None) => StoreError::NotFound,
            Err(error) => error,
        }
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct CountRow {
    id: String,
    location_id: String,
    location_code: String,
    location_name: String,
    status: String,
    note: String,
    line_count: i64,
    counted_count: i64,
    variance_count: i64,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    closed_at: Option<OffsetDateTime>,
    closed_by: Option<String>,
}

impl CountRow {
    fn into_count(self) -> Result<Count> {
        Ok(Count {
            id: InvCountId::new(self.id),
            location_id: InvLocationId::new(self.location_id),
            location_code: self.location_code,
            location_name: self.location_name,
            status: CountStatus::parse(&self.status)?,
            note: self.note,
            line_count: self.line_count,
            counted_count: self.counted_count,
            variance_count: self.variance_count,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
            closed_at: self.closed_at,
            closed_by: self.closed_by,
        })
    }
}

#[derive(sqlx::FromRow)]
struct LineRow {
    product_id: String,
    product_name: String,
    sku: String,
    barcode: String,
    unit: String,
    expected_qty_milli: i64,
    counted_qty_milli: Option<i64>,
    on_hand_qty_milli: i64,
    note: String,
    counted_at: Option<OffsetDateTime>,
    counted_by: Option<String>,
}

impl LineRow {
    fn into_line(self) -> CountLine {
        CountLine {
            product_id: BillingProductId::new(self.product_id),
            product_name: self.product_name,
            sku: self.sku,
            barcode: self.barcode,
            unit: self.unit,
            expected_qty_milli: self.expected_qty_milli,
            counted_qty_milli: self.counted_qty_milli,
            variance_qty_milli: variance_qty_milli(self.counted_qty_milli, self.expected_qty_milli),
            on_hand_qty_milli: self.on_hand_qty_milli,
            moved_since: self.on_hand_qty_milli != self.expected_qty_milli,
            note: self.note,
            counted_at: self.counted_at,
            counted_by: self.counted_by,
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
    fn a_variance_is_what_was_found_minus_what_was_expected() {
        // Four expected, six found: a surplus of two.
        assert_eq!(variance_qty_milli(Some(6_000), 4_000), Some(2_000));
        // Four expected, one found: three are missing.
        assert_eq!(variance_qty_milli(Some(1_000), 4_000), Some(-3_000));
        // Agreement is a variance of nothing, which is not the same as no
        // variance at all — the row was counted, and says so.
        assert_eq!(variance_qty_milli(Some(4_000), 4_000), Some(0));
    }

    #[test]
    fn an_uncounted_row_has_no_variance_at_all() {
        // "Nobody got to this shelf" is not "there are none left": an uncounted
        // row makes no claim, and an apply skips it.
        assert_eq!(variance_qty_milli(None, 4_000), None);
        assert_eq!(variance_qty_milli(None, 0), None);
        // Counting zero, by contrast, is the strongest claim a stocktake makes.
        assert_eq!(variance_qty_milli(Some(0), 4_000), Some(-4_000));
    }

    #[test]
    fn a_variance_saturates_rather_than_overflowing() {
        assert_eq!(variance_qty_milli(Some(i64::MAX), -1), Some(i64::MAX));
        assert_eq!(variance_qty_milli(Some(i64::MIN), 1), Some(i64::MIN));
    }

    #[test]
    fn a_counted_quantity_is_bounded_and_never_negative() {
        assert!(check_counted(None).is_ok());
        assert!(
            check_counted(Some(0)).is_ok(),
            "'there are none' is a finding"
        );
        assert!(check_counted(Some(QTY_MAX_MILLI)).is_ok());
        for bad in [-1, -4_000, QTY_MAX_MILLI + 1, i64::MAX, i64::MIN] {
            assert!(
                invalid(check_counted(Some(bad))).contains("counted quantity"),
                "a shelf cannot hold {bad} milli-units"
            );
        }
    }

    #[test]
    fn a_note_is_bounded_and_trimmed() {
        assert_eq!(
            check_note("  two boxes water-damaged  ").ok(),
            Some("two boxes water-damaged".to_owned())
        );
        assert_eq!(check_note("").ok(), Some(String::new()));
        assert!(check_note(&"x".repeat(MOVE_NOTE_MAX_CHARS)).is_ok());
        assert!(
            invalid(check_note(&"x".repeat(MOVE_NOTE_MAX_CHARS + 1))).contains("at most"),
            "the bound is the movement's, because this note becomes one"
        );
        // Counted in characters, not bytes: a European product cannot have a
        // shorter note in Greek.
        assert!(check_note(&"é".repeat(MOVE_NOTE_MAX_CHARS)).is_ok());
    }

    #[test]
    fn the_status_vocabulary_round_trips_and_refuses_anything_else() {
        for status in COUNT_STATUSES {
            assert_eq!(CountStatus::parse(status.as_str()).ok(), Some(status));
        }
        assert_eq!(CountStatus::parse(" open ").ok(), Some(CountStatus::Open));
        for bad in ["", "OPEN", "closed", "draft", "applying"] {
            let message = invalid(CountStatus::parse(bad));
            for status in COUNT_STATUSES {
                assert!(
                    message.contains(status.as_str()),
                    "{message} omits {status:?}"
                );
            }
        }
    }

    #[test]
    fn only_an_open_count_is_still_being_worked_on() {
        assert!(CountStatus::Open.is_open());
        assert!(!CountStatus::Applied.is_open());
        assert!(
            !CountStatus::Cancelled.is_open(),
            "a cancelled count is not reopened: the shelf has moved on since"
        );
    }

    #[test]
    fn the_default_filter_is_the_whole_history_of_stocktakes() {
        let d = CountFilter::default();
        assert!(d.location_id.is_none() && d.status.is_none());
        assert!(
            d.limit.is_none(),
            "and the page cap is the store's, not the caller's"
        );
    }
}
