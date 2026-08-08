//! Expense claims — what a person spent, on what day, out of whose pocket (alo
//! Finance, ADR 0035, wave B4; `docs/design/finance.md`, "Expenses, receipts
//! and mileage"), reached through the account door and **only ever the
//! caller's own**.
//!
//! # A claim is personal data about an employee
//!
//! A receipt names a restaurant, a pharmacy, a city on a date. Every statement
//! here binds `user_id = self.user` from the account door, so reaching a
//! colleague's claim through this API is **unrepresentable, not merely
//! rejected** — there is no function that takes a user id. This is
//! [`crate::time_entries`]' rule, for a worse case, and it is answered by the
//! same door. The approver's cross-user read and the decisions themselves are
//! tenant-door work behind a role gate and arrive with the approval flow
//! (B4.05b). Merchant, description and note never reach a log: the spans on
//! this path carry ids and cent counts and nothing a human typed.
//!
//! # VAT is stated, never derived
//!
//! [`NewExpense::gross_cents`] is what the receipt totals and
//! [`NewExpense::vat_cents`] is the tax it *shows* — zero when it shows none.
//! Nothing here computes one from the other, and a category's
//! `default_vat_rate_bp` is a value the form offers, never one this module
//! applies. Reclaiming input VAT a receipt does not evidence is a false
//! statement on a return, and the difference between "the receipt does not show
//! it" and "the receipt shows zero" is exactly what a tax inspector asks about.
//!
//! The one arithmetic this module does is [`Expense::net_cents`] — a
//! subtraction of two stored integers, computed where it is displayed rather
//! than stored, so no third column can ever disagree with the two.
//!
//! # What the claim points at, and what happens when that thing goes away
//!
//! - The **category** ([`crate::fin_categories`]) decides the account. `None`
//!   is legitimate and books to the chart's `expense_default` role: a cost
//!   nobody has classified yet is still a cost, and refusing the claim would
//!   lose the receipt to protect the classification. The link is a real foreign
//!   key, so a category that has classified a claim cannot be deleted.
//! - The **project** is the B3 bridge, checked to be one the caller may work on
//!   when it is set. It carries no foreign key: deleting a board must not
//!   delete money a person is owed, and a dangling id resolves to nothing.
//! - The **receipt** is a Drive node the caller can read, checked on write for
//!   the same reason [`crate::base`] checks one. No foreign key either: purging
//!   a file must not delete the claim it evidenced.
//!
//! # Scope of this slice (B4.05a)
//!
//! Create, read, list, correct and delete, on the personal door — the model and
//! its CRUD. The four transitions (`submit`, `approve`, `reject`, `reimburse`),
//! the approver's inbox and the postings they trigger are B4.05b and B4.04's
//! rules; what this slice fixes is the vocabulary ([`ExpenseStatus`]) and the
//! rule every one of them will lean on: **a claim is editable only while it is
//! a draft**. Once it is handed in, it is a document somebody is deciding on,
//! and editing it underneath them would change what they approved.

use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::{
    DEFAULT_CURRENCY, UNIT_PRICE_MAX_CENTS, bounded, currency, vat_rate_bp,
};
use crate::error::{Result, StoreError};
use crate::id::{DriveNodeId, FinCategoryId, FinExpenseId, ProjectId, UserId};

/// The smallest claim that is a claim at all. A claim of nothing is a mistake,
/// and it would still be a row in every total.
pub const GROSS_MIN_CENTS: i64 = 1;

/// Longest merchant name we keep — a payee, not an address.
pub const MERCHANT_MAX: usize = 120;

/// Longest description we keep — a sentence about the purchase.
pub const EXPENSE_DESCRIPTION_MAX: usize = 500;

/// Longest note an approver may attach to a decision (B4.05b). Bounded here
/// because the column is this table's.
pub const EXPENSE_DECISION_NOTE_MAX: usize = 500;

/// The columns every read selects, in [`ExpenseRow`] order.
const EXPENSE_COLS: &str = "id, user_id, spent_on, category_id, merchant, description, \
     gross_cents, vat_cents, vat_rate_bp, currency, method, project_id, receipt_node_id, \
     status, submitted_at, decided_by, decided_at, decision_note, reimbursed_on, \
     created_at, updated_at";

/// Whose money paid, which is what the approval books.
///
/// A `Personal` claim credits the employee — they are owed it, and the
/// reimbursement is a second event. `Card` and `Cash` spent the company's own
/// money: nobody is owed anything, and there is nothing to reimburse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpenseMethod {
    /// The employee paid, and is owed the money.
    Personal,
    /// A company card paid.
    Card,
    /// Company petty cash paid.
    Cash,
}

impl ExpenseMethod {
    /// The stored word — the wire form and the database value, one spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Card => "card",
            Self::Cash => "cash",
        }
    }

    /// Reads the stored word back.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "personal" => Ok(Self::Personal),
            "card" => Ok(Self::Card),
            "cash" => Ok(Self::Cash),
            _ => Err(StoreError::Validation(
                "payment method must be personal, card or cash".to_owned(),
            )),
        }
    }

    /// Whether approving this claim leaves the company owing the employee. The
    /// posting rule (B4.04/B4.05b) asks exactly this question.
    pub fn owes_the_employee(self) -> bool {
        matches!(self, Self::Personal)
    }
}

/// Where a claim is in the flow.
///
/// The transitions are B4.05b's; the vocabulary and the one rule every write
/// here leans on — a draft is the only editable state — are this slice's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpenseStatus {
    /// Being written. The claimant's own, editable, in nobody's queue.
    Draft,
    /// Handed in, awaiting a decision.
    Submitted,
    /// Approved: the cost is the company's, and (for a personal payment) so is
    /// the debt to the employee.
    Approved,
    /// Refused, with a note saying why.
    Rejected,
    /// Approved and paid back.
    Reimbursed,
}

impl ExpenseStatus {
    /// The stored word.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Submitted => "submitted",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Reimbursed => "reimbursed",
        }
    }

    /// Reads the stored word back.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set. A word this build
    /// does not know is a schema disagreement, and answering `500` is honest
    /// where inventing a variant would put a claim in the wrong queue.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "draft" => Ok(Self::Draft),
            "submitted" => Ok(Self::Submitted),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            "reimbursed" => Ok(Self::Reimbursed),
            _ => Err(StoreError::Validation(
                "expense status must be draft, submitted, approved, rejected or reimbursed"
                    .to_owned(),
            )),
        }
    }

    /// Whether the claimant may still change what the claim says. Only a draft:
    /// once it is handed in, it is a document somebody is deciding on.
    pub fn is_editable(self) -> bool {
        matches!(self, Self::Draft)
    }
}

/// The writable shape of a claim, used for both create and correction (a
/// correction is a full replace — the route layer merges a partial `PATCH` onto
/// the stored record before calling, as the chart's routes do).
///
/// Neither the status nor any decision field is here, and that is the point: a
/// claimant states what they spent, and the flow states everything else.
#[derive(Debug, Clone)]
pub struct NewExpense {
    /// The day the money left, in the claimant's own zone.
    pub spent_on: Date,
    /// The word that decides the account, or `None` for "not classified yet".
    pub category_id: Option<FinCategoryId>,
    /// Who was paid. May be empty.
    pub merchant: String,
    /// What it was for. May be empty.
    pub description: String,
    /// What the receipt totals, in integer cents. At least
    /// [`GROSS_MIN_CENTS`].
    pub gross_cents: i64,
    /// The tax the receipt **shows**, in integer cents — zero when it shows
    /// none. Never derived from the gross (see the module header).
    pub vat_cents: i64,
    /// The rate beside that tax, in basis points. Required when
    /// [`Self::vat_cents`] is non-zero: a return line is a rate and a figure.
    pub vat_rate_bp: Option<i32>,
    /// The currency of both amounts. `None` takes [`DEFAULT_CURRENCY`].
    pub currency: Option<String>,
    /// Whose money paid.
    pub method: ExpenseMethod,
    /// The engagement this cost belongs to (the B3 bridge), if any.
    pub project_id: Option<ProjectId>,
    /// The receipt in Drive, if one has been attached.
    pub receipt_node_id: Option<DriveNodeId>,
}

impl NewExpense {
    /// The minimum a claim states: a day, an amount and whose money paid.
    /// Everything else is detail a person adds, or a machine proposes and a
    /// person confirms (B4.06).
    pub fn spent(spent_on: Date, gross_cents: i64, method: ExpenseMethod) -> Self {
        Self {
            spent_on,
            category_id: None,
            merchant: String::new(),
            description: String::new(),
            gross_cents,
            vat_cents: 0,
            vat_rate_bp: None,
            currency: None,
            method,
            project_id: None,
            receipt_node_id: None,
        }
    }
}

/// One expense claim.
#[derive(Debug, Clone)]
pub struct Expense {
    /// Opaque id, unique within the tenant.
    pub id: FinExpenseId,
    /// Whose claim. Always the caller, on this door.
    pub user_id: UserId,
    /// The day the money left.
    pub spent_on: Date,
    /// The category that decides the account, if one was picked.
    pub category_id: Option<FinCategoryId>,
    /// Who was paid. Personal data: never logged.
    pub merchant: String,
    /// What it was for. Personal data: never logged.
    pub description: String,
    /// What the receipt totals, in integer cents.
    pub gross_cents: i64,
    /// The tax the receipt shows, in integer cents.
    pub vat_cents: i64,
    /// The rate beside that tax, in basis points.
    pub vat_rate_bp: Option<i32>,
    /// The currency of both amounts.
    pub currency: String,
    /// Whose money paid.
    pub method: ExpenseMethod,
    /// The engagement this cost belongs to, if any. A dangling id (the board
    /// was deleted) resolves to nothing and does not affect the claim.
    pub project_id: Option<ProjectId>,
    /// The receipt in Drive, if one is attached.
    pub receipt_node_id: Option<DriveNodeId>,
    /// Where the claim is in the flow.
    pub status: ExpenseStatus,
    /// When it was handed in (B4.05b).
    pub submitted_at: Option<OffsetDateTime>,
    /// Who decided it (B4.05b).
    pub decided_by: Option<UserId>,
    /// When they decided (B4.05b).
    pub decided_at: Option<OffsetDateTime>,
    /// What they said. Personal data: never logged.
    pub decision_note: String,
    /// The day the money was paid back (B4.05b).
    pub reimbursed_on: Option<Date>,
    /// When the claim was written.
    pub created_at: OffsetDateTime,
    /// When it was last changed.
    pub updated_at: OffsetDateTime,
}

impl Expense {
    /// The cost without the tax, in integer cents — what books to the expense
    /// account, with the rest going to `vat_input`.
    ///
    /// Derived rather than stored, from two columns the database keeps
    /// consistent (`vat_cents <= gross_cents`), so it cannot drift from them.
    #[must_use]
    pub fn net_cents(&self) -> i64 {
        self.gross_cents - self.vat_cents
    }

    /// Whether the claimant may still change what this says
    /// ([`ExpenseStatus::is_editable`]).
    #[must_use]
    pub fn is_editable(&self) -> bool {
        self.status.is_editable()
    }
}

/// A validated, normalised claim ready to be bound into a statement.
#[derive(Debug, PartialEq, Eq)]
struct Normalized {
    merchant: String,
    description: String,
    gross_cents: i64,
    vat_cents: i64,
    vat_rate_bp: Option<i32>,
    currency: String,
}

/// Validates and normalises the fields that need no database. Pure, so every
/// rule below is unit-tested directly; the category, project and receipt need
/// their own tables and are checked by [`AccountStore::require_links`].
///
/// Three money rules, in the order a receipt is read:
///
/// 1. The gross is a real amount — at least a cent, at most the typo ceiling
///    every other alo money field carries.
/// 2. The VAT is part of that gross, so it cannot exceed it. This is the one
///    arithmetic claim a receipt cannot make.
/// 3. A VAT amount carries the rate it was charged at. A figure with no rate
///    cannot go on a return line, and guessing the rate from the amount is the
///    derivation this module exists to refuse.
fn normalize(input: &NewExpense) -> Result<Normalized> {
    if !(GROSS_MIN_CENTS..=UNIT_PRICE_MAX_CENTS).contains(&input.gross_cents) {
        return Err(StoreError::Validation(format!(
            "the amount must be between {GROSS_MIN_CENTS} and {UNIT_PRICE_MAX_CENTS} cents"
        )));
    }
    if input.vat_cents < 0 {
        return Err(StoreError::Validation(
            "the VAT amount must not be negative".to_owned(),
        ));
    }
    if input.vat_cents > input.gross_cents {
        return Err(StoreError::Validation(
            "the VAT amount must not exceed the total on the receipt".to_owned(),
        ));
    }
    let vat_rate_bp = input.vat_rate_bp.map(vat_rate_bp).transpose()?;
    if input.vat_cents > 0 && vat_rate_bp.unwrap_or(0) == 0 {
        return Err(StoreError::Validation(
            "state the VAT rate the receipt shows beside the VAT amount".to_owned(),
        ));
    }
    Ok(Normalized {
        merchant: bounded("merchant", &input.merchant, MERCHANT_MAX)?,
        description: bounded("description", &input.description, EXPENSE_DESCRIPTION_MAX)?,
        gross_cents: input.gross_cents,
        vat_cents: input.vat_cents,
        vat_rate_bp,
        currency: match input.currency.as_deref() {
            Some(stated) => currency(stated)?,
            None => DEFAULT_CURRENCY.to_owned(),
        },
    })
}

impl AccountStore {
    /// Records a claim of the caller's own. It starts as a draft: nothing is in
    /// anybody's queue until the claimant hands it in (B4.05b).
    ///
    /// # Errors
    /// [`StoreError::Validation`] when an amount, a string or the currency
    /// breaks its rule; [`StoreError::NotFound`] when the category, the project
    /// or the receipt is not one the caller can reach — existence is never
    /// disclosed; [`StoreError::Db`] on failure.
    pub async fn log_expense(&self, new: &NewExpense) -> Result<Expense> {
        let e = normalize(new)?;
        self.require_links(new).await?;
        let id = FinExpenseId::generate();
        let row = sqlx::query_as::<_, ExpenseRow>(&format!(
            "INSERT INTO fin_expenses (tenant_id, id, user_id, spent_on, category_id, merchant, \
                 description, gross_cents, vat_cents, vat_rate_bp, currency, method, project_id, \
                 receipt_node_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) \
             RETURNING {EXPENSE_COLS}"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .bind(new.spent_on)
        .bind(new.category_id.as_ref().map(FinCategoryId::as_str))
        .bind(&e.merchant)
        .bind(&e.description)
        .bind(e.gross_cents)
        .bind(e.vat_cents)
        .bind(e.vat_rate_bp)
        .bind(&e.currency)
        .bind(new.method.as_str())
        .bind(new.project_id.as_ref().map(ProjectId::as_str))
        .bind(new.receipt_node_id.as_ref().map(DriveNodeId::as_str))
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.into_expense()
    }

    /// One of the caller's **own** claims, or `None`.
    ///
    /// A colleague's claim inside the same tenant reads exactly like another
    /// tenant's and like one that never existed: absent. Not a `Forbidden`,
    /// which would confirm that somebody claimed something that day.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Validation`] if the stored
    /// method or status is a word this build does not know.
    pub async fn expense(&self, id: &FinExpenseId) -> Result<Option<Expense>> {
        let row = sqlx::query_as::<_, ExpenseRow>(&format!(
            "SELECT {EXPENSE_COLS} FROM fin_expenses \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(ExpenseRow::into_expense).transpose()
    }

    /// The caller's own claims between `from` and `to`, both days included,
    /// newest purchase first, optionally narrowed to one status.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the period ends before it starts;
    /// [`StoreError::Db`] on failure.
    pub async fn expenses(
        &self,
        from: Date,
        to: Date,
        status: Option<ExpenseStatus>,
    ) -> Result<Vec<Expense>> {
        if to < from {
            return Err(StoreError::Validation(
                "the end of the period must not be before its start".to_owned(),
            ));
        }
        let rows = sqlx::query_as::<_, ExpenseRow>(&format!(
            "SELECT {EXPENSE_COLS} FROM fin_expenses \
             WHERE tenant_id = $1 AND user_id = $2 AND spent_on >= $3 AND spent_on <= $4 \
               AND ($5::text IS NULL OR status = $5) \
             ORDER BY spent_on DESC, created_at DESC, id"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(from)
        .bind(to)
        .bind(status.map(ExpenseStatus::as_str))
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(ExpenseRow::into_expense).collect()
    }

    /// Corrects one of the caller's own **draft** claims.
    ///
    /// A claim that has been handed in is frozen: an approver is looking at it,
    /// and changing what it says underneath them would change what they
    /// approved. Withdrawing it back to a draft is B4.05b's verb.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the claim is not the caller's own, or a
    /// link is not one they can reach; [`StoreError::Conflict`] when it is no
    /// longer a draft; [`StoreError::Validation`] when a field breaks its rule;
    /// [`StoreError::Db`] on failure.
    pub async fn edit_expense(&self, id: &FinExpenseId, edit: &NewExpense) -> Result<Expense> {
        let claim = self.expense(id).await?.ok_or(StoreError::NotFound)?;
        require_draft(&claim, "changed")?;
        let e = normalize(edit)?;
        self.require_links(edit).await?;
        let row = sqlx::query_as::<_, ExpenseRow>(&format!(
            "UPDATE fin_expenses SET spent_on = $4, category_id = $5, merchant = $6, \
                 description = $7, gross_cents = $8, vat_cents = $9, vat_rate_bp = $10, \
                 currency = $11, method = $12, project_id = $13, receipt_node_id = $14, \
                 updated_at = now() \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3 AND status = '{draft}' \
             RETURNING {EXPENSE_COLS}",
            draft = ExpenseStatus::Draft.as_str()
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .bind(edit.spent_on)
        .bind(edit.category_id.as_ref().map(FinCategoryId::as_str))
        .bind(&e.merchant)
        .bind(&e.description)
        .bind(e.gross_cents)
        .bind(e.vat_cents)
        .bind(e.vat_rate_bp)
        .bind(&e.currency)
        .bind(edit.method.as_str())
        .bind(edit.project_id.as_ref().map(ProjectId::as_str))
        .bind(edit.receipt_node_id.as_ref().map(DriveNodeId::as_str))
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?
        // The status is re-tested inside the statement, so a submit that lands
        // between the read and the write wins the race rather than being
        // overwritten by an edit that never saw it.
        .ok_or_else(|| {
            StoreError::Conflict(
                "a claim that has been handed in cannot be changed; withdraw it first".to_owned(),
            )
        })?;
        row.into_expense()
    }

    /// Removes one of the caller's own claims that nobody has acted on.
    ///
    /// A **draft** is the claimant's alone. A **rejected** claim is one the
    /// company has declined to pay, and the claimant is the person who clears
    /// it — refusing that too would leave a refused claim stuck in their list
    /// forever with no verb that removes it. Everything else is a document in
    /// somebody's queue or in the books.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the claim is not the caller's own;
    /// [`StoreError::Conflict`] when it is submitted, approved or reimbursed;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_expense(&self, id: &FinExpenseId) -> Result<()> {
        let claim = self.expense(id).await?.ok_or(StoreError::NotFound)?;
        if !matches!(claim.status, ExpenseStatus::Draft | ExpenseStatus::Rejected) {
            return Err(StoreError::Conflict(
                "a claim that has been handed in cannot be deleted; withdraw it first".to_owned(),
            ));
        }
        let done = sqlx::query(&format!(
            "DELETE FROM fin_expenses \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3 \
               AND status IN ('{draft}', '{rejected}')",
            draft = ExpenseStatus::Draft.as_str(),
            rejected = ExpenseStatus::Rejected.as_str()
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            // Somebody handed it in between the read and the delete.
            return Err(StoreError::Conflict(
                "a claim that has been handed in cannot be deleted; withdraw it first".to_owned(),
            ));
        }
        Ok(())
    }

    /// Confirms every thing a claim points at is one the caller can reach: the
    /// category is the tenant's and offered, the project is a board they may
    /// work on, and the receipt is a file they can open.
    ///
    /// All three answer `NotFound` when they are somebody else's, never a
    /// refusal that would confirm the thing exists.
    async fn require_links(&self, new: &NewExpense) -> Result<()> {
        if let Some(category) = new.category_id.as_ref() {
            let found = self
                .fin_category(category)
                .await?
                .ok_or(StoreError::NotFound)?;
            // An inactive category may stay on a claim that already carried it
            // (a cost does not become uncategorised because nobody may pick
            // that word again), but it cannot be picked afresh.
            if !found.active {
                return Err(StoreError::Validation(
                    "that category is no longer offered".to_owned(),
                ));
            }
        }
        if let Some(project) = new.project_id.as_ref() {
            // The engagement rate this answers with is B3's business; what is
            // needed here is the visibility rule it enforces — a board the
            // caller may work on, and not archived.
            self.writable_project(project).await?;
        }
        if let Some(receipt) = new.receipt_node_id.as_ref() {
            self.drive_require_read(receipt).await?;
        }
        Ok(())
    }
}

/// Refuses a write to a claim that is no longer the claimant's alone, naming
/// the verb that would make it one again.
fn require_draft(claim: &Expense, verb: &str) -> Result<()> {
    if claim.is_editable() {
        return Ok(());
    }
    Err(StoreError::Conflict(format!(
        "a claim that has been handed in cannot be {verb}; withdraw it first"
    )))
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct ExpenseRow {
    id: String,
    user_id: String,
    spent_on: Date,
    category_id: Option<String>,
    merchant: String,
    description: String,
    gross_cents: i64,
    vat_cents: i64,
    vat_rate_bp: Option<i32>,
    currency: String,
    method: String,
    project_id: Option<String>,
    receipt_node_id: Option<String>,
    status: String,
    submitted_at: Option<OffsetDateTime>,
    decided_by: Option<String>,
    decided_at: Option<OffsetDateTime>,
    decision_note: String,
    reimbursed_on: Option<Date>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl ExpenseRow {
    /// Reads a row back into the typed record. The two enums are re-parsed
    /// rather than trusted: a word this build does not know is a schema
    /// disagreement, and answering `500` is honest where inventing a variant
    /// would put a claim in the wrong queue.
    fn into_expense(self) -> Result<Expense> {
        Ok(Expense {
            id: FinExpenseId::new(self.id),
            user_id: UserId::new(self.user_id),
            spent_on: self.spent_on,
            category_id: self.category_id.map(FinCategoryId::new),
            merchant: self.merchant,
            description: self.description,
            gross_cents: self.gross_cents,
            vat_cents: self.vat_cents,
            vat_rate_bp: self.vat_rate_bp,
            currency: self.currency,
            method: ExpenseMethod::parse(&self.method)?,
            project_id: self.project_id.map(ProjectId::new),
            receipt_node_id: self.receipt_node_id.map(DriveNodeId::new),
            status: ExpenseStatus::parse(&self.status)?,
            submitted_at: self.submitted_at,
            decided_by: self.decided_by.map(UserId::new),
            decided_at: self.decided_at,
            decision_note: self.decision_note,
            reimbursed_on: self.reimbursed_on,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    fn day() -> Date {
        Date::from_calendar_date(2026, Month::March, 14).unwrap_or(Date::MIN)
    }

    /// A receipt for €119.00 showing €19.00 of VAT at 19 % — the case the
    /// module header is written about.
    fn receipt() -> NewExpense {
        NewExpense {
            vat_cents: 1900,
            vat_rate_bp: Some(1900),
            merchant: "Bahn".to_owned(),
            description: "Berlin → München".to_owned(),
            ..NewExpense::spent(day(), 11_900, ExpenseMethod::Personal)
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    fn ok(input: &NewExpense) -> Normalized {
        normalize(input).unwrap_or_else(|e| panic!("rejected: {e}"))
    }

    #[test]
    fn a_receipt_is_stored_as_it_reads() {
        let e = ok(&receipt());
        assert_eq!(e.gross_cents, 11_900);
        assert_eq!(e.vat_cents, 1900);
        assert_eq!(e.vat_rate_bp, Some(1900));
        assert_eq!(e.currency, "EUR", "the default when none is stated");
        assert_eq!(e.merchant, "Bahn");
    }

    #[test]
    fn a_receipt_showing_only_a_total_books_no_vat() {
        // The module's central rule: no rate, no reclaim, and the claim is
        // still perfectly valid.
        let e = ok(&NewExpense::spent(day(), 4250, ExpenseMethod::Card));
        assert_eq!(e.gross_cents, 4250);
        assert_eq!(e.vat_cents, 0);
        assert_eq!(e.vat_rate_bp, None);
    }

    #[test]
    fn a_vat_amount_without_its_rate_is_refused() {
        let msg = invalid(normalize(&NewExpense {
            vat_rate_bp: None,
            ..receipt()
        }));
        assert!(msg.contains("VAT rate"), "{msg}");
        // A rate of zero with a non-zero amount is the same false statement
        // wearing a number.
        let msg = invalid(normalize(&NewExpense {
            vat_rate_bp: Some(0),
            ..receipt()
        }));
        assert!(msg.contains("VAT rate"), "{msg}");
        // The other way round is legitimate: a 0 % (exempt, reverse-charge)
        // purchase states its rate and shows no tax.
        let e = ok(&NewExpense {
            vat_cents: 0,
            vat_rate_bp: Some(0),
            ..receipt()
        });
        assert_eq!((e.vat_cents, e.vat_rate_bp), (0, Some(0)));
    }

    #[test]
    fn vat_is_part_of_the_gross_and_never_more_than_it() {
        let msg = invalid(normalize(&NewExpense {
            gross_cents: 1000,
            vat_cents: 1001,
            ..receipt()
        }));
        assert!(msg.contains("exceed"), "{msg}");
        let msg = invalid(normalize(&NewExpense {
            vat_cents: -1,
            ..receipt()
        }));
        assert!(msg.contains("negative"), "{msg}");
        // All of it being tax is arithmetically possible (a pure-VAT correction
        // line on a receipt) and is not this module's business to refuse.
        assert!(
            normalize(&NewExpense {
                gross_cents: 1900,
                ..receipt()
            })
            .is_ok()
        );
    }

    #[test]
    fn the_amount_is_a_real_one_and_bounded() {
        for bad in [0, -1] {
            let msg = invalid(normalize(&NewExpense {
                gross_cents: bad,
                vat_cents: 0,
                vat_rate_bp: None,
                ..receipt()
            }));
            assert!(msg.contains("amount must be between"), "{msg}");
        }
        let msg = invalid(normalize(&NewExpense {
            gross_cents: UNIT_PRICE_MAX_CENTS + 1,
            vat_cents: 0,
            vat_rate_bp: None,
            ..receipt()
        }));
        assert!(msg.contains("amount must be between"), "{msg}");
        assert!(
            normalize(&NewExpense {
                gross_cents: UNIT_PRICE_MAX_CENTS,
                vat_cents: 0,
                vat_rate_bp: None,
                ..receipt()
            })
            .is_ok(),
            "exactly at the bound is fine"
        );
    }

    #[test]
    fn strings_are_trimmed_and_bounded() {
        let e = ok(&NewExpense {
            merchant: "  Bahn  ".to_owned(),
            ..receipt()
        });
        assert_eq!(e.merchant, "Bahn");
        let msg = invalid(normalize(&NewExpense {
            merchant: "x".repeat(MERCHANT_MAX + 1),
            ..receipt()
        }));
        assert!(msg.contains("merchant"), "{msg}");
        let msg = invalid(normalize(&NewExpense {
            description: "x".repeat(EXPENSE_DESCRIPTION_MAX + 1),
            ..receipt()
        }));
        assert!(msg.contains("description"), "{msg}");
        // Both may be empty: a receipt with no legible payee is still a claim.
        assert!(
            normalize(&NewExpense {
                merchant: String::new(),
                description: String::new(),
                ..receipt()
            })
            .is_ok()
        );
    }

    #[test]
    fn the_currency_is_validated_and_uppercased() {
        assert_eq!(
            ok(&NewExpense {
                currency: Some("chf".to_owned()),
                ..receipt()
            })
            .currency,
            "CHF"
        );
        let msg = invalid(normalize(&NewExpense {
            currency: Some("EURO".to_owned()),
            ..receipt()
        }));
        assert!(msg.contains("ISO 4217"), "{msg}");
    }

    #[test]
    fn methods_and_statuses_round_trip_and_reject_invention() {
        for method in [
            ExpenseMethod::Personal,
            ExpenseMethod::Card,
            ExpenseMethod::Cash,
        ] {
            assert_eq!(
                ExpenseMethod::parse(method.as_str()).unwrap_or_else(|e| panic!("rejected: {e}")),
                method
            );
        }
        for bad in ["", "Personal", "credit", "bank"] {
            assert!(invalid(ExpenseMethod::parse(bad)).contains("payment method"));
        }
        for status in [
            ExpenseStatus::Draft,
            ExpenseStatus::Submitted,
            ExpenseStatus::Approved,
            ExpenseStatus::Rejected,
            ExpenseStatus::Reimbursed,
        ] {
            assert_eq!(
                ExpenseStatus::parse(status.as_str()).unwrap_or_else(|e| panic!("rejected: {e}")),
                status
            );
        }
        for bad in ["", "Draft", "paid", "pending"] {
            assert!(invalid(ExpenseStatus::parse(bad)).contains("expense status"));
        }
    }

    #[test]
    fn only_a_personal_payment_leaves_the_company_owing_somebody() {
        assert!(ExpenseMethod::Personal.owes_the_employee());
        assert!(!ExpenseMethod::Card.owes_the_employee());
        assert!(!ExpenseMethod::Cash.owes_the_employee());
    }

    #[test]
    fn a_draft_is_the_only_editable_state() {
        assert!(ExpenseStatus::Draft.is_editable());
        for frozen in [
            ExpenseStatus::Submitted,
            ExpenseStatus::Approved,
            ExpenseStatus::Rejected,
            ExpenseStatus::Reimbursed,
        ] {
            assert!(!frozen.is_editable(), "{} is editable", frozen.as_str());
        }
    }
}
