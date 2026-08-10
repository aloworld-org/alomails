//! Atomic generated-site persistence and its mandatory tenant boundary.

#![allow(clippy::unwrap_used)]

mod common;

use alo_store::{NewGeneratedSite, NewGeneratedSitePage, SectionsEnvelope, SiteTheme, StoreError};
use serde_json::json;

use common::test_store;

fn proposal(subdomain: String) -> NewGeneratedSite {
    NewGeneratedSite {
        name: "Generated Bakery".to_owned(),
        subdomain,
        theme: SiteTheme::new(),
        pages: vec![NewGeneratedSitePage {
            title: "Home".to_owned(),
            slug: String::new(),
            is_home: true,
            seo_title: Some("Fresh bread".to_owned()),
            seo_description: None,
            sections: SectionsEnvelope::from_value(json!({
                "schema_version": 1,
                "sections": [{
                    "type": "hero",
                    "heading": "Fresh every morning",
                    "subheading": null,
                    "image": null,
                    "primary_cta": null,
                    "secondary_cta": null
                }]
            }))
            .unwrap(),
        }],
    }
}

#[tokio::test]
async fn generated_draft_is_atomic_unpublished_and_wrong_tenant_invisible() {
    let store = test_store().await;
    let tenant_a = store.create_tenant("generated-a").await.unwrap();
    let tenant_b = store.create_tenant("generated-b").await.unwrap();
    let user_a = store
        .for_tenant(tenant_a.clone())
        .create_user("generated-a@example.test")
        .await
        .unwrap();
    let user_b = store
        .for_tenant(tenant_b.clone())
        .create_user("generated-b@example.test")
        .await
        .unwrap();
    let a = store.for_account(tenant_a, user_a);
    let b = store.for_account(tenant_b, user_b);
    let subdomain = format!("generated-{}", alo_store::SiteId::generate())
        .to_ascii_lowercase()
        .replace('_', "-");
    let subdomain: String = subdomain.chars().take(40).collect();

    let created = a.create_generated_site(proposal(subdomain)).await.unwrap();
    let site = a.site(&created.site).await.unwrap().unwrap();
    assert_eq!(site.status.as_str(), "draft");
    let pages = a.site_pages(&created.site).await.unwrap();
    assert_eq!(pages.len(), 1);
    assert!(pages[0].is_home);
    assert_eq!(pages[0].sections["sections"][0]["type"], "hero");

    // Mandatory wrong-tenant proof: both the site and every generated page
    // disappear behind another account door.
    assert!(b.site(&created.site).await.unwrap().is_none());
    assert!(b.site_pages(&created.site).await.unwrap().is_empty());
    assert!(
        b.site_page(&created.site, &created.pages[0])
            .await
            .unwrap()
            .is_none()
    );

    // Full validation runs before the transaction, so a later invalid page
    // cannot strand the otherwise-valid first page or its site.
    let before = a.sites().await.unwrap().len();
    let mut invalid = proposal("invalid-generated-draft".to_owned());
    invalid.pages.push(NewGeneratedSitePage {
        title: "Broken".to_owned(),
        slug: String::new(),
        is_home: false,
        seo_title: None,
        seo_description: None,
        sections: SectionsEnvelope::new(),
    });
    assert!(matches!(
        a.create_generated_site(invalid).await,
        Err(StoreError::Conflict(_))
    ));
    assert_eq!(a.sites().await.unwrap().len(), before);
}
