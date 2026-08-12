//! What the website is worth (ADR 0036, S2.10b) — the read half of the
//! Sites → CRM/Billing seam.
//!
//! It answers one question an owner cannot answer today: *of the enquiries
//! this page produced, what became business?* The counters it starts from are
//! the aggregate ones ([`crate::site_conversions`]); everything after the
//! enquiry is read live from the modules that own it — CRM's deals, Billing's
//! invoices — through their own tenant-scoped tables. Nothing is copied and
//! nothing is written: this file is a join, and a join goes stale the moment
//! it is stored.
//!
//! **The period selects the leads, not the revenue.** An invoice raised in
//! March for an enquiry that arrived in January is January's doing, so the
//! window bounds the conversion counts and the handoffs, and the deals and
//! invoices of those handoffs are then read *as they stand now*. A window that
//! also filtered the invoices would report a smaller number every time the
//! customer took longer to pay, which is the opposite of the truth.
//!
//! **The invoice join is stated, not guessed.** Billing does not record which
//! opportunity a document came from — the handoff (`crate::crm_handoff`) is
//! one-way, and adding a column to Billing's table is Billing's decision, not
//! this seam's. So an invoice counts here when it is raised **for the customer
//! the lead became**, and **after** the enquiry arrived: [`SiteAttributionSource::invoices`]
//! is that rule and nothing more. A customer the tenant was already invoicing
//! before they wrote in does not have their back catalogue credited to a form,
//! because those documents predate the link; anything they buy afterwards
//! does, and an interface must say so in those words rather than calling it
//! "revenue this page generated".
//!
//! **Money is never converted and never summed across currencies.** A deal is
//! worth what it is priced in ([`crate::crm_deals`]), a document carries the
//! currency it was raised in, and a forecast has no issue date to convert at —
//! so every figure here is reported per ISO 4217 code, in integer cents, as
//! CRM's own pipeline report does.
//!
//! Drafts and void documents are excluded: a draft is a thought and a void
//! document is a mistake, and neither is business the website brought in.

use std::collections::{BTreeMap, HashMap, HashSet};

use time::Date;

use crate::account::AccountStore;
use crate::billing_totals::{LineFigures, totals};
use crate::crm_deals::DealState;
use crate::error::{Result, StoreError};
use crate::id::SiteId;
use crate::site_conversions::SiteConversionReport;

/// One currency's worth of a conversion point's business. Codes are never
/// mixed and never converted; an interface shows each line as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteAttributionMoney {
    /// ISO 4217, uppercase.
    pub currency: String,
    /// Value of the linked opportunities still being worked, in integer cents.
    pub open_cents: i64,
    /// Value of the linked opportunities recorded as won, in integer cents.
    pub won_cents: i64,
    /// Gross total (net + VAT) of the invoices this rule attributes, in
    /// integer cents. Credit notes net off, because they are issued documents
    /// with negative lines.
    pub invoiced_cents: i64,
}

/// One conversion point, from the first page view to the money.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteAttributionSource {
    /// The stored source word — `form` today.
    pub kind: String,
    /// The site-owned id of the conversion point.
    pub id: String,
    /// The owner-facing label, or `None` for a conversion point that has since
    /// been deleted but whose counts remain.
    pub name: Option<String>,
    pub views: u64,
    pub starts: u64,
    pub submits: u64,
    /// Enquiries handed to an opportunity in the period.
    pub leads: u64,
    /// How those opportunities stand right now.
    pub deals_open: u64,
    pub deals_won: u64,
    pub deals_lost: u64,
    /// Issued or paid invoices raised for a linked lead's customer after the
    /// link was made. See the module docs: this is a stated rule, not a
    /// causal claim.
    pub invoices: u64,
    /// The figures, one line per currency, ascending by code.
    pub money: Vec<SiteAttributionMoney>,
}

/// A site's funnel from traffic to invoices over an inclusive period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteAttributionReport {
    /// Every conversion point of the site, including the ones nothing has
    /// happened on — "no one has reached this form yet" is a finding.
    pub sources: Vec<SiteAttributionSource>,
    pub views: u64,
    pub starts: u64,
    pub submits: u64,
    pub leads: u64,
    pub deals_open: u64,
    pub deals_won: u64,
    pub deals_lost: u64,
    /// Invoices attributed to the site. One document reachable from two
    /// conversion points is counted **once** here and once under each of them,
    /// so the site figure is never larger than the truth and the columns may
    /// not add up to it.
    pub invoices: u64,
    pub money: Vec<SiteAttributionMoney>,
}

/// A linked deal as the report reads it.
#[derive(sqlx::FromRow)]
struct LeadRow {
    source_kind: String,
    source_id: String,
    currency: String,
    value_cents: i64,
    state: String,
    customer_id: Option<String>,
}

/// An attributed invoice header: which conversion point reaches it, and in
/// what currency it was raised.
#[derive(sqlx::FromRow)]
struct InvoiceRow {
    source_kind: String,
    source_id: String,
    invoice_id: String,
    currency: String,
}

#[derive(sqlx::FromRow)]
struct InvoiceLineRow {
    invoice_id: String,
    qty_milli: i64,
    unit_price_cents: i64,
    vat_rate_bp: i32,
}

/// The per-currency figures of one source while they are being summed.
#[derive(Default)]
struct MoneyBuilder {
    open_cents: i64,
    won_cents: i64,
    invoiced_cents: i64,
}

/// One source's figures under construction, keyed by currency so the output is
/// ordered by code without a sort.
#[derive(Default)]
struct SourceBuilder {
    leads: u64,
    deals_open: u64,
    deals_won: u64,
    deals_lost: u64,
    invoices: HashSet<String>,
    money: BTreeMap<String, MoneyBuilder>,
}

impl SourceBuilder {
    fn count_deal(&mut self, state: DealState, currency: &str, value_cents: i64) {
        self.leads += 1;
        let money = self.money.entry(currency.to_owned()).or_default();
        match state {
            DealState::Open => {
                self.deals_open += 1;
                money.open_cents = money.open_cents.saturating_add(value_cents);
            }
            DealState::Won => {
                self.deals_won += 1;
                money.won_cents = money.won_cents.saturating_add(value_cents);
            }
            DealState::Lost => self.deals_lost += 1,
        }
    }

    fn count_invoice(&mut self, id: &str, currency: &str, gross_cents: i64) {
        // The same document can be reachable through two links of one source
        // (two enquiries, one customer). It is one invoice.
        if !self.invoices.insert(id.to_owned()) {
            return;
        }
        let money = self.money.entry(currency.to_owned()).or_default();
        money.invoiced_cents = money.invoiced_cents.saturating_add(gross_cents);
    }

    fn money(&self) -> Vec<SiteAttributionMoney> {
        self.money
            .iter()
            .map(|(currency, figures)| SiteAttributionMoney {
                currency: currency.clone(),
                open_cents: figures.open_cents,
                won_cents: figures.won_cents,
                invoiced_cents: figures.invoiced_cents,
            })
            .collect()
    }
}

impl AccountStore {
    /// Reads the site's funnel from page views to invoices over an inclusive
    /// period.
    ///
    /// A foreign or missing site answers `None` — indistinguishable, by
    /// design; an owned site with nothing on it answers a zeroed report, so an
    /// interface can tell "not yours" from "nothing yet".
    ///
    /// This is a read of CRM and Billing data through a Sites route. The
    /// caller is responsible for having established that the person asking may
    /// see it: a site editor is an outside collaborator and must not
    /// ([`crate::site_editors`]).
    ///
    /// # Errors
    /// [`StoreError::Db`] when the database cannot answer the report.
    pub async fn site_attribution(
        &self,
        site: &SiteId,
        from: Date,
        to: Date,
    ) -> Result<Option<SiteAttributionReport>> {
        // The conversion counters, the site-ownership check and the list of
        // conversion points, in one already-tested read.
        let Some(funnel) = self.site_conversions(site, from, to).await? else {
            return Ok(None);
        };

        let leads = self.attributed_leads(site, from, to).await?;
        let mut sources: HashMap<(String, String), SourceBuilder> = HashMap::new();
        for lead in &leads {
            sources
                .entry((lead.source_kind.clone(), lead.source_id.clone()))
                .or_default()
                .count_deal(
                    DealState::parse(&lead.state).unwrap_or(DealState::Open),
                    &lead.currency,
                    lead.value_cents,
                );
        }

        // Every attributed document once, whichever conversion points reach
        // it: the site's own figure is a sum over this map, so a customer two
        // enquiries brought in is not billed twice to the report.
        let mut documents: HashMap<String, (String, i64)> = HashMap::new();
        if leads.iter().any(|lead| lead.customer_id.is_some()) {
            let invoices = self.attributed_invoices(site, from, to).await?;
            let ids = invoices
                .iter()
                .map(|invoice| invoice.invoice_id.clone())
                .collect::<Vec<_>>();
            let gross = self.invoice_gross_cents(&ids).await?;
            for invoice in invoices {
                let cents = gross.get(&invoice.invoice_id).copied().unwrap_or_default();
                documents.insert(
                    invoice.invoice_id.clone(),
                    (invoice.currency.clone(), cents),
                );
                sources
                    .entry((invoice.source_kind.clone(), invoice.source_id.clone()))
                    .or_default()
                    .count_invoice(&invoice.invoice_id, &invoice.currency, cents);
            }
        }

        Ok(Some(merge(funnel, &sources, &documents)))
    }

    /// Every handoff made on the site in the period, with the deal it named as
    /// that deal stands now.
    async fn attributed_leads(&self, site: &SiteId, from: Date, to: Date) -> Result<Vec<LeadRow>> {
        sqlx::query_as::<_, LeadRow>(
            "SELECT a.source_kind, a.source_id, d.currency, d.value_cents, \
                 COALESCE(d.outcome, 'open') AS state, d.customer_id \
             FROM site_lead_attribution a \
             JOIN crm_deals d ON d.tenant_id = a.tenant_id AND d.id = a.deal_id \
             WHERE a.tenant_id = $1 AND a.site_id = $2 \
               AND a.linked_at >= $3::date AND a.linked_at < ($4::date + 1)",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)
    }

    /// The invoices the stated rule attributes: raised for a linked lead's
    /// customer, after that link was made, and actually issued.
    async fn attributed_invoices(
        &self,
        site: &SiteId,
        from: Date,
        to: Date,
    ) -> Result<Vec<InvoiceRow>> {
        sqlx::query_as::<_, InvoiceRow>(
            "SELECT DISTINCT a.source_kind, a.source_id, i.id AS invoice_id, i.currency \
             FROM site_lead_attribution a \
             JOIN crm_deals d ON d.tenant_id = a.tenant_id AND d.id = a.deal_id \
             JOIN billing_invoices i \
                 ON i.tenant_id = a.tenant_id AND i.customer_id = d.customer_id \
             WHERE a.tenant_id = $1 AND a.site_id = $2 \
               AND a.linked_at >= $3::date AND a.linked_at < ($4::date + 1) \
               AND d.customer_id IS NOT NULL \
               AND i.status IN ('issued', 'paid') \
               AND i.created_at >= a.linked_at",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)
    }

    /// What each of those documents comes to, computed from its own lines by
    /// the one function that totals a document — never a stored column, never
    /// a float.
    async fn invoice_gross_cents(&self, ids: &[String]) -> Result<HashMap<String, i64>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let lines = sqlx::query_as::<_, InvoiceLineRow>(
            "SELECT invoice_id, qty_milli, unit_price_cents, vat_rate_bp \
             FROM billing_invoice_lines \
             WHERE tenant_id = $1 AND invoice_id = ANY($2)",
        )
        .bind(self.tenant.as_str())
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let mut by_invoice: HashMap<String, Vec<LineFigures>> = HashMap::new();
        for line in lines {
            by_invoice
                .entry(line.invoice_id)
                .or_default()
                .push(LineFigures {
                    qty_milli: line.qty_milli,
                    unit_price_cents: line.unit_price_cents,
                    vat_rate_bp: line.vat_rate_bp,
                });
        }
        Ok(by_invoice
            .into_iter()
            .map(|(id, lines)| (id, totals(&lines).gross_cents))
            .collect())
    }
}

/// Lays the business figures over the traffic funnel, keeping the funnel's own
/// order: every conversion point of the site first, then any deleted one that
/// still has counts.
fn merge(
    funnel: SiteConversionReport,
    sources: &HashMap<(String, String), SourceBuilder>,
    documents: &HashMap<String, (String, i64)>,
) -> SiteAttributionReport {
    let mut report = SiteAttributionReport {
        sources: Vec::with_capacity(funnel.sources.len()),
        views: funnel.views,
        starts: funnel.starts,
        submits: funnel.submits,
        leads: 0,
        deals_open: 0,
        deals_won: 0,
        deals_lost: 0,
        invoices: 0,
        money: Vec::new(),
    };
    let mut site_money: BTreeMap<String, MoneyBuilder> = BTreeMap::new();

    for source in funnel.sources {
        let key = (source.kind.clone(), source.id.clone());
        let figures = sources.get(&key);
        let mut merged = SiteAttributionSource {
            kind: source.kind,
            id: source.id,
            name: source.name,
            views: source.views,
            starts: source.starts,
            submits: source.submits,
            leads: 0,
            deals_open: 0,
            deals_won: 0,
            deals_lost: 0,
            invoices: 0,
            money: Vec::new(),
        };
        if let Some(figures) = figures {
            merged.leads = figures.leads;
            merged.deals_open = figures.deals_open;
            merged.deals_won = figures.deals_won;
            merged.deals_lost = figures.deals_lost;
            merged.invoices = u64::try_from(figures.invoices.len()).unwrap_or_default();
            merged.money = figures.money();

            report.leads += figures.leads;
            report.deals_open += figures.deals_open;
            report.deals_won += figures.deals_won;
            report.deals_lost += figures.deals_lost;
            for (currency, money) in &figures.money {
                let site = site_money.entry(currency.clone()).or_default();
                site.open_cents = site.open_cents.saturating_add(money.open_cents);
                site.won_cents = site.won_cents.saturating_add(money.won_cents);
            }
        }
        report.sources.push(merged);
    }

    // The site's invoice figures come from the deduplicated document set, so a
    // customer two conversion points both brought in is counted once.
    for (currency, gross) in documents.values() {
        let site = site_money.entry(currency.clone()).or_default();
        site.invoiced_cents = site.invoiced_cents.saturating_add(*gross);
    }
    report.invoices = u64::try_from(documents.len()).unwrap_or_default();
    report.money = site_money
        .into_iter()
        .map(|(currency, figures)| SiteAttributionMoney {
            currency,
            open_cents: figures.open_cents,
            won_cents: figures.won_cents,
            invoiced_cents: figures.invoiced_cents,
        })
        .collect();
    report
}
