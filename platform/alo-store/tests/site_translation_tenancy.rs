//! Atomicity, stale review, localized posts, and tenant isolation for S2.01e.

#![allow(clippy::unwrap_used)]

mod common;

use alo_store::{
    DriveLocation, NewDriveFile, NewSitePost, SiteTranslationPageContent, SiteTranslationPageWrite,
    SiteTranslationPostContent, SiteTranslationPostWrite, StoreError,
};

#[tokio::test]
async fn reviewed_site_translation_is_atomic_stale_safe_and_tenant_scoped() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("translation-owner").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("owner@translation.test")
        .await
        .unwrap();
    let owner = store.for_account(tenant, user);
    let other_tenant = store.create_tenant("translation-other").await.unwrap();
    let other_user = store
        .for_tenant(other_tenant.clone())
        .create_user("other@translation.test")
        .await
        .unwrap();
    let outsider = store.for_account(other_tenant, other_user);

    let subdomain = format!(
        "translation-{}",
        alo_store::SiteId::generate()
            .as_str()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(16)
            .collect::<String>()
            .to_ascii_lowercase()
    );
    let site = owner.create_site("Atlas", &subdomain).await.unwrap();
    owner
        .set_site_locales(&site, "en", &["en".into(), "fr".into()])
        .await
        .unwrap();
    let page = owner
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    let about_page = owner
        .create_site_page(&site, "About", "about", false)
        .await
        .unwrap();
    let sections = serde_json::json!({"schema_version":1,"sections":[]});
    let doc = owner
        .drive_create_file(
            &DriveLocation::Personal,
            None,
            &NewDriveFile {
                name: "News".into(),
                blob_id: "translation-doc".into(),
                kind: Some("doc".into()),
                ..NewDriveFile::default()
            },
        )
        .await
        .unwrap();
    let post = owner
        .create_site_post(
            &site,
            &NewSitePost {
                doc_node_id: &doc,
                slug: "news",
                title: "News",
                excerpt: "Latest update",
                cover_blob_id: None,
            },
        )
        .await
        .unwrap();
    let pages = vec![SiteTranslationPageWrite {
        id: page.clone(),
        before: SiteTranslationPageContent {
            title: "Home".into(),
            slug: "".into(),
            seo_title: None,
            seo_description: None,
            sections: sections.clone(),
        },
        after: SiteTranslationPageContent {
            title: "Accueil".into(),
            slug: "".into(),
            seo_title: None,
            seo_description: Some("Bienvenue".into()),
            sections: sections.clone(),
        },
    }];
    let posts = vec![SiteTranslationPostWrite {
        id: post.clone(),
        before: SiteTranslationPostContent {
            title: "News".into(),
            slug: "news".into(),
            excerpt: "Latest update".into(),
        },
        after: SiteTranslationPostContent {
            title: "Actualites".into(),
            slug: "actualites".into(),
            excerpt: "Dernieres nouvelles".into(),
        },
    }];

    let invalid_path = vec![SiteTranslationPageWrite {
        id: about_page.clone(),
        before: SiteTranslationPageContent {
            title: "About".into(),
            slug: "about".into(),
            seo_title: None,
            seo_description: None,
            sections: sections.clone(),
        },
        after: SiteTranslationPageContent {
            title: "A propos".into(),
            slug: "".into(),
            seo_title: None,
            seo_description: None,
            sections: sections.clone(),
        },
    }];
    assert!(matches!(
        owner
            .apply_site_translation(&site, "en", "fr", &invalid_path, &[])
            .await,
        Err(StoreError::Conflict(message)) if message.contains("path cannot be empty")
    ));
    let untranslated = owner
        .localized_site_page(&site, &about_page, "fr")
        .await
        .unwrap()
        .unwrap();
    assert!(untranslated.fallback);
    assert_eq!(untranslated.resolved_locale, "en");

    assert!(matches!(
        outsider
            .apply_site_translation(&site, "en", "fr", &pages, &posts)
            .await,
        Err(StoreError::NotFound)
    ));
    owner
        .apply_site_translation(&site, "en", "fr", &pages, &posts)
        .await
        .unwrap();
    let french = owner
        .localized_site_page(&site, &page, "fr")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(french.page.title, "Accueil");
    let french_posts = owner.site_posts_in_locale_exact(&site, "fr").await.unwrap();
    assert_eq!(french_posts[0].title, "Actualites");

    owner.set_page_title(&site, &page, "Welcome").await.unwrap();
    let stale = owner
        .apply_site_translation(&site, "en", "fr", &pages, &posts)
        .await;
    assert!(
        matches!(stale, Err(StoreError::Conflict(message)) if message.contains("fresh translation"))
    );
    assert_eq!(
        owner.site_posts_in_locale_exact(&site, "fr").await.unwrap()[0].title,
        "Actualites"
    );
}
