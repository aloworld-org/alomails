//! Tenancy proof for the supplier master record and the price list a supplier
//! quotes us (alo Inventory, B5.03 — Law 1: isolation is tested, not assumed).
//!
//! Suppliers are tenant-wide — a co-tenant buys from the same list — but an
//! outsider tenant gets the clean `NotFound`/empty on **every** path: read,
//! list, update, archive, and both ends of an offer. The second test carries
//! the offers themselves: the upsert that makes the route an idempotent `PUT`,
//! the effective lead time, and the two cross-tenant refusals that matter most
//! — an offer naming another tenant's product, and a product pointing at
//! another tenant's supplier.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::inv_supplier_prices::NewSupplierPrice;
use alo_store::inv_suppliers::NewSupplier;
use alo_store::{AccountStore, InvSupplierId, NewProduct, Store, StoreError, TenantId};

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error.
fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

/// A fully-specified supplier, so the round trip covers every column.
fn hoffmann() -> NewSupplier {
    NewSupplier {
        name: "Hoffmann Möbel GmbH".to_owned(),
        address_line1: "Industriestraße 4".to_owned(),
        postal_code: "50733".to_owned(),
        city: "Köln".to_owned(),
        country: "de".to_owned(),
        vat_id: Some("de 811.907-980".to_owned()),
        registration_no: "HRB 12345".to_owned(),
        email: Some("orders@hoffmann.test".to_owned()),
        phone: "+49 221 123456".to_owned(),
        iban: Some("nl91 abna 0417 1643 00".to_owned()),
        payment_terms_days: 14,
        lead_time_days: 9,
        note: "Ask for Frau Berger".to_owned(),
        ..Default::default()
    }
}

/// A tenant with one user, returning the account door plus the tenant id.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("supp-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@suppliers.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

#[tokio::test]
async fn suppliers_round_trip_and_never_cross_tenant() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "a").await;
    // A co-tenant user: the supplier list is tenant-wide, like the price list.
    let uc = store
        .for_tenant(t1.clone())
        .create_user("c@suppliers.test")
        .await
        .unwrap();
    let c = store.for_account(t1.clone(), uc);
    let (b, _t2) = tenant_with_user(&store, "b").await;

    // ---- create: normalised on the way in --------------------------------
    let id = a.create_inv_supplier(&hoffmann()).await.unwrap();
    let got = a.inv_supplier(&id).await.unwrap().unwrap();
    assert_eq!(got.name, "Hoffmann Möbel GmbH");
    assert_eq!(got.country, "DE");
    assert_eq!(got.currency, "EUR");
    // Each canonical form is the one its own module publishes.
    assert_eq!(got.vat_id.as_deref(), Some("DE811907980"));
    assert_eq!(got.iban.as_deref(), Some("NL91ABNA0417164300"));
    assert_eq!(got.payment_terms_days, 14);
    assert_eq!(got.lead_time_days, 9);
    assert_eq!(got.note, "Ask for Frau Berger");
    assert!(!got.is_archived());

    // ---- the co-tenant sees the same list --------------------------------
    assert_eq!(c.inv_supplier(&id).await.unwrap().unwrap().name, got.name);
    assert_eq!(c.inv_suppliers(false).await.unwrap().len(), 1);

    // ---- the outsider sees nothing, on every path ------------------------
    assert!(
        b.inv_supplier(&id).await.unwrap().is_none(),
        "another tenant's supplier reads as absent, not as data"
    );
    assert!(b.inv_suppliers(true).await.unwrap().is_empty());
    assert_not_found(b.update_inv_supplier(&id, &hoffmann()).await);
    assert_not_found(b.set_inv_supplier_archived(&id, true).await);
    // …and the record is untouched by any of those attempts.
    let after = a.inv_supplier(&id).await.unwrap().unwrap();
    assert_eq!(after.name, got.name);
    assert!(!after.is_archived());

    // ---- update is a full replace ----------------------------------------
    a.update_inv_supplier(
        &id,
        &NewSupplier {
            name: "Hoffmann Möbel SE".to_owned(),
            country: "DE".to_owned(),
            lead_time_days: 14,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let updated = a.inv_supplier(&id).await.unwrap().unwrap();
    assert_eq!(updated.name, "Hoffmann Möbel SE");
    assert_eq!(updated.lead_time_days, 14);
    assert_eq!(updated.vat_id, None, "a full replace clears what it omits");
    assert_eq!(updated.payment_terms_days, 30, "back to the EU B2B default");

    // ---- validation refuses what the caller can fix ----------------------
    for bad in [
        NewSupplier {
            name: "   ".to_owned(),
            ..hoffmann()
        },
        NewSupplier {
            country: "Germany".to_owned(),
            ..hoffmann()
        },
        NewSupplier {
            iban: Some("NL92ABNA0417164300".to_owned()),
            ..hoffmann()
        },
        NewSupplier {
            lead_time_days: -1,
            ..hoffmann()
        },
    ] {
        assert!(
            matches!(
                a.create_inv_supplier(&bad).await,
                Err(StoreError::Validation(_))
            ),
            "expected a Validation refusal"
        );
    }

    // ---- archive: hidden from the pickers, still readable ----------------
    a.set_inv_supplier_archived(&id, true).await.unwrap();
    assert!(
        a.inv_suppliers(false).await.unwrap().is_empty(),
        "archived suppliers leave the picker"
    );
    let archived = a.inv_suppliers(true).await.unwrap();
    assert_eq!(archived.len(), 1);
    let stamp = archived[0].archived_at;
    assert!(stamp.is_some());
    // Idempotent: re-archiving keeps the original time.
    a.set_inv_supplier_archived(&id, true).await.unwrap();
    assert_eq!(
        a.inv_suppliers(true).await.unwrap()[0].archived_at,
        stamp,
        "re-archiving must not restamp"
    );
    // An order raised last year must still be able to name them.
    assert!(a.inv_supplier(&id).await.unwrap().is_some());
    a.set_inv_supplier_archived(&id, false).await.unwrap();
    assert_eq!(a.inv_suppliers(false).await.unwrap().len(), 1);

    // ---- an unknown id is NotFound, never a Db error ---------------------
    let ghost = InvSupplierId::generate();
    assert!(a.inv_supplier(&ghost).await.unwrap().is_none());
    assert_not_found(a.update_inv_supplier(&ghost, &hoffmann()).await);
    assert_not_found(a.set_inv_supplier_archived(&ghost, true).await);

    // ---- deleting the tenant purges the rows -----------------------------
    store.delete_tenant(&t1).await.unwrap();
    assert!(a.inv_suppliers(true).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_supplier_price_list_is_an_upsert_and_never_reaches_another_tenant() {
    let store = common::test_store().await;
    let (a, _t1) = tenant_with_user(&store, "px-a").await;
    let (b, _t2) = tenant_with_user(&store, "px-b").await;

    let supplier = a.create_inv_supplier(&hoffmann()).await.unwrap();
    let chair = a
        .create_billing_product(&NewProduct {
            name: "Blue chair".to_owned(),
            unit: "piece".to_owned(),
            unit_price_cents: 4_900,
            vat_rate_bp: 2100,
            stocked: true,
            purchase_price_cents: 2_150,
            ..Default::default()
        })
        .await
        .unwrap();
    let desk = a
        .create_billing_product(&NewProduct {
            name: "Ash desk".to_owned(),
            stocked: true,
            ..Default::default()
        })
        .await
        .unwrap();

    // ---- the offer, and the upsert ---------------------------------------
    a.set_inv_supplier_price(
        &supplier,
        &chair,
        &NewSupplierPrice {
            supplier_code: " HM-4471 ".to_owned(),
            purchase_price_cents: 315,
            currency: "eur".to_owned(),
            min_order_qty_milli: 10_000,
            lead_time_days: Some(9),
        },
    )
    .await
    .unwrap();
    let offers = a.inv_supplier_prices(&supplier).await.unwrap();
    assert_eq!(offers.len(), 1);
    assert_eq!(offers[0].product_name, "Blue chair", "the join names it");
    assert_eq!(offers[0].supplier_code, "HM-4471");
    assert_eq!(offers[0].purchase_price_cents, 315, "cents stay integers");
    assert_eq!(offers[0].currency, "EUR");
    assert_eq!(offers[0].min_order_qty_milli, 10_000);
    // The offer's own lead time wins; without one the supplier's applies.
    assert_eq!(offers[0].effective_lead_time_days(14), 9);

    // The same pair written again REPLACES — one row, saying the new thing.
    a.set_inv_supplier_price(
        &supplier,
        &chair,
        &NewSupplierPrice {
            purchase_price_cents: 299,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let offers = a.inv_supplier_prices(&supplier).await.unwrap();
    assert_eq!(offers.len(), 1, "a re-quote replaces, never accumulates");
    assert_eq!(offers[0].purchase_price_cents, 299);
    assert_eq!(
        offers[0].supplier_code, "",
        "a full replace clears the code"
    );
    assert_eq!(
        offers[0].effective_lead_time_days(14),
        14,
        "and falls back to the supplier's lead time"
    );

    // Two products, ordered by name: "Ash desk" before "Blue chair".
    a.set_inv_supplier_price(&supplier, &desk, &NewSupplierPrice::default())
        .await
        .unwrap();
    let names: Vec<String> = a
        .inv_supplier_prices(&supplier)
        .await
        .unwrap()
        .into_iter()
        .map(|o| o.product_name)
        .collect();
    assert_eq!(names, vec!["Ash desk", "Blue chair"]);

    // The mirror read: who sells us this one.
    let sellers = a.inv_product_suppliers(&chair).await.unwrap();
    assert_eq!(sellers.len(), 1);
    assert_eq!(sellers[0].supplier_id, supplier);

    // ---- both ends are tenant-checked ------------------------------------
    let b_supplier = b.create_inv_supplier(&hoffmann()).await.unwrap();
    let b_chair = b
        .create_billing_product(&NewProduct {
            name: "Blue chair".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    // A's supplier, B's product: refused, and nothing written.
    assert_not_found(
        a.set_inv_supplier_price(&supplier, &b_chair, &NewSupplierPrice::default())
            .await,
    );
    // B's supplier, A's product, asked by A: the supplier is not A's.
    assert_not_found(
        a.set_inv_supplier_price(&b_supplier, &chair, &NewSupplierPrice::default())
            .await,
    );
    assert_eq!(
        a.inv_supplier_prices(&supplier).await.unwrap().len(),
        2,
        "a refused write left the list exactly as it was"
    );
    // B cannot read, write or remove anything of A's, on any path.
    assert_not_found(b.inv_supplier_prices(&supplier).await);
    assert_not_found(b.inv_product_suppliers(&chair).await);
    assert_not_found(b.remove_inv_supplier_price(&supplier, &chair).await);
    assert!(
        b.inv_supplier_prices(&b_supplier).await.unwrap().is_empty(),
        "B's own supplier sells nothing — A's offers are invisible"
    );

    // ---- the product's default supplier is the same gate -----------------
    a.update_billing_product(
        &chair,
        &NewProduct {
            name: "Blue chair".to_owned(),
            stocked: true,
            default_supplier_id: Some(supplier.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        a.billing_product(&chair)
            .await
            .unwrap()
            .unwrap()
            .default_supplier_id,
        Some(supplier.clone())
    );
    // Pointing at another tenant's supplier is the same NotFound an id that
    // never existed gets — on update and on create, leaving no half-written row.
    assert_not_found(
        a.update_billing_product(
            &chair,
            &NewProduct {
                name: "Blue chair".to_owned(),
                default_supplier_id: Some(b_supplier.clone()),
                ..Default::default()
            },
        )
        .await,
    );
    assert_eq!(
        a.billing_product(&chair)
            .await
            .unwrap()
            .unwrap()
            .default_supplier_id,
        Some(supplier.clone()),
        "the refused update changed nothing"
    );
    assert_not_found(
        a.create_billing_product(&NewProduct {
            name: "Smuggled".to_owned(),
            default_supplier_id: Some(b_supplier),
            ..Default::default()
        })
        .await,
    );
    assert!(
        !a.billing_products(true)
            .await
            .unwrap()
            .iter()
            .any(|p| p.name == "Smuggled"),
        "a refused create left no row behind"
    );

    // ---- removing an offer ------------------------------------------------
    a.remove_inv_supplier_price(&supplier, &chair)
        .await
        .unwrap();
    assert_eq!(a.inv_supplier_prices(&supplier).await.unwrap().len(), 1);
    assert_not_found(a.remove_inv_supplier_price(&supplier, &chair).await);

    // ---- archiving a supplier leaves the price list readable --------------
    a.set_inv_supplier_archived(&supplier, true).await.unwrap();
    assert_eq!(
        a.inv_supplier_prices(&supplier).await.unwrap().len(),
        1,
        "an archived supplier still explains the orders that name them"
    );
}
