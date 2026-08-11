//! Tenant, Base-table, and field-type boundaries for Sites collections.

#![allow(clippy::unwrap_used)]

mod common;

use alo_store::{
    BaseTableId, DriveLocation, SiteCollectionFieldMapping, SiteCollectionInput, SiteId, StoreError,
};
use serde_json::json;

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

#[tokio::test]
async fn collection_bindings_validate_table_membership_and_field_roles() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("site-collection-fields").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("fields@site-collections.test")
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let site = account
        .create_site("Directory", &subdomain("collection-fields"))
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
    let image = account
        .base_add_field(&table, "Portrait", "attachment", &json!({}))
        .await
        .unwrap();
    let link = account
        .base_add_field(&table, "Profile", "link", &json!({}))
        .await
        .unwrap();
    let published_at = account
        .base_add_field(&table, "Published", "date", &json!({}))
        .await
        .unwrap();
    let mapping = SiteCollectionFieldMapping {
        title: title.clone(),
        slug: None,
        summary: Some(summary),
        body: None,
        image: Some(image),
        link: Some(link),
        published_at: Some(published_at),
    };
    let input = SiteCollectionInput {
        name: " Team ",
        base_node_id: &base_node,
        base_table_id: &table,
        mapping: &mapping,
    };

    let collection = account.create_site_collection(&site, &input).await.unwrap();
    let saved = account
        .site_collection(&site, &collection)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.name, "Team");
    assert_eq!(saved.base_node_id, base_node);
    assert_eq!(saved.base_table_id, table);
    assert_eq!(saved.mapping, mapping);
    assert_eq!(account.site_collections(&site).await.unwrap().len(), 1);

    let missing_table = BaseTableId::generate();
    assert_not_found(
        account
            .create_site_collection(
                &site,
                &SiteCollectionInput {
                    name: "Missing",
                    base_node_id: &base_node,
                    base_table_id: &missing_table,
                    mapping: &mapping,
                },
            )
            .await,
    );

    let second_table = account.base_add_table(&base_node, "Other").await.unwrap();
    match account
        .create_site_collection(
            &site,
            &SiteCollectionInput {
                name: "Cross-table field",
                base_node_id: &base_node,
                base_table_id: &second_table,
                mapping: &mapping,
            },
        )
        .await
    {
        Err(StoreError::Validation(detail)) => assert!(detail.contains("outside")),
        other => panic!("expected field-membership validation, got {other:?}"),
    }

    let wrong_type = SiteCollectionFieldMapping {
        image: Some(title),
        ..mapping.clone()
    };
    match account
        .update_site_collection(
            &site,
            &collection,
            &SiteCollectionInput {
                name: "Team",
                base_node_id: &base_node,
                base_table_id: &table,
                mapping: &wrong_type,
            },
        )
        .await
    {
        Err(StoreError::Validation(detail)) => assert!(detail.contains("attachment")),
        other => panic!("expected field-type validation, got {other:?}"),
    }
    assert_eq!(
        account
            .site_collection(&site, &collection)
            .await
            .unwrap()
            .unwrap()
            .mapping,
        mapping
    );

    account
        .delete_site_collection(&site, &collection)
        .await
        .unwrap();
    assert!(
        account
            .site_collection(&site, &collection)
            .await
            .unwrap()
            .is_none()
    );
    assert!(account.base(&base_node).await.unwrap().is_some());
}

#[tokio::test]
async fn collection_bindings_never_cross_tenants() {
    let store = common::test_store().await;
    let tenant_a = store.create_tenant("site-collection-a").await.unwrap();
    let user_a = store
        .for_tenant(tenant_a.clone())
        .create_user("a@site-collections.test")
        .await
        .unwrap();
    let a = store.for_account(tenant_a, user_a);
    let tenant_b = store.create_tenant("site-collection-b").await.unwrap();
    let user_b = store
        .for_tenant(tenant_b.clone())
        .create_user("b@site-collections.test")
        .await
        .unwrap();
    let b = store.for_account(tenant_b, user_b);

    let site_a = a
        .create_site("A directory", &subdomain("collection-a"))
        .await
        .unwrap();
    let base_a = a
        .create_base(&DriveLocation::Personal, None, "A private Base")
        .await
        .unwrap();
    let loaded_a = a.base(&base_a).await.unwrap().unwrap();
    let table_a = loaded_a.tables[0].id.clone();
    let mapping_a = SiteCollectionFieldMapping {
        title: loaded_a.tables[0].fields[0].id.clone(),
        slug: None,
        summary: None,
        body: None,
        image: None,
        link: None,
        published_at: None,
    };
    let input_a = SiteCollectionInput {
        name: "A people",
        base_node_id: &base_a,
        base_table_id: &table_a,
        mapping: &mapping_a,
    };
    let collection = a.create_site_collection(&site_a, &input_a).await.unwrap();

    let base_b = b
        .create_base(&DriveLocation::Personal, None, "B private Base")
        .await
        .unwrap();
    let loaded_b = b.base(&base_b).await.unwrap().unwrap();
    let table_b = loaded_b.tables[0].id.clone();
    let mapping_b = SiteCollectionFieldMapping {
        title: loaded_b.tables[0].fields[0].id.clone(),
        slug: None,
        summary: None,
        body: None,
        image: None,
        link: None,
        published_at: None,
    };
    let input_b = SiteCollectionInput {
        name: "B people",
        base_node_id: &base_b,
        base_table_id: &table_b,
        mapping: &mapping_b,
    };

    assert!(b.site_collections(&site_a).await.unwrap().is_empty());
    assert!(
        b.site_collection(&site_a, &collection)
            .await
            .unwrap()
            .is_none()
    );
    assert_not_found(b.create_site_collection(&site_a, &input_b).await);
    assert_not_found(
        b.update_site_collection(&site_a, &collection, &input_b)
            .await,
    );
    assert_not_found(b.delete_site_collection(&site_a, &collection).await);
    assert_not_found(a.create_site_collection(&site_a, &input_b).await);

    let owner_copy = a
        .site_collection(&site_a, &collection)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(owner_copy.base_node_id, base_a);
    assert_eq!(owner_copy.mapping, mapping_a);
}
