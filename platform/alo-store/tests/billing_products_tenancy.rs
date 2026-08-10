//! Tenancy proof for the alo Billing price list (Law 1: isolation is tested,
//! not assumed). Products are tenant-wide — a co-tenant user sells from the
//! same list — but an outsider tenant gets the clean `NotFound`/empty on
//! **every** path: read, list, update, and archive. Also covers the CRUD arc
//! the queue item requires (create, read, update, list, archive), that money
//! survives the round trip exactly, and that a tenant deletion purges the
//! rows.
//!
//! The second test proves the catalog upgrade (alo Inventory, B5.02): the
//! codes a warehouse reads are unique **within** a tenant and never across
//! them, and a product photo must be a Drive node the caller can see.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    AccountStore, BillingProductId, DriveLocation, NewDriveFile, NewProduct, Store, StoreError,
    TenantId,
};

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error.
fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

/// A fully-specified product, so the round-trip assertions cover every column.
fn consulting() -> NewProduct {
    NewProduct {
        name: "Consulting".to_owned(),
        unit: "hour".to_owned(),
        unit_price_cents: 12_000,
        vat_rate_bp: 2100,
        ..Default::default()
    }
}

/// A tenant with one user, returning the account door plus the tenant id.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("prod-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@products.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

#[tokio::test]
async fn billing_products_round_trip_and_never_cross_tenant() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "a").await;
    // A co-tenant user of the same tenant: the price list is tenant-wide.
    let uc = store
        .for_tenant(t1.clone())
        .create_user("c@products.test")
        .await
        .unwrap();
    let c = store.for_account(t1.clone(), uc);
    let (b, t2) = tenant_with_user(&store, "b").await;

    // ---- create: normalised on the way in, money exact -------------------
    let id = a
        .create_billing_product(&NewProduct {
            name: "  Consulting  ".to_owned(),
            unit: " hour ".to_owned(),
            ..consulting()
        })
        .await
        .unwrap();
    let got = a.billing_product(&id).await.unwrap().unwrap();
    assert_eq!(got.name, "Consulting");
    assert_eq!(got.unit, "hour");
    assert_eq!(got.unit_price_cents, 12_000, "cents survive as integers");
    assert_eq!(got.vat_rate_bp, 2100);
    assert_eq!(got.created_by, a.user().as_str());
    assert!(!got.is_archived());

    // ---- list: tenant-wide, active only by default -----------------------
    assert_eq!(a.billing_products(false).await.unwrap().len(), 1);
    assert_eq!(
        c.billing_products(false).await.unwrap().len(),
        1,
        "a co-tenant user sees the same price list"
    );
    assert!(
        b.billing_products(true).await.unwrap().is_empty(),
        "another tenant sees nothing, archived included"
    );

    // ---- read/update/archive from another tenant: clean denial -----------
    assert!(b.billing_product(&id).await.unwrap().is_none());
    assert_not_found(
        b.update_billing_product(
            &id,
            &NewProduct {
                name: "Hijacked".to_owned(),
                unit_price_cents: 1,
                ..consulting()
            },
        )
        .await,
    );
    assert_not_found(b.set_billing_product_archived(&id, true).await);
    // ... and nothing they tried changed A's row.
    let after = a.billing_product(&id).await.unwrap().unwrap();
    assert_eq!(after.name, "Consulting");
    assert_eq!(after.unit_price_cents, 12_000);
    assert!(!after.is_archived());

    // An id that never existed is the same answer as another tenant's id —
    // no existence oracle.
    let ghost = BillingProductId::generate();
    assert!(a.billing_product(&ghost).await.unwrap().is_none());
    assert_not_found(a.update_billing_product(&ghost, &consulting()).await);
    assert_not_found(a.set_billing_product_archived(&ghost, true).await);

    // ---- update: full replace, by a co-tenant user -----------------------
    c.update_billing_product(
        &id,
        &NewProduct {
            name: "Senior consulting".to_owned(),
            unit: String::new(),
            unit_price_cents: 0,
            vat_rate_bp: 0,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let edited = a.billing_product(&id).await.unwrap().unwrap();
    assert_eq!(edited.name, "Senior consulting");
    assert_eq!(edited.unit, "", "a unitless item is legitimate");
    assert_eq!(edited.unit_price_cents, 0, "so is a free one");
    assert_eq!(edited.vat_rate_bp, 0, "and a zero-rated one");
    assert!(edited.updated_at >= edited.created_at);

    // ---- validation guards the write paths -------------------------------
    let invalid = [
        NewProduct {
            name: "  ".to_owned(),
            ..consulting()
        },
        NewProduct {
            name: "x".repeat(alo_store::billing_products::PRODUCT_NAME_MAX_CHARS + 1),
            ..consulting()
        },
        NewProduct {
            unit: "x".repeat(alo_store::billing_products::PRODUCT_UNIT_MAX_CHARS + 1),
            ..consulting()
        },
        NewProduct {
            unit_price_cents: -1,
            ..consulting()
        },
        NewProduct {
            unit_price_cents: alo_store::billing_field::UNIT_PRICE_MAX_CENTS + 1,
            ..consulting()
        },
        NewProduct {
            vat_rate_bp: -1,
            ..consulting()
        },
        NewProduct {
            vat_rate_bp: alo_store::billing_field::VAT_RATE_MAX_BP + 1,
            ..consulting()
        },
    ];
    for bad in &invalid {
        match a.create_billing_product(bad).await {
            Err(StoreError::Validation(_)) => {}
            other => panic!("expected Validation for {bad:?}, got {other:?}"),
        }
        match a.update_billing_product(&id, bad).await {
            Err(StoreError::Validation(_)) => {}
            other => panic!("expected Validation for {bad:?}, got {other:?}"),
        }
    }
    // A rejected write left the record and the list untouched.
    assert_eq!(
        a.billing_product(&id).await.unwrap().unwrap().name,
        "Senior consulting"
    );
    assert_eq!(a.billing_products(true).await.unwrap().len(), 1);

    // The ceiling itself is accepted — the bound is inclusive, so a real (if
    // improbable) price at the limit is sellable rather than mysteriously
    // refused.
    let dear = a
        .create_billing_product(&NewProduct {
            name: "Turnkey plant".to_owned(),
            unit_price_cents: alo_store::billing_field::UNIT_PRICE_MAX_CENTS,
            vat_rate_bp: alo_store::billing_field::VAT_RATE_MAX_BP,
            ..consulting()
        })
        .await
        .unwrap();
    let dear_row = a.billing_product(&dear).await.unwrap().unwrap();
    assert_eq!(
        dear_row.unit_price_cents,
        alo_store::billing_field::UNIT_PRICE_MAX_CENTS
    );

    // ---- archive: hidden from the default list, never deleted ------------
    a.set_billing_product_archived(&id, true).await.unwrap();
    let active_names: Vec<String> = a
        .billing_products(false)
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(active_names, vec!["Turnkey plant".to_owned()]);
    let archived = a.billing_product(&id).await.unwrap().unwrap();
    assert!(
        archived.is_archived(),
        "still readable, so an old document can be explained"
    );
    let archived_at = archived.archived_at;
    // Idempotent: re-archiving keeps the original time.
    a.set_billing_product_archived(&id, true).await.unwrap();
    assert_eq!(
        a.billing_product(&id).await.unwrap().unwrap().archived_at,
        archived_at
    );
    // Archived rows sort after active ones in the include-archived list.
    let listed = a.billing_products(true).await.unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].id, dear, "active before archived");
    assert_eq!(listed[1].id, id);
    // Restore.
    a.set_billing_product_archived(&id, false).await.unwrap();
    assert!(!a.billing_product(&id).await.unwrap().unwrap().is_archived());
    // ... and now the list is in name order: "Senior consulting" < "Turnkey".
    let restored = a.billing_products(false).await.unwrap();
    assert_eq!(restored[0].id, id);
    assert_eq!(restored[1].id, dear);

    // ---- deleting the tenant purges its price list -----------------------
    // Read the rows directly: the claim is that they were cascaded away, not
    // merely hidden behind the tenant predicate of the list call.
    store.delete_tenant(&t1).await.unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap();
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM billing_products WHERE tenant_id = $1")
            .bind(t1.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 0, "the tenant's products are purged with it");
    store.delete_tenant(&t2).await.unwrap();
}

/// The catalog upgrade (alo Inventory, B5.02): the five facts a warehouse needs
/// on the same rows, and the three rules that arrive with them — uniqueness
/// **within** the tenant, a check-digit-validated barcode, and a photo the
/// caller can actually see.
///
/// The uniqueness proof is the one this module could most easily get wrong:
/// a global index would fail tenant B's insert because tenant A already sells
/// the same book, which is both an information leak and wrong on the facts.
#[tokio::test]
async fn product_catalog_is_unique_within_a_tenant_and_never_across_them() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "cat-a").await;
    let (b, t2) = tenant_with_user(&store, "cat-b").await;

    let chair = |sku: &str, barcode: &str| NewProduct {
        name: "Blue chair".to_owned(),
        unit: "piece".to_owned(),
        unit_price_cents: 4_900,
        vat_rate_bp: 2100,
        sku: sku.to_owned(),
        barcode: barcode.to_owned(),
        stocked: true,
        purchase_price_cents: 2_150,
        photo_node_id: None,
    };

    // ---- round trip: every new column survives, separators do not ---------
    let photo = a
        .drive_create_file(
            &DriveLocation::Personal,
            None,
            &NewDriveFile {
                name: "blue-chair.jpg".to_owned(),
                blob_id: "product-photo-a".to_owned(),
                content_type: Some("image/jpeg".to_owned()),
                ..NewDriveFile::default()
            },
        )
        .await
        .unwrap();
    let id = a
        .create_billing_product(&NewProduct {
            sku: "  CH-BLUE-01 ".to_owned(),
            barcode: " 400-638 133 393 1 ".to_owned(),
            photo_node_id: Some(photo.clone()),
            ..chair("", "")
        })
        .await
        .unwrap();
    let got = a.billing_product(&id).await.unwrap().unwrap();
    assert_eq!(got.sku, "CH-BLUE-01", "stored trimmed");
    assert_eq!(got.barcode, "4006381333931", "separators are presentation");
    assert!(got.stocked);
    assert_eq!(got.purchase_price_cents, 2_150);
    assert_eq!(
        got.unit_price_cents, 4_900,
        "what we charge is not what we pay"
    );
    assert_eq!(got.photo_node_id.as_ref(), Some(&photo));

    // ---- the scan read: one product or nothing, never another tenant's ----
    let scanned = a
        .billing_product_by_barcode(" 4006381333931 ")
        .await
        .unwrap()
        .expect("the code off the box finds the chair");
    assert_eq!(scanned.id, id);
    assert!(
        b.billing_product_by_barcode("4006381333931")
            .await
            .unwrap()
            .is_none(),
        "the same GTIN is somebody else's stock, and invisible"
    );
    assert!(
        a.billing_product_by_barcode("4006381333930")
            .await
            .unwrap()
            .is_none(),
        "a bad scan found nothing — not an error"
    );
    assert!(a.billing_product_by_barcode("").await.unwrap().is_none());

    // ---- wrong uniqueness: tenant B may carry the very same codes ---------
    let b_id = b
        .create_billing_product(&chair("CH-BLUE-01", "4006381333931"))
        .await
        .unwrap();
    assert_eq!(
        b.billing_product(&b_id).await.unwrap().unwrap().sku,
        "CH-BLUE-01",
        "two businesses legitimately stock the same GTIN"
    );

    // ---- ... but not twice inside one tenant -----------------------------
    match a.create_billing_product(&chair("CH-BLUE-01", "")).await {
        Err(StoreError::Conflict(msg)) => assert!(msg.contains("SKU"), "unhelpful: {msg}"),
        other => panic!("expected a Conflict on the SKU, got {other:?}"),
    }
    match a.create_billing_product(&chair("", "4006381333931")).await {
        Err(StoreError::Conflict(msg)) => assert!(msg.contains("barcode"), "unhelpful: {msg}"),
        other => panic!("expected a Conflict on the barcode, got {other:?}"),
    }
    // The same refusal on the update path, so neither door is the loose one.
    let second = a
        .create_billing_product(&chair("CH-RED-01", "5901234123457"))
        .await
        .unwrap();
    match a
        .update_billing_product(&second, &chair("CH-BLUE-01", "5901234123457"))
        .await
    {
        Err(StoreError::Conflict(msg)) => assert!(msg.contains("SKU"), "unhelpful: {msg}"),
        other => panic!("expected a Conflict on the SKU, got {other:?}"),
    }
    // A rejected write changed nothing.
    assert_eq!(
        a.billing_product(&second).await.unwrap().unwrap().sku,
        "CH-RED-01"
    );

    // ---- blank is "not stated", and repeats freely ------------------------
    for name in ["Consulting", "Training", "Support"] {
        a.create_billing_product(&NewProduct {
            name: name.to_owned(),
            ..consulting()
        })
        .await
        .unwrap();
    }
    assert_eq!(
        a.billing_products(false).await.unwrap().len(),
        5,
        "a services business has no SKU on anything, and that is not a collision"
    );

    // ---- validation: the check digit is the door -------------------------
    for bad in ["4006381333930", "12345", "40063813339A1"] {
        match a.create_billing_product(&chair("", bad)).await {
            Err(StoreError::Validation(msg)) => {
                assert!(msg.contains("barcode"), "unhelpful: {msg}");
                assert!(!msg.contains(bad), "the message carried the code: {msg}");
            }
            other => panic!("expected Validation for {bad}, got {other:?}"),
        }
    }
    let long_sku = "x".repeat(alo_store::billing_products::PRODUCT_SKU_MAX_CHARS + 1);
    match a.create_billing_product(&chair(&long_sku, "")).await {
        Err(StoreError::Validation(msg)) => assert!(msg.contains("SKU"), "unhelpful: {msg}"),
        other => panic!("expected Validation, got {other:?}"),
    }
    match a
        .create_billing_product(&NewProduct {
            purchase_price_cents: -1,
            ..chair("", "")
        })
        .await
    {
        Err(StoreError::Validation(msg)) => {
            assert!(msg.contains("purchase price"), "unhelpful: {msg}");
        }
        other => panic!("expected Validation, got {other:?}"),
    }

    // ---- the photo is gated on the caller being able to SEE the node ------
    // Tenant B pointing at tenant A's photo gets the same answer as a node
    // that never existed: no existence oracle, no borrowed picture.
    assert_not_found(
        b.create_billing_product(&NewProduct {
            photo_node_id: Some(photo.clone()),
            ..chair("CH-GREEN-01", "")
        })
        .await,
    );
    assert_not_found(
        a.create_billing_product(&NewProduct {
            photo_node_id: Some(alo_store::DriveNodeId::new("no-such-node".to_owned())),
            ..chair("CH-GREEN-01", "")
        })
        .await,
    );
    assert_not_found(
        b.update_billing_product(
            &b_id,
            &NewProduct {
                photo_node_id: Some(photo.clone()),
                ..chair("CH-BLUE-01", "4006381333931")
            },
        )
        .await,
    );
    // ... and a refused photo left no half-written product behind.
    assert_eq!(b.billing_products(true).await.unwrap().len(), 1);

    // A photo can be taken off again.
    a.update_billing_product(
        &id,
        &NewProduct {
            photo_node_id: None,
            ..chair("CH-BLUE-01", "4006381333931")
        },
    )
    .await
    .unwrap();
    assert!(
        a.billing_product(&id)
            .await
            .unwrap()
            .unwrap()
            .photo_node_id
            .is_none()
    );

    store.delete_tenant(&t1).await.unwrap();
    store.delete_tenant(&t2).await.unwrap();
}
