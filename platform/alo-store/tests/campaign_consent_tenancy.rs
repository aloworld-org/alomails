//! Consent as a record, and the people it keeps out of a send (C1.2, ADR 0044
//! §2; Law 1: isolation is tested, not assumed).
//!
//! The queue's definition of done for this module: *an item that touches who
//! may be mailed is not done without a test that proves who may not be.* So the
//! assertions here are mostly about absence:
//!
//! - a person the tenant knows perfectly well, with no consent record, is in
//!   the audience and is **not** a recipient — and the count says so too;
//! - a neighbouring tenant's consent record does not make our address mailable,
//!   even when both tenants hold the same person;
//! - consent for somebody no source holds does not invent a recipient;
//! - a record that cannot say what was agreed, or where an import came from, or
//!   that is dated in the future, is refused at the door rather than stored as
//!   evidence.
//!
//! And one thing about presence: consent is **provenance, not a boolean**, so a
//! second agreement joins the first rather than replacing it, and the history
//! answers "how do we know" with the statement the tenant actually gave.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use time::{Duration, OffsetDateTime};

use alo_store::{
    AccountStore, AudiencePage, AudienceSource, ConsentSource, NewCampaignConsent, NewCustomer,
    NewDeal, PipelineSeed, StageSeed, Store, StoreError,
};

/// A tenant with one user, and the account door for them.
async fn account(store: &Store, tag: &str) -> AccountStore {
    let tenant = store.create_tenant(&format!("ccon-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@ccon.test"))
        .await
        .unwrap();
    store.for_account(tenant, user)
}

/// A customer the tenant invoices.
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

/// The contact on a CRM deal.
async fn deal(store: &AccountStore, title: &str, contact_email: &str) {
    let boards = store
        .crm_pipelines_or_seed(&PipelineSeed {
            name: "Sales".to_owned(),
            stages: vec![StageSeed {
                name: "New".to_owned(),
                is_won: false,
                is_lost: false,
            }],
        })
        .await
        .unwrap();
    let board = boards[0].id.clone();
    let stage = store.crm_stages(&board, false).await.unwrap()[0].id.clone();
    store
        .create_crm_deal(
            &board,
            &stage,
            &NewDeal {
                title: title.to_owned(),
                contact_name: "Ann Dupont".to_owned(),
                contact_email: contact_email.to_owned(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
}

/// A plain consent record: somebody said yes, and here is what they said yes to.
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

/// Every address in the audience, in order, read a page at a time so the paging
/// path is the one under test everywhere.
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

/// Every address the tenant may actually mail, paged the same way.
///
/// Paged deliberately: the first page of a keyset walk is the one where a
/// mis-bracketed `WHERE` would let unconsented people through, and a test that
/// only ever read one page would not notice.
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
async fn a_person_with_no_consent_record_cannot_be_a_recipient() {
    let store = common::test_store().await;
    let a = account(&store, "gate").await;

    customer(&a, "Acme BV", "orders@acme.test").await;
    deal(&a, "Spring order", "ann@lead.test").await;
    customer(&a, "Bravo NV", "hello@bravo.test").await;

    // All three are people the tenant knows. Exactly one of them agreed.
    agreed(&a, "ann@lead.test", "Ticked the newsletter box in the shop").await;

    assert_eq!(
        audience(&a).await,
        ["ann@lead.test", "hello@bravo.test", "orders@acme.test"],
        "the audience shows everybody, including the people we may not mail"
    );
    assert_eq!(a.campaign_audience_size().await.unwrap(), 3);

    assert_eq!(
        recipients(&a).await,
        ["ann@lead.test"],
        "somebody the tenant knows is still not somebody it may mail"
    );
    assert_eq!(a.campaign_recipient_count().await.unwrap(), 1);

    // The exclusion is readable rather than merely true: the audience says who
    // was left out and, for the one who was not, why they were kept.
    let members = a
        .campaign_audience(&AudiencePage {
            after: None,
            limit: 10,
        })
        .await
        .unwrap();
    let excluded: Vec<&str> = members
        .iter()
        .filter(|m| m.consent.is_none())
        .map(|m| m.address.as_str())
        .collect();
    assert_eq!(excluded, ["hello@bravo.test", "orders@acme.test"]);

    let kept = members
        .iter()
        .find(|m| m.address == "ann@lead.test")
        .expect("the consented person is in the audience too");
    let evidence = kept.consent.as_ref().expect("with their consent attached");
    assert_eq!(evidence.source, ConsentSource::Manual);
    let record = a
        .campaign_consent_for("ann@lead.test")
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.id == evidence.record)
        .expect("the evidence names a record that can be read");
    assert_eq!(record.statement, "Ticked the newsletter box in the shop");
}

#[tokio::test]
async fn a_neighbours_consent_does_not_make_our_address_mailable() {
    let store = common::test_store().await;
    let a = account(&store, "mine").await;
    let b = account(&store, "theirs").await;

    // The sharpest case: both tenants hold the same person, and only one of
    // them was given permission.
    customer(&a, "Acme BV", "orders@acme.test").await;
    customer(&b, "Rival NV", "orders@acme.test").await;
    agreed(&b, "orders@acme.test", "Signed up at our trade stand").await;

    assert_eq!(audience(&a).await, ["orders@acme.test"]);
    assert_eq!(
        recipients(&a).await,
        Vec::<String>::new(),
        "a neighbour's permission is not ours"
    );
    assert_eq!(a.campaign_recipient_count().await.unwrap(), 0);
    assert!(
        a.campaign_consent_for("orders@acme.test")
            .await
            .unwrap()
            .is_empty(),
        "and their evidence is not even readable from here"
    );

    // Stated from both sides, so a leak would have to show up as a named row
    // rather than as a missing one.
    assert_eq!(recipients(&b).await, ["orders@acme.test"]);
    assert_eq!(b.campaign_recipient_count().await.unwrap(), 1);
    assert_eq!(
        b.campaign_consent_for("orders@acme.test")
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn consent_for_somebody_no_source_holds_does_not_invent_a_recipient() {
    let store = common::test_store().await;
    let a = account(&store, "ghost").await;

    // A record can be written for any address — an import arrives before the
    // customer does — but consent is permission, not existence. There is no
    // list to be on (ADR 0044), so a person with no record anywhere is nobody
    // to mail.
    agreed(&a, "nobody@stranger.test", "Asked to hear from us by post").await;

    assert_eq!(audience(&a).await, Vec::<String>::new());
    assert_eq!(recipients(&a).await, Vec::<String>::new());
    assert_eq!(a.campaign_recipient_count().await.unwrap(), 0);
    assert_eq!(
        a.campaign_consent_for("nobody@stranger.test")
            .await
            .unwrap()
            .len(),
        1,
        "the evidence is kept, so it still counts the day they become a customer"
    );
}

#[tokio::test]
async fn consent_is_provenance_and_a_second_agreement_joins_the_first() {
    let store = common::test_store().await;
    let a = account(&store, "history").await;
    customer(&a, "Acme BV", "orders@acme.test").await;

    let old = OffsetDateTime::now_utc() - Duration::days(400);
    a.record_campaign_consent(&NewCampaignConsent {
        address: "orders@acme.test",
        source: ConsentSource::Import,
        source_ref: Some("trade-fair-2025.csv"),
        statement: "Trade fair sign-up sheet, opt-in box ticked",
        occurred_at: Some(old),
    })
    .await
    .unwrap();

    let recent = OffsetDateTime::now_utc() - Duration::days(2);
    a.record_campaign_consent(&NewCampaignConsent {
        address: "ORDERS@Acme.TEST ",
        source: ConsentSource::SiteForm,
        source_ref: Some("newsletter-form"),
        statement: "Re-confirmed on the newsletter form",
        occurred_at: Some(recent),
    })
    .await
    .unwrap();

    // Both are kept, freshest first, and the casing of the second did not
    // create a second person.
    let history = a.campaign_consent_for("orders@acme.test").await.unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].source, ConsentSource::SiteForm);
    assert_eq!(
        history[0].statement, "Re-confirmed on the newsletter form",
        "the answer to 'how do we know' is the tenant's own words"
    );
    assert_eq!(history[0].source_ref.as_deref(), Some("newsletter-form"));
    assert_eq!(history[0].recorded_by, *a.user());
    assert_eq!(history[1].source, ConsentSource::Import);
    assert_eq!(
        history[1].source_ref.as_deref(),
        Some("trade-fair-2025.csv")
    );
    assert!(
        history[1].occurred_at < history[1].recorded_at,
        "an import's consent is older than the day it was typed in, and the row says so"
    );

    // One person, one recipient, carrying the freshest evidence.
    let people = a
        .campaign_recipients(&AudiencePage {
            after: None,
            limit: 10,
        })
        .await
        .unwrap();
    assert_eq!(people.len(), 1);
    assert_eq!(people[0].address, "orders@acme.test");
    assert_eq!(people[0].sources, [AudienceSource::BillingCustomer]);
    assert_eq!(people[0].consent.source, ConsentSource::SiteForm);
    assert_eq!(people[0].consent.record, history[0].id);
    assert_eq!(a.campaign_recipient_count().await.unwrap(), 1);
}

#[tokio::test]
async fn a_record_that_is_not_evidence_is_refused_rather_than_stored() {
    let store = common::test_store().await;
    let a = account(&store, "refuse").await;
    customer(&a, "Acme BV", "orders@acme.test").await;

    let base = NewCampaignConsent {
        address: "orders@acme.test",
        source: ConsentSource::Manual,
        source_ref: None,
        statement: "Said yes at the counter",
        occurred_at: None,
    };

    // Nothing that was agreed.
    let blank = NewCampaignConsent {
        statement: "   ",
        ..base.clone()
    };
    assert!(matches!(
        a.record_campaign_consent(&blank).await,
        Err(StoreError::Validation(_))
    ));

    // An import that cannot say where its addresses came from — the path ADR
    // 0044 §2 singles out as dangerous.
    let anonymous_import = NewCampaignConsent {
        source: ConsentSource::Import,
        source_ref: None,
        ..base.clone()
    };
    assert!(matches!(
        a.record_campaign_consent(&anonymous_import).await,
        Err(StoreError::Validation(_))
    ));

    // Consent dated after it was given: either a typo or an attempt to make a
    // stale agreement look fresh.
    let ahead = NewCampaignConsent {
        occurred_at: Some(OffsetDateTime::now_utc() + Duration::hours(2)),
        ..base.clone()
    };
    assert!(matches!(
        a.record_campaign_consent(&ahead).await,
        Err(StoreError::Validation(_))
    ));

    // Something that is not an address at all.
    let junk = NewCampaignConsent {
        address: "ask reception",
        ..base.clone()
    };
    assert!(matches!(
        a.record_campaign_consent(&junk).await,
        Err(StoreError::Validation(_))
    ));
    assert!(matches!(
        a.campaign_consent_for("ask reception").await,
        Err(StoreError::Validation(_))
    ));

    // Every refusal above wrote nothing: the customer is still unmailable.
    assert_eq!(recipients(&a).await, Vec::<String>::new());
    assert_eq!(a.campaign_recipient_count().await.unwrap(), 0);
    assert!(
        a.campaign_consent_for("orders@acme.test")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn consent_recorded_in_any_casing_reaches_the_person_it_was_given_for() {
    let store = common::test_store().await;
    let a = account(&store, "fold").await;

    // The source spells them one way, the person consented in another. One
    // person, mailed once, or the unsubscribe of one copy would not reach the
    // other.
    deal(&a, "Spring order", "Ann.Dupont@Example.TEST").await;
    agreed(&a, "  ann.dupont@example.test ", "Replied yes to our email").await;

    assert_eq!(recipients(&a).await, ["ann.dupont@example.test"]);
    assert_eq!(a.campaign_recipient_count().await.unwrap(), 1);
    assert_eq!(
        a.campaign_consent_for("ANN.DUPONT@example.test")
            .await
            .unwrap()
            .len(),
        1,
        "and the history is readable however it is asked for"
    );
}
