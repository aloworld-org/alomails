//! Buying a domain through alo (ADR 0036, S2.15b): the state machine that
//! carries a purchase from a price on a screen to a name serving a website.
//!
//! Five properties are load-bearing, and are proved here against a real
//! Postgres and the deterministic registrar that ships. **Nobody is charged a
//! price they did not see** — approval names the quote it approves, and a
//! purchase cannot skip approval on the way to a payment. **A retry never buys
//! twice** — the same request key returns the same purchase, and the registrar
//! call carries the purchase id as its idempotency key. **Money that moved is
//! not lost** — an interrupted registration is re-offered and then fails
//! visibly. **Billing stays behind its own door** — the payment reference is an
//! opaque string and no billing row is touched. And **tenant scope** — another
//! tenant's purchase cannot be read, approved, paid, cancelled, configured,
//! completed or failed, and is indistinguishable from one that never existed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::LazyLock;

use alo_store::{
    AccountStore, DomainLifecycle, DomainQuote, DomainRegistrar, FixtureRegistrar,
    NewSiteDomainPurchase, RegistrableDomain, RegistrantContact, SiteDomainPurchaseId,
    SiteDomainPurchaseKind, SiteDomainPurchaseState, SiteDomainStatus, SiteId, Store, StoreError,
    TenantId,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::{Duration, OffsetDateTime};
use tokio::sync::{Mutex, MutexGuard};

/// The registration sweep claims across tenants by design, so two tests
/// sweeping at once would steal each other's paid rows. Every test that sweeps
/// takes this first; the concurrency the suite actually exercises is inside a
/// single test.
static SWEEP: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

async fn sweeping() -> MutexGuard<'static, ()> {
    SWEEP.lock().await
}

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(detail)) => detail,
        other => panic!("expected Conflict, got {other:?}"),
    }
}

fn assert_validation<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(detail)) => detail,
        other => panic!("expected Validation, got {other:?}"),
    }
}

async fn raw_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap()
}

/// Backdates a claim so the sweep believes the worker holding it has died.
async fn age_claim(pool: &PgPool, purchase: &SiteDomainPurchaseId, minutes: i64) {
    sqlx::query(&format!(
        "UPDATE site_domain_purchases \
            SET claimed_at = now() - interval '{minutes} minutes' WHERE id = $1"
    ))
    .bind(purchase.as_str())
    .execute(pool)
    .await
    .unwrap();
}

fn subdomain(tag: &str) -> String {
    format!(
        "{tag}-{}",
        SiteId::generate()
            .as_str()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(12)
            .collect::<String>()
            .to_ascii_lowercase()
    )
}

/// A label nothing else in the shared database is buying, so the per-tenant
/// uniqueness rules are exercised by this test and not by yesterday's.
fn unique_label(tag: &str) -> String {
    format!(
        "{tag}{}",
        SiteId::generate()
            .as_str()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(10)
            .collect::<String>()
            .to_ascii_lowercase()
    )
}

fn registrant() -> RegistrantContact {
    RegistrantContact {
        name: "Ada Jansen".to_owned(),
        organisation: Some("Acme BV".to_owned()),
        email: "ada@acme.test".to_owned(),
        street: "Keizersgracht 1".to_owned(),
        postal_code: "1015 CJ".to_owned(),
        city: "Amsterdam".to_owned(),
        country: "nl".to_owned(),
        phone: "+31201234567".to_owned(),
    }
}

const NAMESERVERS: [&str; 2] = ["ns1.alosites.com", "ns2.alosites.com"];

fn nameservers() -> Vec<String> {
    NAMESERVERS.iter().map(|ns| (*ns).to_owned()).collect()
}

/// The registrar that ships, at a fixed moment so every expiry in this suite is
/// an exact date.
fn registrar() -> FixtureRegistrar {
    FixtureRegistrar::new(
        OffsetDateTime::from_unix_timestamp(1_770_000_000).expect("a fixed fixture clock"),
    )
    .expect("the shipped catalog builds")
}

/// A name this installation sells, parsed against the catalog that prices it.
fn buyable(registrar: &FixtureRegistrar, tag: &str) -> RegistrableDomain {
    registrar
        .catalog_ref()
        .parse(&format!("{}.nl", unique_label(tag)))
        .expect(".nl is on the shipped price list")
}

async fn quote_for(
    registrar: &FixtureRegistrar,
    domain: &RegistrableDomain,
    years: u8,
) -> DomainQuote {
    registrar
        .quote(domain.name().to_owned(), years)
        .await
        .expect("an available name is priced")
}

fn purchase_of(
    domain: &RegistrableDomain,
    quote: &DomainQuote,
    request_key: &str,
) -> NewSiteDomainPurchase {
    NewSiteDomainPurchase {
        kind: SiteDomainPurchaseKind::Registration,
        domain: domain.clone(),
        quote: quote.clone(),
        registrant: registrant(),
        nameservers: nameservers(),
        auto_renew: true,
        request_key: request_key.to_owned(),
    }
}

async fn site_for(account: &AccountStore, tag: &str) -> SiteId {
    account.create_site("Acme", &subdomain(tag)).await.unwrap()
}

/// The whole arc: a price, a person agreeing to it, a payment, a sweep that
/// registers the name at the registrar, and the domain attached to the website
/// and live — with the row telling the truth at every step.
#[tokio::test]
async fn a_domain_is_quoted_approved_paid_registered_and_connected() {
    let _sweeping = sweeping().await;
    let store = common::test_store().await;
    let tenant = store.create_tenant("site-domain-buy").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("owner@site-domain-buy.test")
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user.clone());
    let site = site_for(&account, "buy").await;
    let provider = registrar();
    let domain = buyable(&provider, "arc");
    let quote = quote_for(&provider, &domain, 2).await;

    // ---- quoted: a price, and nothing else -------------------------------
    let purchase = account
        .start_site_domain_purchase(&site, purchase_of(&domain, &quote, "buy-click-0001"))
        .await
        .unwrap();
    assert_eq!(purchase.state, SiteDomainPurchaseState::Quoted);
    assert_eq!(purchase.domain, domain.name());
    assert_eq!(purchase.tld, "nl");
    assert_eq!(purchase.term_years, 2);
    assert_eq!(purchase.first_term_cents, quote.first_term_cents);
    // The renewal price is stated from the very first row, never later.
    assert_eq!(
        purchase.renewal_cents_per_year,
        quote.renewal_cents_per_year
    );
    assert_eq!(purchase.currency, "EUR");
    assert_eq!(purchase.nameservers, nameservers());
    assert!(purchase.approved_at.is_none());
    assert!(purchase.payment_reference.is_none());
    // A quote alone connects nothing.
    assert!(account.site_domains(&site).await.unwrap().is_empty());

    // The registrant rests in the row and is reachable only deliberately.
    assert_eq!(
        account
            .site_domain_purchase_registrant(&purchase.id)
            .await
            .unwrap(),
        registrant()
    );

    // ---- approved: a named person agreed to this exact price -------------
    let approved = account
        .approve_site_domain_purchase(&purchase.id, &quote)
        .await
        .unwrap();
    assert_eq!(approved.state, SiteDomainPurchaseState::Approved);
    assert_eq!(approved.approved_by, Some(user.clone()));
    assert!(approved.approved_at.is_some());
    // Approving twice at the same price is a no-op, not a second approval.
    let again = account
        .approve_site_domain_purchase(&purchase.id, &quote)
        .await
        .unwrap();
    assert_eq!(again.approved_at, approved.approved_at);

    // ---- payment: an opaque reference from behind Billing's door ---------
    let awaiting = account
        .await_site_domain_payment(&purchase.id, "pi_2026_08_0001")
        .await
        .unwrap();
    assert_eq!(awaiting.state, SiteDomainPurchaseState::AwaitingPayment);
    assert_eq!(
        awaiting.payment_reference.as_deref(),
        Some("pi_2026_08_0001")
    );
    let paid = account
        .settle_site_domain_payment(&purchase.id, "pi_2026_08_0001")
        .await
        .unwrap();
    assert_eq!(paid.state, SiteDomainPurchaseState::Paid);
    assert!(paid.paid_at.is_some());
    // A webhook delivered twice settles nothing twice.
    let replayed = account
        .settle_site_domain_payment(&purchase.id, "pi_2026_08_0001")
        .await
        .unwrap();
    assert_eq!(replayed.paid_at, paid.paid_at);

    // ---- the sweep registers it ------------------------------------------
    let due = store.claim_site_domain_registrations(20).await.unwrap();
    let mine = due
        .into_iter()
        .find(|row| row.purchase == purchase.id)
        .expect("a paid purchase is offered to the sweep");
    assert_eq!(mine.tenant, tenant);
    assert_eq!(mine.site, site);
    assert_eq!(mine.kind, SiteDomainPurchaseKind::Registration);
    assert_eq!(mine.attempts, 1);
    // The order carries the registrant to the registrar and the purchase id as
    // its replay token — the two things that make the call safe to repeat.
    assert_eq!(mine.order.registrant, registrant());
    assert_eq!(mine.order.idempotency_key, purchase.id.as_str());
    assert_eq!(mine.order.years, 2);
    assert_eq!(
        account
            .site_domain_purchase(&purchase.id)
            .await
            .unwrap()
            .state,
        SiteDomainPurchaseState::Registering
    );

    let registered = provider.register(mine.order.clone()).await.unwrap();
    store
        .complete_site_domain_registration(&tenant, &purchase.id, &registered)
        .await
        .unwrap();
    let after = account.site_domain_purchase(&purchase.id).await.unwrap();
    assert_eq!(after.state, SiteDomainPurchaseState::Registered);
    assert_eq!(
        after.provider_reference,
        Some(registered.provider_reference.clone())
    );
    assert_eq!(after.expires_at, Some(registered.expires_at));
    assert_eq!(after.lifecycle, Some(DomainLifecycle::Active));
    assert!(after.failure.is_none());

    // A repeat of the registrar call under the same key is one registration.
    let replay = provider.register(mine.order).await.unwrap();
    assert_eq!(replay, registered);

    // ---- configured: the name is attached to the website -----------------
    let configured = account
        .configure_site_domain_purchase(&purchase.id)
        .await
        .unwrap();
    assert_eq!(configured.state, SiteDomainPurchaseState::Configured);
    assert!(configured.configured_at.is_some());
    let connected = account.site_domains(&site).await.unwrap();
    assert_eq!(connected.len(), 1);
    assert_eq!(connected[0].domain, domain.name());
    // A name alo bought on alo's nameservers needs no TXT proof: it is live.
    assert_eq!(connected[0].status, SiteDomainStatus::Live);
    // Idempotent — a second click does not connect it twice.
    account
        .configure_site_domain_purchase(&purchase.id)
        .await
        .unwrap();
    assert_eq!(account.site_domains(&site).await.unwrap().len(), 1);

    // The list reads the whole thing back, newest first.
    let listed = account.site_domain_purchases(&site, 50).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].state, SiteDomainPurchaseState::Configured);
}

/// The same buy click, twice, is one purchase; a different one wearing the same
/// token is refused rather than quietly buying a second name.
#[tokio::test]
async fn a_replayed_buy_click_reaches_the_purchase_it_already_made() {
    let store = common::test_store().await;
    let (account, _user, _inbox) = common::fresh_account(&store, "domain-replay").await;
    let site = site_for(&account, "replay").await;
    let provider = registrar();
    let domain = buyable(&provider, "replay");
    let quote = quote_for(&provider, &domain, 1).await;

    let first = account
        .start_site_domain_purchase(&site, purchase_of(&domain, &quote, "double-click-01"))
        .await
        .unwrap();
    let second = account
        .start_site_domain_purchase(&site, purchase_of(&domain, &quote, "double-click-01"))
        .await
        .unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(
        account
            .site_domain_purchases(&site, 50)
            .await
            .unwrap()
            .len(),
        1
    );

    // Same token, different name: a bug in the caller, refused loudly.
    let other = buyable(&provider, "other");
    let other_quote = quote_for(&provider, &other, 1).await;
    let detail = assert_conflict(
        account
            .start_site_domain_purchase(&site, purchase_of(&other, &other_quote, "double-click-01"))
            .await,
    );
    assert!(detail.contains("different purchase"), "{detail}");

    // Same token, different term — and therefore a different price.
    let longer = quote_for(&provider, &domain, 3).await;
    assert_conflict(
        account
            .start_site_domain_purchase(&site, purchase_of(&domain, &longer, "double-click-01"))
            .await,
    );

    // A fresh token for a name already being bought is a conflict, not a
    // second charge for the same domain.
    let detail = assert_conflict(
        account
            .start_site_domain_purchase(&site, purchase_of(&domain, &quote, "second-attempt-1"))
            .await,
    );
    assert!(detail.contains("already buying"), "{detail}");

    // Calling it off releases the name for another attempt.
    account
        .cancel_site_domain_purchase(&first.id)
        .await
        .unwrap();
    let retried = account
        .start_site_domain_purchase(&site, purchase_of(&domain, &quote, "second-attempt-1"))
        .await
        .unwrap();
    assert_ne!(retried.id, first.id);
    assert_eq!(retried.state, SiteDomainPurchaseState::Quoted);
}

/// Nothing is charged at a price nobody saw, and nothing skips a step.
#[tokio::test]
async fn the_order_of_the_states_is_the_only_order() {
    let store = common::test_store().await;
    let (account, _user, _inbox) = common::fresh_account(&store, "domain-order").await;
    let site = site_for(&account, "order").await;
    let provider = registrar();
    let domain = buyable(&provider, "order");
    let quote = quote_for(&provider, &domain, 1).await;
    let purchase = account
        .start_site_domain_purchase(&site, purchase_of(&domain, &quote, "order-key-0001"))
        .await
        .unwrap();

    // A payment before anybody approved the price.
    let detail = assert_conflict(
        account
            .await_site_domain_payment(&purchase.id, "pi_too_early")
            .await,
    );
    assert!(detail.contains("approved"), "{detail}");

    // A price that moved between the screen and the approval.
    let moved = DomainQuote {
        first_term_cents: quote.first_term_cents + 500,
        ..quote.clone()
    };
    let detail = assert_conflict(
        account
            .approve_site_domain_purchase(&purchase.id, &moved)
            .await,
    );
    assert!(detail.contains("price"), "{detail}");
    // …and a renewal price that moved, which is the half a bait price hides in.
    let baited = DomainQuote {
        renewal_cents_per_year: quote.renewal_cents_per_year + 900,
        ..quote.clone()
    };
    assert_conflict(
        account
            .approve_site_domain_purchase(&purchase.id, &baited)
            .await,
    );
    assert_eq!(
        account
            .site_domain_purchase(&purchase.id)
            .await
            .unwrap()
            .state,
        SiteDomainPurchaseState::Quoted
    );

    account
        .approve_site_domain_purchase(&purchase.id, &quote)
        .await
        .unwrap();
    // Settling a payment that was never asked for.
    assert_conflict(
        account
            .settle_site_domain_payment(&purchase.id, "pi_invented")
            .await,
    );
    // A malformed reference is the caller's mistake, before any state moves.
    let detail = assert_validation(account.await_site_domain_payment(&purchase.id, "  ").await);
    assert!(detail.contains("payment reference"), "{detail}");

    account
        .await_site_domain_payment(&purchase.id, "pi_real_0001")
        .await
        .unwrap();
    // A second, different payment for one purchase.
    assert_conflict(
        account
            .await_site_domain_payment(&purchase.id, "pi_real_0002")
            .await,
    );
    // Somebody else's payment settling this purchase.
    let detail = assert_conflict(
        account
            .settle_site_domain_payment(&purchase.id, "pi_real_0002")
            .await,
    );
    assert!(detail.contains("does not belong"), "{detail}");
    // Configuring a name nobody has registered.
    assert_conflict(account.configure_site_domain_purchase(&purchase.id).await);

    // Cancelling is possible right up to the payment, and not after it.
    account
        .settle_site_domain_payment(&purchase.id, "pi_real_0001")
        .await
        .unwrap();
    let detail = assert_conflict(account.cancel_site_domain_purchase(&purchase.id).await);
    assert!(detail.contains("paid"), "{detail}");

    // One payment settles exactly one purchase, tenant-wide.
    let second = buyable(&provider, "second");
    let second_quote = quote_for(&provider, &second, 1).await;
    let other = account
        .start_site_domain_purchase(&site, purchase_of(&second, &second_quote, "order-key-0002"))
        .await
        .unwrap();
    account
        .approve_site_domain_purchase(&other.id, &second_quote)
        .await
        .unwrap();
    let detail = assert_conflict(
        account
            .await_site_domain_payment(&other.id, "pi_real_0001")
            .await,
    );
    assert!(detail.contains("another domain purchase"), "{detail}");

    // Cancelling before the money moves is allowed, and repeatable.
    let cancelled = account
        .cancel_site_domain_purchase(&other.id)
        .await
        .unwrap();
    assert_eq!(cancelled.state, SiteDomainPurchaseState::Cancelled);
    assert_eq!(
        account
            .cancel_site_domain_purchase(&other.id)
            .await
            .unwrap()
            .state,
        SiteDomainPurchaseState::Cancelled
    );
    // A cancelled purchase cannot be revived by approving it again.
    assert_conflict(
        account
            .approve_site_domain_purchase(&other.id, &second_quote)
            .await,
    );
}

/// A renewal extends a name this website actually has, and nothing else.
#[tokio::test]
async fn a_renewal_needs_a_domain_we_already_manage() {
    let _sweeping = sweeping().await;
    let store = common::test_store().await;
    let tenant = store.create_tenant("site-domain-renew").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("owner@site-domain-renew.test")
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    let site = site_for(&account, "renew").await;
    let provider = registrar();
    let domain = buyable(&provider, "renew");
    let quote = quote_for(&provider, &domain, 1).await;
    let renewal = NewSiteDomainPurchase {
        kind: SiteDomainPurchaseKind::Renewal,
        ..purchase_of(&domain, &quote, "renew-key-00001")
    };

    let detail = assert_validation(
        account
            .start_site_domain_purchase(&site, renewal.clone())
            .await,
    );
    assert!(detail.contains("nothing to renew"), "{detail}");

    // Buy it first, the whole way through, so the website manages it.
    let bought = account
        .start_site_domain_purchase(&site, purchase_of(&domain, &quote, "renew-buy-000001"))
        .await
        .unwrap();
    account
        .approve_site_domain_purchase(&bought.id, &quote)
        .await
        .unwrap();
    account
        .await_site_domain_payment(&bought.id, "pi_renew_first")
        .await
        .unwrap();
    account
        .settle_site_domain_payment(&bought.id, "pi_renew_first")
        .await
        .unwrap();
    let due = store.claim_site_domain_registrations(20).await.unwrap();
    let mine = due
        .into_iter()
        .find(|row| row.purchase == bought.id)
        .expect("the paid registration is claimable");
    let registered = provider.register(mine.order).await.unwrap();
    store
        .complete_site_domain_registration(&tenant, &bought.id, &registered)
        .await
        .unwrap();
    account
        .configure_site_domain_purchase(&bought.id)
        .await
        .unwrap();

    // Now the renewal is a real thing to sell.
    let renewing = account
        .start_site_domain_purchase(&site, renewal)
        .await
        .unwrap();
    assert_eq!(renewing.kind, SiteDomainPurchaseKind::Renewal);
    assert_eq!(renewing.state, SiteDomainPurchaseState::Quoted);
    // …and it does not collide with the registration that bought the name.
    assert_eq!(
        account
            .site_domain_purchases(&site, 50)
            .await
            .unwrap()
            .len(),
        2
    );
}

/// A worker that dies mid-registration is retried, bounded, and then the row
/// says so — with the money it cost still visible on it.
#[tokio::test]
async fn an_interrupted_registration_is_retried_and_then_fails_visibly() {
    let _sweeping = sweeping().await;
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let tenant = store.create_tenant("site-domain-stall").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("owner@site-domain-stall.test")
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    let site = site_for(&account, "stall").await;
    let provider = registrar();
    let domain = buyable(&provider, "stall");
    let quote = quote_for(&provider, &domain, 1).await;
    let purchase = account
        .start_site_domain_purchase(&site, purchase_of(&domain, &quote, "stall-key-00001"))
        .await
        .unwrap();
    account
        .approve_site_domain_purchase(&purchase.id, &quote)
        .await
        .unwrap();
    account
        .await_site_domain_payment(&purchase.id, "pi_stall_0001")
        .await
        .unwrap();
    account
        .settle_site_domain_payment(&purchase.id, "pi_stall_0001")
        .await
        .unwrap();

    // Two sweepers at the same instant: one claim, not two registrations.
    let (left, right) = tokio::join!(
        store.claim_site_domain_registrations(20),
        store.claim_site_domain_registrations(20)
    );
    let claims = left
        .unwrap()
        .into_iter()
        .chain(right.unwrap())
        .filter(|row| row.purchase == purchase.id)
        .count();
    assert_eq!(claims, 1, "one paid purchase was claimed twice");

    // A retryable fault puts it back in the queue, with the reason visible.
    let state = store
        .retry_site_domain_registration(&tenant, &purchase.id, "the registry did not answer")
        .await
        .unwrap();
    assert_eq!(state, SiteDomainPurchaseState::Paid);
    let back = account.site_domain_purchase(&purchase.id).await.unwrap();
    assert_eq!(back.failure.as_deref(), Some("the registry did not answer"));
    assert_eq!(back.attempts, 1);

    // Burn the remaining attempts the way a dying worker would: claim, then
    // never come back.
    for _ in 1..alo_store::SITE_DOMAIN_PURCHASE_MAX_ATTEMPTS {
        let due = store.claim_site_domain_registrations(20).await.unwrap();
        assert!(due.iter().any(|row| row.purchase == purchase.id));
        age_claim(
            &pool,
            &purchase.id,
            i64::from(alo_store::SITE_DOMAIN_PURCHASE_CLAIM_STALE_MINUTES) + 1,
        )
        .await;
    }
    let attempts = account
        .site_domain_purchase(&purchase.id)
        .await
        .unwrap()
        .attempts;
    assert_eq!(attempts, alo_store::SITE_DOMAIN_PURCHASE_MAX_ATTEMPTS);

    // The next sweep writes it off instead of offering it a sixth time.
    let due = store.claim_site_domain_registrations(20).await.unwrap();
    assert!(!due.iter().any(|row| row.purchase == purchase.id));
    let dead = account.site_domain_purchase(&purchase.id).await.unwrap();
    assert_eq!(dead.state, SiteDomainPurchaseState::Failed);
    assert_eq!(
        dead.failure.as_deref(),
        Some(alo_store::SITE_DOMAIN_PURCHASE_INTERRUPTED)
    );
    // The money that moved is still on the row for the refund conversation.
    assert_eq!(dead.payment_reference.as_deref(), Some("pi_stall_0001"));
    assert!(dead.paid_at.is_some());
    // Nothing was connected to the website.
    assert!(account.site_domains(&site).await.unwrap().is_empty());
}

/// A registry that refuses is terminal: the reason is recorded in words the
/// tenant can act on, and no retry circles on it.
#[tokio::test]
async fn a_refused_registration_stops_and_says_why() {
    let _sweeping = sweeping().await;
    let store = common::test_store().await;
    let tenant = store.create_tenant("site-domain-refuse").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("owner@site-domain-refuse.test")
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    let site = site_for(&account, "refuse").await;
    let provider = registrar();
    let domain = buyable(&provider, "refuse");
    let quote = quote_for(&provider, &domain, 1).await;
    let purchase = account
        .start_site_domain_purchase(&site, purchase_of(&domain, &quote, "refuse-key-0001"))
        .await
        .unwrap();
    account
        .approve_site_domain_purchase(&purchase.id, &quote)
        .await
        .unwrap();
    account
        .await_site_domain_payment(&purchase.id, "pi_refuse_001")
        .await
        .unwrap();
    account
        .settle_site_domain_payment(&purchase.id, "pi_refuse_001")
        .await
        .unwrap();

    // Somebody else takes the name between the search and the till.
    provider.seed_taken(domain.name()).unwrap();
    let due = store.claim_site_domain_registrations(20).await.unwrap();
    let mine = due
        .into_iter()
        .find(|row| row.purchase == purchase.id)
        .expect("the paid registration is claimable");
    let refusal = provider
        .register(mine.order)
        .await
        .expect_err("a taken name cannot be registered");
    store
        .fail_site_domain_registration(&tenant, &purchase.id, &refusal.to_string())
        .await
        .unwrap();
    let failed = account.site_domain_purchase(&purchase.id).await.unwrap();
    assert_eq!(failed.state, SiteDomainPurchaseState::Failed);
    assert_eq!(
        failed.failure.as_deref(),
        Some("that domain is not available")
    );
    // Terminal: the sweep does not pick it up again.
    let due = store.claim_site_domain_registrations(20).await.unwrap();
    assert!(!due.iter().any(|row| row.purchase == purchase.id));
    // And a worker reporting on a row it no longer holds is refused.
    assert_not_found(
        store
            .fail_site_domain_registration(&tenant, &purchase.id, "again")
            .await,
    );
}

/// Billing's side of the seam: the reference is opaque, stored verbatim, and
/// buying a domain writes nothing into any billing table.
#[tokio::test]
async fn the_payment_reference_is_opaque_and_billing_is_untouched() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let tenant = store.create_tenant("site-domain-billing").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("owner@site-domain-billing.test")
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    let site = site_for(&account, "billing").await;
    let provider = registrar();
    let domain = buyable(&provider, "billing");
    let quote = quote_for(&provider, &domain, 1).await;
    let purchase = account
        .start_site_domain_purchase(&site, purchase_of(&domain, &quote, "billing-key-001"))
        .await
        .unwrap();
    account
        .approve_site_domain_purchase(&purchase.id, &quote)
        .await
        .unwrap();

    // Whatever shape the charging system mints, we store and never parse it.
    let reference = "mollie/tr_7UhSN1zuXS/2026-08";
    let awaiting = account
        .await_site_domain_payment(&purchase.id, reference)
        .await
        .unwrap();
    assert_eq!(awaiting.payment_reference.as_deref(), Some(reference));

    for table in ["billing_invoices", "billing_customers", "billing_payments"] {
        let rows: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {table} WHERE tenant_id = $1"
        ))
        .bind(tenant.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(rows, 0, "buying a domain wrote into {table}");
    }
}

/// Tenant scope. Another tenant's purchase cannot be read, listed, approved,
/// paid, settled, cancelled, configured, completed, retried or failed — and
/// every refusal is the same answer a purchase that never existed gets.
#[tokio::test]
async fn another_tenant_can_neither_see_nor_move_this_purchase() {
    let _sweeping = sweeping().await;
    let store = common::test_store().await;
    let (mine, _user, _inbox) = common::fresh_account(&store, "domain-mine").await;
    let (theirs, _their_user, _their_inbox) = common::fresh_account(&store, "domain-theirs").await;
    let my_site = site_for(&mine, "mine").await;
    let their_site = site_for(&theirs, "theirs").await;
    let provider = registrar();
    let domain = buyable(&provider, "tenancy");
    let quote = quote_for(&provider, &domain, 1).await;
    let purchase = mine
        .start_site_domain_purchase(&my_site, purchase_of(&domain, &quote, "tenancy-key-001"))
        .await
        .unwrap();

    // Reads.
    assert_not_found(theirs.site_domain_purchase(&purchase.id).await);
    assert_not_found(theirs.site_domain_purchase_registrant(&purchase.id).await);
    assert!(
        theirs
            .site_domain_purchases(&my_site, 50)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        theirs
            .site_domain_purchases(&their_site, 50)
            .await
            .unwrap()
            .is_empty()
    );
    // Writing to my site through their door does not reach my site either.
    assert_not_found(
        theirs
            .start_site_domain_purchase(&my_site, purchase_of(&domain, &quote, "tenancy-key-002"))
            .await,
    );

    // Every state change, through the wrong door.
    assert_not_found(
        theirs
            .approve_site_domain_purchase(&purchase.id, &quote)
            .await,
    );
    assert_not_found(
        theirs
            .await_site_domain_payment(&purchase.id, "pi_theirs")
            .await,
    );
    assert_not_found(
        theirs
            .settle_site_domain_payment(&purchase.id, "pi_theirs")
            .await,
    );
    assert_not_found(theirs.cancel_site_domain_purchase(&purchase.id).await);
    assert_not_found(theirs.configure_site_domain_purchase(&purchase.id).await);

    // The same name is not blocked for the other tenant: the uniqueness rule
    // is per tenant, and one tenant's shopping is invisible to another's.
    let theirs_too = theirs
        .start_site_domain_purchase(&their_site, purchase_of(&domain, &quote, "tenancy-key-001"))
        .await
        .unwrap();
    assert_ne!(theirs_too.id, purchase.id);

    // The system-level sweep answers are tenant-anchored as well: a claimed
    // row cannot be completed, retried or failed under the wrong tenant.
    mine.approve_site_domain_purchase(&purchase.id, &quote)
        .await
        .unwrap();
    mine.await_site_domain_payment(&purchase.id, "pi_mine_0001")
        .await
        .unwrap();
    mine.settle_site_domain_payment(&purchase.id, "pi_mine_0001")
        .await
        .unwrap();
    let due = store.claim_site_domain_registrations(20).await.unwrap();
    let claimed = due
        .into_iter()
        .find(|row| row.purchase == purchase.id)
        .expect("my paid purchase is claimable");
    let stranger = TenantId::new(format!("{}x", claimed.tenant.as_str()));
    let registered = provider.register(claimed.order).await.unwrap();
    assert_not_found(
        store
            .complete_site_domain_registration(&stranger, &purchase.id, &registered)
            .await,
    );
    assert_not_found(
        store
            .retry_site_domain_registration(&stranger, &purchase.id, "not yours")
            .await,
    );
    assert_not_found(
        store
            .fail_site_domain_registration(&stranger, &purchase.id, "not yours")
            .await,
    );
    // …and my own purchase is untouched by all of that.
    assert_eq!(
        mine.site_domain_purchase(&purchase.id).await.unwrap().state,
        SiteDomainPurchaseState::Registering
    );

    // Finish mine, and prove their site got nothing connected to it.
    store
        .complete_site_domain_registration(&claimed.tenant, &purchase.id, &registered)
        .await
        .unwrap();
    mine.configure_site_domain_purchase(&purchase.id)
        .await
        .unwrap();
    assert_eq!(mine.site_domains(&my_site).await.unwrap().len(), 1);
    assert!(theirs.site_domains(&their_site).await.unwrap().is_empty());

    // The name is a single deployment-wide namespace: the other tenant's
    // purchase of the same string cannot connect it away from mine.
    theirs
        .approve_site_domain_purchase(&theirs_too.id, &quote)
        .await
        .unwrap();
    theirs
        .await_site_domain_payment(&theirs_too.id, "pi_theirs_001")
        .await
        .unwrap();
    theirs
        .settle_site_domain_payment(&theirs_too.id, "pi_theirs_001")
        .await
        .unwrap();
    let due = store.claim_site_domain_registrations(20).await.unwrap();
    let theirs_claim = due
        .into_iter()
        .find(|row| row.purchase == theirs_too.id)
        .expect("their paid purchase is claimable");
    store
        .complete_site_domain_registration(&theirs_claim.tenant, &theirs_too.id, &registered)
        .await
        .unwrap();
    let detail = assert_conflict(theirs.configure_site_domain_purchase(&theirs_too.id).await);
    assert!(detail.contains("another website"), "{detail}");
}

/// A purchase that never existed and one belonging to somebody else are the
/// same answer, from every door.
#[tokio::test]
async fn an_unknown_purchase_is_simply_not_found() {
    let store = common::test_store().await;
    let (account, _user, _inbox) = common::fresh_account(&store, "domain-unknown").await;
    let ghost = SiteDomainPurchaseId::generate();
    assert_not_found(account.site_domain_purchase(&ghost).await);
    assert_not_found(account.site_domain_purchase_registrant(&ghost).await);
    assert_not_found(account.cancel_site_domain_purchase(&ghost).await);
    assert_not_found(account.configure_site_domain_purchase(&ghost).await);
    // …including a purchase against a website that is not this tenant's.
    let provider = registrar();
    let domain = buyable(&provider, "ghost");
    let quote = quote_for(&provider, &domain, 1).await;
    assert_not_found(
        account
            .start_site_domain_purchase(
                &SiteId::generate(),
                purchase_of(&domain, &quote, "ghost-key-000001"),
            )
            .await,
    );
}

/// The store refuses an order the registrar model would refuse, before a row
/// exists — and names the rule without quoting the registrant back.
#[tokio::test]
async fn a_malformed_order_never_becomes_a_row() {
    let store = common::test_store().await;
    let (account, _user, _inbox) = common::fresh_account(&store, "domain-malformed").await;
    let site = site_for(&account, "malformed").await;
    let provider = registrar();
    let domain = buyable(&provider, "malformed");
    let quote = quote_for(&provider, &domain, 1).await;

    // A replay token nobody could have minted.
    let detail = assert_validation(
        account
            .start_site_domain_purchase(&site, purchase_of(&domain, &quote, "short"))
            .await,
    );
    assert!(detail.contains("idempotency key"), "{detail}");

    // One nameserver: every registry wants two.
    let thin = NewSiteDomainPurchase {
        nameservers: vec![NAMESERVERS[0].to_owned()],
        ..purchase_of(&domain, &quote, "malformed-key-01")
    };
    assert_validation(account.start_site_domain_purchase(&site, thin).await);

    // A registrant a registry would reject, named by field and not by value.
    let secret = "ada.private@example.test";
    let bad_contact = NewSiteDomainPurchase {
        registrant: RegistrantContact {
            phone: "0201234567".to_owned(),
            email: secret.to_owned(),
            ..registrant()
        },
        ..purchase_of(&domain, &quote, "malformed-key-02")
    };
    let detail = assert_validation(account.start_site_domain_purchase(&site, bad_contact).await);
    assert!(detail.contains("telephone"), "{detail}");
    assert!(
        !detail.contains(secret),
        "the refusal quoted the registrant"
    );

    // A price for a different name than the one being bought.
    let other = buyable(&provider, "elsewhere");
    let mismatched = NewSiteDomainPurchase {
        quote: quote_for(&provider, &other, 1).await,
        ..purchase_of(&domain, &quote, "malformed-key-03")
    };
    let detail = assert_validation(account.start_site_domain_purchase(&site, mismatched).await);
    assert!(detail.contains("different domain"), "{detail}");

    assert!(
        account
            .site_domain_purchases(&site, 50)
            .await
            .unwrap()
            .is_empty(),
        "a refused order left a row behind"
    );
}

/// The list is bounded and newest-first, so a screen cannot be flooded by a
/// tenant's own history.
#[tokio::test]
async fn the_list_is_bounded_and_newest_first() {
    let store = common::test_store().await;
    let (account, _user, _inbox) = common::fresh_account(&store, "domain-list").await;
    let site = site_for(&account, "list").await;
    let provider = registrar();
    let mut made = Vec::new();
    for index in 0..3 {
        let domain = buyable(&provider, &format!("list{index}"));
        let quote = quote_for(&provider, &domain, 1).await;
        made.push(
            account
                .start_site_domain_purchase(
                    &site,
                    purchase_of(&domain, &quote, &format!("list-key-{index:08}")),
                )
                .await
                .unwrap(),
        );
        // The rows are ordered by creation time; keep them distinguishable.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let listed = account.site_domain_purchases(&site, 2).await.unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].id, made[2].id);
    assert_eq!(listed[1].id, made[1].id);
    // A caller asking for more than the ceiling gets the ceiling, not an error.
    assert_eq!(
        account
            .site_domain_purchases(&site, 10_000)
            .await
            .unwrap()
            .len(),
        3
    );
}

/// A stored purchase reads back with the same expiry the registry counted, so
/// the renewal date on a screen is the registry's date and not ours.
#[tokio::test]
async fn the_registry_expiry_is_what_is_stored() {
    let _sweeping = sweeping().await;
    let store = common::test_store().await;
    let tenant = store.create_tenant("site-domain-expiry").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("owner@site-domain-expiry.test")
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    let site = site_for(&account, "expiry").await;
    let provider = registrar();
    let domain = buyable(&provider, "expiry");
    let quote = quote_for(&provider, &domain, 3).await;
    let purchase = account
        .start_site_domain_purchase(&site, purchase_of(&domain, &quote, "expiry-key-0001"))
        .await
        .unwrap();
    account
        .approve_site_domain_purchase(&purchase.id, &quote)
        .await
        .unwrap();
    account
        .await_site_domain_payment(&purchase.id, "pi_expiry_001")
        .await
        .unwrap();
    account
        .settle_site_domain_payment(&purchase.id, "pi_expiry_001")
        .await
        .unwrap();
    let due = store.claim_site_domain_registrations(20).await.unwrap();
    let mine = due
        .into_iter()
        .find(|row| row.purchase == purchase.id)
        .expect("the paid registration is claimable");
    let registered = provider.register(mine.order).await.unwrap();
    store
        .complete_site_domain_registration(&tenant, &purchase.id, &registered)
        .await
        .unwrap();

    let stored = account.site_domain_purchase(&purchase.id).await.unwrap();
    assert_eq!(
        stored.expires_at,
        Some(provider.now() + Duration::days(3 * 365))
    );
    assert!(stored.auto_renew);
    assert_eq!(stored.term_years, 3);
    assert_eq!(stored.first_term_cents, quote.renewal_cents_per_year * 3);
}

/// The store is the only thing that decides what a purchase costs to keep: the
/// quote it was created with is the quote it keeps, whatever a later search
/// says.
#[tokio::test]
async fn a_stored_quote_does_not_drift() {
    let store: Store = common::test_store().await;
    let (account, _user, _inbox) = common::fresh_account(&store, "domain-drift").await;
    let site = site_for(&account, "drift").await;
    let provider = registrar();
    let domain = buyable(&provider, "drift");
    let quote = quote_for(&provider, &domain, 1).await;
    let purchase = account
        .start_site_domain_purchase(&site, purchase_of(&domain, &quote, "drift-key-00001"))
        .await
        .unwrap();

    // The registry decides, later, that this is a premium name.
    provider.seed_premium(domain.name(), 250_000).unwrap();
    let dearer = quote_for(&provider, &domain, 1).await;
    assert!(dearer.first_term_cents > quote.first_term_cents);

    // The row still says what the buyer was shown…
    let stored = account.site_domain_purchase(&purchase.id).await.unwrap();
    assert_eq!(stored.first_term_cents, quote.first_term_cents);
    // …and approving at the new price is refused, because nobody saw it.
    assert_conflict(
        account
            .approve_site_domain_purchase(&purchase.id, &dearer)
            .await,
    );
}

/// The two system-level lookups the payment bridge and the registration sweep
/// reach a purchase by (S2.15c2): the person who approved a price, and the
/// purchase one payment is settling. Both are doors a caller holds no tenant
/// token for, so both are tenant-checked exactly as hard as the account ones.
#[tokio::test]
async fn the_machine_doors_find_a_purchase_only_inside_its_own_tenant() {
    let store = common::test_store().await;
    let (account, user, _inbox) = common::fresh_account(&store, "domain-machine").await;
    let tenant = account.tenant().clone();
    let site = site_for(&account, "machine").await;
    let provider = registrar();
    let domain = buyable(&provider, "mach");
    let quote = quote_for(&provider, &domain, 1).await;
    let purchase = account
        .start_site_domain_purchase(&site, purchase_of(&domain, &quote, "machine-key-001"))
        .await
        .unwrap();

    // Nobody has approved anything, so there is no one to act as — which is
    // exactly the situation in which acting would be wrong.
    assert_not_found(
        store
            .site_domain_purchase_approver(&tenant, &purchase.id)
            .await,
    );
    account
        .approve_site_domain_purchase(&purchase.id, &quote)
        .await
        .unwrap();
    assert_eq!(
        store
            .site_domain_purchase_approver(&tenant, &purchase.id)
            .await
            .unwrap(),
        user
    );

    // A payment nobody is waiting for names no purchase; a reference that is
    // not one is refused before any row is read.
    assert_not_found(
        store
            .site_domain_purchase_awaiting_payment(&tenant, "pi_never_minted")
            .await,
    );
    assert_validation(
        store
            .site_domain_purchase_awaiting_payment(&tenant, "  ")
            .await,
    );

    let reference = "pi_machine_0001";
    account
        .await_site_domain_payment(&purchase.id, reference)
        .await
        .unwrap();
    let found = store
        .site_domain_purchase_awaiting_payment(&tenant, reference)
        .await
        .unwrap();
    assert_eq!(found.id, purchase.id);
    assert_eq!(found.state, SiteDomainPurchaseState::AwaitingPayment);

    // The tenant is the boundary on both, not a hint: another tenant's
    // settlement reaches nothing, and neither does another tenant's sweep.
    let stranger = TenantId::new(format!("{}x", tenant.as_str()));
    assert_not_found(
        store
            .site_domain_purchase_awaiting_payment(&stranger, reference)
            .await,
    );
    assert_not_found(
        store
            .site_domain_purchase_approver(&stranger, &purchase.id)
            .await,
    );
    // …and the purchase is exactly as it was.
    assert_eq!(
        account
            .site_domain_purchase(&purchase.id)
            .await
            .unwrap()
            .state,
        SiteDomainPurchaseState::AwaitingPayment
    );
}
