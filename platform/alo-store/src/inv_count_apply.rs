//! **Applying** a stocktake: the variances become movements (alo Inventory,
//! ADR 0035, wave B5.08b; `docs/design/inventory.md`, "Stocktake").
//!
//! Counting ([`crate::inv_count`]) writes a worksheet and nothing else. This is
//! where the worksheet becomes a fact about the ledger, and it does it the only
//! way anything in this module ever changes stock: by writing **movements**
//! ([`crate::inv_moves`]) — out of the counted shelf into the tenant's virtual
//! `adjustment` location for a loss, the other way for a surplus, reason
//! [`MoveReason::Count`], each referencing the count that produced it. There is
//! no quantity column to overwrite, so "where did the other four go" keeps the
//! answer it has had since B5.04a.
//!
//! Everything, or nothing: the movements and the count's closing are one
//! transaction. A tenant is never left holding half a corrected shelf, and a
//! count that failed halfway is still open, still countable, and its sheet still
//! says what somebody found.
//!
//! Three decisions this module makes, and the reason each one is a refusal to
//! guess:
//!
//! - **The variance is recomputed against on-hand *now*, never taken from the
//!   snapshot.** A warehouse does not stop while it is counted. If a delivery
//!   went out at the far end of the room between the sheet being opened and it
//!   being applied, then writing the frozen difference would silently erase that
//!   shipment. So a row whose shelf has moved underneath it is **skipped and
//!   reported** ([`SkipReason::Moved`]) rather than applied — the person
//!   re-counts a few items instead of losing a delivery. The rejected
//!   alternative, locking a location for the duration of a count, is correct in
//!   a system where counting takes ten minutes and unusable in a shop that
//!   counts a shelf on a Tuesday afternoon.
//! - **An uncounted row is skipped, not written off.** "Nobody got to this
//!   shelf" and "there are none left" are opposite facts, and applying the first
//!   as the second would write off everything nobody reached.
//! - **A sheet nobody has counted cannot be applied at all.** Closing a fresh
//!   sheet as `applied` would leave a stocktake that claims to have happened; the
//!   act meant is `cancel`, and the refusal says so.
//!
//! A row that was counted and **agrees** with the shelf is skipped too
//! ([`SkipReason::Unchanged`]) — it is not a failure, it is the ordinary result
//! of a shelf that was right, and a zero-quantity movement would be a lie about
//! goods having moved.
//!
//! What a person wrote against a row ("two boxes water-damaged") travels onto
//! the movement it becomes, because that sentence is the explanation of exactly
//! that variance and the ledger is where somebody will go looking for it.

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{BillingProductId, InvCountId, InvLocationId, InvMoveId};
use crate::inv_count::{Count, CountStatus};
use crate::inv_locations::LocationKind;
use crate::inv_moves::{MoveReason, MoveRefKind, MoveReference, NewMove};

/// Why a counted line was not turned into a movement. Every skipped row states
/// one of these, because "we applied 38 of your 51 rows" without saying which
/// and why is a report nobody can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Nobody counted the row. It makes no claim, so nothing is written.
    Uncounted,
    /// The shelf moved between the sheet being opened and this moment: applying
    /// the recorded difference would erase whatever moved. Re-count the row.
    Moved,
    /// It was counted, and it was right. Nothing moved, so no movement.
    Unchanged,
}

impl SkipReason {
    /// The wire word — one spelling for the store, the API and a screen.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Uncounted => "uncounted",
            Self::Moved => "moved",
            Self::Unchanged => "unchanged",
        }
    }
}

/// A row that became a movement.
#[derive(Debug, Clone)]
pub struct AppliedLine {
    /// The product corrected.
    pub product_id: BillingProductId,
    /// Its name in the catalog, so the report explains itself.
    pub product_name: String,
    /// What was on the shelf before this movement, in milli-units.
    pub on_hand_qty_milli: i64,
    /// What the person found, in milli-units.
    pub counted_qty_milli: i64,
    /// `counted − on-hand`, in milli-units: positive is a surplus into stock,
    /// negative a loss out of it. Never zero — a row that agreed is skipped.
    pub variance_qty_milli: i64,
    /// The movement written for it.
    pub move_id: InvMoveId,
}

/// A row that was left alone, and why.
#[derive(Debug, Clone)]
pub struct SkippedLine {
    /// The product not corrected.
    pub product_id: BillingProductId,
    /// Its name in the catalog.
    pub product_name: String,
    /// Why nothing was written.
    pub reason: SkipReason,
    /// What the sheet said was there when it was opened, in milli-units.
    pub expected_qty_milli: i64,
    /// What was found, in milli-units; `None` for an uncounted row.
    pub counted_qty_milli: Option<i64>,
    /// What is on the shelf now, in milli-units — the number that made a
    /// [`SkipReason::Moved`] row a skip.
    pub on_hand_qty_milli: i64,
}

/// Everything applying a stocktake did: the count as it now stands, the rows
/// that moved the ledger, and the rows that did not.
#[derive(Debug, Clone)]
pub struct ApplyOutcome {
    /// The count, re-read: `applied`, closed, and by whom.
    pub count: Count,
    /// The corrections written, in the sheet's own product order.
    pub applied: Vec<AppliedLine>,
    /// The rows left alone, each saying why.
    pub skipped: Vec<SkippedLine>,
}

/// What to do with one row of the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Plan {
    /// Write a movement of this signed quantity: positive into stock, negative
    /// out of it.
    Adjust(i64),
    /// Leave it, for this reason.
    Skip(SkipReason),
}

/// The whole decision, as a pure function of three numbers: what the sheet
/// expected, what the person found, and what is on the shelf at this moment.
///
/// Pure so the rule that matters most in this module can be read and tested
/// without a warehouse. The correction is against **on-hand**, and the snapshot
/// is used for one thing only — to notice that the shelf moved.
fn plan(expected_qty_milli: i64, counted_qty_milli: Option<i64>, on_hand_qty_milli: i64) -> Plan {
    let Some(counted) = counted_qty_milli else {
        return Plan::Skip(SkipReason::Uncounted);
    };
    if on_hand_qty_milli != expected_qty_milli {
        return Plan::Skip(SkipReason::Moved);
    }
    match counted.saturating_sub(on_hand_qty_milli) {
        0 => Plan::Skip(SkipReason::Unchanged),
        variance => Plan::Adjust(variance),
    }
}

/// The refusal a closed count gets, stated once for both the cheap check that
/// keeps the order of refusals honest and the authoritative one under the row
/// lock.
fn already_closed(status: CountStatus) -> StoreError {
    StoreError::Conflict(format!(
        "this count is {} and cannot be applied again",
        status.as_str()
    ))
}

/// One row of the sheet as the apply reads it.
#[derive(sqlx::FromRow)]
struct PlanRow {
    product_id: String,
    product_name: String,
    expected_qty_milli: i64,
    counted_qty_milli: Option<i64>,
    note: String,
}

impl AccountStore {
    /// Applies a stocktake: every counted row that still disagrees with its
    /// shelf becomes an adjustment movement, and the count closes as `applied`.
    ///
    /// The rows that are *not* applied are returned rather than swallowed —
    /// uncounted, moved-underneath, or simply right — so the screen that asked
    /// for this can tell the person which few items to re-count.
    ///
    /// A count that has been applied cannot be applied again: the status is
    /// checked under the count's own row lock, so two people pressing the button
    /// at the same moment produce one set of movements and one refusal.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the count is not this tenant's;
    /// [`StoreError::Conflict`] when it is already closed, when nothing on the
    /// sheet has been counted, or when a movement it implies would leave a real
    /// place holding less than nothing; [`StoreError::Validation`] when the
    /// tenant has no `adjustment` location to move against;
    /// [`StoreError::Db`] on failure. Every one of them leaves the count open
    /// and the ledger exactly as it was.
    pub async fn apply_inv_count(&self, id: &InvCountId) -> Result<ApplyOutcome> {
        // **Whose count it is, first.** Not the authority — the lock below is —
        // but the order of refusals is itself a tenancy rule (B5.05b): an apply
        // of another tenant's count must be a bare `NotFound`, never a complaint
        // about the caller's own warehouse, which would say the count was at
        // least worth looking at.
        let existing = self.inv_count(id).await?.ok_or(StoreError::NotFound)?;
        if !existing.status.is_open() {
            return Err(already_closed(existing.status));
        }

        // The counterparty of every correction, resolved before anything is
        // written. Absent only for a tenant who deleted what they were seeded
        // with, and the answer is then a refusal rather than a movement booked
        // somewhere plausible — the rule receiving follows (B5.05b).
        let adjustment = self
            .inv_location_of_kind(LocationKind::Adjustment)
            .await?
            .ok_or_else(|| {
                StoreError::Validation(
                    "this workspace has no adjustment location to correct stock against; open \
                     Inventory once to have the standard locations created"
                        .to_owned(),
                )
            })?;

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Under the row lock: a cancel or a second apply that arrived while this
        // one was being decided queues here and then sees what this transaction
        // wrote.
        let held: Option<(String, String)> = sqlx::query_as(
            "SELECT status, location_id FROM inv_counts \
             WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let (status, location_id) = held.ok_or(StoreError::NotFound)?;
        let status = CountStatus::parse(&status)?;
        if !status.is_open() {
            return Err(already_closed(status));
        }
        let location_id = InvLocationId::new(location_id);

        // In product-id order, which is the order every apply takes its stock
        // locks in: two counts of two shelves that share products cannot take
        // them in opposite orders and deadlock.
        let rows = sqlx::query_as::<_, PlanRow>(
            "SELECT cl.product_id, p.name AS product_name, cl.expected_qty_milli, \
                 cl.counted_qty_milli, cl.note \
             FROM inv_count_lines cl \
             JOIN billing_products p ON p.tenant_id = cl.tenant_id AND p.id = cl.product_id \
             WHERE cl.tenant_id = $1 AND cl.count_id = $2 \
             ORDER BY cl.product_id",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_all(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if rows.iter().all(|row| row.counted_qty_milli.is_none()) {
            return Err(StoreError::Conflict(
                "nothing on this sheet has been counted yet: count a row, or cancel the \
                 stocktake"
                    .to_owned(),
            ));
        }

        let mut applied = Vec::new();
        let mut skipped = Vec::new();
        for row in rows {
            let product_id = BillingProductId::new(row.product_id);
            // The shelf as it is at this instant. Locked for the counted rows,
            // so the reading the decision is made on is still true when the
            // movement lands on it.
            let on_hand = self
                .count_on_hand(
                    &mut tx,
                    &product_id,
                    &location_id,
                    row.counted_qty_milli.is_some(),
                )
                .await?;
            match plan(row.expected_qty_milli, row.counted_qty_milli, on_hand) {
                Plan::Skip(reason) => skipped.push(SkippedLine {
                    product_id,
                    product_name: row.product_name,
                    reason,
                    expected_qty_milli: row.expected_qty_milli,
                    counted_qty_milli: row.counted_qty_milli,
                    on_hand_qty_milli: on_hand,
                }),
                Plan::Adjust(variance) => {
                    // Direction is the pair of locations, never a sign: a
                    // surplus comes out of the adjustment location into stock,
                    // a loss goes the other way.
                    let (from, to) = if variance > 0 {
                        (adjustment.id.clone(), location_id.clone())
                    } else {
                        (location_id.clone(), adjustment.id.clone())
                    };
                    let move_id = self
                        .record_move_in(
                            &mut tx,
                            &NewMove {
                                product_id: product_id.clone(),
                                from_location_id: from,
                                to_location_id: to,
                                qty_milli: variance.abs(),
                                reason: MoveReason::Count,
                                reason_code: None,
                                note: row.note,
                                reference: Some(MoveReference {
                                    kind: MoveRefKind::Count,
                                    id: id.as_str().to_owned(),
                                }),
                                occurred_at: None,
                            },
                        )
                        .await?;
                    applied.push(AppliedLine {
                        product_id,
                        product_name: row.product_name,
                        on_hand_qty_milli: on_hand,
                        counted_qty_milli: row.counted_qty_milli.unwrap_or(on_hand),
                        variance_qty_milli: variance,
                        move_id,
                    });
                }
            }
        }

        sqlx::query(
            "UPDATE inv_counts SET status = 'applied', closed_at = now(), closed_by = $3, \
                 updated_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND status = 'open'",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;

        let count = self.inv_count(id).await?.ok_or(StoreError::NotFound)?;
        Ok(ApplyOutcome {
            count,
            applied,
            skipped,
        })
    }

    /// What one shelf holds of one product, inside the apply's transaction.
    ///
    /// `lock` is taken for a row that may be about to move: the cached balance
    /// row is held until commit, so the reading this decision is made on cannot
    /// change between the decision and the movement. A row nobody counted is
    /// read without a lock — it is going to be skipped, and blocking the
    /// warehouse's other work over a row we will not touch is a cost with no
    /// benefit.
    async fn count_on_hand(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        product: &BillingProductId,
        location: &InvLocationId,
        lock: bool,
    ) -> Result<i64> {
        let sql = format!(
            "SELECT qty_milli FROM inv_stock \
             WHERE tenant_id = $1 AND product_id = $2 AND location_id = $3{}",
            if lock { " FOR UPDATE" } else { "" }
        );
        let held: Option<i64> = sqlx::query_scalar(&sql)
            .bind(self.tenant.as_str())
            .bind(product.as_str())
            .bind(location.as_str())
            .fetch_optional(&mut **tx)
            .await
            .map_err(StoreError::Db)?;
        Ok(held.unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_nobody_counted_is_left_alone() {
        // "Nobody got to this shelf" is not "there are none left": applying the
        // second as the first writes off everything nobody reached.
        assert_eq!(plan(5_000, None, 5_000), Plan::Skip(SkipReason::Uncounted));
        assert_eq!(plan(5_000, None, 0), Plan::Skip(SkipReason::Uncounted));
        assert_eq!(plan(0, None, 3_000), Plan::Skip(SkipReason::Uncounted));
    }

    #[test]
    fn a_shelf_that_moved_underneath_the_counter_is_skipped() {
        // Two more chairs arrived while the sheet was being worked down. The
        // recorded difference is now a lie about the delivery, so nothing is
        // written and the row is reported for a re-count.
        assert_eq!(
            plan(5_000, Some(4_000), 7_000),
            Plan::Skip(SkipReason::Moved)
        );
        // Even when the count happens to agree with what is there now: the
        // snapshot it was taken against is stale, so the finding is not about
        // this shelf as it stands.
        assert_eq!(
            plan(5_000, Some(7_000), 7_000),
            Plan::Skip(SkipReason::Moved)
        );
    }

    #[test]
    fn a_row_that_agrees_writes_nothing() {
        assert_eq!(
            plan(5_000, Some(5_000), 5_000),
            Plan::Skip(SkipReason::Unchanged)
        );
        assert_eq!(plan(0, Some(0), 0), Plan::Skip(SkipReason::Unchanged));
    }

    #[test]
    fn a_loss_and_a_surplus_are_the_two_signs_of_one_number() {
        // Four found where five were expected: one is missing.
        assert_eq!(plan(5_000, Some(4_000), 5_000), Plan::Adjust(-1_000));
        // Three found on a shelf the system believed empty: a surplus, which is
        // the case a stocktake most exists to catch.
        assert_eq!(plan(0, Some(3_000), 0), Plan::Adjust(3_000));
        // Counting zero is the strongest claim a stocktake makes, and it writes
        // the whole shelf off.
        assert_eq!(plan(5_000, Some(0), 5_000), Plan::Adjust(-5_000));
    }

    #[test]
    fn the_correction_is_against_the_shelf_and_never_against_the_snapshot() {
        // The one rule this module exists for, stated as a pair: with a moved
        // shelf there is no correction at all, and with a still shelf the
        // correction is the difference from what is actually there.
        assert!(matches!(
            plan(5_000, Some(4_000), 4_500),
            Plan::Skip(SkipReason::Moved)
        ));
        assert_eq!(plan(4_500, Some(4_000), 4_500), Plan::Adjust(-500));
    }

    #[test]
    fn a_variance_saturates_rather_than_overflowing() {
        // Unreachable through the doors — both numbers are bounded by
        // `QTY_MAX_MILLI` — and pinned anyway, because a wrapped subtraction
        // here would be a correction with the wrong sign.
        assert_eq!(
            plan(i64::MIN, Some(i64::MAX), i64::MIN),
            Plan::Adjust(i64::MAX)
        );
    }

    #[test]
    fn every_skip_reason_has_one_spelling() {
        for (reason, word) in [
            (SkipReason::Uncounted, "uncounted"),
            (SkipReason::Moved, "moved"),
            (SkipReason::Unchanged, "unchanged"),
        ] {
            assert_eq!(reason.as_str(), word);
        }
    }
}
