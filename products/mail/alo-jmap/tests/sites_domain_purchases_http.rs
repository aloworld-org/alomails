//! The domain buy surface (`/sites/domain-*`, `/sites/{id}/domain-purchases*`,
//! ADR 0036 / S2.15c) driven through the real router over a real Postgres and
//! the shipped fixture registrar.
//!
//! `alo-store`'s `site_domain_purchases` suite proves the state machine; what
//! this one pins is the **edge**: that a deployment with no registrar says so
//! in the typed shape a buy box can branch on, that the price stored is the
//! seller's rather than the caller's, that approving echoes the numbers the
//! buyer saw, that the registrant is reachable only through its own door, that
//! a site-editor collaborator may not spend the tenant's money, and — mandatory
//! — that another tenant's purchase is invisible and untouchable on every verb.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use alo_jmap::sites::SiteDomainTxtLookup;
use alo_jmap::sites_domain_purchases::SiteDomainCommerce;
use alo_store::{FixtureRegistrar, UserId};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::macros::datetime;

use common::{Harness, harness, harness_on, send};

/// The custom-domain TXT boundary, which this surface never uses: a bought
/// name needs no ownership proof, because alo registered it.
struct NoDns;

impl SiteDomainTxtLookup for NoDns {
    fn lookup(&self, _name: String) -> BoxFuture<'static, Vec<String>> {
        Box::pin(async { Vec::new() })
    }
}

/// A fixed clock, so a test's prices and expiries never depend on the day it
/// is run.
const FIXTURE_NOW: OffsetDateTime = datetime!(2026-01-05 09:00 UTC);

const NAMESERVERS: [&str; 2] = ["ns1.alosites.com", "ns2.alosites.com"];

fn nameservers() -> Vec<String> {
    NAMESERVERS.iter().map(|ns| (*ns).to_owned()).collect()
}

/// The settlement secret of a deployment whose payment bridge is wired. Long
/// enough to be one: a shorter value is read as absent.
const SETTLEMENT_SECRET: &str = "settlement-secret-for-tests-0001";

/// A deployment that sells domains through the fixture reseller, plus the
/// concrete handle a test needs to seed a name as taken. No payment bridge:
/// the settle door is shut until a test asks for one.
fn selling() -> (Arc<FixtureRegistrar>, SiteDomainCommerce) {
    let registrar = Arc::new(FixtureRegistrar::new(FIXTURE_NOW).unwrap());
    let commerce = SiteDomainCommerce {
        registrar: Arc::clone(&registrar) as Arc<dyn alo_store::DomainRegistrar>,
        nameservers: nameservers(),
        settlement_secret: None,
    };
    (registrar, commerce)
}

/// The same deployment with a payment bridge wired, so charges can be declared
/// settled by whoever holds the secret and by nobody else.
fn selling_with_a_payment_bridge() -> (Arc<FixtureRegistrar>, SiteDomainCommerce) {
    let (registrar, commerce) = selling();
    (
        registrar,
        SiteDomainCommerce {
            settlement_secret: Some(SETTLEMENT_SECRET.to_owned()),
            ..commerce
        },
    )
}

fn app_with(h: &Harness, commerce: SiteDomainCommerce) -> Router {
    alo_jmap::app_with_site_boundaries(
        alo_jmap::app_state(Arc::clone(&h.store), h.identity.clone(), "http://test"),
        Arc::new(NoDns),
        commerce,
    )
}

/// A site subdomain unique to this harness run — the namespace is global.
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

/// A domain label unique to this harness run, so two runs against the shared
/// fixture never race for one name.
fn label(tag: &str, h: &Harness) -> String {
    let salt: String = h
        .tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .take(12)
        .collect();
    format!("{tag}{salt}")
}

async fn get(app: &Router, token: Option<&str>, uri: &str) -> (StatusCode, Value) {
    let mut req = Request::builder().method("GET").uri(uri);
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    send(app, req.body(Body::empty()).unwrap()).await
}

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

/// A colleague in the same tenant who did not create the site — the shape a
/// site-editor collaborator has, as far as this surface is concerned.
async fn colleague(h: &Harness) -> (String, UserId) {
    let email = format!("domain-colleague-{}@example.test", h.tenant);
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

fn registrant() -> Value {
    json!({
        "name": "Sanne de Vries",
        "organisation": "Acme BV",
        "email": "sanne@example.test",
        "street": "Keizersgracht 1",
        "postalCode": "1015 CJ",
        "city": "Amsterdam",
        "country": "nl",
        "phone": "+31201234567",
    })
}

fn buy_body(domain: &str, key: &str) -> Value {
    json!({
        "domain": domain,
        "years": 1,
        "requestKey": key,
        "registrant": registrant(),
    })
}

/// The quote a purchase response states, in the shape `/approve` wants back.
fn agreed(purchase: &Value) -> Value {
    json!({
        "domain": purchase["domain"],
        "termYears": purchase["termYears"],
        "currency": purchase["currency"],
        "firstTermCents": purchase["firstTermCents"],
        "renewalCentsPerYear": purchase["renewalCentsPerYear"],
        "premium": purchase["premium"],
    })
}

// ---- the unconfigured deployment ---------------------------------------------

#[tokio::test]
async fn without_a_registrar_every_door_says_unconfigured() {
    let h = harness("domain-unconfigured").await;
    let app = app_with(&h, SiteDomainCommerce::default());
    let site = h.acc.create_site("Shop", &sub("shop", &h)).await.unwrap();

    for uri in ["/sites/domain-catalog", "/sites/domain-search?q=acme"] {
        let (status, body) = get(&app, Some(&h.token), uri).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{uri}: {body}");
        assert_eq!(body["reason"], "unconfigured", "{uri}");
        // The refusal offers the thing that still works.
        assert!(
            body["detail"]
                .as_str()
                .unwrap()
                .contains("connect a domain you already own"),
            "{body}"
        );
    }

    let (status, body) = post(
        &app,
        &h.token,
        &format!("/sites/{}/domain-purchases", site.as_str()),
        buy_body("acme.com", "unconfigured-key-1"),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["reason"], "unconfigured");

    // Nothing was recorded on the way to that refusal.
    let (status, body) = get(
        &app,
        Some(&h.token),
        &format!("/sites/{}/domain-purchases", site.as_str()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["purchases"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn the_price_surface_needs_a_token() {
    let h = harness("domain-anon").await;
    let (_registrar, commerce) = selling();
    let app = app_with(&h, commerce);
    for uri in [
        "/sites/domain-catalog",
        "/sites/domain-search?q=acme",
        "/sites/anything/domain-purchases",
    ] {
        let (status, _) = get(&app, None, uri).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}");
    }
}

// ---- prices ------------------------------------------------------------------

#[tokio::test]
async fn the_catalog_states_both_prices_and_who_sells_them() {
    let h = harness("domain-catalog").await;
    let (_registrar, commerce) = selling();
    let app = app_with(&h, commerce);

    let (status, body) = get(&app, Some(&h.token), "/sites/domain-catalog").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["currency"], "EUR");
    assert_eq!(body["buyable"], true);
    assert_eq!(body["registrar"]["environment"], "fixture");
    assert_eq!(body["registrar"]["spendsMoney"], false);
    // An EU-established reseller is a type invariant of the model; the wire
    // shows it so an operator can see it.
    assert_eq!(body["registrar"]["country"], "nl");

    let endings = body["endings"].as_array().unwrap();
    assert!(endings.len() >= 5, "{body}");
    for ending in endings {
        let register = ending["registerCents"].as_i64().unwrap();
        let renew = ending["renewCents"].as_i64().unwrap();
        assert!(register > 0 && renew > 0, "{ending}");
        // The honest-pricing promise, on the wire: no first-year bait.
        assert!(register >= renew, "{ending} renews above its first year");
    }
    let eu = endings
        .iter()
        .find(|ending| ending["tld"] == "eu")
        .expect(".eu is sold");
    assert_eq!(eu["requirement"]["kind"], "eea_presence");
}

#[tokio::test]
async fn a_search_prices_only_what_can_be_bought() {
    let h = harness("domain-search").await;
    let (_registrar, commerce) = selling();
    let app = app_with(&h, commerce);

    // Typed as a URL, with capitals and a path — one search, and the ending
    // the buyer named comes first (S1.30b's lesson).
    let (status, body) = get(
        &app,
        Some(&h.token),
        "/sites/domain-search?q=https%3A%2F%2FAcme.com%2F&tlds=eu,xyz",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["label"], "acme");
    let offers = body["offers"].as_array().unwrap();
    assert_eq!(offers[0]["domain"], "acme.com");

    // acme.com and acme.eu are seeded as somebody else's: named, never priced.
    for offer in offers.iter().filter(|o| o["availability"] != "available") {
        assert!(offer["quote"].is_null(), "{offer} carries a price");
    }
    let unsupported = offers
        .iter()
        .find(|offer| offer["domain"] == "acme.xyz")
        .expect("an ending we do not sell is still answered");
    assert_eq!(unsupported["availability"], "unsupported");
    assert!(unsupported["quote"].is_null());

    // A free name is priced, with the renewal beside what is paid today.
    let free = label("free", &h);
    let (status, body) = get(
        &app,
        Some(&h.token),
        &format!("/sites/domain-search?q={free}&tlds=com"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let offer = &body["offers"][0];
    assert_eq!(offer["availability"], "available");
    assert!(offer["quote"]["firstTermCents"].as_i64().unwrap() > 0);
    assert!(
        offer["quote"]["renewalCentsPerYear"].as_i64().unwrap() > 0,
        "a quote always states the renewal"
    );
}

#[tokio::test]
async fn an_empty_or_oversized_search_is_told_what_to_type() {
    let h = harness("domain-search-bad").await;
    let (_registrar, commerce) = selling();
    let app = app_with(&h, commerce);

    let (status, body) = get(&app, Some(&h.token), "/sites/domain-search?q=").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"].as_str().unwrap().contains("acme"),
        "the refusal shows what a name looks like: {body}"
    );

    let long = "a".repeat(200);
    let (status, _) = get(
        &app,
        Some(&h.token),
        &format!("/sites/domain-search?q={long}"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // A name inside a domain keeps the sharp refusal rather than becoming a
    // search for something nobody can buy.
    let (status, body) = get(&app, Some(&h.token), "/sites/domain-search?q=shop.acme.com").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}

// ---- buying ------------------------------------------------------------------

#[tokio::test]
async fn buying_stores_the_sellers_price_and_charges_nothing_yet() {
    let h = harness("domain-buy").await;
    let (_registrar, commerce) = selling();
    let app = app_with(&h, commerce);
    let site = h.acc.create_site("Shop", &sub("buy", &h)).await.unwrap();
    let name = format!("{}.com", label("buy", &h));

    let (status, catalog) = get(&app, Some(&h.token), "/sites/domain-catalog").await;
    assert_eq!(status, StatusCode::OK);
    let com = catalog["endings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|ending| ending["tld"] == "com")
        .unwrap()
        .clone();

    // The body carries no price at all — and a client that invents one is
    // ignored rather than believed.
    let mut body = buy_body(&name, "buy-key-abcdef");
    body["firstTermCents"] = json!(1);
    body["years"] = json!(2);
    let (status, purchase) = post(
        &app,
        &h.token,
        &format!("/sites/{}/domain-purchases", site.as_str()),
        body,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{purchase}");
    assert_eq!(purchase["state"], "quoted");
    assert_eq!(purchase["kind"], "registration");
    assert_eq!(purchase["domain"], name);
    assert_eq!(purchase["termYears"], 2);
    assert_eq!(purchase["currency"], "EUR");
    assert_eq!(
        purchase["firstTermCents"].as_i64().unwrap(),
        com["registerCents"].as_i64().unwrap() * 2,
        "two years cost twice one year, not one cheap year plus renewals"
    );
    assert_eq!(purchase["renewalCentsPerYear"], com["renewCents"]);
    assert_eq!(purchase["moneyMoved"], false);
    assert_eq!(purchase["nameservers"][0], NAMESERVERS[0]);
    assert!(purchase["approvedAt"].is_null());
    assert!(purchase["paymentReference"].is_null());

    // A purchase is never a place a registrant is spread.
    let listed = get(
        &app,
        Some(&h.token),
        &format!("/sites/{}/domain-purchases", site.as_str()),
    )
    .await
    .1;
    let text = listed.to_string();
    assert!(!text.contains("Keizersgracht"), "{text}");
    assert!(!text.contains("sanne@example.test"), "{text}");
    assert_eq!(listed["purchases"][0]["id"], purchase["id"]);

    // The registrant has one door, and it shows exactly what will be submitted.
    let (status, contact) = get(
        &app,
        Some(&h.token),
        &format!(
            "/sites/{}/domain-purchases/{}/registrant",
            site.as_str(),
            purchase["id"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{contact}");
    assert_eq!(contact["email"], "sanne@example.test");
    assert_eq!(contact["street"], "Keizersgracht 1");
    assert_eq!(contact["country"], "nl");
}

#[tokio::test]
async fn a_second_click_buys_the_same_domain_once() {
    let h = harness("domain-replay").await;
    let (_registrar, commerce) = selling();
    let app = app_with(&h, commerce);
    let site = h.acc.create_site("Shop", &sub("replay", &h)).await.unwrap();
    let name = format!("{}.com", label("replay", &h));
    let uri = format!("/sites/{}/domain-purchases", site.as_str());

    let (status, first) = post(&app, &h.token, &uri, buy_body(&name, "replay-key-1234")).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let (status, second) = post(&app, &h.token, &uri, buy_body(&name, "replay-key-1234")).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(first["id"], second["id"], "a retry is not a second domain");

    // The same key for a different name is a caller bug, refused in words.
    let other = format!("{}.com", label("other", &h));
    let (status, body) = post(&app, &h.token, &uri, buy_body(&other, "replay-key-1234")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // And a key too short to be a replay token is refused before anything else.
    let (status, body) = post(&app, &h.token, &uri, buy_body(&other, "short")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    let listed = get(&app, Some(&h.token), &uri).await.1;
    assert_eq!(listed["purchases"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_name_that_went_in_the_meantime_is_refused_in_words() {
    let h = harness("domain-race").await;
    let (registrar, commerce) = selling();
    let app = app_with(&h, commerce);
    let site = h.acc.create_site("Shop", &sub("race", &h)).await.unwrap();
    let name = format!("{}.com", label("race", &h));
    registrar.seed_taken(&name).unwrap();

    let (status, body) = post(
        &app,
        &h.token,
        &format!("/sites/{}/domain-purchases", site.as_str()),
        buy_body(&name, "race-key-123456"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["reason"], "unavailable");

    // An ending nobody here sells is a different sentence from a taken name.
    let (status, body) = post(
        &app,
        &h.token,
        &format!("/sites/{}/domain-purchases", site.as_str()),
        buy_body("something.xyz", "race-key-234567"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["reason"], "unsupported");
    assert_eq!(body["tld"], "xyz");
}

#[tokio::test]
async fn a_registrant_a_registry_would_reject_never_becomes_a_purchase() {
    let h = harness("domain-registrant").await;
    let (_registrar, commerce) = selling();
    let app = app_with(&h, commerce);
    let site = h
        .acc
        .create_site("Shop", &sub("registrant", &h))
        .await
        .unwrap();
    let uri = format!("/sites/{}/domain-purchases", site.as_str());
    let name = format!("{}.com", label("reg", &h));

    let mut body = buy_body(&name, "registrant-key-1");
    body["registrant"]["phone"] = json!("020 123 4567");
    let (status, problem) = post(&app, &h.token, &uri, body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
    let detail = problem["detail"].as_str().unwrap();
    assert!(detail.contains("international form"), "{detail}");
    // The refusal names the field and never quotes the value back.
    assert!(!detail.contains("020 123 4567"), "{detail}");

    assert_eq!(
        get(&app, Some(&h.token), &uri).await.1["purchases"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

// ---- approving ---------------------------------------------------------------

#[tokio::test]
async fn approval_agrees_to_the_price_that_was_on_screen() {
    let h = harness("domain-approve").await;
    let (_registrar, commerce) = selling();
    let app = app_with(&h, commerce);
    let site = h
        .acc
        .create_site("Shop", &sub("approve", &h))
        .await
        .unwrap();
    let name = format!("{}.com", label("appr", &h));
    let (_, purchase) = post(
        &app,
        &h.token,
        &format!("/sites/{}/domain-purchases", site.as_str()),
        buy_body(&name, "approve-key-1234"),
    )
    .await;
    let approve = format!(
        "/sites/{}/domain-purchases/{}/approve",
        site.as_str(),
        purchase["id"].as_str().unwrap()
    );

    // A renewal price the buyer never saw is the half a bait price hides in.
    let mut tampered = agreed(&purchase);
    tampered["renewalCentsPerYear"] = json!(1);
    let (status, body) = post(&app, &h.token, &approve, json!({ "agreed": tampered })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body["detail"].as_str().unwrap().contains("price"), "{body}");

    // So is a first term that moved.
    let mut tampered = agreed(&purchase);
    tampered["firstTermCents"] = json!(1);
    let (status, _) = post(&app, &h.token, &approve, json!({ "agreed": tampered })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Nothing moved on the way through those refusals.
    let one = format!(
        "/sites/{}/domain-purchases/{}",
        site.as_str(),
        purchase["id"].as_str().unwrap()
    );
    assert_eq!(get(&app, Some(&h.token), &one).await.1["state"], "quoted");

    let (status, approved) = post(
        &app,
        &h.token,
        &approve,
        json!({ "agreed": agreed(&purchase) }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{approved}");
    assert_eq!(approved["state"], "approved");
    assert_eq!(approved["approvedBy"], h.user.as_str());
    assert!(!approved["approvedAt"].is_null());
    assert_eq!(
        approved["moneyMoved"], false,
        "approving a price is not paying it"
    );

    // Approving twice at the same price is the same purchase, not a second one.
    let (status, again) = post(
        &app,
        &h.token,
        &approve,
        json!({ "agreed": agreed(&purchase) }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["id"], approved["id"]);
}

#[tokio::test]
async fn a_purchase_can_be_called_off_before_money_moves() {
    let h = harness("domain-cancel").await;
    let (_registrar, commerce) = selling();
    let app = app_with(&h, commerce);
    let site = h.acc.create_site("Shop", &sub("cancel", &h)).await.unwrap();
    let name = format!("{}.com", label("canc", &h));
    let (_, purchase) = post(
        &app,
        &h.token,
        &format!("/sites/{}/domain-purchases", site.as_str()),
        buy_body(&name, "cancel-key-1234"),
    )
    .await;
    let id = purchase["id"].as_str().unwrap();

    let (status, cancelled) = post(
        &app,
        &h.token,
        &format!("/sites/{}/domain-purchases/{id}/cancel", site.as_str()),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    assert_eq!(cancelled["state"], "cancelled");
    assert_eq!(cancelled["open"], false);

    // A called-off purchase cannot be approved back to life.
    let (status, body) = post(
        &app,
        &h.token,
        &format!("/sites/{}/domain-purchases/{id}/approve", site.as_str()),
        json!({ "agreed": agreed(&purchase) }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}

// ---- who may buy -------------------------------------------------------------

#[tokio::test]
async fn a_colleague_who_edits_the_site_may_not_spend_the_tenants_money() {
    let h = harness("domain-editor").await;
    let (_registrar, commerce) = selling();
    let app = app_with(&h, commerce);
    let site = h.acc.create_site("Shop", &sub("editor", &h)).await.unwrap();
    let name = format!("{}.com", label("edit", &h));
    let (_, purchase) = post(
        &app,
        &h.token,
        &format!("/sites/{}/domain-purchases", site.as_str()),
        buy_body(&name, "editor-key-1234"),
    )
    .await;
    let id = purchase["id"].as_str().unwrap();
    let (other, _) = colleague(&h).await;
    let base = format!("/sites/{}/domain-purchases", site.as_str());

    for uri in [
        base.clone(),
        format!("{base}/{id}"),
        format!("{base}/{id}/registrant"),
    ] {
        let (status, body) = get(&app, Some(&other), &uri).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {body}");
    }
    for uri in [
        base.clone(),
        format!("{base}/{id}/approve"),
        format!("{base}/{id}/cancel"),
    ] {
        let (status, body) = post(&app, &other, &uri, json!({})).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {body}");
    }

    // The owner's purchase is untouched by all of that.
    let (_, still) = get(&app, Some(&h.token), &format!("{base}/{id}")).await;
    assert_eq!(still["state"], "quoted");
}

// ---- tenancy ------------------------------------------------------------------

#[tokio::test]
async fn another_tenants_purchase_is_invisible_and_untouchable() {
    let a = harness("domain-tenant-a").await;
    let b = harness_on(Arc::clone(&a.store), "domain-tenant-b").await;
    let (_registrar, commerce) = selling();
    let app = app_with(&a, commerce);

    let a_site = a.acc.create_site("A", &sub("tena", &a)).await.unwrap();
    let b_site = b.acc.create_site("B", &sub("tenb", &b)).await.unwrap();
    let name = format!("{}.com", label("ten", &a));
    let (status, purchase) = post(
        &app,
        &a.token,
        &format!("/sites/{}/domain-purchases", a_site.as_str()),
        buy_body(&name, "tenant-key-1234"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{purchase}");
    let id = purchase["id"].as_str().unwrap();

    // A's site simply does not exist for B — the same answer a nonexistent id
    // gets, so the URL is no existence oracle.
    let a_base = format!("/sites/{}/domain-purchases", a_site.as_str());
    for uri in [
        a_base.clone(),
        format!("{a_base}/{id}"),
        format!("{a_base}/{id}/registrant"),
    ] {
        let (status, body) = get(&app, Some(&b.token), &uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {body}");
    }
    for uri in [
        a_base.clone(),
        format!("{a_base}/{id}/approve"),
        format!("{a_base}/{id}/cancel"),
    ] {
        let (status, body) =
            post(&app, &b.token, &uri, json!({ "agreed": agreed(&purchase) })).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {body}");
    }

    // Nor can B reach A's purchase id through B's own site.
    let b_base = format!("/sites/{}/domain-purchases", b_site.as_str());
    for uri in [
        format!("{b_base}/{id}"),
        format!("{b_base}/{id}/registrant"),
    ] {
        let (status, body) = get(&app, Some(&b.token), &uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {body}");
    }
    let (status, body) = post(&app, &b.token, &format!("{b_base}/{id}/cancel"), json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // A's purchase is exactly as it was.
    let (status, still) = get(&app, Some(&a.token), &format!("{a_base}/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{still}");
    assert_eq!(still["state"], "quoted");
    assert_eq!(
        get(&app, Some(&b.token), &b_base).await.1["purchases"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn a_purchase_answers_only_under_the_site_it_belongs_to() {
    let h = harness("domain-crosssite").await;
    let (_registrar, commerce) = selling();
    let app = app_with(&h, commerce);
    let one = h.acc.create_site("One", &sub("one", &h)).await.unwrap();
    let two = h.acc.create_site("Two", &sub("two", &h)).await.unwrap();
    let name = format!("{}.com", label("cross", &h));
    let (_, purchase) = post(
        &app,
        &h.token,
        &format!("/sites/{}/domain-purchases", one.as_str()),
        buy_body(&name, "cross-key-12345"),
    )
    .await;
    let id = purchase["id"].as_str().unwrap();

    let (status, body) = get(
        &app,
        Some(&h.token),
        &format!("/sites/{}/domain-purchases/{id}", two.as_str()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    let (status, body) = get(
        &app,
        Some(&h.token),
        &format!("/sites/{}/domain-purchases/nonexistent", one.as_str()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

// ---- the payment handoff (S2.15c2) --------------------------------------------

/// POSTs to the payment bridge's door, which carries a deployment secret
/// instead of anybody's token.
async fn settle(app: &Router, secret: Option<&str>, body: Value) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method("POST")
        .uri("/sites/domain-payments/settle")
        .header("content-type", "application/json");
    if let Some(secret) = secret {
        req = req.header("x-alo-settlement", secret);
    }
    send(app, req.body(Body::from(body.to_string())).unwrap()).await
}

/// Buys and approves one name, answering with (site id, purchase id, quote).
async fn approved_purchase(h: &Harness, app: &Router, tag: &str) -> (String, String, Value) {
    let site = h
        .acc
        .create_site("Shop", &sub(tag, h))
        .await
        .unwrap()
        .as_str()
        .to_owned();
    let name = format!("{}.com", label(tag, h));
    let (status, purchase) = post(
        app,
        &h.token,
        &format!("/sites/{site}/domain-purchases"),
        buy_body(&name, &format!("{tag}-key-12345678")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{purchase}");
    let id = purchase["id"].as_str().unwrap().to_owned();
    let (status, approved) = post(
        app,
        &h.token,
        &format!("/sites/{site}/domain-purchases/{id}/approve"),
        json!({ "agreed": agreed(&purchase) }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{approved}");
    (site, id, purchase)
}

#[tokio::test]
async fn checkout_records_the_reference_without_moving_any_money() {
    let h = harness("domain-checkout").await;
    let (_registrar, commerce) = selling();
    let app = app_with(&h, commerce);
    let (site, id, _) = approved_purchase(&h, &app, "chk").await;
    let checkout = format!("/sites/{site}/domain-purchases/{id}/checkout");

    // Billing's own string, stored verbatim and never parsed.
    let (status, awaiting) = post(
        &app,
        &h.token,
        &checkout,
        json!({ "paymentReference": "pi/2026-08/0042" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{awaiting}");
    assert_eq!(awaiting["state"], "awaiting_payment");
    assert_eq!(awaiting["paymentReference"], "pi/2026-08/0042");
    assert_eq!(
        awaiting["moneyMoved"], false,
        "asking for money is not receiving it"
    );
    assert!(awaiting["paidAt"].is_null());

    // The same reference again is the same purchase; a second, different one
    // is refused rather than quietly replacing the charge in flight.
    let (status, again) = post(
        &app,
        &h.token,
        &checkout,
        json!({ "paymentReference": "pi/2026-08/0042" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["id"], awaiting["id"]);
    let (status, body) = post(
        &app,
        &h.token,
        &checkout,
        json!({ "paymentReference": "pi/2026-08/0043" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"].as_str().unwrap().contains("another payment"),
        "{body}"
    );

    // A reference that is not one is refused in the rule's own words.
    let (site2, id2, _) = approved_purchase(&h, &app, "chk2").await;
    let (status, body) = post(
        &app,
        &h.token,
        &format!("/sites/{site2}/domain-purchases/{id2}/checkout"),
        json!({ "paymentReference": " " }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"].as_str().unwrap().contains("characters"),
        "{body}"
    );
}

#[tokio::test]
async fn an_unapproved_purchase_cannot_be_handed_to_a_payment() {
    let h = harness("domain-checkout-early").await;
    let (_registrar, commerce) = selling();
    let app = app_with(&h, commerce);
    let site = h.acc.create_site("Shop", &sub("early", &h)).await.unwrap();
    let name = format!("{}.com", label("early", &h));
    let (_, purchase) = post(
        &app,
        &h.token,
        &format!("/sites/{}/domain-purchases", site.as_str()),
        buy_body(&name, "early-key-12345"),
    )
    .await;
    let id = purchase["id"].as_str().unwrap();
    let (status, body) = post(
        &app,
        &h.token,
        &format!("/sites/{}/domain-purchases/{id}/checkout", site.as_str()),
        json!({ "paymentReference": "pi_early_0001" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"].as_str().unwrap().contains("approved"),
        "{body}"
    );
}

#[tokio::test]
async fn only_the_payment_bridge_may_say_a_charge_settled() {
    let h = harness("domain-settle").await;
    let (_registrar, commerce) = selling_with_a_payment_bridge();
    let app = app_with(&h, commerce);
    let (site, id, _) = approved_purchase(&h, &app, "set").await;
    let reference = format!("pi_settle_{}", id);
    let (status, awaiting) = post(
        &app,
        &h.token,
        &format!("/sites/{site}/domain-purchases/{id}/checkout"),
        json!({ "paymentReference": reference }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{awaiting}");

    let body = json!({ "tenant": h.tenant.as_str(), "paymentReference": reference });

    // No secret, and a wrong secret, are the same closed door.
    for secret in [None, Some("not-the-settlement-secret-0001")] {
        let (status, refused) = settle(&app, secret, body.clone()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{refused}");
    }
    // And nothing moved through them.
    let (_, still) = get(
        &app,
        Some(&h.token),
        &format!("/sites/{site}/domain-purchases/{id}"),
    )
    .await;
    assert_eq!(still["state"], "awaiting_payment");

    // A reference nobody is waiting for, and the right reference under the
    // wrong tenant, are one answer: this door is no oracle.
    for wrong in [
        json!({ "tenant": h.tenant.as_str(), "paymentReference": "pi_never_seen_0001" }),
        json!({ "tenant": "tenant-that-does-not-exist", "paymentReference": reference }),
    ] {
        let (status, refused) = settle(&app, Some(SETTLEMENT_SECRET), wrong).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{refused}");
    }

    let (status, paid) = settle(&app, Some(SETTLEMENT_SECRET), body.clone()).await;
    assert_eq!(status, StatusCode::OK, "{paid}");
    assert_eq!(paid["state"], "paid");
    assert_eq!(paid["moneyMoved"], true);
    assert!(!paid["paidAt"].is_null());

    // A webhook delivered twice settles one purchase.
    let (status, twice) = settle(&app, Some(SETTLEMENT_SECRET), body).await;
    assert_eq!(status, StatusCode::OK, "{twice}");
    assert_eq!(twice["id"], paid["id"]);
    assert_eq!(twice["state"], "paid");

    // Past the charge, calling it off is a support conversation, not a button.
    let (status, refused) = post(
        &app,
        &h.token,
        &format!("/sites/{site}/domain-purchases/{id}/cancel"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
}

#[tokio::test]
async fn a_deployment_with_no_payment_bridge_settles_nothing() {
    let h = harness("domain-settle-unwired").await;
    let (_registrar, commerce) = selling();
    let app = app_with(&h, commerce);
    let (site, id, _) = approved_purchase(&h, &app, "unw").await;
    let reference = format!("pi_unwired_{}", id);
    post(
        &app,
        &h.token,
        &format!("/sites/{site}/domain-purchases/{id}/checkout"),
        json!({ "paymentReference": reference }),
    )
    .await;

    // Even holding a secret: there is none to hold.
    for secret in [None, Some(SETTLEMENT_SECRET)] {
        let (status, body) = settle(
            &app,
            secret,
            json!({ "tenant": h.tenant.as_str(), "paymentReference": reference }),
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
        assert_eq!(body["reason"], "unconfigured");
    }
    let (_, still) = get(
        &app,
        Some(&h.token),
        &format!("/sites/{site}/domain-purchases/{id}"),
    )
    .await;
    assert_eq!(still["state"], "awaiting_payment");
    assert_eq!(still["moneyMoved"], false);
}

#[tokio::test]
async fn checkout_is_the_owners_door_and_no_one_elses() {
    let a = harness("domain-checkout-a").await;
    let b = harness_on(Arc::clone(&a.store), "domain-checkout-b").await;
    let (_registrar, commerce) = selling();
    let app = app_with(&a, commerce);
    let (site, id, _) = approved_purchase(&a, &app, "own").await;
    let checkout = format!("/sites/{site}/domain-purchases/{id}/checkout");
    let body = json!({ "paymentReference": "pi_someone_else_01" });

    let (other, _) = colleague(&a).await;
    let (status, refused) = post(&app, &other, &checkout, body.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");

    let (status, refused) = post(&app, &b.token, &checkout, body).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{refused}");

    let (_, still) = get(
        &app,
        Some(&a.token),
        &format!("/sites/{site}/domain-purchases/{id}"),
    )
    .await;
    assert_eq!(still["state"], "approved");
    assert!(still["paymentReference"].is_null());
}
