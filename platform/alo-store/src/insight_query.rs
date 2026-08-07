//! The alo Insights **query engine**: a [`ChartSpec`] compiled into reads of
//! this tenant's own rows (ADR 0037, wave BI1.03).
//!
//! This is the only file in the wave that contains SQL, and every statement in
//! it is a `&'static str` fragment written here at compile time. Nothing a user
//! or a model sends ever reaches a query as SQL text: a spec names *enum
//! variants* ([`crate::insight_catalog`]), each variant maps to a fragment
//! chosen by a `match`, and the only caller-controlled things that cross into a
//! statement are **bound parameters** — dates, integers and opaque ids, all of
//! them data.
//!
//! **Tenancy is structural, twice over.** The tenant id comes from the account
//! door and is bound as `$1` of every statement here; a ChartSpec has no field
//! that could name a tenant, so a model is never in a position to leak one. A
//! filter value that *is* an id (a customer, a pipeline, an owner) is resolved
//! against the tenant's own records **before** it is bound, so a guessed id
//! from another tenant is a refusal rather than a join that quietly matches
//! nothing — a silently empty tile is how a business comes to believe it billed
//! nothing last quarter.
//!
//! ## Where money may be added up
//!
//! One law, and it is the law that keeps a tile and a tax return agreeing:
//!
//! > SQL may sum a stored integer column. SQL may never **derive** money — it
//! > never multiplies quantity by price, never applies a VAT rate, never
//! > rounds, and never converts a currency.
//!
//! So the datasets divide honestly into two shapes:
//!
//! - **Folded** (`billing.documents`, `billing.receivables`). The statements
//!   read document headers and line figures; the money is computed by
//!   [`crate::billing_totals`] — the same function the printed invoice, the
//!   PDF, the e-invoice and the VAT summary use — and restated at each
//!   document's **own frozen rate** through [`crate::billing_fx`]. A document
//!   that carries no usable snapshot is never converted at a guessed rate: it
//!   is counted in a note and left out of the figure.
//! - **Grouped** (`billing.payments`, `crm.deals`). Stored integer columns,
//!   grouped and summed by Postgres, exactly as
//!   [`crate::crm_report`] already does. **Neither is ever converted**: a deal
//!   is a forecast with no tax point, and a payment's value date is not the
//!   document's — the rate frozen on an invoice is the rate of its tax point,
//!   not of the day the money arrived. Both answer one series per currency
//!   rather than adding euros to dollars behind a single bar.
//!
//! ## What bounds the work
//!
//! A folded read is bounded by its period, its filters and a hard row cap
//! ([`MAX_SCANNED_ROWS`]); a grouped read by the number of groups it produces
//! ([`MAX_GROUPS`]). Over either is a typed refusal asking for a narrower
//! period — never a silent truncation, and never a statement that ties up a
//! connection.

use std::collections::{BTreeSet, HashMap};

use sqlx::postgres::{PgArguments, PgRow};
use sqlx::query::QueryAs;
use time::{Date, OffsetDateTime, Time, UtcOffset};

use crate::account::AccountStore;
use crate::billing_fx::{FxSnapshot, convert_cents, restated_into};
use crate::billing_line::{FiguresRow, group_figures};
use crate::billing_payments::Settlement;
use crate::billing_totals::{Totals, totals};
use crate::error::{Result, StoreError};
use crate::insight_catalog::{
    self, Aggregate, Dataset, Dimension, FilterField, FilterOp, Grain, Measure, Unit,
};
use crate::insight_series::{
    ALL_GROUP, Datum, FoldPlan, Label, Note, Series, TOTAL_BUCKET, UNCONVERTED_DOCUMENTS,
    bucket_key, bucket_start, buckets_before, fold, window_buckets,
};
use crate::insight_spec::{ChartSpec, MAX_CATEGORIES, Period, Sort, SortBy, SortDir};

/// The most document and line rows one folded evaluation may scan. At the SME
/// scale ADR 0035 describes, five years is thousands of rows; the cap is what
/// keeps a pathological period from tying up a connection, and going over it is
/// a refusal that names the fix.
pub const MAX_SCANNED_ROWS: usize = 200_000;
/// The most groups one grouped evaluation may produce — a breakdown wider than
/// this is not a chart anybody reads.
pub const MAX_GROUPS: usize = 20_000;

/// The catalog id a bucket carries when the value behind it is simply not set —
/// a deal with no source, a payment with no stated method.
const NONE_LABEL: &str = "value.none";
/// The catalog id a bucket carries when its record no longer resolves.
const UNKNOWN_LABEL: &str = "value.unknown";

/// A value bound to a statement. Every caller-controlled thing in this module
/// is one of these — never a fragment of SQL.
#[derive(Debug, Clone)]
enum Bound {
    /// A single text value.
    Text(String),
    /// A text array, for an `= ANY` / `<> ALL` comparison.
    Texts(Vec<String>),
    /// An integer array (VAT rates, in basis points).
    Ints(Vec<i32>),
    /// A calendar day, for a `DATE` column.
    Day(Date),
    /// An instant, for a `TIMESTAMPTZ` column.
    Stamp(OffsetDateTime),
}

/// A statement under construction: its `WHERE` conjuncts and the parameters
/// they refer to, kept together so a placeholder can never drift from the value
/// it stands for.
#[derive(Debug)]
struct Predicate {
    parts: Vec<String>,
    bounds: Vec<Bound>,
}

impl Predicate {
    /// A predicate that starts, always, with the tenant — `$1` of every
    /// statement this module builds.
    fn for_tenant(tenant: &str, column: &'static str) -> Self {
        Self {
            parts: vec![format!("{column} = $1")],
            bounds: vec![Bound::Text(tenant.to_owned())],
        }
    }

    /// Binds `value` and returns its placeholder index.
    fn bind(&mut self, value: Bound) -> usize {
        self.bounds.push(value);
        self.bounds.len()
    }

    /// Adds a conjunct.
    fn and(&mut self, part: String) {
        self.parts.push(part);
    }

    /// The whole `WHERE` body.
    fn sql(&self) -> String {
        self.parts.join(" AND ")
    }

    /// A copy of this predicate, for a second statement that reuses the same
    /// `WHERE` body and then binds more parameters of its own after it.
    fn clone_bounds(&self) -> Self {
        Self {
            parts: self.parts.clone(),
            bounds: self.bounds.clone(),
        }
    }
}

/// Binds a predicate's parameters, in order, to a prepared statement.
fn bind_all<'q, O>(
    query: QueryAs<'q, sqlx::Postgres, O, PgArguments>,
    bounds: &'q [Bound],
) -> QueryAs<'q, sqlx::Postgres, O, PgArguments>
where
    O: Send + Unpin + for<'r> sqlx::FromRow<'r, PgRow>,
{
    bounds.iter().fold(query, |query, bound| match bound {
        Bound::Text(value) => query.bind(value.as_str()),
        Bound::Texts(values) => query.bind(values.as_slice()),
        Bound::Ints(values) => query.bind(values.as_slice()),
        Bound::Day(value) => query.bind(*value),
        Bound::Stamp(value) => query.bind(*value),
    })
}

/// The refusal a caller can act on.
fn invalid(detail: impl Into<String>) -> StoreError {
    StoreError::Validation(detail.into())
}

/// One document header, whichever billing dataset asked for it.
#[derive(Debug, sqlx::FromRow)]
struct DocumentRow {
    id: String,
    customer_id: String,
    currency: String,
    status: String,
    issue_date: Option<Date>,
    due_date: Option<Date>,
    fx_base_currency: Option<String>,
    fx_rate_micro: Option<i64>,
    fx_rate_date: Option<Date>,
}

impl DocumentRow {
    /// The rate frozen on the document, or `None` — all three columns move
    /// together (the table constrains them), so a partial snapshot is not a
    /// state this can be read out of.
    fn fx(&self) -> Option<FxSnapshot> {
        let base = self.fx_base_currency.clone()?;
        Some(FxSnapshot {
            base_currency: base,
            rate_micro: self.fx_rate_micro?,
            rate_date: self.fx_rate_date?,
        })
    }
}

/// One row of a grouped read: its currency, its bucket (NULL when the row's own
/// date is not set), how many rows it stands for, what they add up to, and how
/// many of them were won.
type GroupedRow = (String, Option<String>, i64, i64, i64);

/// The window a period covers, resolved against a stated day.
///
/// `None` is [`Period::All`] — unbounded by construction, because a spec cannot
/// know how much history a tenant holds. The row caps are what bound that case.
fn window(period: &Period, today: Date) -> Result<Option<(Date, Date)>> {
    match period {
        Period::LastN { n, grain } => {
            // `n` whole buckets ending with the one today is in — "the last 12
            // months" includes this month, part-way through as it is.
            let current =
                bucket_start(today, *grain).ok_or_else(|| invalid("period: out of range"))?;
            let first = buckets_before(current, *grain, n.saturating_sub(1))
                .ok_or_else(|| invalid("period: out of range"))?;
            let last = crate::insight_series::bucket_end(today, *grain)
                .ok_or_else(|| invalid("period: out of range"))?;
            Ok(Some((first, last)))
        }
        Period::Range { from, to } => {
            let parse = |raw: &str| {
                Date::parse(raw, &time::format_description::well_known::Iso8601::DATE)
                    .map_err(|_| invalid("period: from and to must be ISO dates (YYYY-MM-DD)"))
            };
            let from = parse(from)?;
            let to = parse(to)?;
            if to < from {
                return Err(invalid("period: from must not be after to"));
            }
            Ok(Some((from, to)))
        }
        Period::All => Ok(None),
    }
}

/// The half-open instant range a pair of inclusive days covers, in UTC — the
/// same join between days and instants [`crate::crm_report`] states.
fn instants(from: Date, to: Date) -> Result<(OffsetDateTime, OffsetDateTime)> {
    let day_after = to
        .next_day()
        .ok_or_else(|| invalid("period: ends beyond the last day there is"))?;
    Ok((
        from.with_time(Time::MIDNIGHT).assume_offset(UtcOffset::UTC),
        day_after
            .with_time(Time::MIDNIGHT)
            .assume_offset(UtcOffset::UTC),
    ))
}

/// The Postgres pattern that reproduces [`bucket_key`] for a grouped dataset.
/// The two spellings of a bucket are checked against each other by test, so one
/// dataset's January can never land beside another's.
fn to_char_format(grain: Grain) -> &'static str {
    match grain {
        Grain::Day => "YYYY-MM-DD",
        // ISO week-numbering year and week, so 1 January 2027 stays in 2026-W53.
        Grain::Week => "IYYY-\"W\"IW",
        Grain::Month => "YYYY-MM",
        Grain::Quarter => "YYYY-\"Q\"Q",
        Grain::Year => "YYYY",
    }
}

/// Which stored date a period narrows on, and how it is compared.
enum PeriodColumn {
    /// A `DATE` column: both ends inclusive.
    Day(&'static str),
    /// A `TIMESTAMPTZ` column: half-open, midnight to midnight.
    Stamp(&'static str),
}

/// The column each dataset's dates live in. Total over the time dimensions the
/// catalog declares, which is what [`ChartSpec::period_dimension`] can return.
fn period_column(dataset: Dataset, dimension: Dimension) -> Result<PeriodColumn> {
    Ok(match (dataset, dimension) {
        (Dataset::BillingDocuments, Dimension::IssueDate) => PeriodColumn::Day("issue_date"),
        (Dataset::BillingReceivables, Dimension::DueDate) => PeriodColumn::Day("due_date"),
        (Dataset::BillingPayments, Dimension::PaidOn) => PeriodColumn::Day("p.paid_on"),
        (Dataset::CrmDeals, Dimension::CreatedAt) => PeriodColumn::Stamp("d.created_at"),
        (Dataset::CrmDeals, Dimension::ClosedAt) => PeriodColumn::Stamp("d.closed_at"),
        (Dataset::CrmDeals, Dimension::ExpectedClose) => PeriodColumn::Day("d.expected_close"),
        _ => return Err(invalid("period_on: this dataset has no such date")),
    })
}

/// The column a filter field compares against, per dataset. `None` means the
/// field is not compared in SQL at all — the VAT rate of a document filters its
/// *lines*, which is a different statement.
fn filter_column(dataset: Dataset, field: FilterField) -> Option<&'static str> {
    match (dataset, field) {
        (Dataset::BillingDocuments | Dataset::BillingReceivables, FilterField::Customer) => {
            Some("customer_id")
        }
        (Dataset::BillingDocuments | Dataset::BillingReceivables, FilterField::Currency) => {
            Some("currency")
        }
        (Dataset::BillingDocuments, FilterField::Status) => Some("status"),
        (Dataset::BillingDocuments, FilterField::VatRate) => None,
        (Dataset::BillingPayments, FilterField::Customer) => Some("i.customer_id"),
        (Dataset::BillingPayments, FilterField::Currency) => Some("i.currency"),
        (Dataset::BillingPayments, FilterField::Method) => Some("p.method"),
        (Dataset::CrmDeals, FilterField::Pipeline) => Some("d.pipeline_id"),
        (Dataset::CrmDeals, FilterField::Owner) => Some("d.owner_user_id"),
        (Dataset::CrmDeals, FilterField::Outcome) => Some("COALESCE(d.outcome, 'open')"),
        (Dataset::CrmDeals, FilterField::Currency) => Some("d.currency"),
        _ => None,
    }
}

/// The SQL expression a grouped dataset buckets by. `'total'` when the chart
/// has no breakdown — one bucket, one figure.
fn bucket_expression(dataset: Dataset, dimension: Option<(Dimension, Option<Grain>)>) -> String {
    let Some((dimension, grain)) = dimension else {
        return format!("'{TOTAL_BUCKET}'::text");
    };
    let stamp = |column: &str, grain: Option<Grain>| {
        format!(
            "to_char({column} AT TIME ZONE 'UTC', '{}')",
            to_char_format(grain.unwrap_or(Grain::Day))
        )
    };
    let date = |column: &str, grain: Option<Grain>| {
        format!(
            "to_char({column}, '{}')",
            to_char_format(grain.unwrap_or(Grain::Day))
        )
    };
    match (dataset, dimension) {
        (Dataset::BillingPayments, Dimension::PaidOn) => date("p.paid_on", grain),
        (Dataset::BillingPayments, Dimension::Method) => "p.method".to_owned(),
        (Dataset::BillingPayments, Dimension::Customer) => "i.customer_id".to_owned(),
        (Dataset::BillingPayments, Dimension::Currency) => "i.currency".to_owned(),
        (Dataset::CrmDeals, Dimension::Stage) => "d.stage_id".to_owned(),
        (Dataset::CrmDeals, Dimension::Owner) => "d.owner_user_id".to_owned(),
        (Dataset::CrmDeals, Dimension::Source) => "d.source".to_owned(),
        (Dataset::CrmDeals, Dimension::Outcome) => "COALESCE(d.outcome, 'open')".to_owned(),
        (Dataset::CrmDeals, Dimension::Currency) => "d.currency".to_owned(),
        (Dataset::CrmDeals, Dimension::CreatedAt) => stamp("d.created_at", grain),
        (Dataset::CrmDeals, Dimension::ClosedAt) => stamp("d.closed_at", grain),
        (Dataset::CrmDeals, Dimension::ExpectedClose) => date("d.expected_close", grain),
        // Unreachable: the catalog's matrix is what decides which dimensions a
        // dataset offers, and the spec was validated against it. A bucket
        // nothing groups by is one bucket, which is the honest fallback.
        _ => format!("'{TOTAL_BUCKET}'::text"),
    }
}

/// The bucket key of a VAT rate, zero-padded to the width of the highest rate
/// there can be (10 000 bp = 100 %).
///
/// A bucket key is a **sort key** — ordering a chart "by dimension" orders it by
/// this string — so a rate has to sort as the number it is. Unpadded, a 9 %
/// column would land after a 21 % one because `"900" > "2100"` as text. The
/// label carries the rate itself, so nothing a person reads shows the padding.
fn rate_bucket(rate_bp: i32) -> String {
    format!("{rate_bp:05}")
}

/// How overdue a receivable is on `today`, as a bucket key.
///
/// Five bands rather than the four the design note sketched: money that is not
/// yet due is **not** money that is up to thirty days late, and an aged-debtors
/// report that mixed them would overstate how much of the ledger is a problem.
fn age_bucket(due: Option<Date>, today: Date) -> &'static str {
    let Some(due) = due else {
        return "age.not_due";
    };
    let days = (today - due).whole_days();
    match days {
        i64::MIN..=0 => "age.not_due",
        1..=30 => "age.0_30",
        31..=60 => "age.31_60",
        61..=90 => "age.61_90",
        _ => "age.90_plus",
    }
}

impl AccountStore {
    /// The series a chart spec asks for, over this tenant's rows, as of today.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the spec breaks a catalog rule, names an
    /// id that is not this tenant's, or asks for more rows than the caps allow;
    /// [`StoreError::Db`] on failure.
    pub async fn insight_evaluate(&self, spec: &ChartSpec) -> Result<Series> {
        self.insight_evaluate_on(spec, OffsetDateTime::now_utc().date())
            .await
    }

    /// [`Self::insight_evaluate`] against a stated day.
    ///
    /// The day reaches the arithmetic in exactly two places — resolving a
    /// `last_n` period, and deciding how overdue a receivable is — so a golden
    /// test can state a hand-computed series instead of one that changes at
    /// midnight.
    ///
    /// # Errors
    /// As [`Self::insight_evaluate`].
    pub async fn insight_evaluate_on(&self, spec: &ChartSpec, today: Date) -> Result<Series> {
        // Defence in depth: a spec reaching here has normally been through the
        // write gate already, but a caller can build one by hand, and the
        // compiler below trusts the catalog's matrix to have been checked.
        spec.validate().map_err(|e| invalid(e.to_string()))?;
        let window = window(&spec.period, today)?;
        self.check_filter_ids(spec).await?;

        let (data, groups, notes) = match spec.dataset {
            Dataset::BillingDocuments | Dataset::BillingReceivables => {
                self.folded_data(spec, window, today).await?
            }
            Dataset::BillingPayments | Dataset::CrmDeals => self.grouped_data(spec, window).await?,
        };

        let measure = insight_catalog::dataset(spec.dataset)
            .measure(spec.measure.id)
            .ok_or_else(|| invalid("measure: not offered by this dataset"))?;
        let dimension = spec.dimension;
        let is_time = dimension.is_some_and(|d| d.grain.is_some());
        let plan = FoldPlan {
            agg: spec.measure.agg,
            unit: measure.unit,
            unit_currency: restates(spec).then(|| groups.first().cloned()).flatten(),
            always: groups,
            window: match (is_time, window, dimension.and_then(|d| d.grain)) {
                (true, Some((from, to)), Some(grain)) => Some(window_buckets(from, to, grain)),
                _ => None,
            },
            limit: usize::try_from(spec.limit.unwrap_or(MAX_CATEGORIES))
                .unwrap_or(MAX_CATEGORIES as usize),
            sort: spec.sort.unwrap_or(if is_time {
                Sort {
                    by: SortBy::Dimension,
                    dir: SortDir::Asc,
                }
            } else {
                Sort {
                    by: SortBy::Value,
                    dir: SortDir::Desc,
                }
            }),
            is_time,
            has_dimension: dimension.is_some(),
            notes,
        };

        let mut series = fold(data, &plan);
        if let Some(dimension) = dimension.filter(|d| d.grain.is_none()) {
            self.label_buckets(dimension.id, &mut series).await?;
        }
        Ok(series)
    }

    /// Refuses a filter that names an id this tenant does not hold.
    ///
    /// A `422` rather than an empty chart, and the id is resolved through the
    /// tenant's own table — so another tenant's id is indistinguishable from
    /// one that never existed, exactly as every other read here is.
    async fn check_filter_ids(&self, spec: &ChartSpec) -> Result<()> {
        for filter in &spec.filters {
            let table = match filter.id {
                FilterField::Customer => "billing_customers",
                FilterField::Pipeline => "crm_pipelines",
                FilterField::Owner => "users",
                _ => continue,
            };
            let known: Vec<String> = sqlx::query_scalar(&format!(
                "SELECT id FROM {table} WHERE tenant_id = $1 AND id = ANY($2)"
            ))
            .bind(self.tenant.as_str())
            .bind(filter.values.as_slice())
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Db)?;
            let known: BTreeSet<&str> = known.iter().map(String::as_str).collect();
            if let Some(missing) = filter
                .values
                .iter()
                .find(|value| !known.contains(value.as_str()))
            {
                return Err(invalid(format!(
                    "filters: {missing:?} is not one of this workspace's records"
                )));
            }
        }
        Ok(())
    }

    /// The two billing datasets whose money is folded in Rust.
    ///
    /// Returns the data, the currency groups that must exist even when empty,
    /// and the notes the reader has to see beside the figures.
    async fn folded_data(
        &self,
        spec: &ChartSpec,
        window: Option<(Date, Date)>,
        today: Date,
    ) -> Result<(Vec<Datum>, Vec<String>, Vec<Note>)> {
        let receivables = spec.dataset == Dataset::BillingReceivables;
        let mut predicate = Predicate::for_tenant(self.tenant.as_str(), "tenant_id");
        if receivables {
            // What is still owed: raised, not settled, and not money owed back
            // to the customer — a credit note makes nobody late.
            predicate.and("status = 'issued' AND is_credit_note = false".to_owned());
        } else {
            // Only documents that stand. A draft was never raised and a void
            // one was cancelled; neither charged anybody anything.
            predicate.and("status IN ('issued', 'paid')".to_owned());
        }
        self.narrow(&mut predicate, spec, window)?;

        let rows: Vec<DocumentRow> = bind_all(
            sqlx::query_as(&format!(
                "SELECT id, customer_id, currency, status, issue_date, due_date, \
                 fx_base_currency, fx_rate_micro, fx_rate_date \
                 FROM billing_invoices WHERE {} LIMIT {}",
                predicate.sql(),
                MAX_SCANNED_ROWS + 1
            )),
            &predicate.bounds,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if rows.len() > MAX_SCANNED_ROWS {
            return Err(too_wide("documents"));
        }

        // The VAT-rate filter narrows the *lines*, not the documents: an
        // invoice with a 21 % line and a 0 % line is one document with two rate
        // subtotals, and asking for 21 % asks for that part of it. Rounding is
        // per rate subtotal per document, so dropping the other lines first
        // gives exactly the same cents as summing the matching subtotals.
        let mut lines = predicate.clone_bounds();
        let rate_filter = spec
            .filters
            .iter()
            .find(|f| f.id == FilterField::VatRate)
            .map(|f| {
                let rates: Vec<i32> = f
                    .values
                    .iter()
                    .filter_map(|v| v.parse::<i32>().ok())
                    .collect();
                (f.op, rates)
            });
        let mut line_clause = String::new();
        if let Some((op, rates)) = &rate_filter {
            let at = lines.bind(Bound::Ints(rates.clone()));
            line_clause = match op {
                FilterOp::In => format!(" AND vat_rate_bp = ANY(${at})"),
                FilterOp::NotIn => format!(" AND vat_rate_bp <> ALL(${at})"),
            };
        }
        let figures: Vec<FiguresRow> = bind_all(
            sqlx::query_as(&format!(
                "SELECT invoice_id AS doc_id, qty_milli, unit_price_cents, vat_rate_bp \
                 FROM billing_invoice_lines \
                 WHERE tenant_id = $1{line_clause} AND invoice_id IN ( \
                     SELECT id FROM billing_invoices WHERE {}) LIMIT {}",
                predicate.sql(),
                MAX_SCANNED_ROWS + 1
            )),
            &lines.bounds,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if figures.len() > MAX_SCANNED_ROWS {
            return Err(too_wide("document lines"));
        }
        let mut by_document = group_figures(figures);

        // What has been received against each document — what makes a
        // receivable a receivable.
        let mut settled: HashMap<String, i64> = HashMap::new();
        if receivables {
            let paid: Vec<(String, Option<i64>)> = bind_all(
                sqlx::query_as(&format!(
                    "SELECT invoice_id, sum(amount_cents)::bigint FROM billing_payments \
                     WHERE tenant_id = $1 AND invoice_id IN ( \
                         SELECT id FROM billing_invoices WHERE {}) GROUP BY invoice_id",
                    predicate.sql()
                )),
                &predicate.bounds,
            )
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Db)?;
            settled = paid
                .into_iter()
                .map(|(id, sum)| (id, sum.unwrap_or(0)))
                .collect();
        }

        // The accounting currency is what the report is asked to express itself
        // in *today*; a document whose snapshot names another one is reported
        // as unconverted rather than re-crossed at a rate that applied to
        // nothing.
        let base = self.billing_base_currency().await?;
        let money = spec.measure.agg == Aggregate::Sum;
        let by_rate = spec.dimension.is_some_and(|d| d.id == Dimension::VatRate);
        let mut data = Vec::with_capacity(rows.len());
        let mut unconverted = 0i64;
        for row in &rows {
            let figures = by_document.remove(&row.id).unwrap_or_default();
            if rate_filter.is_some() && figures.is_empty() {
                // Nothing of this document was charged at the asked-for rate,
                // so the document is not part of the answer at all.
                continue;
            }
            let own = totals(&figures);
            // A settled document is not a receivable, whether the chart is
            // counting them or adding them up.
            let outstanding = receivables.then(|| {
                let paid = settled.get(&row.id).copied().unwrap_or(0);
                Settlement::of(own.gross_cents, paid).outstanding_cents
            });
            if outstanding.is_some_and(|owed| owed <= 0) {
                continue;
            }
            let bucket = self.document_bucket(spec, row, today);
            if !money {
                data.push(Datum {
                    group: ALL_GROUP.to_owned(),
                    bucket,
                    value: 0,
                    rows: 1,
                    won: 0,
                });
                continue;
            }
            // Restated at the document's **own** frozen rate — the rate of its
            // tax point — never at today's, so re-running last year's quarter
            // answers last year's figure. A document that carries no usable
            // snapshot is counted apart rather than crossed at a guess.
            let Some(restated) = restated_into(&base, row.fx().as_ref(), &own) else {
                unconverted += 1;
                continue;
            };
            if let Some(owed) = outstanding {
                let rate = row.fx().map(|fx| fx.rate_micro).unwrap_or_default();
                let Some(value) = convert_cents(owed, rate) else {
                    unconverted += 1;
                    continue;
                };
                data.push(Datum {
                    group: base.clone(),
                    bucket,
                    value,
                    rows: 1,
                    won: 0,
                });
                continue;
            }
            if by_rate {
                // One contribution per rate subtotal: the money of a two-rate
                // invoice does split per rate, exactly and legitimately.
                for subtotal in &restated.vat_by_rate {
                    data.push(Datum {
                        group: base.clone(),
                        bucket: rate_bucket(subtotal.rate_bp),
                        value: measure_of_subtotal(spec.measure.id, subtotal),
                        rows: 1,
                        won: 0,
                    });
                }
                continue;
            }
            data.push(Datum {
                group: base.clone(),
                bucket,
                value: measure_of_totals(spec.measure.id, &restated),
                rows: 1,
                won: 0,
            });
        }

        let groups = if money {
            vec![base]
        } else {
            vec![ALL_GROUP.to_owned()]
        };
        let notes = if unconverted > 0 {
            vec![Note {
                code: UNCONVERTED_DOCUMENTS,
                count: unconverted,
            }]
        } else {
            Vec::new()
        };
        Ok((data, groups, notes))
    }

    /// Which bucket a document falls in, for the breakdowns a folded dataset
    /// answers in Rust.
    fn document_bucket(&self, spec: &ChartSpec, row: &DocumentRow, today: Date) -> String {
        let Some(dimension) = spec.dimension else {
            return TOTAL_BUCKET.to_owned();
        };
        match (dimension.id, dimension.grain) {
            (Dimension::IssueDate, Some(grain)) => row
                .issue_date
                .map(|d| bucket_key(d, grain))
                .unwrap_or_else(|| TOTAL_BUCKET.to_owned()),
            (Dimension::DueDate, Some(grain)) => row
                .due_date
                .map(|d| bucket_key(d, grain))
                .unwrap_or_else(|| TOTAL_BUCKET.to_owned()),
            (Dimension::Customer, _) => row.customer_id.clone(),
            (Dimension::Currency, _) => row.currency.clone(),
            (Dimension::Status, _) => row.status.clone(),
            (Dimension::AgeBucket, _) => age_bucket(row.due_date, today).to_owned(),
            // The VAT-rate breakdown is handled per subtotal by the caller, and
            // nothing else is a dimension of these datasets.
            _ => TOTAL_BUCKET.to_owned(),
        }
    }

    /// The two datasets Postgres groups and sums itself.
    async fn grouped_data(
        &self,
        spec: &ChartSpec,
        window: Option<(Date, Date)>,
    ) -> Result<(Vec<Datum>, Vec<String>, Vec<Note>)> {
        let deals = spec.dataset == Dataset::CrmDeals;
        let mut predicate = Predicate::for_tenant(
            self.tenant.as_str(),
            if deals { "d.tenant_id" } else { "p.tenant_id" },
        );
        if deals && spec.measure.id == Measure::WinRate {
            // A win rate is a fact about deals that closed; an open deal is in
            // neither the numerator nor the denominator.
            predicate.and("d.outcome IS NOT NULL".to_owned());
        }
        self.narrow(&mut predicate, spec, window)?;

        let bucket = bucket_expression(spec.dataset, spec.dimension.map(|d| (d.id, d.grain)));
        let from = if deals {
            "crm_deals d".to_owned()
        } else {
            "billing_payments p JOIN billing_invoices i \
             ON i.tenant_id = p.tenant_id AND i.id = p.invoice_id"
                .to_owned()
        };
        let (currency, amount, won) = if deals {
            (
                "d.currency",
                "COALESCE(SUM(d.value_cents), 0)::bigint",
                "count(*) FILTER (WHERE d.outcome = 'won')::bigint",
            )
        } else {
            (
                "i.currency",
                "COALESCE(SUM(p.amount_cents), 0)::bigint",
                "0::bigint",
            )
        };
        let rows: Vec<GroupedRow> = bind_all(
            sqlx::query_as(&format!(
                "SELECT {currency}, {bucket} AS bucket, count(*)::bigint, {amount}, {won} \
                 FROM {from} WHERE {} GROUP BY 1, 2 LIMIT {}",
                predicate.sql(),
                MAX_GROUPS + 1
            )),
            &predicate.bounds,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if rows.len() > MAX_GROUPS {
            return Err(invalid(format!(
                "dimension: this breakdown would produce more than {MAX_GROUPS} buckets; \
                 narrow the period or add a filter"
            )));
        }

        // Money is never converted here, so it stays in its own currency; a
        // count and a ratio are currency-free and answer one series.
        let by_currency = insight_catalog::dataset(spec.dataset)
            .measure(spec.measure.id)
            .is_some_and(|m| m.unit == Unit::Money);
        let mut currencies = BTreeSet::new();
        let mut data = Vec::with_capacity(rows.len());
        for (currency, bucket, count, amount, won) in rows {
            // A row whose own date is not set (a deal nobody dated) buckets
            // nowhere rather than into a bucket it does not belong to.
            let Some(bucket) = bucket else { continue };
            let group = if by_currency {
                currencies.insert(currency.clone());
                currency
            } else {
                ALL_GROUP.to_owned()
            };
            data.push(Datum {
                group,
                bucket,
                value: amount,
                rows: count,
                won,
            });
        }
        let groups = if by_currency {
            currencies.into_iter().collect()
        } else {
            vec![ALL_GROUP.to_owned()]
        };
        Ok((data, groups, Vec::new()))
    }

    /// Adds the period and the spec's SQL filters to a predicate.
    fn narrow(
        &self,
        predicate: &mut Predicate,
        spec: &ChartSpec,
        window: Option<(Date, Date)>,
    ) -> Result<()> {
        if let Some((from, to)) = window {
            match period_column(spec.dataset, spec.period_dimension())? {
                PeriodColumn::Day(column) => {
                    let a = predicate.bind(Bound::Day(from));
                    let b = predicate.bind(Bound::Day(to));
                    predicate.and(format!("{column} >= ${a} AND {column} <= ${b}"));
                }
                PeriodColumn::Stamp(column) => {
                    let (start, end) = instants(from, to)?;
                    let a = predicate.bind(Bound::Stamp(start));
                    let b = predicate.bind(Bound::Stamp(end));
                    predicate.and(format!("{column} >= ${a} AND {column} < ${b}"));
                }
            }
        }
        for filter in &spec.filters {
            let Some(column) = filter_column(spec.dataset, filter.id) else {
                continue;
            };
            let at = predicate.bind(Bound::Texts(filter.values.clone()));
            predicate.and(match filter.op {
                FilterOp::In => format!("{column} = ANY(${at})"),
                FilterOp::NotIn => format!("{column} <> ALL(${at})"),
            });
        }
        Ok(())
    }

    /// Names the category buckets that survived the fold — the tenant's own
    /// words where the value is theirs, a catalog id where it is ours.
    async fn label_buckets(&self, dimension: Dimension, series: &mut Series) -> Result<()> {
        let keys: Vec<String> = series
            .groups
            .iter()
            .flat_map(|g| g.points.iter())
            .filter(|p| p.label.is_none())
            .map(|p| p.bucket.clone())
            .collect();
        let names = match dimension {
            Dimension::Customer => {
                self.names("SELECT id, name FROM billing_customers", &keys)
                    .await?
            }
            Dimension::Stage => self.names("SELECT id, name FROM crm_stages", &keys).await?,
            Dimension::Owner => self.names("SELECT id, email FROM users", &keys).await?,
            _ => HashMap::new(),
        };
        for group in &mut series.groups {
            for point in &mut group.points {
                if point.label.is_some() {
                    continue;
                }
                point.label = Some(label_for(dimension, &point.bucket, &names));
            }
        }
        Ok(())
    }

    /// Reads `id → name` for the given ids, tenant-scoped like every other
    /// statement here.
    async fn names(&self, select: &str, ids: &[String]) -> Result<HashMap<String, String>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<(String, String)> =
            sqlx::query_as(&format!("{select} WHERE tenant_id = $1 AND id = ANY($2)"))
                .bind(self.tenant.as_str())
                .bind(ids)
                .fetch_all(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        Ok(rows.into_iter().collect())
    }
}

/// Whether a spec's money is restated into one accounting currency (and so has
/// a currency for the whole series) or stays in the currency it was recorded
/// in (and so is one series per currency).
fn restates(spec: &ChartSpec) -> bool {
    matches!(
        spec.dataset,
        Dataset::BillingDocuments | Dataset::BillingReceivables
    ) && spec.measure.agg == Aggregate::Sum
}

/// The figure a measure takes off a document's totals.
fn measure_of_totals(measure: Measure, totals: &Totals) -> i64 {
    match measure {
        Measure::Net => totals.net_cents,
        Measure::Vat => totals.vat_cents,
        Measure::Gross => totals.gross_cents,
        // Outstanding is computed by the caller (it needs the payments), and
        // the rest are other datasets' measures.
        _ => 0,
    }
}

/// The figure a measure takes off one VAT-rate subtotal.
fn measure_of_subtotal(measure: Measure, subtotal: &crate::billing_totals::VatSubtotal) -> i64 {
    match measure {
        Measure::Net => subtotal.net_cents,
        Measure::Vat => subtotal.vat_cents,
        Measure::Gross => subtotal.net_cents.saturating_add(subtotal.vat_cents),
        _ => 0,
    }
}

/// What a category bucket is called.
fn label_for(dimension: Dimension, key: &str, names: &HashMap<String, String>) -> Label {
    let named = |key: &str| match names.get(key) {
        Some(name) => Label::Raw {
            text: name.to_owned(),
        },
        None => Label::Catalog { id: UNKNOWN_LABEL },
    };
    let own_words = |text: &str| {
        if text.trim().is_empty() {
            Label::Catalog { id: NONE_LABEL }
        } else {
            Label::Raw {
                text: text.to_owned(),
            }
        }
    };
    match dimension {
        Dimension::Customer | Dimension::Stage | Dimension::Owner => named(key),
        Dimension::Currency => Label::Raw {
            text: key.to_owned(),
        },
        Dimension::Method | Dimension::Source => own_words(key),
        Dimension::VatRate => match key.parse::<i32>() {
            Ok(bp) => Label::RateBp { bp },
            Err(_) => Label::Catalog { id: UNKNOWN_LABEL },
        },
        Dimension::Status => match key {
            "issued" => Label::Catalog {
                id: "status.issued",
            },
            "paid" => Label::Catalog { id: "status.paid" },
            _ => Label::Catalog { id: UNKNOWN_LABEL },
        },
        Dimension::Outcome => match key {
            "won" => Label::Catalog { id: "outcome.won" },
            "lost" => Label::Catalog { id: "outcome.lost" },
            "open" => Label::Catalog { id: "outcome.open" },
            _ => Label::Catalog { id: UNKNOWN_LABEL },
        },
        Dimension::AgeBucket => match key {
            "age.not_due" => Label::Catalog { id: "age.not_due" },
            "age.0_30" => Label::Catalog { id: "age.0_30" },
            "age.31_60" => Label::Catalog { id: "age.31_60" },
            "age.61_90" => Label::Catalog { id: "age.61_90" },
            "age.90_plus" => Label::Catalog { id: "age.90_plus" },
            _ => Label::Catalog { id: UNKNOWN_LABEL },
        },
        // Time dimensions never reach here: their keys are ISO strings the
        // client formats itself.
        _ => Label::Catalog { id: UNKNOWN_LABEL },
    }
}

/// The refusal a period that would scan too much gets: it names the cap and
/// asks for the fix, rather than truncating an answer somebody would file.
fn too_wide(what: &str) -> StoreError {
    invalid(format!(
        "period: this chart would read more than {MAX_SCANNED_ROWS} {what}; \
         narrow the period or add a filter"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn last_n_covers_whole_buckets_ending_with_the_one_today_is_in() {
        let today = day(2026, Month::August, 7);
        let (from, to) = window(
            &Period::LastN {
                n: 12,
                grain: Grain::Month,
            },
            today,
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
        .unwrap_or_else(|| panic!("bounded"));
        assert_eq!(from, day(2025, Month::September, 1));
        assert_eq!(to, day(2026, Month::August, 31), "the current month, whole");

        // One bucket is this bucket.
        let (from, to) = window(
            &Period::LastN {
                n: 1,
                grain: Grain::Quarter,
            },
            today,
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
        .unwrap_or_else(|| panic!("bounded"));
        assert_eq!(from, day(2026, Month::July, 1));
        assert_eq!(to, day(2026, Month::September, 30));
    }

    #[test]
    fn a_range_is_two_inclusive_days_and_all_is_unbounded() {
        let range = window(
            &Period::Range {
                from: "2026-01-01".to_owned(),
                to: "2026-03-31".to_owned(),
            },
            day(2026, Month::August, 7),
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(
            range,
            Some((day(2026, Month::January, 1), day(2026, Month::March, 31)))
        );
        assert_eq!(
            window(&Period::All, day(2026, Month::August, 7)).unwrap_or_else(|e| panic!("{e:?}")),
            None,
            "a spec cannot know how much history a tenant holds"
        );
    }

    #[test]
    fn every_dataset_binds_the_tenant_as_the_first_parameter() {
        // The structural half of the tenancy story: a dataset added without a
        // tenant predicate cannot get past this.
        for column in ["tenant_id", "d.tenant_id", "p.tenant_id"] {
            let predicate = Predicate::for_tenant("t-1", column);
            assert_eq!(predicate.sql(), format!("{column} = $1"));
            assert!(matches!(predicate.bounds.first(), Some(Bound::Text(t)) if t == "t-1"));
        }
    }

    #[test]
    fn a_filter_value_is_bound_and_never_spliced() {
        let mut predicate = Predicate::for_tenant("t-1", "tenant_id");
        let at = predicate.bind(Bound::Texts(vec!["1' OR '1'='1".to_owned()]));
        predicate.and(format!("customer_id = ANY(${at})"));
        assert_eq!(predicate.sql(), "tenant_id = $1 AND customer_id = ANY($2)");
        assert!(
            !predicate.sql().contains("OR"),
            "the value is a parameter, so it is nowhere in the statement text"
        );
    }

    #[test]
    fn no_bucket_expression_carries_anything_but_our_own_words() {
        // Every expression a dimension compiles to is built from `&'static str`
        // fragments; nothing in a spec is spliced, so nothing here can be.
        for dataset in [Dataset::BillingPayments, Dataset::CrmDeals] {
            for dimension in insight_catalog::dataset(dataset).dimensions {
                let grain = match dimension.kind {
                    crate::insight_catalog::DimensionKind::Time(grains) => grains.first().copied(),
                    crate::insight_catalog::DimensionKind::Category => None,
                };
                let sql = bucket_expression(dataset, Some((dimension.dimension, grain)));
                assert!(!sql.is_empty());
                assert!(
                    !sql.contains(';') && !sql.contains("--"),
                    "{dimension:?} compiled to {sql:?}"
                );
            }
        }
        assert_eq!(
            bucket_expression(Dataset::CrmDeals, None),
            format!("'{TOTAL_BUCKET}'::text")
        );
    }

    #[test]
    fn an_age_band_separates_what_is_late_from_what_is_merely_owed() {
        let today = day(2026, Month::August, 7);
        assert_eq!(
            age_bucket(Some(day(2026, Month::September, 1)), today),
            "age.not_due"
        );
        assert_eq!(age_bucket(Some(today), today), "age.not_due");
        assert_eq!(
            age_bucket(Some(day(2026, Month::August, 6)), today),
            "age.0_30"
        );
        assert_eq!(
            age_bucket(Some(day(2026, Month::July, 8)), today),
            "age.0_30"
        );
        assert_eq!(
            age_bucket(Some(day(2026, Month::July, 7)), today),
            "age.31_60"
        );
        assert_eq!(
            age_bucket(Some(day(2026, Month::June, 8)), today),
            "age.31_60"
        );
        assert_eq!(
            age_bucket(Some(day(2026, Month::June, 7)), today),
            "age.61_90"
        );
        assert_eq!(
            age_bucket(Some(day(2026, Month::May, 9)), today),
            "age.61_90"
        );
        assert_eq!(
            age_bucket(Some(day(2026, Month::May, 8)), today),
            "age.90_plus"
        );
        assert_eq!(age_bucket(None, today), "age.not_due");
    }

    #[test]
    fn only_the_billing_documents_are_restated_into_one_currency() {
        let money = |dataset: &str, measure: &str, agg: &str| {
            let spec = ChartSpec::from_value(serde_json::json!({
                "schema_version": 1,
                "dataset": dataset,
                "measure": { "id": measure, "agg": agg },
                "period": { "kind": "all" },
                "viz": "number"
            }))
            .unwrap_or_else(|e| panic!("{e}"));
            restates(&spec)
        };
        assert!(money("billing.documents", "net", "sum"));
        assert!(money("billing.receivables", "outstanding", "sum"));
        // A deal is a forecast with no tax point and a payment's value date is
        // not the document's: neither has an honest rate, so neither converts.
        assert!(!money("crm.deals", "value", "sum"));
        assert!(!money("billing.payments", "amount", "sum"));
        // A count is not money at all.
        assert!(!money("billing.documents", "count", "count"));
    }
}
