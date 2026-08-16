//! Fulfilment of paid stock orders (ADR 0041, item S3.05a2): the sweep that
//! makes a goods sale good on paper — the invoice in Billing and the contact
//! in CRM, each written through the owning module's own door.
//!
//! The physical half of a stock sale is already done by the time this sweep
//! runs: settling the order claimed the hold through Inventory's seam, which
//! recorded the real outbound movement ([`crate::site_stock_orders`]). What
//! remains is the paper: a paid order with no fulfilment row is one nobody
//! has invoiced yet, and [`Store::claim_stock_fulfilments`] claims such
//! orders **by inserting the fulfilment row itself** — the row is the claim,
//! so two concurrent sweeps meet a unique index, not a double invoice.
//!
//! What fulfilment writes, and through whose door, is the ticket sweep's
//! exact shape ([`crate::site_ticket_fulfil`]): the invoice by Billing's own
//! writers, born settled, with the VAT carved OUT of what the buyer was
//! actually charged; the contact by CRM's own lead seam, duplicates
//! answered, never doubled. Two things are stock's own. The invoice shows
//! **delivery as its own line** at the goods' VAT rate — an ancillary cost
//! follows the main supply — with the two nets split so the document's total
//! still equals the charge to the cent. And the customer's country is the
//! **buyer's own** (the delivery address): goods go where the buyer is,
//! which is where a destination-based rule will look; the rate itself stays
//! the order's snapshot until the S3.04e rules table lands (flagged in
//! STATE, never guessed).
//!
//! Claiming is **at-most-once**, exactly like every sweep: a crash between
//! claim and act leaves a fulfilment row whose invoice columns stay empty —
//! visible in the row itself, never a duplicate invoice or a double lead.
//! Nothing that reaches a log here carries a buyer's name or address: only
//! ids and coarse errors (Law 1).

use time::OffsetDateTime;

use crate::billing_catalog_read::BillingCatalogRead;
use crate::billing_customers::NewCustomer;
use crate::billing_invoices::NewInvoice;
use crate::billing_line::NewLine;
use crate::billing_payments::NewPayment;
use crate::crm_lead_capture::{CapturedLead, ConversationLead, CrmLeadCapture};
use crate::crm_pipelines::PipelineSeed;
use crate::error::{Result, StoreError};
use crate::id::{
    BillingProductId, SiteId, SiteStockFulfilmentId, SiteStockOrderId, TenantId, UserId,
};
use crate::site_ticket_fulfil::{crm_title, net_within};
use crate::store::Store;

/// Everything the sweep needs to invoice one paid order, resolved in the
/// claim itself so the act never re-reads what the claim already proved.
#[derive(Debug, Clone)]
pub struct ClaimedStockFulfilment {
    /// The tenant the sale belongs to — the only tenant whose Billing and
    /// CRM this fulfilment may touch.
    pub tenant: TenantId,
    /// The site's creator: the account door the invoice and the lead are
    /// written through.
    pub owner: UserId,
    pub site: SiteId,
    pub site_name: String,
    pub site_subdomain: String,
    /// The site's default language — which words the caller resolves the
    /// invoice lines and the CRM seed in.
    pub default_locale: String,
    /// The fulfilment row the claim inserted.
    pub fulfilment: SiteStockFulfilmentId,
    pub order: SiteStockOrderId,
    /// Billing's id for what was sold — resolved to a name through the
    /// catalog seam at act time.
    pub product: BillingProductId,
    pub units: i64,
    pub buyer_name: String,
    pub buyer_email: String,
    /// Where the goods go — the country Billing's customer card is raised
    /// with.
    pub ship_to_country: String,
    /// The sale as the order struck it: integer cents, VAT in basis points,
    /// the tenant's accounting currency at the moment of sale.
    pub unit_price_cents: i64,
    pub shipping_cents: i64,
    pub amount_cents: i64,
    pub vat_rate_bp: i32,
    pub currency: String,
    pub paid_at: OffsetDateTime,
    /// The provider's opaque payment id — the payment's bank-side reference.
    pub payment_reference: String,
}

/// The caller's words, in the site's language — the store composes documents
/// from words it is handed and invents none.
#[derive(Debug, Clone, Copy)]
pub struct StockFulfilWords {
    /// The invoice line's unit label when the product has left the price
    /// list and its own unit word with it ("piece").
    pub unit: &'static str,
    /// The line description when the product has left the price list.
    pub fallback_item: &'static str,
    /// The delivery line's description ("Shipping").
    pub shipping: &'static str,
    /// How the money arrived, printed on the payment record.
    pub payment_method: &'static str,
    /// The CRM card's title prefix ("Shop sale").
    pub crm_title: &'static str,
}

/// What one fulfilment act came to — for the sweep's log line, nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StockFulfilmentOutcome {
    /// An invoice was raised and settled in Billing.
    pub invoiced: bool,
    /// CRM raised a card (`false` covers both duplicates and failures — the
    /// row's `crm_outcome` says which).
    pub lead_raised: bool,
}

#[derive(sqlx::FromRow)]
struct ClaimRow {
    tenant_id: String,
    owner: String,
    site_id: String,
    site_name: String,
    site_subdomain: String,
    default_locale: String,
    order_id: String,
    product_id: String,
    units: i64,
    buyer_name: String,
    buyer_email: String,
    ship_to_country: String,
    unit_price_cents: i64,
    shipping_cents: i64,
    amount_cents: i64,
    vat_rate_bp: i32,
    currency: String,
    paid_at: OffsetDateTime,
    payment_reference: String,
}

impl ClaimRow {
    fn into_claim(self, fulfilment: SiteStockFulfilmentId) -> ClaimedStockFulfilment {
        ClaimedStockFulfilment {
            tenant: TenantId::new(self.tenant_id),
            owner: UserId::new(self.owner),
            site: SiteId::new(self.site_id),
            site_name: self.site_name,
            site_subdomain: self.site_subdomain,
            default_locale: self.default_locale,
            fulfilment,
            order: SiteStockOrderId::new(self.order_id),
            product: BillingProductId::new(self.product_id),
            units: self.units,
            buyer_name: self.buyer_name,
            buyer_email: self.buyer_email,
            ship_to_country: self.ship_to_country,
            unit_price_cents: self.unit_price_cents,
            shipping_cents: self.shipping_cents,
            amount_cents: self.amount_cents,
            vat_rate_bp: self.vat_rate_bp,
            currency: self.currency,
            paid_at: self.paid_at,
            payment_reference: self.payment_reference,
        }
    }
}

impl Store {
    /// Claims up to `limit` paid, unfulfilled stock orders by inserting
    /// their fulfilment rows — at-most-once — and returns everything the
    /// act needs.
    ///
    /// Two sweeps cannot claim the same order: candidates are locked
    /// (`FOR UPDATE SKIP LOCKED`) and the row's one-per-order unique index
    /// is the backstop. A claim that then crashes leaves the row with empty
    /// invoice columns — visible, never a duplicate.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn claim_stock_fulfilments(&self, limit: i64) -> Result<Vec<ClaimedStockFulfilment>> {
        let mut tx = self.pool().begin().await.map_err(StoreError::Db)?;
        let rows = sqlx::query_as::<_, ClaimRow>(
            "SELECT o.tenant_id, s.created_by AS owner, o.site_id, s.name AS site_name, \
                    s.subdomain AS site_subdomain, s.default_locale, \
                    o.id AS order_id, o.product_id, o.units, \
                    o.buyer_name, o.buyer_email, o.ship_to_country, \
                    o.unit_price_cents, o.shipping_cents, o.amount_cents, \
                    o.vat_rate_bp, o.currency, \
                    COALESCE(o.paid_at, now()) AS paid_at, \
                    COALESCE(o.provider_payment_id, '') AS payment_reference \
               FROM site_stock_orders o \
               JOIN sites s ON s.tenant_id = o.tenant_id AND s.id = o.site_id \
               LEFT JOIN site_stock_fulfilments f \
                 ON f.tenant_id = o.tenant_id AND f.order_id = o.id \
              WHERE o.state = 'paid' AND f.id IS NULL \
              ORDER BY o.paid_at, o.id \
              LIMIT $1 \
                FOR UPDATE OF o SKIP LOCKED",
        )
        .bind(limit)
        .fetch_all(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        let mut claims = Vec::with_capacity(rows.len());
        for row in rows {
            let id = SiteStockFulfilmentId::generate();
            let inserted = sqlx::query(
                "INSERT INTO site_stock_fulfilments (id, tenant_id, site_id, order_id) \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT ON CONSTRAINT site_stock_fulfilments_one_per_order DO NOTHING",
            )
            .bind(id.as_str())
            .bind(&row.tenant_id)
            .bind(&row.site_id)
            .bind(&row.order_id)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
            if inserted.rows_affected() == 1 {
                claims.push(row.into_claim(id));
            }
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(claims)
    }

    /// Makes one claimed sale good on paper: the invoice in Billing, the
    /// contact in CRM, and the record of both on the fulfilment row. Each
    /// act goes through the owning module's own door under the site owner's
    /// account; an act that fails is written down (`invoice_note`,
    /// `crm_outcome`) and never repeated — the claim was the once.
    ///
    /// # Errors
    /// [`StoreError::Db`] when the record itself cannot be written; a failed
    /// invoice or capture is an outcome, not an error.
    pub async fn fulfil_claimed_stock(
        &self,
        claim: &ClaimedStockFulfilment,
        words: &StockFulfilWords,
        seed: &PipelineSeed,
    ) -> Result<StockFulfilmentOutcome> {
        let (description, unit) = self.resolve_stock_description(claim, words).await;
        // CRM first, deliberately: the invoice below makes this buyer a
        // billing customer, and CRM's duplicate rule reads customers — a
        // capture that ran second would answer "already a customer" for the
        // very sale that made them one.
        let crm_outcome = self.capture_stock_buyer(claim, words, seed).await;
        let (invoice, note) = match self
            .raise_stock_invoice(claim, words, &description, &unit)
            .await
        {
            Ok(raised) => (Some(raised), None),
            Err(error) => {
                tracing::warn!(
                    order = claim.order.as_str(),
                    %error,
                    "stock fulfilment: invoice not raised"
                );
                (None, Some(error.to_string()))
            }
        };

        sqlx::query(
            "UPDATE site_stock_fulfilments \
                SET description = $3, invoice_id = $4, invoice_number = $5, \
                    invoice_note = $6, crm_outcome = $7, updated_at = now() \
              WHERE tenant_id = $1 AND id = $2",
        )
        .bind(claim.tenant.as_str())
        .bind(claim.fulfilment.as_str())
        .bind(&description)
        .bind(invoice.as_ref().map(|(id, _)| id.as_str().to_owned()))
        .bind(invoice.as_ref().map(|(_, number)| number.clone()))
        .bind(&note)
        .bind(&crm_outcome)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;

        Ok(StockFulfilmentOutcome {
            invoiced: invoice.is_some(),
            lead_raised: crm_outcome.as_deref() == Some(CRM_OUTCOME_LEAD),
        })
    }

    /// The invoice line's words: the price list's current name and unit for
    /// the product — or the caller's fallbacks when the product has left the
    /// list since the sale.
    async fn resolve_stock_description(
        &self,
        claim: &ClaimedStockFulfilment,
        words: &StockFulfilWords,
    ) -> (String, String) {
        let catalog = BillingCatalogRead::open(
            self.pool().clone(),
            self.blobs().clone(),
            claim.tenant.clone(),
            claim.owner.clone(),
        );
        match catalog.sale_item(&claim.product).await {
            Ok(Some(item)) => {
                let unit = if item.unit.is_empty() {
                    words.unit.to_owned()
                } else {
                    item.unit
                };
                (item.name, unit)
            }
            Ok(None) => (words.fallback_item.to_owned(), words.unit.to_owned()),
            Err(error) => {
                tracing::warn!(
                    order = claim.order.as_str(),
                    %error,
                    "stock fulfilment: price list unreadable"
                );
                (words.fallback_item.to_owned(), words.unit.to_owned())
            }
        }
    }

    /// Raises and settles Billing's document for the sale, through Billing's
    /// own writers — born settled, showing exactly what the buyer paid, with
    /// delivery as its own line at the goods' rate. The two nets are split
    /// from one carve so the billed total equals the charge to the cent:
    /// `net(goods+shipping)` is the document's net, and the shipping line
    /// takes `net(shipping)` of it (see [`net_within`] — with one shared VAT
    /// rate, Billing's at-the-subtotal rounding makes the sum exact).
    async fn raise_stock_invoice(
        &self,
        claim: &ClaimedStockFulfilment,
        words: &StockFulfilWords,
        description: &str,
        unit: &str,
    ) -> Result<(crate::id::BillingInvoiceId, String)> {
        let account = self.for_account(claim.tenant.clone(), claim.owner.clone());
        let seller = account.billing_settings().await?;
        if seller.country.is_empty() {
            // An invoice needs a place of supply; a seller profile without a
            // country cannot state one. Written down for the tenant, never
            // guessed (S3.04e owns the real rules table).
            return Err(StoreError::Validation(
                "the billing seller profile has no country; add it under Billing settings to \
                 invoice shop sales"
                    .to_owned(),
            ));
        }

        let buyer_email_lower = claim.buyer_email.to_ascii_lowercase();
        let existing = account
            .billing_customers(false)
            .await?
            .into_iter()
            .find(|customer| {
                customer
                    .email
                    .as_deref()
                    .is_some_and(|address| address.to_ascii_lowercase() == buyer_email_lower)
            })
            .map(|customer| customer.id);
        let customer_id = match existing {
            Some(id) => id,
            None => {
                account
                    .create_billing_customer(&NewCustomer {
                        name: claim.buyer_name.clone(),
                        // Goods go where the buyer is: the delivery country
                        // is the customer's country. The VAT rate applied
                        // stays the order's snapshot — the goods' own rate —
                        // until the S3.04e rules table lands (flagged in
                        // STATE, never guessed).
                        country: claim.ship_to_country.clone(),
                        email: Some(claim.buyer_email.clone()),
                        payment_terms_days: 0,
                        currency: claim.currency.clone(),
                        ..NewCustomer::default()
                    })
                    .await?
            }
        };

        let invoice_id = account
            .create_billing_invoice(&NewInvoice {
                customer_id,
                currency: Some(claim.currency.clone()),
                payment_terms_days: Some(0),
                reference: claim.order.as_str().to_owned(),
                note: String::new(),
            })
            .await?;
        // The buyer was charged `amount_cents`, full stop — a consumer price
        // is VAT-inclusive, so the document carves the VAT OUT of what
        // arrived rather than adding it on top. One net for the whole
        // charge, split between the goods line and the delivery line; each
        // line carries its exact money as its net (the unit count travels
        // in the goods line's words), because a per-unit net cannot survive
        // Billing's at-the-subtotal rounding for every quantity, and the
        // money must be exact.
        let total_net = net_within(claim.amount_cents, claim.vat_rate_bp);
        let shipping_net = if claim.shipping_cents > 0 {
            net_within(claim.shipping_cents, claim.vat_rate_bp)
        } else {
            0
        };
        let goods_net = total_net - shipping_net;
        let mut lines = vec![NewLine {
            description: format!("{} × {description}", claim.units),
            unit: unit.to_owned(),
            qty_milli: 1000,
            unit_price_cents: goods_net,
            vat_rate_bp: claim.vat_rate_bp,
        }];
        if claim.shipping_cents > 0 {
            // Delivery follows the goods: same rate, own line.
            lines.push(NewLine {
                description: words.shipping.to_owned(),
                unit: String::new(),
                qty_milli: 1000,
                unit_price_cents: shipping_net,
                vat_rate_bp: claim.vat_rate_bp,
            });
        }
        account
            .set_billing_invoice_lines(&invoice_id, &lines)
            .await?;
        let issued = account.issue_billing_invoice(&invoice_id).await?;
        account
            .record_billing_payment(
                &invoice_id,
                &NewPayment {
                    paid_on: Some(claim.paid_at.date()),
                    amount_cents: claim.amount_cents,
                    method: words.payment_method.to_owned(),
                    reference: claim.payment_reference.clone(),
                },
            )
            .await?;
        Ok((invoice_id, issued.invoice.number.unwrap_or_default()))
    }

    /// Hands the buyer to CRM's own lead seam and writes down what CRM
    /// answered — a raised card, or the fact that made one unnecessary. A
    /// failure is an outcome too, in a coarse word rather than a person's
    /// data.
    async fn capture_stock_buyer(
        &self,
        claim: &ClaimedStockFulfilment,
        words: &StockFulfilWords,
        seed: &PipelineSeed,
    ) -> Option<String> {
        let door = CrmLeadCapture::open(
            self.pool().clone(),
            self.blobs().clone(),
            claim.tenant.clone(),
            claim.owner.clone(),
        );
        let lead = ConversationLead {
            title: crm_title(words.crm_title, &claim.site_name),
            visitor_name: claim.buyer_name.clone(),
            visitor_email: claim.buyer_email.clone(),
            company_name: String::new(),
            source: claim.site_subdomain.clone(),
        };
        match door.capture(seed, &lead).await {
            Ok(CapturedLead::Created(_)) => Some(CRM_OUTCOME_LEAD.to_owned()),
            Ok(CapturedLead::AlreadyKnown(_)) => Some("already-known".to_owned()),
            Ok(CapturedLead::AlreadyCustomer) => Some("already-customer".to_owned()),
            Err(error) => {
                tracing::warn!(
                    order = claim.order.as_str(),
                    %error,
                    "stock fulfilment: lead not captured"
                );
                Some("failed".to_owned())
            }
        }
    }
}

/// The recorded outcome that means CRM raised a card.
const CRM_OUTCOME_LEAD: &str = "lead";

#[cfg(test)]
mod tests {
    use crate::site_ticket_fulfil::net_within;

    /// Billing's own arithmetic across lines of one rate: the nets sum, then
    /// the VAT subtotal rounds half away from zero once.
    fn billed_gross(nets: &[i64], rate_bp: i32) -> i64 {
        let net: i64 = nets.iter().sum();
        net + ((i128::from(net) * i128::from(rate_bp) + 5_000) / 10_000) as i64
    }

    #[test]
    fn the_split_nets_bill_exactly_what_one_carve_would() {
        for (goods, shipping, rate) in [
            (4_800, 595, 2100),
            (2_400, 0, 600),
            (1, 1, 2100),
            (123_456, 9_900, 2500),
        ] {
            let amount = goods + shipping;
            let total_net = net_within(amount, rate);
            let shipping_net = if shipping > 0 {
                net_within(shipping, rate)
            } else {
                0
            };
            let goods_net = total_net - shipping_net;
            assert!(goods_net >= 0, "goods {goods} shipping {shipping}");
            assert_eq!(
                billed_gross(&[goods_net, shipping_net], rate),
                billed_gross(&[total_net], rate),
                "goods {goods} shipping {shipping} rate {rate}"
            );
        }
    }
}
