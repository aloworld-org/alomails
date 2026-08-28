//! JMAP test harness: an in-process router over a real Postgres store,
//! per test, with a logged-in account. Requests are driven through the
//! router as a `tower::Service` (no socket).
#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

/// The offline scripted model an agent suite drives its turns against.
pub mod model;

use std::sync::Arc;

use alo_identity::{Identity, IdentityConfig};
use alo_store::{AccountStore, BlobStore, Store, TenantId, TenantStore, UserId};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

/// Builds a test `Identity` over a store handle (a fixed dev issuer).
pub fn test_identity(store: Arc<Store>) -> Identity {
    Identity::new(store, IdentityConfig::new("https://id.test")).expect("identity")
}

/// The database this suite runs against.
///
/// Delegates to `alo_test_db`, which refuses the database the product
/// runs on: suites create and drop their own, they never write into `alo`.
pub fn database_url() -> String {
    alo_test_db::url()
}

pub struct Harness {
    pub app: Router,
    pub token: String,
    pub account_id: String,
    pub email: String,
    pub store: Arc<Store>,
    pub identity: Identity,
    pub ts: TenantStore,
    pub acc: AccountStore,
    pub user: UserId,
    pub tenant: TenantId,
}

/// A fresh tenant + logged-in user over the shared Postgres, with the
/// JMAP router wired up.
pub async fn harness(tag: &str) -> Harness {
    let (harness, _) = harness_with_blobs(tag).await;
    harness
}

/// A fresh harness plus the exact blob backend attached to its store. Public
/// service integration tests use the clone to prove that bytes uploaded
/// through JMAP are the bytes served anonymously by Sites.
pub async fn harness_with_blobs(tag: &str) -> (Harness, BlobStore) {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url())
        .await
        .expect("connect to test postgres");
    let blobs = BlobStore::in_memory(50 * 1024 * 1024);
    let store = Arc::new(Store::new(pool, blobs.clone()));
    store.migrate().await.unwrap();
    (harness_on(store, tag).await, blobs)
}

/// A fresh tenant + logged-in user on an EXISTING store handle — for tests
/// that need two tenants sharing one process-wide store (e.g. cross-tenant
/// sweeps), the way production runs.
pub async fn harness_on(store: Arc<Store>, tag: &str) -> Harness {
    let tenant = store.create_tenant(&format!("jmap-{tag}")).await.unwrap();
    // The username has a global unique index; include the random tenant id
    // so reruns against the shared database never collide.
    let email = format!("{tag}-{tenant}@example.test");
    let ts = store.for_tenant(tenant.clone());
    let user = ts.create_user(&email).await.unwrap();
    let identity = test_identity(Arc::clone(&store));
    identity
        .set_password(&tenant, &user, &email, "s3cret-pw")
        .await
        .unwrap();
    let acc = store.for_account(tenant.clone(), user.clone());
    let token = identity
        .password_login(&email, "s3cret-pw", None)
        .await
        .unwrap()
        .expect("token issued")
        .0
        .reveal()
        .to_owned();
    // **https**, and not cosmetic. Campaign mail builds its unsubscribe URLs
    // from this origin, and RFC 8058 §3.1 ties one-click to HTTPS — a header
    // emitted beside any other scheme is one every client ignores. A harness on
    // `http://` exercises a deployment that cannot lawfully send bulk mail, and
    // would hide that refusal rather than prove it. Nothing here opens a
    // socket, so the scheme costs nothing.
    let app = alo_jmap::app(alo_jmap::app_state(
        Arc::clone(&store),
        identity.clone(),
        "https://test",
    ));
    Harness {
        app,
        token,
        account_id: user.to_string(),
        email,
        store,
        identity,
        ts,
        acc,
        user,
        tenant,
    }
}

/// Sends a raw request through the router; returns (status, body-json).
pub async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024 * 1024)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

/// POSTs a JMAP Request to `/jmap/api` with the given bearer token.
pub async fn api(app: &Router, token: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri("/jmap/api")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, req).await
}

/// GETs `path` with the given bearer token (for the small REST endpoints
/// alongside the JMAP API, e.g. `/contacts`).
pub async fn get(app: &Router, token: &str, path: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

/// GETs `path` and returns the raw response body as text (for non-JSON
/// endpoints, e.g. the `.vcf` export).
pub async fn get_text(app: &Router, token: &str, path: &str) -> (StatusCode, String) {
    let req = Request::builder()
        .method("GET")
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024 * 1024)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// POSTs a raw text/bytes body to `path` (e.g. a `.vcf` import).
pub async fn post_raw(app: &Router, token: &str, path: &str, body: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "text/vcard")
        .body(Body::from(body.to_owned()))
        .unwrap();
    send(app, req).await
}

/// A single method call wrapped in a Request envelope.
pub fn call(method: &str, args: Value) -> Value {
    serde_json::json!({
        "using": ["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:mail"],
        "methodCalls": [[method, args, "c0"]]
    })
}

/// Seeds the default chart of accounts for `account`'s tenant, with plain
/// per-code test names.
///
/// Issuing a document **books it** in the same transaction (B7.01), so any
/// suite that issues an invoice or records a payment needs a chart the booking
/// can resolve its roles against — the setup a real tenant performs by opening
/// the Accounts screen once.
pub async fn seed_default_chart(account: &AccountStore) {
    let seed = alo_store::ChartSeed {
        names: alo_store::CHART
            .iter()
            .map(|entry| alo_store::ChartName {
                code: entry.code.to_owned(),
                name: format!("Account {}", entry.code),
            })
            .collect(),
    };
    account
        .fin_accounts_or_seed(&seed, false)
        .await
        .expect("seed the default chart");
}
