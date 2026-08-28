//! The purchase-order record and its lifecycle (alo Inventory, wave B5.05a) on
//! the real wire — and the tenancy proof Law 1 demands for it.
//!
//! The pure transition table is unit-tested over all twenty-five ordered pairs
//! in `inv_po.rs`. What these tests prove is the part only a database can
//! answer: that a draft snapshots the supplier's currency and carries no
//! number, that the line set is replaced whole with its product links intact
//! and its totals derived, that a frozen order refuses every write, that
//! cancelling stamps the day and is terminal — and that none of it, on any
//! path, reaches another tenant's data.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::inv_po::{NewPurchaseOrder, PoStatus};
use alo_store::inv_po_lines::NewPoLine;
use alo_store::inv_suppliers::NewSupplier;
use alo_store::{
    AccountStore, BillingProductId, InvPurchaseOrderId, InvSupplierId, NewLine, NewProduct, Store,
    StoreError, TenantId,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::{Date, Month};

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error.
fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

/// Asserts a result is the typed lifecycle refusal, returning its message.
fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(message)) => message,
        other => panic!("expected Conflict, got: {other:?}"),
    }
}

/// Asserts a result is the typed field refusal, returning its message.
fn assert_validation<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(message)) => message,
        other => panic!("expected Validation, got: {other:?}"),
    }
}

/// A tenant with one user, returning the account door plus the tenant id.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("po-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@purchasing.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

/// A supplier who quotes in their own currency, so the snapshot is visible.
async fn supplier(account: &AccountStore, tag: &str, currency: &str) -> InvSupplierId {
    account
        .create_inv_supplier(&NewSupplier {
            name: format!("Hoffmann {tag}"),
            country: "DE".to_owned(),
            currency: currency.to_owned(),
            email: Some(format!("orders+{tag}@hoffmann.test")),
            lead_time_days: 9,
            ..Default::default()
        })
        .await
        .unwrap()
}

/// A stocked catalog item.
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

/// A line for goods: a product, a quantity in milli-units, their price.
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

/// A charge in words: no product, and free to be negative.
fn charge(description: &str, qty_milli: i64, price_cents: i64) -> NewPoLine {
    NewPoLine {
        product_id: None,
        line: NewLine {
            description: description.to_owned(),
            unit: String::new(),
            qty_milli,
            unit_price_cents: price_cents,
            vat_rate_bp: 1900,
        },
    }
}

/// A raw pool alongside the store, for planting the states only later items
/// write (sending, receiving) and for reading columns no store read surfaces.
async fn raw_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::database_url())
        .await
        .expect("connect to test postgres")
}

/// Puts an order into a state B5.05a2/B5.05b will write, so this item's guards
/// can be proved against it today. Writes exactly what those transitions will:
/// the number the caller names and an order date.
async fn place(
    pool: &PgPool,
    tenant: &TenantId,
    id: &InvPurchaseOrderId,
    status: PoStatus,
    number: &str,
) {
    sqlx::query(
        "UPDATE inv_purchase_orders \
            SET status = $3, number = $4, ordered_date = CURRENT_DATE \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_str())
    .bind(id.as_str())
    .bind(status.as_str())
    .bind(number)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn a_draft_order_is_written_read_and_edited_as_a_whole() {
    let store = common::test_store().await;
    let (a, _t) = tenant_with_user(&store, "draft").await;
    let hoffmann = supplier(&a, "draft", "CHF").await;
    let chair = product(&a, "Blue chair").await;

    // ---- create: the supplier's currency is snapshotted, nothing else is --
    let id = a
        .create_inv_purchase_order(&NewPurchaseOrder::for_supplier(hoffmann.clone()))
        .await
        .unwrap();
    let document = a.inv_purchase_order(&id).await.unwrap().unwrap();
    assert_eq!(document.order.status, PoStatus::Draft);
    assert_eq!(document.order.currency, "CHF");
    assert_eq!(document.supplier_name, "Hoffmann draft");
    assert!(
        document.order.number.is_none() && document.order.ordered_date.is_none(),
        "a draft nobody sent must not consume a number"
    );
    assert!(document.order.closed_date.is_none());
    assert!(document.lines.is_empty());
    assert_eq!(document.totals.gross_cents, 0);

    // ---- the lines: replaced whole, in the caller's order -----------------
    a.set_inv_purchase_order_lines(
        &id,
        &[
            goods(&chair, 4_000, 4_300),
            charge("Freight", 1_000, 2_500),
            charge("Agreed discount", -1_000, 1_000),
        ],
    )
    .await
    .unwrap();
    let document = a.inv_purchase_order(&id).await.unwrap().unwrap();
    assert_eq!(document.lines.len(), 3);
    assert_eq!(
        document.lines.first().unwrap().product_id.as_ref(),
        Some(&chair)
    );
    assert!(document.lines[1].product_id.is_none());
    assert_eq!(document.lines[2].line.qty_milli, -1_000);
    for (position, line) in document.lines.iter().enumerate() {
        assert_eq!(line.line.line_order, i32::try_from(position).unwrap());
    }
    // 4 × 43.00 + 1 × 25.00 − 1 × 10.00 = 187.00 net, 19 % → 222.53 gross.
    assert_eq!(document.totals.net_cents, 18_700);
    assert_eq!(document.totals.gross_cents, 22_253);

    // A second set replaces, never appends.
    a.set_inv_purchase_order_lines(&id, &[goods(&chair, 1_000, 4_000)])
        .await
        .unwrap();
    let document = a.inv_purchase_order(&id).await.unwrap().unwrap();
    assert_eq!(document.lines.len(), 1);
    assert_eq!(document.totals.net_cents, 4_000);

    // ---- the header: a full replace, currency stated outright -------------
    let expected = Date::from_calendar_date(2026, Month::September, 1).unwrap();
    a.update_inv_purchase_order(
        &id,
        &NewPurchaseOrder {
            supplier_id: hoffmann.clone(),
            currency: Some("eur".to_owned()),
            expected_date: Some(expected),
            reference: "Project Falkenstein".to_owned(),
            note: "Deliver to the rear entrance".to_owned(),
        },
    )
    .await
    .unwrap();
    let document = a.inv_purchase_order(&id).await.unwrap().unwrap();
    assert_eq!(document.order.currency, "EUR");
    assert_eq!(document.order.expected_date, Some(expected));
    assert_eq!(document.order.reference, "Project Falkenstein");
    assert!(
        !document.order.is_late(expected.next_day().unwrap()),
        "a draft is waiting on nobody, whatever date it names"
    );

    // ---- the list: newest first, with the supplier and the money ----------
    let listed = a.inv_purchase_orders(None).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].supplier_name, "Hoffmann draft");
    assert_eq!(listed[0].totals.net_cents, 4_000);
    assert_eq!(
        a.inv_purchase_orders(Some(PoStatus::Draft))
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        a.inv_purchase_orders(Some(PoStatus::Sent))
            .await
            .unwrap()
            .is_empty(),
        "the status filter narrows rather than widening"
    );

    // ---- delete: the only order ever removed ------------------------------
    a.delete_inv_purchase_order(&id).await.unwrap();
    assert!(a.inv_purchase_order(&id).await.unwrap().is_none());
    assert!(a.inv_purchase_orders(None).await.unwrap().is_empty());
}

#[tokio::test]
async fn what_an_order_refuses_it_refuses_by_rule() {
    let store = common::test_store().await;
    let (a, _t) = tenant_with_user(&store, "rules").await;
    let hoffmann = supplier(&a, "rules", "EUR").await;
    let chair = product(&a, "Blue chair").await;

    // An order from a supplier we no longer buy from is a mistake worth
    // reporting rather than obeying.
    let retired = supplier(&a, "retired", "EUR").await;
    a.set_inv_supplier_archived(&retired, true).await.unwrap();
    let message = assert_validation(
        a.create_inv_purchase_order(&NewPurchaseOrder::for_supplier(retired.clone()))
            .await,
    );
    assert!(message.contains("archived"), "{message}");

    let id = a
        .create_inv_purchase_order(&NewPurchaseOrder::for_supplier(hoffmann.clone()))
        .await
        .unwrap();

    // A line that orders goods orders more than nothing…
    let message = assert_validation(
        a.set_inv_purchase_order_lines(&id, &[goods(&chair, 0, 4_300)])
            .await,
    );
    assert!(message.starts_with("line 1: "), "{message}");

    // …and a product that is not in this catalog is not orderable at all.
    assert_not_found(
        a.set_inv_purchase_order_lines(
            &id,
            &[goods(&BillingProductId::new("no-such-product"), 1_000, 100)],
        )
        .await,
    );

    // An archived product is refused by name of the line, not of the product.
    let discontinued = product(&a, "Red chair").await;
    a.set_billing_product_archived(&discontinued, true)
        .await
        .unwrap();
    let message = assert_validation(
        a.set_inv_purchase_order_lines(
            &id,
            &[
                goods(&chair, 1_000, 4_300),
                goods(&discontinued, 1_000, 100),
            ],
        )
        .await,
    );
    assert!(message.starts_with("line 2: "), "{message}");
    assert!(message.contains("archived"), "{message}");

    // A refused set leaves the order exactly as it was — nothing half-written.
    assert!(
        a.inv_purchase_order(&id)
            .await
            .unwrap()
            .unwrap()
            .lines
            .is_empty()
    );

    // An unknown order is a 404 on every path, not an empty success.
    let ghost = InvPurchaseOrderId::new("ghost");
    assert!(a.inv_purchase_order(&ghost).await.unwrap().is_none());
    assert_not_found(
        a.update_inv_purchase_order(&ghost, &NewPurchaseOrder::for_supplier(hoffmann))
            .await,
    );
    assert_not_found(a.set_inv_purchase_order_lines(&ghost, &[]).await);
    assert_not_found(a.delete_inv_purchase_order(&ghost).await);
    assert_not_found(a.cancel_inv_purchase_order(&ghost, false).await);
}

#[tokio::test]
async fn a_placed_order_is_frozen_and_a_cancelled_one_is_final() {
    let store = common::test_store().await;
    let (a, t) = tenant_with_user(&store, "frozen").await;
    let pool = raw_pool().await;
    let hoffmann = supplier(&a, "frozen", "EUR").await;
    let chair = product(&a, "Blue chair").await;

    let expected = Date::from_calendar_date(2026, Month::September, 1).unwrap();
    let id = a
        .create_inv_purchase_order(&NewPurchaseOrder {
            expected_date: Some(expected),
            ..NewPurchaseOrder::for_supplier(hoffmann.clone())
        })
        .await
        .unwrap();
    a.set_inv_purchase_order_lines(&id, &[goods(&chair, 4_000, 4_300)])
        .await
        .unwrap();

    // The state B5.05a2 will write. Every write path must refuse it.
    place(&pool, &t, &id, PoStatus::Sent, "PO-2026-09001").await;
    let frozen = a.inv_purchase_order(&id).await.unwrap().unwrap();
    assert_eq!(frozen.order.status, PoStatus::Sent);
    assert_eq!(frozen.order.number.as_deref(), Some("PO-2026-09001"));
    assert!(
        frozen.order.is_late(expected.next_day().unwrap()),
        "a placed order past the day we expected the goods is late"
    );
    assert!(!frozen.order.is_late(expected), "they still have the day");

    for message in [
        assert_conflict(
            a.update_inv_purchase_order(&id, &NewPurchaseOrder::for_supplier(hoffmann))
                .await,
        ),
        assert_conflict(
            a.set_inv_purchase_order_lines(&id, &[goods(&chair, 1_000, 1)])
                .await,
        ),
        assert_conflict(a.delete_inv_purchase_order(&id).await),
    ] {
        assert!(message.contains("draft"), "{message}");
        assert!(message.contains("sent"), "{message}");
    }
    // …and the document still says what the supplier was told.
    let unchanged = a.inv_purchase_order(&id).await.unwrap().unwrap();
    assert_eq!(unchanged.lines.len(), 1);
    assert_eq!(unchanged.totals.net_cents, 17_200);

    // ---- cancelling: stamped, terminal, and it keeps its number -----------
    let cancelled = a.cancel_inv_purchase_order(&id, false).await.unwrap();
    assert_eq!(cancelled.order.status, PoStatus::Cancelled);
    assert!(cancelled.order.closed_date.is_some());
    assert_eq!(cancelled.order.number.as_deref(), Some("PO-2026-09001"));
    let message = assert_conflict(a.cancel_inv_purchase_order(&id, false).await);
    assert!(message.contains("closed"), "{message}");

    // ---- a part-delivered order needs the shortfall accepted out loud -----
    let partly = a
        .create_inv_purchase_order(&NewPurchaseOrder::for_supplier(
            supplier(&a, "partly", "EUR").await,
        ))
        .await
        .unwrap();
    place(
        &pool,
        &t,
        &partly,
        PoStatus::PartiallyReceived,
        "PO-2026-09002",
    )
    .await;
    let message = assert_conflict(a.cancel_inv_purchase_order(&partly, false).await);
    assert!(message.contains("short delivery"), "{message}");
    let closed = a.cancel_inv_purchase_order(&partly, true).await.unwrap();
    assert_eq!(closed.order.status, PoStatus::Cancelled);

    // A draft is cancellable too — the decision to drop it is on the record.
    let abandoned = a
        .create_inv_purchase_order(&NewPurchaseOrder::for_supplier(
            supplier(&a, "abandoned", "EUR").await,
        ))
        .await
        .unwrap();
    let dropped = a
        .cancel_inv_purchase_order(&abandoned, false)
        .await
        .unwrap();
    assert_eq!(dropped.order.status, PoStatus::Cancelled);
    assert!(
        dropped.order.number.is_none(),
        "it was never placed, so it never had a number"
    );
    assert!(dropped.order.closed_date.is_some());
}

#[tokio::test]
async fn orders_never_reach_across_tenants() {
    let store = common::test_store().await;
    let (a, _ta) = tenant_with_user(&store, "a").await;
    let (b, _tb) = tenant_with_user(&store, "b").await;

    let ours = supplier(&a, "ours", "EUR").await;
    let chair = product(&a, "Blue chair").await;
    let id = a
        .create_inv_purchase_order(&NewPurchaseOrder::for_supplier(ours.clone()))
        .await
        .unwrap();
    a.set_inv_purchase_order_lines(&id, &[goods(&chair, 4_000, 4_300)])
        .await
        .unwrap();

    // ---- B cannot read it, by id, by list or by number --------------------
    assert!(b.inv_purchase_order(&id).await.unwrap().is_none());
    assert!(b.inv_purchase_orders(None).await.unwrap().is_empty());

    // ---- nor write it on any path -----------------------------------------
    let theirs = supplier(&b, "theirs", "EUR").await;
    assert_not_found(
        b.update_inv_purchase_order(&id, &NewPurchaseOrder::for_supplier(theirs.clone()))
            .await,
    );
    assert_not_found(b.set_inv_purchase_order_lines(&id, &[]).await);
    assert_not_found(b.delete_inv_purchase_order(&id).await);
    assert_not_found(b.cancel_inv_purchase_order(&id, true).await);

    // ---- nor borrow our supplier or our catalog ---------------------------
    assert_not_found(
        b.create_inv_purchase_order(&NewPurchaseOrder::for_supplier(ours))
            .await,
    );
    let theirs_id = b
        .create_inv_purchase_order(&NewPurchaseOrder::for_supplier(theirs))
        .await
        .unwrap();
    assert_not_found(
        b.set_inv_purchase_order_lines(&theirs_id, &[goods(&chair, 1_000, 4_300)])
            .await,
    );

    // ---- and ours is untouched by all of it -------------------------------
    let ours_still = a.inv_purchase_order(&id).await.unwrap().unwrap();
    assert_eq!(ours_still.order.status, PoStatus::Draft);
    assert_eq!(ours_still.lines.len(), 1);
    assert_eq!(a.inv_purchase_orders(None).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_number_belongs_to_the_tenant_that_drew_it() {
    // The lookup B5.10's agent will use, proved before there is an agent: a
    // number is a tenant-local word, and another tenant's is simply absent.
    let store = common::test_store().await;
    let (a, ta) = tenant_with_user(&store, "number").await;
    let (b, _tb) = tenant_with_user(&store, "outsider").await;
    let pool = raw_pool().await;

    let id = a
        .create_inv_purchase_order(&NewPurchaseOrder::for_supplier(
            supplier(&a, "number", "EUR").await,
        ))
        .await
        .unwrap();
    assert!(
        a.inv_purchase_order_id_by_number("PO-2026-09999")
            .await
            .unwrap()
            .is_none(),
        "a draft has no number, so it cannot be found by one"
    );

    place(&pool, &ta, &id, PoStatus::Sent, "PO-2026-09999").await;
    assert_eq!(
        a.inv_purchase_order_id_by_number("  po-2026-09999 ")
            .await
            .unwrap()
            .map(|found| found.as_str().to_owned()),
        Some(id.as_str().to_owned()),
        "blanks trimmed, case ignored, otherwise exact"
    );
    assert!(
        a.inv_purchase_order_id_by_number("   ")
            .await
            .unwrap()
            .is_none(),
        "no number was asked for"
    );
    assert!(
        b.inv_purchase_order_id_by_number("PO-2026-09999")
            .await
            .unwrap()
            .is_none(),
        "another tenant's number is not a number here"
    );
}
