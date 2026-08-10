//! **Placing** a purchase order (alo Inventory, ADR 0035, wave B5.05a2) — the
//! one act that draws the number, stamps the day, freezes the document and
//! writes the letter that tells the supplier.
//!
//! A purchase order's `sent` state means precisely *"we have asked them"*
//! (`docs/design/inventory.md` § Sending). An order marked sent that nobody
//! ever sent is the state that makes a shortage report lie: the goods are
//! counted as on their way while no supplier has heard of them. So sending is
//! not a status field a caller may set. It is this function, and it either does
//! all four things or none of them.
//!
//! **How "all or none" is actually held.** The number, the date and the status
//! are written in one transaction, and the letter is written by a callback
//! *inside* that transaction — the caller's ([`AccountStore::send_inv_purchase_order`]'s
//! `letter` argument), because only the route layer can render a PDF and reach
//! a mailbox. If the letter fails, the transaction rolls back: the order is
//! still a draft, and — this is the part the row-locked counter buys us — the
//! number it had drawn is given back rather than left as a hole.
//!
//! The one crack in that, stated rather than hidden: the letter is a message in
//! the caller's mailbox, written on its own connection, so a commit that fails
//! *after* the letter was written leaves a draft email for an order that is
//! still a draft. That is visible, harmless and correctable by a person. The
//! opposite — an order the supplier is expected to fulfil with no letter ever
//! written — is the one this design refuses.
//!
//! Nothing here sends mail. The letter is a **draft** in the caller's Drafts
//! (ADR 0034), sent by a human through the one audited submission path.

use std::future::Future;

use time::Date;

use crate::account::AccountStore;
use crate::billing_sequence::{
    PURCHASE_ORDER_NUMBER_PREFIX, PURCHASE_ORDER_SEQUENCE_KIND, document_number, draw_next,
};
use crate::billing_totals::totals;
use crate::error::{Result, StoreError};
use crate::id::InvPurchaseOrderId;
use crate::inv_po::{PoStatus, PurchaseOrder, PurchaseOrderDocument};
use crate::inv_po_lines::{self, PoLine};

/// The columns [`read_in_tx`] selects, in `SentPoRow` order.
const SENT_PO_COLS: &str = "id, supplier_id, status, currency, number, ordered_date, expected_date, closed_date, \
     reference, note, created_by, created_at, updated_at";

impl AccountStore {
    /// Places one of this tenant's **draft** orders with its supplier: draws
    /// `PO-YYYY-NNNNN`, stamps today as the order date, moves the order to
    /// `sent`, and writes the covering letter — in that order, in one
    /// transaction.
    ///
    /// `letter` is handed the order **as it will be stored**, number and date
    /// included, so the paper it renders is the document the supplier will quote
    /// back. Whatever it returns is returned to the caller beside the document;
    /// whatever it fails with rolls the placement back untouched.
    ///
    /// Three refusals, all before anything is written:
    ///
    /// - an order that is **not a draft** — already sent, received or cancelled
    ///   — is a [`StoreError::Conflict`] naming its state, so re-sending can
    ///   never draw a second number for one document;
    /// - an order with **no lines** is a [`StoreError::Validation`]: an empty
    ///   order asks a supplier for nothing, and their reply would be a
    ///   telephone call;
    /// - an order that is **not this tenant's** is a [`StoreError::NotFound`],
    ///   indistinguishable from one that does not exist.
    ///
    /// # Errors
    /// The three above, `E` from the letter, and [`StoreError::Db`] on failure.
    /// Every one of them leaves the order exactly as it was.
    pub async fn send_inv_purchase_order<T, E, F, Fut>(
        &self,
        id: &InvPurchaseOrderId,
        letter: F,
    ) -> std::result::Result<(PurchaseOrderDocument, T), E>
    where
        E: From<StoreError>,
        F: FnOnce(PurchaseOrderDocument) -> Fut,
        Fut: Future<Output = std::result::Result<T, E>>,
    {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // The status under the row lock is the authority: a `PATCH` or a second
        // send that arrived while this one was composing queues here and then
        // sees the state this transaction wrote.
        let locked = lock_status(&mut tx, self.tenant.as_str(), id.as_str()).await?;
        locked.ensure_transition(PoStatus::Sent).map_err(E::from)?;

        let lines = inv_po_lines::read(&mut *tx, self.tenant.as_str(), id.as_str())
            .await
            .map_err(E::from)?;
        if lines.is_empty() {
            return Err(E::from(StoreError::Validation(
                "an order with no lines asks the supplier for nothing; add a line before sending it"
                    .to_owned(),
            )));
        }

        // One clock for the whole transaction, and the same clock the row's own
        // timestamps use — never the caller's.
        let today: Date = sqlx::query_scalar("SELECT CURRENT_DATE")
            .fetch_one(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        let drawn = draw_next(
            &mut tx,
            self.tenant.as_str(),
            PURCHASE_ORDER_SEQUENCE_KIND,
            today.year(),
        )
        .await
        .map_err(E::from)?;
        let number = document_number(PURCHASE_ORDER_NUMBER_PREFIX, today.year(), drawn);

        sqlx::query(
            "UPDATE inv_purchase_orders \
                SET status = 'sent', number = $3, ordered_date = $4, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&number)
        .bind(today)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        // Read back through the same transaction rather than rebuilt in memory:
        // the document the letter renders is then literally the stored row,
        // including whatever the database's own CHECKs and defaults made of it.
        let document = read_in_tx(&mut tx, self.tenant.as_str(), id.as_str(), lines).await?;
        // The letter is handed its own copy of the order rather than a borrow
        // into the open transaction: the callback is the caller's code, it will
        // hold references of its own (a mailbox, a renderer), and a borrow that
        // has to outlive none of them is one lifetime puzzle nobody needs to
        // solve at a call site.
        let carried = letter(document.clone()).await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok((document, carried))
    }
}

/// Takes the order's row lock inside `tx` and returns its status.
///
/// A duplicate in spirit of [`crate::inv_po`]'s own locking read, and
/// deliberately not shared with it: that one is private to the record's write
/// paths, and a lock helper that two modules reach into is how a lock ends up
/// taken in one place and relied on in another.
///
/// # Errors
/// [`StoreError::NotFound`] when the id is absent **or another tenant's**;
/// [`StoreError::Db`] on failure or on a status the code does not know.
async fn lock_status(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    id: &str,
) -> Result<PoStatus> {
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT status FROM inv_purchase_orders WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(tenant)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    let stored = stored.ok_or(StoreError::NotFound)?;
    PoStatus::parse(&stored).ok_or_else(|| {
        StoreError::Db(sqlx::Error::Decode(
            "inv_purchase_orders.status is not a known status".into(),
        ))
    })
}

/// Reads the just-placed order back inside the transaction that placed it.
///
/// Takes the lines it already has: they were read (and required to be
/// non-empty) before the number was drawn, they cannot change under the lock,
/// and reading them twice would only invite the two reads to disagree.
///
/// # Errors
/// [`StoreError::NotFound`] if the row has vanished — impossible under the
/// lock; [`StoreError::Db`] on failure or a status the code does not know.
async fn read_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    id: &str,
    lines: Vec<PoLine>,
) -> Result<PurchaseOrderDocument> {
    let row: Option<SentPoRow> = sqlx::query_as(&format!(
        "SELECT {SENT_PO_COLS}, \
             (SELECT name FROM inv_suppliers s \
               WHERE s.tenant_id = o.tenant_id AND s.id = o.supplier_id) AS supplier_name \
         FROM inv_purchase_orders o WHERE tenant_id = $1 AND id = $2"
    ))
    .bind(tenant)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    let row = row.ok_or(StoreError::NotFound)?;
    let supplier_name = row.supplier_name.clone().unwrap_or_default();
    let figures: Vec<_> = lines.iter().map(PoLine::figures).collect();
    Ok(PurchaseOrderDocument {
        order: row.into_order()?,
        supplier_name,
        totals: totals(&figures),
        lines,
    })
}

/// The header row, read inside the placing transaction.
///
/// Its own row type rather than [`crate::inv_po`]'s: that one is private to the
/// record module, and a row type shared across modules is a column list two
/// files have to keep in step.
#[derive(sqlx::FromRow)]
struct SentPoRow {
    id: String,
    supplier_id: String,
    status: String,
    currency: String,
    number: Option<String>,
    ordered_date: Option<Date>,
    expected_date: Option<Date>,
    closed_date: Option<Date>,
    reference: String,
    note: String,
    created_by: String,
    created_at: time::OffsetDateTime,
    updated_at: time::OffsetDateTime,
    supplier_name: Option<String>,
}

impl SentPoRow {
    fn into_order(self) -> Result<PurchaseOrder> {
        let status = PoStatus::parse(&self.status).ok_or_else(|| {
            StoreError::Db(sqlx::Error::Decode(
                "inv_purchase_orders.status is not a known status".into(),
            ))
        })?;
        Ok(PurchaseOrder {
            id: InvPurchaseOrderId::new(self.id),
            supplier_id: crate::id::InvSupplierId::new(self.supplier_id),
            status,
            currency: self.currency,
            number: self.number,
            ordered_date: self.ordered_date,
            expected_date: self.expected_date,
            closed_date: self.closed_date,
            reference: self.reference,
            note: self.note,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_draft_may_be_placed_and_never_twice() {
        // The transition table is the whole rule (`inv_po.rs` tests it over all
        // twenty-five pairs); what this states is which pair *sending* asks
        // for, so a later refactor cannot quietly let a sent order be re-sent
        // and draw a second number for one document.
        assert!(PoStatus::Draft.can_advance_to(PoStatus::Sent));
        for already in [
            PoStatus::Sent,
            PoStatus::PartiallyReceived,
            PoStatus::Received,
            PoStatus::Cancelled,
        ] {
            assert!(
                !already.can_advance_to(PoStatus::Sent),
                "{already:?} must not be sendable"
            );
        }
    }

    #[test]
    fn the_number_is_drawn_from_the_orders_own_series() {
        // Not the invoice series and not the quote series: a number an order
        // consumed must leave no hole in either of the two a customer sees.
        assert_eq!(PURCHASE_ORDER_SEQUENCE_KIND, "purchase_order");
        assert_eq!(
            document_number(PURCHASE_ORDER_NUMBER_PREFIX, 2026, 1),
            "PO-2026-00001"
        );
    }
}
