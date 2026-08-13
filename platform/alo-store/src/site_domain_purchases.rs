//! Buying a domain through alo (ADR 0036, S2.15b) — the record that carries a
//! purchase from the price somebody was shown to the name answering for their
//! website.
//!
//! [`site_registrar`](crate::site_registrar) is the outside: what a reseller
//! sells and what it charges. This module is the inside: which tenant asked for
//! which name, at which price, **who approved that price**, which payment paid
//! for it, and what the registrar answered. The row is the state machine —
//! there is no queue of intentions kept beside the truth, and the registrar's
//! own answer is written back onto the same row that quoted the price.
//!
//! Four properties this module is responsible for.
//!
//! - **Nothing is bought that nobody approved.** A purchase starts
//!   [`Quoted`](SiteDomainPurchaseState::Quoted) and cannot reach a payment
//!   without passing through
//!   [`Approved`](SiteDomainPurchaseState::Approved), and approval names the
//!   exact price it approves: [`AccountStore::approve_site_domain_purchase`]
//!   refuses when the quote on the screen is no longer the quote in the row.
//!   The honest-pricing rule of `site_registrar` is worth nothing if the number
//!   may move between the screen and the charge.
//! - **A retry never buys twice.** Creating is idempotent under the caller's
//!   `request_key`, so a double-clicked buy button reaches the row it already
//!   made; the registrar call is made under the purchase id as its idempotency
//!   key, so a sweep that dies after the registry answered registers nothing
//!   the second time.
//! - **Money that moved is not silently lost.** Registration failures are
//!   separated into "ask again" ([`Store::retry_site_domain_registration`]) and
//!   "this will fail identically forever"
//!   ([`Store::fail_site_domain_registration`]); an interrupted sweep is
//!   re-offered until [`SITE_DOMAIN_PURCHASE_MAX_ATTEMPTS`] and then fails
//!   **visibly**, with a sentence the tenant can act on, rather than staying
//!   `registering` forever.
//! - **Billing stays behind its own door.** The only thing this module knows
//!   about a charge is [`SiteDomainPurchase::payment_reference`], an opaque
//!   string minted by whatever charges the tenant. Sites writes no billing row
//!   and reads none; a unique index makes one payment settle exactly one
//!   purchase.
//!
//! # The registrant
//!
//! [`RegistrantContact`] is the personal data a registry requires by contract.
//! It rests in the buying tenant's own row and is read only by the two
//! deliberate calls that need it —
//! [`AccountStore::site_domain_purchase_registrant`], for the person reviewing
//! what they are about to submit, and [`Store::claim_site_domain_registrations`],
//! which hands it to the registrar. It is deliberately **not** a field of
//! [`SiteDomainPurchase`]: a list of purchases is not a place to spread
//! somebody's home address, and the type system is a better guarantee of that
//! than a code review.

use serde_json::Value;
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{self, SiteDomainPurchaseId, SiteId, TenantId, UserId};
use crate::site_registrar::{
    DomainLifecycle, DomainOrder, DomainQuote, REGISTRAR_CURRENCY, RegisteredDomain,
    RegistrableDomain, RegistrantContact, RegistrarError, validate_idempotency_key,
};
use crate::store::Store;

/// How long a claimed registration may stay unfinished before the sweep assumes
/// the worker died and offers the row again.
pub const SITE_DOMAIN_PURCHASE_CLAIM_STALE_MINUTES: i32 = 10;

/// How many times one paid purchase may be handed to the registrar before it is
/// declared failed. Higher than the publishing sweep's three, because the money
/// has already moved: giving up costs the tenant a refund conversation.
pub const SITE_DOMAIN_PURCHASE_MAX_ATTEMPTS: i32 = 5;

/// Most purchases one list read returns.
pub const MAX_SITE_DOMAIN_PURCHASES: i64 = 50;

/// Longest failure sentence stored on a purchase. The sentence is a registrar
/// error message — which by [`RegistrarError`]'s own contract carries neither
/// registrant data nor a provider's raw response — never request content.
pub const SITE_DOMAIN_PURCHASE_FAILURE_MAX_CHARS: usize = 500;

/// Shortest payment reference Billing may hand back.
pub const PAYMENT_REFERENCE_MIN: usize = 4;

/// Longest payment reference Billing may hand back.
pub const PAYMENT_REFERENCE_MAX: usize = 200;

/// What a purchase says when its worker never came back.
pub const SITE_DOMAIN_PURCHASE_INTERRUPTED: &str = "registering this domain was interrupted too many times; \
     alo support can finish it or refund it";

/// Whether a purchase buys a name or extends one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteDomainPurchaseKind {
    /// Buying a name nobody holds.
    Registration,
    /// Extending a name this tenant already has.
    Renewal,
}

impl SiteDomainPurchaseKind {
    /// The stable token this kind is stored and named by on the wire.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Registration => "registration",
            Self::Renewal => "renewal",
        }
    }

    /// Reads a stored token back.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "registration" => Some(Self::Registration),
            "renewal" => Some(Self::Renewal),
            _ => None,
        }
    }
}

/// Where one purchase is in its life.
///
/// The order is the only order: every transition this module allows moves
/// forwards through it, and the two endings —
/// [`Cancelled`](Self::Cancelled) before money moved,
/// [`Failed`](Self::Failed) after a registrar refusal — are terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteDomainPurchaseState {
    /// A price was worked out and shown. Nobody has agreed to it.
    Quoted,
    /// A named person agreed to that exact price.
    Approved,
    /// Billing has been asked for the money and gave us its reference.
    AwaitingPayment,
    /// The money moved. The registrar has not been called yet.
    Paid,
    /// A sweep is registering it right now.
    Registering,
    /// The registry holds the name for this tenant.
    Registered,
    /// The name is attached to the website and serving.
    Configured,
    /// The registrar refused, or too many attempts were interrupted.
    Failed,
    /// Called off before any money moved.
    Cancelled,
}

impl SiteDomainPurchaseState {
    /// The stable token this state is stored and named by on the wire.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Quoted => "quoted",
            Self::Approved => "approved",
            Self::AwaitingPayment => "awaiting_payment",
            Self::Paid => "paid",
            Self::Registering => "registering",
            Self::Registered => "registered",
            Self::Configured => "configured",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Reads a stored token back.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "quoted" => Some(Self::Quoted),
            "approved" => Some(Self::Approved),
            "awaiting_payment" => Some(Self::AwaitingPayment),
            "paid" => Some(Self::Paid),
            "registering" => Some(Self::Registering),
            "registered" => Some(Self::Registered),
            "configured" => Some(Self::Configured),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// Whether the tenant has been charged. The line cancellation may not
    /// cross: everything from here on is a support conversation, not a button.
    #[must_use]
    pub fn money_moved(self) -> bool {
        matches!(
            self,
            Self::Paid | Self::Registering | Self::Registered | Self::Configured
        )
    }

    /// Whether this purchase is still going to happen.
    #[must_use]
    pub fn is_open(self) -> bool {
        !matches!(self, Self::Configured | Self::Failed | Self::Cancelled)
    }
}

/// One domain purchase, as the tenant sees it — without the registrant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteDomainPurchase {
    pub id: SiteDomainPurchaseId,
    pub site: SiteId,
    pub kind: SiteDomainPurchaseKind,
    /// The name being bought, lowercase.
    pub domain: String,
    /// Its ending, without the leading dot.
    pub tld: String,
    pub state: SiteDomainPurchaseState,
    /// Years the first payment covers.
    pub term_years: u8,
    /// Always [`REGISTRAR_CURRENCY`]; carried so a stored purchase reads back
    /// as the quote it was.
    pub currency: String,
    /// What the tenant pays now, VAT exclusive, in cents.
    pub first_term_cents: i64,
    /// What one year costs afterwards, VAT exclusive, in cents. Stated before
    /// approval, always.
    pub renewal_cents_per_year: i64,
    /// Whether the registry prices this name above its ending's list price.
    pub premium: bool,
    /// Whether the name should renew itself.
    pub auto_renew: bool,
    /// The nameservers the registration is created with, in order.
    pub nameservers: Vec<String>,
    /// The caller's replay token for creating this purchase.
    pub request_key: String,
    /// When a person agreed to this exact price.
    pub approved_at: Option<OffsetDateTime>,
    /// Which person that was.
    pub approved_by: Option<UserId>,
    /// Billing's opaque identifier for the charge. Never parsed here.
    pub payment_reference: Option<String>,
    pub paid_at: Option<OffsetDateTime>,
    /// When the registration sweep last took this row.
    pub claimed_at: Option<OffsetDateTime>,
    /// How many times a sweep has taken it.
    pub attempts: i32,
    /// The registrar's own identifier for the registration.
    pub provider_reference: Option<String>,
    pub registered_at: Option<OffsetDateTime>,
    /// When the paid term runs out, as the registry counts it.
    pub expires_at: Option<OffsetDateTime>,
    /// Where the registered name is in its life.
    pub lifecycle: Option<DomainLifecycle>,
    /// When the name was attached to the website.
    pub configured_at: Option<OffsetDateTime>,
    /// Why this purchase stopped, in words the tenant can act on.
    pub failure: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// What starting a purchase needs.
///
/// [`domain`](Self::domain) is a [`RegistrableDomain`], which only a
/// [`TldCatalog`](crate::site_registrar::TldCatalog) can build: an ending this
/// installation does not sell cannot reach the store at all, and the store
/// therefore never has to guess where a name's label stops and its ending
/// begins.
#[derive(Debug, Clone)]
pub struct NewSiteDomainPurchase {
    pub kind: SiteDomainPurchaseKind,
    /// The name, parsed against the catalog that priced it.
    pub domain: RegistrableDomain,
    /// The price the buyer was shown, both halves of it.
    pub quote: DomainQuote,
    /// Who the registry will record as the owner.
    pub registrant: RegistrantContact,
    /// The nameservers to register with — alo's, for the "live in minutes"
    /// path.
    pub nameservers: Vec<String>,
    pub auto_renew: bool,
    /// The caller's replay token: the same key returns the same purchase.
    pub request_key: String,
}

/// A paid purchase a sweep has claimed, with everything the registrar call
/// needs and nothing it does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueSiteDomainRegistration {
    /// The tenant this purchase belongs to — the only tenant it may touch.
    pub tenant: TenantId,
    pub site: SiteId,
    pub purchase: SiteDomainPurchaseId,
    pub kind: SiteDomainPurchaseKind,
    /// The order to place. Its `idempotency_key` is the purchase id, so a
    /// second attempt at the same row is a replay to the registrar rather than
    /// a second registration.
    pub order: DomainOrder,
    /// Which attempt this is, starting at 1.
    pub attempts: i32,
}

/// Maps a registrar-model validation failure onto the store's own error type.
/// Only [`RegistrarError::Validation`] can arise from the checks this module
/// runs (nothing here talks to a provider); anything else would be a bug in a
/// caller, and is reported as one rather than widened into a friendly message.
fn from_registrar(error: &RegistrarError) -> StoreError {
    match error {
        RegistrarError::Validation(message) => StoreError::Validation(message.clone()),
        RegistrarError::Unavailable => {
            StoreError::Conflict("that domain is not available".to_owned())
        }
        RegistrarError::Conflict(message) => StoreError::Conflict(message.clone()),
        other => StoreError::Validation(other.to_string()),
    }
}

/// Checks a payment reference: something Billing minted, opaque to us, short
/// enough to store and printable enough to show in a support conversation.
///
/// # Errors
/// [`StoreError::Validation`] naming the rule broken.
pub fn validate_payment_reference(reference: &str) -> Result<String> {
    let reference = reference.trim();
    if !(PAYMENT_REFERENCE_MIN..=PAYMENT_REFERENCE_MAX).contains(&reference.chars().count()) {
        return Err(StoreError::Validation(format!(
            "a payment reference is {PAYMENT_REFERENCE_MIN}-{PAYMENT_REFERENCE_MAX} characters"
        )));
    }
    if reference
        .chars()
        .any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(StoreError::Validation(
            "a payment reference may not contain spaces or control characters".to_owned(),
        ));
    }
    Ok(reference.to_owned())
}

/// The words a state change refuses with, so the same situation reads the same
/// way wherever it is met.
fn wrong_state(state: SiteDomainPurchaseState) -> StoreError {
    let detail = match state {
        SiteDomainPurchaseState::Quoted => {
            "nobody has approved the price of this domain purchase yet"
        }
        SiteDomainPurchaseState::Approved => "this domain purchase has not been paid for yet",
        SiteDomainPurchaseState::AwaitingPayment => "this domain purchase is waiting for payment",
        SiteDomainPurchaseState::Paid | SiteDomainPurchaseState::Registering => {
            "this domain purchase has been paid for and is being registered"
        }
        SiteDomainPurchaseState::Registered => "this domain has already been registered",
        SiteDomainPurchaseState::Configured => "this domain is already connected to the website",
        SiteDomainPurchaseState::Failed => "this domain purchase failed",
        SiteDomainPurchaseState::Cancelled => "this domain purchase was called off",
    };
    StoreError::Conflict(detail.to_owned())
}

fn map_purchase_unique(error: sqlx::Error) -> StoreError {
    if let sqlx::Error::Database(ref database) = error {
        match database.constraint() {
            Some("site_domain_purchases_one_registration") => {
                return StoreError::Conflict("you are already buying that domain".to_owned());
            }
            Some("site_domain_purchases_one_open_renewal") => {
                return StoreError::Conflict("that domain is already being renewed".to_owned());
            }
            Some("site_domain_purchases_request_key") => {
                return StoreError::Conflict(
                    "that request was already used for a different purchase".to_owned(),
                );
            }
            Some("site_domain_purchases_one_payment") => {
                return StoreError::Conflict(
                    "that payment already settles another domain purchase".to_owned(),
                );
            }
            _ => {}
        }
    }
    error.into()
}

impl AccountStore {
    /// Starts a purchase at the price the buyer was shown, in
    /// [`Quoted`](SiteDomainPurchaseState::Quoted): a record of an intention,
    /// with nothing charged and nothing registered.
    ///
    /// Idempotent under [`NewSiteDomainPurchase::request_key`]. A repeat of the
    /// identical request returns the purchase it already made; a *different*
    /// request under the same key is [`StoreError::Conflict`], never a second
    /// domain nobody meant to buy.
    ///
    /// A renewal may only be started for a domain this website already has
    /// connected — otherwise there is nothing to renew, and paying to extend
    /// somebody else's registration is not a mistake worth allowing.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site isn't this tenant's;
    /// [`StoreError::Validation`] for a malformed request key, a quote for a
    /// different name or currency, or an order the registrar model refuses;
    /// [`StoreError::Conflict`] when the key was used for a different purchase
    /// or another live purchase already covers the name; [`StoreError::Db`].
    pub async fn start_site_domain_purchase(
        &self,
        site: &SiteId,
        new: NewSiteDomainPurchase,
    ) -> Result<SiteDomainPurchase> {
        validate_idempotency_key(&new.request_key).map_err(|e| from_registrar(&e))?;
        if new.quote.domain != new.domain.name() {
            return Err(StoreError::Validation(
                "the price shown is for a different domain".to_owned(),
            ));
        }
        if new.quote.currency != REGISTRAR_CURRENCY {
            return Err(StoreError::Validation(format!(
                "domains are sold in {REGISTRAR_CURRENCY}"
            )));
        }
        // The id doubles as the registrar's idempotency key, so it must exist
        // before the order can be validated as the shape it will be sent in.
        let id = SiteDomainPurchaseId::generate();
        let order = DomainOrder {
            domain: new.domain.name().to_owned(),
            years: new.quote.term_years,
            registrant: new.registrant,
            nameservers: new.nameservers,
            auto_renew: new.auto_renew,
            idempotency_key: id.as_str().to_owned(),
        };
        order.validate_shape().map_err(|e| from_registrar(&e))?;
        let fingerprint = order.fingerprint();
        let registrant = serde_json::to_value(&order.registrant)
            .map_err(|_| StoreError::Validation("that registrant cannot be stored".to_owned()))?;
        let nameservers = Value::from(order.nameservers.clone());

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Existence check and serialization point in one: two buy clicks meet
        // here, so the second sees the first one's row instead of racing the
        // request-key index into a stranger error.
        let owned: Option<String> =
            sqlx::query_scalar("SELECT id FROM sites WHERE tenant_id = $1 AND id = $2 FOR UPDATE")
                .bind(self.tenant.as_str())
                .bind(site.as_str())
                .fetch_optional(&mut *tx)
                .await
                .map_err(StoreError::Db)?;
        if owned.is_none() {
            return Err(StoreError::NotFound);
        }
        if new.kind == SiteDomainPurchaseKind::Renewal {
            let connected: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM site_domains \
                 WHERE tenant_id = $1 AND site_id = $2 AND domain = $3)",
            )
            .bind(self.tenant.as_str())
            .bind(site.as_str())
            .bind(new.domain.name())
            .fetch_one(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
            if !connected {
                return Err(StoreError::Validation(
                    "alo does not manage that domain for this website, so there is nothing to \
                     renew"
                        .to_owned(),
                ));
            }
        }
        let existing = sqlx::query_as::<_, PurchaseRow>(&format!(
            "SELECT {PURCHASE_COLUMNS} FROM site_domain_purchases \
             WHERE tenant_id = $1 AND request_key = $2 FOR UPDATE"
        ))
        .bind(self.tenant.as_str())
        .bind(&new.request_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if let Some(row) = existing {
            let same = row.site_id == site.as_str()
                && row.kind == new.kind.as_str()
                && row.order_fingerprint == fingerprint
                && row.term_years == i32::from(new.quote.term_years)
                && row.first_term_cents == new.quote.first_term_cents
                && row.renewal_cents_per_year == new.quote.renewal_cents_per_year;
            tx.commit().await.map_err(StoreError::Db)?;
            if same {
                return row.into_purchase();
            }
            return Err(StoreError::Conflict(
                "that request was already used for a different purchase".to_owned(),
            ));
        }
        let row = sqlx::query_as::<_, PurchaseRow>(&format!(
            "INSERT INTO site_domain_purchases \
                 (tenant_id, site_id, id, kind, domain, tld, state, term_years, currency, \
                  first_term_cents, renewal_cents_per_year, premium, auto_renew, nameservers, \
                  registrant, request_key, order_fingerprint) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17) \
             RETURNING {PURCHASE_COLUMNS}"
        ))
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(id.as_str())
        .bind(new.kind.as_str())
        .bind(new.domain.name())
        .bind(new.domain.tld())
        .bind(SiteDomainPurchaseState::Quoted.as_str())
        .bind(i32::from(new.quote.term_years))
        .bind(&new.quote.currency)
        .bind(new.quote.first_term_cents)
        .bind(new.quote.renewal_cents_per_year)
        .bind(new.quote.premium)
        .bind(order.auto_renew)
        .bind(&nameservers)
        .bind(&registrant)
        .bind(&new.request_key)
        .bind(&fingerprint)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_purchase_unique)?;
        tx.commit().await.map_err(StoreError::Db)?;
        row.into_purchase()
    }

    /// One purchase of this tenant. Another tenant's purchase is
    /// [`StoreError::NotFound`], exactly as one that never existed.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]; [`StoreError::Db`]; [`StoreError::Conflict`]
    /// if a stored token is unreadable.
    pub async fn site_domain_purchase(
        &self,
        purchase: &SiteDomainPurchaseId,
    ) -> Result<SiteDomainPurchase> {
        let row = sqlx::query_as::<_, PurchaseRow>(&format!(
            "SELECT {PURCHASE_COLUMNS} FROM site_domain_purchases \
             WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(purchase.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)?;
        row.into_purchase()
    }

    /// A website's purchases, newest first, capped at `limit` (clamped to
    /// [`MAX_SITE_DOMAIN_PURCHASES`]). Another tenant's site reads as an empty
    /// list, by design.
    ///
    /// # Errors
    /// [`StoreError::Db`]; [`StoreError::Conflict`] if a stored token is
    /// unreadable.
    pub async fn site_domain_purchases(
        &self,
        site: &SiteId,
        limit: i64,
    ) -> Result<Vec<SiteDomainPurchase>> {
        let limit = limit.clamp(1, MAX_SITE_DOMAIN_PURCHASES);
        let rows = sqlx::query_as::<_, PurchaseRow>(&format!(
            "SELECT {PURCHASE_COLUMNS} FROM site_domain_purchases \
             WHERE tenant_id = $1 AND site_id = $2 \
             ORDER BY created_at DESC, id DESC LIMIT $3"
        ))
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(PurchaseRow::into_purchase).collect()
    }

    /// The registrant of one purchase — the deliberate read this module's
    /// header promises, for the person checking what they are about to submit
    /// to a registry. Never part of a list.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the purchase isn't this tenant's;
    /// [`StoreError::Conflict`] if the stored registrant is unreadable;
    /// [`StoreError::Db`].
    pub async fn site_domain_purchase_registrant(
        &self,
        purchase: &SiteDomainPurchaseId,
    ) -> Result<RegistrantContact> {
        let stored: Option<Value> = sqlx::query_scalar(
            "SELECT registrant FROM site_domain_purchases WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(purchase.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let stored = stored.ok_or(StoreError::NotFound)?;
        serde_json::from_value(stored)
            .map_err(|_| StoreError::Conflict("the stored registrant is unreadable".to_owned()))
    }

    /// Records that this user agreed to **this exact price**.
    ///
    /// `agreed` is the quote the approving person had in front of them. A quote
    /// that no longer matches the row is [`StoreError::Conflict`]: a price that
    /// moved between the screen and the charge is the one thing the honest
    /// pricing rule exists to prevent, and re-quoting silently would defeat it.
    /// Approving an already-approved purchase at the same price is a no-op that
    /// returns the row.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]; [`StoreError::Conflict`] when the price
    /// changed or the purchase has moved past approval; [`StoreError::Db`].
    pub async fn approve_site_domain_purchase(
        &self,
        purchase: &SiteDomainPurchaseId,
        agreed: &DomainQuote,
    ) -> Result<SiteDomainPurchase> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let row = locked_purchase(&mut tx, self.tenant.as_str(), purchase).await?;
        let state = row.state()?;
        match state {
            SiteDomainPurchaseState::Quoted | SiteDomainPurchaseState::Approved => {}
            other => return Err(wrong_state(other)),
        }
        if row.domain != agreed.domain
            || row.term_years != i32::from(agreed.term_years)
            || row.currency != agreed.currency
            || row.first_term_cents != agreed.first_term_cents
            || row.renewal_cents_per_year != agreed.renewal_cents_per_year
            || row.premium != agreed.premium
        {
            return Err(StoreError::Conflict(
                "the price of that domain changed; check it again before approving".to_owned(),
            ));
        }
        if state == SiteDomainPurchaseState::Approved {
            tx.commit().await.map_err(StoreError::Db)?;
            return row.into_purchase();
        }
        let row = sqlx::query_as::<_, PurchaseRow>(&format!(
            "UPDATE site_domain_purchases \
                SET state = $3, approved_at = now(), approved_by = $4, updated_at = now() \
              WHERE tenant_id = $1 AND id = $2 \
          RETURNING {PURCHASE_COLUMNS}"
        ))
        .bind(self.tenant.as_str())
        .bind(purchase.as_str())
        .bind(SiteDomainPurchaseState::Approved.as_str())
        .bind(self.user.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        row.into_purchase()
    }

    /// Attaches Billing's opaque reference for the charge and moves the
    /// purchase to [`AwaitingPayment`](SiteDomainPurchaseState::AwaitingPayment).
    ///
    /// The reference is stored verbatim and never parsed: how the tenant is
    /// charged is Billing's business, and this module's only interest in it is
    /// that one payment settles one purchase. Repeating the call with the same
    /// reference returns the row.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]; [`StoreError::Validation`] for a malformed
    /// reference; [`StoreError::Conflict`] when the purchase is not approved,
    /// is already waiting for a different payment, or the reference already
    /// settles another purchase; [`StoreError::Db`].
    pub async fn await_site_domain_payment(
        &self,
        purchase: &SiteDomainPurchaseId,
        payment_reference: &str,
    ) -> Result<SiteDomainPurchase> {
        let reference = validate_payment_reference(payment_reference)?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let row = locked_purchase(&mut tx, self.tenant.as_str(), purchase).await?;
        match row.state()? {
            SiteDomainPurchaseState::Approved => {}
            SiteDomainPurchaseState::AwaitingPayment => {
                if row.payment_reference.as_deref() == Some(reference.as_str()) {
                    tx.commit().await.map_err(StoreError::Db)?;
                    return row.into_purchase();
                }
                return Err(StoreError::Conflict(
                    "this domain purchase is already waiting for another payment".to_owned(),
                ));
            }
            other => return Err(wrong_state(other)),
        }
        let row = sqlx::query_as::<_, PurchaseRow>(&format!(
            "UPDATE site_domain_purchases \
                SET state = $3, payment_reference = $4, updated_at = now() \
              WHERE tenant_id = $1 AND id = $2 \
          RETURNING {PURCHASE_COLUMNS}"
        ))
        .bind(self.tenant.as_str())
        .bind(purchase.as_str())
        .bind(SiteDomainPurchaseState::AwaitingPayment.as_str())
        .bind(&reference)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_purchase_unique)?;
        tx.commit().await.map_err(StoreError::Db)?;
        row.into_purchase()
    }

    /// Records that the payment settled, which is what puts the purchase in the
    /// registration sweep's queue.
    ///
    /// The reference must be the one this purchase is waiting for — a payment
    /// that belongs to something else may not buy this domain. Repeating the
    /// call on an already-settled purchase returns the row, so a webhook
    /// delivered twice is harmless.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]; [`StoreError::Validation`] for a malformed
    /// reference; [`StoreError::Conflict`] when the purchase is not waiting for
    /// a payment or the reference is a different one; [`StoreError::Db`].
    pub async fn settle_site_domain_payment(
        &self,
        purchase: &SiteDomainPurchaseId,
        payment_reference: &str,
    ) -> Result<SiteDomainPurchase> {
        let reference = validate_payment_reference(payment_reference)?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let row = locked_purchase(&mut tx, self.tenant.as_str(), purchase).await?;
        let state = row.state()?;
        if row.payment_reference.as_deref() != Some(reference.as_str()) {
            return Err(match state {
                SiteDomainPurchaseState::Quoted | SiteDomainPurchaseState::Approved => {
                    wrong_state(state)
                }
                _ => StoreError::Conflict(
                    "that payment does not belong to this domain purchase".to_owned(),
                ),
            });
        }
        if state.money_moved() {
            tx.commit().await.map_err(StoreError::Db)?;
            return row.into_purchase();
        }
        if state != SiteDomainPurchaseState::AwaitingPayment {
            return Err(wrong_state(state));
        }
        let row = sqlx::query_as::<_, PurchaseRow>(&format!(
            "UPDATE site_domain_purchases \
                SET state = $3, paid_at = now(), updated_at = now() \
              WHERE tenant_id = $1 AND id = $2 \
          RETURNING {PURCHASE_COLUMNS}"
        ))
        .bind(self.tenant.as_str())
        .bind(purchase.as_str())
        .bind(SiteDomainPurchaseState::Paid.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        row.into_purchase()
    }

    /// Calls a purchase off, which is only possible before the money moved.
    ///
    /// The row is kept as [`Cancelled`](SiteDomainPurchaseState::Cancelled) —
    /// somebody asked for something and changed their mind, and a screen that
    /// says so is kinder than an entry that vanishes. Cancelling twice returns
    /// the same row.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]; [`StoreError::Conflict`] once the purchase has
    /// been paid for, or has already failed; [`StoreError::Db`].
    pub async fn cancel_site_domain_purchase(
        &self,
        purchase: &SiteDomainPurchaseId,
    ) -> Result<SiteDomainPurchase> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let row = locked_purchase(&mut tx, self.tenant.as_str(), purchase).await?;
        let state = row.state()?;
        if state == SiteDomainPurchaseState::Cancelled {
            tx.commit().await.map_err(StoreError::Db)?;
            return row.into_purchase();
        }
        if state.money_moved() || state == SiteDomainPurchaseState::Failed {
            return Err(wrong_state(state));
        }
        let row = sqlx::query_as::<_, PurchaseRow>(&format!(
            "UPDATE site_domain_purchases \
                SET state = $3, updated_at = now() \
              WHERE tenant_id = $1 AND id = $2 \
          RETURNING {PURCHASE_COLUMNS}"
        ))
        .bind(self.tenant.as_str())
        .bind(purchase.as_str())
        .bind(SiteDomainPurchaseState::Cancelled.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        row.into_purchase()
    }

    /// Attaches a registered name to its website and finishes the purchase.
    ///
    /// A bought domain needs no TXT proof of ownership: alo registered it, on
    /// alo's nameservers, for this tenant. So the claim
    /// ([`crate::site_domains`]) is written straight to `live` here, in the
    /// same transaction that stamps the purchase
    /// [`Configured`](SiteDomainPurchaseState::Configured) — the state means
    /// "the name is attached", and it would be a lie if the two could come
    /// apart. A renewal has nothing to attach and only moves the state.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]; [`StoreError::Conflict`] when the purchase is
    /// not registered yet, or the name is connected to another website;
    /// [`StoreError::Db`].
    pub async fn configure_site_domain_purchase(
        &self,
        purchase: &SiteDomainPurchaseId,
    ) -> Result<SiteDomainPurchase> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let row = locked_purchase(&mut tx, self.tenant.as_str(), purchase).await?;
        let state = row.state()?;
        if state == SiteDomainPurchaseState::Configured {
            tx.commit().await.map_err(StoreError::Db)?;
            return row.into_purchase();
        }
        if state != SiteDomainPurchaseState::Registered {
            return Err(wrong_state(state));
        }
        if row.kind()? == SiteDomainPurchaseKind::Registration {
            sqlx::query(
                "INSERT INTO site_domains \
                     (tenant_id, site_id, domain, verify_token, status, verified_at) \
                 VALUES ($1, $2, $3, $4, 'live', now()) \
                 ON CONFLICT (tenant_id, site_id, domain) DO UPDATE \
                     SET status = 'live', verified_at = COALESCE(site_domains.verified_at, now()), \
                         updated_at = now()",
            )
            .bind(self.tenant.as_str())
            .bind(&row.site_id)
            .bind(&row.domain)
            .bind(id::generate_token())
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                if let sqlx::Error::Database(ref database) = error
                    && database.constraint() == Some("site_domains_domain_unique")
                {
                    return StoreError::Conflict(
                        "that domain is already connected to another website".to_owned(),
                    );
                }
                error.into()
            })?;
        }
        let row = sqlx::query_as::<_, PurchaseRow>(&format!(
            "UPDATE site_domain_purchases \
                SET state = $3, configured_at = now(), updated_at = now() \
              WHERE tenant_id = $1 AND id = $2 \
          RETURNING {PURCHASE_COLUMNS}"
        ))
        .bind(self.tenant.as_str())
        .bind(purchase.as_str())
        .bind(SiteDomainPurchaseState::Configured.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        row.into_purchase()
    }
}

impl Store {
    /// Claims up to `limit` paid purchases for registration, marking each
    /// [`Registering`](SiteDomainPurchaseState::Registering) in the statement
    /// that reads them. Concurrent sweeps skip each other's locked rows
    /// (`FOR UPDATE SKIP LOCKED`) rather than registering one name twice.
    ///
    /// The same call first writes off rows whose worker never came back and
    /// which have used up [`SITE_DOMAIN_PURCHASE_MAX_ATTEMPTS`], so an
    /// interrupted registration ends as a visible failure with a sentence about
    /// the money, rather than a row that stays `registering` forever.
    ///
    /// System-level by design: the sweep spans tenants, and every returned row
    /// names the tenant it belongs to. This is also the call that reads a
    /// [`RegistrantContact`] out of storage — deliberately, to hand it to the
    /// registrar and nowhere else.
    ///
    /// # Errors
    /// [`StoreError::Db`]; [`StoreError::Conflict`] if a stored token or
    /// registrant is unreadable.
    pub async fn claim_site_domain_registrations(
        &self,
        limit: i64,
    ) -> Result<Vec<DueSiteDomainRegistration>> {
        let mut tx = self.pool().begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "UPDATE site_domain_purchases \
                SET state = 'failed', failure = $1, updated_at = now() \
              WHERE state = 'registering' \
                AND claimed_at < now() - make_interval(mins => $2) \
                AND attempts >= $3",
        )
        .bind(SITE_DOMAIN_PURCHASE_INTERRUPTED)
        .bind(SITE_DOMAIN_PURCHASE_CLAIM_STALE_MINUTES)
        .bind(SITE_DOMAIN_PURCHASE_MAX_ATTEMPTS)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let rows = sqlx::query_as::<_, DueRow>(
            "UPDATE site_domain_purchases p \
                SET state = 'registering', claimed_at = now(), \
                    attempts = p.attempts + 1, updated_at = now() \
              WHERE (p.tenant_id, p.id) IN ( \
                    SELECT tenant_id, id FROM site_domain_purchases \
                     WHERE state = 'paid' \
                        OR (state = 'registering' \
                            AND claimed_at < now() - make_interval(mins => $2) \
                            AND attempts < $3) \
                     ORDER BY created_at, id \
                     LIMIT $1 \
                     FOR UPDATE SKIP LOCKED) \
          RETURNING p.tenant_id, p.site_id, p.id, p.kind, p.domain, p.term_years, \
                    p.registrant, p.nameservers, p.auto_renew, p.attempts",
        )
        .bind(limit)
        .bind(SITE_DOMAIN_PURCHASE_CLAIM_STALE_MINUTES)
        .bind(SITE_DOMAIN_PURCHASE_MAX_ATTEMPTS)
        .fetch_all(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        rows.into_iter().map(DueRow::into_due).collect()
    }

    /// Writes the registrar's answer onto a claimed purchase: it is registered,
    /// and here is the provider's own reference, the expiry it counted and the
    /// name's lifecycle.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the purchase isn't claimed or isn't that
    /// tenant's; [`StoreError::Db`].
    pub async fn complete_site_domain_registration(
        &self,
        tenant: &TenantId,
        purchase: &SiteDomainPurchaseId,
        registered: &RegisteredDomain,
    ) -> Result<()> {
        let done = sqlx::query(
            "UPDATE site_domain_purchases \
                SET state = 'registered', provider_reference = $3, registered_at = now(), \
                    expires_at = $4, lifecycle = $5, auto_renew = $6, failure = NULL, \
                    claimed_at = NULL, updated_at = now() \
              WHERE tenant_id = $1 AND id = $2 AND state = 'registering'",
        )
        .bind(tenant.as_str())
        .bind(purchase.as_str())
        .bind(&registered.provider_reference)
        .bind(registered.expires_at)
        .bind(registered.status.as_str())
        .bind(registered.auto_renew)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Puts a claimed purchase back in the queue after a fault that repeating
    /// could survive — a provider timeout, a registry that is briefly down.
    ///
    /// The attempt is already counted, so this is bounded: at
    /// [`SITE_DOMAIN_PURCHASE_MAX_ATTEMPTS`] the purchase fails visibly instead
    /// of circling forever. Returns the state the row ended in, so a worker can
    /// tell "we will try again" from "a person has to look at this".
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the purchase isn't claimed or isn't that
    /// tenant's; [`StoreError::Db`]; [`StoreError::Conflict`] if the resulting
    /// stored state is unreadable.
    pub async fn retry_site_domain_registration(
        &self,
        tenant: &TenantId,
        purchase: &SiteDomainPurchaseId,
        reason: &str,
    ) -> Result<SiteDomainPurchaseState> {
        let reason = failure_sentence(reason);
        let state: Option<String> = sqlx::query_scalar(
            "UPDATE site_domain_purchases \
                SET state = CASE WHEN attempts >= $4 THEN 'failed' ELSE 'paid' END, \
                    failure = $3, claimed_at = NULL, updated_at = now() \
              WHERE tenant_id = $1 AND id = $2 AND state = 'registering' \
          RETURNING state",
        )
        .bind(tenant.as_str())
        .bind(purchase.as_str())
        .bind(&reason)
        .bind(SITE_DOMAIN_PURCHASE_MAX_ATTEMPTS)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        let state = state.ok_or(StoreError::NotFound)?;
        SiteDomainPurchaseState::parse(&state)
            .ok_or_else(|| StoreError::Conflict("a purchase has an unknown state".to_owned()))
    }

    /// Fails a claimed purchase for good, with the reason the tenant needs.
    /// Terminal: a registry that refuses this order will refuse it identically
    /// in ten minutes, and the money that moved is a support conversation, not
    /// a retry.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the purchase isn't claimed or isn't that
    /// tenant's; [`StoreError::Db`].
    pub async fn fail_site_domain_registration(
        &self,
        tenant: &TenantId,
        purchase: &SiteDomainPurchaseId,
        reason: &str,
    ) -> Result<()> {
        let reason = failure_sentence(reason);
        let done = sqlx::query(
            "UPDATE site_domain_purchases \
                SET state = 'failed', failure = $3, claimed_at = NULL, updated_at = now() \
              WHERE tenant_id = $1 AND id = $2 AND state = 'registering'",
        )
        .bind(tenant.as_str())
        .bind(purchase.as_str())
        .bind(&reason)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

/// Trims a failure reason to what a row may hold. The reason is a registrar
/// error message by contract — never a provider's raw body, never registrant
/// data.
fn failure_sentence(reason: &str) -> String {
    reason
        .chars()
        .take(SITE_DOMAIN_PURCHASE_FAILURE_MAX_CHARS)
        .collect()
}

/// Reads and locks one purchase of `tenant` inside an open transaction, so a
/// state check and the write that depends on it cannot be interleaved with
/// another door's.
async fn locked_purchase(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    purchase: &SiteDomainPurchaseId,
) -> Result<PurchaseRow> {
    sqlx::query_as::<_, PurchaseRow>(&format!(
        "SELECT {PURCHASE_COLUMNS} FROM site_domain_purchases \
         WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    ))
    .bind(tenant)
    .bind(purchase.as_str())
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::Db)?
    .ok_or(StoreError::NotFound)
}

/// The columns every purchase read returns — one list, shared by the `SELECT`s
/// and by the `RETURNING` of every write that answers with a row. The
/// registrant is deliberately absent (see the module header).
const PURCHASE_COLUMNS: &str = "id, site_id, kind, domain, tld, state, term_years, currency, \
     first_term_cents, renewal_cents_per_year, premium, auto_renew, nameservers, request_key, \
     order_fingerprint, approved_at, approved_by, payment_reference, paid_at, claimed_at, \
     attempts, provider_reference, registered_at, expires_at, lifecycle, configured_at, failure, \
     created_at, updated_at";

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct PurchaseRow {
    id: String,
    site_id: String,
    kind: String,
    domain: String,
    tld: String,
    state: String,
    term_years: i32,
    currency: String,
    first_term_cents: i64,
    renewal_cents_per_year: i64,
    premium: bool,
    auto_renew: bool,
    nameservers: Value,
    request_key: String,
    order_fingerprint: String,
    approved_at: Option<OffsetDateTime>,
    approved_by: Option<String>,
    payment_reference: Option<String>,
    paid_at: Option<OffsetDateTime>,
    claimed_at: Option<OffsetDateTime>,
    attempts: i32,
    provider_reference: Option<String>,
    registered_at: Option<OffsetDateTime>,
    expires_at: Option<OffsetDateTime>,
    lifecycle: Option<String>,
    configured_at: Option<OffsetDateTime>,
    failure: Option<String>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

/// A stored token this process wrote and can no longer read is a schema
/// disagreement, not a caller's mistake — the same shape the other site modules
/// use for it.
fn unreadable(what: &str) -> StoreError {
    StoreError::Conflict(format!("a domain purchase has an unknown stored {what}"))
}

impl PurchaseRow {
    fn state(&self) -> Result<SiteDomainPurchaseState> {
        SiteDomainPurchaseState::parse(&self.state).ok_or_else(|| unreadable("state"))
    }

    fn kind(&self) -> Result<SiteDomainPurchaseKind> {
        SiteDomainPurchaseKind::parse(&self.kind).ok_or_else(|| unreadable("kind"))
    }

    fn into_purchase(self) -> Result<SiteDomainPurchase> {
        let state = self.state()?;
        let kind = self.kind()?;
        let term_years =
            u8::try_from(self.term_years).map_err(|_| unreadable("registration term"))?;
        let lifecycle = match self.lifecycle {
            Some(ref token) => {
                Some(DomainLifecycle::parse(token).ok_or_else(|| unreadable("state"))?)
            }
            None => None,
        };
        Ok(SiteDomainPurchase {
            id: SiteDomainPurchaseId::new(self.id),
            site: SiteId::new(self.site_id),
            kind,
            domain: self.domain,
            tld: self.tld,
            state,
            term_years,
            currency: self.currency,
            first_term_cents: self.first_term_cents,
            renewal_cents_per_year: self.renewal_cents_per_year,
            premium: self.premium,
            auto_renew: self.auto_renew,
            nameservers: read_nameservers(self.nameservers)?,
            request_key: self.request_key,
            approved_at: self.approved_at,
            approved_by: self.approved_by.map(UserId::new),
            payment_reference: self.payment_reference,
            paid_at: self.paid_at,
            claimed_at: self.claimed_at,
            attempts: self.attempts,
            provider_reference: self.provider_reference,
            registered_at: self.registered_at,
            expires_at: self.expires_at,
            lifecycle,
            configured_at: self.configured_at,
            failure: self.failure,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn read_nameservers(stored: Value) -> Result<Vec<String>> {
    serde_json::from_value(stored).map_err(|_| unreadable("nameserver list"))
}

#[derive(sqlx::FromRow)]
struct DueRow {
    tenant_id: String,
    site_id: String,
    id: String,
    kind: String,
    domain: String,
    term_years: i32,
    registrant: Value,
    nameservers: Value,
    auto_renew: bool,
    attempts: i32,
}

impl DueRow {
    fn into_due(self) -> Result<DueSiteDomainRegistration> {
        let kind = SiteDomainPurchaseKind::parse(&self.kind).ok_or_else(|| unreadable("kind"))?;
        let registrant: RegistrantContact =
            serde_json::from_value(self.registrant).map_err(|_| unreadable("registrant"))?;
        let years = u8::try_from(self.term_years).map_err(|_| unreadable("registration term"))?;
        Ok(DueSiteDomainRegistration {
            tenant: TenantId::new(self.tenant_id),
            site: SiteId::new(self.site_id),
            purchase: SiteDomainPurchaseId::new(self.id.clone()),
            kind,
            order: DomainOrder {
                domain: self.domain,
                years,
                registrant,
                nameservers: read_nameservers(self.nameservers)?,
                auto_renew: self.auto_renew,
                // The purchase id is the replay token: a second attempt at the
                // same row is one order to the registrar, not two.
                idempotency_key: self.id,
            },
            attempts: self.attempts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_tokens_are_stable() {
        for state in [
            SiteDomainPurchaseState::Quoted,
            SiteDomainPurchaseState::Approved,
            SiteDomainPurchaseState::AwaitingPayment,
            SiteDomainPurchaseState::Paid,
            SiteDomainPurchaseState::Registering,
            SiteDomainPurchaseState::Registered,
            SiteDomainPurchaseState::Configured,
            SiteDomainPurchaseState::Failed,
            SiteDomainPurchaseState::Cancelled,
        ] {
            assert_eq!(SiteDomainPurchaseState::parse(state.as_str()), Some(state));
        }
        assert_eq!(SiteDomainPurchaseState::parse("refunded"), None);
        for kind in [
            SiteDomainPurchaseKind::Registration,
            SiteDomainPurchaseKind::Renewal,
        ] {
            assert_eq!(SiteDomainPurchaseKind::parse(kind.as_str()), Some(kind));
        }
    }

    /// The line cancellation may not cross, stated once so a new state cannot
    /// be added on the wrong side of it by accident.
    #[test]
    fn money_moved_exactly_where_the_charge_has_happened() {
        for state in [
            SiteDomainPurchaseState::Quoted,
            SiteDomainPurchaseState::Approved,
            SiteDomainPurchaseState::AwaitingPayment,
            SiteDomainPurchaseState::Failed,
            SiteDomainPurchaseState::Cancelled,
        ] {
            assert!(!state.money_moved(), "{} charges nobody", state.as_str());
        }
        for state in [
            SiteDomainPurchaseState::Paid,
            SiteDomainPurchaseState::Registering,
            SiteDomainPurchaseState::Registered,
            SiteDomainPurchaseState::Configured,
        ] {
            assert!(state.money_moved(), "{} is paid for", state.as_str());
        }
        assert!(SiteDomainPurchaseState::Quoted.is_open());
        assert!(!SiteDomainPurchaseState::Configured.is_open());
        assert!(!SiteDomainPurchaseState::Cancelled.is_open());
    }

    #[test]
    fn payment_references_are_opaque_but_bounded() {
        assert_eq!(
            validate_payment_reference("  tr_8fj2Kd9  ").ok(),
            Some("tr_8fj2Kd9".to_owned())
        );
        // Whatever Billing mints is fine, as long as it is one printable token.
        assert!(validate_payment_reference("pi/2026-08/0001").is_ok());
        for bad in ["", "abc", "one two", "line\nbreak", &"x".repeat(201)] {
            assert!(
                validate_payment_reference(bad).is_err(),
                "accepted {bad:?} as a payment reference"
            );
        }
    }

    /// Every refusal names the rule and never quotes stored values back.
    #[test]
    fn refusals_say_what_is_wrong_without_quoting_anything() {
        for state in [
            SiteDomainPurchaseState::Quoted,
            SiteDomainPurchaseState::Paid,
            SiteDomainPurchaseState::Configured,
            SiteDomainPurchaseState::Failed,
        ] {
            let StoreError::Conflict(detail) = wrong_state(state) else {
                panic!("a wrong state must be a conflict");
            };
            assert!(!detail.is_empty());
            assert!(!detail.contains('@'), "{detail:?} looks like personal data");
        }
    }

    #[test]
    fn a_failure_sentence_is_bounded() {
        let long = "x".repeat(SITE_DOMAIN_PURCHASE_FAILURE_MAX_CHARS * 2);
        assert_eq!(
            failure_sentence(&long).chars().count(),
            SITE_DOMAIN_PURCHASE_FAILURE_MAX_CHARS
        );
        assert_eq!(failure_sentence("registry refused"), "registry refused");
    }
}
