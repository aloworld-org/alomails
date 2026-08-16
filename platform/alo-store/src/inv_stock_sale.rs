//! Inventory's stock-sale seam — the one door a site's shop may read and
//! reserve the warehouse through (ADR 0041, item S3.05a1).
//!
//! Stock is one number: the shelf count lives in `inv_stock`, written only by
//! the move ledger ([`crate::inv_moves`]), and nothing here stores a second
//! copy of it. What a shop may sell is computed at every read — the ledger's
//! on-hand at real `stock` locations, minus the live holds this module keeps —
//! so there is no cached availability anywhere to drift, which is the entire
//! argument of ADR 0041.
//!
//! The hold is the ticket hold's shape ([`crate::site_ticket_holds`]) applied
//! to goods: taken **before** payment, counting against available-to-sell from
//! that instant, and freeing itself by time passing (`expires_at > now` is a
//! predicate, never a sweeper). Two buyers after the last unit are settled by
//! the database — every state-changing path takes a transaction-scoped
//! advisory lock on `(tenant, product)` before it counts — so exactly one of
//! them gets it and the other is told "sold out" rather than given goods that
//! are not there.
//!
//! **Completing a hold is a movement, not a flag.** [`InvStockSale::claim`]
//! marks the hold completed and records the real outbound movement — reason
//! `sale`, from the tenant's own stock locations to their `customer`
//! counterparty, through [`record_move`]'s own transaction-shape — in ONE
//! transaction. That is why a completed hold counts for nothing in the
//! availability sum where a completed *ticket* hold counts forever: the shelf
//! count itself has already dropped, and counting the hold too would subtract
//! the sale twice.
//!
//! What this seam deliberately does **not** do: it does not make Inventory's
//! own doors honour shop holds. "Confirming reserves nothing" is Inventory's
//! documented decision ([`crate::inv_so`]), and a warehouse door that suddenly
//! consulted a shop table would repeal it from the outside. A transfer or
//! delivery may therefore take goods a hold was counting on; the ledger stays
//! non-negative regardless ([`record_move`] refuses), and the claim that can
//! no longer be satisfied fails cleanly for the payment flow to handle —
//! honest scarcity, never an oversold ledger.
//!
//! Privacy: a hold is pure quantity accounting and stores **no buyer identity
//! of any kind**. Who bought lives where the sale puts it (the order, the
//! invoice, the CRM card — S3.05a2), in records the tenant already owns. The
//! columns-of-the-table proof lives in `tests/inv_stock_sale.rs`.
//!
//! This file belongs to Inventory, not to Sites — what moves, from where, and
//! what the ledger refuses stay Inventory's to decide, in Inventory's own
//! words. Scoping is the seam handshake every door uses
//! ([`crate::billing_catalog_read`], [`crate::calendar_availability`]): opened
//! with a `(tenant, owner)` pair the caller must already have resolved from
//! its own trusted row — for Sites, the site's record, never a request.
//!
//! [`record_move`]: crate::account::AccountStore::record_move

use sqlx::PgPool;
use time::{Duration, OffsetDateTime};

use crate::account::AccountStore;
use crate::blob::BlobStore;
use crate::error::{Result, StoreError};
use crate::id::{BillingProductId, InvLocationId, InvStockHoldId, TenantId, UserId};
use crate::inv_moves::{MoveReason, NewMove};

/// The most whole units one hold may cover — a buyer's basket, not a
/// scalper's script. A buyer wanting more is a phone call, not a checkout.
pub const STOCK_HOLD_MAX_UNITS: i64 = 20;
/// The shortest life a hold may be given — under a minute is a race with the
/// payment page itself.
pub const STOCK_HOLD_MIN_TTL: Duration = Duration::minutes(1);
/// The longest life a hold may be given — beyond an hour an abandoned basket
/// is squatting goods real buyers want.
pub const STOCK_HOLD_MAX_TTL: Duration = Duration::hours(1);

/// One milli-unit conversion, named once: the ledger counts milli-units
/// ([`crate::inv_moves`]), the shop sells whole ones.
const MILLI_PER_UNIT: i64 = 1_000;

/// What Inventory will say about one product's sellability across this seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StockForSale {
    /// Not held in stock — a service or a dated product has no shelf, so the
    /// shop needs no reservation to sell it.
    NotStocked,
    /// Held in stock: what a buyer could take right now, in whole units —
    /// the ledger's on-hand at `stock` locations minus live holds, floored
    /// to units and never below zero.
    Stocked {
        /// Whole units a new buyer could reserve at this instant.
        available_units: i64,
    },
}

/// Where one hold stands. `Expired` is derived on read: a stored `held` row
/// whose expiry has passed already counts for nothing, whether or not
/// anything has tidied it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvStockHoldState {
    /// Counting against available-to-sell until `expires_at`.
    Held,
    /// Claimed by a finished sale — the outbound movement is recorded, and
    /// the hold no longer counts (the shelf itself dropped).
    Completed,
    /// The buyer walked away; the goods are sellable again.
    Released,
    /// The buyer never finished; the goods freed themselves at expiry.
    Expired,
}

/// One hold as the machinery reports it. No buyer fields exist to expose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvStockHold {
    pub id: InvStockHoldId,
    pub product: BillingProductId,
    /// Whole units reserved.
    pub units: i64,
    pub state: InvStockHoldState,
    pub expires_at: OffsetDateTime,
    pub created_at: OffsetDateTime,
}

#[derive(sqlx::FromRow)]
struct HoldRow {
    id: String,
    product_id: String,
    qty_milli: i64,
    state: String,
    expires_at: OffsetDateTime,
    created_at: OffsetDateTime,
}

impl HoldRow {
    fn into_hold(self, now: OffsetDateTime) -> Result<InvStockHold> {
        let state = match self.state.as_str() {
            "held" if self.expires_at <= now => InvStockHoldState::Expired,
            "held" => InvStockHoldState::Held,
            "completed" => InvStockHoldState::Completed,
            "released" => InvStockHoldState::Released,
            "expired" => InvStockHoldState::Expired,
            other => {
                return Err(StoreError::Conflict(format!(
                    "stored hold state '{other}' is not one this release knows"
                )));
            }
        };
        Ok(InvStockHold {
            id: InvStockHoldId::new(self.id),
            product: BillingProductId::new(self.product_id),
            units: self.qty_milli.div_euclid(MILLI_PER_UNIT),
            state,
            expires_at: self.expires_at,
            created_at: self.created_at,
        })
    }
}

/// The columns every read of a hold selects, in `HoldRow` order.
const HOLD_COLS: &str = "id, product_id, qty_milli, state, expires_at, created_at";

/// One writer at a time per product, for the length of the transaction: the
/// availability count and the write that follows it are one decision, and two
/// buyers must not interleave them.
async fn lock_stock_product(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &TenantId,
    product: &BillingProductId,
) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1), hashtext($2))")
        .bind(tenant.as_str())
        .bind(product.as_str())
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
    Ok(())
}

/// A write-and-reserve door onto one tenant's warehouse that can only answer
/// *what is sellable* and move goods the way a sale moves them. Open it with
/// a tenant and owner resolved from a trusted row; everything it touches is
/// scoped to that tenant, and the movements a claim records are signed by
/// that owner.
pub struct InvStockSale {
    account: AccountStore,
}

impl InvStockSale {
    /// Opens the stock-sale door of one tenant's Inventory.
    ///
    /// The caller vouches for the pair: `tenant` and `owner` must come from a
    /// row the caller already trusts (a site's own record, never a request).
    #[must_use]
    pub fn open(pool: PgPool, blobs: BlobStore, tenant: TenantId, owner: UserId) -> Self {
        Self {
            account: AccountStore {
                pool,
                blobs,
                tenant,
                user: owner,
            },
        }
    }

    /// Whether — and how much of — one product a shop could sell right now,
    /// or `None` when the id is archived, another tenant's, or was never
    /// anything (indistinguishable by design, so a stale or guessed reference
    /// shows nothing rather than leaking that a foreign id exists).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn stock_for_sale(
        &self,
        product: &BillingProductId,
        now: OffsetDateTime,
    ) -> Result<Option<StockForSale>> {
        let found: Option<(bool, i64, i64)> = sqlx::query_as(
            "SELECT p.stocked, \
                    COALESCE((SELECT SUM(s.qty_milli) FROM inv_stock s \
                        JOIN inv_locations l \
                          ON l.tenant_id = s.tenant_id AND l.id = s.location_id \
                        WHERE s.tenant_id = p.tenant_id AND s.product_id = p.id \
                          AND l.kind = 'stock'), 0)::bigint, \
                    COALESCE((SELECT SUM(h.qty_milli) FROM inv_stock_sale_holds h \
                        WHERE h.tenant_id = p.tenant_id AND h.product_id = p.id \
                          AND h.state = 'held' AND h.expires_at > $3), 0)::bigint \
             FROM billing_products p \
             WHERE p.tenant_id = $1 AND p.id = $2 AND p.archived_at IS NULL",
        )
        .bind(self.account.tenant.as_str())
        .bind(product.as_str())
        .bind(now)
        .fetch_optional(&self.account.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(found.map(|(stocked, on_hand_milli, held_milli)| {
            if stocked {
                StockForSale::Stocked {
                    available_units: available_units(on_hand_milli, held_milli),
                }
            } else {
                StockForSale::NotStocked
            }
        }))
    }

    /// Reserves `units` of a product for `ttl` from `now` — the step that
    /// comes **before** payment. The count and the insert run under the
    /// per-product advisory lock, so of two simultaneous buyers after the
    /// last unit exactly one gets it and the other is told so.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the units or the ttl are out of range,
    /// or the product is not a stocked one (a service has no shelf to reserve
    /// from); [`StoreError::NotFound`] when the product is archived or not
    /// this tenant's; [`StoreError::Conflict`] when the goods are not there —
    /// "sold out", or how many are left when some are; [`StoreError::Db`] on
    /// failure.
    pub async fn reserve(
        &self,
        product: &BillingProductId,
        units: i64,
        ttl: Duration,
        now: OffsetDateTime,
    ) -> Result<InvStockHold> {
        if !(1..=STOCK_HOLD_MAX_UNITS).contains(&units) {
            return Err(StoreError::Validation(format!(
                "a hold may cover between 1 and {STOCK_HOLD_MAX_UNITS} units"
            )));
        }
        if !(STOCK_HOLD_MIN_TTL..=STOCK_HOLD_MAX_TTL).contains(&ttl) {
            return Err(StoreError::Validation(
                "a hold must last between one minute and one hour".to_owned(),
            ));
        }
        let mut tx = self.account.pool.begin().await.map_err(StoreError::Db)?;
        lock_stock_product(&mut tx, &self.account.tenant, product).await?;
        let found: Option<(String, bool)> = sqlx::query_as(
            "SELECT name, stocked FROM billing_products \
             WHERE tenant_id = $1 AND id = $2 AND archived_at IS NULL",
        )
        .bind(self.account.tenant.as_str())
        .bind(product.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let (name, stocked) = found.ok_or(StoreError::NotFound)?;
        if !stocked {
            return Err(StoreError::Validation(format!(
                "{name} is not a stocked product, so there is no stock to reserve"
            )));
        }
        let available = self.available_in(&mut tx, product, now).await?;
        if available < units {
            return Err(StoreError::Conflict(if available <= 0 {
                "sold out".to_owned()
            } else if available == 1 {
                "only 1 is left".to_owned()
            } else {
                format!("only {available} are left")
            }));
        }
        let id = InvStockHoldId::generate();
        let expires_at = now + ttl;
        sqlx::query(
            "INSERT INTO inv_stock_sale_holds \
                (tenant_id, id, product_id, qty_milli, expires_at, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(self.account.tenant.as_str())
        .bind(id.as_str())
        .bind(product.as_str())
        .bind(units * MILLI_PER_UNIT)
        .bind(expires_at)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(InvStockHold {
            id,
            product: product.clone(),
            units,
            state: InvStockHoldState::Held,
            expires_at,
            created_at: now,
        })
    }

    /// One hold as it stands at `now`, or `None`. A stored `held` row past
    /// its expiry reads as [`InvStockHoldState::Expired`].
    ///
    /// # Errors
    /// [`StoreError::Conflict`] when the stored state is unreadable;
    /// [`StoreError::Db`] on failure.
    pub async fn stock_hold(
        &self,
        hold: &InvStockHoldId,
        now: OffsetDateTime,
    ) -> Result<Option<InvStockHold>> {
        let row = sqlx::query_as::<_, HoldRow>(&format!(
            "SELECT {HOLD_COLS} FROM inv_stock_sale_holds WHERE tenant_id = $1 AND id = $2",
        ))
        .bind(self.account.tenant.as_str())
        .bind(hold.as_str())
        .fetch_optional(&self.account.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(|row| row.into_hold(now)).transpose()
    }

    /// Gives a hold's goods back — the buyer walked away. Releasing a hold
    /// that already lapsed or was already released is a success (the goods
    /// are free either way, and a cancel button may be pressed twice); a
    /// claimed hold is a sale and cannot be released here.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the hold is not this tenant's;
    /// [`StoreError::Conflict`] when it was already claimed;
    /// [`StoreError::Db`] on failure.
    pub async fn release(
        &self,
        hold: &InvStockHoldId,
        now: OffsetDateTime,
    ) -> Result<InvStockHold> {
        let row = sqlx::query_as::<_, HoldRow>(&format!(
            "UPDATE inv_stock_sale_holds SET state = 'released' \
             WHERE tenant_id = $1 AND id = $2 AND state = 'held' \
             RETURNING {HOLD_COLS}",
        ))
        .bind(self.account.tenant.as_str())
        .bind(hold.as_str())
        .fetch_optional(&self.account.pool)
        .await
        .map_err(StoreError::Db)?;
        if let Some(row) = row {
            return row.into_hold(now);
        }
        let stood = self
            .stock_hold(hold, now)
            .await?
            .ok_or(StoreError::NotFound)?;
        match stood.state {
            InvStockHoldState::Released | InvStockHoldState::Expired => Ok(stood),
            InvStockHoldState::Completed => Err(StoreError::Conflict(
                "this purchase is complete; the goods can no longer be released".to_owned(),
            )),
            InvStockHoldState::Held => Err(StoreError::Conflict(
                "this hold could not be released; try again".to_owned(),
            )),
        }
    }

    /// Claims a hold's goods as **sold** — what the payment path calls when
    /// the money is confirmed. In one transaction, the hold is marked
    /// completed and the real outbound movement is recorded through the move
    /// ledger's own writer: reason `sale`, from the tenant's stock locations
    /// (fullest shelf first, then location id, split across shelves when one
    /// has not got it all) to their `customer` counterparty, signed by the
    /// owner this door was opened as. `note` is the caller's sentence about
    /// the sale (the shop names its order); this seam adds no words of its
    /// own.
    ///
    /// Claiming an already-claimed hold returns it unchanged and moves
    /// nothing, so a retried payment webhook is harmless. Only a live,
    /// unexpired hold can complete: a buyer whose hold lapsed mid-payment is
    /// refused here and handled by the payment flow, never given goods that
    /// may since have been sold again. If the warehouse's own doors took the
    /// goods after the hold was placed (holds bind the shop, never Inventory
    /// — see the module doc), the claim refuses cleanly and the hold stays
    /// live for a retry after restocking.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the hold is not this tenant's;
    /// [`StoreError::Conflict`] when it expired, was released, or the goods
    /// have since left stock; [`StoreError::Validation`] when the note is
    /// over the move ledger's bound; [`StoreError::Db`] on failure.
    pub async fn claim(
        &self,
        hold: &InvStockHoldId,
        note: &str,
        now: OffsetDateTime,
    ) -> Result<InvStockHold> {
        let mut tx = self.account.pool.begin().await.map_err(StoreError::Db)?;
        let row = sqlx::query_as::<_, HoldRow>(&format!(
            "SELECT {HOLD_COLS} FROM inv_stock_sale_holds WHERE tenant_id = $1 AND id = $2",
        ))
        .bind(self.account.tenant.as_str())
        .bind(hold.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let stood = row.ok_or(StoreError::NotFound)?.into_hold(now)?;
        match stood.state {
            // A retried webhook: the movement was recorded when the state
            // flipped; recording it again would sell the goods twice.
            InvStockHoldState::Completed => return Ok(stood),
            InvStockHoldState::Expired => {
                return Err(StoreError::Conflict(
                    "this hold has expired and its goods have been released".to_owned(),
                ));
            }
            InvStockHoldState::Released => {
                return Err(StoreError::Conflict("this hold was released".to_owned()));
            }
            InvStockHoldState::Held => {}
        }
        let product = stood.product.clone();
        lock_stock_product(&mut tx, &self.account.tenant, &product).await?;
        // Re-check under the lock: a racing claim of the same hold has either
        // completed it (return it unchanged) or cannot have started (we hold
        // the product lock until commit).
        let claimed = sqlx::query_as::<_, HoldRow>(&format!(
            "UPDATE inv_stock_sale_holds SET state = 'completed', completed_at = $3 \
             WHERE tenant_id = $1 AND id = $2 AND state = 'held' AND expires_at > $3 \
             RETURNING {HOLD_COLS}",
        ))
        .bind(self.account.tenant.as_str())
        .bind(hold.as_str())
        .bind(now)
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let Some(claimed) = claimed else {
            // The guarded update refused under the lock; the freshest truth
            // is another claimer finished first, or expiry passed between the
            // two reads.
            drop(tx);
            let stood = self
                .stock_hold(hold, now)
                .await?
                .ok_or(StoreError::NotFound)?;
            return match stood.state {
                InvStockHoldState::Completed => Ok(stood),
                _ => Err(StoreError::Conflict(
                    "this hold has expired and its goods have been released".to_owned(),
                )),
            };
        };
        let claimed = claimed.into_hold(now)?;

        let customer: Option<String> = sqlx::query_scalar(
            "SELECT id FROM inv_locations WHERE tenant_id = $1 AND kind = 'customer'",
        )
        .bind(self.account.tenant.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let customer = InvLocationId::new(customer.ok_or_else(|| {
            StoreError::Conflict(
                "this tenant's inventory has no customer counterparty to deliver to".to_owned(),
            )
        })?);

        // Fullest shelf first, then location id: deterministic, and the fewest
        // movements for the common one-warehouse tenant.
        let shelves: Vec<(String, i64)> = sqlx::query_as(
            "SELECT s.location_id, s.qty_milli FROM inv_stock s \
             JOIN inv_locations l ON l.tenant_id = s.tenant_id AND l.id = s.location_id \
             WHERE s.tenant_id = $1 AND s.product_id = $2 \
               AND l.kind = 'stock' AND s.qty_milli > 0 \
             ORDER BY s.qty_milli DESC, s.location_id",
        )
        .bind(self.account.tenant.as_str())
        .bind(product.as_str())
        .fetch_all(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let mut wanted = claimed.units * MILLI_PER_UNIT;
        let on_shelves: i64 = shelves.iter().map(|(_, qty)| qty).sum();
        if on_shelves < wanted {
            // The warehouse's own doors took the goods after the hold was
            // placed. Refuse whole: the transaction drops, the hold stays
            // held, and the payment flow decides what to tell the buyer.
            return Err(StoreError::Conflict(format!(
                "the goods have since left stock: {} of the reserved {} units remain",
                on_shelves.div_euclid(MILLI_PER_UNIT),
                claimed.units
            )));
        }
        for (location_id, on_shelf) in shelves {
            if wanted == 0 {
                break;
            }
            let take = wanted.min(on_shelf);
            self.account
                .record_move_in(
                    &mut tx,
                    &NewMove {
                        product_id: product.clone(),
                        from_location_id: InvLocationId::new(location_id),
                        to_location_id: customer.clone(),
                        qty_milli: take,
                        reason: MoveReason::Sale,
                        reason_code: None,
                        note: note.to_owned(),
                        reference: None,
                        occurred_at: Some(now),
                    },
                )
                .await?;
            wanted -= take;
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(claimed)
    }

    /// Available-to-sell in whole units, inside the caller's transaction so
    /// it can sit under the advisory lock: the ledger's on-hand at `stock`
    /// locations minus live holds, floored to units and never below zero.
    async fn available_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        product: &BillingProductId,
        now: OffsetDateTime,
    ) -> Result<i64> {
        let (on_hand_milli, held_milli): (i64, i64) = sqlx::query_as(
            "SELECT COALESCE((SELECT SUM(s.qty_milli) FROM inv_stock s \
                        JOIN inv_locations l \
                          ON l.tenant_id = s.tenant_id AND l.id = s.location_id \
                        WHERE s.tenant_id = $1 AND s.product_id = $2 \
                          AND l.kind = 'stock'), 0)::bigint, \
                    COALESCE((SELECT SUM(h.qty_milli) FROM inv_stock_sale_holds h \
                        WHERE h.tenant_id = $1 AND h.product_id = $2 \
                          AND h.state = 'held' AND h.expires_at > $3), 0)::bigint",
        )
        .bind(self.account.tenant.as_str())
        .bind(product.as_str())
        .bind(now)
        .fetch_one(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        Ok(available_units(on_hand_milli, held_milli))
    }
}

/// The availability arithmetic, named once: milli on hand minus milli held,
/// floored to whole units and never below zero. Pure, so the flooring rule
/// (2.5 on the shelf sells as 2) is unit-tested without a database.
fn available_units(on_hand_milli: i64, held_milli: i64) -> i64 {
    (on_hand_milli - held_milli)
        .div_euclid(MILLI_PER_UNIT)
        .max(0)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn row(state: &str, expires_in_seconds: i64) -> HoldRow {
        HoldRow {
            id: "hold".to_owned(),
            product_id: "prod".to_owned(),
            qty_milli: 3_000,
            state: state.to_owned(),
            expires_at: OffsetDateTime::UNIX_EPOCH + Duration::seconds(expires_in_seconds),
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn a_stored_hold_past_its_expiry_reads_as_expired() {
        let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(100);
        let live = row("held", 101).into_hold(now).unwrap();
        assert_eq!(live.state, InvStockHoldState::Held);
        assert_eq!(live.units, 3, "milli-units read back as whole units");
        let lapsed = row("held", 100).into_hold(now).unwrap();
        assert_eq!(lapsed.state, InvStockHoldState::Expired);
        let tidied = row("expired", 100).into_hold(now).unwrap();
        assert_eq!(tidied.state, InvStockHoldState::Expired);
    }

    #[test]
    fn terminal_states_read_as_themselves_and_unknown_words_refuse() {
        let now = OffsetDateTime::UNIX_EPOCH;
        assert_eq!(
            row("completed", -1).into_hold(now).unwrap().state,
            InvStockHoldState::Completed
        );
        assert_eq!(
            row("released", -1).into_hold(now).unwrap().state,
            InvStockHoldState::Released
        );
        assert!(matches!(
            row("bartered", 1).into_hold(now),
            Err(StoreError::Conflict(_))
        ));
    }

    #[test]
    fn availability_floors_to_whole_units_and_never_goes_negative() {
        // A full shelf sells whole.
        assert_eq!(available_units(12_000, 0), 12);
        // 2.5 on the shelf sells as 2 — a shop sells books, not halves.
        assert_eq!(available_units(2_500, 0), 2);
        // Holds subtract before flooring: 2.5 on hand with 1 held is 1.5 → 1.
        assert_eq!(available_units(2_500, 1_000), 1);
        // Goods left through the warehouse door under a live hold: never a
        // negative offer, just nothing to sell.
        assert_eq!(available_units(1_000, 3_000), 0);
    }
}
