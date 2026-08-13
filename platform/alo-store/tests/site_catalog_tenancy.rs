//! Tenant boundaries and write rules of the site catalog model, and of the
//! Base import that seeds it.

#![allow(clippy::unwrap_used)]

mod common;

use alo_store::{
    BlobId, DriveLocation, SITE_CATALOG_IMAGE_ALT_MAX_CHARS, SiteCatalogAvailability,
    SiteCatalogCategoryInput, SiteCatalogImport, SiteCatalogImportMapping, SiteCatalogInput,
    SiteCatalogItemInput, SiteId, StoreError,
};
use bytes::Bytes;
use serde_json::{Map, Value, json};

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

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

fn cells(entries: &[(&str, Value)]) -> Value {
    Value::Object(
        entries
            .iter()
            .map(|(field, value)| ((*field).to_owned(), value.clone()))
            .collect::<Map<String, Value>>(),
    )
}

fn item<'a>(name: &'a str, slug: &'a str, price: Option<i64>) -> SiteCatalogItemInput<'a> {
    SiteCatalogItemInput {
        category: None,
        name,
        slug,
        description: None,
        price_cents: price,
        price_note: None,
        image: None,
        image_alt: None,
        availability: SiteCatalogAvailability::Available,
        position: 0,
    }
}

/// Tenant A's catalog, its categories, and its items are invisible and
/// unwritable to tenant B — every door, not just the listing one.
#[tokio::test]
async fn a_foreign_tenant_can_neither_read_nor_write_a_catalog() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("site-catalog-owner").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("owner@site-catalog.test")
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let site = account
        .create_site("Harbour", &subdomain("catalog-owner"))
        .await
        .unwrap();
    let catalog = account
        .create_site_catalog(
            &site,
            &SiteCatalogInput {
                name: " Harbour menu ",
                currency: "eur",
                orders_enabled: false,
            },
        )
        .await
        .unwrap();
    let stored = account
        .site_catalog(&site, &catalog)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.name, "Harbour menu", "the name is trimmed");
    assert_eq!(stored.currency, "EUR", "the currency is normalized");
    let category = account
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
    let filter = account
        .create_site_catalog_item(&site, &catalog, &item("Filter brew", "filter", Some(350)))
        .await
        .unwrap();

    let other_tenant = store.create_tenant("site-catalog-rival").await.unwrap();
    let other_user = store
        .for_tenant(other_tenant.clone())
        .create_user("rival@site-catalog.test")
        .await
        .unwrap();
    let rival = store.for_account(other_tenant, other_user);

    assert!(rival.site_catalogs(&site).await.unwrap().is_empty());
    assert!(rival.site_catalog(&site, &catalog).await.unwrap().is_none());
    assert_not_found(rival.site_catalog_categories(&site, &catalog).await);
    assert_not_found(rival.site_catalog_items(&site, &catalog).await);
    assert_not_found(rival.site_catalog_item(&site, &catalog, &filter).await);
    assert_not_found(
        rival
            .update_site_catalog(
                &site,
                &catalog,
                &SiteCatalogInput {
                    name: "Stolen",
                    currency: "USD",
                    orders_enabled: false,
                },
            )
            .await,
    );
    assert_not_found(rival.delete_site_catalog(&site, &catalog).await);
    assert_not_found(
        rival
            .create_site_catalog_item(&site, &catalog, &item("Theirs", "theirs", None))
            .await,
    );
    assert_not_found(
        rival
            .update_site_catalog_item(&site, &catalog, &filter, &item("Theirs", "theirs", None))
            .await,
    );
    assert_not_found(
        rival
            .delete_site_catalog_item(&site, &catalog, &filter)
            .await,
    );
    assert_not_found(
        rival
            .update_site_catalog_category(
                &site,
                &catalog,
                &category,
                &SiteCatalogCategoryInput {
                    name: "Theirs",
                    slug: "theirs",
                    position: 0,
                },
            )
            .await,
    );
    assert_not_found(
        rival
            .delete_site_catalog_category(&site, &catalog, &category)
            .await,
    );
    assert_not_found(rival.site_catalog_preview(&site, &catalog).await);

    // Nothing the rival attempted left a mark.
    assert_eq!(
        account
            .site_catalog(&site, &catalog)
            .await
            .unwrap()
            .unwrap()
            .name,
        "Harbour menu"
    );
    assert_eq!(
        account
            .site_catalog_items(&site, &catalog)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        account
            .site_catalog_categories(&site, &catalog)
            .await
            .unwrap()
            .len(),
        1
    );
}

/// The write rules that keep a public page honest: handles are unique per
/// catalog, a price note without a price is refused, a category from another
/// catalog is not a category, and deleting a grouping keeps its items.
#[tokio::test]
async fn catalog_writes_are_validated_whole() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("site-catalog-rules").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("rules@site-catalog.test")
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let site = account
        .create_site("Harbour", &subdomain("catalog-rules"))
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
    let other_catalog = account
        .create_site_catalog(
            &site,
            &SiteCatalogInput {
                name: "Rooms",
                currency: "EUR",
                orders_enabled: false,
            },
        )
        .await
        .unwrap();

    assert!(matches!(
        account
            .create_site_catalog(
                &site,
                &SiteCatalogInput {
                    name: "Menu",
                    currency: "EURO",
                    orders_enabled: false,
                }
            )
            .await,
        Err(StoreError::Validation(_))
    ));
    assert!(matches!(
        account
            .create_site_catalog_item(&site, &catalog, &item("No handle", "Not A Slug", None))
            .await,
        Err(StoreError::Validation(_))
    ));

    account
        .create_site_catalog_item(&site, &catalog, &item("Filter brew", "filter", Some(350)))
        .await
        .unwrap();
    let duplicate = account
        .create_site_catalog_item(&site, &catalog, &item("Filter, again", "filter", Some(400)))
        .await;
    assert!(
        matches!(duplicate, Err(StoreError::Conflict(_))),
        "a repeated handle must be refused: {duplicate:?}"
    );
    // The same handle in another catalog is a different thing entirely.
    account
        .create_site_catalog_item(&site, &other_catalog, &item("Filter room", "filter", None))
        .await
        .unwrap();

    let mut noted = item("Cold brew", "cold-brew", None);
    noted.price_note = Some("per litre");
    assert!(matches!(
        account
            .create_site_catalog_item(&site, &catalog, &noted)
            .await,
        Err(StoreError::Validation(_))
    ));

    let foreign_category = account
        .create_site_catalog_category(
            &site,
            &other_catalog,
            &SiteCatalogCategoryInput {
                name: "Suites",
                slug: "suites",
                position: 0,
            },
        )
        .await
        .unwrap();
    let mut misfiled = item("Misfiled", "misfiled", None);
    misfiled.category = Some(&foreign_category);
    assert_not_found(
        account
            .create_site_catalog_item(&site, &catalog, &misfiled)
            .await,
    );

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
    let mut grouped = item("Espresso", "espresso", Some(250));
    grouped.category = Some(&brews);
    let espresso = account
        .create_site_catalog_item(&site, &catalog, &grouped)
        .await
        .unwrap();
    account
        .delete_site_catalog_category(&site, &catalog, &brews)
        .await
        .unwrap();
    let survivor = account
        .site_catalog_item(&site, &catalog, &espresso)
        .await
        .unwrap()
        .unwrap();
    assert!(
        survivor.category_id.is_none(),
        "deleting a grouping must not delete what it grouped"
    );
    assert_eq!(survivor.price_cents, Some(250));
}

/// What an item's photograph shows is the owner's sentence: it round-trips,
/// it is bounded, it cannot describe a picture that is not there, and it
/// cannot point at another tenant's file.
#[tokio::test]
async fn an_item_photograph_carries_the_words_that_describe_it() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("site-catalog-photo").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("photo@site-catalog.test")
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let other_tenant = store.create_tenant("site-catalog-photo-b").await.unwrap();
    let other_user = store
        .for_tenant(other_tenant.clone())
        .create_user("rival@site-catalog.test")
        .await
        .unwrap();
    let rival = store.for_account(other_tenant, other_user);
    let site = account
        .create_site("Harbour", &subdomain("catalog-photo"))
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
    let photo = account
        .put_blob(Bytes::from_static(b"beans"), Some("image/png"))
        .await
        .unwrap();
    let rival_photo = rival
        .put_blob(Bytes::from_static(b"rival-beans"), Some("image/png"))
        .await
        .unwrap();

    // A description with nothing to describe is refused by name, so the
    // sentence the editor shows says what to do about it.
    let mut orphan = item("Harbour blend", "harbour-blend", Some(123_450));
    orphan.image_alt = Some("A kraft bag of whole beans");
    match account
        .create_site_catalog_item(&site, &catalog, &orphan)
        .await
    {
        Err(StoreError::Validation(detail)) => assert!(
            detail.contains("needs an image"),
            "the refusal must say what is missing: {detail}"
        ),
        other => panic!("expected a validation error, got {other:?}"),
    }

    let mut described = item("Harbour blend", "harbour-blend", Some(123_450));
    described.image = Some(&photo);
    described.image_alt = Some("  A kraft bag of whole beans on the counter  ");
    let id = account
        .create_site_catalog_item(&site, &catalog, &described)
        .await
        .unwrap();
    let stored = account
        .site_catalog_item(&site, &catalog, &id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.image_alt.as_deref(),
        Some("A kraft bag of whole beans on the counter"),
        "the description is stored trimmed"
    );

    // Blank is not a description: it stores as "nobody wrote one", which is
    // what makes the published card fall back to the item name.
    let mut blanked = item("Harbour blend", "harbour-blend", Some(123_450));
    blanked.image = Some(&photo);
    blanked.image_alt = Some("   ");
    account
        .update_site_catalog_item(&site, &catalog, &id, &blanked)
        .await
        .unwrap();
    assert!(
        account
            .site_catalog_item(&site, &catalog, &id)
            .await
            .unwrap()
            .unwrap()
            .image_alt
            .is_none()
    );

    let long = "a".repeat(SITE_CATALOG_IMAGE_ALT_MAX_CHARS + 1);
    let mut overlong = item("Harbour blend", "harbour-blend", Some(123_450));
    overlong.image = Some(&photo);
    overlong.image_alt = Some(&long);
    assert!(matches!(
        account
            .update_site_catalog_item(&site, &catalog, &id, &overlong)
            .await,
        Err(StoreError::Validation(_))
    ));

    // The rival's file is not a file: the item cannot reach it, described or
    // not, and the item that tried keeps the photograph it had.
    let mut stolen = item("Harbour blend", "harbour-blend", Some(123_450));
    stolen.image = Some(&rival_photo);
    stolen.image_alt = Some("Someone else's beans");
    assert_not_found(
        account
            .update_site_catalog_item(&site, &catalog, &id, &stolen)
            .await,
    );
    let untouched = account
        .site_catalog_item(&site, &catalog, &id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        untouched.image.as_ref().map(BlobId::as_str),
        Some(photo.as_str())
    );
}

/// The Base import copies rows once, names the row it cannot read, and updates
/// its own rows on a second run instead of duplicating them.
#[tokio::test]
async fn importing_from_base_copies_rows_once_and_refuses_a_price_it_cannot_read() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("site-catalog-import").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("import@site-catalog.test")
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let site = account
        .create_site("Harbour", &subdomain("catalog-import"))
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
    let base_node = account
        .create_base(&DriveLocation::Personal, None, "Dishes")
        .await
        .unwrap();
    let base = account.base(&base_node).await.unwrap().unwrap();
    let table = base.tables[0].id.clone();
    let name_field = base.tables[0].fields[0].id.clone();
    let notes_field = base.tables[0].fields[1].id.clone();
    let price_field = account
        .base_add_field(&table, "Price", "number", &json!({}))
        .await
        .unwrap();
    let course_field = account
        .base_add_field(&table, "Course", "text", &json!({}))
        .await
        .unwrap();
    let photo_field = account
        .base_add_field(&table, "Photo", "attachment", &json!({}))
        .await
        .unwrap();
    let photo = account
        .put_blob(Bytes::from_static(b"mussels"), Some("image/png"))
        .await
        .unwrap();
    let new_photo = account
        .put_blob(Bytes::from_static(b"mussels-again"), Some("image/png"))
        .await
        .unwrap();
    let first = account
        .base_add_record(
            &table,
            &cells(&[
                (name_field.as_str(), json!("Mussels")),
                (notes_field.as_str(), json!("From the bay, with cream")),
                (price_field.as_str(), json!(18.5)),
                (course_field.as_str(), json!("Mains")),
                (photo_field.as_str(), json!([{"blob_id": photo.as_str()}])),
            ]),
        )
        .await
        .unwrap();
    account
        .base_add_record(
            &table,
            &cells(&[
                (name_field.as_str(), json!("Mussels")),
                (price_field.as_str(), json!(24)),
                (course_field.as_str(), json!("Mains")),
            ]),
        )
        .await
        .unwrap();

    let mapping = SiteCatalogImportMapping {
        name: name_field.clone(),
        description: Some(notes_field.clone()),
        price: Some(price_field.clone()),
        category: Some(course_field.clone()),
        image: Some(photo_field.clone()),
    };
    let import = SiteCatalogImport {
        base_node_id: &base_node,
        base_table_id: &table,
        mapping: &mapping,
    };
    let report = account
        .import_site_catalog_from_base(&site, &catalog, &import)
        .await
        .unwrap();
    assert_eq!(report.created, 2);
    assert_eq!(report.updated, 0);
    assert_eq!(report.categories_created, 1);
    assert!(report.skipped >= 1, "the blank default rows are skipped");

    let items = account.site_catalog_items(&site, &catalog).await.unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].name, "Mussels");
    assert_eq!(items[0].price_cents, Some(1_850), "18.5 EUR is 1850 cents");
    assert_eq!(
        items[0].description.as_deref(),
        Some("From the bay, with cream")
    );
    assert_eq!(
        items[1].slug, "mussels-2",
        "a repeated name gets its own handle"
    );
    assert!(items.iter().all(|item| item.category_id.is_some()));
    assert_eq!(
        items[0].image.as_ref().map(BlobId::as_str),
        Some(photo.as_str()),
        "the attachment column is the item's picture"
    );

    // The owner describes that picture by hand. Base has no column for it, so
    // the next import must not be the thing that erases it.
    let described = SiteCatalogItemInput {
        category: items[0].category_id.as_ref(),
        name: &items[0].name,
        slug: &items[0].slug,
        description: items[0].description.as_deref(),
        price_cents: items[0].price_cents,
        price_note: None,
        image: items[0].image.as_ref(),
        image_alt: Some("A bowl of mussels in a cream broth"),
        availability: items[0].availability,
        position: items[0].position,
    };
    account
        .update_site_catalog_item(&site, &catalog, &items[0].id, &described)
        .await
        .unwrap();

    // Second run: the same rows, updated in place.
    account
        .base_update_record(
            &first,
            &cells(&[
                (name_field.as_str(), json!("Mussels, large")),
                (price_field.as_str(), json!("19,50")),
                (course_field.as_str(), json!("Mains")),
                (photo_field.as_str(), json!([{"blob_id": photo.as_str()}])),
            ]),
        )
        .await
        .unwrap();
    let second = account
        .import_site_catalog_from_base(&site, &catalog, &import)
        .await
        .unwrap();
    assert_eq!(second.created, 0);
    assert_eq!(
        second.updated, 2,
        "both rows are recognised, not re-created"
    );
    assert_eq!(second.categories_created, 0);
    let items = account.site_catalog_items(&site, &catalog).await.unwrap();
    assert_eq!(items.len(), 2, "a second import must not duplicate a row");
    assert_eq!(items[0].name, "Mussels, large");
    assert_eq!(items[0].price_cents, Some(1_950));
    assert_eq!(
        items[0].slug, "mussels",
        "the handle a link points at is kept"
    );
    assert_eq!(
        items[0].image_alt.as_deref(),
        Some("A bowl of mussels in a cream broth"),
        "the same picture keeps the words written about it"
    );

    // A different photograph, though, is not what those words described.
    account
        .base_update_record(
            &first,
            &cells(&[
                (name_field.as_str(), json!("Mussels, large")),
                (price_field.as_str(), json!("19,50")),
                (course_field.as_str(), json!("Mains")),
                (
                    photo_field.as_str(),
                    json!([{"blob_id": new_photo.as_str()}]),
                ),
            ]),
        )
        .await
        .unwrap();
    account
        .import_site_catalog_from_base(&site, &catalog, &import)
        .await
        .unwrap();
    let items = account.site_catalog_items(&site, &catalog).await.unwrap();
    assert_eq!(
        items[0].image.as_ref().map(BlobId::as_str),
        Some(new_photo.as_str())
    );
    assert!(
        items[0].image_alt.is_none(),
        "a replaced picture must not keep the old description"
    );

    // An ambiguous price stops the import naming the row, rather than guessing.
    account
        .base_update_record(
            &first,
            &cells(&[
                (name_field.as_str(), json!("Mussels, large")),
                (price_field.as_str(), json!("1,234")),
            ]),
        )
        .await
        .unwrap();
    match account
        .import_site_catalog_from_base(&site, &catalog, &import)
        .await
    {
        Err(StoreError::Validation(detail)) => {
            // A new Base ships blank default rows, so the row number is the
            // position in the table — what the tenant sees, not the item index.
            assert!(detail.starts_with("row "), "{detail}");
            assert!(detail.contains("two different prices"), "{detail}");
        }
        other => panic!("expected a named validation failure, got {other:?}"),
    }

    // A Base another tenant owns is not importable, and neither is a catalog
    // that is not the importer's.
    let other_tenant = store
        .create_tenant("site-catalog-import-rival")
        .await
        .unwrap();
    let other_user = store
        .for_tenant(other_tenant.clone())
        .create_user("rival@site-catalog-import.test")
        .await
        .unwrap();
    let rival = store.for_account(other_tenant, other_user);
    assert_not_found(
        rival
            .import_site_catalog_from_base(&site, &catalog, &import)
            .await,
    );
}
