//! **Reorder rules and the shortage query** against the real database (alo
//! Inventory, wave B5.07).
//!
//! [`alo_store::inv_reorder`]'s unit tests already prove the pure arithmetic —
//! what `available` is, what a supplier's minimum does to the quantity to buy,
//! which numbers are refused. What only a database can prove is what this suite
//! is for:
//!
//! | Property | Where |
//! |---|---|
//! | a rule round-trips, and no path of it reaches another tenant | `rules_round_trip_and_never_cross_tenant` |
//! | a rule is refused on a service, a counterparty, an archived end, a second time | `a_rule_needs_a_stocked_product_at_a_real_place` |
//! | the report counts the shelf, the open orders and the promises | `the_report_counts_the_shelf_the_open_orders_and_the_promises` |
//! | a parked rule, an archived product and an archived place report nothing | `a_parked_rule_and_an_archived_end_report_nothing` |
//! | one tenant's shortages are only ever their own | `one_tenants_shortages_are_never_another_tenants` |
//! | one product's pipeline reads without a rule, and never crosses a tenant | `the_pipeline_of_one_product_is_read_without_a_rule_and_never_crosses_a_tenant` |
//!
//! The third is the one the item is really about: two tenants set up
//! identically, and every number in the report has to come from the caller's own
//! rows.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::inv_locations::{Location, LocationKind, LocationSeed, NewLocation};
use alo_store::inv_po::NewPurchaseOrder;
use alo_store::inv_po_lines::NewPoLine;
use alo_store::inv_reorder::{
    NewReorderRule, ReorderLimits, ReorderRuleFilter, Shortage, ShortageFilter,
};
use alo_store::inv_so::NewSalesOrder;
use alo_store::inv_so_lines::NewSoLine;
use alo_store::inv_supplier_prices::NewSupplierPrice;
use alo_store::inv_suppliers::NewSupplier;
use alo_store::{
    AccountStore, BillingCustomerId, BillingProductId, InvLocationId, InvReorderRuleId,
    InvSupplierId, NewCustomer, NewLine, NewProduct, Store, StoreError, TenantId,
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

/// A tenant with its locations seeded, a supplier who quotes us for one chair,
/// a customer to sell to, and nothing on the shelf yet.
struct Buying {
    door: AccountStore,
    tenant: TenantId,
    chair: BillingProductId,
    warehouse: InvLocationId,
    supplier_location: InvLocationId,
    supplier: InvSupplierId,
    customer: BillingCustomerId,
}

impl Buying {
    async fn open(store: &Store, tag: &str) -> Self {
        let tenant = store
            .create_tenant(&format!("reorder-{tag}"))
            .await
            .unwrap();
        let user = store
            .for_tenant(tenant.clone())
            .create_user(&format!("{tag}@reorder.test"))
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
        let supplier = door
            .create_inv_supplier(&NewSupplier {
                name: format!("Hoffmann {tag}"),
                country: "DE".to_owned(),
                currency: "EUR".to_owned(),
                email: Some(format!("orders+{tag}@hoffmann.test")),
                lead_time_days: 9,
                ..Default::default()
            })
            .await
            .unwrap();
        let customer = door
            .create_billing_customer(&NewCustomer {
                name: format!("Meubelhuis {tag}"),
                country: "NL".to_owned(),
                currency: "EUR".to_owned(),
                ..Default::default()
            })
            .await
            .unwrap();
        let chair = door
            .create_billing_product(&NewProduct {
                name: "Blue chair".to_owned(),
                unit: "piece".to_owned(),
                unit_price_cents: 8_600,
                vat_rate_bp: 1900,
                stocked: true,
                purchase_price_cents: 4_300,
                default_supplier_id: Some(supplier.clone()),
                ..Default::default()
            })
            .await
            .unwrap();
        // What they quote us: €31.50 a chair, and they will not sell under ten.
        door.set_inv_supplier_price(
            &supplier,
            &chair,
            &NewSupplierPrice {
                supplier_code: "HM-4471".to_owned(),
                purchase_price_cents: 3_150,
                min_order_qty_milli: 10_000,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        Self {
            warehouse: of(LocationKind::Stock),
            supplier_location: of(LocationKind::Supplier),
            chair,
            supplier,
            customer,
            door,
            tenant,
        }
    }

    /// "Keep at least four chairs here, and buy back up to twenty."
    async fn watch_chairs(&self) -> InvReorderRuleId {
        self.door
            .create_inv_reorder_rule(&NewReorderRule {
                product_id: self.chair.clone(),
                location_id: self.warehouse.clone(),
                min_qty_milli: 4_000,
                target_qty_milli: 20_000,
                active: true,
            })
            .await
            .unwrap()
    }

    /// Puts `qty_milli` of chairs on the shelf, the way a receipt does.
    async fn receive(&self, qty_milli: i64) {
        use alo_store::inv_moves::{MoveReason, NewMove};
        self.door
            .record_move(&NewMove {
                product_id: self.chair.clone(),
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

    /// Places an order with the supplier for `qty_milli` chairs — drafted and
    /// **sent**, which is what puts it on order.
    async fn order_chairs(&self, qty_milli: i64) {
        let order = self
            .door
            .create_inv_purchase_order(&NewPurchaseOrder::for_supplier(self.supplier.clone()))
            .await
            .unwrap();
        self.door
            .set_inv_purchase_order_lines(
                &order,
                &[NewPoLine {
                    product_id: Some(self.chair.clone()),
                    line: NewLine {
                        description: "Blue chair".to_owned(),
                        unit: "piece".to_owned(),
                        qty_milli,
                        unit_price_cents: 3_150,
                        vat_rate_bp: 1900,
                    },
                }],
            )
            .await
            .unwrap();
        self.door
            .send_inv_purchase_order::<(), StoreError, _, _>(&order, |_| async { Ok(()) })
            .await
            .unwrap();
    }

    /// Takes a customer order for `qty_milli` chairs and **confirms** it, which
    /// is what promises the goods.
    async fn promise_chairs(&self, qty_milli: i64) {
        let order = self
            .door
            .create_inv_sales_order(&NewSalesOrder::for_customer(self.customer.clone()))
            .await
            .unwrap();
        self.door
            .set_inv_sales_order_lines(
                &order,
                &[NewSoLine {
                    product_id: Some(self.chair.clone()),
                    line: NewLine {
                        description: "Blue chair".to_owned(),
                        unit: "piece".to_owned(),
                        qty_milli,
                        unit_price_cents: 8_600,
                        vat_rate_bp: 1900,
                    },
                }],
            )
            .await
            .unwrap();
        self.door.confirm_inv_sales_order(&order).await.unwrap();
    }

    async fn shortages(&self) -> Vec<Shortage> {
        self.door
            .inv_shortages(&ShortageFilter::default())
            .await
            .unwrap()
    }

    /// The one shortage the report is expected to hold, or a failure naming what
    /// it held instead.
    async fn only_shortage(&self) -> Shortage {
        let found = self.shortages().await;
        assert_eq!(found.len(), 1, "expected exactly one shortage: {found:?}");
        found.into_iter().next().unwrap()
    }
}

#[tokio::test]
async fn rules_round_trip_and_never_cross_tenant() {
    let store = common::test_store().await;
    let a = Buying::open(&store, "a").await;
    let b = Buying::open(&store, "b").await;
    // A co-tenant sees the same rules: a minimum is the company's, not a
    // person's.
    let co_user = store
        .for_tenant(a.tenant.clone())
        .create_user("co@reorder.test")
        .await
        .unwrap();
    let co = store.for_account(a.tenant.clone(), co_user);

    let id = a.watch_chairs().await;

    // ---- read ------------------------------------------------------------
    let stored = a.door.inv_reorder_rule(&id).await.unwrap().unwrap();
    assert_eq!(stored.product_name, "Blue chair");
    assert_eq!(stored.sku, "");
    assert_eq!(stored.unit, "piece");
    assert_eq!(stored.location_code, "MAIN");
    assert_eq!(stored.min_qty_milli, 4_000);
    assert_eq!(stored.target_qty_milli, 20_000);
    assert!(stored.active);
    assert_eq!(
        co.inv_reorder_rule(&id).await.unwrap().map(|r| r.id),
        Some(id.clone()),
        "a co-tenant reads the company's own rule"
    );
    assert!(
        b.door.inv_reorder_rule(&id).await.unwrap().is_none(),
        "another tenant's rule is not readable, and asking says nothing"
    );

    // ---- list ------------------------------------------------------------
    assert_eq!(
        a.door
            .inv_reorder_rules(&ReorderRuleFilter::default())
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        b.door
            .inv_reorder_rules(&ReorderRuleFilter::default())
            .await
            .unwrap()
            .is_empty(),
        "the list is the caller's tenant and nothing else"
    );
    // Narrowing by another tenant's ids is an empty list, never a refusal that
    // would confirm they exist.
    assert!(
        a.door
            .inv_reorder_rules(&ReorderRuleFilter {
                product_id: Some(b.chair.clone()),
                ..Default::default()
            })
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        a.door
            .inv_reorder_rules(&ReorderRuleFilter {
                location_id: Some(b.warehouse.clone()),
                ..Default::default()
            })
            .await
            .unwrap()
            .is_empty()
    );

    // ---- create with another tenant's ends --------------------------------
    assert_not_found(
        a.door
            .create_inv_reorder_rule(&NewReorderRule {
                product_id: b.chair.clone(),
                location_id: a.warehouse.clone(),
                min_qty_milli: 1_000,
                target_qty_milli: 2_000,
                active: true,
            })
            .await,
    );
    assert_not_found(
        a.door
            .create_inv_reorder_rule(&NewReorderRule {
                product_id: a.chair.clone(),
                location_id: b.warehouse.clone(),
                min_qty_milli: 1_000,
                target_qty_milli: 2_000,
                active: true,
            })
            .await,
    );

    // ---- update ----------------------------------------------------------
    let limits = ReorderLimits {
        min_qty_milli: 6_000,
        target_qty_milli: 30_000,
        active: false,
    };
    assert_not_found(b.door.update_inv_reorder_rule(&id, &limits).await);
    a.door.update_inv_reorder_rule(&id, &limits).await.unwrap();
    let parked = a.door.inv_reorder_rule(&id).await.unwrap().unwrap();
    assert_eq!(parked.min_qty_milli, 6_000);
    assert_eq!(parked.target_qty_milli, 30_000);
    assert!(!parked.active);
    assert!(
        a.door
            .inv_reorder_rules(&ReorderRuleFilter::default())
            .await
            .unwrap()
            .is_empty(),
        "a parked rule is not what the tenant is watching"
    );
    assert_eq!(
        a.door
            .inv_reorder_rules(&ReorderRuleFilter {
                include_inactive: true,
                ..Default::default()
            })
            .await
            .unwrap()
            .len(),
        1,
        "…but it is still there when asked for"
    );

    // ---- delete ----------------------------------------------------------
    assert_not_found(b.door.delete_inv_reorder_rule(&id).await);
    assert!(
        a.door.inv_reorder_rule(&id).await.unwrap().is_some(),
        "the outsider's delete left the rule alone"
    );
    a.door.delete_inv_reorder_rule(&id).await.unwrap();
    assert!(a.door.inv_reorder_rule(&id).await.unwrap().is_none());
    assert_not_found(a.door.delete_inv_reorder_rule(&id).await);
}

#[tokio::test]
async fn a_rule_needs_a_stocked_product_at_a_real_place() {
    let store = common::test_store().await;
    let a = Buying::open(&store, "shape").await;

    let rule = |product: BillingProductId, location: InvLocationId, min, target| NewReorderRule {
        product_id: product,
        location_id: location,
        min_qty_milli: min,
        target_qty_milli: target,
        active: true,
    };

    // A service has no on-hand to be under, so a rule about one could never come
    // true.
    let delivery = a
        .door
        .create_billing_product(&NewProduct {
            name: "Assembly".to_owned(),
            unit_price_cents: 6_000,
            vat_rate_bp: 1900,
            stocked: false,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        invalid(
            a.door
                .create_inv_reorder_rule(&rule(delivery, a.warehouse.clone(), 1_000, 2_000))
                .await
        )
        .contains("service"),
    );

    // The supplier counterparty holds a number that is negative by
    // construction; a minimum on it is a minimum on nothing.
    assert!(
        invalid(
            a.door
                .create_inv_reorder_rule(&rule(
                    a.chair.clone(),
                    a.supplier_location.clone(),
                    1_000,
                    2_000
                ))
                .await
        )
        .contains("real stock location"),
    );

    // The numbers.
    assert!(
        invalid(
            a.door
                .create_inv_reorder_rule(&rule(
                    a.chair.clone(),
                    a.warehouse.clone(),
                    20_000,
                    10_000
                ))
                .await
        )
        .contains("below the minimum"),
    );
    assert!(
        invalid(
            a.door
                .create_inv_reorder_rule(&rule(a.chair.clone(), a.warehouse.clone(), -1, 10_000))
                .await
        )
        .contains("minimum quantity"),
    );

    // One rule per pair: a second is a refusal, never a second row.
    let id = a.watch_chairs().await;
    assert!(
        conflict(
            a.door
                .create_inv_reorder_rule(&rule(a.chair.clone(), a.warehouse.clone(), 1_000, 2_000))
                .await
        )
        .contains("already watched"),
    );
    // …and the refusal wrote nothing.
    assert_eq!(
        a.door
            .inv_reorder_rules(&ReorderRuleFilter::default())
            .await
            .unwrap()
            .len(),
        1
    );

    // An archived end cannot be picked up again by a new rule.
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
                .create_inv_reorder_rule(&rule(a.chair.clone(), shed, 1_000, 2_000))
                .await
        )
        .contains("archived location"),
    );

    // An update is held to the same arithmetic as a create.
    assert!(
        invalid(
            a.door
                .update_inv_reorder_rule(
                    &id,
                    &ReorderLimits {
                        min_qty_milli: 20_000,
                        target_qty_milli: 10_000,
                        active: true,
                    }
                )
                .await
        )
        .contains("below the minimum"),
    );
}

#[tokio::test]
async fn the_report_counts_the_shelf_the_open_orders_and_the_promises() {
    let store = common::test_store().await;
    let a = Buying::open(&store, "arith").await;
    a.watch_chairs().await;

    // ---- nothing anywhere: short by the whole minimum ---------------------
    let empty = a.only_shortage().await;
    assert_eq!(empty.on_hand_qty_milli, 0);
    assert_eq!(empty.on_order_qty_milli, 0);
    assert_eq!(empty.committed_qty_milli, 0);
    assert_eq!(empty.available_qty_milli, 0);
    assert_eq!(empty.short_by_qty_milli, 4_000);
    // Twenty to reach the target, and they will not sell under ten — twenty is
    // already more than ten, so twenty it is.
    assert_eq!(empty.buy_qty_milli, 20_000);
    let supplier = empty.supplier.as_ref().expect("the quoting supplier");
    assert_eq!(supplier.supplier_id, a.supplier);
    assert_eq!(supplier.supplier_code, "HM-4471");
    assert_eq!(supplier.purchase_price_cents, 3_150);
    assert_eq!(supplier.currency, "EUR");
    assert_eq!(supplier.min_order_qty_milli, 10_000);
    assert_eq!(
        supplier.lead_time_days, 9,
        "the offer states none, so the supplier's own default applies"
    );
    // 20 × €31.50, in integer cents, at the price THEY quote — not ours.
    assert_eq!(empty.estimated_cost_cents, 63_000);

    // ---- two on the shelf: still short ------------------------------------
    a.receive(2_000).await;
    let thin = a.only_shortage().await;
    assert_eq!(thin.on_hand_qty_milli, 2_000);
    assert_eq!(thin.available_qty_milli, 2_000);
    assert_eq!(thin.short_by_qty_milli, 2_000);
    assert_eq!(thin.buy_qty_milli, 18_000);

    // ---- five on the shelf: not short at all ------------------------------
    a.receive(3_000).await;
    assert!(
        a.shortages().await.is_empty(),
        "five is over the minimum of four"
    );

    // ---- promise seven to a customer: short again -------------------------
    a.promise_chairs(7_000).await;
    let promised = a.only_shortage().await;
    assert_eq!(promised.on_hand_qty_milli, 5_000);
    assert_eq!(promised.committed_qty_milli, 7_000);
    assert_eq!(
        promised.available_qty_milli, -2_000,
        "more promised than exists is legitimately negative"
    );
    assert_eq!(promised.short_by_qty_milli, 6_000);
    assert_eq!(promised.buy_qty_milli, 22_000);

    // ---- order thirty from the supplier: no longer short ------------------
    // This is the number that stops the report repeating itself every morning.
    a.order_chairs(30_000).await;
    assert!(
        a.shortages().await.is_empty(),
        "a shortage already on order is not a shortage"
    );

    // ---- narrowing ---------------------------------------------------------
    a.promise_chairs(40_000).await;
    let short = a.only_shortage().await;
    assert_eq!(short.on_order_qty_milli, 30_000);
    assert_eq!(short.committed_qty_milli, 47_000);
    for (label, filter, expected) in [
        (
            "our own warehouse",
            ShortageFilter {
                location_id: Some(a.warehouse.clone()),
                ..Default::default()
            },
            1,
        ),
        (
            "the counterparty nobody keeps stock at",
            ShortageFilter {
                location_id: Some(a.supplier_location.clone()),
                ..Default::default()
            },
            0,
        ),
        (
            "the product itself",
            ShortageFilter {
                product_id: Some(a.chair.clone()),
                ..Default::default()
            },
            1,
        ),
        (
            "the supplier who quotes for it",
            ShortageFilter {
                supplier_id: Some(a.supplier.clone()),
                ..Default::default()
            },
            1,
        ),
    ] {
        assert_eq!(
            a.door.inv_shortages(&filter).await.unwrap().len(),
            expected,
            "narrowing by {label}"
        );
    }
}

#[tokio::test]
async fn a_parked_rule_and_an_archived_end_report_nothing() {
    let store = common::test_store().await;
    let a = Buying::open(&store, "quiet").await;
    let id = a.watch_chairs().await;
    assert_eq!(a.shortages().await.len(), 1, "short to start with");

    // Parked: the numbers stay, the report goes quiet.
    a.door
        .update_inv_reorder_rule(
            &id,
            &ReorderLimits {
                min_qty_milli: 4_000,
                target_qty_milli: 20_000,
                active: false,
            },
        )
        .await
        .unwrap();
    assert!(a.shortages().await.is_empty());

    a.door
        .update_inv_reorder_rule(
            &id,
            &ReorderLimits {
                min_qty_milli: 4_000,
                target_qty_milli: 20_000,
                active: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(a.shortages().await.len(), 1);

    // A shelf we are emptying on purpose is not a shortage.
    a.door
        .set_inv_location_archived(&a.warehouse, true)
        .await
        .unwrap();
    assert!(a.shortages().await.is_empty());
    a.door
        .set_inv_location_archived(&a.warehouse, false)
        .await
        .unwrap();

    // Neither is a product we have stopped selling.
    a.door
        .set_billing_product_archived(&a.chair, true)
        .await
        .unwrap();
    assert!(a.shortages().await.is_empty());
}

#[tokio::test]
async fn one_tenants_shortages_are_never_another_tenants() {
    let store = common::test_store().await;
    let a = Buying::open(&store, "own").await;
    let b = Buying::open(&store, "other").await;
    a.watch_chairs().await;
    b.watch_chairs().await;

    // B fills their shelf, orders more and promises none. A does nothing.
    b.receive(50_000).await;
    b.order_chairs(100_000).await;
    assert!(
        b.shortages().await.is_empty(),
        "B is well stocked by their own numbers"
    );

    // A is still short by exactly their own minimum: not one unit of B's stock,
    // not one line of B's order, reaches this answer.
    let only = a.only_shortage().await;
    assert_eq!(only.on_hand_qty_milli, 0);
    assert_eq!(only.on_order_qty_milli, 0);
    assert_eq!(only.committed_qty_milli, 0);
    assert_eq!(only.short_by_qty_milli, 4_000);
    assert_eq!(only.product_id, a.chair);
    assert_eq!(only.location_id, a.warehouse);
    assert_eq!(
        only.supplier.as_ref().map(|s| s.supplier_id.clone()),
        Some(a.supplier.clone()),
        "and the supplier proposed is A's own"
    );

    // The mirror: A promises everything away, and B's report stays empty.
    a.promise_chairs(500_000).await;
    assert!(
        b.shortages().await.is_empty(),
        "A's promises are invisible to B"
    );
}

#[tokio::test]
async fn the_pipeline_of_one_product_is_read_without_a_rule_and_never_crosses_a_tenant() {
    let store = common::test_store().await;
    let a = Buying::open(&store, "pipe-own").await;
    let b = Buying::open(&store, "pipe-other").await;

    // Nobody watches this chair — which is the whole point of the read: the
    // agent's stock answer is asked about products that have no rule at all
    // (B5.10), and a fold that only worked for watched items would answer half
    // the catalog.
    assert!(
        a.door
            .inv_reorder_rules(&ReorderRuleFilter::default())
            .await
            .unwrap()
            .is_empty()
    );
    let quiet = a.door.inv_product_pipeline(&a.chair).await.unwrap();
    assert_eq!(quiet.on_order_qty_milli, 0);
    assert_eq!(quiet.committed_qty_milli, 0);

    // A places one order and promises some away; B does the same, larger.
    a.order_chairs(30_000).await;
    a.promise_chairs(7_000).await;
    b.order_chairs(900_000).await;
    b.promise_chairs(800_000).await;

    let mine = a.door.inv_product_pipeline(&a.chair).await.unwrap();
    assert_eq!(mine.on_order_qty_milli, 30_000);
    assert_eq!(mine.committed_qty_milli, 7_000);
    // The same two numbers the shortage report states, from the same folds: two
    // readings of "on order" that disagreed would be two truths about one
    // warehouse. Promising forty more takes A under their minimum, which is
    // what puts a row in the report to compare against.
    a.watch_chairs().await;
    a.promise_chairs(40_000).await;
    let reported = a.only_shortage().await;
    let after = a.door.inv_product_pipeline(&a.chair).await.unwrap();
    assert_eq!(after.committed_qty_milli, 47_000);
    assert_eq!(reported.on_order_qty_milli, after.on_order_qty_milli);
    assert_eq!(reported.committed_qty_milli, after.committed_qty_milli);

    // Wrong tenant: A asking about B's chair — a real id, in another tenant's
    // catalog — folds none of B's lines. Two zeroes, never B's figures and
    // never a 500.
    let foreign = a.door.inv_product_pipeline(&b.chair).await.unwrap();
    assert_eq!(foreign.on_order_qty_milli, 0);
    assert_eq!(foreign.committed_qty_milli, 0);
    // And the mirror, so the isolation is not an accident of who asked first.
    let theirs = b.door.inv_product_pipeline(&b.chair).await.unwrap();
    assert_eq!(theirs.on_order_qty_milli, 900_000);
    assert_eq!(theirs.committed_qty_milli, 800_000);
    assert_eq!(
        b.door.inv_product_pipeline(&a.chair).await.unwrap(),
        alo_store::inv_reorder::ProductPipeline::default()
    );
}
