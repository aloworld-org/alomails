//! **Confirming** a sales order (alo Inventory, ADR 0035, wave B5.06a) — the
//! one act that draws the number, stamps the day and freezes the document.
//!
//! A sales order's `confirmed` state means precisely *"we have said yes"*
//! (`docs/design/inventory.md` § Sales orders). From that moment the customer
//! holds a document with a number on it, so the lines stop being editable and
//! the number goes on every delivery note that travels in a box against it.
//!
//! **What confirming deliberately does NOT do is touch stock.** It writes no
//! movement and reserves no quantity. A sales order is a promise; goods move
//! when they are picked ([`crate::inv_so_deliver`]). A reservation would be a
//! second ledger of quantities that must agree with the first, and two ledgers
//! that must agree are one ledger and one bug — what "promised out" means is
//! answered by summing the open orders' outstanding lines, which is the shortage
//! query's job (B5.07) and costs nothing to keep true.
//!
//! It is [`crate::inv_po_send`] with the letter left out, and the letter is left
//! out for a reason rather than an omission: placing an order with a supplier
//! *is* asking them, so an order marked sent that nobody sent is a lie the
//! shortage report repeats. Confirming an order a customer placed is recording
//! an answer we already gave them — on the telephone, in a reply, at a counter —
//! so no message has to be written for the record to be true. Sending the
//! customer a confirmation is an ordinary letter, and it goes through the one
//! audited submission path like every other message this product composes.

use time::Date;

use crate::account::AccountStore;
use crate::billing_sequence::{
    SALES_ORDER_NUMBER_PREFIX, SALES_ORDER_SEQUENCE_KIND, document_number, draw_next,
};
use crate::error::{Result, StoreError};
use crate::id::InvSalesOrderId;
use crate::inv_so::{SalesOrderDocument, SoStatus};
use crate::inv_so_lines;

impl AccountStore {
    /// Confirms one of this tenant's **draft** orders: draws `SO-YYYY-NNNNN`,
    /// stamps today as the confirmation date and moves the order to
    /// `confirmed` — in one transaction.
    ///
    /// Three refusals, all before anything is written:
    ///
    /// - an order that is **not a draft** — already confirmed, delivered or
    ///   cancelled — is a [`StoreError::Conflict`] naming its state, so
    ///   re-confirming can never draw a second number for one document;
    /// - an order with **no lines** is a [`StoreError::Validation`]: a
    ///   confirmation of nothing promises nothing, and the delivery note it
    ///   would eventually print would be a blank sheet;
    /// - an order that is **not this tenant's** is a [`StoreError::NotFound`],
    ///   indistinguishable from one that does not exist.
    ///
    /// # Errors
    /// The three above, and [`StoreError::Db`] on failure. Every one of them
    /// leaves the order exactly as it was.
    pub async fn confirm_inv_sales_order(
        &self,
        id: &InvSalesOrderId,
    ) -> Result<SalesOrderDocument> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // The status under the row lock is the authority: a `PATCH` or a second
        // confirmation that arrived while this one was composing queues here and
        // then sees the state this transaction wrote.
        let locked = lock_status(&mut tx, self.tenant.as_str(), id.as_str()).await?;
        locked.ensure_transition(SoStatus::Confirmed)?;

        let lines = inv_so_lines::read(&mut *tx, self.tenant.as_str(), id.as_str()).await?;
        if lines.is_empty() {
            return Err(StoreError::Validation(
                "an order with no lines promises the customer nothing; add a line before \
                 confirming it"
                    .to_owned(),
            ));
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
            SALES_ORDER_SEQUENCE_KIND,
            today.year(),
        )
        .await?;
        let number = document_number(SALES_ORDER_NUMBER_PREFIX, today.year(), drawn);

        sqlx::query(
            "UPDATE inv_sales_orders \
                SET status = 'confirmed', number = $3, confirmed_date = $4, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&number)
        .bind(today)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;

        self.inv_sales_order(id).await?.ok_or(StoreError::NotFound)
    }
}

/// Takes the order's row lock inside `tx` and returns its status.
///
/// A duplicate in spirit of [`crate::inv_so`]'s own locking read, and
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
) -> Result<SoStatus> {
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT status FROM inv_sales_orders WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(tenant)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    let stored = stored.ok_or(StoreError::NotFound)?;
    SoStatus::parse(&stored).ok_or_else(|| {
        StoreError::Db(sqlx::Error::Decode(
            "inv_sales_orders.status is not a known status".into(),
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_draft_may_be_confirmed_and_never_twice() {
        // The transition table is the whole rule (`inv_so.rs` tests it over all
        // twenty-five pairs); what this states is which pair *confirming* asks
        // for, so a later refactor cannot quietly let a confirmed order be
        // confirmed again and draw a second number for one document.
        assert!(SoStatus::Draft.can_advance_to(SoStatus::Confirmed));
        for already in [
            SoStatus::Confirmed,
            SoStatus::PartiallyDelivered,
            SoStatus::Delivered,
            SoStatus::Cancelled,
        ] {
            assert!(
                !already.can_advance_to(SoStatus::Confirmed),
                "{already:?} must not be confirmable"
            );
        }
    }

    #[test]
    fn the_number_is_drawn_from_the_orders_own_series() {
        // Not the invoice series and not the quote series: a number an order
        // consumed must leave no hole in either of the two a customer sees on a
        // document that reaches their bookkeeping.
        assert_eq!(SALES_ORDER_SEQUENCE_KIND, "sales_order");
        assert_eq!(
            document_number(SALES_ORDER_NUMBER_PREFIX, 2026, 1),
            "SO-2026-00001"
        );
    }
}
