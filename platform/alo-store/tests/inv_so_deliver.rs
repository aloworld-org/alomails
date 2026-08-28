//! **Delivering** a sales order (alo Inventory, wave B5.06a) against the real
//! database — the two things one consignment does, proved to happen together or
//! not at all, and the refusal the whole moves-only ledger exists for.
//!
//! [`alo_store::inv_so_deliver`]'s unit tests already prove the pure rules —
//! what a stated consignment resolves to, when an order is complete, what an
//! over-delivery is refused with. What only a database can prove is what this
//! suite is for:
//!
//! | Property | Where |
//! |---|---|
//! | confirming numbers the order and freezes it, and moves no stock | `confirming_numbers_the_order_freezes_it_and_moves_nothing` |
//! | the goods move out and the order advances, in one act | `a_delivery_moves_the_goods_out_and_advances_the_order` |
//! | the rest of a consignment closes the order | `the_last_delivery_closes_the_order` |
//! | **you cannot ship what you do not have**, and nothing is written | `a_shelf_that_has_not_got_the_goods_refuses_the_whole_delivery` |
//! | a refused delivery writes nothing at all — no movement, no accumulator | `nothing_a_refused_delivery_asked_for_reaches_the_ledger` |
//! | goods leave only against an order we confirmed | `a_draft_and_a_cancelled_order_deliver_nothing` |
//! | cancelling a part-delivered order un-delivers nothing | `cancelling_after_a_delivery_closes_the_remainder_and_moves_nothing_back` |
//! | one tenant's order can never be delivered or read by another | `another_tenants_order_can_never_be_delivered` |
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::inv_locations::{Location, LocationKind, LocationSeed};
use alo_store::inv_moves::{MoveFilter, MoveReason, MoveRefKind};
use alo_store::inv_so::{NewSalesOrder, SoStatus};
use alo_store::inv_so_deliver::{NewDelivery, NewDeliveryLine};
use alo_store::inv_so_lines::NewSoLine;
use alo_store::{
    AccountStore, BillingCustomerId, BillingLineId, BillingProductId, InvLocationId,
    InvSalesOrderId, NewCustomer, NewLine, NewProduct, Store, StoreError, TenantId,
};

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

/// A tenant with locations, a customer, a product with four chairs actually on
/// the shelf, and a **confirmed** order for those four chairs plus a delivery
/// charge — the state every consignment here leaves from.
struct Selling {
    door: AccountStore,
    tenant: TenantId,
    chair: BillingProductId,
    warehouse: InvLocationId,
    customer_location: InvLocationId,
    supplier_location: InvLocationId,
    customer: BillingCustomerId,
    order: InvSalesOrderId,
}

impl Selling {
    /// Opens the yard with `on_hand` milli-units of chairs already in the
    /// warehouse — planted as a movement from the virtual `supplier` location,
    /// which is how stock enters this ledger at all.
    async fn open(store: &Store, tag: &str, on_hand: i64) -> Self {
        let tenant = store.create_tenant(&format!("sell-{tag}")).await.unwrap();
        let user = store
            .for_tenant(tenant.clone())
            .create_user(&format!("{tag}@selling.test"))
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
        let customer = door
            .create_billing_customer(&NewCustomer {
                name: format!("Meubelhuis {tag}"),
                address_line1: "Keizersgracht 1".to_owned(),
                postal_code: "1015".to_owned(),
                city: "Amsterdam".to_owned(),
                country: "NL".to_owned(),
                currency: "EUR".to_owned(),
                email: Some(format!("inkoop+{tag}@meubelhuis.test")),
                payment_terms_days: 30,
                ..Default::default()
            })
            .await
            .unwrap();
        let chair = door
            .create_billing_product(&NewProduct {
                name: "Blue chair".to_owned(),
                unit: "piece".to_owned(),
                unit_price_cents: 8_600,
                vat_rate_bp: 2100,
                stocked: true,
                purchase_price_cents: 4_300,
                ..Default::default()
            })
            .await
            .unwrap();

        let yard = Self {
            warehouse: of(LocationKind::Stock),
            customer_location: of(LocationKind::Customer),
            supplier_location: of(LocationKind::Supplier),
            chair,
            customer,
            order: InvSalesOrderId::new(""),
            door,
            tenant,
        };
        if on_hand > 0 {
            yard.stock_up(on_hand).await;
        }
        let order = yard.confirmed_order().await;
        Self { order, ..yard }
    }

    /// Puts goods on the shelf the only way this ledger allows: a movement in
    /// from the virtual supplier.
    async fn stock_up(&self, qty_milli: i64) {
        self.door
            .record_move(&alo_store::inv_moves::NewMove {
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

    /// A confirmed order for four chairs and a delivery charge.
    async fn confirmed_order(&self) -> InvSalesOrderId {
        let order = self
            .door
            .create_inv_sales_order(&NewSalesOrder {
                reference: "Their PO 4711".to_owned(),
                ..NewSalesOrder::for_customer(self.customer.clone())
            })
            .await
            .unwrap();
        self.door
            .set_inv_sales_order_lines(
                &order,
                &[
                    NewSoLine {
                        product_id: Some(self.chair.clone()),
                        line: NewLine {
                            description: "Blue chair".to_owned(),
                            unit: "piece".to_owned(),
                            qty_milli: 4_000,
                            unit_price_cents: 8_600,
                            vat_rate_bp: 2100,
                        },
                    },
                    NewSoLine {
                        product_id: None,
                        line: NewLine {
                            description: "Delivery to the third floor".to_owned(),
                            unit: String::new(),
                            qty_milli: 1_000,
                            unit_price_cents: 4_500,
                            vat_rate_bp: 2100,
                        },
                    },
                ],
            )
            .await
            .unwrap();
        self.door
            .confirm_inv_sales_order(&order, true)
            .await
            .unwrap();
        order
    }

    /// The id of the order's nth line (1-based), as a caller would have read it.
    async fn line(&self, position: usize) -> BillingLineId {
        self.door
            .inv_sales_order(&self.order)
            .await
            .unwrap()
            .unwrap()
            .lines[position - 1]
            .line
            .id
            .clone()
    }

    fn consignment(&self, lines: Option<Vec<NewDeliveryLine>>) -> NewDelivery {
        NewDelivery {
            location_id: self.warehouse.clone(),
            lines,
            note: String::new(),
        }
    }

    async fn on_hand(&self, location: &InvLocationId) -> i64 {
        self.door.inv_on_hand(&self.chair, location).await.unwrap()
    }

    async fn moves(&self) -> usize {
        self.door
            .inv_moves(&MoveFilter::default())
            .await
            .unwrap()
            .len()
    }

    async fn status(&self) -> SoStatus {
        self.door
            .inv_sales_order(&self.order)
            .await
            .unwrap()
            .unwrap()
            .order
            .status
    }

    async fn delivered(&self, position: usize) -> i64 {
        self.door
            .inv_sales_order(&self.order)
            .await
            .unwrap()
            .unwrap()
            .lines[position - 1]
            .delivered_qty_milli
    }
}

#[tokio::test]
async fn confirming_numbers_the_order_freezes_it_and_moves_nothing() {
    let store = common::test_store().await;
    let yard = Selling::open(&store, "confirm", 4_000).await;

    let document = yard
        .door
        .inv_sales_order(&yard.order)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(document.order.status, SoStatus::Confirmed);
    let number = document
        .order
        .number
        .clone()
        .expect("a confirmed order is numbered");
    assert!(number.starts_with("SO-"), "{number}");
    assert!(document.order.confirmed_date.is_some());
    assert!(
        document.order.closed_date.is_none(),
        "a confirmed order is not closed"
    );
    // The promise is a promise: nothing has moved, and nothing is reserved.
    assert_eq!(yard.on_hand(&yard.warehouse).await, 4_000);
    assert_eq!(
        yard.moves().await,
        1,
        "only the movement that stocked us up"
    );

    // The number is reachable by the name a person uses.
    assert_eq!(
        yard.door
            .inv_sales_order_id_by_number(&number.to_lowercase())
            .await
            .unwrap()
            .map(|id| id.as_str().to_owned()),
        Some(yard.order.as_str().to_owned())
    );

    // Frozen: neither the header nor the lines may be edited, and confirming
    // again would draw a second number for one document.
    let refused = conflict(yard.door.set_inv_sales_order_lines(&yard.order, &[]).await);
    assert!(refused.contains("draft"), "{refused}");
    assert!(
        conflict(yard.door.confirm_inv_sales_order(&yard.order, false).await).contains("confirmed")
    );
    assert!(conflict(yard.door.delete_inv_sales_order(&yard.order).await).contains("draft"));
}

#[tokio::test]
async fn a_confirmation_of_nothing_is_refused() {
    let store = common::test_store().await;
    let yard = Selling::open(&store, "empty", 0).await;
    let empty = yard
        .door
        .create_inv_sales_order(&NewSalesOrder::for_customer(yard.customer.clone()))
        .await
        .unwrap();
    let refused = invalid(yard.door.confirm_inv_sales_order(&empty, false).await);
    assert!(refused.contains("no lines"), "{refused}");
    // And it is still a draft, still unnumbered, still deletable.
    let document = yard.door.inv_sales_order(&empty).await.unwrap().unwrap();
    assert_eq!(document.order.status, SoStatus::Draft);
    assert!(document.order.number.is_none());
    yard.door.delete_inv_sales_order(&empty).await.unwrap();
}

#[tokio::test]
async fn a_delivery_moves_the_goods_out_and_advances_the_order() {
    let store = common::test_store().await;
    let yard = Selling::open(&store, "arc", 4_000).await;
    let chairs = yard.line(1).await;

    let outcome = yard
        .door
        .deliver_inv_sales_order(
            &yard.order,
            &NewDelivery {
                note: "two boxes, driver Kowalski".to_owned(),
                ..yard.consignment(Some(vec![NewDeliveryLine {
                    so_line_id: chairs.clone(),
                    qty_milli: 2_500,
                }]))
            },
        )
        .await
        .unwrap();

    // The order advanced and the accumulator moved with it.
    assert_eq!(outcome.order.order.status, SoStatus::PartiallyDelivered);
    assert!(
        outcome.order.order.closed_date.is_none(),
        "an order still owing goods is not closed"
    );
    assert_eq!(outcome.order.lines[0].delivered_qty_milli, 2_500);
    assert_eq!(outcome.order.lines[0].outstanding_qty_milli(), 1_500);
    assert_eq!(
        outcome.order.lines[1].outstanding_qty_milli(),
        0,
        "a charge in words never holds an order open"
    );

    // The note: numbered within its order, quantities only, one line.
    assert_eq!(outcome.delivery.sequence_no, 1);
    let number = outcome.order.order.number.clone().unwrap();
    assert_eq!(
        outcome.delivery.note_number(&number),
        format!("{number}/D1")
    );
    assert_eq!(outcome.delivery.note, "two boxes, driver Kowalski");
    assert_eq!(outcome.delivery.lines.len(), 1);
    assert_eq!(outcome.delivery.lines[0].qty_milli, 2_500);
    assert_eq!(outcome.delivery.lines[0].unit, "piece");

    // The goods actually left: warehouse down, customer up, one movement of the
    // right shape carrying the order as its reference.
    assert_eq!(yard.on_hand(&yard.warehouse).await, 1_500);
    assert_eq!(yard.on_hand(&yard.customer_location).await, 2_500);
    let moves = yard.door.inv_moves(&MoveFilter::default()).await.unwrap();
    let booked = moves
        .iter()
        .find(|m| m.id == outcome.delivery.lines[0].move_id)
        .expect("the delivery's movement is in the ledger");
    assert_eq!(booked.reason, MoveReason::Sale);
    assert_eq!(booked.qty_milli, 2_500);
    assert_eq!(booked.from_location_id, yard.warehouse);
    assert_eq!(booked.to_location_id, yard.customer_location);
    let reference = booked
        .reference
        .as_ref()
        .expect("a delivery names its order");
    assert_eq!(reference.kind, MoveRefKind::SalesOrder);
    assert_eq!(reference.id, yard.order.as_str());
}

#[tokio::test]
async fn the_last_delivery_closes_the_order() {
    let store = common::test_store().await;
    let yard = Selling::open(&store, "last", 4_000).await;
    let chairs = yard.line(1).await;
    yard.door
        .deliver_inv_sales_order(
            &yard.order,
            &yard.consignment(Some(vec![NewDeliveryLine {
                so_line_id: chairs,
                qty_milli: 2_500,
            }])),
        )
        .await
        .unwrap();

    // The unstated form: everything still owed, which nobody should have to
    // type out.
    let outcome = yard
        .door
        .deliver_inv_sales_order(&yard.order, &yard.consignment(None))
        .await
        .unwrap();
    assert_eq!(outcome.order.order.status, SoStatus::Delivered);
    assert!(
        outcome.order.order.closed_date.is_some(),
        "a delivered order is stamped with the day it closed"
    );
    assert_eq!(outcome.delivery.sequence_no, 2);
    assert_eq!(outcome.delivery.lines[0].qty_milli, 1_500);
    assert_eq!(yard.on_hand(&yard.warehouse).await, 0);
    assert_eq!(yard.on_hand(&yard.customer_location).await, 4_000);

    // Both notes are readable, newest first, each with its lines.
    let notes = yard
        .door
        .inv_sales_order_deliveries(&yard.order)
        .await
        .unwrap();
    assert_eq!(
        notes.iter().map(|d| d.sequence_no).collect::<Vec<_>>(),
        [2, 1]
    );
    assert!(notes.iter().all(|d| d.lines.len() == 1));

    // And there is nothing left to send.
    let refused = conflict(
        yard.door
            .deliver_inv_sales_order(&yard.order, &yard.consignment(None))
            .await,
    );
    assert!(refused.contains("already gone out"), "{refused}");
}

#[tokio::test]
async fn a_shelf_that_has_not_got_the_goods_refuses_the_whole_delivery() {
    // The single most useful thing the module does, and the reason the ledger is
    // moves-only: the refusal is trustworthy because on-hand is a sum of
    // movements, not a number somebody typed.
    let store = common::test_store().await;
    let yard = Selling::open(&store, "short", 1_000).await;
    let chairs = yard.line(1).await;

    let refused = conflict(
        yard.door
            .deliver_inv_sales_order(
                &yard.order,
                &yard.consignment(Some(vec![NewDeliveryLine {
                    so_line_id: chairs,
                    qty_milli: 4_000,
                }])),
            )
            .await,
    );
    assert!(refused.contains("Blue chair"), "{refused}");
    assert!(
        refused.contains("1000"),
        "it names what is there: {refused}"
    );

    // Nothing at all was written: the shelf, the ledger, the accumulator and the
    // order are exactly as they were.
    assert_eq!(yard.on_hand(&yard.warehouse).await, 1_000);
    assert_eq!(yard.on_hand(&yard.customer_location).await, 0);
    assert_eq!(
        yard.moves().await,
        1,
        "only the movement that stocked us up"
    );
    assert_eq!(yard.delivered(1).await, 0);
    assert_eq!(yard.status().await, SoStatus::Confirmed);
    assert!(
        yard.door
            .inv_sales_order_deliveries(&yard.order)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn nothing_a_refused_delivery_asked_for_reaches_the_ledger() {
    let store = common::test_store().await;
    let yard = Selling::open(&store, "refused", 4_000).await;
    let chairs = yard.line(1).await;
    let words = yard.line(2).await;

    // Over-delivery, a charge in words, a quantity of nothing, an unknown line,
    // a virtual source, and a consignment that books nothing — six refusals.
    let over = conflict(
        yard.door
            .deliver_inv_sales_order(
                &yard.order,
                &yard.consignment(Some(vec![NewDeliveryLine {
                    so_line_id: chairs.clone(),
                    qty_milli: 4_001,
                }])),
            )
            .await,
    );
    assert!(over.contains("4001"), "{over}");
    assert!(
        invalid(
            yard.door
                .deliver_inv_sales_order(
                    &yard.order,
                    &yard.consignment(Some(vec![NewDeliveryLine {
                        so_line_id: words,
                        qty_milli: 1_000,
                    }])),
                )
                .await
        )
        .contains("charge in words")
    );
    assert!(
        invalid(
            yard.door
                .deliver_inv_sales_order(
                    &yard.order,
                    &yard.consignment(Some(vec![NewDeliveryLine {
                        so_line_id: chairs.clone(),
                        qty_milli: 0,
                    }])),
                )
                .await
        )
        .contains("more than nothing")
    );
    assert_not_found(
        yard.door
            .deliver_inv_sales_order(
                &yard.order,
                &yard.consignment(Some(vec![NewDeliveryLine {
                    so_line_id: BillingLineId::new("nope"),
                    qty_milli: 1_000,
                }])),
            )
            .await,
    );
    let virtual_source = invalid(
        yard.door
            .deliver_inv_sales_order(
                &yard.order,
                &NewDelivery {
                    location_id: yard.customer_location.clone(),
                    lines: None,
                    note: String::new(),
                },
            )
            .await,
    );
    assert!(
        virtual_source.contains("not a place anybody can walk into"),
        "{virtual_source}"
    );
    assert!(
        invalid(
            yard.door
                .deliver_inv_sales_order(&yard.order, &yard.consignment(Some(Vec::new())))
                .await
        )
        .contains("at least one line")
    );

    assert_eq!(yard.on_hand(&yard.warehouse).await, 4_000);
    assert_eq!(yard.moves().await, 1);
    assert_eq!(yard.delivered(1).await, 0);
    assert_eq!(yard.status().await, SoStatus::Confirmed);
}

#[tokio::test]
async fn a_draft_and_a_cancelled_order_deliver_nothing() {
    let store = common::test_store().await;
    let yard = Selling::open(&store, "shut", 4_000).await;

    let draft = yard
        .door
        .create_inv_sales_order(&NewSalesOrder::for_customer(yard.customer.clone()))
        .await
        .unwrap();
    yard.door
        .set_inv_sales_order_lines(
            &draft,
            &[NewSoLine {
                product_id: Some(yard.chair.clone()),
                line: NewLine {
                    description: "Blue chair".to_owned(),
                    unit: "piece".to_owned(),
                    qty_milli: 1_000,
                    unit_price_cents: 8_600,
                    vat_rate_bp: 2100,
                },
            }],
        )
        .await
        .unwrap();
    let refused = conflict(
        yard.door
            .deliver_inv_sales_order(&draft, &yard.consignment(None))
            .await,
    );
    assert!(refused.contains("not been confirmed"), "{refused}");

    yard.door
        .cancel_inv_sales_order(&yard.order, false)
        .await
        .unwrap();
    let cancelled = conflict(
        yard.door
            .deliver_inv_sales_order(&yard.order, &yard.consignment(None))
            .await,
    );
    assert!(cancelled.contains("cancelled"), "{cancelled}");
    assert_eq!(yard.on_hand(&yard.warehouse).await, 4_000);
    assert_eq!(yard.moves().await, 1);
}

#[tokio::test]
async fn cancelling_after_a_delivery_closes_the_remainder_and_moves_nothing_back() {
    let store = common::test_store().await;
    let yard = Selling::open(&store, "shortclose", 4_000).await;
    let chairs = yard.line(1).await;
    yard.door
        .deliver_inv_sales_order(
            &yard.order,
            &yard.consignment(Some(vec![NewDeliveryLine {
                so_line_id: chairs,
                qty_milli: 2_500,
            }])),
        )
        .await
        .unwrap();

    // Accepting the shortfall has to be said out loud.
    let refused = conflict(yard.door.cancel_inv_sales_order(&yard.order, false).await);
    assert!(refused.contains("already gone out"), "{refused}");
    assert_eq!(yard.status().await, SoStatus::PartiallyDelivered);

    let closed = yard
        .door
        .cancel_inv_sales_order(&yard.order, true)
        .await
        .unwrap();
    assert_eq!(closed.order.status, SoStatus::Cancelled);
    assert!(closed.order.closed_date.is_some());
    assert!(
        closed.order.number.is_some(),
        "a cancelled order keeps the number the customer holds"
    );
    // What has gone out has gone out: the ledger is append-only and a
    // cancellation writes no movement.
    assert_eq!(closed.lines[0].delivered_qty_milli, 2_500);
    assert_eq!(yard.on_hand(&yard.warehouse).await, 1_500);
    assert_eq!(yard.on_hand(&yard.customer_location).await, 2_500);
    assert_eq!(yard.moves().await, 2);
    assert_eq!(
        yard.door
            .inv_sales_order_deliveries(&yard.order)
            .await
            .unwrap()
            .len(),
        1,
        "the delivery note outlives the cancellation"
    );
}

#[tokio::test]
async fn another_tenants_order_can_never_be_delivered() {
    let store = common::test_store().await;
    let ours = Selling::open(&store, "ours", 4_000).await;
    let theirs = Selling::open(&store, "theirs", 4_000).await;
    assert_ne!(ours.tenant, theirs.tenant);
    let their_line = theirs.line(1).await;

    // Reading it, delivering against it, and reading what has gone out against
    // it are all the same bare denial — never a complaint that would say the
    // order was worth looking at.
    assert!(
        ours.door
            .inv_sales_order(&theirs.order)
            .await
            .unwrap()
            .is_none()
    );
    assert_not_found(
        ours.door
            .deliver_inv_sales_order(&theirs.order, &ours.consignment(None))
            .await,
    );
    assert_not_found(
        ours.door
            .deliver_inv_sales_order(
                &theirs.order,
                &ours.consignment(Some(vec![NewDeliveryLine {
                    so_line_id: their_line.clone(),
                    qty_milli: 1_000,
                }])),
            )
            .await,
    );
    assert!(
        ours.door
            .inv_sales_order_deliveries(&theirs.order)
            .await
            .unwrap()
            .is_empty()
    );
    // Nor by naming their line on our own order, nor by picking from their shelf.
    assert_not_found(
        ours.door
            .deliver_inv_sales_order(
                &ours.order,
                &ours.consignment(Some(vec![NewDeliveryLine {
                    so_line_id: their_line,
                    qty_milli: 1_000,
                }])),
            )
            .await,
    );
    assert_not_found(
        ours.door
            .deliver_inv_sales_order(
                &ours.order,
                &NewDelivery {
                    location_id: theirs.warehouse.clone(),
                    lines: None,
                    note: String::new(),
                },
            )
            .await,
    );
    // Their confirmation, their cancellation and their draft-only guards are
    // equally unreachable, and their list never shows ours.
    assert_not_found(
        ours.door
            .confirm_inv_sales_order(&theirs.order, false)
            .await,
    );
    assert_not_found(ours.door.cancel_inv_sales_order(&theirs.order, true).await);
    assert_not_found(ours.door.delete_inv_sales_order(&theirs.order).await);
    assert_not_found(
        ours.door
            .update_inv_sales_order(
                &theirs.order,
                &NewSalesOrder::for_customer(ours.customer.clone()),
            )
            .await,
    );
    // Numbers are drawn per tenant, so both first orders are SO-YYYY-00001 —
    // which is exactly why resolving one by name must answer with the asker's
    // own document and never with the other tenant's.
    let their_number = theirs
        .door
        .inv_sales_order(&theirs.order)
        .await
        .unwrap()
        .unwrap()
        .order
        .number
        .unwrap();
    let resolved = ours
        .door
        .inv_sales_order_id_by_number(&their_number)
        .await
        .unwrap();
    assert!(
        resolved
            .as_ref()
            .is_none_or(|id| id.as_str() != theirs.order.as_str()),
        "a number is resolved within the tenant that asked, never across"
    );
    let ours_only = ours.door.inv_sales_orders(None).await.unwrap();
    assert!(
        ours_only
            .iter()
            .all(|s| s.order.id.as_str() != theirs.order.as_str())
    );

    // And their goods are still on their shelf.
    assert_eq!(theirs.on_hand(&theirs.warehouse).await, 4_000);
    assert_eq!(theirs.moves().await, 1);
}

#[tokio::test]
async fn an_order_can_only_be_raised_for_this_tenants_active_customer() {
    let store = common::test_store().await;
    let ours = Selling::open(&store, "cust", 0).await;
    let theirs = Selling::open(&store, "cust2", 0).await;

    assert_not_found(
        ours.door
            .create_inv_sales_order(&NewSalesOrder::for_customer(theirs.customer.clone()))
            .await,
    );
    ours.door
        .set_billing_customer_archived(&ours.customer, true)
        .await
        .unwrap();
    let archived = invalid(
        ours.door
            .create_inv_sales_order(&NewSalesOrder::for_customer(ours.customer.clone()))
            .await,
    );
    assert!(archived.contains("archived"), "{archived}");
}
