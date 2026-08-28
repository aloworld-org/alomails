//! **Receiving** a purchase order (alo Inventory, wave B5.05b) against the real
//! database — the three things one delivery does, proved to happen together or
//! not at all.
//!
//! [`alo_store::inv_po_receive`]'s unit tests already prove the pure rules —
//! what a stated delivery resolves to, when an order is complete, what an
//! over-receipt is refused with. What only a database can prove is what this
//! suite is for:
//!
//! | Property | Where |
//! |---|---|
//! | the goods move, the order advances and a bill is drafted, in one act | `a_delivery_moves_the_goods_advances_the_order_and_drafts_a_bill` |
//! | the rest of a delivery closes the order and drafts a second bill | `the_last_delivery_closes_the_order_and_bills_only_what_it_brought` |
//! | a refused delivery writes nothing at all — no movement, no bill | `nothing_a_refused_delivery_asked_for_reaches_the_ledger` |
//! | goods arrive only against an order the supplier actually has | `a_draft_and_a_cancelled_order_receive_nothing` |
//! | one tenant's order can never be received, read or billed by another | `another_tenants_order_can_never_be_received` |
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::inv_locations::{Location, LocationKind, LocationSeed};
use alo_store::inv_moves::{MoveFilter, MoveReason, MoveRefKind};
use alo_store::inv_po::{NewPurchaseOrder, PoStatus};
use alo_store::inv_po_lines::NewPoLine;
use alo_store::inv_po_receive::{NewReceipt, NewReceiptLine};
use alo_store::inv_suppliers::NewSupplier;
use alo_store::{
    AccountStore, BillStatus, BillingLineId, BillingProductId, InvLocationId, InvPurchaseOrderId,
    InvSupplierId, NewLine, NewProduct, Store, StoreError, TenantId,
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

/// A tenant with locations, a supplier, a product and a **sent** order for four
/// chairs plus a freight charge — the state every delivery here arrives into.
struct Purchasing {
    door: AccountStore,
    tenant: TenantId,
    chair: BillingProductId,
    warehouse: InvLocationId,
    supplier_location: InvLocationId,
    customer_location: InvLocationId,
    supplier: InvSupplierId,
    order: InvPurchaseOrderId,
}

impl Purchasing {
    async fn open(store: &Store, tag: &str) -> Self {
        let tenant = store.create_tenant(&format!("recv-{tag}")).await.unwrap();
        let user = store
            .for_tenant(tenant.clone())
            .create_user(&format!("{tag}@purchasing.test"))
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
                address_line1: "Bahnhofstrasse 1".to_owned(),
                postal_code: "8001".to_owned(),
                city: "Zürich".to_owned(),
                country: "CH".to_owned(),
                currency: "CHF".to_owned(),
                email: Some(format!("orders+{tag}@hoffmann.test")),
                payment_terms_days: 30,
                lead_time_days: 9,
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
                ..Default::default()
            })
            .await
            .unwrap();

        let order = door
            .create_inv_purchase_order(&NewPurchaseOrder {
                reference: "Project Falkenstein".to_owned(),
                ..NewPurchaseOrder::for_supplier(supplier.clone())
            })
            .await
            .unwrap();
        door.set_inv_purchase_order_lines(
            &order,
            &[
                NewPoLine {
                    product_id: Some(chair.clone()),
                    line: NewLine {
                        description: "Blue chair".to_owned(),
                        unit: "piece".to_owned(),
                        qty_milli: 4_000,
                        unit_price_cents: 4_300,
                        vat_rate_bp: 1900,
                    },
                },
                NewPoLine {
                    product_id: None,
                    line: NewLine {
                        description: "Freight".to_owned(),
                        unit: String::new(),
                        qty_milli: 1_000,
                        unit_price_cents: 2_500,
                        vat_rate_bp: 1900,
                    },
                },
            ],
        )
        .await
        .unwrap();
        door.send_inv_purchase_order::<(), StoreError, _, _>(&order, |_| async { Ok(()) })
            .await
            .unwrap();

        Self {
            warehouse: of(LocationKind::Stock),
            supplier_location: of(LocationKind::Supplier),
            customer_location: of(LocationKind::Customer),
            chair,
            supplier,
            order,
            door,
            tenant,
        }
    }

    /// The id of the order's nth line (1-based), as a caller would have read it.
    async fn line(&self, position: usize) -> BillingLineId {
        self.door
            .inv_purchase_order(&self.order)
            .await
            .unwrap()
            .unwrap()
            .lines[position - 1]
            .line
            .id
            .clone()
    }

    fn delivery(&self, lines: Option<Vec<NewReceiptLine>>) -> NewReceipt {
        NewReceipt {
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

    async fn bills(&self) -> usize {
        self.door.billing_bills(None).await.unwrap().len()
    }

    async fn status(&self) -> PoStatus {
        self.door
            .inv_purchase_order(&self.order)
            .await
            .unwrap()
            .unwrap()
            .order
            .status
    }
}

#[tokio::test]
async fn a_delivery_moves_the_goods_advances_the_order_and_drafts_a_bill() {
    let store = common::test_store().await;
    let yard = Purchasing::open(&store, "arc").await;
    let chairs = yard.line(1).await;

    let outcome = yard
        .door
        .receive_inv_purchase_order(
            &yard.order,
            &yard.delivery(Some(vec![NewReceiptLine {
                po_line_id: chairs.clone(),
                qty_milli: 2_500,
            }])),
        )
        .await
        .unwrap();

    // 1. The goods moved, from the supplier into the warehouse, referencing the
    //    order that asked for them.
    assert_eq!(yard.on_hand(&yard.warehouse).await, 2_500);
    assert_eq!(
        yard.on_hand(&yard.supplier_location).await,
        -2_500,
        "the virtual counterparty is how much has come from outside"
    );
    let ledger = yard.door.inv_moves(&MoveFilter::default()).await.unwrap();
    assert_eq!(ledger.len(), 1);
    assert_eq!(ledger[0].reason, MoveReason::Purchase);
    assert!(
        ledger[0].reason_code.is_none(),
        "a receipt is not a correction"
    );
    let reference = ledger[0]
        .reference
        .clone()
        .expect("a movement from a document");
    assert_eq!(reference.kind, MoveRefKind::PurchaseOrder);
    assert_eq!(reference.id, yard.order.as_str());

    // 2. The order took the quantity and is open on the rest.
    assert_eq!(outcome.order.order.status, PoStatus::PartiallyReceived);
    assert!(
        outcome.order.order.closed_date.is_none(),
        "an order still expecting goods is not closed"
    );
    assert_eq!(outcome.order.lines[0].received_qty_milli, 2_500);
    assert_eq!(outcome.order.lines[0].outstanding_qty_milli(), 1_500);
    assert_eq!(
        outcome.order.lines[1].outstanding_qty_milli(),
        0,
        "freight never arrives on a pallet"
    );
    assert_eq!(
        outcome.order.totals.gross_cents, 23_443,
        "what was ordered does not change because part of it arrived"
    );

    // 3. A draft bill for what arrived — ours, not theirs.
    let bill = yard
        .door
        .billing_bill(&outcome.bill_id)
        .await
        .unwrap()
        .expect("the drafted bill");
    assert_eq!(bill.bill.status, BillStatus::Received, "nobody has decided");
    assert!(
        bill.bill.source_syntax.is_none() && bill.bill.source_sha256.is_empty(),
        "it was read from no file"
    );
    let number = bill.bill.number.clone();
    assert!(number.ends_with("/R1"), "{number}");
    assert!(number.starts_with("PO-"), "{number}");
    assert_eq!(bill.bill.supplier.name, "Hoffmann arc");
    assert_eq!(bill.bill.supplier.country, "CH");
    assert_eq!(bill.bill.currency, "CHF", "the order's currency");
    assert_eq!(bill.bill.buyer_reference, "Project Falkenstein");
    assert_eq!(
        (bill.bill.due_date.unwrap() - bill.bill.issue_date).whole_days(),
        30,
        "their payment terms, counted from the day the goods arrived"
    );
    assert_eq!(bill.lines.len(), 1, "only what arrived is billed");
    assert_eq!(bill.lines[0].qty_milli, 2_500);
    assert_eq!(bill.lines[0].unit_price_cents, 4_300, "the agreed price");
    assert_eq!(bill.bill.totals.tax_exclusive_cents, 10_750);
    assert_eq!(bill.bill.totals.tax_total_cents, 2_043);
    assert_eq!(bill.bill.totals.payable_cents, 12_793);
    assert_eq!(
        bill.computed.gross_cents, bill.bill.totals.tax_inclusive_cents,
        "what we state and what the lines say are the same figure"
    );

    // The receipt itself reads back as one delivery, with its movement.
    let receipts = yard
        .door
        .inv_purchase_order_receipts(&yard.order)
        .await
        .unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].sequence_no, 1);
    assert_eq!(receipts[0].location_id, yard.warehouse);
    assert_eq!(receipts[0].bill_id.as_ref(), Some(&outcome.bill_id));
    assert_eq!(receipts[0].lines.len(), 1);
    assert_eq!(receipts[0].lines[0].po_line_id, chairs);
    assert_eq!(receipts[0].lines[0].qty_milli, 2_500);
    assert_eq!(receipts[0].lines[0].move_id, ledger[0].id);

    store.delete_tenant(&yard.tenant).await.unwrap();
}

#[tokio::test]
async fn the_last_delivery_closes_the_order_and_bills_only_what_it_brought() {
    let store = common::test_store().await;
    let yard = Purchasing::open(&store, "rest").await;
    let chairs = yard.line(1).await;

    yard.door
        .receive_inv_purchase_order(
            &yard.order,
            &yard.delivery(Some(vec![NewReceiptLine {
                po_line_id: chairs,
                qty_milli: 2_500,
            }])),
        )
        .await
        .unwrap();

    // The rest of it, stated as nothing at all: what is still outstanding.
    let outcome = yard
        .door
        .receive_inv_purchase_order(&yard.order, &yard.delivery(None))
        .await
        .unwrap();

    assert_eq!(outcome.order.order.status, PoStatus::Received);
    assert!(
        outcome.order.order.closed_date.is_some(),
        "a finished order is stamped with the day it finished"
    );
    assert_eq!(outcome.order.lines[0].received_qty_milli, 4_000);
    assert_eq!(yard.on_hand(&yard.warehouse).await, 4_000);
    assert_eq!(yard.moves().await, 2);

    let bill = yard
        .door
        .billing_bill(&outcome.bill_id)
        .await
        .unwrap()
        .unwrap();
    assert!(bill.bill.number.ends_with("/R2"), "{}", bill.bill.number);
    assert_eq!(
        bill.lines[0].qty_milli, 1_500,
        "the second bill is for the second delivery, not the whole order"
    );
    assert_eq!(yard.bills().await, 2, "one bill per delivery");
    assert_eq!(
        yard.door
            .inv_purchase_order_receipts(&yard.order)
            .await
            .unwrap()
            .len(),
        2
    );

    // Nothing more can arrive against it, and it cannot be re-opened by editing.
    let refused = conflict(
        yard.door
            .receive_inv_purchase_order(&yard.order, &yard.delivery(None))
            .await,
    );
    assert!(refused.contains("already arrived"), "{refused}");
    assert_eq!(yard.moves().await, 2);
    assert_eq!(yard.bills().await, 2);

    store.delete_tenant(&yard.tenant).await.unwrap();
}

#[tokio::test]
async fn nothing_a_refused_delivery_asked_for_reaches_the_ledger() {
    let store = common::test_store().await;
    let yard = Purchasing::open(&store, "refused").await;
    let chairs = yard.line(1).await;
    let freight = yard.line(2).await;

    // More than was ordered.
    let over = conflict(
        yard.door
            .receive_inv_purchase_order(
                &yard.order,
                &yard.delivery(Some(vec![NewReceiptLine {
                    po_line_id: chairs.clone(),
                    qty_milli: 4_001,
                }])),
            )
            .await,
    );
    assert!(
        over.contains("4000") && over.contains("adjustment"),
        "{over}"
    );

    // A charge in words.
    let words = invalid(
        yard.door
            .receive_inv_purchase_order(
                &yard.order,
                &yard.delivery(Some(vec![NewReceiptLine {
                    po_line_id: freight,
                    qty_milli: 1_000,
                }])),
            )
            .await,
    );
    assert!(words.contains("charge in words"), "{words}");

    // A place nobody can walk into.
    let virtual_place = invalid(
        yard.door
            .receive_inv_purchase_order(
                &yard.order,
                &NewReceipt {
                    location_id: yard.customer_location.clone(),
                    lines: None,
                    note: String::new(),
                },
            )
            .await,
    );
    assert!(virtual_place.contains("walk into"), "{virtual_place}");

    // A location that is not this tenant's at all.
    assert_not_found(
        yard.door
            .receive_inv_purchase_order(
                &yard.order,
                &NewReceipt {
                    location_id: InvLocationId::new("not-a-place"),
                    lines: None,
                    note: String::new(),
                },
            )
            .await,
    );

    // A line from no order of ours.
    assert_not_found(
        yard.door
            .receive_inv_purchase_order(
                &yard.order,
                &yard.delivery(Some(vec![NewReceiptLine {
                    po_line_id: BillingLineId::new("someone-elses-line"),
                    qty_milli: 1_000,
                }])),
            )
            .await,
    );

    // Four refusals, and the warehouse, the ledger, the bills and the order are
    // exactly where they were.
    assert_eq!(yard.moves().await, 0);
    assert_eq!(yard.bills().await, 0);
    assert_eq!(yard.on_hand(&yard.warehouse).await, 0);
    assert_eq!(yard.status().await, PoStatus::Sent);
    assert!(
        yard.door
            .inv_purchase_order_receipts(&yard.order)
            .await
            .unwrap()
            .is_empty()
    );

    store.delete_tenant(&yard.tenant).await.unwrap();
}

#[tokio::test]
async fn a_draft_and_a_cancelled_order_receive_nothing() {
    let store = common::test_store().await;
    let yard = Purchasing::open(&store, "closed").await;

    // A draft: nobody has asked for these goods.
    let draft = yard
        .door
        .create_inv_purchase_order(&NewPurchaseOrder::for_supplier(yard.supplier.clone()))
        .await
        .unwrap();
    yard.door
        .set_inv_purchase_order_lines(
            &draft,
            &[NewPoLine {
                product_id: Some(yard.chair.clone()),
                line: NewLine {
                    description: "Blue chair".to_owned(),
                    unit: "piece".to_owned(),
                    qty_milli: 1_000,
                    unit_price_cents: 4_300,
                    vat_rate_bp: 1900,
                },
            }],
        )
        .await
        .unwrap();
    let unsent = conflict(
        yard.door
            .receive_inv_purchase_order(&draft, &yard.delivery(None))
            .await,
    );
    assert!(unsent.contains("not been sent"), "{unsent}");

    // A cancelled one: we told them to stop.
    yard.door
        .cancel_inv_purchase_order(&yard.order, false)
        .await
        .unwrap();
    let stopped = conflict(
        yard.door
            .receive_inv_purchase_order(&yard.order, &yard.delivery(None))
            .await,
    );
    assert!(stopped.contains("cancelled"), "{stopped}");

    assert_eq!(yard.moves().await, 0);
    assert_eq!(yard.bills().await, 0);

    store.delete_tenant(&yard.tenant).await.unwrap();
}

/// Law 1 on the door that writes stock, a document and a liability at once.
#[tokio::test]
async fn another_tenants_order_can_never_be_received() {
    let store = common::test_store().await;
    let ours = Purchasing::open(&store, "ours").await;
    let theirs = Purchasing::open(&store, "theirs").await;
    let our_chairs = ours.line(1).await;

    // Their door, our order — with their own location, and with ours.
    assert_not_found(
        theirs
            .door
            .receive_inv_purchase_order(
                &ours.order,
                &NewReceipt {
                    location_id: theirs.warehouse.clone(),
                    lines: None,
                    note: String::new(),
                },
            )
            .await,
    );
    assert_not_found(
        theirs
            .door
            .receive_inv_purchase_order(
                &ours.order,
                &NewReceipt {
                    location_id: ours.warehouse.clone(),
                    lines: None,
                    note: String::new(),
                },
            )
            .await,
    );
    // And our order's line id, offered against their own order, is not a line.
    assert_not_found(
        theirs
            .door
            .receive_inv_purchase_order(
                &theirs.order,
                &theirs.delivery(Some(vec![NewReceiptLine {
                    po_line_id: our_chairs,
                    qty_milli: 1_000,
                }])),
            )
            .await,
    );

    // And a tenant who has never opened Inventory at all gets the same bare
    // refusal — not a complaint about their own missing locations, which would
    // say that our order was at least worth looking at.
    let bare = store.create_tenant("recv-bare").await.unwrap();
    let user = store
        .for_tenant(bare.clone())
        .create_user("bare@purchasing.test")
        .await
        .unwrap();
    let stranger = store.for_account(bare.clone(), user);
    assert_not_found(
        stranger
            .receive_inv_purchase_order(
                &ours.order,
                &NewReceipt {
                    location_id: ours.warehouse.clone(),
                    lines: None,
                    note: String::new(),
                },
            )
            .await,
    );
    store.delete_tenant(&bare).await.unwrap();

    // Nothing of ours moved, and nothing of theirs was written.
    assert_eq!(ours.status().await, PoStatus::Sent);
    assert_eq!(ours.moves().await, 0);
    assert_eq!(theirs.moves().await, 0);
    assert_eq!(theirs.bills().await, 0);

    // Ours receives normally, and stays invisible from their door.
    let outcome = ours
        .door
        .receive_inv_purchase_order(&ours.order, &ours.delivery(None))
        .await
        .unwrap();
    assert_eq!(outcome.order.order.status, PoStatus::Received);
    assert!(
        theirs
            .door
            .inv_purchase_order_receipts(&ours.order)
            .await
            .unwrap()
            .is_empty(),
        "what arrived for us is not a fact their door can read"
    );
    assert!(
        theirs
            .door
            .inv_purchase_order_receipt(&outcome.receipt.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        theirs
            .door
            .billing_bill(&outcome.bill_id)
            .await
            .unwrap()
            .is_none(),
        "nor is the liability it drafted"
    );
    assert_eq!(theirs.on_hand(&theirs.warehouse).await, 0);
    assert_eq!(theirs.bills().await, 0);

    store.delete_tenant(&ours.tenant).await.unwrap();
    store.delete_tenant(&theirs.tenant).await.unwrap();
}
