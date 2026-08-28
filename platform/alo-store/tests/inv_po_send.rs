//! **Placing** a purchase order (alo Inventory, wave B5.05a2) on the real wire
//! — the one act that draws the number, stamps the day, freezes the order and
//! writes the letter, proved to be all-or-nothing.
//!
//! Three properties are what this suite exists for, and none of them can be
//! argued from reading the SQL:
//!
//! 1. **The letter decides.** The callback that writes it runs inside the
//!    placing transaction, so a letter that fails leaves the order a draft —
//!    and gives its drawn number back rather than leaving a hole. That is the
//!    property a Postgres `SEQUENCE` could not have given us.
//! 2. **One document, one number.** A sent order refuses to be sent again, and
//!    twenty-five parallel placements produce exactly the numbers 1..=25.
//! 3. **Tenancy.** Another tenant's order is a clean `NotFound`, its letter
//!    callback never runs, and nothing about it moves.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use alo_store::inv_po::{NewPurchaseOrder, PoStatus, PurchaseOrderDocument};
use alo_store::inv_po_lines::NewPoLine;
use alo_store::inv_suppliers::NewSupplier;
use alo_store::{
    AccountStore, BillingProductId, InvPurchaseOrderId, InvSupplierId, NewLine, NewProduct, Store,
    StoreError, TenantId, document_number,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::Date;

/// The prefix and series a purchase-order number is drawn from — deliberately
/// spelled out here rather than imported, so a change to either has to be
/// deliberate enough to update this test.
const PO_PREFIX: &str = "PO";
const PO_KIND: &str = "purchase_order";

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(message)) => message,
        other => panic!("expected Conflict, got: {other:?}"),
    }
}

fn assert_validation<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(message)) => message,
        other => panic!("expected Validation, got: {other:?}"),
    }
}

/// A raw pool alongside the store, for reading the counter row and the columns
/// no store read surfaces.
async fn raw_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::database_url())
        .await
        .expect("connect to test postgres")
}

async fn today(pool: &PgPool) -> Date {
    sqlx::query_scalar("SELECT CURRENT_DATE")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The counter's current state: the number the next order would take, or `None`
/// while the series has never been drawn from.
async fn next_value(pool: &PgPool, tenant: &TenantId, year: i32) -> Option<i64> {
    sqlx::query_scalar(
        "SELECT next_value FROM billing_sequences \
         WHERE tenant_id = $1 AND kind = $2 AND year = $3",
    )
    .bind(tenant.as_str())
    .bind(PO_KIND)
    .bind(year)
    .fetch_optional(pool)
    .await
    .unwrap()
}

async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("send-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@purchasing.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

async fn supplier(account: &AccountStore, tag: &str) -> InvSupplierId {
    account
        .create_inv_supplier(&NewSupplier {
            name: format!("Hoffmann {tag}"),
            country: "DE".to_owned(),
            currency: "CHF".to_owned(),
            email: Some(format!("orders+{tag}@hoffmann.test")),
            lead_time_days: 9,
            ..Default::default()
        })
        .await
        .unwrap()
}

async fn product(account: &AccountStore, name: &str) -> BillingProductId {
    account
        .create_billing_product(&NewProduct {
            name: name.to_owned(),
            unit: "piece".to_owned(),
            unit_price_cents: 8_600,
            vat_rate_bp: 1900,
            stocked: true,
            purchase_price_cents: 4_300,
            ..Default::default()
        })
        .await
        .unwrap()
}

fn goods(product_id: &BillingProductId, qty_milli: i64, price_cents: i64) -> NewPoLine {
    NewPoLine {
        product_id: Some(product_id.clone()),
        line: NewLine {
            description: "Blue chair".to_owned(),
            unit: "piece".to_owned(),
            qty_milli,
            unit_price_cents: price_cents,
            vat_rate_bp: 1900,
        },
    }
}

/// A draft order with one line of goods on it — the state a placement starts
/// from.
async fn draft_with_a_line(
    account: &AccountStore,
    supplier: &InvSupplierId,
    product: &BillingProductId,
) -> InvPurchaseOrderId {
    let id = account
        .create_inv_purchase_order(&NewPurchaseOrder::for_supplier(supplier.clone()))
        .await
        .unwrap();
    account
        .set_inv_purchase_order_lines(&id, &[goods(product, 4_000, 4_300)])
        .await
        .unwrap();
    id
}

/// A letter that succeeds, recording the document it was handed.
///
/// Stands for the route's real callback (render the PDF, write the draft): what
/// the store guarantees is *when* it runs and what happens if it fails, and
/// both are visible from a closure that only remembers what it saw.
type Seen = Arc<std::sync::Mutex<Option<PurchaseOrderDocument>>>;

#[tokio::test]
async fn placing_an_order_numbers_it_dates_it_and_freezes_it() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant) = tenant_with_user(&store, "place").await;
    let hoffmann = supplier(&a, "place").await;
    let chair = product(&a, "Blue chair").await;
    let day = today(&pool).await;

    let id = draft_with_a_line(&a, &hoffmann, &chair).await;
    let seen: Seen = Arc::new(std::sync::Mutex::new(None));

    let (order, carried) = a
        .send_inv_purchase_order::<&'static str, StoreError, _, _>(&id, |placed| {
            let seen = Arc::clone(&seen);
            async move {
                *seen.lock().unwrap() = Some(placed);
                Ok("written")
            }
        })
        .await
        .unwrap();
    assert_eq!(carried, "written", "what the letter returns comes back");

    // The letter saw the order as it will be stored — number and date included,
    // because the paper it renders is what the supplier quotes back.
    let letter_saw = seen.lock().unwrap().clone().expect("the letter ran");
    let expected = document_number(PO_PREFIX, day.year(), 1);
    assert_eq!(letter_saw.order.number.as_deref(), Some(expected.as_str()));
    assert_eq!(letter_saw.order.ordered_date, Some(day));
    assert_eq!(letter_saw.order.status, PoStatus::Sent);
    assert_eq!(letter_saw.lines.len(), 1, "with its lines");
    assert_eq!(letter_saw.totals.gross_cents, 20_468);
    assert_eq!(letter_saw.supplier_name, "Hoffmann place");

    // And the returned document is the same document.
    assert_eq!(order.order.number, letter_saw.order.number);
    assert_eq!(order.order.status, PoStatus::Sent);

    // …as is what a later read gives.
    let stored = a.inv_purchase_order(&id).await.unwrap().unwrap();
    assert_eq!(stored.order.number.as_deref(), Some(expected.as_str()));
    assert_eq!(stored.order.ordered_date, Some(day));
    assert_eq!(stored.order.status, PoStatus::Sent);
    assert!(
        stored.order.closed_date.is_none(),
        "a sent order is not finished with"
    );

    // Frozen: the supplier holds this paper now.
    let refused = assert_conflict(
        a.set_inv_purchase_order_lines(&id, &[goods(&chair, 1_000, 4_300)])
            .await,
    );
    assert!(
        refused.contains("draft") && refused.contains("sent"),
        "{refused}"
    );
    assert_conflict(a.delete_inv_purchase_order(&id).await);

    // Sending it again is refused — one document, one number.
    let again = assert_conflict(
        a.send_inv_purchase_order::<(), StoreError, _, _>(&id, |_| async {
            panic!("the letter must not run for an order that is already out")
        })
        .await,
    );
    assert!(again.contains("sent"), "{again}");

    // The next order takes the next number, and the counter agrees.
    let second = draft_with_a_line(&a, &hoffmann, &chair).await;
    let (second, _) = a
        .send_inv_purchase_order::<(), StoreError, _, _>(&second, |_| async { Ok(()) })
        .await
        .unwrap();
    assert_eq!(
        second.order.number,
        Some(document_number(PO_PREFIX, day.year(), 2))
    );
    assert_eq!(next_value(&pool, &tenant, day.year()).await, Some(3));

    store.delete_tenant(&tenant).await.unwrap();
}

#[tokio::test]
async fn a_letter_that_cannot_be_written_leaves_the_order_a_draft_and_its_number_unspent() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant) = tenant_with_user(&store, "rollback").await;
    let hoffmann = supplier(&a, "rollback").await;
    let chair = product(&a, "Blue chair").await;
    let day = today(&pool).await;

    let id = draft_with_a_line(&a, &hoffmann, &chair).await;

    // The letter fails — a supplier with no address, a mailbox that cannot be
    // opened. The whole act is off.
    let refused = assert_validation(
        a.send_inv_purchase_order::<(), StoreError, _, _>(&id, |placed| async move {
            // It *was* numbered inside the transaction: that is precisely the
            // number this test proves is given back.
            assert!(placed.order.number.is_some());
            Err(StoreError::Validation("no mailbox".to_owned()))
        })
        .await,
    );
    assert_eq!(refused, "no mailbox");

    let stored = a.inv_purchase_order(&id).await.unwrap().unwrap();
    assert_eq!(stored.order.status, PoStatus::Draft, "still a draft");
    assert!(
        stored.order.number.is_none() && stored.order.ordered_date.is_none(),
        "an order nobody was told about must carry no number"
    );
    // Editable again, because it never left the building.
    a.set_inv_purchase_order_lines(&id, &[goods(&chair, 2_000, 4_300)])
        .await
        .unwrap();

    // The number was given back, not burned: the next successful placement is
    // the *first* number of the year, with no hole before it.
    let (placed, _) = a
        .send_inv_purchase_order::<(), StoreError, _, _>(&id, |_| async { Ok(()) })
        .await
        .unwrap();
    assert_eq!(
        placed.order.number,
        Some(document_number(PO_PREFIX, day.year(), 1)),
        "a rolled-back draw leaves no gap"
    );
    assert_eq!(next_value(&pool, &tenant, day.year()).await, Some(2));

    store.delete_tenant(&tenant).await.unwrap();
}

#[tokio::test]
async fn an_order_that_asks_for_nothing_is_not_sent() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant) = tenant_with_user(&store, "empty").await;
    let hoffmann = supplier(&a, "empty").await;
    let day = today(&pool).await;

    let id = a
        .create_inv_purchase_order(&NewPurchaseOrder::for_supplier(hoffmann))
        .await
        .unwrap();
    let refused = assert_validation(
        a.send_inv_purchase_order::<(), StoreError, _, _>(&id, |_| async {
            panic!("an empty order must be refused before the letter")
        })
        .await,
    );
    assert!(refused.contains("no lines"), "{refused}");

    let stored = a.inv_purchase_order(&id).await.unwrap().unwrap();
    assert_eq!(stored.order.status, PoStatus::Draft);
    assert!(stored.order.number.is_none());
    // Refused before the counter was ever touched.
    assert_eq!(next_value(&pool, &tenant, day.year()).await, None);

    store.delete_tenant(&tenant).await.unwrap();
}

/// Law 1, on the one route that both writes a document and writes a message:
/// tenant B cannot place tenant A's order, cannot learn that it exists, and
/// cannot get a letter written about it.
#[tokio::test]
async fn another_tenants_order_is_never_placed_and_never_written_about() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant_a) = tenant_with_user(&store, "ours").await;
    let (b, tenant_b) = tenant_with_user(&store, "theirs").await;
    let hoffmann = supplier(&a, "ours").await;
    let chair = product(&a, "Blue chair").await;
    let day = today(&pool).await;

    let id = draft_with_a_line(&a, &hoffmann, &chair).await;

    let letters = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&letters);
    assert_not_found(
        b.send_inv_purchase_order::<(), StoreError, _, _>(&id, |_| {
            let counted = Arc::clone(&counted);
            async move {
                counted.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .await,
    );
    assert_eq!(
        letters.load(Ordering::SeqCst),
        0,
        "no letter is written about a document that is not yours"
    );

    // Nothing about ours moved, and B's own series was never opened.
    let stored = a.inv_purchase_order(&id).await.unwrap().unwrap();
    assert_eq!(stored.order.status, PoStatus::Draft);
    assert!(stored.order.number.is_none());
    assert_eq!(next_value(&pool, &tenant_b, day.year()).await, None);

    // Ours still places, and B still sees nothing.
    a.send_inv_purchase_order::<(), StoreError, _, _>(&id, |_| async { Ok(()) })
        .await
        .unwrap();
    assert!(b.inv_purchase_orders(None).await.unwrap().is_empty());
    assert!(b.inv_purchase_order(&id).await.unwrap().is_none());
    // The number is ours, and asking for it by number as B finds nothing.
    let number = document_number(PO_PREFIX, day.year(), 1);
    assert_eq!(
        a.inv_purchase_order_id_by_number(&number)
            .await
            .unwrap()
            .map(|found| found.as_str().to_owned()),
        Some(id.as_str().to_owned())
    );
    assert!(
        b.inv_purchase_order_id_by_number(&number)
            .await
            .unwrap()
            .is_none()
    );

    store.delete_tenant(&tenant_a).await.unwrap();
    store.delete_tenant(&tenant_b).await.unwrap();
}

/// The item's gate: placements fired concurrently against one tenant's series
/// produce exactly the numbers 1..=N — none shared, none skipped.
///
/// Sharing would mean two orders a supplier cannot tell apart; skipping is
/// harmless in law for a PO but would mean the counter and the documents
/// disagree, which is the same defect that matters for invoices.
#[tokio::test]
async fn parallel_placements_never_share_or_skip_a_number() {
    const ORDERS: usize = 25;

    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant) = tenant_with_user(&store, "parallel").await;
    let hoffmann = supplier(&a, "parallel").await;
    let chair = product(&a, "Blue chair").await;
    let day = today(&pool).await;

    let mut drafts = Vec::with_capacity(ORDERS);
    for _ in 0..ORDERS {
        drafts.push(draft_with_a_line(&a, &hoffmann, &chair).await);
    }

    // All at once, contending on one counter row: the real race, not a
    // staggered sequence of calls.
    let mut running = Vec::with_capacity(ORDERS);
    for id in drafts {
        let account = a.clone();
        running.push(tokio::spawn(async move {
            account
                .send_inv_purchase_order::<(), StoreError, _, _>(&id, |_| async { Ok(()) })
                .await
                .map(|(document, ())| document)
        }));
    }

    let mut numbers = BTreeSet::new();
    for task in running {
        let placed = task.await.unwrap().unwrap();
        let number = placed.order.number.expect("a sent order is numbered");
        assert!(numbers.insert(number.clone()), "number {number} was shared");
    }
    let expected: BTreeSet<String> = (1..=ORDERS as i64)
        .map(|value| document_number(PO_PREFIX, day.year(), value))
        .collect();
    assert_eq!(numbers, expected, "the series must be exactly 1..={ORDERS}");
    assert_eq!(
        next_value(&pool, &tenant, day.year()).await,
        Some(ORDERS as i64 + 1),
        "and the counter agrees with what was handed out"
    );

    // Read back from the table: distinct numbers, every order sent.
    let stored: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT number) FROM inv_purchase_orders \
         WHERE tenant_id = $1 AND status = 'sent'",
    )
    .bind(tenant.as_str())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, ORDERS as i64);

    store.delete_tenant(&tenant).await.unwrap();
}

/// The order's series is its own: placing an order must not move the invoice or
/// quote counters, and must not leave a hole in either.
#[tokio::test]
async fn an_order_draws_from_its_own_series_and_leaves_the_others_alone() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant) = tenant_with_user(&store, "series").await;
    let hoffmann = supplier(&a, "series").await;
    let chair = product(&a, "Blue chair").await;
    let day = today(&pool).await;

    let id = draft_with_a_line(&a, &hoffmann, &chair).await;
    a.send_inv_purchase_order::<(), StoreError, _, _>(&id, |_| async { Ok(()) })
        .await
        .unwrap();

    for other in ["invoice", "quote"] {
        let value: Option<i64> = sqlx::query_scalar(
            "SELECT next_value FROM billing_sequences \
             WHERE tenant_id = $1 AND kind = $2 AND year = $3",
        )
        .bind(tenant.as_str())
        .bind(other)
        .bind(day.year())
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert_eq!(value, None, "the {other} series must be untouched");
    }
    assert_eq!(next_value(&pool, &tenant, day.year()).await, Some(2));

    store.delete_tenant(&tenant).await.unwrap();
}
