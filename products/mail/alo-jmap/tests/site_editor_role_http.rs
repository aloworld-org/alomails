//! The per-site editor authorization matrix through the real HTTP router.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::UserId;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use common::{Harness, harness, harness_on, send};

fn subdomain(tag: &str, h: &Harness) -> String {
    let salt: String = h
        .tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|value| value.to_ascii_lowercase())
        .take(20)
        .collect();
    format!("{tag}{salt}")
}

async fn colleague(h: &Harness) -> (String, UserId) {
    let email = format!("site-editor-{}@example.test", h.tenant);
    let user = h.ts.create_user(&email).await.unwrap();
    h.identity
        .set_password(&h.tenant, &user, &email, "s3cret-pw")
        .await
        .unwrap();
    let token = h
        .identity
        .password_login(&email, "s3cret-pw", None)
        .await
        .unwrap()
        .expect("token issued")
        .0
        .reveal()
        .to_owned();
    (token, user)
}

async fn request(
    app: &Router,
    token: &str,
    method: &str,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    send(
        app,
        Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

async fn public_request(app: &Router, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(
        app,
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

#[tokio::test]
async fn an_editor_sees_and_changes_only_the_granted_site() {
    let h = harness("site-editor-scope").await;
    let (token, editor) = colleague(&h).await;
    let granted = h
        .acc
        .create_site("Granted", &subdomain("granted", &h))
        .await
        .unwrap();
    let other = h
        .acc
        .create_site("Other", &subdomain("other", &h))
        .await
        .unwrap();
    h.ts.grant_site_editor(&editor, &granted, &h.user)
        .await
        .unwrap();

    let (status, body) = request(&h.app, &token, "GET", "/sites", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let sites = body["sites"].as_array().unwrap();
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0]["id"], granted.as_str());

    let (status, body) = request(
        &h.app,
        &token,
        "PUT",
        &format!("/sites/{granted}"),
        json!({ "name": "Changed by editor" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        h.acc.site(&granted).await.unwrap().unwrap().name,
        "Changed by editor"
    );

    for uri in [format!("/sites/{other}"), "/sites/not-a-site".to_owned()] {
        let (status, body) = request(&h.app, &token, "GET", &uri, json!({})).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {body}");
    }
    let (status, _) = request(
        &h.app,
        &token,
        "POST",
        "/sites",
        json!({ "name": "No", "subdomain": "not-allowed" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn the_api_mount_scopes_a_site_editor_exactly_as_the_bare_path_does() {
    // The whole API answers at two addresses, and the browser uses the second
    // one: every call the web module makes is `/api/sites/…`. A gate that
    // reads the matched template therefore sees `/api/sites/{id}` for real
    // traffic and `/sites/{id}` only in tests — so the two mounts are asserted
    // together, or the rule is only ever proven at the address nobody uses.
    let h = harness("site-editor-api-mount").await;
    let (token, editor) = colleague(&h).await;
    let granted = h
        .acc
        .create_site("Granted", &subdomain("apimount", &h))
        .await
        .unwrap();
    let other = h
        .acc
        .create_site("Other", &subdomain("apiother", &h))
        .await
        .unwrap();
    h.ts.grant_site_editor(&editor, &granted, &h.user)
        .await
        .unwrap();

    let (status, body) = request(&h.app, &token, "GET", "/api/sites", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let sites = body["sites"].as_array().unwrap();
    assert_eq!(sites.len(), 1, "{body}");
    assert_eq!(sites[0]["id"], granted.as_str());

    let (status, body) = request(
        &h.app,
        &token,
        "GET",
        &format!("/api/sites/{granted}/pages"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = request(
        &h.app,
        &token,
        "PUT",
        &format!("/api/sites/{granted}"),
        json!({ "name": "Changed through the api mount" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        h.acc.site(&granted).await.unwrap().unwrap().name,
        "Changed through the api mount"
    );

    // ...and the closed half stays closed at the second address, which is the
    // half that would make this a hole rather than a lockout.
    for uri in [
        format!("/api/sites/{other}"),
        "/api/sites/not-a-site".to_owned(),
        "/api/contacts".to_owned(),
        "/api/drive/list".to_owned(),
        "/api/billing/customers".to_owned(),
        "/api/admin/users".to_owned(),
    ] {
        let (status, body) = request(&h.app, &token, "GET", &uri, json!({})).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {body}");
    }
}

#[tokio::test]
async fn the_editor_matrix_holds_over_the_surface_added_after_the_grant() {
    // The grant (S2.03a) was written when `/sites/{id}/*` was pages, sections,
    // theme and publish. Two waves later the same prefix also spends money and
    // holds the CRM identities behind a website, and the one middleware says
    // yes to every `/sites/{id}` template by construction — so what keeps the
    // money and the business doors shut is a per-handler guard, and the only
    // way to know each one is still there is to knock.
    let h = harness("site-editor-matrix").await;
    let (token, editor) = colleague(&h).await;
    let site = h
        .acc
        .create_site("Matrix", &subdomain("matrix", &h))
        .await
        .unwrap();
    h.ts.grant_site_editor(&editor, &site, &h.user)
        .await
        .unwrap();

    // The website itself, and the records it produced: this is the work the
    // collaborator was invited to do.
    for path in [
        "pages",
        "posts",
        "submissions",
        "orders",
        "bookings",
        "catalogs",
        "analytics",
        "heatmap",
        "conversions",
        "domains",
        "publishes",
    ] {
        let (status, body) = request(
            &h.app,
            &token,
            "GET",
            &format!("/api/sites/{site}/{path}"),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
    }

    // The money door, the guest list, and the CRM/Billing identities behind
    // the enquiries — each refused by its own handler, not by the middleware.
    for path in ["collaborators", "domain-purchases", "leads", "attribution"] {
        let (status, body) = request(
            &h.app,
            &token,
            "GET",
            &format!("/api/sites/{site}/{path}"),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}: {body}");
    }

    // Buying is not reachable from the site-less half of the surface either.
    for uri in [
        "/api/sites/domain-catalog",
        "/api/sites/domain-search?q=acme",
    ] {
        let (status, body) = request(&h.app, &token, "GET", uri, json!({})).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {body}");
    }
}

#[tokio::test]
async fn the_site_editor_role_opens_no_surrounding_workspace_surface() {
    let h = harness("site-editor-doors").await;
    let (token, editor) = colleague(&h).await;
    let site = h
        .acc
        .create_site("Granted", &subdomain("doors", &h))
        .await
        .unwrap();
    h.ts.grant_site_editor(&editor, &site, &h.user)
        .await
        .unwrap();

    for uri in [
        "/contacts",
        "/drive/list",
        "/calendar/calendars",
        "/tasks",
        "/billing/customers",
        "/crm/deals",
        "/admin/users",
        "/.well-known/jmap",
    ] {
        let (status, body) = request(&h.app, &token, "GET", uri, json!({})).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {body}");
    }

    let (status, body) = request(&h.app, &token, "GET", &format!("/sites/{site}"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    h.ts.revoke_site_editor(&editor, &site).await.unwrap();
    let (status, _) = request(&h.app, &token, "GET", "/contacts", json!({})).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "revocation restores ordinary membership"
    );
}

#[tokio::test]
async fn the_owner_invites_setup_edits_publishes_and_revokes_without_admin_access() {
    let owner = harness("site-editor-invite-http").await;
    let outsider = harness_on(owner.store.clone(), "site-editor-invite-outsider").await;
    let site = owner
        .acc
        .create_site("Shared site", &subdomain("shared", &owner))
        .await
        .unwrap();
    owner
        .acc
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();

    let (status, detail) = request(
        &owner.app,
        &owner.token,
        "GET",
        &format!("/sites/{site}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["canManageCollaborators"], true);

    let email = format!("restricted-{site}@example.test").to_ascii_lowercase();
    let (status, invited) = request(
        &owner.app,
        &owner.token,
        "POST",
        &format!("/sites/{site}/collaborators"),
        json!({ "email": email }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{invited}");
    assert_eq!(invited["collaborator"]["status"], "pending");
    let collaborator_id = invited["collaborator"]["id"].as_str().unwrap().to_owned();
    let invite_url = invited["inviteUrl"].as_str().expect("one-time link");
    let token = invite_url.rsplit('/').next().unwrap();
    assert!(!token.is_empty());

    let invitation_uri = format!("/sites/invitations/{token}");
    let (status, facts) = public_request(&owner.app, "GET", &invitation_uri, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{facts}");
    assert_eq!(facts["siteName"], "Shared site");
    assert_eq!(facts["email"], email);
    let (status, accepted) = public_request(
        &owner.app,
        "POST",
        &invitation_uri,
        json!({ "password": "a-private-password" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let (status, reused) = public_request(&owner.app, "GET", &invitation_uri, json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{reused}");

    let editor_token = owner
        .identity
        .password_login(&email, "a-private-password", None)
        .await
        .unwrap()
        .expect("the collaborator can sign in")
        .0
        .reveal()
        .to_owned();
    let (status, detail) = request(
        &owner.app,
        &editor_token,
        "GET",
        &format!("/sites/{site}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["canManageCollaborators"], false);
    let (status, body) = request(
        &owner.app,
        &editor_token,
        "GET",
        &format!("/sites/{site}/collaborators"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, body) = request(
        &owner.app,
        &editor_token,
        "PUT",
        &format!("/sites/{site}"),
        json!({ "name": "Edited together" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = request(
        &owner.app,
        &editor_token,
        "POST",
        &format!("/sites/{site}/publish"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = request(&owner.app, &editor_token, "GET", "/admin/users", json!({})).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    let (status, collaborators) = request(
        &owner.app,
        &owner.token,
        "GET",
        &format!("/sites/{site}/collaborators"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{collaborators}");
    assert_eq!(collaborators["collaborators"][0]["status"], "active");

    let (status, body) = request(
        &owner.app,
        &outsider.token,
        "DELETE",
        &format!("/sites/{site}/collaborators/{collaborator_id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, body) = request(
        &owner.app,
        &owner.token,
        "DELETE",
        &format!("/sites/{site}/collaborators/{collaborator_id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, _) = request(
        &owner.app,
        &editor_token,
        "GET",
        &format!("/sites/{site}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn inviting_an_existing_workspace_member_is_refused_without_narrowing_them() {
    let h = harness("site-editor-existing-member").await;
    let site = h
        .acc
        .create_site("Team site", &subdomain("team", &h))
        .await
        .unwrap();
    let member_email = format!("member-{site}@example.test");
    let member = h.ts.create_user(&member_email).await.unwrap();
    let (status, body) = request(
        &h.app,
        &h.token,
        "POST",
        &format!("/sites/{site}/collaborators"),
        json!({ "email": member_email }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(h.ts.site_editor_grants(&member).await.unwrap().is_empty());
}
