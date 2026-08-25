//! Per-identity signatures over `Identity/get` + `Identity/set` (RFC 8621 §6).
//!
//! The fact under test is per-identity: someone who sends as support@ and as
//! their own name signs those two differently. Before `Identity/set` existed,
//! `Identity/get` advertised `textSignature`/`htmlSignature` on every identity
//! and a client's attempt to set one was refused as an unknown method — a
//! per-identity fact was promised with nowhere to put it, and every identity
//! silently shared the one account signature.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{api, harness};
use serde_json::{Value, json};

fn call(account_id: &str, method: &str, mut args: Value) -> Value {
    args["accountId"] = json!(account_id);
    json!({
        "using": ["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:mail",
                  "urn:ietf:params:jmap:submission"],
        "methodCalls": [[method, args, "c"]],
    })
}

/// All identities the server lists for this account.
async fn identities(h: &common::Harness) -> Vec<Value> {
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&h.user.to_string(), "Identity/get", json!({ "ids": null })),
    )
    .await;
    body["methodResponses"][0][1]["list"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[tokio::test]
async fn an_untouched_identity_serves_the_account_signature() {
    // The fall-back IS the old behaviour: an account that never opens
    // Identity/set must go on exactly as before this existed.
    let h = harness("idsig-fallback").await;
    h.acc.set_signature("<p>Kind regards, A</p>").await.unwrap();

    let list = identities(&h).await;
    assert_eq!(list.len(), 1, "{list:?}");
    assert_eq!(list[0]["htmlSignature"], json!("<p>Kind regards, A</p>"));
    assert_eq!(list[0]["textSignature"], json!(""));
}

#[tokio::test]
async fn each_identity_keeps_its_own_signature() {
    // Two send identities, two signatures — the case the feature exists for.
    let h = harness("idsig-two").await;
    let acct = h.user.to_string();
    h.ts.add_alias(&h.user, &format!("support-{}@example.test", h.tenant))
        .await
        .unwrap();
    h.acc.set_signature("<p>Me, personally</p>").await.unwrap();

    let list = identities(&h).await;
    assert_eq!(list.len(), 2, "canonical + alias: {list:?}");
    let own = list[0]["id"].as_str().unwrap().to_owned();
    let support = list[1]["id"].as_str().unwrap().to_owned();

    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &acct,
            "Identity/set",
            json!({ "update": { &support: {
                "textSignature": "The support team",
                "htmlSignature": "<p>The support team</p>",
            }}}),
        ),
    )
    .await;
    assert!(
        body["methodResponses"][0][1]["updated"]
            .get(&support)
            .is_some(),
        "{body}",
    );

    let list = identities(&h).await;
    let by_id = |id: &str| {
        list.iter()
            .find(|i| i["id"].as_str() == Some(id))
            .unwrap()
            .clone()
    };
    assert_eq!(
        by_id(&support)["htmlSignature"],
        json!("<p>The support team</p>"),
        "the alias signs as the team",
    );
    assert_eq!(by_id(&support)["textSignature"], json!("The support team"));
    assert_eq!(
        by_id(&own)["htmlSignature"],
        json!("<p>Me, personally</p>"),
        "the personal identity is untouched by the alias's signature",
    );
}

#[tokio::test]
async fn a_patch_touches_only_the_spelling_it_names() {
    // RFC 8620 §5.3: unnamed properties keep their value. A client that edits
    // the plain-text spelling must not silently wipe the HTML one.
    let h = harness("idsig-patch").await;
    let acct = h.user.to_string();
    let id = identities(&h).await[0]["id"].as_str().unwrap().to_owned();

    api(
        &h.app,
        &h.token,
        call(
            &acct,
            "Identity/set",
            json!({ "update": { &id: {
                "textSignature": "plain", "htmlSignature": "<p>rich</p>",
            }}}),
        ),
    )
    .await;
    api(
        &h.app,
        &h.token,
        call(
            &acct,
            "Identity/set",
            json!({ "update": { &id: { "textSignature": "plain v2" }}}),
        ),
    )
    .await;

    let list = identities(&h).await;
    assert_eq!(list[0]["textSignature"], json!("plain v2"));
    assert_eq!(
        list[0]["htmlSignature"],
        json!("<p>rich</p>"),
        "the unnamed spelling survived",
    );
}

#[tokio::test]
async fn clearing_both_spellings_restores_the_account_fallback() {
    // An explicit "nothing" is deliberately the same as "never set": the row
    // is deleted, and the identity goes back to the account-level signature
    // rather than pinning emptiness over it.
    let h = harness("idsig-clear").await;
    let acct = h.user.to_string();
    h.acc.set_signature("<p>account-wide</p>").await.unwrap();
    let id = identities(&h).await[0]["id"].as_str().unwrap().to_owned();

    api(
        &h.app,
        &h.token,
        call(
            &acct,
            "Identity/set",
            json!({ "update": { &id: { "htmlSignature": "<p>bespoke</p>" }}}),
        ),
    )
    .await;
    assert_eq!(
        identities(&h).await[0]["htmlSignature"],
        json!("<p>bespoke</p>"),
    );

    api(
        &h.app,
        &h.token,
        call(
            &acct,
            "Identity/set",
            json!({ "update": { &id: { "textSignature": "", "htmlSignature": "" }}}),
        ),
    )
    .await;
    assert_eq!(
        identities(&h).await[0]["htmlSignature"],
        json!("<p>account-wide</p>"),
        "back on the fall-back",
    );
}

#[tokio::test]
async fn identities_are_provisioned_not_created_or_destroyed() {
    let h = harness("idsig-frozen").await;
    let acct = h.user.to_string();
    let id = identities(&h).await[0]["id"].as_str().unwrap().to_owned();

    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &acct,
            "Identity/set",
            json!({ "create": { "x": { "email": "new@example.test" } }, "destroy": [&id] }),
        ),
    )
    .await;
    let r = &body["methodResponses"][0][1];
    assert_eq!(r["notCreated"]["x"]["type"], json!("forbidden"), "{body}");
    assert_eq!(r["notDestroyed"][&id]["type"], json!("forbidden"), "{body}");
}

#[tokio::test]
async fn only_the_signature_is_settable() {
    // Refusing is honest; answering "updated" to a name change that went
    // nowhere teaches a client to trust what did not happen.
    let h = harness("idsig-props").await;
    let acct = h.user.to_string();
    let id = identities(&h).await[0]["id"].as_str().unwrap().to_owned();

    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &acct,
            "Identity/set",
            json!({ "update": { &id: { "email": "other@example.test" }}}),
        ),
    )
    .await;
    assert_eq!(
        body["methodResponses"][0][1]["notUpdated"][&id]["type"],
        json!("invalidProperties"),
        "{body}",
    );
}

#[tokio::test]
async fn another_users_identity_is_not_found_not_forbidden() {
    // B patching A's identity id gets the same notFound as a made-up id: no
    // oracle over which identities exist in the tenant beyond your own.
    let store = {
        let h = harness("idsig-owner-a").await;
        let a_id = identities(&h).await[0]["id"].as_str().unwrap().to_owned();
        let hb = common::harness_on(std::sync::Arc::clone(&h.store), "idsig-owner-b").await;
        let (_s, body) = api(
            &hb.app,
            &hb.token,
            call(
                &hb.user.to_string(),
                "Identity/set",
                json!({ "update": { &a_id: { "htmlSignature": "<p>hijack</p>" }}}),
            ),
        )
        .await;
        assert_eq!(
            body["methodResponses"][0][1]["notUpdated"][&a_id]["type"],
            json!("notFound"),
            "{body}",
        );
        // And A's identity is untouched.
        assert_eq!(identities(&h).await[0]["htmlSignature"], json!(""));
        std::sync::Arc::clone(&h.store)
    };
    drop(store);
}
