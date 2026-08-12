//! Owner analytics reads stay inside the account door's tenant, while the
//! report rolls the anonymous public aggregates into useful daily rankings.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_store::{
    AccountStore, BlobStore, DeviceClass, PublicSiteVisit, SiteId, SitePublicStore, Store,
};
use sqlx::postgres::PgPoolOptions;
use time::{Date, Month};

/// A view with the dimensions a real request would have reduced to.
fn visit<'a>(
    day: Date,
    path: &'a str,
    referrer_domain: &'a str,
    campaign: &'a str,
    country: &'a str,
    device: DeviceClass,
    visitor_hash: &'a [u8; 32],
) -> PublicSiteVisit<'a> {
    PublicSiteVisit {
        day,
        path,
        referrer_domain,
        campaign,
        country,
        device,
        visitor_hash,
    }
}

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

    for view in [
        visit(
            first,
            "/",
            "",
            "spring",
            "NL",
            DeviceClass::Phone,
            &visitor_a,
        ),
        visit(
            first,
            "/",
            "",
            "spring",
            "NL",
            DeviceClass::Phone,
            &visitor_a,
        ),
        visit(
            first,
            "/about",
            "news.example",
            "",
            "NL",
            DeviceClass::Phone,
            &visitor_a,
        ),
        visit(
            second,
            "/",
            "news.example",
            "",
            "BE",
            DeviceClass::Desktop,
            &visitor_b,
        ),
    ] {
        public
            .record_public_site_view(&published_a, &view)
            .await
            .unwrap();
    }
    public
        .record_public_site_view(
            &published_b,
            &visit(first, "/", "", "", "NL", DeviceClass::Phone, &visitor_a),
        )
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

    // Second-generation dimensions: counted per view, ranked, and never
    // holding more than the bucket they name.
    let labelled = |rows: &[alo_store::SiteAnalyticsDimension]| {
        rows.iter()
            .map(|row| (row.label.clone(), row.visits))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        labelled(&report.campaigns),
        vec![(String::new(), 2), ("spring".to_owned(), 2)],
        "an unlabelled visit is its own bucket, not a missing one"
    );
    assert_eq!(
        labelled(&report.countries),
        vec![("NL".to_owned(), 3), ("BE".to_owned(), 1)]
    );
    assert_eq!(
        labelled(&report.devices),
        vec![("phone".to_owned(), 3), ("desktop".to_owned(), 1)]
    );
    assert_eq!(
        labelled(&report.entry_pages),
        vec![("/".to_owned(), 2)],
        "each visitor-day contributes exactly one entry"
    );
    assert_eq!(
        labelled(&report.exit_pages),
        vec![("/".to_owned(), 1), ("/about".to_owned(), 1)]
    );

    // On the first day alone the visitor moved on from "/", so "/" is not an
    // exit that day and does not linger at zero in the report.
    let first_day = owner_a
        .site_analytics(&site_a, first, first)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        labelled(&first_day.exit_pages),
        vec![("/about".to_owned(), 1)]
    );
    assert_eq!(labelled(&first_day.entry_pages), vec![("/".to_owned(), 1)]);

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
    assert_eq!(
        labelled(&report_b.campaigns),
        vec![(String::new(), 1)],
        "tenant A's campaign leaked into tenant B's report"
    );
    assert_eq!(labelled(&report_b.entry_pages), vec![("/".to_owned(), 1)]);
}
