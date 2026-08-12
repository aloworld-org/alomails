//! Password-protected pages (ADR 0036, S2.06a): the model behind "this page
//! is online, but only for people who have the password".
//!
//! Four properties are load-bearing and are proved here against a real
//! Postgres. **The password never comes back** — nothing on either door
//! answers the plaintext, and the stored value is an argon2id hash.
//! **Tenant scope** — another tenant cannot protect, read, change, or lift the
//! protection on a page, and the public door resolved to their own site can
//! never open ours. **Rotation is revocation** — setting a password again
//! mints a new session version, so sessions opened with the old one are dead.
//! And **deleting the draft page does not open the published one** — the
//! protection survives, because the snapshot the internet is being served
//! survives.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::site_page_protection::{SITE_PAGE_PASSWORD_MAX_CHARS, SITE_PAGE_PASSWORD_MIN_CHARS};
use alo_store::{BlobStore, SiteId, SitePageId, SitePublicStore, StoreError};
use serde_json::json;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

fn assert_validation<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(detail)) => detail,
        other => panic!("expected Validation, got {other:?}"),
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

/// A plain pool, for the assertions that look at the stored row itself.
async fn raw_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap()
}

/// The public serving door, on its own pool like the real service.
async fn public_store() -> SitePublicStore {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&common::database_url())
        .await
        .expect("connect to test postgres (is compose up? is DATABASE_URL set?)");
    SitePublicStore::new(pool, BlobStore::in_memory(4 * 1024 * 1024))
}

/// A published site with a home page and a `/prices` page, returning both.
async fn published_site(
    account: &alo_store::AccountStore,
    tag: &str,
) -> (SiteId, String, SitePageId, SitePageId) {
    let sub = subdomain(tag);
    let site = account.create_site("Acme", &sub).await.unwrap();
    let home = account
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    account
        .set_page_sections(
            &site,
            &home,
            json!({"schema_version": 1, "sections": [{"type": "hero", "heading": "Acme"}]}),
        )
        .await
        .unwrap();
    let prices = account
        .create_site_page(&site, "Prices", "prices", false)
        .await
        .unwrap();
    account.publish_site(&site).await.unwrap();
    (site, sub, prices, home)
}

/// The whole arc through both doors: protect a page, open it with the right
/// password, change the password (which ends the old session version), and
/// lift the protection.
#[tokio::test]
async fn a_page_password_opens_only_with_the_right_word_and_rotates_on_change() {
    let store = common::test_store().await;
    let (account, _user, _inbox) = common::fresh_account(&store, "page-password").await;
    let (site, sub, prices, _home) = published_site(&account, "protect").await;
    let public = public_store().await;
    let resolved = public.resolve_published(&sub).await.unwrap().unwrap();

    // ---- nothing is protected yet -----------------------------------------
    assert!(
        account
            .site_page_protection(&site, &prices)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        account
            .site_page_protections(&site)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        public
            .published_page_protections(&resolved)
            .await
            .unwrap()
            .is_empty()
    );
    // A password on an unprotected page opens nothing.
    assert!(
        public
            .verify_page_password(&resolved, prices.as_str(), "anything at all")
            .await
            .unwrap()
            .is_none()
    );

    // ---- the rules, before anything is hashed ------------------------------
    let short = assert_validation(
        account
            .set_site_page_password(&site, &prices, "sh0rt")
            .await,
    );
    assert!(
        short.contains(&SITE_PAGE_PASSWORD_MIN_CHARS.to_string()),
        "{short}"
    );
    let long = assert_validation(
        account
            .set_site_page_password(
                &site,
                &prices,
                &"x".repeat(SITE_PAGE_PASSWORD_MAX_CHARS + 1),
            )
            .await,
    );
    assert!(
        long.contains(&SITE_PAGE_PASSWORD_MAX_CHARS.to_string()),
        "{long}"
    );
    assert_validation(
        account
            .set_site_page_password(&site, &prices, "          ")
            .await,
    );
    assert!(
        public
            .published_page_protections(&resolved)
            .await
            .unwrap()
            .is_empty(),
        "a refused password protects nothing"
    );

    // ---- protect it --------------------------------------------------------
    let protection = account
        .set_site_page_password(&site, &prices, "kaneelstokjes 2026")
        .await
        .unwrap();
    assert_eq!(protection.page, prices);
    assert_eq!(
        account
            .site_page_protections(&site)
            .await
            .unwrap()
            .iter()
            .map(|p| p.page.clone())
            .collect::<Vec<_>>(),
        vec![prices.clone()]
    );

    // The plaintext is nowhere in the row, and nothing reads it back.
    let stored: (String, String) = sqlx::query_as(
        "SELECT password_hash, version FROM site_page_passwords \
         WHERE tenant_id = $1 AND site_id = $2 AND page_id = $3",
    )
    .bind(account.tenant().as_str())
    .bind(site.as_str())
    .bind(prices.as_str())
    .fetch_one(&raw_pool().await)
    .await
    .unwrap();
    assert!(stored.0.starts_with("$argon2id$"), "{}", stored.0);
    assert!(!stored.0.contains("kaneelstokjes"), "{}", stored.0);
    assert!(!stored.1.contains("kaneelstokjes"), "{}", stored.1);

    // ---- the public door opens it, and only with the right word ------------
    let listed = public.published_page_protections(&resolved).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].page, prices);
    let version = listed[0].version.clone();
    assert_eq!(version, stored.1, "the served version is the stored one");
    assert_eq!(
        public
            .verify_page_password(&resolved, prices.as_str(), "kaneelstokjes 2026")
            .await
            .unwrap(),
        Some(version.clone())
    );
    for wrong in ["kaneelstokjes 2025", "", "KANEELSTOKJES 2026"] {
        assert!(
            public
                .verify_page_password(&resolved, prices.as_str(), wrong)
                .await
                .unwrap()
                .is_none(),
            "opened with {wrong:?}"
        );
    }

    // ---- changing the password ends every session opened with the old one --
    let rotated = account
        .set_site_page_password(&site, &prices, "andere kaneelstokjes")
        .await
        .unwrap();
    assert_eq!(rotated.created_at, protection.created_at, "same protection");
    let after = public.published_page_protections(&resolved).await.unwrap();
    assert_ne!(after[0].version, version, "the session version rotated");
    assert!(
        public
            .verify_page_password(&resolved, prices.as_str(), "kaneelstokjes 2026")
            .await
            .unwrap()
            .is_none(),
        "the old password is dead"
    );

    // ---- lifting it makes the page public again ----------------------------
    account
        .remove_site_page_password(&site, &prices)
        .await
        .unwrap();
    assert!(
        account
            .site_page_protection(&site, &prices)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        public
            .published_page_protections(&resolved)
            .await
            .unwrap()
            .is_empty()
    );
    // Lifting a password twice is the same statement about the world.
    account
        .remove_site_page_password(&site, &prices)
        .await
        .unwrap();

    account.delete_site(&site).await.unwrap();
}

/// Another tenant can neither protect our page, nor read, change, or lift the
/// protection on it — and their own resolved site cannot open ours.
#[tokio::test]
async fn another_tenant_can_neither_read_nor_change_nor_open_our_protection() {
    let store = common::test_store().await;
    let (ours, _u1, _i1) = common::fresh_account(&store, "protect-a").await;
    let (theirs, _u2, _i2) = common::fresh_account(&store, "protect-b").await;
    let (our_site, our_sub, our_page, _our_home) = published_site(&ours, "prot-a").await;
    let (their_site, their_sub, _their_page, _their_home) = published_site(&theirs, "prot-b").await;
    ours.set_site_page_password(&our_site, &our_page, "our own password")
        .await
        .unwrap();

    // Reads: our protection simply does not exist for them.
    assert!(
        theirs
            .site_page_protection(&our_site, &our_page)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        theirs
            .site_page_protections(&our_site)
            .await
            .unwrap()
            .is_empty()
    );
    // Writes: setting and lifting are refused exactly like an invented id.
    assert_not_found(
        theirs
            .set_site_page_password(&our_site, &our_page, "their password")
            .await,
    );
    assert_not_found(theirs.remove_site_page_password(&our_site, &our_page).await);
    // ...including naming our page under a site they do own.
    assert_not_found(
        theirs
            .set_site_page_password(&their_site, &our_page, "their password")
            .await,
    );
    assert_not_found(
        ours.set_site_page_password(&our_site, &SitePageId::generate(), "our password")
            .await,
    );
    theirs
        .remove_site_page_password(&their_site, &our_page)
        .await
        .unwrap();
    // Our protection is untouched by everything they just tried.
    assert!(
        ours.site_page_protection(&our_site, &our_page)
            .await
            .unwrap()
            .is_some()
    );

    // The public door: their host can neither see nor open our protection.
    let public = public_store().await;
    let theirs_resolved = public.resolve_published(&their_sub).await.unwrap().unwrap();
    assert!(
        public
            .published_page_protections(&theirs_resolved)
            .await
            .unwrap()
            .is_empty(),
        "our protected page is not on their site"
    );
    assert!(
        public
            .verify_page_password(&theirs_resolved, our_page.as_str(), "our own password")
            .await
            .unwrap()
            .is_none(),
        "the right password on the wrong host opens nothing"
    );
    // And ours still opens through our own host, so the refusal above was
    // about scope, not about the password.
    let ours_resolved = public.resolve_published(&our_sub).await.unwrap().unwrap();
    assert!(
        public
            .verify_page_password(&ours_resolved, our_page.as_str(), "our own password")
            .await
            .unwrap()
            .is_some()
    );
    // Nonsense ids are refused without reaching the database.
    for id in ["", &"x".repeat(500)] {
        assert!(
            public
                .verify_page_password(&ours_resolved, id, "our own password")
                .await
                .unwrap()
                .is_none()
        );
    }

    ours.delete_site(&our_site).await.unwrap();
    theirs.delete_site(&their_site).await.unwrap();
}

/// Deleting the draft page does not unpublish its snapshot — so the protection
/// has to survive it, or the still-served page would silently open. Deleting
/// the whole site does take it away.
#[tokio::test]
async fn protection_survives_the_draft_page_and_dies_with_the_site() {
    let store = common::test_store().await;
    let (account, _user, _inbox) = common::fresh_account(&store, "protect-life").await;
    let (site, sub, prices, home) = published_site(&account, "prot-life").await;
    account
        .set_site_page_password(&site, &prices, "still protected")
        .await
        .unwrap();
    let public = public_store().await;
    let resolved = public.resolve_published(&sub).await.unwrap().unwrap();

    // The published set still serves the page after the draft is deleted.
    account.delete_site_page(&site, &prices).await.unwrap();
    assert!(
        public
            .published_pages(&resolved)
            .await
            .unwrap()
            .iter()
            .any(|snapshot| snapshot.page_id == prices),
        "the snapshot outlives the draft page"
    );
    let protections = public.published_page_protections(&resolved).await.unwrap();
    assert_eq!(
        protections.len(),
        1,
        "the page the internet is being served is still closed"
    );
    assert!(
        public
            .verify_page_password(&resolved, prices.as_str(), "still protected")
            .await
            .unwrap()
            .is_some()
    );
    // The owner can still lift it, naming the page that no longer drafts.
    account
        .remove_site_page_password(&site, &prices)
        .await
        .unwrap();
    assert!(
        public
            .published_page_protections(&resolved)
            .await
            .unwrap()
            .is_empty()
    );

    // A page that no longer drafts cannot be protected *again*, though: there
    // is nothing left to decide about, and the next publish will drop it.
    assert_not_found(
        account
            .set_site_page_password(&site, &prices, "protected again")
            .await,
    );

    // And deleting the site takes every protection with it.
    account
        .set_site_page_password(&site, &home, "the home page now")
        .await
        .unwrap();
    account.delete_site(&site).await.unwrap();
    let left: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM site_page_passwords WHERE tenant_id = $1 AND site_id = $2",
    )
    .bind(account.tenant().as_str())
    .bind(site.as_str())
    .fetch_one(&raw_pool().await)
    .await
    .unwrap();
    assert_eq!(left, 0, "the site's protections cascade away with it");
}
