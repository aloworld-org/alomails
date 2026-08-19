//! What confirming a sales order may promise (alo Orders, ADR 0054 §3) against
//! the real database.
//!
//! **The one test this suite exists for is the race: two confirmations for the
//! last fan, exactly one of which may be allowed to promise it.** Before the
//! refusal existed, both succeeded — which is the failure the whole wave is
//! named after, and it is invisible to any single-threaded test.
//!
//! Everything else is the frame around it:
//!
//! | Property | Where |
//! |---|---|
//! | two simultaneous confirmations, exactly one promise | `two_confirmations_for_the_last_fan_leave_exactly_one_promise` |
//! | a refusal says the product and how many are short | `an_order_beyond_the_shelf_is_refused_with_what_is_short` |
//! | a refusal writes nothing: no number, no date, still a draft | `nothing_a_refused_confirmation_asked_for_reaches_the_order` |
//! | goods already ordered from a supplier may be promised | `what_is_already_on_order_from_a_supplier_may_be_promised` |
//! | a service line has no shelf and never blocks | `a_service_promises_nothing_and_never_blocks_an_order` |
//! | delivering releases the promise, with no hook to forget | `delivering_releases_the_promise_for_the_next_order` |
//! | cancelling releases it too | `cancelling_releases_the_promise_for_the_next_order` |
//! | a seller who says so may promise goods they will buy | `a_seller_who_says_so_may_promise_goods_they_will_buy` |
//! | a neighbour’s shelf can never satisfy our promise | `another_tenants_stock_can_never_back_our_promise` |
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::billing_line::NewLine;
use alo_store::inv_locations::{Location, LocationKind, LocationSeed};
use alo_store::inv_moves::{MoveReason, NewMove};
use alo_store::inv_po::NewPurchaseOrder;
use alo_store::inv_po_lines::NewPoLine;
use alo_store::inv_so::{NewSalesOrder, SoStatus};
use alo_store::inv_so_deliver::{NewDelivery, NewDeliveryLine};
use alo_store::inv_so_lines::NewSoLine;
use alo_store::inv_suppliers::NewSupplier;
use alo_store::{
    AccountStore, BillingCustomerId, BillingProductId, InvLocationId, InvSalesOrderId, NewCustomer,
    NewProduct, Store, StoreError,
};

fn conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(said)) => said,
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

/// One unit, in the milli-units every quantity in this suite is stated in.
const UNIT: i64 = 1_000;

/// A tenant selling fans: locations, a customer, a stocked product, and however
/// many units were put on the shelf.
struct Selling {
    door: AccountStore,
    fan: BillingProductId,
    warehouse: InvLocationId,
    supplier_location: InvLocationId,
    customer: BillingCustomerId,
}

impl Selling {
    async fn open(store: &Store, tag: &str, on_hand_units: i64) -> Self {
        let tenant = store.create_tenant(&format!("commit-{tag}")).await.unwrap();
        let user = store
            .for_tenant(tenant.clone())
            .create_user(&format!("{tag}@commit.test"))
            .await
            .unwrap();
        let door = store.for_account(tenant, user);
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
        let customer = door
            .create_billing_customer(&NewCustomer {
                name: format!("Koelhuis {tag}"),
                address_line1: "Keizersgracht 1".to_owned(),
                postal_code: "1015".to_owned(),
                city: "Amsterdam".to_owned(),
                country: "NL".to_owned(),
                currency: "EUR".to_owned(),
                payment_terms_days: 30,
                ..Default::default()
            })
            .await
            .unwrap();
        let fan = door
            .create_billing_product(&NewProduct {
                name: "AF-630 axial fan".to_owned(),
                unit: "piece".to_owned(),
                unit_price_cents: 129_500,
                vat_rate_bp: 2100,
                stocked: true,
                purchase_price_cents: 74_000,
                ..Default::default()
            })
            .await
            .unwrap();
        let yard = Self {
            warehouse: of(LocationKind::Stock),
            supplier_location: of(LocationKind::Supplier),
            fan,
            customer,
            door,
        };
        if on_hand_units > 0 {
            yard.stock_up(on_hand_units * UNIT).await;
        }
        yard
    }

    /// Puts goods on the shelf the only way this ledger allows: a movement in
    /// from the virtual supplier.
    async fn stock_up(&self, qty_milli: i64) {
        self.door
            .record_move(&NewMove {
                product_id: self.fan.clone(),
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

    /// A **draft** order for `units` fans, ready to be confirmed.
    async fn draft_for(&self, units: i64) -> InvSalesOrderId {
        let order = self
            .door
            .create_inv_sales_order(&NewSalesOrder::for_customer(self.customer.clone()))
            .await
            .unwrap();
        self.door
            .set_inv_sales_order_lines(
                &order,
                &[NewSoLine {
                    product_id: Some(self.fan.clone()),
                    line: NewLine {
                        description: "AF-630 axial fan".to_owned(),
                        unit: "piece".to_owned(),
                        qty_milli: units * UNIT,
                        unit_price_cents: 129_500,
                        vat_rate_bp: 2100,
                    },
                }],
            )
            .await
            .unwrap();
        order
    }

    /// A draft order for one service line and nothing stocked.
    async fn draft_for_a_service(&self) -> InvSalesOrderId {
        let order = self
            .door
            .create_inv_sales_order(&NewSalesOrder::for_customer(self.customer.clone()))
            .await
            .unwrap();
        self.door
            .set_inv_sales_order_lines(
                &order,
                &[NewSoLine {
                    product_id: None,
                    line: NewLine {
                        description: "Commissioning, two days".to_owned(),
                        unit: "day".to_owned(),
                        qty_milli: 2 * UNIT,
                        unit_price_cents: 95_000,
                        vat_rate_bp: 2100,
                    },
                }],
            )
            .await
            .unwrap();
        order
    }

    /// Places `units` on a supplier and **sends** the order, so they count as
    /// on their way in.
    async fn put_on_order(&self, units: i64) {
        let supplier = self
            .door
            .create_inv_supplier(&NewSupplier {
                name: "Hoffmann Ventilatoren".to_owned(),
                address_line1: "Industriestrasse 4".to_owned(),
                postal_code: "40210".to_owned(),
                city: "Duesseldorf".to_owned(),
                country: "DE".to_owned(),
                ..Default::default()
            })
            .await
            .unwrap();
        let po = self
            .door
            .create_inv_purchase_order(&NewPurchaseOrder::for_supplier(supplier))
            .await
            .unwrap();
        self.door
            .set_inv_purchase_order_lines(
                &po,
                &[NewPoLine {
                    product_id: Some(self.fan.clone()),
                    line: NewLine {
                        description: "AF-630 axial fan".to_owned(),
                        unit: "piece".to_owned(),
                        qty_milli: units * UNIT,
                        unit_price_cents: 74_000,
                        vat_rate_bp: 2100,
                    },
                }],
            )
            .await
            .unwrap();
        self.door
            .send_inv_purchase_order::<(), StoreError, _, _>(&po, |_| async { Ok(()) })
            .await
            .unwrap();
    }

    /// The id of an order's first line, as a caller would have read it.
    async fn first_line(&self, order: &InvSalesOrderId) -> alo_store::BillingLineId {
        self.door
            .inv_sales_order(order)
            .await
            .unwrap()
            .unwrap()
            .lines[0]
            .line
            .id
            .clone()
    }

    async fn status(&self, order: &InvSalesOrderId) -> SoStatus {
        self.door
            .inv_sales_order(order)
            .await
            .unwrap()
            .unwrap()
            .order
            .status
    }
}

// ---------------------------------------------------------------------------
// The race this suite exists for
// ---------------------------------------------------------------------------

#[tokio::test]
async fn two_confirmations_for_the_last_fan_leave_exactly_one_promise() {
    // One fan on the shelf and two customers who both want it. Before the
    // refusal existed BOTH of these succeeded — the read of what is committed
    // and the write that commits it were not one decision, so two orders could
    // interleave between them and each see a free fan.
    //
    // Deliberately two separate orders rather than one confirmed twice: the
    // order's own row lock already stops the second, and a test that passed on
    // that lock would prove nothing about the shelf.
    let store = common::test_store().await;
    let yard = Selling::open(&store, "race", 1).await;
    let first = yard.draft_for(1).await;
    let second = yard.draft_for(1).await;

    let (a, b) = tokio::join!(
        yard.door.confirm_inv_sales_order(&first, false),
        yard.door.confirm_inv_sales_order(&second, false)
    );

    let won = [&a, &b].iter().filter(|r| r.is_ok()).count();
    assert_eq!(
        won, 1,
        "exactly one of two confirmations for the last fan may be allowed to \
         promise it; got first={a:?} second={b:?}"
    );

    // And the loser is refused in words a salesperson could repeat to the
    // customer, not with a stack trace.
    let refused = match (a, b) {
        (Err(e), Ok(_)) | (Ok(_), Err(e)) => conflict(Err::<(), _>(e)),
        other => panic!("expected exactly one refusal, got {other:?}"),
    };
    assert!(
        refused.to_lowercase().contains("af-630"),
        "the refusal must name the product: {refused}"
    );

    // The shelf is untouched by either: confirming promises, it never moves
    // goods (`inv_so_confirm.rs`).
    assert_eq!(
        yard.door
            .inv_on_hand(&yard.fan, &yard.warehouse)
            .await
            .unwrap(),
        UNIT
    );
}

// ---------------------------------------------------------------------------
// The refusal, single-threaded
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_order_beyond_the_shelf_is_refused_with_what_is_short() {
    let store = common::test_store().await;
    let yard = Selling::open(&store, "short", 4).await;
    let six = yard.draft_for(6).await;

    let refused = conflict(yard.door.confirm_inv_sales_order(&six, false).await);
    assert!(
        refused.to_lowercase().contains("af-630"),
        "name the product: {refused}"
    );
    assert!(
        refused.contains('2'),
        "say how many are short — six wanted against four available is two: {refused}"
    );

    // Four fits exactly. The boundary is the whole point: `available` is a
    // quantity that may be promised in full, not one to stay under.
    let four = yard.draft_for(4).await;
    yard.door
        .confirm_inv_sales_order(&four, false)
        .await
        .unwrap();
    assert_eq!(yard.status(&four).await, SoStatus::Confirmed);

    // And now the shelf is spoken for, so even one more is refused.
    let one_more = yard.draft_for(1).await;
    assert!(
        conflict(yard.door.confirm_inv_sales_order(&one_more, false).await)
            .to_lowercase()
            .contains("af-630")
    );
}

#[tokio::test]
async fn nothing_a_refused_confirmation_asked_for_reaches_the_order() {
    // A refusal that half-wrote would be worse than no refusal: an order with a
    // number drawn from the series but still a draft is a hole in a sequence a
    // customer's bookkeeping can see.
    let store = common::test_store().await;
    let yard = Selling::open(&store, "nothing", 1).await;
    let too_many = yard.draft_for(9).await;

    let _ = conflict(yard.door.confirm_inv_sales_order(&too_many, false).await);

    let read = yard.door.inv_sales_order(&too_many).await.unwrap().unwrap();
    assert_eq!(read.order.status, SoStatus::Draft, "still a draft");
    assert!(read.order.number.is_none(), "no number was drawn");
    assert!(read.order.confirmed_date.is_none(), "no day was stamped");
    // It is still editable and still deletable — the order was left exactly as
    // it was found.
    yard.door.delete_inv_sales_order(&too_many).await.unwrap();
}

// ---------------------------------------------------------------------------
// What may be promised
// ---------------------------------------------------------------------------

#[tokio::test]
async fn what_is_already_on_order_from_a_supplier_may_be_promised() {
    // Nothing on the shelf, six on their way in. A business that may not
    // promise what it has already bought cannot take an order at all, and
    // `available = on_hand + on_order − committed` is the reorder report's own
    // arithmetic (ADR 0054 §3) rather than a second reading invented here.
    let store = common::test_store().await;
    let yard = Selling::open(&store, "onorder", 0).await;
    let six = yard.draft_for(6).await;
    assert!(
        conflict(yard.door.confirm_inv_sales_order(&six, false).await)
            .to_lowercase()
            .contains("af-630"),
        "with nothing on hand and nothing ordered, six may not be promised"
    );

    yard.put_on_order(6).await;
    yard.door
        .confirm_inv_sales_order(&six, false)
        .await
        .unwrap();
    assert_eq!(yard.status(&six).await, SoStatus::Confirmed);

    // And the seventh is still refused: what is on order is finite too.
    let seventh = yard.draft_for(1).await;
    assert!(
        conflict(yard.door.confirm_inv_sales_order(&seventh, false).await)
            .to_lowercase()
            .contains("af-630")
    );
}

#[tokio::test]
async fn a_service_promises_nothing_and_never_blocks_an_order() {
    // A quote of consultancy days has no shelf to draw on. An empty warehouse
    // must not stop it, or the refusal would break the flow that has always
    // worked for services.
    let store = common::test_store().await;
    let yard = Selling::open(&store, "service", 0).await;
    let commissioning = yard.draft_for_a_service().await;
    yard.door
        .confirm_inv_sales_order(&commissioning, false)
        .await
        .unwrap();
    assert_eq!(yard.status(&commissioning).await, SoStatus::Confirmed);
}

#[tokio::test]
async fn a_seller_who_says_so_may_promise_goods_they_will_buy() {
    // The escape, and the reason it exists. `inv_reorder`'s own tests state that
    // "more promised than exists is legitimately negative" — that is the state
    // its shortage report was built to report, and a refusal with no way past it
    // would make that state unreachable and the report poorer.
    //
    // So the rule is not "never over-promise", it is "never over-promise by
    // accident". The same shape as short-closing a part-delivered order: the
    // seller says it out loud.
    let store = common::test_store().await;
    let yard = Selling::open(&store, "backorder", 1).await;

    let ten = yard.draft_for(10).await;
    let refused = conflict(yard.door.confirm_inv_sales_order(&ten, false).await);
    assert!(refused.contains("short by 9"), "{refused}");

    // Said out loud, it is taken.
    yard.door.confirm_inv_sales_order(&ten, true).await.unwrap();
    assert_eq!(yard.status(&ten).await, SoStatus::Confirmed);

    // And the shortage is now visible where a buyer looks, as a negative
    // availability rather than as a hidden problem — which is the whole point of
    // allowing it.
    let pipeline = yard.door.inv_product_pipeline(&yard.fan).await.unwrap();
    assert_eq!(pipeline.committed_qty_milli, 10 * UNIT);
    assert_eq!(
        alo_store::inv_reorder::available_qty_milli(
            UNIT,
            pipeline.on_order_qty_milli,
            pipeline.committed_qty_milli
        ),
        -9 * UNIT,
        "the promise the seller chose to make is exactly what the report shows"
    );

    // A backorder is a decision about *this* order and never a setting: the next
    // one is refused again unless it, too, says so.
    let another = yard.draft_for(1).await;
    let refused_again = conflict(yard.door.confirm_inv_sales_order(&another, false).await);
    assert!(
        refused_again.contains("already promised 10"),
        "{refused_again}"
    );
}

// ---------------------------------------------------------------------------
// The promise releases itself
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delivering_releases_the_promise_for_the_next_order() {
    // `committed` is the *undelivered* remainder, so what has gone out stops
    // counting the moment it goes — no hook anywhere to forget (ADR 0054 §3).
    // Two fans, promised and shipped, and the shelf refilled: the next order
    // fits because the first is no longer outstanding, not because anything
    // released it.
    let store = common::test_store().await;
    let yard = Selling::open(&store, "deliver", 2).await;
    let first = yard.draft_for(2).await;
    yard.door
        .confirm_inv_sales_order(&first, false)
        .await
        .unwrap();

    let second = yard.draft_for(2).await;
    assert!(
        conflict(yard.door.confirm_inv_sales_order(&second, false).await)
            .to_lowercase()
            .contains("af-630"),
        "while the first order is outstanding the fans are spoken for"
    );

    let line = yard.first_line(&first).await;
    yard.door
        .deliver_inv_sales_order(
            &first,
            &NewDelivery {
                location_id: yard.warehouse.clone(),
                lines: Some(vec![NewDeliveryLine {
                    so_line_id: line,
                    qty_milli: 2 * UNIT,
                }]),
                note: String::new(),
            },
        )
        .await
        .unwrap();
    assert_eq!(yard.status(&first).await, SoStatus::Delivered);

    // The shelf is empty now, so the second still cannot be promised — for the
    // right reason this time.
    assert_eq!(
        yard.door
            .inv_on_hand(&yard.fan, &yard.warehouse)
            .await
            .unwrap(),
        0
    );
    let _ = conflict(yard.door.confirm_inv_sales_order(&second, false).await);

    // Restock, and it fits: the delivered order releases nothing because it is
    // holding nothing.
    yard.stock_up(2 * UNIT).await;
    yard.door
        .confirm_inv_sales_order(&second, false)
        .await
        .unwrap();
}

#[tokio::test]
async fn cancelling_releases_the_promise_for_the_next_order() {
    // A cancelled order is not one of the two reserving states, so its
    // remainder stops counting with no hook to forget.
    let store = common::test_store().await;
    let yard = Selling::open(&store, "cancel", 2).await;
    let dropped = yard.draft_for(2).await;
    yard.door
        .confirm_inv_sales_order(&dropped, false)
        .await
        .unwrap();

    let wanted = yard.draft_for(2).await;
    let _ = conflict(yard.door.confirm_inv_sales_order(&wanted, false).await);

    yard.door
        .cancel_inv_sales_order(&dropped, true)
        .await
        .unwrap();
    yard.door
        .confirm_inv_sales_order(&wanted, false)
        .await
        .unwrap();
    assert_eq!(yard.status(&wanted).await, SoStatus::Confirmed);
}

// ---------------------------------------------------------------------------
// The tenant wall
// ---------------------------------------------------------------------------

#[tokio::test]
async fn another_tenants_stock_can_never_back_our_promise() {
    // The mandatory wrong-tenant test, in the shape this module could actually
    // break it: a neighbour with a full warehouse of the same kind of fan must
    // not make our empty one look stocked, and our own confirmed orders must not
    // eat into theirs.
    let store = common::test_store().await;
    let ours = Selling::open(&store, "wall-ours", 0).await;
    let theirs = Selling::open(&store, "wall-theirs", 50).await;

    let ours_order = ours.draft_for(1).await;
    assert!(
        conflict(ours.door.confirm_inv_sales_order(&ours_order, false).await)
            .to_lowercase()
            .contains("af-630"),
        "a neighbour's fifty fans are not ours to promise"
    );

    // Theirs is unaffected in both directions: they may still promise all fifty.
    let theirs_order = theirs.draft_for(50).await;
    theirs
        .door
        .confirm_inv_sales_order(&theirs_order, false)
        .await
        .unwrap();
    assert_eq!(theirs.status(&theirs_order).await, SoStatus::Confirmed);

    // And their confirmed fifty do not make our empty warehouse any emptier —
    // our refusal is about our own shelf, not theirs.
    ours.stock_up(UNIT).await;
    ours.door
        .confirm_inv_sales_order(&ours_order, false)
        .await
        .unwrap();
}
