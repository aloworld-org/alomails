//! A deployment with no submission listener must say *configuration*, not
//! *permission*.
//!
//! `EmailSubmission/set` hands the message to alo-smtp's trusted internal
//! listener. With `ALO_JMAP_SUBMISSION_ADDR` unset there is nowhere to hand it,
//! and nobody can send at all. That used to answer `forbiddenToSend`, which
//! clients render as "you may not send from this address" — so whoever hit it
//! went through identities, aliases and send-as rules, none of which were
//! wrong, while the actual cause was a missing environment variable.
//!
//! The distinction only matters when a deployment is broken, which is exactly
//! when nobody can afford to be sent the wrong way.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use alo_identity::{Identity, IdentityConfig};
use alo_jmap::PushHub;
use alo_jmap::mime::{Addr, Outgoing, build};
use alo_jmap::state::{Account, AppState, Limits};
use alo_store::{BlobStore, Store};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;

#[tokio::test]
async fn a_missing_submission_listener_is_a_server_fault_not_a_forbidden_sender() {
    let Ok(pool) = PgPoolOptions::new()
        .max_connections(4)
        .connect(&alo_test_db::url())
        .await
    else {
        eprintln!("SKIP: no database at {}", alo_test_db::url());
        return;
    };
    let store = Arc::new(Store::new(pool, BlobStore::in_memory(8 * 1024 * 1024)));
    store.migrate().await.unwrap();

    let tenant = store.create_tenant("submit-unconfigured").await.unwrap();
    let sender = format!("sender-{tenant}@sink.test").to_lowercase();
    let ts = store.for_tenant(tenant.clone());
    let user = ts.create_user(&sender).await.unwrap();
    let acc = store.for_account(tenant.clone(), user.clone());

    // A draft with nothing wrong with it: the sender is the authenticated user,
    // the recipient is ordinary, and it is a draft. Every reason to refuse that
    // is about *this caller* has been removed, so the only thing left to answer
    // is the missing listener.
    let raw = build(&Outgoing {
        from: Addr {
            name: None,
            email: sender.clone(),
        },
        to: vec![Addr {
            name: None,
            email: "alice@recipient.test".into(),
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: "Anyone home".into(),
        in_reply_to: Vec::new(),
        references: Vec::new(),
        body_text: "Hello.\n".into(),
        body_html: None,
        attachments: Vec::new(),
        message_id_domain: "sink.test".into(),
        message_id_token: "unconfigured001".into(),
    });
    let drafts = acc
        .create_mailbox(None, "Drafts", Some("drafts"))
        .await
        .unwrap();
    acc.create_mailbox(None, "Sent", Some("sent"))
        .await
        .unwrap();
    let mid = acc.ingest(&drafts, &raw).await.unwrap();
    acc.set_keyword(&mid, "$draft", true).await.unwrap();

    let identity =
        Identity::new(Arc::clone(&store), IdentityConfig::new("https://id.test")).unwrap();
    let state = AppState {
        media: None,
        turns: Default::default(),
        store: Arc::clone(&store),
        identity,
        push: PushHub::new(),
        limits: Limits::default(),
        base_url: "http://test".into(),
        // The whole point of this test.
        submission_addr: None,
        session_origins: Vec::new(),
        web_push: None,
        junk_learner: None,
        personal_domains: Vec::new(),
        signup_limiter: alo_identity::ratelimit::RateLimiter::new(),
    };
    let account = Account {
        tenant,
        user: user.clone(),
        acc,
        is_admin: false,
        roles: Vec::new(),
        denied_modules: Vec::new(),
        delegated: None,
    };

    let args = json!({
        "accountId": user.to_string(),
        "create": { "c1": { "emailId": mid.to_string() } },
    });
    let resp = alo_jmap::submission::set(&account, &args, &state)
        .await
        .expect("the method itself should answer, not fail");

    let refusal = &resp["notCreated"]["c1"];
    assert_eq!(
        refusal["type"],
        json!("serverFail"),
        "a server that cannot send is our fault, not the sender's: {resp}",
    );
    assert!(
        refusal["description"]
            .as_str()
            .unwrap_or_default()
            .contains("configured"),
        "and the description should point at configuration: {resp}",
    );
    assert!(
        resp["created"]
            .as_object()
            .is_none_or(serde_json::Map::is_empty),
        "nothing was sent: {resp}",
    );
}
