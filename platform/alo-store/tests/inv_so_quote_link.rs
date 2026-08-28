//! The offer an order was taken from (alo Orders, ADR 0054 §4, item O1.b)
//! against the real database.
//!
//! One nullable column with a composite foreign key, mirroring
//! `billing_invoices.quote_id` (migration 0106) — so the two branches of an
//! acceptance answer "where did this come from?" the same way. What only a
//! database can prove is here:
//!
//! | Property | Where |
//! |---|---|
//! | the link is written, read back, and survives an edit | `an_order_remembers_the_offer_it_was_taken_from` |
//! | an order from no offer is the ordinary case | `an_order_taken_over_a_counter_has_no_offer_behind_it` |
//! | **a neighbour's offer can never be claimed** | `another_tenants_quote_can_never_be_the_origin_of_our_order` |
//! | one offer yields at most one order, enforced by the database | `an_offer_can_be_taken_up_as_an_order_only_once` |
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::billing_line::NewLine;
use alo_store::billing_quote_lines::NewQuoteLine;
use alo_store::billing_quotes::NewQuote;
use alo_store::inv_so::NewSalesOrder;
use alo_store::{AccountStore, BillingCustomerId, BillingQuoteId, NewCustomer, Store, StoreError};

fn conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(said)) => said,
        other => panic!("expected Conflict, got: {other:?}"),
    }
}

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    assert!(
        matches!(result, Err(StoreError::NotFound)),
        "expected the clean not-found denial, got: {result:?}"
    );
}

/// A tenant with a customer and one quote, which is all this link needs.
struct Trading {
    door: AccountStore,
    customer: BillingCustomerId,
}

impl Trading {
    async fn open(store: &Store, tag: &str) -> Self {
        let tenant = store.create_tenant(&format!("qlink-{tag}")).await.unwrap();
        let user = store
            .for_tenant(tenant.clone())
            .create_user(&format!("{tag}@qlink.test"))
            .await
            .unwrap();
        let door = store.for_account(tenant, user);
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
        Self { door, customer }
    }

    /// An offer for six fans, as a customer would have received it.
    async fn quote(&self) -> BillingQuoteId {
        let id = self
            .door
            .create_billing_quote(&NewQuote {
                customer_id: self.customer.clone(),
                ..NewQuote::for_customer(self.customer.clone())
            })
            .await
            .unwrap();
        self.door
            .set_billing_quote_lines(
                &id,
                &[NewQuoteLine::from(NewLine {
                    description: "AF-630 axial fan".to_owned(),
                    unit: "piece".to_owned(),
                    qty_milli: 6_000,
                    unit_price_cents: 129_500,
                    vat_rate_bp: 2100,
                })],
            )
            .await
            .unwrap();
        id
    }

    /// A draft order, optionally recording the offer it came from.
    async fn order_from(&self, quote: Option<&BillingQuoteId>) -> Result<NewOrder, StoreError> {
        let mut input = NewSalesOrder::for_customer(self.customer.clone());
        if let Some(quote) = quote {
            input = input.from_quote(quote.clone());
        }
        self.door.create_inv_sales_order(&input).await
    }
}

type NewOrder = alo_store::InvSalesOrderId;

#[tokio::test]
async fn an_order_remembers_the_offer_it_was_taken_from() {
    let store = common::test_store().await;
    let us = Trading::open(&store, "remember").await;
    let offer = us.quote().await;

    let order = us.order_from(Some(&offer)).await.unwrap();
    let read = us.door.inv_sales_order(&order).await.unwrap().unwrap();
    assert_eq!(
        read.order.quote_id.as_ref().map(BillingQuoteId::as_str),
        Some(offer.as_str())
    );

    // It survives an ordinary edit of the header: the link is provenance, and an
    // edit that lost it would make the record unable to answer the one question
    // it was added for.
    us.door
        .update_inv_sales_order(
            &order,
            &NewSalesOrder {
                reference: "Their PO 4711".to_owned(),
                ..NewSalesOrder::for_customer(us.customer.clone()).from_quote(offer.clone())
            },
        )
        .await
        .unwrap();
    let edited = us.door.inv_sales_order(&order).await.unwrap().unwrap();
    assert_eq!(edited.order.reference, "Their PO 4711");
    assert_eq!(
        edited.order.quote_id.as_ref().map(BillingQuoteId::as_str),
        Some(offer.as_str()),
        "an edit must not lose where the order came from"
    );

    // And it shows up in the list, not only on the single read.
    let listed = us.door.inv_sales_orders(None).await.unwrap();
    let ours = listed
        .iter()
        .find(|s| s.order.id.as_str() == order.as_str())
        .expect("our order is listed");
    assert_eq!(
        ours.order.quote_id.as_ref().map(BillingQuoteId::as_str),
        Some(offer.as_str())
    );
}

#[tokio::test]
async fn an_order_taken_over_a_counter_has_no_offer_behind_it() {
    // The ordinary case, and the reason the column is nullable: most orders come
    // from a telephone call rather than a document.
    let store = common::test_store().await;
    let us = Trading::open(&store, "counter").await;
    let order = us.order_from(None).await.unwrap();
    let read = us.door.inv_sales_order(&order).await.unwrap().unwrap();
    assert!(read.order.quote_id.is_none());
}

#[tokio::test]
async fn another_tenants_quote_can_never_be_the_origin_of_our_order() {
    // The mandatory wrong-tenant test. The composite foreign key would refuse a
    // stranger's id on its own, but it would refuse it as a database error —
    // and a foreign id must be indistinguishable from one that never existed,
    // which is what every other cross-tenant reference in this module answers
    // with.
    let store = common::test_store().await;
    let us = Trading::open(&store, "wall-ours").await;
    let neighbour = Trading::open(&store, "wall-theirs").await;
    let theirs = neighbour.quote().await;

    assert_not_found(us.order_from(Some(&theirs)).await);

    // Nothing of ours was written for it, and their offer is untouched: still
    // theirs, still unclaimed, and still able to become their own order.
    assert!(us.door.inv_sales_orders(None).await.unwrap().is_empty());
    assert!(
        neighbour
            .door
            .billing_quote(&theirs)
            .await
            .unwrap()
            .is_some()
    );
    neighbour.order_from(Some(&theirs)).await.unwrap();

    // An id that never existed answers the same way, so ours discloses nothing
    // about whether theirs is real.
    assert_not_found(
        us.order_from(Some(&BillingQuoteId::new("quo-never-existed")))
            .await,
    );
}

#[tokio::test]
async fn an_offer_can_be_taken_up_as_an_order_only_once() {
    // Acceptance is terminal, so the store can produce at most one order per
    // offer — and the partial unique index is what makes "the order taken from
    // this offer" a single row a reader can rely on rather than a list. It is
    // the database saying so, not a check that could be raced past.
    let store = common::test_store().await;
    let us = Trading::open(&store, "once").await;
    let offer = us.quote().await;

    us.order_from(Some(&offer)).await.unwrap();
    let refused = conflict(us.order_from(Some(&offer)).await);
    assert!(
        refused.contains("already been taken up"),
        "the refusal must say what happened: {refused}"
    );

    // A second order with no offer behind it is still perfectly ordinary — the
    // index is partial, so the orders that come from nothing never collide.
    us.order_from(None).await.unwrap();
    us.order_from(None).await.unwrap();
    assert_eq!(us.door.inv_sales_orders(None).await.unwrap().len(), 3);
}
