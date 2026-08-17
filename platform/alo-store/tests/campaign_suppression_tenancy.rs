//! Suppression, absolute and tenant-wide (C1.3, ADR 0044 §2; Law 1: isolation
//! is tested, not assumed).
//!
//! The queue's definition of done for this module: *an item that touches who
//! may be mailed is not done without a test that proves who may not be.* C1.3
//! adds the sharpest version of that — the person who **used** to be mailable
//! and asked us to stop — so the assertions here are about a rule holding
//! against the things that would normally undo it:
//!
//! - a fresh import that re-states consent for a suppressed address changes
//!   nothing, and the consent record is kept as evidence that it tried;
//! - a second suppression does not rewrite why the first happened;
//! - a neighbouring tenant's suppression does not silence our address, and ours
//!   does not silence theirs;
//! - the audience still shows a suppressed person, with the reason, because
//!   they are usually still a customer and a count that dropped them quietly
//!   could not be audited.
//!
//! There is no test that a suppression can be lifted, because there is no way
//! to lift one — see `campaign_suppression.rs`, and the unit test there that
//! holds the module's SQL to it.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use time::{Duration, OffsetDateTime};

use alo_store::{
    AccountStore, AudiencePage, ConsentSource, NewCampaignConsent, NewCustomer, NewSuppression,
    Store, StoreError, SuppressionReason, TenantId, TenantStore,
};

/// A tenant with one user: the account door for the audience, and the tenant
/// door for suppression.
///
/// Two handles on purpose. Suppression is a fact about the tenant, not about a
/// user's mailbox, and the endpoint that will write most of these rows (the
/// one-click unsubscribe of queue item C2s.2) has no logged-in user at all.
async fn tenant(store: &Store, tag: &str) -> (AccountStore, TenantStore) {
    let tenant: TenantId = store.create_tenant(&format!("csup-{tag}")).await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let user = ts.create_user(&format!("{tag}@csup.test")).await.unwrap();
    (store.for_account(tenant.clone(), user), ts)
}

/// A customer the tenant invoices — one of the three audience sources.
async fn customer(store: &AccountStore, name: &str, email: &str) {
    store
        .create_billing_customer(&NewCustomer {
            name: name.to_owned(),
            country: "BE".to_owned(),
            email: Some(email.to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();
}

/// Somebody said yes, and here is what they said yes to.
async fn agreed(store: &AccountStore, address: &str, statement: &str) {
    store
        .record_campaign_consent(&NewCampaignConsent {
            address,
            source: ConsentSource::Manual,
            source_ref: None,
            statement,
            occurred_at: None,
        })
        .await
        .unwrap();
}

/// Every address the tenant may actually mail, paged so the first page — where
/// a mis-bracketed `WHERE` would let somebody through — is always exercised.
async fn recipients(store: &AccountStore) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    loop {
        let page = store
            .campaign_recipients(&AudiencePage {
                after: out.last().cloned(),
                limit: 2,
            })
            .await
            .unwrap();
        if page.is_empty() {
            return out;
        }
        out.extend(page.into_iter().map(|r| r.address));
    }
}

/// Every address the tenant holds a record of, mailable or not.
async fn audience(store: &AccountStore) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    loop {
        let page = store
            .campaign_audience(&AudiencePage {
                after: out.last().cloned(),
                limit: 2,
            })
            .await
            .unwrap();
        if page.is_empty() {
            return out;
        }
        out.extend(page.into_iter().map(|m| m.address));
    }
}

#[tokio::test]
async fn an_import_cannot_resurrect_a_suppressed_address() {
    // The item's named test, and the reason ADR 0044 §2 says "no segment,
    // import or re-upload can bring them back": the obvious way a suppression
    // gets undone in a real product is a tenant re-uploading last year's list.
    let store = common::test_store().await;
    let (a, ts) = tenant(&store, "import").await;

    customer(&a, "Acme BV", "orders@acme.test").await;
    agreed(&a, "orders@acme.test", "Ticked the newsletter box").await;
    assert_eq!(
        recipients(&a).await,
        ["orders@acme.test"],
        "they were mailable before they asked us to stop"
    );

    ts.suppress_campaign_address(&NewSuppression {
        address: "orders@acme.test",
        reason: SuppressionReason::Unsubscribe,
        source_ref: None,
        occurred_at: None,
    })
    .await
    .unwrap();

    assert_eq!(recipients(&a).await, Vec::<String>::new());
    assert_eq!(a.campaign_recipient_count().await.unwrap(), 0);

    // Now the import: fresh consent, dated today, from the dangerous path, with
    // a statement that says exactly what a tenant would type.
    a.record_campaign_consent(&NewCampaignConsent {
        address: "ORDERS@Acme.TEST ",
        source: ConsentSource::Import,
        source_ref: Some("newsletter-2026.csv"),
        statement: "Opted in on the trade-fair sign-up sheet",
        occurred_at: None,
    })
    .await
    .unwrap();

    assert_eq!(
        recipients(&a).await,
        Vec::<String>::new(),
        "a fresh agreement does not undo a suppression"
    );
    assert_eq!(a.campaign_recipient_count().await.unwrap(), 0);

    // The import was not refused, and that is deliberate: the record is kept
    // because an import claiming an agreement is itself evidence worth having.
    // It simply grants nothing.
    let history = a.campaign_consent_for("orders@acme.test").await.unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].source, ConsentSource::Import);

    // And the person is still in the audience, named with the reason — they are
    // still a customer this tenant invoices.
    let members = a
        .campaign_audience(&AudiencePage {
            after: None,
            limit: 10,
        })
        .await
        .unwrap();
    assert_eq!(members.len(), 1);
    let excluded = members[0]
        .suppression
        .as_ref()
        .expect("the audience says why they were excluded");
    assert_eq!(excluded.reason, SuppressionReason::Unsubscribe);
    assert!(
        members[0].consent.is_some(),
        "and it does not pretend they never agreed — the stronger fact simply arrived later"
    );
}

#[tokio::test]
async fn a_neighbours_suppression_does_not_silence_our_address() {
    // The mandatory wrong-tenant test, in both directions: both tenants hold
    // the same person and both may mail them, until one of them suppresses.
    let store = common::test_store().await;
    let (a, ts_a) = tenant(&store, "mine").await;
    let (b, ts_b) = tenant(&store, "theirs").await;

    for acc in [&a, &b] {
        customer(acc, "Acme BV", "orders@acme.test").await;
        agreed(acc, "orders@acme.test", "Signed up at our stand").await;
    }
    assert_eq!(recipients(&a).await, ["orders@acme.test"]);
    assert_eq!(recipients(&b).await, ["orders@acme.test"]);

    ts_b.suppress_campaign_address(&NewSuppression {
        address: "orders@acme.test",
        reason: SuppressionReason::Complaint,
        source_ref: Some("fbl-2026-08"),
        occurred_at: None,
    })
    .await
    .unwrap();

    // Stated from both sides, so a leak has to show up as a named row rather
    // than as a missing one.
    assert_eq!(
        recipients(&b).await,
        Vec::<String>::new(),
        "the tenant that was complained about loses them"
    );
    assert_eq!(
        recipients(&a).await,
        ["orders@acme.test"],
        "and the tenant that was not keeps them: unsubscribing from one company \
         is not unsubscribing from every company on the platform"
    );
    assert_eq!(a.campaign_recipient_count().await.unwrap(), 1);

    // Nor is the neighbour's record even readable from here.
    assert!(
        ts_a.campaign_suppression_for("orders@acme.test")
            .await
            .unwrap()
            .is_none()
    );
    assert!(ts_a.campaign_suppressions(10).await.unwrap().is_empty());
    let theirs = ts_b
        .campaign_suppression_for("orders@acme.test")
        .await
        .unwrap()
        .expect("their own record is");
    assert_eq!(theirs.reason, SuppressionReason::Complaint);
    assert_eq!(theirs.source_ref.as_deref(), Some("fbl-2026-08"));
    assert_eq!(ts_b.campaign_suppressions(10).await.unwrap().len(), 1);

    // And the audience of the suppressing tenant still holds them.
    assert_eq!(audience(&b).await, ["orders@acme.test"]);
}

#[tokio::test]
async fn a_second_suppression_does_not_rewrite_why_the_first_happened() {
    // A hard bounce three months after somebody unsubscribed must not turn
    // "they asked to stop" into "their mailbox was full" — that reads as a
    // technical problem somebody might try to fix.
    let store = common::test_store().await;
    let (a, ts) = tenant(&store, "twice").await;
    customer(&a, "Acme BV", "orders@acme.test").await;
    agreed(&a, "orders@acme.test", "Ticked the newsletter box").await;

    let asked = OffsetDateTime::now_utc() - Duration::days(90);
    let first = ts
        .suppress_campaign_address(&NewSuppression {
            address: "orders@acme.test",
            reason: SuppressionReason::Unsubscribe,
            source_ref: Some("send-spring"),
            occurred_at: Some(asked),
        })
        .await
        .unwrap();

    let again = ts
        .suppress_campaign_address(&NewSuppression {
            address: "  ORDERS@acme.TEST ",
            reason: SuppressionReason::HardBounce,
            source_ref: Some("bounce-report-2026-08"),
            occurred_at: None,
        })
        .await
        .unwrap();

    // Idempotent, and the caller is told what is actually in force rather than
    // what it just asked for — the answer to "so are they suppressed, and why"
    // must not depend on which call you happen to be looking at.
    assert_eq!(again.id, first.id);
    assert_eq!(again.reason, SuppressionReason::Unsubscribe);
    assert_eq!(again.source_ref.as_deref(), Some("send-spring"));
    assert_eq!(again.occurred_at, first.occurred_at);

    let stored = ts
        .campaign_suppression_for("orders@acme.test")
        .await
        .unwrap()
        .expect("one row, one answer");
    assert_eq!(stored, first);
    assert_eq!(
        ts.campaign_suppressions(10).await.unwrap().len(),
        1,
        "one person, one row — the casing of the second attempt made no second one"
    );
    assert_eq!(recipients(&a).await, Vec::<String>::new());
}

#[tokio::test]
async fn suppression_reaches_the_person_it_was_given_for_however_it_is_spelled() {
    // The fold that makes an unsubscribe honest: the source spells them one
    // way, the click arrives another, and one copy of a person still being
    // mailed is the failure ADR 0044's "cannot unsubscribe from one copy of
    // themselves" names.
    let store = common::test_store().await;
    let (a, ts) = tenant(&store, "fold").await;
    customer(&a, "Acme BV", "Ann.Dupont@Example.TEST").await;
    agreed(&a, "ann.dupont@example.test", "Replied yes to our email").await;
    assert_eq!(recipients(&a).await, ["ann.dupont@example.test"]);

    ts.suppress_campaign_address(&NewSuppression {
        address: " ANN.DUPONT@Example.test ",
        reason: SuppressionReason::Unsubscribe,
        source_ref: None,
        occurred_at: None,
    })
    .await
    .unwrap();

    assert_eq!(recipients(&a).await, Vec::<String>::new());
    assert!(
        ts.campaign_suppression_for("Ann.Dupont@EXAMPLE.test")
            .await
            .unwrap()
            .is_some(),
        "and the record is readable however it is asked for"
    );
}

#[tokio::test]
async fn a_suppression_that_could_never_be_applied_is_refused_at_the_door() {
    // A suppression row that does not join the audience is not a near miss: it
    // is somebody who asked to stop and is still being mailed.
    let store = common::test_store().await;
    let (a, ts) = tenant(&store, "refuse").await;
    customer(&a, "Acme BV", "orders@acme.test").await;
    agreed(&a, "orders@acme.test", "Ticked the newsletter box").await;

    let base = NewSuppression {
        address: "orders@acme.test",
        reason: SuppressionReason::Unsubscribe,
        source_ref: None,
        occurred_at: None,
    };

    for junk in ["", "   ", "n/a", "ask reception", "ann@localhost"] {
        let candidate = NewSuppression {
            address: junk,
            ..base.clone()
        };
        assert!(
            matches!(
                ts.suppress_campaign_address(&candidate).await,
                Err(StoreError::Validation(_))
            ),
            "accepted a suppression for {junk:?}"
        );
    }
    assert!(matches!(
        ts.campaign_suppression_for("ask reception").await,
        Err(StoreError::Validation(_))
    ));

    let ahead = NewSuppression {
        occurred_at: Some(OffsetDateTime::now_utc() + Duration::hours(2)),
        ..base.clone()
    };
    assert!(matches!(
        ts.suppress_campaign_address(&ahead).await,
        Err(StoreError::Validation(_))
    ));

    let long = "x".repeat(400);
    let overlong = NewSuppression {
        source_ref: Some(&long),
        ..base.clone()
    };
    assert!(matches!(
        ts.suppress_campaign_address(&overlong).await,
        Err(StoreError::Validation(_))
    ));

    // Every refusal wrote nothing — a half-written suppression would be the
    // worst outcome of all, because it looks like the rule working.
    assert!(ts.campaign_suppressions(10).await.unwrap().is_empty());
    assert_eq!(recipients(&a).await, ["orders@acme.test"]);

    // And a page size nobody meant is refused rather than silently clamped.
    for bad in [0, -1, 501] {
        assert!(matches!(
            ts.campaign_suppressions(bad).await,
            Err(StoreError::Validation(_))
        ));
    }
}

#[tokio::test]
async fn suppression_stands_alone_and_does_not_need_a_consent_record() {
    // A hard bounce arrives for somebody who never consented, and a complaint
    // can arrive for an address no source holds. Neither is a reason to lose
    // the fact: the row is written now so that the day they become a customer
    // they are already excluded, which is the same reason consent is kept for
    // strangers (C1.2).
    let store = common::test_store().await;
    let (a, ts) = tenant(&store, "alone").await;

    ts.suppress_campaign_address(&NewSuppression {
        address: "nobody@stranger.test",
        reason: SuppressionReason::HardBounce,
        source_ref: Some("bounce-report-2026-08"),
        occurred_at: None,
    })
    .await
    .unwrap();
    assert_eq!(audience(&a).await, Vec::<String>::new());

    // They arrive as a customer and consent, and are still not mailable.
    customer(&a, "Stranger BV", "nobody@stranger.test").await;
    agreed(&a, "nobody@stranger.test", "Ticked the box on arrival").await;
    assert_eq!(audience(&a).await, ["nobody@stranger.test"]);
    assert_eq!(
        recipients(&a).await,
        Vec::<String>::new(),
        "the suppression was waiting for them"
    );
    assert_eq!(a.campaign_audience_size().await.unwrap(), 1);
    assert_eq!(a.campaign_recipient_count().await.unwrap(), 0);
}
