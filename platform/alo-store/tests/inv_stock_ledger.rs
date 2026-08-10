//! **The move ledger's property suite** (alo Inventory, ADR 0035, wave B5.04a)
//! — the invariants of `docs/design/inventory.md` § "The invariant, and how it
//! will be proven", asserted over a randomly generated month of warehouse work
//! rather than over a hand-picked example.
//!
//! Every property is asserted **against the database**, never against the
//! numbers the test just held in memory, and every movement is written through
//! [`alo_store::AccountStore::record_move`] the way a receipt will write it —
//! a property that holds only for rows a test inserted by hand proves nothing
//! about the code that will run.
//!
//! | Property | Where |
//! |---|---|
//! | **P1** a generated month sums to zero per product across all locations | `a_generated_month_sums_to_zero_per_product` |
//! | **P2** the cached balance equals the fold, after every single write | asserted inside every test, via `assert_cache_matches_ledger` |
//! | **P3** on-hand is order-independent | `the_same_movements_in_a_different_order_leave_the_same_balances` |
//! | **P4** a movement and its reversal leave every balance identical | `a_movement_and_its_reversal_leave_the_ledger_where_they_found_it` |
//! | **P5** received in full and returned in full leaves both ends at zero | same |
//! | **P6** no sequence of calls produces a negative balance at a real location | `no_sequence_of_calls_can_drive_a_stock_location_below_zero` |
//! | **P7** one tenant's month leaves another tenant's balances byte-identical | `one_tenants_month_leaves_another_tenants_stock_untouched` |
//!
//! Plus the two the design note calls mandatory beside them: the wrong-tenant
//! denial on every path, and the concurrency proof that two shipments of the
//! last unit produce exactly one success.
//!
//! **The generator is seeded** with the same tiny xorshift64\* the journal's
//! property suite uses, so a failure names the seed that produced it and is
//! replayable to the movement.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::BTreeMap;

use alo_store::inv_locations::{Location, LocationKind, LocationSeed, NewLocation};
use alo_store::inv_moves::{MoveFilter, MoveReason, MoveRefKind, MoveReference, NewMove};
use alo_store::inv_stock::{StockBalance, StockFilter, stock_value_cents};
use alo_store::{
    AccountStore, BillingProductId, InvLocationId, InvMoveId, NewProduct, Store, StoreError,
    TenantId,
};
use time::{Duration, OffsetDateTime};

/// A tiny deterministic generator — xorshift64\*, seeded per test, so the month
/// below is replayable: the same seed writes the same movements.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A value in `0..=max`.
    fn upto(&mut self, max: u64) -> u64 {
        self.next() % (max + 1)
    }

    /// A quantity in milli-units, in `low..=high`.
    fn qty(&mut self, low: i64, high: i64) -> i64 {
        low + i64::try_from(self.upto(u64::try_from(high - low).unwrap_or(0))).unwrap_or(0)
    }

    /// One of a slice, uniformly.
    fn pick<'a, T>(&mut self, values: &'a [T]) -> &'a T {
        &values
            [usize::try_from(self.upto(u64::try_from(values.len() - 1).unwrap_or(0))).unwrap_or(0)]
    }
}

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error.
fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

/// Asserts a result is a refusal the caller could have predicted, and returns
/// its sentence.
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

/// A tenant with its starting locations seeded and a second warehouse created —
/// the shape every test here works in.
struct Warehouse {
    door: AccountStore,
    tenant: TenantId,
    main: InvLocationId,
    second: InvLocationId,
    supplier: InvLocationId,
    customer: InvLocationId,
    adjustment: InvLocationId,
}

impl Warehouse {
    async fn open(store: &Store, tag: &str) -> Self {
        let tenant = store.create_tenant(&format!("inv-{tag}")).await.unwrap();
        let user = store
            .for_tenant(tenant.clone())
            .create_user(&format!("{tag}@stock.test"))
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
        let second = door
            .create_inv_location(&NewLocation {
                code: "WH2".to_owned(),
                name: "Tweede magazijn".to_owned(),
                kind: LocationKind::Stock,
            })
            .await
            .unwrap();
        Self {
            main: of(LocationKind::Stock),
            supplier: of(LocationKind::Supplier),
            customer: of(LocationKind::Customer),
            adjustment: of(LocationKind::Adjustment),
            second,
            door,
            tenant,
        }
    }

    /// A stocked product with a purchase price, so its value is checkable.
    async fn product(&self, name: &str, purchase_price_cents: i64) -> BillingProductId {
        self.door
            .create_billing_product(&NewProduct {
                name: name.to_owned(),
                unit: "piece".to_owned(),
                unit_price_cents: purchase_price_cents * 2,
                vat_rate_bp: 2100,
                stocked: true,
                purchase_price_cents,
                ..Default::default()
            })
            .await
            .unwrap()
    }

    /// Records a movement the way a document will.
    async fn move_qty(
        &self,
        product: &BillingProductId,
        from: &InvLocationId,
        to: &InvLocationId,
        qty_milli: i64,
        reason: MoveReason,
    ) -> Result<InvMoveId, StoreError> {
        self.door
            .record_move(&NewMove {
                product_id: product.clone(),
                from_location_id: from.clone(),
                to_location_id: to.clone(),
                qty_milli,
                reason,
                note: String::new(),
                reference: None,
                occurred_at: None,
            })
            .await
    }

    /// Receives goods from the outside world — the movement every stock story
    /// starts with.
    async fn receive(&self, product: &BillingProductId, to: &InvLocationId, qty_milli: i64) {
        self.move_qty(product, &self.supplier, to, qty_milli, MoveReason::Purchase)
            .await
            .unwrap_or_else(|e| panic!("receiving must succeed: {e}"));
    }
}

/// **P2, asserted everywhere.** The cache and the fold over the movements must
/// agree, row for row, after every single write — the assertion that makes the
/// cache trustworthy rather than merely fast.
async fn assert_cache_matches_ledger(door: &AccountStore, context: &str) {
    let cached = door.inv_stock_cached().await.unwrap();
    let folded = door.inv_stock_folded().await.unwrap();
    assert_eq!(
        cached.len(),
        folded.len(),
        "{context}: the cache has a different number of rows than the ledger implies"
    );
    for (left, right) in cached.iter().zip(folded.iter()) {
        assert_eq!(
            left, right,
            "{context}: the cached balance disagrees with the fold over the movements"
        );
    }
}

/// The sum of every balance of one product, over every location — **P1**, read
/// back from the database.
async fn total_across_locations(door: &AccountStore, product: &BillingProductId) -> i64 {
    door.inv_stock_folded()
        .await
        .unwrap()
        .iter()
        .filter(|balance| &balance.product_id == product)
        .map(|balance| balance.qty_milli)
        .sum()
}

#[tokio::test]
async fn a_movement_is_recorded_whole_and_the_balances_follow_it() {
    let store = common::test_store().await;
    let w = Warehouse::open(&store, "basic").await;
    let chair = w.product("Blue chair", 2_150).await;

    // ---- receiving: goods come FROM the supplier counterparty -------------
    let id = w
        .door
        .record_move(&NewMove {
            product_id: chair.clone(),
            from_location_id: w.supplier.clone(),
            to_location_id: w.main.clone(),
            qty_milli: 12_000,
            reason: MoveReason::Purchase,
            note: "Pallet 4, one box dented".to_owned(),
            reference: Some(MoveReference {
                kind: MoveRefKind::PurchaseOrder,
                id: "po-1".to_owned(),
            }),
            occurred_at: None,
        })
        .await
        .unwrap();
    assert_cache_matches_ledger(&w.door, "after a receipt").await;

    let recorded = w.door.inv_move(&id).await.unwrap().unwrap();
    assert_eq!(recorded.product_name, "Blue chair", "the join names it");
    assert_eq!(recorded.qty_milli, 12_000);
    assert_eq!(recorded.reason, MoveReason::Purchase);
    assert_eq!(recorded.note, "Pallet 4, one box dented");
    assert_eq!(
        recorded.reference,
        Some(MoveReference {
            kind: MoveRefKind::PurchaseOrder,
            id: "po-1".to_owned(),
        })
    );
    assert_eq!(recorded.from_code, "SUPPLIER");
    assert_eq!(recorded.to_name, "Hoofdmagazijn");

    // ---- the balances, and the closed system they live in -----------------
    assert_eq!(w.door.inv_on_hand(&chair, &w.main).await.unwrap(), 12_000);
    assert_eq!(
        w.door.inv_on_hand(&chair, &w.supplier).await.unwrap(),
        -12_000,
        "the supplier counterparty holds minus what has come from outside"
    );
    assert_eq!(
        total_across_locations(&w.door, &chair).await,
        0,
        "P1: the ledger is a closed system"
    );

    // ---- the stock screen's read: shelves only, valued at cost ------------
    let levels = w.door.inv_stock(&StockFilter::default()).await.unwrap();
    assert_eq!(levels.len(), 1, "a counterparty is not a shelf");
    assert_eq!(levels[0].location_code, "MAIN");
    assert_eq!(levels[0].location_kind, LocationKind::Stock);
    assert_eq!(levels[0].qty_milli, 12_000);
    assert_eq!(
        levels[0].value_cents,
        stock_value_cents(12_000, 2_150),
        "stock is worth what it cost us"
    );
    assert_eq!(levels[0].value_cents, 25_800);

    // …and with the counterparties, the two sides that add to nothing.
    let all = w
        .door
        .inv_stock(&StockFilter {
            include_virtual: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all.iter().map(|l| l.qty_milli).sum::<i64>(), 0);
    assert_eq!(all.iter().map(|l| l.value_cents).sum::<i64>(), 0);

    // ---- a transfer between two of our own places -------------------------
    w.move_qty(&chair, &w.main, &w.second, 5_000, MoveReason::Transfer)
        .await
        .unwrap();
    assert_cache_matches_ledger(&w.door, "after a transfer").await;
    assert_eq!(w.door.inv_on_hand(&chair, &w.main).await.unwrap(), 7_000);
    assert_eq!(w.door.inv_on_hand(&chair, &w.second).await.unwrap(), 5_000);
    assert_eq!(total_across_locations(&w.door, &chair).await, 0);

    // ---- the ledger read: newest first, filterable by either end ----------
    let history = w
        .door
        .inv_moves(&MoveFilter {
            product_id: Some(chair.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(
        history[0].reason,
        MoveReason::Transfer,
        "newest movement first"
    );
    let at_second = w
        .door
        .inv_moves(&MoveFilter {
            location_id: Some(w.second.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(at_second.len(), 1, "a location filter matches either end");

    // ---- the refusals a caller can fix ------------------------------------
    let service = w
        .door
        .create_billing_product(&NewProduct {
            name: "Assembly hour".to_owned(),
            stocked: false,
            ..Default::default()
        })
        .await
        .unwrap();
    let moved_service = w
        .move_qty(&service, &w.supplier, &w.main, 1_000, MoveReason::Purchase)
        .await;
    match moved_service {
        Err(StoreError::Validation(message)) => {
            assert!(message.contains("Assembly hour"), "{message}");
            assert!(message.contains("not a stocked product"), "{message}");
        }
        other => panic!("expected a Validation refusal, got {other:?}"),
    }
    for (from, to, qty) in [
        (&w.main, &w.main, 1_000),
        (&w.main, &w.second, 0),
        (&w.main, &w.second, -1_000),
    ] {
        assert!(
            matches!(
                w.move_qty(&chair, from, to, qty, MoveReason::Transfer)
                    .await,
                Err(StoreError::Validation(_))
            ),
            "expected a Validation refusal for qty {qty}"
        );
    }
    assert_cache_matches_ledger(&w.door, "after every refusal").await;

    // ---- un-stocking a product that has moved is refused -------------------
    let unstock = w
        .door
        .update_billing_product(
            &chair,
            &NewProduct {
                name: "Blue chair".to_owned(),
                stocked: false,
                ..Default::default()
            },
        )
        .await;
    assert!(conflict(unstock).contains("stock movements"));
    assert!(
        w.door
            .billing_product(&chair)
            .await
            .unwrap()
            .unwrap()
            .stocked,
        "the refused update changed nothing"
    );
    // A product that has never moved may still stop being stocked.
    w.door
        .update_billing_product(
            &service,
            &NewProduct {
                name: "Assembly hour".to_owned(),
                stocked: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // ---- a location that has carried movements is archived, not deleted ---
    assert!(
        conflict(w.door.delete_inv_location(&w.second).await).contains("movements"),
        "history is not deleted to make a delete succeed"
    );
    w.door
        .set_inv_location_archived(&w.second, true)
        .await
        .unwrap();
    // …and stock can still leave it, which is the whole point of archiving a
    // shed that is being emptied.
    w.move_qty(&chair, &w.second, &w.main, 5_000, MoveReason::Transfer)
        .await
        .unwrap();
    assert_eq!(w.door.inv_on_hand(&chair, &w.second).await.unwrap(), 0);
    assert_cache_matches_ledger(&w.door, "after emptying an archived shed").await;
}

#[tokio::test]
async fn no_sequence_of_calls_can_drive_a_stock_location_below_zero() {
    let store = common::test_store().await;
    let w = Warehouse::open(&store, "negative").await;
    let chair = w.product("Blue chair", 2_150).await;
    w.receive(&chair, &w.main, 4_000).await;

    // **P6.** Shipping more than is there is refused, naming everything the
    // person needs to decide what to do.
    let refusal = conflict(
        w.move_qty(&chair, &w.main, &w.customer, 4_001, MoveReason::Sale)
            .await,
    );
    assert!(refusal.contains("Blue chair"), "{refusal}");
    assert!(refusal.contains("MAIN"), "{refusal}");
    assert!(refusal.contains("4000"), "{refusal}");
    assert!(refusal.contains("4001"), "{refusal}");
    assert_eq!(
        w.door.inv_on_hand(&chair, &w.main).await.unwrap(),
        4_000,
        "a refused movement leaves the balance exactly as it was"
    );
    assert!(
        w.door
            .inv_moves(&MoveFilter::default())
            .await
            .unwrap()
            .iter()
            .all(|m| m.qty_milli != 4_001),
        "and writes no ledger row"
    );
    assert_cache_matches_ledger(&w.door, "after a refused shipment").await;

    // Exactly what is there is allowed, and lands on nothing.
    w.move_qty(&chair, &w.main, &w.customer, 4_000, MoveReason::Sale)
        .await
        .unwrap();
    assert_eq!(w.door.inv_on_hand(&chair, &w.main).await.unwrap(), 0);
    // …and the next unit is refused too.
    assert!(matches!(
        w.move_qty(&chair, &w.main, &w.customer, 1, MoveReason::Sale)
            .await,
        Err(StoreError::Conflict(_))
    ));

    // A generated storm of shipments never leaves a real location negative.
    let mut rng = Rng(0x5745_4152_484F_5553);
    for round in 0..60 {
        let target = if round % 3 == 0 {
            &w.second
        } else {
            &w.customer
        };
        let _ = w
            .move_qty(
                &chair,
                &w.main,
                target,
                rng.qty(1, 3_000),
                MoveReason::Transfer,
            )
            .await;
        if round % 5 == 0 {
            w.receive(&chair, &w.main, rng.qty(1_000, 6_000)).await;
        }
        for balance in w.door.inv_stock_folded().await.unwrap() {
            let location = w
                .door
                .inv_location(&balance.location_id)
                .await
                .unwrap()
                .unwrap();
            assert!(
                !location.kind.is_real() || balance.qty_milli >= 0,
                "round {round}: {} went to {} milli-units",
                location.code,
                balance.qty_milli
            );
        }
    }
    assert_cache_matches_ledger(&w.door, "after the storm").await;

    // The virtual counterparties are unbounded by construction — which is the
    // correct reading of "how much has come from outside".
    assert!(w.door.inv_on_hand(&chair, &w.supplier).await.unwrap() < 0);
    assert_eq!(total_across_locations(&w.door, &chair).await, 0, "P1 holds");
}

#[tokio::test]
async fn a_movement_and_its_reversal_leave_the_ledger_where_they_found_it() {
    let store = common::test_store().await;
    let w = Warehouse::open(&store, "reversal").await;
    let chair = w.product("Blue chair", 1_299).await;
    w.receive(&chair, &w.main, 40_000).await;
    let before: Vec<StockBalance> = w.door.inv_stock_folded().await.unwrap();

    // **P4.** A transfer and the transfer back.
    w.move_qty(&chair, &w.main, &w.second, 7_500, MoveReason::Transfer)
        .await
        .unwrap();
    w.move_qty(&chair, &w.second, &w.main, 7_500, MoveReason::Transfer)
        .await
        .unwrap();
    let after = w.door.inv_stock_folded().await.unwrap();
    let quantities = |rows: &[StockBalance]| -> BTreeMap<String, i64> {
        rows.iter()
            .filter(|row| row.qty_milli != 0)
            .map(|row| (row.location_id.as_str().to_owned(), row.qty_milli))
            .collect()
    };
    assert_eq!(
        quantities(&before),
        quantities(&after),
        "P4: every balance is exactly where it was"
    );
    assert_cache_matches_ledger(&w.door, "after a reversal").await;
    // The correction is itself a fact: nothing was edited away.
    assert_eq!(
        w.door
            .inv_moves(&MoveFilter::default())
            .await
            .unwrap()
            .len(),
        3,
        "a movement is corrected by a movement, never by an edit"
    );

    // **P5.** Received in full, returned in full: both ends land on zero.
    w.move_qty(&chair, &w.main, &w.supplier, 40_000, MoveReason::ReturnOut)
        .await
        .unwrap();
    assert_eq!(w.door.inv_on_hand(&chair, &w.main).await.unwrap(), 0);
    assert_eq!(w.door.inv_on_hand(&chair, &w.supplier).await.unwrap(), 0);
    assert_eq!(total_across_locations(&w.door, &chair).await, 0);
    assert_cache_matches_ledger(&w.door, "after a full return").await;
    // Nothing on a shelf, so the stock screen says nothing — "we have none" and
    // "we have never had any" are the same answer to the question it asks.
    assert!(
        w.door
            .inv_stock(&StockFilter::default())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        w.door
            .inv_stock(&StockFilter {
                include_zero: true,
                include_virtual: true,
                ..Default::default()
            })
            .await
            .unwrap()
            .len(),
        3,
        "…while the history of every place it has been is still readable"
    );
}

#[tokio::test]
async fn the_same_movements_in_a_different_order_leave_the_same_balances() {
    let store = common::test_store().await;
    let w = Warehouse::open(&store, "order").await;
    // **P3.** Two products, the same movements, applied in opposite orders.
    // Two products rather than two tenants, so the comparison is between rows
    // whose location ids are literally the same.
    let first = w.product("Blue chair", 2_150).await;
    let second = w.product("Ash desk", 9_900).await;
    w.receive(&first, &w.main, 100_000).await;
    w.receive(&second, &w.main, 100_000).await;

    let mut rng = Rng(0x4F52_4445_5249_4E47);
    let mut script: Vec<(usize, i64)> = Vec::new();
    for _ in 0..40 {
        script.push((usize::try_from(rng.upto(2)).unwrap_or(0), rng.qty(1, 4_000)));
    }
    let legs = [
        (&w.main, &w.second),
        (&w.second, &w.main),
        (&w.main, &w.customer),
    ];

    for (leg, qty) in &script {
        let (from, to) = legs[*leg];
        // A leg that would go short is skipped for BOTH products, so the two
        // ledgers stay the same set of movements in a different order.
        if w.move_qty(&first, from, to, *qty, MoveReason::Transfer)
            .await
            .is_err()
        {
            continue;
        }
        w.move_qty(&second, from, to, *qty, MoveReason::Transfer)
            .await
            .unwrap();
    }
    // …and the second product's script replays in reverse.
    let mut replay: Vec<(usize, i64)> = script.clone();
    replay.reverse();

    let balances_of =
        |rows: Vec<StockBalance>, product: &BillingProductId| -> BTreeMap<String, i64> {
            rows.into_iter()
                .filter(|row| &row.product_id == product)
                .map(|row| (row.location_id.as_str().to_owned(), row.qty_milli))
                .collect()
        };
    let folded = w.door.inv_stock_folded().await.unwrap();
    assert_eq!(
        balances_of(folded.clone(), &first),
        balances_of(folded, &second),
        "P3: the same movements land on the same balances however they are ordered"
    );
    assert_cache_matches_ledger(&w.door, "after two interleaved scripts").await;

    // The rebuild is the cache's disposability, made real: throw it away and
    // recompute, and nothing about the answer changes.
    let cached_before = w.door.inv_stock_cached().await.unwrap();
    let rebuilt = w.door.inv_stock_rebuild().await.unwrap();
    assert_eq!(rebuilt, cached_before.len());
    assert_eq!(
        w.door.inv_stock_cached().await.unwrap(),
        cached_before,
        "a rebuilt cache is byte-identical to the one the writer maintained"
    );
    assert!(!replay.is_empty());
}

#[tokio::test]
async fn a_generated_month_sums_to_zero_per_product() {
    let store = common::test_store().await;
    let w = Warehouse::open(&store, "month").await;
    let products = vec![
        w.product("Blue chair", 2_150).await,
        w.product("Ash desk", 9_900).await,
        w.product("Oak shelf", 4_450).await,
    ];
    for product in &products {
        w.receive(product, &w.main, 250_000).await;
    }

    let mut rng = Rng(0x4D4F_4E54_4820_3031);
    let reasons = [
        MoveReason::Transfer,
        MoveReason::Sale,
        MoveReason::Adjustment,
        MoveReason::ReturnIn,
    ];
    let day = OffsetDateTime::now_utc() - Duration::days(30);
    for step in 0..120_i64 {
        let product = rng.pick(&products).clone();
        let reason = *rng.pick(&reasons);
        // Each reason moves between the two places it actually means.
        let (from, to) = match reason {
            MoveReason::Sale => (&w.main, &w.customer),
            MoveReason::ReturnIn => (&w.customer, &w.main),
            MoveReason::Adjustment => (&w.main, &w.adjustment),
            _ => (&w.main, &w.second),
        };
        let _ = w
            .door
            .record_move(&NewMove {
                product_id: product,
                from_location_id: from.clone(),
                to_location_id: to.clone(),
                qty_milli: rng.qty(1, 5_000),
                reason,
                note: String::new(),
                reference: None,
                // Back-dated across the month, which the ledger allows and the
                // negative-stock rule deliberately ignores: goods physics is
                // not retroactive.
                occurred_at: Some(day + Duration::hours(step * 6)),
            })
            .await;
    }

    // **P1**, per product, read back from the database.
    for product in &products {
        assert_eq!(
            total_across_locations(&w.door, product).await,
            0,
            "a month of movements must still sum to zero"
        );
    }
    // **P2**, over the whole month at once.
    assert_cache_matches_ledger(&w.door, "after a generated month").await;
    // And a real location never went short along the way.
    for balance in w.door.inv_stock_folded().await.unwrap() {
        let location = w
            .door
            .inv_location(&balance.location_id)
            .await
            .unwrap()
            .unwrap();
        assert!(!location.kind.is_real() || balance.qty_milli >= 0);
    }
    // The ledger read is capped, and the cap is honest about being one.
    let page = w
        .door
        .inv_moves(&MoveFilter {
            limit: Some(10),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.len(), 10);
    // The window filter reads the month it is given, not the month it wants.
    let recent = w
        .door
        .inv_moves(&MoveFilter {
            from: Some(OffsetDateTime::now_utc() - Duration::days(2)),
            limit: Some(500),
            ..Default::default()
        })
        .await
        .unwrap();
    let everything = w
        .door
        .inv_moves(&MoveFilter {
            limit: Some(500),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(recent.len() < everything.len(), "the window narrowed");
    assert!(
        recent
            .iter()
            .all(|m| m.occurred_at > OffsetDateTime::now_utc() - Duration::days(3))
    );
}

#[tokio::test]
async fn one_tenants_month_leaves_another_tenants_stock_untouched() {
    let store = common::test_store().await;
    let a = Warehouse::open(&store, "agg-a").await;
    let b = Warehouse::open(&store, "agg-b").await;

    // B does a modest amount of business first.
    let b_chair = b.product("Blue chair", 2_150).await;
    b.receive(&b_chair, &b.main, 30_000).await;
    b.move_qty(&b_chair, &b.main, &b.customer, 4_000, MoveReason::Sale)
        .await
        .unwrap();
    let b_before = b.door.inv_stock_folded().await.unwrap();
    let b_levels_before = b.door.inv_stock(&StockFilter::default()).await.unwrap();
    let b_moves_before = b
        .door
        .inv_moves(&MoveFilter::default())
        .await
        .unwrap()
        .len();

    // A does a month of it. Deliberately the same product NAME and the same
    // location codes: a SUM that forgot its tenant_id would fold them together.
    let a_chair = a.product("Blue chair", 2_150).await;
    let mut rng = Rng(0x4147_4752_4547_4154);
    a.receive(&a_chair, &a.main, 400_000).await;
    for _ in 0..80 {
        let _ = a
            .move_qty(
                &a_chair,
                &a.main,
                &a.customer,
                rng.qty(1, 4_000),
                MoveReason::Sale,
            )
            .await;
    }

    // **P7.** Every one of B's balances, byte-identical — a single-row read
    // test cannot catch this, and this module *is* sums.
    assert_eq!(
        b.door.inv_stock_folded().await.unwrap(),
        b_before,
        "P7: another tenant's month moved none of ours"
    );
    let b_levels_after = b.door.inv_stock(&StockFilter::default()).await.unwrap();
    assert_eq!(b_levels_after.len(), b_levels_before.len());
    for (before, after) in b_levels_before.iter().zip(b_levels_after.iter()) {
        assert_eq!(before.qty_milli, after.qty_milli);
        assert_eq!(before.value_cents, after.value_cents);
        assert_eq!(before.location_code, after.location_code);
    }
    assert_eq!(
        b.door
            .inv_moves(&MoveFilter::default())
            .await
            .unwrap()
            .len(),
        b_moves_before
    );
    assert_cache_matches_ledger(&b.door, "after the other tenant's month").await;

    // ---- and the wrong-tenant denial on every path ------------------------
    assert!(b.door.inv_location(&a.main).await.unwrap().is_none());
    assert!(
        b.door.inv_on_hand(&a_chair, &a.main).await.unwrap() == 0,
        "another tenant's on-hand reads as zero, never as their number"
    );
    assert!(
        b.door
            .inv_stock(&StockFilter {
                product_id: Some(a_chair.clone()),
                include_virtual: true,
                include_zero: true,
                ..Default::default()
            })
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        b.door
            .inv_moves(&MoveFilter {
                product_id: Some(a_chair.clone()),
                ..Default::default()
            })
            .await
            .unwrap()
            .iter()
            .all(|m| m.product_id != a_chair),
        "another tenant's movements are not in our ledger"
    );
    let a_move = a
        .door
        .inv_moves(&MoveFilter::default())
        .await
        .unwrap()
        .first()
        .map(|m| m.id.clone())
        .unwrap();
    assert!(b.door.inv_move(&a_move).await.unwrap().is_none());
    assert!(
        b.door
            .inv_move(&InvMoveId::generate())
            .await
            .unwrap()
            .is_none()
    );

    // A movement naming another tenant's product or location is NotFound —
    // never a refusal that would confirm the id exists, and never a row.
    assert_not_found(
        b.move_qty(&a_chair, &b.supplier, &b.main, 1_000, MoveReason::Purchase)
            .await,
    );
    assert_not_found(
        b.move_qty(&b_chair, &a.supplier, &b.main, 1_000, MoveReason::Purchase)
            .await,
    );
    assert_not_found(
        b.move_qty(&b_chair, &b.supplier, &a.main, 1_000, MoveReason::Purchase)
            .await,
    );
    assert_eq!(
        b.door
            .inv_moves(&MoveFilter::default())
            .await
            .unwrap()
            .len(),
        b_moves_before,
        "a refused movement wrote nothing"
    );
    assert_cache_matches_ledger(&b.door, "after three cross-tenant attempts").await;

    // ---- deleting the tenant purges the ledger ----------------------------
    store.delete_tenant(&a.tenant).await.unwrap();
    assert!(
        a.door
            .inv_moves(&MoveFilter::default())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(a.door.inv_stock_cached().await.unwrap().is_empty());
    assert_eq!(
        b.door.inv_stock_folded().await.unwrap(),
        b_before,
        "…and takes nothing of ours with it"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_shipments_of_the_last_unit_produce_exactly_one_success() {
    let store = common::test_store().await;
    let w = Warehouse::open(&store, "race").await;
    let chair = w.product("Blue chair", 2_150).await;
    w.receive(&chair, &w.main, 1_000).await;

    // Six simultaneous attempts to ship the one unit that is there. The cached
    // row's lock serialises them, so exactly one wins and the rest are told
    // cleanly that the goods are gone.
    let mut handles = Vec::new();
    for _ in 0..6 {
        let door = w.door.clone();
        let input = NewMove {
            product_id: chair.clone(),
            from_location_id: w.main.clone(),
            to_location_id: w.customer.clone(),
            qty_milli: 1_000,
            reason: MoveReason::Sale,
            note: String::new(),
            reference: None,
            occurred_at: None,
        };
        handles.push(tokio::spawn(async move { door.record_move(&input).await }));
    }
    let mut shipped = 0;
    let mut refused = 0;
    for handle in handles {
        match handle.await.unwrap() {
            Ok(_) => shipped += 1,
            Err(StoreError::Conflict(_)) => refused += 1,
            Err(other) => panic!("expected a clean Conflict, got: {other:?}"),
        }
    }
    assert_eq!(shipped, 1, "exactly one shipment of the last unit");
    assert_eq!(refused, 5, "and five clean refusals, not five races");
    assert_eq!(w.door.inv_on_hand(&chair, &w.main).await.unwrap(), 0);
    assert_eq!(
        w.door
            .inv_moves(&MoveFilter::default())
            .await
            .unwrap()
            .len(),
        2,
        "one receipt and one shipment — a refused attempt writes no row"
    );
    assert_cache_matches_ledger(&w.door, "after the race").await;
    assert_eq!(total_across_locations(&w.door, &chair).await, 0);
}
