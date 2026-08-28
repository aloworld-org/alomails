//! The chart of accounts — alo Finance's list of the places money can be
//! (ADR 0035, wave B4; `docs/design/finance.md`, "The chart of accounts").
//!
//! Everything the ledger does starts here, and two rules make this file the
//! one that decides whether the books of a German tenant and a French one can
//! be kept by the same code:
//!
//! - **A posting rule finds its account by [`AccountRole`], never by code.**
//!   The roles are a closed set of our own words — `ar`, `bank`, `vat_output`
//!   and ten others — and at most one account per tenant may hold each. So a
//!   tenant who renumbers their whole chart to match their accountant's (which
//!   they will) changes no posting rule and breaks no report. Hardcoding
//!   `1400` in a rule was the alternative, and it is wrong in every country
//!   but one, silently, in the direction of a misfiled tax return.
//! - **The default chart is neutral, and seeded once per tenant, on first
//!   read.** Once is recorded in `fin_seeds`, separately from the accounts it
//!   wrote, so a tenant who deletes the chart is not handed it again the next
//!   morning — [`crate::insight_overview`]'s mechanism, reused whole,
//!   including the primary key that makes two simultaneous first reads produce
//!   one chart without a lock. Shipping a national chart (SKR03/04, the PCG,
//!   the Belgian MAR) is a *compliance claim* about somebody else's document
//!   and is out of scope for B4 by decision, not by omission.
//!
//! **No English lives here.** [`CHART`] states each default account's code,
//! kind and role; its *name* arrives from the HTTP edge in the language of
//! whoever opened the chart first, exactly as an Insights board's captions do.
//! A hardcoded English account name would be a bug in a European product.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_field::required;
use crate::error::{Result, StoreError};
use crate::id::FinAccountId;

/// An account code is what an accountant types; every national convention
/// fits well inside this (the longest in common use is the French PCG's six
/// digits, and SAP's is ten).
pub const ACCOUNT_CODE_MAX_CHARS: usize = 20;
/// An account name is a label on a report line, not a description.
pub const ACCOUNT_NAME_MAX_CHARS: usize = 120;

/// The `fin_seeds` key under which the default chart is recorded. Ours, never
/// a caller's — nothing accepts a seed key as input.
pub const CHART_SEED_KEY: &str = "eu_sme_chart";

/// The columns every read of an account selects, in [`AccountRow`] order.
/// `type` is aliased because it is a Rust keyword, not because the column is
/// named wrongly: the note's schema says `type`, and an accountant reading the
/// table directly should find the word they expect.
const ACCOUNT_COLS: &str =
    "id, code, name, type AS kind, role, active, system, created_at, updated_at";

/// What kind of thing an account holds — the five categories every
/// double-entry chart has had since Pacioli, and the reason a report knows
/// which side of the balance sheet or P&L a line belongs on.
///
/// This set does not grow. A sixth variant would not be a new account kind but
/// a different accounting model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountType {
    /// What the business owns or is owed: bank, cash, receivables.
    Asset,
    /// What it owes: payables, VAT collected, money owed to employees.
    Liability,
    /// The owners' stake, and where the opening balances land.
    Equity,
    /// What it earns.
    Income,
    /// What it spends.
    Expense,
}

impl AccountType {
    /// The stored word — the wire form and the database value, one spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Asset => "asset",
            Self::Liability => "liability",
            Self::Equity => "equity",
            Self::Income => "income",
            Self::Expense => "expense",
        }
    }

    /// Reads the stored word back.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set when the word is not
    /// one of the five.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "asset" => Ok(Self::Asset),
            "liability" => Ok(Self::Liability),
            "equity" => Ok(Self::Equity),
            "income" => Ok(Self::Income),
            "expense" => Ok(Self::Expense),
            _ => Err(StoreError::Validation(
                "account type must be asset, liability, equity, income or expense".to_owned(),
            )),
        }
    }

    /// Whether a balance on this account belongs to the balance sheet (as
    /// opposed to the profit and loss). Stated here, once, so B4.11's two
    /// reports cannot disagree about where an account goes.
    pub fn is_balance_sheet(self) -> bool {
        matches!(self, Self::Asset | Self::Liability | Self::Equity)
    }
}

/// The job an account does in a posting rule, as opposed to the number an
/// accountant calls it by.
///
/// The set is closed and it is *ours*: a role is not a translation of a
/// national chart's line, it is the question "where does the receivable go"
/// with exactly one answer per tenant. A wave that needs another one adds a
/// variant here — deliberately a code change with its own tests, rather than a
/// string a caller may invent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountRole {
    /// Trade receivables — what customers owe us (an issued invoice's debit).
    Ar,
    /// Trade payables — what we owe suppliers (an approved bill's credit).
    Ap,
    /// The bank account money actually moves through.
    Bank,
    /// Petty cash.
    Cash,
    /// VAT we charged and owe the state.
    VatOutput,
    /// VAT we paid and may recover.
    VatInput,
    /// Sales revenue — where an invoice line's net lands by default.
    Revenue,
    /// The expense account an uncategorised cost falls to.
    ExpenseDefault,
    /// What we owe an employee for an approved expense claim, until it is
    /// reimbursed.
    EmployeePayable,
    /// Foreign-exchange differences, which are a gain as often as a loss (the
    /// sign carries that; the account does not need two of itself).
    FxDiff,
    /// The cent that rounding leaves behind, so an entry still balances.
    Rounding,
    /// Where the balances a tenant arrives with are counterweighted, the day
    /// their books open here.
    OpeningBalance,
    /// Money whose owner is genuinely unknown — a bank line nobody can place.
    /// Never a dumping ground for a configuration mistake: a posting rule that
    /// cannot find its role refuses the *document* (`docs/design/finance.md`).
    Suspense,
}

impl AccountRole {
    /// The stored word.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ar => "ar",
            Self::Ap => "ap",
            Self::Bank => "bank",
            Self::Cash => "cash",
            Self::VatOutput => "vat_output",
            Self::VatInput => "vat_input",
            Self::Revenue => "revenue",
            Self::ExpenseDefault => "expense_default",
            Self::EmployeePayable => "employee_payable",
            Self::FxDiff => "fx_diff",
            Self::Rounding => "rounding",
            Self::OpeningBalance => "opening_balance",
            Self::Suspense => "suspense",
        }
    }

    /// Every role, in the order the default chart introduces them.
    pub const ALL: &'static [Self] = &[
        Self::Bank,
        Self::Cash,
        Self::Ar,
        Self::VatInput,
        Self::Suspense,
        Self::Ap,
        Self::VatOutput,
        Self::EmployeePayable,
        Self::OpeningBalance,
        Self::Revenue,
        Self::ExpenseDefault,
        Self::Rounding,
        Self::FxDiff,
    ];

    /// Reads a role word, where `""` means "an ordinary account".
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the field when the word is not one of
    /// ours — a caller inventing a role would otherwise create an account that
    /// no posting rule will ever find, which looks like a working chart and is
    /// not.
    pub fn parse(value: &str) -> Result<Option<Self>> {
        let value = value.trim();
        if value.is_empty() {
            return Ok(None);
        }
        Self::ALL
            .iter()
            .copied()
            .find(|role| role.as_str() == value)
            .map(Some)
            .ok_or_else(|| {
                StoreError::Validation(format!(
                    "account role must be empty or one of: {}",
                    Self::ALL
                        .iter()
                        .map(|role| role.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })
    }
}

/// One account of the default chart: its code, what it holds, and the job it
/// does. The name is deliberately absent — see the module header.
#[derive(Debug, Clone, Copy)]
pub struct ChartAccount {
    /// The code the seed writes, and the key a caller's name is matched on.
    pub code: &'static str,
    /// Which of the five categories it belongs to.
    pub kind: AccountType,
    /// The posting-rule job it does, if any.
    pub role: Option<AccountRole>,
}

/// The neutral EU-SME chart, in code order.
///
/// It is deliberately small: every role a posting rule resolves, plus the
/// handful of expense lines a small business splits its costs across on day
/// one. A tenant who wants their accountant's chart adds accounts (or renames
/// these) — which is why every one of them is renameable and recodeable, and
/// why the roles rather than the codes are what the rest of the module reads.
///
/// The codes follow the international 1–7 blocks (assets, liabilities, equity,
/// income, expenses) rather than any one member state's convention, because a
/// neutral chart that resembles nobody's is honest, while one that resembles
/// SKR04 without being it is a claim we are not entitled to make.
pub const CHART: &[ChartAccount] = &[
    ChartAccount {
        code: "1000",
        kind: AccountType::Asset,
        role: Some(AccountRole::Bank),
    },
    ChartAccount {
        code: "1010",
        kind: AccountType::Asset,
        role: Some(AccountRole::Cash),
    },
    ChartAccount {
        code: "1100",
        kind: AccountType::Asset,
        role: Some(AccountRole::Ar),
    },
    ChartAccount {
        code: "1200",
        kind: AccountType::Asset,
        role: Some(AccountRole::VatInput),
    },
    ChartAccount {
        code: "1900",
        kind: AccountType::Asset,
        role: Some(AccountRole::Suspense),
    },
    ChartAccount {
        code: "2000",
        kind: AccountType::Liability,
        role: Some(AccountRole::Ap),
    },
    ChartAccount {
        code: "2100",
        kind: AccountType::Liability,
        role: Some(AccountRole::VatOutput),
    },
    ChartAccount {
        code: "2200",
        kind: AccountType::Liability,
        role: Some(AccountRole::EmployeePayable),
    },
    ChartAccount {
        code: "3000",
        kind: AccountType::Equity,
        role: Some(AccountRole::OpeningBalance),
    },
    ChartAccount {
        code: "3100",
        kind: AccountType::Equity,
        role: None,
    },
    ChartAccount {
        code: "4000",
        kind: AccountType::Income,
        role: Some(AccountRole::Revenue),
    },
    ChartAccount {
        code: "4900",
        kind: AccountType::Income,
        role: None,
    },
    ChartAccount {
        code: "5000",
        kind: AccountType::Expense,
        role: None,
    },
    ChartAccount {
        code: "6000",
        kind: AccountType::Expense,
        role: Some(AccountRole::ExpenseDefault),
    },
    ChartAccount {
        code: "6100",
        kind: AccountType::Expense,
        role: None,
    },
    ChartAccount {
        code: "6200",
        kind: AccountType::Expense,
        role: None,
    },
    ChartAccount {
        code: "6300",
        kind: AccountType::Expense,
        role: None,
    },
    ChartAccount {
        code: "6400",
        kind: AccountType::Expense,
        role: None,
    },
    ChartAccount {
        code: "6900",
        kind: AccountType::Expense,
        role: Some(AccountRole::Rounding),
    },
    ChartAccount {
        code: "6950",
        kind: AccountType::Expense,
        role: Some(AccountRole::FxDiff),
    },
];

/// The words the default chart is written with, handed in by the edge in the
/// language of whoever opened the chart first.
#[derive(Debug, Clone, Default)]
pub struct ChartSeed {
    /// One name per [`CHART`] entry, in any order, matched on the code. A code
    /// the chart wants and this list has not got is a bug in the caller, and
    /// it is refused rather than filled in with something we invented.
    pub names: Vec<ChartName>,
}

/// One default account's name, against the code it belongs to.
#[derive(Debug, Clone)]
pub struct ChartName {
    /// The [`ChartAccount::code`] this name is for.
    pub code: String,
    /// What the account is called, in the caller's language.
    pub name: String,
}

/// The writable shape of an account, used for both create and update (an
/// update is a full replace — the route layer merges a partial `PATCH` onto
/// the stored record before calling).
#[derive(Debug, Clone)]
pub struct NewAccount {
    /// What an accountant types. Required, unique within the tenant,
    /// uppercased on the way in.
    pub code: String,
    /// What the account is called. Required.
    pub name: String,
    /// Which of the five categories it belongs to.
    pub kind: AccountType,
    /// The posting-rule job it does, if any. At most one account per tenant
    /// may hold a given role; taking one that is held is a
    /// [`StoreError::Conflict`], never a silent move.
    pub role: Option<AccountRole>,
}

/// A stored account.
#[derive(Debug, Clone)]
pub struct Account {
    /// Opaque id, unique within the tenant.
    pub id: FinAccountId,
    /// The code an accountant types.
    pub code: String,
    /// What the account is called.
    pub name: String,
    /// Which of the five categories it belongs to.
    pub kind: AccountType,
    /// The posting-rule job it does, if any.
    pub role: Option<AccountRole>,
    /// Whether it takes new postings and appears in the pickers.
    pub active: bool,
    /// Whether we seeded it — renameable, never deletable.
    pub system: bool,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time.
    pub updated_at: OffsetDateTime,
}

/// A validated, normalised account ready to be bound into a statement.
#[derive(Debug)]
struct Normalized {
    code: String,
    name: String,
    kind: AccountType,
    role: String,
}

/// Validates and normalises a whole account. Pure — no database, so the rules
/// are unit-tested directly.
///
/// The code is uppercased: `ar` and `AR` are the same account to every human
/// who reads a printed chart, and storing both would produce two rows that a
/// trial balance shows as separate lines with the same label.
fn normalize(input: &NewAccount) -> Result<Normalized> {
    let code = required("account code", &input.code, ACCOUNT_CODE_MAX_CHARS)?.to_ascii_uppercase();
    if code.chars().any(char::is_whitespace) {
        return Err(StoreError::Validation(
            "account code must not contain spaces".to_owned(),
        ));
    }
    Ok(Normalized {
        code,
        name: required("account name", &input.name, ACCOUNT_NAME_MAX_CHARS)?,
        kind: input.kind,
        role: input.role.map(AccountRole::as_str).unwrap_or("").to_owned(),
    })
}

/// Turns the chart's two uniqueness violations into typed conflicts naming
/// which rule was hit, and leaves every other database failure alone.
///
/// A duplicate code and a taken role are both `409`s at the edge: the request
/// is well-formed and disagrees with the current state of the chart.
fn map_chart_conflict(error: sqlx::Error) -> StoreError {
    let constraint = match error {
        sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23505") => {
            db.constraint().unwrap_or_default().to_owned()
        }
        // Something referencing the account is what makes a delete fail with
        // 23503, and the message names which thing: a posting (B4.03a's
        // `fin_postings`) or an expense category pointing at it (B4.05a's
        // `fin_categories`). Both foreign keys restrict at the end of the
        // statement — `NO ACTION`, so a whole tenant can still be dropped — and
        // both refusals are `409`s: the chart is history and configuration, not
        // a preference.
        sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23503") => {
            return match db.constraint().unwrap_or_default() {
                "fin_categories_account_fk" => StoreError::Conflict(
                    "an account an expense category books to cannot be deleted".to_owned(),
                ),
                _ => StoreError::Conflict(
                    "an account that carries postings cannot be deleted".to_owned(),
                ),
            };
        }
        other => return StoreError::Db(other),
    };
    match constraint.as_str() {
        "fin_accounts_code_unique" => {
            StoreError::Conflict("an account with this code already exists".to_owned())
        }
        "fin_accounts_one_per_role" => {
            StoreError::Conflict("another account already holds this role".to_owned())
        }
        _ => StoreError::Conflict("unique constraint".to_owned()),
    }
}

/// Checks the seed against [`CHART`]: one non-blank name for every account the
/// chart states, and no name for a code the chart has not got.
///
/// It is *our* input rather than a caller's, so a failure here is a bug — and
/// a bug that hands a tenant half a chart is worse than one that refuses to
/// write it at all.
fn normalize_seed(seed: &ChartSeed) -> Result<Vec<(&'static ChartAccount, String)>> {
    let mut out = Vec::with_capacity(CHART.len());
    for account in CHART {
        let named = seed
            .names
            .iter()
            .find(|name| name.code == account.code)
            .ok_or_else(|| {
                StoreError::Validation(format!("no name for the {} account", account.code))
            })?;
        let name = required(
            &format!("the {} account name", account.code),
            &named.name,
            ACCOUNT_NAME_MAX_CHARS,
        )?;
        out.push((account, name));
    }
    Ok(out)
}

impl AccountStore {
    /// The tenant's chart, **seeding the default one on first use**.
    ///
    /// A tenant that has never opened Finance is given a working chart: every
    /// posting rule's role resolves from the first document, which is what
    /// makes issuing an invoice on day one book itself instead of failing with
    /// "the chart is missing the ar role".
    ///
    /// Seeding is a first-use rule, not an every-read one. A tenant that
    /// deleted the chart and typed their accountant's own is not handed ours
    /// again the next morning, because the question asked is whether the seed
    /// has ever *run* (the `fin_seeds` ledger), not whether the accounts are
    /// still there.
    ///
    /// Two first reads at the same instant produce exactly one chart: the
    /// loser of the race on the ledger's primary key writes nothing and reads
    /// back what the winner wrote.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the seed itself is malformed (a missing
    /// or blank name); [`StoreError::Db`] on failure.
    pub async fn fin_accounts_or_seed(
        &self,
        seed: &ChartSeed,
        include_inactive: bool,
    ) -> Result<Vec<Account>> {
        let accounts = normalize_seed(seed)?;
        if !self.fin_seed_ran(CHART_SEED_KEY).await? {
            match self.seed_chart(&accounts).await {
                // A concurrent first read won: its chart is the tenant's.
                Ok(()) | Err(StoreError::Conflict(_)) => {}
                Err(other) => return Err(other),
            }
        }
        self.fin_accounts(include_inactive).await
    }

    /// Whether the seed named by `system_key` has ever run for this tenant —
    /// the ledger's question, which survives the rows it wrote.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn fin_seed_ran(&self, system_key: &str) -> Result<bool> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM fin_seeds WHERE tenant_id = $1 AND system_key = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(system_key)
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)
    }

    /// Writes the ledger row and every default account in **one transaction**:
    /// a tenant is never left holding half a chart, and never left with a
    /// ledger row and no accounts.
    async fn seed_chart(&self, accounts: &[(&ChartAccount, String)]) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let claimed = sqlx::query(
            "INSERT INTO fin_seeds (tenant_id, system_key, seeded_by) \
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(CHART_SEED_KEY)
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if claimed.rows_affected() == 0 {
            // Somebody else is writing it, or already has. Nothing of ours is
            // committed, and the caller reads back their chart.
            return Ok(());
        }
        for (account, name) in accounts {
            sqlx::query(
                "INSERT INTO fin_accounts (tenant_id, id, code, name, type, role, system) \
                 VALUES ($1, $2, $3, $4, $5, $6, TRUE)",
            )
            .bind(self.tenant.as_str())
            .bind(FinAccountId::generate().as_str())
            .bind(account.code)
            .bind(name)
            .bind(account.kind.as_str())
            .bind(account.role.map(AccountRole::as_str).unwrap_or(""))
            .execute(&mut *tx)
            .await
            .map_err(map_chart_conflict)?;
        }
        tx.commit().await.map_err(StoreError::Db)
    }

    /// The tenant's chart in code order. Inactive accounts are excluded unless
    /// `include_inactive`, and then sort after the active ones.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn fin_accounts(&self, include_inactive: bool) -> Result<Vec<Account>> {
        let rows = sqlx::query_as::<_, AccountRow>(&format!(
            "SELECT {ACCOUNT_COLS} FROM fin_accounts \
             WHERE tenant_id = $1 AND ($2 OR active) \
             ORDER BY (NOT active), code"
        ))
        .bind(self.tenant.as_str())
        .bind(include_inactive)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(AccountRow::into_account).collect()
    }

    /// One account of the tenant, or `None` — including when the id belongs to
    /// another tenant (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn fin_account(&self, id: &FinAccountId) -> Result<Option<Account>> {
        let row = sqlx::query_as::<_, AccountRow>(&format!(
            "SELECT {ACCOUNT_COLS} FROM fin_accounts WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(AccountRow::into_account).transpose()
    }

    /// **The lookup every posting rule makes**: the tenant's account for a
    /// role, whether or not the tenant kept our code for it.
    ///
    /// `None` means the chart cannot answer — and the document that asked is
    /// refused, naming the role, rather than posted to suspense
    /// (`docs/design/finance.md`, "Posting rules"). An *inactive* account
    /// counts as no answer: a tenant who deactivated their bank account has
    /// said something, and quietly posting to it anyway would be us deciding
    /// they did not mean it.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn fin_account_for_role(&self, role: AccountRole) -> Result<Option<Account>> {
        self.fin_account_for_role_on(&self.pool, role).await
    }

    /// [`AccountStore::fin_account_for_role`] against any executor.
    ///
    /// A booking that runs inside a document's own transaction ([`crate::fin_booking`])
    /// resolves its roles **there**, so the accounts it posts to are the ones
    /// that transaction can actually see.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn fin_account_for_role_on<'e, E>(
        &self,
        executor: E,
        role: AccountRole,
    ) -> Result<Option<Account>>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let row = sqlx::query_as::<_, AccountRow>(&format!(
            "SELECT {ACCOUNT_COLS} FROM fin_accounts \
             WHERE tenant_id = $1 AND role = $2 AND active"
        ))
        .bind(self.tenant.as_str())
        .bind(role.as_str())
        .fetch_optional(executor)
        .await
        .map_err(StoreError::Db)?;
        row.map(AccountRow::into_account).transpose()
    }

    /// Creates a custom account — the tenant's own line, never a system one.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on any field the caller can fix (a blank or
    /// over-long code or name, a code with a space, an unknown role);
    /// [`StoreError::Conflict`] when the code is taken or the role is already
    /// held; [`StoreError::Db`] on failure.
    pub async fn create_fin_account(&self, input: &NewAccount) -> Result<FinAccountId> {
        let a = normalize(input)?;
        let id = FinAccountId::generate();
        sqlx::query(
            "INSERT INTO fin_accounts (tenant_id, id, code, name, type, role) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&a.code)
        .bind(&a.name)
        .bind(a.kind.as_str())
        .bind(&a.role)
        .execute(&self.pool)
        .await
        .map_err(map_chart_conflict)?;
        Ok(id)
    }

    /// Replaces every writable field of an account, **including a system
    /// one**: a tenant whose accountant wants `1400` for receivables must be
    /// able to say so, and the posting rules follow the role, not the code.
    ///
    /// Deactivating is a separate operation
    /// ([`AccountStore::set_fin_account_active`]) so an ordinary rename can
    /// never drop an account out of the posting rules by accident.
    ///
    /// # Errors
    /// [`StoreError::Validation`] as for create; [`StoreError::NotFound`] when
    /// the account isn't the tenant's; [`StoreError::Conflict`] when the code
    /// is taken or the role is held by another account; [`StoreError::Db`] on
    /// failure.
    pub async fn update_fin_account(&self, id: &FinAccountId, input: &NewAccount) -> Result<()> {
        let a = normalize(input)?;
        let done = sqlx::query(
            "UPDATE fin_accounts SET code = $3, name = $4, type = $5, role = $6, \
                 updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&a.code)
        .bind(&a.name)
        .bind(a.kind.as_str())
        .bind(&a.role)
        .execute(&self.pool)
        .await
        .map_err(map_chart_conflict)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deactivates or reactivates an account. Idempotent.
    ///
    /// This is the removal a chart normally wants: the account keeps its
    /// history and its balance, and stops offering itself for new work. An
    /// account holding a role that is deactivated makes its posting rule
    /// unanswerable ([`AccountStore::fin_account_for_role`]), which refuses
    /// the document loudly rather than booking it somewhere else quietly.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the account isn't the tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn set_fin_account_active(&self, id: &FinAccountId, active: bool) -> Result<()> {
        let done = sqlx::query(
            "UPDATE fin_accounts SET active = $3, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(active)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes a custom account that has never been used.
    ///
    /// Two refusals, both `409`: a **system** account is never deletable (the
    /// posting rules resolve through it, and deactivating is what a tenant who
    /// does not use one actually wants), and an account that **carries a
    /// posting** is history rather than a preference — the database enforces
    /// that second rule through `fin_postings`' foreign key (B4.03a), so it
    /// holds against a concurrent posting rather than only against a slow one.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the account isn't the tenant's;
    /// [`StoreError::Conflict`] when it is a system account or carries
    /// postings; [`StoreError::Db`] on failure.
    pub async fn delete_fin_account(&self, id: &FinAccountId) -> Result<()> {
        let account = self.fin_account(id).await?.ok_or(StoreError::NotFound)?;
        if account.system {
            return Err(StoreError::Conflict(
                "a system account cannot be deleted; deactivate it instead".to_owned(),
            ));
        }
        let done = sqlx::query("DELETE FROM fin_accounts WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(map_chart_conflict)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct AccountRow {
    id: String,
    code: String,
    name: String,
    kind: String,
    role: String,
    active: bool,
    system: bool,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl AccountRow {
    /// Reads a row back into the typed record. The two enums are re-parsed
    /// rather than trusted: a word this build does not know is a schema
    /// disagreement, and answering `500` is honest where inventing a variant
    /// would be a wrong number on a report.
    fn into_account(self) -> Result<Account> {
        Ok(Account {
            id: FinAccountId::new(self.id),
            code: self.code,
            name: self.name,
            kind: AccountType::parse(&self.kind)?,
            role: AccountRole::parse(&self.role)?,
            active: self.active,
            system: self.system,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn office() -> NewAccount {
        NewAccount {
            code: "6300".to_owned(),
            name: "Office and administration".to_owned(),
            kind: AccountType::Expense,
            role: None,
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn every_role_has_exactly_one_default_account() {
        let mut seen = HashSet::new();
        for account in CHART {
            if let Some(role) = account.role {
                assert!(
                    seen.insert(role.as_str()),
                    "two default accounts hold {}",
                    role.as_str()
                );
            }
        }
        for role in AccountRole::ALL {
            assert!(
                seen.contains(role.as_str()),
                "no default account holds {} — a posting rule would refuse \
                 every document that needs it",
                role.as_str()
            );
        }
    }

    #[test]
    fn default_codes_are_unique_and_storable() {
        let mut seen = HashSet::new();
        for account in CHART {
            assert!(seen.insert(account.code), "duplicate code {}", account.code);
            assert!(!account.code.is_empty());
            assert!(account.code.chars().count() <= ACCOUNT_CODE_MAX_CHARS);
            assert!(!account.code.chars().any(char::is_whitespace));
            // The seed writes what a caller could have typed: same normaliser,
            // same result, so a seeded account and a custom one are one shape.
            let normalized = normalize(&NewAccount {
                code: account.code.to_owned(),
                name: "x".to_owned(),
                kind: account.kind,
                role: account.role,
            })
            .unwrap_or_else(|e| panic!("the default chart is not storable: {e}"));
            assert_eq!(normalized.code, account.code);
        }
    }

    #[test]
    fn the_chart_covers_all_five_kinds() {
        for kind in [
            AccountType::Asset,
            AccountType::Liability,
            AccountType::Equity,
            AccountType::Income,
            AccountType::Expense,
        ] {
            assert!(
                CHART.iter().any(|account| account.kind == kind),
                "the default chart has no {} account",
                kind.as_str()
            );
        }
    }

    #[test]
    fn account_types_round_trip_and_reject_invention() {
        for kind in [
            AccountType::Asset,
            AccountType::Liability,
            AccountType::Equity,
            AccountType::Income,
            AccountType::Expense,
        ] {
            assert_eq!(
                AccountType::parse(kind.as_str()).unwrap_or_else(|e| panic!("rejected: {e}")),
                kind
            );
        }
        for bad in ["", "Asset", "revenue", "liabilities", "profit"] {
            assert!(
                invalid(AccountType::parse(bad)).contains("account type"),
                "expected rejection: {bad:?}"
            );
        }
    }

    #[test]
    fn balance_sheet_and_profit_and_loss_split_the_five_kinds() {
        assert!(AccountType::Asset.is_balance_sheet());
        assert!(AccountType::Liability.is_balance_sheet());
        assert!(AccountType::Equity.is_balance_sheet());
        assert!(!AccountType::Income.is_balance_sheet());
        assert!(!AccountType::Expense.is_balance_sheet());
    }

    #[test]
    fn roles_round_trip_and_blank_means_ordinary() {
        for role in AccountRole::ALL {
            assert_eq!(
                AccountRole::parse(role.as_str()).unwrap_or_else(|e| panic!("rejected: {e}")),
                Some(*role)
            );
        }
        for blank in ["", "   ", "\t"] {
            assert_eq!(
                AccountRole::parse(blank).unwrap_or(Some(AccountRole::Ar)),
                None
            );
        }
        // An invented role would create an account no posting rule can ever
        // find — a chart that looks configured and is not.
        for bad in ["receivables", "AR", "vat", "bank_account"] {
            assert!(
                invalid(AccountRole::parse(bad)).contains("account role"),
                "expected rejection: {bad:?}"
            );
        }
    }

    #[test]
    fn role_words_are_stable_and_match_the_schema_shape() {
        // The migration's CHECK accepts `^[a-z][a-z_]{0,30}$`; a variant whose
        // word breaks that would be rejected by Postgres at the first write.
        for role in AccountRole::ALL {
            let word = role.as_str();
            assert!(!word.is_empty());
            assert!(word.chars().count() <= 31);
            assert!(word.starts_with(|c: char| c.is_ascii_lowercase()));
            assert!(word.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
        }
    }

    #[test]
    fn code_is_required_uppercased_and_space_free() {
        let a = normalize(&NewAccount {
            code: "  ar-1  ".to_owned(),
            ..office()
        })
        .unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(a.code, "AR-1", "codes are compared as humans read them");

        for blank in ["", "   ", "\t\n"] {
            let msg = invalid(normalize(&NewAccount {
                code: blank.to_owned(),
                ..office()
            }));
            assert!(msg.contains("account code"), "{msg}");
        }
        let msg = invalid(normalize(&NewAccount {
            code: "1 000".to_owned(),
            ..office()
        }));
        assert!(msg.contains("spaces"), "{msg}");
        let msg = invalid(normalize(&NewAccount {
            code: "9".repeat(ACCOUNT_CODE_MAX_CHARS + 1),
            ..office()
        }));
        assert!(msg.contains("at most"), "{msg}");
        assert!(
            normalize(&NewAccount {
                code: "9".repeat(ACCOUNT_CODE_MAX_CHARS),
                ..office()
            })
            .is_ok(),
            "exactly at the bound is fine"
        );
    }

    #[test]
    fn name_is_required_and_bounded() {
        for blank in ["", "   "] {
            assert!(
                invalid(normalize(&NewAccount {
                    name: blank.to_owned(),
                    ..office()
                }))
                .contains("account name")
            );
        }
        assert!(
            invalid(normalize(&NewAccount {
                name: "x".repeat(ACCOUNT_NAME_MAX_CHARS + 1),
                ..office()
            }))
            .contains("at most")
        );
        let a = normalize(&NewAccount {
            name: "  Bürobedarf  ".to_owned(),
            ..office()
        })
        .unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(a.name, "Bürobedarf", "the name is the tenant's, not ours");
    }

    #[test]
    fn a_role_normalises_to_its_word_and_none_to_blank() {
        let a = normalize(&NewAccount {
            role: Some(AccountRole::VatOutput),
            ..office()
        })
        .unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(a.role, "vat_output");
        assert_eq!(
            normalize(&office())
                .unwrap_or_else(|e| panic!("rejected: {e}"))
                .role,
            ""
        );
    }

    /// A seed naming every default account, as the HTTP edge will build it
    /// from the caller's language.
    fn full_seed() -> ChartSeed {
        ChartSeed {
            names: CHART
                .iter()
                .map(|account| ChartName {
                    code: account.code.to_owned(),
                    name: format!("Account {}", account.code),
                })
                .collect(),
        }
    }

    #[test]
    fn the_seed_needs_a_name_for_every_default_account() {
        let ok = normalize_seed(&full_seed()).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(ok.len(), CHART.len());

        let mut short = full_seed();
        let dropped = short.names.remove(3).code;
        let msg = invalid(normalize_seed(&short));
        assert!(msg.contains(&dropped), "{msg} should name {dropped}");

        // A blank name is refused rather than filled in with the code: half a
        // chart is worse than no chart.
        let mut blank = full_seed();
        blank.names[0].name = "   ".to_owned();
        assert!(invalid(normalize_seed(&blank)).contains("must not be empty"));
    }

    #[test]
    fn the_seed_ignores_names_for_codes_the_chart_has_not_got() {
        let mut extra = full_seed();
        extra.names.push(ChartName {
            code: "9999".to_owned(),
            name: "Somebody else's account".to_owned(),
        });
        let ok = normalize_seed(&extra).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(ok.len(), CHART.len(), "the chart decides, not the caller");
    }
}
