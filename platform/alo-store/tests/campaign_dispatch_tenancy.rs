//! Compiling one recipient's copy of a campaign (C4.2, ADR 0044; Law 1:
//! isolation is tested, not assumed).
//!
//! What is held here:
//!
//! - **every recipient gets their own token**, because RFC 8058 §7 requires an
//!   unguessable per-recipient URI and one link shared across an audience
//!   unsubscribes whoever clicks it — including somebody the mail was forwarded
//!   to;
//! - **every recipient gets their own words**, resolved from their own record;
//! - **somebody suppressed since the send opened is written off, not mailed** —
//!   the consent and suppression pass happens again at the last moment it can,
//!   which is C2.9's promise;
//! - **a pause stops the dispatcher**, rather than being a flag it may ignore;
//! - a neighbouring workspace cannot prepare, or write off, anybody.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::campaign_dispatch::{DispatchLinks, reason};
use alo_store::{
    AccountStore, AudiencePage, CampaignContent, CampaignSendId, ConsentSource, NewCampaign,
    NewCampaignConsent, NewCustomer, NewSuppression, Store, StoreError, SuppressionReason,
    TenantId, TenantStore,
};
use serde_json::json;
use time::{Duration, OffsetDateTime};

const LINKS: DispatchLinks<'static> = DispatchLinks {
    base_url: "https://alo.test/",
    // Not English: the words are the caller's, in the audience's language.
    link_text: "Uitschrijven",
};

async fn tenant(store: &Store, tag: &str) -> (AccountStore, TenantStore, TenantId) {
    let tenant: TenantId = store.create_tenant(&format!("cdis-{tag}")).await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let user = ts.create_user(&format!("{tag}@cdis.test")).await.unwrap();
    (store.for_account(tenant.clone(), user), ts, tenant)
}

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

/// A letter that greets by name, so a copy rendered against the wrong record is
/// visible in the words rather than only in a count.
fn a_letter() -> CampaignContent {
    CampaignContent::from_value(json!({
        "schema_version": 1,
        "blocks": [
            { "type": "paragraph", "id": "p1", "text": "Hi {{first_name|there}}, prices change on Monday." },
        ],
    }))
    .expect("a body a campaign may carry")
}

/// A campaign, opened as a send that has finished enrolling.
/// Records a warm-up old enough that the day's ceiling is not what is under
/// test. Day 15+ allows 500, which is far past any fixture here.
async fn warmed(acc: &AccountStore) {
    let long_ago = OffsetDateTime::now_utc().date() - Duration::days(20);
    acc.record_campaign_warm_up_start(long_ago).await.unwrap();
}

async fn a_send(acc: &AccountStore) -> CampaignSendId {
    let campaign = acc
        .create_campaign(&NewCampaign {
            subject: "Prices for {{first_name|you}}",
            preheader: Some("From Monday"),
            topic: "Monthly Newsletter",
            content: a_letter(),
        })
        .await
        .unwrap();
    warmed(acc).await;
    let send = acc.open_campaign_send(&campaign.id).await.unwrap();
    let mut after = None;
    loop {
        let page = AudiencePage {
            after: after.clone(),
            limit: 50,
        };
        let done = acc.enrol_campaign_send_page(&send.id, &page).await.unwrap();
        match done.next_cursor {
            None => break,
            Some(cursor) => after = Some(cursor),
        }
    }
    send.id
}

#[tokio::test]
async fn each_recipient_gets_their_own_words_and_their_own_way_out() {
    let store = common::test_store().await;
    let (acc, _, _) = tenant(&store, "each").await;
    mailable(&acc, "Ann Meier", "ann@example.test").await;
    mailable(&acc, "Ben Dupont", "ben@example.test").await;

    let send = a_send(&acc).await;
    let pass = acc
        .prepare_campaign_send_batch(&send, 10, &LINKS)
        .await
        .unwrap();

    assert_eq!(pass.prepared.len(), 2);
    assert_eq!(pass.failed, 0);

    let ann = pass
        .prepared
        .iter()
        .find(|m| m.address == "ann@example.test")
        .expect("Ann's copy");
    let ben = pass
        .prepared
        .iter()
        .find(|m| m.address == "ben@example.test")
        .expect("Ben's copy");

    // Their own words, in the subject and in the body.
    assert_eq!(ann.subject, "Prices for Ann");
    assert_eq!(ben.subject, "Prices for Ben");
    assert!(ann.message.html.contains("Hi Ann,"), "{}", ann.message.html);
    assert!(ben.message.text.contains("Hi Ben,"), "{}", ben.message.text);
    assert!(
        !ann.message.html.contains("Ben") && !ben.message.html.contains("Ann"),
        "one recipient's copy must not carry another's name"
    );

    // Their own token. RFC 8058 §7: one link shared across an audience
    // unsubscribes whoever clicks it, including somebody it was forwarded to.
    let ann_header = &ann.message.headers[0].1;
    let ben_header = &ben.message.headers[0].1;
    assert_ne!(
        ann_header, ben_header,
        "two recipients must never share an unsubscribe link"
    );
    assert!(ann_header.contains("/jmap/campaign-unsubscribe/"));
    // And the footer points at the page a person can read, not the API.
    assert!(
        ann.message.text.contains("https://alo.test/unsubscribe/"),
        "{}",
        ann.message.text
    );
    // RFC 8058 §3.1's literal, verbatim.
    assert_eq!(ann.message.headers[1].1, "List-Unsubscribe=One-Click");
}

#[tokio::test]
async fn somebody_suppressed_since_the_send_opened_is_written_off_rather_than_mailed() {
    let store = common::test_store().await;
    let (acc, ts, _) = tenant(&store, "late").await;
    mailable(&acc, "Ann", "ann@example.test").await;
    mailable(&acc, "Ben", "ben@example.test").await;

    let send = a_send(&acc).await;

    // Ben unsubscribes at 10:00 and the batch goes out at 10:05. C2.9: he must
    // not receive it. The consent and suppression pass is applied again here,
    // at the last moment it can be, rather than trusted from enrolment.
    ts.suppress_campaign_address(&NewSuppression {
        address: "ben@example.test",
        reason: SuppressionReason::Unsubscribe,
        source_ref: None,
        occurred_at: None,
    })
    .await
    .unwrap();

    let pass = acc
        .prepare_campaign_send_batch(&send, 10, &LINKS)
        .await
        .unwrap();

    assert_eq!(pass.prepared.len(), 1, "only Ann");
    assert_eq!(pass.prepared[0].address, "ann@example.test");
    assert_eq!(pass.failed, 1, "and Ben is accounted for");

    // Written off on his own row, with the reason, rather than silently absent.
    let tally = acc.campaign_send_tally(&send).await.unwrap();
    assert_eq!(tally.failed, 1);
    assert_eq!(tally.pending, 1, "Ann is still to be sent to");
    assert_eq!(reason::NO_LONGER_MAILABLE, "no_longer_mailable");
}

#[tokio::test]
async fn one_recipient_failing_does_not_take_the_campaign_with_it() {
    let store = common::test_store().await;
    let (acc, ts, _) = tenant(&store, "isolate").await;
    for who in ["ann", "ben", "cara"] {
        mailable(&acc, who, &format!("{who}@example.test")).await;
    }
    let send = a_send(&acc).await;

    // The middle one, in address order, becomes unmailable.
    ts.suppress_campaign_address(&NewSuppression {
        address: "ben@example.test",
        reason: SuppressionReason::HardBounce,
        source_ref: None,
        occurred_at: None,
    })
    .await
    .unwrap();

    let pass = acc
        .prepare_campaign_send_batch(&send, 10, &LINKS)
        .await
        .unwrap();

    // The pass continued past the failure. A dispatcher that stopped here would
    // leave an operator unable to say which of the rest were reached.
    assert_eq!(pass.failed, 1);
    assert_eq!(pass.prepared.len(), 2);
    let reached: Vec<&str> = pass.prepared.iter().map(|m| m.address.as_str()).collect();
    assert!(reached.contains(&"ann@example.test"));
    assert!(reached.contains(&"cara@example.test"));
}

#[tokio::test]
async fn a_paused_send_hands_nobody_to_submission() {
    let store = common::test_store().await;
    let (acc, _, _) = tenant(&store, "paused").await;
    mailable(&acc, "Ann", "ann@example.test").await;
    let send = a_send(&acc).await;

    acc.pause_campaign_send(&send).await.unwrap();
    // An operator who pressed pause has said stop handing people to
    // submission. A dispatcher that kept preparing would be racing them.
    assert!(matches!(
        acc.prepare_campaign_send_batch(&send, 10, &LINKS).await,
        Err(StoreError::Conflict(_))
    ));

    acc.resume_campaign_send(&send).await.unwrap();
    let pass = acc
        .prepare_campaign_send_batch(&send, 10, &LINKS)
        .await
        .unwrap();
    assert_eq!(pass.prepared.len(), 1, "and resuming lets it work again");
}

#[tokio::test]
async fn a_stopped_send_prepares_nobody_even_with_recipients_still_pending() {
    let store = common::test_store().await;
    let (acc, _, _) = tenant(&store, "stopped").await;
    mailable(&acc, "Ann", "ann@example.test").await;
    let send = a_send(&acc).await;

    acc.stop_campaign_send(&send, Some("typo")).await.unwrap();
    assert!(matches!(
        acc.prepare_campaign_send_batch(&send, 10, &LINKS).await,
        Err(StoreError::Conflict(_))
    ));

    // The pending row survives — what was going to happen is still readable.
    let tally = acc.campaign_send_tally(&send).await.unwrap();
    assert_eq!(tally.pending, 1);
}

#[tokio::test]
async fn a_batch_is_bounded_and_the_bound_is_the_callers_error() {
    let store = common::test_store().await;
    let (acc, _, _) = tenant(&store, "bounded").await;
    mailable(&acc, "Ann", "ann@example.test").await;
    let send = a_send(&acc).await;

    for bad in [0, -1, 10_000] {
        assert!(
            matches!(
                acc.prepare_campaign_send_batch(&send, bad, &LINKS).await,
                Err(StoreError::Validation(_))
            ),
            "{bad} is not a pass size"
        );
    }
}

#[tokio::test]
async fn a_neighbouring_workspace_can_neither_prepare_nor_write_anybody_off() {
    let store = common::test_store().await;
    let (ours, _, _) = tenant(&store, "ours").await;
    let (theirs, _, _) = tenant(&store, "theirs").await;
    mailable(&ours, "Ann", "ann@example.test").await;
    let send = a_send(&ours).await;

    assert!(
        matches!(
            theirs.prepare_campaign_send_batch(&send, 10, &LINKS).await,
            Err(StoreError::NotFound)
        ),
        "a wrong-tenant send id is indistinguishable from a missing one"
    );
    assert!(matches!(
        theirs
            .mark_campaign_recipient_failed(&send, "ann@example.test", reason::RENDER_REFUSED)
            .await,
        Err(StoreError::NotFound)
    ));
    assert!(
        theirs
            .campaign_send_pending(&send, 10)
            .await
            .unwrap()
            .is_empty()
    );

    // And ours is untouched by any of it.
    let tally = ours.campaign_send_tally(&send).await.unwrap();
    assert_eq!(tally.pending, 1);
    assert_eq!(tally.failed, 0);
}

#[tokio::test]
async fn a_recipient_already_sent_to_cannot_be_written_off_by_a_retry() {
    let store = common::test_store().await;
    let (acc, _, _) = tenant(&store, "settled").await;
    mailable(&acc, "Ann", "ann@example.test").await;
    let send = a_send(&acc).await;

    assert!(
        acc.mark_campaign_recipient_sent(&send, "ann@example.test")
            .await
            .unwrap()
    );
    // The mail has gone. The row is the only record that it did, and a
    // dispatcher retrying after a crash must not be able to rewrite it.
    assert!(
        !acc.mark_campaign_recipient_failed(&send, "ann@example.test", reason::RENDER_REFUSED)
            .await
            .unwrap()
    );

    let tally = acc.campaign_send_tally(&send).await.unwrap();
    assert_eq!(tally.sent, 1);
    assert_eq!(tally.failed, 0);
}

#[tokio::test]
async fn the_warm_up_ceiling_bounds_the_day_and_says_so() {
    let store = common::test_store().await;
    let (acc, _, _) = tenant(&store, "ceiling").await;
    for who in ["ann", "ben", "cara", "dan"] {
        mailable(&acc, who, &format!("{who}@example.test")).await;
    }
    let send = a_send(&acc).await;

    // Back to day 1, where the published schedule allows five a day. Four
    // recipients is under it, so this pass is not the interesting one — it is
    // what makes the next assertion mean something.
    acc.record_campaign_warm_up_start(OffsetDateTime::now_utc().date())
        .await
        .unwrap();

    let pass = acc
        .prepare_campaign_send_batch(&send, 500, &LINKS)
        .await
        .unwrap();
    assert_eq!(pass.allowance.day, 1);
    assert_eq!(pass.allowance.ceiling, 5, "the published day-1 ceiling");
    assert_eq!(
        pass.prepared.len(),
        4,
        "a caller asking for 500 on day one gets what the schedule allows"
    );

    // Spend the day: mark all four sent, then ask again.
    for who in ["ann", "ben", "cara", "dan"] {
        assert!(
            acc.mark_campaign_recipient_sent(&send, &format!("{who}@example.test"))
                .await
                .unwrap()
        );
    }
    let after = acc.campaign_send_allowance().await.unwrap();
    assert_eq!(after.sent_today, 4);
    assert_eq!(after.remaining, 1, "five minus the four that have gone");
}

#[tokio::test]
async fn a_tenant_with_no_recorded_warm_up_may_send_nothing() {
    let store = common::test_store().await;
    let (acc, _, _) = tenant(&store, "unwarmed").await;
    mailable(&acc, "Ann", "ann@example.test").await;

    // Deliberately not calling `warmed`. An identity nobody has started warming
    // is one nobody has checked the DNS of, and the cost of sending from it is
    // months of deliverability rather than a refused request.
    let campaign = acc
        .create_campaign(&NewCampaign {
            subject: "Nothing yet",
            preheader: None,
            topic: "Monthly Newsletter",
            content: a_letter(),
        })
        .await
        .unwrap();
    let send = acc.open_campaign_send(&campaign.id).await.unwrap();
    // Walked to exhaustion, so the send reaches `sending` — otherwise this
    // would be testing the enrolling-state refusal rather than the ceiling.
    let mut after = None;
    loop {
        let page = AudiencePage {
            after: after.clone(),
            limit: 50,
        };
        let done = acc.enrol_campaign_send_page(&send.id, &page).await.unwrap();
        match done.next_cursor {
            None => break,
            Some(cursor) => after = Some(cursor),
        }
    }

    let pass = acc
        .prepare_campaign_send_batch(&send.id, 10, &LINKS)
        .await
        .unwrap();
    assert_eq!(pass.allowance.day, 0);
    assert_eq!(pass.allowance.ceiling, 0);
    assert!(pass.prepared.is_empty(), "nobody is handed to submission");
    assert_eq!(pass.failed, 0, "and nobody is written off for it either");
    assert!(
        pass.allowance.is_exhausted(),
        "the caller is told why, rather than left with an empty list"
    );
}

#[tokio::test]
async fn the_ceiling_is_the_identitys_and_is_shared_across_campaigns() {
    let store = common::test_store().await;
    let (acc, _, _) = tenant(&store, "shared").await;
    for who in ["ann", "ben", "cara", "dan", "eve"] {
        mailable(&acc, who, &format!("{who}@example.test")).await;
    }

    // One campaign spends most of day one.
    let first = a_send(&acc).await;
    acc.record_campaign_warm_up_start(OffsetDateTime::now_utc().date())
        .await
        .unwrap();
    let pass = acc
        .prepare_campaign_send_batch(&first, 500, &LINKS)
        .await
        .unwrap();
    assert_eq!(pass.prepared.len(), 5, "day one allows five");
    for who in ["ann", "ben", "cara", "dan"] {
        acc.mark_campaign_recipient_sent(&first, &format!("{who}@example.test"))
            .await
            .unwrap();
    }

    // A second campaign the same day does not get a fresh five. The reputation
    // being spent belongs to the sending identity, not to a campaign.
    acc.stop_campaign_send(&first, Some("done for today"))
        .await
        .unwrap();
    let second = a_send(&acc).await;
    // `a_send` records a warm-up of its own so its enrolment is not the thing
    // under test; put the clock back to day one now that it has.
    acc.record_campaign_warm_up_start(OffsetDateTime::now_utc().date())
        .await
        .unwrap();
    let pass = acc
        .prepare_campaign_send_batch(&second, 500, &LINKS)
        .await
        .unwrap();
    assert_eq!(pass.allowance.sent_today, 4);
    assert_eq!(pass.allowance.remaining, 1);
    assert_eq!(
        pass.prepared.len(),
        1,
        "the second campaign gets what is left of the day, not a new day"
    );
}
