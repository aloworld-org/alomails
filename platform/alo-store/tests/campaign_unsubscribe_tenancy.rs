//! The one-click unsubscribe token (C2s.1, ADR 0044 §3; Law 1: isolation is
//! tested, not assumed).
//!
//! The queue item names two failures this token exists to prevent, and both are
//! here rather than in a comment:
//!
//! - **iterating identifiers to unsubscribe other people** — a link is a
//!   256-bit secret rather than an encoded id, the digest that is stored is not
//!   a link, and a neighbouring workspace's link resolves to that workspace and
//!   never to ours;
//! - **confirming an address is live by watching what the endpoint does** — a
//!   guess, a malformed token and an empty one are the same answer as a token
//!   for an address this deployment has never heard of, so a million posted
//!   guesses teach a spammer nothing about who we hold.
//!
//! Plus the property the module is for: what resolves is enough to suppress
//! somebody, and it names the send it came from, so C2s.3 can write a
//! suppression whose `source_ref` points back at exactly one link.
//!
//! There is no test that a link can be revoked or re-issued, because there is
//! no way to do either — see `campaign_unsubscribe.rs` and the unit tests that
//! hold the module's SQL to it.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    NewUnsubscribeToken, Store, StoreError, TenantId, TenantStore, UNSUBSCRIBE_SEND_REF_MAX,
};

/// A tenant handle: minting is a fact about the workspace's send, not about a
/// colleague's mailbox, so there is no account door in this file at all.
async fn tenant(store: &Store, tag: &str) -> TenantStore {
    let tenant: TenantId = store.create_tenant(&format!("cunsub-{tag}")).await.unwrap();
    store.for_tenant(tenant)
}

#[tokio::test]
async fn a_link_resolves_to_the_person_and_the_send_it_was_minted_for() {
    let store = common::test_store().await;
    let ts = tenant(&store, "resolve").await;

    let issued = ts
        .mint_campaign_unsubscribe_token(&NewUnsubscribeToken {
            send_ref: "august-newsletter",
            // The casing the sender happens to hold them in.
            address: "  Ann@Lead.TEST ",
        })
        .await
        .unwrap();

    // Folded to the one identity every campaign query uses, so the suppression
    // this will write joins the audience rather than sitting beside it.
    assert_eq!(issued.address, "ann@lead.test");

    let target = store
        .resolve_campaign_unsubscribe_token(&issued.token)
        .await
        .unwrap()
        .expect("the link we just minted resolves");

    assert_eq!(target.tenant, ts.tenant().clone());
    assert_eq!(target.record, issued.record);
    assert_eq!(target.send_ref, "august-newsletter");
    assert_eq!(target.address, "ann@lead.test");
    assert_eq!(target.issued_at, issued.issued_at);
}

#[tokio::test]
async fn a_link_cannot_be_guessed_from_the_person_it_is_for() {
    // The item's first named failure. Nothing about the recipient or the send
    // is recoverable from the URL, and two links for the same person differ —
    // so holding one mail (forwarded, quoted, scanned) is not holding a way to
    // unsubscribe anybody else, or even to recognise a second link to the same
    // person.
    let store = common::test_store().await;
    let ts = tenant(&store, "guess").await;

    let request = NewUnsubscribeToken {
        send_ref: "august-newsletter",
        address: "ann@lead.test",
    };
    let first = ts.mint_campaign_unsubscribe_token(&request).await.unwrap();
    let second = ts.mint_campaign_unsubscribe_token(&request).await.unwrap();

    assert_ne!(first.token, second.token);
    assert_ne!(first.record, second.record);
    for part in ["ann", "lead", "test", "august", "newsletter"] {
        assert!(
            !first.token.to_lowercase().contains(part),
            "the link spells out {part:?}: {}",
            first.token
        );
    }

    // Minting again did not kill the first link. We hold only a digest, so
    // "re-issue" would mean breaking a link already sitting in an inbox — and a
    // dead unsubscribe link is what makes somebody press the spam button.
    for token in [&first.token, &second.token] {
        assert!(
            store
                .resolve_campaign_unsubscribe_token(token)
                .await
                .unwrap()
                .is_some(),
            "a link stopped working when another was minted"
        );
    }

    // And the handles the two rows carry are not links: an id is safe to log
    // and safe to write into a suppression, which is the whole reason it is a
    // separate value.
    for record in [&first.record, &second.record] {
        assert!(
            store
                .resolve_campaign_unsubscribe_token(record.as_str())
                .await
                .unwrap()
                .is_none(),
            "the record id works as a link"
        );
    }
}

#[tokio::test]
async fn holding_the_stored_row_is_not_holding_the_link() {
    // What a database dump, a backup on a laptop, or a `SELECT *` over
    // somebody's shoulder actually yields. The column is `sha256(token)`, and
    // presenting it back gets nowhere.
    let store = common::test_store().await;
    let ts = tenant(&store, "dump").await;

    let issued = ts
        .mint_campaign_unsubscribe_token(&NewUnsubscribeToken {
            send_ref: "august-newsletter",
            address: "ann@lead.test",
        })
        .await
        .unwrap();

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap();
    let stored: String = sqlx::query_scalar(
        "SELECT token_hash FROM campaign_unsubscribe_tokens WHERE tenant_id = $1 AND id = $2",
    )
    .bind(ts.tenant().as_str())
    .bind(issued.record.as_str())
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_ne!(stored, issued.token, "the link itself is in the table");
    assert_eq!(stored.len(), 64);
    assert!(
        store
            .resolve_campaign_unsubscribe_token(&stored)
            .await
            .unwrap()
            .is_none(),
        "the stored digest works as a link"
    );
}

#[tokio::test]
async fn a_guess_teaches_a_spammer_nothing_about_who_we_hold() {
    // The item's second named failure. Every wrong answer is the same wrong
    // answer: no error for a malformed token, no distinct outcome for an
    // address we do hold versus one we have never seen, nothing that separates
    // "never existed" from anything else.
    let store = common::test_store().await;
    let ts = tenant(&store, "oracle").await;

    ts.mint_campaign_unsubscribe_token(&NewUnsubscribeToken {
        send_ref: "august-newsletter",
        address: "ann@lead.test",
    })
    .await
    .unwrap();

    for guess in [
        "",
        "   ",
        "ann@lead.test",
        "1",
        "0000000000000000000000000000000000000000000",
        "../../etc/passwd",
        "%00",
        "a token with spaces in it",
    ] {
        assert_eq!(
            store
                .resolve_campaign_unsubscribe_token(guess)
                .await
                .unwrap(),
            None,
            "{guess:?} was answered differently"
        );
    }
}

#[tokio::test]
async fn a_neighbours_link_is_never_ours() {
    // Law 1, in the shape this module can break it: resolution is deliberately
    // cross-tenant (the public route has no login), so the token is the only
    // thing standing between two workspaces. It resolves to exactly the one
    // that minted it, and the caller is handed that tenant rather than
    // assuming its own.
    let store = common::test_store().await;
    let ours = tenant(&store, "a").await;
    let theirs = tenant(&store, "b").await;

    let our_link = ours
        .mint_campaign_unsubscribe_token(&NewUnsubscribeToken {
            send_ref: "our-newsletter",
            address: "shared@person.test",
        })
        .await
        .unwrap();
    let their_link = theirs
        .mint_campaign_unsubscribe_token(&NewUnsubscribeToken {
            send_ref: "their-newsletter",
            // The same person: somebody is on both workspaces' lists, which is
            // ordinary and is exactly when a mixed-up tenant would leak.
            address: "shared@person.test",
        })
        .await
        .unwrap();

    let resolved_ours = store
        .resolve_campaign_unsubscribe_token(&our_link.token)
        .await
        .unwrap()
        .expect("our link resolves");
    assert_eq!(resolved_ours.tenant, ours.tenant().clone());
    assert_eq!(resolved_ours.send_ref, "our-newsletter");

    let resolved_theirs = store
        .resolve_campaign_unsubscribe_token(&their_link.token)
        .await
        .unwrap()
        .expect("their link resolves");
    assert_eq!(resolved_theirs.tenant, theirs.tenant().clone());
    assert_eq!(resolved_theirs.send_ref, "their-newsletter");

    // The two are different rows for the same address, so unsubscribing from
    // one workspace's mail will not silently end the other's — which is the
    // right answer: they are two separate relationships and ADR 0044 §2 scopes
    // suppression to the tenant.
    assert_ne!(resolved_ours.record, resolved_theirs.record);
    assert_ne!(resolved_ours.tenant, resolved_theirs.tenant);
}

#[tokio::test]
async fn a_link_nobody_could_use_is_refused_at_the_mint() {
    // A token minted for junk is a link whose suppression would not join the
    // audience: somebody who pressed unsubscribe and is still being mailed.
    let store = common::test_store().await;
    let ts = tenant(&store, "junk").await;

    for junk in ["", "   ", "n/a", "ask reception", "ann@localhost"] {
        let refused = ts
            .mint_campaign_unsubscribe_token(&NewUnsubscribeToken {
                send_ref: "august-newsletter",
                address: junk,
            })
            .await;
        assert!(
            matches!(refused, Err(StoreError::Validation(_))),
            "minted a link for {junk:?}"
        );
    }

    // And a link that cannot name its send is refused too: the send reference
    // is what C5m.1 will hang the per-recipient record off, and an unsubscribe
    // nobody can attribute teaches the tenant nothing.
    let long = "s".repeat(UNSUBSCRIBE_SEND_REF_MAX + 1);
    for bad_send in ["", "   ", long.as_str()] {
        let refused = ts
            .mint_campaign_unsubscribe_token(&NewUnsubscribeToken {
                send_ref: bad_send,
                address: "ann@lead.test",
            })
            .await;
        assert!(
            matches!(refused, Err(StoreError::Validation(_))),
            "minted a link for send {bad_send:?}"
        );
    }
}
