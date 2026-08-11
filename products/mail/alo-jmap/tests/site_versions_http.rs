//! The `/sites/{id}/publishes*` version-history surface (ADR 0036, S2.04a),
//! driven through the real router over a real Postgres.
//!
//! `alo-store`'s own suite proves the storage. What this pins is the edge:
//! the auth guard, the exact JSON the visible surface (S2.04b) will read,
//! the `404` that makes another tenant's version indistinguishable from one
//! that never existed, and the restore answering with both the new publish
//! and the version it came from.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use common::{Harness, harness, harness_on, send};

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

async fn put(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, with_json("PUT", uri, Some(token), body)).await
}

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

/// A subdomain unique to this harness run — the global namespace is shared.
fn sub(tag: &str, h: &Harness) -> String {
    let salt: String = h
        .tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .take(20)
        .collect();
    format!("{tag}{salt}")
}

fn hero(heading: &str) -> Value {
    json!({ "type": "hero", "heading": heading })
}

/// Creates a site with a home page carrying one hero, and publishes it.
async fn site_with_home(h: &Harness, tag: &str, heading: &str) -> (String, String, String) {
    let (status, site) = post(
        &h.app,
        &h.token,
        "/sites",
        json!({ "name": "Roastery", "subdomain": sub(tag, h) }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create site: {site}");
    let site_id = site["id"].as_str().unwrap().to_owned();
    let (status, page) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site_id}/pages"),
        json!({ "title": "Home", "slug": "", "home": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create page: {page}");
    let page_id = page["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site_id}/pages/{page_id}/sections"),
        json!({ "section": hero(heading) }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "add hero: {body}");
    let (status, published) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site_id}/publish"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "publish: {published}");
    let publish_id = published["publishId"].as_str().unwrap().to_owned();
    (site_id, page_id, publish_id)
}

#[tokio::test]
async fn version_routes_require_a_bearer_token() {
    let h = harness("site-versions-401").await;
    let attempts = [
        ("GET", "/sites/some-id/publishes", None),
        (
            "GET",
            "/sites/some-id/publishes/compare?from=a&to=b",
            None::<Value>,
        ),
        (
            "POST",
            "/sites/some-id/publishes/some-publish/restore",
            Some(json!({})),
        ),
    ];
    for (method, uri, body) in attempts {
        let (status, problem) = send(
            &h.app,
            with_json(method, uri, None, body.unwrap_or(Value::Null)),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri}: {problem}"
        );
    }
}

/// The whole visible arc the history surface needs: two versions listed
/// newest first, the live one named, a metadata comparison, and a restore
/// that appends rather than rewrites.
#[tokio::test]
async fn the_history_lists_compares_and_restores_versions() {
    let h = harness("site-versions").await;
    let (site, page, first) = site_with_home(&h, "ver", "First heading").await;

    // An empty history is a route that answers, not one that fails.
    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}/publishes")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["publishes"].as_array().unwrap().len(), 1);
    assert_eq!(body["current"], json!(first));
    assert_eq!(body["publishes"][0]["current"], json!(true));
    assert_eq!(body["publishes"][0]["pages"], json!(1));
    assert_eq!(body["publishes"][0]["restoredFrom"], Value::Null);
    assert_eq!(body["publishes"][0]["enabledLocales"], json!(["en"]));
    assert_eq!(body["publishes"][0]["locales"], json!(["en"]));
    assert!(
        body["publishes"][0]["publishedAt"]
            .as_str()
            .unwrap()
            .contains('T')
    );

    // A second version: the hero rewritten, a page added, another theme.
    let (status, body) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/{page}/sections/0"),
        json!({ "section": hero("Second heading") }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages"),
        json!({ "title": "About", "slug": "about" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/theme"),
        json!({ "schema_version": 1, "preset": "terra" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, published) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site}/publish"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{published}");
    let second = published["publishId"].as_str().unwrap().to_owned();

    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}/publishes")).await;
    assert_eq!(status, StatusCode::OK);
    let listed: Vec<&str> = body["publishes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|version| version["id"].as_str().unwrap())
        .collect();
    assert_eq!(listed, vec![second.as_str(), first.as_str()]);
    assert_eq!(body["current"], json!(second));
    assert_eq!(body["publishes"][0]["pages"], json!(2));

    // ?limit= narrows the list without refusing anything.
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/sites/{site}/publishes?limit=1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["publishes"].as_array().unwrap().len(), 1);
    assert_eq!(body["publishes"][0]["id"], json!(second));

    // ---- the comparison -----------------------------------------------------
    let (status, diff) = get(
        &h.app,
        &h.token,
        &format!("/sites/{site}/publishes/compare?from={first}&to={second}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{diff}");
    assert_eq!(diff["identical"], json!(false));
    assert_eq!(diff["themeChanged"], json!(true));
    assert_eq!(diff["defaultLocaleChanged"], json!(false));
    assert_eq!(diff["localesAdded"], json!([]));
    assert_eq!(diff["from"]["id"], json!(first));
    assert_eq!(diff["to"]["id"], json!(second));
    let pages = diff["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 2);
    let changed = pages
        .iter()
        .find(|entry| entry["pageId"] == json!(page))
        .unwrap();
    assert_eq!(changed["change"], json!("changed"));
    assert_eq!(changed["fields"], json!(["sections"]));
    let added = pages
        .iter()
        .find(|entry| entry["change"] == json!("added"))
        .unwrap();
    assert_eq!(added["slug"], json!("about"));
    assert_eq!(added["fields"], json!([]));

    // ---- the restore --------------------------------------------------------
    let (status, restored) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site}/publishes/{first}/restore"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    assert_eq!(restored["restoredFrom"], json!(first));
    assert_eq!(restored["status"], json!("live"));
    let new_publish = restored["publishId"].as_str().unwrap().to_owned();
    assert_ne!(new_publish, first, "a restore appends a new version");

    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}/publishes")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["publishes"].as_array().unwrap().len(), 3);
    assert_eq!(body["current"], json!(new_publish));
    assert_eq!(body["publishes"][0]["restoredFrom"], json!(first));

    // What is live is the old version again; the draft kept the new work.
    let (status, diff) = get(
        &h.app,
        &h.token,
        &format!("/sites/{site}/publishes/compare?from={first}&to={new_publish}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(diff["identical"], json!(true));
    assert_eq!(diff["pages"], json!([]));
    let (status, draft) = get(&h.app, &h.token, &format!("/sites/{site}/pages/{page}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        draft["sections"]["sections"][0]["heading"],
        json!("Second heading"),
        "restoring what is published never rewrites the draft"
    );

    // The site itself reports the restored version as its current publish.
    let (status, site_body) = get(&h.app, &h.token, &format!("/sites/{site}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(site_body["publish"]["id"], json!(new_publish));
    assert_eq!(site_body["status"], json!("live"));
}

/// An unknown site, an unknown version, and another tenant's version all
/// answer the same `404` — the surface is not an oracle.
#[tokio::test]
async fn another_tenants_version_is_invisible_and_unrestorable() {
    let a = harness("site-versions-a").await;
    let b = harness_on(Arc::clone(&a.store), "site-versions-b").await;
    let (a_site, _, a_publish) = site_with_home(&a, "vera", "Owned by A").await;
    let (b_site, _, b_publish) = site_with_home(&b, "verb", "Owned by B").await;

    let attempts = [
        ("GET", format!("/sites/{a_site}/publishes")),
        (
            "GET",
            format!("/sites/{a_site}/publishes/compare?from={a_publish}&to={a_publish}"),
        ),
        (
            "POST",
            format!("/sites/{a_site}/publishes/{a_publish}/restore"),
        ),
        // A's version id addressed through B's own site: real id, wrong tenant.
        (
            "GET",
            format!("/sites/{b_site}/publishes/compare?from={a_publish}&to={b_publish}"),
        ),
        (
            "POST",
            format!("/sites/{b_site}/publishes/{a_publish}/restore"),
        ),
        // Ids that never existed answer identically.
        ("GET", "/sites/no-such-site/publishes".to_owned()),
        (
            "POST",
            format!("/sites/{b_site}/publishes/no-such-version/restore"),
        ),
    ];
    for (method, uri) in attempts {
        let (status, problem) =
            send(&b.app, with_json(method, &uri, Some(&b.token), json!({}))).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}: {problem}");
        assert!(
            !problem.to_string().contains(a_publish.as_str()),
            "a refusal must not echo another tenant's ids: {problem}"
        );
    }

    // A is untouched: still live on its own version, with a history of one.
    let (status, body) = get(&a.app, &a.token, &format!("/sites/{a_site}/publishes")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["publishes"].as_array().unwrap().len(), 1);
    assert_eq!(body["current"], json!(a_publish));
    let (status, body) = get(&b.app, &b.token, &format!("/sites/{b_site}/publishes")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["publishes"].as_array().unwrap().len(), 1);
    assert_eq!(body["current"], json!(b_publish));
}

/// GETs a route that answers a document rather than JSON.
async fn get_text(
    app: &Router,
    token: Option<&str>,
    uri: &str,
) -> (StatusCode, axum::http::HeaderMap, String) {
    use tower::ServiceExt;
    let mut req = Request::builder().method("GET").uri(uri);
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    let resp = app
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

/// What the visible history surface (S2.04b) reads: the pages one version
/// froze, and each of them rendered as the document that version served —
/// never the draft, and never today's theme.
#[tokio::test]
async fn a_version_lists_and_renders_the_pages_it_froze() {
    let h = harness("site-versions-preview").await;
    let (site, page, first) = site_with_home(&h, "verpv", "Bread & butter").await;

    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/sites/{site}/publishes/{first}/pages"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let pages = body["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0]["pageId"], json!(page));
    assert_eq!(pages[0]["locale"], json!("en"));
    assert_eq!(pages[0]["slug"], json!(""));
    assert_eq!(pages[0]["title"], json!("Home"));
    assert_eq!(pages[0]["home"], json!(true));

    // Move the draft on: new copy, another page, another theme, published.
    let (status, body) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/{page}/sections/0"),
        json!({ "section": hero("Sourdough, daily") }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/theme"),
        json!({ "schema_version": 1, "preset": "midnight" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, about) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages"),
        json!({ "title": "About", "slug": "about" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{about}");
    let about_page = about["id"].as_str().unwrap().to_owned();
    let (status, published) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site}/publish"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{published}");
    let second = published["publishId"].as_str().unwrap().to_owned();

    // The first version still renders what it froze, theme included.
    let (status, headers, html) = get_text(
        &h.app,
        Some(&h.token),
        &format!("/sites/{site}/publishes/{first}/pages/{page}/preview"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{html}");
    assert_eq!(headers["content-type"], "text/html; charset=utf-8");
    assert_eq!(headers["cache-control"], "no-store");
    assert!(html.contains("Bread &amp; butter"), "{html}");
    assert!(
        !html.contains("Sourdough, daily"),
        "a version preview must never render the draft: {html}"
    );
    assert!(
        html.contains("<style>"),
        "the preview is self-contained: {html}"
    );

    // The newer version renders the newer copy, and lists both its pages.
    let (status, _, html) = get_text(
        &h.app,
        Some(&h.token),
        &format!("/sites/{site}/publishes/{second}/pages/{page}/preview"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Sourdough, daily"), "{html}");
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/sites/{site}/publishes/{second}/pages"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["pages"].as_array().unwrap().len(), 2);

    // A page the older version never froze is not in it, and says so.
    let (status, problem) = get(
        &h.app,
        &h.token,
        &format!("/sites/{site}/publishes/{first}/pages/{about_page}/preview"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    assert_eq!(problem["detail"], json!("no such page in this version"));

    // A language this version never froze falls back to the one it did.
    let (status, _, html) = get_text(
        &h.app,
        Some(&h.token),
        &format!("/sites/{site}/publishes/{first}/pages/{page}/preview?locale=fr"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Bread &amp; butter"), "{html}");

    // An unknown version is a refusal on both routes, and neither is open.
    for uri in [
        format!("/sites/{site}/publishes/no-such-version/pages"),
        format!("/sites/{site}/publishes/no-such-version/pages/{page}/preview"),
    ] {
        let (status, problem) = get(&h.app, &h.token, &uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {problem}");
        let (status, _, _) = get_text(&h.app, None, &uri).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri} unauthenticated");
    }
}

/// Another tenant cannot list or render what a version of a foreign site
/// froze, even holding the real ids.
#[tokio::test]
async fn a_foreign_tenant_cannot_read_a_versions_pages() {
    let a = harness("site-versions-pv-a").await;
    let b = harness_on(Arc::clone(&a.store), "site-versions-pv-b").await;
    let (a_site, a_page, a_publish) = site_with_home(&a, "verpva", "Owned by A").await;
    let (b_site, _, _) = site_with_home(&b, "verpvb", "Owned by B").await;

    for uri in [
        format!("/sites/{a_site}/publishes/{a_publish}/pages"),
        format!("/sites/{a_site}/publishes/{a_publish}/pages/{a_page}/preview"),
        // A's real version id addressed through B's own site.
        format!("/sites/{b_site}/publishes/{a_publish}/pages"),
        format!("/sites/{b_site}/publishes/{a_publish}/pages/{a_page}/preview"),
    ] {
        let (status, headers, body) = get_text(&b.app, Some(&b.token), &uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {body}");
        assert!(
            !body.contains("Owned by A"),
            "{uri} leaked another tenant's content: {body}"
        );
        assert!(
            !body.contains(a_publish.as_str()),
            "{uri} echoed another tenant's id: {body}"
        );
        assert_ne!(
            headers.get("content-type").map(|v| v.to_str().unwrap()),
            Some("text/html; charset=utf-8"),
            "{uri} answered a document to an outsider"
        );
    }
}

/// A comparison needs both ends: a malformed request is a refusal, not a
/// half-answer.
#[tokio::test]
async fn a_comparison_without_both_ends_is_refused() {
    let h = harness("site-versions-compare").await;
    let (site, _, publish) = site_with_home(&h, "vercmp", "Only version").await;

    let (status, problem) = get(
        &h.app,
        &h.token,
        &format!("/sites/{site}/publishes/compare?from={publish}"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        problem["detail"],
        json!("a comparison needs both a from and a to version"),
        "the refusal is this surface's own Problem, not a framework message"
    );

    // A limit that is not a number is a default, not a broken screen.
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/sites/{site}/publishes?limit=abc"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["publishes"].as_array().unwrap().len(), 1);

    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/sites/{site}/publishes/compare?from={publish}&to={publish}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["identical"], json!(true));
    assert_eq!(body["unchangedPages"], json!(1));
}
