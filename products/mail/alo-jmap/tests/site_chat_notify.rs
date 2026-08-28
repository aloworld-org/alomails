//! The assistant-ceiling notification sweep (ADR 0040 §3, item S3.02c),
//! driven end-to-end over a real Postgres: a site whose assistant spent its
//! monthly ceiling becomes ONE internally delivered message in the site
//! owner's own inbox — the right tenant's inbox and nobody else's — exactly
//! once per site-month.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_store::{BlobStore, Page, SitePublicStore};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use sqlx::postgres::PgPoolOptions;

use crate::common::{database_url, harness, harness_on};

/// Splits a raw RFC 5322 message into (headers, base64-decoded body text).
fn open_message(raw: &[u8]) -> (String, String) {
    let raw = String::from_utf8_lossy(raw).into_owned();
    let (headers, body) = raw.split_once("\r\n\r\n").expect("header/body split");
    let body_bytes = B64
        .decode(body.replace(['\r', '\n'], ""))
        .expect("base64 body");
    (
        headers.to_owned(),
        String::from_utf8(body_bytes).expect("utf-8 body"),
    )
}

/// One test on purpose: the sweep is global, so concurrently running
/// scenarios in separate tests could claim (and deliver) each other's rows
/// mid-assertion. Sequenced here, every step's outcome is deterministic.
#[tokio::test]
async fn the_owner_is_told_once_in_their_own_inbox() {
    let a = harness("chatceila").await;
    // Tenant B lives on the SAME store handle: production runs one process
    // for every tenant, and the sweep must serve them all — without ever
    // crossing a wall.
    let b = harness_on(a.store.clone(), "chatceilb").await;

    // Unique subdomain: the compose Postgres is shared across runs.
    let salt: String = a
        .tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .take(20)
        .collect();
    let sub = format!("chatceil{salt}");

    let site = a.acc.create_site("Aurora Atelier", &sub).await.unwrap();
    a.acc
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    a.acc.publish_site(&site).await.unwrap();
    a.acc
        .set_site_chat_settings(&site, true, 150, "2026-08")
        .await
        .unwrap();

    // The public service records the spend that crosses the ceiling.
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url())
        .await
        .unwrap();
    let public = SitePublicStore::new(pool, BlobStore::in_memory(1024 * 1024));
    let resolved = public.resolve_published(&sub).await.unwrap().unwrap();
    assert!(
        !public
            .record_chat_spend(&resolved, "2026-08", 90)
            .await
            .unwrap()
    );
    assert!(
        public
            .record_chat_spend(&resolved, "2026-08", 90)
            .await
            .unwrap()
    );

    // The sweep delivers exactly one message, to the owning inbox only.
    alo_jmap::site_chat_notify::run_due(&a.store).await;

    let inbox_a = a.acc.inbox().await.unwrap();
    let listed = a.acc.list_mailbox(&inbox_a, Page::default()).await.unwrap();
    assert_eq!(listed.len(), 1, "owner A gets exactly one notification");
    let raw = a.acc.message_bytes(&listed[0].id).await.unwrap();
    let (headers, body) = open_message(&raw);
    assert!(
        headers.contains("Subject: Your website assistant reached its monthly budget"),
        "subject names the event: {headers}"
    );
    assert!(
        headers.contains(&format!("To: {}", a.email)),
        "addressed to the owner: {headers}"
    );
    assert!(body.contains("Aurora Atelier"), "names the site: {body}");
    assert!(
        body.contains("€1.80") && body.contains("€1.50"),
        "states spend and ceiling in euros: {body}"
    );
    assert!(body.contains("2026-08"), "names the month: {body}");
    assert!(
        body.contains("contact form"),
        "explains the visitor fallback: {body}"
    );

    let inbox_b = b.acc.inbox().await.unwrap();
    assert!(
        b.acc
            .list_mailbox(&inbox_b, Page::default())
            .await
            .unwrap()
            .is_empty(),
        "a tenant whose assistant hit nothing is told nothing"
    );

    // Claimed means notified: another sweep delivers nothing new.
    alo_jmap::site_chat_notify::run_due(&b.store).await;
    assert_eq!(
        a.acc
            .list_mailbox(&inbox_a, Page::default())
            .await
            .unwrap()
            .len(),
        1,
        "a hit ceiling is never announced twice"
    );

    // Spending on past the stamped ceiling stays silent too.
    assert!(
        !public
            .record_chat_spend(&resolved, "2026-08", 30)
            .await
            .unwrap()
    );
    alo_jmap::site_chat_notify::run_due(&a.store).await;
    assert_eq!(
        a.acc
            .list_mailbox(&inbox_a, Page::default())
            .await
            .unwrap()
            .len(),
        1
    );

    // A fresh month is a fresh budget — and, once spent, a fresh telling.
    assert!(
        public
            .record_chat_spend(&resolved, "2026-09", 150)
            .await
            .unwrap()
    );
    alo_jmap::site_chat_notify::run_due(&a.store).await;
    assert_eq!(
        a.acc
            .list_mailbox(&inbox_a, Page::default())
            .await
            .unwrap()
            .len(),
        2,
        "each exhausted month is announced once"
    );
}
