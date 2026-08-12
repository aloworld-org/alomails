//! The `/sites/*` edit surface (ADR 0036, S1.10), driven through the real
//! router over a real Postgres.
//!
//! `alo-store`'s own suites prove the storage; what this suite pins is the
//! **edge**: the auth guard, the status codes `docs/design/sites.md`
//! publishes (`404` for anything that doesn't resolve in the caller's
//! tenant, `422` with a rule-naming detail for every validation refusal,
//! subdomain-taken included), the index-addressed section operations, and —
//! mandatory — that another tenant's site is invisible and untouchable on
//! every verb.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::HashMap;
use std::sync::Arc;

use alo_sites::serve::{AppState as PublicAppState, app as public_app};
use alo_store::{DriveLocation, NewDriveFile, SiteId, SitePublicStore};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

use common::{Harness, database_url, harness, harness_on, harness_with_blobs, send};

struct FakeSiteDomainDns {
    answers: HashMap<String, Vec<String>>,
}

impl alo_jmap::sites::SiteDomainTxtLookup for FakeSiteDomainDns {
    fn lookup(&self, name: String) -> BoxFuture<'static, Vec<String>> {
        let records = self.answers.get(&name).cloned().unwrap_or_default();
        Box::pin(async move { records })
    }
}

fn app_with_dns(harness: &Harness, answers: HashMap<String, Vec<String>>) -> Router {
    alo_jmap::app_with_site_domain_dns(
        alo_jmap::app_state(
            Arc::clone(&harness.store),
            harness.identity.clone(),
            "http://test",
        ),
        Arc::new(FakeSiteDomainDns { answers }),
    )
}

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

async fn delete(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

async fn public_text(state: &Arc<PublicAppState>, request: Request<Body>) -> (StatusCode, String) {
    let response = public_app(Arc::clone(state))
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

/// A subdomain unique to this harness run: the tenant id is random per test,
/// so reruns against the shared database never collide in the global
/// subdomain namespace.
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

fn created_id(kind: &str, (status, body): (StatusCode, Value)) -> String {
    assert_eq!(status, StatusCode::OK, "create {kind} failed: {body}");
    body["id"].as_str().expect("created id").to_owned()
}

fn hero() -> Value {
    json!({ "type": "hero", "heading": "Coffee roasted the morning it ships",
            "subheading": "Small-batch roastery on the harbour." })
}

fn cta() -> Value {
    json!({ "type": "cta", "heading": "Taste the difference",
            "button": { "label": "Order now", "href": "/order" } })
}

fn faq() -> Value {
    json!({ "type": "faq", "items": [
        { "question": "Do you ship abroad?", "answer": "Across the EU, yes." },
    ] })
}

// ---- auth guard --------------------------------------------------------------

#[tokio::test]
async fn every_route_family_requires_a_bearer_token() {
    let h = harness("sites-401").await;
    let attempts = [
        ("GET", "/sites".to_owned(), None),
        (
            "POST",
            "/sites".to_owned(),
            Some(json!({ "name": "X", "subdomain": "no-token" })),
        ),
        (
            "GET",
            "/sites/subdomain-check?subdomain=whatever".to_owned(),
            None,
        ),
        ("GET", "/sites/theme-presets".to_owned(), None),
        ("GET", "/sites/config".to_owned(), None),
        ("GET", "/sites/some-id/submissions".to_owned(), None),
        ("GET", "/sites/some-id/analytics".to_owned(), None),
        ("GET", "/sites/some-id/domains".to_owned(), None),
        (
            "POST",
            "/sites/some-id/domains".to_owned(),
            Some(json!({ "domain": "example.test" })),
        ),
        (
            "POST",
            "/sites/some-id/domains/example.test/verify".to_owned(),
            Some(json!({})),
        ),
        (
            "DELETE",
            "/sites/some-id/domains/example.test".to_owned(),
            Some(json!({})),
        ),
        ("GET", "/sites/some-id/submissions.csv".to_owned(), None),
        ("GET", "/sites/some-id/posts".to_owned(), None),
        ("GET", "/sites/some-id/collections".to_owned(), None),
        ("GET", "/sites/some-id/collaborators".to_owned(), None),
        (
            "POST",
            "/sites/some-id/collaborators".to_owned(),
            Some(json!({ "email": "editor@example.test" })),
        ),
        (
            "DELETE",
            "/sites/some-id/collaborators/editor".to_owned(),
            None,
        ),
        (
            "POST",
            "/sites/some-id/collections".to_owned(),
            Some(json!({})),
        ),
        (
            "PUT",
            "/sites/some-id/collections/collection".to_owned(),
            Some(json!({})),
        ),
        (
            "DELETE",
            "/sites/some-id/collections/collection".to_owned(),
            Some(json!({})),
        ),
        (
            "GET",
            "/sites/some-id/collections/collection/preview".to_owned(),
            None,
        ),
        (
            "POST",
            "/sites/some-id/posts".to_owned(),
            Some(json!({ "docNodeId": "doc", "slug": "post", "title": "Post" })),
        ),
        ("GET", "/sites/some-id/posts/post".to_owned(), None),
        (
            "POST",
            "/sites/some-id/posts/post/publish".to_owned(),
            Some(json!({})),
        ),
        (
            "PUT",
            "/sites/some-id/forms/form/submissions/submission".to_owned(),
            Some(json!({ "handled": true })),
        ),
        ("PUT", "/sites/some-id/theme".to_owned(), Some(json!({}))),
        ("POST", "/sites/some-id/publish".to_owned(), Some(json!({}))),
        (
            "POST",
            "/sites/some-id/pages".to_owned(),
            Some(json!({ "title": "T", "slug": "t" })),
        ),
        (
            "PUT",
            "/sites/some-id/pages/p/sections/0".to_owned(),
            Some(json!({ "section": {} })),
        ),
        ("GET", "/sites/some-id/images/blob".to_owned(), None),
        ("GET", "/sites/some-id/pages/p/preview".to_owned(), None),
        ("GET", "/sites/some-id/pages/p/locales/fr".to_owned(), None),
    ];
    for (method, uri, body) in attempts {
        let req = match body {
            Some(b) => with_json(method, &uri, None, b),
            None => Request::builder()
                .method(method)
                .uri(&uri)
                .body(Body::empty())
                .unwrap(),
        };
        let (status, _) = send(&h.app, req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
}

// ---- Base-backed collections ------------------------------------------------

#[tokio::test]
async fn collection_routes_map_preview_disconnect_and_hide_other_tenants() {
    let owner = harness("sites-collections-http").await;
    let outsider = harness_on(Arc::clone(&owner.store), "sites-collections-other").await;
    let site = created_id(
        "site",
        post(
            &owner.app,
            &owner.token,
            "/sites",
            json!({ "name": "Roastery", "subdomain": sub("collection", &owner) }),
        )
        .await,
    );
    let page = created_id(
        "page",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/pages"),
            json!({ "title": "Home", "home": true }),
        )
        .await,
    );

    let base_node = owner
        .acc
        .create_base(&DriveLocation::Personal, None, "Roasts")
        .await
        .unwrap();
    let base = owner.acc.base(&base_node).await.unwrap().unwrap();
    let table = base.tables[0].id.clone();
    let title = base.tables[0].fields[0].id.clone();
    let summary = base.tables[0].fields[1].id.clone();
    let mut cells = serde_json::Map::new();
    cells.insert(title.as_str().to_owned(), json!("Harbour Blend"));
    cells.insert(
        summary.as_str().to_owned(),
        json!("Chocolate and red apple"),
    );
    owner
        .acc
        .base_add_record(&table, &Value::Object(cells))
        .await
        .unwrap();

    let connection = json!({
        "name": "Seasonal roasts",
        "baseNodeId": base_node.as_str(),
        "baseTableId": table.as_str(),
        "mapping": {
            "title": title.as_str(),
            "summary": summary.as_str()
        }
    });
    let collection = created_id(
        "collection",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/collections"),
            connection.clone(),
        )
        .await,
    );

    let (status, listed) = get(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/collections"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["collections"][0]["name"], json!("Seasonal roasts"));

    let (status, preview) = get(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/collections/{collection}/preview"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "collection preview failed: {preview}"
    );
    assert_eq!(preview["items"][0]["title"], json!("Harbour Blend"));
    assert_eq!(
        preview["items"][0]["summary"],
        json!("Chocolate and red apple")
    );

    let (status, section) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/pages/{page}/sections"),
        json!({
            "section": {
                "type": "collection",
                "collection_id": collection,
                "heading": "Fresh from the roaster"
            }
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "collection section failed: {section}"
    );
    let (status, _, html) = get_text(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/pages/{page}/preview"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Fresh from the roaster"));
    assert!(html.contains("Harbour Blend"));
    assert!(html.contains("Chocolate and red apple"));

    let mut renamed = connection;
    renamed["name"] = json!("Featured roasts");
    let (status, updated) = put(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/collections/{collection}"),
        renamed.clone(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "collection update failed: {updated}"
    );
    assert_eq!(updated["name"], json!("Featured roasts"));

    let foreign_attempts = [
        ("GET", format!("/sites/{site}/collections"), json!({})),
        (
            "POST",
            format!("/sites/{site}/collections"),
            renamed.clone(),
        ),
        (
            "PUT",
            format!("/sites/{site}/collections/{collection}"),
            renamed,
        ),
        (
            "GET",
            format!("/sites/{site}/collections/{collection}/preview"),
            json!({}),
        ),
        (
            "DELETE",
            format!("/sites/{site}/collections/{collection}"),
            json!({}),
        ),
    ];
    for (method, uri, body) in foreign_attempts {
        let (status, answer) = send(
            &outsider.app,
            with_json(method, &uri, Some(&outsider.token), body),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}: {answer}");
        assert!(!answer.to_string().contains("Featured roasts"));
    }

    let (status, body) = delete(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/collections/{collection}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "disconnect failed: {body}");
    assert!(
        owner.acc.base(&base_node).await.unwrap().is_some(),
        "disconnect deleted the source Base"
    );
}

// ---- form submissions inbox ------------------------------------------------

#[tokio::test]
async fn analytics_answers_a_complete_period_and_validates_the_range() {
    let h = harness("sites-analytics").await;
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Measured site", "subdomain": sub("measured", &h) }),
        )
        .await,
    );

    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}/analytics?days=7")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["daily"].as_array().unwrap().len(), 7);
    assert_eq!(body["totals"]["visits"], json!(0));
    assert_eq!(body["totals"]["uniqueVisitors"], json!(0));
    assert_eq!(body["topPages"], json!([]));
    assert_eq!(body["topReferrers"], json!([]));
    // The second-generation dimensions answer as empty lists rather than
    // absent keys, so the interface can render its own empty states.
    for dimension in [
        "campaigns",
        "countries",
        "devices",
        "entryPages",
        "exitPages",
    ] {
        assert_eq!(body[dimension], json!([]), "{dimension}");
    }

    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}/analytics?days=0")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body["detail"],
        json!("analytics period must be between 1 and 365 days")
    );
}

#[tokio::test]
async fn custom_domain_claim_and_mocked_txt_verification_run_on_the_wire() {
    let h = harness("sites-domain").await;
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Domain site", "subdomain": sub("domain", &h) }),
        )
        .await,
    );
    let base = format!("/sites/{site}/domains");

    let (status, body) = post(
        &h.app,
        &h.token,
        &base,
        json!({ "domain": "https://wrong.example/path" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body["detail"].as_str().unwrap().contains("ASCII"));

    let host = format!("custom-{}.example.test", sub("host", &h));
    let (status, claim) = post(
        &h.app,
        &h.token,
        &base,
        json!({ "domain": host.to_ascii_uppercase() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claim}");
    assert_eq!(claim["domain"], json!(host));
    assert_eq!(claim["status"], json!("pending"));
    assert_eq!(claim["verifyRecord"]["type"], json!("TXT"));
    let record_name = claim["verifyRecord"]["name"].as_str().unwrap().to_owned();
    let record_value = claim["verifyRecord"]["value"].as_str().unwrap().to_owned();
    assert_eq!(record_name, format!("_alo-sites.{host}"));
    assert!(record_value.starts_with("alo-site-verification="));

    let (status, listed) = get(&h.app, &h.token, &base).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["domains"].as_array().unwrap().len(), 1);

    let verify_path = format!("{base}/{host}/verify");
    let missing_dns = app_with_dns(&h, HashMap::new());
    let (status, still_pending) = post(&missing_dns, &h.token, &verify_path, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{still_pending}");
    assert_eq!(still_pending["status"], json!("pending"));

    let matching_dns = app_with_dns(&h, HashMap::from([(record_name, vec![record_value])]));
    let (status, live) = post(&matching_dns, &h.token, &verify_path, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{live}");
    assert_eq!(live["status"], json!("live"));
    assert!(live["verifiedAt"].is_string());

    let (status, body) = delete(&h.app, &h.token, &format!("{base}/{host}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], json!("deleted"));
}

#[tokio::test]
async fn submissions_list_is_site_scoped_and_handled_is_one_write() {
    let h = harness("sites-submissions").await;
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Inbox site", "subdomain": sub("inbox", &h) }),
        )
        .await,
    );
    let site_id = SiteId::new(&site);
    let form = h
        .acc
        .create_site_form(&site_id, "Contact us")
        .await
        .unwrap();
    let older = h
        .acc
        .add_site_form_submission(&site_id, &form, "Ada", "ada@example.test", "First")
        .await
        .unwrap();
    let newer = h
        .acc
        .add_site_form_submission(&site_id, &form, "Grace", "grace@example.test", "Second")
        .await
        .unwrap();

    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}/submissions")).await;
    assert_eq!(status, StatusCode::OK);
    let rows = body["submissions"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], json!(newer.as_str()));
    assert_eq!(rows[0]["formName"], json!("Contact us"));
    assert_eq!(rows[0]["senderEmail"], json!("grace@example.test"));
    assert_eq!(rows[0]["handled"], json!(false));

    let (status, body) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/forms/{form}/submissions/{older}"),
        json!({ "handled": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (_, body) = get(&h.app, &h.token, &format!("/sites/{site}/submissions")).await;
    let handled = body["submissions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == older.as_str())
        .unwrap();
    assert_eq!(handled["handled"], json!(true));
}

#[tokio::test]
async fn submissions_export_is_safe_complete_and_downloadable() {
    let h = harness("sites-submissions-csv").await;
    let subdomain = sub("export", &h);
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Export site", "subdomain": subdomain }),
        )
        .await,
    );
    let site_id = SiteId::new(&site);
    let form = h
        .acc
        .create_site_form(&site_id, "=Risky, form")
        .await
        .unwrap();
    h.acc
        .add_site_form_submission(
            &site_id,
            &form,
            "+Visitor",
            "visitor@example.test",
            "Hello,\nplease call me",
        )
        .await
        .unwrap();

    let (status, headers, csv) =
        get_text(&h.app, &h.token, &format!("/sites/{site}/submissions.csv")).await;
    assert_eq!(status, StatusCode::OK, "{csv}");
    assert_eq!(
        headers
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("text/csv; charset=utf-8")
    );
    let expected_disposition = format!("attachment; filename=\"submissions-{subdomain}.csv\"");
    assert_eq!(
        headers
            .get("content-disposition")
            .and_then(|value| value.to_str().ok()),
        Some(expected_disposition.as_str()),
    );
    assert_eq!(
        headers
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(
        headers
            .get("x-content-type-options")
            .and_then(|value| value.to_str().ok()),
        Some("nosniff")
    );
    assert!(csv.starts_with("receivedAt,form,senderName,senderEmail,message,status\r\n"));
    assert!(csv.contains("'=Risky, form"));
    assert!(csv.contains("'+Visitor"));
    assert!(csv.contains("\"Hello,\nplease call me\""));
    assert!(csv.ends_with(",needs reply\r\n"));
}

// ---- the site arc ------------------------------------------------------------

#[tokio::test]
async fn site_lifecycle_create_check_edit_delete() {
    let h = harness("sites-arc").await;
    let claimed = sub("arc", &h);

    // Fresh namespace: the label is free, then claimed.
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/sites/subdomain-check?subdomain={claimed}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["available"], json!(true));

    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Harbour Roastery", "subdomain": claimed }),
        )
        .await,
    );

    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/sites/subdomain-check?subdomain={claimed}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["available"],
        json!(false),
        "claimed label still reads free"
    );

    // The list and the single read agree; a new site is a draft with no publish.
    let (status, body) = get(&h.app, &h.token, "/sites").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["sites"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == json!(site))
    );
    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], json!("Harbour Roastery"));
    assert_eq!(body["status"], json!("draft"));
    assert_eq!(body["publish"], Value::Null);

    // Rename and move to a new subdomain in one PUT.
    let moved = sub("arcm", &h);
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}"),
        json!({ "name": "Harbour Roastery bv", "subdomain": moved }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = get(&h.app, &h.token, &format!("/sites/{site}")).await;
    assert_eq!(body["name"], json!("Harbour Roastery bv"));
    assert_eq!(body["subdomain"], json!(moved));

    // An empty PUT is a refusal, not a silent no-op.
    let (status, _) = put(&h.app, &h.token, &format!("/sites/{site}"), json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Delete releases the claim.
    let (status, _) = delete(&h.app, &h.token, &format!("/sites/{site}")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}")).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "deleted site still resolves: {body}"
    );
    let (_, body) = get(
        &h.app,
        &h.token,
        &format!("/sites/subdomain-check?subdomain={moved}"),
    )
    .await;
    assert_eq!(
        body["available"],
        json!(true),
        "delete did not release the subdomain"
    );
}

#[tokio::test]
async fn site_languages_are_canonical_validated_and_tenant_scoped() {
    let owner = harness("sites-locales").await;
    let outsider = harness_on(Arc::clone(&owner.store), "sites-locales-other").await;
    let site = created_id(
        "localized site",
        post(
            &owner.app,
            &owner.token,
            "/sites",
            json!({
                "name": "European journal",
                "subdomain": sub("locales", &owner),
                "defaultLocale": "PT-br",
                "enabledLocales": ["PT-BR", "en", "nl"]
            }),
        )
        .await,
    );

    let (status, created) = get(&owner.app, &owner.token, &format!("/sites/{site}")).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["defaultLocale"], json!("pt-br"));
    assert_eq!(created["enabledLocales"], json!(["pt-br", "en", "nl"]));

    let (status, changed) = put(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}"),
        json!({ "defaultLocale": "fr", "enabledLocales": ["fr", "de", "EN-gb"] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    let (_, changed) = get(&owner.app, &owner.token, &format!("/sites/{site}")).await;
    assert_eq!(changed["defaultLocale"], json!("fr"));
    assert_eq!(changed["enabledLocales"], json!(["fr", "de", "en-gb"]));

    let (status, invalid) = put(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}"),
        json!({ "defaultLocale": "it", "enabledLocales": ["fr", "de"] }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{invalid}");
    assert_eq!(
        invalid["detail"],
        json!("default language 'it' must also be enabled")
    );

    let (status, hidden) = put(
        &outsider.app,
        &outsider.token,
        &format!("/sites/{site}"),
        json!({ "defaultLocale": "nl", "enabledLocales": ["nl"] }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{hidden}");
    let (_, unchanged) = get(&owner.app, &owner.token, &format!("/sites/{site}")).await;
    assert_eq!(unchanged["defaultLocale"], json!("fr"));
    assert_eq!(unchanged["enabledLocales"], json!(["fr", "de", "en-gb"]));
}

// ---- pages -------------------------------------------------------------------

#[tokio::test]
async fn page_lifecycle_slug_seo_home_and_order() {
    let h = harness("sites-pages").await;
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Pages", "subdomain": sub("pg", &h) }),
        )
        .await,
    );
    let base = format!("/sites/{site}/pages");

    // Home page at the empty slug; a second page with a real slug.
    let home = created_id(
        "home page",
        post(
            &h.app,
            &h.token,
            &base,
            json!({ "title": "Home", "home": true }),
        )
        .await,
    );
    let about = created_id(
        "about page",
        post(
            &h.app,
            &h.token,
            &base,
            json!({ "title": "About", "slug": "about" }),
        )
        .await,
    );

    let (status, body) = get(&h.app, &h.token, &base).await;
    assert_eq!(status, StatusCode::OK);
    let pages = body["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 2);
    assert_eq!(
        pages[0]["id"],
        json!(home),
        "home was created first, leads the nav"
    );
    assert_eq!(pages[0]["home"], json!(true));
    assert!(pages[0].get("sections").is_none(), "the list stays lean");

    // Title, slug, and SEO in one PUT; then a partial SEO update must keep
    // the other field, and a blank must clear it.
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("{base}/{about}"),
        json!({ "title": "Our story", "slug": "story",
                "seoTitle": "Our story — Pages", "seoDescription": "How it began." }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("{base}/{about}"),
        json!({ "seoTitle": "The story" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = get(&h.app, &h.token, &format!("{base}/{about}")).await;
    assert_eq!(body["slug"], json!("story"));
    assert_eq!(body["seoTitle"], json!("The story"));
    assert_eq!(
        body["seoDescription"],
        json!("How it began."),
        "partial SEO update dropped the description"
    );
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("{base}/{about}"),
        json!({ "seoDescription": "" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = get(&h.app, &h.token, &format!("{base}/{about}")).await;
    assert_eq!(
        body["seoDescription"],
        Value::Null,
        "blank did not clear the override"
    );

    // Reorder is the full permutation; the list follows it.
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("{base}/order"),
        json!({ "order": [about, home] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = get(&h.app, &h.token, &base).await;
    assert_eq!(body["pages"][0]["id"], json!(about));

    // Home moves: give the old home a slug first, then promote the other.
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("{base}/{home}"),
        json!({ "slug": "welcome" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = post(&h.app, &h.token, &format!("{base}/{about}/home"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "promote failed: {body}");
    let (_, body) = get(&h.app, &h.token, &format!("{base}/{about}")).await;
    assert_eq!(body["home"], json!(true));

    let (status, _) = delete(&h.app, &h.token, &format!("{base}/{home}")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = get(&h.app, &h.token, &format!("{base}/{home}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn localized_page_drafts_resolve_fallback_and_hide_other_tenants() {
    let owner = harness("sites-page-locales").await;
    let outsider = harness_on(Arc::clone(&owner.store), "sites-page-locales-other").await;
    let site = created_id(
        "localized site",
        post(
            &owner.app,
            &owner.token,
            "/sites",
            json!({
                "name": "European journal",
                "subdomain": sub("page-locales", &owner),
                "defaultLocale": "en",
                "enabledLocales": ["en", "fr", "nl"]
            }),
        )
        .await,
    );
    let page = created_id(
        "localized page",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/pages"),
            json!({ "title": "About", "slug": "about" }),
        )
        .await,
    );
    let french_uri = format!("/sites/{site}/pages/{page}/locales/fr");

    let (status, fallback) = get(&owner.app, &owner.token, &french_uri).await;
    assert_eq!(status, StatusCode::OK, "{fallback}");
    assert_eq!(fallback["id"], json!(page));
    assert_eq!(fallback["title"], json!("About"));
    assert_eq!(fallback["requestedLocale"], json!("fr"));
    assert_eq!(fallback["resolvedLocale"], json!("en"));
    assert_eq!(fallback["fallback"], json!(true));

    let french_sections = json!({
        "schema_version": 1,
        "sections": [{"type": "hero", "heading": "Notre histoire"}]
    });
    let (status, localized) = put(
        &owner.app,
        &owner.token,
        &french_uri,
        json!({
            "title": "Notre histoire",
            "slug": "notre-histoire",
            "sections": french_sections,
            "seoTitle": "À propos",
            "seoDescription": "Notre équipe."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{localized}");
    assert_eq!(localized["id"], json!(page));
    assert_eq!(localized["slug"], json!("notre-histoire"));
    assert_eq!(localized["resolvedLocale"], json!("fr"));
    assert_eq!(localized["fallback"], json!(false));
    assert_eq!(
        localized["sections"]["sections"][0]["heading"],
        json!("Notre histoire")
    );

    let french_preview_uri = format!("{french_uri}/preview");
    let (status, headers, preview) = get_text(&owner.app, &owner.token, &french_preview_uri).await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(
        headers
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    assert!(preview.contains("<html lang=\"fr\">"), "{preview}");
    assert!(preview.contains("Notre histoire"), "{preview}");
    assert!(preview.contains("/fr/notre-histoire"), "{preview}");

    let readiness_uri = format!("/sites/{site}/translation-readiness");
    let (status, readiness) = get(&owner.app, &owner.token, &readiness_uri).await;
    assert_eq!(status, StatusCode::OK, "{readiness}");
    assert_eq!(readiness["defaultLocale"], json!("en"));
    assert_eq!(readiness["totalPages"], json!(1));
    assert_eq!(readiness["languages"][0]["translatedPages"], json!(1));
    assert_eq!(readiness["languages"][1]["translatedPages"], json!(1));
    assert_eq!(readiness["languages"][2]["translatedPages"], json!(0));
    assert_eq!(readiness["languages"][2]["ready"], json!(false));

    let (status, disabled) = get(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/pages/{page}/locales/de"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{disabled}");
    assert!(disabled["detail"].as_str().unwrap().contains("not enabled"));

    let (status, hidden) = get(&outsider.app, &outsider.token, &french_uri).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{hidden}");
    let (status, hidden) = get(&outsider.app, &outsider.token, &readiness_uri).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{hidden}");
    let (status, _, hidden_preview) =
        get_text(&outsider.app, &outsider.token, &french_preview_uri).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{hidden_preview}");
    let (status, hidden) = put(
        &outsider.app,
        &outsider.token,
        &french_uri,
        json!({
            "title": "Defaced",
            "slug": "defaced",
            "sections": {"schema_version": 1, "sections": []}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{hidden}");
    let (_, unchanged) = get(&owner.app, &owner.token, &french_uri).await;
    assert_eq!(unchanged["title"], json!("Notre histoire"));
}

// ---- sections ----------------------------------------------------------------

#[tokio::test]
async fn section_ops_add_update_move_remove_and_full_set() {
    let h = harness("sites-sections").await;
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Sections", "subdomain": sub("sc", &h) }),
        )
        .await,
    );
    let page = created_id(
        "page",
        post(
            &h.app,
            &h.token,
            &format!("/sites/{site}/pages"),
            json!({ "title": "Home", "home": true }),
        )
        .await,
    );
    let sections = format!("/sites/{site}/pages/{page}/sections");

    // Append hero, append faq, insert cta between them.
    let (status, body) = post(&h.app, &h.token, &sections, json!({ "section": hero() })).await;
    assert_eq!(status, StatusCode::OK, "add hero: {body}");
    let (status, _) = post(&h.app, &h.token, &sections, json!({ "section": faq() })).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = post(
        &h.app,
        &h.token,
        &sections,
        json!({ "section": cta(), "index": 1 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let kinds: Vec<&str> = body["sections"]["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["hero", "cta", "faq"]);

    // Update in place; the response is the canonical stored envelope.
    let (status, body) = put(
        &h.app,
        &h.token,
        &format!("{sections}/1"),
        json!({ "section": { "type": "cta", "heading": "New heading",
                             "button": { "label": "Go", "href": "/go" } } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["sections"]["sections"][1]["heading"],
        json!("New heading")
    );

    // Move the faq to the front; then remove the hero (now index 1).
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("{sections}/2/move"),
        json!({ "to": 0 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["sections"]["sections"][0]["type"], json!("faq"));
    let (status, body) = delete(&h.app, &h.token, &format!("{sections}/1")).await;
    assert_eq!(status, StatusCode::OK);
    let kinds: Vec<&str> = body["sections"]["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["faq", "cta"]);

    // The page read returns what the ops built.
    let (_, body) = get(&h.app, &h.token, &format!("/sites/{site}/pages/{page}")).await;
    assert_eq!(body["sections"]["sections"].as_array().unwrap().len(), 2);

    // The atomic full set replaces the stack wholesale.
    let (status, body) = put(
        &h.app,
        &h.token,
        &sections,
        json!({ "schema_version": 1, "sections": [hero()] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["sections"]["sections"].as_array().unwrap().len(), 1);

    // Out-of-range and malformed indexes are named refusals.
    let (status, _) = delete(&h.app, &h.token, &format!("{sections}/7")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = post(
        &h.app,
        &h.token,
        &format!("{sections}/one/move"),
        json!({ "to": 0 }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn adding_a_contact_section_creates_and_links_its_form() {
    let owner = harness("sites-contact-section").await;
    let outsider = harness("sites-contact-outsider").await;
    let site = created_id(
        "site",
        post(
            &owner.app,
            &owner.token,
            "/sites",
            json!({ "name": "Contact Co", "subdomain": sub("cf", &owner) }),
        )
        .await,
    );
    let page = created_id(
        "page",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/pages"),
            json!({ "title": "Home", "home": true }),
        )
        .await,
    );
    let sections = format!("/sites/{site}/pages/{page}/sections");

    let (status, body) = post(
        &owner.app,
        &owner.token,
        &sections,
        json!({ "section": {
            "type": "contact_form",
            "heading": "Talk to our team",
            "body": "We reply within one working day."
        }}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "add contact section: {body}");
    let linked = body["sections"]["sections"][0]["form_id"]
        .as_str()
        .expect("server-linked form id")
        .to_owned();
    let stored = owner.acc.site_forms(&SiteId::new(&site)).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].id.as_str(), linked);
    assert_eq!(stored[0].name, "Talk to our team");

    // Section headings have a wider content cap than owner-facing form
    // names. A valid long Unicode heading is shortened by characters and
    // still produces a working linked form.
    let long_heading = "é".repeat(120);
    let (status, body) = post(
        &owner.app,
        &owner.token,
        &sections,
        json!({ "section": {
            "type": "contact_form",
            "heading": long_heading
        }}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "add long-heading form: {body}");
    let stored = owner.acc.site_forms(&SiteId::new(&site)).await.unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[1].name.chars().count(), 100);
    assert!(stored[1].name.chars().all(|character| character == 'é'));

    // A form id is never a cross-tenant capability: even if an outsider's
    // valid id is supplied, the owner's page refuses it and keeps its stack.
    let foreign_site = outsider
        .acc
        .create_site("Outside", &sub("cfo", &outsider))
        .await
        .unwrap();
    let foreign_form = outsider
        .acc
        .create_site_form(&foreign_site, "Outside form")
        .await
        .unwrap();
    let (status, _) = post(
        &owner.app,
        &owner.token,
        &sections,
        json!({ "section": {
            "type": "contact_form",
            "form_id": foreign_form.as_str()
        }}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, body) = get(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/pages/{page}"),
    )
    .await;
    assert_eq!(body["sections"]["sections"].as_array().unwrap().len(), 2);
    assert_eq!(body["sections"]["sections"][0]["form_id"], json!(linked));
}

// ---- publish + theme ---------------------------------------------------------

#[tokio::test]
async fn theme_gate_and_publish_flow() {
    let h = harness("sites-publish").await;
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Live", "subdomain": sub("pub", &h) }),
        )
        .await,
    );

    // Publish preconditions: no pages, then no home page → named 422s.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site}/publish"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body["detail"].as_str().unwrap().contains("no pages"),
        "{body}"
    );
    created_id(
        "page",
        post(
            &h.app,
            &h.token,
            &format!("/sites/{site}/pages"),
            json!({ "title": "Loose", "slug": "loose" }),
        )
        .await,
    );
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site}/publish"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["detail"].as_str().unwrap().contains("home"), "{body}");

    // Theme: an off-schema envelope is refused, a preset lands.
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/theme"),
        json!({ "schema_version": 1, "preset": "no-such-preset" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, body) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/theme"),
        json!({ "schema_version": 1, "preset": "terra" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "theme set: {body}");
    let (_, body) = get(&h.app, &h.token, &format!("/sites/{site}")).await;
    assert_eq!(body["theme"]["preset"], json!("terra"));

    // With a home page the publish goes through and the site reads live.
    created_id(
        "home",
        post(
            &h.app,
            &h.token,
            &format!("/sites/{site}/pages"),
            json!({ "title": "Home", "home": true }),
        )
        .await,
    );
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site}/publish"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "publish: {body}");
    let publish_id = body["publishId"].as_str().unwrap().to_owned();
    assert_eq!(body["status"], json!("live"));
    let (_, body) = get(&h.app, &h.token, &format!("/sites/{site}")).await;
    assert_eq!(body["status"], json!("live"));
    assert_eq!(body["publish"]["id"], json!(publish_id));

    // Unpublish flips back to draft; idempotent.
    for _ in 0..2 {
        let (status, body) = post(
            &h.app,
            &h.token,
            &format!("/sites/{site}/unpublish"),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], json!("draft"));
    }
    let (_, body) = get(&h.app, &h.token, &format!("/sites/{site}")).await;
    assert_eq!(body["status"], json!("draft"));
    assert_eq!(body["publish"], Value::Null);
}

// ---- validation --------------------------------------------------------------

#[tokio::test]
async fn rule_violations_answer_422_with_the_rule() {
    let h = harness("sites-422").await;

    // Subdomain rules on create and on the live check.
    for bad in ["ab", "-edge-", "UPPER", "www"] {
        let (status, body) = post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "X", "subdomain": bad }),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "create with {bad:?}: {body}"
        );
        assert!(
            body["detail"].is_string(),
            "no rule named for {bad:?}: {body}"
        );
    }
    let (status, _) = get(&h.app, &h.token, "/sites/subdomain-check?subdomain=www").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // A taken subdomain answers taken/free only.
    let claimed = sub("tk", &h);
    created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "First", "subdomain": claimed }),
        )
        .await,
    );
    let (status, body) = post(
        &h.app,
        &h.token,
        "/sites",
        json!({ "name": "Second", "subdomain": claimed }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["detail"], json!("subdomain is already taken"));

    // Page slug rules: reserved public path, then a duplicate.
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Rules", "subdomain": sub("rl", &h) }),
        )
        .await,
    );
    let pages = format!("/sites/{site}/pages");
    let (status, _) = post(
        &h.app,
        &h.token,
        &pages,
        json!({ "title": "Blog", "slug": "blog" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "reserved slug accepted"
    );
    created_id(
        "page",
        post(
            &h.app,
            &h.token,
            &pages,
            json!({ "title": "A", "slug": "a" }),
        )
        .await,
    );
    let (status, _) = post(
        &h.app,
        &h.token,
        &pages,
        json!({ "title": "A2", "slug": "a" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "duplicate slug accepted"
    );

    // Sections: an unknown type and a content-rule violation both name the
    // refusal; malformed JSON is the 400 family, not a 500.
    let page = created_id(
        "page",
        post(
            &h.app,
            &h.token,
            &pages,
            json!({ "title": "Home", "home": true }),
        )
        .await,
    );
    let sections = format!("/sites/{site}/pages/{page}/sections");
    let (status, _) = post(
        &h.app,
        &h.token,
        &sections,
        json!({ "section": { "type": "carousel", "heading": "nope" } }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "unknown section type accepted"
    );
    let (status, body) = post(
        &h.app,
        &h.token,
        &sections,
        json!({ "section": { "type": "hero", "heading": "x",
                             "primary_cta": { "label": "x", "href": "javascript:alert(1)" } } }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "unsafe href accepted: {body}"
    );
    let req = Request::builder()
        .method("PUT")
        .uri(&sections)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", h.token))
        .body(Body::from("{not json"))
        .unwrap();
    let (status, _) = send(&h.app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ---- blog posts ------------------------------------------------------------

#[tokio::test]
async fn blog_post_routes_keep_the_body_in_drive_and_metadata_on_the_site() {
    let h = harness("sites-posts").await;
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Journal", "subdomain": sub("journal", &h) }),
        )
        .await,
    );
    let doc = h
        .acc
        .drive_create_file(
            &DriveLocation::Personal,
            None,
            &NewDriveFile {
                name: "Opening story".to_owned(),
                blob_id: "http-post-doc".to_owned(),
                kind: Some("doc".to_owned()),
                ..NewDriveFile::default()
            },
        )
        .await
        .unwrap();
    let base = format!("/sites/{site}/posts");
    let post_id = created_id(
        "post",
        post(
            &h.app,
            &h.token,
            &base,
            json!({
                "docNodeId": doc.as_str(),
                "slug": "opening-story",
                "title": "Opening story",
                "excerpt": "The first chapter"
            }),
        )
        .await,
    );

    let (status, body) = get(&h.app, &h.token, &base).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["posts"][0]["id"], json!(post_id));
    assert_eq!(body["posts"][0]["status"], json!("draft"));
    assert_eq!(body["posts"][0]["docNodeId"], json!(doc.as_str()));

    let item = format!("{base}/{post_id}");
    let (status, body) = put(
        &h.app,
        &h.token,
        &item,
        json!({
            "slug": "opening-notes",
            "title": "Opening notes",
            "excerpt": "Revised"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "update failed: {body}");
    let (status, body) = post(&h.app, &h.token, &format!("{item}/publish"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "publish failed: {body}");
    let (_, body) = get(&h.app, &h.token, &item).await;
    assert_eq!(body["title"], json!("Opening notes"));
    assert_eq!(body["status"], json!("published"));
    assert!(body["publishedAt"].is_string());

    let (status, body) = post(&h.app, &h.token, &format!("{item}/unpublish"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "unpublish failed: {body}");
    let (status, body) = delete(&h.app, &h.token, &item).await;
    assert_eq!(status, StatusCode::OK, "delete failed: {body}");
    assert!(h.acc.drive_node(&doc).await.unwrap().is_some());

    let (status, body) = post(
        &h.app,
        &h.token,
        &base,
        json!({ "docNodeId": "missing", "slug": "missing", "title": "Missing" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "missing doc: {body}");
}

#[tokio::test]
async fn final_blog_domain_and_privacy_arc_uses_a_real_drive_document() {
    let (owner, blobs) = harness_with_blobs("sites-final-blog").await;
    let outsider = harness_on(Arc::clone(&owner.store), "sites-final-blog-other").await;
    let site = created_id(
        "site",
        post(
            &owner.app,
            &owner.token,
            "/sites",
            json!({ "name": "Field journal", "subdomain": sub("field-journal", &owner) }),
        )
        .await,
    );
    created_id(
        "home page",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/pages"),
            json!({ "title": "Home", "home": true }),
        )
        .await,
    );
    let (status, published) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/publish"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "site publish failed: {published}");

    // Create the article exactly as Docs does: upload BlockNote JSON, then
    // create a Drive node whose current blob is that document.
    let document =
        include_str!("../../../sites/alo-sites/tests/fixtures/blocknote/core_document.json");
    let upload_request = Request::builder()
        .method("POST")
        .uri(format!("/jmap/upload/{}", owner.account_id))
        .header("authorization", format!("Bearer {}", owner.token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(document))
        .unwrap();
    let (status, uploaded) = send(&owner.app, upload_request).await;
    assert_eq!(status, StatusCode::OK, "document upload failed: {uploaded}");
    let blob_id = uploaded["blobId"].as_str().unwrap();
    let doc_node = created_id(
        "document",
        post(
            &owner.app,
            &owner.token,
            "/drive/files",
            json!({
                "space": null,
                "parent": null,
                "name": "A field guide to Utrecht mornings",
                "blobId": blob_id,
                "size": document.len(),
                "contentType": "application/json",
                "kind": "doc"
            }),
        )
        .await,
    );
    let post_base = format!("/sites/{site}/posts");
    let article = created_id(
        "post",
        post(
            &owner.app,
            &owner.token,
            &post_base,
            json!({
                "docNodeId": doc_node,
                "slug": "utrecht-mornings",
                "title": "A field guide to Utrecht mornings",
                "excerpt": "Notes from before the city wakes"
            }),
        )
        .await,
    );
    let (status, article_state) = post(
        &owner.app,
        &owner.token,
        &format!("{post_base}/{article}/publish"),
        json!({}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "post publish failed: {article_state}"
    );

    // Claim and verify a customer address through the same DNS seam used in
    // production. A live site promotes a successful proof directly to live.
    let custom_host = format!("journal-{}.example.test", sub("host", &owner));
    let domain_base = format!("/sites/{site}/domains");
    let (status, claim) = post(
        &owner.app,
        &owner.token,
        &domain_base,
        json!({ "domain": custom_host }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "domain claim failed: {claim}");
    let record_name = claim["verifyRecord"]["name"].as_str().unwrap().to_owned();
    let record_value = claim["verifyRecord"]["value"].as_str().unwrap().to_owned();
    let dns_app = app_with_dns(&owner, HashMap::from([(record_name, vec![record_value])]));
    let (status, verified) = post(
        &dns_app,
        &owner.token,
        &format!("{domain_base}/{custom_host}/verify"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "domain verify failed: {verified}");
    assert_eq!(verified["status"], json!("live"));

    // The anonymous service shares the real blob backend. One custom-Host
    // article request renders the BlockNote body and records only safe,
    // reduced analytics dimensions.
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url())
        .await
        .unwrap();
    let public = PublicAppState::new(
        SitePublicStore::new(pool.clone(), blobs),
        "sites.test".to_owned(),
        b"sites-final-blog-analytics-secret",
    );
    let sensitive_ip = "203.0.113.132";
    let sensitive_agent = "PrivateBrowser/tenant-secret";
    let sensitive_referrer =
        "https://NEWS.Example/private/customer?token=must-not-be-stored#account";
    let (status, html) = public_text(
        &public,
        Request::builder()
            .uri("/blog/utrecht-mornings?campaign=private-token")
            .header(header::HOST, &custom_host)
            .header("x-forwarded-for", sensitive_ip)
            .header(header::USER_AGENT, sensitive_agent)
            .header(header::REFERER, sensitive_referrer)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "custom Host did not serve: {html}");
    assert!(html.contains("A field guide to Utrecht mornings"));
    assert!(html.contains("Great work is <strong><em>clear &amp; deliberate</em></strong>"));

    let (status, report) = get(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/analytics?days=7"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "analytics failed: {report}");
    assert_eq!(report["totals"]["visits"], json!(1));
    assert_eq!(report["totals"]["uniqueVisitors"], json!(1));
    assert_eq!(
        report["topPages"][0]["path"],
        json!("/blog/utrecht-mornings")
    );
    assert_eq!(report["topReferrers"][0]["domain"], json!("news.example"));
    let report_text = report.to_string();
    assert!(!report_text.contains(sensitive_ip));
    assert!(!report_text.contains(sensitive_agent));
    assert!(!report_text.contains("must-not-be-stored"));
    let (status, _) = get(
        &outsider.app,
        &outsider.token,
        &format!("/sites/{site}/analytics?days=7"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let stored: Vec<(String, String)> =
        sqlx::query_as("SELECT path, referrer_domain FROM site_analytics_daily WHERE site_id = $1")
            .bind(&site)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        stored,
        vec![(
            "/blog/utrecht-mornings".to_owned(),
            "news.example".to_owned()
        )]
    );
    let hashes: Vec<Vec<u8>> = sqlx::query_scalar(
        "SELECT visitor_hash FROM site_analytics_daily_visitors WHERE site_id = $1",
    )
    .bind(&site)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(hashes.len(), 1);
    assert_eq!(hashes[0].len(), 32);
    assert_ne!(hashes[0], sensitive_ip.as_bytes());
    let unsafe_columns: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns \
         WHERE table_schema = 'public' AND table_name LIKE 'site_analytics%' \
           AND column_name IN ('ip', 'ip_address', 'user_agent', 'query_string', \
                               'referrer_url', 'raw_referrer')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(unsafe_columns, 0, "analytics schema has a raw-PII column");
}

// ---- the wrong-tenant test (mandatory: CLAUDE.md law 1) ----------------------

#[tokio::test]
async fn another_tenants_site_is_invisible_on_every_route() {
    let a = harness("sites-tenant-a").await;
    let b = harness("sites-tenant-b").await;

    // Tenant B's site and page, built through B's own door.
    let b_site = created_id(
        "site",
        post(
            &b.app,
            &b.token,
            "/sites",
            json!({ "name": "B Marketing", "subdomain": sub("iso", &b) }),
        )
        .await,
    );
    let b_page = created_id(
        "page",
        post(
            &b.app,
            &b.token,
            &format!("/sites/{b_site}/pages"),
            json!({ "title": "B Home", "home": true }),
        )
        .await,
    );
    let b_doc = b
        .acc
        .drive_create_file(
            &DriveLocation::Personal,
            None,
            &NewDriveFile {
                name: "B private post".to_owned(),
                blob_id: "tenant-b-post-doc".to_owned(),
                kind: Some("doc".to_owned()),
                ..NewDriveFile::default()
            },
        )
        .await
        .unwrap();
    let b_post = created_id(
        "post",
        post(
            &b.app,
            &b.token,
            &format!("/sites/{b_site}/posts"),
            json!({
                "docNodeId": b_doc.as_str(),
                "slug": "private-post",
                "title": "B private post"
            }),
        )
        .await,
    );
    let b_site_id = SiteId::new(&b_site);
    let b_form = b
        .acc
        .create_site_form(&b_site_id, "Private form")
        .await
        .unwrap();
    let b_submission = b
        .acc
        .add_site_form_submission(
            &b_site_id,
            &b_form,
            "Private sender",
            "private@example.test",
            "Tenant B only",
        )
        .await
        .unwrap();
    let b_domain = format!("private-{}.example.test", sub("domain", &b));
    let (status, body) = post(
        &b.app,
        &b.token,
        &format!("/sites/{b_site}/domains"),
        json!({ "domain": b_domain }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "B domain claim failed: {body}");

    // A's list never mentions it.
    let (status, body) = get(&a.app, &a.token, "/sites").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.to_string().contains("B Marketing"),
        "list leaked: {body}"
    );

    // Every verb on B's ids answers A with the same 404 an invented id gets.
    let attempts: Vec<(&str, String, Value)> = vec![
        ("GET", format!("/sites/{b_site}"), json!({})),
        (
            "PUT",
            format!("/sites/{b_site}"),
            json!({ "name": "Taken Over" }),
        ),
        ("DELETE", format!("/sites/{b_site}"), json!({})),
        (
            "PUT",
            format!("/sites/{b_site}/theme"),
            json!({ "schema_version": 1, "preset": "terra" }),
        ),
        ("POST", format!("/sites/{b_site}/publish"), json!({})),
        ("POST", format!("/sites/{b_site}/unpublish"), json!({})),
        ("GET", format!("/sites/{b_site}/pages"), json!({})),
        (
            "GET",
            format!("/sites/{b_site}/translation-readiness"),
            json!({}),
        ),
        ("GET", format!("/sites/{b_site}/analytics"), json!({})),
        ("GET", format!("/sites/{b_site}/domains"), json!({})),
        (
            "POST",
            format!("/sites/{b_site}/domains"),
            json!({ "domain": "foreign-write.example.test" }),
        ),
        (
            "POST",
            format!("/sites/{b_site}/domains/{b_domain}/verify"),
            json!({}),
        ),
        (
            "DELETE",
            format!("/sites/{b_site}/domains/{b_domain}"),
            json!({}),
        ),
        ("GET", format!("/sites/{b_site}/submissions"), json!({})),
        ("GET", format!("/sites/{b_site}/submissions.csv"), json!({})),
        ("GET", format!("/sites/{b_site}/posts"), json!({})),
        (
            "POST",
            format!("/sites/{b_site}/posts"),
            json!({ "docNodeId": b_doc.as_str(), "slug": "injected", "title": "Injected" }),
        ),
        ("GET", format!("/sites/{b_site}/posts/{b_post}"), json!({})),
        (
            "PUT",
            format!("/sites/{b_site}/posts/{b_post}"),
            json!({ "slug": "defaced", "title": "Defaced" }),
        ),
        (
            "POST",
            format!("/sites/{b_site}/posts/{b_post}/publish"),
            json!({}),
        ),
        (
            "POST",
            format!("/sites/{b_site}/posts/{b_post}/unpublish"),
            json!({}),
        ),
        (
            "DELETE",
            format!("/sites/{b_site}/posts/{b_post}"),
            json!({}),
        ),
        (
            "PUT",
            format!("/sites/{b_site}/forms/{b_form}/submissions/{b_submission}"),
            json!({ "handled": true }),
        ),
        (
            "POST",
            format!("/sites/{b_site}/pages"),
            json!({ "title": "Injected", "slug": "injected" }),
        ),
        (
            "PUT",
            format!("/sites/{b_site}/pages/order"),
            json!({ "order": [b_page.clone()] }),
        ),
        ("GET", format!("/sites/{b_site}/pages/{b_page}"), json!({})),
        (
            "GET",
            format!("/sites/{b_site}/pages/{b_page}/preview"),
            json!({}),
        ),
        (
            "GET",
            format!("/sites/{b_site}/pages/{b_page}/locales/en/preview"),
            json!({}),
        ),
        (
            "PUT",
            format!("/sites/{b_site}/pages/{b_page}"),
            json!({ "title": "Defaced" }),
        ),
        (
            "DELETE",
            format!("/sites/{b_site}/pages/{b_page}"),
            json!({}),
        ),
        (
            "POST",
            format!("/sites/{b_site}/pages/{b_page}/home"),
            json!({}),
        ),
        (
            "PUT",
            format!("/sites/{b_site}/pages/{b_page}/sections"),
            json!({ "schema_version": 1, "sections": [] }),
        ),
        (
            "POST",
            format!("/sites/{b_site}/pages/{b_page}/sections"),
            json!({ "section": hero() }),
        ),
        (
            "PUT",
            format!("/sites/{b_site}/pages/{b_page}/sections/0"),
            json!({ "section": hero() }),
        ),
        (
            "POST",
            format!("/sites/{b_site}/pages/{b_page}/sections/0/move"),
            json!({ "to": 0 }),
        ),
        (
            "DELETE",
            format!("/sites/{b_site}/pages/{b_page}/sections/0"),
            json!({}),
        ),
    ];
    for (method, uri, body) in attempts {
        let (status, answer) = send(&a.app, with_json(method, &uri, Some(&a.token), body)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri} → {answer}");
        assert!(
            !answer.to_string().contains("B Marketing") && !answer.to_string().contains("B Home"),
            "{method} {uri} leaked the record it refused: {answer}"
        );
    }

    // B's site is untouched by the barrage.
    let (status, body) = get(&b.app, &b.token, &format!("/sites/{b_site}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], json!("B Marketing"));
    let (_, body) = get(&b.app, &b.token, &format!("/sites/{b_site}/pages/{b_page}")).await;
    assert_eq!(body["title"], json!("B Home"));
    let (_, body) = get(&b.app, &b.token, &format!("/sites/{b_site}/posts/{b_post}")).await;
    assert_eq!(body["title"], json!("B private post"));
    assert_eq!(body["status"], json!("draft"));
    let forms = b.acc.site_forms(&b_site_id).await.unwrap();
    assert_eq!(
        forms.len(),
        1,
        "a foreign add-section request created a form"
    );
    let submissions = b
        .acc
        .site_form_submissions(&b_site_id, &b_form)
        .await
        .unwrap();
    assert_eq!(submissions.len(), 1);
    assert!(!submissions[0].handled, "foreign tenant marked it handled");
    let domains = b.acc.site_domains(&b_site_id).await.unwrap();
    assert_eq!(domains.len(), 1, "foreign tenant changed B's domain claim");
    assert_eq!(domains[0].domain, b_domain);
}

// ---- the draft preview (S1.13) -----------------------------------------------

/// GETs a route that answers a raw document rather than JSON.
async fn get_text(
    app: &Router,
    token: &str,
    uri: &str,
) -> (StatusCode, axum::http::HeaderMap, String) {
    use tower::ServiceExt;
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

/// The preview answers the DRAFT as one self-contained HTML document: the
/// same renderer as public serving with the stylesheet inlined, following
/// every edit immediately — no publish involved.
#[tokio::test]
async fn preview_renders_the_draft_as_a_self_contained_document() {
    let h = harness("sites-preview").await;
    let claimed = sub("pv", &h);
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Preview Roastery", "subdomain": claimed }),
        )
        .await,
    );
    let page = created_id(
        "page",
        post(
            &h.app,
            &h.token,
            &format!("/sites/{site}/pages"),
            json!({ "title": "Home", "home": true }),
        )
        .await,
    );
    let (status, _) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/{page}/sections"),
        json!({ "section": hero() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let uri = format!("/sites/{site}/pages/{page}/preview");
    let (status, headers, html) = get_text(&h.app, &h.token, &uri).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    // A draft has no cache life.
    assert_eq!(headers.get("cache-control").unwrap(), "no-store");
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("Coffee roasted the morning it ships"));
    // Self-contained: the stylesheet is inlined; the public asset path (which
    // does not resolve on this origin) is referenced nowhere.
    assert!(html.contains("<style>"));
    assert!(!html.contains("/assets/site.css"));
    // Canonical/OG advertise the site's future public origin.
    assert!(html.contains(&format!("https://{claimed}.alosites.com/")));

    // The preview follows the draft: an edit shows on the next fetch.
    let mut edited = hero();
    edited["heading"] = json!("Now even fresher");
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/{page}/sections/0"),
        json!({ "section": edited }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, _, html) = get_text(&h.app, &h.token, &uri).await;
    assert!(html.contains("Now even fresher"));
    assert!(!html.contains("Coffee roasted the morning it ships"));
}

// ---- the editor's own image source (S2.07c) ---------------------------------

/// GETs a route that answers bytes rather than text.
async fn get_bytes(
    app: &Router,
    token: &str,
    uri: &str,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    (status, headers, bytes.to_vec())
}

/// Uploads bytes through the JMAP upload endpoint and returns the blob id —
/// the same door the editor's image picker uses.
async fn upload(h: &Harness, content_type: &str, bytes: Vec<u8>) -> String {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/jmap/upload/{}", h.account_id))
        .header("authorization", format!("Bearer {}", h.token))
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(bytes))
        .unwrap();
    let (status, body) = send(&h.app, request).await;
    assert_eq!(status, StatusCode::OK, "upload failed: {body}");
    body["blobId"].as_str().expect("blob id").to_owned()
}

/// The framing control needs the SOURCE pixels, not the rendered preview's
/// `data:` URIs — so the edit surface serves one image blob at a time, and
/// serves it only to the tenant that owns it. Everything that does not
/// resolve in the caller's tenant is the same `404`: another tenant's blob,
/// a blob that is not an image, and an id that never existed.
#[tokio::test]
async fn the_editor_reads_its_own_image_blobs_and_no_other_tenants() {
    let owner = harness("sites-image-owner").await;
    let outsider = harness_on(Arc::clone(&owner.store), "sites-image-other").await;

    let site = created_id(
        "site",
        post(
            &owner.app,
            &owner.token,
            "/sites",
            json!({ "name": "Framing", "subdomain": sub("framing", &owner) }),
        )
        .await,
    );
    let other_site = created_id(
        "site",
        post(
            &outsider.app,
            &outsider.token,
            "/sites",
            json!({ "name": "Elsewhere", "subdomain": sub("elsewhere", &outsider) }),
        )
        .await,
    );

    // A one-pixel PNG: the smallest thing that is really an image.
    let png: Vec<u8> = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    let photo = upload(&owner, "image/png", png.clone()).await;
    let note = upload(&owner, "text/plain", b"not a picture".to_vec()).await;
    let foreign_photo = upload(&outsider, "image/png", png.clone()).await;

    let (status, headers, bytes) = get_bytes(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/images/{photo}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "image/png");
    assert_eq!(bytes, png, "the editor was served different bytes");
    // Authenticated origin: cacheable, but never in a shared cache.
    assert_eq!(
        headers.get("cache-control").unwrap(),
        "private, max-age=3600, immutable"
    );
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(
        headers.get("content-security-policy").unwrap(),
        "default-src 'none'; style-src 'unsafe-inline'"
    );

    // A blob that is not an image, and an id that never existed, are the same
    // answer as an image that is not yours.
    for id in [note.as_str(), "Nev3rExisted0000000001"] {
        let (status, _, _) = get_bytes(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/images/{id}"),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{id} was served");
    }

    // The mandatory wrong-tenant proof, from both directions: the outsider
    // cannot name the owner's site, and cannot smuggle the owner's blob id
    // through a site of their own.
    let (status, _, _) = get_bytes(
        &outsider.app,
        &outsider.token,
        &format!("/sites/{site}/images/{photo}"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "another tenant's site served"
    );
    let (status, _, _) = get_bytes(
        &outsider.app,
        &outsider.token,
        &format!("/sites/{other_site}/images/{photo}"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "another tenant's image bytes were served"
    );
    // ...and the owner cannot read the outsider's, so the boundary is not an
    // accident of who happened to upload first.
    let (status, _, _) = get_bytes(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/images/{foreign_photo}"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---- themes (S1.14) ----------------------------------------------------------

/// The preset listing the theme picker renders: at least the six the queue
/// requires, the default first, every palette token a hex color, every
/// preset named — static product data behind the same auth as the rest of
/// the edit surface.
#[tokio::test]
async fn theme_presets_list_the_shipped_palettes() {
    let h = harness("sites-presets").await;
    let (status, body) = get(&h.app, &h.token, "/sites/theme-presets").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let presets = body["presets"].as_array().expect("presets array");
    assert!(presets.len() >= 6, "at least six shipped presets");
    assert_eq!(presets[0]["id"], json!("north"), "the default leads");
    for preset in presets {
        assert!(!preset["name"].as_str().unwrap().is_empty());
        for token in [
            "background",
            "surface",
            "text",
            "mutedText",
            "primary",
            "onPrimary",
            "border",
        ] {
            let hex = preset["palette"][token].as_str().unwrap();
            assert!(
                hex.len() == 7 && hex.starts_with('#'),
                "{}: palette.{token} = {hex}",
                preset["id"]
            );
        }
        assert!(
            !preset["typography"]["headingFamily"]
                .as_str()
                .unwrap()
                .is_empty()
        );
        assert!(preset["typography"]["headingWeight"].is_u64());
    }
}

/// The config the publish UI composes its "goes live at" copy and live links
/// from: the deployment-wide sites domain. The suite (like the preview test
/// above) runs against the product default, `alosites.com`.
#[tokio::test]
async fn config_names_the_sites_domain() {
    let h = harness("sites-config").await;
    let (status, body) = get(&h.app, &h.token, "/sites/config").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["domain"], json!("alosites.com"));
}

/// The preview inlines visible theme and section images as `data:` URIs (the
/// public image path does not resolve on the edit origin), while crawler-only
/// OG metadata remains an absolute public URL and a referenced blob that is
/// not an image falls back instead of inlining non-image bytes.
#[tokio::test]
async fn preview_inlines_theme_and_section_images() {
    let h = harness("sites-imgprev").await;
    let logo = h
        .acc
        .put_blob(
            axum::body::Bytes::from_static(b"logo-bytes"),
            Some("image/png"),
        )
        .await
        .unwrap();
    let not_an_image = h
        .acc
        .put_blob(
            axum::body::Bytes::from_static(b"plain text"),
            Some("text/plain"),
        )
        .await
        .unwrap();

    let claimed = sub("imgpv", &h);
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Image Preview Co", "subdomain": claimed }),
        )
        .await,
    );
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/theme"),
        json!({ "schema_version": 1, "preset": "north", "logo": logo.as_str() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let page = created_id(
        "page",
        post(
            &h.app,
            &h.token,
            &format!("/sites/{site}/pages"),
            json!({ "title": "Home", "home": true }),
        )
        .await,
    );
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/{page}/sections"),
        json!({ "schema_version": 1, "sections": [
            { "type": "nav", "links": [] },
            { "type": "gallery", "images": [
                { "blob_id": not_an_image.as_str(), "alt": "" }
            ]}
        ]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let uri = format!("/sites/{site}/pages/{page}/preview");
    let (status, _, html) = get_text(&h.app, &h.token, &uri).await;
    assert_eq!(status, StatusCode::OK);
    // The logo is inlined: its bytes as a data URI ("logo-bytes" base64).
    assert!(html.contains("data:image/png;base64,bG9nby1ieXRlcw=="));
    assert!(html.contains(&format!(
        "https://{claimed}.alosites.com/assets/img/{}",
        logo.as_str()
    )));
    // The non-image blob is not inlined — public-path fallback.
    assert!(html.contains(&format!("/assets/img/{}", not_an_image.as_str())));
}
