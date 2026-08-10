//! alo Finance (ADR 0035, wave B4.11c): **aged receivables and payables** — who
//! owes us, who we owe, and for how long, on one day
//! (`docs/design/finance.md`, "The four reports").
//!
//! It is the one report of the four that reads **documents** rather than the
//! journal, and that is a decision rather than an accident: ageing is a property
//! of a document. A receivable account holds one number; only the invoices
//! behind it know that six hundred euro of it has been owed since March and the
//! rest since last week. So this fold reads [`crate::billing_invoices`] with
//! [`crate::billing_payments`] against them for the receivable side, and the
//! approved bills of [`crate::billing_bills`] for the payable side, and buckets
//! each open document by its own due date.
//!
//! Five things a reader should know before reading a figure this module returns.
//!
//! **The day is a real boundary, in both directions.** A document counts when it
//! was issued on or before the date asked for, and the money against it counts
//! when it arrived on or before that date. Re-running last quarter's ageing next
//! year therefore answers last quarter — a payment keyed in afterwards for a day
//! inside the period moves the old report, which is right, because it moves the
//! debt too.
//!
//! **Only documents that stand are read.** `issued` and `paid` on the receivable
//! side (a document that is settled today may well have been owed on the date
//! asked for), `approved` on the payable side. A draft was never raised, a void
//! one was cancelled, a received-but-undecided bill is an intention rather than a
//! liability — the same line `docs/design/finance.md` draws for the journal.
//!
//! **Credit notes are included, negatively.** They carry negated lines already,
//! so a customer's group nets to what they actually owe; each document says
//! whether it is one, so a screen can show a credit apart from a debt.
//!
//! **A document with nothing open is not a row.** Zero is the state of almost
//! every document a business has ever raised, and an aged list that printed them
//! would bury the eight that matter. An **over**paid document is a row, with a
//! negative amount, because money we are holding for a customer is a fact a
//! bookkeeper needs on exactly this report.
//!
//! **The buckets are added in the accounting currency**, each document crossed
//! at the rate frozen on it when it was issued ([`crate::billing_fx`]) — never
//! at today's rate, and never at a guessed one. A document that cannot be
//! restated honestly (no snapshot, or one taken against a currency the tenant no
//! longer keeps books in) is **in no bucket** and is counted in
//! [`AgedReport::unconverted_count`], which is what stops a total from quietly
//! being part invention. Every document also carries its own currency and its
//! own open amount, so the paperwork behind a converted figure stays readable.
//!
//! Tenancy is structural, as everywhere in this crate: every statement carries
//! `tenant_id` from the handle — including the join to the customer that gives a
//! group its name — so another tenant's documents are never read into a total
//! rather than filtered out of one.
//!
//! **What this report deliberately does not do.** It does not tie itself to the
//! ledger's `ar` and `ap` balances (P6). That tie is real and is stated in the
//! design note, but it can only be asserted once issuing a document books it,
//! which is not wired yet (`docs/autonomy/STATE.md`, the standing flag) — and a
//! test that asserted it today would be asserting that both sides are empty.

use time::Date;

use crate::account::AccountStore;
use crate::billing_fx::{FxSnapshot, restated_open_cents};
use crate::billing_line::{FiguresRow, group_figures};
use crate::billing_totals::totals;
use crate::error::{Result, StoreError};

/// Which side of the ledger an ageing is asked for.
///
/// Two reports rather than one with both sides: they read different tables,
/// they are chased by different people, and a file holding both would have to
/// invent a column saying which of the two a row belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgedSide {
    /// What customers owe us — issued invoices, less the money that has
    /// arrived against them.
    Receivable,
    /// What we owe suppliers — approved bills.
    Payable,
}

impl AgedSide {
    /// The value this side is asked for and reported as.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Receivable => "receivable",
            Self::Payable => "payable",
        }
    }

    /// The side a stated value names, or `None` when it is neither.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "receivable" => Some(Self::Receivable),
            "payable" => Some(Self::Payable),
            _ => None,
        }
    }
}

/// How late a document is, in the bands every aged listing is read in.
///
/// Thirty-day bands from the **due** date, not the issue date: a customer on
/// sixty-day terms is not late on day thirty-one, and a report that said so
/// would have people chasing debts that are not yet debts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgedBucket {
    /// Not yet due on the date asked for.
    Current,
    /// One to thirty days past due.
    Days1To30,
    /// Thirty-one to sixty days past due.
    Days31To60,
    /// Sixty-one to ninety days past due.
    Days61To90,
    /// More than ninety days past due — the band that is a write-off
    /// conversation rather than a reminder.
    Days90Plus,
}

impl AgedBucket {
    /// The band `days_overdue` falls in. Zero (and, by
    /// [`OpenDocument::days_overdue`]'s clamp, anything not yet due) is
    /// [`Self::Current`].
    #[must_use]
    pub fn of(days_overdue: i64) -> Self {
        match days_overdue {
            i64::MIN..=0 => Self::Current,
            1..=30 => Self::Days1To30,
            31..=60 => Self::Days31To60,
            61..=90 => Self::Days61To90,
            _ => Self::Days90Plus,
        }
    }

    /// The value this band is reported as, on the wire and as a CSV column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Days1To30 => "d1_30",
            Self::Days31To60 => "d31_60",
            Self::Days61To90 => "d61_90",
            Self::Days90Plus => "d90_plus",
        }
    }
}

/// The five bands and what they add up to, in the accounting currency.
///
/// The total is carried rather than left to the reader because it is the figure
/// a screen prints largest, and summing five fields in three surfaces is three
/// chances to sum four of them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgedBuckets {
    /// Not yet due, in cents.
    pub current_cents: i64,
    /// One to thirty days past due, in cents.
    pub days_1_30_cents: i64,
    /// Thirty-one to sixty days past due, in cents.
    pub days_31_60_cents: i64,
    /// Sixty-one to ninety days past due, in cents.
    pub days_61_90_cents: i64,
    /// More than ninety days past due, in cents.
    pub days_90_plus_cents: i64,
    /// The five above, added up — what is open in total.
    pub total_cents: i64,
}

impl AgedBuckets {
    /// Adds one document's open amount into its band and into the total.
    ///
    /// Saturating, for [`crate::billing_totals`]' reason: an absurd set of
    /// documents gets an absurd figure, never a plausible wrong one, and never
    /// a panic.
    fn add(&mut self, bucket: AgedBucket, cents: i64) {
        let field = match bucket {
            AgedBucket::Current => &mut self.current_cents,
            AgedBucket::Days1To30 => &mut self.days_1_30_cents,
            AgedBucket::Days31To60 => &mut self.days_31_60_cents,
            AgedBucket::Days61To90 => &mut self.days_61_90_cents,
            AgedBucket::Days90Plus => &mut self.days_90_plus_cents,
        };
        *field = field.saturating_add(cents);
        self.total_cents = self.total_cents.saturating_add(cents);
    }

    /// The amount standing in one band — the accessor a CSV row and a tile use
    /// so neither has to match on the enum itself.
    #[must_use]
    pub fn of(&self, bucket: AgedBucket) -> i64 {
        match bucket {
            AgedBucket::Current => self.current_cents,
            AgedBucket::Days1To30 => self.days_1_30_cents,
            AgedBucket::Days31To60 => self.days_31_60_cents,
            AgedBucket::Days61To90 => self.days_61_90_cents,
            AgedBucket::Days90Plus => self.days_90_plus_cents,
        }
    }
}

/// Every band, in reading order — the one place their order is stated, so the
/// wire, the file and a screen cannot each choose their own.
pub const AGED_BUCKETS: [AgedBucket; 5] = [
    AgedBucket::Current,
    AgedBucket::Days1To30,
    AgedBucket::Days31To60,
    AgedBucket::Days61To90,
    AgedBucket::Days90Plus,
];

/// One open document on an aged list: what it is, when it was due, how late it
/// is, and what is still open on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgedDocument {
    /// The document's opaque id — an invoice id on the receivable side, a bill
    /// id on the payable one.
    pub document_id: String,
    /// The number the document carries. Never empty on the receivable side (a
    /// document is only here once it has been numbered); a supplier's own
    /// number on the payable one, which they may have left blank.
    pub number: String,
    /// The day it was issued.
    pub issue_date: Date,
    /// The day it was payable. A bill that states no due date is payable on
    /// receipt, so its issue date stands here.
    pub due_date: Date,
    /// How many days past due it is on the date asked for; `0` when it is not
    /// yet due.
    pub days_overdue: i64,
    /// The band it stands in.
    pub bucket: AgedBucket,
    /// ISO 4217 code the document itself was raised in.
    pub currency: String,
    /// What is still open on it, in cents **of its own currency**: gross less
    /// the money that had arrived by the date asked for. Negative when it was
    /// overpaid, and negative on a credit note.
    pub open_cents: i64,
    /// The same amount in the tenant's accounting currency at the rate frozen
    /// on the document, or `None` when it cannot be restated honestly — in
    /// which case this document is in no bucket and is counted as unconverted.
    pub base_open_cents: Option<i64>,
    /// Whether it is a credit note rather than an invoice or a bill.
    pub is_credit_note: bool,
}

/// One counterparty's ageing: the bands, and the documents they were built
/// from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgedParty {
    /// The customer's id on the receivable side; on the payable side the
    /// supplier's comparable key ([`crate::billing_bills::Supplier::key`]),
    /// because a bill copies its supplier rather than linking to a record.
    pub party_id: String,
    /// What they are called, as the customer record or the document says.
    pub name: String,
    /// Their bands, in the accounting currency.
    pub buckets: AgedBuckets,
    /// How many of their documents are in none of those bands because they
    /// could not be restated.
    pub unconverted_count: i64,
    /// Their open documents, by due date and then by number.
    pub documents: Vec<AgedDocument>,
}

/// What is owed, by whom, and for how long, on one day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgedReport {
    /// The day asked for, inclusive.
    pub on: Date,
    /// Which side was asked for.
    pub side: AgedSide,
    /// The accounting currency every bucket figure is in.
    pub currency: String,
    /// One group per counterparty with something open, by name.
    pub parties: Vec<AgedParty>,
    /// Every party's bands added together.
    pub buckets: AgedBuckets,
    /// How many open documents are in none of the bands because they could not
    /// be restated. **Non-zero means the totals are incomplete**, and a surface
    /// must say so rather than print them plain.
    pub unconverted_count: i64,
    /// How many open documents the report is built from, restated or not.
    pub document_count: i64,
}

/// One document as the fold receives it: the same shape whichever table it came
/// out of, which is what lets both sides share the ageing itself.
#[derive(Debug, Clone)]
struct OpenDocument {
    party_id: String,
    party_name: String,
    document_id: String,
    number: String,
    issue_date: Date,
    due_date: Date,
    currency: String,
    open_cents: i64,
    fx: Option<FxSnapshot>,
    is_credit_note: bool,
}

impl OpenDocument {
    /// How late this document is on `on` — never negative, because "due in
    /// twelve days" is not a degree of lateness and a bucket built from a
    /// negative number would be an arithmetic accident rather than a band.
    fn days_overdue(&self, on: Date) -> i64 {
        (on - self.due_date).whole_days().max(0)
    }
}

/// Ages the documents into parties and bands — pure, so every figure below is
/// unit-tested without a database.
fn age(on: Date, side: AgedSide, base: &str, documents: Vec<OpenDocument>) -> AgedReport {
    // Kept sorted by party id while it fills, then sorted by name at the end:
    // a business has a handful of parties with something open, so a sorted
    // vector beats a map and gives the grouping for free.
    let mut parties: Vec<AgedParty> = Vec::new();
    let mut whole = AgedBuckets::default();
    let mut unconverted_count = 0_i64;
    let mut document_count = 0_i64;

    for document in documents {
        // Nothing open is not a row: see the module note.
        if document.open_cents == 0 {
            continue;
        }
        document_count += 1;
        let days_overdue = document.days_overdue(on);
        let bucket = AgedBucket::of(days_overdue);
        let base_open_cents = restated_open_cents(
            base,
            &document.currency,
            document.fx.as_ref(),
            document.open_cents,
        );

        let at = match parties.binary_search_by(|party| party.party_id.cmp(&document.party_id)) {
            Ok(at) => at,
            Err(at) => {
                parties.insert(
                    at,
                    AgedParty {
                        party_id: document.party_id.clone(),
                        name: document.party_name.clone(),
                        buckets: AgedBuckets::default(),
                        unconverted_count: 0,
                        documents: Vec::new(),
                    },
                );
                at
            }
        };
        let party = &mut parties[at];
        match base_open_cents {
            Some(cents) => {
                party.buckets.add(bucket, cents);
                whole.add(bucket, cents);
            }
            None => {
                party.unconverted_count += 1;
                unconverted_count += 1;
            }
        }
        party.documents.push(AgedDocument {
            document_id: document.document_id,
            number: document.number,
            issue_date: document.issue_date,
            due_date: document.due_date,
            days_overdue,
            bucket,
            currency: document.currency,
            open_cents: document.open_cents,
            base_open_cents,
            is_credit_note: document.is_credit_note,
        });
    }

    for party in &mut parties {
        // The oldest debt first inside a group, which is the order it is chased
        // in; the number breaks a tie so the report is stable between runs.
        party
            .documents
            .sort_by(|a, b| a.due_date.cmp(&b.due_date).then(a.number.cmp(&b.number)));
    }
    // By name for a reader, by id for a tie: two customers may share a name.
    parties.sort_by(|a, b| a.name.cmp(&b.name).then(a.party_id.cmp(&b.party_id)));

    AgedReport {
        on,
        side,
        currency: base.to_owned(),
        parties,
        buckets: whole,
        unconverted_count,
        document_count,
    }
}

/// The receivable side's header row, in `SELECT` order.
type ReceivableRow = (
    String,         // invoice id
    Option<String>, // number
    Date,           // issue date
    Option<Date>,   // due date
    String,         // currency
    bool,           // is credit note
    Option<String>, // fx base currency
    Option<i64>,    // fx rate, micro-units
    Option<Date>,   // fx rate date
    String,         // customer id
    String,         // customer name
);

/// The payable side's header row, in `SELECT` order.
type PayableRow = (
    String,       // bill id
    String,       // number, as the supplier wrote it
    Date,         // issue date
    Option<Date>, // due date
    String,       // currency
    String,       // type code: 381 is a credit note
    i64,          // payable amount, in ledger direction
    String,       // supplier name
    String,       // supplier VAT id
);

impl AccountStore {
    /// **The aged receivables or payables** of this tenant on one day: every
    /// counterparty with something open, their documents, and the five bands
    /// those documents fall in.
    ///
    /// Three statements on the receivable side whatever the length of the list
    /// (the documents, their lines, the money against them) and one on the
    /// payable side. Each document's gross is computed by
    /// [`crate::billing_totals`] — the same code the document itself, its PDF
    /// and its e-invoice are printed from — so an aged listing and the paperwork
    /// behind it can never disagree about a cent.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure, or when a stored row holds a shape this
    /// build cannot read.
    pub async fn fin_aged(&self, on: Date, side: AgedSide) -> Result<AgedReport> {
        let base = self.billing_base_currency().await?;
        let documents = match side {
            AgedSide::Receivable => self.open_receivables(on).await?,
            AgedSide::Payable => self.open_payables(on).await?,
        };
        Ok(age(on, side, &base, documents))
    }

    /// The invoices that stood on `on`, each with what was still open on it
    /// then.
    async fn open_receivables(&self, on: Date) -> Result<Vec<OpenDocument>> {
        // The one predicate all three reads state — down to the table alias, so
        // the lines and the payments fetched are exactly those of the documents
        // counted and no second spelling of "issued by then" can drift from the
        // first.
        const STOOD: &str = "i.tenant_id = $1 AND i.status IN ('issued', 'paid') AND i.issue_date IS NOT NULL \
             AND i.issue_date <= $2";

        let headers: Vec<ReceivableRow> = sqlx::query_as(&format!(
            "SELECT i.id, i.number, i.issue_date, i.due_date, i.currency, i.is_credit_note, \
                 i.fx_base_currency, i.fx_rate_micro, i.fx_rate_date, i.customer_id, c.name \
             FROM billing_invoices i \
             JOIN billing_customers c ON c.tenant_id = i.tenant_id AND c.id = i.customer_id \
             WHERE {STOOD}"
        ))
        .bind(self.tenant.as_str())
        .bind(on)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let figures = sqlx::query_as::<_, FiguresRow>(&format!(
            "SELECT invoice_id AS doc_id, qty_milli, unit_price_cents, vat_rate_bp \
             FROM billing_invoice_lines \
             WHERE tenant_id = $1 AND invoice_id IN ( \
                 SELECT i.id FROM billing_invoices i WHERE {STOOD})"
        ))
        .bind(self.tenant.as_str())
        .bind(on)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut by_document = group_figures(figures);

        // Only the money that had arrived by the date asked for: an aged list
        // is a photograph of a day, and a payment keyed in later for a later
        // day did not settle anything on it.
        let paid: Vec<(String, Option<i64>)> = sqlx::query_as(&format!(
            "SELECT invoice_id, sum(amount_cents)::bigint FROM billing_payments \
             WHERE tenant_id = $1 AND paid_on <= $2 AND invoice_id IN ( \
                 SELECT i.id FROM billing_invoices i WHERE {STOOD}) \
             GROUP BY invoice_id"
        ))
        .bind(self.tenant.as_str())
        .bind(on)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut by_paid: std::collections::HashMap<String, i64> = paid
            .into_iter()
            .map(|(id, sum)| (id, sum.unwrap_or(0)))
            .collect();

        Ok(headers
            .into_iter()
            .map(|row| {
                let (
                    id,
                    number,
                    issue_date,
                    due_date,
                    currency,
                    is_credit_note,
                    fx_base_currency,
                    fx_rate_micro,
                    fx_rate_date,
                    customer_id,
                    customer_name,
                ) = row;
                let gross = totals(&by_document.remove(&id).unwrap_or_default()).gross_cents;
                let open_cents = gross.saturating_sub(by_paid.remove(&id).unwrap_or(0));
                OpenDocument {
                    party_id: customer_id,
                    party_name: customer_name,
                    // A document that stands always carries a number; the
                    // fallback is the empty string rather than a panic, because
                    // a report must print what is there.
                    number: number.unwrap_or_default(),
                    issue_date,
                    // Every issued document is stamped with a due date; one from
                    // before that was true is payable on issue.
                    due_date: due_date.unwrap_or(issue_date),
                    currency,
                    open_cents,
                    // All three columns or none: the table constrains them to
                    // move together, so a partial snapshot is not a state this
                    // can be read out of.
                    fx: fx_base_currency.zip(fx_rate_micro).zip(fx_rate_date).map(
                        |((base_currency, rate_micro), rate_date)| FxSnapshot {
                            base_currency,
                            rate_micro,
                            rate_date,
                        },
                    ),
                    is_credit_note,
                    document_id: id,
                }
            })
            .collect())
    }

    /// The approved bills issued by `on`, each with what it asks to be paid.
    ///
    /// A bill carries no payment rows of its own: money leaves through the bank,
    /// and a SEPA file is an instruction rather than a payment
    /// ([`crate::billing_bills::Bill::exported_at`]). Until a paid bill can be
    /// marked settled, a payable ages from the day it was due and stays open —
    /// which is honest about what alo knows, and is recorded as such in
    /// `docs/design/finance.md`.
    async fn open_payables(&self, on: Date) -> Result<Vec<OpenDocument>> {
        let rows: Vec<PayableRow> = sqlx::query_as(
            "SELECT id, number, issue_date, due_date, currency, type_code, payable_cents, \
                 supplier_name, supplier_vat_id \
             FROM billing_bills \
             WHERE tenant_id = $1 AND status = 'approved' AND issue_date <= $2",
        )
        .bind(self.tenant.as_str())
        .bind(on)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let (
                    id,
                    number,
                    issue_date,
                    due_date,
                    currency,
                    type_code,
                    payable_cents,
                    supplier_name,
                    supplier_vat_id,
                ) = row;
                let supplier = crate::billing_bills::Supplier {
                    name: supplier_name.clone(),
                    vat_id: supplier_vat_id,
                    ..Default::default()
                };
                OpenDocument {
                    party_id: supplier.key(),
                    party_name: supplier_name,
                    document_id: id,
                    number,
                    issue_date,
                    // BT-9 is optional; a document that states no due date is
                    // payable on receipt, which is the strict reading of
                    // EN 16931 and the one that ages it soonest.
                    due_date: due_date.unwrap_or(issue_date),
                    currency,
                    open_cents: payable_cents,
                    // A bill carries no snapshot: somebody else's system wrote
                    // it. It is therefore restatable exactly when it is already
                    // in the currency the books are kept in, which
                    // [`restated_open_cents`] decides from the currency itself.
                    fx: None,
                    is_credit_note: type_code == "381",
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_fx::IDENTITY_RATE_MICRO;
    use time::Month;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    /// The day every report in this suite stands on.
    fn today() -> Date {
        day(2026, Month::August, 10)
    }

    /// A euro document of `party`, due on `due`, with `open_cents` still open.
    fn owed(party: &str, number: &str, due: Date, open_cents: i64) -> OpenDocument {
        OpenDocument {
            party_id: format!("cus-{party}"),
            party_name: party.to_owned(),
            document_id: format!("inv-{number}"),
            number: number.to_owned(),
            issue_date: due - time::Duration::days(14),
            due_date: due,
            currency: "EUR".to_owned(),
            open_cents,
            fx: Some(FxSnapshot::identity("EUR", due)),
            is_credit_note: false,
        }
    }

    /// One customer, one document per band, plus one not yet due.
    fn a_ladder() -> AgedReport {
        let on = today();
        age(
            on,
            AgedSide::Receivable,
            "EUR",
            vec![
                owed(
                    "Wave",
                    "INV-2026-00005",
                    on + time::Duration::days(7),
                    10_000,
                ),
                owed(
                    "Wave",
                    "INV-2026-00004",
                    on - time::Duration::days(1),
                    20_000,
                ),
                owed(
                    "Wave",
                    "INV-2026-00003",
                    on - time::Duration::days(30),
                    30_000,
                ),
                owed(
                    "Wave",
                    "INV-2026-00002",
                    on - time::Duration::days(31),
                    40_000,
                ),
                owed(
                    "Wave",
                    "INV-2026-00001",
                    on - time::Duration::days(61),
                    50_000,
                ),
                owed(
                    "Wave",
                    "INV-2025-00009",
                    on - time::Duration::days(91),
                    60_000,
                ),
            ],
        )
    }

    #[test]
    fn every_band_holds_what_its_edges_say_and_the_edges_are_the_due_date() {
        let report = a_ladder();
        let party = &report.parties[0];
        assert_eq!(party.buckets.current_cents, 10_000, "due in a week");
        assert_eq!(party.buckets.days_1_30_cents, 50_000, "1 day and 30 days");
        assert_eq!(party.buckets.days_31_60_cents, 40_000);
        assert_eq!(party.buckets.days_61_90_cents, 50_000);
        assert_eq!(party.buckets.days_90_plus_cents, 60_000);
        assert_eq!(party.buckets.total_cents, 210_000);
        assert_eq!(
            report.buckets, party.buckets,
            "one party is the whole report"
        );
        assert_eq!(report.document_count, 6);
        assert_eq!(report.unconverted_count, 0);
        assert_eq!(report.currency, "EUR");
        assert_eq!(report.side, AgedSide::Receivable);
        assert_eq!(report.on, today());
    }

    #[test]
    fn a_band_is_chosen_by_the_day_and_the_boundaries_are_exact() {
        assert_eq!(AgedBucket::of(-9), AgedBucket::Current);
        assert_eq!(AgedBucket::of(0), AgedBucket::Current);
        assert_eq!(AgedBucket::of(1), AgedBucket::Days1To30);
        assert_eq!(AgedBucket::of(30), AgedBucket::Days1To30);
        assert_eq!(AgedBucket::of(31), AgedBucket::Days31To60);
        assert_eq!(AgedBucket::of(60), AgedBucket::Days31To60);
        assert_eq!(AgedBucket::of(61), AgedBucket::Days61To90);
        assert_eq!(AgedBucket::of(90), AgedBucket::Days61To90);
        assert_eq!(AgedBucket::of(91), AgedBucket::Days90Plus);
        assert_eq!(AgedBucket::of(i64::MAX), AgedBucket::Days90Plus);
    }

    #[test]
    fn a_document_not_yet_due_is_current_and_is_never_late_by_a_negative_number() {
        let report = a_ladder();
        let document = report.parties[0]
            .documents
            .iter()
            .find(|d| d.number == "INV-2026-00005")
            .unwrap_or_else(|| panic!("the unpaid future document is on the list"));
        assert_eq!(document.days_overdue, 0);
        assert_eq!(document.bucket, AgedBucket::Current);
        assert_eq!(document.base_open_cents, Some(10_000));
    }

    #[test]
    fn the_oldest_debt_is_first_inside_a_group_and_the_groups_are_by_name() {
        let on = today();
        let report = age(
            on,
            AgedSide::Receivable,
            "EUR",
            vec![
                owed("Zephyr", "INV-2026-00010", on, 1_000),
                owed("Anchor", "INV-2026-00011", on, 2_000),
                owed(
                    "Anchor",
                    "INV-2026-00009",
                    on - time::Duration::days(40),
                    3_000,
                ),
            ],
        );
        let names: Vec<&str> = report.parties.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Anchor", "Zephyr"]);
        let numbers: Vec<&str> = report.parties[0]
            .documents
            .iter()
            .map(|d| d.number.as_str())
            .collect();
        assert_eq!(numbers, ["INV-2026-00009", "INV-2026-00011"]);
        assert_eq!(report.parties[0].buckets.total_cents, 5_000);
        assert_eq!(report.buckets.total_cents, 6_000);
    }

    #[test]
    fn a_settled_document_is_not_a_row_and_an_overpaid_one_is_a_negative_one() {
        let on = today();
        let report = age(
            on,
            AgedSide::Receivable,
            "EUR",
            vec![
                owed("Wave", "INV-2026-00001", on, 0),
                owed("Wave", "INV-2026-00002", on, -2_500),
            ],
        );
        assert_eq!(
            report.document_count, 1,
            "the settled one is not on the list"
        );
        assert_eq!(report.parties.len(), 1);
        assert_eq!(report.parties[0].documents.len(), 1);
        assert_eq!(report.parties[0].documents[0].open_cents, -2_500);
        assert_eq!(report.buckets.current_cents, -2_500);
        assert_eq!(report.buckets.total_cents, -2_500);
    }

    #[test]
    fn a_credit_note_subtracts_inside_the_customers_own_group() {
        let on = today();
        let mut credit = owed(
            "Wave",
            "CRN-2026-00001",
            on - time::Duration::days(5),
            -30_000,
        );
        credit.is_credit_note = true;
        let report = age(
            on,
            AgedSide::Receivable,
            "EUR",
            vec![
                owed(
                    "Wave",
                    "INV-2026-00001",
                    on - time::Duration::days(5),
                    121_000,
                ),
                credit,
            ],
        );
        assert_eq!(report.parties.len(), 1);
        assert_eq!(report.parties[0].buckets.days_1_30_cents, 91_000);
        assert_eq!(report.parties[0].buckets.total_cents, 91_000);
        let credit = report.parties[0]
            .documents
            .iter()
            .find(|d| d.is_credit_note)
            .unwrap_or_else(|| panic!("the credit note is a row of its own"));
        assert_eq!(credit.open_cents, -30_000);
    }

    #[test]
    fn a_foreign_document_is_added_at_its_own_frozen_rate() {
        let on = today();
        let mut abroad = owed(
            "Wave",
            "INV-2026-00007",
            on - time::Duration::days(2),
            100_000,
        );
        abroad.currency = "USD".to_owned();
        abroad.fx = Some(FxSnapshot {
            base_currency: "EUR".to_owned(),
            // 1 EUR = 1.10 USD: a thousand dollars is 909.09 euro.
            rate_micro: 1_100_000,
            rate_date: on,
        });
        let report = age(on, AgedSide::Receivable, "EUR", vec![abroad]);
        let document = &report.parties[0].documents[0];
        assert_eq!(document.currency, "USD");
        assert_eq!(document.open_cents, 100_000, "its own currency, untouched");
        assert_eq!(document.base_open_cents, Some(90_909));
        assert_eq!(report.buckets.days_1_30_cents, 90_909);
        assert_eq!(report.unconverted_count, 0);
    }

    #[test]
    fn a_document_that_cannot_be_restated_is_in_no_band_and_is_counted() {
        let on = today();
        let mut orphan = owed(
            "Wave",
            "INV-2026-00008",
            on - time::Duration::days(2),
            100_000,
        );
        orphan.currency = "USD".to_owned();
        orphan.fx = None;
        let mut foreign_books = owed("Wave", "INV-2026-00009", on, 50_000);
        foreign_books.currency = "CHF".to_owned();
        foreign_books.fx = Some(FxSnapshot {
            base_currency: "CHF".to_owned(),
            rate_micro: IDENTITY_RATE_MICRO,
            rate_date: on,
        });
        let report = age(on, AgedSide::Receivable, "EUR", vec![orphan, foreign_books]);
        assert_eq!(report.unconverted_count, 2);
        assert_eq!(report.parties[0].unconverted_count, 2);
        assert_eq!(report.buckets, AgedBuckets::default(), "no invented euro");
        assert_eq!(report.document_count, 2, "they are still on the list");
        for document in &report.parties[0].documents {
            assert_eq!(document.base_open_cents, None);
            assert!(document.open_cents > 0, "in its own currency it is real");
        }
    }

    #[test]
    fn a_day_before_anything_was_owed_is_a_report_of_zeroes_not_an_absence() {
        let report = age(
            day(2019, Month::December, 31),
            AgedSide::Payable,
            "EUR",
            Vec::new(),
        );
        assert!(report.parties.is_empty());
        assert_eq!(report.buckets, AgedBuckets::default());
        assert_eq!(report.document_count, 0);
        assert_eq!(report.unconverted_count, 0);
        assert_eq!(report.currency, "EUR");
        assert_eq!(report.side, AgedSide::Payable);
    }

    #[test]
    fn the_bands_are_named_once_and_read_back_by_name() {
        let mut buckets = AgedBuckets::default();
        for (index, bucket) in AGED_BUCKETS.iter().enumerate() {
            buckets.add(*bucket, 100 * (i64::try_from(index).unwrap_or(0) + 1));
        }
        assert_eq!(buckets.of(AgedBucket::Current), 100);
        assert_eq!(buckets.of(AgedBucket::Days1To30), 200);
        assert_eq!(buckets.of(AgedBucket::Days31To60), 300);
        assert_eq!(buckets.of(AgedBucket::Days61To90), 400);
        assert_eq!(buckets.of(AgedBucket::Days90Plus), 500);
        assert_eq!(buckets.total_cents, 1_500);
        let names: Vec<&str> = AGED_BUCKETS.iter().map(|b| b.as_str()).collect();
        assert_eq!(names, ["current", "d1_30", "d31_60", "d61_90", "d90_plus"]);
    }

    #[test]
    fn a_side_is_read_from_its_own_word_and_from_no_other() {
        assert_eq!(AgedSide::parse("receivable"), Some(AgedSide::Receivable));
        assert_eq!(AgedSide::parse("payable"), Some(AgedSide::Payable));
        for wrong in ["", "Receivable", "debtors", "both", "ar"] {
            assert_eq!(AgedSide::parse(wrong), None, "{wrong:?}");
        }
        assert_eq!(AgedSide::Receivable.as_str(), "receivable");
        assert_eq!(AgedSide::Payable.as_str(), "payable");
    }

    #[test]
    fn a_total_cannot_wrap_however_absurd_the_documents_are() {
        let on = today();
        let report = age(
            on,
            AgedSide::Receivable,
            "EUR",
            vec![
                owed("Wave", "INV-1", on, i64::MAX),
                owed("Wave", "INV-2", on, i64::MAX),
            ],
        );
        assert_eq!(report.buckets.current_cents, i64::MAX);
        assert_eq!(report.buckets.total_cents, i64::MAX);
    }
}
