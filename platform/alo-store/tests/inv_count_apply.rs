//! **Applying a stocktake** against the real database (alo Inventory, wave
//! B5.08b).
//!
//! [`alo_store::inv_count_apply`]'s unit tests already prove the decision — what
//! a variance is measured against, and which rows are skipped. What only a
//! database can prove is that the decision becomes the ledger, once, and only
//! ever this tenant's:
//!
//! | Property | Where |
//! |---|---|
//! | a loss and a surplus become movements, and the shelf ends where it was counted | `the_variances_become_movements_and_the_shelf_ends_where_it_was_counted` |
//! | a row whose shelf moved underneath it is skipped, not written | `a_shelf_that_moved_underneath_the_counter_is_left_alone` |
//! | uncounted and agreeing rows write nothing | `an_uncounted_row_and_an_agreeing_row_move_no_goods` |
//! | a count is applied exactly once, and closes | `a_count_is_applied_once_and_then_it_is_a_record` |
//! | an untouched sheet cannot be applied | `a_sheet_nobody_counted_is_not_an_apply` |
//! | applying frees the place for the next count | `applying_frees_the_shelf_to_be_counted_again` |
//! | **no path of an apply ever reaches another tenant** | `one_tenants_apply_never_touches_anothers_stock` |
//!
//! The second is the one the design note is really about: applying a frozen
//! difference over a delivery that went out at the far end of the room would
//! silently erase the delivery.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::inv_count::{CountEntry, CountStatus, NewCount};
use alo_store::inv_count_apply::SkipReason;
use alo_store::inv_locations::{Location, LocationKind, LocationSeed};
use alo_store::inv_moves::{MoveFilter, MoveReason, MoveRefKind, NewMove};
use alo_store::{
    AccountStore, BillingProductId, InvCountId, InvLocationId, NewProduct, Store, StoreError,
    TenantId,
};

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

fn conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(message)) => message,
        other => panic!("expected Conflict, got: {other:?}"),
    }
}

fn seed_names() -> LocationSeed {
    LocationSeed {
        stock: "Hoofdmagazijn".to_owned(),
        supplier: "Leveranciers".to_owned(),
        customer: "Klanten".to_owned(),
        adjustment: "Correcties".to_owned(),
        production: "Productie".to_owned(),
    }
}

/// A tenant with one warehouse, two stocked products, and five chairs on the
/// shelf.
struct Warehouse {
    door: AccountStore,
    tenant: TenantId,
    chair: BillingProductId,
    desk: BillingProductId,
    warehouse: InvLocationId,
    adjustment: InvLocationId,
    supplier_location: InvLocationId,
}

impl Warehouse {
    async fn open(store: &Store, tag: &str) -> Self {
        let tenant = store.create_tenant(&format!("apply-{tag}")).await.unwrap();
        let user = store
            .for_tenant(tenant.clone())
            .create_user(&format!("{tag}@apply.test"))
            .await
            .unwrap();
        let door = store.for_account(tenant.clone(), user);
        let seeded = door
            .inv_locations_or_seed(&seed_names(), false)
            .await
            .unwrap();
        let of = |kind: LocationKind| -> InvLocationId {
            seeded
                .iter()
                .find(|l: &&Location| l.kind == kind)
                .unwrap_or_else(|| panic!("the seed must write a {kind:?} location"))
                .id
                .clone()
        };
        let product = |name: &str| NewProduct {
            name: name.to_owned(),
            unit: "piece".to_owned(),
            unit_price_cents: 8_600,
            vat_rate_bp: 1900,
            stocked: true,
            purchase_price_cents: 4_300,
            ..Default::default()
        };
        let chair = door
            .create_billing_product(&product("Blue chair"))
            .await
            .unwrap();
        let desk = door
            .create_billing_product(&product("Oak desk"))
            .await
            .unwrap();
        let this = Self {
            warehouse: of(LocationKind::Stock),
            adjustment: of(LocationKind::Adjustment),
            supplier_location: of(LocationKind::Supplier),
            chair,
            desk,
            door,
            tenant,
        };
        this.receive(&this.chair, 5_000).await;
        this
    }

    /// Puts `qty_milli` of a product on the warehouse shelf, the way a receipt
    /// does.
    async fn receive(&self, product: &BillingProductId, qty_milli: i64) {
        self.door
            .record_move(&NewMove {
                product_id: product.clone(),
                from_location_id: self.supplier_location.clone(),
                to_location_id: self.warehouse.clone(),
                qty_milli,
                reason: MoveReason::Purchase,
                reason_code: None,
                note: String::new(),
                reference: None,
                occurred_at: None,
            })
            .await
            .unwrap();
    }

    async fn count(&self) -> InvCountId {
        self.door
            .open_inv_count(&NewCount {
                location_id: self.warehouse.clone(),
                note: "Tuesday, back shelves".to_owned(),
            })
            .await
            .unwrap()
    }

    /// Records a finding against one row of the sheet.
    async fn record(
        &self,
        id: &InvCountId,
        product: &BillingProductId,
        qty_milli: i64,
        note: &str,
    ) {
        self.door
            .set_inv_count_line(
                id,
                product,
                &CountEntry {
                    counted_qty_milli: Some(qty_milli),
                    note: note.to_owned(),
                },
            )
            .await
            .unwrap();
    }

    async fn on_hand(&self, product: &BillingProductId) -> i64 {
        self.door
            .inv_on_hand(product, &self.warehouse)
            .await
            .unwrap()
    }

    /// Every movement of one product, newest first.
    async fn moves(&self, product: &BillingProductId) -> Vec<alo_store::inv_moves::Move> {
        self.door
            .inv_moves(&MoveFilter {
                product_id: Some(product.clone()),
                ..Default::default()
            })
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn the_variances_become_movements_and_the_shelf_ends_where_it_was_counted() {
    let store = common::test_store().await;
    let a = Warehouse::open(&store, "movements").await;
    let id = a.count().await;

    // One chair short of the five the ledger believed, and three desks nobody
    // knew were there at all.
    a.record(&id, &a.chair, 4_000, "one broken").await;
    a.record(&id, &a.desk, 3_000, "behind the pallets").await;

    let outcome = a.door.apply_inv_count(&id).await.unwrap();
    assert!(
        outcome.skipped.is_empty(),
        "both rows were counted and both disagreed: {:?}",
        outcome.skipped
    );
    assert_eq!(outcome.applied.len(), 2);

    let chair = outcome
        .applied
        .iter()
        .find(|l| l.product_id == a.chair)
        .expect("the chair was corrected");
    assert_eq!(chair.product_name, "Blue chair");
    assert_eq!(chair.on_hand_qty_milli, 5_000);
    assert_eq!(chair.counted_qty_milli, 4_000);
    assert_eq!(chair.variance_qty_milli, -1_000);
    let desk = outcome
        .applied
        .iter()
        .find(|l| l.product_id == a.desk)
        .expect("the desks were found");
    assert_eq!(desk.variance_qty_milli, 3_000);

    // **The shelf now says what the person counted.** That is the whole point.
    assert_eq!(a.on_hand(&a.chair).await, 4_000);
    assert_eq!(a.on_hand(&a.desk).await, 3_000);

    // And it says it as movements, with the direction, the reason and the
    // document that explains them — never as an edited quantity.
    let loss = a.moves(&a.chair).await;
    assert_eq!(loss.len(), 2, "the receipt, and now the correction");
    let loss = &loss[0];
    assert_eq!(loss.qty_milli, 1_000, "direction is the pair of locations");
    assert_eq!(loss.from_location_id, a.warehouse);
    assert_eq!(loss.to_location_id, a.adjustment);
    assert_eq!(loss.reason, MoveReason::Count);
    assert_eq!(
        loss.reason_code, None,
        "the reason code is the manual door's; a count is explained by its sheet"
    );
    assert_eq!(
        loss.note, "one broken",
        "what the counter wrote travels onto the movement it became"
    );
    let reference = loss.reference.as_ref().expect("a movement from a document");
    assert_eq!(reference.kind, MoveRefKind::Count);
    assert_eq!(reference.id, id.as_str());
    assert_eq!(loss.id, chair.move_id);

    let surplus = a.moves(&a.desk).await;
    assert_eq!(surplus.len(), 1);
    assert_eq!(surplus[0].from_location_id, a.adjustment);
    assert_eq!(surplus[0].to_location_id, a.warehouse);
    assert_eq!(surplus[0].qty_milli, 3_000);

    // The count is closed, and says who closed it.
    assert_eq!(outcome.count.status, CountStatus::Applied);
    assert!(outcome.count.closed_at.is_some());
    assert!(outcome.count.closed_by.is_some());

    // The cache and the fold still agree: the apply wrote through the ledger's
    // one door and invented nothing.
    let folded = a.door.inv_stock_folded().await.unwrap();
    let cached = a.door.inv_stock_cached().await.unwrap();
    assert_eq!(folded, cached);
}

#[tokio::test]
async fn a_shelf_that_moved_underneath_the_counter_is_left_alone() {
    let store = common::test_store().await;
    let a = Warehouse::open(&store, "moved").await;
    let id = a.count().await;
    a.record(&id, &a.chair, 4_000, "").await;

    // A delivery lands at the far end of the room while the sheet is being
    // worked down. Applying the recorded difference would erase it.
    a.receive(&a.chair, 2_000).await;
    assert_eq!(a.on_hand(&a.chair).await, 7_000);

    let outcome = a.door.apply_inv_count(&id).await.unwrap();
    assert!(outcome.applied.is_empty());
    assert_eq!(outcome.skipped.len(), 1);
    let skipped = &outcome.skipped[0];
    assert_eq!(skipped.product_id, a.chair);
    assert_eq!(skipped.reason, SkipReason::Moved);
    assert_eq!(skipped.expected_qty_milli, 5_000);
    assert_eq!(skipped.counted_qty_milli, Some(4_000));
    assert_eq!(
        skipped.on_hand_qty_milli, 7_000,
        "the report says what is there now, so the person can re-count that row"
    );
    assert_eq!(
        a.on_hand(&a.chair).await,
        7_000,
        "the delivery survived the stocktake"
    );
    assert_eq!(
        a.moves(&a.chair).await.len(),
        2,
        "no correction was written"
    );
    // The count is still closed: it happened, and what it did is the report.
    assert_eq!(outcome.count.status, CountStatus::Applied);
}

#[tokio::test]
async fn an_uncounted_row_and_an_agreeing_row_move_no_goods() {
    let store = common::test_store().await;
    let a = Warehouse::open(&store, "quiet").await;
    // Two desks as well, so the sheet has a row nobody will reach.
    a.receive(&a.desk, 2_000).await;
    let id = a.count().await;

    // The chairs are counted and they are right; nobody gets to the desks.
    a.record(&id, &a.chair, 5_000, "").await;

    let outcome = a.door.apply_inv_count(&id).await.unwrap();
    assert!(outcome.applied.is_empty());
    assert_eq!(outcome.skipped.len(), 2);
    let reason_for = |product: &BillingProductId| {
        outcome
            .skipped
            .iter()
            .find(|l| &l.product_id == product)
            .unwrap_or_else(|| panic!("no report for {product:?}"))
            .reason
    };
    assert_eq!(reason_for(&a.chair), SkipReason::Unchanged);
    assert_eq!(
        reason_for(&a.desk),
        SkipReason::Uncounted,
        "'nobody got to this shelf' is never written off as 'there are none'"
    );
    assert_eq!(a.on_hand(&a.chair).await, 5_000);
    assert_eq!(
        a.on_hand(&a.desk).await,
        2_000,
        "the desks nobody counted are exactly as they were"
    );
    assert_eq!(a.moves(&a.desk).await.len(), 1, "only the receipt");
}

#[tokio::test]
async fn a_count_is_applied_once_and_then_it_is_a_record() {
    let store = common::test_store().await;
    let a = Warehouse::open(&store, "once").await;
    let id = a.count().await;
    a.record(&id, &a.chair, 4_000, "").await;
    a.door.apply_inv_count(&id).await.unwrap();

    // A second press writes nothing: one afternoon's variance cannot be booked
    // into the ledger twice.
    assert!(
        conflict(a.door.apply_inv_count(&id).await).contains("applied"),
        "the refusal names the state it is in"
    );
    assert_eq!(a.on_hand(&a.chair).await, 4_000);
    assert_eq!(a.moves(&a.chair).await.len(), 2);

    // And an applied sheet is a record: no more counting, no more editing, no
    // cancelling it after the fact.
    assert!(
        conflict(
            a.door
                .set_inv_count_line(
                    &id,
                    &a.chair,
                    &CountEntry {
                        counted_qty_milli: Some(1_000),
                        note: String::new(),
                    },
                )
                .await
        )
        .contains("applied")
    );
    assert!(
        conflict(a.door.update_inv_count_note(&id, "second thoughts").await).contains("applied")
    );
    assert!(conflict(a.door.cancel_inv_count(&id).await).contains("applied"));

    // A cancelled count is not applied either — the other terminal state.
    let walked_away = a.count().await;
    a.record(&walked_away, &a.chair, 3_000, "").await;
    a.door.cancel_inv_count(&walked_away).await.unwrap();
    assert!(conflict(a.door.apply_inv_count(&walked_away).await).contains("cancelled"));
    assert_eq!(a.on_hand(&a.chair).await, 4_000);
}

#[tokio::test]
async fn a_sheet_nobody_counted_is_not_an_apply() {
    let store = common::test_store().await;
    let a = Warehouse::open(&store, "untouched").await;
    let id = a.count().await;

    // Closing a fresh sheet as `applied` would leave a stocktake claiming to
    // have happened. The act meant is `cancel`, and the refusal says so.
    let message = conflict(a.door.apply_inv_count(&id).await);
    assert!(message.contains("counted"), "{message}");
    assert!(message.contains("cancel"), "{message}");
    assert_eq!(
        a.door.inv_count(&id).await.unwrap().map(|c| c.status),
        Some(CountStatus::Open),
        "and the sheet is still there to be counted"
    );
}

#[tokio::test]
async fn applying_frees_the_shelf_to_be_counted_again() {
    let store = common::test_store().await;
    let a = Warehouse::open(&store, "again").await;
    let first = a.count().await;
    a.record(&first, &a.chair, 4_000, "").await;
    a.door.apply_inv_count(&first).await.unwrap();

    // A shelf can be counted every week forever: only an OPEN count holds the
    // place.
    let second = a.count().await;
    assert_ne!(second, first);
    let sheet = a.door.inv_count_sheet(&second).await.unwrap();
    assert_eq!(sheet.len(), 1);
    assert_eq!(
        sheet[0].expected_qty_milli, 4_000,
        "the new sheet snapshots the shelf the last count corrected"
    );
    assert!(!sheet[0].moved_since);
}

#[tokio::test]
async fn one_tenants_apply_never_touches_anothers_stock() {
    let store = common::test_store().await;
    let a = Warehouse::open(&store, "ours").await;
    let b = Warehouse::open(&store, "theirs").await;

    let id = a.count().await;
    a.record(&id, &a.chair, 4_000, "one broken").await;

    // Their apply of our count is not a refusal about a count they may not see:
    // it is a bare NotFound, the same answer a count that never existed gets.
    assert_not_found(b.door.apply_inv_count(&id).await);
    // Including for a tenant who has never opened Inventory at all: the order
    // of refusals is a tenancy rule, so "you have no adjustment location" must
    // never be said about somebody else's stocktake — that would confirm the
    // count was worth looking at.
    let bare_tenant = store.create_tenant("apply-bare").await.unwrap();
    let bare_user = store
        .for_tenant(bare_tenant.clone())
        .create_user("bare@apply.test")
        .await
        .unwrap();
    assert_not_found(
        store
            .for_account(bare_tenant, bare_user)
            .apply_inv_count(&id)
            .await,
    );
    // Nor by way of a count of their own naming our shelf.
    assert_not_found(
        b.door
            .open_inv_count(&NewCount {
                location_id: a.warehouse.clone(),
                note: String::new(),
            })
            .await,
    );

    // Our shelf, our count and our ledger are untouched by any of it.
    assert_eq!(a.on_hand(&a.chair).await, 5_000);
    assert_eq!(a.moves(&a.chair).await.len(), 1);
    assert_eq!(
        a.door.inv_count(&id).await.unwrap().map(|c| c.status),
        Some(CountStatus::Open)
    );

    // And when we apply it, nothing of theirs moves.
    a.door.apply_inv_count(&id).await.unwrap();
    assert_eq!(a.on_hand(&a.chair).await, 4_000);
    assert_eq!(
        b.on_hand(&b.chair).await,
        5_000,
        "their shelf is theirs: our stocktake said nothing about it"
    );
    assert_eq!(b.moves(&b.chair).await.len(), 1, "only their own receipt");
    assert!(
        b.door
            .inv_counts(&Default::default())
            .await
            .unwrap()
            .is_empty(),
        "their stocktake list is theirs, and they have opened none"
    );

    // A colleague in our tenant sees the applied count, as they saw the sheet.
    let colleague_user = store
        .for_tenant(a.tenant.clone())
        .create_user("co@apply.test")
        .await
        .unwrap();
    let colleague = store.for_account(a.tenant.clone(), colleague_user);
    assert_eq!(
        colleague.inv_count(&id).await.unwrap().map(|c| c.status),
        Some(CountStatus::Applied)
    );
}
