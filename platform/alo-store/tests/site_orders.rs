//! Order forms on published catalogs: the tenant boundary around an order, the
//! rules of the anonymous door that writes one, and the at-most-once owner
//! notification it produces.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, BlobStore, OrderContact, OrderRequestLine, SiteCatalogAvailability,
    SiteCatalogId, SiteCatalogInput, SiteCatalogItemInput, SiteId, SiteOrderId, SiteOrderStatus,
    SitePublicStore, Store, StoreError, normalize_order_contact,
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
        image_alt: None,
        availability,
        position: 0,
    }
}

fn contact() -> OrderContact {
    normalize_order_contact(
        "Ada Lovelace",
        "ada@example.test",
        "+32 2 555 01",
        "no nuts",
    )
    .unwrap()
}

fn asked(slug: &str, quantity: i32) -> OrderRequestLine {
    OrderRequestLine {
        item_slug: slug.to_owned(),
        quantity,
    }
}

async fn public_store(blobs: BlobStore) -> SitePublicStore {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&common::database_url())
        .await
        .unwrap();
    SitePublicStore::new(pool, blobs)
}

/// A live bakery site: one page showing one orderable catalog with a priced
/// item, an unpriced item, a sold-out item and a hidden one.
async fn orderable_site(account: &AccountStore, tag: &str) -> (SiteId, SiteCatalogId, String) {
    let sub = subdomain(tag);
    let site = account.create_site("Bakery", &sub).await.unwrap();
    let home = account
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    let catalog = account
        .create_site_catalog(
            &site,
            &SiteCatalogInput {
                name: "Saturday bake",
                currency: "EUR",
                orders_enabled: true,
            },
        )
        .await
        .unwrap();
    for input in [
        item(
            "Sourdough",
            "sourdough",
            Some(450),
            SiteCatalogAvailability::Available,
        ),
        item(
            "Wedding cake",
            "wedding-cake",
            None,
            SiteCatalogAvailability::Available,
        ),
        item(
            "Focaccia",
            "focaccia",
            Some(600),
            SiteCatalogAvailability::SoldOut,
        ),
        item(
            "Staff loaf",
            "staff-loaf",
            Some(1),
            SiteCatalogAvailability::Hidden,
        ),
    ] {
        account
            .create_site_catalog_item(&site, &catalog, &input)
            .await
            .unwrap();
    }
    account
        .set_page_sections(
            &site,
            &home,
            json!({
                "schema_version": 1,
                "sections": [{
                    "type": "catalog",
                    "catalog_id": catalog.as_str(),
                    "heading": "Order for Saturday"
                }]
            }),
        )
        .await
        .unwrap();
    account.publish_site(&site).await.unwrap();
    (site, catalog, sub)
}

async fn fresh_account(store: &Store, tag: &str) -> AccountStore {
    let tenant = store.create_tenant(&format!("t-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@orders.test"))
        .await
        .unwrap();
    store.for_account(tenant, user)
}

/// An order belongs to the tenant whose catalog it came from, and to nobody
/// else: a rival tenant cannot list it, read it, read its lines, move it
/// through the workflow, or delete it.
#[tokio::test]
async fn a_foreign_tenant_can_neither_read_nor_write_an_order() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let owner = fresh_account(&store, "order-owner").await;
    let rival = fresh_account(&store, "order-rival").await;
    let (site, catalog, _) = orderable_site(&owner, "order-owner").await;
    let public = public_store(blobs).await;

    let placed = public
        .place_public_order(
            catalog.as_str(),
            &contact(),
            &[asked("sourdough", 3), asked("wedding-cake", 1)],
        )
        .await
        .unwrap()
        .expect("a live orderable catalog accepts the order");

    // The owner sees the order exactly as it was placed, priced from the
    // published snapshot.
    let orders = owner.site_orders(&site).await.unwrap();
    assert_eq!(orders.len(), 1);
    let order = &orders[0];
    assert_eq!(order.id.as_str(), placed.as_str());
    assert_eq!(order.customer_name, "Ada Lovelace");
    assert_eq!(order.customer_email, "ada@example.test");
    assert_eq!(order.customer_phone.as_deref(), Some("+32 2 555 01"));
    assert_eq!(order.note.as_deref(), Some("no nuts"));
    assert_eq!(order.catalog_name, "Saturday bake");
    assert_eq!(order.currency, "EUR");
    assert_eq!(
        order.total_cents, 1_350,
        "3 x 450, the unpriced line adds 0"
    );
    assert_eq!(order.status, SiteOrderStatus::New);
    let lines = owner.site_order_lines(&site, &placed).await.unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].item_slug, "sourdough");
    assert_eq!(lines[0].item_name, "Sourdough");
    assert_eq!(lines[0].quantity, 3);
    assert_eq!(lines[0].unit_price_cents, Some(450));
    assert_eq!(lines[0].line_total_cents, Some(1_350));
    assert_eq!(lines[1].item_slug, "wedding-cake");
    assert_eq!(lines[1].unit_price_cents, None, "quoted by hand");
    assert_eq!(lines[1].line_total_cents, None);

    // The rival's door is blind to all of it, through every address.
    assert!(rival.site_orders(&site).await.unwrap().is_empty());
    assert!(rival.site_order(&site, &placed).await.unwrap().is_none());
    assert!(
        rival
            .site_order_lines(&site, &placed)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        rival
            .set_site_order_status(&site, &placed, SiteOrderStatus::Cancelled)
            .await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        rival.delete_site_order(&site, &placed).await,
        Err(StoreError::NotFound)
    ));
    // …and the rival's failed writes left no mark.
    assert_eq!(
        owner
            .site_order(&site, &placed)
            .await
            .unwrap()
            .unwrap()
            .status,
        SiteOrderStatus::New
    );

    // The owner's own workflow moves in both directions, and deleting takes
    // the lines with it.
    owner
        .set_site_order_status(&site, &placed, SiteOrderStatus::Confirmed)
        .await
        .unwrap();
    assert_eq!(
        owner
            .site_order(&site, &placed)
            .await
            .unwrap()
            .unwrap()
            .status,
        SiteOrderStatus::Confirmed
    );
    owner
        .set_site_order_status(&site, &placed, SiteOrderStatus::Cancelled)
        .await
        .unwrap();
    owner.delete_site_order(&site, &placed).await.unwrap();
    assert!(owner.site_orders(&site).await.unwrap().is_empty());
    assert!(
        owner
            .site_order_lines(&site, &placed)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        owner.delete_site_order(&site, &placed).await,
        Err(StoreError::NotFound)
    ));
}

/// The anonymous door opens for exactly one thing: a catalog in the current
/// publish of a live site, published with ordering switched on. Everything
/// else — unknown id, draft site, ordering off, an unpublished toggle — is the
/// same silent `None`, which the public wire turns into one uniform 404.
#[tokio::test]
async fn only_a_live_catalog_published_with_ordering_on_takes_orders() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let owner = fresh_account(&store, "order-door").await;
    let public = public_store(blobs).await;

    // A draft site's catalog: real, orderable, never published.
    let draft = owner
        .create_site("Draft", &subdomain("order-draft"))
        .await
        .unwrap();
    let draft_catalog = owner
        .create_site_catalog(
            &draft,
            &SiteCatalogInput {
                name: "Nothing yet",
                currency: "EUR",
                orders_enabled: true,
            },
        )
        .await
        .unwrap();
    owner
        .create_site_catalog_item(
            &draft,
            &draft_catalog,
            &item(
                "Sourdough",
                "sourdough",
                Some(450),
                SiteCatalogAvailability::Available,
            ),
        )
        .await
        .unwrap();

    for unreachable in [
        SiteCatalogId::generate().as_str().to_owned(),
        draft_catalog.as_str().to_owned(),
        "not a valid id at all".to_owned(),
        String::new(),
    ] {
        assert!(
            public
                .place_public_order(&unreachable, &contact(), &[asked("sourdough", 1)])
                .await
                .unwrap()
                .is_none(),
            "{unreachable} must not be orderable"
        );
    }

    // A live catalog published with ordering OFF is equally closed…
    let (site, catalog, _) = orderable_site(&owner, "order-door").await;
    owner
        .update_site_catalog(
            &site,
            &catalog,
            &SiteCatalogInput {
                name: "Saturday bake",
                currency: "EUR",
                orders_enabled: false,
            },
        )
        .await
        .unwrap();
    assert!(
        public
            .place_public_order(catalog.as_str(), &contact(), &[asked("sourdough", 1)])
            .await
            .unwrap()
            .is_some(),
        "the toggle is frozen: the live publish still offers ordering"
    );
    owner.publish_site(&site).await.unwrap();
    assert!(
        public
            .place_public_order(catalog.as_str(), &contact(), &[asked("sourdough", 1)])
            .await
            .unwrap()
            .is_none(),
        "after republishing, the door is closed"
    );

    // …and so is a site that has been taken down entirely.
    owner
        .update_site_catalog(
            &site,
            &catalog,
            &SiteCatalogInput {
                name: "Saturday bake",
                currency: "EUR",
                orders_enabled: true,
            },
        )
        .await
        .unwrap();
    owner.publish_site(&site).await.unwrap();
    owner.unpublish_site(&site).await.unwrap();
    assert!(
        public
            .place_public_order(catalog.as_str(), &contact(), &[asked("sourdough", 1)])
            .await
            .unwrap()
            .is_none(),
        "an unpublished site takes no orders"
    );
}

/// What an order costs is read from the publish the visitor is looking at, not
/// from the editor and never from the request. Items that are not on that
/// published page cannot be ordered at all.
#[tokio::test]
async fn prices_come_from_the_publish_and_absent_items_are_refused() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let owner = fresh_account(&store, "order-price").await;
    let (site, catalog, _) = orderable_site(&owner, "order-price").await;
    let public = public_store(blobs).await;

    // A sold-out item, a hidden item and an item that never existed are all
    // refused with a sentence a visitor can act on — and write nothing.
    for slug in ["focaccia", "staff-loaf", "brioche"] {
        let refused = public
            .place_public_order(catalog.as_str(), &contact(), &[asked(slug, 1)])
            .await;
        assert!(
            matches!(refused, Err(StoreError::Validation(_))),
            "{slug} must be refused, got {refused:?}"
        );
    }
    assert!(owner.site_orders(&site).await.unwrap().is_empty());

    // The editor raises the price; the published page — and therefore the
    // order — still says 4.50 until the tenant publishes again.
    owner
        .update_site_catalog_item(
            &site,
            &catalog,
            &owner
                .site_catalog_items(&site, &catalog)
                .await
                .unwrap()
                .into_iter()
                .find(|row| row.slug == "sourdough")
                .unwrap()
                .id,
            &item(
                "Sourdough",
                "sourdough",
                Some(500),
                SiteCatalogAvailability::Available,
            ),
        )
        .await
        .unwrap();
    let before = public
        .place_public_order(catalog.as_str(), &contact(), &[asked("sourdough", 2)])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        owner
            .site_order(&site, &before)
            .await
            .unwrap()
            .unwrap()
            .total_cents,
        900,
        "the published price, not the edited one"
    );

    owner.publish_site(&site).await.unwrap();
    let after = public
        .place_public_order(catalog.as_str(), &contact(), &[asked("sourdough", 2)])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        owner
            .site_order(&site, &after)
            .await
            .unwrap()
            .unwrap()
            .total_cents,
        1_000,
        "republishing is what makes a new price public"
    );
}

/// Every order is offered to the owner's inbox exactly once, carrying its own
/// tenant, its own site and its own lines — two tenants' orders claimed in the
/// same sweep never mix.
#[tokio::test]
async fn orders_are_claimed_for_notification_at_most_once_and_never_mix_tenants() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let first = fresh_account(&store, "order-notify-one").await;
    let second = fresh_account(&store, "order-notify-two").await;
    let (first_site, first_catalog, first_sub) = orderable_site(&first, "order-notify-one").await;
    let (_, second_catalog, second_sub) = orderable_site(&second, "order-notify-two").await;
    let public = public_store(blobs).await;

    // Drain whatever earlier tests left pending, so this test reads only its
    // own rows out of the shared database.
    let mut mine = Vec::new();
    loop {
        let claimed = store.claim_order_notifications(200).await.unwrap();
        if claimed.is_empty() {
            break;
        }
        mine.extend(
            claimed
                .into_iter()
                .filter(|n| n.site_subdomain == first_sub || n.site_subdomain == second_sub),
        );
    }
    assert!(mine.is_empty(), "no order has been placed yet");

    let one = public
        .place_public_order(first_catalog.as_str(), &contact(), &[asked("sourdough", 2)])
        .await
        .unwrap()
        .unwrap();
    let two = public
        .place_public_order(
            second_catalog.as_str(),
            &contact(),
            &[asked("wedding-cake", 1)],
        )
        .await
        .unwrap()
        .unwrap();

    let claimed = store.claim_order_notifications(50).await.unwrap();
    let for_one = claimed
        .iter()
        .find(|n| n.order.as_str() == one.as_str())
        .expect("the first tenant's order was claimed");
    let for_two = claimed
        .iter()
        .find(|n| n.order.as_str() == two.as_str())
        .expect("the second tenant's order was claimed");
    assert_ne!(for_one.tenant.as_str(), for_two.tenant.as_str());
    assert_eq!(for_one.site_subdomain, first_sub);
    assert_eq!(for_one.customer_email, "ada@example.test");
    assert_eq!(for_one.total_cents, 900);
    assert_eq!(for_one.lines.len(), 1);
    assert_eq!(for_one.lines[0].item_name, "Sourdough");
    assert_eq!(for_one.lines[0].quantity, 2);
    assert_eq!(for_two.site_subdomain, second_sub);
    assert_eq!(for_two.lines.len(), 1);
    assert_eq!(for_two.lines[0].item_name, "Wedding cake");
    assert_eq!(for_two.lines[0].unit_price_cents, None);

    // At-most-once: a second sweep finds nothing of ours, and the orders
    // themselves are untouched by the claim.
    let again = store.claim_order_notifications(50).await.unwrap();
    assert!(
        !again
            .iter()
            .any(|n| n.order.as_str() == one.as_str() || n.order.as_str() == two.as_str()),
        "an order was offered for notification twice"
    );
    assert_eq!(
        first
            .site_order(&first_site, &one)
            .await
            .unwrap()
            .unwrap()
            .status,
        SiteOrderStatus::New,
        "claiming a notification does not touch the owner's workflow"
    );
}

/// The write gate is one gate: the anonymous door refuses the same bodies the
/// typed helpers refuse, before anything reaches the database.
#[tokio::test]
async fn the_public_door_shares_the_write_gate() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let owner = fresh_account(&store, "order-gate").await;
    let (site, catalog, _) = orderable_site(&owner, "order-gate").await;
    let public = public_store(blobs).await;

    // Nothing ordered.
    assert!(matches!(
        public
            .place_public_order(catalog.as_str(), &contact(), &[asked("sourdough", 0)])
            .await,
        Err(StoreError::Validation(_))
    ));
    // A quantity above the ceiling.
    assert!(matches!(
        public
            .place_public_order(catalog.as_str(), &contact(), &[asked("sourdough", 10_000)])
            .await,
        Err(StoreError::Validation(_))
    ));
    assert!(owner.site_orders(&site).await.unwrap().is_empty());

    // A repeated handle is merged rather than refused: the visitor cannot fix
    // a page that renders the same item twice.
    let merged = public
        .place_public_order(
            catalog.as_str(),
            &contact(),
            &[asked("sourdough", 1), asked("sourdough", 2)],
        )
        .await
        .unwrap()
        .unwrap();
    let lines = owner.site_order_lines(&site, &merged).await.unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].quantity, 3);
}

/// A deleted order is gone from every door, including the notifier's.
#[tokio::test]
async fn a_deleted_order_is_never_notified() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let owner = fresh_account(&store, "order-deleted").await;
    let (site, catalog, sub) = orderable_site(&owner, "order-deleted").await;
    let public = public_store(blobs).await;

    let placed: SiteOrderId = public
        .place_public_order(catalog.as_str(), &contact(), &[asked("sourdough", 1)])
        .await
        .unwrap()
        .unwrap();
    owner.delete_site_order(&site, &placed).await.unwrap();

    let claimed = store.claim_order_notifications(200).await.unwrap();
    assert!(
        !claimed
            .iter()
            .any(|n| n.site_subdomain == sub || n.order.as_str() == placed.as_str()),
        "a deleted order must never reach the owner's inbox"
    );
}
