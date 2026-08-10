//! alo Finance (ADR 0035, wave B4.03b): **how the journal is read in
//! aggregate** — the query API every report in B4.11 is a fold over
//! (`docs/design/finance.md`, "The four reports").
//!
//! [`crate::fin_journal`] owns writing an entry and reading one back;
//! this module owns the three questions a report asks of *many*:
//!
//! - **What moved, per account, over a period** — [`AccountStore::fin_trial_balance`].
//!   The P&L is this filtered to income and expense for a quarter; the balance
//!   sheet is this with no lower bound at a date, filtered to the other three
//!   types. Both are that fold and nothing else, which is the point of having
//!   one journal.
//! - **What moved on one account, line by line** — [`AccountStore::fin_account_ledger`].
//!   The drill-down behind every figure above, carrying the opening balance so
//!   a running column adds up to the closing one.
//! - **What moved, grouped by a dimension** — [`AccountStore::fin_dimension_balances`].
//!   Receivables by customer, payables by supplier, cost by engagement, VAT by
//!   rate: one query, because they are one query.
//!
//! Four rules hold across all three, and a reader who knows them can predict
//! every figure this module returns.
//!
//! **Every amount here is in the tenant's accounting currency** — the
//! `base_cents` column, restated at each entry's own frozen rate (B1.21, EU VAT
//! Directive art. 91). A report adds documents together, and documents raised
//! in three currencies have exactly one comparable number. The document's own
//! currency is not lost: it is on the entry and on every posting, and
//! [`AccountLedger`] hands both columns back per line so a drill-down can show
//! "$1,200.00 → €1,103.45". *Rejected: aggregating the document column too* —
//! a total that adds dollars to euro is a number with no meaning, and one that
//! silently reports only the majority currency is worse.
//!
//! **A debit is a positive `base_cents` and a credit is a negative one**, so
//! `debit_cents - credit_cents = balance_cents` on every row, and the two
//! columns exist only because that is how an accountant reads a page. The sign
//! convention is the journal's ([`crate::fin_journal::Posting::debit_cents`]
//! states it for the document column); it is restated here for the base column
//! rather than re-derived by each report.
//!
//! **The period is judged on `entry_date`** — the document's own date, never
//! when it was keyed — which is what makes a re-run of last quarter answer last
//! quarter's figure. Both bounds are inclusive and either may be absent; absent
//! means "since the books opened", which is precisely what a balance sheet
//! wants and a P&L never does.
//!
//! **Only accounts and dimension values that actually moved appear.** A chart
//! has a hundred accounts and a quarter touches twelve; a report that wants the
//! silent ones listed at zero joins [`AccountStore::fin_accounts`], which it
//! needs anyway for the ones with no postings ever. Zero rows for a quiet
//! period is the honest answer, not an empty-looking bug.
//!
//! Tenancy is structural, as everywhere in this crate: every statement carries
//! `tenant_id` from the handle, so another tenant's postings are not filtered
//! out of a total, they are never read into it. That is the failure mode an
//! aggregate has and a read-by-id does not, and it is why the property suite
//! asserts a second tenant's every balance is unchanged by the first's month.

use time::Date;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::fin_accounts::{AccountRole, AccountType};
use crate::fin_journal::{EntryKind, EntrySource, SourceEvent, SourceKind};
use crate::id::{FinAccountId, FinEntryId, FinPostingId};

/// The most ledger lines one account read returns. A drill-down is read by a
/// human or written to a CSV a page at a time; a year of a busy bank account is
/// well inside it.
pub const LEDGER_PAGE_MAX: i64 = 2_000;

/// The most groups one dimension read returns. Reached only by a tenant with
/// more counterparties than that in one period — [`DimensionBalances::truncated`]
/// says so rather than the total quietly being wrong.
pub const LEDGER_GROUPS_MAX: i64 = 2_000;

/// What one account moved over a period, in the accounting currency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountBalance {
    /// The account.
    pub account_id: FinAccountId,
    /// The code an accountant types, and what reports sort by.
    pub code: String,
    /// What the account is called, as this tenant renamed it.
    pub name: String,
    /// Which of the five categories it belongs to — what splits a trial
    /// balance into a P&L and a balance sheet.
    pub kind: AccountType,
    /// The posting-rule job it does, if any.
    pub role: Option<AccountRole>,
    /// The sum of its debits (positive postings), as a positive number.
    pub debit_cents: i64,
    /// The sum of its credits, as a positive number.
    pub credit_cents: i64,
    /// `debit_cents - credit_cents`: positive is a debit balance.
    pub balance_cents: i64,
    /// How many postings the period put on it.
    pub postings: i64,
}

/// Every account that moved in a period, and the two totals that must be equal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrialBalance {
    /// The inclusive lower bound asked for, or `None` for "since the books
    /// opened".
    pub from: Option<Date>,
    /// The inclusive upper bound asked for, or `None` for "to date".
    pub to: Option<Date>,
    /// The accounts, in code order.
    pub accounts: Vec<AccountBalance>,
    /// Every debit in the period.
    pub debit_cents: i64,
    /// Every credit in the period.
    pub credit_cents: i64,
}

impl TrialBalance {
    /// Whether the period's debits equal its credits.
    ///
    /// It is always `true` — every entry balances in the base column, so any
    /// sum of whole entries does — and that is exactly why it is worth
    /// asserting: a `false` here means postings were written by something other
    /// than [`AccountStore::post_fin_entry`], or a date bound cut an entry in
    /// half, and both are bugs a report would otherwise present as a figure.
    pub fn balances(&self) -> bool {
        self.debit_cents == self.credit_cents
    }

    /// The net movement of the accounts of one type — income and expense for a
    /// P&L, the other three for a balance sheet.
    pub fn net_of(&self, kind: AccountType) -> i64 {
        self.accounts
            .iter()
            .filter(|account| account.kind == kind)
            .map(|account| account.balance_cents)
            .sum()
    }
}

/// One line of an account's ledger: the posting, and enough of its entry for a
/// human to recognise what it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerLine {
    /// The posting.
    pub posting_id: FinPostingId,
    /// The entry it belongs to — what a drill-down opens.
    pub entry_id: FinEntryId,
    /// The accounting date.
    pub entry_date: Date,
    /// What kind of event the entry books.
    pub kind: EntryKind,
    /// The document event behind it, or `None` for a manual entry.
    pub source: Option<EntrySource>,
    /// The entry's memo — the invoice number, usually.
    pub entry_memo: String,
    /// The posting's own memo, where a rule wrote one.
    pub memo: String,
    /// The currency the document was raised in.
    pub currency: String,
    /// Signed, in that currency.
    pub amount_cents: i64,
    /// The same money in the accounting currency: positive debits.
    pub base_cents: i64,
    /// The balance of the account after this line, opening included.
    pub running_cents: i64,
    /// The VAT rate this posting's tax belongs to, if any.
    pub vat_rate_bp: Option<i32>,
    /// Who owed it.
    pub customer_id: Option<String>,
    /// Whose bill it was.
    pub supplier_key: Option<String>,
    /// Which engagement it belongs to.
    pub project_id: Option<String>,
    /// Whose expense it was.
    pub user_id: Option<String>,
}

/// One account's movement over a period, with the balance it started from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountLedger {
    /// The account read.
    pub account_id: FinAccountId,
    /// Everything on it strictly before `from`, in the accounting currency —
    /// zero when `from` is absent, because then nothing is before the period.
    pub opening_cents: i64,
    /// The lines, oldest first, each carrying the running balance.
    pub lines: Vec<LedgerLine>,
    /// The balance after the last line — `opening_cents` plus the period's
    /// movement, and equal to this account's row in the cumulative trial
    /// balance at `to` whenever the page was not truncated.
    pub closing_cents: i64,
    /// Whether [`LEDGER_PAGE_MAX`] cut the page short. A caller showing a
    /// running column must say so: a closing balance under a truncated page is
    /// the balance of what is shown, not of the account.
    pub truncated: bool,
}

/// Which dimension a grouped read groups by.
///
/// Closed, and each variant is one column of `fin_postings` — a caller can
/// therefore never name a column, which is what keeps the one piece of
/// interpolated SQL in this module safe by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerDimension {
    /// Who owed it — receivables by customer.
    Customer,
    /// Whose bill it was — payables by supplier.
    Supplier,
    /// Which engagement — cost and revenue by project.
    Project,
    /// Whose expense claim — what an employee is owed.
    User,
    /// Which VAT rate — the tax return's own grouping.
    VatRate,
}

impl LedgerDimension {
    /// The column this dimension lives in, as an expression that yields text.
    ///
    /// The VAT rate is the one dimension that is not already a string, so it is
    /// cast: a rate is a small integer and rendering it as its own basis points
    /// keeps one row shape for all five rather than a second return type for
    /// one of them. [`DimensionBalance::vat_rate_bp`] reads it back.
    fn column(self) -> &'static str {
        match self {
            Self::Customer => "p.customer_id",
            Self::Supplier => "p.supplier_key",
            Self::Project => "p.project_id",
            Self::User => "p.user_id",
            Self::VatRate => "p.vat_rate_bp::text",
        }
    }
}

/// Which accounts a grouped read covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerScope {
    /// Every account — cost by project across the whole chart.
    All,
    /// The account doing one job, whichever code this tenant gave it —
    /// receivables by customer is `Role(Ar)`, and it stays right after an
    /// accountant recodes the chart.
    Role(AccountRole),
    /// Every account of one of the five categories — the VAT return's taxable
    /// base is `Type(Income)` by rate, because a rate travels on the *revenue*
    /// posting as well as on the tax one and a tenant's own expense accounts
    /// carry no role to name them by.
    Type(AccountType),
    /// One named account.
    Account(FinAccountId),
}

impl LedgerScope {
    /// The predicate and the value it binds. `All` binds `NULL` against a
    /// tautology, so all four variants share one statement and one bind slot.
    fn predicate(&self) -> (&'static str, Option<String>) {
        match self {
            Self::All => ("($4::text IS NULL OR $4::text IS NOT NULL)", None),
            Self::Role(role) => ("a.role = $4", Some(role.as_str().to_owned())),
            // `type` is the chart's own column name (0129) and a keyword worth
            // quoting, as [`AccountStore::fin_trial_balance`] quotes it.
            Self::Type(kind) => ("a.\"type\" = $4", Some(kind.as_str().to_owned())),
            Self::Account(id) => ("p.account_id = $4", Some(id.as_str().to_owned())),
        }
    }
}

/// What one value of a dimension moved over a period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimensionBalance {
    /// The dimension's value, or `None` for the postings that carry none — a
    /// real group, and one worth seeing: receivables with no customer on them
    /// are a posting rule that forgot.
    pub value: Option<String>,
    /// The sum of debits, as a positive number.
    pub debit_cents: i64,
    /// The sum of credits, as a positive number.
    pub credit_cents: i64,
    /// `debit_cents - credit_cents`.
    pub balance_cents: i64,
    /// How many postings are behind the figure.
    pub postings: i64,
}

impl DimensionBalance {
    /// The rate in basis points, for a [`LedgerDimension::VatRate`] read. `None` for
    /// the ungrouped row and for every other dimension.
    pub fn vat_rate_bp(&self) -> Option<i32> {
        self.value.as_deref()?.parse().ok()
    }
}

/// A grouped read, and whether the cap cut it short.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimensionBalances {
    /// The groups, largest debit balance first, then by value — a stable order,
    /// so the same period read twice is the same page.
    pub rows: Vec<DimensionBalance>,
    /// Whether [`LEDGER_GROUPS_MAX`] cut the read short. When it is `true` the
    /// rows shown are the largest balances, and the ones dropped are the
    /// smallest; a caller totalling the column must say so rather than present
    /// a partial sum as the period's.
    pub truncated: bool,
}

impl DimensionBalances {
    /// What the returned groups add up to. Not the period's total when
    /// [`Self::truncated`] is set — which is the whole reason that flag exists.
    pub fn balance_cents(&self) -> i64 {
        self.rows.iter().map(|row| row.balance_cents).sum()
    }
}

/// The aggregate columns every read in this module selects, over `p`.
///
/// One spelling of "a debit is a positive base amount", so a trial balance, a
/// drill-down and a VAT return cannot disagree about which side a posting is
/// on. `FILTER` rather than `CASE` because it is the same plan and says what it
/// means.
///
/// **The `::bigint` casts are the honest half of the module's overflow story.**
/// Postgres widens `SUM(bigint)` to `numeric`, which never overflows and which
/// this crate has no type for; narrowing it back is where a total too large for
/// `i64` would be *refused* (SQLSTATE 22003, surfacing as a
/// [`StoreError::Db`]) rather than silently truncated. It cannot happen with
/// the journal's own ceilings — [`crate::fin_journal::POSTING_AMOUNT_MAX_CENTS`]
/// leaves four orders of magnitude of headroom — and a report that errored
/// would still be better than one that printed a wrapped number.
const SUM_COLS: &str = "COALESCE(SUM(p.base_cents) FILTER (WHERE p.base_cents > 0), 0)::bigint \
                            AS debit_cents, \
                        COALESCE(-SUM(p.base_cents) FILTER (WHERE p.base_cents < 0), 0)::bigint \
                            AS credit_cents, \
                        COALESCE(SUM(p.base_cents), 0)::bigint AS balance_cents, \
                        COUNT(p.id) AS postings";

/// The join and the date window every read in this module shares: postings, the
/// entry that dates them, and the account that classifies them.
const FROM_JOIN: &str = "FROM fin_postings p \
                         JOIN fin_entries e ON e.tenant_id = p.tenant_id AND e.id = p.entry_id \
                         JOIN fin_accounts a ON a.tenant_id = p.tenant_id AND a.id = p.account_id \
                         WHERE p.tenant_id = $1 \
                             AND ($2::date IS NULL OR e.entry_date >= $2) \
                             AND ($3::date IS NULL OR e.entry_date <= $3)";

/// The ledger-line columns, in [`LineRow`] order.
const LINE_COLS: &str = "p.id, p.entry_id, e.entry_date, e.kind, e.source_kind, e.source_id, \
                         e.source_event, e.memo AS entry_memo, p.memo, e.currency, \
                         p.amount_cents, p.base_cents, p.vat_rate_bp, p.customer_id, \
                         p.supplier_key, p.project_id, p.user_id";

impl AccountStore {
    /// **The trial balance**: what every account moved between two dates, in
    /// the accounting currency, in code order.
    ///
    /// Both bounds are inclusive and either may be absent — a P&L passes both
    /// (a quarter), a balance sheet passes only `to` (everything up to a date),
    /// and passing neither is the whole journal. Accounts with no posting in
    /// the window are absent rather than listed at zero.
    ///
    /// [`TrialBalance::balances`] is `true` on every honest result and the
    /// property suite asserts it after every generated month; a caller
    /// rendering the report should assert it too, because the figure that a
    /// broken one produces looks exactly like a real report.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when a stored account type or role is one
    /// this build does not know (a schema disagreement, honest to fail on);
    /// [`StoreError::Db`] on failure.
    pub async fn fin_trial_balance(
        &self,
        from: Option<Date>,
        to: Option<Date>,
    ) -> Result<TrialBalance> {
        let rows = sqlx::query_as::<_, BalanceRow>(&format!(
            // `type` is the chart's own column name (0129) and a keyword worth
            // quoting; `kind` is what it is called everywhere in Rust.
            "SELECT a.id, a.code, a.name, a.\"type\" AS kind, a.role, {SUM_COLS} \
             {FROM_JOIN} \
             GROUP BY a.id, a.code, a.name, a.\"type\", a.role \
             ORDER BY a.code, a.id"
        ))
        .bind(self.tenant.as_str())
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let accounts = rows
            .into_iter()
            .map(BalanceRow::into_balance)
            .collect::<Result<Vec<_>>>()?;
        // Summed from the rows rather than asked for again: a second query
        // could see a different journal, and a total that does not add up to
        // the page under it is the one defect a trial balance must not have.
        let debit_cents = accounts.iter().map(|account| account.debit_cents).sum();
        let credit_cents = accounts.iter().map(|account| account.credit_cents).sum();
        Ok(TrialBalance {
            from,
            to,
            accounts,
            debit_cents,
            credit_cents,
        })
    }

    /// **One account's ledger**: its opening balance, then every line in the
    /// period oldest first, each carrying the running balance.
    ///
    /// The opening balance is everything strictly before `from` — zero when
    /// `from` is absent, because then there is no "before". Lines are ordered
    /// by accounting date, then by when the entry was posted, then by position
    /// within the entry, so two lines of one entry are never split and a page
    /// read twice is the same page.
    ///
    /// An account of another tenant reads as an empty ledger rather than an
    /// error: the id is simply not this tenant's, exactly as
    /// [`AccountStore::fin_entry`] answers.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when a stored word is one this build does not
    /// know; [`StoreError::Db`] on failure.
    pub async fn fin_account_ledger(
        &self,
        account_id: &FinAccountId,
        from: Option<Date>,
        to: Option<Date>,
        limit: i64,
    ) -> Result<AccountLedger> {
        let opening_cents: i64 = match from {
            None => 0,
            Some(start) => sqlx::query_scalar(
                "SELECT COALESCE(SUM(p.base_cents), 0)::bigint FROM fin_postings p \
                 JOIN fin_entries e ON e.tenant_id = p.tenant_id AND e.id = p.entry_id \
                 WHERE p.tenant_id = $1 AND p.account_id = $2 AND e.entry_date < $3",
            )
            .bind(self.tenant.as_str())
            .bind(account_id.as_str())
            .bind(start)
            .fetch_one(&self.pool)
            .await
            .map_err(StoreError::Db)?,
        };

        // One more than asked for, so a full page is distinguishable from a
        // page that happens to end on the cap.
        let page = limit.clamp(1, LEDGER_PAGE_MAX);
        let rows = sqlx::query_as::<_, LineRow>(&format!(
            "SELECT {LINE_COLS} FROM fin_postings p \
             JOIN fin_entries e ON e.tenant_id = p.tenant_id AND e.id = p.entry_id \
             WHERE p.tenant_id = $1 AND p.account_id = $2 \
                 AND ($3::date IS NULL OR e.entry_date >= $3) \
                 AND ($4::date IS NULL OR e.entry_date <= $4) \
             ORDER BY e.entry_date, e.created_at, e.id, p.position LIMIT $5"
        ))
        .bind(self.tenant.as_str())
        .bind(account_id.as_str())
        .bind(from)
        .bind(to)
        .bind(page + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let truncated = i64::try_from(rows.len()).unwrap_or(i64::MAX) > page;
        let mut running = opening_cents;
        let mut lines = Vec::with_capacity(rows.len().min(usize::try_from(page).unwrap_or(0)));
        for row in rows.into_iter().take(usize::try_from(page).unwrap_or(0)) {
            let line = row.into_line(&mut running)?;
            lines.push(line);
        }
        Ok(AccountLedger {
            account_id: account_id.clone(),
            opening_cents,
            lines,
            closing_cents: running,
            truncated,
        })
    }

    /// **A grouped read**: what each value of one dimension moved, over the
    /// accounts a scope names.
    ///
    /// Receivables by customer is `(Scope::Role(Ar), LedgerDimension::Customer)`;
    /// the VAT return's output tax is `(Scope::Role(VatOutput),
    /// LedgerDimension::VatRate)`; cost by engagement is `(Scope::All,
    /// LedgerDimension::Project)`. Postings carrying no value for the dimension are
    /// one group with `value: None` rather than dropped — a receivable nobody
    /// is owed by is a rule with a bug, and hiding it would hide the bug.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn fin_dimension_balances(
        &self,
        scope: &LedgerScope,
        dimension: LedgerDimension,
        from: Option<Date>,
        to: Option<Date>,
    ) -> Result<DimensionBalances> {
        let (predicate, key) = scope.predicate();
        let column = dimension.column();
        let rows = sqlx::query_as::<_, DimensionRow>(&format!(
            "SELECT {column} AS value, {SUM_COLS} \
             {FROM_JOIN} AND {predicate} \
             GROUP BY {column} \
             ORDER BY balance_cents DESC, value ASC NULLS LAST LIMIT $5"
        ))
        .bind(self.tenant.as_str())
        .bind(from)
        .bind(to)
        .bind(key)
        .bind(LEDGER_GROUPS_MAX + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let truncated = i64::try_from(rows.len()).unwrap_or(i64::MAX) > LEDGER_GROUPS_MAX;
        let rows = rows
            .into_iter()
            .take(usize::try_from(LEDGER_GROUPS_MAX).unwrap_or(0))
            .map(DimensionRow::into_balance)
            .collect();
        Ok(DimensionBalances { rows, truncated })
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct BalanceRow {
    id: String,
    code: String,
    name: String,
    kind: String,
    role: String,
    debit_cents: i64,
    credit_cents: i64,
    balance_cents: i64,
    postings: i64,
}

impl BalanceRow {
    /// Re-parses the two stored words rather than trusting them, for
    /// [`crate::fin_journal`]'s reason: a word this build does not know is a
    /// schema disagreement, and a wrong figure on a P&L is worse than an error.
    fn into_balance(self) -> Result<AccountBalance> {
        Ok(AccountBalance {
            account_id: FinAccountId::new(self.id),
            code: self.code,
            name: self.name,
            kind: AccountType::parse(&self.kind)?,
            role: AccountRole::parse(&self.role)?,
            debit_cents: self.debit_cents,
            credit_cents: self.credit_cents,
            balance_cents: self.balance_cents,
            postings: self.postings,
        })
    }
}

#[derive(sqlx::FromRow)]
struct LineRow {
    id: String,
    entry_id: String,
    entry_date: Date,
    kind: String,
    source_kind: String,
    source_id: String,
    source_event: String,
    entry_memo: String,
    memo: String,
    currency: String,
    amount_cents: i64,
    base_cents: i64,
    vat_rate_bp: Option<i32>,
    customer_id: Option<String>,
    supplier_key: Option<String>,
    project_id: Option<String>,
    user_id: Option<String>,
}

impl LineRow {
    /// Reads one line and advances the running balance, so the column a screen
    /// prints is produced in exactly one place.
    fn into_line(self, running: &mut i64) -> Result<LedgerLine> {
        let source = if self.source_kind.is_empty() {
            None
        } else {
            Some(EntrySource {
                kind: SourceKind::parse(&self.source_kind)?,
                id: self.source_id,
                event: SourceEvent::parse(&self.source_event)?,
            })
        };
        *running = running.checked_add(self.base_cents).ok_or_else(|| {
            StoreError::Validation("this account's balance is too large to add up".to_owned())
        })?;
        Ok(LedgerLine {
            posting_id: FinPostingId::new(self.id),
            entry_id: FinEntryId::new(self.entry_id),
            entry_date: self.entry_date,
            kind: EntryKind::parse(&self.kind)?,
            source,
            entry_memo: self.entry_memo,
            memo: self.memo,
            currency: self.currency,
            amount_cents: self.amount_cents,
            base_cents: self.base_cents,
            running_cents: *running,
            vat_rate_bp: self.vat_rate_bp,
            customer_id: self.customer_id,
            supplier_key: self.supplier_key,
            project_id: self.project_id,
            user_id: self.user_id,
        })
    }
}

#[derive(sqlx::FromRow)]
struct DimensionRow {
    value: Option<String>,
    debit_cents: i64,
    credit_cents: i64,
    balance_cents: i64,
    postings: i64,
}

impl DimensionRow {
    fn into_balance(self) -> DimensionBalance {
        DimensionBalance {
            value: self.value,
            debit_cents: self.debit_cents,
            credit_cents: self.credit_cents,
            balance_cents: self.balance_cents,
            postings: self.postings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn balance(kind: AccountType, code: &str, cents: i64) -> AccountBalance {
        AccountBalance {
            account_id: FinAccountId::new(code),
            code: code.to_owned(),
            name: code.to_owned(),
            kind,
            role: None,
            debit_cents: cents.max(0),
            credit_cents: (-cents).max(0),
            balance_cents: cents,
            postings: 1,
        }
    }

    #[test]
    fn a_trial_balance_balances_when_its_two_totals_agree() {
        let sheet = TrialBalance {
            from: None,
            to: None,
            accounts: vec![
                balance(AccountType::Asset, "1000", 12_100),
                balance(AccountType::Income, "7000", -10_000),
                balance(AccountType::Liability, "2200", -2_100),
            ],
            debit_cents: 12_100,
            credit_cents: 12_100,
        };
        assert!(sheet.balances());
        // The P&L side and the balance-sheet side of the same fold.
        assert_eq!(sheet.net_of(AccountType::Income), -10_000);
        assert_eq!(sheet.net_of(AccountType::Asset), 12_100);
        assert_eq!(sheet.net_of(AccountType::Expense), 0);
    }

    #[test]
    fn a_trial_balance_that_does_not_add_up_says_so() {
        let sheet = TrialBalance {
            from: None,
            to: None,
            accounts: vec![balance(AccountType::Asset, "1000", 12_100)],
            debit_cents: 12_100,
            credit_cents: 12_000,
        };
        assert!(!sheet.balances());
    }

    #[test]
    fn each_dimension_names_a_posting_column_and_only_that() {
        // The one interpolated fragment in the module: every value must be a
        // column of `fin_postings` written here, never a caller's string.
        for dimension in [
            LedgerDimension::Customer,
            LedgerDimension::Supplier,
            LedgerDimension::Project,
            LedgerDimension::User,
            LedgerDimension::VatRate,
        ] {
            let column = dimension.column();
            assert!(
                column.starts_with("p."),
                "{column:?} must be a posting column"
            );
            assert!(
                column
                    .trim_start_matches("p.")
                    .trim_end_matches("::text")
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_'),
                "{column:?} must be a plain column name"
            );
        }
    }

    #[test]
    fn a_scope_binds_its_value_and_never_interpolates_it() {
        let (predicate, key) = LedgerScope::All.predicate();
        assert!(predicate.contains("$4"));
        assert_eq!(key, None);

        let (predicate, key) = LedgerScope::Role(AccountRole::Ar).predicate();
        assert_eq!(predicate, "a.role = $4");
        assert_eq!(key.as_deref(), Some("ar"));

        let (predicate, key) = LedgerScope::Type(AccountType::Income).predicate();
        assert_eq!(predicate, "a.\"type\" = $4");
        assert_eq!(key.as_deref(), Some("income"));

        let (predicate, key) =
            LedgerScope::Account(FinAccountId::new("acc-1'; DROP TABLE fin_postings--"))
                .predicate();
        assert_eq!(predicate, "p.account_id = $4");
        // The hostile string travels as a bind parameter, never as SQL.
        assert_eq!(key.as_deref(), Some("acc-1'; DROP TABLE fin_postings--"));
    }

    #[test]
    fn a_vat_group_reads_its_rate_back_and_other_dimensions_do_not() {
        let rate = DimensionBalance {
            value: Some("2100".to_owned()),
            debit_cents: 0,
            credit_cents: 2_100,
            balance_cents: -2_100,
            postings: 1,
        };
        assert_eq!(rate.vat_rate_bp(), Some(2100));

        let customer = DimensionBalance {
            value: Some("cust-1".to_owned()),
            ..rate.clone()
        };
        assert_eq!(customer.vat_rate_bp(), None);

        let ungrouped = DimensionBalance {
            value: None,
            ..rate
        };
        assert_eq!(ungrouped.vat_rate_bp(), None);
    }

    #[test]
    fn a_grouped_read_totals_only_what_it_returned() {
        let groups = DimensionBalances {
            rows: vec![
                DimensionBalance {
                    value: Some("cust-1".to_owned()),
                    debit_cents: 12_100,
                    credit_cents: 0,
                    balance_cents: 12_100,
                    postings: 1,
                },
                DimensionBalance {
                    value: Some("cust-2".to_owned()),
                    debit_cents: 0,
                    credit_cents: 100,
                    balance_cents: -100,
                    postings: 1,
                },
            ],
            truncated: true,
        };
        assert_eq!(groups.balance_cents(), 12_000);
        // And the flag is what stops a caller printing that as the period's.
        assert!(groups.truncated);
    }
}
