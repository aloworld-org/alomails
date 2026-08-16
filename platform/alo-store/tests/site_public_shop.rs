//! The public ticket checkout against a real database (ADR 0041, item
//! S3.04f): what an anonymous visitor is offered, the purchase they start,
//! and what a stranger can reach (nothing).
//!
//! The hold arithmetic, the order state machine and the settle rules are
//! proven in their own suites (`site_ticket_holds`, `site_ticket_orders`);
//! this one proves the public door — the Host anchoring, the price read from
//! the seam at every offer, the typo gate that costs no seat, and the arc a
//! visitor actually walks: offer → checkout → hosted payment → settle →
//! ticket.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    AccountStore, BillingProductId, FixtureSitePayments, PublishedSite, SiteId,
    SitePaymentProvider, SitePaymentRequest, SitePaymentStatus, SitePublicStore, SiteTicketEventId,
    SiteTicketOrderState, Store, StoreError, TICKET_CHECKOUT_HOLD_TTL,
};
use sqlx::postgres::PgPoolOptions;
use time::{Duration, OffsetDateTime};

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

fn clock() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

/// One tenant with a published site selling seats to one event, and the
/// public door an anonymous visitor arrives through.
struct Venue {
    account: AccountStore,
    store: Store,
    site: SiteId,
    product: BillingProductId,
    event: SiteTicketEventId,
    public: SitePublicStore,
    resolved: PublishedSite,
    pool: sqlx::PgPool,
    now: OffsetDateTime,
}

/// A venue with an event of `capacity` seats, a week out, at 8 500 cents.
async fn venue(tag: &str, capacity: i32) -> Venue {
    let (store, blobs) = common::test_store_with_blobs().await;
    let tenant = store
        .create_tenant(&format!("public-shop-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("owner@{tag}.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let site_subdomain = subdomain(tag);
    let site = account.create_site("Venue", &site_subdomain).await.unwrap();
    account
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    account.publish_site(&site).await.unwrap();
    let product = account
        .create_billing_product(&alo_store::NewProduct {
            name: "Letterpress workshop".to_owned(),
            unit: "seat".to_owned(),
            unit_price_cents: 8_500,
            vat_rate_bp: 2100,
            ..Default::default()
        })
        .await
        .unwrap();
    let now = clock();
    let event = account
        .create_site_ticket_event(&site, &product, now + Duration::days(7), capacity)
        .await
        .unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&common::database_url())
        .await
        .unwrap();
    let public = SitePublicStore::new(pool.clone(), blobs);
    let resolved = public
        .resolve_published(&site_subdomain)
        .await
        .unwrap()
        .expect("the published site resolves");
    Venue {
        account,
        store,
        site,
        product,
        event,
        public,
        resolved,
        pool,
        now,
    }
}

/// The provider request the checkout route will build from a
/// [`alo_store::PublicTicketCheckout`].
fn payment_request(key: &str, amount_cents: i64, description: &str) -> SitePaymentRequest {
    SitePaymentRequest {
        idempotency_key: key.to_owned(),
        amount_cents,
        currency: "EUR".to_owned(),
        description: description.to_owned(),
        redirect_url: "https://venue.alosites.com/tix/thanks".to_owned(),
        webhook_url: "https://venue.alosites.com/_alo/pay".to_owned(),
    }
}

#[tokio::test]
async fn the_offer_is_the_price_lists_answer_now() {
    let v = venue("offer", 10).await;

    // A second event whose product has since left the price list, and a
    // third that has already started: neither may be offered.
    let archived_product = v
        .account
        .create_billing_product(&alo_store::NewProduct {
            name: "Retired evening".to_owned(),
            unit: "seat".to_owned(),
            unit_price_cents: 5_000,
            vat_rate_bp: 2100,
            ..Default::default()
        })
        .await
        .unwrap();
    v.account
        .create_site_ticket_event(&v.site, &archived_product, v.now + Duration::days(3), 5)
        .await
        .unwrap();
    v.account
        .set_billing_product_archived(&archived_product, true)
        .await
        .unwrap();
    let started = v
        .account
        .create_site_ticket_event(&v.site, &v.product, v.now - Duration::hours(2), 5)
        .await
        .unwrap();

    let offered = v
        .public
        .public_ticket_events(&v.resolved, v.now)
        .await
        .unwrap();
    assert_eq!(offered.len(), 1, "one sellable upcoming event: {offered:?}");
    let event = &offered[0];
    assert_eq!(event.id, v.event);
    assert_eq!(event.name, "Letterpress workshop");
    assert_eq!(event.unit_price_cents, 8_500);
    assert_eq!(event.currency, "EUR");
    assert_eq!(event.remaining, 10);

    // The single read agrees, and the misses are one uniform None.
    let one = v
        .public
        .public_ticket_event(&v.resolved, v.event.as_str(), v.now)
        .await
        .unwrap()
        .expect("the event is offered");
    assert_eq!(one, *event);
    for miss in [started.as_str(), "never-was", "", "two words", "ev;drop"] {
        assert!(
            v.public
                .public_ticket_event(&v.resolved, miss, v.now)
                .await
                .unwrap()
                .is_none(),
            "{miss:?} was offered"
        );
    }

    // A price change on the list is the offer's price at the next read —
    // nothing was copied anywhere.
    v.account
        .update_billing_product(
            &v.product,
            &alo_store::NewProduct {
                name: "Letterpress workshop".to_owned(),
                unit: "seat".to_owned(),
                unit_price_cents: 9_900,
                vat_rate_bp: 2100,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let repriced = v
        .public
        .public_ticket_event(&v.resolved, v.event.as_str(), v.now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(repriced.unit_price_cents, 9_900);
}

#[tokio::test]
async fn the_arc_from_offer_to_ticket() {
    let v = venue("arc", 10).await;
    let provider = FixtureSitePayments::new();

    let checkout = v
        .public
        .public_begin_ticket_checkout(
            &v.resolved,
            v.event.as_str(),
            2,
            "Maud Adams",
            "maud@example.org",
            v.now,
        )
        .await
        .unwrap()
        .expect("the event is on sale");
    assert_eq!(checkout.quantity, 2);
    assert_eq!(checkout.amount_cents, 17_000);
    assert_eq!(checkout.currency, "EUR");
    assert!(
        checkout
            .description
            .starts_with("2 × Letterpress workshop — "),
        "description was {:?}",
        checkout.description
    );
    assert_eq!(checkout.expires_at, v.now + TICKET_CHECKOUT_HOLD_TTL);

    // The seats are held from this instant.
    let offer = v
        .public
        .public_ticket_event(&v.resolved, v.event.as_str(), v.now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(offer.remaining, 8);

    // The hosted handoff, exactly as the /tix route will drive it.
    let created = provider
        .create_payment(payment_request(
            checkout.order.as_str(),
            checkout.amount_cents,
            &checkout.description,
        ))
        .await
        .unwrap();
    v.public
        .public_open_ticket_payment(
            &v.resolved,
            &checkout.order,
            &created.provider_payment_id,
            &created.checkout_url,
        )
        .await
        .unwrap()
        .expect("the order is this site's");

    let waiting = v
        .public
        .public_ticket_order(&v.resolved, checkout.order.as_str())
        .await
        .unwrap()
        .expect("the return page can see its order");
    assert_eq!(waiting.state, SiteTicketOrderState::AwaitingPayment);
    assert_eq!(
        waiting.checkout_url.as_deref(),
        Some(created.checkout_url.as_str())
    );
    assert_eq!(
        waiting.provider_payment_id.as_deref(),
        Some(created.provider_payment_id.as_str())
    );
    assert!(waiting.ticket_token.is_none());

    // The buyer pays; the webhook rings; the status is fetched and applied.
    provider
        .mark(&created.provider_payment_id, SitePaymentStatus::Paid)
        .unwrap();
    let target = v
        .public
        .public_ticket_payment_target(&created.provider_payment_id)
        .await
        .unwrap()
        .expect("the payment names an order");
    assert_eq!(target.order, checkout.order);
    let status = provider
        .payment_status(created.provider_payment_id.clone())
        .await
        .unwrap();
    v.public
        .public_settle_ticket_payment(&target, status, v.now)
        .await
        .unwrap();
    // The webhook replayed is one sale.
    v.public
        .public_settle_ticket_payment(&target, SitePaymentStatus::Paid, v.now)
        .await
        .unwrap();

    let paid = v
        .public
        .public_ticket_order(&v.resolved, checkout.order.as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(paid.state, SiteTicketOrderState::Paid);
    assert!(
        paid.checkout_url.is_none(),
        "a settled order offers no checkout link"
    );

    // Fulfilment mints the ticket; the return page then carries it. The
    // claim sweep is global and another suite may claim our row first, so
    // the truth is read from the table rather than from our claim's result.
    v.store.claim_ticket_fulfilments(500).await.unwrap();
    let token: Option<String> =
        sqlx::query_scalar("SELECT token FROM site_ticket_fulfilments WHERE order_id = $1")
            .bind(checkout.order.as_str())
            .fetch_optional(&v.pool)
            .await
            .unwrap();
    let token = token.expect("the paid order was claimed for fulfilment");
    let with_ticket = v
        .public
        .public_ticket_order(&v.resolved, checkout.order.as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(with_ticket.ticket_token.as_deref(), Some(token.as_str()));

    // Sold seats count forever.
    let after = v
        .public
        .public_ticket_event(&v.resolved, v.event.as_str(), v.now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.remaining, 8);
}

#[tokio::test]
async fn a_typo_costs_no_seat_and_the_seats_speak_for_themselves() {
    let v = venue("gates", 2).await;

    // The typo gate runs before the hold: nothing is reserved for a bad
    // address, a bad name, or a silly quantity.
    for (quantity, name, email) in [
        (1, "Maud Adams", "not-an-address"),
        (1, "   ", "maud@example.org"),
        (0, "Maud Adams", "maud@example.org"),
        (21, "Maud Adams", "maud@example.org"),
    ] {
        let refused = v
            .public
            .public_begin_ticket_checkout(
                &v.resolved,
                v.event.as_str(),
                quantity,
                name,
                email,
                v.now,
            )
            .await;
        assert!(
            matches!(refused, Err(StoreError::Validation(_))),
            "({quantity}, {name:?}, {email:?}) got {refused:?}"
        );
    }
    let untouched = v
        .account
        .ticket_availability(&v.site, &v.event, v.now)
        .await
        .unwrap();
    assert_eq!(untouched.held, 0);
    assert_eq!(untouched.remaining, 2);

    // The last seats go to the buyer who starts first; the next visitor is
    // told so in the seats' own words.
    v.public
        .public_begin_ticket_checkout(
            &v.resolved,
            v.event.as_str(),
            2,
            "Maud Adams",
            "maud@example.org",
            v.now,
        )
        .await
        .unwrap()
        .expect("the seats were there");
    let told = v
        .public
        .public_begin_ticket_checkout(
            &v.resolved,
            v.event.as_str(),
            1,
            "Ada Lovelace",
            "ada@example.test",
            v.now,
        )
        .await;
    match told {
        Err(StoreError::Conflict(said)) => assert_eq!(said, "sold out"),
        other => panic!("expected the sold-out sentence, got {other:?}"),
    }
}

#[tokio::test]
async fn a_dead_payment_frees_the_seats() {
    let v = venue("dead", 10).await;
    let provider = FixtureSitePayments::new();

    let checkout = v
        .public
        .public_begin_ticket_checkout(
            &v.resolved,
            v.event.as_str(),
            3,
            "Maud Adams",
            "maud@example.org",
            v.now,
        )
        .await
        .unwrap()
        .unwrap();
    let created = provider
        .create_payment(payment_request(
            checkout.order.as_str(),
            checkout.amount_cents,
            &checkout.description,
        ))
        .await
        .unwrap();
    v.public
        .public_open_ticket_payment(
            &v.resolved,
            &checkout.order,
            &created.provider_payment_id,
            &created.checkout_url,
        )
        .await
        .unwrap()
        .unwrap();
    provider
        .mark(&created.provider_payment_id, SitePaymentStatus::Canceled)
        .unwrap();
    let target = v
        .public
        .public_ticket_payment_target(&created.provider_payment_id)
        .await
        .unwrap()
        .unwrap();
    let status = provider
        .payment_status(created.provider_payment_id.clone())
        .await
        .unwrap();
    v.public
        .public_settle_ticket_payment(&target, status, v.now)
        .await
        .unwrap();

    let closed = v
        .public
        .public_ticket_order(&v.resolved, checkout.order.as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(closed.state, SiteTicketOrderState::Cancelled);
    assert!(closed.checkout_url.is_none());
    let freed = v
        .public
        .public_ticket_event(&v.resolved, v.event.as_str(), v.now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(freed.remaining, 10, "the buyer is not coming back");
}

#[tokio::test]
async fn the_walls_hold_on_every_door() {
    let a = venue("wall-a", 10).await;
    let b = venue("wall-b", 10).await;

    // A checkout of A's, to test the order and payment walls with.
    let checkout = a
        .public
        .public_begin_ticket_checkout(
            &a.resolved,
            a.event.as_str(),
            1,
            "Maud Adams",
            "maud@example.org",
            a.now,
        )
        .await
        .unwrap()
        .unwrap();

    // Another tenant's Host resolves A's ids to nothing — offer, checkout,
    // order and payment-open alike.
    assert!(
        b.public
            .public_ticket_event(&b.resolved, a.event.as_str(), b.now)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        b.public
            .public_begin_ticket_checkout(
                &b.resolved,
                a.event.as_str(),
                1,
                "Ada Lovelace",
                "ada@example.test",
                b.now,
            )
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        b.public
            .public_ticket_order(&b.resolved, checkout.order.as_str())
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        b.public
            .public_open_ticket_payment(
                &b.resolved,
                &checkout.order,
                "fixpay-stranger",
                "https://pay.example.test/x",
            )
            .await
            .unwrap()
            .is_none()
    );
    // B's own list is untouched by any of it.
    let b_events = b
        .public
        .public_ticket_events(&b.resolved, b.now)
        .await
        .unwrap();
    assert_eq!(b_events.len(), 1);
    assert_eq!(b_events[0].remaining, 10);

    // The same tenant's *other* site is the same stranger.
    let second_subdomain = subdomain("wall-a2");
    let second = a
        .account
        .create_site("Annex", &second_subdomain)
        .await
        .unwrap();
    a.account
        .create_site_page(&second, "Home", "", true)
        .await
        .unwrap();
    a.account.publish_site(&second).await.unwrap();
    let second_resolved = a
        .public
        .resolve_published(&second_subdomain)
        .await
        .unwrap()
        .unwrap();
    assert!(
        a.public
            .public_ticket_event(&second_resolved, a.event.as_str(), a.now)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        a.public
            .public_ticket_order(&second_resolved, checkout.order.as_str())
            .await
            .unwrap()
            .is_none()
    );

    // The webhook door tells an unauthenticated probe nothing.
    for probe in ["", "never-was", "fixpay-;drop", &"x".repeat(300)] {
        assert!(
            a.public
                .public_ticket_payment_target(probe)
                .await
                .unwrap()
                .is_none(),
            "{probe:?} answered"
        );
    }
}
