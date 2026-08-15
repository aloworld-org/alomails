//! The assistant's cost controls (ADR 0040 §3, item S3.02c): the ceiling is
//! defaulted rather than blank, off is the fail-closed reading of absence,
//! spend is integer cents in a per-month ledger, exhaustion is computed live
//! (raising the ceiling reopens the assistant), the ceiling-hit stamp lands
//! exactly once per site-month however the writes land, and — Law 1 — none
//! of it is reachable across a tenant wall.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    BlobStore, ChatGate, DEFAULT_CHAT_MONTHLY_CEILING_CENTS, PublishedSite, SitePublicStore,
    StoreError, chat_month_key,
};
use sqlx::postgres::PgPoolOptions;
use time::OffsetDateTime;
use time::macros::datetime;

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::Conflict(_)) => {}
        other => panic!("expected Conflict, got {other:?}"),
    }
}

/// A unique dns-safe subdomain per test run (the compose Postgres is shared).
fn subdomain(tag: &str) -> String {
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
}

/// The public door on its own small pool, plus the resolved published site.
async fn public_door(sub: &str) -> (SitePublicStore, PublishedSite) {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&common::database_url())
        .await
        .unwrap();
    let store = SitePublicStore::new(pool, BlobStore::in_memory(1024 * 1024));
    let site = store
        .resolve_published(sub)
        .await
        .unwrap()
        .expect("published site resolves");
    (store, site)
}

#[test]
fn month_key_is_utc_year_month() {
    assert_eq!(chat_month_key(datetime!(2026-08-15 12:00 UTC)), "2026-08");
    // A moment that is still July in UTC, whatever the local offset says.
    assert_eq!(chat_month_key(datetime!(2026-08-01 01:30 +2)), "2026-07");
}

#[tokio::test]
async fn settings_are_defaulted_validated_and_tenant_walled() {
    let store = common::test_store().await;
    let (a, _, _) = common::fresh_account(&store, "chat-set-a").await;
    let (b, _, _) = common::fresh_account(&store, "chat-set-b").await;
    let site = a
        .create_site("Chat Co", &subdomain("chatset"))
        .await
        .unwrap();
    let month = chat_month_key(OffsetDateTime::now_utc());

    // Defaulted rather than blank: a site that never touched the settings
    // reads as off with the default ceiling — never an absent value.
    let fresh = a.site_chat_settings(&site, &month).await.unwrap();
    assert!(!fresh.enabled, "the assistant starts off");
    assert_eq!(
        fresh.monthly_ceiling_cents,
        DEFAULT_CHAT_MONTHLY_CEILING_CENTS
    );
    assert_eq!(fresh.spent_cents, 0);
    assert!(!fresh.ceiling_hit);

    // The wrong tenant resolves nothing, in either direction of the wall.
    assert_not_found(b.site_chat_settings(&site, &month).await);
    assert_not_found(b.set_site_chat_settings(&site, true, 500, &month).await);

    // The ceiling range is a named rule: too low, zero, negative, too high.
    for cents in [99, 0, -5, 1_000_001] {
        assert_conflict(a.set_site_chat_settings(&site, true, cents, &month).await);
    }

    // One write sets switch and ceiling together and returns the view.
    let set = a
        .set_site_chat_settings(&site, true, 500, &month)
        .await
        .unwrap();
    assert!(set.enabled);
    assert_eq!(set.monthly_ceiling_cents, 500);
    let read = a.site_chat_settings(&site, &month).await.unwrap();
    assert_eq!(read, set);

    // Switching off keeps the chosen ceiling.
    let off = a
        .set_site_chat_settings(&site, false, 500, &month)
        .await
        .unwrap();
    assert!(!off.enabled);
    assert_eq!(off.monthly_ceiling_cents, 500);
}

#[tokio::test]
async fn spend_ledger_gates_stamps_once_and_keeps_months_apart() {
    let store = common::test_store().await;
    let (a, _, _) = common::fresh_account(&store, "chat-spend").await;
    let sub = subdomain("chatspend");
    let site = a.create_site("Spend Co", &sub).await.unwrap();
    a.create_site_page(&site, "Home", "", true).await.unwrap();
    a.publish_site(&site).await.unwrap();
    let (public, resolved) = public_door(&sub).await;

    // No settings row: the gate fails closed, whatever is published.
    assert_eq!(
        public.chat_gate(&resolved, "2026-08").await.unwrap(),
        ChatGate::Disabled
    );

    a.set_site_chat_settings(&site, true, 300, "2026-08")
        .await
        .unwrap();
    assert_eq!(
        public.chat_gate(&resolved, "2026-08").await.unwrap(),
        ChatGate::Ready {
            remaining_cents: 300
        }
    );

    // Spend must be a positive number of cents.
    for cents in [0, -10] {
        match public.record_chat_spend(&resolved, "2026-08", cents).await {
            Err(StoreError::Validation(_)) => {}
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    // Accumulation below the ceiling never reports a crossing.
    assert!(
        !public
            .record_chat_spend(&resolved, "2026-08", 120)
            .await
            .unwrap()
    );
    assert_eq!(
        public.chat_gate(&resolved, "2026-08").await.unwrap(),
        ChatGate::Ready {
            remaining_cents: 180
        }
    );
    assert!(
        !public
            .record_chat_spend(&resolved, "2026-08", 120)
            .await
            .unwrap()
    );

    // Exactly one write is the crossing write, later spend is not it again.
    assert!(
        public
            .record_chat_spend(&resolved, "2026-08", 120)
            .await
            .unwrap()
    );
    assert_eq!(
        public.chat_gate(&resolved, "2026-08").await.unwrap(),
        ChatGate::Exhausted
    );
    assert!(
        !public
            .record_chat_spend(&resolved, "2026-08", 50)
            .await
            .unwrap()
    );

    let view = a.site_chat_settings(&site, "2026-08").await.unwrap();
    assert_eq!(view.spent_cents, 410);
    assert!(view.ceiling_hit);

    // A new month is a fresh budget by key — no reset job, no carry-over.
    assert_eq!(
        public.chat_gate(&resolved, "2026-09").await.unwrap(),
        ChatGate::Ready {
            remaining_cents: 300
        }
    );

    // Exhaustion is computed live: raising the ceiling reopens the
    // assistant immediately, and the view stops reading as hit.
    a.set_site_chat_settings(&site, true, 1_000, "2026-08")
        .await
        .unwrap();
    assert_eq!(
        public.chat_gate(&resolved, "2026-08").await.unwrap(),
        ChatGate::Ready {
            remaining_cents: 590
        }
    );
    assert!(
        !a.site_chat_settings(&site, "2026-08")
            .await
            .unwrap()
            .ceiling_hit
    );

    // Switching off closes the gate regardless of budget.
    a.set_site_chat_settings(&site, false, 1_000, "2026-08")
        .await
        .unwrap();
    assert_eq!(
        public.chat_gate(&resolved, "2026-08").await.unwrap(),
        ChatGate::Disabled
    );
}

#[tokio::test]
async fn ceiling_hit_is_claimed_once_with_the_owning_tenant() {
    let store = common::test_store().await;
    let (a, owner, _) = common::fresh_account(&store, "chat-claim-a").await;
    let (b, _, _) = common::fresh_account(&store, "chat-claim-b").await;
    let sub = subdomain("chatclaim");
    let site = a.create_site("Claim Co", &sub).await.unwrap();
    a.create_site_page(&site, "Home", "", true).await.unwrap();
    a.publish_site(&site).await.unwrap();
    let (public, resolved) = public_door(&sub).await;

    a.set_site_chat_settings(&site, true, 100, "2026-08")
        .await
        .unwrap();
    assert!(
        public
            .record_chat_spend(&resolved, "2026-08", 100)
            .await
            .unwrap()
    );

    // The claim is system-level and the shared database may hold other
    // tests' pending rows, so every assertion filters to this site.
    let claimed = store.claim_chat_ceiling_notifications(1_000).await.unwrap();
    let ours = claimed
        .iter()
        .find(|n| n.site_subdomain == sub)
        .expect("the hit ceiling is claimed");
    assert_eq!(ours.tenant.as_str(), a.tenant().as_str());
    assert_eq!(ours.owner.as_str(), owner.as_str());
    assert_eq!(ours.site_name, "Claim Co");
    assert_eq!(ours.month, "2026-08");
    assert_eq!(ours.monthly_ceiling_cents, 100);
    assert_eq!(ours.spent_cents, 100);
    assert!(
        !claimed
            .iter()
            .any(|n| n.tenant.as_str() == b.tenant().as_str()),
        "a tenant that hit nothing is never in the claim"
    );

    // At-most-once: claimed means notified, whoever sweeps next.
    assert!(
        !store
            .claim_chat_ceiling_notifications(1_000)
            .await
            .unwrap()
            .iter()
            .any(|n| n.site_subdomain == sub),
        "a hit ceiling is never claimed twice"
    );

    // Further spend past an already-stamped ceiling never re-arms the claim.
    assert!(
        !public
            .record_chat_spend(&resolved, "2026-08", 40)
            .await
            .unwrap()
    );
    assert!(
        !store
            .claim_chat_ceiling_notifications(1_000)
            .await
            .unwrap()
            .iter()
            .any(|n| n.site_subdomain == sub)
    );
}
