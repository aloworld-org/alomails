//! The won-deal handoff (alo CRM, ADR 0035, wave B2.08) — turning an
//! opportunity into a **draft** quote or invoice that billing owns from that
//! moment on.
//!
//! This is the seam between the two modules, and it is deliberately one-way and
//! one-shot. It reads a deal, makes sure there is a customer to bill, and
//! raises a draft document ([`crate::billing_quotes`],
//! [`crate::billing_invoices`]) with a single line carrying the deal's value.
//! It never issues anything, never sends anything, and never touches a document
//! it did not just create — a draft is a starting point a human edits, which is
//! the same rule the agent tools and the quote→invoice copy (B1.12) hold.
//!
//! Three decisions shape the file:
//!
//! - **A lead becomes a customer, and the deal is linked to it.** A deal that
//!   already names a `billing_customers` row bills that row; one that does not
//!   creates it from the deal's own `company_name` / `contact_name` /
//!   `contact_email` — the fields that are shaped like a customer's for exactly
//!   this reason (`docs/design/crm.md` § The customer, the lead and the
//!   contact) — and writes it back onto the deal, so raising a second document
//!   bills the same company rather than a twin of it.
//! - **The VAT rate is stated by the caller, never guessed.** A deal carries one
//!   number, and an invoice line needs a rate; picking one on the tenant's
//!   behalf would be a compliance statement made by a machine. A deal worth
//!   something therefore demands a rate, and a deal worth nothing raises an
//!   empty draft.
//! - **A lost deal raises nothing.** Quoting an *open* deal is ordinary sales —
//!   the quote is how it is won — so the rule here is only that a deal somebody
//!   has recorded as lost is not a thing to invoice for.
//!
//! Money is integer cents from the deal to the line, unrounded and unconverted:
//! the document is raised in the deal's own currency, whatever the customer's
//! default is, because that is the currency the opportunity was priced in.

use crate::account::AccountStore;
use crate::billing_customers::NewCustomer;
use crate::billing_field::{UNIT_PRICE_MAX_CENTS, country};
use crate::billing_invoices::NewInvoice;
use crate::billing_line::NewLine;
use crate::billing_quotes::NewQuote;
use crate::crm_deals::{Deal, DealState};
use crate::error::{Result, StoreError};
use crate::id::{BillingCustomerId, BillingInvoiceId, BillingQuoteId, CrmDealId};

/// What a caller has to add to a deal to make it a billing document.
#[derive(Debug, Clone, Default)]
pub struct DealHandoff {
    /// The VAT rate, in basis points, for the one line raised from the deal's
    /// value. Required when the deal is worth anything; ignored when it is
    /// worth nothing, because then there is no line to rate.
    pub vat_rate_bp: Option<i32>,
    /// ISO 3166-1 alpha-2 country of the customer created from a lead — the
    /// one fact a customer needs that a deal does not carry, and the one that
    /// decides VAT treatment. Ignored when the deal already names a customer.
    pub country: String,
}

/// The quantity of the single line a deal's value becomes: one whole unit, in
/// the milli-units [`crate::billing_line`] counts in.
const ONE_UNIT_MILLI: i64 = 1_000;

impl AccountStore {
    /// Raises a **draft quote** for a deal and answers its id.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the deal is not this tenant's;
    /// [`StoreError::Validation`] when the deal is lost, is worth more than one
    /// line may carry, is worth something and no VAT rate was stated, or has no
    /// customer and nothing to create one from; [`StoreError::Db`] on failure.
    pub async fn crm_deal_quote(
        &self,
        id: &CrmDealId,
        handoff: &DealHandoff,
    ) -> Result<BillingQuoteId> {
        let deal = self.deal_to_bill(id).await?;
        let lines = deal_lines(&deal, handoff)?;
        let customer_id = self.customer_to_bill(&deal, handoff).await?;
        let quote = self
            .create_billing_quote(&NewQuote {
                currency: Some(deal.currency.clone()),
                ..NewQuote::for_customer(customer_id)
            })
            .await?;
        // Written as a second call because the lines are validated as a set;
        // a refusal here leaves an empty draft rather than a wrong one, and an
        // empty draft is a document a human can finish or delete.
        // A deal becomes one line **in words**, never a catalog item: what a
        // deal names is a piece of work, and inventing a product for it would
        // put an item nobody chose onto an offer. So this offer still routes to
        // an invoice on acceptance, exactly as it did before ADR 0054 §5.
        let quote_lines: Vec<crate::billing_quote_lines::NewQuoteLine> = lines
            .into_iter()
            .map(|line| crate::billing_quote_lines::NewQuoteLine {
                product_id: None,
                line,
            })
            .collect();
        self.set_billing_quote_lines(&quote, &quote_lines).await?;
        Ok(quote)
    }

    /// Raises a **draft invoice** for a deal and answers its id.
    ///
    /// The invoice is a draft like any other: it carries no number, consumes
    /// nothing from the gapless sequence, and is issued — if it ever is — by
    /// the same route every other invoice is.
    ///
    /// # Errors
    /// As [`AccountStore::crm_deal_quote`].
    pub async fn crm_deal_invoice(
        &self,
        id: &CrmDealId,
        handoff: &DealHandoff,
    ) -> Result<BillingInvoiceId> {
        let deal = self.deal_to_bill(id).await?;
        let lines = deal_lines(&deal, handoff)?;
        let customer_id = self.customer_to_bill(&deal, handoff).await?;
        let invoice = self
            .create_billing_invoice(&NewInvoice {
                currency: Some(deal.currency.clone()),
                ..NewInvoice::for_customer(customer_id)
            })
            .await?;
        self.set_billing_invoice_lines(&invoice, &lines).await?;
        Ok(invoice)
    }

    /// The deal a document is being raised from: this tenant's, and not one
    /// somebody has already recorded as lost.
    async fn deal_to_bill(&self, id: &CrmDealId) -> Result<Deal> {
        let deal = self.crm_deal(id).await?.ok_or(StoreError::NotFound)?;
        if deal.state() == DealState::Lost {
            return Err(StoreError::Validation(
                "this deal was lost; reopen it before raising a document for it".to_owned(),
            ));
        }
        Ok(deal)
    }

    /// The customer to bill: the one the deal names, or a new one created from
    /// the lead and linked back onto the deal.
    ///
    /// The link is written **only while the deal still has none**, so two
    /// callers racing this cannot overwrite each other's customer; the loser
    /// re-reads and bills the winner's, leaving one unused customer row behind
    /// rather than two documents for two spellings of one company. That row is
    /// the price of not holding a lock across billing's own writes, and it is
    /// an ordinary customer a tenant can archive.
    async fn customer_to_bill(
        &self,
        deal: &Deal,
        handoff: &DealHandoff,
    ) -> Result<BillingCustomerId> {
        if let Some(existing) = &deal.customer_id {
            let customer = self
                .billing_customer(existing)
                .await?
                .ok_or(StoreError::NotFound)?;
            if customer.is_archived() {
                return Err(StoreError::Validation(
                    "the customer is archived; restore it before raising documents for it"
                        .to_owned(),
                ));
            }
            return Ok(customer.id);
        }
        let created = self
            .create_billing_customer(&lead_customer(deal, handoff)?)
            .await?;
        let linked = sqlx::query(
            "UPDATE crm_deals SET customer_id = $3, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND customer_id IS NULL",
        )
        .bind(self.tenant.as_str())
        .bind(deal.id.as_str())
        .bind(created.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if linked.rows_affected() == 1 {
            return Ok(created);
        }
        // Somebody linked one between our read and our write — or deleted the
        // deal. Re-read rather than bill a customer the deal does not name.
        let deal = self.crm_deal(&deal.id).await?.ok_or(StoreError::NotFound)?;
        deal.customer_id.ok_or(StoreError::NotFound)
    }
}

/// The customer a lead becomes.
///
/// Only what the deal actually knows is copied. The company name is required —
/// naming a customer after the *opportunity* ("Renewal — Acme GmbH") would put
/// a sentence where a legal name belongs on every document that follows — and
/// the address is left blank for a human to complete, which billing already
/// allows and the print view already handles.
fn lead_customer(deal: &Deal, handoff: &DealHandoff) -> Result<NewCustomer> {
    let name = deal.company_name.trim();
    if name.is_empty() {
        return Err(StoreError::Validation(
            "this deal names no company; give it one, or link it to a customer, before raising a \
             document"
                .to_owned(),
        ));
    }
    let email = deal.contact_email.trim();
    Ok(NewCustomer {
        name: name.to_owned(),
        // Validated here rather than left to billing so the refusal names the
        // field the CRM caller actually sent.
        country: country(&handoff.country)?,
        email: (!email.is_empty()).then(|| email.to_owned()),
        // The document is raised in the deal's currency, so the customer's
        // default is the same one — otherwise their next document would quietly
        // change currency.
        currency: deal.currency.clone(),
        contact_id: deal.contact_id.clone(),
        ..NewCustomer::default()
    })
}

/// The document's lines: one carrying the whole deal, or none at all.
///
/// A deal worth nothing raises an **empty** draft — a header for the right
/// customer that a human prices — rather than a line worth zero, which would be
/// a zero-rated supply nobody meant to declare.
fn deal_lines(deal: &Deal, handoff: &DealHandoff) -> Result<Vec<NewLine>> {
    if deal.value_cents == 0 {
        return Ok(Vec::new());
    }
    let vat_rate_bp = handoff.vat_rate_bp.ok_or_else(|| {
        StoreError::Validation(
            "a document raised from a deal needs the VAT rate its line is billed at".to_owned(),
        )
    })?;
    if deal.value_cents > UNIT_PRICE_MAX_CENTS {
        // The deal cap is a hundred times the line cap, so this is reachable
        // with a legitimate deal. Say which rule it broke here, where the
        // caller can act on it, instead of letting billing refuse a line the
        // caller never wrote.
        return Err(StoreError::Validation(format!(
            "this deal is worth more than one document line may carry ({UNIT_PRICE_MAX_CENTS} \
             cents); raise the document in billing and split it across lines"
        )));
    }
    Ok(vec![NewLine {
        description: deal.title.clone(),
        unit: String::new(),
        qty_milli: ONE_UNIT_MILLI,
        unit_price_cents: deal.value_cents,
        vat_rate_bp,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::ContactId;

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    /// A lead worth 2 500 EUR: no customer row yet, with a company, a contact
    /// and an address-book pointer.
    fn lead() -> Deal {
        let mut deal = Deal::blank("dea_test");
        deal.title = "Renewal — Acme GmbH".to_owned();
        deal.company_name = "Acme GmbH".to_owned();
        deal.contact_name = "Ada".to_owned();
        deal.contact_email = "ada@acme.test".to_owned();
        deal.contact_id = Some(ContactId::new("con_test"));
        deal.value_cents = 250_000;
        deal
    }

    #[test]
    fn a_priced_deal_becomes_one_line_at_the_stated_rate() {
        let lines = deal_lines(
            &lead(),
            &DealHandoff {
                vat_rate_bp: Some(2100),
                country: "DE".to_owned(),
            },
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].description, "Renewal — Acme GmbH");
        assert_eq!(lines[0].qty_milli, 1_000, "one whole unit");
        assert_eq!(lines[0].unit_price_cents, 250_000, "the deal's own cents");
        assert_eq!(lines[0].vat_rate_bp, 2100);
        assert_eq!(lines[0].unit, "", "a deal is not sold by the hour");
    }

    #[test]
    fn a_priced_deal_without_a_rate_is_refused_rather_than_rated_at_zero() {
        let refusal = invalid(deal_lines(&lead(), &DealHandoff::default()));
        assert!(refusal.contains("VAT rate"), "{refusal}");
    }

    #[test]
    fn a_deal_worth_nothing_raises_an_empty_draft_and_needs_no_rate() {
        let mut unpriced = lead();
        unpriced.value_cents = 0;
        let lines =
            deal_lines(&unpriced, &DealHandoff::default()).unwrap_or_else(|e| panic!("{e:?}"));
        assert!(lines.is_empty());
    }

    #[test]
    fn a_deal_worth_more_than_a_line_may_carry_names_the_rule() {
        let mut huge = lead();
        huge.value_cents = UNIT_PRICE_MAX_CENTS + 1;
        let refusal = invalid(deal_lines(
            &huge,
            &DealHandoff {
                vat_rate_bp: Some(2100),
                country: String::new(),
            },
        ));
        assert!(refusal.contains("split it across lines"), "{refusal}");
        // The ceiling itself still passes: the rule is "more than", not "near".
        huge.value_cents = UNIT_PRICE_MAX_CENTS;
        assert!(
            deal_lines(
                &huge,
                &DealHandoff {
                    vat_rate_bp: Some(2100),
                    country: String::new(),
                },
            )
            .is_ok()
        );
    }

    #[test]
    fn a_lead_becomes_a_customer_from_the_fields_the_deal_carries() {
        let customer = lead_customer(
            &lead(),
            &DealHandoff {
                vat_rate_bp: None,
                country: " de ".to_owned(),
            },
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(
            customer.name, "Acme GmbH",
            "the company, not the deal title"
        );
        assert_eq!(customer.country, "DE", "trimmed and upper-cased");
        assert_eq!(customer.email.as_deref(), Some("ada@acme.test"));
        assert_eq!(customer.currency, "EUR", "the deal's currency");
        assert!(customer.vat_id.is_none(), "nothing is invented");
        assert_eq!(customer.address_line1, "");
    }

    #[test]
    fn a_lead_with_no_company_name_is_refused_rather_than_named_after_the_deal() {
        let mut nameless = lead();
        nameless.company_name = "   ".to_owned();
        let refusal = invalid(lead_customer(
            &nameless,
            &DealHandoff {
                vat_rate_bp: None,
                country: "DE".to_owned(),
            },
        ));
        assert!(refusal.contains("names no company"), "{refusal}");
    }

    #[test]
    fn a_customer_created_from_a_lead_needs_a_country() {
        for bad in ["", "  ", "D", "DEU", "12"] {
            let refusal = invalid(lead_customer(
                &lead(),
                &DealHandoff {
                    vat_rate_bp: None,
                    country: bad.to_owned(),
                },
            ));
            assert!(refusal.contains("two-letter"), "{bad:?}: {refusal}");
        }
    }

    #[test]
    fn an_unknown_email_is_left_unknown_rather_than_stored_blank() {
        let mut anonymous = lead();
        anonymous.contact_email = "  ".to_owned();
        let customer = lead_customer(
            &anonymous,
            &DealHandoff {
                vat_rate_bp: None,
                country: "NL".to_owned(),
            },
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert!(customer.email.is_none());
    }
}
