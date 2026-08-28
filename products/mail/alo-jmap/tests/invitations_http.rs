//! iTIP over iMIP (RFC 5546/6047), proven end to end across two accounts on a
//! real local stack: the organizer creates an event with a guest → the
//! internal submission listener (an in-process SMTP sink) receives the
//! `METHOD:REQUEST` → the captured wire bytes are delivered into the guest's
//! mailbox → `Email/get` surfaces the invitation the reading-pane card renders
//! → the guest RSVPs → the sink receives the `METHOD:REPLY` → delivered to the
//! organizer, applying it records the guest's `PARTSTAT` on the organizer's
//! event. CANCEL: one naming a `RECURRENCE-ID` removes the instance, not the
//! series; one without removes the series. Foreign blobs are 404 at every
//! door. Real routes, real Postgres, real SMTP dialog — nothing mocked but the
//! far end of the wire.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use std::sync::Arc;

use alo_identity::Identity;
use alo_jmap::PushHub;
use alo_jmap::state::{AppState, Limits};
use alo_store::{AccountStore, BlobStore, Store, TenantId, UserId};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use crate::common::send;

const NL: u8 = 10;
const DOT: u8 = 46;

/// Strip SMTP dot-stuffing and the terminating `.` line from captured DATA,
/// yielding the message bytes as the sender composed them.
fn undot(raw: &[u8]) -> Vec<u8> {
    let body = match raw.windows(5).position(|w| w == b"\r\n.\r\n") {
        Some(pos) => &raw[..pos + 2],
        None => raw,
    };
    let mut out = Vec::with_capacity(body.len());
    for seg in body.split_inclusive(|&b| b == NL) {
        if seg.first() == Some(&DOT) {
            out.extend_from_slice(&seg[1..]);
        } else {
            out.extend_from_slice(seg);
        }
    }
    out
}

/// A multi-connection SMTP sink: accepts any number of submissions and hands
/// each one's (RCPT lines, un-stuffed DATA) to the test over a channel — so a
/// test can follow invitation → reply → cancel without re-arming a listener.
async fn spawn_sink() -> (String, mpsc::UnboundedReceiver<(Vec<String>, Vec<u8>)>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _peer)) = listener.accept().await else {
                break;
            };
            let (rd, mut wr) = stream.into_split();
            let mut reader = BufReader::new(rd);
            let mut rcpts = Vec::new();
            let mut data = Vec::new();
            wr.write_all(b"220 sink.test ESMTP\r\n").await.unwrap();
            let mut line = String::new();
            loop {
                line.clear();
                let n = reader.read_line(&mut line).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                let cmd = line.trim_end();
                let upper = cmd.to_ascii_uppercase();
                if upper.starts_with("EHLO") || upper.starts_with("HELO") {
                    wr.write_all(b"250 sink.test\r\n").await.unwrap();
                } else if upper.starts_with("MAIL FROM") {
                    wr.write_all(b"250 2.1.0 ok\r\n").await.unwrap();
                } else if upper.starts_with("RCPT TO") {
                    rcpts.push(cmd.to_owned());
                    wr.write_all(b"250 2.1.5 ok\r\n").await.unwrap();
                } else if upper.starts_with("DATA") {
                    wr.write_all(b"354 go\r\n").await.unwrap();
                    loop {
                        let mut buf = [0u8; 4096];
                        let m = reader.read(&mut buf).await.unwrap_or(0);
                        if m == 0 {
                            break;
                        }
                        data.extend_from_slice(&buf[..m]);
                        if data.ends_with(b"\r\n.\r\n") {
                            break;
                        }
                    }
                    wr.write_all(b"250 2.0.0 queued\r\n").await.unwrap();
                } else if upper.starts_with("QUIT") {
                    wr.write_all(b"221 2.0.0 bye\r\n").await.unwrap();
                    break;
                } else {
                    wr.write_all(b"250 ok\r\n").await.unwrap();
                }
            }
            if tx.send((rcpts, undot(&data))).is_err() {
                break;
            }
        }
    });
    (addr, rx)
}

fn app_with_sink(store: &Arc<Store>, identity: &Identity, sink_addr: String) -> Router {
    alo_jmap::app(AppState {
        media: None,
        turns: Default::default(),
        store: Arc::clone(store),
        identity: identity.clone(),
        push: PushHub::new(),
        limits: Limits::default(),
        base_url: "https://test".into(),
        submission_addr: Some(sink_addr),
        session_origins: Vec::new(),
        web_push: None,
        junk_learner: None,
        personal_domains: Vec::new(),
        signup_limiter: alo_identity::ratelimit::RateLimiter::new(),
    })
}

/// One provisioned, logged-in user: account handle + bearer token.
struct Login {
    email: String,
    token: String,
    acc: AccountStore,
    user: UserId,
}

async fn provision(store: &Arc<Store>, identity: &Identity, tenant: &TenantId, tag: &str) -> Login {
    let email = format!("{tag}-{tenant}@example.test").to_lowercase();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&email)
        .await
        .unwrap();
    identity
        .set_password(tenant, &user, &email, "s3cret-pw")
        .await
        .unwrap();
    let token = identity
        .password_login(&email, "s3cret-pw", None)
        .await
        .unwrap()
        .expect("token issued")
        .0
        .reveal()
        .to_owned();
    Login {
        email,
        token,
        acc: store.for_account(tenant.clone(), user.clone()),
        user,
    }
}

async fn post(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, req).await
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

/// Deliver captured wire bytes into an account's inbox (what the SMTP delivery
/// path would do) and return the message's blob id + the `alo:invitation` the
/// reading pane gets from `Email/get` — proving the card renders parsed data
/// from the real message, not client-side guesswork.
async fn deliver_and_read(app: &Router, login: &Login, wire: &[u8]) -> (String, Value) {
    let mid = login.acc.deliver(wire).await.unwrap();
    let blob_id = login
        .acc
        .message(&mid)
        .await
        .unwrap()
        .blob_id
        .as_str()
        .to_owned();
    let (status, resp) = common::api(
        app,
        &login.token,
        common::call(
            "Email/get",
            json!({
                "accountId": login.user.to_string(),
                "ids": [mid.as_str()],
                "fetchTextBodyValues": true,
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resp}");
    let invitation = resp["methodResponses"][0][1]["list"][0]["alo:invitation"].clone();
    (blob_id, invitation)
}

/// The full iMIP arc the queue item demands: REQUEST out through the internal
/// listener, in through the guest's mailbox, RSVP back as a REPLY, and the
/// guest's PARTSTAT recorded on the organizer's event.
#[tokio::test]
async fn request_reply_round_trip_across_two_accounts() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&common::database_url())
        .await
        .expect("connect to test postgres");
    let store = Arc::new(Store::new(pool, BlobStore::in_memory(50 * 1024 * 1024)));
    store.migrate().await.unwrap();
    let tenant = store.create_tenant("jmap-imip-rt").await.unwrap();
    let identity = common::test_identity(Arc::clone(&store));
    let organizer = provision(&store, &identity, &tenant, "organizer").await;
    let guest = provision(&store, &identity, &tenant, "guest").await;
    let (sink_addr, mut sink) = spawn_sink().await;
    let app = app_with_sink(&store, &identity, sink_addr);

    // Organizer invites the guest. The save answers only after the REQUEST
    // went through the submission listener (best-effort, but on this stack the
    // listener is up, so it lands).
    let (status, created) = post(
        &app,
        &organizer.token,
        "/calendar/events",
        json!({
            "summary": "Design review",
            "startsAt": "2026-09-08T09:00:00Z",
            "endsAt": "2026-09-08T10:00:00Z",
            "attendees": [guest.email],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let event_id = created["id"].as_str().unwrap().to_owned();

    // The wire: one submission, addressed to the guest, a text/calendar
    // REQUEST part beside the plain-text alternative.
    let (rcpts, wire) = sink.recv().await.expect("the REQUEST reached the sink");
    assert!(
        rcpts.iter().any(|r| r.contains(&guest.email)),
        "envelope goes to the guest: {rcpts:?}"
    );
    let text = String::from_utf8_lossy(&wire);
    assert!(text.contains("method=REQUEST"), "{text}");

    // Delivered into the guest's inbox, the reading pane sees an invitation —
    // the exact data the Accept/Decline/Tentative card renders.
    let (blob_id, invitation) = deliver_and_read(&app, &guest, &wire).await;
    assert_eq!(invitation["method"], "REQUEST", "{invitation}");
    assert_eq!(invitation["uid"], event_id.as_str(), "{invitation}");
    assert_eq!(invitation["summary"], "Design review", "{invitation}");
    assert_eq!(invitation["organizer"], organizer.email.as_str());

    // Declining records nothing on the guest's calendar…
    let (status, declined) = post(
        &app,
        &guest.token,
        "/calendar/rsvp",
        json!({ "blobId": blob_id, "response": "declined" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{declined}");
    assert_eq!(declined["added"], false, "{declined}");
    assert_eq!(declined["replied"], true, "{declined}");
    let (status, _) = get(&app, &guest.token, &format!("/calendar/events/{event_id}")).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a declined event is not added"
    );
    let (_decline_rcpts, _decline_wire) = sink.recv().await.expect("the DECLINED reply was sent");

    // …then accepting (a change of mind) lands the event and replies again.
    let (status, accepted) = post(
        &app,
        &guest.token,
        "/calendar/rsvp",
        json!({ "blobId": blob_id, "response": "accepted" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(accepted["added"], true, "{accepted}");
    assert_eq!(accepted["replied"], true, "{accepted}");
    let (status, stored) = get(&app, &guest.token, &format!("/calendar/events/{event_id}")).await;
    assert_eq!(status, StatusCode::OK, "{stored}");
    assert_eq!(stored["summary"], "Design review", "{stored}");

    // The REPLY rides back to the organizer through the same listener.
    let (reply_rcpts, reply_wire) = sink.recv().await.expect("the ACCEPTED reply was sent");
    assert!(
        reply_rcpts.iter().any(|r| r.contains(&organizer.email)),
        "the reply is addressed to the organizer: {reply_rcpts:?}"
    );
    let reply_text = String::from_utf8_lossy(&reply_wire);
    assert!(reply_text.contains("method=REPLY"), "{reply_text}");

    // Delivered to the organizer, the reading pane sees the guest's response…
    let (reply_blob, reply_inv) = deliver_and_read(&app, &organizer, &reply_wire).await;
    assert_eq!(reply_inv["method"], "REPLY", "{reply_inv}");
    assert_eq!(reply_inv["attendee"], guest.email.as_str(), "{reply_inv}");
    assert_eq!(reply_inv["partstat"], "accepted", "{reply_inv}");

    // …and applying it (what the reply card does on mount) records the
    // PARTSTAT on the organizer's copy of the event.
    let (status, applied) = post(
        &app,
        &organizer.token,
        "/calendar/apply-reply",
        json!({ "blobId": reply_blob }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    assert_eq!(applied["applied"], true, "{applied}");
    assert_eq!(applied["email"], guest.email.as_str(), "{applied}");
    assert_eq!(applied["status"], "ACCEPTED", "{applied}");
    let (status, org_event) = get(
        &app,
        &organizer.token,
        &format!("/calendar/events/{event_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{org_event}");
    let statuses = org_event["attendeeStatus"].as_array().unwrap();
    assert!(
        statuses
            .iter()
            .any(|s| s["email"] == guest.email.as_str() && s["status"] == "ACCEPTED"),
        "the guest's acceptance is on the organizer's event: {org_event}"
    );
}

/// A CANCEL naming one instance (`RECURRENCE-ID`) removes that occurrence from
/// the guest's stored series — never the series; a CANCEL without one removes
/// the whole event. The queue item's mandated test.
#[tokio::test]
async fn cancel_removes_the_instance_not_the_series_when_it_names_one() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&common::database_url())
        .await
        .expect("connect to test postgres");
    let store = Arc::new(Store::new(pool, BlobStore::in_memory(50 * 1024 * 1024)));
    store.migrate().await.unwrap();
    let tenant = store.create_tenant("jmap-imip-cx").await.unwrap();
    let identity = common::test_identity(Arc::clone(&store));
    let organizer = provision(&store, &identity, &tenant, "organizer").await;
    let guest = provision(&store, &identity, &tenant, "guest").await;
    let (sink_addr, mut sink) = spawn_sink().await;
    let app = app_with_sink(&store, &identity, sink_addr);

    // A weekly series (4 Mondays 09:00Z) with the guest invited.
    let (status, created) = post(
        &app,
        &organizer.token,
        "/calendar/events",
        json!({
            "summary": "Weekly standup",
            "startsAt": "2026-09-07T09:00:00Z",
            "endsAt": "2026-09-07T09:30:00Z",
            "recurrence": "FREQ=WEEKLY;COUNT=4",
            "attendees": [guest.email],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let event_id = created["id"].as_str().unwrap().to_owned();

    // Guest receives + accepts, so the series is on their calendar.
    let (_rcpts, request_wire) = sink.recv().await.expect("REQUEST sent");
    let (blob_id, _) = deliver_and_read(&app, &guest, &request_wire).await;
    let (status, accepted) = post(
        &app,
        &guest.token,
        "/calendar/rsvp",
        json!({ "blobId": blob_id, "response": "accepted" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let _reply = sink.recv().await.expect("REPLY sent");

    // The organizer cancels ONE occurrence (the 2nd Monday).
    let (status, resp) = delete(
        &app,
        &organizer.token,
        &format!("/calendar/events/{event_id}?occurrence=2026-09-14T09:00:00Z"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resp}");
    let (_rcpts, cancel_wire) = sink.recv().await.expect("one-instance CANCEL sent");
    assert!(
        String::from_utf8_lossy(&cancel_wire).contains("method=CANCEL"),
        "the wire carries a CANCEL part"
    );

    // Applied on the guest's side: the instance goes, the series stays.
    let (cancel_blob, cancel_inv) = deliver_and_read(&app, &guest, &cancel_wire).await;
    assert_eq!(cancel_inv["method"], "CANCEL", "{cancel_inv}");
    let (status, applied) = post(
        &app,
        &guest.token,
        "/calendar/cancel",
        json!({ "blobId": cancel_blob }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    assert_eq!(applied["removed"], true, "{applied}");
    assert_eq!(applied["scope"], "occurrence", "{applied}");
    let (status, _) = get(&app, &guest.token, &format!("/calendar/events/{event_id}")).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the series survives an instance CANCEL"
    );
    let (status, listed) = get(
        &app,
        &guest.token,
        "/calendar/events?from=2026-09-01T00:00:00Z&to=2026-10-05T00:00:00Z",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let starts: Vec<&str> = listed["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["id"] == event_id.as_str())
        .map(|e| e["startsAt"].as_str().unwrap())
        .collect();
    assert_eq!(
        starts,
        vec![
            "2026-09-07T09:00:00Z",
            "2026-09-21T09:00:00Z",
            "2026-09-28T09:00:00Z",
        ],
        "exactly the cancelled Monday is gone"
    );

    // The organizer then cancels the whole series: no RECURRENCE-ID, and the
    // guest's copy goes with it.
    let (status, resp) = delete(
        &app,
        &organizer.token,
        &format!("/calendar/events/{event_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resp}");
    let (_rcpts, series_cancel) = sink.recv().await.expect("series CANCEL sent");
    let (series_blob, _) = deliver_and_read(&app, &guest, &series_cancel).await;
    let (status, applied) = post(
        &app,
        &guest.token,
        "/calendar/cancel",
        json!({ "blobId": series_blob }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    assert_eq!(applied["removed"], true, "{applied}");
    assert_eq!(applied["scope"], "series", "{applied}");
    let (status, _) = get(&app, &guest.token, &format!("/calendar/events/{event_id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "the series is gone");

    // Re-applying the same cancellation is honoured, not an error.
    let (status, again) = post(
        &app,
        &guest.token,
        "/calendar/cancel",
        json!({ "blobId": series_blob }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["removed"], false, "{again}");
}

/// The blob load is the ownership boundary on every iMIP door: another user's
/// (or another tenant's) message blob is a clean 404 — never data, never a 500.
#[tokio::test]
async fn foreign_blobs_are_not_found_on_every_imip_door() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&common::database_url())
        .await
        .expect("connect to test postgres");
    let store = Arc::new(Store::new(pool, BlobStore::in_memory(50 * 1024 * 1024)));
    store.migrate().await.unwrap();
    let tenant = store.create_tenant("jmap-imip-t1").await.unwrap();
    let tenant2 = store.create_tenant("jmap-imip-t2").await.unwrap();
    let identity = common::test_identity(Arc::clone(&store));
    let organizer = provision(&store, &identity, &tenant, "organizer").await;
    let guest = provision(&store, &identity, &tenant, "guest").await;
    let mallory = provision(&store, &identity, &tenant2, "mallory").await;
    let (sink_addr, mut sink) = spawn_sink().await;
    let app = app_with_sink(&store, &identity, sink_addr);

    let (status, created) = post(
        &app,
        &organizer.token,
        "/calendar/events",
        json!({
            "summary": "Private sync",
            "startsAt": "2026-09-08T09:00:00Z",
            "endsAt": "2026-09-08T10:00:00Z",
            "attendees": [guest.email],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let (_rcpts, wire) = sink.recv().await.expect("REQUEST sent");
    let (blob_id, _) = deliver_and_read(&app, &guest, &wire).await;

    // Wrong tenant AND wrong user (same tenant): every door answers 404, and
    // the guest's ability to act on their own blob is untouched after.
    for token in [&mallory.token, &organizer.token] {
        for (uri, body) in [
            (
                "/calendar/rsvp",
                json!({ "blobId": blob_id, "response": "accepted" }),
            ),
            ("/calendar/cancel", json!({ "blobId": blob_id })),
            ("/calendar/apply-reply", json!({ "blobId": blob_id })),
        ] {
            let (status, resp) = post(&app, token, uri, body).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {resp}");
        }
    }
    let (status, ok) = post(
        &app,
        &guest.token,
        "/calendar/rsvp",
        json!({ "blobId": blob_id, "response": "tentative" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{ok}");
    assert_eq!(ok["added"], true, "{ok}");
}
