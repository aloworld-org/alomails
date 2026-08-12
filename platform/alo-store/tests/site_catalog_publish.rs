//! Publishing freezes a catalog, hidden items never reach the public copy, and
//! every read of that copy stays tenant-safe.

#![allow(clippy::unwrap_used)]

mod common;

use alo_store::{
    SiteCatalogAvailability, SiteCatalogCategoryInput, SiteCatalogInput, SiteCatalogItemInput,
    SiteId, SitePublicStore, StoreError,
};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;

fn subdomain(tag: &str) -> String {
    format!(
        "{tag}-{}",
        SiteId::generate()
            .as_str()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(12)
            .collect::<String>()
            .to_ascii_lowercase()
    )
}

fn item<'a>(
    name: &'a str,
    slug: &'a str,
    price: Option<i64>,
    availability: SiteCatalogAvailability,
) -> SiteCatalogItemInput<'a> {
    SiteCatalogItemInput {
        category: None,
        name,
        slug,
        description: None,
        price_cents: price,
        price_note: None,
        image: None,
        availability,
        position: 0,
    }
}

#[tokio::test]
async fn publishing_freezes_the_catalog_and_hidden_items_never_leave_the_editor() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let tenant = store.create_tenant("site-catalog-publish").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("publisher@site-catalog.test")
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let site_subdomain = subdomain("catalog-publish");
    let site = account
        .create_site("Harbour", &site_subdomain)
        .await
        .unwrap();
    let home = account
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    let catalog = account
        .create_site_catalog(
            &site,
            &SiteCatalogInput {
                name: "Harbour menu",
                currency: "EUR",
                orders_enabled: false,
            },
        )
        .await
        .unwrap();
    let brews = account
        .create_site_catalog_category(
            &site,
            &catalog,
            &SiteCatalogCategoryInput {
                name: "Brews",
                slug: "brews",
                position: 0,
            },
        )
        .await
        .unwrap();
    let mut filter = item(
        "Filter brew",
        "filter",
        Some(350),
        SiteCatalogAvailability::Available,
    );
    filter.category = Some(&brews);
    account
        .create_site_catalog_item(&site, &catalog, &filter)
        .await
        .unwrap();
    account
        .create_site_catalog_item(
            &site,
            &catalog,
            &item(
                "Cold brew",
                "cold-brew",
                Some(450),
                SiteCatalogAvailability::SoldOut,
            ),
        )
        .await
        .unwrap();
    let secret = account
        .create_site_catalog_item(
            &site,
            &catalog,
            &item(
                "Staff blend",
                "staff-blend",
                Some(1),
                SiteCatalogAvailability::Hidden,
            ),
        )
        .await
        .unwrap();
    account
        .set_page_sections(
            &site,
            &home,
            json!({
                "schema_version": 1,
                "sections": [{
                    "type": "catalog",
                    "catalog_id": catalog.as_str(),
                    "heading": "On the counter"
                }]
            }),
        )
        .await
        .unwrap();

    // The draft preview already answers with what a publish would freeze.
    let preview = account.site_catalog_preview(&site, &catalog).await.unwrap();
    assert_eq!(preview.items.len(), 2, "the hidden item is absent already");

    let first_publish = account.publish_site(&site).await.unwrap();
    let frozen = account
        .site_publish_catalog_snapshots(&site, &first_publish)
        .await
        .unwrap();
    assert_eq!(frozen.len(), 1);
    let snapshot = &frozen[0];
    assert_eq!(snapshot.currency, "EUR");
    assert_eq!(snapshot.categories.len(), 1);
    assert_eq!(snapshot.categories[0].slug, "brews");
    assert_eq!(snapshot.items.len(), 2, "hidden items are never frozen");
    assert!(
        !snapshot
            .items
            .iter()
            .any(|item| item.name.contains("Staff blend")),
        "a hidden item reached the published copy: {snapshot:?}"
    );
    assert_eq!(snapshot.items[0].price_cents, Some(350));
    assert_eq!(snapshot.items[0].category.as_deref(), Some("brews"));
    assert!(snapshot.items[1].sold_out, "sold out survives the freeze");

    // Editing after publishing changes nothing that is already public.
    account
        .update_site_catalog_item(
            &site,
            &catalog,
            &secret,
            &item(
                "Staff blend",
                "staff-blend",
                Some(1),
                SiteCatalogAvailability::Available,
            ),
        )
        .await
        .unwrap();
    let still = account
        .site_publish_catalog_snapshots(&site, &first_publish)
        .await
        .unwrap();
    assert_eq!(still[0].items.len(), 2, "the publish is immutable");

    // The public service reads exactly the same copy, resolved from a Host.
    let public_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&common::database_url())
        .await
        .unwrap();
    let public = SitePublicStore::new(public_pool, blobs);
    let resolved = public
        .resolve_published(&site_subdomain)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(public.published_catalogs(&resolved).await.unwrap(), still);

    // Publishing again picks the change up — and only then.
    let second_publish = account.publish_site(&site).await.unwrap();
    let second = account
        .site_publish_catalog_snapshots(&site, &second_publish)
        .await
        .unwrap();
    assert_eq!(second[0].items.len(), 3);

    let other_tenant = store
        .create_tenant("site-catalog-publish-rival")
        .await
        .unwrap();
    let other_user = store
        .for_tenant(other_tenant.clone())
        .create_user("rival@site-catalog-publish.test")
        .await
        .unwrap();
    let rival = store.for_account(other_tenant, other_user);
    assert!(
        rival
            .site_publish_catalog_snapshots(&site, &first_publish)
            .await
            .unwrap()
            .is_empty(),
        "a foreign tenant must read no snapshot at all"
    );
}

#[tokio::test]
async fn a_page_pointing_at_a_deleted_catalog_refuses_to_publish() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("site-catalog-dangling").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("dangling@site-catalog.test")
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let site = account
        .create_site("Harbour", &subdomain("catalog-dangling"))
        .await
        .unwrap();
    let home = account
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    let catalog = account
        .create_site_catalog(
            &site,
            &SiteCatalogInput {
                name: "Menu",
                currency: "EUR",
                orders_enabled: false,
            },
        )
        .await
        .unwrap();
    account
        .create_site_catalog_item(
            &site,
            &catalog,
            &item(
                "Filter",
                "filter",
                Some(300),
                SiteCatalogAvailability::Available,
            ),
        )
        .await
        .unwrap();
    account
        .set_page_sections(
            &site,
            &home,
            json!({
                "schema_version": 1,
                "sections": [{"type": "catalog", "catalog_id": catalog.as_str()}]
            }),
        )
        .await
        .unwrap();
    let live = account.publish_site(&site).await.unwrap();

    account.delete_site_catalog(&site, &catalog).await.unwrap();
    match account.publish_site(&site).await {
        Err(StoreError::Conflict(detail)) => {
            assert!(detail.contains("catalog"), "{detail}");
        }
        other => panic!("expected a refusal to publish, got {other:?}"),
    }
    // The refusal is total: the live publish still serves what it always did.
    let frozen = account
        .site_publish_catalog_snapshots(&site, &live)
        .await
        .unwrap();
    assert_eq!(frozen.len(), 1);
    assert_eq!(frozen[0].items.len(), 1);
    assert_eq!(
        account.site_publish_history(&site, 10).await.unwrap().len(),
        1,
        "a refused publish must leave no half-written publish behind"
    );
}
