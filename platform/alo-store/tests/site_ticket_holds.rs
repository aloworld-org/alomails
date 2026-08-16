//! Ticketed events and the hold-with-expiry against a real database
//! (ADR 0041, item S3.04b).
//!
//! The one test this suite exists for is the race: **two simultaneous buyers
//! after the last seat, exactly one of whom may get it**. Everything else is
//! the frame around it — the event that can only sell what the price list
//! answers for, the tenant and site walls, expiry freeing seats by time
//! passing, completion holding them forever, and the columns-of-the-table
//! proof that a hold carries no buyer identity at all.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    AccountStore, BillingProductId, NewProduct, SITE_TICKET_EVENT_MAX_PER_SITE, SiteId,
    SiteTicketHoldState, StoreError, TICKET_HOLD_MAX_QUANTITY, TICKET_HOLD_MAX_TTL,
    TICKET_HOLD_MIN_TTL,
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

fn validation_of<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(said)) => said,
        other => panic!("expected Validation, got {other:?}"),
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

/// A dated product the way wave one sells one: a seat with a price.
fn workshop() -> NewProduct {
    NewProduct {
        name: "Letterpress workshop".to_owned(),
        unit: "seat".to_owned(),
        unit_price_cents: 8_500,
        vat_rate_bp: 2100,
        ..Default::default()
    }
}

/// A tenant, a user's account door, a site, and a sellable product.
async fn venue(tag: &str) -> (AccountStore, SiteId, BillingProductId) {
    let store = common::test_store().await;
    let tenant = store
        .create_tenant(&format!("tickets-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("owner@{tag}.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let site = account.create_site("Venue", &subdomain(tag)).await.unwrap();
    let product = account.create_billing_product(&workshop()).await.unwrap();
    (account, site, product)
}

/// An instant comfortably before the event, so holds are live unless a test
/// says otherwise.
fn clock() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

const TTL: Duration = Duration::minutes(10);

#[tokio::test]
async fn an_event_round_trips_and_lists_in_start_order() {
    let (account, site, product) = venue("roundtrip").await;
    let now = clock();
    let later = account
        .create_site_ticket_event(&site, &product, now + Duration::days(14), 40)
        .await
        .unwrap();
    let sooner = account
        .create_site_ticket_event(&site, &product, now + Duration::days(7), 12)
        .await
        .unwrap();

    let events = account.site_ticket_events(&site).await.unwrap();
    assert_eq!(
        events.iter().map(|e| e.id.clone()).collect::<Vec<_>>(),
        vec![sooner.clone(), later]
    );
    let read = account
        .site_ticket_event(&site, &sooner)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.capacity, 12);
    assert_eq!(read.product, product);
}

#[tokio::test]
async fn an_event_can_only_sell_what_the_price_list_answers_for() {
    let (account, site, product) = venue("pricelist").await;
    let now = clock();

    let ghost = BillingProductId::generate();
    let said = validation_of(
        account
            .create_site_ticket_event(&site, &ghost, now + Duration::days(7), 10)
            .await,
    );
    assert!(said.contains("price list"), "{said}");

    account
        .set_billing_product_archived(&product, true)
        .await
        .unwrap();
    let said = validation_of(
        account
            .create_site_ticket_event(&site, &product, now + Duration::days(7), 10)
            .await,
    );
    assert!(said.contains("price list"), "{said}");
}

#[tokio::test]
async fn capacity_and_quantity_and_ttl_bounds_hold() {
    let (account, site, product) = venue("bounds").await;
    let now = clock();
    let starts = now + Duration::days(7);

    assert!(matches!(
        account
            .create_site_ticket_event(&site, &product, starts, 0)
            .await,
        Err(StoreError::Validation(_))
    ));
    let event = account
        .create_site_ticket_event(&site, &product, starts, 5)
        .await
        .unwrap();

    for bad in [0, TICKET_HOLD_MAX_QUANTITY + 1] {
        assert!(matches!(
            account.take_ticket_hold(&site, &event, bad, TTL, now).await,
            Err(StoreError::Validation(_))
        ));
    }
    for bad in [
        TICKET_HOLD_MIN_TTL - Duration::seconds(1),
        TICKET_HOLD_MAX_TTL + Duration::seconds(1),
    ] {
        assert!(matches!(
            account.take_ticket_hold(&site, &event, 1, bad, now).await,
            Err(StoreError::Validation(_))
        ));
    }
}

#[tokio::test]
async fn the_tenant_and_site_walls_hold() {
    let store = common::test_store().await;
    let (account_a, site_a, product_a) = venue("wall-a").await;
    let now = clock();
    let event_a = account_a
        .create_site_ticket_event(&site_a, &product_a, now + Duration::days(7), 10)
        .await
        .unwrap();
    let hold_a = account_a
        .take_ticket_hold(&site_a, &event_a, 2, TTL, now)
        .await
        .unwrap();

    // A second tenant, holding tenant A's real ids.
    let tenant_b = store.create_tenant("tickets-wall-b").await.unwrap();
    let user_b = store
        .for_tenant(tenant_b.clone())
        .create_user("owner@wall-b.test")
        .await
        .unwrap();
    let account_b = store.for_account(tenant_b, user_b);
    let site_b = account_b
        .create_site("Venue B", &subdomain("wall-b"))
        .await
        .unwrap();

    // Nothing of A's answers to B, under any verb.
    assert_not_found(
        account_b
            .create_site_ticket_event(&site_a, &product_a, now + Duration::days(7), 10)
            .await,
    );
    assert!(
        account_b
            .site_ticket_event(&site_a, &event_a)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        account_b
            .site_ticket_events(&site_a)
            .await
            .unwrap()
            .is_empty()
    );
    assert_not_found(
        account_b
            .take_ticket_hold(&site_a, &event_a, 1, TTL, now)
            .await,
    );
    assert_not_found(
        account_b
            .complete_ticket_hold(&site_a, &hold_a.id, now)
            .await,
    );
    assert_not_found(
        account_b
            .release_ticket_hold(&site_a, &hold_a.id, now)
            .await,
    );
    assert_not_found(account_b.ticket_availability(&site_a, &event_a, now).await);
    assert!(
        account_b
            .site_ticket_hold(&site_a, &hold_a.id, now)
            .await
            .unwrap()
            .is_none()
    );

    // And the same-tenant, wrong-site wall: B's own site holds none of A's
    // records; A's event is not reachable through another site either.
    assert!(
        account_a
            .site_ticket_event(&site_b, &event_a)
            .await
            .unwrap()
            .is_none()
    );
    assert_not_found(
        account_a
            .take_ticket_hold(&site_b, &event_a, 1, TTL, now)
            .await,
    );

    // A's own view is untouched by all of it.
    let availability = account_a
        .ticket_availability(&site_a, &event_a, now)
        .await
        .unwrap();
    assert_eq!(availability.held, 2);
    assert_eq!(availability.remaining, 8);
}

#[tokio::test]
async fn two_simultaneous_buyers_cannot_oversell_the_last_seat() {
    let (account, site, product) = venue("race-last").await;
    let now = clock();
    let event = account
        .create_site_ticket_event(&site, &product, now + Duration::days(7), 1)
        .await
        .unwrap();

    // The race this module exists for: both buyers in flight at once, each on
    // its own connection and transaction. The advisory lock serializes them;
    // exactly one may win whatever the interleaving.
    let (first, second) = tokio::join!(
        account.take_ticket_hold(&site, &event, 1, TTL, now),
        account.take_ticket_hold(&site, &event, 1, TTL, now),
    );
    let wins = [&first, &second]
        .iter()
        .filter(|result| result.is_ok())
        .count();
    assert_eq!(wins, 1, "exactly one buyer may get the last seat");
    let said = [first, second]
        .into_iter()
        .find_map(Result::err)
        .map(|error| format!("{error}"))
        .unwrap();
    assert!(said.contains("sold out"), "{said}");

    let availability = account
        .ticket_availability(&site, &event, now)
        .await
        .unwrap();
    assert_eq!(availability.remaining, 0);
    assert_eq!(availability.held, 1);
}

#[tokio::test]
async fn a_crowd_of_buyers_gets_exactly_the_capacity_and_no_more() {
    let (account, site, product) = venue("race-crowd").await;
    let now = clock();
    let event = account
        .create_site_ticket_event(&site, &product, now + Duration::days(7), 3)
        .await
        .unwrap();

    let mut buyers = Vec::new();
    for _ in 0..8 {
        let account = account.clone();
        let site = site.clone();
        let event = event.clone();
        buyers.push(tokio::spawn(async move {
            account.take_ticket_hold(&site, &event, 1, TTL, now).await
        }));
    }
    let mut wins = 0;
    for buyer in buyers {
        if buyer.await.unwrap().is_ok() {
            wins += 1;
        }
    }
    assert_eq!(wins, 3, "eight buyers, three seats, three holds");

    let availability = account
        .ticket_availability(&site, &event, now)
        .await
        .unwrap();
    assert_eq!(availability.held, 3);
    assert_eq!(availability.remaining, 0);
}

#[tokio::test]
async fn an_abandoned_hold_frees_its_seats_by_time_passing_alone() {
    let (account, site, product) = venue("expiry").await;
    let now = clock();
    let event = account
        .create_site_ticket_event(&site, &product, now + Duration::days(7), 1)
        .await
        .unwrap();
    let hold = account
        .take_ticket_hold(&site, &event, 1, TTL, now)
        .await
        .unwrap();

    // While the hold lives, the seat is gone.
    let said = conflict_of(account.take_ticket_hold(&site, &event, 1, TTL, now).await);
    assert!(said.contains("sold out"), "{said}");

    // One second past expiry — no sweeper ran, nothing touched the row — the
    // seat is free and the stale hold reads as expired.
    let later = now + TTL + Duration::seconds(1);
    let availability = account
        .ticket_availability(&site, &event, later)
        .await
        .unwrap();
    assert_eq!(availability.remaining, 1);
    let stale = account
        .site_ticket_hold(&site, &hold.id, later)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stale.state, SiteTicketHoldState::Expired);
    account
        .take_ticket_hold(&site, &event, 1, TTL, later)
        .await
        .unwrap();

    // And the lapsed buyer cannot complete into the reissued seat.
    let said = conflict_of(account.complete_ticket_hold(&site, &hold.id, later).await);
    assert!(said.contains("expired"), "{said}");
}

#[tokio::test]
async fn completion_keeps_the_seats_forever_and_retries_are_harmless() {
    let (account, site, product) = venue("complete").await;
    let now = clock();
    let event = account
        .create_site_ticket_event(&site, &product, now + Duration::days(7), 5)
        .await
        .unwrap();
    let hold = account
        .take_ticket_hold(&site, &event, 2, TTL, now)
        .await
        .unwrap();

    let completed = account
        .complete_ticket_hold(&site, &hold.id, now)
        .await
        .unwrap();
    assert_eq!(completed.state, SiteTicketHoldState::Completed);

    // A retried payment webhook is a no-op, not an error.
    let again = account
        .complete_ticket_hold(&site, &hold.id, now)
        .await
        .unwrap();
    assert_eq!(again.state, SiteTicketHoldState::Completed);

    // Sold seats never lapse: far past any ttl they still count.
    let much_later = now + Duration::days(1);
    let availability = account
        .ticket_availability(&site, &event, much_later)
        .await
        .unwrap();
    assert_eq!(availability.sold, 2);
    assert_eq!(availability.remaining, 3);

    // And a sale cannot be released from here — refunds are a later wave.
    let said = conflict_of(
        account
            .release_ticket_hold(&site, &hold.id, much_later)
            .await,
    );
    assert!(said.contains("complete"), "{said}");
}

#[tokio::test]
async fn a_released_hold_frees_its_seats_at_once_and_releasing_twice_is_fine() {
    let (account, site, product) = venue("release").await;
    let now = clock();
    let event = account
        .create_site_ticket_event(&site, &product, now + Duration::days(7), 1)
        .await
        .unwrap();
    let hold = account
        .take_ticket_hold(&site, &event, 1, TTL, now)
        .await
        .unwrap();

    let released = account
        .release_ticket_hold(&site, &hold.id, now)
        .await
        .unwrap();
    assert_eq!(released.state, SiteTicketHoldState::Released);
    let availability = account
        .ticket_availability(&site, &event, now)
        .await
        .unwrap();
    assert_eq!(availability.remaining, 1);

    // The cancel button pressed twice.
    let again = account
        .release_ticket_hold(&site, &hold.id, now)
        .await
        .unwrap();
    assert_eq!(again.state, SiteTicketHoldState::Released);

    // A released hold is spent: it cannot become a sale.
    let said = conflict_of(account.complete_ticket_hold(&site, &hold.id, now).await);
    assert!(said.contains("released"), "{said}");
}

#[tokio::test]
async fn a_partial_basket_is_told_how_many_seats_are_left() {
    let (account, site, product) = venue("partial").await;
    let now = clock();
    let event = account
        .create_site_ticket_event(&site, &product, now + Duration::days(7), 5)
        .await
        .unwrap();
    account
        .take_ticket_hold(&site, &event, 4, TTL, now)
        .await
        .unwrap();

    let said = conflict_of(account.take_ticket_hold(&site, &event, 2, TTL, now).await);
    assert!(said.contains("only 1 seat is left"), "{said}");
}

#[tokio::test]
async fn an_event_that_has_started_sells_nothing() {
    let (account, site, product) = venue("started").await;
    let now = clock();
    let event = account
        .create_site_ticket_event(&site, &product, now + Duration::hours(1), 5)
        .await
        .unwrap();

    let said = conflict_of(
        account
            .take_ticket_hold(&site, &event, 1, TTL, now + Duration::hours(1))
            .await,
    );
    assert!(said.contains("started"), "{said}");
}

#[tokio::test]
async fn capacity_can_grow_freely_but_never_shrink_below_committed_seats() {
    let (account, site, product) = venue("shrink").await;
    let now = clock();
    let event = account
        .create_site_ticket_event(&site, &product, now + Duration::days(7), 5)
        .await
        .unwrap();
    let sold = account
        .take_ticket_hold(&site, &event, 2, TTL, now)
        .await
        .unwrap();
    account
        .complete_ticket_hold(&site, &sold.id, now)
        .await
        .unwrap();
    account
        .take_ticket_hold(&site, &event, 1, TTL, now)
        .await
        .unwrap();

    // Three committed (two sold, one live hold): 2 is too small, 3 is exact.
    let said = conflict_of(
        account
            .set_site_ticket_capacity(&site, &event, 2, now)
            .await,
    );
    assert!(said.contains("3 seats"), "{said}");
    account
        .set_site_ticket_capacity(&site, &event, 3, now)
        .await
        .unwrap();
    account
        .set_site_ticket_capacity(&site, &event, 500, now)
        .await
        .unwrap();
    let availability = account
        .ticket_availability(&site, &event, now)
        .await
        .unwrap();
    assert_eq!(availability.capacity, 500);
    assert_eq!(availability.remaining, 497);
}

#[tokio::test]
async fn an_event_with_sales_cannot_be_deleted_and_one_without_goes_cleanly() {
    let (account, site, product) = venue("delete").await;
    let now = clock();
    let starts = now + Duration::days(7);

    let sold_out = account
        .create_site_ticket_event(&site, &product, starts, 5)
        .await
        .unwrap();
    let hold = account
        .take_ticket_hold(&site, &sold_out, 1, TTL, now)
        .await
        .unwrap();
    account
        .complete_ticket_hold(&site, &hold.id, now)
        .await
        .unwrap();
    let said = conflict_of(account.delete_site_ticket_event(&site, &sold_out).await);
    assert!(said.contains("sold"), "{said}");

    let unsold = account
        .create_site_ticket_event(&site, &product, starts, 5)
        .await
        .unwrap();
    let abandoned = account
        .take_ticket_hold(&site, &unsold, 1, TTL, now)
        .await
        .unwrap();
    account
        .delete_site_ticket_event(&site, &unsold)
        .await
        .unwrap();
    assert!(
        account
            .site_ticket_event(&site, &unsold)
            .await
            .unwrap()
            .is_none()
    );
    // The hold went with its event.
    assert!(
        account
            .site_ticket_hold(&site, &abandoned.id, now)
            .await
            .unwrap()
            .is_none()
    );
    assert_not_found(account.delete_site_ticket_event(&site, &unsold).await);
}

#[tokio::test]
async fn a_site_stops_at_its_event_ceiling() {
    let (account, site, product) = venue("ceiling").await;
    let now = clock();
    let starts = now + Duration::days(7);
    for _ in 0..SITE_TICKET_EVENT_MAX_PER_SITE {
        account
            .create_site_ticket_event(&site, &product, starts, 10)
            .await
            .unwrap();
    }
    let said = conflict_of(
        account
            .create_site_ticket_event(&site, &product, starts, 10)
            .await,
    );
    assert!(
        said.contains(&SITE_TICKET_EVENT_MAX_PER_SITE.to_string()),
        "{said}"
    );
}

#[tokio::test]
async fn the_hold_table_stores_no_buyer_identity_by_construction() {
    // Make sure migrations have run, then read the schema itself: every
    // column of site_ticket_holds, from the live database. A buyer's name,
    // address, token or note has no column to land in, whatever any calling
    // code does.
    let _ = common::test_store().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::database_url())
        .await
        .unwrap();
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_name = 'site_ticket_holds' ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        columns,
        vec![
            "completed_at",
            "created_at",
            "event_id",
            "expires_at",
            "id",
            "quantity",
            "site_id",
            "state",
            "tenant_id",
        ]
    );
}

/// One ticket event id is one lock key: two events sell independently even
/// mid-race.
#[tokio::test]
async fn two_events_do_not_contend_for_each_others_seats() {
    let (account, site, product) = venue("independent").await;
    let now = clock();
    let one = account
        .create_site_ticket_event(&site, &product, now + Duration::days(7), 1)
        .await
        .unwrap();
    let two = account
        .create_site_ticket_event(&site, &product, now + Duration::days(8), 1)
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        account.take_ticket_hold(&site, &one, 1, TTL, now),
        account.take_ticket_hold(&site, &two, 1, TTL, now),
    );
    a.unwrap();
    b.unwrap();
}

/// The owner's screen counts seats with one query per site (S3.04f3): sold
/// and live-held per event, expired holds counting nothing, and the walls —
/// a foreign tenant or the wrong site is answered an empty list, never a
/// neighbour's arithmetic.
#[tokio::test]
async fn seat_counts_tally_per_event_and_stop_at_the_walls() {
    let store = common::test_store().await;
    let (account, site, product) = venue("counts").await;
    let now = clock();
    let selling = account
        .create_site_ticket_event(&site, &product, now + Duration::days(7), 10)
        .await
        .unwrap();
    let quiet = account
        .create_site_ticket_event(&site, &product, now + Duration::days(8), 5)
        .await
        .unwrap();

    // Two seats sold, three mid-checkout, one hold long expired.
    let bought = account
        .take_ticket_hold(&site, &selling, 2, TTL, now)
        .await
        .unwrap();
    account
        .complete_ticket_hold(&site, &bought.id, now)
        .await
        .unwrap();
    account
        .take_ticket_hold(&site, &selling, 3, TTL, now)
        .await
        .unwrap();
    account
        .take_ticket_hold(&site, &selling, 4, TICKET_HOLD_MIN_TTL, now)
        .await
        .unwrap();
    let after_expiry = now + TICKET_HOLD_MIN_TTL + Duration::seconds(1);

    let counts = account
        .site_ticket_seat_counts(&site, after_expiry)
        .await
        .unwrap();
    let of = |event: &alo_store::SiteTicketEventId| {
        counts
            .iter()
            .find(|count| &count.event == event)
            .expect("event missing from the tally")
    };
    assert_eq!((of(&selling).sold, of(&selling).held), (2, 3));
    assert_eq!((of(&quiet).sold, of(&quiet).held), (0, 0));

    // The walls: another tenant holding the real site id, and the same
    // tenant asking through the wrong site, both hear "no events" — not zero
    // rows of somebody else's sales.
    let tenant_b = store.create_tenant("tickets-counts-b").await.unwrap();
    let user_b = store
        .for_tenant(tenant_b.clone())
        .create_user("owner@counts-b.test")
        .await
        .unwrap();
    let account_b = store.for_account(tenant_b, user_b);
    assert!(
        account_b
            .site_ticket_seat_counts(&site, after_expiry)
            .await
            .unwrap()
            .is_empty()
    );
    let site_b = account_b
        .create_site("Venue B", &subdomain("counts-b"))
        .await
        .unwrap();
    assert!(
        account
            .site_ticket_seat_counts(&site_b, after_expiry)
            .await
            .unwrap()
            .is_empty()
    );
}

/// The event dialog's price list is the seam's answer *now*: the tenant's own
/// items with their prices and the list currency, an archived item gone at
/// the next read, and never a neighbour's list.
#[tokio::test]
async fn sale_items_are_the_tenants_own_price_list_read_live() {
    let store = common::test_store().await;
    let (account, _site, product) = venue("items").await;

    let (currency, items) = account.site_ticket_sale_items().await.unwrap();
    assert_eq!(currency, "EUR");
    let workshop = items
        .iter()
        .find(|item| item.id == product)
        .expect("the workshop is on the list");
    assert_eq!(workshop.name, "Letterpress workshop");
    assert_eq!(workshop.unit_price_cents, 8_500);
    assert_eq!(workshop.vat_rate_bp, 2100);

    // Archiving is visible at the very next read — nothing was copied.
    account
        .set_billing_product_archived(&product, true)
        .await
        .unwrap();
    let (_, items) = account.site_ticket_sale_items().await.unwrap();
    assert!(items.iter().all(|item| item.id != product));

    // A second tenant reads its own (empty) list, not tenant A's.
    let tenant_b = store.create_tenant("tickets-items-b").await.unwrap();
    let user_b = store
        .for_tenant(tenant_b.clone())
        .create_user("owner@items-b.test")
        .await
        .unwrap();
    let account_b = store.for_account(tenant_b, user_b);
    let (_, items_b) = account_b.site_ticket_sale_items().await.unwrap();
    assert!(items_b.is_empty());
}
