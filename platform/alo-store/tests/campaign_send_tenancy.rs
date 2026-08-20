//! The send ledger, and the two promises it exists to keep (C4.1, ADR 0044;
//! Law 1: isolation is tested, not assumed).
//!
//! What is held here:
//!
//! - **nobody is mailed twice** — and specifically, not by a *second send of
//!   the same campaign*, which is the accident the weaker per-send uniqueness
//!   would let through: press send, spot the typo, stop, fix, press send again;
//! - enrolment **resumes rather than restarts**, so re-running a page a caller
//!   is unsure about writes nothing the second time;
//! - somebody who declined the topic is **recorded as skipped**, not quietly
//!   dropped, because a send that reports "900 of 1000" with no account of the
//!   hundred is a number nobody can defend;
//! - a neighbouring workspace's campaigns and sends are unreachable through
//!   every verb, and reach `NotFound` rather than a database error;
//! - the state machine refuses the transitions it should and is idempotent on
//!   the one an operator presses twice while panicking.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::campaign_send::{RecipientState, SendState, reason};
use alo_store::{
    AccountStore, AudiencePage, CampaignContent, CampaignSendId, ConsentSource, NewCampaign,
    NewCampaignConsent, NewCustomer, NewTopicOptOut, Store, StoreError, TenantId, TenantStore,
};

/// A tenant with one user: the account door for campaigns and sends, the tenant
/// door for preferences, which the public landing page writes with no login.
async fn tenant(store: &Store, tag: &str) -> (AccountStore, TenantStore, TenantId) {
    let tenant: TenantId = store.create_tenant(&format!("csnd-{tag}")).await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let user = ts.create_user(&format!("{tag}@csnd.test")).await.unwrap();
    (store.for_account(tenant.clone(), user), ts, tenant)
}

/// Somebody this tenant may actually mail: a customer record to be found in,
/// and a consent record to be allowed by. Both are needed — the audience is the
/// join of the two.
async fn mailable(acc: &AccountStore, name: &str, email: &str) {
    acc.create_billing_customer(&NewCustomer {
        name: name.to_owned(),
        country: "DE".to_owned(),
        email: Some(email.to_owned()),
        ..Default::default()
    })
    .await
    .unwrap();
    acc.record_campaign_consent(&NewCampaignConsent {
        address: email,
        source: ConsentSource::Manual,
        statement: "Asked for the newsletter at the counter",
        source_ref: None,
        occurred_at: None,
    })
    .await
    .unwrap();
}

fn a_campaign(subject: &str) -> NewCampaign<'_> {
    NewCampaign {
        subject,
        preheader: Some("Ten per cent off until Friday"),
        topic: "Monthly Newsletter",
        content: CampaignContent::empty(),
    }
}

/// Walks enrolment to exhaustion, the way a caller is meant to.
async fn enrol_all(acc: &AccountStore, send: &CampaignSendId) -> (i64, i64, i64) {
    let (mut enrolled, mut skipped, mut already) = (0, 0, 0);
    let mut after = None;
    loop {
        let page = AudiencePage {
            after: after.clone(),
            limit: 2,
        };
        let done = acc.enrol_campaign_send_page(send, &page).await.unwrap();
        enrolled += done.enrolled;
        skipped += done.skipped;
        already += done.already_enrolled;
        match done.next_cursor {
            None => break,
            Some(cursor) => after = Some(cursor),
        }
    }
    (enrolled, skipped, already)
}

#[tokio::test]
async fn a_second_send_of_one_campaign_cannot_reach_anybody_it_already_did() {
    let store = common::test_store().await;
    let (acc, _, _) = tenant(&store, "twice").await;
    mailable(&acc, "Ann", "ann@example.test").await;
    mailable(&acc, "Ben", "ben@example.test").await;

    let campaign = acc.create_campaign(&a_campaign("Spring")).await.unwrap();

    let first = acc.open_campaign_send(&campaign.id).await.unwrap();
    let (enrolled, _, _) = enrol_all(&acc, &first.id).await;
    assert_eq!(enrolled, 2, "both mailable people are enrolled once");

    // The operator spots the typo and stops the send.
    let stopped = acc
        .stop_campaign_send(&first.id, Some("typo"))
        .await
        .unwrap();
    assert_eq!(stopped.state, SendState::Stopped);

    // They fix it and press send again. THIS is the accident the campaign-wide
    // uniqueness exists for: with per-send uniqueness both people would be
    // enrolled a second time and receive the letter twice.
    let second = acc.open_campaign_send(&campaign.id).await.unwrap();
    let (enrolled, _, already) = enrol_all(&acc, &second.id).await;
    assert_eq!(enrolled, 0, "nobody is enrolled a second time");
    assert_eq!(already, 2, "and the ledger says why, rather than silently");

    let tally = acc.campaign_send_tally(&second.id).await.unwrap();
    assert_eq!(tally.total(), 0, "the second send reaches nobody at all");
}

#[tokio::test]
async fn re_running_a_page_writes_nothing_the_second_time() {
    let store = common::test_store().await;
    let (acc, _, _) = tenant(&store, "resume").await;
    mailable(&acc, "Ann", "ann@example.test").await;
    mailable(&acc, "Ben", "ben@example.test").await;

    let campaign = acc.create_campaign(&a_campaign("Resume")).await.unwrap();
    let send = acc.open_campaign_send(&campaign.id).await.unwrap();

    let page = AudiencePage {
        after: None,
        limit: 2,
    };
    let first = acc.enrol_campaign_send_page(&send.id, &page).await.unwrap();
    assert_eq!(first.enrolled, 2);

    // A caller that crashed and does not know whether its last page landed asks
    // for it again. This must be a no-op, because it is the intended recovery.
    let again = acc.enrol_campaign_send_page(&send.id, &page).await.unwrap();
    assert_eq!(again.enrolled, 0);
    assert_eq!(again.already_enrolled, 2);

    let tally = acc.campaign_send_tally(&send.id).await.unwrap();
    assert_eq!(tally.pending, 2, "still two people, not four");
}

#[tokio::test]
async fn somebody_who_declined_the_topic_is_recorded_rather_than_dropped() {
    let store = common::test_store().await;
    let (acc, ts, _) = tenant(&store, "declined").await;
    mailable(&acc, "Ann", "ann@example.test").await;
    mailable(&acc, "Ben", "ben@example.test").await;

    // Ben asked for less, not for nothing — he is still mailable, just not
    // about this.
    ts.decline_campaign_topic(&NewTopicOptOut {
        address: "ben@example.test",
        topic: "Monthly Newsletter",
        source_ref: None,
        occurred_at: None,
    })
    .await
    .unwrap();

    let campaign = acc.create_campaign(&a_campaign("Spring")).await.unwrap();
    let send = acc.open_campaign_send(&campaign.id).await.unwrap();
    let (enrolled, skipped, _) = enrol_all(&acc, &send.id).await;

    assert_eq!(enrolled, 1, "only Ann is to be mailed");
    assert_eq!(skipped, 1, "and Ben is accounted for rather than missing");

    let tally = acc.campaign_send_tally(&send.id).await.unwrap();
    assert_eq!(tally.pending, 1);
    assert_eq!(tally.skipped, 1);
    assert_eq!(
        tally.total(),
        2,
        "the tally accounts for everybody the audience offered"
    );
}

#[tokio::test]
async fn the_state_machine_refuses_what_it_should_and_repeats_what_it_must() {
    let store = common::test_store().await;
    let (acc, _, _) = tenant(&store, "states").await;
    mailable(&acc, "Ann", "ann@example.test").await;

    let campaign = acc.create_campaign(&a_campaign("States")).await.unwrap();
    let send = acc.open_campaign_send(&campaign.id).await.unwrap();
    assert_eq!(send.state, SendState::Enrolling);
    assert!(send.enrolled_at.is_none(), "nothing has been walked yet");

    // Pausing something that is still enrolling is a conflict: there is nothing
    // being dispatched to pause.
    assert!(matches!(
        acc.pause_campaign_send(&send.id).await,
        Err(StoreError::Conflict(_))
    ));

    enrol_all(&acc, &send.id).await;
    let sending = acc.campaign_send(&send.id).await.unwrap().unwrap();
    assert_eq!(sending.state, SendState::Sending);
    assert!(
        sending.enrolled_at.is_some(),
        "finishing the walk is recorded, so 'nobody enrolled yet' and 'nobody was eligible' differ"
    );

    // A settled audience cannot be added to.
    let page = AudiencePage {
        after: None,
        limit: 2,
    };
    assert!(matches!(
        acc.enrol_campaign_send_page(&send.id, &page).await,
        Err(StoreError::Conflict(_))
    ));

    acc.pause_campaign_send(&send.id).await.unwrap();
    assert!(
        matches!(
            acc.pause_campaign_send(&send.id).await,
            Err(StoreError::Conflict(_))
        ),
        "pausing twice is a conflict — the second press means something already true"
    );
    let resumed = acc.resume_campaign_send(&send.id).await.unwrap();
    assert_eq!(resumed.state, SendState::Sending);

    let stopped = acc
        .stop_campaign_send(&send.id, Some("  wrong list  "))
        .await
        .unwrap();
    assert_eq!(stopped.state, SendState::Stopped);
    assert_eq!(
        stopped.stopped_note.as_deref(),
        Some("wrong list"),
        "the note is trimmed, and a blank one is no note at all"
    );

    // Stop is idempotent ON PURPOSE: the operator pressing it twice means the
    // same thing both times, and the second press must not be an error at the
    // exact moment they are panicking about what is going out.
    let again = acc.stop_campaign_send(&send.id, None).await.unwrap();
    assert_eq!(again.state, SendState::Stopped);
    assert_eq!(
        again.stopped_note.as_deref(),
        Some("wrong list"),
        "and it does not erase the reason the first press recorded"
    );
}

#[tokio::test]
async fn one_campaign_may_not_have_two_sends_running_at_once() {
    let store = common::test_store().await;
    let (acc, _, _) = tenant(&store, "one-live").await;
    mailable(&acc, "Ann", "ann@example.test").await;

    let campaign = acc.create_campaign(&a_campaign("Live")).await.unwrap();
    let first = acc.open_campaign_send(&campaign.id).await.unwrap();

    // Two live sends of one campaign is the state in which two dispatchers race
    // for the same recipients.
    assert!(matches!(
        acc.open_campaign_send(&campaign.id).await,
        Err(StoreError::Conflict(_))
    ));

    // Stopping the first releases the campaign for another attempt.
    acc.stop_campaign_send(&first.id, None).await.unwrap();
    let second = acc.open_campaign_send(&campaign.id).await.unwrap();
    assert_ne!(second.id, first.id);

    let sends = acc.campaign_sends(&campaign.id).await.unwrap();
    assert_eq!(
        sends.len(),
        2,
        "both acts are kept — what happened, happened"
    );
}

#[tokio::test]
async fn a_neighbouring_workspace_reaches_none_of_this() {
    let store = common::test_store().await;
    let (ours, _, _) = tenant(&store, "ours").await;
    let (theirs, _, _) = tenant(&store, "theirs").await;
    mailable(&ours, "Ann", "ann@example.test").await;

    let campaign = ours.create_campaign(&a_campaign("Ours")).await.unwrap();
    let send = ours.open_campaign_send(&campaign.id).await.unwrap();

    // Opening a send against our campaign: NotFound, and specifically not a
    // foreign-key error, which would confirm the id exists somewhere.
    assert!(
        matches!(
            theirs.open_campaign_send(&campaign.id).await,
            Err(StoreError::NotFound)
        ),
        "a wrong-tenant campaign id is indistinguishable from a missing one"
    );

    // Reading it.
    assert_eq!(theirs.campaign_send(&send.id).await.unwrap(), None);
    assert!(
        theirs
            .campaign_sends(&campaign.id)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        theirs.campaign_send_tally(&send.id).await.unwrap().total(),
        0
    );

    // Enrolling into it, and every control verb.
    let page = AudiencePage {
        after: None,
        limit: 2,
    };
    assert!(matches!(
        theirs.enrol_campaign_send_page(&send.id, &page).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        theirs.pause_campaign_send(&send.id).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        theirs.resume_campaign_send(&send.id).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        theirs.stop_campaign_send(&send.id, None).await,
        Err(StoreError::NotFound)
    ));

    // And ours is untouched by all of that.
    let ours_still = ours.campaign_send(&send.id).await.unwrap().expect("stored");
    assert_eq!(ours_still.state, SendState::Enrolling);
}

#[tokio::test]
async fn an_unknown_send_is_not_found_rather_than_a_database_error() {
    let store = common::test_store().await;
    let (acc, _, _) = tenant(&store, "missing").await;
    let nobody = CampaignSendId::generate();

    assert_eq!(acc.campaign_send(&nobody).await.unwrap(), None);
    assert!(matches!(
        acc.pause_campaign_send(&nobody).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        acc.stop_campaign_send(&nobody, None).await,
        Err(StoreError::NotFound)
    ));
}

#[tokio::test]
async fn a_send_of_an_empty_audience_is_finished_rather_than_stuck() {
    let store = common::test_store().await;
    let (acc, _, _) = tenant(&store, "empty").await;
    // Nobody is mailable at all — no customers, no consent.

    let campaign = acc.create_campaign(&a_campaign("Nobody")).await.unwrap();
    let send = acc.open_campaign_send(&campaign.id).await.unwrap();
    let (enrolled, skipped, _) = enrol_all(&acc, &send.id).await;
    assert_eq!((enrolled, skipped), (0, 0));

    let read = acc.campaign_send(&send.id).await.unwrap().unwrap();
    assert_eq!(
        read.state,
        SendState::Sending,
        "an empty audience finishes enrolling rather than leaving the send stuck"
    );
    assert!(
        read.enrolled_at.is_some(),
        "and says so, which is what separates it from 'nobody enrolled yet'"
    );
}

#[tokio::test]
async fn the_skip_reason_is_the_one_the_module_publishes() {
    // Guards the code the tally and any future screen will group by: a reason
    // string changed here without changing the constant is a silent break.
    assert_eq!(reason::TOPIC_DECLINED, "topic_declined");
    assert_eq!(RecipientState::Skipped.as_str(), "skipped");
}
