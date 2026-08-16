//! Inventory's stock-sale seam against a real database (ADR 0041, item
//! S3.05a1).
//!
//! The one test this suite exists for is the race: **two simultaneous buyers
//! after the last unit, exactly one of whom may get it**. Everything else is
//! the frame around it — availability computed from the ledger's own on-hand
//! and never from a stored copy, holds freeing goods by time passing, a claim
//! that IS the outbound movement (recorded once, split across shelves,
//! refused cleanly when the warehouse's own doors got there first), the
//! tenant walls on every path, and the columns-of-the-table proof that a
//! hold carries no buyer identity at all.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::inv_locations::{Location, LocationKind, LocationSeed};
use alo_store::inv_moves::{MoveFilter, MoveReason, NewMove};
use alo_store::inv_stock_sale::{
    InvStockHoldState, InvStockSale, STOCK_HOLD_MAX_TTL, STOCK_HOLD_MAX_UNITS, STOCK_HOLD_MIN_TTL,
    StockForSale,
};
use alo_store::{
    AccountStore, BillingProductId, BlobStore, InvLocationId, NewProduct, Store, StoreError,
};
use sqlx::postgres::PgPoolOptions;
use time::{Duration, OffsetDateTime};

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

fn conflict_of<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(said)) => said,
        other => panic!("expected Conflict, got {other:?}"),
    }
}

fn validation_of<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(said)) => said,
        other => panic!("expected Validation, got {other:?}"),
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

/// A stock item the way wave two sells one: a book with a shelf count.
fn book() -> NewProduct {
    NewProduct {
        name: "Field guide".to_owned(),
        unit: "piece".to_owned(),
        unit_price_cents: 2_400,
        vat_rate_bp: 600,
        stocked: true,
        purchase_price_cents: 900,
        ..Default::default()
    }
}

/// A tenant with seeded locations, a stocked product, and the seam door a
/// shop would open — the shape every test here works in.
struct Shop {
    account: AccountStore,
    blobs: BlobStore,
    pool: sqlx::PgPool,
    product: BillingProductId,
    main: InvLocationId,
    supplier: InvLocationId,
    customer: InvLocationId,
}

impl Shop {
    async fn open(store: &Store, blobs: BlobStore, tag: &str) -> Self {
        let tenant = store.create_tenant(&format!("shop-{tag}")).await.unwrap();
        let user = store
            .for_tenant(tenant.clone())
            .create_user(&format!("owner@{tag}.test"))
            .await
            .unwrap();
        let account = store.for_account(tenant, user);
        let seeded = account
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
        let product = account.create_billing_product(&book()).await.unwrap();
        let pool = PgPoolOptions::new()
            .max_connections(6)
            .connect(&common::database_url())
            .await
            .unwrap();
        Self {
            main: of(LocationKind::Stock),
            supplier: of(LocationKind::Supplier),
            customer: of(LocationKind::Customer),
            account,
            blobs,
            pool,
            product,
        }
    }

    /// The seam door, opened the way a shop opens it: with the tenant and
    /// owner from its own trusted row.
    fn door(&self) -> InvStockSale {
        InvStockSale::open(
            self.pool.clone(),
            self.blobs.clone(),
            self.account.tenant().clone(),
            self.account.user().clone(),
        )
    }

    /// Puts `units` of the product on a shelf, the way a receipt would.
    async fn receive(&self, to: &InvLocationId, units: i64) {
        self.account
            .record_move(&NewMove {
                product_id: self.product.clone(),
                from_location_id: self.supplier.clone(),
                to_location_id: to.clone(),
                qty_milli: units * 1_000,
                reason: MoveReason::Purchase,
                reason_code: None,
                note: String::new(),
                reference: None,
                occurred_at: None,
            })
            .await
            .unwrap();
    }

    /// The product's on-hand at one location, in milli-units, as the ledger's
    /// cache states it.
    async fn on_hand(&self, at: &InvLocationId) -> i64 {
        self.account.inv_on_hand(&self.product, at).await.unwrap()
    }
}

fn clock() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

const TTL: Duration = Duration::minutes(10);

#[tokio::test]
async fn availability_is_the_ledgers_number_minus_live_holds() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let shop = Shop::open(&store, blobs, "avail").await;
    let door = shop.door();
    let now = clock();

    // Nothing ever received: sellable, none available.
    assert_eq!(
        door.stock_for_sale(&shop.product, now).await.unwrap(),
        Some(StockForSale::Stocked { available_units: 0 })
    );

    shop.receive(&shop.main, 12).await;
    assert_eq!(
        door.stock_for_sale(&shop.product, now).await.unwrap(),
        Some(StockForSale::Stocked {
            available_units: 12
        })
    );

    // A live hold subtracts; its expiry gives the goods back by time passing.
    let hold = door.reserve(&shop.product, 5, TTL, now).await.unwrap();
    assert_eq!(
        door.stock_for_sale(&shop.product, now).await.unwrap(),
        Some(StockForSale::Stocked { available_units: 7 })
    );
    let after_expiry = hold.expires_at + Duration::seconds(1);
    assert_eq!(
        door.stock_for_sale(&shop.product, after_expiry)
            .await
            .unwrap(),
        Some(StockForSale::Stocked {
            available_units: 12
        })
    );

    // The shelf count is the ledger's own: goods leaving through Inventory's
    // door change the answer here with no shop write anywhere.
    shop.account
        .record_move(&NewMove {
            product_id: shop.product.clone(),
            from_location_id: shop.main.clone(),
            to_location_id: shop.supplier.clone(),
            qty_milli: 10_000,
            reason: MoveReason::ReturnOut,
            reason_code: None,
            note: String::new(),
            reference: None,
            occurred_at: None,
        })
        .await
        .unwrap();
    assert_eq!(
        door.stock_for_sale(&shop.product, after_expiry)
            .await
            .unwrap(),
        Some(StockForSale::Stocked { available_units: 2 })
    );
}

#[tokio::test]
async fn a_service_answers_not_stocked_and_a_ghost_answers_nothing() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let shop = Shop::open(&store, blobs, "ghost").await;
    let door = shop.door();
    let now = clock();

    let service = shop
        .account
        .create_billing_product(&NewProduct {
            name: "Consulting hour".to_owned(),
            unit: "hour".to_owned(),
            unit_price_cents: 9_500,
            vat_rate_bp: 2100,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        door.stock_for_sale(&service, now).await.unwrap(),
        Some(StockForSale::NotStocked)
    );
    let said = validation_of(door.reserve(&service, 1, TTL, now).await);
    assert!(
        said.contains("not a stocked product"),
        "the refusal must say why: {said}"
    );

    // Unknown, archived and foreign ids are indistinguishable: nothing.
    let ghost = BillingProductId::generate();
    assert_eq!(door.stock_for_sale(&ghost, now).await.unwrap(), None);
    assert_not_found(door.reserve(&ghost, 1, TTL, now).await);

    shop.account
        .set_billing_product_archived(&shop.product, true)
        .await
        .unwrap();
    assert_eq!(door.stock_for_sale(&shop.product, now).await.unwrap(), None);
    assert_not_found(door.reserve(&shop.product, 1, TTL, now).await);
}

#[tokio::test]
async fn a_reserve_is_bounded_like_a_basket_not_a_script() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let shop = Shop::open(&store, blobs, "bounds").await;
    let door = shop.door();
    let now = clock();
    shop.receive(&shop.main, 100).await;

    validation_of(door.reserve(&shop.product, 0, TTL, now).await);
    validation_of(
        door.reserve(&shop.product, STOCK_HOLD_MAX_UNITS + 1, TTL, now)
            .await,
    );
    validation_of(
        door.reserve(
            &shop.product,
            1,
            STOCK_HOLD_MIN_TTL - Duration::seconds(1),
            now,
        )
        .await,
    );
    validation_of(
        door.reserve(
            &shop.product,
            1,
            STOCK_HOLD_MAX_TTL + Duration::seconds(1),
            now,
        )
        .await,
    );

    // Scarcity is a sentence, not a stack trace.
    shop.receive(&shop.main, 3).await; // 103 on hand
    door.reserve(&shop.product, 20, TTL, now).await.unwrap();
    door.reserve(&shop.product, 20, TTL, now).await.unwrap();
    door.reserve(&shop.product, 20, TTL, now).await.unwrap();
    door.reserve(&shop.product, 20, TTL, now).await.unwrap();
    door.reserve(&shop.product, 20, TTL, now).await.unwrap();
    let said = conflict_of(door.reserve(&shop.product, 4, TTL, now).await);
    assert_eq!(said, "only 3 are left");
    door.reserve(&shop.product, 2, TTL, now).await.unwrap();
    let said = conflict_of(door.reserve(&shop.product, 2, TTL, now).await);
    assert_eq!(said, "only 1 is left");
    door.reserve(&shop.product, 1, TTL, now).await.unwrap();
    let said = conflict_of(door.reserve(&shop.product, 1, TTL, now).await);
    assert_eq!(said, "sold out");
}

#[tokio::test]
async fn two_simultaneous_buyers_after_the_last_unit_get_exactly_one_sale() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let shop = Shop::open(&store, blobs, "race").await;
    let now = clock();
    shop.receive(&shop.main, 1).await;

    let one = shop.door();
    let two = shop.door();
    let (first, second) = tokio::join!(
        one.reserve(&shop.product, 1, TTL, now),
        two.reserve(&shop.product, 1, TTL, now),
    );
    let wins = [&first, &second].iter().filter(|r| r.is_ok()).count();
    assert_eq!(
        wins, 1,
        "exactly one buyer may get the last unit: {first:?} / {second:?}"
    );
    let said = match (first, second) {
        (Err(StoreError::Conflict(said)), Ok(_)) | (Ok(_), Err(StoreError::Conflict(said))) => said,
        other => panic!("the loser must be told a clean sold-out, got {other:?}"),
    };
    assert_eq!(said, "sold out");
}

#[tokio::test]
async fn a_claim_is_the_movement_recorded_once_and_split_across_shelves() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let shop = Shop::open(&store, blobs, "claim").await;
    let door = shop.door();
    let now = clock();

    // Two shelves: 2 on the second, 1 on the main — a claim of 3 must split.
    let second = shop
        .account
        .create_inv_location(&alo_store::inv_locations::NewLocation {
            code: "WH2".to_owned(),
            name: "Tweede magazijn".to_owned(),
            kind: LocationKind::Stock,
        })
        .await
        .unwrap();
    shop.receive(&shop.main, 1).await;
    shop.receive(&second, 2).await;

    let hold = door.reserve(&shop.product, 3, TTL, now).await.unwrap();
    let claimed = door.claim(&hold.id, "Order web-1", now).await.unwrap();
    assert_eq!(claimed.state, InvStockHoldState::Completed);
    assert_eq!(claimed.units, 3);

    // The shelves are empty, the customer counterparty holds the goods, and
    // the ledger explains the sale in the caller's words.
    assert_eq!(shop.on_hand(&shop.main).await, 0);
    assert_eq!(shop.on_hand(&second).await, 0);
    assert_eq!(shop.on_hand(&shop.customer).await, 3_000);
    let moves = shop
        .account
        .inv_moves(&MoveFilter {
            product_id: Some(shop.product.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    let sales: Vec<_> = moves
        .iter()
        .filter(|m| m.reason == MoveReason::Sale)
        .collect();
    assert_eq!(sales.len(), 2, "one movement per shelf the claim drew from");
    assert!(sales.iter().all(|m| m.note == "Order web-1"));
    assert_eq!(sales.iter().map(|m| m.qty_milli).sum::<i64>(), 3_000);

    // A completed hold counts for nothing: the shelf already dropped, and
    // counting the hold too would subtract the sale twice.
    assert_eq!(
        door.stock_for_sale(&shop.product, now).await.unwrap(),
        Some(StockForSale::Stocked { available_units: 0 })
    );

    // A retried webhook claims again and moves nothing.
    let again = door.claim(&hold.id, "Order web-1", now).await.unwrap();
    assert_eq!(again.state, InvStockHoldState::Completed);
    let moves_after = shop
        .account
        .inv_moves(&MoveFilter {
            product_id: Some(shop.product.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        moves_after.len(),
        moves.len(),
        "an idempotent claim records no second movement"
    );
    assert_eq!(shop.on_hand(&shop.customer).await, 3_000);
}

#[tokio::test]
async fn a_lapsed_or_released_hold_cannot_be_claimed_and_release_is_idempotent() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let shop = Shop::open(&store, blobs, "lapse").await;
    let door = shop.door();
    let now = clock();
    shop.receive(&shop.main, 5).await;

    let lapsed = door.reserve(&shop.product, 2, TTL, now).await.unwrap();
    let late = lapsed.expires_at + Duration::seconds(1);
    let said = conflict_of(door.claim(&lapsed.id, "Order late", late).await);
    assert!(said.contains("expired"), "{said}");
    assert_eq!(shop.on_hand(&shop.main).await, 5_000, "nothing moved");

    let walked = door.reserve(&shop.product, 2, TTL, now).await.unwrap();
    let released = door.release(&walked.id, now).await.unwrap();
    assert_eq!(released.state, InvStockHoldState::Released);
    // Pressed twice: still a success, still released.
    let again = door.release(&walked.id, now).await.unwrap();
    assert_eq!(again.state, InvStockHoldState::Released);
    let said = conflict_of(door.claim(&walked.id, "Order walked", now).await);
    assert!(said.contains("released"), "{said}");

    // A claimed hold is a sale; releasing it is refused with the reason.
    let sold = door.reserve(&shop.product, 1, TTL, now).await.unwrap();
    door.claim(&sold.id, "Order sold", now).await.unwrap();
    let said = conflict_of(door.release(&sold.id, now).await);
    assert!(said.contains("complete"), "{said}");
}

#[tokio::test]
async fn a_claim_refuses_cleanly_when_the_warehouse_got_there_first() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let shop = Shop::open(&store, blobs, "beaten").await;
    let door = shop.door();
    let now = clock();
    shop.receive(&shop.main, 3).await;

    let hold = door.reserve(&shop.product, 3, TTL, now).await.unwrap();
    // Holds bind the shop, never Inventory: the warehouse door ships 2 out.
    shop.account
        .record_move(&NewMove {
            product_id: shop.product.clone(),
            from_location_id: shop.main.clone(),
            to_location_id: shop.supplier.clone(),
            qty_milli: 2_000,
            reason: MoveReason::ReturnOut,
            reason_code: None,
            note: String::new(),
            reference: None,
            occurred_at: None,
        })
        .await
        .unwrap();

    let said = conflict_of(door.claim(&hold.id, "Order beaten", now).await);
    assert_eq!(
        said,
        "the goods have since left stock: 1 of the reserved 3 units remain"
    );
    // The refusal moved nothing and the hold stays live for a retry.
    assert_eq!(shop.on_hand(&shop.main).await, 1_000);
    assert_eq!(shop.on_hand(&shop.customer).await, 0);
    let stood = door.stock_hold(&hold.id, now).await.unwrap().unwrap();
    assert_eq!(stood.state, InvStockHoldState::Held);

    // Restocked, the same hold claims whole.
    shop.receive(&shop.main, 2).await;
    let claimed = door.claim(&hold.id, "Order beaten", now).await.unwrap();
    assert_eq!(claimed.state, InvStockHoldState::Completed);
    assert_eq!(shop.on_hand(&shop.customer).await, 3_000);
}

#[tokio::test]
async fn every_path_stops_at_the_tenant_wall() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let ours = Shop::open(&store, blobs.clone(), "wall-a").await;
    let theirs = Shop::open(&store, blobs, "wall-b").await;
    let now = clock();
    ours.receive(&ours.main, 10).await;
    theirs.receive(&theirs.main, 10).await;

    let our_door = ours.door();
    let their_door = theirs.door();
    let hold = our_door.reserve(&ours.product, 4, TTL, now).await.unwrap();

    // Their door sees none of it: not the product, not the hold — and their
    // claim or release of our hold is the clean not-found denial.
    assert_eq!(
        their_door.stock_for_sale(&ours.product, now).await.unwrap(),
        None
    );
    assert_not_found(their_door.reserve(&ours.product, 1, TTL, now).await);
    assert_eq!(their_door.stock_hold(&hold.id, now).await.unwrap(), None);
    assert_not_found(their_door.claim(&hold.id, "Order theft", now).await);
    assert_not_found(their_door.release(&hold.id, now).await);

    // Our hold never counts against their availability.
    assert_eq!(
        their_door
            .stock_for_sale(&theirs.product, now)
            .await
            .unwrap(),
        Some(StockForSale::Stocked {
            available_units: 10
        })
    );
    // And nothing they did dented ours.
    assert_eq!(
        our_door.stock_for_sale(&ours.product, now).await.unwrap(),
        Some(StockForSale::Stocked { available_units: 6 })
    );
}

#[tokio::test]
async fn a_hold_carries_no_buyer_identity_at_all() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let shop = Shop::open(&store, blobs, "columns").await;
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_name = 'inv_stock_sale_holds' ORDER BY column_name",
    )
    .fetch_all(&shop.pool)
    .await
    .unwrap();
    assert_eq!(
        columns,
        vec![
            "completed_at",
            "created_at",
            "expires_at",
            "id",
            "product_id",
            "qty_milli",
            "state",
            "tenant_id",
        ],
        "a hold is pure quantity accounting; who bought lives on the order"
    );
}
