//! Owner analytics reads stay inside the account door's tenant, while the
//! report rolls the anonymous public aggregates into useful daily rankings.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_store::{AccountStore, BlobStore, SiteId, SitePublicStore, Store};
use sqlx::postgres::PgPoolOptions;
use time::{Date, Month};

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://alo:alo-dev-only@127.0.0.1:5432/alo".to_owned())
}

async fn account(store: &Store, tag: &str) -> AccountStore {
    let tenant = store
        .create_tenant(&format!("analytics-read-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}-{tenant}@example.test"))
        .await
        .unwrap();
    store.for_account(tenant, user)
}

async fn published(acc: &AccountStore, tag: &str) -> (SiteId, String) {
    let suffix = SiteId::generate()
        .as_str()
        .to_ascii_lowercase()
        .replace('_', "-");
    let subdomain = format!("{tag}-{suffix}");
    let site = acc.create_site(tag, &subdomain).await.unwrap();
    acc.create_site_page(&site, "Home", "", true).await.unwrap();
    acc.publish_site(&site).await.unwrap();
    (site, subdomain)
}

#[tokio::test]
async fn report_groups_useful_dimensions_and_hides_foreign_sites() {
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&database_url())
        .await
        .expect("connect to local postgres");
    let blobs = BlobStore::in_memory(1024 * 1024);
    let store = Store::new(pool.clone(), blobs.clone());
    store.migrate().await.unwrap();
    let public = SitePublicStore::new(pool, blobs);
    let owner_a = account(&store, "a").await;
    let owner_b = account(&store, "b").await;
    let (site_a, subdomain_a) = published(&owner_a, "alpha").await;
    let (site_b, subdomain_b) = published(&owner_b, "bravo").await;
    let published_a = public
        .resolve_published(&subdomain_a)
        .await
        .unwrap()
        .unwrap();
    let published_b = public
        .resolve_published(&subdomain_b)
        .await
        .unwrap()
        .unwrap();
    let first = Date::from_calendar_date(2026, Month::August, 8).unwrap();
    let second = Date::from_calendar_date(2026, Month::August, 9).unwrap();
    let visitor_a = [1_u8; 32];
    let visitor_b = [2_u8; 32];

    public
        .record_public_site_view(&published_a, first, "/", "", &visitor_a)
        .await
        .unwrap();
    public
        .record_public_site_view(&published_a, first, "/", "", &visitor_a)
        .await
        .unwrap();
    public
        .record_public_site_view(&published_a, first, "/about", "news.example", &visitor_a)
        .await
        .unwrap();
    public
        .record_public_site_view(&published_a, second, "/", "news.example", &visitor_b)
        .await
        .unwrap();
    public
        .record_public_site_view(&published_b, first, "/", "", &visitor_a)
        .await
        .unwrap();

    let report = owner_a
        .site_analytics(&site_a, first, second)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(report.daily.len(), 2);
    assert_eq!(
        (report.daily[0].visits, report.daily[0].unique_visitors),
        (3, 1)
    );
    assert_eq!(
        (report.daily[1].visits, report.daily[1].unique_visitors),
        (1, 1)
    );
    assert_eq!(report.top_pages[0].label, "/");
    assert_eq!(
        (
            report.top_pages[0].visits,
            report.top_pages[0].unique_visitors
        ),
        (3, 2)
    );
    assert_eq!(report.top_pages[1].label, "/about");
    assert_eq!(report.top_referrers[0].label, "");
    assert_eq!(report.top_referrers[0].visits, 2);
    assert_eq!(report.top_referrers[1].label, "news.example");

    assert!(
        owner_a
            .site_analytics(&site_b, first, second)
            .await
            .unwrap()
            .is_none(),
        "tenant A resolved tenant B's report"
    );
    let report_b = owner_b
        .site_analytics(&site_b, first, second)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(report_b.daily.len(), 1);
    assert_eq!(report_b.daily[0].visits, 1);
}
