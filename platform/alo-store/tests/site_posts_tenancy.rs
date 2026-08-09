//! Tenant and document-reference boundary for alo Sites blog posts.

#![allow(clippy::unwrap_used)]

mod common;

use alo_store::{
    DriveLocation, NewDriveFile, NewSitePost, SitePostStatus, SitePostUpdate, StoreError,
};

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn posts_bind_only_readable_tenant_documents() {
    let store = common::test_store().await;
    let t1 = store.create_tenant("site-posts-a").await.unwrap();
    let user_a = store
        .for_tenant(t1.clone())
        .create_user("a@site-posts.test")
        .await
        .unwrap();
    let a = store.for_account(t1, user_a);
    let t2 = store.create_tenant("site-posts-b").await.unwrap();
    let user_b = store
        .for_tenant(t2.clone())
        .create_user("b@site-posts.test")
        .await
        .unwrap();
    let b = store.for_account(t2, user_b);

    let sub = |tag: &str| {
        format!(
            "{tag}{}",
            alo_store::SiteId::generate()
                .as_str()
                .chars()
                .filter(char::is_ascii_alphanumeric)
                .take(16)
                .collect::<String>()
                .to_ascii_lowercase()
        )
    };
    let site = a.create_site("A journal", &sub("journal")).await.unwrap();
    let other_site = a.create_site("A second", &sub("second")).await.unwrap();
    let b_site = b.create_site("B private", &sub("private")).await.unwrap();
    let doc = a
        .drive_create_file(
            &DriveLocation::Personal,
            None,
            &NewDriveFile {
                name: "Launch story".to_owned(),
                blob_id: "post-doc-a".to_owned(),
                kind: Some("doc".to_owned()),
                ..NewDriveFile::default()
            },
        )
        .await
        .unwrap();
    let ordinary_file = a
        .drive_create_file(
            &DriveLocation::Personal,
            None,
            &NewDriveFile {
                name: "Not a document".to_owned(),
                blob_id: "post-file-a".to_owned(),
                ..NewDriveFile::default()
            },
        )
        .await
        .unwrap();
    let b_doc = b
        .drive_create_file(
            &DriveLocation::Personal,
            None,
            &NewDriveFile {
                name: "B secret".to_owned(),
                blob_id: "post-doc-b".to_owned(),
                kind: Some("doc".to_owned()),
                ..NewDriveFile::default()
            },
        )
        .await
        .unwrap();

    assert_not_found(
        a.create_site_post(
            &site,
            &NewSitePost {
                doc_node_id: &b_doc,
                slug: "stolen",
                title: "Stolen",
                excerpt: "",
                cover_blob_id: None,
            },
        )
        .await,
    );
    match a
        .create_site_post(
            &site,
            &NewSitePost {
                doc_node_id: &ordinary_file,
                slug: "wrong-kind",
                title: "Wrong kind",
                excerpt: "",
                cover_blob_id: None,
            },
        )
        .await
    {
        Err(StoreError::Conflict(detail)) => assert!(detail.contains("alo document")),
        other => panic!("expected document-kind conflict, got {other:?}"),
    }

    let post = a
        .create_site_post(
            &site,
            &NewSitePost {
                doc_node_id: &doc,
                slug: "launch-story",
                title: " Launch story ",
                excerpt: " What we learned. ",
                cover_blob_id: None,
            },
        )
        .await
        .unwrap();
    let saved = a.site_post(&site, &post).await.unwrap().unwrap();
    assert_eq!(saved.doc_node_id.as_str(), doc.as_str());
    assert_eq!(saved.status, SitePostStatus::Draft);
    assert_eq!(saved.title, "Launch story");
    assert_eq!(saved.excerpt, "What we learned.");

    a.update_site_post(
        &site,
        &post,
        &SitePostUpdate {
            slug: "launch-notes",
            title: "Launch notes",
            excerpt: "Updated",
            cover_blob_id: None,
        },
    )
    .await
    .unwrap();
    a.publish_site_post(&site, &post).await.unwrap();
    let published = a.site_post(&site, &post).await.unwrap().unwrap();
    assert_eq!(published.status, SitePostStatus::Published);
    assert!(published.published_at.is_some());
    a.unpublish_site_post(&site, &post).await.unwrap();
    assert!(
        a.site_post(&site, &post)
            .await
            .unwrap()
            .unwrap()
            .published_at
            .is_none()
    );

    assert!(a.site_post(&other_site, &post).await.unwrap().is_none());
    assert_not_found(a.publish_site_post(&other_site, &post).await);
    assert!(b.site_posts(&site).await.unwrap().is_empty());
    assert!(b.site_post(&site, &post).await.unwrap().is_none());
    assert_not_found(b.publish_site_post(&site, &post).await);
    assert_not_found(b.unpublish_site_post(&site, &post).await);
    assert_not_found(b.delete_site_post(&site, &post).await);
    assert_not_found(
        b.create_site_post(
            &site,
            &NewSitePost {
                doc_node_id: &b_doc,
                slug: "injected",
                title: "Injected",
                excerpt: "",
                cover_blob_id: None,
            },
        )
        .await,
    );

    a.delete_site_post(&site, &post).await.unwrap();
    assert!(a.site_post(&site, &post).await.unwrap().is_none());
    assert!(a.drive_node(&doc).await.unwrap().is_some());
    assert!(b.site(&b_site).await.unwrap().is_some());
}
