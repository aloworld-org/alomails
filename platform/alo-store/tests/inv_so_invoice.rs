//! **Invoicing** a sales order (alo Inventory, wave B5.06b) against the real
//! database — the bridge from what left the warehouse into alo Billing.
//!
//! [`alo_store::inv_so_invoice`]'s unit tests already prove the pure rule: what
//! each line contributes, when a charge in words rides along, and why a line
//! with nothing left to bill is left off the document. What only a database can
//! prove is what this suite is for:
//!
//! | Property | Where |
//! |---|---|
//! | the draft carries what was **delivered**, at the order's own prices | `an_invoice_carries_what_went_out_at_the_orders_prices` |
//! | a second delivery raises a second draft for the new quantity only | `a_second_delivery_bills_the_new_quantity_and_not_the_charge_again` |
//! | pressing again with nothing new delivered raises nothing | `nothing_new_delivered_raises_nothing_and_says_why` |
//! | throwing the draft away releases what it carried | `throwing_the_draft_away_makes_the_goods_billable_again` |
//! | voiding the issued document releases it too | `voiding_an_issued_invoice_releases_what_it_carried` |
//! | an order nobody confirmed is refused | `a_draft_order_is_never_invoiced` |
//! | a short-closed order still bills what the customer received | `a_cancelled_order_still_bills_what_they_received` |
//! | one tenant's order can never be invoiced or read by another | `another_tenants_order_can_never_be_invoiced` |
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::billing_invoices::InvoiceStatus;
use alo_store::inv_locations::{Location, LocationKind, LocationSeed};
use alo_store::inv_moves::{MoveReason, NewMove};
use alo_store::inv_so::{NewSalesOrder, SoStatus};
use alo_store::inv_so_deliver::{NewDelivery, NewDeliveryLine};
use alo_store::inv_so_invoice::SalesOrderInvoice;
use alo_store::inv_so_lines::NewSoLine;
use alo_store::{
    AccountStore, BillingCustomerId, BillingInvoiceId, BillingLineId, BillingProductId,
    InvLocationId, InvSalesOrderId, NewCustomer, NewLine, NewProduct, Store, StoreError, TenantId,
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

/// A tenant with stock on the shelf and a **confirmed** order for four chairs at
/// €86 plus a €45 delivery charge — the state every raising here starts from.
struct Selling {
    door: AccountStore,
    tenant: TenantId,
    chair: BillingProductId,
    warehouse: InvLocationId,
    supplier_location: InvLocationId,
    customer: BillingCustomerId,
    order: InvSalesOrderId,
}

impl Selling {
    async fn open(store: &Store, tag: &str, on_hand: i64) -> Self {
        let tenant = store.create_tenant(&format!("bill-{tag}")).await.unwrap();
        let user = store
            .for_tenant(tenant.clone())
            .create_user(&format!("{tag}@billing.test"))
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

    /// Puts goods on the shelf the only way this ledger allows.
    async fn stock_up(&self, qty_milli: i64) {
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

    /// A **draft** order for four chairs and a delivery charge.
    async fn draft_order(&self) -> InvSalesOrderId {
        let order = self
            .door
            .create_inv_sales_order(&NewSalesOrder {
                reference: "Their PO 4711".to_owned(),
                note: "ring the bell at the back".to_owned(),
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
        order
    }

    async fn confirmed_order(&self) -> InvSalesOrderId {
        let order = self.draft_order().await;
        self.door.confirm_inv_sales_order(&order).await.unwrap();
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

    /// Ships `qty_milli` chairs off the first line.
    async fn deliver(&self, qty_milli: i64) {
        let chairs = self.line(1).await;
        self.door
            .deliver_inv_sales_order(
                &self.order,
                &NewDelivery {
                    location_id: self.warehouse.clone(),
                    lines: Some(vec![NewDeliveryLine {
                        so_line_id: chairs,
                        qty_milli,
                    }]),
                    note: String::new(),
                },
            )
            .await
            .unwrap();
    }

    /// What the order's nth line reports as already billed.
    async fn invoiced(&self, position: usize) -> i64 {
        self.door
            .inv_sales_order(&self.order)
            .await
            .unwrap()
            .unwrap()
            .lines[position - 1]
            .invoiced_qty_milli
    }

    /// The raisings recorded against the order, newest first.
    async fn raisings(&self) -> Vec<SalesOrderInvoice> {
        self.door
            .inv_sales_order_invoices(&self.order)
            .await
            .unwrap()
    }
}

/// The lines of a raised invoice as `(description, qty_milli, unit_price_cents)`
/// in print order — what the customer actually reads.
async fn invoice_lines(door: &AccountStore, id: &BillingInvoiceId) -> Vec<(String, i64, i64)> {
    door.billing_invoice(id)
        .await
        .unwrap()
        .expect("the raised document exists")
        .lines
        .iter()
        .map(|line| {
            (
                line.description.clone(),
                line.qty_milli,
                line.unit_price_cents,
            )
        })
        .collect()
}

#[tokio::test]
async fn an_invoice_carries_what_went_out_at_the_orders_prices() {
    let store = common::test_store().await;
    let yard = Selling::open(&store, "arc", 4_000).await;
    yard.deliver(2_500).await;

    let outcome = yard
        .door
        .invoice_inv_sales_order(&yard.order)
        .await
        .unwrap();

    // A draft: no number, nothing drawn from the gapless series.
    let document = yard
        .door
        .billing_invoice(&outcome.invoice.invoice_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(document.invoice.status, InvoiceStatus::Draft);
    assert!(document.invoice.number.is_none());
    assert_eq!(document.invoice.currency, "EUR");
    assert_eq!(
        document.invoice.customer_id, yard.customer,
        "the order's customer, not a twin of them"
    );
    assert_eq!(
        document.invoice.reference, "Their PO 4711",
        "their own reference travels onto the document they will read"
    );

    // Two and a half chairs — what went out — plus the delivery charge in full,
    // both at the prices the order snapshotted.
    assert_eq!(
        invoice_lines(&yard.door, &outcome.invoice.invoice_id).await,
        vec![
            ("Blue chair".to_owned(), 2_500, 8_600),
            ("Delivery to the third floor".to_owned(), 1_000, 4_500),
        ]
    );
    // Net = 2.5 × 8600 + 1 × 4500 = 26 000 cents, VAT at 21 %.
    assert_eq!(document.totals.net_cents, 26_000);
    assert_eq!(document.totals.gross_cents, 26_000 + 5_460);

    // The order records what was carried, line by line.
    assert_eq!(outcome.order.order.status, SoStatus::PartiallyDelivered);
    assert_eq!(outcome.order.lines[0].invoiced_qty_milli, 2_500);
    assert_eq!(outcome.order.lines[1].invoiced_qty_milli, 1_000);
    let raisings = yard.raisings().await;
    assert_eq!(raisings.len(), 1);
    assert_eq!(raisings[0].invoice_id, outcome.invoice.invoice_id);
    assert_eq!(raisings[0].invoice_status, InvoiceStatus::Draft);
    assert!(raisings[0].invoice_number.is_none());
    assert_eq!(raisings[0].lines.len(), 2);
    assert_eq!(raisings[0].lines[0].qty_milli, 2_500);
    assert_eq!(raisings[0].lines[1].qty_milli, 1_000);
}

#[tokio::test]
async fn a_second_delivery_bills_the_new_quantity_and_not_the_charge_again() {
    let store = common::test_store().await;
    let yard = Selling::open(&store, "second", 4_000).await;
    yard.deliver(2_500).await;
    let first = yard
        .door
        .invoice_inv_sales_order(&yard.order)
        .await
        .unwrap();

    // The rest of the order goes out, and is billed on its own document.
    yard.deliver(1_500).await;
    let second = yard
        .door
        .invoice_inv_sales_order(&yard.order)
        .await
        .unwrap();
    assert_ne!(second.invoice.invoice_id, first.invoice.invoice_id);
    assert_eq!(
        invoice_lines(&yard.door, &second.invoice.invoice_id).await,
        vec![("Blue chair".to_owned(), 1_500, 8_600)],
        "the new quantity only, and the delivery charge is not charged twice"
    );

    // Ordered and billed now agree, and the order is delivered and closed.
    assert_eq!(second.order.order.status, SoStatus::Delivered);
    assert_eq!(yard.invoiced(1).await, 4_000);
    assert_eq!(yard.invoiced(2).await, 1_000);
    let raisings = yard.raisings().await;
    assert_eq!(raisings.len(), 2, "newest first");
    assert_eq!(raisings[0].invoice_id, second.invoice.invoice_id);
    assert_eq!(raisings[1].invoice_id, first.invoice.invoice_id);

    // And there is nothing left to bill.
    let refused = invalid(yard.door.invoice_inv_sales_order(&yard.order).await);
    assert!(refused.contains("already on an invoice"), "{refused}");
}

#[tokio::test]
async fn nothing_new_delivered_raises_nothing_and_says_why() {
    let store = common::test_store().await;
    let yard = Selling::open(&store, "again", 4_000).await;
    let number = yard
        .door
        .inv_sales_order(&yard.order)
        .await
        .unwrap()
        .unwrap()
        .order
        .number
        .unwrap();

    // Confirmed, but nothing has left the building: an invoice would be a VAT
    // event asserted on a hope.
    let waiting = invalid(yard.door.invoice_inv_sales_order(&yard.order).await);
    assert!(waiting.contains(&number), "{waiting}");
    assert!(waiting.contains("nothing has gone out"), "{waiting}");
    assert!(yard.raisings().await.is_empty());

    yard.deliver(2_500).await;
    yard.door
        .invoice_inv_sales_order(&yard.order)
        .await
        .unwrap();
    let billed = invalid(yard.door.invoice_inv_sales_order(&yard.order).await);
    assert!(billed.contains(&number), "{billed}");
    assert!(billed.contains("deliver more"), "{billed}");
    assert_eq!(
        yard.raisings().await.len(),
        1,
        "a second press writes no second document"
    );
}

#[tokio::test]
async fn throwing_the_draft_away_makes_the_goods_billable_again() {
    let store = common::test_store().await;
    let yard = Selling::open(&store, "discard", 4_000).await;
    yard.deliver(2_500).await;
    let raised = yard
        .door
        .invoice_inv_sales_order(&yard.order)
        .await
        .unwrap();
    assert_eq!(yard.invoiced(1).await, 2_500);

    // A draft is a thing a person throws away. The link goes with it, and what
    // it carried is billable again in the same instant — no hook to forget.
    yard.door
        .delete_billing_invoice(&raised.invoice.invoice_id)
        .await
        .unwrap();
    assert_eq!(yard.invoiced(1).await, 0);
    assert_eq!(yard.invoiced(2).await, 0);
    assert!(yard.raisings().await.is_empty());

    let again = yard
        .door
        .invoice_inv_sales_order(&yard.order)
        .await
        .unwrap();
    assert_eq!(
        invoice_lines(&yard.door, &again.invoice.invoice_id).await,
        vec![
            ("Blue chair".to_owned(), 2_500, 8_600),
            ("Delivery to the third floor".to_owned(), 1_000, 4_500),
        ]
    );
}

#[tokio::test]
async fn voiding_an_issued_invoice_releases_what_it_carried() {
    let store = common::test_store().await;
    let yard = Selling::open(&store, "void", 4_000).await;
    yard.deliver(4_000).await;
    let raised = yard
        .door
        .invoice_inv_sales_order(&yard.order)
        .await
        .unwrap();
    let issued = yard
        .door
        .issue_billing_invoice(&raised.invoice.invoice_id)
        .await
        .unwrap();
    let number = issued
        .invoice
        .number
        .expect("an issued document is numbered");

    // Issued: the raising reports the number and the status, and the goods stay
    // billed.
    let raisings = yard.raisings().await;
    assert_eq!(raisings[0].invoice_number.as_deref(), Some(number.as_str()));
    assert_eq!(raisings[0].invoice_status, InvoiceStatus::Issued);
    assert_eq!(yard.invoiced(1).await, 4_000);

    // Voided: the document keeps its number and stays readable, and what it
    // carried becomes billable again — the goods are still with the customer.
    yard.door
        .void_billing_invoice(&raised.invoice.invoice_id)
        .await
        .unwrap();
    assert_eq!(yard.invoiced(1).await, 0);
    let raisings = yard.raisings().await;
    assert_eq!(
        raisings.len(),
        1,
        "the record of having raised it is not erased"
    );
    assert_eq!(raisings[0].invoice_status, InvoiceStatus::Void);

    let again = yard
        .door
        .invoice_inv_sales_order(&yard.order)
        .await
        .unwrap();
    assert_eq!(
        invoice_lines(&yard.door, &again.invoice.invoice_id).await,
        vec![
            ("Blue chair".to_owned(), 4_000, 8_600),
            ("Delivery to the third floor".to_owned(), 1_000, 4_500),
        ]
    );
    assert_eq!(yard.raisings().await.len(), 2);
}

#[tokio::test]
async fn a_credit_note_does_not_release_the_goods_it_corrects() {
    let store = common::test_store().await;
    let yard = Selling::open(&store, "credit", 4_000).await;
    yard.deliver(4_000).await;
    let raised = yard
        .door
        .invoice_inv_sales_order(&yard.order)
        .await
        .unwrap();
    yard.door
        .issue_billing_invoice(&raised.invoice.invoice_id)
        .await
        .unwrap();
    yard.door
        .create_billing_credit_note(&raised.invoice.invoice_id)
        .await
        .unwrap();

    // Crediting corrects a document; the goods stay billed against the original,
    // and re-billing them would charge the customer twice for one delivery.
    assert_eq!(yard.invoiced(1).await, 4_000);
    let refused = invalid(yard.door.invoice_inv_sales_order(&yard.order).await);
    assert!(refused.contains("already on an invoice"), "{refused}");
}

#[tokio::test]
async fn a_draft_order_is_never_invoiced() {
    let store = common::test_store().await;
    let yard = Selling::open(&store, "draft", 4_000).await;
    let draft = yard.draft_order().await;

    let refused = conflict(yard.door.invoice_inv_sales_order(&draft).await);
    assert!(refused.contains("draft"), "{refused}");
    assert!(refused.contains("nothing to invoice"), "{refused}");
    assert!(
        yard.door
            .inv_sales_order_invoices(&draft)
            .await
            .unwrap()
            .is_empty()
    );
    // And it is still a draft, still deletable — nothing was written.
    yard.door.delete_inv_sales_order(&draft).await.unwrap();
}

#[tokio::test]
async fn a_cancelled_order_still_bills_what_they_received() {
    let store = common::test_store().await;
    let yard = Selling::open(&store, "shortclose", 4_000).await;
    yard.deliver(2_500).await;
    // Giving up on the remainder closes the order and leaves the customer to be
    // invoiced for what they received — which is what the refusal that demands
    // `short_close` says out loud.
    yard.door
        .cancel_inv_sales_order(&yard.order, true)
        .await
        .unwrap();

    let outcome = yard
        .door
        .invoice_inv_sales_order(&yard.order)
        .await
        .unwrap();
    assert_eq!(outcome.order.order.status, SoStatus::Cancelled);
    assert_eq!(
        invoice_lines(&yard.door, &outcome.invoice.invoice_id).await,
        vec![
            ("Blue chair".to_owned(), 2_500, 8_600),
            ("Delivery to the third floor".to_owned(), 1_000, 4_500),
        ],
        "what went out, never the 1.5 chairs that did not"
    );
}

#[tokio::test]
async fn an_order_that_never_shipped_bills_nothing_when_it_is_cancelled() {
    let store = common::test_store().await;
    let yard = Selling::open(&store, "abandoned", 4_000).await;
    yard.door
        .cancel_inv_sales_order(&yard.order, false)
        .await
        .unwrap();

    // The customer received nothing, so they owe nothing — not even the
    // delivery charge for a van that never came.
    let refused = invalid(yard.door.invoice_inv_sales_order(&yard.order).await);
    assert!(refused.contains("nothing has gone out"), "{refused}");
}

#[tokio::test]
async fn another_tenants_order_can_never_be_invoiced() {
    let store = common::test_store().await;
    let ours = Selling::open(&store, "ours", 4_000).await;
    let theirs = Selling::open(&store, "theirs", 4_000).await;
    assert_ne!(ours.tenant, theirs.tenant);
    ours.deliver(2_500).await;
    let raised = ours
        .door
        .invoice_inv_sales_order(&ours.order)
        .await
        .unwrap();

    // Our order, seen from their door: not a refusal that would confirm it
    // exists, and not a document raised on our customer.
    assert_not_found(theirs.door.invoice_inv_sales_order(&ours.order).await);
    assert!(
        theirs
            .door
            .inv_sales_order_invoices(&ours.order)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        theirs
            .door
            .inv_sales_order_invoice(&raised.invoice.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        theirs
            .door
            .billing_invoice(&raised.invoice.invoice_id)
            .await
            .unwrap()
            .is_none(),
        "the document it raised is ours too"
    );
    // Their own order is untouched by any of it.
    assert!(theirs.raisings().await.is_empty());
    assert_eq!(ours.invoiced(1).await, 2_500);
    assert_eq!(theirs.invoiced(1).await, 0);
}
