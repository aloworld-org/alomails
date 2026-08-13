//! Buying a domain name from inside alo Sites (ADR 0036, S2.15c): the
//! authenticated `/sites/domain-*` price surface and the
//! `/sites/{id}/domain-purchases*` record of one tenant's purchases.
//!
//! A separate module from [`crate::sites`]' custom-domain claims for a
//! separate reason to change: those routes attach a name the tenant already
//! owns elsewhere, these ones spend money to acquire one. The storage arc —
//! quote → approval → payment → registration → configuration — is
//! `alo_store::site_domain_purchases`; this is its edge.
//!
//! Three rules shape every handler here.
//!
//! **The price is never posted.** A create request names a domain and a term;
//! what it costs is asked of the registrar in the same request and stored from
//! that answer. A client cannot propose a price, so no client bug and no
//! tampered request can put a number on a purchase that the seller did not
//! state. Approval, conversely, *must* echo the six numbers the buyer had on
//! screen — the store refuses an approval whose quote has drifted, which is
//! the honest-pricing promise (`docs/features.md` → alo Sites) enforced at the
//! point of charge rather than in a policy page.
//!
//! **Buying is the site owner's, not the site editor's.** A site-editor
//! collaborator (S2.03a) may write the website and may not spend the tenant's
//! money or read a registrant's home address, so this whole surface is behind
//! [`require_site_manager`](crate::sites::require_site_manager) — the same
//! guard collaborator management uses.
//!
//! **An unconfigured deployment says so.** With no registrar wired (the
//! default: [`UnconfiguredRegistrar`]) or no nameservers configured, every
//! door answers `503` with `{"reason":"unconfigured"}` — the typed shape the
//! AI paths established (S1.28a) and the buy box branches on, rather than a
//! screen that fails at the price.
//!
//! # The payment handoff (S2.15c2)
//!
//! Two doors, deliberately on opposite sides of the money.
//!
//! [`checkout_purchase`] is the tenant's: it records the opaque reference
//! whatever charges them minted, and moves an approved purchase to
//! `awaiting_payment`. Recording a reference charges nobody, so it sits behind
//! the same site-owner guard as the rest of this surface.
//!
//! [`settle_payment`] is **not** the tenant's. "The money arrived" is the one
//! statement a buyer may not make about their own purchase — it is what puts
//! the purchase in the registration sweep's queue, and a tenant who could say
//! it would register domains nobody paid for. So it is a machine door: no user
//! token, a deployment secret in `X-Alo-Settlement` instead
//! ([`SiteDomainCommerce::settlement_secret`], from
//! `SITE_PAYMENT_SETTLEMENT_SECRET`), and the payment reference — unique per
//! tenant by index — as the key naming what settled. With the secret unset the
//! door is `503 unconfigured`, never open: a deployment that has wired no
//! payment bridge settles nothing.
//!
//! The settlement is still written through a person's door — the one who
//! approved that exact price ([`alo_store::Store::site_domain_purchase_approver`]) —
//! because a row that says a machine did it is a row that says nobody did.
//!
//! Error contract, otherwise identical to the rest of `/sites/*`: `401`
//! unauthenticated, `404` for a site or purchase that does not resolve in the
//! caller's tenant, `422` for a rule the caller can fix (with the store's or
//! the registrar model's own sentence as detail), `502` for a provider that
//! failed.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Extension, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use alo_store::{
    DomainOffer, DomainQuote, DomainRegistrar, DomainSearch, FixtureRegistrar,
    NewSiteDomainPurchase, REGISTRAR_CURRENCY, RegistrantContact, RegistrarError,
    SiteDomainPurchase, SiteDomainPurchaseId, SiteDomainPurchaseKind, SiteId, TldCatalog,
    TldRequirement, UnconfiguredRegistrar,
};

use crate::error::Problem;
use crate::sites::{map_store_err, require_site, require_site_manager};
use crate::state::{Account, AppState, authenticate};

/// Longest search box this surface reads — a label is at most 63 characters
/// and an ending is short; anything longer is not a name somebody typed.
pub const MAX_DOMAIN_QUERY_CHARS: usize = 120;

/// Environment variable naming the nameservers a bought domain is registered
/// with, comma separated. Unset means this deployment cannot point a name at
/// anything, so it does not sell one.
const SITE_NAMESERVERS_ENV: &str = "SITE_NAMESERVERS";

/// Environment variable choosing the registrar. Only `fixture` is recognised —
/// the deterministic in-memory reseller of `alo_store::site_registrar_fixture`,
/// for local development and demos. Anything else, including unset, is
/// [`UnconfiguredRegistrar`]: wiring a real reseller is an ADR, not a
/// deployment guess.
const SITE_REGISTRAR_ENV: &str = "SITE_REGISTRAR";

/// Environment variable holding the secret the payment bridge presents to
/// [`settle_payment`]. Unset — the default, and what production holds — means
/// no charge can be declared settled at all.
const SITE_PAYMENT_SETTLEMENT_SECRET_ENV: &str = "SITE_PAYMENT_SETTLEMENT_SECRET";

/// Shortest settlement secret this door will honour. Short enough to type,
/// long enough that guessing it is not a way to acquire domains.
const SETTLEMENT_SECRET_MIN: usize = 24;

/// The header the payment bridge carries its secret in.
const SETTLEMENT_HEADER: &str = "x-alo-settlement";

/// The deployment facts domain buying needs, resolved once when the router is
/// built: who registers the names, which nameservers they answer from, and the
/// secret that lets a payment bridge say a charge settled.
///
/// A struct rather than three `Extension`s so a test builds one value with a
/// fixture registrar and its own nameservers, and reads no environment at all
/// — process-wide environment mutation is not a thing a test suite can do
/// safely on several threads.
#[derive(Clone)]
pub struct SiteDomainCommerce {
    /// The reseller behind the buy box.
    pub registrar: Arc<dyn DomainRegistrar>,
    /// The nameservers every registration is created with, in order. Empty
    /// disables buying while leaving prices readable.
    pub nameservers: Vec<String>,
    /// What the payment bridge must present to settle a charge. `None` — the
    /// default — closes the door entirely rather than opening it to everybody.
    pub settlement_secret: Option<String>,
}

impl SiteDomainCommerce {
    /// The configuration this process was started with.
    ///
    /// Production ships [`UnconfiguredRegistrar`] until an ADR names a
    /// reseller: an installation that quietly pretended to sell domains would
    /// take money for names nobody registered.
    #[must_use]
    pub fn from_env() -> Self {
        let registrar: Arc<dyn DomainRegistrar> = match std::env::var(SITE_REGISTRAR_ENV)
            .unwrap_or_default()
            .as_str()
        {
            "fixture" => FixtureRegistrar::new(OffsetDateTime::now_utc()).map_or_else(
                |_| Arc::new(UnconfiguredRegistrar) as Arc<dyn DomainRegistrar>,
                |r| Arc::new(r),
            ),
            _ => Arc::new(UnconfiguredRegistrar),
        };
        Self {
            registrar,
            nameservers: std::env::var(SITE_NAMESERVERS_ENV)
                .unwrap_or_default()
                .split(',')
                .map(|host| host.trim().to_ascii_lowercase())
                .filter(|host| !host.is_empty())
                .collect(),
            // A secret too short to be one is treated as absent: the door
            // stays shut, which is the safe reading of a misconfiguration
            // whose other reading hands out domains.
            settlement_secret: std::env::var(SITE_PAYMENT_SETTLEMENT_SECRET_ENV)
                .ok()
                .filter(|secret| secret.len() >= SETTLEMENT_SECRET_MIN),
        }
    }

    /// Whether this deployment can register anything at all — the gate the
    /// registration sweep is started behind, so an installation that sells no
    /// domains also runs no worker for them.
    #[must_use]
    pub fn sells_domains(&self) -> bool {
        !self.nameservers.is_empty()
    }
}

impl Default for SiteDomainCommerce {
    fn default() -> Self {
        Self {
            registrar: Arc::new(UnconfiguredRegistrar),
            nameservers: Vec::new(),
            settlement_secret: None,
        }
    }
}

/// A registrar refusal as the problem it is on the wire.
///
/// `Unconfigured` is `503` with the typed reason the UI branches on;
/// everything the caller can fix is `422` carrying the model's own sentence
/// (which never quotes a registrant's value back); a provider fault is `502`
/// and says whether repeating the request could work.
fn map_registrar_err(error: &RegistrarError) -> Problem {
    match error {
        RegistrarError::Unconfigured => unconfigured(),
        RegistrarError::Validation(message) => {
            Problem::with(StatusCode::UNPROCESSABLE_ENTITY, message.clone())
        }
        RegistrarError::Unsupported { tld } => Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("alo does not sell .{tld} domains."),
        )
        .with_extra(json!({ "reason": "unsupported", "tld": tld })),
        RegistrarError::Unavailable => Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "That domain is not available any more. Choose another name.",
        )
        .with_extra(json!({ "reason": "unavailable" })),
        RegistrarError::Conflict(message) => {
            Problem::with(StatusCode::UNPROCESSABLE_ENTITY, message.clone())
                .with_extra(json!({ "reason": "conflict" }))
        }
        RegistrarError::Provider { retryable, message } => Problem::with(
            StatusCode::BAD_GATEWAY,
            format!("The domain registrar could not answer: {message}"),
        )
        .with_extra(json!({ "reason": "provider", "retryable": retryable })),
    }
}

/// The one refusal a deployment fact produces, in the shape the AI surfaces
/// established: a service that is not wired says so, and says it the same way
/// everywhere.
fn unconfigured() -> Problem {
    Problem::with(
        StatusCode::SERVICE_UNAVAILABLE,
        "Buying domain names is not configured on this alo deployment. \
         You can still connect a domain you already own.",
    )
    .with_extra(json!({ "reason": "unconfigured" }))
}

fn iso(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

/// A quote as JSON — always both halves of the price, because a shape that
/// could omit the renewal is a shape that could hide it.
fn quote_json(quote: &DomainQuote) -> Value {
    json!({
        "domain": quote.domain,
        "termYears": quote.term_years,
        "currency": quote.currency,
        "firstTermCents": quote.first_term_cents,
        "renewalCentsPerYear": quote.renewal_cents_per_year,
        "premium": quote.premium,
    })
}

fn offer_json(offer: &DomainOffer) -> Value {
    json!({
        "domain": offer.domain,
        "availability": offer.availability.as_str(),
        "quote": offer.quote.as_ref().map_or(Value::Null, quote_json),
    })
}

fn requirement_json(requirement: &TldRequirement) -> Value {
    match requirement {
        TldRequirement::None => json!({ "kind": "none" }),
        TldRequirement::EeaPresence => json!({ "kind": "eea_presence" }),
        TldRequirement::CountryPresence { country } => {
            json!({ "kind": "country_presence", "country": country })
        }
    }
}

/// One purchase as the tenant sees it. The registrant is deliberately absent:
/// it lives behind its own route, so a list of purchases is never a place
/// somebody's home address is spread (S2.15b).
fn purchase_json(purchase: &SiteDomainPurchase) -> Value {
    json!({
        "id": purchase.id.as_str(),
        "site": purchase.site.as_str(),
        "kind": purchase.kind.as_str(),
        "domain": purchase.domain,
        "tld": purchase.tld,
        "state": purchase.state.as_str(),
        "moneyMoved": purchase.state.money_moved(),
        "open": purchase.state.is_open(),
        "termYears": purchase.term_years,
        "currency": purchase.currency,
        "firstTermCents": purchase.first_term_cents,
        "renewalCentsPerYear": purchase.renewal_cents_per_year,
        "premium": purchase.premium,
        "autoRenew": purchase.auto_renew,
        "nameservers": purchase.nameservers,
        "requestKey": purchase.request_key,
        "approvedAt": purchase.approved_at.map(iso),
        "approvedBy": purchase.approved_by.as_ref().map(|u| u.as_str().to_owned()),
        "paymentReference": purchase.payment_reference,
        "paidAt": purchase.paid_at.map(iso),
        "attempts": purchase.attempts,
        "providerReference": purchase.provider_reference,
        "registeredAt": purchase.registered_at.map(iso),
        "expiresAt": purchase.expires_at.map(iso),
        "lifecycle": purchase.lifecycle.map(|l| l.as_str()),
        "configuredAt": purchase.configured_at.map(iso),
        "failure": purchase.failure,
        "createdAt": iso(purchase.created_at),
        "updatedAt": iso(purchase.updated_at),
    })
}

/// The site this purchase surface is about, or the refusal that says why not.
///
/// One helper rather than three lines in six handlers, because the guard is
/// the same everywhere and a handler that forgot half of it would be a hole
/// in the money door.
async fn require_purchasing_site(account: &Account, site: &SiteId) -> Result<(), Problem> {
    let site = require_site(account, site).await?;
    require_site_manager(account, &site).map_err(|_| {
        Problem::with(
            StatusCode::FORBIDDEN,
            "Only this website's owner can buy or manage its domain names.",
        )
    })
}

// ---- prices ------------------------------------------------------------------

/// `GET /sites/domain-catalog` → the endings this deployment sells, in
/// editorial order, with their yearly prices.
///
/// Both prices are stated on every ending, VAT exclusive: what a year costs
/// today and what it costs every year afterwards. The registrar's own identity
/// travels with them, so an operator can see which company registers their
/// customers' names and whether that connection spends real money.
pub async fn domain_catalog(
    State(state): State<AppState>,
    Extension(commerce): Extension<SiteDomainCommerce>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    authenticate(&state, &headers).await?;
    let catalog = commerce
        .registrar
        .catalog()
        .await
        .map_err(|e| map_registrar_err(&e))?;
    let identity = commerce.registrar.identity();
    Ok(Json(json!({
        "registrar": {
            "name": identity.name,
            "country": identity.country,
            "environment": identity.environment.as_str(),
            "spendsMoney": identity.environment.spends_money(),
        },
        "currency": REGISTRAR_CURRENCY,
        "buyable": !commerce.nameservers.is_empty(),
        "endings": catalog.offers().iter().map(|offer| json!({
            "tld": offer.tld,
            "registerCents": offer.register_cents,
            "renewCents": offer.renew_cents,
            "transferCents": offer.transfer_cents,
            "minYears": offer.min_years,
            "maxYears": offer.max_years,
            "requirement": requirement_json(&offer.requirement),
        })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
pub struct DomainSearchQuery {
    /// What the buyer typed. `acme`, `Acme.com` and `https://acme.com/` are
    /// the same search (S1.30b's lesson, built into the model).
    q: String,
    /// Endings to check, comma separated. Empty means "what you recommend".
    #[serde(default)]
    tlds: Option<String>,
}

/// `GET /sites/domain-search?q=acme&tlds=com,eu` → one offer per candidate:
/// whether it can be bought and, only then, what it costs.
///
/// An ending we do not sell comes back marked `unsupported` rather than being
/// dropped, so a buyer who asked for one is told instead of left wondering.
pub async fn search_domains(
    State(state): State<AppState>,
    Extension(commerce): Extension<SiteDomainCommerce>,
    headers: HeaderMap,
    Query(query): Query<DomainSearchQuery>,
) -> Result<Json<Value>, Problem> {
    authenticate(&state, &headers).await?;
    if query.q.chars().count() > MAX_DOMAIN_QUERY_CHARS {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("A domain search is at most {MAX_DOMAIN_QUERY_CHARS} characters."),
        ));
    }
    let catalog = commerce
        .registrar
        .catalog()
        .await
        .map_err(|e| map_registrar_err(&e))?;
    let tlds = split_tlds(query.tlds.as_deref());
    let search =
        DomainSearch::parse(&query.q, &tlds, &catalog).map_err(|e| map_registrar_err(&e))?;
    let label = search.label().to_owned();
    let offers = commerce
        .registrar
        .search(search)
        .await
        .map_err(|e| map_registrar_err(&e))?;
    Ok(Json(json!({
        "label": label,
        "currency": REGISTRAR_CURRENCY,
        "buyable": !commerce.nameservers.is_empty(),
        "offers": offers.iter().map(offer_json).collect::<Vec<_>>(),
    })))
}

/// Splits the `tlds` parameter. Empty entries are dropped rather than
/// refused: `?tlds=com,` is what a UI that joins an array produces.
fn split_tlds(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or_default()
        .split(',')
        .map(|tld| tld.trim().to_owned())
        .filter(|tld| !tld.is_empty())
        .collect()
}

// ---- purchases ---------------------------------------------------------------

/// `GET /sites/:id/domain-purchases` → this website's purchases, newest first.
pub async fn list_purchases(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_purchasing_site(&account, &site).await?;
    let purchases = account
        .acc
        .site_domain_purchases(&site, alo_store::MAX_SITE_DOMAIN_PURCHASES)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "purchases": purchases.iter().map(purchase_json).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
struct RegistrantBody {
    name: String,
    #[serde(default)]
    organisation: Option<String>,
    email: String,
    street: String,
    #[serde(rename = "postalCode")]
    postal_code: String,
    city: String,
    country: String,
    phone: String,
}

impl From<RegistrantBody> for RegistrantContact {
    fn from(body: RegistrantBody) -> Self {
        Self {
            name: body.name,
            organisation: body.organisation,
            email: body.email,
            street: body.street,
            postal_code: body.postal_code,
            city: body.city,
            country: body.country,
            phone: body.phone,
        }
    }
}

#[derive(Deserialize)]
struct NewPurchaseBody {
    /// The name to buy, as typed. Tidied and split against the catalog.
    domain: String,
    /// How many years the first payment covers.
    #[serde(default)]
    years: Option<u8>,
    /// `registration` (default) or `renewal`.
    #[serde(default)]
    kind: Option<String>,
    #[serde(default, rename = "autoRenew")]
    auto_renew: Option<bool>,
    /// The caller's replay token: the same key returns the same purchase
    /// rather than buying a second name.
    #[serde(rename = "requestKey")]
    request_key: String,
    registrant: RegistrantBody,
}

/// `POST /sites/:id/domain-purchases` → a `quoted` purchase at the price the
/// registrar states in this very request.
///
/// The body carries **no price**. What the buyer is charged is what the seller
/// answered a moment ago, stored from that answer; the client's job is to show
/// it and — in [`approve_purchase`] — to prove it showed the same numbers.
/// Nothing is charged and nothing is registered by this call.
pub async fn create_purchase(
    State(state): State<AppState>,
    Extension(commerce): Extension<SiteDomainCommerce>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_purchasing_site(&account, &site).await?;
    let body: NewPurchaseBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    if commerce.nameservers.is_empty() {
        return Err(unconfigured());
    }
    let kind = match body.kind.as_deref() {
        None | Some("registration") => SiteDomainPurchaseKind::Registration,
        Some("renewal") => SiteDomainPurchaseKind::Renewal,
        Some(_) => {
            return Err(Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "a domain purchase is either a registration or a renewal",
            ));
        }
    };
    if body.domain.chars().count() > MAX_DOMAIN_QUERY_CHARS {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("A domain name is at most {MAX_DOMAIN_QUERY_CHARS} characters."),
        ));
    }

    let catalog: TldCatalog = commerce
        .registrar
        .catalog()
        .await
        .map_err(|e| map_registrar_err(&e))?;
    let domain = catalog
        .parse(&body.domain)
        .map_err(|e| map_registrar_err(&e))?;
    let years = body.years.unwrap_or(1);
    // The seller's own number, asked for now — not a number that travelled
    // through a browser.
    let quote = commerce
        .registrar
        .quote(domain.name().to_owned(), years)
        .await
        .map_err(|e| map_registrar_err(&e))?;

    let purchase = account
        .acc
        .start_site_domain_purchase(
            &site,
            NewSiteDomainPurchase {
                kind,
                domain,
                quote,
                registrant: body.registrant.into(),
                nameservers: commerce.nameservers.clone(),
                auto_renew: body.auto_renew.unwrap_or(true),
                request_key: body.request_key,
            },
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(purchase_json(&purchase)))
}

/// `GET /sites/:id/domain-purchases/:purchase` → one purchase.
pub async fn get_purchase(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, purchase)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_purchasing_site(&account, &site).await?;
    let purchase = load_purchase(&account, &site, &purchase).await?;
    Ok(Json(purchase_json(&purchase)))
}

/// `GET /sites/:id/domain-purchases/:purchase/registrant` → who this purchase
/// will name to the registry.
///
/// The deliberate read S2.15b's storage promises: personal data reached by one
/// route that exists to show a person what they are about to submit, never as
/// a field of a list.
pub async fn get_registrant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, purchase)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_purchasing_site(&account, &site).await?;
    let purchase = load_purchase(&account, &site, &purchase).await?;
    let registrant = account
        .acc
        .site_domain_purchase_registrant(&purchase.id)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "name": registrant.name,
        "organisation": registrant.organisation,
        "email": registrant.email,
        "street": registrant.street,
        "postalCode": registrant.postal_code,
        "city": registrant.city,
        "country": registrant.country,
        "phone": registrant.phone,
    })))
}

#[derive(Deserialize)]
struct AgreedQuoteBody {
    domain: String,
    #[serde(rename = "termYears")]
    term_years: u8,
    #[serde(default)]
    currency: Option<String>,
    #[serde(rename = "firstTermCents")]
    first_term_cents: i64,
    #[serde(rename = "renewalCentsPerYear")]
    renewal_cents_per_year: i64,
    premium: bool,
}

#[derive(Deserialize)]
struct ApproveBody {
    agreed: AgreedQuoteBody,
}

/// `POST /sites/:id/domain-purchases/:purchase/approve` `{agreed:{…}}` records
/// that this named person agreed to **this exact price**.
///
/// The body is the quote the browser had on screen, all six numbers of it. The
/// store compares it with the row and refuses any disagreement, so a price
/// that moved between the screen and the charge stops here instead of being
/// silently re-quoted — including the renewal price, which is the half a bait
/// price hides in.
pub async fn approve_purchase(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, purchase)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_purchasing_site(&account, &site).await?;
    let body: ApproveBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let purchase = load_purchase(&account, &site, &purchase).await?;
    let agreed = DomainQuote {
        domain: body.agreed.domain.trim().to_ascii_lowercase(),
        term_years: body.agreed.term_years,
        currency: body
            .agreed
            .currency
            .unwrap_or_else(|| REGISTRAR_CURRENCY.to_owned()),
        first_term_cents: body.agreed.first_term_cents,
        renewal_cents_per_year: body.agreed.renewal_cents_per_year,
        premium: body.agreed.premium,
    };
    let approved = account
        .acc
        .approve_site_domain_purchase(&purchase.id, &agreed)
        .await
        .map_err(map_store_err)?;
    Ok(Json(purchase_json(&approved)))
}

#[derive(Deserialize)]
struct CheckoutBody {
    /// What the charge is called by whatever charges the tenant. Opaque here.
    #[serde(rename = "paymentReference")]
    payment_reference: String,
}

/// `POST /sites/:id/domain-purchases/:purchase/checkout`
/// `{"paymentReference":"…"}` hands the approved purchase to a payment.
///
/// The reference is Billing's — or that of whatever payment bridge a
/// deployment wires — and is stored verbatim, never parsed: how a tenant is
/// charged is not this module's business, and its only interest in the
/// reference is that one payment settles one purchase.
///
/// Nothing here says money moved. The purchase reaches `awaiting_payment` and
/// waits for [`settle_payment`], on the other side of the door, to say that it
/// did.
pub async fn checkout_purchase(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, purchase)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_purchasing_site(&account, &site).await?;
    let body: CheckoutBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let purchase = load_purchase(&account, &site, &purchase).await?;
    let awaiting = account
        .acc
        .await_site_domain_payment(&purchase.id, &body.payment_reference)
        .await
        .map_err(map_store_err)?;
    Ok(Json(purchase_json(&awaiting)))
}

#[derive(Deserialize)]
struct SettleBody {
    /// Whose charge settled. The bridge knows it; a browser has no business
    /// naming it.
    tenant: String,
    /// The reference the bridge minted at checkout — unique per tenant, so it
    /// names exactly one purchase.
    #[serde(rename = "paymentReference")]
    payment_reference: String,
}

/// `POST /sites/domain-payments/settle` `{"tenant":"…","paymentReference":"…"}`
/// records that a charge arrived, which is what queues the registration.
///
/// The machine door of this module (see the header): no user token, the
/// deployment's settlement secret in `X-Alo-Settlement` instead. A tenant may
/// not declare their own payment settled, because that statement is worth a
/// domain.
///
/// Repeating it is harmless — the store returns the already-settled purchase —
/// so a bridge that delivers its webhook twice registers one name.
pub async fn settle_payment(
    State(state): State<AppState>,
    Extension(commerce): Extension<SiteDomainCommerce>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let Some(expected) = commerce.settlement_secret.as_deref() else {
        return Err(unconfigured());
    };
    let presented = headers
        .get(SETTLEMENT_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !secret_matches(expected, presented) {
        return Err(Problem::with(
            StatusCode::UNAUTHORIZED,
            "This door is for the payment bridge of this alo deployment.",
        ));
    }
    let body: SettleBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let tenant = alo_store::TenantId::new(body.tenant);
    let purchase = state
        .store
        .site_domain_purchase_awaiting_payment(&tenant, &body.payment_reference)
        .await
        .map_err(map_settlement_err)?;
    // Through the door of the person who approved this exact price: a
    // settlement written by nobody is a settlement nobody can be asked about.
    let approver = state
        .store
        .site_domain_purchase_approver(&tenant, &purchase.id)
        .await
        .map_err(map_settlement_err)?;
    let settled = state
        .store
        .for_account(tenant, approver)
        .settle_site_domain_payment(&purchase.id, &body.payment_reference)
        .await
        .map_err(map_store_err)?;
    Ok(Json(purchase_json(&settled)))
}

/// Whether the presented secret is the configured one, compared without
/// leaking where it first differs.
fn secret_matches(expected: &str, presented: &str) -> bool {
    if expected.len() != presented.len() {
        return false;
    }
    expected
        .bytes()
        .zip(presented.bytes())
        .fold(0u8, |differs, (a, b)| differs | (a ^ b))
        == 0
}

/// A settlement that resolves to nothing is one `404`, whatever part of it was
/// wrong: a bridge holding the deployment secret is not a caller we need to
/// help tell a wrong tenant from an unknown reference, and a door that did
/// would answer questions about which tenants exist.
fn map_settlement_err(error: alo_store::StoreError) -> Problem {
    match error {
        alo_store::StoreError::NotFound => Problem::with(
            StatusCode::NOT_FOUND,
            "no domain purchase is waiting for that payment",
        ),
        other => map_store_err(other),
    }
}

/// `POST /sites/:id/domain-purchases/:purchase/cancel` calls a purchase off.
///
/// Only before money moved: after the charge this is a support conversation
/// about a refund, not a button, and the store says so in those words.
pub async fn cancel_purchase(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, purchase)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_purchasing_site(&account, &site).await?;
    let purchase = load_purchase(&account, &site, &purchase).await?;
    let cancelled = account
        .acc
        .cancel_site_domain_purchase(&purchase.id)
        .await
        .map_err(map_store_err)?;
    Ok(Json(purchase_json(&cancelled)))
}

/// Resolves a purchase id **within the site in the path**.
///
/// The store scopes by tenant; this adds the site, so a purchase id from
/// another of the caller's own websites does not answer under this one's URL
/// and the path stays the truth about what is being changed.
async fn load_purchase(
    account: &Account,
    site: &SiteId,
    purchase: &str,
) -> Result<SiteDomainPurchase, Problem> {
    let purchase = account
        .acc
        .site_domain_purchase(&SiteDomainPurchaseId::new(purchase.to_owned()))
        .await
        .map_err(map_store_err)?;
    if purchase.site.as_str() != site.as_str() {
        return Err(Problem::with(
            StatusCode::NOT_FOUND,
            "no such domain purchase",
        ));
    }
    Ok(purchase)
}
