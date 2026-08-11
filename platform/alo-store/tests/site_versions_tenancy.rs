//! Website version history (ADR 0036, S2.04a): the history read, the
//! comparison, and the restore that puts an earlier version back online.
//!
//! Two properties are load-bearing and are proved here against a real
//! Postgres: **history is immutable** — a restore appends a copy and never
//! rewrites, re-points, or deletes an earlier publish, and it never touches
//! the editable draft — and **history is tenant-scoped**: another tenant's
//! version is unreadable, uncomparable, and unrestorable, and is
//! indistinguishable from a version that never existed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    SiteCollectionId, SiteId, SitePublishId, SiteStatus, SiteVersionChange, StoreError,
};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;

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

fn hero(heading: &str) -> serde_json::Value {
    json!({"schema_version": 1, "sections": [{"type": "hero", "heading": heading}]})
}

/// The whole arc a version history exists for: publish, change, publish
/// again, see both, compare them, and put the first one back — with every
/// earlier version and the editable draft surviving untouched.
#[tokio::test]
async fn history_lists_every_version_and_restoring_appends_a_copy() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("site-versions").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("owner@site-versions.test")
        .await
        .unwrap();
    let account = store.for_account(tenant, user.clone());
    let site = account
        .create_site("Acme", &subdomain("versions"))
        .await
        .unwrap();
    let home = account
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    account
        .set_page_sections(&site, &home, hero("First"))
        .await
        .unwrap();
    account
        .set_site_theme(&site, json!({"schema_version": 1, "preset": "terra"}))
        .await
        .unwrap();

    // ---- an unpublished site has no history ---------------------------------
    assert!(
        account
            .site_publish_history(&site, 50)
            .await
            .unwrap()
            .is_empty()
    );

    let first = account.publish_site(&site).await.unwrap();

    // ---- the second version: an edit, a new page, another theme -------------
    account
        .set_page_sections(&site, &home, hero("Second"))
        .await
        .unwrap();
    account
        .set_page_title(&site, &home, "Welcome")
        .await
        .unwrap();
    let about = account
        .create_site_page(&site, "About", "about", false)
        .await
        .unwrap();
    account
        .set_site_theme(&site, json!({"schema_version": 1, "preset": "ink"}))
        .await
        .unwrap();
    let second = account.publish_site(&site).await.unwrap();

    // ---- the history reads newest first, and names the live version ---------
    let history = account.site_publish_history(&site, 50).await.unwrap();
    assert_eq!(
        history.iter().map(|v| v.id.clone()).collect::<Vec<_>>(),
        vec![second.clone(), first.clone()]
    );
    assert!(history[0].is_current);
    assert!(!history[1].is_current);
    assert_eq!(history[0].pages, 2);
    assert_eq!(history[1].pages, 1);
    assert_eq!(history[0].locales, vec!["en".to_owned()]);
    assert_eq!(history[0].published_by, user.as_str());
    assert_eq!(history[0].collections, 0);
    assert!(
        history
            .iter()
            .all(|version| version.restored_from.is_none())
    );

    // A limit outside the allowed band is clamped, never refused.
    assert_eq!(
        account.site_publish_history(&site, 1).await.unwrap().len(),
        1
    );
    assert_eq!(
        account.site_publish_history(&site, 0).await.unwrap().len(),
        1,
        "a nonsensical limit still answers a usable list"
    );
    assert_eq!(
        account
            .site_publish_history(&site, i64::MAX)
            .await
            .unwrap()
            .len(),
        2
    );

    // ---- comparing the two versions -----------------------------------------
    let diff = account
        .compare_site_publishes(&site, &first, &second)
        .await
        .unwrap();
    assert!(!diff.is_identical());
    assert!(diff.theme_changed);
    assert!(!diff.default_locale_changed);
    assert!(diff.locales_added.is_empty() && diff.locales_removed.is_empty());
    assert_eq!(diff.unchanged_pages, 0);
    assert_eq!(diff.pages.len(), 2);
    let changed = diff
        .pages
        .iter()
        .find(|page| page.page_id == home)
        .expect("the home page changed");
    assert_eq!(changed.change, SiteVersionChange::Changed);
    assert_eq!(changed.title, "Welcome", "the newer name is the one shown");
    assert_eq!(
        changed
            .fields
            .iter()
            .map(|field| field.as_str())
            .collect::<Vec<_>>(),
        vec!["title", "sections"]
    );
    let added = diff
        .pages
        .iter()
        .find(|page| page.page_id == about)
        .expect("the About page was added");
    assert_eq!(added.change, SiteVersionChange::Added);
    assert!(added.fields.is_empty());

    // Comparing a version with itself is the honest empty answer.
    let same = account
        .compare_site_publishes(&site, &second, &second)
        .await
        .unwrap();
    assert!(same.is_identical());
    assert_eq!(same.unchanged_pages, 2);
    assert!(same.pages.is_empty());

    // Read the other way around and the same difference is reported from the
    // other side: the added page is now a removed one.
    let reversed = account
        .compare_site_publishes(&site, &second, &first)
        .await
        .unwrap();
    assert_eq!(
        reversed
            .pages
            .iter()
            .find(|page| page.page_id == about)
            .unwrap()
            .change,
        SiteVersionChange::Removed
    );

    // ---- restoring the first version ---------------------------------------
    let restored = account.restore_site_publish(&site, &first).await.unwrap();
    assert_ne!(restored, first, "a restore appends, it never re-points");
    let current = account
        .current_site_publish(&site)
        .await
        .unwrap()
        .expect("the site is live again on the restored version");
    assert_eq!(current.id, restored);
    assert_eq!(
        account.site(&site).await.unwrap().unwrap().status,
        SiteStatus::Live
    );
    let version = account
        .site_publish_version(&site, &restored)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(version.restored_from, Some(first.clone()));
    assert!(version.is_current);
    assert_eq!(version.published_by, user.as_str());

    // The restored set is the frozen one, page for page and byte for byte.
    let source_pages = account.site_publish_snapshots(&site, &first).await.unwrap();
    let restored_pages = account
        .site_publish_snapshots(&site, &restored)
        .await
        .unwrap();
    assert_eq!(restored_pages.len(), source_pages.len());
    for (was, now) in source_pages.iter().zip(restored_pages.iter()) {
        assert_eq!(was.page_id, now.page_id);
        assert_eq!(was.locale, now.locale);
        assert_eq!(was.slug, now.slug);
        assert_eq!(was.title, now.title);
        assert_eq!(was.sections, now.sections);
        assert_eq!(was.is_home, now.is_home);
    }
    assert!(
        account
            .compare_site_publishes(&site, &first, &restored)
            .await
            .unwrap()
            .is_identical(),
        "the copy is indistinguishable from what it copied"
    );

    // Every earlier version survives, unchanged, and history grew by one.
    let history = account.site_publish_history(&site, 50).await.unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].id, restored);
    assert!(history.iter().filter(|version| version.is_current).count() == 1);
    let still_second = account
        .site_publish_snapshots(&site, &second)
        .await
        .unwrap();
    assert_eq!(still_second.len(), 2);
    assert!(still_second.iter().any(|page| page.title == "Welcome"));

    // The editable draft is the tenant's work, not the restore's to rewrite.
    let draft = account.site_page(&site, &home).await.unwrap().unwrap();
    assert_eq!(draft.title, "Welcome");
    assert_eq!(draft.sections, hero("Second"));
    assert_eq!(account.site_pages(&site).await.unwrap().len(), 2);

    // A restore brings an offline site back online.
    account.unpublish_site(&site).await.unwrap();
    assert!(account.current_site_publish(&site).await.unwrap().is_none());
    let again = account.restore_site_publish(&site, &second).await.unwrap();
    assert_eq!(
        account
            .current_site_publish(&site)
            .await
            .unwrap()
            .unwrap()
            .id,
        again
    );
    assert_eq!(
        account.site(&site).await.unwrap().unwrap().status,
        SiteStatus::Live
    );
    assert_eq!(
        account.site_publish_history(&site, 50).await.unwrap().len(),
        4
    );

    // Provenance points from one publish to another inside the same table;
    // deleting the site must still take the whole history with it.
    account.delete_site(&site).await.unwrap();
    assert!(
        account
            .site_publish_history(&site, 50)
            .await
            .unwrap()
            .is_empty()
    );
}

/// A version of another tenant — or of another site of the same tenant — is
/// unreadable, uncomparable, and unrestorable, and reads exactly like a
/// version that never existed.
#[tokio::test]
async fn versions_scope_by_tenant_and_by_site() {
    let store = common::test_store().await;
    let t1 = store.create_tenant("versions-t1").await.unwrap();
    let u1 = store
        .for_tenant(t1.clone())
        .create_user("a@versions.test")
        .await
        .unwrap();
    let a = store.for_account(t1, u1);
    let t2 = store.create_tenant("versions-t2").await.unwrap();
    let u2 = store
        .for_tenant(t2.clone())
        .create_user("b@versions.test")
        .await
        .unwrap();
    let b = store.for_account(t2, u2);

    let site = a.create_site("Acme", &subdomain("iso-a")).await.unwrap();
    let home = a.create_site_page(&site, "Home", "", true).await.unwrap();
    a.set_page_sections(&site, &home, hero("Owned by A"))
        .await
        .unwrap();
    let first = a.publish_site(&site).await.unwrap();
    a.set_page_sections(&site, &home, hero("Changed by A"))
        .await
        .unwrap();
    let second = a.publish_site(&site).await.unwrap();

    // A's own second site never sees the first site's versions.
    let other = a.create_site("Other", &subdomain("iso-a2")).await.unwrap();
    let other_home = a.create_site_page(&other, "Home", "", true).await.unwrap();
    a.set_page_sections(&other, &other_home, hero("Other site"))
        .await
        .unwrap();
    let other_publish = a.publish_site(&other).await.unwrap();
    assert_eq!(a.site_publish_history(&other, 50).await.unwrap().len(), 1);
    assert!(
        a.site_publish_version(&other, &first)
            .await
            .unwrap()
            .is_none()
    );
    assert_not_found(
        a.compare_site_publishes(&other, &first, &other_publish)
            .await,
    );
    assert_not_found(a.restore_site_publish(&other, &first).await);

    // ---- the outsider tenant ------------------------------------------------
    assert!(b.site_publish_history(&site, 50).await.unwrap().is_empty());
    assert!(
        b.site_publish_version(&site, &first)
            .await
            .unwrap()
            .is_none()
    );
    assert_not_found(b.compare_site_publishes(&site, &first, &second).await);
    assert_not_found(b.restore_site_publish(&site, &first).await);

    // The outsider's own site cannot reach A's version either — the id is
    // real, the tenant is not.
    let theirs = b.create_site("Beta", &subdomain("iso-b")).await.unwrap();
    let their_home = b.create_site_page(&theirs, "Home", "", true).await.unwrap();
    b.set_page_sections(&theirs, &their_home, hero("Owned by B"))
        .await
        .unwrap();
    let their_publish = b.publish_site(&theirs).await.unwrap();
    assert!(
        b.site_publish_version(&theirs, &first)
            .await
            .unwrap()
            .is_none()
    );
    assert_not_found(
        b.compare_site_publishes(&theirs, &first, &their_publish)
            .await,
    );
    assert_not_found(b.restore_site_publish(&theirs, &first).await);
    assert_not_found(b.restore_site_publish(&theirs, &second).await);

    // Nothing the outsider did moved A's site off its own second version, and
    // A's history is exactly the two publishes A made.
    assert_eq!(
        a.current_site_publish(&site).await.unwrap().unwrap().id,
        second
    );
    let history = a.site_publish_history(&site, 50).await.unwrap();
    assert_eq!(history.len(), 2);
    assert!(
        history
            .iter()
            .all(|version| version.restored_from.is_none())
    );
    assert_eq!(
        b.site_publish_history(&theirs, 50).await.unwrap()[0].id,
        their_publish
    );

    // Deleting the site takes its whole history with it.
    a.delete_site(&site).await.unwrap();
    assert!(a.site_publish_history(&site, 50).await.unwrap().is_empty());
    assert!(
        a.site_publish_version(&site, &first)
            .await
            .unwrap()
            .is_none()
    );
    assert_not_found(a.restore_site_publish(&site, &first).await);
}

/// A publish whose frozen pages are gone is not something to put back online.
/// The rows cannot be removed through any store call — that is the point of
/// immutability — so this drives the guard through raw SQL, the only way a
/// database could ever reach that state.
#[tokio::test]
async fn a_version_with_nothing_frozen_is_refused() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("versions-empty").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("owner@versions-empty.test")
        .await
        .unwrap();
    let tenant_id = tenant.as_str().to_owned();
    let account = store.for_account(tenant, user);
    let site = account
        .create_site("Acme", &subdomain("empty"))
        .await
        .unwrap();
    let home = account
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    account
        .set_page_sections(&site, &home, hero("Only version"))
        .await
        .unwrap();
    let publish = account.publish_site(&site).await.unwrap();

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap();
    sqlx::query("DELETE FROM site_page_snapshots WHERE tenant_id = $1 AND publish_id = $2")
        .bind(&tenant_id)
        .bind(publish.as_str())
        .execute(&pool)
        .await
        .unwrap();

    match account.restore_site_publish(&site, &publish).await {
        Err(StoreError::Conflict(message)) => {
            assert!(
                message.contains("no pages"),
                "refusal names the rule: {message}"
            );
        }
        other => panic!("expected a Conflict, got {other:?}"),
    }
    assert_eq!(
        account.site_publish_history(&site, 50).await.unwrap().len(),
        1,
        "a refused restore writes nothing"
    );
}

/// A version freezes its collections too, so restoring one must bring them
/// back — and comparing must say which collections differ.
#[tokio::test]
async fn restoring_brings_back_the_collections_the_version_froze() {
    use alo_store::{DriveLocation, SiteCollectionInput};

    let store = common::test_store().await;
    let tenant = store.create_tenant("versions-collections").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("owner@versions-collections.test")
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let site = account
        .create_site("Acme", &subdomain("coll"))
        .await
        .unwrap();
    let base_node = account
        .create_base(&DriveLocation::Personal, None, "People")
        .await
        .unwrap();
    let base = account.base(&base_node).await.unwrap().unwrap();
    let table = base.tables[0].id.clone();
    let title_field = base.tables[0].fields[0].id.clone();
    let rows_before = base.tables[0].records.len();
    account
        .base_add_record(&table, &json!({ title_field.as_str(): "Ada" }))
        .await
        .unwrap();
    let mapping = alo_store::SiteCollectionFieldMapping {
        title: title_field.clone(),
        slug: None,
        summary: None,
        body: None,
        image: None,
        link: None,
        published_at: None,
    };
    let collection = account
        .create_site_collection(
            &site,
            &SiteCollectionInput {
                name: "Team",
                base_node_id: &base_node,
                base_table_id: &table,
                mapping: &mapping,
            },
        )
        .await
        .unwrap();
    let home = account
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    account
        .set_page_sections(
            &site,
            &home,
            json!({
                "schema_version": 1,
                "sections": [{
                    "type": "collection",
                    "collection_id": collection.as_str(),
                    "heading": "Our team"
                }]
            }),
        )
        .await
        .unwrap();
    let with_one = account.publish_site(&site).await.unwrap();

    account
        .base_add_record(&table, &json!({ title_field.as_str(): "Grace" }))
        .await
        .unwrap();
    let with_two = account.publish_site(&site).await.unwrap();

    let diff = account
        .compare_site_publishes(&site, &with_one, &with_two)
        .await
        .unwrap();
    assert_eq!(diff.collections.len(), 1);
    assert_eq!(diff.collections[0].change, SiteVersionChange::Changed);
    assert_eq!(diff.collections[0].items_before, 1);
    assert_eq!(diff.collections[0].items_after, 2);
    assert_eq!(diff.collections[0].name, "Team");
    assert_eq!(
        diff.collections[0].collection_id,
        SiteCollectionId::new(collection.as_str().to_owned())
    );
    assert!(
        diff.pages.is_empty(),
        "the page itself did not change, only the rows it shows"
    );

    let restored = account
        .restore_site_publish(&site, &with_one)
        .await
        .unwrap();
    let frozen = account
        .site_publish_collection_snapshots(&site, &restored)
        .await
        .unwrap();
    assert_eq!(frozen.len(), 1);
    assert_eq!(frozen[0].items.len(), 1, "the version's own rows came back");
    assert_eq!(frozen[0].items[0].title, "Ada");
    assert_eq!(
        account
            .site_publish_version(&site, &restored)
            .await
            .unwrap()
            .unwrap()
            .collections,
        1
    );
    // The Base table itself is untouched by a restore: it is the editable
    // source, and a rollback of the website is not a rollback of the data.
    let table_now = account.base(&base_node).await.unwrap().unwrap();
    assert_eq!(table_now.tables[0].records.len(), rows_before + 2);
}

/// An unknown id is not an oracle: it answers exactly like a real version the
/// caller may not see.
#[tokio::test]
async fn an_invented_version_id_reads_like_a_hidden_one() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("versions-unknown").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("owner@versions-unknown.test")
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let site = account
        .create_site("Acme", &subdomain("unknown"))
        .await
        .unwrap();
    let home = account
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    account
        .set_page_sections(&site, &home, hero("Only"))
        .await
        .unwrap();
    let real = account.publish_site(&site).await.unwrap();
    let invented = SitePublishId::generate();

    assert!(
        account
            .site_publish_version(&site, &invented)
            .await
            .unwrap()
            .is_none()
    );
    assert_not_found(
        account
            .compare_site_publishes(&site, &real, &invented)
            .await,
    );
    assert_not_found(
        account
            .compare_site_publishes(&site, &invented, &real)
            .await,
    );
    assert_not_found(account.restore_site_publish(&site, &invented).await);
    assert_not_found(
        account
            .restore_site_publish(&SiteId::generate(), &real)
            .await,
    );
}
