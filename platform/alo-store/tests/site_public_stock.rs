//! The public stock checkout against a real database (ADR 0041, item
//! S3.05a2): what an anonymous visitor is offered, the purchase they start,
//! and the two honest failures — money after the hold lapsed, and goods the
//! warehouse took first — closing the order visibly instead of overselling.
//!
//! The hold arithmetic and the buyer race are Inventory's own suite
//! (`inv_stock_sale`); this one proves the doors around it: the offer priced
//! by the catalog seam and counted by the ledger at every read, the typo
//! gate that costs no hold, the arc a visitor actually walks (offer →
//! checkout → hosted payment → webhook target → settle → movement → invoice
//! → CRM), the tenant and site walls on every verb, and the
//! columns-of-the-table proof that no card data can live in alo.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::inv_locations::{Location, LocationKind, LocationSeed};
use alo_store::inv_moves::{MoveReason, NewMove};
use alo_store::{
    AccountStore, BillingProductId, DealFilter, FixtureSitePayments, InvLocationId,
    NewBillingSettings, NewProduct, PipelineSeed, PublishedSite, STOCK_CHECKOUT_HOLD_TTL,
    STOCK_ORDER_GOODS_GONE, STOCK_ORDER_PAID_AFTER_LAPSE, ShipTo, SiteId, SitePaymentProvider,
    SitePaymentRequest, SitePaymentStatus, SitePublicStore, SiteShopItemId, SiteStockOrderState,
    StageSeed, StockFulfilWords, Store, StoreError,
};
use sqlx::postgres::PgPoolOptions;
use time::{Duration, OffsetDateTime};

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

fn clock() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

fn seed_names() -> LocationSeed {
    LocationSeed {
        stock: "Hoofdmagazijn".to_owned(),
        supplier: "Leveranciers".to_owned(),
        customer: "Klanten".to_owned(),
        adjustment: "Correcties".to_owned(),
        production: "Productie".to_owned(),
    }
}

/// A stock item the way wave two sells one: a book with a shelf count,
/// 2 400 cents a piece at the reduced 6 % rate.
fn book() -> NewProduct {
    NewProduct {
        name: "Field guide".to_owned(),
        unit: "piece".to_owned(),
        unit_price_cents: 2_400,
        vat_rate_bp: 600,
        stocked: true,
        purchase_price_cents: 900,
        ..Default::default()
    }
}

fn ship_to() -> ShipTo {
    ShipTo {
        line: "Keizersgracht 1".to_owned(),
        city: "Amsterdam".to_owned(),
        postcode: "1015 CS".to_owned(),
        country: "NL".to_owned(),
    }
}

/// A buyer no other run of the shared database can have seen — CRM and
/// Billing dedup by address, and this suite must own its own facts.
fn buyer(tag: &str) -> String {
    format!(
        "maud@{tag}-{}.example",
        SiteId::generate().as_str().to_ascii_lowercase()
    )
}

/// The site's flat delivery price every test charges: € 5.95.
const SHIPPING: i64 = 595;

/// One tenant with a published site selling a stocked book, `shelf` units
/// received, and the public door an anonymous visitor arrives through.
struct Shop {
    store: Store,
    account: AccountStore,
    site: SiteId,
    product: BillingProductId,
    item: SiteShopItemId,
    main: InvLocationId,
    supplier: InvLocationId,
    customer: InvLocationId,
    public: SitePublicStore,
    resolved: PublishedSite,
    pool: sqlx::PgPool,
    now: OffsetDateTime,
}

impl Shop {
    async fn open(tag: &str, shelf: i64) -> Self {
        let (store, blobs) = common::test_store_with_blobs().await;
        let tenant = store
            .create_tenant(&format!("stock-shop-{tag}"))
            .await
            .unwrap();
        let user = store
            .for_tenant(tenant.clone())
            .create_user(&format!("owner@{tag}.test"))
            .await
            .unwrap();
        let account = store.for_account(tenant, user);
        common::seed_default_chart(&account).await;
        let seeded = account
            .inv_locations_or_seed(&seed_names(), false)
            .await
            .unwrap();
        let of = |kind: LocationKind| -> InvLocationId {
            seeded
                .iter()
                .find(|l: &&Location| l.kind == kind)
                .unwrap_or_else(|| panic!("the seed must write a {kind:?} location"))
                .id
                .clone()
        };
        let main = of(LocationKind::Stock);
        let supplier = of(LocationKind::Supplier);
        let customer = of(LocationKind::Customer);
        let product = account.create_billing_product(&book()).await.unwrap();
        if shelf > 0 {
            account
                .record_move(&NewMove {
                    product_id: product.clone(),
                    from_location_id: supplier.clone(),
                    to_location_id: main.clone(),
                    qty_milli: shelf * 1_000,
                    reason: MoveReason::Purchase,
                    reason_code: None,
                    note: String::new(),
                    reference: None,
                    occurred_at: None,
                })
                .await
                .unwrap();
        }
        let site_subdomain = subdomain(tag);
        let site = account.create_site("Shop", &site_subdomain).await.unwrap();
        account
            .create_site_page(&site, "Home", "", true)
            .await
            .unwrap();
        account.publish_site(&site).await.unwrap();
        let now = clock();
        let item = account
            .add_site_shop_item(&site, &product, now)
            .await
            .unwrap();
        account
            .set_site_shop_shipping_cents(&site, SHIPPING)
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
        Shop {
            store,
            account,
            site,
            product,
            item,
            main,
            supplier,
            customer,
            public,
            resolved,
            pool,
            now,
        }
    }

    /// Milli-units of the product at one location, straight from the ledger.
    async fn at(&self, location: &InvLocationId) -> i64 {
        sqlx::query_scalar(
            "SELECT COALESCE(SUM(qty_milli), 0)::bigint FROM inv_stock \
              WHERE tenant_id = $1 AND product_id = $2 AND location_id = $3",
        )
        .bind(self.account.tenant().as_str())
        .bind(self.product.as_str())
        .bind(location.as_str())
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    /// Stock-sale holds this tenant has, of any state.
    async fn holds(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM inv_stock_sale_holds WHERE tenant_id = $1")
            .bind(self.account.tenant().as_str())
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn begin(
        &self,
        units: i64,
        buyer_email: &str,
    ) -> Result<Option<alo_store::PublicStockCheckout>, StoreError> {
        self.public
            .public_begin_stock_checkout(
                &self.resolved,
                self.item.as_str(),
                units,
                "Maud Adams",
                buyer_email,
                &ship_to(),
                self.now,
            )
            .await
    }
}

/// The provider request the checkout route will build from a
/// [`alo_store::PublicStockCheckout`].
fn payment_request(key: &str, amount_cents: i64, description: &str) -> SitePaymentRequest {
    SitePaymentRequest {
        idempotency_key: key.to_owned(),
        amount_cents,
        currency: "EUR".to_owned(),
        description: description.to_owned(),
        redirect_url: "https://shop.alosites.com/shop/thanks".to_owned(),
        webhook_url: "https://shop.alosites.com/_alo/pay".to_owned(),
    }
}

fn fulfil_words() -> StockFulfilWords {
    StockFulfilWords {
        unit: "piece",
        fallback_item: "Shop item",
        shipping: "Shipping",
        payment_method: "Hosted checkout",
        crm_title: "Shop sale",
    }
}

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

#[tokio::test]
async fn the_offer_is_the_owning_seams_answer_now() {
    let s = Shop::open("offer", 10).await;

    // The listing prices from the catalog and counts from the ledger.
    let items = s
        .public
        .public_stock_items(&s.resolved, s.now)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    let offered = &items[0];
    assert_eq!(offered.id, s.item);
    assert_eq!(offered.name, "Field guide");
    assert_eq!(offered.unit, "piece");
    assert_eq!(offered.unit_price_cents, 2_400);
    assert_eq!(offered.currency, "EUR");
    assert_eq!(offered.available_units, 10);
    assert_eq!(
        s.public
            .public_stock_shipping_cents(&s.resolved)
            .await
            .unwrap(),
        SHIPPING
    );

    // A second product of the tenant, stocked and priced but never listed,
    // is not offered: the site's shelf is the site's own naming.
    let other = s
        .account
        .create_billing_product(&NewProduct {
            name: "City atlas".to_owned(),
            ..book()
        })
        .await
        .unwrap();
    let items = s
        .public
        .public_stock_items(&s.resolved, s.now)
        .await
        .unwrap();
    assert_eq!(items.len(), 1, "an unlisted product is not offered");

    // A price change on the list reprices the offer with no shop write.
    s.account
        .update_billing_product(
            &s.product,
            &NewProduct {
                unit_price_cents: 2_900,
                ..book()
            },
        )
        .await
        .unwrap();
    let repriced = s
        .public
        .public_stock_item(&s.resolved, s.item.as_str(), s.now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(repriced.unit_price_cents, 2_900);

    // Archiving the product takes the offer down — a shop cannot sell the
    // past — and the listing registry itself refuses what the seams refuse.
    s.account
        .set_billing_product_archived(&s.product, true)
        .await
        .unwrap();
    assert!(
        s.public
            .public_stock_item(&s.resolved, s.item.as_str(), s.now)
            .await
            .unwrap()
            .is_none()
    );
    let service = s
        .account
        .create_billing_product(&NewProduct {
            name: "Gift wrapping".to_owned(),
            stocked: false,
            ..book()
        })
        .await
        .unwrap();
    let said = validation_of(s.account.add_site_shop_item(&s.site, &service, s.now).await);
    assert!(said.contains("stocked"), "said: {said}");
    let said = validation_of(
        s.account
            .add_site_shop_item(&s.site, &s.product, s.now)
            .await,
    );
    assert!(said.contains("price list"), "an archived product: {said}");
    let twice = s.account.add_site_shop_item(&s.site, &other, s.now).await;
    twice.unwrap();
    let said = conflict_of(s.account.add_site_shop_item(&s.site, &other, s.now).await);
    assert!(said.contains("already"), "said: {said}");

    // Shipping keeps its bounds, and only for the tenant's own site.
    let said = validation_of(s.account.set_site_shop_shipping_cents(&s.site, -1).await);
    assert!(said.contains("shipping"), "said: {said}");
    assert!(
        s.account
            .set_site_shop_shipping_cents(&s.site, 100_001)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_typo_costs_no_hold_and_the_goods_speak_for_themselves() {
    let s = Shop::open("gates", 3).await;

    let bad_address = ShipTo {
        country: "NLD".to_owned(),
        ..ship_to()
    };
    for (units, name, email, to) in [
        (1, "Maud Adams", "not-an-address", ship_to()),
        (1, "   ", "maud@example.org", ship_to()),
        (0, "Maud Adams", "maud@example.org", ship_to()),
        (21, "Maud Adams", "maud@example.org", ship_to()),
        (1, "Maud Adams", "maud@example.org", bad_address),
    ] {
        let refused = s
            .public
            .public_begin_stock_checkout(
                &s.resolved,
                s.item.as_str(),
                units,
                name,
                email,
                &to,
                s.now,
            )
            .await;
        assert!(
            matches!(refused, Err(StoreError::Validation(_))),
            "units={units} name={name:?} email={email:?} was accepted: {refused:?}"
        );
    }
    assert_eq!(s.holds().await, 0, "a typo took a hold");

    // The goods speak for themselves: reserve all three, and the next buyer
    // is told "sold out" in the seam's own words.
    let checkout = s.begin(3, &buyer("gates")).await.unwrap().unwrap();
    assert_eq!(checkout.units, 3);
    let said = conflict_of(s.begin(1, &buyer("gates2")).await);
    assert!(said.contains("sold out"), "said: {said}");
}

#[tokio::test]
async fn the_arc_from_offer_to_invoice() {
    let s = Shop::open("arc", 10).await;
    let provider = FixtureSitePayments::new();
    s.account
        .save_billing_settings(&NewBillingSettings {
            legal_name: "Field Guides BV".to_owned(),
            country: "NL".to_owned(),
            ..NewBillingSettings::default()
        })
        .await
        .unwrap();
    let buyer_email = buyer("arc");

    let checkout = s.begin(2, &buyer_email).await.unwrap().unwrap();
    assert_eq!(checkout.units, 2);
    assert_eq!(checkout.amount_cents, 2 * 2_400 + SHIPPING);
    assert_eq!(checkout.shipping_cents, SHIPPING);
    assert_eq!(checkout.currency, "EUR");
    assert_eq!(checkout.description, "2 × Field guide");
    assert_eq!(checkout.expires_at, s.now + STOCK_CHECKOUT_HOLD_TTL);

    // The goods are reserved from this instant, and nothing has moved.
    let offer = s
        .public
        .public_stock_item(&s.resolved, s.item.as_str(), s.now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(offer.available_units, 8);
    assert_eq!(s.at(&s.main).await, 10_000, "a hold moves nothing");

    // A double-clicked buy button reaches the order it already made; a
    // different buyer on the same goods is refused, never quietly replaced.
    let order = s
        .account
        .site_stock_order(&s.site, &checkout.order)
        .await
        .unwrap()
        .unwrap();
    let replay = s
        .account
        .create_stock_order(
            &s.site,
            &order.hold,
            "Maud Adams",
            &buyer_email,
            &ship_to(),
            s.now,
        )
        .await
        .unwrap();
    assert_eq!(replay.id, checkout.order);
    let said = conflict_of(
        s.account
            .create_stock_order(
                &s.site,
                &order.hold,
                "Someone Else",
                &buyer("arc-else"),
                &ship_to(),
                s.now,
            )
            .await,
    );
    assert!(said.contains("different buyer"), "said: {said}");

    // The hosted handoff, exactly as the shop route will drive it.
    let created = provider
        .create_payment(payment_request(
            checkout.order.as_str(),
            checkout.amount_cents,
            &checkout.description,
        ))
        .await
        .unwrap();
    s.public
        .public_open_stock_payment(
            &s.resolved,
            &checkout.order,
            &created.provider_payment_id,
            &created.checkout_url,
        )
        .await
        .unwrap()
        .expect("the order is this site's");
    let waiting = s
        .public
        .public_stock_order(&s.resolved, checkout.order.as_str())
        .await
        .unwrap()
        .expect("the return page can see its order");
    assert_eq!(waiting.state, SiteStockOrderState::AwaitingPayment);
    assert_eq!(
        waiting.checkout_url.as_deref(),
        Some(created.checkout_url.as_str())
    );
    assert_eq!(waiting.shipping_cents, SHIPPING);
    assert!(waiting.failure.is_none());

    // The buyer pays; the webhook rings; the status is fetched and applied —
    // and "paid" is a movement: the shelf drops, the customer counterparty
    // rises, through the ledger's own writer.
    provider
        .mark(&created.provider_payment_id, SitePaymentStatus::Paid)
        .unwrap();
    let target = s
        .public
        .public_stock_payment_target(&created.provider_payment_id)
        .await
        .unwrap()
        .expect("the payment names an order");
    assert_eq!(target.order, checkout.order);
    assert_eq!(target.site, s.site);
    let status = provider
        .payment_status(created.provider_payment_id.clone())
        .await
        .unwrap();
    s.public
        .public_settle_stock_payment(&target, status, s.now)
        .await
        .unwrap();
    // The webhook replayed is one sale and one movement.
    s.public
        .public_settle_stock_payment(&target, SitePaymentStatus::Paid, s.now)
        .await
        .unwrap();
    assert_eq!(s.at(&s.main).await, 8_000);
    assert_eq!(s.at(&s.customer).await, 2_000);

    let paid = s
        .public
        .public_stock_order(&s.resolved, checkout.order.as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(paid.state, SiteStockOrderState::Paid);
    assert!(
        paid.checkout_url.is_none(),
        "a settled order offers no checkout link"
    );
    // The movement carries the order id as its note — the sale's own
    // reference in the tenant's ledger.
    let noted: i64 =
        sqlx::query_scalar("SELECT count(*) FROM inv_moves WHERE tenant_id = $1 AND note = $2")
            .bind(s.account.tenant().as_str())
            .bind(checkout.order.as_str())
            .fetch_one(&s.pool)
            .await
            .unwrap();
    assert_eq!(noted, 1);

    // Fulfilment puts the sale on paper. All claim calls of this suite live
    // in THIS test, so no concurrent claimer can steal the watched row.
    let claims = s.store.claim_stock_fulfilments(500).await.unwrap();
    let claim = claims
        .iter()
        .find(|claim| claim.order == checkout.order)
        .expect("the paid order is offered to the sweep");
    assert_eq!(claim.units, 2);
    assert_eq!(claim.amount_cents, 5_395);
    assert_eq!(claim.shipping_cents, SHIPPING);
    assert_eq!(claim.vat_rate_bp, 600);
    assert_eq!(claim.ship_to_country, "NL");
    assert_eq!(claim.buyer_email, buyer_email);
    let outcome = s
        .store
        .fulfil_claimed_stock(claim, &fulfil_words(), &crm_seed())
        .await
        .unwrap();
    assert!(outcome.invoiced);
    assert!(outcome.lead_raised);
    // Claimed once: a second sweep finds nothing to do for this order.
    let again = s.store.claim_stock_fulfilments(500).await.unwrap();
    assert!(again.iter().all(|claim| claim.order != checkout.order));

    // Billing's document: issued, referencing the order, delivery on its own
    // line at the goods' rate, worth no more than the buyer paid (VAT carved
    // out of the consumer price, never added on top), settled by the
    // recorded payment.
    let invoices = s.account.billing_invoices(None).await.unwrap();
    let summary = invoices
        .iter()
        .find(|summary| summary.invoice.reference == checkout.order.as_str())
        .expect("the sale has an invoice");
    assert!(summary.invoice.number.is_some());
    assert!(summary.totals.gross_cents <= 5_395);
    assert!(summary.totals.gross_cents >= 5_393);
    assert!(summary.totals.vat_cents > 0);
    assert_eq!(summary.paid_cents, 5_395);
    let lines: Vec<(String, i64, i32)> = sqlx::query_as(
        "SELECT description, unit_price_cents, vat_rate_bp FROM billing_invoice_lines \
          WHERE tenant_id = $1 AND invoice_id = $2 ORDER BY line_order",
    )
    .bind(s.account.tenant().as_str())
    .bind(summary.invoice.id.as_str())
    .fetch_all(&s.pool)
    .await
    .unwrap();
    assert_eq!(lines.len(), 2, "goods and delivery, each its own line");
    assert_eq!(lines[0].0, "2 × Field guide");
    assert_eq!(lines[1].0, "Shipping");
    assert_eq!(lines[1].2, 600, "delivery follows the goods' rate");

    // CRM's card: one lead, titled by the caller, sourced from the site.
    let deals = s.account.crm_deals(&DealFilter::default()).await.unwrap();
    assert_eq!(deals.len(), 1);
    assert_eq!(deals[0].title, "Shop sale — Shop");
}

#[tokio::test]
async fn money_after_the_hold_lapsed_fails_visibly_and_moves_nothing() {
    let s = Shop::open("lapse", 5).await;
    let provider = FixtureSitePayments::new();
    let checkout = s.begin(2, &buyer("lapse")).await.unwrap().unwrap();
    let created = provider
        .create_payment(payment_request(
            checkout.order.as_str(),
            5_395,
            "2 × Field guide",
        ))
        .await
        .unwrap();
    s.public
        .public_open_stock_payment(
            &s.resolved,
            &checkout.order,
            &created.provider_payment_id,
            &created.checkout_url,
        )
        .await
        .unwrap()
        .unwrap();

    // The payment lands half a minute after the hold's half hour lapsed.
    let late = s.now + STOCK_CHECKOUT_HOLD_TTL + Duration::seconds(30);
    let target = s
        .public
        .public_stock_payment_target(&created.provider_payment_id)
        .await
        .unwrap()
        .unwrap();
    s.public
        .public_settle_stock_payment(&target, SitePaymentStatus::Paid, late)
        .await
        .unwrap();

    let failed = s
        .public
        .public_stock_order(&s.resolved, checkout.order.as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed.state, SiteStockOrderState::Failed);
    assert_eq!(
        failed.failure.as_deref(),
        Some(STOCK_ORDER_PAID_AFTER_LAPSE)
    );
    // Nothing moved, and the lapsed hold no longer counts: all five sell.
    assert_eq!(s.at(&s.main).await, 5_000);
    assert_eq!(s.at(&s.customer).await, 0);
    let offer = s
        .public
        .public_stock_item(&s.resolved, s.item.as_str(), late)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(offer.available_units, 5);
}

#[tokio::test]
async fn goods_the_warehouse_took_first_fail_the_sale_visibly() {
    let s = Shop::open("gone", 10).await;
    let provider = FixtureSitePayments::new();
    let checkout = s.begin(2, &buyer("gone")).await.unwrap().unwrap();
    let created = provider
        .create_payment(payment_request(
            checkout.order.as_str(),
            5_395,
            "2 × Field guide",
        ))
        .await
        .unwrap();
    s.public
        .public_open_stock_payment(
            &s.resolved,
            &checkout.order,
            &created.provider_payment_id,
            &created.checkout_url,
        )
        .await
        .unwrap()
        .unwrap();

    // The warehouse's own doors take nine of the ten while the buyer is on
    // the payment page — holds bind the shop, never Inventory (S3.05a1).
    s.account
        .record_move(&NewMove {
            product_id: s.product.clone(),
            from_location_id: s.main.clone(),
            to_location_id: s.supplier.clone(),
            qty_milli: 9_000,
            reason: MoveReason::ReturnOut,
            reason_code: None,
            note: String::new(),
            reference: None,
            occurred_at: None,
        })
        .await
        .unwrap();

    let target = s
        .public
        .public_stock_payment_target(&created.provider_payment_id)
        .await
        .unwrap()
        .unwrap();
    s.public
        .public_settle_stock_payment(&target, SitePaymentStatus::Paid, s.now)
        .await
        .unwrap();

    // The honest path: the order fails naming the refund, no movement is
    // recorded, and the one unit that remains goes back on open sale
    // (the hold was released with the sale).
    let failed = s
        .public
        .public_stock_order(&s.resolved, checkout.order.as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed.state, SiteStockOrderState::Failed);
    assert_eq!(failed.failure.as_deref(), Some(STOCK_ORDER_GOODS_GONE));
    assert_eq!(s.at(&s.customer).await, 0, "no goods moved to the customer");
    let offer = s
        .public
        .public_stock_item(&s.resolved, s.item.as_str(), s.now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(offer.available_units, 1);
    // A failed order is never a claimable sale.
    let claims = s.store.claim_stock_fulfilments(500).await.unwrap();
    assert!(claims.iter().all(|claim| claim.order != checkout.order));
}

#[tokio::test]
async fn a_dead_payment_frees_the_goods() {
    let s = Shop::open("dead", 4).await;
    let provider = FixtureSitePayments::new();
    let checkout = s.begin(3, &buyer("dead")).await.unwrap().unwrap();
    let created = provider
        .create_payment(payment_request(
            checkout.order.as_str(),
            7_795,
            "3 × Field guide",
        ))
        .await
        .unwrap();
    s.public
        .public_open_stock_payment(
            &s.resolved,
            &checkout.order,
            &created.provider_payment_id,
            &created.checkout_url,
        )
        .await
        .unwrap()
        .unwrap();
    let offer = s
        .public
        .public_stock_item(&s.resolved, s.item.as_str(), s.now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(offer.available_units, 1);

    let target = s
        .public
        .public_stock_payment_target(&created.provider_payment_id)
        .await
        .unwrap()
        .unwrap();
    s.public
        .public_settle_stock_payment(&target, SitePaymentStatus::Canceled, s.now)
        .await
        .unwrap();
    let cancelled = s
        .public
        .public_stock_order(&s.resolved, checkout.order.as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cancelled.state, SiteStockOrderState::Cancelled);
    assert!(cancelled.checkout_url.is_none());
    let offer = s
        .public
        .public_stock_item(&s.resolved, s.item.as_str(), s.now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(offer.available_units, 4, "the goods went back on sale");
}

#[tokio::test]
async fn the_tenant_and_site_walls_hold_on_every_verb() {
    let s = Shop::open("wall-a", 5).await;
    let other = Shop::open("wall-b", 5).await;

    // Site B's host cannot see, price, or buy site A's listing: one uniform
    // absence, indistinguishable from an id that never existed.
    assert!(
        other
            .public
            .public_stock_item(&other.resolved, s.item.as_str(), s.now)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        other
            .public
            .public_begin_stock_checkout(
                &other.resolved,
                s.item.as_str(),
                1,
                "Maud Adams",
                "maud@example.org",
                &ship_to(),
                s.now,
            )
            .await
            .unwrap()
            .is_none()
    );

    // An order is visible only on the site it was placed on.
    let checkout = s.begin(1, &buyer("wall")).await.unwrap().unwrap();
    assert!(
        other
            .public
            .public_stock_order(&other.resolved, checkout.order.as_str())
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        s.public
            .public_stock_order(&s.resolved, checkout.order.as_str())
            .await
            .unwrap()
            .is_some()
    );

    // Owner doors: a foreign tenant cannot list A's product, read A's
    // orders, or set A's shipping.
    let said = validation_of(
        other
            .account
            .add_site_shop_item(&other.site, &s.product, s.now)
            .await,
    );
    assert!(said.contains("price list"), "said: {said}");
    assert!(
        other
            .account
            .site_stock_order(&s.site, &checkout.order)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        other
            .account
            .site_stock_orders(&s.site)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        other
            .account
            .set_site_shop_shipping_cents(&s.site, 100)
            .await,
        Err(StoreError::NotFound)
    ));
    // And the malformed id never reaches the database at all.
    assert!(
        s.public
            .public_stock_item(&s.resolved, "two words", s.now)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn the_order_table_has_no_room_for_a_card() {
    // Make sure migrations have run, then read the schema itself.
    let _ = common::test_store().await;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::database_url())
        .await
        .unwrap();
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
          WHERE table_name = 'site_stock_orders' ORDER BY column_name",
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
            "failure",
            "hold_id",
            "id",
            "paid_at",
            "product_id",
            "provider_payment_id",
            "ship_to_city",
            "ship_to_country",
            "ship_to_line",
            "ship_to_postcode",
            "shipping_cents",
            "site_id",
            "state",
            "tenant_id",
            "unit_price_cents",
            "units",
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
