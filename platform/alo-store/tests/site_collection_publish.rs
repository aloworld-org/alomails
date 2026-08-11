//! Collection publishing freezes Base rows and keeps every read tenant-safe.

#![allow(clippy::unwrap_used)]

mod common;

use alo_store::{
    DriveLocation, SiteCollectionFieldMapping, SiteCollectionInput, SiteId, SitePublicStore,
    StoreError,
};
use serde_json::{Map, Value, json};
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

fn cells(entries: &[(&str, &str)]) -> Value {
    Value::Object(
        entries
            .iter()
            .map(|(field, value)| ((*field).to_owned(), Value::String((*value).to_owned())))
            .collect::<Map<String, Value>>(),
    )
}

#[tokio::test]
async fn publish_freezes_only_referenced_rows_and_public_reads_the_snapshot() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let tenant = store
        .create_tenant("site-collection-publish")
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("publisher@site-collection.test")
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    let site_subdomain = subdomain("collection-publish");
    let site = account
        .create_site("Roastery", &site_subdomain)
        .await
        .unwrap();
    let home = account
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    let base_node = account
        .create_base(&DriveLocation::Personal, None, "Roasts")
        .await
        .unwrap();
    let base = account.base(&base_node).await.unwrap().unwrap();
    let table = base.tables[0].id.clone();
    let title = base.tables[0].fields[0].id.clone();
    let summary = base.tables[0].fields[1].id.clone();
    let mapping = SiteCollectionFieldMapping {
        title: title.clone(),
        slug: None,
        summary: Some(summary.clone()),
        body: None,
        image: None,
        link: None,
        published_at: None,
    };
    let collection = account
        .create_site_collection(
            &site,
            &SiteCollectionInput {
                name: "Seasonal roasts",
                base_node_id: &base_node,
                base_table_id: &table,
                mapping: &mapping,
            },
        )
        .await
        .unwrap();
    let record = account
        .base_add_record(
            &table,
            &cells(&[
                (title.as_str(), "Harbour Blend"),
                (summary.as_str(), "Chocolate and red apple"),
            ]),
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
                    "type": "collection",
                    "collection_id": collection.as_str(),
                    "heading": "Fresh from the roaster"
                }]
            }),
        )
        .await
        .unwrap();

    let first_publish = account.publish_site(&site).await.unwrap();
    let first = account
        .site_publish_collection_snapshots(&site, &first_publish)
        .await
        .unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(
        first[0].items.len(),
        1,
        "three blank default rows are skipped"
    );
    assert_eq!(first[0].items[0].title, "Harbour Blend");

    account
        .base_update_record(
            &record,
            &cells(&[
                (title.as_str(), "Night Ferry"),
                (summary.as_str(), "Cocoa and blackberry"),
            ]),
        )
        .await
        .unwrap();
    let still_first = account
        .site_publish_collection_snapshots(&site, &first_publish)
        .await
        .unwrap();
    assert_eq!(still_first[0].items[0].title, "Harbour Blend");

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
    assert_eq!(
        public.published_collections(&resolved).await.unwrap(),
        still_first
    );

    let second_publish = account.publish_site(&site).await.unwrap();
    let second = account
        .site_publish_collection_snapshots(&site, &second_publish)
        .await
        .unwrap();
    assert_eq!(second[0].items[0].title, "Night Ferry");

    let other_tenant = store
        .create_tenant("site-collection-outsider")
        .await
        .unwrap();
    let outsider_user = store
        .for_tenant(other_tenant.clone())
        .create_user("outsider@site-collection.test")
        .await
        .unwrap();
    let outsider = store.for_account(other_tenant, outsider_user);
    assert!(
        outsider
            .site_publish_collection_snapshots(&site, &first_publish)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn empty_is_stable_and_invalid_rows_never_replace_the_live_publish() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("site-collection-errors").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("errors@site-collection.test")
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let site = account
        .create_site("Directory", &subdomain("collection-errors"))
        .await
        .unwrap();
    let home = account
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    let base_node = account
        .create_base(&DriveLocation::Personal, None, "People")
        .await
        .unwrap();
    let base = account.base(&base_node).await.unwrap().unwrap();
    let table = base.tables[0].id.clone();
    let title = base.tables[0].fields[0].id.clone();
    let summary = base.tables[0].fields[1].id.clone();
    let mapping = SiteCollectionFieldMapping {
        title: title.clone(),
        slug: None,
        summary: Some(summary.clone()),
        body: None,
        image: None,
        link: None,
        published_at: None,
    };
    let collection = account
        .create_site_collection(
            &site,
            &SiteCollectionInput {
                name: "People",
                base_node_id: &base_node,
                base_table_id: &table,
                mapping: &mapping,
            },
        )
        .await
        .unwrap();
    account
        .set_page_sections(
            &site,
            &home,
            json!({
                "schema_version": 1,
                "sections": [{"type": "collection", "collection_id": collection.as_str()}]
            }),
        )
        .await
        .unwrap();

    let live = account.publish_site(&site).await.unwrap();
    let empty = account
        .site_publish_collection_snapshots(&site, &live)
        .await
        .unwrap();
    assert!(empty[0].items.is_empty());

    let invalid_record = account
        .base_add_record(&table, &cells(&[(summary.as_str(), "Missing a title")]))
        .await
        .unwrap();
    match account.publish_site(&site).await {
        Err(StoreError::Conflict(detail)) => {
            assert!(detail.contains("content but no title"), "{detail}")
        }
        other => panic!("expected a deterministic collection conflict, got {other:?}"),
    }
    assert_eq!(
        account
            .current_site_publish(&site)
            .await
            .unwrap()
            .unwrap()
            .id,
        live
    );

    account.base_delete_record(&invalid_record).await.unwrap();
    account
        .delete_site_collection(&site, &collection)
        .await
        .unwrap();
    match account.publish_site(&site).await {
        Err(StoreError::Conflict(detail)) => assert!(detail.contains("not connected"), "{detail}"),
        other => panic!("expected a disconnected collection conflict, got {other:?}"),
    }
    assert_eq!(
        account
            .current_site_publish(&site)
            .await
            .unwrap()
            .unwrap()
            .id,
        live
    );
}
