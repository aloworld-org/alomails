//! The assistant's tenant-facing transcript (ADR 0040, item S3.03e): every
//! entry round-trips with its fact and its cited pages, the transcript reads
//! newest first, it is bounded per site by construction, the stored schema
//! can hold neither a question nor a visitor — and, Law 1, none of it is
//! reachable across a tenant wall.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    BlobStore, CHAT_ACTIONS_KEPT, ChatActionCitation, NewChatAction, PublishedSite,
    SiteChatActionKind, SitePublicStore, StoreError,
};
use sqlx::postgres::PgPoolOptions;
use time::macros::datetime;

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

#[tokio::test]
async fn every_entry_round_trips_and_the_transcript_reads_newest_first() {
    let store = common::test_store().await;
    let (acc, _, _) = common::fresh_account(&store, "chat-act-rt").await;
    let sub = subdomain("chatactrt");
    let site = acc.create_site("Transcript Co", &sub).await.unwrap();
    acc.create_site_page(&site, "Home", "", true).await.unwrap();
    acc.publish_site(&site).await.unwrap();
    let (door, published) = public_door(&sub).await;

    let citations = vec![
        ChatActionCitation {
            title: "Pricing".to_owned(),
            path: Some("/pricing".to_owned()),
        },
        ChatActionCitation {
            title: "Opening hours".to_owned(),
            // A knowledge document has no public URL: named by title alone.
            path: None,
        },
    ];
    let booked_at = datetime!(2026-09-01 09:30 UTC);
    for action in [
        NewChatAction::answered(&citations),
        NewChatAction::refused(),
        NewChatAction::booking_offered("Intro call"),
        NewChatAction::booked("Intro call", booked_at),
        NewChatAction::lead_offered(),
        NewChatAction::lead_saved(),
        NewChatAction::lead_known(),
    ] {
        door.record_chat_action(&published, &action).await.unwrap();
    }

    let transcript = acc.site_chat_actions(&site).await.unwrap();
    assert_eq!(transcript.len(), 7);
    // Newest first: the last recorded act leads the transcript.
    assert_eq!(transcript[0].kind, SiteChatActionKind::LeadKnown);
    assert_eq!(transcript[6].kind, SiteChatActionKind::Answered);
    assert!(
        transcript.windows(2).all(|w| w[0].occurred_at >= w[1].occurred_at),
        "the transcript reads newest first"
    );

    // The answered entry carries the fact's sources verbatim.
    assert_eq!(transcript[6].citations, citations);
    assert_eq!(transcript[6].fact, None);

    // The acts carry the published fact they used, and the booked instant.
    let booked = &transcript[3];
    assert_eq!(booked.kind, SiteChatActionKind::Booked);
    assert_eq!(booked.fact.as_deref(), Some("Intro call"));
    assert_eq!(booked.slot_at, Some(booked_at));
    assert_eq!(transcript[4].fact.as_deref(), Some("Intro call"));
    assert_eq!(transcript[4].slot_at, None);
}

#[tokio::test]
async fn the_transcript_is_tenant_walled() {
    let store = common::test_store().await;
    let (a, _, _) = common::fresh_account(&store, "chat-act-a").await;
    let (b, _, _) = common::fresh_account(&store, "chat-act-b").await;
    let sub = subdomain("chatacta");
    let site_a = a.create_site("A Co", &sub).await.unwrap();
    a.create_site_page(&site_a, "Home", "", true).await.unwrap();
    a.publish_site(&site_a).await.unwrap();
    let (door, published) = public_door(&sub).await;
    door.record_chat_action(&published, &NewChatAction::lead_saved())
        .await
        .unwrap();

    // The mandatory wrong-tenant proof: B reading A's site id is a clean
    // NotFound, indistinguishable from a site that never existed.
    match b.site_chat_actions(&site_a).await {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound across the tenant wall, got {other:?}"),
    }

    // And B's own transcript is untouched by A's traffic.
    let site_b = b.create_site("B Co", &subdomain("chatactb")).await.unwrap();
    assert!(b.site_chat_actions(&site_b).await.unwrap().is_empty());

    // A sees exactly its own entry.
    let own = a.site_chat_actions(&site_a).await.unwrap();
    assert_eq!(own.len(), 1);
    assert_eq!(own[0].kind, SiteChatActionKind::LeadSaved);
}

#[tokio::test]
async fn the_transcript_is_bounded_per_site() {
    let store = common::test_store().await;
    let (acc, _, _) = common::fresh_account(&store, "chat-act-cap").await;
    let sub = subdomain("chatactcap");
    let site = acc.create_site("Busy Co", &sub).await.unwrap();
    acc.create_site_page(&site, "Home", "", true).await.unwrap();
    acc.publish_site(&site).await.unwrap();
    let (door, published) = public_door(&sub).await;

    let extra = 10;
    for n in 0..(CHAT_ACTIONS_KEPT + extra) {
        door.record_chat_action(&published, &NewChatAction::booking_offered(&format!("s-{n}")))
            .await
            .unwrap();
    }

    let transcript = acc.site_chat_actions(&site).await.unwrap();
    assert_eq!(
        transcript.len(),
        usize::try_from(CHAT_ACTIONS_KEPT).unwrap(),
        "writes prune the site to its newest entries"
    );
    // The newest survive, the oldest were shed.
    assert_eq!(
        transcript[0].fact.as_deref(),
        Some(format!("s-{}", CHAT_ACTIONS_KEPT + extra - 1).as_str())
    );
    assert!(
        transcript
            .iter()
            .all(|entry| entry.fact.as_deref() != Some(format!("s-{}", extra - 1).as_str())),
        "the earliest entries are gone"
    );
}

#[tokio::test]
async fn the_stored_schema_can_hold_neither_a_question_nor_a_visitor() {
    // Migrations have run whenever the shared test store exists.
    let _ = common::test_store().await;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap();
    // These are all the columns there are: no question, no answer text, no
    // visitor token, no address, no name — the privacy model as schema.
    let columns = sqlx::query_scalar::<_, String>(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_name = 'site_chat_actions' ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        columns,
        vec![
            "citations",
            "fact",
            "id",
            "kind",
            "occurred_at",
            "site_id",
            "slot_at",
            "tenant_id"
        ],
        "the transcript table grew a column that could carry a conversation or a visitor"
    );
}
