//! The manual door onto the ledger — a transfer between two of the tenant's own
//! places, or an adjustment a person signed for (alo Inventory, ADR 0035, wave
//! B5.04b; `docs/design/inventory.md`, "Adjustments and transfers").
//!
//! Every other movement in the system is a **consequence of a document**: goods
//! arrive because a purchase order was received, leave because a sales order was
//! delivered, move because a stocktake was applied. This is the one place a
//! movement is written because a human said so, and that is why it is the most
//! carefully guarded write in the module — it is the one that can make theft
//! look like paperwork.
//!
//! Three rules follow from that, and all three are refusals:
//!
//! 1. **A manual movement is a transfer or an adjustment, and nothing else.**
//!    `purchase`, `sale`, `return_in`, `return_out` and `count` name documents;
//!    accepting them here would let a purchase be booked with no order behind
//!    it, and the three-way match (B5.05b) would stop meaning anything.
//! 2. **Neither end may be the `supplier` or `customer` location.** They are
//!    reachable only through a receipt or a delivery — the same rule stated from
//!    the other side, and enforced separately so the refusal says which of the
//!    two mistakes was made.
//! 3. **An adjustment carries a reason code**, from the closed list in
//!    [`AdjustReason`]. "Why is stock disappearing" has a small number of real
//!    answers, and a free-text field answers it with the empty string.
//!
//! [`AccountStore::record_manual_move`] adds no writing of its own: it checks
//! what a person may ask for and hands the movement to
//! [`AccountStore::record_move`], which stays the one writer of the ledger and
//! of the cached balance. The quantity bounds, the negative-stock rule, the
//! tenancy of every id and the append-only discipline are that function's, and
//! are not restated here.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{BillingProductId, InvLocationId, InvMoveId};
use crate::inv_locations::LocationKind;
use crate::inv_moves::{MoveReason, NewMove};

/// Why the shelf disagreed with the system — the closed vocabulary an
/// adjustment is required to pick from.
///
/// The list is short on purpose. It is not a taxonomy of warehouse life; it is
/// the set of answers that change what somebody does next — write it off, look
/// for it, order more, or correct a keying mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdjustReason {
    /// Broken, spoiled, unsellable. Gone, and known to be gone.
    Damaged,
    /// Missing, with no explanation. The honest word for shrinkage.
    Lost,
    /// Past its date.
    Expired,
    /// Surplus: more on the shelf than the system believed.
    Found,
    /// Given away as a sample or used for a demonstration.
    Sample,
    /// Consumed by the business itself rather than sold.
    InternalUse,
    /// A keying mistake being put right — the counterpart of a movement
    /// recorded in error, which is corrected by another movement and never by
    /// an edit.
    Correction,
}

/// Every code, in the order a picker offers them: the losses first, because
/// that is what an adjustment usually is, then the two that are not losses.
pub const ADJUST_REASONS: [AdjustReason; 7] = [
    AdjustReason::Damaged,
    AdjustReason::Lost,
    AdjustReason::Expired,
    AdjustReason::InternalUse,
    AdjustReason::Sample,
    AdjustReason::Found,
    AdjustReason::Correction,
];

impl AdjustReason {
    /// The stored word — the database value and the wire form, one spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Damaged => "damaged",
            Self::Lost => "lost",
            Self::Expired => "expired",
            Self::Found => "found",
            Self::Sample => "sample",
            Self::InternalUse => "internal_use",
            Self::Correction => "correction",
        }
    }

    /// Reads the stored word back.
    ///
    /// # Errors
    /// [`StoreError::Validation`] listing every accepted code — the message a
    /// caller needs to fix the request without reading our source.
    pub fn parse(value: &str) -> Result<Self> {
        ADJUST_REASONS
            .into_iter()
            .find(|code| code.as_str() == value.trim())
            .ok_or_else(|| StoreError::Validation(format!("reason code must be {}", code_list())))
    }
}

/// The accepted codes as one comma-separated phrase, built from
/// [`ADJUST_REASONS`] so a code added later cannot ship with a message that
/// fails to mention it.
fn code_list() -> String {
    ADJUST_REASONS
        .iter()
        .map(|code| code.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The message a movement gets when its reason code is missing or unwanted.
/// Public to [`crate::inv_moves`], which enforces the pairing at the moment of
/// writing so no door can get past it.
pub(crate) fn reason_code_required() -> StoreError {
    StoreError::Validation(format!(
        "an adjustment needs a reason code: {}",
        code_list()
    ))
}

/// The writable shape of a movement a person makes directly.
///
/// Deliberately **not** [`NewMove`]: this shape cannot express a document
/// reference at all, so the one door a human reaches can never claim a purchase
/// order stands behind a movement that no order produced.
#[derive(Debug, Clone)]
pub struct NewManualMove {
    /// What moved. One of this tenant's stocked products.
    pub product_id: BillingProductId,
    /// Where it left.
    pub from_location_id: InvLocationId,
    /// Where it arrived.
    pub to_location_id: InvLocationId,
    /// How much, in milli-units. Strictly positive — direction is the pair of
    /// locations.
    pub qty_milli: i64,
    /// [`MoveReason::Transfer`] or [`MoveReason::Adjustment`]; anything else is
    /// a document's movement and is refused.
    pub reason: MoveReason,
    /// Why the shelf disagreed. Required for an adjustment, refused otherwise.
    pub reason_code: Option<AdjustReason>,
    /// What the person wrote about it. Bounded, and never logged.
    pub note: String,
    /// When it physically happened. `None` means now.
    pub occurred_at: Option<OffsetDateTime>,
}

/// Checks a manual movement against the kinds of the two places it names.
///
/// Pure, so the whole rule is unit-tested without a database — the shape
/// [`crate::inv_moves`]' `normalize` has, and for the same reason: this is the
/// rule most worth reading, and it should not need a warehouse to read it.
///
/// # Errors
/// [`StoreError::Validation`] naming which of the three rules was broken.
fn check_manual(reason: MoveReason, from: LocationKind, to: LocationKind) -> Result<()> {
    if !matches!(reason, MoveReason::Transfer | MoveReason::Adjustment) {
        return Err(StoreError::Validation(
            "a movement made by hand is a transfer or an adjustment — every other reason \
             comes from a document"
                .to_owned(),
        ));
    }
    for kind in [from, to] {
        if matches!(kind, LocationKind::Supplier | LocationKind::Customer) {
            return Err(StoreError::Validation(
                "a supplier or customer location is reached only through a purchase order or \
                 a sales order"
                    .to_owned(),
            ));
        }
    }
    match reason {
        MoveReason::Transfer if !(from.is_real() && to.is_real()) => Err(StoreError::Validation(
            "a transfer moves between two of the tenant's own locations".to_owned(),
        )),
        MoveReason::Adjustment
            if (from == LocationKind::Adjustment) == (to == LocationKind::Adjustment) =>
        {
            Err(StoreError::Validation(
                "an adjustment moves to or from the adjustment location: out of stock for a \
                 loss, into stock for a surplus"
                    .to_owned(),
            ))
        }
        _ => Ok(()),
    }
}

impl AccountStore {
    /// Records a movement **a person made**: a transfer between two of their own
    /// places, or an adjustment against the adjustment location with a reason
    /// code and, usually, a sentence about it.
    ///
    /// The checks that are this door's are in [`check_manual`]; everything else
    /// — the quantity bounds, the note bound, the product being one of this
    /// tenant's stocked ones, the negative-stock rule and the single
    /// transaction — is [`AccountStore::record_move`]'s and is not restated.
    ///
    /// Moving *into* an archived location is refused, and moving *out of* one
    /// is not: a shed is archived while it is being emptied, and the movements
    /// that empty it are exactly what archiving must not block.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a reason that names a document, a
    /// `supplier` or `customer` end, an adjustment that does not touch the
    /// adjustment location, a missing or unwanted reason code, or any of
    /// [`AccountStore::record_move`]'s field rules; [`StoreError::NotFound`]
    /// when the product or either location is not this tenant's;
    /// [`StoreError::Conflict`] when the goods are not there or the destination
    /// is archived; [`StoreError::Db`] on failure.
    pub async fn record_manual_move(&self, input: &NewManualMove) -> Result<InvMoveId> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let from = self
            .require_tenant_location(&mut tx, &input.from_location_id)
            .await?;
        let to = self
            .require_tenant_location(&mut tx, &input.to_location_id)
            .await?;
        check_manual(input.reason, from.kind, to.kind)?;
        if to.is_archived() {
            return Err(StoreError::Conflict(format!(
                "{} is archived and cannot receive stock",
                to.code
            )));
        }
        let id = self
            .record_move_in(
                &mut tx,
                &NewMove {
                    product_id: input.product_id.clone(),
                    from_location_id: input.from_location_id.clone(),
                    to_location_id: input.to_location_id.clone(),
                    qty_milli: input.qty_milli,
                    reason: input.reason,
                    reason_code: input.reason_code,
                    note: input.note.clone(),
                    reference: None,
                    occurred_at: input.occurred_at,
                },
            )
            .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
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
    fn the_code_vocabulary_round_trips_and_refuses_anything_else() {
        for code in ADJUST_REASONS {
            assert_eq!(
                AdjustReason::parse(code.as_str()).unwrap_or(AdjustReason::Correction),
                code
            );
        }
        assert_eq!(AdjustReason::parse(" lost ").ok(), Some(AdjustReason::Lost));
        for bad in ["", "shrinkage", "LOST", "internal use", "stolen"] {
            let message = invalid(AdjustReason::parse(bad));
            assert!(message.contains("reason code must be"), "{message}");
            // The refusal lists every code, so a caller never has to read our
            // source to fix the request.
            for code in ADJUST_REASONS {
                assert!(message.contains(code.as_str()), "{message} omits {code:?}");
            }
        }
    }

    #[test]
    fn only_a_transfer_or_an_adjustment_can_be_made_by_hand() {
        for document in [
            MoveReason::Purchase,
            MoveReason::Sale,
            MoveReason::Count,
            MoveReason::ReturnIn,
            MoveReason::ReturnOut,
        ] {
            let message = invalid(check_manual(
                document,
                LocationKind::Stock,
                LocationKind::Stock,
            ));
            assert!(message.contains("comes from a document"), "{message}");
        }
    }

    #[test]
    fn the_two_trading_counterparties_are_unreachable_by_hand() {
        // This refusal is what keeps the three-way match meaningful: a purchase
        // can never be booked without an order behind it.
        for (reason, from, to) in [
            (
                MoveReason::Transfer,
                LocationKind::Supplier,
                LocationKind::Stock,
            ),
            (
                MoveReason::Transfer,
                LocationKind::Stock,
                LocationKind::Customer,
            ),
            (
                MoveReason::Adjustment,
                LocationKind::Customer,
                LocationKind::Stock,
            ),
            (
                MoveReason::Adjustment,
                LocationKind::Stock,
                LocationKind::Supplier,
            ),
        ] {
            let message = invalid(check_manual(reason, from, to));
            assert!(message.contains("purchase order"), "{message}");
        }
    }

    #[test]
    fn a_transfer_moves_between_two_real_places() {
        for (from, to) in [
            (LocationKind::Stock, LocationKind::Stock),
            (LocationKind::Stock, LocationKind::Transit),
            (LocationKind::Transit, LocationKind::Stock),
        ] {
            assert!(check_manual(MoveReason::Transfer, from, to).is_ok());
        }
        for (from, to) in [
            (LocationKind::Adjustment, LocationKind::Stock),
            (LocationKind::Stock, LocationKind::Production),
        ] {
            let message = invalid(check_manual(MoveReason::Transfer, from, to));
            assert!(
                message.contains("two of the tenant's own locations"),
                "{message}"
            );
        }
    }

    #[test]
    fn an_adjustment_touches_the_adjustment_location_exactly_once() {
        // A loss out of stock, and a surplus into it.
        assert!(
            check_manual(
                MoveReason::Adjustment,
                LocationKind::Stock,
                LocationKind::Adjustment
            )
            .is_ok()
        );
        assert!(
            check_manual(
                MoveReason::Adjustment,
                LocationKind::Adjustment,
                LocationKind::Transit
            )
            .is_ok()
        );
        for (from, to) in [
            // Neither end: this is a transfer wearing the wrong word, and
            // accepting it would put "why did stock disappear" on a movement
            // where nothing left the building.
            (LocationKind::Stock, LocationKind::Transit),
            // Both ends cannot happen (the endpoints differ) but the rule is
            // stated as an equality, so the case is pinned rather than assumed.
            (LocationKind::Adjustment, LocationKind::Adjustment),
            // `production` is seeded and unused in B5: assembly is a cut, and
            // an adjustment is not the door it would arrive through.
            (LocationKind::Stock, LocationKind::Production),
        ] {
            let message = invalid(check_manual(MoveReason::Adjustment, from, to));
            assert!(message.contains("adjustment location"), "{message}");
        }
    }
}
