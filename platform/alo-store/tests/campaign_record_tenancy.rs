//! The campaign record — the letter, its body, and the two boundaries around it
//! (C3.1, ADR 0044; Law 1: isolation is tested, not assumed).
//!
//! The store's own unit tests hold the field rules down without a database.
//! What needs a real Postgres, and what this suite is for:
//!
//! - **A campaign belongs to the tenant, not to whoever typed it.** A colleague
//!   can read and rewrite the mail a colleague drafted, because a campaign is
//!   the company's letter — and the moment that is true, the *only* thing
//!   standing between two customers' drafts is the tenant predicate. So every
//!   statement is driven from both sides: another tenant's campaign is a row
//!   that does not exist, on every read and every write, and each tenant's list
//!   is asserted whole so a leak has to appear as a named extra row.
//! - **A body survives the round trip byte for byte.** Wave C3.2 compiles these
//!   blocks into email-safe HTML against golden files; a body that came back
//!   from the database subtly different from the one the editor wrote would make
//!   every one of those goldens a lie about what customers receive.
//! - **The rules are the database's as well as Rust's.** The migration carries
//!   `CHECK`s for the subject, the preheader and the envelope; a blank subject
//!   written past the validator must still fail.
//! - **Deleting a letter is not deleting evidence.** Consent records and
//!   suppressions outlive any campaign, and a tenant tidying up its drafts must
//!   not be able to lose the reason somebody may or may not be mailed.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    AccountStore, CAMPAIGN_CONTENT_SCHEMA_VERSION, Campaign, CampaignContent, CampaignId,
    ConsentSource, NewCampaign, NewCampaignConsent, NewSuppression, Store, StoreError,
    SuppressionReason, TenantId, TenantStore,
};
use serde_json::json;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// A tenant with one user: the account door for campaigns, the tenant door for
/// suppression (which has no logged-in colleague behind it).
async fn tenant(store: &Store, tag: &str) -> (AccountStore, TenantStore, TenantId) {
    let tenant: TenantId = store.create_tenant(&format!("crec-{tag}")).await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let user = ts.create_user(&format!("{tag}@crec.test")).await.unwrap();
    (store.for_account(tenant.clone(), user), ts, tenant)
}

/// A second colleague inside an existing tenant — the person who edits somebody
/// else's draft.
async fn colleague(store: &Store, tenant: &TenantId, tag: &str) -> AccountStore {
    let ts = store.for_tenant(tenant.clone());
    let user = ts
        .create_user(&format!("{tag}-colleague@crec.test"))
        .await
        .unwrap();
    store.for_account(tenant.clone(), user)
}

/// A direct pool, for the one thing no API offers: writing past the validator
/// to prove the migration's `CHECK`s are the second lock on these fields.
async fn pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::database_url())
        .await
        .unwrap()
}

/// A body with one of each block a campaign may carry.
fn a_full_body() -> CampaignContent {
    CampaignContent::from_value(json!({
        "schema_version": CAMPAIGN_CONTENT_SCHEMA_VERSION,
        "blocks": [
            { "type": "heading", "id": "h1", "level": 1, "text": "Spring prices" },
            { "type": "paragraph", "id": "p1", "text": "Everything below is per litre." },
            { "type": "table", "id": "t1", "rows": [["Product", "Price"], ["Oil", "€12"]] },
            { "type": "code", "id": "c1", "code": "curl https://alo", "language": "bash" },
        ],
    }))
    .expect("a body of the blocks a campaign may carry")
}

fn a_campaign<'a>(subject: &'a str, content: CampaignContent) -> NewCampaign<'a> {
    NewCampaign {
        subject,
        preheader: Some("Ten per cent off until Friday"),
        topic: "Monthly Newsletter",
        content,
    }
}

/// The detail of a validation error, or a panic naming what came back instead.
fn validation(result: Result<Campaign, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(detail)) => detail,
        other => panic!("expected a validation error, got {:?}", other.map(|c| c.id)),
    }
}

#[tokio::test]
async fn a_campaign_survives_the_round_trip_the_renderer_will_depend_on() {
    let store = common::test_store().await;
    let (acc, _, _) = tenant(&store, "roundtrip").await;

    let written = acc
        .create_campaign(&a_campaign("Spring prices", a_full_body()))
        .await
        .unwrap();
    assert_eq!(written.subject, "Spring prices");
    assert_eq!(
        written.preheader.as_deref(),
        Some("Ten per cent off until Friday")
    );
    assert_eq!(written.topic, "Monthly Newsletter");

    let read = acc.campaign(&written.id).await.unwrap().expect("stored");
    assert_eq!(
        read.content,
        a_full_body(),
        "C3.2's golden files are only meaningful if a body comes back exactly as it went in"
    );
    assert_eq!(read, written, "the write and the read agree in every field");
}

#[tokio::test]
async fn a_campaign_is_the_tenants_letter_rather_than_its_authors() {
    let store = common::test_store().await;
    let (author, _, tenant_id) = tenant(&store, "shared").await;
    let other = colleague(&store, &tenant_id, "shared").await;

    let written = author
        .create_campaign(&a_campaign("Draft one", CampaignContent::empty()))
        .await
        .unwrap();

    // The colleague sees it, and can finish it: a campaign is the company's
    // letter, not somebody's private document.
    let seen = other.campaign(&written.id).await.unwrap().expect("visible");
    assert_eq!(
        seen.created_by, written.created_by,
        "authorship is recorded"
    );
    let edited = other
        .update_campaign(
            &written.id,
            &NewCampaign {
                subject: "Draft one, finished",
                preheader: None,
                topic: "Monthly Newsletter",
                content: a_full_body(),
            },
        )
        .await
        .unwrap();
    assert_eq!(edited.subject, "Draft one, finished");
    assert_eq!(edited.preheader, None, "a preheader can be taken away");
    assert_eq!(edited.content, a_full_body());
    assert_eq!(
        edited.created_by, written.created_by,
        "editing a letter does not rewrite who drafted it"
    );
}

#[tokio::test]
async fn another_tenants_campaign_does_not_exist_on_any_statement() {
    let store = common::test_store().await;
    let (ours, _, _) = tenant(&store, "tenancy-a").await;
    let (theirs, _, _) = tenant(&store, "tenancy-b").await;

    let ours_written = ours
        .create_campaign(&a_campaign("Our spring mail", a_full_body()))
        .await
        .unwrap();
    let theirs_written = theirs
        .create_campaign(&a_campaign("Their spring mail", CampaignContent::empty()))
        .await
        .unwrap();

    // Read: not a 403, not an empty campaign — nothing.
    assert!(
        theirs.campaign(&ours_written.id).await.unwrap().is_none(),
        "a neighbour's campaign must be indistinguishable from one that was never written"
    );
    // Write, from both sides, on both statements that take an id.
    assert!(matches!(
        theirs
            .update_campaign(
                &ours_written.id,
                &a_campaign("Rewritten by a stranger", CampaignContent::empty()),
            )
            .await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        theirs.delete_campaign(&ours_written.id).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        ours.delete_campaign(&theirs_written.id).await,
        Err(StoreError::NotFound)
    ));

    // And the lists are asserted whole, so a leak shows up as a named extra row.
    let ours_list = ours.campaigns(200).await.unwrap();
    assert_eq!(
        ours_list
            .iter()
            .map(|c| c.subject.clone())
            .collect::<Vec<_>>(),
        vec!["Our spring mail".to_owned()]
    );
    let theirs_list = theirs.campaigns(200).await.unwrap();
    assert_eq!(
        theirs_list
            .iter()
            .map(|c| c.subject.clone())
            .collect::<Vec<_>>(),
        vec!["Their spring mail".to_owned()]
    );
    // The neighbour's rewrite attempt changed nothing.
    let ours_again = ours
        .campaign(&ours_written.id)
        .await
        .unwrap()
        .expect("ours");
    assert_eq!(ours_again, ours_written);
}

#[tokio::test]
async fn the_list_says_how_far_along_each_letter_is_without_carrying_it() {
    let store = common::test_store().await;
    let (acc, _, _) = tenant(&store, "list").await;

    let empty = acc
        .create_campaign(&a_campaign("Named, not written", CampaignContent::empty()))
        .await
        .unwrap();
    let full = acc
        .create_campaign(&a_campaign("Written", a_full_body()))
        .await
        .unwrap();

    let list = acc.campaigns(200).await.unwrap();
    assert_eq!(list.len(), 2);
    // Newest first — the draft somebody is working on is the one they are
    // looking for.
    assert_eq!(list[0].id, full.id);
    assert_eq!(list[0].blocks, 4, "the block count is how far along it is");
    assert_eq!(list[1].id, empty.id);
    assert_eq!(list[1].blocks, 0, "zero is named and not yet written");

    // A page size the store cannot honour is the caller's error, not a silently
    // truncated list.
    assert!(matches!(
        acc.campaigns(0).await,
        Err(StoreError::Validation(_))
    ));
    assert!(matches!(
        acc.campaigns(10_000).await,
        Err(StoreError::Validation(_))
    ));
}

#[tokio::test]
async fn the_rules_that_protect_a_recipient_hold_at_the_store_boundary() {
    let store = common::test_store().await;
    let (acc, _, _) = tenant(&store, "rules").await;

    // No subject: there is nothing to arrive in an inbox.
    let detail = validation(
        acc.create_campaign(&NewCampaign {
            subject: "   ",
            preheader: None,
            topic: "Monthly Newsletter",
            content: CampaignContent::empty(),
        })
        .await,
    );
    assert!(detail.contains("subject"), "{detail}");

    // No topic: C2s.2's page could then only offer "stop everything", and a
    // recipient offered that presses the spam button instead.
    let detail = validation(
        acc.create_campaign(&NewCampaign {
            subject: "Spring prices",
            preheader: None,
            topic: "",
            content: CampaignContent::empty(),
        })
        .await,
    );
    assert!(detail.contains("kind of mail"), "{detail}");

    // A body this build cannot draw in a mail client, refused by name — see
    // `campaign_content`. Constructed as raw JSON because the typed model has
    // no way to express it, which is itself the point.
    let formula = CampaignContent::from_value(json!({
        "schema_version": 1,
        "blocks": [{ "type": "equation", "id": "e1", "latex": "x^2", "numbered": true }],
    }));
    match formula {
        Err(StoreError::Validation(detail)) => assert!(detail.contains("formula"), "{detail}"),
        other => panic!("a formula must be refused before it reaches the table: {other:?}"),
    }

    // Nothing above was written.
    assert!(acc.campaigns(200).await.unwrap().is_empty());
}

#[tokio::test]
async fn the_database_keeps_the_rules_too_rather_than_trusting_the_validator() {
    let store = common::test_store().await;
    let (acc, _, tenant_id) = tenant(&store, "checks").await;
    let written = acc
        .create_campaign(&a_campaign("Spring prices", a_full_body()))
        .await
        .unwrap();

    // A future writer that skipped `validate` must still fail: the migration's
    // CHECKs are the second lock on the fields a recipient sees.
    let pool = &pool().await;
    for (column, value) in [("subject", "   "), ("topic", " ")] {
        let sql = format!("UPDATE campaigns SET {column} = $3 WHERE tenant_id = $1 AND id = $2");
        let attempt = sqlx::query(&sql)
            .bind(tenant_id.as_str())
            .bind(written.id.as_str())
            .bind(value)
            .execute(pool)
            .await;
        assert!(
            attempt.is_err(),
            "the database accepted a blank {column} the validator refuses"
        );
    }
    // A preheader of spaces is not a third state between "set" and "none".
    let blank_preheader =
        sqlx::query("UPDATE campaigns SET preheader = '  ' WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id.as_str())
            .bind(written.id.as_str())
            .execute(pool)
            .await;
    assert!(
        blank_preheader.is_err(),
        "a blank preheader must be refused"
    );

    // And a body with no envelope is refused, so a row nobody can read cannot
    // be created by a path that forgot the version.
    let no_envelope =
        sqlx::query("UPDATE campaigns SET content = '[]'::jsonb WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id.as_str())
            .bind(written.id.as_str())
            .execute(pool)
            .await;
    assert!(
        no_envelope.is_err(),
        "a bare block list says nothing about which model it is written in"
    );

    // The row is exactly as it was.
    assert_eq!(acc.campaign(&written.id).await.unwrap(), Some(written));
}

#[tokio::test]
async fn deleting_a_letter_never_deletes_the_evidence_about_a_person() {
    let store = common::test_store().await;
    let (acc, ts, _) = tenant(&store, "delete").await;

    acc.record_campaign_consent(&NewCampaignConsent {
        address: "ann@example.test",
        source: ConsentSource::Manual,
        statement: "Told us at the counter she wants the newsletter",
        source_ref: None,
        occurred_at: None,
    })
    .await
    .unwrap();
    ts.suppress_campaign_address(&NewSuppression {
        address: "ben@example.test",
        reason: SuppressionReason::Unsubscribe,
        source_ref: None,
        occurred_at: None,
    })
    .await
    .unwrap();

    let written = acc
        .create_campaign(&a_campaign("Spring prices", a_full_body()))
        .await
        .unwrap();
    acc.delete_campaign(&written.id).await.unwrap();
    assert!(acc.campaign(&written.id).await.unwrap().is_none());
    // A second delete is not found rather than a silent success — the caller
    // that thought it held a campaign is told it does not.
    assert!(matches!(
        acc.delete_campaign(&written.id).await,
        Err(StoreError::NotFound)
    ));

    assert_eq!(
        acc.campaign_consent_for("ann@example.test")
            .await
            .unwrap()
            .len(),
        1,
        "consent outlives every campaign — it is how we answer a complaint"
    );
    assert!(
        ts.campaign_suppression_for("ben@example.test")
            .await
            .unwrap()
            .is_some(),
        "a suppression is absolute and cannot be tidied away with a draft"
    );
}

#[tokio::test]
async fn a_campaign_id_from_nowhere_is_simply_absent() {
    let store = common::test_store().await;
    let (acc, _, _) = tenant(&store, "unknown").await;
    let made_up = CampaignId::new("not-an-id-we-issued".to_owned());
    assert!(acc.campaign(&made_up).await.unwrap().is_none());
    assert!(matches!(
        acc.delete_campaign(&made_up).await,
        Err(StoreError::NotFound)
    ));
}
