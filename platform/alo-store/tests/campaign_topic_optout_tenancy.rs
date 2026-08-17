//! Fewer, rather than only none (C2s.2, ADR 0044 §3; Law 1: isolation is
//! tested, not assumed).
//!
//! The queue item's argument: *a recipient offered only all-or-nothing presses
//! the spam button instead.* So the narrower answer has to be a real, separate
//! decision — and the thing that proves it is real is the assertion nobody
//! writes by accident: **somebody who declines one kind of mail is still
//! mailable.** A "fewer" button that quietly suppressed everything would pass
//! every test about the person being taken off the newsletter, and would be the
//! exact failure the item exists to avoid.
//!
//! What is held here:
//!
//! - declining a kind of mail does **not** suppress the person, and the wider
//!   choice beside it does;
//! - one kind of mail is one topic however it is spelled, because declining
//!   `Newsletter` and being sent `newsletter` is unsubscribing from one copy of
//!   yourself;
//! - pressing the same link twice keeps the first decision and its date;
//! - a neighbouring workspace's preferences are unreachable, and ours do not
//!   silence theirs.
//!
//! There is no test that a preference can be lifted, because there is no way to
//! lift one — see `campaign_topic_optout.rs` and the unit test there that holds
//! the module's SQL to it.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use time::{Duration, OffsetDateTime};

use alo_store::{
    AccountStore, AudiencePage, ConsentSource, NewCampaignConsent, NewCustomer, NewSuppression,
    NewTopicOptOut, Store, StoreError, SuppressionReason, TenantId, TenantStore,
};

/// A tenant with one user: the account door for the audience, the tenant door
/// for preferences.
///
/// Two handles for the reason `campaign_suppression_tenancy.rs` gives: a
/// preference is a fact about the workspace's mail, and the endpoint that
/// writes it (the landing page of C2s.2) has no logged-in user at all.
async fn tenant(store: &Store, tag: &str) -> (AccountStore, TenantStore) {
    let tenant: TenantId = store.create_tenant(&format!("ctopic-{tag}")).await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let user = ts.create_user(&format!("{tag}@ctopic.test")).await.unwrap();
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

/// Every address the tenant may actually mail, paged so the first page is
/// always exercised.
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

#[tokio::test]
async fn declining_one_kind_of_mail_is_not_declining_all_of_it() {
    // The item, stated as a test. "Fewer rather than only none" is only true if
    // the narrower button leaves the person reachable — a "fewer" that
    // suppressed everything would satisfy every assertion about them being off
    // the newsletter, and would be exactly the failure the item exists to
    // prevent.
    let store = common::test_store().await;
    let (acct, ts) = tenant(&store, "fewer").await;

    customer(&acct, "Ann Dupont", "ann@ctopic.test").await;
    customer(&acct, "Bram Peeters", "bram@ctopic.test").await;
    agreed(
        &acct,
        "ann@ctopic.test",
        "Ticked the newsletter box at checkout",
    )
    .await;
    agreed(
        &acct,
        "bram@ctopic.test",
        "Ticked the newsletter box at checkout",
    )
    .await;

    assert_eq!(
        recipients(&acct).await,
        ["ann@ctopic.test", "bram@ctopic.test"]
    );

    // Ann presses "stop sending me this kind of mail".
    let declined = ts
        .decline_campaign_topic(&NewTopicOptOut {
            address: "ann@ctopic.test",
            topic: "Newsletter",
            source_ref: Some("cut_2026_08"),
            occurred_at: None,
        })
        .await
        .unwrap();
    assert_eq!(declined.topic, "newsletter");
    assert_eq!(declined.source_ref.as_deref(), Some("cut_2026_08"));

    // She is still somebody this tenant may mail. She asked for less, not for
    // nothing, and the invoice reminders she never complained about must keep
    // arriving.
    assert_eq!(
        recipients(&acct).await,
        ["ann@ctopic.test", "bram@ctopic.test"],
        "declining one kind of mail must not suppress the person"
    );
    assert!(
        ts.campaign_suppression_for("ann@ctopic.test")
            .await
            .unwrap()
            .is_none(),
        "the narrower choice wrote a suppression"
    );

    // And the wider choice, beside it, does end everything — the two buttons
    // are different decisions and this is the difference.
    ts.suppress_campaign_address(&NewSuppression {
        address: "bram@ctopic.test",
        reason: SuppressionReason::Unsubscribe,
        source_ref: Some("cut_2026_08"),
        occurred_at: None,
    })
    .await
    .unwrap();
    assert_eq!(
        recipients(&acct).await,
        ["ann@ctopic.test"],
        "the wider choice must end everything"
    );

    // What Ann declined is readable back, whole, by the address the caller
    // already holds.
    let hers = ts
        .campaign_topics_declined_by("  ANN@Ctopic.TEST ")
        .await
        .unwrap();
    assert_eq!(hers.len(), 1);
    assert_eq!(hers[0].id, declined.id);
    assert_eq!(hers[0].topic, "newsletter");
    assert_eq!(hers[0].address, "ann@ctopic.test");
    // Bram declined no kind of mail; he took the other option entirely.
    assert!(
        ts.campaign_topics_declined_by("bram@ctopic.test")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn one_kind_of_mail_is_one_decision_however_it_is_spelled() {
    // The failure the fold prevents: a person who declined "Newsletter" and is
    // then sent "newsletter" has unsubscribed from one copy of themselves —
    // the same failure the address fold prevents one file over, and the one ADR
    // 0044's "there is no list" claim is about.
    let store = common::test_store().await;
    let (_, ts) = tenant(&store, "fold").await;

    let first = ts
        .decline_campaign_topic(&NewTopicOptOut {
            address: "  Ann.Dupont@Example.TEST ",
            topic: "  Product   Updates ",
            source_ref: None,
            occurred_at: None,
        })
        .await
        .unwrap();
    assert_eq!(first.address, "ann.dupont@example.test");
    assert_eq!(first.topic, "product updates");

    // A second send spells it differently and the same person presses the same
    // button. One decision, not two.
    let second = ts
        .decline_campaign_topic(&NewTopicOptOut {
            address: "ANN.DUPONT@example.test",
            topic: "product updates",
            source_ref: Some("a-later-send"),
            occurred_at: None,
        })
        .await
        .unwrap();
    assert_eq!(second.id, first.id);
    assert_eq!(second.occurred_at, first.occurred_at);
    assert_eq!(
        second.source_ref, first.source_ref,
        "the second press restamped which link the decision came from"
    );

    let declined = ts
        .campaign_topics_declined_by("ann.dupont@example.test")
        .await
        .unwrap();
    assert_eq!(
        declined.len(),
        1,
        "one person, one decision, however spelled"
    );

    // Declining a second, genuinely different kind is a second decision — that
    // is the whole point of offering fewer rather than none.
    ts.decline_campaign_topic(&NewTopicOptOut {
        address: "ann.dupont@example.test",
        topic: "Newsletter",
        source_ref: None,
        occurred_at: None,
    })
    .await
    .unwrap();
    let declined = ts
        .campaign_topics_declined_by("ann.dupont@example.test")
        .await
        .unwrap();
    assert_eq!(
        declined
            .iter()
            .map(|d| d.topic.as_str())
            .collect::<Vec<_>>(),
        ["newsletter", "product updates"]
    );
}

#[tokio::test]
async fn a_neighbours_preferences_are_unreachable_and_ours_do_not_reach_them() {
    // The mandatory wrong-tenant test, in the shape this module could actually
    // break: two workspaces holding the SAME person, who is on both their
    // newsletters. Declining one company's newsletter is not declining every
    // company's newsletter on the platform, and a mixed-up tenant here would
    // read as the feature working.
    let store = common::test_store().await;
    let (ours, our_ts) = tenant(&store, "ours").await;
    let (theirs, their_ts) = tenant(&store, "theirs").await;

    for acct in [&ours, &theirs] {
        customer(acct, "Shared Person", "shared@person.test").await;
        agreed(acct, "shared@person.test", "Asked us to keep them posted").await;
    }

    our_ts
        .decline_campaign_topic(&NewTopicOptOut {
            address: "shared@person.test",
            topic: "Newsletter",
            source_ref: None,
            occurred_at: None,
        })
        .await
        .unwrap();

    assert_eq!(
        our_ts
            .campaign_topics_declined_by("shared@person.test")
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        their_ts
            .campaign_topics_declined_by("shared@person.test")
            .await
            .unwrap()
            .is_empty(),
        "one company's preference reached another company's records"
    );

    // Neither side lost a recipient: a preference is not a suppression, and
    // asserted from both sides so a leak would have to show up as a named row.
    assert_eq!(recipients(&ours).await, ["shared@person.test"]);
    assert_eq!(recipients(&theirs).await, ["shared@person.test"]);
}

#[tokio::test]
async fn a_preference_that_could_never_be_applied_is_refused_at_the_door() {
    // A row that does not join is not a near miss: it is somebody who pressed
    // the button and is still being mailed. So the refusals are checked, and
    // then the tenant is re-read to prove no half-write left a decision behind.
    let store = common::test_store().await;
    let (acct, ts) = tenant(&store, "refuse").await;

    customer(&acct, "Ann Dupont", "ann@ctopic.test").await;
    agreed(&acct, "ann@ctopic.test", "Ticked the box").await;

    for junk in ["", "   ", "n/a", "ask reception", "ann@localhost"] {
        assert!(
            matches!(
                ts.decline_campaign_topic(&NewTopicOptOut {
                    address: junk,
                    topic: "Newsletter",
                    source_ref: None,
                    occurred_at: None,
                })
                .await,
                Err(StoreError::Validation(_))
            ),
            "accepted a preference for {junk:?}"
        );
    }

    let overlong_topic = "n".repeat(81);
    for topic in ["", "   ", overlong_topic.as_str()] {
        assert!(
            matches!(
                ts.decline_campaign_topic(&NewTopicOptOut {
                    address: "ann@ctopic.test",
                    topic,
                    source_ref: None,
                    occurred_at: None,
                })
                .await,
                Err(StoreError::Validation(_))
            ),
            "accepted a preference about {topic:?}"
        );
    }

    // Next year is a lie, whatever the caller's clock says.
    assert!(matches!(
        ts.decline_campaign_topic(&NewTopicOptOut {
            address: "ann@ctopic.test",
            topic: "Newsletter",
            source_ref: None,
            occurred_at: Some(OffsetDateTime::now_utc() + Duration::hours(2)),
        })
        .await,
        Err(StoreError::Validation(_))
    ));

    let overlong_ref = "x".repeat(201);
    assert!(matches!(
        ts.decline_campaign_topic(&NewTopicOptOut {
            address: "ann@ctopic.test",
            topic: "Newsletter",
            source_ref: Some(&overlong_ref),
            occurred_at: None,
        })
        .await,
        Err(StoreError::Validation(_))
    ));

    assert!(
        ts.campaign_topics_declined_by("ann@ctopic.test")
            .await
            .unwrap()
            .is_empty(),
        "a refused preference was half-written"
    );
    assert_eq!(
        recipients(&acct).await,
        ["ann@ctopic.test"],
        "a refused preference changed who may be mailed"
    );
    // And an address that is not one is refused on the read too, rather than
    // answering "they have declined nothing" about a string nobody could mail.
    assert!(matches!(
        ts.campaign_topics_declined_by("ask reception").await,
        Err(StoreError::Validation(_))
    ));
}

#[tokio::test]
async fn a_preference_stands_alone_and_does_not_need_a_record_of_the_person() {
    // Somebody can decline a kind of mail before the tenant has any other
    // record of them — a forwarded newsletter, a link pressed by a person the
    // tenant has never invoiced. The decision is kept, waiting, and it grants
    // them nothing either: a preference is not consent.
    let store = common::test_store().await;
    let (acct, ts) = tenant(&store, "stranger").await;

    ts.decline_campaign_topic(&NewTopicOptOut {
        address: "stranger@ctopic.test",
        topic: "Newsletter",
        source_ref: None,
        occurred_at: None,
    })
    .await
    .unwrap();

    assert!(recipients(&acct).await.is_empty());
    assert_eq!(
        ts.campaign_topics_declined_by("stranger@ctopic.test")
            .await
            .unwrap()
            .len(),
        1
    );

    // They later become a consenting customer, and are mailable — because the
    // preference they left is about one kind of mail, not about all of it.
    customer(&acct, "A Stranger", "stranger@ctopic.test").await;
    agreed(&acct, "stranger@ctopic.test", "Asked to be kept posted").await;
    assert_eq!(recipients(&acct).await, ["stranger@ctopic.test"]);
}
