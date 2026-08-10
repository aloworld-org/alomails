//! **The manual door onto the ledger** (alo Inventory, ADR 0035, wave B5.04b)
//! — `docs/design/inventory.md` § "Adjustments and transfers", asserted against
//! the real database rather than against the rules in isolation.
//!
//! [`alo_store::inv_adjust`]'s unit tests already prove the pure rule over
//! every combination of kinds. What only a database can prove is what this
//! suite is for:
//!
//! | Property | Where |
//! |---|---|
//! | a loss leaves the shelf shorter and the ledger explaining why | `an_adjustment_out_of_stock_writes_the_loss_and_its_reason` |
//! | a surplus is the mirror of it, and the pair sums to zero | `a_surplus_and_a_loss_of_the_same_size_leave_the_ledger_where_they_found_it` |
//! | a transfer moves goods without changing what the tenant owns | `a_transfer_moves_the_goods_and_not_the_total` |
//! | the refusals are refusals, and nothing is written by one | `nothing_a_refused_movement_asked_for_reaches_the_ledger` |
//! | an archived place stops receiving and keeps giving | `an_archived_location_can_be_emptied_and_cannot_be_filled` |
//! | one tenant's adjustment is invisible and unreachable from another | `another_tenants_locations_are_not_reachable_through_this_door` |
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::inv_adjust::{ADJUST_REASONS, AdjustReason, NewManualMove};
use alo_store::inv_locations::{Location, LocationKind, LocationSeed, NewLocation};
use alo_store::inv_moves::{MoveFilter, MoveReason};
use alo_store::{
    AccountStore, BillingProductId, InvLocationId, NewProduct, Store, StoreError, TenantId,
};

/// Asserts a result is the refusal a caller could have predicted, and returns
/// its sentence — never data, never an internal (`Db`) error.
fn invalid<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(message)) => message,
        other => panic!("expected Validation, got: {other:?}"),
    }
}

fn conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(message)) => message,
        other => panic!("expected Conflict, got: {other:?}"),
    }
}

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    assert!(
        matches!(result, Err(StoreError::NotFound)),
        "expected the clean not-found denial, got: {result:?}"
    );
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

/// A tenant with its locations seeded, a second warehouse, and a product with
/// stock on the shelf — the shape every test here works in.
struct Warehouse {
    door: AccountStore,
    tenant: TenantId,
    chair: BillingProductId,
    main: InvLocationId,
    second: InvLocationId,
    supplier: InvLocationId,
    customer: InvLocationId,
    adjustment: InvLocationId,
    production: InvLocationId,
}

impl Warehouse {
    async fn open(store: &Store, tag: &str) -> Self {
        let tenant = store.create_tenant(&format!("adj-{tag}")).await.unwrap();
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
        let chair = door
            .create_billing_product(&NewProduct {
                name: "Blue chair".to_owned(),
                unit: "piece".to_owned(),
                unit_price_cents: 4_300,
                vat_rate_bp: 2100,
                stocked: true,
                purchase_price_cents: 2_150,
                ..Default::default()
            })
            .await
            .unwrap();
        let this = Self {
            main: of(LocationKind::Stock),
            supplier: of(LocationKind::Supplier),
            customer: of(LocationKind::Customer),
            adjustment: of(LocationKind::Adjustment),
            production: of(LocationKind::Production),
            second,
            chair,
            door,
            tenant,
        };
        // Ten chairs arrive the way B5.05b will make them arrive: through the
        // ledger's document door, not by planting a row.
        this.receive(10_000).await;
        this
    }

    /// Goods in from the outside world, so there is something to adjust.
    async fn receive(&self, qty_milli: i64) {
        self.door
            .record_move(&alo_store::inv_moves::NewMove {
                product_id: self.chair.clone(),
                from_location_id: self.supplier.clone(),
                to_location_id: self.main.clone(),
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

    /// A manual movement of chairs, with everything else left at its default.
    fn manual(
        &self,
        from: &InvLocationId,
        to: &InvLocationId,
        qty_milli: i64,
        reason: MoveReason,
        reason_code: Option<AdjustReason>,
    ) -> NewManualMove {
        NewManualMove {
            product_id: self.chair.clone(),
            from_location_id: from.clone(),
            to_location_id: to.clone(),
            qty_milli,
            reason,
            reason_code,
            note: String::new(),
            occurred_at: None,
        }
    }

    async fn on_hand(&self, location: &InvLocationId) -> i64 {
        self.door.inv_on_hand(&self.chair, location).await.unwrap()
    }

    /// How many movements this tenant's ledger holds — the count that must not
    /// move when a request is refused.
    async fn ledger_len(&self) -> usize {
        self.door
            .inv_moves(&MoveFilter::default())
            .await
            .unwrap()
            .len()
    }
}

#[tokio::test]
async fn an_adjustment_out_of_stock_writes_the_loss_and_its_reason() {
    let store = common::test_store().await;
    let w = Warehouse::open(&store, "loss").await;

    let mut input = w.manual(
        &w.main,
        &w.adjustment,
        2_000,
        MoveReason::Adjustment,
        Some(AdjustReason::Damaged),
    );
    input.note = "Two chairs crushed by the forklift".to_owned();
    let id = w.door.record_manual_move(&input).await.unwrap();

    assert_eq!(w.on_hand(&w.main).await, 8_000, "the shelf is two short");
    assert_eq!(
        w.on_hand(&w.adjustment).await,
        2_000,
        "and the counterparty holds the explanation"
    );

    // The row says who, why and in whose words — an id is not an explanation.
    let recorded = w.door.inv_move(&id).await.unwrap().expect("the movement");
    assert_eq!(recorded.reason, MoveReason::Adjustment);
    assert_eq!(recorded.reason_code, Some(AdjustReason::Damaged));
    assert_eq!(recorded.note, "Two chairs crushed by the forklift");
    assert_eq!(recorded.qty_milli, 2_000, "quantities are never signed");
    assert_eq!(recorded.from_code, "MAIN");
    assert_eq!(recorded.to_code, "ADJUST");
    assert!(
        recorded.reference.is_none(),
        "a manual movement points at no document, and cannot claim one"
    );

    // Every code the vocabulary offers round-trips through the column.
    for code in ADJUST_REASONS {
        let id = w
            .door
            .record_manual_move(&w.manual(
                &w.adjustment,
                &w.main,
                1,
                MoveReason::Adjustment,
                Some(code),
            ))
            .await
            .unwrap();
        assert_eq!(
            w.door
                .inv_move(&id)
                .await
                .unwrap()
                .expect("the movement")
                .reason_code,
            Some(code)
        );
    }
}

#[tokio::test]
async fn a_surplus_and_a_loss_of_the_same_size_leave_the_ledger_where_they_found_it() {
    let store = common::test_store().await;
    let w = Warehouse::open(&store, "mirror").await;

    w.door
        .record_manual_move(&w.manual(
            &w.adjustment,
            &w.main,
            3_000,
            MoveReason::Adjustment,
            Some(AdjustReason::Found),
        ))
        .await
        .unwrap();
    assert_eq!(w.on_hand(&w.main).await, 13_000);
    assert_eq!(w.on_hand(&w.adjustment).await, -3_000);

    w.door
        .record_manual_move(&w.manual(
            &w.main,
            &w.adjustment,
            3_000,
            MoveReason::Adjustment,
            Some(AdjustReason::Correction),
        ))
        .await
        .unwrap();
    assert_eq!(w.on_hand(&w.main).await, 10_000, "back where it started");
    assert_eq!(w.on_hand(&w.adjustment).await, 0);
    assert_eq!(
        w.ledger_len().await,
        3,
        "and both movements are still there — a correction is a fact, not an erasure"
    );
}

#[tokio::test]
async fn a_transfer_moves_the_goods_and_not_the_total() {
    let store = common::test_store().await;
    let w = Warehouse::open(&store, "transfer").await;

    w.door
        .record_manual_move(&w.manual(&w.main, &w.second, 4_000, MoveReason::Transfer, None))
        .await
        .unwrap();
    assert_eq!(w.on_hand(&w.main).await, 6_000);
    assert_eq!(w.on_hand(&w.second).await, 4_000);
    assert_eq!(
        w.on_hand(&w.main).await + w.on_hand(&w.second).await,
        10_000,
        "a transfer changes where the goods are and not how many there are"
    );

    // More than is there, from the place it is not.
    let refusal = conflict(
        w.door
            .record_manual_move(&w.manual(&w.second, &w.main, 9_000, MoveReason::Transfer, None))
            .await,
    );
    assert!(refusal.contains("Blue chair"), "{refusal}");
    assert!(refusal.contains("WH2"), "{refusal}");
    assert!(refusal.contains("4000"), "{refusal}");
}

#[tokio::test]
async fn nothing_a_refused_movement_asked_for_reaches_the_ledger() {
    let store = common::test_store().await;
    let w = Warehouse::open(&store, "refusals").await;
    let before = w.ledger_len().await;

    // The two trading counterparties, in both directions and under both
    // reasons — the refusal that keeps a purchase from being booked without an
    // order behind it.
    for (from, to) in [
        (&w.supplier, &w.main),
        (&w.main, &w.customer),
        (&w.customer, &w.main),
    ] {
        for (reason, code) in [
            (MoveReason::Transfer, None),
            (MoveReason::Adjustment, Some(AdjustReason::Lost)),
        ] {
            let message = invalid(
                w.door
                    .record_manual_move(&w.manual(from, to, 1_000, reason, code))
                    .await,
            );
            assert!(message.contains("purchase order"), "{message}");
        }
    }

    // A reason that names a document.
    for document in [
        MoveReason::Purchase,
        MoveReason::Sale,
        MoveReason::Count,
        MoveReason::ReturnIn,
        MoveReason::ReturnOut,
    ] {
        let message = invalid(
            w.door
                .record_manual_move(&w.manual(&w.main, &w.second, 1_000, document, None))
                .await,
        );
        assert!(message.contains("comes from a document"), "{message}");
    }

    // An adjustment with no code, and a transfer carrying one.
    let uncoded = invalid(
        w.door
            .record_manual_move(&w.manual(
                &w.main,
                &w.adjustment,
                1_000,
                MoveReason::Adjustment,
                None,
            ))
            .await,
    );
    assert!(
        uncoded.contains("an adjustment needs a reason code"),
        "{uncoded}"
    );
    for code in ADJUST_REASONS {
        assert!(uncoded.contains(code.as_str()), "{uncoded} omits {code:?}");
    }
    let coded = invalid(
        w.door
            .record_manual_move(&w.manual(
                &w.main,
                &w.second,
                1_000,
                MoveReason::Transfer,
                Some(AdjustReason::Lost),
            ))
            .await,
    );
    assert!(coded.contains("only an adjustment"), "{coded}");

    // An adjustment that touches no adjustment location, and a transfer that
    // touches one: each is the other's word for what it is doing.
    let mislabelled = invalid(
        w.door
            .record_manual_move(&w.manual(
                &w.main,
                &w.second,
                1_000,
                MoveReason::Adjustment,
                Some(AdjustReason::Lost),
            ))
            .await,
    );
    assert!(mislabelled.contains("adjustment location"), "{mislabelled}");
    let via_production = invalid(
        w.door
            .record_manual_move(&w.manual(
                &w.main,
                &w.production,
                1_000,
                MoveReason::Transfer,
                None,
            ))
            .await,
    );
    assert!(
        via_production.contains("two of the tenant's own locations"),
        "{via_production}"
    );

    // The field rules the ledger owns still apply through this door.
    assert!(
        invalid(
            w.door
                .record_manual_move(&w.manual(&w.main, &w.second, 0, MoveReason::Transfer, None))
                .await
        )
        .contains("greater than zero")
    );
    assert!(
        invalid(
            w.door
                .record_manual_move(&w.manual(&w.main, &w.main, 1_000, MoveReason::Transfer, None))
                .await
        )
        .contains("two different locations")
    );

    assert_eq!(
        w.ledger_len().await,
        before,
        "not one refusal wrote a row, and not one left a cached balance behind"
    );
    assert_eq!(w.on_hand(&w.main).await, 10_000);
    assert_eq!(w.on_hand(&w.second).await, 0);
}

#[tokio::test]
async fn an_archived_location_can_be_emptied_and_cannot_be_filled() {
    let store = common::test_store().await;
    let w = Warehouse::open(&store, "archived").await;
    w.door
        .record_manual_move(&w.manual(&w.main, &w.second, 5_000, MoveReason::Transfer, None))
        .await
        .unwrap();
    w.door
        .set_inv_location_archived(&w.second, true)
        .await
        .unwrap();

    // Out of the shed being emptied: exactly what archiving must not block.
    w.door
        .record_manual_move(&w.manual(&w.second, &w.main, 5_000, MoveReason::Transfer, None))
        .await
        .unwrap();
    assert_eq!(w.on_hand(&w.second).await, 0);

    // Into it: refused, naming the place, because a location nobody offers in
    // a picker should not quietly start holding stock again.
    let message = conflict(
        w.door
            .record_manual_move(&w.manual(&w.main, &w.second, 1_000, MoveReason::Transfer, None))
            .await,
    );
    assert!(message.contains("WH2"), "{message}");
    assert!(message.contains("archived"), "{message}");
    assert_eq!(w.on_hand(&w.main).await, 10_000);
}

#[tokio::test]
async fn another_tenants_locations_are_not_reachable_through_this_door() {
    let store = common::test_store().await;
    let ours = Warehouse::open(&store, "tenant-a").await;
    let theirs = Warehouse::open(&store, "tenant-b").await;
    assert_ne!(ours.tenant.as_str(), theirs.tenant.as_str());

    // Their warehouse as our destination, their warehouse as our source, and
    // their product out of our own shelf: every one is the clean not-found
    // denial, which is indistinguishable from an id that never existed.
    assert_not_found(
        ours.door
            .record_manual_move(&ours.manual(
                &ours.main,
                &theirs.second,
                1_000,
                MoveReason::Transfer,
                None,
            ))
            .await,
    );
    assert_not_found(
        ours.door
            .record_manual_move(&ours.manual(
                &theirs.main,
                &ours.second,
                1_000,
                MoveReason::Transfer,
                None,
            ))
            .await,
    );
    let mut foreign_product = ours.manual(
        &ours.main,
        &ours.adjustment,
        1_000,
        MoveReason::Adjustment,
        Some(AdjustReason::Lost),
    );
    foreign_product.product_id = theirs.chair.clone();
    assert_not_found(ours.door.record_manual_move(&foreign_product).await);

    // And nothing of theirs moved: not the ledger, not the shelf.
    assert_eq!(theirs.on_hand(&theirs.main).await, 10_000);
    assert_eq!(theirs.on_hand(&theirs.second).await, 0);
    assert_eq!(
        theirs.ledger_len().await,
        1,
        "their receipt, and nothing else"
    );
    assert_eq!(ours.ledger_len().await, 1);
}
