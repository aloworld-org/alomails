//! The registration sweep (ADR 0036, S2.15c2): what happens after a domain
//! purchase is paid for.
//!
//! The store suite proves the state machine and the HTTP suite proves the
//! doors; this one drives the **whole arc on one process** — buy, approve,
//! hand to a payment, settle through the bridge's door, sweep — and pins the
//! three outcomes that decide whether somebody's money bought them a website:
//! a registered name attaches itself and starts serving, a name that went
//! while the payment was in flight ends in a sentence about a refund rather
//! than a retry loop, and a registrar that is briefly down loses nothing.
//!
//! Every registrar here is in memory. Nothing in this file can register a
//! domain or spend a cent.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};

use alo_jmap::sites::SiteDomainTxtLookup;
use alo_jmap::sites_domain_purchases::SiteDomainCommerce;
use alo_store::site_registrar::RegistrarFuture;
use alo_store::{
    DomainOffer, DomainOrder, DomainQuote, DomainRegistrar, DomainSearch, FixtureRegistrar,
    RegisteredDomain, RegistrarError, RegistrarIdentity, SiteDomainStatus, SiteId, TldCatalog,
};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::macros::datetime;

use tokio::sync::{Mutex, MutexGuard};

use crate::common::{Harness, harness, harness_on, send};

/// The registration sweep claims paid purchases across every tenant — that is
/// what a sweep is — so two tests sweeping at once take each other's rows.
/// Every test here holds this first; `.config/nextest.toml` does the same job
/// for the process-per-test runner, and between the two the suite is honest
/// under both.
static SWEEP: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

async fn sweeping() -> MutexGuard<'static, ()> {
    SWEEP.lock().await
}

/// The custom-domain TXT boundary. A bought name never reaches it: alo
/// registered it, so there is no ownership left to prove.
struct NoDns;

impl SiteDomainTxtLookup for NoDns {
    fn lookup(&self, _name: String) -> BoxFuture<'static, Vec<String>> {
        Box::pin(async { Vec::new() })
    }
}

const FIXTURE_NOW: OffsetDateTime = datetime!(2026-01-05 09:00 UTC);

const SETTLEMENT_SECRET: &str = "settlement-secret-for-tests-0001";

fn nameservers() -> Vec<String> {
    ["ns1.alosites.com", "ns2.alosites.com"]
        .iter()
        .map(|ns| (*ns).to_owned())
        .collect()
}

/// A registrar that can be switched off, so a test can watch a paid purchase
/// survive an outage instead of being written off by it.
///
/// Everything except [`DomainRegistrar::register`] is the fixture's; while it
/// is down, registering is the retryable provider fault a real reseller
/// timeout produces.
struct FlakyRegistrar {
    inner: Arc<FixtureRegistrar>,
    down: AtomicBool,
}

impl FlakyRegistrar {
    fn new(inner: Arc<FixtureRegistrar>) -> Self {
        Self {
            inner,
            down: AtomicBool::new(true),
        }
    }

    fn come_back_up(&self) {
        self.down.store(false, Ordering::SeqCst);
    }
}

impl DomainRegistrar for FlakyRegistrar {
    fn identity(&self) -> RegistrarIdentity {
        self.inner.identity()
    }

    fn catalog(&self) -> RegistrarFuture<'_, TldCatalog> {
        self.inner.catalog()
    }

    fn search(&self, search: DomainSearch) -> RegistrarFuture<'_, Vec<DomainOffer>> {
        self.inner.search(search)
    }

    fn quote(&self, domain: String, years: u8) -> RegistrarFuture<'_, DomainQuote> {
        self.inner.quote(domain, years)
    }

    fn register(&self, order: DomainOrder) -> RegistrarFuture<'_, RegisteredDomain> {
        if self.down.load(Ordering::SeqCst) {
            return Box::pin(async {
                Err(RegistrarError::Provider {
                    retryable: true,
                    message: "the registrar did not answer in time".to_owned(),
                })
            });
        }
        self.inner.register(order)
    }

    fn renew(
        &self,
        domain: String,
        years: u8,
        idempotency_key: String,
    ) -> RegistrarFuture<'_, RegisteredDomain> {
        self.inner.renew(domain, years, idempotency_key)
    }

    fn lookup(&self, domain: String) -> RegistrarFuture<'_, Option<RegisteredDomain>> {
        self.inner.lookup(domain)
    }
}

fn app_with(h: &Harness, registrar: Arc<dyn DomainRegistrar>) -> Router {
    alo_jmap::app_with_site_boundaries(
        alo_jmap::app_state(Arc::clone(&h.store), h.identity.clone(), "http://test"),
        Arc::new(NoDns),
        SiteDomainCommerce {
            registrar,
            nameservers: nameservers(),
            settlement_secret: Some(SETTLEMENT_SECRET.to_owned()),
        },
    )
}

fn salt(h: &Harness) -> String {
    h.tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .take(12)
        .collect()
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

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(
        app,
        Request::builder()
            .method("GET")
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

async fn settle(app: &Router, tenant: &str, reference: &str) -> (StatusCode, Value) {
    send(
        app,
        Request::builder()
            .method("POST")
            .uri("/sites/domain-payments/settle")
            .header("x-alo-settlement", SETTLEMENT_SECRET)
            .header("content-type", "application/json")
            .body(Body::from(
                json!({ "tenant": tenant, "paymentReference": reference }).to_string(),
            ))
            .unwrap(),
    )
    .await
}

/// One website with one name bought, approved and paid for — the state the
/// sweep is supposed to find. Answers with (site, purchase id, domain).
async fn a_paid_purchase(h: &Harness, app: &Router, tag: &str) -> (SiteId, String, String) {
    let site = h
        .acc
        .create_site("Shop", &format!("{tag}{}", salt(h)))
        .await
        .unwrap();
    let domain = format!("{tag}{}.com", salt(h));
    let base = format!("/sites/{}/domain-purchases", site.as_str());
    let (status, purchase) = post(
        app,
        &h.token,
        &base,
        json!({
            "domain": domain,
            "years": 1,
            "requestKey": format!("{tag}-key-{}", salt(h)),
            "registrant": {
                "name": "Sanne de Vries",
                "organisation": "Acme BV",
                "email": "sanne@example.test",
                "street": "Keizersgracht 1",
                "postalCode": "1015 CJ",
                "city": "Amsterdam",
                "country": "nl",
                "phone": "+31201234567",
            },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{purchase}");
    let id = purchase["id"].as_str().unwrap().to_owned();

    let (status, approved) = post(
        app,
        &h.token,
        &format!("{base}/{id}/approve"),
        json!({ "agreed": {
            "domain": purchase["domain"],
            "termYears": purchase["termYears"],
            "currency": purchase["currency"],
            "firstTermCents": purchase["firstTermCents"],
            "renewalCentsPerYear": purchase["renewalCentsPerYear"],
            "premium": purchase["premium"],
        }}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{approved}");

    let reference = format!("pi_{tag}_{}", salt(h));
    let (status, awaiting) = post(
        app,
        &h.token,
        &format!("{base}/{id}/checkout"),
        json!({ "paymentReference": reference }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{awaiting}");

    let (status, paid) = settle(app, h.tenant.as_str(), &reference).await;
    assert_eq!(status, StatusCode::OK, "{paid}");
    assert_eq!(paid["state"], "paid");
    (site, id, domain)
}

async fn purchase_of(h: &Harness, app: &Router, site: &SiteId, id: &str) -> Value {
    let (status, body) = get(
        app,
        &h.token,
        &format!("/sites/{}/domain-purchases/{id}", site.as_str()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

#[tokio::test]
async fn a_paid_name_is_registered_and_attaches_itself_to_its_website() {
    let _sweeping = sweeping().await;
    let a = harness("domain-sweep-a").await;
    let b = harness_on(Arc::clone(&a.store), "domain-sweep-b").await;
    let fixture = Arc::new(FixtureRegistrar::new(FIXTURE_NOW).unwrap());
    let registrar = Arc::clone(&fixture) as Arc<dyn DomainRegistrar>;
    let app = app_with(&a, Arc::clone(&registrar));

    // Two tenants, both waiting — one sweep, and neither ends up holding the
    // other's name.
    let (a_site, a_id, a_domain) = a_paid_purchase(&a, &app, "swa").await;
    let (b_site, b_id, b_domain) = a_paid_purchase(&b, &app, "swb").await;

    // The sweep is global by design — it may well carry rows another suite
    // left behind — so what is asserted is what happened to *these* two.
    let live = alo_jmap::site_domain_worker::run_due(&a.store, &registrar).await;
    assert!(live >= 2, "both paid purchases went live");

    for (h, site, id, domain) in [
        (&a, &a_site, &a_id, &a_domain),
        (&b, &b_site, &b_id, &b_domain),
    ] {
        let purchase = purchase_of(h, &app, site, id).await;
        assert_eq!(purchase["state"], "configured", "{purchase}");
        assert_eq!(purchase["domain"], domain.as_str());
        assert!(!purchase["registeredAt"].is_null(), "{purchase}");
        assert!(!purchase["configuredAt"].is_null(), "{purchase}");
        assert!(!purchase["expiresAt"].is_null(), "{purchase}");
        assert_eq!(purchase["lifecycle"], "active");
        assert!(
            !purchase["providerReference"].as_str().unwrap().is_empty(),
            "the reseller's own identifier is kept: {purchase}"
        );
        assert!(purchase["failure"].is_null(), "{purchase}");

        // The name is attached and serving — no TXT proof, no second step.
        let domains = h.acc.site_domains(site).await.unwrap();
        assert_eq!(domains.len(), 1, "{domain} is the site's only claim");
        assert_eq!(domains[0].domain, *domain);
        assert_eq!(domains[0].status, SiteDomainStatus::Live);
        assert!(domains[0].verified_at.is_some());
    }

    // Each tenant's website holds only its own name.
    assert!(
        a.acc
            .site_domains(&a_site)
            .await
            .unwrap()
            .iter()
            .all(|d| d.domain != b_domain)
    );
    // And another tenant's site is not even a place to look.
    assert!(
        matches!(
            a.acc.site_domains(&b_site).await,
            Err(alo_store::StoreError::NotFound)
        ),
        "another tenant's site is no site at all"
    );

    // Nothing of theirs is left in the queue: a second sweep neither registers
    // them again nor touches the names it already attached.
    alo_jmap::site_domain_worker::run_due(&a.store, &registrar).await;
    for (h, site, id) in [(&a, &a_site, &a_id), (&b, &b_site, &b_id)] {
        let after = purchase_of(h, &app, site, id).await;
        assert_eq!(after["state"], "configured");
        assert_eq!(after["attempts"], 1, "one claim, one registration");
        assert_eq!(h.acc.site_domains(site).await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn a_name_taken_while_the_payment_was_going_through_ends_in_a_refund_sentence() {
    let _sweeping = sweeping().await;
    let h = harness("domain-sweep-gone").await;
    let fixture = Arc::new(FixtureRegistrar::new(FIXTURE_NOW).unwrap());
    let registrar = Arc::clone(&fixture) as Arc<dyn DomainRegistrar>;
    let app = app_with(&h, Arc::clone(&registrar));
    let (site, id, domain) = a_paid_purchase(&h, &app, "gone").await;

    // Somebody else registers it between the charge and the sweep.
    fixture.seed_taken(&domain).unwrap();

    alo_jmap::site_domain_worker::run_due(&h.store, &registrar).await;

    let purchase = purchase_of(&h, &app, &site, &id).await;
    assert_eq!(purchase["state"], "failed", "{purchase}");
    assert_eq!(purchase["moneyMoved"], false, "{purchase}");
    let failure = purchase["failure"].as_str().unwrap();
    assert!(failure.contains("refund"), "{failure}");
    // A refusal names the situation, never a person or their address.
    assert!(!failure.contains('@'), "{failure}");
    assert!(!failure.contains("Keizersgracht"), "{failure}");

    // Nothing was attached to the website on the way to that.
    assert!(h.acc.site_domains(&site).await.unwrap().is_empty());

    // Terminal: a second sweep does not pick it up again.
    alo_jmap::site_domain_worker::run_due(&h.store, &registrar).await;
    let again = purchase_of(&h, &app, &site, &id).await;
    assert_eq!(again["state"], "failed");
    assert_eq!(again["attempts"], purchase["attempts"]);
}

#[tokio::test]
async fn a_registrar_that_is_briefly_down_loses_nothing() {
    let _sweeping = sweeping().await;
    let h = harness("domain-sweep-flaky").await;
    let fixture = Arc::new(FixtureRegistrar::new(FIXTURE_NOW).unwrap());
    let flaky = Arc::new(FlakyRegistrar::new(Arc::clone(&fixture)));
    let registrar = Arc::clone(&flaky) as Arc<dyn DomainRegistrar>;
    let app = app_with(&h, Arc::clone(&registrar));
    let (site, id, domain) = a_paid_purchase(&h, &app, "flak").await;

    alo_jmap::site_domain_worker::run_due(&h.store, &registrar).await;
    let waiting = purchase_of(&h, &app, &site, &id).await;
    assert_eq!(
        waiting["state"], "paid",
        "a timeout puts the purchase back in the queue: {waiting}"
    );
    assert_eq!(waiting["moneyMoved"], true);
    assert_eq!(waiting["attempts"], 1);
    assert!(
        waiting["failure"]
            .as_str()
            .unwrap()
            .contains("did not answer"),
        "{waiting}"
    );
    assert!(h.acc.site_domains(&site).await.unwrap().is_empty());

    flaky.come_back_up();
    assert!(alo_jmap::site_domain_worker::run_due(&h.store, &registrar).await >= 1);
    let done = purchase_of(&h, &app, &site, &id).await;
    assert_eq!(done["state"], "configured", "{done}");
    assert!(
        done["failure"].is_null(),
        "a purchase that recovered says nothing about the outage: {done}"
    );
    let domains = h.acc.site_domains(&site).await.unwrap();
    assert_eq!(domains.len(), 1);
    assert_eq!(domains[0].domain, domain);
    assert_eq!(domains[0].status, SiteDomainStatus::Live);
}
