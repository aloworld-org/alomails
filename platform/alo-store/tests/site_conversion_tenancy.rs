//! Conversion counts stay inside the tenant that owns the conversion point,
//! the anonymous doors count stages without ever holding an identity, and the
//! only id involved belongs to the site rather than to a visitor.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_store::{
    AccountStore, BlobStore, ConversionStage, SiteFormId, SiteId, SitePublicStore, Store,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::{Date, Month};

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://alo:alo-dev-only@127.0.0.1:5432/alo".to_owned())
}

async fn account(store: &Store, tag: &str) -> AccountStore {
    let tenant = store
        .create_tenant(&format!("conversion-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}-{tenant}@example.test"))
        .await
        .unwrap();
    store.for_account(tenant, user)
}

/// A published site with one contact form on it — the conversion point.
async fn published(acc: &AccountStore, tag: &str) -> (SiteId, String, SiteFormId) {
    let suffix = SiteId::generate()
        .as_str()
        .to_ascii_lowercase()
        .replace('_', "-");
    let subdomain = format!("{tag}-{suffix}");
    let site = acc.create_site(tag, &subdomain).await.unwrap();
    acc.create_site_page(&site, "Home", "", true).await.unwrap();
    let form = acc.create_site_form(&site, "Contact").await.unwrap();
    acc.publish_site(&site).await.unwrap();
    (site, subdomain, form)
}

/// Every bucket a site has opened, summed over its days — one row per
/// (source, stage), so a new bucket shows up as a new row rather than hiding
/// inside a count.
async fn stored_rows(pool: &PgPool, site: &SiteId) -> Vec<(String, String, String, i64)> {
    sqlx::query_as::<_, (String, String, String, i64)>(
        "SELECT source_kind, source_id, stage, SUM(hits)::BIGINT \
         FROM site_conversion_daily WHERE site_id = $1 \
         GROUP BY source_kind, source_id, stage ORDER BY source_id, stage",
    )
    .bind(site.as_str())
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_funnel_counts_stages_and_never_leaves_its_tenant() {
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
    let (site_a, subdomain_a, form_a) = published(&owner_a, "alpha").await;
    let (site_b, subdomain_b, form_b) = published(&owner_b, "bravo").await;
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

    // Three visitors saw the form over two days, one of them started filling
    // it in, and one submission was written.
    for (day, stage) in [
        (first, ConversionStage::View),
        (first, ConversionStage::View),
        (second, ConversionStage::View),
        (second, ConversionStage::Start),
    ] {
        assert!(
            public
                .record_public_site_conversion(&resolved_a, day, form_a.as_str(), stage)
                .await
                .unwrap()
        );
    }
    assert!(
        public
            .record_public_form_conversion(form_a.as_str(), second, ConversionStage::Submit)
            .await
            .unwrap()
    );

    // Tenant B's own site collects its own view of its own form.
    assert!(
        public
            .record_public_site_conversion(
                &resolved_b,
                first,
                form_b.as_str(),
                ConversionStage::View
            )
            .await
            .unwrap()
    );

    let report = owner_a
        .site_conversions(&site_a, first, second)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((report.views, report.starts, report.submits), (3, 1, 1));
    assert_eq!(report.sources.len(), 1);
    let source = &report.sources[0];
    assert_eq!(source.kind, "form");
    assert_eq!(source.id, form_a.as_str());
    assert_eq!(source.name.as_deref(), Some("Contact"));
    assert_eq!((source.views, source.starts, source.submits), (3, 1, 1));

    // One day alone is a narrower report, not a different one.
    let first_only = owner_a
        .site_conversions(&site_a, first, first)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (first_only.views, first_only.starts, first_only.submits),
        (2, 0, 0)
    );

    // Tenant A cannot read tenant B's funnel at all.
    assert!(
        owner_a
            .site_conversions(&site_b, first, second)
            .await
            .unwrap()
            .is_none(),
        "tenant A read tenant B's conversions"
    );
    // And tenant B's own report holds only tenant B's single view.
    let report_b = owner_b
        .site_conversions(&site_b, first, second)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (report_b.views, report_b.starts, report_b.submits),
        (1, 0, 0)
    );

    // The write door is scoped by the site the Host resolved, so tenant B's
    // form id — which is public in tenant B's own markup — cannot be counted
    // against tenant A's site, and nothing is written either way.
    assert!(
        !public
            .record_public_site_conversion(
                &resolved_a,
                first,
                form_b.as_str(),
                ConversionStage::View
            )
            .await
            .unwrap(),
        "tenant B's form was counted on tenant A's site"
    );
    for invented in ["not-a-real-form-id", "", "'; DROP TABLE sites; --"] {
        assert!(
            !public
                .record_public_site_conversion(&resolved_a, first, invented, ConversionStage::View)
                .await
                .unwrap()
        );
        assert!(
            !public
                .record_public_form_conversion(invented, first, ConversionStage::Submit)
                .await
                .unwrap()
        );
    }
    assert_eq!(
        stored_rows(&pool, &site_a).await,
        vec![
            (
                "form".to_owned(),
                form_a.as_str().to_owned(),
                "start".to_owned(),
                1
            ),
            (
                "form".to_owned(),
                form_a.as_str().to_owned(),
                "submit".to_owned(),
                1
            ),
            (
                "form".to_owned(),
                form_a.as_str().to_owned(),
                "view".to_owned(),
                3
            ),
        ],
        "an invented or foreign source opened a bucket"
    );

    // A site of this tenant with nothing collected answers zeroes rather than
    // `None`: "not yours" and "nothing yet" are different answers, and a form
    // nobody has reached is listed so the owner can see that it is quiet.
    let (quiet_site, _, quiet_form) = published(&owner_a, "quiet").await;
    let quiet = owner_a
        .site_conversions(&quiet_site, first, second)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((quiet.views, quiet.starts, quiet.submits), (0, 0, 0));
    assert_eq!(quiet.sources.len(), 1);
    assert_eq!(quiet.sources[0].id, quiet_form.as_str());
    assert_eq!(quiet.sources[0].views, 0);

    // Deleting the form keeps its record: the counts stay, named as an
    // unnamed source, rather than rewriting what last week reported.
    owner_a.delete_site_form(&site_a, &form_a).await.unwrap();
    let after = owner_a
        .site_conversions(&site_a, first, second)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((after.views, after.starts, after.submits), (3, 1, 1));
    assert_eq!(after.sources.len(), 1);
    assert_eq!(after.sources[0].name, None);

    // The stored schema can hold no identity: these are all the columns there
    // are, and none of them is a visitor, a session, an address, or a time.
    let columns = sqlx::query_scalar::<_, String>(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_name = 'site_conversion_daily' ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        columns,
        vec![
            "day",
            "hits",
            "site_id",
            "source_id",
            "source_kind",
            "stage",
            "tenant_id"
        ],
        "the conversion table grew a column that can identify a visitor"
    );
}

/// The submit door counts what could actually have been submitted: a form on a
/// site that is not live is not writable, so it must not be countable either.
#[tokio::test]
async fn a_draft_site_s_form_counts_no_submit() {
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&database_url())
        .await
        .expect("connect to local postgres");
    let blobs = BlobStore::in_memory(1024 * 1024);
    let store = Store::new(pool.clone(), blobs.clone());
    store.migrate().await.unwrap();
    let public = SitePublicStore::new(pool.clone(), blobs);
    let owner = account(&store, "draft").await;
    let suffix = SiteId::generate()
        .as_str()
        .to_ascii_lowercase()
        .replace('_', "-");
    let site = owner
        .create_site("Draft", &format!("draft-{suffix}"))
        .await
        .unwrap();
    owner
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    let form = owner.create_site_form(&site, "Contact").await.unwrap();
    let day = Date::from_calendar_date(2026, Month::August, 10).unwrap();

    assert!(
        !public
            .record_public_form_conversion(form.as_str(), day, ConversionStage::Submit)
            .await
            .unwrap(),
        "a draft site's form counted a submit"
    );
    assert!(stored_rows(&pool, &site).await.is_empty());

    // Once the site is live the same call counts, exactly as the submission
    // write becomes possible at the same moment.
    owner.publish_site(&site).await.unwrap();
    assert!(
        public
            .record_public_form_conversion(form.as_str(), day, ConversionStage::Submit)
            .await
            .unwrap()
    );
    assert_eq!(
        stored_rows(&pool, &site).await,
        vec![(
            "form".to_owned(),
            form.as_str().to_owned(),
            "submit".to_owned(),
            1
        )]
    );
}
