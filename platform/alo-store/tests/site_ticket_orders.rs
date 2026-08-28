//! The ticket order against a real database (ADR 0041, item S3.04c): order →
//! payment-reference → paid, driven end to end through the fixture payment
//! provider.
//!
//! The tests this suite exists for: the full arc (hold → order → hosted
//! payment → webhook target → status fetched → paid, seats sold), the webhook
//! replayed being one sale, money that arrives after the seats are gone
//! failing **visibly**, and the columns-of-the-table proof that no card data
//! can live in alo. Around them, the frame every storage suite carries: the
//! tenant and site walls on every verb.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, BillingProductId, FixtureSitePayments, SiteId, SitePaymentProvider,
    SitePaymentRequest, SitePaymentStatus, SiteTicketEventId, SiteTicketHoldId,
    SiteTicketHoldState, SiteTicketOrderState, StoreError, TICKET_ORDER_PAID_AFTER_LAPSE,
};
use time::{Duration, OffsetDateTime};

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

fn conflict_of<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(said)) => said,
        other => panic!("expected Conflict, got {other:?}"),
    }
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

fn clock() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

/// A provider payment id no other run of this suite can have used: the
/// one-payment-one-order index is global by design (it is the webhook's
/// lookup key), and the shared test database remembers every earlier run.
fn ppid(tag: &str) -> String {
    format!("fixpay-{tag}-{}", SiteId::generate().as_str())
}

const TTL: Duration = Duration::minutes(10);

/// A tenant, a site, a sellable 8 500-cent product, an event with seats, and
/// a live hold on two of them — the moment a buyer clicks "buy".
struct Venue {
    account: AccountStore,
    site: SiteId,
    product: BillingProductId,
    event: SiteTicketEventId,
    hold: SiteTicketHoldId,
    now: OffsetDateTime,
}

async fn venue(tag: &str) -> Venue {
    let store = common::test_store().await;
    let tenant = store
        .create_tenant(&format!("ticket-orders-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("owner@{tag}.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    common::seed_default_chart(&account).await;
    let site = account.create_site("Venue", &subdomain(tag)).await.unwrap();
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
        .create_site_ticket_event(&site, &product, now + Duration::days(7), 10)
        .await
        .unwrap();
    let hold = account
        .take_ticket_hold(&site, &event, 2, TTL, now)
        .await
        .unwrap();
    Venue {
        account,
        site,
        product,
        event,
        hold: hold.id,
        now,
    }
}

/// The provider request the checkout route will build — the fixture only
/// needs it to be well-formed.
fn payment_request(key: &str, amount_cents: i64) -> SitePaymentRequest {
    SitePaymentRequest {
        idempotency_key: key.to_owned(),
        amount_cents,
        currency: "EUR".to_owned(),
        description: "2 × Letterpress workshop".to_owned(),
        redirect_url: "https://venue.alosites.com/tickets/thanks".to_owned(),
        webhook_url: "https://venue.alosites.com/pay/webhook".to_owned(),
    }
}

#[tokio::test]
async fn the_arc_from_hold_to_paid_sells_the_seats() {
    let v = venue("arc").await;
    let store = common::test_store().await;
    let provider = FixtureSitePayments::new();

    // The order records the buyer and the price list's answer: 2 × 8 500.
    let order = v
        .account
        .create_ticket_order(&v.site, &v.hold, "Maud Adams", "maud@example.org", v.now)
        .await
        .unwrap();
    assert_eq!(order.state, SiteTicketOrderState::Pending);
    assert_eq!(order.quantity, 2);
    assert_eq!(order.unit_price_cents, 8_500);
    assert_eq!(order.amount_cents, 17_000);
    assert_eq!(order.vat_rate_bp, 2100);
    assert_eq!(order.currency, "EUR");
    assert_eq!(order.event, v.event);

    // The hosted handoff: the provider mints the payment, alo stores the
    // reference and where the checkout lives.
    let created = provider
        .create_payment(payment_request(order.id.as_str(), order.amount_cents))
        .await
        .unwrap();
    let waiting = v
        .account
        .open_ticket_payment(
            &v.site,
            &order.id,
            &created.provider_payment_id,
            &created.checkout_url,
        )
        .await
        .unwrap();
    assert_eq!(waiting.state, SiteTicketOrderState::AwaitingPayment);
    assert_eq!(
        waiting.checkout_url.as_deref(),
        Some(created.checkout_url.as_str())
    );

    // The buyer pays on the provider's page; its webhook names the payment.
    provider
        .mark(&created.provider_payment_id, SitePaymentStatus::Paid)
        .unwrap();
    let target = store
        .ticket_payment_target(&created.provider_payment_id)
        .await
        .unwrap()
        .expect("the payment names an order");
    assert_eq!(target.order, order.id);
    assert_eq!(target.site, v.site);

    // The status is fetched from the provider — never read from the webhook —
    // and settles the order and the hold as one decision.
    let status = provider
        .payment_status(created.provider_payment_id.clone())
        .await
        .unwrap();
    let paid = v
        .account
        .apply_ticket_payment(&target.site, &target.order, status, v.now)
        .await
        .unwrap();
    assert_eq!(paid.state, SiteTicketOrderState::Paid);
    assert!(paid.paid_at.is_some());
    assert!(paid.failure.is_none());

    let hold = v
        .account
        .site_ticket_hold(&v.site, &v.hold, v.now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(hold.state, SiteTicketHoldState::Completed);
    let seats = v
        .account
        .ticket_availability(&v.site, &v.event, v.now)
        .await
        .unwrap();
    assert_eq!(seats.sold, 2);
    assert_eq!(seats.held, 0);
    assert_eq!(seats.remaining, 8);
}

#[tokio::test]
async fn a_webhook_replayed_five_times_is_one_sale() {
    let v = venue("replay").await;
    let order = v
        .account
        .create_ticket_order(&v.site, &v.hold, "Maud Adams", "maud@example.org", v.now)
        .await
        .unwrap();
    v.account
        .open_ticket_payment(
            &v.site,
            &order.id,
            &ppid("replay"),
            "https://checkout.fixture.invalid/replay",
        )
        .await
        .unwrap();
    let first = v
        .account
        .apply_ticket_payment(&v.site, &order.id, SitePaymentStatus::Paid, v.now)
        .await
        .unwrap();
    for _ in 0..4 {
        let again = v
            .account
            .apply_ticket_payment(&v.site, &order.id, SitePaymentStatus::Paid, v.now)
            .await
            .unwrap();
        assert_eq!(again.state, SiteTicketOrderState::Paid);
        assert_eq!(again.paid_at, first.paid_at);
    }
    let seats = v
        .account
        .ticket_availability(&v.site, &v.event, v.now)
        .await
        .unwrap();
    assert_eq!(seats.sold, 2, "a replayed webhook must not sell twice");
}

#[tokio::test]
async fn one_payment_settles_exactly_one_order() {
    let v = venue("onepay").await;
    let order = v
        .account
        .create_ticket_order(&v.site, &v.hold, "Maud Adams", "maud@example.org", v.now)
        .await
        .unwrap();
    let shared = ppid("shared");
    v.account
        .open_ticket_payment(
            &v.site,
            &order.id,
            &shared,
            "https://checkout.fixture.invalid/a",
        )
        .await
        .unwrap();

    let second_hold = v
        .account
        .take_ticket_hold(&v.site, &v.event, 1, TTL, v.now)
        .await
        .unwrap();
    let second = v
        .account
        .create_ticket_order(
            &v.site,
            &second_hold.id,
            "Iris Bell",
            "iris@example.org",
            v.now,
        )
        .await
        .unwrap();
    let said = conflict_of(
        v.account
            .open_ticket_payment(
                &v.site,
                &second.id,
                &shared,
                "https://checkout.fixture.invalid/b",
            )
            .await,
    );
    assert!(said.contains("another order"), "said: {said}");
}

#[tokio::test]
async fn money_after_the_seats_are_gone_fails_visibly() {
    let v = venue("late").await;
    let order = v
        .account
        .create_ticket_order(&v.site, &v.hold, "Maud Adams", "maud@example.org", v.now)
        .await
        .unwrap();
    v.account
        .open_ticket_payment(
            &v.site,
            &order.id,
            &ppid("late"),
            "https://checkout.fixture.invalid/late",
        )
        .await
        .unwrap();

    // The buyer dawdled past the hold's expiry, then paid. The seats may
    // already be resold: the order fails visibly, naming the refund — it
    // never sells seats it no longer holds.
    let after_expiry = v.now + TTL + Duration::minutes(1);
    let failed = v
        .account
        .apply_ticket_payment(&v.site, &order.id, SitePaymentStatus::Paid, after_expiry)
        .await
        .unwrap();
    assert_eq!(failed.state, SiteTicketOrderState::Failed);
    assert_eq!(
        failed.failure.as_deref(),
        Some(TICKET_ORDER_PAID_AFTER_LAPSE)
    );
    assert!(failed.paid_at.is_none());

    let seats = v
        .account
        .ticket_availability(&v.site, &v.event, after_expiry)
        .await
        .unwrap();
    assert_eq!(seats.sold, 0);
    assert_eq!(seats.remaining, 10, "the lapsed seats stay on sale");
}

#[tokio::test]
async fn a_dead_status_frees_the_seats_and_a_later_payment_names_the_refund() {
    let v = venue("dead").await;
    let order = v
        .account
        .create_ticket_order(&v.site, &v.hold, "Maud Adams", "maud@example.org", v.now)
        .await
        .unwrap();
    v.account
        .open_ticket_payment(
            &v.site,
            &order.id,
            &ppid("dead"),
            "https://checkout.fixture.invalid/dead",
        )
        .await
        .unwrap();

    // The buyer cancelled on the provider's page: the order closes and the
    // seats go straight back on sale — no waiting for the hold to time out.
    let cancelled = v
        .account
        .apply_ticket_payment(&v.site, &order.id, SitePaymentStatus::Canceled, v.now)
        .await
        .unwrap();
    assert_eq!(cancelled.state, SiteTicketOrderState::Cancelled);
    let hold = v
        .account
        .site_ticket_hold(&v.site, &v.hold, v.now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(hold.state, SiteTicketHoldState::Released);
    let seats = v
        .account
        .ticket_availability(&v.site, &v.event, v.now)
        .await
        .unwrap();
    assert_eq!(seats.remaining, 10);

    // The cancel webhook replayed changes nothing.
    let again = v
        .account
        .apply_ticket_payment(&v.site, &order.id, SitePaymentStatus::Canceled, v.now)
        .await
        .unwrap();
    assert_eq!(again.state, SiteTicketOrderState::Cancelled);

    // And a paid status arriving after the cancel is money with no seats:
    // visible failure, naming the refund.
    let late = v
        .account
        .apply_ticket_payment(&v.site, &order.id, SitePaymentStatus::Paid, v.now)
        .await
        .unwrap();
    assert_eq!(late.state, SiteTicketOrderState::Failed);
    assert_eq!(late.failure.as_deref(), Some(TICKET_ORDER_PAID_AFTER_LAPSE));
}

#[tokio::test]
async fn a_paid_order_is_never_unsold_by_a_late_status() {
    let v = venue("sold").await;
    let order = v
        .account
        .create_ticket_order(&v.site, &v.hold, "Maud Adams", "maud@example.org", v.now)
        .await
        .unwrap();
    v.account
        .open_ticket_payment(
            &v.site,
            &order.id,
            &ppid("sold"),
            "https://checkout.fixture.invalid/sold",
        )
        .await
        .unwrap();
    v.account
        .apply_ticket_payment(&v.site, &order.id, SitePaymentStatus::Paid, v.now)
        .await
        .unwrap();
    for status in [
        SitePaymentStatus::Failed,
        SitePaymentStatus::Canceled,
        SitePaymentStatus::Expired,
        SitePaymentStatus::Open,
    ] {
        let still = v
            .account
            .apply_ticket_payment(&v.site, &order.id, status, v.now)
            .await
            .unwrap();
        assert_eq!(
            still.state,
            SiteTicketOrderState::Paid,
            "{status:?} unsold a sale"
        );
    }
    let seats = v
        .account
        .ticket_availability(&v.site, &v.event, v.now)
        .await
        .unwrap();
    assert_eq!(seats.sold, 2);
}

#[tokio::test]
async fn creation_replays_for_the_same_buyer_and_refuses_another() {
    let v = venue("createreplay").await;
    let order = v
        .account
        .create_ticket_order(&v.site, &v.hold, "Maud Adams", "maud@example.org", v.now)
        .await
        .unwrap();
    // The double-clicked buy button reaches the order it already made.
    let again = v
        .account
        .create_ticket_order(&v.site, &v.hold, "Maud Adams", "maud@example.org", v.now)
        .await
        .unwrap();
    assert_eq!(again.id, order.id);
    // A different buyer on the same seats is refused, never a quiet swap.
    let said = conflict_of(
        v.account
            .create_ticket_order(&v.site, &v.hold, "Iris Bell", "iris@example.org", v.now)
            .await,
    );
    assert!(said.contains("different buyer"), "said: {said}");
}

#[tokio::test]
async fn an_order_needs_a_live_hold_and_a_listed_price() {
    let v = venue("gates").await;

    // A lapsed hold cannot be bought — the seats may belong to somebody else.
    let after_expiry = v.now + TTL + Duration::minutes(1);
    let said = conflict_of(
        v.account
            .create_ticket_order(
                &v.site,
                &v.hold,
                "Maud Adams",
                "maud@example.org",
                after_expiry,
            )
            .await,
    );
    assert!(said.contains("expired"), "said: {said}");

    // A released hold likewise.
    let released = v
        .account
        .take_ticket_hold(&v.site, &v.event, 1, TTL, v.now)
        .await
        .unwrap();
    v.account
        .release_ticket_hold(&v.site, &released.id, v.now)
        .await
        .unwrap();
    let said = conflict_of(
        v.account
            .create_ticket_order(
                &v.site,
                &released.id,
                "Maud Adams",
                "maud@example.org",
                v.now,
            )
            .await,
    );
    assert!(said.contains("released"), "said: {said}");

    // An archived product answers for nothing: the shop can never sell the
    // past, so the order is refused at the door.
    let live = v
        .account
        .take_ticket_hold(&v.site, &v.event, 1, TTL, v.now)
        .await
        .unwrap();
    v.account
        .set_billing_product_archived(&v.product, true)
        .await
        .unwrap();
    let said = conflict_of(
        v.account
            .create_ticket_order(&v.site, &live.id, "Maud Adams", "maud@example.org", v.now)
            .await,
    );
    assert!(said.contains("price list"), "said: {said}");

    // Malformed buyers never reach the database.
    assert!(matches!(
        v.account
            .create_ticket_order(&v.site, &live.id, "", "maud@example.org", v.now)
            .await,
        Err(StoreError::Validation(_))
    ));
    assert!(matches!(
        v.account
            .create_ticket_order(&v.site, &live.id, "Maud", "not-an-address", v.now)
            .await,
        Err(StoreError::Validation(_))
    ));
}

#[tokio::test]
async fn the_tenant_and_site_walls_hold_on_every_verb() {
    let a = venue("wall-a").await;
    let b = venue("wall-b").await;

    // Tenant B addressing tenant A's records: a clean NotFound on every verb,
    // never data and never a 500.
    assert_not_found(
        b.account
            .create_ticket_order(&a.site, &a.hold, "Sly Fox", "fox@example.org", b.now)
            .await,
    );
    let a_order = a
        .account
        .create_ticket_order(&a.site, &a.hold, "Maud Adams", "maud@example.org", a.now)
        .await
        .unwrap();
    assert_not_found(
        b.account
            .open_ticket_payment(
                &a.site,
                &a_order.id,
                &ppid("wall"),
                "https://checkout.fixture.invalid/b",
            )
            .await,
    );
    assert_not_found(
        b.account
            .apply_ticket_payment(&a.site, &a_order.id, SitePaymentStatus::Paid, b.now)
            .await,
    );
    assert!(
        b.account
            .site_ticket_order(&a.site, &a_order.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        b.account
            .site_ticket_orders(&a.site)
            .await
            .unwrap()
            .is_empty()
    );

    // The same walls between two sites of ONE tenant: B's own site cannot
    // reach A's hold, and a hold from another of the tenant's sites is not
    // orderable through this one.
    let second_site = a
        .account
        .create_site("Annex", &subdomain("wall-annex"))
        .await
        .unwrap();
    assert_not_found(
        a.account
            .create_ticket_order(
                &second_site,
                &a.hold,
                "Maud Adams",
                "maud@example.org",
                a.now,
            )
            .await,
    );
    assert_not_found(
        a.account
            .apply_ticket_payment(&second_site, &a_order.id, SitePaymentStatus::Paid, a.now)
            .await,
    );

    // A's view is untouched by everything B tried.
    let mine = a.account.site_ticket_orders(&a.site).await.unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].state, SiteTicketOrderState::Pending);
}

#[tokio::test]
async fn the_webhook_door_answers_none_for_strangers() {
    let v = venue("door").await;
    let store = common::test_store().await;
    let order = v
        .account
        .create_ticket_order(&v.site, &v.hold, "Maud Adams", "maud@example.org", v.now)
        .await
        .unwrap();
    let door = ppid("door");
    v.account
        .open_ticket_payment(
            &v.site,
            &order.id,
            &door,
            "https://checkout.fixture.invalid/door",
        )
        .await
        .unwrap();

    // A probe with a guessed or garbage id learns nothing.
    assert!(
        store
            .ticket_payment_target("fixpay-nobody")
            .await
            .unwrap()
            .is_none()
    );
    assert!(store.ticket_payment_target("").await.unwrap().is_none());
    assert!(
        store
            .ticket_payment_target("two words")
            .await
            .unwrap()
            .is_none()
    );

    // The real id names exactly its own order.
    let target = store
        .ticket_payment_target(&door)
        .await
        .unwrap()
        .expect("the payment names its order");
    assert_eq!(target.order, order.id);
    assert_eq!(target.site, v.site);
}

#[tokio::test]
async fn the_table_has_no_room_for_a_card() {
    // Make sure migrations have run, then read the schema itself.
    let _ = common::test_store().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::database_url())
        .await
        .unwrap();
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
          WHERE table_name = 'site_ticket_orders' ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    // The exact column list IS the privacy proof: a column that could carry
    // a card number, an expiry, a CVC or a cardholder would have to appear
    // here, in a diff a reviewer reads next to this sentence.
    assert_eq!(
        columns,
        vec![
            "amount_cents",
            "buyer_email",
            "buyer_name",
            "checkout_url",
            "created_at",
            "currency",
            "event_id",
            "failure",
            "hold_id",
            "id",
            "paid_at",
            "provider_payment_id",
            "quantity",
            "site_id",
            "state",
            "tenant_id",
            "unit_price_cents",
            "updated_at",
            "vat_rate_bp",
        ]
    );
    for column in &columns {
        for forbidden in ["card", "pan", "cvc", "cvv", "expiry", "holder", "iban"] {
            assert!(
                !column.contains(forbidden),
                "column '{column}' could carry payment-instrument data"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Fulfilment (item S3.04d): a paid sale is made good — the ticket minted, the
// invoice raised and settled in Billing, the buyer in CRM — at most once, and
// walled per tenant and per site.
//
// Every call to `claim_ticket_fulfilments` lives inside ONE test function:
// the claim sweep is global by design (it is a system worker), so two tests
// claiming concurrently would claim each other's orders. One claiming test
// cannot race itself.
// ---------------------------------------------------------------------------

use alo_store::{
    ClaimedTicketFulfilment, DealFilter, NewBillingSettings, PipelineSeed, SitePublicStore,
    SiteTicketFulfilmentId, SiteTicketOrder, SiteTicketOrderId, StageSeed, Store,
    TicketFulfilWords, TicketMailNotification,
};

fn fulfil_words() -> TicketFulfilWords {
    TicketFulfilWords {
        unit: "ticket",
        fallback_item: "Event ticket",
        payment_method: "Hosted checkout",
        crm_title: "Ticket sale",
    }
}

/// The board a first capture seeds — the caller's strings, as the worker
/// resolves them per site language.
fn crm_seed() -> PipelineSeed {
    PipelineSeed {
        name: "Sales".to_owned(),
        stages: [
            ("New", false, false),
            ("Won", true, false),
            ("Lost", false, true),
        ]
        .into_iter()
        .map(|(name, is_won, is_lost)| StageSeed {
            name: name.to_owned(),
            is_won,
            is_lost,
        })
        .collect(),
    }
}

/// Order → hosted payment → webhook-confirmed paid, exactly as S3.04c wires
/// it — the moment fulfilment exists to follow.
async fn paid(v: &Venue, tag: &str, hold: &SiteTicketHoldId, email: &str) -> SiteTicketOrder {
    let order = v
        .account
        .create_ticket_order(&v.site, hold, "Maud Adams", email, v.now)
        .await
        .unwrap();
    v.account
        .open_ticket_payment(
            &v.site,
            &order.id,
            &ppid(tag),
            "https://checkout.fixture.invalid/fulfil",
        )
        .await
        .unwrap();
    v.account
        .apply_ticket_payment(&v.site, &order.id, SitePaymentStatus::Paid, v.now)
        .await
        .unwrap()
}

/// Claims rounds until the given order is offered. The shared database may
/// hold other suites' unfulfilled paid orders; each round makes progress, so
/// the loop is bounded.
async fn claim_for(store: &Store, order: &SiteTicketOrderId) -> ClaimedTicketFulfilment {
    for _ in 0..100 {
        let claims = store.claim_ticket_fulfilments(100).await.unwrap();
        let found = claims.iter().any(|claim| &claim.order == order);
        let ours = claims.into_iter().find(|claim| &claim.order == order);
        if found {
            return ours.unwrap();
        }
    }
    panic!("the paid order was never offered to the sweep");
}

#[tokio::test]
async fn a_paid_sale_is_made_good_once_and_walled() {
    let store = common::test_store().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&common::database_url())
        .await
        .unwrap();
    let public = SitePublicStore::new(
        pool.clone(),
        alo_store::BlobStore::in_memory(4 * 1024 * 1024),
    );

    // A venue whose seller profile can invoice, with a published site the
    // public ticket page will resolve.
    let v = venue("fulfil").await;
    let sub = v.account.site(&v.site).await.unwrap().unwrap().subdomain;
    v.account
        .create_site_page(&v.site, "Home", "", true)
        .await
        .unwrap();
    v.account.publish_site(&v.site).await.unwrap();
    v.account
        .save_billing_settings(&NewBillingSettings {
            legal_name: "Letterpress BV".to_owned(),
            country: "BE".to_owned(),
            ..NewBillingSettings::default()
        })
        .await
        .unwrap();
    let buyer = format!(
        "maud@{}.example",
        SiteId::generate().as_str().to_lowercase()
    );
    let order = paid(&v, "fulfil-a", &v.hold, &buyer).await;

    // The claim carries the sale and mints the ticket.
    let claim = claim_for(&store, &order.id).await;
    assert_eq!(claim.quantity, 2);
    assert_eq!(claim.amount_cents, 17_000);
    assert_eq!(claim.vat_rate_bp, 2100);
    assert_eq!(claim.buyer_email, buyer);
    assert_eq!(claim.site_subdomain, sub);
    assert!(!claim.token.is_empty());

    // The act: invoice raised and settled, buyer in CRM, record written.
    let outcome = store
        .fulfil_claimed_ticket(&claim, &fulfil_words(), &crm_seed())
        .await
        .unwrap();
    assert!(outcome.invoiced);
    assert!(outcome.lead_raised);

    // Billing's document: issued, referencing the order, worth no more than
    // the buyer paid (VAT carved out of the consumer price, never added on
    // top of it), and settled by the recorded hosted payment.
    let invoices = v.account.billing_invoices(None).await.unwrap();
    let summary = invoices
        .iter()
        .find(|summary| summary.invoice.reference == order.id.as_str())
        .expect("the sale has an invoice");
    assert!(summary.invoice.number.is_some());
    assert!(summary.totals.gross_cents <= 17_000);
    assert!(summary.totals.gross_cents >= 16_998);
    assert!(summary.totals.vat_cents > 0);
    assert_eq!(summary.paid_cents, 17_000);

    // CRM's card: one lead, titled by the caller, carrying the buyer.
    let deals = v.account.crm_deals(&DealFilter::default()).await.unwrap();
    assert_eq!(deals.len(), 1);
    assert_eq!(deals[0].title, "Ticket sale — Venue");
    assert_eq!(deals[0].contact_email, buyer);
    assert_eq!(deals[0].value_cents, 0, "a sale states no pipeline value");

    // At most once: no later round ever offers this order again.
    let again = store.claim_ticket_fulfilments(200).await.unwrap();
    assert!(
        again.iter().all(|c| c.order != order.id),
        "a fulfilled order must never be claimed twice"
    );

    // The buyer's ticket, on the site it was minted for.
    let site = public.resolve_published(&sub).await.unwrap().unwrap();
    let ticket = public
        .public_ticket(&site, &claim.token)
        .await
        .unwrap()
        .expect("the token answers on its own site");
    assert_eq!(ticket.quantity, 2);
    assert_eq!(ticket.holder, "Maud Adams");
    assert!(ticket.description.contains("Letterpress workshop"));

    // The walls: another tenant's site, a sibling site of the same tenant,
    // and garbage all get the same nothing.
    let stranger = venue("fulfil-wall").await;
    let stranger_sub = stranger
        .account
        .site(&stranger.site)
        .await
        .unwrap()
        .unwrap()
        .subdomain;
    stranger
        .account
        .create_site_page(&stranger.site, "Home", "", true)
        .await
        .unwrap();
    stranger.account.publish_site(&stranger.site).await.unwrap();
    let foreign = public
        .resolve_published(&stranger_sub)
        .await
        .unwrap()
        .unwrap();
    assert!(
        public
            .public_ticket(&foreign, &claim.token)
            .await
            .unwrap()
            .is_none()
    );
    let sibling_sub = subdomain("fulfil-sib");
    let sibling = v.account.create_site("Annex", &sibling_sub).await.unwrap();
    v.account
        .create_site_page(&sibling, "Home", "", true)
        .await
        .unwrap();
    v.account.publish_site(&sibling).await.unwrap();
    let sibling_site = public
        .resolve_published(&sibling_sub)
        .await
        .unwrap()
        .unwrap();
    assert!(
        public
            .public_ticket(&sibling_site, &claim.token)
            .await
            .unwrap()
            .is_none(),
        "a ticket answers only on the site it was minted for"
    );
    assert!(
        public
            .public_ticket(&site, "no-such-token")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        public
            .public_ticket(&site, "a token; drop")
            .await
            .unwrap()
            .is_none()
    );

    // The same buyer again: Billing reuses the customer, CRM answers
    // already-customer rather than raising a twin card.
    let hold2 = v
        .account
        .take_ticket_hold(&v.site, &v.event, 1, TTL, v.now)
        .await
        .unwrap();
    let order2 = paid(&v, "fulfil-b", &hold2.id, &buyer).await;
    let claim2 = claim_for(&store, &order2.id).await;
    let outcome2 = store
        .fulfil_claimed_ticket(&claim2, &fulfil_words(), &crm_seed())
        .await
        .unwrap();
    assert!(outcome2.invoiced);
    assert!(!outcome2.lead_raised, "a known buyer raises no second card");
    let customers = v.account.billing_customers(false).await.unwrap();
    assert_eq!(
        customers
            .iter()
            .filter(|c| c.email.as_deref() == Some(buyer.as_str()))
            .count(),
        1,
        "one buyer is one customer"
    );
    assert_eq!(v.account.billing_invoices(None).await.unwrap().len(), 2);
    assert_eq!(
        v.account
            .crm_deals(&DealFilter::default())
            .await
            .unwrap()
            .len(),
        1
    );

    // A venue that cannot invoice yet: the sale is still made good — ticket
    // and CRM — and the missing invoice is written down, not guessed.
    let bare = venue("fulfil-bare").await;
    let bare_buyer = format!("ada@{}.example", SiteId::generate().as_str().to_lowercase());
    let order3 = paid(&bare, "fulfil-c", &bare.hold, &bare_buyer).await;
    let claim3 = claim_for(&store, &order3.id).await;
    let outcome3 = store
        .fulfil_claimed_ticket(&claim3, &fulfil_words(), &crm_seed())
        .await
        .unwrap();
    assert!(!outcome3.invoiced, "no seller country, no invoice");
    assert!(
        bare.account
            .billing_invoices(None)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        bare.account
            .crm_deals(&DealFilter::default())
            .await
            .unwrap()
            .len(),
        1,
        "the buyer still reaches CRM"
    );
}

#[tokio::test]
async fn the_fulfilment_table_has_no_buyer_column() {
    let _ = common::test_store().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::database_url())
        .await
        .unwrap();
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
          WHERE table_name = 'site_ticket_fulfilments' ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    // The exact column list IS the privacy proof: who bought lives on the
    // order, and a column that could carry a person would have to appear
    // here, in a diff a reviewer reads next to this sentence.
    assert_eq!(
        columns,
        vec![
            "created_at",
            "crm_outcome",
            "description",
            "event_id",
            "id",
            "invoice_id",
            "invoice_note",
            "invoice_number",
            // The ticket email's at-most-once marker (ADR 0050): when the
            // buyer was mailed, never who — the address stays on the order.
            "mailed_at",
            "order_id",
            "site_id",
            "tenant_id",
            "token",
            "updated_at",
        ]
    );
    for column in &columns {
        for forbidden in ["name", "email", "phone", "buyer", "card", "address"] {
            assert!(
                !column.contains(forbidden),
                "column '{column}' could carry a person"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The ticket email's claim (ADR 0050, item S3.04h). Every call to
// `claim_ticket_mails` in the workspace lives inside the ONE test below: the
// claim spans tenants (that is what a sweep is), so a second concurrent
// claimer would take rows this test is watching — the same shape the other
// sweep suites serialise in `.config/nextest.toml`, solved here by having no
// second claimer at all.
// ---------------------------------------------------------------------------

/// Claims fulfilment rounds until every watched paid order has been offered.
/// Watching several orders in one loop matters: a single round can claim two
/// of them, and a per-order loop started afterwards would wait forever for a
/// claim another loop already received.
async fn claims_for(store: &Store, orders: &[&SiteTicketOrderId]) -> Vec<ClaimedTicketFulfilment> {
    let mut got: Vec<ClaimedTicketFulfilment> = Vec::new();
    for _ in 0..100 {
        let round = store.claim_ticket_fulfilments(100).await.unwrap();
        got.extend(
            round
                .into_iter()
                .filter(|claim| orders.iter().any(|order| **order == claim.order)),
        );
        if got.len() >= orders.len() {
            return got;
        }
    }
    panic!("a paid order was never offered to the fulfilment sweep");
}

/// Claims mail rounds until every watched fulfilment was offered or the
/// pending set drains; returns only the watched notifications. The shared
/// database may hold other suites' fulfilled sales — they are claimed too
/// (nothing else reads `mailed_at`), and only the watched ids are kept.
async fn claim_mail_for(
    store: &Store,
    want: &[&SiteTicketFulfilmentId],
    cap: i64,
) -> Vec<TicketMailNotification> {
    let mut got: Vec<TicketMailNotification> = Vec::new();
    for _ in 0..100 {
        let round = store.claim_ticket_mails(500, cap).await.unwrap();
        let drained = round.is_empty();
        got.extend(
            round
                .into_iter()
                .filter(|n| want.iter().any(|w| **w == n.fulfilment)),
        );
        if drained || got.len() >= want.len() {
            return got;
        }
    }
    got
}

async fn mailed_at_of(
    pool: &sqlx::PgPool,
    fulfilment: &SiteTicketFulfilmentId,
) -> Option<OffsetDateTime> {
    sqlx::query_scalar::<_, Option<OffsetDateTime>>(
        "SELECT mailed_at FROM site_ticket_fulfilments WHERE id = $1",
    )
    .bind(fulfilment.as_str())
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn the_ticket_mail_waits_for_fulfilment_claims_once_and_never_crosses_tenants() {
    let store = common::test_store().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::database_url())
        .await
        .unwrap();

    let a = venue("mail-a").await;
    let b = venue("mail-b").await;
    let order_a = paid(&a, "mail-a", &a.hold, "maud@mail-a.test").await;
    let order_b = paid(&b, "mail-b", &b.hold, "nils@mail-b.test").await;
    let claims = claims_for(&store, &[&order_a.id, &order_b.id]).await;
    let claim_a = claims
        .iter()
        .find(|c| c.order == order_a.id)
        .unwrap()
        .clone();
    let claim_b = claims
        .iter()
        .find(|c| c.order == order_b.id)
        .unwrap()
        .clone();

    // Before the fulfilment act there is no description: the sale must not
    // be offered to the mail sweep, however hard it claims.
    let early = claim_mail_for(&store, &[&claim_a.fulfilment, &claim_b.fulfilment], 200).await;
    assert!(
        early.is_empty(),
        "an unfulfilled sale was offered for mailing: {early:?}"
    );
    assert!(mailed_at_of(&pool, &claim_a.fulfilment).await.is_none());

    store
        .fulfil_claimed_ticket(&claim_a, &fulfil_words(), &crm_seed())
        .await
        .unwrap();
    store
        .fulfil_claimed_ticket(&claim_b, &fulfil_words(), &crm_seed())
        .await
        .unwrap();

    // Fulfilled: each sale is offered exactly once, paired with its OWN
    // tenant's site, buyer, token and owner.
    let offered = claim_mail_for(&store, &[&claim_a.fulfilment, &claim_b.fulfilment], 200).await;
    assert_eq!(
        offered.len(),
        2,
        "each fulfilled sale is offered exactly once: {offered:?}"
    );
    let mail_a = offered
        .iter()
        .find(|n| n.fulfilment == claim_a.fulfilment)
        .unwrap();
    let mail_b = offered
        .iter()
        .find(|n| n.fulfilment == claim_b.fulfilment)
        .unwrap();
    assert_eq!(mail_a.tenant, *a.account.tenant());
    assert_eq!(mail_a.owner, *a.account.user());
    assert_eq!(mail_a.site, a.site);
    assert_eq!(mail_a.buyer_email, "maud@mail-a.test");
    assert_eq!(mail_a.token, claim_a.token);
    assert!(
        mail_a.description.starts_with("Letterpress workshop"),
        "{}",
        mail_a.description
    );
    assert_eq!(mail_a.quantity, 2);
    assert_eq!(mail_a.amount_cents, 17_000);
    assert_eq!(mail_b.tenant, *b.account.tenant());
    assert_eq!(mail_b.buyer_email, "nils@mail-b.test");
    assert_ne!(mail_a.tenant, mail_b.tenant);

    // The identity the sweep replies through is tenant-scoped: the owner's
    // address resolves inside the sale's own tenant and nowhere else — a
    // foreign tenant's sale can never mail through another tenant's identity.
    let own = store
        .for_tenant(mail_a.tenant.clone())
        .email_of(&mail_a.owner)
        .await
        .unwrap();
    assert_eq!(own.as_deref(), Some("owner@mail-a.test"));
    let foreign = store
        .for_tenant(mail_b.tenant.clone())
        .email_of(&mail_a.owner)
        .await
        .unwrap();
    assert_eq!(
        foreign, None,
        "another tenant resolved this owner's address"
    );

    // At-most-once: the claim was the once; nothing is offered again.
    let again = claim_mail_for(&store, &[&claim_a.fulfilment, &claim_b.fulfilment], 200).await;
    assert!(
        again.is_empty(),
        "a mailed sale was offered again: {again:?}"
    );
    assert!(mailed_at_of(&pool, &claim_a.fulfilment).await.is_some());

    // The daily ceiling defers, never drops: a tenant at the cap keeps its
    // remaining sale pending for the next window, and a window with
    // allowance left releases it.
    let c = venue("mail-cap").await;
    let order_c1 = paid(&c, "mail-c1", &c.hold, "one@mail-cap.test").await;
    let second_hold = c
        .account
        .take_ticket_hold(&c.site, &c.event, 1, TTL, c.now)
        .await
        .unwrap();
    let order_c2 = paid(&c, "mail-c2", &second_hold.id, "two@mail-cap.test").await;
    let c_claims = claims_for(&store, &[&order_c1.id, &order_c2.id]).await;
    for claim in &c_claims {
        store
            .fulfil_claimed_ticket(claim, &fulfil_words(), &crm_seed())
            .await
            .unwrap();
    }
    let c_fulfilments: Vec<&SiteTicketFulfilmentId> =
        c_claims.iter().map(|c| &c.fulfilment).collect();

    let capped = claim_mail_for(&store, &c_fulfilments, 1).await;
    assert_eq!(
        capped.len(),
        1,
        "a cap of one mails exactly one: {capped:?}"
    );
    let held_back = c_claims
        .iter()
        .find(|claim| claim.fulfilment != capped[0].fulfilment)
        .unwrap();
    let still_capped = claim_mail_for(&store, &[&held_back.fulfilment], 1).await;
    assert!(still_capped.is_empty(), "the ceiling did not hold");
    assert!(
        mailed_at_of(&pool, &held_back.fulfilment).await.is_none(),
        "deferred means still pending, not dropped"
    );
    let released = claim_mail_for(&store, &[&held_back.fulfilment], 200).await;
    assert_eq!(
        released.len(),
        1,
        "allowance left releases the deferred sale"
    );
}
