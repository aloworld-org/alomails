//! The hosted-payment boundary — what alo may say to a payment provider and
//! what it may hear back (ADR 0041, item S3.04c).
//!
//! **Payments are never ours.** The buyer types their card on the provider's
//! hosted page, and this module is the type-level proof: the whole of what
//! crosses to a provider is [`SitePaymentRequest`] — an amount, a currency, a
//! description and two URLs — and the whole of what comes back is
//! [`SitePaymentCreated`] (an opaque id and where the checkout lives) and
//! [`SitePaymentStatus`]. There is no field a card number, an expiry, a CVC
//! or a cardholder could travel in, in either direction, and the test
//! `the_vocabulary_has_no_room_for_a_card` destructures both types
//! exhaustively so a field added tomorrow fails to compile until somebody
//! decides it may cross.
//!
//! **A webhook is a doorbell, not a message.** Providers in the Mollie shape
//! call a webhook with nothing but a payment id; the status is then asked of
//! the provider through [`SitePaymentProvider::payment_status`], so an
//! unauthenticated POST can make alo *look*, never make it *believe*. The
//! settle path ([`crate::site_ticket_orders`]) is built on that: it accepts a
//! status the caller fetched from the provider, never one a request body
//! asserted.
//!
//! The trait mirrors [`crate::site_registrar::DomainRegistrar`], the crate's
//! other money-moving boundary: an object-safe trait a route holds as
//! `dyn`, a deterministic in-memory fixture
//! ([`crate::site_payments_fixture::FixtureSitePayments`]) that tests and
//! local development run against, and an [`UnconfiguredSitePayments`] whose
//! typed refusal the surfaces above can branch on. A live Mollie or Adyen
//! adapter — both Dutch, per the ADR's sovereignty ordering — implements this
//! trait when a human wires one; no test may ever reach one.

use std::pin::Pin;

use crate::site_registrar::{is_eea_country, validate_idempotency_key};

/// Longest description a payment may carry to the provider — what the buyer
/// sees on the provider's page and their statement.
pub const SITE_PAYMENT_DESCRIPTION_MAX_CHARS: usize = 255;

/// Longest URL a payment request may carry.
pub const SITE_PAYMENT_URL_MAX_CHARS: usize = 2_000;

/// Sanity ceiling on one hosted payment, in cents: a basket of twenty seats,
/// not a corporate invoice. Anything above this is a mistake somewhere, and
/// refusing it here beats discovering it on a statement.
pub const SITE_PAYMENT_MAX_AMOUNT_CENTS: i64 = 10_000_000;

/// The future every provider call answers with — boxed so the trait stays
/// object-safe and a route can hold `dyn SitePaymentProvider`.
pub type SitePaymentFuture<'a, T> =
    Pin<Box<dyn std::future::Future<Output = SitePaymentResult<T>> + Send + 'a>>;

/// Shorthand for a provider-call outcome.
pub type SitePaymentResult<T> = std::result::Result<T, SitePaymentError>;

/// Why a payment provider could not answer.
///
/// No variant carries a buyer's personal data and none carries a provider's
/// raw response: an upstream body may quote credentials back at us, and these
/// messages are shown to people.
#[derive(Debug, thiserror::Error)]
pub enum SitePaymentError {
    /// No payment provider is wired into this installation. The surfaces
    /// above branch on this to hide the checkout rather than show a broken
    /// one — the same shape as the registrar and the AI paths.
    #[error("no payment provider is configured")]
    Unconfigured,
    /// The request is malformed — a field the caller can fix before retrying.
    /// The message names the violated rule.
    #[error("invalid input: {0}")]
    Validation(String),
    /// The request disagrees with something already done — most often an
    /// idempotency key reused for different parameters, which is a bug in the
    /// caller and must never silently charge a second time.
    #[error("conflict: {0}")]
    Conflict(String),
    /// The provider failed. `retryable` distinguishes "ask again in a minute"
    /// from "this will fail identically forever".
    #[error("payment provider unavailable: {message}")]
    Provider {
        /// Whether repeating the identical request could succeed.
        retryable: bool,
        /// A safe summary — never the provider's raw body.
        message: String,
    },
}

/// Which provider a request would reach, and whether its calls move money.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitePaymentIdentity {
    /// The provider's name, for operator-facing display.
    pub name: String,
    /// ISO-3166 alpha-2, lowercase, of the company that processes the money.
    pub country: String,
    /// Which of the provider's worlds this points at.
    pub environment: SitePaymentEnvironment,
}

impl SitePaymentIdentity {
    /// Names a provider, refusing one established outside the EEA.
    ///
    /// The sovereignty promise made mechanical, exactly as for registrars:
    /// the company that processes our customers' buyers' payments must be
    /// subject to European law. A processor elsewhere is a decision for a
    /// human and an ADR, not a configuration value.
    ///
    /// # Errors
    /// [`SitePaymentError::Validation`] for an empty name, a malformed
    /// country code, or a country outside the EU/EEA.
    pub fn new(
        name: &str,
        country: &str,
        environment: SitePaymentEnvironment,
    ) -> SitePaymentResult<Self> {
        let name = name.trim();
        if name.is_empty() {
            return Err(SitePaymentError::Validation(
                "a payment provider needs a name".to_owned(),
            ));
        }
        let country = country.trim().to_ascii_lowercase();
        if country.len() != 2 || !country.chars().all(|c| c.is_ascii_lowercase()) {
            return Err(SitePaymentError::Validation(
                "a provider country is a two-letter ISO code".to_owned(),
            ));
        }
        if !is_eea_country(&country) {
            return Err(SitePaymentError::Validation(format!(
                "payment provider '{name}' sits outside the EU/EEA; \
                 wiring one is an ADR, not a configuration value"
            )));
        }
        Ok(Self {
            name: name.to_owned(),
            country,
            environment,
        })
    }
}

/// Which world a provider's calls land in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SitePaymentEnvironment {
    /// In-memory, deterministic, no network, no money. Tests and local
    /// development run here and nowhere else.
    Fixture,
    /// The provider's test platform: real API, no real charges.
    Sandbox,
    /// The real thing. Charges are real money.
    Live,
}

impl SitePaymentEnvironment {
    /// Stable token for configuration and operator surfaces.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fixture => "fixture",
            Self::Sandbox => "sandbox",
            Self::Live => "live",
        }
    }

    /// Whether a call in this environment can charge somebody.
    #[must_use]
    pub fn spends_money(self) -> bool {
        matches!(self, Self::Live)
    }
}

/// Everything alo may tell a provider about one payment. This struct is the
/// entire outbound vocabulary: no card field exists, so no code path can send
/// one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitePaymentRequest {
    /// The caller's replay token — the order id, so a retried create reaches
    /// the payment it already made rather than charging twice.
    pub idempotency_key: String,
    /// What the buyer pays, in integer cents. Computed server-side; never a
    /// figure a request asserted.
    pub amount_cents: i64,
    /// ISO-4217 code, three uppercase ASCII letters.
    pub currency: String,
    /// What the buyer sees on the provider's page and their statement.
    pub description: String,
    /// Where the provider sends the buyer afterwards, paid or not.
    pub redirect_url: String,
    /// Where the provider rings the doorbell — the webhook that names a
    /// payment id and nothing else.
    pub webhook_url: String,
}

impl SitePaymentRequest {
    /// Checks the request against the boundary's rules. Every implementation
    /// calls this first, so a malformed request is refused identically
    /// whether the provider is a fixture or a live one.
    ///
    /// # Errors
    /// [`SitePaymentError::Validation`] naming the rule broken.
    pub fn validate(&self) -> SitePaymentResult<()> {
        validate_idempotency_key(&self.idempotency_key).map_err(|error| {
            SitePaymentError::Validation(match error {
                crate::site_registrar::RegistrarError::Validation(message) => message,
                other => other.to_string(),
            })
        })?;
        if !(1..=SITE_PAYMENT_MAX_AMOUNT_CENTS).contains(&self.amount_cents) {
            return Err(SitePaymentError::Validation(format!(
                "a payment is between 1 and {SITE_PAYMENT_MAX_AMOUNT_CENTS} cents"
            )));
        }
        if self.currency.len() != 3 || !self.currency.chars().all(|c| c.is_ascii_uppercase()) {
            return Err(SitePaymentError::Validation(
                "a currency is a three-letter uppercase ISO code".to_owned(),
            ));
        }
        let description = self.description.trim();
        if description.is_empty()
            || description.chars().count() > SITE_PAYMENT_DESCRIPTION_MAX_CHARS
        {
            return Err(SitePaymentError::Validation(format!(
                "a payment description is 1-{SITE_PAYMENT_DESCRIPTION_MAX_CHARS} characters"
            )));
        }
        if description.chars().any(char::is_control) {
            return Err(SitePaymentError::Validation(
                "a payment description may not contain control characters".to_owned(),
            ));
        }
        validate_payment_url(&self.redirect_url, "redirect")?;
        validate_payment_url(&self.webhook_url, "webhook")?;
        Ok(())
    }
}

/// Checks one of the request's URLs: https only — a hosted checkout that
/// reports back over plain http would leak the payment id in the clear.
/// Crate-visible because the order machinery holds a provider's checkout URL
/// to the same rule before storing it.
pub(crate) fn validate_payment_url(value: &str, what: &str) -> SitePaymentResult<()> {
    if value.len() > SITE_PAYMENT_URL_MAX_CHARS {
        return Err(SitePaymentError::Validation(format!(
            "a {what} URL is at most {SITE_PAYMENT_URL_MAX_CHARS} characters"
        )));
    }
    let rest = value.strip_prefix("https://").unwrap_or_default();
    if rest.is_empty() || value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(SitePaymentError::Validation(format!(
            "a {what} URL must be https:// and carry no spaces"
        )));
    }
    Ok(())
}

/// Everything a provider answers a create with: its own opaque id for the
/// payment, and where the buyer goes to pay. The card is typed there, on the
/// provider's page — there is no field it could come back in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitePaymentCreated {
    /// The provider's identifier — what its webhook will name, and the key
    /// [`SitePaymentProvider::payment_status`] is asked with. Opaque to alo.
    pub provider_payment_id: String,
    /// The hosted page the buyer is sent to.
    pub checkout_url: String,
}

/// Where one payment stands, in the five words the order state machine needs.
/// A provider's richer vocabulary (pending, authorized, …) collapses onto
/// [`Open`](Self::Open) in its adapter — from alo's side a payment is
/// underway, done, or dead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SitePaymentStatus {
    /// The buyer has not finished; the outcome is still open.
    Open,
    /// The money moved.
    Paid,
    /// The payment failed.
    Failed,
    /// The buyer cancelled on the provider's page.
    Canceled,
    /// The provider's checkout lapsed before the buyer finished.
    Expired,
}

impl SitePaymentStatus {
    /// The stable token this status is named by on the wire and in tests.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Paid => "paid",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
            Self::Expired => "expired",
        }
    }

    /// Reads a token back.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "paid" => Some(Self::Paid),
            "failed" => Some(Self::Failed),
            "canceled" => Some(Self::Canceled),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }

    /// Whether this status can still change. An open payment is in flight;
    /// the other four are the provider's final word.
    #[must_use]
    pub fn is_final(self) -> bool {
        !matches!(self, Self::Open)
    }
}

/// A hosted-payment provider: create a payment, learn where it stands.
///
/// The contract every implementation is held to:
/// - [`create_payment`](Self::create_payment) is idempotent under the
///   request's `idempotency_key` — the same key with the same request returns
///   the payment already made; the same key with a *different* request is
///   [`SitePaymentError::Conflict`], never a second charge;
/// - [`payment_status`](Self::payment_status) answers from the provider's own
///   record — it is the truth a webhook makes us fetch, never a cache of what
///   a request body claimed;
/// - errors never carry a buyer's data or the provider's raw response.
pub trait SitePaymentProvider: Send + Sync {
    /// Who this provider is and whether its calls move real money.
    fn identity(&self) -> SitePaymentIdentity;

    /// Creates a hosted payment: the provider mints an id and a checkout URL,
    /// and the buyer pays there. Idempotent under the request's key.
    fn create_payment(
        &self,
        request: SitePaymentRequest,
    ) -> SitePaymentFuture<'_, SitePaymentCreated>;

    /// Where one payment stands, by the provider's own id — the call a
    /// webhook triggers, and the only account of a payment alo believes.
    fn payment_status(
        &self,
        provider_payment_id: String,
    ) -> SitePaymentFuture<'_, SitePaymentStatus>;
}

/// The provider an installation has until a human wires a real one.
///
/// Every method answers [`SitePaymentError::Unconfigured`], a typed answer
/// the surfaces above branch on: a shop with no way to take money shows no
/// checkout, rather than a broken one.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnconfiguredSitePayments;

impl SitePaymentProvider for UnconfiguredSitePayments {
    fn identity(&self) -> SitePaymentIdentity {
        SitePaymentIdentity {
            name: "none".to_owned(),
            country: "eu".to_owned(),
            environment: SitePaymentEnvironment::Fixture,
        }
    }

    fn create_payment(
        &self,
        _request: SitePaymentRequest,
    ) -> SitePaymentFuture<'_, SitePaymentCreated> {
        Box::pin(async { Err(SitePaymentError::Unconfigured) })
    }

    fn payment_status(
        &self,
        _provider_payment_id: String,
    ) -> SitePaymentFuture<'_, SitePaymentStatus> {
        Box::pin(async { Err(SitePaymentError::Unconfigured) })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn request() -> SitePaymentRequest {
        SitePaymentRequest {
            idempotency_key: "order-0001".to_owned(),
            amount_cents: 25_500,
            currency: "EUR".to_owned(),
            description: "3 × Letterpress workshop".to_owned(),
            redirect_url: "https://axon.alosites.com/tickets/thanks".to_owned(),
            webhook_url: "https://axon.alosites.com/pay/webhook".to_owned(),
        }
    }

    #[test]
    fn the_vocabulary_has_no_room_for_a_card() {
        // Outbound: these six fields are everything alo may tell a provider.
        // A card number, expiry, CVC or cardholder has no field to travel in,
        // and a field added tomorrow fails to compile here until this proof
        // names it.
        let SitePaymentRequest {
            idempotency_key: _,
            amount_cents: _,
            currency: _,
            description: _,
            redirect_url: _,
            webhook_url: _,
        } = request();
        // Inbound: everything a provider may answer with is an opaque id,
        // a checkout URL, and one of five statuses. Nothing about the buyer
        // or their instrument can come back.
        let SitePaymentCreated {
            provider_payment_id: _,
            checkout_url: _,
        } = SitePaymentCreated {
            provider_payment_id: "fixpay-1".to_owned(),
            checkout_url: "https://checkout.fixture.invalid/fixpay-1".to_owned(),
        };
        for status in [
            SitePaymentStatus::Open,
            SitePaymentStatus::Paid,
            SitePaymentStatus::Failed,
            SitePaymentStatus::Canceled,
            SitePaymentStatus::Expired,
        ] {
            assert_eq!(SitePaymentStatus::parse(status.as_str()), Some(status));
        }
    }

    #[test]
    fn a_well_formed_request_passes_and_each_rule_refuses() {
        assert!(request().validate().is_ok());

        let mut bad = request();
        bad.amount_cents = 0;
        assert!(matches!(
            bad.validate(),
            Err(SitePaymentError::Validation(_))
        ));

        let mut bad = request();
        bad.amount_cents = SITE_PAYMENT_MAX_AMOUNT_CENTS + 1;
        assert!(matches!(
            bad.validate(),
            Err(SitePaymentError::Validation(_))
        ));

        let mut bad = request();
        bad.currency = "eur".to_owned();
        assert!(matches!(
            bad.validate(),
            Err(SitePaymentError::Validation(_))
        ));

        let mut bad = request();
        bad.description = String::new();
        assert!(matches!(
            bad.validate(),
            Err(SitePaymentError::Validation(_))
        ));

        let mut bad = request();
        bad.description = "line\nbreak".to_owned();
        assert!(matches!(
            bad.validate(),
            Err(SitePaymentError::Validation(_))
        ));

        // The hosted handoff is https or nothing.
        let mut bad = request();
        bad.redirect_url = "http://axon.alosites.com/tickets/thanks".to_owned();
        assert!(matches!(
            bad.validate(),
            Err(SitePaymentError::Validation(_))
        ));

        let mut bad = request();
        bad.webhook_url = "https://".to_owned();
        assert!(matches!(
            bad.validate(),
            Err(SitePaymentError::Validation(_))
        ));

        let mut bad = request();
        bad.idempotency_key = String::new();
        assert!(matches!(
            bad.validate(),
            Err(SitePaymentError::Validation(_))
        ));
    }

    #[test]
    fn a_provider_outside_the_eea_is_refused_by_construction() {
        assert!(SitePaymentIdentity::new("mollie", "nl", SitePaymentEnvironment::Fixture).is_ok());
        let refused = SitePaymentIdentity::new("stripe", "us", SitePaymentEnvironment::Live);
        assert!(matches!(refused, Err(SitePaymentError::Validation(_))));
        assert!(matches!(
            SitePaymentIdentity::new("", "nl", SitePaymentEnvironment::Fixture),
            Err(SitePaymentError::Validation(_))
        ));
    }

    #[test]
    fn only_the_live_environment_spends_money() {
        assert!(!SitePaymentEnvironment::Fixture.spends_money());
        assert!(!SitePaymentEnvironment::Sandbox.spends_money());
        assert!(SitePaymentEnvironment::Live.spends_money());
    }

    #[tokio::test]
    async fn the_unconfigured_provider_answers_with_its_type() {
        let provider = UnconfiguredSitePayments;
        assert!(matches!(
            provider.create_payment(request()).await,
            Err(SitePaymentError::Unconfigured)
        ));
        assert!(matches!(
            provider.payment_status("fixpay-1".to_owned()).await,
            Err(SitePaymentError::Unconfigured)
        ));
    }
}
