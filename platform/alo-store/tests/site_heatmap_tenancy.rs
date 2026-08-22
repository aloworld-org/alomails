//! Owner heatmap reads stay inside the account door's tenant, the anonymous
//! write door counts events without ever holding an identity, and the one key
//! a visitor's browser names — the page path — is capped per site and day.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_store::{
    AccountStore, BlobStore, HEATMAP_DAILY_PATHS, HeatmapCell, HeatmapSignal,
    PublicSiteHeatmapReport, ScrollDepth, SiteId, SitePublicStore, Store, ViewportClass,
};
use sqlx::postgres::PgPoolOptions;
use time::{Date, Month};

/// The database this suite runs against.
///
/// Delegates to `alo_test_db`, which refuses the database the product
/// runs on: suites create and drop their own, they never write into `alo`.
fn database_url() -> String {
    alo_test_db::url()
}

async fn account(store: &Store, tag: &str) -> AccountStore {
    let tenant = store
        .create_tenant(&format!("heatmap-read-{tag}"))
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

fn click(
    day: Date,
    path: &str,
    viewport: ViewportClass,
    x: u16,
    y: u16,
) -> PublicSiteHeatmapReport<'_> {
    PublicSiteHeatmapReport {
        day,
        path,
        viewport,
        signal: HeatmapSignal::Click(HeatmapCell::from_permille(x, y)),
    }
}

fn scroll(
    day: Date,
    path: &str,
    viewport: ViewportClass,
    depth: u16,
) -> PublicSiteHeatmapReport<'_> {
    PublicSiteHeatmapReport {
        day,
        path,
        viewport,
        signal: HeatmapSignal::Scroll(ScrollDepth::from_permille(depth)),
    }
}

#[tokio::test]
async fn a_heatmap_counts_cells_and_never_leaves_its_tenant() {
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&database_url())
        .await
        .expect("connect to local postgres");
    let blobs = BlobStore::in_memory(1024 * 1024);
    let store = Store::new(pool.clone(), blobs.clone());
    store.migrate().await.unwrap();
    let public = SitePublicStore::new(pool.clone(), blobs);
    let owner_a = account(&store, "a").await;
    let owner_b = account(&store, "b").await;
    let (site_a, subdomain_a) = published(&owner_a, "alpha").await;
    let (site_b, subdomain_b) = published(&owner_b, "bravo").await;
    let resolved_a = public
        .resolve_published(&subdomain_a)
        .await
        .unwrap()
        .unwrap();
    let resolved_b = public
        .resolve_published(&subdomain_b)
        .await
        .unwrap()
        .unwrap();
    let first = Date::from_calendar_date(2026, Month::August, 10).unwrap();
    let second = Date::from_calendar_date(2026, Month::August, 11).unwrap();

    // Two readers on a phone hit the same region of the pricing page, a third
    // hits it elsewhere on a desktop, and two of them scroll to different
    // depths. The same day is written twice to prove hits accumulate.
    for report in [
        click(first, "/prices", ViewportClass::Phone, 500, 250),
        click(second, "/prices", ViewportClass::Phone, 505, 253),
        click(first, "/prices", ViewportClass::Desktop, 10, 990),
        click(first, "/about", ViewportClass::Phone, 0, 0),
        scroll(first, "/prices", ViewportClass::Phone, 880),
        scroll(second, "/prices", ViewportClass::Phone, 120),
    ] {
        public
            .record_public_site_heatmap(&resolved_a, &report)
            .await
            .unwrap();
    }
    // Tenant B's own site collects its own event on the same page path.
    public
        .record_public_site_heatmap(
            &resolved_b,
            &click(first, "/prices", ViewportClass::Phone, 500, 250),
        )
        .await
        .unwrap();

    let report = owner_a
        .site_heatmap(&site_a, "/prices", first, second)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((report.columns, report.rows), (32, 64));
    assert_eq!(report.viewports.len(), 3);
    let phone = &report.viewports[0];
    assert_eq!(phone.viewport, "phone");
    assert_eq!(
        phone
            .clicks
            .iter()
            .map(|cell| (cell.column, cell.row, cell.hits))
            .collect::<Vec<_>>(),
        vec![(16, 16, 2)],
        "two nearby clicks are one cell, and the positions are gone"
    );
    assert_eq!(phone.click_total, 2);
    assert_eq!(
        phone
            .scroll_depth
            .iter()
            .map(|bucket| (bucket.bucket, bucket.hits))
            .collect::<Vec<_>>(),
        vec![
            (0, 0),
            (1, 1),
            (2, 0),
            (3, 0),
            (4, 0),
            (5, 0),
            (6, 0),
            (7, 0),
            (8, 1),
            (9, 0)
        ],
        "the depth curve keeps its quiet tenths"
    );
    assert_eq!(phone.scroll_total, 2);
    let desktop = &report.viewports[2];
    assert_eq!(desktop.viewport, "desktop");
    assert_eq!(
        desktop
            .clicks
            .iter()
            .map(|cell| (cell.column, cell.row, cell.hits))
            .collect::<Vec<_>>(),
        vec![(0, 63, 1)]
    );
    assert_eq!(report.viewports[1].click_total, 0, "nothing on a tablet");

    // One day alone is a narrower report, not a different one.
    let first_only = owner_a
        .site_heatmap(&site_a, "/prices", first, first)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first_only.viewports[0].click_total, 1);

    // The page menu is ranked by how much there is to look at.
    let paths = owner_a
        .site_heatmap_paths(&site_a, first, second)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        paths
            .iter()
            .map(|row| (row.path.as_str(), row.events))
            .collect::<Vec<_>>(),
        vec![("/prices", 5), ("/about", 1)]
    );

    // Tenant A cannot resolve tenant B's site at all, in either read.
    assert!(
        owner_a
            .site_heatmap(&site_b, "/prices", first, second)
            .await
            .unwrap()
            .is_none(),
        "tenant A read tenant B's heatmap"
    );
    assert!(
        owner_a
            .site_heatmap_paths(&site_b, first, second)
            .await
            .unwrap()
            .is_none(),
        "tenant A listed tenant B's heatmap pages"
    );
    // And tenant B's own report holds only tenant B's single event, on the
    // page path both tenants happen to share.
    let report_b = owner_b
        .site_heatmap(&site_b, "/prices", first, second)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        report_b.viewports[0].click_total, 1,
        "tenant A's clicks leaked into tenant B's heatmap"
    );
    assert_eq!(report_b.viewports[0].scroll_total, 0);

    // An owned site with nothing on this page is an empty grid, not a miss:
    // "not yours" and "nothing yet" are different answers.
    let quiet = owner_a
        .site_heatmap(&site_a, "/never-visited", first, second)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(quiet.path, "/never-visited");
    assert!(quiet.viewports.iter().all(|view| view.click_total == 0));

    // The stored schema can hold no identity: these are all the columns there
    // are, and none of them is a visitor, a session, an address, or a time.
    let columns = sqlx::query_scalar::<_, String>(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_name = 'site_analytics_heatmap_daily' ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        columns,
        vec![
            "day",
            "grid_x",
            "grid_y",
            "hits",
            "metric",
            "path",
            "site_id",
            "tenant_id",
            "viewport"
        ],
        "the heatmap table grew a column that can identify a visitor"
    );
}

/// The page path is the one heatmap key a visitor's browser names freely, so
/// the number of distinct pages one site can open in a day is capped. Past the
/// ceiling a new page is dropped rather than folded into an overflow bucket —
/// a heatmap of "some other page" would be an overlay over nothing — while
/// pages already open keep counting.
#[tokio::test]
async fn heatmap_pages_are_bounded_per_site_and_day() {
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

    for index in 0..(HEATMAP_DAILY_PATHS * 2) {
        public
            .record_public_site_heatmap(
                &resolved,
                &click(
                    day,
                    &format!("/page-{index}"),
                    ViewportClass::Phone,
                    500,
                    500,
                ),
            )
            .await
            .unwrap();
    }

    let distinct = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(DISTINCT path) FROM site_analytics_heatmap_daily \
         WHERE site_id = $1 AND day = $2",
    )
    .bind(site.as_str())
    .bind(day)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        (HEATMAP_DAILY_PATHS..=HEATMAP_DAILY_PATHS + 1).contains(&distinct),
        "the day kept {distinct} distinct heatmap pages"
    );

    // A page opened before the cap keeps counting after it.
    public
        .record_public_site_heatmap(
            &resolved,
            &click(day, "/page-0", ViewportClass::Phone, 500, 500),
        )
        .await
        .unwrap();
    let report = owner
        .site_heatmap(&site, "/page-0", day, day)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(report.viewports[0].click_total, 2);

    // A path that is not a page path is refused by the door, not stored.
    assert!(
        public
            .record_public_site_heatmap(
                &resolved,
                &click(
                    day,
                    "https://elsewhere.example/x",
                    ViewportClass::Phone,
                    1,
                    1
                ),
            )
            .await
            .is_err(),
        "a path that is not a page path was accepted"
    );
}
