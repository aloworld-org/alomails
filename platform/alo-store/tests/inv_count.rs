//! **The stocktake** against the real database (alo Inventory, wave B5.08a).
//!
//! [`alo_store::inv_count`]'s unit tests already prove the pure parts — what a
//! variance is, what a counted quantity may be, which statuses exist. What only
//! a database can prove is what this suite is for:
//!
//! | Property | Where |
//! |---|---|
//! | opening a count snapshots exactly what is on that shelf | `a_sheet_snapshots_what_is_on_that_shelf_and_nothing_else` |
//! | counting a row records a variance, and can be undone | `counting_a_row_records_a_variance_and_can_be_undone` |
//! | something the sheet did not expect joins it when scanned | `a_product_the_sheet_did_not_expect_joins_it_when_scanned` |
//! | the sheet says when the shelf moved underneath the counter | `the_sheet_says_when_the_shelf_moved_underneath_the_counter` |
//! | one open count per place, and writes only while open | `one_count_per_place_and_lines_only_while_it_is_open` |
//! | a count needs a real, unarchived shelf and stocked goods | `a_count_needs_a_real_shelf_and_things_that_have_a_quantity` |
//! | **no path of a count ever reaches another tenant** | `one_tenants_stocktake_is_never_anothers` |
//!
//! The fourth is the one the design note is really about: a warehouse does not
//! stop while it is counted, so the sheet has to be honest about the difference
//! between what it wrote down and what is true now.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::inv_count::{CountEntry, CountFilter, CountLine, CountStatus, NewCount};
use alo_store::inv_locations::{Location, LocationKind, LocationSeed, NewLocation};
use alo_store::inv_moves::{MoveReason, NewMove};
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

fn invalid<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(message)) => message,
        other => panic!("expected Validation, got: {other:?}"),
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

/// A tenant with two real places, three products (two stocked, one a service),
/// and five chairs on the main shelf.
struct Counting {
    door: AccountStore,
    tenant: TenantId,
    chair: BillingProductId,
    desk: BillingProductId,
    service: BillingProductId,
    warehouse: InvLocationId,
    shop: InvLocationId,
    supplier_location: InvLocationId,
}

impl Counting {
    async fn open(store: &Store, tag: &str) -> Self {
        let tenant = store.create_tenant(&format!("count-{tag}")).await.unwrap();
        let user = store
            .for_tenant(tenant.clone())
            .create_user(&format!("{tag}@count.test"))
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
        let shop = door
            .create_inv_location(&NewLocation {
                code: "SHOP".to_owned(),
                name: "Winkel".to_owned(),
                kind: LocationKind::Stock,
            })
            .await
            .unwrap();
        let product = |name: &str, stocked: bool| NewProduct {
            name: name.to_owned(),
            unit: "piece".to_owned(),
            unit_price_cents: 8_600,
            vat_rate_bp: 1900,
            stocked,
            purchase_price_cents: 4_300,
            ..Default::default()
        };
        let chair = door
            .create_billing_product(&product("Blue chair", true))
            .await
            .unwrap();
        let desk = door
            .create_billing_product(&product("Oak desk", true))
            .await
            .unwrap();
        let service = door
            .create_billing_product(&product("Assembly hour", false))
            .await
            .unwrap();
        let this = Self {
            warehouse: of(LocationKind::Stock),
            supplier_location: of(LocationKind::Supplier),
            shop,
            chair,
            desk,
            service,
            door,
            tenant,
        };
        this.receive(&this.chair, &this.warehouse, 5_000).await;
        this
    }

    /// Puts `qty_milli` of a product on a shelf, the way a receipt does.
    async fn receive(&self, product: &BillingProductId, to: &InvLocationId, qty_milli: i64) {
        self.door
            .record_move(&NewMove {
                product_id: product.clone(),
                from_location_id: self.supplier_location.clone(),
                to_location_id: to.clone(),
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

    /// Opens a stocktake of the main warehouse.
    async fn count_warehouse(&self) -> InvCountId {
        self.door
            .open_inv_count(&NewCount {
                location_id: self.warehouse.clone(),
                note: "Tuesday, back shelves".to_owned(),
            })
            .await
            .unwrap()
    }

    async fn sheet(&self, id: &InvCountId) -> Vec<CountLine> {
        self.door.inv_count_sheet(id).await.unwrap()
    }

    /// The one row of the sheet for a product, or a failure naming what the
    /// sheet held instead.
    async fn row(&self, id: &InvCountId, product: &BillingProductId) -> CountLine {
        let sheet = self.sheet(id).await;
        sheet
            .iter()
            .find(|line| &line.product_id == product)
            .unwrap_or_else(|| panic!("no line for {product:?}: {sheet:?}"))
            .clone()
    }
}

#[tokio::test]
async fn a_sheet_snapshots_what_is_on_that_shelf_and_nothing_else() {
    let store = common::test_store().await;
    let a = Counting::open(&store, "snapshot").await;
    // Two desks in the shop, and an hour of labour that is not a thing at all.
    a.receive(&a.desk, &a.shop, 2_000).await;

    let id = a.count_warehouse().await;
    let sheet = a.sheet(&id).await;
    assert_eq!(
        sheet.len(),
        1,
        "only what is on THIS shelf: the desks are in the shop and the service has no shelf at \
         all — {sheet:?}"
    );
    let chair = &sheet[0];
    assert_eq!(chair.product_id, a.chair);
    assert_eq!(chair.product_name, "Blue chair");
    assert_eq!(chair.unit, "piece");
    assert_eq!(chair.expected_qty_milli, 5_000);
    assert_eq!(chair.on_hand_qty_milli, 5_000);
    assert!(!chair.moved_since);
    assert_eq!(
        chair.counted_qty_milli, None,
        "a fresh sheet has claimed nothing about anything"
    );
    assert_eq!(chair.variance_qty_milli, None);

    let count = a.door.inv_count(&id).await.unwrap().unwrap();
    assert_eq!(count.status, CountStatus::Open);
    assert_eq!(count.location_code, "MAIN");
    assert_eq!(count.location_name, "Hoofdmagazijn");
    assert_eq!(count.note, "Tuesday, back shelves");
    assert_eq!(count.line_count, 1);
    assert_eq!(count.counted_count, 0);
    assert_eq!(count.variance_count, 0);
    assert!(count.closed_at.is_none() && count.closed_by.is_none());

    // The shop's own count is its own sheet.
    let shop_count = a
        .door
        .open_inv_count(&NewCount {
            location_id: a.shop.clone(),
            note: String::new(),
        })
        .await
        .unwrap();
    let shop_sheet = a.sheet(&shop_count).await;
    assert_eq!(shop_sheet.len(), 1);
    assert_eq!(shop_sheet[0].product_id, a.desk);
    assert_eq!(shop_sheet[0].expected_qty_milli, 2_000);

    // Both are listed, newest first, and a filter narrows to one place.
    let all = a.door.inv_counts(&CountFilter::default()).await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].id, shop_count, "newest first");
    let here = a
        .door
        .inv_counts(&CountFilter {
            location_id: Some(a.warehouse.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(here.len(), 1);
    assert_eq!(here[0].id, id);
    assert_eq!(
        a.door
            .inv_counts(&CountFilter {
                status: Some(CountStatus::Applied),
                ..Default::default()
            })
            .await
            .unwrap()
            .len(),
        0,
        "nothing has been applied: B5.08b writes the movements"
    );
}

#[tokio::test]
async fn counting_a_row_records_a_variance_and_can_be_undone() {
    let store = common::test_store().await;
    let a = Counting::open(&store, "variance").await;
    let id = a.count_warehouse().await;

    // Four on the shelf where the system expected five.
    let line = a
        .door
        .set_inv_count_line(
            &id,
            &a.chair,
            &CountEntry {
                counted_qty_milli: Some(4_000),
                note: "  one broken  ".to_owned(),
            },
        )
        .await
        .unwrap();
    assert_eq!(line.expected_qty_milli, 5_000);
    assert_eq!(line.counted_qty_milli, Some(4_000));
    assert_eq!(line.variance_qty_milli, Some(-1_000));
    assert_eq!(line.note, "one broken");
    assert!(line.counted_at.is_some() && line.counted_by.is_some());

    let count = a.door.inv_count(&id).await.unwrap().unwrap();
    assert_eq!(count.counted_count, 1);
    assert_eq!(count.variance_count, 1);

    // Counting again overwrites rather than accumulates — a scanner that fires
    // twice records one row.
    let recount = a
        .door
        .set_inv_count_line(
            &id,
            &a.chair,
            &CountEntry {
                counted_qty_milli: Some(5_000),
                note: String::new(),
            },
        )
        .await
        .unwrap();
    assert_eq!(recount.counted_qty_milli, Some(5_000));
    assert_eq!(recount.variance_qty_milli, Some(0));
    assert_eq!(recount.note, "");
    assert_eq!(a.sheet(&id).await.len(), 1, "one row, not two");
    let count = a.door.inv_count(&id).await.unwrap().unwrap();
    assert_eq!(count.counted_count, 1);
    assert_eq!(
        count.variance_count, 0,
        "agreement is a counted row with nothing to correct"
    );

    // Counting zero is a claim — the strongest one a stocktake makes.
    let none_left = a
        .door
        .set_inv_count_line(
            &id,
            &a.chair,
            &CountEntry {
                counted_qty_milli: Some(0),
                note: String::new(),
            },
        )
        .await
        .unwrap();
    assert_eq!(none_left.counted_qty_milli, Some(0));
    assert_eq!(none_left.variance_qty_milli, Some(-5_000));

    // …and clearing it is the undo of a mis-scan, NOT a count of zero.
    let cleared = a
        .door
        .set_inv_count_line(&id, &a.chair, &CountEntry::default())
        .await
        .unwrap();
    assert_eq!(cleared.counted_qty_milli, None);
    assert_eq!(cleared.variance_qty_milli, None);
    assert!(cleared.counted_at.is_none() && cleared.counted_by.is_none());
    assert_eq!(
        cleared.expected_qty_milli, 5_000,
        "and the snapshot survives every re-count"
    );
    let count = a.door.inv_count(&id).await.unwrap().unwrap();
    assert_eq!(count.counted_count, 0);
    assert_eq!(count.variance_count, 0);

    // The note the counter writes is bounded, and a negative finding refused.
    for bad in [-1, 1_000_000_001] {
        assert!(
            invalid(
                a.door
                    .set_inv_count_line(
                        &id,
                        &a.chair,
                        &CountEntry {
                            counted_qty_milli: Some(bad),
                            note: String::new(),
                        },
                    )
                    .await
            )
            .contains("counted quantity")
        );
    }
    assert!(
        invalid(
            a.door
                .set_inv_count_line(
                    &id,
                    &a.chair,
                    &CountEntry {
                        counted_qty_milli: Some(1_000),
                        note: "x".repeat(501),
                    },
                )
                .await
        )
        .contains("note"),
        "and a refused write leaves the row exactly as it was"
    );
    assert_eq!(a.row(&id, &a.chair).await.counted_qty_milli, None);
}

#[tokio::test]
async fn a_product_the_sheet_did_not_expect_joins_it_when_scanned() {
    let store = common::test_store().await;
    let a = Counting::open(&store, "surplus").await;
    let id = a.count_warehouse().await;
    assert_eq!(a.sheet(&id).await.len(), 1, "no desks are expected here");

    // A box of desks turns up on the warehouse shelf that the ledger knows
    // nothing about. Scanning it adds the row.
    let found = a
        .door
        .set_inv_count_line(
            &id,
            &a.desk,
            &CountEntry {
                counted_qty_milli: Some(3_000),
                note: "behind the pallets".to_owned(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        found.expected_qty_milli, 0,
        "the ledger says none are here, and that is exactly the surplus"
    );
    assert_eq!(found.variance_qty_milli, Some(3_000));
    assert!(!found.moved_since);

    let sheet = a.sheet(&id).await;
    assert_eq!(sheet.len(), 2);
    assert_eq!(sheet[0].product_name, "Blue chair", "product-name order");
    assert_eq!(sheet[1].product_name, "Oak desk");
    let count = a.door.inv_count(&id).await.unwrap().unwrap();
    assert_eq!(count.line_count, 2);
    assert_eq!(count.variance_count, 1);
}

#[tokio::test]
async fn the_sheet_says_when_the_shelf_moved_underneath_the_counter() {
    let store = common::test_store().await;
    let a = Counting::open(&store, "moved").await;
    let id = a.count_warehouse().await;
    a.door
        .set_inv_count_line(
            &id,
            &a.chair,
            &CountEntry {
                counted_qty_milli: Some(4_000),
                note: String::new(),
            },
        )
        .await
        .unwrap();

    // Two more chairs arrive at the far end of the room while the count is
    // being worked down. The snapshot does not move — it is a reading of a
    // moment — and the row says so, which is what stops B5.08b writing a
    // difference that would erase the delivery.
    a.receive(&a.chair, &a.warehouse, 2_000).await;
    let line = a.row(&id, &a.chair).await;
    assert_eq!(line.expected_qty_milli, 5_000);
    assert_eq!(line.on_hand_qty_milli, 7_000);
    assert!(
        line.moved_since,
        "the counter must be told to re-count this one rather than lose the delivery"
    );
    assert_eq!(
        line.variance_qty_milli,
        Some(-1_000),
        "the variance against the snapshot is still stated; it is simply not the authority"
    );
}

#[tokio::test]
async fn one_count_per_place_and_lines_only_while_it_is_open() {
    let store = common::test_store().await;
    let a = Counting::open(&store, "lifecycle").await;
    let id = a.count_warehouse().await;

    // Two people counting one shelf produce two truths.
    let message = conflict(
        a.door
            .open_inv_count(&NewCount {
                location_id: a.warehouse.clone(),
                note: String::new(),
            })
            .await,
    );
    assert!(message.contains("already has a count open"), "{message}");
    // Another shelf is another count, and is fine.
    a.door
        .open_inv_count(&NewCount {
            location_id: a.shop.clone(),
            note: String::new(),
        })
        .await
        .unwrap();

    a.door
        .update_inv_count_note(&id, "back shelves")
        .await
        .unwrap();
    assert_eq!(
        a.door.inv_count(&id).await.unwrap().unwrap().note,
        "back shelves"
    );

    a.door.cancel_inv_count(&id).await.unwrap();
    let cancelled = a.door.inv_count(&id).await.unwrap().unwrap();
    assert_eq!(cancelled.status, CountStatus::Cancelled);
    assert!(cancelled.closed_at.is_some());
    assert!(cancelled.closed_by.is_some());
    assert_eq!(
        cancelled.line_count, 1,
        "the sheet is kept exactly as it was; only the ledger was left alone"
    );

    // Nothing further may be written to it, and it is not reopened.
    for message in [
        conflict(
            a.door
                .set_inv_count_line(
                    &id,
                    &a.chair,
                    &CountEntry {
                        counted_qty_milli: Some(4_000),
                        note: String::new(),
                    },
                )
                .await,
        ),
        conflict(a.door.update_inv_count_note(&id, "again").await),
        conflict(a.door.cancel_inv_count(&id).await),
    ] {
        assert!(message.contains("cancelled"), "{message}");
    }

    // Cancelling frees the shelf: it can be counted again from scratch.
    let again = a.count_warehouse().await;
    assert_ne!(again, id);
    assert_eq!(a.sheet(&again).await[0].expected_qty_milli, 5_000);
}

#[tokio::test]
async fn a_count_needs_a_real_shelf_and_things_that_have_a_quantity() {
    let store = common::test_store().await;
    let a = Counting::open(&store, "refusals").await;

    // A counterparty is not a shelf: its balance is negative by construction.
    let message = invalid(
        a.door
            .open_inv_count(&NewCount {
                location_id: a.supplier_location.clone(),
                note: String::new(),
            })
            .await,
    );
    assert!(message.contains("real stock location"), "{message}");
    // An id nobody owns says nothing about whose it might be.
    assert_not_found(
        a.door
            .open_inv_count(&NewCount {
                location_id: InvLocationId::new("nope"),
                note: String::new(),
            })
            .await,
    );
    // A note is bounded before anything is written.
    assert!(
        invalid(
            a.door
                .open_inv_count(&NewCount {
                    location_id: a.warehouse.clone(),
                    note: "x".repeat(501),
                })
                .await
        )
        .contains("note")
    );

    // A shelf being emptied on purpose is not counted.
    let shed = a
        .door
        .create_inv_location(&NewLocation {
            code: "SHED".to_owned(),
            name: "Schuur".to_owned(),
            kind: LocationKind::Stock,
        })
        .await
        .unwrap();
    a.door.set_inv_location_archived(&shed, true).await.unwrap();
    assert!(
        conflict(
            a.door
                .open_inv_count(&NewCount {
                    location_id: shed,
                    note: String::new(),
                })
                .await
        )
        .contains("archived")
    );

    let id = a.count_warehouse().await;
    // An hour of labour has no shelf, so it cannot be found on one.
    assert!(
        invalid(
            a.door
                .set_inv_count_line(
                    &id,
                    &a.service,
                    &CountEntry {
                        counted_qty_milli: Some(1_000),
                        note: String::new(),
                    },
                )
                .await
        )
        .contains("not a stocked product")
    );
    assert_not_found(
        a.door
            .set_inv_count_line(
                &id,
                &BillingProductId::new("nope"),
                &CountEntry {
                    counted_qty_milli: Some(1_000),
                    note: String::new(),
                },
            )
            .await,
    );
    assert_not_found(
        a.door
            .set_inv_count_line(&InvCountId::new("nope"), &a.chair, &CountEntry::default())
            .await,
    );
    assert_not_found(a.door.cancel_inv_count(&InvCountId::new("nope")).await);
    assert_not_found(
        a.door
            .update_inv_count_note(&InvCountId::new("nope"), "")
            .await,
    );
    assert_eq!(a.sheet(&id).await.len(), 1, "and nothing was written");
}

#[tokio::test]
async fn one_tenants_stocktake_is_never_anothers() {
    let store = common::test_store().await;
    let a = Counting::open(&store, "ours").await;
    let b = Counting::open(&store, "theirs").await;
    // A co-tenant is not another tenant: a stocktake is the company's.
    let co_user = store
        .for_tenant(a.tenant.clone())
        .create_user("co@count.test")
        .await
        .unwrap();
    let co = store.for_account(a.tenant.clone(), co_user);

    let id = a.count_warehouse().await;
    a.door
        .set_inv_count_line(
            &id,
            &a.chair,
            &CountEntry {
                counted_qty_milli: Some(4_000),
                note: String::new(),
            },
        )
        .await
        .unwrap();

    assert_eq!(
        co.inv_count(&id).await.unwrap().map(|c| c.id),
        Some(id.clone()),
        "a colleague works down the same sheet"
    );
    assert_eq!(co.inv_count_sheet(&id).await.unwrap().len(), 1);

    // ---- and the other tenant reaches none of it --------------------------
    assert!(
        b.door.inv_count(&id).await.unwrap().is_none(),
        "asking about our count says nothing about whether it exists"
    );
    assert!(b.door.inv_count_sheet(&id).await.unwrap().is_empty());
    assert!(
        b.door
            .inv_counts(&CountFilter::default())
            .await
            .unwrap()
            .is_empty(),
        "their list is theirs: they have opened none"
    );
    assert_not_found(
        b.door
            .set_inv_count_line(
                &id,
                &b.chair,
                &CountEntry {
                    counted_qty_milli: Some(99_000),
                    note: String::new(),
                },
            )
            .await,
    );
    assert_not_found(
        b.door
            .set_inv_count_line(
                &id,
                &a.chair,
                &CountEntry {
                    counted_qty_milli: Some(99_000),
                    note: String::new(),
                },
            )
            .await,
    );
    assert_not_found(b.door.update_inv_count_note(&id, "theirs now").await);
    assert_not_found(b.door.cancel_inv_count(&id).await);
    // A count of OUR shelf, opened by THEM, is not a count at all.
    assert_not_found(
        b.door
            .open_inv_count(&NewCount {
                location_id: a.warehouse.clone(),
                note: String::new(),
            })
            .await,
    );

    // Five attempts later, our sheet is exactly as we left it.
    let ours = a.door.inv_count(&id).await.unwrap().unwrap();
    assert_eq!(ours.status, CountStatus::Open);
    assert_eq!(ours.note, "Tuesday, back shelves");
    assert_eq!(ours.line_count, 1);
    assert_eq!(ours.counted_count, 1);
    let line = a.row(&id, &a.chair).await;
    assert_eq!(line.counted_qty_milli, Some(4_000));
    assert_eq!(line.expected_qty_milli, 5_000);
}
