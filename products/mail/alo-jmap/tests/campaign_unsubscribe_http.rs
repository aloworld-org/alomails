//! The page at the end of the unsubscribe link (C2s.2, ADR 0044 §3), driven
//! through the real router over a real Postgres.
//!
//! This is the only surface in the product a **stranger** reaches: no account,
//! no login, no token but the one in their mail. Three things are asserted here
//! that no other HTTP suite has to assert:
//!
//! - **The GET writes nothing.** Every link-prefetching scanner between us and
//!   the recipient fetches the URL before a human sees it, and there is no way
//!   to lift a suppression — so a GET with a side effect would permanently
//!   unsubscribe people who never clicked, and it would look like the feature
//!   working. That is why RFC 8058 requires a POST at all.
//! - **Fewer really is fewer.** Pressing "stop this kind of mail" must leave
//!   the person mailable. A narrower button that quietly suppressed everything
//!   would pass every assertion about them being off the newsletter and would
//!   be exactly the failure the item exists to prevent.
//! - **A guess teaches a stranger nothing.** An unknown token, a malformed one
//!   and one for an address this deployment has never heard of are the same
//!   `404` with the same sentence.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::{
    AudiencePage, ConsentSource, NewCampaignConsent, NewCustomer, NewUnsubscribeToken,
};

use common::{Harness, harness, harness_on, send};

// ---- request helpers ---------------------------------------------------------

fn uri(token: &str) -> String {
    format!("/jmap/campaign-unsubscribe/{token}")
}

/// The page's own read. No `Authorization` header anywhere in this file: the
/// whole point is that a stranger with the link can do this.
async fn show(app: &Router, token: &str) -> (StatusCode, Value) {
    send(
        app,
        Request::builder()
            .method("GET")
            .uri(uri(token))
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

/// The page's own act: one of the two buttons.
async fn press(app: &Router, token: &str, scope: &str) -> (StatusCode, Value) {
    send(
        app,
        Request::builder()
            .method("POST")
            .uri(uri(token))
            .header("content-type", "application/json")
            .body(Body::from(json!({ "scope": scope }).to_string()))
            .unwrap(),
    )
    .await
}

/// A mail client's own Unsubscribe button, exactly as RFC 8058 §3.1 spells it.
async fn one_click(app: &Router, token: &str) -> (StatusCode, Value) {
    send(
        app,
        Request::builder()
            .method("POST")
            .uri(uri(token))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("List-Unsubscribe=One-Click"))
            .unwrap(),
    )
    .await
}

// ---- seeding -----------------------------------------------------------------

/// A consenting customer — somebody this tenant may mail before anything else
/// happens, so every assertion below is about a change rather than a state.
async fn mailable(h: &Harness, name: &str, address: &str) {
    h.acc
        .create_billing_customer(&NewCustomer {
            name: name.to_owned(),
            country: "BE".to_owned(),
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
            statement: "Ticked the newsletter box at checkout",
            occurred_at: None,
        })
        .await
        .unwrap();
}

/// One recipient's link for one send, with the kind of mail it came from.
async fn link(h: &Harness, send_ref: &str, address: &str, topic: Option<&str>) -> (String, String) {
    let issued =
        h.ts.mint_campaign_unsubscribe_token(&NewUnsubscribeToken {
            send_ref,
            address,
            topic,
        })
        .await
        .unwrap();
    (issued.token, issued.record.as_str().to_owned())
}

/// Every address this tenant may still mail.
async fn recipients(h: &Harness) -> Vec<String> {
    h.acc
        .campaign_recipients(&AudiencePage {
            after: None,
            limit: 100,
        })
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.address)
        .collect()
}

// ---- the tests ---------------------------------------------------------------

#[tokio::test]
async fn a_stranger_with_the_link_is_offered_fewer_as_well_as_none() {
    // The item, as a test: "offering fewer rather than only none — this kind of
    // mail, or all of it. One click either way, no confirmation maze."
    let h = harness("cunsub-fewer").await;
    mailable(&h, "Ann Dupont", "ann@cunsub.test").await;
    mailable(&h, "Bram Peeters", "bram@cunsub.test").await;
    let (ann_token, ann_record) =
        link(&h, "august-send", "ann@cunsub.test", Some("Newsletter")).await;
    let (bram_token, _) = link(&h, "august-send", "bram@cunsub.test", Some("Newsletter")).await;

    // The page draws itself with no account, no login and no bearer token.
    let (status, page) = show(&h.app, &ann_token).await;
    assert_eq!(status, StatusCode::OK);
    // The kind of mail, as the sender wrote it — the one thing here that
    // describes the mail rather than the person.
    assert_eq!(page["topic"], "Newsletter");
    assert_eq!(page["stopped"], false);
    assert_eq!(page["topicDeclined"], false);
    // And the address is nowhere in it. A link is forwarded, quoted in replies
    // and read by scanners; a page that echoed the recipient back would turn a
    // forwarded mail into a disclosure.
    assert!(
        !page.to_string().contains("ann@cunsub.test"),
        "the page names the recipient: {page}"
    );

    // Ann presses the narrower button. One press, no second screen.
    let (status, after) = press(&h.app, &ann_token, "topic").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(after["scope"], "topic");
    assert_eq!(after["topicDeclined"], true);
    assert_eq!(
        after["stopped"], false,
        "the narrower choice suppressed the person"
    );

    // The assertion the whole item turns on: she asked for less, not for
    // nothing, and she is still somebody this tenant may mail.
    assert_eq!(
        recipients(&h).await,
        ["ann@cunsub.test", "bram@cunsub.test"],
        "\"fewer\" took away everything"
    );
    // The decision is recorded against her, folded, and names the link she used
    // by its record id — never the token itself.
    let declined =
        h.ts.campaign_topics_declined_by("ann@cunsub.test")
            .await
            .unwrap();
    assert_eq!(declined.len(), 1);
    assert_eq!(declined[0].topic, "newsletter");
    assert_eq!(declined[0].source_ref.as_deref(), Some(ann_record.as_str()));
    assert!(
        !declined[0]
            .source_ref
            .as_deref()
            .unwrap_or_default()
            .contains(&ann_token),
        "the working link was copied into a second table"
    );

    // Bram presses the wider one, and that does end everything.
    let (status, after) = press(&h.app, &bram_token, "all").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(after["scope"], "all");
    assert_eq!(after["stopped"], true);
    assert_eq!(recipients(&h).await, ["ann@cunsub.test"]);
    assert_eq!(
        h.ts.campaign_suppression_for("bram@cunsub.test")
            .await
            .unwrap()
            .map(|s| s.reason.as_str()),
        Some("unsubscribe")
    );
    // His link had a topic, and he did not use it: the wider choice is not a
    // topic preference with a bigger name.
    assert!(
        h.ts.campaign_topics_declined_by("bram@cunsub.test")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn looking_at_the_page_never_unsubscribes_anybody() {
    // Why RFC 8058 requires a POST, held as a test. Corporate scanners,
    // antivirus proxies and preview panes fetch every URL in a message before a
    // human sees it, and a suppression cannot be lifted — so a GET with a side
    // effect would permanently unsubscribe people who never clicked, and would
    // read as the feature working.
    let h = harness("cunsub-prefetch").await;
    mailable(&h, "Ann Dupont", "ann@cunsub.test").await;
    let (token, _) = link(&h, "august-send", "ann@cunsub.test", Some("Newsletter")).await;

    for _ in 0..5 {
        let (status, page) = show(&h.app, &token).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(page["stopped"], false);
        assert_eq!(page["topicDeclined"], false);
    }

    assert_eq!(recipients(&h).await, ["ann@cunsub.test"]);
    assert!(
        h.ts.campaign_suppression_for("ann@cunsub.test")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        h.ts.campaign_topics_declined_by("ann@cunsub.test")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_mail_clients_own_button_stops_everything_and_says_so() {
    // RFC 8058 §3.1: the client posts `List-Unsubscribe=One-Click` with no page
    // and no chance for the recipient to choose. One unconditional gesture,
    // read as the wider choice — the narrower reading would leave them
    // receiving mail they believed they had stopped, and the next press is the
    // spam button.
    let h = harness("cunsub-oneclick").await;
    mailable(&h, "Ann Dupont", "ann@cunsub.test").await;
    let (token, record) = link(&h, "august-send", "ann@cunsub.test", Some("Newsletter")).await;

    let (status, after) = one_click(&h.app, &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(after["scope"], "all");
    assert_eq!(after["stopped"], true);
    assert!(recipients(&h).await.is_empty());

    let suppression =
        h.ts.campaign_suppression_for("ann@cunsub.test")
            .await
            .unwrap()
            .expect("one-click did not suppress");
    assert_eq!(suppression.reason.as_str(), "unsubscribe");
    assert_eq!(suppression.source_ref.as_deref(), Some(record.as_str()));

    // Pressing it again — which everybody who is not sure it worked does —
    // answers the same thing and does not restamp when they decided.
    let again =
        h.ts.campaign_suppression_for("ann@cunsub.test")
            .await
            .unwrap()
            .expect("still suppressed");
    let (status, _) = one_click(&h.app, &token).await;
    assert_eq!(status, StatusCode::OK);
    let third =
        h.ts.campaign_suppression_for("ann@cunsub.test")
            .await
            .unwrap()
            .expect("still suppressed");
    assert_eq!(third.id, again.id);
    assert_eq!(third.occurred_at, again.occurred_at);
}

#[tokio::test]
async fn a_send_that_named_no_kind_of_mail_offers_one_button_rather_than_a_broken_one() {
    // Honest rather than lazy: `topic: null` tells the page to draw one button.
    // Offering "stop this kind" for a kind nothing named would decline a
    // category no send matches — a person who pressed the button and is still
    // being mailed.
    let h = harness("cunsub-untopiced").await;
    mailable(&h, "Ann Dupont", "ann@cunsub.test").await;
    let (token, _) = link(&h, "august-send", "ann@cunsub.test", None).await;

    let (status, page) = show(&h.app, &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["topic"], Value::Null);

    let (status, problem) = press(&h.app, &token, "topic").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        problem.to_string().contains("stop all of it"),
        "the refusal must say what the person can do instead: {problem}"
    );
    // Refused, and nothing was written on the way.
    assert_eq!(recipients(&h).await, ["ann@cunsub.test"]);
    assert!(
        h.ts.campaign_topics_declined_by("ann@cunsub.test")
            .await
            .unwrap()
            .is_empty()
    );

    // The wider choice still works, which is the whole reason the refusal is
    // safe to make.
    let (status, after) = press(&h.app, &token, "all").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(after["stopped"], true);
    assert!(recipients(&h).await.is_empty());
}

#[tokio::test]
async fn a_guess_teaches_a_stranger_nothing_about_who_we_hold() {
    // A malformed token, an unknown one and one for an address this deployment
    // has never heard of are the same `404` with the same sentence. Telling
    // them apart is what turns this endpoint into an oracle for which addresses
    // we hold — and this is the one route in the product a spammer can post a
    // million times without an account.
    let h = harness("cunsub-guess").await;
    mailable(&h, "Ann Dupont", "ann@cunsub.test").await;
    let (real, _) = link(&h, "august-send", "ann@cunsub.test", Some("Newsletter")).await;

    // A near miss on a real one: the same length, one character different.
    let last = real.chars().last().unwrap_or('a');
    let near = format!(
        "{}{}",
        &real[..real.len() - 1],
        if last == 'x' { 'y' } else { 'x' }
    );
    assert_ne!(near, real);

    let mut answers: Vec<(StatusCode, String)> = Vec::new();
    for guess in [
        "0",
        "00000000000000000000000000000000000000000000",
        "ann@cunsub.test",
        "not-a-token",
        near.as_str(),
    ] {
        let (status, body) = show(&h.app, guess).await;
        answers.push((status, body.to_string()));
        let (status, body) = press(&h.app, guess, "all").await;
        answers.push((status, body.to_string()));
    }
    let first = answers.first().cloned().expect("guesses were made");
    for (status, body) in &answers {
        assert_eq!(*status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(
            (*status, body.clone()),
            first,
            "two guesses got different answers, which is the oracle"
        );
    }

    // And nothing was written by a million guesses.
    assert_eq!(recipients(&h).await, ["ann@cunsub.test"]);

    // A body that says neither of the two things this route accepts is refused
    // rather than guessed at — reading a stray post as "stop everything" would
    // let whatever sent it end a relationship nothing can restore.
    for (content_type, body) in [
        ("application/json", ""),
        ("application/json", "{}"),
        ("application/json", r#"{"scope":"everything"}"#),
        ("application/x-www-form-urlencoded", "scope=all"),
        (
            "application/x-www-form-urlencoded",
            "List-Unsubscribe=Two-Click",
        ),
    ] {
        let (status, _) = send(
            &h.app,
            Request::builder()
                .method("POST")
                .uri(uri(&real))
                .header("content-type", content_type)
                .body(Body::from(body))
                .unwrap(),
        )
        .await;
        assert!(
            status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::BAD_REQUEST,
            "{content_type} {body:?} was accepted as a choice ({status})"
        );
    }
    assert_eq!(recipients(&h).await, ["ann@cunsub.test"]);
}

#[tokio::test]
async fn a_neighbours_link_stops_a_neighbours_mail_and_never_ours() {
    // The mandatory wrong-tenant test, in the shape this route could actually
    // break: two workspaces on one store hold the SAME person and each minted
    // them a link. The token is the only thing that names a tenant here, so a
    // mixed-up one would suppress the wrong company's customer — and would read
    // as the feature working.
    let ours = harness("cunsub-ours").await;
    let theirs = harness_on(std::sync::Arc::clone(&ours.store), "cunsub-theirs").await;

    for h in [&ours, &theirs] {
        mailable(h, "Shared Person", "shared@person.test").await;
    }
    let (our_link, _) = link(&ours, "our-send", "shared@person.test", Some("Newsletter")).await;
    let (their_link, _) = link(
        &theirs,
        "their-send",
        "shared@person.test",
        Some("Their Newsletter"),
    )
    .await;

    // Each link describes its own send, whichever router it is presented to —
    // there is one deployment and one table, so this is the real test.
    let (status, page) = show(&ours.app, &their_link).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["topic"], "Their Newsletter");

    // Using their link stops their mail, and ours keeps flowing: unsubscribing
    // from one company is not unsubscribing from every company on the platform.
    let (status, after) = press(&ours.app, &their_link, "all").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(after["stopped"], true);
    assert!(recipients(&theirs).await.is_empty());
    assert_eq!(recipients(&ours).await, ["shared@person.test"]);
    assert!(
        ours.ts
            .campaign_suppression_for("shared@person.test")
            .await
            .unwrap()
            .is_none(),
        "a neighbour's link silenced our address"
    );

    // And ours still works, from the other router, on the same person.
    let (status, after) = press(&theirs.app, &our_link, "all").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(after["stopped"], true);
    assert!(recipients(&ours).await.is_empty());
}
