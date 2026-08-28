//! The deal ↔ mail linking HTTP surface (B2.05), driven through the real router
//! over a real Postgres.
//!
//! `alo-store`'s own suite proves the records and the two boundaries; what
//! matters here is the **edge**: the auth guard on every route, the status codes
//! `docs/design/crm.md` publishes, the idempotent link, and above all that a
//! conversation the caller has no message in — whether it belongs to another
//! tenant, to a colleague, or to nobody at all — answers with exactly the same
//! `404`, so the route cannot be used to ask what is in somebody else's mailbox.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::{AccountStore, MailboxId};

use crate::common::{Harness, harness, send};

// ---- request helpers ---------------------------------------------------------

fn with_json(method: &str, uri: &str, token: Option<&str>, body: Value) -> Request<Body> {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    req.body(Body::from(body.to_string())).unwrap()
}

async fn post(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, with_json("POST", uri, Some(token), body)).await
}

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

async fn delete(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

// ---- fixtures ----------------------------------------------------------------

/// A deal on the tenant's seeded board, with the contact address the
/// suggestions match on.
async fn deal_with_contact(h: &Harness, contact: &str) -> String {
    let (status, body) = get(&h.app, &h.token, "/crm/pipelines").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let pipeline = body["pipelines"][0]["id"].as_str().unwrap().to_owned();
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/crm/pipelines/{pipeline}/stages"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let stage = body["stages"][0]["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        "/crm/deals",
        json!({
            "pipelineId": pipeline,
            "stageId": stage,
            "title": "Renewal — Acme GmbH",
            "contactEmail": contact,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["deal"]["id"].as_str().unwrap().to_owned()
}

/// Delivers one message into an account and answers the conversation it landed
/// in — the only way a user comes to hold a thread.
async fn conversation(
    acc: &AccountStore,
    inbox: &MailboxId,
    tag: &str,
    from: &str,
    to: &str,
    subject: &str,
) -> String {
    let raw = format!(
        "From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\n\
         Message-ID: <{tag}@crmthr.test>\r\n\r\nbody of {tag}\r\n"
    );
    let message = acc.ingest(inbox, raw.as_bytes()).await.unwrap();
    acc.message(&message)
        .await
        .unwrap()
        .thread_id
        .as_str()
        .to_owned()
}

/// The caller's own inbox.
async fn inbox(h: &Harness) -> MailboxId {
    h.acc.inbox().await.unwrap()
}

// ---- the arc -----------------------------------------------------------------

#[tokio::test]
async fn a_conversation_is_linked_read_and_unlinked() {
    let h = harness("crmthr-arc").await;
    let deal = deal_with_contact(&h, "ada@acme.test").await;
    let mailbox = inbox(&h).await;
    let thread = conversation(
        &h.acc,
        &mailbox,
        "arc",
        "Ada <ada@acme.test>",
        &h.email,
        "Renewal 2027",
    )
    .await;

    // Nothing is linked until somebody says so.
    let (status, body) = get(&h.app, &h.token, &format!("/crm/deals/{deal}/threads")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["threads"].as_array().map(Vec::len), Some(0));

    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/threads"),
        json!({ "threadId": thread }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["created"], true);
    assert_eq!(body["thread"]["threadId"], thread.as_str());
    assert_eq!(body["thread"]["subject"], "Renewal 2027");
    assert_eq!(body["thread"]["readable"], true);
    assert_eq!(body["thread"]["linkedBy"], h.account_id.as_str());
    // The link carries no message content — not a body, not an address list,
    // not a count. Those fields are absent, not empty.
    for never in ["body", "from", "to", "messages", "messageCount", "preview"] {
        assert!(body["thread"][never].is_null(), "{never} leaked: {body}");
    }

    // Linking twice is the same link, answered without an error.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/threads"),
        json!({ "threadId": format!("  {thread}  ") }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["created"], false);

    let (status, body) = get(&h.app, &h.token, &format!("/crm/deals/{deal}/threads")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["threads"].as_array().map(Vec::len), Some(1));

    let (status, body) = delete(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/threads/{thread}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["unlinked"], true);

    // Unlinking twice is a 404: the link really is gone.
    let (status, _) = delete(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/threads/{thread}"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn suggestions_propose_and_write_nothing() {
    let h = harness("crmthr-suggest").await;
    let deal = deal_with_contact(&h, "ada@acme.test").await;
    let mailbox = inbox(&h).await;
    let contact = conversation(
        &h.acc,
        &mailbox,
        "sg-contact",
        "Ada <ada@acme.test>",
        &h.email,
        "Renewal 2027",
    )
    .await;
    let colleague = conversation(
        &h.acc,
        &mailbox,
        "sg-domain",
        "Bob <bob@acme.test>",
        &h.email,
        "Procurement",
    )
    .await;
    let private = conversation(
        &h.acc,
        &mailbox,
        "sg-private",
        "Mum <mum@gmail.com>",
        &h.email,
        "Sunday",
    )
    .await;

    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/thread-suggestions"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let proposed = body["suggestions"].as_array().cloned().unwrap_or_default();
    let ids: Vec<&str> = proposed
        .iter()
        .map(|s| s["threadId"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(ids, vec![contact.as_str(), colleague.as_str()]);
    assert!(
        !ids.contains(&private.as_str()),
        "free-mail never domain-matches"
    );
    assert_eq!(proposed[0]["reason"], "address");
    assert_eq!(proposed[0]["matchedAddress"], "ada@acme.test");
    assert_eq!(proposed[1]["reason"], "domain");

    // A proposal is not a link.
    let (_, body) = get(&h.app, &h.token, &format!("/crm/deals/{deal}/threads")).await;
    assert_eq!(body["threads"].as_array().map(Vec::len), Some(0));

    // `limit` is a page size, clamped rather than refused — it is not an
    // assertion about the data.
    for (query, expected) in [("?limit=1", 1), ("?limit=0", 1), ("?limit=99999", 2)] {
        let (status, body) = get(
            &h.app,
            &h.token,
            &format!("/crm/deals/{deal}/thread-suggestions{query}"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{query}: {body}");
        assert_eq!(
            body["suggestions"].as_array().map(Vec::len),
            Some(expected),
            "{query}"
        );
    }

    // Once linked, it stops being proposed.
    post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/threads"),
        json!({ "threadId": contact }),
    )
    .await;
    let (_, body) = get(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/thread-suggestions"),
    )
    .await;
    assert_eq!(
        body["suggestions"][0]["threadId"],
        colleague.as_str(),
        "{body}"
    );
}

// ---- the guards --------------------------------------------------------------

#[tokio::test]
async fn a_link_states_a_conversation_and_a_bad_body_is_refused() {
    let h = harness("crmthr-422").await;
    let deal = deal_with_contact(&h, "ada@acme.test").await;

    for bad in [
        json!({}),
        json!({ "threadId": "" }),
        json!({ "threadId": "  " }),
    ] {
        let (status, body) = post(
            &h.app,
            &h.token,
            &format!("/crm/deals/{deal}/threads"),
            bad.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {body}");
        assert_eq!(body["detail"], "threadId is required");
    }

    // A body that is not JSON at all is a 400 that never echoes the input.
    let req = Request::builder()
        .method("POST")
        .uri(format!("/crm/deals/{deal}/threads"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", h.token))
        .body(Body::from("{not json"))
        .unwrap();
    let (status, body) = send(&h.app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["detail"], "malformed request body");
}

#[tokio::test]
async fn a_conversation_the_caller_cannot_see_answers_exactly_as_one_that_never_existed() {
    let a = harness("crmthr-a").await;
    let b = harness("crmthr-b").await;

    let a_deal = deal_with_contact(&a, "ada@acme.test").await;
    let a_mailbox = inbox(&a).await;
    let a_thread = conversation(
        &a.acc,
        &a_mailbox,
        "iso-a",
        "Ada <ada@acme.test>",
        &a.email,
        "A's conversation",
    )
    .await;
    a_thread_linked(&a, &a_deal, &a_thread).await;

    let b_deal = deal_with_contact(&b, "ada@acme.test").await;
    let b_mailbox = inbox(&b).await;
    let b_thread = conversation(
        &b.acc,
        &b_mailbox,
        "iso-b",
        "Ada <ada@acme.test>",
        &b.email,
        "B's conversation",
    )
    .await;

    // B cannot see, link to, or unlink from A's deal, and cannot tell A's deal
    // apart from one that never existed.
    for uri in [
        format!("/crm/deals/{a_deal}/threads"),
        format!("/crm/deals/{a_deal}/thread-suggestions"),
        "/crm/deals/crd_nope/threads".to_owned(),
        "/crm/deals/crd_nope/thread-suggestions".to_owned(),
    ] {
        let (status, body) = get(&b.app, &b.token, &uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {body}");
    }
    for (deal, thread) in [
        (a_deal.as_str(), b_thread.as_str()),
        (a_deal.as_str(), a_thread.as_str()),
        ("crd_nope", b_thread.as_str()),
    ] {
        let (status, body) = post(
            &b.app,
            &b.token,
            &format!("/crm/deals/{deal}/threads"),
            json!({ "threadId": thread }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{deal}/{thread}: {body}");
        let (status, _) = delete(
            &b.app,
            &b.token,
            &format!("/crm/deals/{deal}/threads/{thread}"),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    // And A cannot link B's conversation to A's own deal: a thread id is not
    // authority for anything. The answer is the one an invented id gets.
    for thread in [b_thread.as_str(), "thr_nope"] {
        let (status, body) = post(
            &a.app,
            &a.token,
            &format!("/crm/deals/{a_deal}/threads"),
            json!({ "threadId": thread }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{thread}: {body}");
    }

    // B's suggestions are computed over B's own mail only — A's conversation
    // matches the same contact address and is still nowhere in them.
    let (status, body) = get(
        &b.app,
        &b.token,
        &format!("/crm/deals/{b_deal}/thread-suggestions"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let ids: Vec<String> = body["suggestions"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|s| s["threadId"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(ids, vec![b_thread.clone()]);

    // A's link survived every attempt.
    let (_, body) = get(&a.app, &a.token, &format!("/crm/deals/{a_deal}/threads")).await;
    assert_eq!(body["threads"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["threads"][0]["threadId"], a_thread.as_str());
}

/// Links a conversation and asserts it took, so a later failure is not a
/// confusing consequence of a failed setup.
async fn a_thread_linked(h: &Harness, deal: &str, thread: &str) {
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/threads"),
        json!({ "threadId": thread }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn every_link_route_refuses_an_unauthenticated_caller() {
    let h = harness("crmthr-401").await;
    let deal = deal_with_contact(&h, "ada@acme.test").await;
    let mailbox = inbox(&h).await;
    let thread = conversation(
        &h.acc,
        &mailbox,
        "401",
        "Ada <ada@acme.test>",
        &h.email,
        "Renewal 2027",
    )
    .await;

    let mut unauthenticated: Vec<Request<Body>> = vec![with_json(
        "POST",
        &format!("/crm/deals/{deal}/threads"),
        None,
        json!({ "threadId": thread }),
    )];
    for uri in [
        format!("/crm/deals/{deal}/threads"),
        format!("/crm/deals/{deal}/thread-suggestions"),
    ] {
        unauthenticated.push(Request::builder().uri(uri).body(Body::empty()).unwrap());
    }
    unauthenticated.push(
        Request::builder()
            .method("DELETE")
            .uri(format!("/crm/deals/{deal}/threads/{thread}"))
            .body(Body::empty())
            .unwrap(),
    );

    for req in unauthenticated {
        let (method, uri) = (req.method().clone(), req.uri().clone());
        let (status, body) = send(&h.app, req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}: {body}");
    }
}
