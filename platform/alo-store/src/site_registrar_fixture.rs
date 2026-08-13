//! The registrar that ships: an in-memory, deterministic reseller with a
//! European price list and nothing behind it.
//!
//! [`FixtureRegistrar`] is a complete implementation of
//! [`DomainRegistrar`](crate::site_registrar::DomainRegistrar) — it sells a
//! catalog, answers availability, quotes, registers, renews and remembers —
//! and it **cannot spend money or open a socket**. That is what makes it usable
//! in three places at once: the test suite, local development, and the day a
//! live provider is wired, when the fixture's behaviour is the contract the new
//! implementation is held to.
//!
//! # Deterministic on purpose
//!
//! Nothing here reads the clock, hashes a name or randomises anything. Time is
//! given at construction and moves only when a test calls
//! [`FixtureRegistrar::advance_days`]; taken, blocked and premium names come
//! from a seeded list a test can add to. A fixture that decided availability by
//! hashing would produce a suite whose failures depend on the name somebody
//! happened to type.
//!
//! # The prices are real arithmetic, not decoration
//!
//! The shipped catalog is built by applying [`RetailPolicy::THIN`] to a
//! wholesale list, exactly as the live path will: the honest-pricing rule is
//! therefore exercised by the fixture's own construction, and a change to the
//! retail policy that produced bait pricing would fail to build a catalog at
//! all.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, MutexGuard};

use time::{Duration, OffsetDateTime};

use crate::site_registrar::{
    DomainAvailability, DomainLifecycle, DomainOffer, DomainOrder, DomainQuote, DomainRegistrar,
    DomainSearch, RegisteredDomain, RegistrableDomain, RegistrarEnvironment, RegistrarError,
    RegistrarFuture, RegistrarIdentity, RegistrarResult, RetailPolicy, TldCatalog, TldOffer,
    TldRequirement, validate_idempotency_key,
};

/// Days a name stays recoverable by its owner after expiry.
pub const FIXTURE_GRACE_DAYS: i64 = 30;

/// Days after the grace period during which the registry will still sell it
/// back, at a redemption fee.
pub const FIXTURE_REDEMPTION_DAYS: i64 = 30;

/// Days in the fixture's year. Registries count calendar years; the fixture
/// counts 365 days, which keeps every expiry in a test exactly predictable.
const FIXTURE_YEAR_DAYS: i64 = 365;

/// The wholesale price list the shipped catalog is built from: ending,
/// wholesale registration, wholesale renewal, wholesale transfer, requirement.
/// Endings are in editorial order — a European buyer is offered `.eu` and their
/// own country before `.com`.
const WHOLESALE: [(&str, i64, i64, i64, &str); 9] = [
    ("eu", 650, 650, 650, "eea"),
    ("nl", 700, 700, 700, ""),
    ("be", 750, 750, 750, ""),
    ("de", 690, 690, 690, ""),
    ("fr", 780, 780, 780, "eea"),
    ("com", 1_050, 1_050, 1_050, ""),
    ("org", 1_150, 1_150, 1_150, ""),
    ("net", 1_250, 1_250, 1_250, ""),
    ("shop", 2_400, 2_400, 2_400, ""),
];

/// Names the fixture starts out believing are registered by somebody else.
const SEEDED_TAKEN: [&str; 4] = ["acme.com", "acme.eu", "acme.nl", "shop.shop"];

/// Names no registry will sell to anybody. `example.*` is reserved by IANA;
/// the fixture blocks it so a test can exercise the difference between "taken"
/// and "nobody can have this".
const SEEDED_BLOCKED: [&str; 3] = ["example.com", "example.eu", "example.nl"];

/// Names the registry holds back and prices itself, in cents per year.
const SEEDED_PREMIUM: [(&str, i64); 2] = [("coffee.com", 250_000), ("bank.eu", 480_000)];

/// A deterministic registrar with no network and no money.
#[derive(Debug)]
pub struct FixtureRegistrar {
    identity: RegistrarIdentity,
    catalog: TldCatalog,
    state: Mutex<FixtureState>,
}

#[derive(Debug)]
struct FixtureState {
    now: OffsetDateTime,
    taken: BTreeSet<String>,
    blocked: BTreeSet<String>,
    premium: BTreeMap<String, i64>,
    registered: BTreeMap<String, StoredDomain>,
    /// Idempotency key → the fingerprint it was used with and the name it
    /// produced. This is the whole of the replay contract.
    replays: BTreeMap<String, (String, String)>,
    counter: u32,
}

/// What the fixture keeps about a name it registered. The lifecycle is not
/// stored: it is derived from the expiry and the current time, so advancing the
/// clock ages every domain at once, the way a registry does.
#[derive(Debug, Clone)]
struct StoredDomain {
    domain: String,
    expires_at: OffsetDateTime,
    auto_renew: bool,
    nameservers: Vec<String>,
    provider_reference: String,
}

/// Builds the shipped catalog from the wholesale list.
///
/// # Errors
/// [`RegistrarError::Validation`] if the retail policy ever produced a price
/// list that breaks the honest-pricing rule — which is the point of building it
/// this way rather than writing retail numbers by hand.
pub fn fixture_catalog(policy: RetailPolicy) -> RegistrarResult<TldCatalog> {
    let mut offers = Vec::with_capacity(WHOLESALE.len());
    for (tld, register, renew, transfer, requirement) in WHOLESALE {
        offers.push(TldOffer {
            tld: tld.to_owned(),
            register_cents: policy.retail(register)?,
            renew_cents: policy.retail(renew)?,
            transfer_cents: policy.retail(transfer)?,
            min_years: 1,
            max_years: 10,
            requirement: match requirement {
                "eea" => TldRequirement::EeaPresence,
                "" => TldRequirement::None,
                country => TldRequirement::CountryPresence {
                    country: country.to_owned(),
                },
            },
        });
    }
    TldCatalog::new(offers)
}

impl FixtureRegistrar {
    /// The shipped fixture: the European price list above, seeded taken,
    /// blocked and premium names, and a clock that starts at `now`.
    ///
    /// # Errors
    /// [`RegistrarError::Validation`] only if the built-in price list stopped
    /// satisfying the catalog rules.
    pub fn new(now: OffsetDateTime) -> RegistrarResult<Self> {
        let catalog = fixture_catalog(RetailPolicy::THIN)?;
        Self::with_catalog(catalog, now)
    }

    /// A fixture over a caller-supplied catalog, for tests that need a
    /// particular ending or price.
    ///
    /// # Errors
    /// [`RegistrarError::Validation`] if the fixture's own identity is somehow
    /// invalid — it is not, and the signature says so only because
    /// [`RegistrarIdentity::new`] is the single door.
    pub fn with_catalog(catalog: TldCatalog, now: OffsetDateTime) -> RegistrarResult<Self> {
        let identity =
            RegistrarIdentity::new("alo fixture registrar", "nl", RegistrarEnvironment::Fixture)?;
        Ok(Self {
            identity,
            catalog,
            state: Mutex::new(FixtureState {
                now,
                taken: SEEDED_TAKEN.iter().map(|name| (*name).to_owned()).collect(),
                blocked: SEEDED_BLOCKED
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
                premium: SEEDED_PREMIUM
                    .iter()
                    .map(|(name, cents)| ((*name).to_owned(), *cents))
                    .collect(),
                registered: BTreeMap::new(),
                replays: BTreeMap::new(),
                counter: 0,
            }),
        })
    }

    /// The endings this fixture sells.
    #[must_use]
    pub fn catalog_ref(&self) -> &TldCatalog {
        &self.catalog
    }

    /// Moves the fixture's clock, ageing every registration at once.
    pub fn advance_days(&self, days: i64) {
        let mut state = self.state();
        state.now += Duration::days(days);
    }

    /// The fixture's current time.
    #[must_use]
    pub fn now(&self) -> OffsetDateTime {
        self.state().now
    }

    /// Declares a name registered by somebody else — the way a test reproduces
    /// the race between a search and a purchase.
    ///
    /// # Errors
    /// [`RegistrarError::Validation`] or [`RegistrarError::Unsupported`] when
    /// the name is not one this catalog could sell.
    pub fn seed_taken(&self, domain: &str) -> RegistrarResult<()> {
        let parsed = self.catalog.parse(domain)?;
        self.state().taken.insert(parsed.name().to_owned());
        Ok(())
    }

    /// Declares a name premium at a price per year.
    ///
    /// # Errors
    /// [`RegistrarError::Validation`] for an unusable price, or the parse
    /// errors of an unsellable name.
    pub fn seed_premium(&self, domain: &str, cents_per_year: i64) -> RegistrarResult<()> {
        let parsed = self.catalog.parse(domain)?;
        // Priced through the same door a quote uses, so an impossible premium
        // is refused here rather than at the till.
        DomainQuote::new(parsed.name(), 1, cents_per_year, cents_per_year, true)?;
        self.state()
            .premium
            .insert(parsed.name().to_owned(), cents_per_year);
        Ok(())
    }

    /// The state lock. A poisoned fixture is still a fixture: the data behind
    /// the lock is plain values, so recovering is honest and panicking a second
    /// time would only hide the first test's failure.
    fn state(&self) -> MutexGuard<'_, FixtureState> {
        match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn availability(state: &FixtureState, domain: &RegistrableDomain) -> DomainAvailability {
        if state.blocked.contains(domain.name()) {
            DomainAvailability::Blocked
        } else if state.taken.contains(domain.name()) {
            DomainAvailability::Taken
        } else {
            DomainAvailability::Available
        }
    }

    fn quote_for(
        &self,
        state: &FixtureState,
        domain: &RegistrableDomain,
        years: u8,
    ) -> RegistrarResult<DomainQuote> {
        self.catalog
            .quote(domain, years, state.premium.get(domain.name()).copied())
    }

    fn search_now(&self, search: &DomainSearch) -> RegistrarResult<Vec<DomainOffer>> {
        let state = self.state();
        let mut offers = Vec::new();
        for candidate in search.candidates() {
            if !candidate.supported {
                offers.push(DomainOffer::new(
                    &candidate.domain,
                    DomainAvailability::Unsupported,
                    None,
                )?);
                continue;
            }
            let domain = self.catalog.parse(&candidate.domain)?;
            let availability = Self::availability(&state, &domain);
            let quote = match availability {
                DomainAvailability::Available => {
                    let years = self
                        .catalog
                        .offer(domain.tld())
                        .map_or(1, |offer| offer.min_years);
                    Some(self.quote_for(&state, &domain, years)?)
                }
                _ => None,
            };
            offers.push(DomainOffer::new(domain.name(), availability, quote)?);
        }
        Ok(offers)
    }

    fn quote_now(&self, domain: &str, years: u8) -> RegistrarResult<DomainQuote> {
        let parsed = self.catalog.parse(domain)?;
        let state = self.state();
        match Self::availability(&state, &parsed) {
            DomainAvailability::Available => self.quote_for(&state, &parsed, years),
            _ => Err(RegistrarError::Unavailable),
        }
    }

    fn register_now(&self, order: &DomainOrder) -> RegistrarResult<RegisteredDomain> {
        let domain = order.validate(&self.catalog)?;
        let fingerprint = order.fingerprint();
        let mut state = self.state();

        if let Some((seen, name)) = state.replays.get(&order.idempotency_key) {
            if *seen != fingerprint {
                return Err(RegistrarError::Conflict(
                    "that idempotency key was already used for a different order".to_owned(),
                ));
            }
            let name = name.clone();
            let now = state.now;
            return state
                .registered
                .get(&name)
                .map(|stored| stored.view(now))
                .ok_or(RegistrarError::Unavailable);
        }

        if Self::availability(&state, &domain) != DomainAvailability::Available {
            return Err(RegistrarError::Unavailable);
        }
        // Quoting before writing means a price that cannot be expressed stops
        // the purchase instead of producing a domain nobody can be billed for.
        self.quote_for(&state, &domain, order.years)?;

        let expires_at = state
            .now
            .checked_add(Duration::days(FIXTURE_YEAR_DAYS * i64::from(order.years)))
            .ok_or_else(|| {
                RegistrarError::Validation("that term ends past any date we can store".to_owned())
            })?;
        state.counter += 1;
        let stored = StoredDomain {
            domain: domain.name().to_owned(),
            expires_at,
            auto_renew: order.auto_renew,
            nameservers: order.nameservers.clone(),
            provider_reference: format!("fixture-{:06}", state.counter),
        };
        let view = stored.view(state.now);
        state.taken.insert(stored.domain.clone());
        state.registered.insert(stored.domain.clone(), stored);
        state.replays.insert(
            order.idempotency_key.clone(),
            (fingerprint, domain.name().to_owned()),
        );
        Ok(view)
    }

    fn renew_now(
        &self,
        domain: &str,
        years: u8,
        idempotency_key: &str,
    ) -> RegistrarResult<RegisteredDomain> {
        let parsed = self.catalog.parse(domain)?;
        validate_idempotency_key(idempotency_key)?;
        let fingerprint = format!("renew|{}|{years}", parsed.name());
        let mut state = self.state();

        if let Some((seen, name)) = state.replays.get(idempotency_key) {
            if *seen != fingerprint {
                return Err(RegistrarError::Conflict(
                    "that idempotency key was already used for a different order".to_owned(),
                ));
            }
            let name = name.clone();
            let now = state.now;
            return state
                .registered
                .get(&name)
                .map(|stored| stored.view(now))
                .ok_or(RegistrarError::Unavailable);
        }

        let now = state.now;
        let Some(stored) = state.registered.get(parsed.name()) else {
            return Err(RegistrarError::Unavailable);
        };
        if stored.lifecycle(now) == DomainLifecycle::Released {
            // Past redemption the registry has already sold it on; renewing is
            // not a thing that can happen, and pretending otherwise would take
            // money for nothing.
            return Err(RegistrarError::Unavailable);
        }
        self.quote_for(&state, &parsed, years)?;

        let Some(stored) = state.registered.get_mut(parsed.name()) else {
            return Err(RegistrarError::Unavailable);
        };
        // A renewal extends the term from the expiry, not from today — paying
        // early may not cost the buyer the days they already own.
        let base = stored.expires_at.max(now);
        stored.expires_at = base
            .checked_add(Duration::days(FIXTURE_YEAR_DAYS * i64::from(years)))
            .ok_or_else(|| {
                RegistrarError::Validation("that term ends past any date we can store".to_owned())
            })?;
        let view = stored.view(now);
        state.replays.insert(
            idempotency_key.to_owned(),
            (fingerprint, parsed.name().to_owned()),
        );
        Ok(view)
    }

    fn lookup_now(&self, domain: &str) -> RegistrarResult<Option<RegisteredDomain>> {
        let parsed = self.catalog.parse(domain)?;
        let state = self.state();
        let now = state.now;
        Ok(state
            .registered
            .get(parsed.name())
            .map(|stored| stored.view(now)))
    }
}

impl StoredDomain {
    fn lifecycle(&self, now: OffsetDateTime) -> DomainLifecycle {
        if now <= self.expires_at {
            DomainLifecycle::Active
        } else if now <= self.expires_at + Duration::days(FIXTURE_GRACE_DAYS) {
            DomainLifecycle::Expired
        } else if now
            <= self.expires_at + Duration::days(FIXTURE_GRACE_DAYS + FIXTURE_REDEMPTION_DAYS)
        {
            DomainLifecycle::Redemption
        } else {
            DomainLifecycle::Released
        }
    }

    fn view(&self, now: OffsetDateTime) -> RegisteredDomain {
        RegisteredDomain {
            domain: self.domain.clone(),
            status: self.lifecycle(now),
            expires_at: self.expires_at,
            auto_renew: self.auto_renew,
            nameservers: self.nameservers.clone(),
            provider_reference: self.provider_reference.clone(),
        }
    }
}

impl DomainRegistrar for FixtureRegistrar {
    fn identity(&self) -> RegistrarIdentity {
        self.identity.clone()
    }

    fn catalog(&self) -> RegistrarFuture<'_, TldCatalog> {
        let catalog = self.catalog.clone();
        Box::pin(async move { Ok(catalog) })
    }

    fn search(&self, search: DomainSearch) -> RegistrarFuture<'_, Vec<DomainOffer>> {
        // Every method computes before it awaits: the state lock is never held
        // across an await point, and the returned future is already resolved.
        let result = self.search_now(&search);
        Box::pin(async move { result })
    }

    fn quote(&self, domain: String, years: u8) -> RegistrarFuture<'_, DomainQuote> {
        let result = self.quote_now(&domain, years);
        Box::pin(async move { result })
    }

    fn register(&self, order: DomainOrder) -> RegistrarFuture<'_, RegisteredDomain> {
        let result = self.register_now(&order);
        Box::pin(async move { result })
    }

    fn renew(
        &self,
        domain: String,
        years: u8,
        idempotency_key: String,
    ) -> RegistrarFuture<'_, RegisteredDomain> {
        let result = self.renew_now(&domain, years, &idempotency_key);
        Box::pin(async move { result })
    }

    fn lookup(&self, domain: String) -> RegistrarFuture<'_, Option<RegisteredDomain>> {
        let result = self.lookup_now(&domain);
        Box::pin(async move { result })
    }
}
