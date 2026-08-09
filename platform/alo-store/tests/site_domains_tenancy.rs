//! Custom-domain claims never cross the account door.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_store::{
    AccountStore, BlobStore, SiteDomainStatus, SiteId, SitePublicStore, Store, StoreError,
};
use sqlx::postgres::PgPoolOptions;

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://alo:alo-dev-only@127.0.0.1:5432/alo".to_owned())
}

async fn account(store: &Store, tag: &str) -> AccountStore {
    let tenant = store
        .create_tenant(&format!("site-domain-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}-{tenant}@example.test"))
        .await
        .unwrap();
    store.for_account(tenant, user)
}

async fn site(account: &AccountStore, tag: &str) -> SiteId {
    let suffix = SiteId::generate()
        .as_str()
        .to_ascii_lowercase()
        .replace('_', "-");
    account
        .create_site(tag, &format!("{tag}-{suffix}"))
        .await
        .unwrap()
}

fn domain(tag: &str) -> String {
    let suffix = SiteId::generate()
        .as_str()
        .to_ascii_lowercase()
        .replace('_', "-");
    format!("{tag}-{suffix}.example.test")
}

fn assert_not_found<T>(result: Result<T, StoreError>) {
    assert!(matches!(result, Err(StoreError::NotFound)));
}

#[tokio::test]
async fn claims_transition_deliberately_and_foreign_tenants_see_nothing() {
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&database_url())
        .await
        .expect("connect to local postgres");
    let blobs = BlobStore::in_memory(1024 * 1024);
    let public = SitePublicStore::new(pool.clone(), blobs.clone());
    let store = Store::new(pool, blobs);
    store.migrate().await.unwrap();
    let owner_a = account(&store, "a").await;
    let owner_b = account(&store, "b").await;
    let site_a = site(&owner_a, "alpha").await;
    let site_b = site(&owner_b, "bravo").await;
    let host_a = domain("alpha");
    owner_a
        .create_site_page(&site_a, "Home", "", true)
        .await
        .unwrap();
    owner_a.publish_site(&site_a).await.unwrap();

    let claim = owner_a
        .create_site_domain(&site_a, &host_a.to_ascii_uppercase())
        .await
        .unwrap();
    assert_eq!(claim.domain, host_a);
    assert_eq!(claim.status, SiteDomainStatus::Pending);
    assert!(claim.verified_at.is_none());
    assert!(claim.verify_token.len() >= 20);
    assert!(
        public
            .resolve_custom_published(&host_a)
            .await
            .unwrap()
            .is_none()
    );

    let listed = owner_a.site_domains(&site_a).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].domain, host_a);
    assert!(matches!(
        owner_a.activate_site_domain(&site_a, &host_a).await,
        Err(StoreError::Conflict(message)) if message.contains("verified")
    ));

    let verified = owner_a.verify_site_domain(&site_a, &host_a).await.unwrap();
    assert_eq!(verified.status, SiteDomainStatus::Verified);
    assert!(verified.verified_at.is_some());
    assert!(
        public
            .resolve_custom_published(&host_a)
            .await
            .unwrap()
            .is_none()
    );
    let live = owner_a
        .activate_site_domain(&site_a, &host_a)
        .await
        .unwrap();
    assert_eq!(live.status, SiteDomainStatus::Live);
    let resolved = public
        .resolve_custom_published(&host_a)
        .await
        .unwrap()
        .expect("live custom host resolves");
    assert_eq!(resolved.site, site_a);
    assert_ne!(resolved.site, site_b, "Host lookup cannot cross tenants");

    assert_not_found(owner_a.site_domains(&site_b).await);
    assert_not_found(
        owner_a
            .create_site_domain(&site_b, &domain("foreign"))
            .await,
    );
    assert_not_found(owner_b.verify_site_domain(&site_a, &host_a).await);
    assert_not_found(owner_b.activate_site_domain(&site_a, &host_a).await);
    assert_not_found(owner_b.delete_site_domain(&site_a, &host_a).await);

    let collision = owner_b.create_site_domain(&site_b, &host_a).await;
    assert!(matches!(
        collision,
        Err(StoreError::Conflict(message)) if message == "domain is already connected"
    ));
    assert_eq!(owner_a.site_domains(&site_a).await.unwrap().len(), 1);
    assert!(owner_b.site_domains(&site_b).await.unwrap().is_empty());

    owner_a.delete_site_domain(&site_a, &host_a).await.unwrap();
    assert!(
        public
            .resolve_custom_published(&host_a)
            .await
            .unwrap()
            .is_none()
    );
    assert!(owner_a.site_domains(&site_a).await.unwrap().is_empty());
    let reclaimed = owner_b.create_site_domain(&site_b, &host_a).await.unwrap();
    assert_eq!(reclaimed.status, SiteDomainStatus::Pending);
}
