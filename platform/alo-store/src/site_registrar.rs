//! Buying a domain inside alo: what a registrar must be able to do, what a
//! name costs, and the promise that price carries.
//!
//! `docs/features.md` lists **"sell domains in-product"** at tier `[S+]` with
//! two sentences that decide almost everything in this module: it is built as a
//! **reseller over an EU wholesale registrar** (Openprovider / Realtime
//! Register / INWX class — `docs/guides/openprovider-domains.md`), and it is
//! **honest flat pricing, no first-year-bait renewals**, thin margin by design.
//! This module is where those two sentences become types.
//!
//! # The seam, and why nothing here reaches the network
//!
//! [`DomainRegistrar`] is the whole surface a provider must implement: the
//! catalog it sells, availability, a price, a registration, a renewal, and what
//! it knows about a name we already bought. One implementation ships in this
//! repository — [`crate::site_registrar_fixture::FixtureRegistrar`], entirely in
//! memory — and a second one, speaking a real reseller API, is wired by a human
//! who has credentials, an IP allowlist and a sandbox account. **No code path
//! in this crate opens a socket to a registrar**, which is what makes the
//! fixture suite a contract rather than a smoke test: both implementations
//! answer the same questions, and only one of them can spend money.
//!
//! [`UnconfiguredRegistrar`] is what an installation holds until that day. It
//! answers every question with [`RegistrarError::Unconfigured`] so the surfaces
//! above can say "domain buying is not available here" instead of showing a
//! search box that fails at the price.
//!
//! # Honest pricing is a type invariant, not a policy document
//!
//! A bait price — cheap first year, expensive renewal — is refused twice over:
//! [`TldOffer::validate`] rejects a catalog entry whose first-year price is
//! below its renewal price, and [`DomainQuote::new`] rejects the same shape at
//! the moment a person is quoted. [`RetailPolicy`] applies the identical markup
//! to both numbers, so a wholesale price list that is honest stays honest after
//! our margin is added. A quote states the renewal price **beside** the price
//! being paid today; there is no shape of this type that can hide it.
//!
//! **The alternative rejected:** passing the provider's prices straight
//! through, quoting whatever the reseller's response happens to say. It is less
//! code and it is what most integrations do — and it would carry a wholesale
//! bait price to a buyer untouched, with alo's name on the invoice. A validated
//! catalog costs one type and makes the promise unbreakable by a price list we
//! do not control.
//!
//! # Out of scope here
//!
//! No route, no screen, no stored row: the tenant-scoped purchase state machine
//! (quote → explicit approval → payment reference → register/configure/renew,
//! with Billing behind its own seam) is the item after this one, and the search
//! and checkout screens the one after that. Transfers-in, DNSSEC, registry
//! trustee services and WHOIS privacy products are not modelled at all —
//! [`TldOffer::transfer_cents`] exists because a price list has the number, not
//! because a transfer can be started from here.
//!
//! # Money, tax and tenancy
//!
//! Every amount is integer cents, VAT **exclusive**, in euro: an EU wholesale
//! registrar bills in euro, and VAT is Billing's to compute at checkout against
//! the buyer's country — never guessed here.
//!
//! Nothing in this module is tenant-scoped, because nothing in it stores
//! anything: it is a pure model plus an outbound interface. The tenant-scoped
//! record of *which tenant asked for which domain, at which price, and who
//! approved it* is the purchase state machine that follows this item, and it is
//! the only thing that may hold a [`RegistrantContact`] at rest.
//!
//! # The registrant is a person
//!
//! [`RegistrantContact`] is name, address, e-mail and telephone number: the
//! data the registry requires by contract, and the only personal data this path
//! sends anywhere. It is never logged, never put in an error message, and never
//! echoed back into a search result — the errors here name rules and fields,
//! never values.

use std::pin::Pin;

use time::OffsetDateTime;

use crate::error::StoreError;
use crate::site_domains::normalize_site_domain;

/// The only currency this path prices in. An EU wholesale registrar bills in
/// euro; a provider offering anything else is refused by [`TldOffer::validate`]
/// rather than converted, because a converted price is a price that moves
/// between the search result and the invoice.
pub const REGISTRAR_CURRENCY: &str = "EUR";

/// Shortest registration term, in years. Every registry sells at least one.
pub const TERM_YEARS_MIN: u8 = 1;

/// Longest registration term, in years. Registries cap at ten; so do we.
pub const TERM_YEARS_MAX: u8 = 10;

/// Most a single year of a single domain may cost, in cents (€10 000). Above
/// this the number is a mistake in a price list, not a domain — and a mistake
/// in a price list is a charge somebody has to reverse.
pub const DOMAIN_PRICE_MAX_CENTS: i64 = 1_000_000;

/// Most endings one search may ask about. A search box is not a bulk API.
pub const SEARCH_TLDS_MAX: usize = 12;

/// Longest label (the part before the ending) a search accepts — the DNS limit.
pub const DOMAIN_LABEL_MAX: usize = 63;

/// Fewest nameservers a registration may carry. Every registry requires two.
pub const NAMESERVERS_MIN: usize = 2;

/// Most nameservers a registration may carry.
pub const NAMESERVERS_MAX: usize = 6;

/// Shortest idempotency key a purchase may carry.
pub const IDEMPOTENCY_KEY_MIN: usize = 8;

/// Longest idempotency key a purchase may carry.
pub const IDEMPOTENCY_KEY_MAX: usize = 64;

/// Longest free-text field on a registrant contact.
pub const CONTACT_FIELD_MAX: usize = 120;

/// Result of anything a registrar is asked to do.
pub type RegistrarResult<T> = std::result::Result<T, RegistrarError>;

/// The boxed future every [`DomainRegistrar`] method returns.
///
/// `async fn` in a trait cannot be used behind `dyn`, and a registrar is held
/// in application state as a trait object precisely so the wiring can change
/// without the callers changing. Same shape as the DNS seam in
/// `alo-auth-mail`'s resolver.
pub type RegistrarFuture<'a, T> =
    Pin<Box<dyn std::future::Future<Output = RegistrarResult<T>> + Send + 'a>>;

/// Why a registrar could not answer.
///
/// No variant carries a domain owner's personal data, and none carries a
/// provider's raw response: an upstream body may quote our credentials back at
/// us, and these messages are shown to people.
#[derive(Debug, thiserror::Error)]
pub enum RegistrarError {
    /// No registrar is wired into this installation. The surfaces above branch
    /// on this to hide the buy box rather than to show a broken one.
    #[error("no domain registrar is configured")]
    Unconfigured,
    /// The request is malformed — a field the caller can fix before retrying.
    /// The message names the violated rule.
    #[error("invalid input: {0}")]
    Validation(String),
    /// We do not sell this ending. Distinct from [`Self::Unavailable`]: the
    /// name may well be free, we simply have nothing to sell.
    #[error("we do not sell .{tld} domains")]
    Unsupported {
        /// The ending, without its leading dot.
        tld: String,
    },
    /// The name cannot be bought right now: somebody owns it, or the registry
    /// blocks it. Also what a race at registration time looks like — available
    /// in the search, gone by the purchase.
    #[error("that domain is not available")]
    Unavailable,
    /// The request disagrees with something already done — most often an
    /// idempotency key reused for different parameters, which is a bug in the
    /// caller and must never silently register a second name.
    #[error("conflict: {0}")]
    Conflict(String),
    /// The provider failed. `retryable` distinguishes "ask again in a minute"
    /// from "this will fail identically forever", which is the difference
    /// between a queue and a person.
    #[error("registrar unavailable: {message}")]
    Provider {
        /// Whether repeating the identical request could succeed.
        retryable: bool,
        /// A safe summary — never the provider's raw body.
        message: String,
    },
}

impl From<StoreError> for RegistrarError {
    /// Validation raised by the shared DNS-name rules keeps its sentence; a
    /// store error of any other kind has no business on this path and becomes a
    /// non-retryable provider fault rather than being silently widened.
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Validation(message) => Self::Validation(message),
            other => Self::Provider {
                retryable: false,
                message: other.to_string(),
            },
        }
    }
}

/// Which registrar a request would reach, and whether it can spend money.
///
/// Held by every implementation so an operator can see, in one place, who
/// registers their customers' domains and in which country that company sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrarIdentity {
    /// The reseller platform's name, for operator-facing display.
    pub name: String,
    /// ISO-3166 alpha-2, lowercase, of the company we resell through.
    pub country: String,
    /// Which of the provider's worlds this points at.
    pub environment: RegistrarEnvironment,
}

/// Which world a registrar's calls land in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrarEnvironment {
    /// In-memory, deterministic, no network, no money. Tests and local
    /// development run here and nowhere else.
    Fixture,
    /// The provider's test platform: real API, no real registrations.
    Sandbox,
    /// The real thing. Registrations cost money and are hard to undo.
    Live,
}

impl RegistrarEnvironment {
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

impl RegistrarIdentity {
    /// Names a provider, refusing one established outside the EEA.
    ///
    /// This is the sovereignty promise made mechanical: alo is a European
    /// product, and the company that ends up holding our customers' registrant
    /// data must be subject to European law. A registrar elsewhere is a
    /// decision for a human and an ADR, not a configuration value.
    ///
    /// # Errors
    /// [`RegistrarError::Validation`] for an empty name, a malformed country
    /// code, or a country outside the EU/EEA.
    pub fn new(
        name: &str,
        country: &str,
        environment: RegistrarEnvironment,
    ) -> RegistrarResult<Self> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > CONTACT_FIELD_MAX {
            return Err(RegistrarError::Validation(format!(
                "registrar name must be 1-{CONTACT_FIELD_MAX} characters"
            )));
        }
        let country = normalize_country(country)?;
        if !is_eea_country(&country) {
            return Err(RegistrarError::Validation(
                "the domain registrar must be established in the EU or EEA".to_owned(),
            ));
        }
        Ok(Self {
            name: name.to_owned(),
            country,
            environment,
        })
    }
}

/// The EU/EEA, lowercase ISO-3166 alpha-2, sorted. Iceland, Liechtenstein and
/// Norway are in: the EEA agreement puts them under the same data-protection
/// regime, which is the property this list is actually testing for.
const EEA_COUNTRIES: [&str; 30] = [
    "at", "be", "bg", "cy", "cz", "de", "dk", "ee", "es", "fi", "fr", "gr", "hr", "hu", "ie", "is",
    "it", "li", "lt", "lu", "lv", "mt", "nl", "no", "pl", "pt", "ro", "se", "si", "sk",
];

/// Whether a lowercase alpha-2 code names an EU/EEA country.
#[must_use]
pub fn is_eea_country(code: &str) -> bool {
    EEA_COUNTRIES.binary_search(&code).is_ok()
}

/// Lowercases and checks an ISO-3166 alpha-2 country code.
fn normalize_country(value: &str) -> RegistrarResult<String> {
    let code = value.trim().to_ascii_lowercase();
    if code.len() != 2 || !code.bytes().all(|byte| byte.is_ascii_lowercase()) {
        return Err(RegistrarError::Validation(
            "country must be a two-letter ISO-3166 code, such as nl".to_owned(),
        ));
    }
    Ok(code)
}

// ---- what a name costs ------------------------------------------------------

/// Turning a wholesale price into the price a customer sees.
///
/// The margin is deliberately thin — this feature is the onboarding closer, not
/// a profit line — and it is applied **identically to registration and
/// renewal**, which is the mechanical reason an honest wholesale price list
/// cannot become a baiting retail one on the way through alo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetailPolicy {
    /// Margin in basis points of the wholesale price (1 500 = 15 %).
    pub markup_bp: u16,
    /// Floor on the margin, in cents, for the cheap endings where a percentage
    /// would not cover the payment fee.
    pub min_markup_cents: i64,
}

impl RetailPolicy {
    /// Largest margin this type will express: doubling a wholesale price is not
    /// "thin by design", and a mistyped basis-point figure should be refused
    /// rather than charged.
    pub const MARKUP_BP_MAX: u16 = 5_000;

    /// The shipped posture: 15 % over wholesale, at least 100 cents.
    pub const THIN: Self = Self {
        markup_bp: 1_500,
        min_markup_cents: 100,
    };

    /// The retail price for one wholesale price, rounded **up** to the cent.
    ///
    /// Rounding up rather than to nearest: a rounded-down cent is a margin we
    /// chose to explain in a spreadsheet later, and the difference to the buyer
    /// is one cent.
    ///
    /// # Errors
    /// [`RegistrarError::Validation`] for a non-positive or absurd wholesale
    /// price, a margin above [`Self::MARKUP_BP_MAX`], or a result past
    /// [`DOMAIN_PRICE_MAX_CENTS`].
    pub fn retail(&self, wholesale_cents: i64) -> RegistrarResult<i64> {
        if self.markup_bp > Self::MARKUP_BP_MAX || self.min_markup_cents < 0 {
            return Err(RegistrarError::Validation(format!(
                "retail markup must be 0-{} basis points over a non-negative floor",
                Self::MARKUP_BP_MAX
            )));
        }
        if wholesale_cents <= 0 || wholesale_cents > DOMAIN_PRICE_MAX_CENTS {
            return Err(RegistrarError::Validation(format!(
                "wholesale price must be 1-{DOMAIN_PRICE_MAX_CENTS} cents"
            )));
        }
        // i128 so the multiplication cannot wrap before the ceiling check.
        let scaled = i128::from(wholesale_cents) * i128::from(self.markup_bp);
        let percentage =
            i64::try_from(scaled.div_euclid(10_000) + i128::from(scaled % 10_000 != 0))
                .unwrap_or(i64::MAX);
        let margin = percentage.max(self.min_markup_cents);
        let retail = wholesale_cents.saturating_add(margin);
        if retail > DOMAIN_PRICE_MAX_CENTS {
            return Err(RegistrarError::Validation(format!(
                "retail price must be at most {DOMAIN_PRICE_MAX_CENTS} cents"
            )));
        }
        Ok(retail)
    }
}

/// What the registry demands of whoever registers under an ending.
///
/// Typed, not prose: the surfaces above translate these into the language the
/// buyer reads, and the purchase flow can refuse a registrant who cannot
/// satisfy one before taking any money.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TldRequirement {
    /// Anybody, anywhere.
    None,
    /// The registrant must be resident or established in the EU/EEA — `.eu`.
    EeaPresence,
    /// The registrant must have a presence in one specific country.
    CountryPresence {
        /// ISO-3166 alpha-2, lowercase.
        country: String,
    },
}

/// One ending we sell, at the prices we sell it for.
///
/// Prices are retail, per year, VAT exclusive, in [`REGISTRAR_CURRENCY`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TldOffer {
    /// The ending without its leading dot: `com`, `eu`, `co.uk`.
    pub tld: String,
    /// One year of a new registration.
    pub register_cents: i64,
    /// One year of renewal — what it costs every year after, forever.
    pub renew_cents: i64,
    /// Bringing an existing domain here.
    pub transfer_cents: i64,
    /// Shortest term the registry sells.
    pub min_years: u8,
    /// Longest term the registry sells.
    pub max_years: u8,
    /// What the registry demands of the registrant.
    pub requirement: TldRequirement,
}

impl TldOffer {
    /// Checks one catalog entry, including the honest-pricing rule.
    ///
    /// # Errors
    /// [`RegistrarError::Validation`], naming the rule broken.
    pub fn validate(&self) -> RegistrarResult<()> {
        validate_tld(&self.tld)?;
        for (label, cents) in [
            ("registration", self.register_cents),
            ("renewal", self.renew_cents),
            ("transfer", self.transfer_cents),
        ] {
            if cents <= 0 || cents > DOMAIN_PRICE_MAX_CENTS {
                return Err(RegistrarError::Validation(format!(
                    "{label} price for .{} must be 1-{DOMAIN_PRICE_MAX_CENTS} cents",
                    self.tld
                )));
            }
        }
        if self.register_cents < self.renew_cents {
            return Err(RegistrarError::Validation(format!(
                ".{}: the first year may not cost less than the renewal — \
                 alo does not sell bait pricing",
                self.tld
            )));
        }
        if self.min_years < TERM_YEARS_MIN
            || self.max_years > TERM_YEARS_MAX
            || self.min_years > self.max_years
        {
            return Err(RegistrarError::Validation(format!(
                ".{}: term must be a range inside {TERM_YEARS_MIN}-{TERM_YEARS_MAX} years",
                self.tld
            )));
        }
        if let TldRequirement::CountryPresence { country } = &self.requirement {
            let normalized = normalize_country(country)?;
            if normalized != *country {
                return Err(RegistrarError::Validation(
                    "country requirement must be a lowercase two-letter ISO-3166 code".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

/// Checks an ending: lowercase ASCII labels, one to four of them, no dots at
/// either end. `co.uk` is an ending; `.com` with its dot, or `example.com`, is
/// not.
fn validate_tld(tld: &str) -> RegistrarResult<()> {
    let bad = tld.is_empty()
        || tld.len() > 24
        || tld.split('.').count() > 4
        || tld.split('.').any(|label| {
            label.is_empty()
                || label.len() > DOMAIN_LABEL_MAX
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                || label.starts_with('-')
                || label.ends_with('-')
        });
    if bad {
        return Err(RegistrarError::Validation(
            "an ending is lowercase letters, digits and hyphens, such as com or co.uk, \
             written without its leading dot"
                .to_owned(),
        ));
    }
    Ok(())
}

/// The endings one provider sells, in the order a buyer should see them.
///
/// Order is the operator's editorial choice — the first entries are what a
/// search offers when the buyer names no ending — and it is preserved exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TldCatalog {
    offers: Vec<TldOffer>,
}

impl TldCatalog {
    /// Builds a catalog, validating every entry and refusing duplicates.
    ///
    /// # Errors
    /// [`RegistrarError::Validation`] for an empty catalog, a duplicate
    /// ending, or any entry [`TldOffer::validate`] refuses.
    pub fn new(offers: Vec<TldOffer>) -> RegistrarResult<Self> {
        if offers.is_empty() {
            return Err(RegistrarError::Validation(
                "a registrar catalog must offer at least one ending".to_owned(),
            ));
        }
        for (index, offer) in offers.iter().enumerate() {
            offer.validate()?;
            if offers[..index].iter().any(|other| other.tld == offer.tld) {
                return Err(RegistrarError::Validation(format!(
                    ".{} appears twice in the catalog",
                    offer.tld
                )));
            }
        }
        Ok(Self { offers })
    }

    /// Every ending sold, in editorial order.
    #[must_use]
    pub fn offers(&self) -> &[TldOffer] {
        &self.offers
    }

    /// The entry for one ending, if we sell it.
    #[must_use]
    pub fn offer(&self, tld: &str) -> Option<&TldOffer> {
        self.offers.iter().find(|offer| offer.tld == tld)
    }

    /// Splits a typed name into the ending we sell and the label in front of it.
    ///
    /// Forgiving on the way in and strict on the way out, the same rule the
    /// site-address field learned the hard way (S1.30b): a scheme, a path, a
    /// port, a trailing dot or capitals are tidied away, and what remains must
    /// be exactly one label plus one ending from this catalog. `shop.acme.com`
    /// is refused by name — it is a subdomain of something else, and buying it
    /// is not a thing that can happen.
    ///
    /// # Errors
    /// [`RegistrarError::Validation`] for a malformed name;
    /// [`RegistrarError::Unsupported`] for an ending we do not sell.
    pub fn parse(&self, input: &str) -> RegistrarResult<RegistrableDomain> {
        let tidy = tidy_domain_input(input);
        let name = normalize_site_domain(&tidy)?;
        // Longest ending first, so `co.uk` wins over `uk` when we sell both.
        let mut best: Option<&TldOffer> = None;
        for offer in &self.offers {
            let suffix = format!(".{}", offer.tld);
            if name.ends_with(&suffix)
                && name.len() > suffix.len()
                && best.is_none_or(|current| current.tld.len() < offer.tld.len())
            {
                best = Some(offer);
            }
        }
        let Some(offer) = best else {
            let tld = name
                .split_once('.')
                .map_or(name.clone(), |(_, rest)| rest.to_owned());
            return Err(RegistrarError::Unsupported { tld });
        };
        let label = name[..name.len() - offer.tld.len() - 1].to_owned();
        validate_label(&label)?;
        Ok(RegistrableDomain {
            name,
            label,
            tld: offer.tld.clone(),
        })
    }

    /// A quote for a name this catalog sells at its list price.
    ///
    /// `premium_per_year_cents` is the registry's own price for a name it holds
    /// back from the list; when present it replaces both list prices, because a
    /// premium name renews at its premium price and saying otherwise would be
    /// the bait this module exists to refuse.
    ///
    /// # Errors
    /// [`RegistrarError::Unsupported`] when the ending is not sold;
    /// [`RegistrarError::Validation`] for a term outside the ending's range or
    /// an unusable premium price.
    pub fn quote(
        &self,
        domain: &RegistrableDomain,
        years: u8,
        premium_per_year_cents: Option<i64>,
    ) -> RegistrarResult<DomainQuote> {
        let offer = self
            .offer(domain.tld())
            .ok_or_else(|| RegistrarError::Unsupported {
                tld: domain.tld().to_owned(),
            })?;
        if years < offer.min_years || years > offer.max_years {
            return Err(RegistrarError::Validation(format!(
                ".{} is sold for {}-{} years at a time",
                offer.tld, offer.min_years, offer.max_years
            )));
        }
        match premium_per_year_cents {
            Some(premium) => DomainQuote::new(domain.name(), years, premium, premium, true),
            None => DomainQuote::new(
                domain.name(),
                years,
                offer.register_cents,
                offer.renew_cents,
                false,
            ),
        }
    }
}

/// Tidies what a person typed into something the DNS rules can judge: trims,
/// lowercases, drops a scheme, a path, a query, a port and trailing dots.
fn tidy_domain_input(input: &str) -> String {
    let mut value = input.trim().to_ascii_lowercase();
    for scheme in ["https://", "http://"] {
        if let Some(rest) = value.strip_prefix(scheme) {
            value = rest.to_owned();
        }
    }
    for cut in ['/', '?', '#', ':'] {
        if let Some((head, _)) = value.split_once(cut) {
            value = head.to_owned();
        }
    }
    value.trim_end_matches('.').trim().to_owned()
}

/// Checks the part in front of the ending — the thing actually being bought.
fn validate_label(label: &str) -> RegistrarResult<()> {
    if label.is_empty() || label.len() > DOMAIN_LABEL_MAX {
        return Err(RegistrarError::Validation(format!(
            "a domain name is 1-{DOMAIN_LABEL_MAX} characters before the ending"
        )));
    }
    if label.contains('.') {
        return Err(RegistrarError::Validation(
            "a domain is bought as one name plus its ending, such as acme.com — \
             addresses below that are yours to create once you own it"
                .to_owned(),
        ));
    }
    if !label
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || label.starts_with('-')
        || label.ends_with('-')
    {
        return Err(RegistrarError::Validation(
            "a domain name may only contain lowercase letters, digits and hyphens, \
             and may not start or end with a hyphen"
                .to_owned(),
        ));
    }
    Ok(())
}

/// A name that can actually be registered: one label, one ending we sell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrableDomain {
    name: String,
    label: String,
    tld: String,
}

impl RegistrableDomain {
    /// The full name, lowercase — `acme.com`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The part being bought — `acme`.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The ending, without its dot — `com`.
    #[must_use]
    pub fn tld(&self) -> &str {
        &self.tld
    }
}

/// What one name costs, stated the way it will be charged.
///
/// [`first_term_cents`](Self::first_term_cents) is what the buyer pays now for
/// [`term_years`](Self::term_years) years;
/// [`renewal_cents_per_year`](Self::renewal_cents_per_year) is what it costs
/// every year afterwards. Both are VAT exclusive: the tax depends on where the
/// buyer is, and Billing owns that question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainQuote {
    /// The name quoted, lowercase.
    pub domain: String,
    /// How many years the first payment covers.
    pub term_years: u8,
    /// Always [`REGISTRAR_CURRENCY`].
    pub currency: String,
    /// The whole first term, in cents.
    pub first_term_cents: i64,
    /// One year of renewal, in cents.
    pub renewal_cents_per_year: i64,
    /// Whether the registry prices this name above its ending's list price.
    pub premium: bool,
}

impl DomainQuote {
    /// Builds a quote, enforcing the honest-pricing rule at the point of sale.
    ///
    /// Multi-year terms are `n × the yearly price`, not "one cheap year plus
    /// renewals": alo charges the same amount every year, which is the whole
    /// content of the promise.
    ///
    /// # Errors
    /// [`RegistrarError::Validation`] for a term outside
    /// [`TERM_YEARS_MIN`]..=[`TERM_YEARS_MAX`], a price outside
    /// `1..=`[`DOMAIN_PRICE_MAX_CENTS`], or a first year priced below renewal.
    pub fn new(
        domain: &str,
        term_years: u8,
        register_per_year_cents: i64,
        renewal_per_year_cents: i64,
        premium: bool,
    ) -> RegistrarResult<Self> {
        if !(TERM_YEARS_MIN..=TERM_YEARS_MAX).contains(&term_years) {
            return Err(RegistrarError::Validation(format!(
                "a domain is registered for {TERM_YEARS_MIN}-{TERM_YEARS_MAX} years at a time"
            )));
        }
        for cents in [register_per_year_cents, renewal_per_year_cents] {
            if cents <= 0 || cents > DOMAIN_PRICE_MAX_CENTS {
                return Err(RegistrarError::Validation(format!(
                    "a yearly domain price must be 1-{DOMAIN_PRICE_MAX_CENTS} cents"
                )));
            }
        }
        if register_per_year_cents < renewal_per_year_cents {
            return Err(RegistrarError::Validation(
                "the first year may not cost less than the renewal — \
                 alo does not sell bait pricing"
                    .to_owned(),
            ));
        }
        let first_term_cents = register_per_year_cents
            .checked_mul(i64::from(term_years))
            .ok_or_else(|| {
                RegistrarError::Validation("that term costs more than we can charge".to_owned())
            })?;
        Ok(Self {
            domain: domain.to_owned(),
            term_years,
            currency: REGISTRAR_CURRENCY.to_owned(),
            first_term_cents,
            renewal_cents_per_year: renewal_per_year_cents,
            premium,
        })
    }
}

// ---- searching --------------------------------------------------------------

/// Whether a name can be bought.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainAvailability {
    /// Free, and priced in the offer beside it.
    Available,
    /// Somebody already registered it.
    Taken,
    /// The registry will not sell it to anybody: reserved, in a sunrise
    /// period, or blocked by policy.
    Blocked,
    /// We do not sell this ending — the name may well be free elsewhere.
    Unsupported,
}

impl DomainAvailability {
    /// Stable token for wire surfaces.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Taken => "taken",
            Self::Blocked => "blocked",
            Self::Unsupported => "unsupported",
        }
    }
}

/// One line of a search result.
///
/// The quote is present exactly when [`availability`](Self::availability) is
/// [`DomainAvailability::Available`] — a price on something nobody can buy is
/// how a buy box lies — and [`DomainOffer::new`] is the only constructor, so
/// that pairing cannot come apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainOffer {
    /// The full name, lowercase.
    pub domain: String,
    /// Whether it can be bought.
    pub availability: DomainAvailability,
    /// What it would cost for one year, when it can be bought.
    pub quote: Option<DomainQuote>,
}

impl DomainOffer {
    /// Pairs a name with what may be done about it.
    ///
    /// # Errors
    /// [`RegistrarError::Validation`] when a price is attached to a name that
    /// cannot be bought, or missing from one that can.
    pub fn new(
        domain: &str,
        availability: DomainAvailability,
        quote: Option<DomainQuote>,
    ) -> RegistrarResult<Self> {
        let priced = quote.is_some();
        if priced != matches!(availability, DomainAvailability::Available) {
            return Err(RegistrarError::Validation(
                "only an available domain carries a price".to_owned(),
            ));
        }
        Ok(Self {
            domain: domain.to_owned(),
            availability,
            quote,
        })
    }
}

/// One name to check, produced by [`DomainSearch::candidates`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainCandidate {
    /// The full name, lowercase.
    pub domain: String,
    /// Its ending, without the dot.
    pub tld: String,
    /// Whether this catalog sells that ending. A candidate that does not is
    /// still returned, so the buyer is told rather than left wondering.
    pub supported: bool,
}

/// What a buyer typed into the search box, resolved against a catalog.
///
/// A person types `acme`, `Acme.com`, or `https://acme.com/` and means the same
/// thing; all three arrive here as the label `acme`, and the ending they named
/// — if they named one — is offered first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainSearch {
    label: String,
    tlds: Vec<String>,
    unsupported: Vec<String>,
}

impl DomainSearch {
    /// Reads a search box against a catalog.
    ///
    /// `requested_tlds` are the endings the buyer ticked; empty means "what you
    /// recommend", which is the head of the catalog's editorial order. An
    /// ending typed into the box itself always comes first, and one we do not
    /// sell is kept as an unsupported candidate rather than silently dropped.
    ///
    /// # Errors
    /// [`RegistrarError::Validation`] for an empty or malformed query, a
    /// malformed ending, or more than [`SEARCH_TLDS_MAX`] endings.
    pub fn parse(
        query: &str,
        requested_tlds: &[String],
        catalog: &TldCatalog,
    ) -> RegistrarResult<Self> {
        let tidy = tidy_domain_input(query);
        if tidy.is_empty() {
            return Err(RegistrarError::Validation(
                "type the name you would like, such as acme".to_owned(),
            ));
        }
        if requested_tlds.len() > SEARCH_TLDS_MAX {
            return Err(RegistrarError::Validation(format!(
                "one search may ask about at most {SEARCH_TLDS_MAX} endings"
            )));
        }
        // A typed name is read through the catalog first, so `acme.co.uk` is
        // one name under an ending we sell rather than the label `acme.co`.
        // Only when no ending matches do we fall back to splitting at the first
        // dot — and an address *inside* a domain we sell keeps the sharp
        // refusal [`TldCatalog::parse`] raises instead of becoming a search for
        // something nobody can buy.
        let (label, typed_tld) = if tidy.contains('.') {
            match catalog.parse(&tidy) {
                Ok(domain) => (domain.label().to_owned(), Some(domain.tld().to_owned())),
                Err(RegistrarError::Unsupported { tld }) => (
                    tidy.split_once('.')
                        .map_or(tidy.clone(), |(head, _)| head.to_owned()),
                    Some(tld),
                ),
                Err(other) => return Err(other),
            }
        } else {
            (tidy.clone(), None)
        };
        validate_label(&label)?;

        let mut tlds: Vec<String> = Vec::new();
        let mut unsupported: Vec<String> = Vec::new();
        let push = |tld: &str, tlds: &mut Vec<String>, unsupported: &mut Vec<String>| {
            if catalog.offer(tld).is_some() {
                if !tlds.iter().any(|known| known == tld) {
                    tlds.push(tld.to_owned());
                }
            } else if !unsupported.iter().any(|known| known == tld) {
                unsupported.push(tld.to_owned());
            }
        };

        if let Some(tld) = typed_tld.as_deref() {
            validate_tld(tld)?;
            push(tld, &mut tlds, &mut unsupported);
        }
        for tld in requested_tlds {
            let tld = tld.trim().trim_start_matches('.').to_ascii_lowercase();
            validate_tld(&tld)?;
            push(&tld, &mut tlds, &mut unsupported);
        }
        if tlds.is_empty() && unsupported.is_empty() {
            for offer in catalog.offers().iter().take(SEARCH_TLDS_MAX) {
                tlds.push(offer.tld.clone());
            }
        }
        Ok(Self {
            label,
            tlds,
            unsupported,
        })
    }

    /// The name being searched for, without any ending.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Every name this search asks about, in the order a buyer should see them:
    /// the ending they typed, then the ones they ticked, then — only when they
    /// named none at all — the catalog's own order. Endings we do not sell come
    /// last, marked, never priced.
    #[must_use]
    pub fn candidates(&self) -> Vec<DomainCandidate> {
        self.tlds
            .iter()
            .map(|tld| (tld, true))
            .chain(self.unsupported.iter().map(|tld| (tld, false)))
            .map(|(tld, supported)| DomainCandidate {
                domain: format!("{}.{tld}", self.label),
                tld: tld.clone(),
                supported,
            })
            .collect()
    }
}

// ---- buying -----------------------------------------------------------------

/// The person or company the registry will record as the owner.
///
/// This is the registrant data a registry requires by contract — the one piece
/// of personal data this path sends outside alo. It is validated here, carried
/// no further than the registrar, and never logged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrantContact {
    /// Full name of the person responsible.
    pub name: String,
    /// Company name, where the registrant is a company.
    pub organisation: Option<String>,
    /// Contact e-mail. Registries send expiry and verification notices here;
    /// an unreachable address loses the domain.
    pub email: String,
    /// Street and number.
    pub street: String,
    /// Postal code.
    pub postal_code: String,
    /// City.
    pub city: String,
    /// ISO-3166 alpha-2, lowercase. May be anywhere: our *registrar* must sit
    /// in the EEA, our customers need not.
    pub country: String,
    /// Telephone in international form, `+31201234567`.
    pub phone: String,
}

impl RegistrantContact {
    /// Checks every field a registry will reject us for.
    ///
    /// # Errors
    /// [`RegistrarError::Validation`], naming the field — never quoting its
    /// value back, because these messages are shown and logged upstream.
    pub fn validate(&self) -> RegistrarResult<()> {
        for (field, value) in [
            ("name", &self.name),
            ("street", &self.street),
            ("postal code", &self.postal_code),
            ("city", &self.city),
        ] {
            check_contact_field(field, value)?;
        }
        if let Some(organisation) = &self.organisation {
            check_contact_field("organisation", organisation)?;
        }
        check_contact_field("email", &self.email)?;
        if !valid_email(&self.email) {
            return Err(RegistrarError::Validation(
                "the registrant e-mail must be an address the registry can write to".to_owned(),
            ));
        }
        let country = normalize_country(&self.country)?;
        if country != self.country {
            return Err(RegistrarError::Validation(
                "the registrant country must be a lowercase two-letter ISO-3166 code".to_owned(),
            ));
        }
        check_contact_field("telephone", &self.phone)?;
        if !valid_phone(&self.phone) {
            return Err(RegistrarError::Validation(
                "the registrant telephone must be in international form, such as +31201234567"
                    .to_owned(),
            ));
        }
        Ok(())
    }
}

fn check_contact_field(field: &str, value: &str) -> RegistrarResult<()> {
    let length = value.trim().chars().count();
    if length == 0 || length > CONTACT_FIELD_MAX {
        return Err(RegistrarError::Validation(format!(
            "the registrant {field} must be 1-{CONTACT_FIELD_MAX} characters"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(RegistrarError::Validation(format!(
            "the registrant {field} may not contain control characters"
        )));
    }
    Ok(())
}

fn valid_email(value: &str) -> bool {
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !value.chars().any(char::is_whitespace)
        && !domain.contains('@')
        && normalize_site_domain(domain).is_ok()
}

fn valid_phone(value: &str) -> bool {
    let digits = value.strip_prefix('+').unwrap_or("");
    (7..=15).contains(&digits.len()) && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// A registration about to be paid for.
///
/// [`idempotency_key`](Self::idempotency_key) is what makes a retry safe: the
/// registrar must answer a repeat of the identical order with the identical
/// outcome, and a *different* order under the same key with
/// [`RegistrarError::Conflict`] — never with a second domain nobody meant to
/// buy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainOrder {
    /// The name to register.
    pub domain: String,
    /// How many years to pay for.
    pub years: u8,
    /// Who the registry will record as the owner.
    pub registrant: RegistrantContact,
    /// The nameservers the domain should answer from — alo's, when the buyer
    /// wants the "live in minutes" path.
    pub nameservers: Vec<String>,
    /// Whether the domain should renew itself.
    pub auto_renew: bool,
    /// The caller's replay token.
    pub idempotency_key: String,
}

impl DomainOrder {
    /// Checks everything that can be checked before money moves.
    ///
    /// # Errors
    /// [`RegistrarError::Validation`], naming the rule broken.
    pub fn validate(&self, catalog: &TldCatalog) -> RegistrarResult<RegistrableDomain> {
        let domain = catalog.parse(&self.domain)?;
        let offer = catalog
            .offer(domain.tld())
            .ok_or_else(|| RegistrarError::Unsupported {
                tld: domain.tld().to_owned(),
            })?;
        if self.years < offer.min_years || self.years > offer.max_years {
            return Err(RegistrarError::Validation(format!(
                ".{} is sold for {}-{} years at a time",
                offer.tld, offer.min_years, offer.max_years
            )));
        }
        self.registrant.validate()?;
        if !(NAMESERVERS_MIN..=NAMESERVERS_MAX).contains(&self.nameservers.len()) {
            return Err(RegistrarError::Validation(format!(
                "a domain needs {NAMESERVERS_MIN}-{NAMESERVERS_MAX} nameservers"
            )));
        }
        let mut seen: Vec<String> = Vec::new();
        for nameserver in &self.nameservers {
            let host = normalize_site_domain(nameserver)?;
            if seen.contains(&host) {
                return Err(RegistrarError::Validation(
                    "the same nameserver is listed twice".to_owned(),
                ));
            }
            seen.push(host);
        }
        validate_idempotency_key(&self.idempotency_key)?;
        Ok(domain)
    }

    /// What "the same order" means for a replay: everything a registry would
    /// act on. Two calls that agree on this are one purchase.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        let mut nameservers = self.nameservers.clone();
        nameservers.sort();
        format!(
            "{}|{}|{}|{}|{}|{}",
            self.domain.trim().to_ascii_lowercase(),
            self.years,
            self.registrant.email.trim().to_ascii_lowercase(),
            self.registrant.country,
            u8::from(self.auto_renew),
            nameservers.join(",")
        )
    }
}

/// Checks a replay token: long enough to be unguessable, plain enough to put in
/// a header.
///
/// # Errors
/// [`RegistrarError::Validation`] when it is too short, too long, or carries
/// anything but letters, digits, hyphens and underscores.
pub fn validate_idempotency_key(key: &str) -> RegistrarResult<()> {
    let ok = (IDEMPOTENCY_KEY_MIN..=IDEMPOTENCY_KEY_MAX).contains(&key.len())
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if ok {
        Ok(())
    } else {
        Err(RegistrarError::Validation(format!(
            "an idempotency key is {IDEMPOTENCY_KEY_MIN}-{IDEMPOTENCY_KEY_MAX} letters, digits, \
             hyphens or underscores"
        )))
    }
}

/// Where a registered name is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainLifecycle {
    /// Registered and working.
    Active,
    /// Past its expiry date: still ours to renew, no longer resolving.
    Expired,
    /// Past expiry and past grace — recoverable only by paying the registry's
    /// redemption fee, and only for a few weeks more.
    Redemption,
    /// Gone: released back to the registry or transferred away.
    Released,
}

impl DomainLifecycle {
    /// Stable storage and wire token.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Expired => "expired",
            Self::Redemption => "redemption",
            Self::Released => "released",
        }
    }

    /// Reads a stored token back.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "expired" => Some(Self::Expired),
            "redemption" => Some(Self::Redemption),
            "released" => Some(Self::Released),
            _ => None,
        }
    }
}

/// What the registrar knows about a name we bought.
///
/// Deliberately without the registrant: this is the shape that may be read back
/// into a list, and a list of domains is not a place to spread somebody's home
/// address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredDomain {
    /// The name, lowercase.
    pub domain: String,
    /// Where it is in its life.
    pub status: DomainLifecycle,
    /// When the paid term runs out.
    pub expires_at: OffsetDateTime,
    /// Whether it renews itself.
    pub auto_renew: bool,
    /// The nameservers it currently answers from.
    pub nameservers: Vec<String>,
    /// The provider's own identifier, for support conversations and
    /// reconciliation. Opaque to us.
    pub provider_reference: String,
}

// ---- the seam ---------------------------------------------------------------

/// Everything alo needs a domain reseller to do.
///
/// Implementations are held as trait objects in application state, so every
/// method returns a [`RegistrarFuture`] rather than being an `async fn`.
///
/// The contract the fixture suite pins, and which any live implementation must
/// also satisfy:
///
/// - [`catalog`](Self::catalog) is stable enough to price a search against;
/// - [`search`](Self::search) prices only what can actually be bought;
/// - [`register`](Self::register) is idempotent under
///   [`DomainOrder::idempotency_key`] and refuses a key reused for different
///   parameters;
/// - a name that is gone between search and purchase yields
///   [`RegistrarError::Unavailable`], never a silent substitution;
/// - errors never carry registrant data or the provider's raw response.
pub trait DomainRegistrar: Send + Sync {
    /// Who this registrar is and whether its calls spend money.
    fn identity(&self) -> RegistrarIdentity;

    /// The endings sold, in the order a buyer should see them.
    fn catalog(&self) -> RegistrarFuture<'_, TldCatalog>;

    /// Availability and price for every candidate of a search.
    fn search(&self, search: DomainSearch) -> RegistrarFuture<'_, Vec<DomainOffer>>;

    /// The price of one name for a given term.
    fn quote(&self, domain: String, years: u8) -> RegistrarFuture<'_, DomainQuote>;

    /// Registers a name. Idempotent under the order's key.
    fn register(&self, order: DomainOrder) -> RegistrarFuture<'_, RegisteredDomain>;

    /// Extends a name we already hold. Idempotent under `idempotency_key`.
    fn renew(
        &self,
        domain: String,
        years: u8,
        idempotency_key: String,
    ) -> RegistrarFuture<'_, RegisteredDomain>;

    /// What the provider knows about a name — `None` when it holds none.
    fn lookup(&self, domain: String) -> RegistrarFuture<'_, Option<RegisteredDomain>>;
}

/// The registrar an installation has until a human wires a real one.
///
/// Every method answers [`RegistrarError::Unconfigured`], which is a typed
/// answer the surfaces above can branch on — the same shape as the AI paths
/// (S1.28a): a feature that is not configured says so, rather than failing
/// somewhere deeper with a stranger message.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnconfiguredRegistrar;

impl DomainRegistrar for UnconfiguredRegistrar {
    fn identity(&self) -> RegistrarIdentity {
        RegistrarIdentity {
            name: "none".to_owned(),
            country: "eu".to_owned(),
            environment: RegistrarEnvironment::Fixture,
        }
    }

    fn catalog(&self) -> RegistrarFuture<'_, TldCatalog> {
        Box::pin(async { Err(RegistrarError::Unconfigured) })
    }

    fn search(&self, _search: DomainSearch) -> RegistrarFuture<'_, Vec<DomainOffer>> {
        Box::pin(async { Err(RegistrarError::Unconfigured) })
    }

    fn quote(&self, _domain: String, _years: u8) -> RegistrarFuture<'_, DomainQuote> {
        Box::pin(async { Err(RegistrarError::Unconfigured) })
    }

    fn register(&self, _order: DomainOrder) -> RegistrarFuture<'_, RegisteredDomain> {
        Box::pin(async { Err(RegistrarError::Unconfigured) })
    }

    fn renew(
        &self,
        _domain: String,
        _years: u8,
        _idempotency_key: String,
    ) -> RegistrarFuture<'_, RegisteredDomain> {
        Box::pin(async { Err(RegistrarError::Unconfigured) })
    }

    fn lookup(&self, _domain: String) -> RegistrarFuture<'_, Option<RegisteredDomain>> {
        Box::pin(async { Err(RegistrarError::Unconfigured) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(tld: &str, register: i64, renew: i64) -> TldOffer {
        TldOffer {
            tld: tld.to_owned(),
            register_cents: register,
            renew_cents: renew,
            transfer_cents: renew,
            min_years: 1,
            max_years: 10,
            requirement: TldRequirement::None,
        }
    }

    fn catalog() -> TldCatalog {
        let Ok(catalog) = TldCatalog::new(vec![
            offer("com", 1_299, 1_299),
            offer("eu", 899, 899),
            offer("co.uk", 1_099, 1_099),
            offer("uk", 999, 999),
        ]) else {
            panic!("the test catalog is invalid");
        };
        catalog
    }

    fn contact() -> RegistrantContact {
        RegistrantContact {
            name: "Ada Lovelace".to_owned(),
            organisation: Some("Acme BV".to_owned()),
            email: "ada@acme.example".to_owned(),
            street: "Keizersgracht 1".to_owned(),
            postal_code: "1015 CJ".to_owned(),
            city: "Amsterdam".to_owned(),
            country: "nl".to_owned(),
            phone: "+31201234567".to_owned(),
        }
    }

    #[test]
    fn a_catalog_refuses_bait_pricing() {
        let bait = TldOffer {
            register_cents: 199,
            renew_cents: 1_499,
            ..offer("shop", 199, 1_499)
        };
        let Err(RegistrarError::Validation(message)) = bait.validate() else {
            panic!("a first year cheaper than the renewal was accepted");
        };
        assert!(message.contains("bait"), "unexpected message: {message}");
        // The same shape is refused again at the point of sale, not only in the
        // price list a live provider might bypass.
        assert!(DomainQuote::new("acme.shop", 1, 199, 1_499, false).is_err());
    }

    #[test]
    fn a_catalog_refuses_duplicates_and_nonsense() {
        assert!(TldCatalog::new(Vec::new()).is_err());
        assert!(
            TldCatalog::new(vec![offer("com", 1_299, 1_299), offer("com", 1_499, 1_499)]).is_err()
        );
        assert!(TldCatalog::new(vec![offer(".com", 1_299, 1_299)]).is_err());
        assert!(TldCatalog::new(vec![offer("com", 0, 0)]).is_err());
        assert!(
            TldCatalog::new(vec![offer(
                "com",
                DOMAIN_PRICE_MAX_CENTS + 1,
                DOMAIN_PRICE_MAX_CENTS + 1
            )])
            .is_err()
        );
        let mut bad_term = offer("com", 1_299, 1_299);
        bad_term.max_years = 11;
        assert!(TldCatalog::new(vec![bad_term]).is_err());
    }

    #[test]
    fn retail_keeps_an_honest_list_honest() {
        let policy = RetailPolicy::THIN;
        // 15 % of 899 is 134.85 — rounded up, never down.
        let Ok(retail) = policy.retail(899) else {
            panic!("a normal wholesale price was refused");
        };
        assert_eq!(retail, 899 + 135);
        // The floor covers the endings where a percentage would not.
        let Ok(cheap) = policy.retail(100) else {
            panic!("a cheap wholesale price was refused");
        };
        assert_eq!(cheap, 200);
        // Applied to both numbers, an honest wholesale list stays honest.
        for (wholesale_register, wholesale_renew) in [(899, 899), (1_500, 1_200), (100, 100)] {
            let (Ok(register), Ok(renew)) = (
                policy.retail(wholesale_register),
                policy.retail(wholesale_renew),
            ) else {
                panic!("a valid wholesale pair was refused");
            };
            assert!(register >= renew);
            assert!(
                TldOffer {
                    register_cents: register,
                    renew_cents: renew,
                    transfer_cents: renew,
                    ..offer("com", register, renew)
                }
                .validate()
                .is_ok()
            );
        }
        assert!(policy.retail(0).is_err());
        assert!(policy.retail(-1).is_err());
        assert!(policy.retail(DOMAIN_PRICE_MAX_CENTS).is_err());
        assert!(
            RetailPolicy {
                markup_bp: RetailPolicy::MARKUP_BP_MAX + 1,
                min_markup_cents: 0,
            }
            .retail(899)
            .is_err()
        );
    }

    #[test]
    fn a_name_is_parsed_the_way_a_person_types_it() {
        let catalog = catalog();
        for typed in [
            " ACME.com ",
            "https://acme.com",
            "http://acme.com/pricing?x=1",
            "acme.com:443",
            "acme.com.",
        ] {
            let Ok(domain) = catalog.parse(typed) else {
                panic!("a person typing {typed:?} was refused");
            };
            assert_eq!(domain.name(), "acme.com");
            assert_eq!(domain.label(), "acme");
            assert_eq!(domain.tld(), "com");
        }
        // The longest ending wins: this is co.uk, not uk with the label acme.co.
        let Ok(british) = catalog.parse("acme.co.uk") else {
            panic!("a co.uk name was refused");
        };
        assert_eq!(british.tld(), "co.uk");
        assert_eq!(british.label(), "acme");
    }

    #[test]
    fn a_subdomain_is_not_a_purchase() {
        let catalog = catalog();
        let Err(RegistrarError::Validation(message)) = catalog.parse("shop.acme.com") else {
            panic!("a subdomain was sold");
        };
        assert!(message.contains("one name plus its ending"), "{message}");
        assert!(matches!(
            catalog.parse("acme.zzz"),
            Err(RegistrarError::Unsupported { ref tld }) if tld == "zzz"
        ));
        assert!(catalog.parse("acme").is_err());
        assert!(catalog.parse("-acme.com").is_err());
        assert!(catalog.parse(".com").is_err());
    }

    #[test]
    fn a_quote_states_the_renewal_and_multiplies_the_term() {
        let catalog = catalog();
        let Ok(domain) = catalog.parse("acme.com") else {
            panic!("a valid name was refused");
        };
        let Ok(quote) = catalog.quote(&domain, 3, None) else {
            panic!("a three-year term was refused");
        };
        assert_eq!(quote.first_term_cents, 1_299 * 3);
        assert_eq!(quote.renewal_cents_per_year, 1_299);
        assert_eq!(quote.currency, REGISTRAR_CURRENCY);
        assert!(!quote.premium);
        // A premium name renews at its premium price; nothing here can show a
        // cheap renewal beside an expensive first year.
        let Ok(premium) = catalog.quote(&domain, 1, Some(250_000)) else {
            panic!("a premium quote was refused");
        };
        assert_eq!(premium.first_term_cents, 250_000);
        assert_eq!(premium.renewal_cents_per_year, 250_000);
        assert!(premium.premium);
        assert!(catalog.quote(&domain, 0, None).is_err());
        assert!(catalog.quote(&domain, TERM_YEARS_MAX + 1, None).is_err());
    }

    #[test]
    fn only_an_available_domain_carries_a_price() {
        let Ok(quote) = DomainQuote::new("acme.com", 1, 1_299, 1_299, false) else {
            panic!("a valid quote was refused");
        };
        assert!(
            DomainOffer::new(
                "acme.com",
                DomainAvailability::Available,
                Some(quote.clone())
            )
            .is_ok()
        );
        assert!(DomainOffer::new("acme.com", DomainAvailability::Taken, Some(quote)).is_err());
        assert!(DomainOffer::new("acme.com", DomainAvailability::Available, None).is_err());
        assert!(DomainOffer::new("acme.zzz", DomainAvailability::Unsupported, None).is_ok());
    }

    #[test]
    fn a_search_offers_what_was_typed_first() {
        let catalog = catalog();
        let Ok(search) = DomainSearch::parse("Acme.eu", &[], &catalog) else {
            panic!("a typed ending was refused");
        };
        assert_eq!(search.label(), "acme");
        let candidates = search.candidates();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].domain, "acme.eu");

        let Ok(picked) = DomainSearch::parse(
            "acme.com",
            &[".eu".to_owned(), "com".to_owned(), "zzz".to_owned()],
            &catalog,
        ) else {
            panic!("a ticked ending was refused");
        };
        let names: Vec<String> = picked
            .candidates()
            .into_iter()
            .map(|candidate| candidate.domain)
            .collect();
        // Typed first, ticked next, no duplicate for the one named twice, and
        // the ending we do not sell last — kept, so the buyer is told.
        assert_eq!(names, ["acme.com", "acme.eu", "acme.zzz"]);
        assert!(
            picked
                .candidates()
                .iter()
                .any(|candidate| !candidate.supported)
        );

        let Ok(bare) = DomainSearch::parse("acme", &[], &catalog) else {
            panic!("a bare label was refused");
        };
        assert_eq!(bare.candidates().len(), catalog.offers().len());

        // A typed multi-label name is read through the catalog: this is one
        // name under an ending we sell, not the label `acme.co`.
        let Ok(british) = DomainSearch::parse("acme.co.uk", &[], &catalog) else {
            panic!("a co.uk name typed into the search box was refused");
        };
        assert_eq!(british.label(), "acme");
        assert_eq!(british.candidates()[0].domain, "acme.co.uk");
        // An ending we do not sell is a candidate the buyer is told about…
        let Ok(elsewhere) = DomainSearch::parse("shop.acme", &[], &catalog) else {
            panic!("an ending we do not sell was refused rather than reported");
        };
        assert_eq!(elsewhere.candidates()[0].domain, "shop.acme");
        assert!(!elsewhere.candidates()[0].supported);
        // …but an address inside a domain we *do* sell keeps the sharp refusal,
        // because the thing to buy is acme.com and we should say so.
        let Err(RegistrarError::Validation(message)) =
            DomainSearch::parse("shop.acme.com", &[], &catalog)
        else {
            panic!("a subdomain was searched for as if it could be bought");
        };
        assert!(message.contains("one name plus its ending"), "{message}");
        assert!(DomainSearch::parse("", &[], &catalog).is_err());
        assert!(
            DomainSearch::parse(
                "acme",
                &vec!["com".to_owned(); SEARCH_TLDS_MAX + 1],
                &catalog
            )
            .is_err()
        );
    }

    #[test]
    fn a_registrant_is_checked_before_money_moves() {
        let catalog = catalog();
        let order = DomainOrder {
            domain: "acme.com".to_owned(),
            years: 1,
            registrant: contact(),
            nameservers: vec!["ns1.alo.example".to_owned(), "ns2.alo.example".to_owned()],
            auto_renew: true,
            idempotency_key: "order-0001".to_owned(),
        };
        assert!(order.validate(&catalog).is_ok());

        let mut no_email = order.clone();
        no_email.registrant.email = "ada".to_owned();
        assert!(no_email.validate(&catalog).is_err());

        let mut bad_phone = order.clone();
        bad_phone.registrant.phone = "020 123 4567".to_owned();
        assert!(bad_phone.validate(&catalog).is_err());

        let mut bad_country = order.clone();
        bad_country.registrant.country = "NL".to_owned();
        assert!(bad_country.validate(&catalog).is_err());

        let mut one_nameserver = order.clone();
        one_nameserver.nameservers = vec!["ns1.alo.example".to_owned()];
        assert!(one_nameserver.validate(&catalog).is_err());

        let mut same_twice = order.clone();
        same_twice.nameservers = vec!["ns1.alo.example".to_owned(), "NS1.alo.example".to_owned()];
        assert!(same_twice.validate(&catalog).is_err());

        let mut short_key = order.clone();
        short_key.idempotency_key = "abc".to_owned();
        assert!(short_key.validate(&catalog).is_err());

        // A validation message names the field, never the value: these are
        // shown to people and travel through logs upstream.
        let mut long_name = order.clone();
        long_name.registrant.name = "Ada".repeat(CONTACT_FIELD_MAX);
        let Err(RegistrarError::Validation(message)) = long_name.validate(&catalog) else {
            panic!("an oversized registrant name was accepted");
        };
        assert!(!message.contains("Ada"), "the message quoted the value");
    }

    #[test]
    fn a_replay_is_the_same_order_only_when_nothing_moved() {
        let order = DomainOrder {
            domain: "ACME.com ".to_owned(),
            years: 1,
            registrant: contact(),
            nameservers: vec!["ns2.alo.example".to_owned(), "ns1.alo.example".to_owned()],
            auto_renew: true,
            idempotency_key: "order-0001".to_owned(),
        };
        let mut same = order.clone();
        same.domain = "acme.com".to_owned();
        same.nameservers.reverse();
        assert_eq!(order.fingerprint(), same.fingerprint());

        let mut longer = order.clone();
        longer.years = 2;
        assert_ne!(order.fingerprint(), longer.fingerprint());
    }

    #[test]
    fn the_registrar_itself_must_be_european() {
        assert!(
            RegistrarIdentity::new("Openprovider", "NL", RegistrarEnvironment::Sandbox).is_ok()
        );
        assert!(
            RegistrarIdentity::new("A Norwegian reseller", "no", RegistrarEnvironment::Live)
                .is_ok()
        );
        assert!(RegistrarIdentity::new("A US reseller", "us", RegistrarEnvironment::Live).is_err());
        assert!(RegistrarIdentity::new("", "nl", RegistrarEnvironment::Live).is_err());
        assert!(RegistrarIdentity::new("Bad code", "nld", RegistrarEnvironment::Live).is_err());
        // The list is searched, so it must stay sorted.
        let mut sorted = EEA_COUNTRIES;
        sorted.sort_unstable();
        assert_eq!(sorted, EEA_COUNTRIES);
        assert!(!RegistrarEnvironment::Fixture.spends_money());
        assert!(RegistrarEnvironment::Live.spends_money());
    }

    #[test]
    fn lifecycle_tokens_are_stable() {
        for status in [
            DomainLifecycle::Active,
            DomainLifecycle::Expired,
            DomainLifecycle::Redemption,
            DomainLifecycle::Released,
        ] {
            assert_eq!(DomainLifecycle::parse(status.as_str()), Some(status));
        }
        assert_eq!(DomainLifecycle::parse("bought"), None);
    }

    #[tokio::test]
    async fn an_unconfigured_installation_says_so_from_every_door() {
        let registrar = UnconfiguredRegistrar;
        let catalog = catalog();
        let Ok(search) = DomainSearch::parse("acme", &[], &catalog) else {
            panic!("a valid search was refused");
        };
        let order = DomainOrder {
            domain: "acme.com".to_owned(),
            years: 1,
            registrant: contact(),
            nameservers: vec!["ns1.alo.example".to_owned(), "ns2.alo.example".to_owned()],
            auto_renew: true,
            idempotency_key: "order-0001".to_owned(),
        };
        assert!(matches!(
            registrar.catalog().await,
            Err(RegistrarError::Unconfigured)
        ));
        assert!(matches!(
            registrar.search(search).await,
            Err(RegistrarError::Unconfigured)
        ));
        assert!(matches!(
            registrar.quote("acme.com".to_owned(), 1).await,
            Err(RegistrarError::Unconfigured)
        ));
        assert!(matches!(
            registrar.register(order).await,
            Err(RegistrarError::Unconfigured)
        ));
        assert!(matches!(
            registrar
                .renew("acme.com".to_owned(), 1, "order-0001".to_owned())
                .await,
            Err(RegistrarError::Unconfigured)
        ));
        assert!(matches!(
            registrar.lookup("acme.com".to_owned()).await,
            Err(RegistrarError::Unconfigured)
        ));
    }
}
