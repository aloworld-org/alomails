//! The form-notification sweep (ADR 0036, S1.16c1), driven end-to-end over
//! a real Postgres: a new site-form submission becomes ONE internally
//! delivered message in the site owner's own inbox — the right tenant's
//! inbox and nobody else's — exactly once, and hostile submission fields
//! can never inject headers into that message.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::Page;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;

use common::{harness, harness_on};

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
async fn notifications_land_in_the_owning_inbox_only_and_resist_injection() {
    let a = harness("notifa").await;
    // Tenant B lives on the SAME store handle: production runs one process
    // (one blob store) for every tenant, and the sweep must serve them all.
    let b = harness_on(a.store.clone(), "notifb").await;

    // Unique subdomains: the compose Postgres is shared across runs.
    let sub = |tag: &str, t: &alo_store::TenantId| {
        let salt: String = t
            .as_str()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .take(20)
            .collect();
        format!("{tag}{salt}")
    };

    let site_a = a
        .acc
        .create_site("Alpha Roastery", &sub("nfa", &a.tenant))
        .await
        .unwrap();
    let form_a = a.acc.create_site_form(&site_a, "Contact").await.unwrap();
    a.acc
        .add_site_form_submission(
            &site_a,
            &form_a,
            "Ada Lovelace",
            "ada@example.test",
            "I would like five kilos of beans.",
        )
        .await
        .unwrap();

    let site_b = b
        .acc
        .create_site("Beta Studio", &sub("nfb", &b.tenant))
        .await
        .unwrap();
    let form_b = b.acc.create_site_form(&site_b, "Careers").await.unwrap();
    b.acc
        .add_site_form_submission(
            &site_b,
            &form_b,
            "Grace Hopper",
            "grace@example.test",
            "Beta-only submission words.",
        )
        .await
        .unwrap();

    // One sweep serves every tenant: each notification is built from the
    // claimed row's own context and delivered through that tenant's door.
    alo_jmap::site_notify::run_due(&a.store).await;

    let inbox_a = a.acc.inbox().await.unwrap();
    let listed_a = a.acc.list_mailbox(&inbox_a, Page::default()).await.unwrap();
    assert_eq!(listed_a.len(), 1, "owner A gets exactly one notification");
    let raw_a = a.acc.message_bytes(&listed_a[0].id).await.unwrap();
    let (headers_a, body_a) = open_message(&raw_a);
    assert!(
        headers_a.contains("Subject: New message from Ada Lovelace (Alpha Roastery)"),
        "subject names the sender and site: {headers_a}"
    );
    assert!(
        headers_a.contains("Reply-To: \"Ada Lovelace\" <ada@example.test>"),
        "reply goes to the visitor: {headers_a}"
    );
    assert!(
        headers_a.contains(&format!("To: {}", a.email)),
        "addressed to the owner: {headers_a}"
    );
    assert!(body_a.contains("I would like five kilos of beans."));
    assert!(body_a.contains("Ada Lovelace <ada@example.test>"));
    assert!(body_a.contains("\"Contact\" form on Alpha Roastery"));
    assert!(
        !body_a.contains("Beta-only"),
        "tenant A must never see tenant B's submission"
    );

    let inbox_b = b.acc.inbox().await.unwrap();
    let listed_b = b.acc.list_mailbox(&inbox_b, Page::default()).await.unwrap();
    assert_eq!(listed_b.len(), 1, "owner B gets exactly one notification");
    let (_, body_b) = open_message(&b.acc.message_bytes(&listed_b[0].id).await.unwrap());
    assert!(body_b.contains("Beta-only submission words."));
    assert!(
        !body_b.contains("five kilos"),
        "tenant B must never see tenant A's submission"
    );

    // Claimed means notified: another sweep delivers nothing new to anyone.
    alo_jmap::site_notify::run_due(&b.store).await;
    assert_eq!(
        a.acc
            .list_mailbox(&inbox_a, Page::default())
            .await
            .unwrap()
            .len(),
        1,
        "a submission is never delivered twice"
    );
    assert_eq!(
        b.acc
            .list_mailbox(&inbox_b, Page::default())
            .await
            .unwrap()
            .len(),
        1
    );

    // Hostile fields: a CR/LF-bearing name (the write gate bounds but does
    // not forbid inner whitespace) and a non-ASCII name both cross into the
    // message without injecting headers — CR/LF dies in the RFC 2047 path,
    // free text travels base64.
    a.acc
        .add_site_form_submission(
            &site_a,
            &form_a,
            "Eve\r\nX-Evil: injected",
            "eve@example.test",
            "Injection attempt body.",
        )
        .await
        .unwrap();
    a.acc
        .add_site_form_submission(
            &site_a,
            &form_a,
            "Åsa Ödegård",
            "asa@example.test",
            "Hej från Sverige!",
        )
        .await
        .unwrap();
    alo_jmap::site_notify::run_due(&a.store).await;

    let listed = a.acc.list_mailbox(&inbox_a, Page::default()).await.unwrap();
    assert_eq!(listed.len(), 3);
    for summary in &listed {
        let raw = a.acc.message_bytes(&summary.id).await.unwrap();
        let (headers, _) = open_message(&raw);
        // The hostile name may appear as inert TEXT inside a header value
        // (CR/LF sanitized to spaces); what must never exist is a header
        // LINE of the attacker's making.
        assert!(
            !headers.lines().any(|l| l.starts_with("X-Evil")),
            "submission fields must never become headers: {headers}"
        );
    }
    // The non-ASCII name arrives as an RFC 2047 encoded word on the wire and
    // reads back decoded in the stored subject.
    assert!(
        listed.iter().any(|m| m.subject.contains("Åsa Ödegård")),
        "non-ASCII sender names survive the encode/decode round trip"
    );
}
