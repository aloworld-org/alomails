//! Accepting a quote routes by its content (alo Orders, ADR 0054 §5, item O1.c).
//!
//! An offer naming any catalog item is for **goods** and becomes a draft sales
//! order, because goods are reserved, picked and delivered before anybody is
//! billed. An offer naming none is for **services** and becomes the draft
//! invoice it always did.
//!
//! | Property | Where |
//! |---|---|
//! | **a services offer still becomes an invoice, unchanged** | `an_offer_of_services_still_becomes_an_invoice_directly` |
//! | a goods offer becomes a draft order carrying its products | `an_offer_of_goods_becomes_a_draft_order_that_can_be_delivered` |
//! | one stocked line among services is still goods | `one_line_of_goods_is_enough_to_make_it_an_order` |
//! | the order is a **draft**: acceptance never commits stock | `accepting_never_confirms_and_so_never_promises_stock` |
//! | a neighbour's product can never be put on our offer | `another_tenants_product_can_never_be_offered_by_us` |
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::billing_line::NewLine;
use alo_store::billing_quote_lines::NewQuoteLine;
use alo_store::billing_quotes::NewQuote;
use alo_store::inv_so::SoStatus;
use alo_store::{
    AccountStore, BillingCustomerId, BillingProductId, BillingQuoteId, NewCustomer, NewProduct,
    Store, StoreError,
};

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    assert!(
        matches!(result, Err(StoreError::NotFound)),
        "expected the clean not-found denial, got: {result:?}"
    );
}

/// A tenant with a customer and a stocked product to offer.
struct Trading {
    door: AccountStore,
    customer: BillingCustomerId,
    fan: BillingProductId,
}

impl Trading {
    async fn open(store: &Store, tag: &str) -> Self {
        let tenant = store.create_tenant(&format!("route-{tag}")).await.unwrap();
        let user = store
            .for_tenant(tenant.clone())
            .create_user(&format!("{tag}@route.test"))
            .await
            .unwrap();
        let door = store.for_account(tenant, user);
        let customer = door
            .create_billing_customer(&NewCustomer {
                name: format!("Koelhuis {tag}"),
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
        Self {
            door,
            customer,
            fan,
        }
    }

    /// A **sent** offer with the given lines — the only state that can be
    /// accepted.
    async fn sent(&self, lines: &[NewQuoteLine]) -> BillingQuoteId {
        let id = self
            .door
            .create_billing_quote(&NewQuote::for_customer(self.customer.clone()))
            .await
            .unwrap();
        self.door.set_billing_quote_lines(&id, lines).await.unwrap();
        self.door.send_billing_quote(&id).await.unwrap();
        id
    }

    /// A line for six fans, naming the catalog item — this is what makes an
    /// offer goods.
    fn goods(&self, units: i64) -> NewQuoteLine {
        NewQuoteLine {
            product_id: Some(self.fan.clone()),
            line: NewLine {
                description: "AF-630 axial fan".to_owned(),
                unit: "piece".to_owned(),
                qty_milli: units * 1_000,
                unit_price_cents: 129_500,
                vat_rate_bp: 2100,
            },
        }
    }
}

/// A charge in words: two days of commissioning, naming nothing in the catalog.
fn services() -> NewQuoteLine {
    NewQuoteLine::from(NewLine {
        description: "Commissioning, two days".to_owned(),
        unit: "day".to_owned(),
        qty_milli: 2_000,
        unit_price_cents: 95_000,
        vat_rate_bp: 2100,
    })
}

#[tokio::test]
async fn an_offer_of_services_still_becomes_an_invoice_directly() {
    // The path that already worked and must go on working, byte for byte. Its
    // fuller proof is `billing_quote_to_invoice.rs`; what this asserts is the
    // *routing decision* — that an offer with nothing stocked on it does not
    // wander off into an order.
    let store = common::test_store().await;
    let us = Trading::open(&store, "services").await;
    let offer = us.sent(&[services()]).await;

    let accepted = us.door.accept_billing_quote(&offer).await.unwrap();
    let invoice_id = accepted
        .outcome
        .invoice_id()
        .expect("a services offer becomes an invoice");
    assert!(
        accepted.outcome.sales_order_id().is_none(),
        "and no order at all: there is nothing to pick"
    );
    let invoice = us
        .door
        .billing_invoice(invoice_id)
        .await
        .unwrap()
        .expect("the draft exists");
    assert_eq!(invoice.lines.len(), 1);
    assert_eq!(invoice.totals.net_cents, 190_000);
}

#[tokio::test]
async fn an_offer_of_goods_becomes_a_draft_order_that_can_be_delivered() {
    let store = common::test_store().await;
    let us = Trading::open(&store, "goods").await;
    let offer = us.sent(&[us.goods(6)]).await;

    let accepted = us.door.accept_billing_quote(&offer).await.unwrap();
    let order_id = accepted
        .outcome
        .sales_order_id()
        .expect("a goods offer becomes an order");
    assert!(
        accepted.outcome.invoice_id().is_none(),
        "and no invoice: goods are billed once they have shipped"
    );

    let order = us
        .door
        .inv_sales_order(order_id)
        .await
        .unwrap()
        .expect("the order exists");
    assert_eq!(order.order.status, SoStatus::Draft);
    assert_eq!(order.order.currency, "EUR");
    assert_eq!(order.totals.net_cents, 777_000, "six fans at EUR 1 295");

    // **The line names the product**, which is the whole reason this item needed
    // a schema change: an order line that named nothing could never be
    // delivered, because `inv_so_deliver` refuses a charge in words.
    assert_eq!(order.lines.len(), 1);
    assert_eq!(
        order.lines[0]
            .product_id
            .as_ref()
            .map(BillingProductId::as_str),
        Some(us.fan.as_str())
    );
    assert_eq!(order.lines[0].line.qty_milli, 6_000);
    assert_eq!(order.lines[0].line.description, "AF-630 axial fan");

    // And it records where it came from (item O1.b), so the two documents can
    // each answer what became of the other.
    assert_eq!(
        order.order.quote_id.as_ref().map(BillingQuoteId::as_str),
        Some(offer.as_str())
    );
    // The offer is closed exactly as a services acceptance closes it.
    assert_eq!(
        accepted.quote.quote.status,
        alo_store::QuoteStatus::Accepted
    );
}

#[tokio::test]
async fn one_line_of_goods_is_enough_to_make_it_an_order() {
    // A fan plus two days of commissioning is a goods offer: something has to be
    // picked and shipped, so it needs an order. The services line rides along on
    // it as a charge in words, exactly as it would on an order typed by hand.
    let store = common::test_store().await;
    let us = Trading::open(&store, "mixed").await;
    let offer = us.sent(&[services(), us.goods(1)]).await;

    let accepted = us.door.accept_billing_quote(&offer).await.unwrap();
    let order_id = accepted
        .outcome
        .sales_order_id()
        .expect("one stocked line is enough");
    let order = us.door.inv_sales_order(order_id).await.unwrap().unwrap();

    assert_eq!(order.lines.len(), 2, "both lines came across");
    assert_eq!(
        order.lines[0].product_id, None,
        "the commissioning is still a charge in words"
    );
    assert_eq!(
        order.lines[1]
            .product_id
            .as_ref()
            .map(BillingProductId::as_str),
        Some(us.fan.as_str())
    );
    assert_eq!(
        order.lines[0].line.line_order, 0,
        "and the offer's order is kept"
    );
    assert_eq!(order.totals.net_cents, 190_000 + 129_500);
}

#[tokio::test]
async fn accepting_never_confirms_and_so_never_promises_stock() {
    // The order is a **draft**. Confirming is a separate act — the one that
    // draws the number and, since O1.a, refuses to promise goods that cannot
    // exist. If acceptance confirmed, a customer saying yes would silently
    // commit stock nobody had checked.
    let store = common::test_store().await;
    let us = Trading::open(&store, "draft").await;
    let offer = us.sent(&[us.goods(9)]).await;

    let accepted = us.door.accept_billing_quote(&offer).await.unwrap();
    let order_id = accepted.outcome.sales_order_id().unwrap();
    let order = us.door.inv_sales_order(order_id).await.unwrap().unwrap();
    assert_eq!(order.order.status, SoStatus::Draft);
    assert!(order.order.number.is_none(), "a draft consumed no number");
    assert!(order.order.confirmed_date.is_none());

    // Nothing is promised: with an empty warehouse, confirming it now is
    // refused — which is the proof that acceptance had not already done so.
    let refused = us.door.confirm_inv_sales_order(order_id, false).await;
    assert!(
        matches!(refused, Err(StoreError::Conflict(_))),
        "an empty warehouse cannot back nine fans: {refused:?}"
    );
}

#[tokio::test]
async fn another_tenants_product_can_never_be_offered_by_us() {
    // The mandatory wrong-tenant test, at the door this item opened: a quote
    // line now names a catalog item, so it is a new way to reach across a
    // boundary. A stranger's product is the clean not-found every other
    // cross-tenant reference answers with — never a foreign-key error, which
    // would confirm the id exists.
    let store = common::test_store().await;
    let us = Trading::open(&store, "wall-ours").await;
    let neighbour = Trading::open(&store, "wall-theirs").await;

    let theirs = NewQuoteLine {
        product_id: Some(neighbour.fan.clone()),
        line: NewLine {
            description: "Their fan".to_owned(),
            unit: "piece".to_owned(),
            qty_milli: 1_000,
            unit_price_cents: 129_500,
            vat_rate_bp: 2100,
        },
    };
    let id = us
        .door
        .create_billing_quote(&NewQuote::for_customer(us.customer.clone()))
        .await
        .unwrap();
    assert_not_found(us.door.set_billing_quote_lines(&id, &[theirs]).await);

    // Nothing was written: the offer is still empty, so a refusal cannot have
    // half-applied a line naming somebody else's item.
    let read = us.door.billing_quote(&id).await.unwrap().unwrap();
    assert!(read.lines.is_empty());

    // An id that never existed answers identically.
    assert_not_found(
        us.door
            .set_billing_quote_lines(
                &id,
                &[NewQuoteLine {
                    product_id: Some(BillingProductId::new("prod-never-existed")),
                    line: services().line,
                }],
            )
            .await,
    );
}
