//! Owner analytics reads stay inside the account door's tenant, while the
//! report rolls the anonymous public aggregates into useful daily rankings.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_store::{
    AccountStore, BlobStore, DeviceClass, OUTBOUND_OVERFLOW, PublicSiteSignal, PublicSiteVisit,
    ReadTimeBucket, SiteId, SitePublicStore, Store,
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

/// The database this suite runs against.
///
/// Delegates to `alo_test_db`, which refuses the database the product
/// runs on: suites create and drop their own, they never write into `alo`.
fn database_url() -> String {
    alo_test_db::url()
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

    // What the published page's beacon reported, on both tenants' sites.
    for signal in [
        PublicSiteSignal::ReadTime(ReadTimeBucket::from_seconds(240)),
        PublicSiteSignal::ReadTime(ReadTimeBucket::from_seconds(5)),
        PublicSiteSignal::ReadTime(ReadTimeBucket::from_seconds(5)),
        PublicSiteSignal::Outbound("shop.example"),
        PublicSiteSignal::Outbound("shop.example"),
        PublicSiteSignal::Outbound("directory.example"),
    ] {
        public
            .record_public_site_signal(&published_a, first, signal)
            .await
            .unwrap();
    }
    public
        .record_public_site_signal(
            &published_b,
            first,
            PublicSiteSignal::ReadTime(ReadTimeBucket::from_seconds(900)),
        )
        .await
        .unwrap();
    // A value that is not a bounded DNS host never reaches a bucket, even if
    // something upstream of the collect endpoint forgets to fold it.
    for hostile in [
        "",
        "localhost",
        "news.example/path",
        "NEWS.example",
        &"a".repeat(300),
    ] {
        assert!(
            public
                .record_public_site_signal(&published_a, first, PublicSiteSignal::Outbound(hostile))
                .await
                .is_err(),
            "{hostile:?} was accepted as an outbound domain"
        );
    }

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

    // Beacon dimensions: read time in bucket order (not by count — the two
    // short reads outnumber the long one and still come second), destinations
    // ranked like every other list.
    assert_eq!(
        labelled(&report.read_time),
        vec![("0-10s".to_owned(), 2), ("3-10m".to_owned(), 1)]
    );
    assert_eq!(
        labelled(&report.outbound),
        vec![
            ("shop.example".to_owned(), 2),
            ("directory.example".to_owned(), 1)
        ]
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
    assert_eq!(
        labelled(&report_b.read_time),
        vec![("10m+".to_owned(), 1)],
        "tenant A's read times leaked into tenant B's report"
    );
    assert!(
        report_b.outbound.is_empty(),
        "tenant A's outbound domains leaked into tenant B's report"
    );
}

/// Outbound domains are the one dimension a *visitor's browser* names, so the
/// number of distinct ones a site can accumulate in a day is capped: past the
/// ceiling, new destinations are counted under one overflow bucket instead of
/// turning an aggregate into a data dump. Existing destinations keep counting.
#[tokio::test]
async fn outbound_domains_are_bounded_per_site_and_day() {
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&database_url())
        .await
        .expect("connect to local postgres");
    let blobs = BlobStore::in_memory(1024 * 1024);
    let store = Store::new(pool.clone(), blobs.clone());
    store.migrate().await.unwrap();
    let public = SitePublicStore::new(pool.clone(), blobs);
    let owner = account(&store, "flood").await;
    let (site, subdomain) = published(&owner, "flood").await;
    let resolved = public.resolve_published(&subdomain).await.unwrap().unwrap();
    let day = Date::from_calendar_date(2026, Month::August, 10).unwrap();

    // Far more distinct destinations in one day than a real site produces.
    for index in 0..320 {
        public
            .record_public_site_signal(
                &resolved,
                day,
                PublicSiteSignal::Outbound(&format!("host-{index}.example")),
            )
            .await
            .unwrap();
    }

    let distinct = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM site_analytics_dimension_daily \
         WHERE site_id = $1 AND day = $2 AND dimension = 'outbound'",
    )
    .bind(site.as_str())
    .bind(day)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        (200..=201).contains(&distinct),
        "the day kept {distinct} distinct outbound buckets"
    );

    let report = owner
        .site_analytics(&site, day, day)
        .await
        .unwrap()
        .unwrap();
    let overflow = report
        .outbound
        .iter()
        .find(|row| row.label == OUTBOUND_OVERFLOW)
        .expect("the overflow bucket must be reported, not hidden");
    assert_eq!(
        overflow.visits, 120,
        "every destination past the cap is still counted, just not named"
    );
    assert!(
        overflow.label != "host-0.example" && !overflow.label.contains('.'),
        "the overflow bucket cannot be mistaken for a domain"
    );

    // A destination the site already knows keeps counting past the cap.
    public
        .record_public_site_signal(&resolved, day, PublicSiteSignal::Outbound("host-0.example"))
        .await
        .unwrap();
    let known = sqlx::query_scalar::<_, i64>(
        "SELECT hits FROM site_analytics_dimension_daily \
         WHERE site_id = $1 AND day = $2 AND dimension = 'outbound' AND value = 'host-0.example'",
    )
    .bind(site.as_str())
    .bind(day)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(known, 2);
}
