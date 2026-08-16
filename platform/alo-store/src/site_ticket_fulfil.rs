//! Fulfilment of paid ticket orders (ADR 0041, item S3.04d): the sweep that
//! makes a sale good — the ticket the buyer can hold, the invoice in Billing
//! and the contact in CRM, each written through the owning module's own door.
//!
//! The order ([`crate::site_ticket_orders`]) records the sale; nothing about
//! it says the buyer ever received anything. This module is the bridge: a
//! paid order with no fulfilment row is one nobody has made good yet, and
//! [`Store::claim_ticket_fulfilments`] claims such orders **by inserting the
//! fulfilment row itself** — the row is the claim, so two concurrent sweeps
//! meet a unique index, not a double fulfilment. The claim also mints the
//! ticket token: the capability the buyer holds, served by the public ticket
//! page ([`crate::site_public_tickets`]) the way a booking's manage token is.
//!
//! What fulfilment writes, and through whose door:
//!
//! - **The invoice is Billing's document, raised by Billing's own writers**
//!   ([`crate::billing_invoices`], [`crate::billing_customers`],
//!   [`crate::billing_payments`]) through the owner's account door — created,
//!   lined with the order's own struck price, issued, and the hosted payment
//!   recorded against it, so the document is born settled. No billing file
//!   knows tickets exist.
//! - **The contact is CRM's card, raised by CRM's own lead seam**
//!   ([`crate::crm_lead_capture`]) — and CRM's duplicate rules stand: a buyer
//!   the tenant already knows is an answer, never a second card.
//! - The item's name on the invoice line is read from **Billing's catalog
//!   seam** at fulfilment time; a product retired between sale and sweep gets
//!   the caller's fallback word rather than a resurrected list entry.
//!
//! Claiming is **at-most-once**, exactly like the notification sweeps: a
//! crash between claim and act leaves a fulfilment row whose invoice columns
//! stay empty — visible in the row itself, never a duplicate invoice or a
//! double lead. The order stays in the owner's order list either way, so
//! nothing is silently lost.
//!
//! What is deliberately absent: **the ticket email.** alo has no
//! outbound-mail-to-strangers path — even a purchase order leaves as a draft
//! a human sends through the one audited submission path (ADR 0034) — so the
//! ticket travels on the checkout return page and in the buyer's own
//! calendar (the public ticket page's `.ics`), and the email is its own
//! queue item behind its own ADR. Nothing that reaches a log here carries a
//! buyer's name or address: only ids and coarse errors (Law 1).

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
    BillingProductId, SiteId, SiteTicketEventId, SiteTicketFulfilmentId, SiteTicketOrderId,
    TenantId, UserId, generate_token,
};
use crate::store::Store;

/// The longest CRM card title fulfilment will compose — CRM's own
/// `DEAL_TITLE_MAX_CHARS`, mirrored so a maximal site name cannot make a
/// legitimate capture fail CRM's title rule.
const CRM_TITLE_MAX_CHARS: usize = 200;

/// Everything the sweep needs to make one paid order good, resolved in the
/// claim itself so the act never re-reads what the claim already proved.
#[derive(Debug, Clone)]
pub struct ClaimedTicketFulfilment {
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
    /// invoice unit and the CRM seed in.
    pub default_locale: String,
    /// The fulfilment row the claim inserted.
    pub fulfilment: SiteTicketFulfilmentId,
    /// The ticket the buyer holds: the public page's capability.
    pub token: String,
    pub order: SiteTicketOrderId,
    pub event: SiteTicketEventId,
    /// Billing's id for what was sold — resolved to a name through the
    /// catalog seam at act time.
    pub product: BillingProductId,
    /// When the event happens — the `.ics` the buyer imports.
    pub starts_at: OffsetDateTime,
    pub quantity: i32,
    pub buyer_name: String,
    pub buyer_email: String,
    /// The sale as the order struck it: integer cents, VAT in basis points,
    /// the tenant's accounting currency at the moment of sale.
    pub unit_price_cents: i64,
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
pub struct TicketFulfilWords {
    /// The invoice line's unit label ("ticket").
    pub unit: &'static str,
    /// The line description when the product has left the price list.
    pub fallback_item: &'static str,
    /// How the money arrived, printed on the payment record.
    pub payment_method: &'static str,
    /// The CRM card's title prefix ("Ticket sale").
    pub crm_title: &'static str,
}

/// What one fulfilment act came to — for the sweep's log line, nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TicketFulfilmentOutcome {
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
    event_id: String,
    product_id: String,
    starts_at: OffsetDateTime,
    quantity: i32,
    buyer_name: String,
    buyer_email: String,
    unit_price_cents: i64,
    amount_cents: i64,
    vat_rate_bp: i32,
    currency: String,
    paid_at: OffsetDateTime,
    payment_reference: String,
}

impl ClaimRow {
    fn into_claim(
        self,
        fulfilment: SiteTicketFulfilmentId,
        token: String,
    ) -> ClaimedTicketFulfilment {
        ClaimedTicketFulfilment {
            tenant: TenantId::new(self.tenant_id),
            owner: UserId::new(self.owner),
            site: SiteId::new(self.site_id),
            site_name: self.site_name,
            site_subdomain: self.site_subdomain,
            default_locale: self.default_locale,
            fulfilment,
            token,
            order: SiteTicketOrderId::new(self.order_id),
            event: SiteTicketEventId::new(self.event_id),
            product: BillingProductId::new(self.product_id),
            starts_at: self.starts_at,
            quantity: self.quantity,
            buyer_name: self.buyer_name,
            buyer_email: self.buyer_email,
            unit_price_cents: self.unit_price_cents,
            amount_cents: self.amount_cents,
            vat_rate_bp: self.vat_rate_bp,
            currency: self.currency,
            paid_at: self.paid_at,
            payment_reference: self.payment_reference,
        }
    }
}

impl Store {
    /// Claims up to `limit` paid, unfulfilled ticket orders by inserting
    /// their fulfilment rows — token minted, at-most-once — and returns
    /// everything the act needs.
    ///
    /// Two sweeps cannot claim the same order: candidates are locked
    /// (`FOR UPDATE SKIP LOCKED`) and the row's one-per-order unique index
    /// is the backstop. A claim that then crashes leaves the row with empty
    /// invoice columns — visible, never a duplicate.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn claim_ticket_fulfilments(
        &self,
        limit: i64,
    ) -> Result<Vec<ClaimedTicketFulfilment>> {
        let mut tx = self.pool().begin().await.map_err(StoreError::Db)?;
        let rows = sqlx::query_as::<_, ClaimRow>(
            "SELECT o.tenant_id, s.created_by AS owner, o.site_id, s.name AS site_name, \
                    s.subdomain AS site_subdomain, s.default_locale, \
                    o.id AS order_id, o.event_id, e.product_id, e.starts_at, \
                    o.quantity, o.buyer_name, o.buyer_email, o.unit_price_cents, \
                    o.amount_cents, o.vat_rate_bp, o.currency, \
                    COALESCE(o.paid_at, now()) AS paid_at, \
                    COALESCE(o.provider_payment_id, '') AS payment_reference \
               FROM site_ticket_orders o \
               JOIN sites s ON s.tenant_id = o.tenant_id AND s.id = o.site_id \
               JOIN site_ticket_events e ON e.tenant_id = o.tenant_id AND e.id = o.event_id \
               LEFT JOIN site_ticket_fulfilments f \
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
            let id = SiteTicketFulfilmentId::generate();
            let token = generate_token();
            let inserted = sqlx::query(
                "INSERT INTO site_ticket_fulfilments \
                     (id, tenant_id, site_id, order_id, event_id, token) \
                 VALUES ($1, $2, $3, $4, $5, $6) \
                 ON CONFLICT ON CONSTRAINT site_ticket_fulfilments_one_per_order DO NOTHING",
            )
            .bind(id.as_str())
            .bind(&row.tenant_id)
            .bind(&row.site_id)
            .bind(&row.order_id)
            .bind(&row.event_id)
            .bind(&token)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
            if inserted.rows_affected() == 1 {
                claims.push(row.into_claim(id, token));
            }
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(claims)
    }

    /// Makes one claimed sale good: the invoice in Billing, the contact in
    /// CRM, and the record of both on the fulfilment row. Each act goes
    /// through the owning module's own door under the site owner's account;
    /// an act that fails is written down (`invoice_note`, `crm_outcome`) and
    /// never repeated — the claim was the once.
    ///
    /// # Errors
    /// [`StoreError::Db`] when the record itself cannot be written; a failed
    /// invoice or capture is an outcome, not an error.
    pub async fn fulfil_claimed_ticket(
        &self,
        claim: &ClaimedTicketFulfilment,
        words: &TicketFulfilWords,
        seed: &PipelineSeed,
    ) -> Result<TicketFulfilmentOutcome> {
        let description = self.resolve_description(claim, words).await;
        // CRM first, deliberately: the invoice below makes this buyer a
        // billing customer, and CRM's duplicate rule reads customers — a
        // capture that ran second would answer "already a customer" for the
        // very sale that made them one, and no first-time buyer would ever
        // reach the board.
        let crm_outcome = self.capture_buyer(claim, words, seed).await;
        let (invoice, note) = match self.raise_invoice(claim, words, &description).await {
            Ok(raised) => (Some(raised), None),
            Err(error) => {
                tracing::warn!(
                    order = claim.order.as_str(),
                    %error,
                    "ticket fulfilment: invoice not raised"
                );
                (None, Some(error.to_string()))
            }
        };

        sqlx::query(
            "UPDATE site_ticket_fulfilments \
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

        Ok(TicketFulfilmentOutcome {
            invoiced: invoice.is_some(),
            lead_raised: crm_outcome.as_deref() == Some(CRM_OUTCOME_LEAD),
        })
    }

    /// The invoice line's words: the price list's current name for the
    /// product and the event's date — or the caller's fallback when the
    /// product has left the list since the sale.
    async fn resolve_description(
        &self,
        claim: &ClaimedTicketFulfilment,
        words: &TicketFulfilWords,
    ) -> String {
        let catalog = BillingCatalogRead::open(
            self.pool().clone(),
            self.blobs().clone(),
            claim.tenant.clone(),
            claim.owner.clone(),
        );
        let name = match catalog.sale_item(&claim.product).await {
            Ok(Some(item)) => item.name,
            Ok(None) => words.fallback_item.to_owned(),
            Err(error) => {
                tracing::warn!(
                    order = claim.order.as_str(),
                    %error,
                    "ticket fulfilment: price list unreadable"
                );
                words.fallback_item.to_owned()
            }
        };
        format!("{name} — {}", claim.starts_at.date())
    }

    /// Raises and settles Billing's document for the sale, through Billing's
    /// own writers: find or create the customer, create the draft, line it
    /// with the struck price, issue it, and record the hosted payment against
    /// it — so the document is born settled, showing exactly what the buyer
    /// paid.
    async fn raise_invoice(
        &self,
        claim: &ClaimedTicketFulfilment,
        words: &TicketFulfilWords,
        description: &str,
    ) -> Result<(crate::id::BillingInvoiceId, String)> {
        let account = self.for_account(claim.tenant.clone(), claim.owner.clone());
        let seller = account.billing_settings().await?;
        if seller.country.is_empty() {
            // An invoice needs a place of supply; a seller profile without a
            // country cannot state one. Written down for the tenant, never
            // guessed (S3.04e owns the real rules table).
            return Err(StoreError::Validation(
                "the billing seller profile has no country; add it under Billing settings to \
                 invoice ticket sales"
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
                        // A ticket buyer states no address; the admission is
                        // supplied where the seller runs the event, so the
                        // seller's own country carries the VAT treatment the
                        // order already snapshotted (flagged for S3.04e).
                        country: seller.country.clone(),
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
        // The buyer was charged `amount_cents`, full stop — a price shown to
        // a consumer is VAT-inclusive, so the document carves the VAT OUT of
        // what arrived rather than adding it on top (which would invoice
        // money nobody was charged). One line carries the exact total as its
        // net (the seat count travels in its words): a per-seat net cannot
        // survive Billing's at-the-subtotal rounding for every quantity,
        // and the money must be exact. S3.04e owns the real rules table;
        // this is the strict reading until it lands, flagged in STATE.
        let net = net_within(claim.amount_cents, claim.vat_rate_bp);
        account
            .set_billing_invoice_lines(
                &invoice_id,
                &[NewLine {
                    description: format!("{} × {description}", claim.quantity),
                    unit: words.unit.to_owned(),
                    qty_milli: 1000,
                    unit_price_cents: net,
                    vat_rate_bp: claim.vat_rate_bp,
                }],
            )
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
    async fn capture_buyer(
        &self,
        claim: &ClaimedTicketFulfilment,
        words: &TicketFulfilWords,
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
                    "ticket fulfilment: lead not captured"
                );
                Some("failed".to_owned())
            }
        }
    }
}

/// The recorded outcome that means CRM raised a card.
const CRM_OUTCOME_LEAD: &str = "lead";

/// The largest net whose gross — computed exactly as Billing computes it,
/// VAT rounded half away from zero at the subtotal — does not exceed the
/// amount the buyer was charged. Never more than the charge: the recorded
/// payment must settle the document, and a stray cent of overpayment is
/// honest where a cent of phantom debt is not. A zero rate carves nothing:
/// the net is the charge itself.
fn net_within(amount_cents: i64, vat_rate_bp: i32) -> i64 {
    let rate = i128::from(vat_rate_bp.max(0));
    let amount = i128::from(amount_cents.max(0));
    let gross_of = |net: i128| net + (net * rate + 5_000) / 10_000;
    let mut net = amount * 10_000 / (10_000 + rate);
    while gross_of(net + 1) <= amount {
        net += 1;
    }
    while net > 0 && gross_of(net) > amount {
        net -= 1;
    }
    i64::try_from(net).unwrap_or(0)
}

/// The CRM card's title: the caller's word and the site's name, held to
/// CRM's own title bound so a maximal site name cannot fail the capture.
fn crm_title(prefix: &str, site_name: &str) -> String {
    let title = format!("{prefix} — {site_name}");
    if title.chars().count() <= CRM_TITLE_MAX_CHARS {
        return title;
    }
    title.chars().take(CRM_TITLE_MAX_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crm_title_is_the_prefix_and_the_site() {
        assert_eq!(crm_title("Ticket sale", "Studio"), "Ticket sale — Studio");
    }

    #[test]
    fn a_maximal_site_name_is_held_to_crms_title_bound() {
        let long = "s".repeat(400);
        let title = crm_title("Ticket sale", &long);
        assert_eq!(title.chars().count(), CRM_TITLE_MAX_CHARS);
        assert!(title.starts_with("Ticket sale — "));
    }

    /// Billing's own arithmetic for one line of quantity 1: net plus the VAT
    /// subtotal, rounded half away from zero.
    fn billed_gross(net: i64, rate_bp: i32) -> i64 {
        net + ((i128::from(net) * i128::from(rate_bp) + 5_000) / 10_000) as i64
    }

    #[test]
    fn the_carved_net_never_bills_more_than_the_buyer_paid_and_never_leaves_a_whole_cent() {
        for (amount, rate) in [
            (17_000, 2100),
            (8_500, 2100),
            (1, 2100),
            (0, 2100),
            (9_999, 600),
            (123_456_789, 2500),
        ] {
            let net = net_within(amount, rate);
            let gross = billed_gross(net, rate);
            assert!(
                gross <= amount,
                "amount {amount} rate {rate}: gross {gross}"
            );
            // The next cent of net would overshoot — nothing tighter exists.
            assert!(
                billed_gross(net + 1, rate) > amount,
                "amount {amount} rate {rate}: net {net} is not maximal"
            );
        }
    }

    #[test]
    fn a_zero_rate_carves_nothing() {
        assert_eq!(net_within(17_000, 0), 17_000);
    }
}
