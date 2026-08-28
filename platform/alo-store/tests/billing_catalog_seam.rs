//! Billing's public catalog seam against a real database (ADR 0041): what a
//! site's shop may read from a tenant's price list — the active items, their
//! sale prices and the accounting currency — and everything it may not: the
//! archived past, the tenant's costs and codes, and — Law 1 — any other
//! tenant's catalog.
//!
//! The vocabulary proof (the exhaustive destructure of the sale item) lives as
//! a unit test inside `billing_catalog_read`; this suite proves the seam on
//! real rows.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, BillingCatalogRead, BillingProductId, NewBillingSettings, NewProduct,
};
use sqlx::postgres::PgPoolOptions;

/// A dated product the way wave one sells one: a service with a price and a
/// VAT rate, nothing stocked, nothing shipped.
fn workshop() -> NewProduct {
    NewProduct {
        name: "Antwerp letterpress workshop".to_owned(),
        unit: "seat".to_owned(),
        unit_price_cents: 8_500,
        vat_rate_bp: 2100,
        ..Default::default()
    }
}

/// A stock item carrying every private fact the seam must withhold.
fn book() -> NewProduct {
    NewProduct {
        name: "Field guide (paperback)".to_owned(),
        unit: "piece".to_owned(),
        unit_price_cents: 2_400,
        vat_rate_bp: 600,
        sku: "BK-GUIDE-01".to_owned(),
        barcode: "4006381333931".to_owned(),
        stocked: true,
        purchase_price_cents: 1_100,
        photo_node_id: None,
        default_supplier_id: None,
    }
}

/// A tenant, its one user's account door, and the catalog door a site's shop
/// would open for that owner.
struct Owned {
    blobs: alo_store::BlobStore,
    account: AccountStore,
    pool: sqlx::PgPool,
}

async fn owned(tag: &str) -> Owned {
    let (store, blobs) = common::test_store_with_blobs().await;
    let tenant = store
        .create_tenant(&format!("catalog-seam-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("owner@{tag}.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&common::database_url())
        .await
        .unwrap();
    Owned {
        blobs,
        account,
        pool,
    }
}

impl Owned {
    fn door(&self) -> BillingCatalogRead {
        BillingCatalogRead::open(
            self.pool.clone(),
            self.blobs.clone(),
            self.account.tenant().clone(),
            self.account.user().clone(),
        )
    }
}

#[tokio::test]
async fn the_active_price_list_crosses_with_only_the_buyers_facts() {
    let owned = owned("list").await;
    let workshop_id = owned
        .account
        .create_billing_product(&workshop())
        .await
        .unwrap();
    let book_id = owned.account.create_billing_product(&book()).await.unwrap();
    let retired = owned
        .account
        .create_billing_product(&NewProduct {
            name: "Discontinued mug".to_owned(),
            unit_price_cents: 900,
            ..Default::default()
        })
        .await
        .unwrap();
    owned
        .account
        .set_billing_product_archived(&retired, true)
        .await
        .unwrap();

    // The active list, in name order — Billing's own picker order — and the
    // archived item simply is not there.
    let items = owned.door().sale_items().await.unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].id.as_str(), workshop_id.as_str());
    assert_eq!(items[0].name, "Antwerp letterpress workshop");
    assert_eq!(items[0].unit, "seat");
    assert_eq!(items[0].unit_price_cents, 8_500);
    assert_eq!(items[0].vat_rate_bp, 2100);
    assert_eq!(items[1].id.as_str(), book_id.as_str());
    assert_eq!(items[1].unit_price_cents, 2_400);
    assert_eq!(items[1].vat_rate_bp, 600);

    // The book's private facts never cross: nothing the seam answers renders
    // the cost, the SKU or the barcode anywhere.
    let rendered = format!("{items:?}");
    assert!(!rendered.contains("1100"), "cost leaked: {rendered}");
    assert!(!rendered.contains("BK-GUIDE-01"), "SKU leaked: {rendered}");
    assert!(
        !rendered.contains("4006381333931"),
        "barcode leaked: {rendered}"
    );
    assert!(
        !rendered.contains("Discontinued"),
        "the archived past leaked: {rendered}"
    );
}

#[tokio::test]
async fn one_item_answers_while_active_and_never_after() {
    let owned = owned("one").await;
    let id = owned
        .account
        .create_billing_product(&workshop())
        .await
        .unwrap();

    let item = owned.door().sale_item(&id).await.unwrap().unwrap();
    assert_eq!(item.name, "Antwerp letterpress workshop");
    assert_eq!(item.unit_price_cents, 8_500);

    // A price edit is answered live at the next read — the reference is the
    // storage, so there is no second copy to go stale.
    owned
        .account
        .update_billing_product(
            &id,
            &NewProduct {
                unit_price_cents: 9_000,
                ..workshop()
            },
        )
        .await
        .unwrap();
    let item = owned.door().sale_item(&id).await.unwrap().unwrap();
    assert_eq!(item.unit_price_cents, 9_000);

    // Archived: the same reference now shows nothing — a shop cannot sell
    // the past.
    owned
        .account
        .set_billing_product_archived(&id, true)
        .await
        .unwrap();
    assert!(owned.door().sale_item(&id).await.unwrap().is_none());
    assert!(owned.door().sale_items().await.unwrap().is_empty());

    // An id that was never anything is the same answer.
    let ghost = BillingProductId::new("prod-never-existed".to_owned());
    assert!(owned.door().sale_item(&ghost).await.unwrap().is_none());
}

#[tokio::test]
async fn no_pair_of_tenants_can_reach_each_other_through_the_door() {
    let a = owned("tenant-a").await;
    let b = owned("tenant-b").await;
    let a_item = a.account.create_billing_product(&workshop()).await.unwrap();
    b.account.create_billing_product(&book()).await.unwrap();

    // Each door lists its own tenant's catalog and nothing of the other's.
    let a_items = a.door().sale_items().await.unwrap();
    assert_eq!(a_items.len(), 1);
    assert_eq!(a_items[0].name, "Antwerp letterpress workshop");
    let b_items = b.door().sale_items().await.unwrap();
    assert_eq!(b_items.len(), 1);
    assert_eq!(b_items[0].name, "Field guide (paperback)");

    // Tenant B asking for A's id gets the same answer as for an id that never
    // existed — indistinguishable by design.
    assert!(b.door().sale_item(&a_item).await.unwrap().is_none());
}

#[tokio::test]
async fn the_currency_is_billings_own_and_never_a_second_copy() {
    let owned = owned("currency").await;

    // A tenant that has never opened Billing's settings still keeps books in
    // something — the seam answers the same default Billing's documents use.
    assert_eq!(owned.door().currency().await.unwrap(), "EUR");

    // The tenant states a currency once, in Billing; the seam answers it at
    // the next read, because there is no copy to update.
    owned
        .account
        .save_billing_settings(&NewBillingSettings {
            legal_name: "Studio BV".to_owned(),
            base_currency: "usd".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(owned.door().currency().await.unwrap(), "USD");
}
