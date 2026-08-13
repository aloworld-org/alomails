//! The contract every domain registrar must satisfy, exercised against the
//! implementation that ships (S2.15a).
//!
//! These tests are the reason the fixture exists: they pin what a live reseller
//! implementation will also have to do — price only what can be bought, refuse
//! a name that vanished between the search and the till, register exactly once
//! per idempotency key, and never invent a cheap first year. **Nothing here
//! opens a socket or spends money**, and the assertions on
//! [`RegistrarEnvironment`] say so in the suite itself.

use alo_store::site_registrar::{
    DomainAvailability, DomainLifecycle, DomainOrder, DomainRegistrar, DomainSearch,
    REGISTRAR_CURRENCY, RegistrantContact, RegistrarEnvironment, RegistrarError,
};
use alo_store::site_registrar_fixture::FixtureRegistrar;
use time::OffsetDateTime;

/// A fixed instant, so every expiry in this file is an exact date.
fn start() -> OffsetDateTime {
    let Ok(now) = OffsetDateTime::from_unix_timestamp(1_800_000_000) else {
        panic!("the fixed test instant is not a time");
    };
    now
}

fn registrar() -> FixtureRegistrar {
    let Ok(registrar) = FixtureRegistrar::new(start()) else {
        panic!("the shipped fixture catalog is invalid");
    };
    registrar
}

fn search(registrar: &FixtureRegistrar, query: &str, tlds: &[&str]) -> DomainSearch {
    let tlds: Vec<String> = tlds.iter().map(|tld| (*tld).to_owned()).collect();
    let Ok(search) = DomainSearch::parse(query, &tlds, registrar.catalog_ref()) else {
        panic!("a valid search was refused: {query}");
    };
    search
}

fn contact() -> RegistrantContact {
    RegistrantContact {
        name: "Ada Lovelace".to_owned(),
        organisation: Some("Brightwave BV".to_owned()),
        email: "ada@brightwave.example".to_owned(),
        street: "Keizersgracht 1".to_owned(),
        postal_code: "1015 CJ".to_owned(),
        city: "Amsterdam".to_owned(),
        country: "nl".to_owned(),
        phone: "+31201234567".to_owned(),
    }
}

fn order(domain: &str, years: u8, key: &str) -> DomainOrder {
    DomainOrder {
        domain: domain.to_owned(),
        years,
        registrant: contact(),
        nameservers: vec!["ns1.alo.example".to_owned(), "ns2.alo.example".to_owned()],
        auto_renew: true,
        idempotency_key: key.to_owned(),
    }
}

#[tokio::test]
async fn the_shipped_catalog_is_european_honest_and_priced_in_euro() {
    let registrar = registrar();
    let identity = registrar.identity();
    assert_eq!(identity.environment, RegistrarEnvironment::Fixture);
    assert!(!identity.environment.spends_money());

    let Ok(catalog) = registrar.catalog().await else {
        panic!("the fixture could not produce its own catalog");
    };
    assert!(!catalog.offers().is_empty());
    for offer in catalog.offers() {
        // The whole product promise, asserted on every ending we sell: the
        // first year is never cheaper than the year after it.
        assert!(
            offer.register_cents >= offer.renew_cents,
            ".{} is priced as bait",
            offer.tld
        );
        assert!(offer.validate().is_ok());
    }
    // European endings come first: a Dutch buyer should not have to scroll past
    // .com to find .nl.
    assert_eq!(catalog.offers()[0].tld, "eu");
}

#[tokio::test]
async fn a_search_prices_only_what_can_actually_be_bought() {
    let registrar = registrar();
    let Ok(offers) = registrar
        .search(search(&registrar, "brightwave", &["eu", "nl", "com"]))
        .await
    else {
        panic!("a plain search failed");
    };
    assert_eq!(offers.len(), 3);
    for offer in &offers {
        assert_eq!(offer.availability, DomainAvailability::Available);
        let Some(quote) = &offer.quote else {
            panic!("{} was available with no price", offer.domain);
        };
        assert_eq!(quote.currency, REGISTRAR_CURRENCY);
        assert_eq!(quote.term_years, 1);
        assert!(quote.first_term_cents >= quote.renewal_cents_per_year);
        assert!(!quote.premium);
    }

    // Seeded as registered by somebody else, and blocked by the registry: both
    // are unbuyable, both are told apart, and neither carries a price.
    let Ok(taken) = registrar.search(search(&registrar, "acme.com", &[])).await else {
        panic!("a search for a taken name failed");
    };
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].availability, DomainAvailability::Taken);
    assert!(taken[0].quote.is_none());

    let Ok(blocked) = registrar
        .search(search(&registrar, "example.eu", &[]))
        .await
    else {
        panic!("a search for a blocked name failed");
    };
    assert_eq!(blocked[0].availability, DomainAvailability::Blocked);
    assert!(blocked[0].quote.is_none());
}

#[tokio::test]
async fn an_ending_we_do_not_sell_is_said_out_loud_not_dropped() {
    let registrar = registrar();
    let Ok(offers) = registrar
        .search(search(&registrar, "brightwave", &["zzz", "eu"]))
        .await
    else {
        panic!("a search naming an unsold ending failed");
    };
    let names: Vec<&str> = offers.iter().map(|offer| offer.domain.as_str()).collect();
    assert_eq!(names, ["brightwave.eu", "brightwave.zzz"]);
    assert_eq!(offers[1].availability, DomainAvailability::Unsupported);
    assert!(offers[1].quote.is_none());

    // Asking for a price on it, rather than searching, is an error that names
    // the ending — the buy box must not offer a button here.
    assert!(matches!(
        registrar.quote("brightwave.zzz".to_owned(), 1).await,
        Err(RegistrarError::Unsupported { ref tld }) if tld == "zzz"
    ));
    assert!(matches!(
        registrar
            .register(order("brightwave.zzz", 1, "order-00000001"))
            .await,
        Err(RegistrarError::Unsupported { .. })
    ));
}

#[tokio::test]
async fn a_premium_name_is_quoted_at_the_price_it_also_renews_for() {
    let registrar = registrar();
    let Ok(quote) = registrar.quote("coffee.com".to_owned(), 2).await else {
        panic!("a seeded premium name could not be quoted");
    };
    assert!(quote.premium);
    assert_eq!(quote.first_term_cents, 250_000 * 2);
    assert_eq!(quote.renewal_cents_per_year, 250_000);

    assert!(registrar.seed_premium("brightwave.eu", 90_000).is_ok());
    let Ok(seeded) = registrar.quote("brightwave.eu".to_owned(), 1).await else {
        panic!("a seeded premium price could not be quoted");
    };
    assert_eq!(seeded.first_term_cents, 90_000);
    assert_eq!(seeded.renewal_cents_per_year, 90_000);
}

#[tokio::test]
async fn buying_a_name_takes_it_away_from_everybody_else() {
    let registrar = registrar();
    let Ok(bought) = registrar
        .register(order("Brightwave.EU", 2, "order-00000001"))
        .await
    else {
        panic!("a valid registration was refused");
    };
    assert_eq!(bought.domain, "brightwave.eu");
    assert_eq!(bought.status, DomainLifecycle::Active);
    assert_eq!(bought.expires_at, start() + time::Duration::days(730));
    assert!(bought.auto_renew);
    assert_eq!(bought.nameservers.len(), 2);
    assert!(!bought.provider_reference.is_empty());

    let Ok(offers) = registrar
        .search(search(&registrar, "brightwave.eu", &[]))
        .await
    else {
        panic!("a search after a purchase failed");
    };
    assert_eq!(offers[0].availability, DomainAvailability::Taken);
    assert!(offers[0].quote.is_none());

    // A different buyer, a different key, the same name: no second sale.
    assert!(matches!(
        registrar
            .register(order("brightwave.eu", 1, "order-00000002"))
            .await,
        Err(RegistrarError::Unavailable)
    ));

    let Ok(Some(found)) = registrar.lookup("brightwave.eu".to_owned()).await else {
        panic!("a registered name could not be looked up");
    };
    assert_eq!(found.provider_reference, bought.provider_reference);
    let Ok(missing) = registrar.lookup("nobody-owns-this.eu".to_owned()).await else {
        panic!("looking up an unregistered name failed");
    };
    assert!(missing.is_none());
}

#[tokio::test]
async fn a_retried_purchase_buys_one_domain_and_a_reused_key_is_refused() {
    let registrar = registrar();
    let first = registrar
        .register(order("brightwave.nl", 1, "order-abc-123"))
        .await;
    let second = registrar
        .register(order("brightwave.nl", 1, "order-abc-123"))
        .await;
    let (Ok(first), Ok(second)) = (first, second) else {
        panic!("a retried registration was not idempotent");
    };
    assert_eq!(first.provider_reference, second.provider_reference);
    assert_eq!(first.expires_at, second.expires_at);

    // The same key with different parameters is a caller bug, and the only
    // safe answer is to refuse — never to buy a second name nobody asked for.
    assert!(matches!(
        registrar
            .register(order("brightwave.be", 1, "order-abc-123"))
            .await,
        Err(RegistrarError::Conflict(_))
    ));
    let Ok(untouched) = registrar.lookup("brightwave.be".to_owned()).await else {
        panic!("looking up the refused name failed");
    };
    assert!(untouched.is_none());
}

#[tokio::test]
async fn a_name_taken_between_the_search_and_the_till_is_refused() {
    let registrar = registrar();
    let Ok(offers) = registrar
        .search(search(&registrar, "brightwave.de", &[]))
        .await
    else {
        panic!("the search before the race failed");
    };
    assert_eq!(offers[0].availability, DomainAvailability::Available);

    assert!(registrar.seed_taken("brightwave.de").is_ok());
    assert!(matches!(
        registrar
            .register(order("brightwave.de", 1, "order-00000003"))
            .await,
        Err(RegistrarError::Unavailable)
    ));
}

#[tokio::test]
async fn a_renewal_extends_from_the_expiry_and_a_replay_never_doubles_it() {
    let registrar = registrar();
    let Ok(bought) = registrar
        .register(order("brightwave.com", 1, "order-00000004"))
        .await
    else {
        panic!("a valid registration was refused");
    };
    assert_eq!(bought.expires_at, start() + time::Duration::days(365));

    registrar.advance_days(10);
    let Ok(renewed) = registrar
        .renew("brightwave.com".to_owned(), 1, "renew-00000001".to_owned())
        .await
    else {
        panic!("a valid renewal was refused");
    };
    // Paying ten days early does not cost those ten days.
    assert_eq!(renewed.expires_at, start() + time::Duration::days(730));

    let Ok(replayed) = registrar
        .renew("brightwave.com".to_owned(), 1, "renew-00000001".to_owned())
        .await
    else {
        panic!("a replayed renewal was refused");
    };
    assert_eq!(replayed.expires_at, renewed.expires_at);

    assert!(matches!(
        registrar
            .renew("brightwave.com".to_owned(), 2, "renew-00000001".to_owned())
            .await,
        Err(RegistrarError::Conflict(_))
    ));
    // A name nobody registered here cannot be renewed.
    assert!(matches!(
        registrar
            .renew(
                "nobody-owns-this.com".to_owned(),
                1,
                "renew-00000002".to_owned()
            )
            .await,
        Err(RegistrarError::Unavailable)
    ));
    // A key must still look like a key.
    assert!(matches!(
        registrar
            .renew("brightwave.com".to_owned(), 1, "short".to_owned())
            .await,
        Err(RegistrarError::Validation(_))
    ));
}

#[tokio::test]
async fn a_domain_ages_out_of_reach_one_stage_at_a_time() {
    let registrar = registrar();
    assert!(
        registrar
            .register(order("brightwave.org", 1, "order-00000005"))
            .await
            .is_ok()
    );

    for (days, expected) in [
        (365, DomainLifecycle::Active),
        (1, DomainLifecycle::Expired),
        (29, DomainLifecycle::Expired),
        (1, DomainLifecycle::Redemption),
        (29, DomainLifecycle::Redemption),
        (1, DomainLifecycle::Released),
    ] {
        registrar.advance_days(days);
        let Ok(Some(found)) = registrar.lookup("brightwave.org".to_owned()).await else {
            panic!("an ageing domain disappeared from the registrar");
        };
        assert_eq!(found.status, expected, "after {days} more days");
    }

    // Past redemption the registry has sold it on; taking a renewal fee would
    // be taking money for nothing.
    assert!(matches!(
        registrar
            .renew("brightwave.org".to_owned(), 1, "renew-00000003".to_owned())
            .await,
        Err(RegistrarError::Unavailable)
    ));
}

#[tokio::test]
async fn a_purchase_is_checked_before_anything_is_bought() {
    let registrar = registrar();
    let mut bad_registrant = order("brightwave.fr", 1, "order-00000006");
    bad_registrant.registrant.email = "not-an-address".to_owned();
    assert!(matches!(
        registrar.register(bad_registrant).await,
        Err(RegistrarError::Validation(_))
    ));

    let mut one_nameserver = order("brightwave.fr", 1, "order-00000007");
    one_nameserver.nameservers = vec!["ns1.alo.example".to_owned()];
    assert!(matches!(
        registrar.register(one_nameserver).await,
        Err(RegistrarError::Validation(_))
    ));

    let mut too_long = order("brightwave.fr", 11, "order-00000008");
    too_long.years = 11;
    assert!(matches!(
        registrar.register(too_long).await,
        Err(RegistrarError::Validation(_))
    ));

    // None of the refusals registered anything.
    let Ok(nothing) = registrar.lookup("brightwave.fr".to_owned()).await else {
        panic!("looking up the refused name failed");
    };
    assert!(nothing.is_none());
}
