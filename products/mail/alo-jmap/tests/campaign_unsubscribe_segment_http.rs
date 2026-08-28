//! An unsubscribe reaches the saved segment at once (C2s.3, ADR 0044 §2) —
//! driven end to end through the real router over a real Postgres.
//!
//! The queue item: *an unsubscribe suppresses immediately through C1.3, and a
//! test proves a recipient who unsubscribes cannot appear in a segment
//! evaluated one second later.* Both halves already exist — the landing page
//! writes a suppression (C2s.2), and the audience CTEs exclude suppressed
//! people in SQL (C1.3) — so what is unproven is the **join between them**, and
//! that is what this file is. Every other campaign suite tests one half: the
//! store suites suppress by calling the store, and the landing-page suite reads
//! the raw audience back. Neither shows a colleague's saved question changing
//! its answer because a stranger with no account pressed a button.
//!
//! Three things are asserted here that no other suite asserts:
//!
//! - **The segment answers differently with no write to the segment.** The
//!   stored row is re-read and compared before and after: a segment holds
//!   conditions, never people, so nothing refreshes and nothing is invalidated
//!   (ADR 0044: *there is nothing to sync, because there is no list*). A cached
//!   membership would pass every store-level test in the tree and would still
//!   mail somebody on Tuesday who unsubscribed on Monday.
//! - **Immediately, not eventually.** The item says "one second later"; this
//!   re-asks the question in the same breath as the press, with no sleep, which
//!   is the stronger claim. There is no window in which a send could still pick
//!   them up.
//! - **The count that dropped says who dropped out of it.** `mailable` falls by
//!   one *and* `suppressed:unsubscribe` gains one, with `matched` unmoved: they
//!   were selected by the conditions and were not mailed, and a screen that
//!   showed the smaller number alone would be a count nobody could audit — the
//!   failure C1.4 exists to prevent.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::{
    AudiencePage, ConsentSource, NewCampaignConsent, NewCustomer, NewUnsubscribeToken,
    SegmentConditions,
};

use crate::common::{Harness, harness, harness_on, send};

// ---- request helpers ---------------------------------------------------------

/// A colleague's read, with their bearer token.
async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(
        app,
        Request::builder()
            .method("GET")
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

/// A colleague's write.
async fn post(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(
        app,
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

/// The recipient's press, from their mail. **No `Authorization` header**: the
/// person holding the link has no account here and never will.
async fn press(app: &Router, token: &str, scope: &str) -> (StatusCode, Value) {
    send(
        app,
        Request::builder()
            .method("POST")
            .uri(format!("/jmap/campaign-unsubscribe/{token}"))
            .header("content-type", "application/json")
            .body(Body::from(json!({ "scope": scope }).to_string()))
            .unwrap(),
    )
    .await
}

// ---- reading a segment back the way the screen does --------------------------

/// The query string a screen builds from a segment it has just read back.
///
/// Deliberately assembled from the **route's own answer** rather than from the
/// values the test posted: a saved segment is counted by putting its conditions
/// back on the URL of `/campaigns/audience*`, so if the two shapes ever drift
/// apart, a saved segment would silently be counted as a different question.
/// That round trip is part of what this file proves.
fn query_of(conditions: &Value) -> String {
    let mut parts: Vec<String> = Vec::new();
    let countries: Vec<String> = conditions["countries"]
        .as_array()
        .map(|codes| {
            codes
                .iter()
                .map(|code| code.as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default();
    if !countries.is_empty() {
        parts.push(format!("countries={}", countries.join(",")));
    }
    if let Some(purchase) = conditions["purchase"].as_object() {
        parts.push(format!(
            "purchase={}",
            purchase["condition"].as_str().unwrap_or_default()
        ));
        if let Some(days) = purchase["withinDays"].as_i64() {
            parts.push(format!("withinDays={days}"));
        }
    }
    parts.join("&")
}

/// A saved segment, and the query string that asks it.
struct Saved {
    id: String,
    query: String,
}

/// Saves a segment through the route a colleague uses, then reads it back and
/// builds the question from what came back.
async fn save_segment(h: &Harness, name: &str, countries: &[&str]) -> Saved {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/campaigns/segments",
        json!({ "name": name, "conditions": { "countries": countries } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id = body["segment"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no segment id in {body}"))
        .to_owned();

    let (status, reread) = get(&h.app, &h.token, &format!("/campaigns/segments/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{reread}");
    Saved {
        query: query_of(&reread["segment"]["conditions"]),
        id,
    }
}

/// The stored segment as the route answers it — used to prove the row itself is
/// untouched by a press.
async fn stored(h: &Harness, saved: &Saved) -> Value {
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/campaigns/segments/{}", saved.id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["segment"].clone()
}

/// A question's count, as `(mailable, matched, exclusions)`.
///
/// Takes the query string rather than the saved segment, because that is what
/// the surface takes: a saved segment and a half-typed one are counted by one
/// code path (`GET /campaigns/audience/tally`), so this helper can ask both.
async fn tally(h: &Harness, query: &str) -> (i64, i64, Vec<(String, i64)>) {
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/campaigns/audience/tally?{query}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let excluded = body["tally"]["excluded"]
        .as_array()
        .unwrap_or_else(|| panic!("no exclusions in {body}"))
        .iter()
        .map(|bucket| {
            (
                bucket["reason"].as_str().unwrap_or_default().to_owned(),
                bucket["people"].as_i64().unwrap_or_default(),
            )
        })
        .collect();
    (
        body["tally"]["mailable"].as_i64().unwrap_or_default(),
        body["tally"]["matched"].as_i64().unwrap_or_default(),
        excluded,
    )
}

/// Everybody the segment selects, as `(address, mailable, exclusionReason)` —
/// the list beside the count, exclusions included.
async fn listed(h: &Harness, query: &str) -> Vec<(String, bool, Option<String>)> {
    let (status, body) = get(&h.app, &h.token, &format!("/campaigns/audience?{query}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["people"]
        .as_array()
        .unwrap_or_else(|| panic!("no people in {body}"))
        .iter()
        .map(|person| {
            (
                person["address"].as_str().unwrap_or_default().to_owned(),
                person["mailable"].as_bool().unwrap_or_default(),
                person["exclusionReason"].as_str().map(str::to_owned),
            )
        })
        .collect()
}

/// The people a sender would actually be handed for this segment — the list
/// that decides an inbox, read through the store's own gate rather than through
/// the screen's.
async fn sendable(h: &Harness, countries: &[&str]) -> Vec<String> {
    h.acc
        .campaign_segment_recipients(
            &SegmentConditions {
                countries: countries.iter().map(|c| (*c).to_owned()).collect(),
                purchase: None,
            },
            &AudiencePage {
                after: None,
                limit: 100,
            },
        )
        .await
        .unwrap()
        .into_iter()
        .map(|recipient| recipient.address)
        .collect()
}

// ---- seeding -----------------------------------------------------------------

/// A consenting customer in a country — somebody a segment may mail before
/// anything happens, so every assertion below is about a change.
async fn mailable(h: &Harness, name: &str, address: &str, country: &str) {
    h.acc
        .create_billing_customer(&NewCustomer {
            name: name.to_owned(),
            country: country.to_owned(),
            currency: "EUR".to_owned(),
            email: Some(address.to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();
    h.acc
        .record_campaign_consent(&NewCampaignConsent {
            address,
            source: ConsentSource::Manual,
            source_ref: None,
            statement: "Ticked the newsletter box at the counter",
            occurred_at: None,
        })
        .await
        .unwrap();
}

/// One recipient's link for one send.
///
/// Minted through the store because there is nothing that sends yet — C2 waits
/// on a second IP that has to be bought (ADR 0044 §1). The token this returns is
/// the same one a message would carry, and the route under test cannot tell the
/// difference.
async fn link(h: &Harness, address: &str, topic: Option<&str>) -> String {
    h.ts.mint_campaign_unsubscribe_token(&NewUnsubscribeToken {
        send_ref: "august-send",
        address,
        topic,
    })
    .await
    .unwrap()
    .token
}

// ---- the tests ---------------------------------------------------------------

#[tokio::test]
async fn a_press_on_the_link_leaves_the_saved_segment_at_once() {
    // The item, whole: somebody the segment would have mailed presses the link
    // in their mail, and the same saved question — re-asked with no sleep and
    // no refresh — no longer offers them to a sender.
    let h = harness("cunsub-seg-now").await;
    mailable(&h, "Ann Dupont", "ann@cseg-unsub.test", "BE").await;
    mailable(&h, "Bram Peeters", "bram@cseg-unsub.test", "BE").await;
    mailable(&h, "Chris Jansen", "chris@cseg-unsub.test", "NL").await;

    let saved = save_segment(&h, "Belgian customers", &["BE"]).await;
    let before = stored(&h, &saved).await;

    // The question, asked. Two Belgians, both mailable, nobody excluded.
    assert_eq!(tally(&h, &saved.query).await, (2, 2, Vec::new()));
    assert_eq!(
        listed(&h, &saved.query).await,
        [
            ("ann@cseg-unsub.test".to_owned(), true, None),
            ("bram@cseg-unsub.test".to_owned(), true, None),
        ]
    );
    assert_eq!(
        sendable(&h, &["BE"]).await,
        ["ann@cseg-unsub.test", "bram@cseg-unsub.test"]
    );

    // Ann presses the wider button, from her mail, with no account.
    let token = link(&h, "ann@cseg-unsub.test", Some("Newsletter")).await;
    let (status, after) = press(&h.app, &token, "all").await;
    assert_eq!(status, StatusCode::OK, "{after}");
    assert_eq!(after["stopped"], true);

    // And the count has already moved — no sleep, no refresh, no second call to
    // anything that "recomputes" a segment. "One second later" is the weaker
    // claim; this is the same breath.
    assert_eq!(
        tally(&h, &saved.query).await,
        (1, 2, vec![("suppressed:unsubscribe".to_owned(), 1_i64)]),
        "a saved segment still offered somebody who had just unsubscribed"
    );

    // She is still *in* the answer, carrying the reason. A count that quietly
    // dropped her would be one nobody could audit — and she is still a customer
    // this tenant invoices, which is a different fact from being mailable.
    assert_eq!(
        listed(&h, &saved.query).await,
        [
            (
                "ann@cseg-unsub.test".to_owned(),
                false,
                Some("suppressed:unsubscribe".to_owned())
            ),
            ("bram@cseg-unsub.test".to_owned(), true, None),
        ]
    );

    // The list a sender is handed, which is the one an inbox depends on.
    assert_eq!(sendable(&h, &["BE"]).await, ["bram@cseg-unsub.test"]);

    // Nothing wrote to the segment. It stores a question, so there was nothing
    // to invalidate — which is exactly why the answer could not be stale.
    assert_eq!(stored(&h, &saved).await, before);

    // And the press was hers alone. The wider question — no conditions at all,
    // the whole audience — still holds Bram and Chris, and holds Ann under the
    // same one reason: her unsubscribe is tenant-wide, not segment-shaped.
    assert_eq!(
        tally(&h, "").await,
        (2, 3, vec![("suppressed:unsubscribe".to_owned(), 1_i64)])
    );
}

#[tokio::test]
async fn an_import_that_restates_consent_cannot_put_them_back_in_the_segment() {
    // The realistic shape of "one second later": the press lands, and then a
    // nightly import re-states the agreement that person gave last year. C1.3
    // says suppression is absolute and outranks any consent record, so the
    // import must be recorded — evidence is never deleted — and must change
    // nothing about who may be mailed.
    let h = harness("cunsub-seg-import").await;
    mailable(&h, "Ann Dupont", "ann@cseg-unsub.test", "BE").await;
    mailable(&h, "Bram Peeters", "bram@cseg-unsub.test", "BE").await;
    let saved = save_segment(&h, "Belgian customers", &["BE"]).await;
    assert_eq!(tally(&h, &saved.query).await, (2, 2, Vec::new()));

    let token = link(&h, "ann@cseg-unsub.test", Some("Newsletter")).await;
    let (status, _) = press(&h.app, &token, "all").await;
    assert_eq!(status, StatusCode::OK);
    let after_press = tally(&h, &saved.query).await;
    assert_eq!(
        after_press,
        (1, 2, vec![("suppressed:unsubscribe".to_owned(), 1_i64)])
    );

    // The import, through the route an importer would use.
    let (status, recorded) = post(
        &h.app,
        &h.token,
        "/campaigns/consent",
        json!({
            "address": "ann@cseg-unsub.test",
            "source": "import",
            "sourceRef": "customers-2026-08.csv",
            "statement": "Agreed on the signup form in 2025",
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "recording what somebody agreed to is never refused: {recorded}"
    );

    // The evidence is kept, and the answer has not budged.
    assert_eq!(
        tally(&h, &saved.query).await,
        after_press,
        "a re-stated agreement resurrected somebody who had unsubscribed"
    );
    assert_eq!(sendable(&h, &["BE"]).await, ["bram@cseg-unsub.test"]);
    let (status, history) = get(&h.app, &h.token, "/campaigns/consent/ann@cseg-unsub.test").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        history["consent"].as_array().map(Vec::len),
        Some(2),
        "the import was silently dropped instead of recorded: {history}"
    );
}

#[tokio::test]
async fn the_narrower_button_leaves_the_segment_where_it_was() {
    // The other direction, and the one an over-eager fix would break: "fewer,
    // not only none" (C2s.2) must not shrink a segment. A narrower button that
    // suppressed would pass every assertion about the person being off the
    // newsletter and would be the failure that item exists to prevent.
    //
    // Nothing enforces a declined topic yet, deliberately: no send can name a
    // kind of mail until the campaign record (C3.1) exists, so the preference is
    // stored and waits. This test pins the *segment* half of that state — a
    // topic decline is recorded and is not a suppression.
    let h = harness("cunsub-seg-fewer").await;
    mailable(&h, "Ann Dupont", "ann@cseg-unsub.test", "BE").await;
    mailable(&h, "Bram Peeters", "bram@cseg-unsub.test", "BE").await;
    let saved = save_segment(&h, "Belgian customers", &["BE"]).await;
    let before = tally(&h, &saved.query).await;
    assert_eq!(before, (2, 2, Vec::new()));

    let token = link(&h, "ann@cseg-unsub.test", Some("Newsletter")).await;
    let (status, after) = press(&h.app, &token, "topic").await;
    assert_eq!(status, StatusCode::OK, "{after}");
    assert_eq!(after["topicDeclined"], true);
    assert_eq!(after["stopped"], false);

    assert_eq!(
        tally(&h, &saved.query).await,
        before,
        "\"stop this kind of mail\" took away everything"
    );
    assert_eq!(
        sendable(&h, &["BE"]).await,
        ["ann@cseg-unsub.test", "bram@cseg-unsub.test"]
    );
    // The preference is a record rather than a no-op: it is what a send that
    // names a kind will read.
    let declined =
        h.ts.campaign_topics_declined_by("ann@cseg-unsub.test")
            .await
            .unwrap();
    assert_eq!(declined.len(), 1);
    assert_eq!(declined[0].topic, "newsletter");
    assert!(
        h.ts.campaign_suppression_for("ann@cseg-unsub.test")
            .await
            .unwrap()
            .is_none(),
        "the narrower button wrote a suppression"
    );
}

#[tokio::test]
async fn a_neighbours_unsubscribe_never_shrinks_our_segment() {
    // Law 1, in the shape this chain could actually break it: two workspaces on
    // one store hold the same person, each has a Belgian-customers segment, and
    // each minted them a link. The token is the only thing naming a tenant on
    // the public route, so a mixed-up one would quietly cost the wrong company a
    // customer — and would read as the feature working.
    let ours = harness("cunsub-seg-ours").await;
    let theirs = harness_on(Arc::clone(&ours.store), "cunsub-seg-theirs").await;

    for h in [&ours, &theirs] {
        mailable(h, "Shared Person", "shared@cseg-unsub.test", "BE").await;
        mailable(h, "Their Own", "own@cseg-unsub.test", "BE").await;
    }
    let our_segment = save_segment(&ours, "Belgian customers", &["BE"]).await;
    let their_segment = save_segment(&theirs, "Belgian customers", &["BE"]).await;
    assert_eq!(tally(&ours, &our_segment.query).await, (2, 2, Vec::new()));
    assert_eq!(
        tally(&theirs, &their_segment.query).await,
        (2, 2, Vec::new())
    );

    // The shared person presses the link that came from the neighbour's send,
    // and does it against our router — one deployment, one table, so this is the
    // real test rather than a staged one.
    let their_link = link(&theirs, "shared@cseg-unsub.test", Some("Newsletter")).await;
    let (status, after) = press(&ours.app, &their_link, "all").await;
    assert_eq!(status, StatusCode::OK, "{after}");
    assert_eq!(after["stopped"], true);

    assert_eq!(
        tally(&theirs, &their_segment.query).await,
        (1, 2, vec![("suppressed:unsubscribe".to_owned(), 1_i64)]),
        "the neighbour's own segment did not honour their own unsubscribe"
    );
    assert_eq!(
        tally(&ours, &our_segment.query).await,
        (2, 2, Vec::new()),
        "a neighbour's unsubscribe emptied our segment"
    );
    assert_eq!(
        sendable(&ours, &["BE"]).await,
        ["own@cseg-unsub.test", "shared@cseg-unsub.test"]
    );
    assert_eq!(sendable(&theirs, &["BE"]).await, ["own@cseg-unsub.test"]);
}
