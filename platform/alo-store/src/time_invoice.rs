//! Approved billable hours become a **draft** invoice (alo Projects, ADR 0035,
//! wave B3.06) — the seam between the timesheet and alo Billing.
//!
//! Two functions and one rule each. [`AccountStore::unbilled_time`] answers
//! *what could be billed to this customer, grouped the way an invoice would
//! group it*; [`AccountStore::bill_time_entries`] carries a chosen set of those
//! hours onto a draft document and stamps them with it. Like
//! [`crate::crm_handoff`], it is one-way and one-shot: it issues nothing, sends
//! nothing, and touches no document it did not just create. A human issues the
//! invoice, through billing's own route, having read it.
//!
//! # What an hour must be before it may be billed
//!
//! This tenant's, `active` (never a proposal, ADR 0023), `billable`, in an
//! **approved** week, not already on a document, on a project whose client facts
//! name the customer being invoiced, and carrying a rate in the document's
//! currency. Nothing here guesses a missing one of those: an unrated billable
//! hour is legal and normal ([`crate::time_entries`]), and pricing it at zero —
//! or at today's engagement rate, which is not what was agreed when the work was
//! done — would put a number on a customer's document that nobody chose.
//!
//! Every refusal names **how many** hours broke the rule rather than the first
//! one it met: the caller is looking at a selection, and a refusal it cannot
//! size is one it cannot act on. Hours are personal data, so no refusal names a
//! person, a note or a day.
//!
//! # One transaction, and why it has to be
//!
//! The whole call — resolving the header, writing the lines, stamping the hours
//! — is one transaction, and the selected rows are locked (`FOR UPDATE`) before
//! anything is written. A partial invoice, with half the hours marked billed and
//! half still open, would be a document that under-bills a client *and* hours
//! that can never be billed again; two callers raising an invoice for the same
//! customer at the same instant would otherwise produce exactly that. They
//! serialise here instead, and the loser sees the hours as already billed.
//!
//! # Grouping: one line per (project, rate)
//!
//! Description is the project's name, unit is the word `hour` in the caller's
//! language (the edge owns the word — the store writes no untranslated text),
//! quantity is [`crate::time_hours::qty_milli_hours`] of the group's **summed**
//! minutes, and the price is the rate those hours were snapshotted at. One line
//! per entry was rejected: a month of six-minute stints is a two-hundred-line
//! invoice nobody reads. One line per task was rejected too, and named in the
//! design note's out-of-scope list so its absence is a decision — it is a
//! per-line rounding multiplier and a disclosure question (whose task names
//! travel to the customer?).
//!
//! # The link is released when the document goes away
//!
//! [`release_billed_hours`] is called by billing inside the transaction that
//! deletes a draft or voids an issued document, and the hours return to the
//! unbilled view. A **credit note does not release**: crediting corrects a
//! document, the hours stay billed against the original, and re-billing them
//! would charge a client twice for one piece of work.

use std::collections::BTreeMap;

use time::Date;

use crate::account::AccountStore;
use crate::billing_invoices::NewInvoice;
use crate::billing_line::{INVOICE_LINES, NewLine};
use crate::error::{Result, StoreError};
use crate::id::{BillingCustomerId, BillingInvoiceId, ProjectId, TimeEntryId};
use crate::time_hours::{hours_net_cents, qty_milli_hours};

/// The most hours one handoff — or one read of the unbilled view — may carry.
///
/// A year of full-time work is roughly two thousand entries, so a selection past
/// this is a period nobody meant to bill in one document. Refused, never
/// silently truncated: a shortened selection is an invoice that quietly leaves
/// hours behind, and the hours it left behind look billable forever.
pub const MAX_HANDOFF_ENTRIES: usize = 5_000;

/// One group of unbilled hours: what a line of the invoice would say.
///
/// The grouping key is the pair `(project, rate)`, which is the invoice's own
/// grouping — so what this view shows and what the document carries can never
/// disagree.
#[derive(Debug, Clone)]
pub struct UnbilledGroup {
    /// The project the hours were worked on.
    pub project_id: ProjectId,
    /// Its name — what the invoice line would say.
    pub project_name: String,
    /// The rate these hours were snapshotted at, or `None` when nobody had
    /// priced the engagement. An unrated group is shown, never priced, and
    /// cannot be billed until a human states a rate.
    pub rate_cents: Option<i64>,
    /// The currency of [`Self::rate_cents`], snapshotted with it.
    pub currency: Option<String>,
    /// The hours in the group, in minutes.
    pub minutes: i64,
    /// The entries themselves, in work order — what a caller selects from and
    /// sends back to [`AccountStore::bill_time_entries`].
    pub entry_ids: Vec<TimeEntryId>,
    /// What the group is worth in integer cents, net of VAT, computed by the
    /// same code the invoice line uses ([`crate::time_hours::hours_net_cents`]).
    /// `None` for an unrated group.
    pub net_cents: Option<i64>,
}

/// What a whole unbilled view adds up to.
///
/// Money is folded **per currency and never across one**: adding euros to
/// dollars at a rate somebody chose today is an invented figure
/// ([`crate::crm_report`]'s rule, and this module's for the same reason). Hours
/// nobody has priced are counted apart rather than valued at zero.
#[derive(Debug, Clone, Default)]
pub struct UnbilledTotals {
    /// Every eligible minute in the view, priced or not.
    pub minutes: i64,
    /// The minutes among them that carry no rate and therefore no value.
    pub unrated_minutes: i64,
    /// The value of the rest, one row per currency, in currency order.
    pub by_currency: Vec<UnbilledCurrencyTotal>,
}

/// What the view is worth in one currency.
#[derive(Debug, Clone)]
pub struct UnbilledCurrencyTotal {
    /// ISO 4217 code.
    pub currency: String,
    /// The priced minutes in it.
    pub minutes: i64,
    /// What they are worth, net of VAT, in integer cents.
    pub net_cents: i64,
}

/// Folds the groups of an unbilled view into its totals.
///
/// Pure, and in the store rather than at the edge because it is money: the
/// figure a screen shows must be one the server computed from the same rows the
/// invoice will carry.
#[must_use]
pub fn unbilled_totals(groups: &[UnbilledGroup]) -> UnbilledTotals {
    let mut totals = UnbilledTotals::default();
    for group in groups {
        totals.minutes = totals.minutes.saturating_add(group.minutes);
        let (Some(currency), Some(net)) = (group.currency.as_deref(), group.net_cents) else {
            totals.unrated_minutes = totals.unrated_minutes.saturating_add(group.minutes);
            continue;
        };
        match totals
            .by_currency
            .binary_search_by(|row| row.currency.as_str().cmp(currency))
        {
            Ok(at) => {
                let row = &mut totals.by_currency[at];
                row.minutes = row.minutes.saturating_add(group.minutes);
                row.net_cents = row.net_cents.saturating_add(net);
            }
            Err(at) => totals.by_currency.insert(
                at,
                UnbilledCurrencyTotal {
                    currency: currency.to_owned(),
                    minutes: group.minutes,
                    net_cents: net,
                },
            ),
        }
    }
    totals
}

/// What a caller adds to a set of hours to make them a document.
#[derive(Debug, Clone)]
pub struct TimeBilling {
    /// The customer being invoiced. Every selected hour must be on a project
    /// worked for exactly this customer.
    pub customer_id: BillingCustomerId,
    /// The VAT rate, in basis points, every line is billed at. Stated by the
    /// caller and never guessed: picking a rate on a tenant's behalf is a
    /// compliance statement made by a machine ([`crate::crm_handoff`]'s rule).
    pub vat_rate_bp: i32,
    /// The document's currency, or `None` for the customer's own. Every selected
    /// hour's rate must be expressed in whichever it resolves to.
    pub currency: Option<String>,
    /// The unit label the lines carry — the word `hour` in the caller's
    /// language, supplied by the edge because the store writes no untranslated
    /// words onto a document a customer reads.
    pub unit: String,
    /// The hours to carry. A set: repeats are the same hour named twice, not two
    /// hours.
    pub entry_ids: Vec<TimeEntryId>,
}

/// What a handoff produced: the draft it raised and what it put on it.
#[derive(Debug, Clone)]
pub struct TimeInvoiceDraft {
    /// The **draft** invoice. It carries no number, consumes nothing from the
    /// gapless sequence, and is issued — if it ever is — by billing's own route.
    pub invoice_id: BillingInvoiceId,
    /// How many hours were carried onto it.
    pub entries: usize,
    /// How many lines they were grouped into.
    pub lines: usize,
    /// The minutes those hours add up to.
    pub minutes: i64,
}

/// One selected hour, as the handoff needs to judge it.
///
/// No id, no user, no day and no note: the rules below are counts over the
/// selection, the stamping addresses the rows by the ids the caller sent, and an
/// hour's owner is personal data that this path has no reason to carry.
#[derive(Debug, sqlx::FromRow)]
struct BillableRow {
    project_id: String,
    project_name: String,
    minutes: i64,
    billable: bool,
    state: String,
    rate_cents: Option<i64>,
    currency: Option<String>,
    billed: bool,
    /// The customer the project is worked for, or `None` for an internal
    /// project with no client facts at all.
    customer_id: Option<String>,
    /// The status of the week the hour falls in, or `None` when that week has
    /// never been handed in — which is what open means.
    week_status: Option<String>,
}

/// The grouping key of an invoice line: a project, and a rate in a currency.
/// `BTreeMap` rather than a hash map so the lines come out in a stable order for
/// a given set of hours — a document whose line order depends on hash seeds is
/// one whose PDF differs between two identical runs.
type GroupKey = (String, Option<i64>, Option<String>);

/// The fold of one group as it is accumulated.
#[derive(Default)]
struct Group {
    project_name: String,
    minutes: i64,
    entry_ids: Vec<TimeEntryId>,
}

/// The status a week must be in before its hours may reach a document.
const WEEK_APPROVED: &str = "approved";

/// The state a real, countable entry is in (a proposal is not work yet).
const STATE_ACTIVE: &str = "active";

/// Clears the invoice link from every hour a document carried — called by
/// [`crate::billing_invoices`] inside the transaction that deletes a draft or
/// voids an issued document, so the hours return to the unbilled view in the
/// same instant the document stops existing.
///
/// Deliberately a statement rather than a foreign key: `ON DELETE CASCADE` would
/// delete the *hours* when a draft is discarded, and the composite
/// `ON DELETE SET NULL` this schema would need would null `tenant_id` too
/// (`docs/design/projects.md`).
///
/// # Errors
/// [`StoreError::Db`] on failure.
pub(crate) async fn release_billed_hours(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    invoice: &BillingInvoiceId,
) -> Result<()> {
    sqlx::query(
        "UPDATE time_entries SET invoice_id = NULL, billed_at = NULL, updated_at = now() \
         WHERE tenant_id = $1 AND invoice_id = $2",
    )
    .bind(tenant)
    .bind(invoice.as_str())
    .execute(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    Ok(())
}

/// Counts the hours a rule refused, and refuses with the count.
fn refuse_count<T>(count: usize, what: &str) -> Result<T> {
    Err(StoreError::Validation(format!("{count} {what}")))
}

/// How many of the selected hours a rule holds against.
fn count<P: Fn(&BillableRow) -> bool>(rows: &[BillableRow], predicate: P) -> usize {
    rows.iter().filter(|row| predicate(row)).count()
}

impl AccountStore {
    /// What could be billed to `customer`: every approved, billable, unbilled
    /// hour on a project worked for them, grouped one line per (project, rate),
    /// with what each group is worth.
    ///
    /// `to` is an optional cut-off — the last day to include — because an
    /// invoice is raised for a period. Absent, everything eligible comes back.
    ///
    /// **This is a tenant-wide read on the account door, and deliberately so.**
    /// An invoice carries the team's hours, not the caller's, so this crosses
    /// the personal boundary — but only as an *aggregate*: it answers with
    /// projects, minutes and money, and never with who worked when. The
    /// per-person breakdown stays an admin column
    /// (`docs/design/projects.md` § The hours of a person are personal data).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the customer is not this tenant's —
    /// existence is never disclosed; [`StoreError::Validation`] when more than
    /// [`MAX_HANDOFF_ENTRIES`] hours match, which is a period to narrow rather
    /// than a list to truncate; [`StoreError::Db`] on failure.
    pub async fn unbilled_time(
        &self,
        customer: &BillingCustomerId,
        to: Option<Date>,
    ) -> Result<Vec<UnbilledGroup>> {
        // The customer is resolved under this handle first, so another tenant's
        // id answers `404` rather than an empty list that reads like "nothing to
        // bill".
        self.billing_customer(customer)
            .await?
            .ok_or(StoreError::NotFound)?;
        let rows = self.billable_rows_for(customer, to).await?;
        if rows.len() > MAX_HANDOFF_ENTRIES {
            return Err(StoreError::Validation(format!(
                "more than {MAX_HANDOFF_ENTRIES} unbilled hours match; bill an earlier period \
                 first by setting a cut-off date"
            )));
        }
        Ok(fold_groups(rows))
    }

    /// The eligible hours for one customer, in the order the groups are built
    /// in. One statement: the entry, its project's name, and the week it falls
    /// in.
    async fn billable_rows_for(
        &self,
        customer: &BillingCustomerId,
        to: Option<Date>,
    ) -> Result<Vec<UnbilledRow>> {
        sqlx::query_as::<_, UnbilledRow>(
            "SELECT e.id, e.project_id, p.name AS project_name, e.minutes, e.rate_cents, \
                 e.currency \
             FROM time_entries e \
             JOIN task_projects p ON p.tenant_id = e.tenant_id AND p.id = e.project_id \
             JOIN project_clients c ON c.tenant_id = e.tenant_id AND c.project_id = e.project_id \
             JOIN time_weeks w ON w.tenant_id = e.tenant_id AND w.user_id = e.user_id \
                 AND w.week_start = date_trunc('week', e.work_date)::date \
             WHERE e.tenant_id = $1 AND c.customer_id = $2 \
               AND e.state = 'active' AND e.billable AND e.invoice_id IS NULL \
               AND w.status = 'approved' \
               AND ($3::date IS NULL OR e.work_date <= $3) \
             ORDER BY lower(p.name), p.id, e.rate_cents NULLS LAST, e.currency, e.work_date, e.id \
             LIMIT $4",
        )
        .bind(self.tenant.as_str())
        .bind(customer.as_str())
        .bind(to)
        // One past the ceiling, so "too many" is distinguishable from "exactly
        // the maximum" and the refusal is never a silent truncation.
        .bind(
            i64::try_from(MAX_HANDOFF_ENTRIES)
                .unwrap_or(i64::MAX)
                .saturating_add(1),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)
    }

    /// Carries a chosen set of approved billable hours onto a **draft** invoice
    /// and stamps them with it.
    ///
    /// All-or-nothing in one transaction: either every named hour is on the new
    /// document, or nothing was written at all. The document is a draft like any
    /// other — no number, no dates, freely editable, deletable — and deleting or
    /// voiding it releases the hours back to the unbilled view
    /// ([`release_billed_hours`]).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the customer, or any named hour, is not
    /// this tenant's; [`StoreError::Validation`] when the selection is empty or
    /// too long, or when hours are proposals, not billable, on another
    /// customer's work, unrated, or priced in another currency — each naming how
    /// many; [`StoreError::Conflict`] when hours are already on a document, are
    /// in a week that has not been approved, or changed under the call;
    /// [`StoreError::Db`] on failure.
    pub async fn bill_time_entries(&self, billing: &TimeBilling) -> Result<TimeInvoiceDraft> {
        let ids = unique_ids(&billing.entry_ids)?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // The header first: it resolves the customer under this handle (a
        // guessed id is a `NotFound`), refuses an archived one, and decides the
        // currency every selected hour is then held to.
        let header = self
            .normalize_invoice_in(
                &mut tx,
                &NewInvoice {
                    currency: billing.currency.clone(),
                    ..NewInvoice::for_customer(billing.customer_id.clone())
                },
            )
            .await?;

        let rows = self.lock_selected_hours(&mut tx, &ids).await?;
        let judged = judge(&rows, ids.len(), &header.customer_id, &header.currency)?;
        let lines = invoice_lines(&judged, &billing.unit, billing.vat_rate_bp);

        let invoice_id = self.insert_draft_invoice(&mut tx, &header).await?;
        INVOICE_LINES
            .replace(&mut tx, self.tenant.as_str(), invoice_id.as_str(), &lines)
            .await?;

        // `invoice_id IS NULL` is belt and braces: the rows are already locked,
        // so nothing can have taken them since they were judged. It is the
        // difference between a bug that double-bills a client and one that
        // refuses a call.
        let stamped = sqlx::query(
            "UPDATE time_entries SET invoice_id = $3, billed_at = now(), updated_at = now() \
             WHERE tenant_id = $1 AND id = ANY($2) AND invoice_id IS NULL",
        )
        .bind(self.tenant.as_str())
        .bind(&ids)
        .bind(invoice_id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if stamped.rows_affected() != u64::try_from(ids.len()).unwrap_or(u64::MAX) {
            return Err(StoreError::Conflict(
                "these hours changed while the invoice was being raised; read the unbilled view \
                 again and retry"
                    .to_owned(),
            ));
        }
        tx.commit().await.map_err(StoreError::Db)?;

        Ok(TimeInvoiceDraft {
            invoice_id,
            entries: ids.len(),
            lines: lines.len(),
            minutes: judged.iter().map(|row| row.minutes).sum(),
        })
    }

    /// Reads the named hours **under a row lock**, with everything a rule has to
    /// judge them by. Rows of other tenants simply do not match: the predicate
    /// is this handle's tenant.
    async fn lock_selected_hours(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        ids: &[String],
    ) -> Result<Vec<BillableRow>> {
        sqlx::query_as::<_, BillableRow>(
            "SELECT e.project_id, p.name AS project_name, e.minutes, e.billable, e.state, \
                 e.rate_cents, e.currency, (e.invoice_id IS NOT NULL) AS billed, c.customer_id, \
                 w.status AS week_status \
             FROM time_entries e \
             JOIN task_projects p ON p.tenant_id = e.tenant_id AND p.id = e.project_id \
             LEFT JOIN project_clients c \
                 ON c.tenant_id = e.tenant_id AND c.project_id = e.project_id \
             LEFT JOIN time_weeks w ON w.tenant_id = e.tenant_id AND w.user_id = e.user_id \
                 AND w.week_start = date_trunc('week', e.work_date)::date \
             WHERE e.tenant_id = $1 AND e.id = ANY($2) \
             ORDER BY lower(p.name), p.id, e.rate_cents NULLS LAST, e.currency, e.work_date, e.id \
             FOR UPDATE OF e",
        )
        .bind(self.tenant.as_str())
        .bind(ids)
        .fetch_all(&mut **tx)
        .await
        .map_err(StoreError::Db)
    }
}

/// One eligible hour as the unbilled view reads it — no user, no day, no note:
/// the view is an aggregate, and the hours of a person are personal data.
#[derive(sqlx::FromRow)]
struct UnbilledRow {
    id: String,
    project_id: String,
    project_name: String,
    minutes: i64,
    rate_cents: Option<i64>,
    currency: Option<String>,
}

/// The caller's selection as a set of stored ids, in the order given.
///
/// Repeats are the same hour named twice and are dropped rather than refused —
/// a UI that ticks a row twice has said one thing — but an empty selection is a
/// caller with nothing to bill, and a selection past the ceiling is a period to
/// narrow.
fn unique_ids(ids: &[TimeEntryId]) -> Result<Vec<String>> {
    let mut unique: Vec<String> = Vec::with_capacity(ids.len());
    for id in ids {
        let id = id.as_str();
        if !unique.iter().any(|seen| seen == id) {
            unique.push(id.to_owned());
        }
    }
    if unique.is_empty() {
        return Err(StoreError::Validation(
            "select at least one hour to bill".to_owned(),
        ));
    }
    if unique.len() > MAX_HANDOFF_ENTRIES {
        return Err(StoreError::Validation(format!(
            "one invoice may carry at most {MAX_HANDOFF_ENTRIES} hours; bill an earlier period \
             first"
        )));
    }
    Ok(unique)
}

/// Holds every selected hour to every rule, and answers with the rows that
/// passed — or with the first rule that failed, naming **how many** hours failed
/// it.
///
/// Pure over rows already read, so the whole rule set is unit-tested without a
/// database. The order of the checks is the order a human would want to hear
/// them: what is missing, then what is not work, then what is already gone, then
/// what does not belong on this document, then what cannot be priced.
///
/// `requested` is the **count** of distinct ids asked for, and comparing counts
/// is exactly as strong as comparing the sets: the rows were read with
/// `id = ANY(<those ids>)` under this tenant, against a primary key, so a row can
/// neither repeat nor be one nobody asked for.
fn judge<'r>(
    rows: &'r [BillableRow],
    requested: usize,
    customer_id: &str,
    currency: &str,
) -> Result<Vec<&'r BillableRow>> {
    if rows.len() != requested {
        // Some id named nothing this tenant owns. Never says which — an id that
        // is another tenant's must be indistinguishable from one that never
        // existed.
        return Err(StoreError::NotFound);
    }
    let proposals = count(rows, |r| r.state != STATE_ACTIVE);
    if proposals > 0 {
        return refuse_count(
            proposals,
            "of the selected hours are still proposals; accept them before billing them",
        );
    }
    let unbillable = count(rows, |r| !r.billable);
    if unbillable > 0 {
        return refuse_count(
            unbillable,
            "of the selected hours are not billable; a client is charged only for hours somebody \
             marked chargeable",
        );
    }
    let billed = count(rows, |r| r.billed);
    if billed > 0 {
        return Err(StoreError::Conflict(format!(
            "{billed} of the selected hours are already on a document; void or delete it to \
             release them"
        )));
    }
    let unapproved = count(rows, |r| r.week_status.as_deref() != Some(WEEK_APPROVED));
    if unapproved > 0 {
        return Err(StoreError::Conflict(format!(
            "{unapproved} of the selected hours are in a week that has not been approved; a \
             client is billed for hours somebody has signed off"
        )));
    }
    let elsewhere = count(rows, |r| r.customer_id.as_deref() != Some(customer_id));
    if elsewhere > 0 {
        return refuse_count(
            elsewhere,
            "of the selected hours are worked for another customer, or for none at all; an \
             invoice carries one customer's work",
        );
    }
    let unrated = count(rows, |r| r.rate_cents.is_none());
    if unrated > 0 {
        return refuse_count(
            unrated,
            "of the selected hours carry no rate; price the engagement, then log or re-log them — \
             an hour is never billed at a rate nobody agreed",
        );
    }
    let other_currency = count(rows, |r| r.currency.as_deref() != Some(currency));
    if other_currency > 0 {
        return Err(StoreError::Validation(format!(
            "{other_currency} of the selected hours are priced in another currency; this invoice \
             is in {currency}, and money is never converted on the way onto a document"
        )));
    }
    Ok(rows.iter().collect())
}

/// Folds judged hours into the document's lines: one per (project, rate), in the
/// order the rows arrived (project name, then rate).
///
/// The minutes of a group are summed **before** the conversion to milli-hours,
/// so a month of six-minute stints is priced as the hours they add up to rather
/// than as two hundred separate roundings ([`crate::time_hours`]).
fn invoice_lines(rows: &[&BillableRow], unit: &str, vat_rate_bp: i32) -> Vec<NewLine> {
    let mut lines: Vec<NewLine> = Vec::new();
    let mut keys: Vec<(String, Option<i64>)> = Vec::new();
    for row in rows {
        let key = (row.project_id.clone(), row.rate_cents);
        match keys.iter().position(|seen| *seen == key) {
            Some(at) => {
                lines[at].qty_milli = lines[at].qty_milli.saturating_add(row.minutes);
            }
            None => {
                keys.push(key);
                lines.push(NewLine {
                    description: row.project_name.clone(),
                    unit: unit.to_owned(),
                    // Minutes for now; converted once, below, when the group is
                    // complete.
                    qty_milli: row.minutes,
                    unit_price_cents: row.rate_cents.unwrap_or_default(),
                    vat_rate_bp,
                });
            }
        }
    }
    for line in &mut lines {
        line.qty_milli = qty_milli_hours(line.qty_milli);
    }
    lines
}

/// Folds the eligible rows of the unbilled view into its groups.
fn fold_groups(rows: Vec<UnbilledRow>) -> Vec<UnbilledGroup> {
    let mut order: Vec<GroupKey> = Vec::new();
    let mut groups: BTreeMap<GroupKey, Group> = BTreeMap::new();
    for row in rows {
        let key = (row.project_id, row.rate_cents, row.currency);
        let group = groups.entry(key.clone()).or_insert_with(|| {
            order.push(key);
            Group {
                project_name: row.project_name,
                ..Group::default()
            }
        });
        group.minutes = group.minutes.saturating_add(row.minutes);
        group.entry_ids.push(TimeEntryId::new(row.id));
    }
    order
        .into_iter()
        .filter_map(|key| {
            let group = groups.remove(&key)?;
            let (project_id, rate_cents, currency) = key;
            Some(UnbilledGroup {
                project_id: ProjectId::new(project_id),
                project_name: group.project_name,
                rate_cents,
                currency,
                minutes: group.minutes,
                net_cents: rate_cents.map(|rate| hours_net_cents(group.minutes, rate)),
                entry_ids: group.entry_ids,
            })
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn row(project: &str, rate: Option<i64>, minutes: i64) -> BillableRow {
        BillableRow {
            project_id: project.to_owned(),
            project_name: format!("Project {project}"),
            minutes,
            billable: true,
            state: STATE_ACTIVE.to_owned(),
            rate_cents: rate,
            currency: rate.map(|_| "EUR".to_owned()),
            billed: false,
            customer_id: Some("cust-1".to_owned()),
            week_status: Some(WEEK_APPROVED.to_owned()),
        }
    }

    fn judged(rows: &[BillableRow]) -> Vec<&BillableRow> {
        judge(rows, rows.len(), "cust-1", "EUR").expect("every rule holds")
    }

    fn refusal(rows: &[BillableRow]) -> StoreError {
        judge(rows, rows.len(), "cust-1", "EUR").expect_err("refused")
    }

    fn message(error: &StoreError) -> String {
        match error {
            StoreError::Validation(msg) | StoreError::Conflict(msg) => msg.clone(),
            other => panic!("expected a message, got {other:?}"),
        }
    }

    #[test]
    fn hours_on_one_project_at_one_rate_become_one_line() {
        let rows = [row("a", Some(9_500), 90), row("a", Some(9_500), 30)];
        let lines = invoice_lines(&judged(&rows), "hour", 2_100);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].description, "Project a");
        assert_eq!(lines[0].unit, "hour");
        // Two hours: summed as minutes, converted once.
        assert_eq!(lines[0].qty_milli, 2_000);
        assert_eq!(lines[0].unit_price_cents, 9_500);
        assert_eq!(lines[0].vat_rate_bp, 2_100);
    }

    #[test]
    fn a_rate_change_splits_the_line_and_a_second_project_adds_one() {
        let rows = [
            row("a", Some(9_500), 60),
            row("a", Some(11_000), 60),
            row("b", Some(9_500), 60),
        ];
        let lines = invoice_lines(&judged(&rows), "hour", 2_100);
        assert_eq!(lines.len(), 3);
        assert_eq!(
            lines
                .iter()
                .map(|l| (l.description.as_str(), l.unit_price_cents))
                .collect::<Vec<_>>(),
            vec![
                ("Project a", 9_500),
                ("Project a", 11_000),
                ("Project b", 9_500),
            ]
        );
    }

    #[test]
    fn the_group_is_summed_before_it_is_converted() {
        // Ten one-minute stints are ten minutes (167 milli-hours), not ten
        // roundings of one minute (170).
        let rows: Vec<BillableRow> = (0..10).map(|_| row("a", Some(6_000), 1)).collect();
        let lines = invoice_lines(&judged(&rows), "hour", 2_100);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].qty_milli, 167);
    }

    #[test]
    fn a_missing_hour_is_a_clean_not_found_and_never_says_which() {
        let rows = [row("a", Some(9_500), 60)];
        // Two ids asked for, one row found: the other named nothing this tenant
        // owns.
        match judge(&rows, 2, "cust-1", "EUR") {
            Err(StoreError::NotFound) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn every_refusal_names_how_many_hours_broke_the_rule() {
        let proposals = [
            row("a", Some(9_500), 60),
            BillableRow {
                state: "proposed".to_owned(),
                ..row("a", Some(9_500), 30)
            },
        ];
        assert!(message(&refusal(&proposals)).starts_with("1 of the selected hours are still"));

        let unbillable = [
            BillableRow {
                billable: false,
                ..row("a", Some(9_500), 60)
            },
            BillableRow {
                billable: false,
                ..row("a", Some(9_500), 30)
            },
        ];
        assert!(message(&refusal(&unbillable)).starts_with("2 of the selected hours are not"));

        let billed = [BillableRow {
            billed: true,
            ..row("a", Some(9_500), 60)
        }];
        let error = refusal(&billed);
        assert!(matches!(error, StoreError::Conflict(_)));
        assert!(message(&error).contains("already on a document"));

        let unapproved = [BillableRow {
            week_status: Some("submitted".to_owned()),
            ..row("a", Some(9_500), 60)
        }];
        let error = refusal(&unapproved);
        assert!(matches!(error, StoreError::Conflict(_)));
        assert!(message(&error).contains("has not been approved"));

        // A week nobody ever submitted is open, and open is not approved.
        let never_submitted = [BillableRow {
            week_status: None,
            ..row("a", Some(9_500), 60)
        }];
        assert!(message(&refusal(&never_submitted)).contains("has not been approved"));

        let elsewhere = [BillableRow {
            customer_id: Some("cust-2".to_owned()),
            ..row("a", Some(9_500), 60)
        }];
        assert!(message(&refusal(&elsewhere)).contains("another customer"));

        let internal = [BillableRow {
            customer_id: None,
            ..row("a", Some(9_500), 60)
        }];
        assert!(message(&refusal(&internal)).contains("another customer"));

        let unrated = [row("a", None, 60)];
        assert!(message(&refusal(&unrated)).contains("no rate"));

        let foreign = [BillableRow {
            currency: Some("USD".to_owned()),
            ..row("a", Some(9_500), 60)
        }];
        let error = refusal(&foreign);
        assert!(matches!(error, StoreError::Validation(_)));
        assert!(message(&error).contains("EUR"));
    }

    #[test]
    fn the_view_totals_per_currency_and_never_across_one() {
        let group = |rate: Option<i64>, currency: Option<&str>, minutes: i64| UnbilledGroup {
            project_id: ProjectId::new("p".to_owned()),
            project_name: "Project".to_owned(),
            rate_cents: rate,
            currency: currency.map(str::to_owned),
            minutes,
            entry_ids: Vec::new(),
            net_cents: rate.map(|r| hours_net_cents(minutes, r)),
        };
        let totals = unbilled_totals(&[
            group(Some(9_500), Some("EUR"), 60),
            group(Some(10_000), Some("USD"), 120),
            group(Some(9_500), Some("EUR"), 30),
            group(None, None, 45),
        ]);
        assert_eq!(totals.minutes, 255);
        assert_eq!(totals.unrated_minutes, 45, "never valued at zero");
        assert_eq!(
            totals.by_currency.len(),
            2,
            "euros are not added to dollars"
        );
        assert_eq!(totals.by_currency[0].currency, "EUR");
        assert_eq!(totals.by_currency[0].minutes, 90);
        assert_eq!(totals.by_currency[0].net_cents, 9_500 + 4_750);
        assert_eq!(totals.by_currency[1].currency, "USD");
        assert_eq!(totals.by_currency[1].net_cents, 20_000);
    }

    #[test]
    fn a_selection_is_a_set_and_is_never_empty_or_endless() {
        let one = TimeEntryId::new("e1".to_owned());
        assert_eq!(
            unique_ids(&[one.clone(), one.clone()]).unwrap(),
            vec!["e1".to_owned()]
        );
        assert!(unique_ids(&[]).is_err());
        let too_many: Vec<TimeEntryId> = (0..=MAX_HANDOFF_ENTRIES)
            .map(|i| TimeEntryId::new(format!("e{i}")))
            .collect();
        let error = unique_ids(&too_many).expect_err("refused");
        assert!(message(&error).contains(&MAX_HANDOFF_ENTRIES.to_string()));
    }

    #[test]
    fn the_unbilled_view_groups_and_prices_the_same_way_the_document_does() {
        let groups = fold_groups(vec![
            UnbilledRow {
                id: "e1".to_owned(),
                project_id: "a".to_owned(),
                project_name: "Project a".to_owned(),
                minutes: 90,
                rate_cents: Some(9_500),
                currency: Some("EUR".to_owned()),
            },
            UnbilledRow {
                id: "e2".to_owned(),
                project_id: "a".to_owned(),
                project_name: "Project a".to_owned(),
                minutes: 30,
                rate_cents: Some(9_500),
                currency: Some("EUR".to_owned()),
            },
            UnbilledRow {
                id: "e3".to_owned(),
                project_id: "a".to_owned(),
                project_name: "Project a".to_owned(),
                minutes: 60,
                rate_cents: None,
                currency: None,
            },
        ]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].minutes, 120);
        assert_eq!(groups[0].entry_ids.len(), 2);
        // Two hours at €95 — the same cents the invoice line will carry.
        assert_eq!(groups[0].net_cents, Some(19_000));
        // An unrated group is shown and never priced.
        assert_eq!(groups[1].minutes, 60);
        assert_eq!(groups[1].rate_cents, None);
        assert_eq!(groups[1].net_cents, None);
    }
}
