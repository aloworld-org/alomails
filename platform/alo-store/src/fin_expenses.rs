//! Expense claims — what a person spent, on what day, out of whose pocket (alo
//! Finance, ADR 0035, wave B4; `docs/design/finance.md`, "Expenses, receipts
//! and mileage"), reached through the account door and **only ever the
//! caller's own**.
//!
//! # A claim is personal data about an employee
//!
//! A receipt names a restaurant, a pharmacy, a city on a date. Every statement
//! on the **personal door** ([`AccountStore`]) binds `user_id = self.user`, so
//! reaching a colleague's claim through it is **unrepresentable, not merely
//! rejected** — no function there takes a user id. This is
//! [`crate::time_entries`]' rule, for a worse case, and it is answered by the
//! same door.
//!
//! The **approver's door** ([`TenantStore`]) crosses that line on purpose and
//! only behind a role gate at the edge (`Account::require_admin`, the decision
//! [`crate::time_weeks`] recorded for hours and this module inherits). It is
//! deliberately the narrowest cross-user surface the module has: the claims
//! awaiting a decision, who made each and what it books to, and the four
//! statements that decide them. Merchant, description and decision note never
//! reach a log: the spans on this path carry ids and cent counts and nothing a
//! human typed.
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
//! # The flow, and the one rule under all of it
//!
//! ```text
//!  draft ──submit──> submitted ──approve──> approved ──reimburse──> reimbursed
//!    ^                   │  │                                (personal money only)
//!    └──withdraw─────────┘  └──reject──> rejected ──submit──> submitted
//!                                            │
//!    (draft and rejected are the claimant's own: editable, deletable, submittable)
//! ```
//!
//! **A claim is the claimant's to change while nobody is deciding it, and
//! frozen the moment somebody is.** [`ExpenseStatus::is_editable`] is that
//! sentence: draft and rejected yes, submitted and approved and reimbursed no.
//! A rejection is editable on purpose — the whole point of refusing a claim is
//! that the person fixes it and hands it in again, and a refused claim that
//! could only be deleted and retyped would lose the receipt link and the note
//! explaining it. (B4.05a shipped this predicate as draft-only, with its test
//! deferred to this slice because nothing could yet set another status;
//! widening it to the rejection is the same call [`crate::time_weeks`] made for
//! a refused week, and no claim anybody has approved becomes editable by it.)
//!
//! What is **not** here: the postings an approval writes (`employee_payable`
//! for money the employee is owed, `bank` for the company's own card — the rule
//! in `docs/design/finance.md` § "Posting rules"). B4.04's rules are pure
//! functions not yet wired into any document verb, and an expense that booked
//! at approval while an issued invoice still did not would make the ledger read
//! half-live. It lands with the rest of them.

use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::{
    DEFAULT_CURRENCY, UNIT_PRICE_MAX_CENTS, bounded, currency, vat_rate_bp,
};
use crate::error::{Result, StoreError};
use crate::id::{DriveNodeId, FinCategoryId, FinExpenseId, ProjectId, UserId};
use crate::store::TenantStore;

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

/// Most claims one read of the approvals inbox returns. A queue longer than
/// this is a paging question, and answering it in full would be a page nobody
/// works through anyway.
const PENDING_LIMIT: i64 = 500;

/// The statuses in which a claim is still its claimant's own, as a SQL list —
/// the `WHERE` half of [`ExpenseStatus::is_editable`], spelled once so the
/// predicate and the statements cannot drift apart.
const CLAIMANTS_STATUSES: &str = "'draft', 'rejected'";

/// The columns every read selects, in [`ExpenseRow`] order.
pub(crate) const EXPENSE_COLS: &str = "id, user_id, spent_on, category_id, merchant, description, \
     gross_cents, vat_cents, vat_rate_bp, currency, method, project_id, receipt_node_id, \
     status, submitted_at, decided_by, decided_at, decision_note, reimbursed_on, \
     proposed_category_id, proposed_at, proposed_reason, proposal_declined_at, \
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

/// Where a claim is in the flow (the diagram in the module header).
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

    /// Whether the claim is still the claimant's own — to correct, to remove
    /// and to hand in.
    ///
    /// Draft and rejected. Once it is submitted it is a document somebody is
    /// deciding on, and changing it underneath them would change what they
    /// approved; once it is approved the company owes money on it. A rejection
    /// is the claimant's again by design (module header).
    pub fn is_editable(self) -> bool {
        matches!(self, Self::Draft | Self::Rejected)
    }

    /// Whether the claimant may hand this claim in.
    ///
    /// Exactly [`Self::is_editable`], and not by coincidence: a claim you may
    /// still change is a claim nobody is deciding, which is the same claim you
    /// may hand in. Named separately because the two questions read differently
    /// at their call sites.
    pub fn can_submit(self) -> bool {
        self.is_editable()
    }

    /// Whether the claimant may take this claim back out of the queue. Only one
    /// that is waiting: an approved claim is not theirs to unmake, and a draft
    /// or a rejection is already theirs.
    pub fn can_withdraw(self) -> bool {
        matches!(self, Self::Submitted)
    }

    /// Whether an approver may still decide this claim — only one that is
    /// waiting for them. Deciding a decided claim is not a transition this
    /// module has: the claimant resubmits a rejection, and an approval that was
    /// wrong is a matter for the books, not for a status flip.
    pub fn can_decide(self) -> bool {
        matches!(self, Self::Submitted)
    }

    /// Whether this claim can be marked paid back. Only an approved one: money
    /// is not repaid against a claim nobody has agreed to.
    pub fn can_reimburse(self) -> bool {
        matches!(self, Self::Approved)
    }
}

/// What an approver decided about a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpenseDecision {
    /// Yes: the cost is the company's, and — when the employee's own money paid
    /// — so is the debt to them.
    Approve,
    /// No, with a note saying why. The claim goes back to being the claimant's
    /// own, so they can correct it and hand it in again.
    Reject,
}

impl ExpenseDecision {
    /// The status a claim reaches when this decision is recorded.
    pub fn resulting_status(self) -> ExpenseStatus {
        match self {
            Self::Approve => ExpenseStatus::Approved,
            Self::Reject => ExpenseStatus::Rejected,
        }
    }
}

/// The writable shape of a claim, used for both create and correction (a
/// correction is a full replace — `finance_expenses`' `PATCH` merges the stated
/// fields onto the stored record before calling, so a field left out of a
/// request keeps its value and an explicit `null` clears one).
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
    /// What the agent SUGGESTS this claim books to (B4.14a,
    /// [`crate::fin_categorise`]), if anything. Deliberately a different field
    /// from [`Self::category_id`], and read by nothing but the screen that
    /// offers it: a guess in the decided column would be in the P&L.
    pub proposed_category_id: Option<FinCategoryId>,
    /// When that suggestion was made.
    pub proposed_at: Option<OffsetDateTime>,
    /// Why it was suggested, as a machine-readable code — never a sentence.
    /// Empty when there is no suggestion.
    pub proposed_reason: String,
    /// When the claimant declined a suggestion on this claim. Kept after the
    /// suggestion itself is cleared, so nothing offers the same word again.
    pub proposal_declined_at: Option<OffsetDateTime>,
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

    /// Whether the claim is still the claimant's own to change
    /// ([`ExpenseStatus::is_editable`]).
    #[must_use]
    pub fn is_editable(&self) -> bool {
        self.status.is_editable()
    }
}

/// One claim waiting in the approvals inbox: the claim, who made it, and the
/// word that says where it books.
///
/// The two joined fields are what an approver needs and the account door cannot
/// answer — a colleague's address, and the category's name rather than its
/// opaque id. Nothing else crosses: no other claim of that person's, no
/// history, no totals about them.
#[derive(Debug, Clone)]
pub struct PendingExpense {
    /// The claim itself.
    pub expense: Expense,
    /// The claimant's address — what an inbox shows instead of an opaque id.
    /// Empty when the user record has since been removed.
    pub user_email: String,
    /// The category's name, when the claim carries one. `None` is a claim
    /// nobody has classified, which books to the chart's `expense_default`.
    pub category_name: Option<String>,
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
    /// The links are checked before the fields, so a claim pointing at somebody
    /// else's category is `NotFound` whatever else is wrong with it — a refusal
    /// that never becomes a way to ask which of two mistakes was made.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when an amount, a string or the currency
    /// breaks its rule; [`StoreError::NotFound`] when the category, the project
    /// or the receipt is not one the caller can reach — existence is never
    /// disclosed; [`StoreError::Db`] on failure.
    pub async fn log_expense(&self, new: &NewExpense) -> Result<Expense> {
        self.require_links(new).await?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // A failure drops `tx`, which rolls it back: there is no half-written
        // claim to clean up.
        let claim =
            insert_expense_in(&mut tx, self.tenant.as_str(), self.user.as_str(), new).await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(claim)
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

    /// Corrects one of the caller's own claims that nobody is deciding.
    ///
    /// A claim that has been handed in is frozen: an approver is looking at it,
    /// and changing what it says underneath them would change what they
    /// approved. [`Self::withdraw_expense`] takes it back out of the queue
    /// first. A **rejected** claim is editable — the point of a refusal is that
    /// the person fixes it and hands it in again.
    ///
    /// Any edit clears an open **suggestion** ([`crate::fin_categorise`]): it
    /// was made about the claim as it stood, and a suggestion about a merchant
    /// that has since changed is an offer about a different purchase. The
    /// claimant's earlier "no" is not cleared — that was about the suggesting,
    /// not about the claim.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the claim is not the caller's own, or a
    /// link is not one they can reach; [`StoreError::Conflict`] when somebody is
    /// deciding it or has; [`StoreError::Validation`] when a field breaks its
    /// rule; [`StoreError::Db`] on failure.
    pub async fn edit_expense(&self, id: &FinExpenseId, edit: &NewExpense) -> Result<Expense> {
        let claim = self.expense(id).await?.ok_or(StoreError::NotFound)?;
        require_claimants(&claim, "changed")?;
        let e = normalize(edit)?;
        self.require_links(edit).await?;
        let row = sqlx::query_as::<_, ExpenseRow>(&format!(
            "UPDATE fin_expenses SET spent_on = $4, category_id = $5, merchant = $6, \
                 description = $7, gross_cents = $8, vat_cents = $9, vat_rate_bp = $10, \
                 currency = $11, method = $12, project_id = $13, receipt_node_id = $14, \
                 proposed_category_id = NULL, proposed_at = NULL, proposed_reason = '', \
                 updated_at = now() \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3 AND status IN ({claimants}) \
             RETURNING {EXPENSE_COLS}",
            claimants = CLAIMANTS_STATUSES
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
        .ok_or_else(|| handed_in("changed"))?;
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
        require_claimants(&claim, "deleted")?;
        let done = sqlx::query(&format!(
            "DELETE FROM fin_expenses \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3 \
               AND status IN ({CLAIMANTS_STATUSES})"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            // Somebody handed it in between the read and the delete.
            return Err(handed_in("deleted"));
        }
        Ok(())
    }

    /// Hands one of the caller's own claims in for a decision.
    ///
    /// One statement, whose `WHERE` clause is the state machine: a claim that
    /// is already waiting, approved or paid back moves nothing and is read back
    /// to say what it actually is. Handing a **rejected** claim in again clears
    /// the old decision, because a decision that no longer stands must not still
    /// be displayed on the record — the history of it is in the audit log, which
    /// is what an append-only log is for.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the claim is not the caller's own;
    /// [`StoreError::Conflict`] when it is not the claimant's to hand in, naming
    /// what it is; [`StoreError::Db`] on failure.
    pub async fn submit_expense(&self, id: &FinExpenseId) -> Result<Expense> {
        let row = sqlx::query_as::<_, ExpenseRow>(&format!(
            "UPDATE fin_expenses \
                SET status = '{submitted}', submitted_at = now(), decided_by = NULL, \
                    decided_at = NULL, decision_note = '', updated_at = now() \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3 AND status IN ({CLAIMANTS_STATUSES}) \
             RETURNING {EXPENSE_COLS}",
            submitted = ExpenseStatus::Submitted.as_str()
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        match row {
            Some(row) => row.into_expense(),
            None => Err(self.claim_refusal(id, "handed in").await),
        }
    }

    /// Takes one of the caller's own waiting claims back out of the queue.
    ///
    /// Only one nobody has decided. An approved claim is not the claimant's to
    /// unmake — the company owes money on it, and the way back is the approver's
    /// — and a draft or a rejection is already theirs.
    ///
    /// `submitted_at` is cleared, which is what the schema's
    /// `fin_expenses_submitted_when_past_draft` expects of a draft: a claim that
    /// is not in a queue was not handed in.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the claim is not the caller's own;
    /// [`StoreError::Conflict`] when it is not waiting for a decision, naming
    /// what it is; [`StoreError::Db`] on failure.
    pub async fn withdraw_expense(&self, id: &FinExpenseId) -> Result<Expense> {
        let row = sqlx::query_as::<_, ExpenseRow>(&format!(
            "UPDATE fin_expenses \
                SET status = '{draft}', submitted_at = NULL, updated_at = now() \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3 AND status = '{submitted}' \
             RETURNING {EXPENSE_COLS}",
            draft = ExpenseStatus::Draft.as_str(),
            submitted = ExpenseStatus::Submitted.as_str()
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        match row {
            Some(row) => row.into_expense(),
            None => Err(self.claim_refusal(id, "withdrawn").await),
        }
    }

    /// Names, in a refusal, what the claim actually is — read after a statement
    /// declined to move it.
    ///
    /// A second read rather than a guess: "already waiting for a decision" and
    /// "already approved" want different answers from the person reading them.
    /// A claim that is not the caller's own is [`StoreError::NotFound`] and
    /// never a conflict, so no refusal here is an existence oracle.
    async fn claim_refusal(&self, id: &FinExpenseId, verb: &str) -> StoreError {
        match self.expense(id).await {
            Ok(None) => StoreError::NotFound,
            Ok(Some(claim)) => StoreError::Conflict(format!(
                "this claim is {} and cannot be {verb}",
                claim.status.as_str()
            )),
            Err(error) => error,
        }
    }

    /// Confirms every thing a claim points at is one the caller can reach: the
    /// category is the tenant's and offered, the project is a board they may
    /// work on, and the receipt is a file they can open.
    ///
    /// All three answer `NotFound` when they are somebody else's, never a
    /// refusal that would confirm the thing exists.
    ///
    /// Visible to the crate because [`crate::fin_mileage`] writes a claim of its
    /// own and must apply exactly these rules — a journey pointing at a
    /// colleague's category would otherwise be the one way into this table that
    /// skips them.
    pub(crate) async fn require_links(&self, new: &NewExpense) -> Result<()> {
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

/// Validates a claim and writes it, as a draft, inside a caller-supplied
/// transaction.
///
/// The one place the `INSERT` lives. [`AccountStore::log_expense`] is a
/// transaction around it, and [`crate::fin_mileage`] calls it in the same
/// transaction that writes the journey the claim came from — a journey whose
/// claim did not land, or a claim with no journey to explain it, are both states
/// this atomicity makes unreachable.
///
/// The caller owns the two things this cannot see: that the links are the
/// caller's ([`AccountStore::require_links`]) and that `user` is the
/// authenticated user rather than request input.
///
/// # Errors
/// [`StoreError::Validation`] when an amount, a string or the currency breaks
/// its rule; [`StoreError::Db`] on failure.
pub(crate) async fn insert_expense_in(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    user: &str,
    new: &NewExpense,
) -> Result<Expense> {
    let e = normalize(new)?;
    let id = FinExpenseId::generate();
    let row = sqlx::query_as::<_, ExpenseRow>(&format!(
        "INSERT INTO fin_expenses (tenant_id, id, user_id, spent_on, category_id, merchant, \
             description, gross_cents, vat_cents, vat_rate_bp, currency, method, project_id, \
             receipt_node_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) \
         RETURNING {EXPENSE_COLS}"
    ))
    .bind(tenant)
    .bind(id.as_str())
    .bind(user)
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
    .fetch_one(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    row.into_expense()
}

/// Refuses a write to a claim that is no longer the claimant's alone, naming
/// the verb that would make it one again.
fn require_claimants(claim: &Expense, verb: &str) -> Result<()> {
    if claim.is_editable() {
        return Ok(());
    }
    Err(handed_in(verb))
}

/// The refusal a write to a handed-in claim reads, in one place because the
/// check and the statement's own `WHERE` clause both produce it — the second
/// when a submit lands between the read and the write.
fn handed_in(verb: &str) -> StoreError {
    StoreError::Conflict(format!(
        "a claim that has been handed in cannot be {verb}; withdraw it first"
    ))
}

impl TenantStore {
    /// Every claim of this tenant awaiting a decision, oldest purchase first —
    /// the approvals inbox. **Admin only**, gated at the edge by
    /// `Account::require_admin`.
    ///
    /// It crosses the personal-data line the account door exists to hold, so it
    /// is deliberately the narrowest cross-user read the module has: the
    /// waiting claims, their claimants' addresses, and the category each books
    /// to. Nothing about anybody's other claims, and no totals per person.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Validation`] if a stored
    /// method or status is a word this build does not know.
    pub async fn pending_expenses(&self) -> Result<Vec<PendingExpense>> {
        self.expense_queue(
            &format!(
                "e.status = '{submitted}'",
                submitted = ExpenseStatus::Submitted.as_str()
            ),
            "e.spent_on, e.submitted_at, e.id",
        )
        .await
    }

    /// Every claim of this tenant the company has approved and **still owes the
    /// employee**, oldest decision first — the payer's queue. **Admin or
    /// accountant**, gated at the edge by `Account::require_finance`.
    ///
    /// Two conditions, and the second is the one a status filter alone would
    /// get wrong: a claim a company card or petty cash paid left nobody owed
    /// anything ([`ExpenseMethod::owes_the_employee`]), and
    /// [`TenantStore::reimburse_expense`] refuses one — so listing it here
    /// would be a queue of work that cannot be done, with no way to clear it.
    ///
    /// It is the same narrow cross-user read [`TenantStore::pending_expenses`]
    /// is, for the same reason and with the same three facts: the claim, whose
    /// it is, and what it books to.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Validation`] if a stored
    /// method or status is a word this build does not know.
    pub async fn reimbursable_expenses(&self) -> Result<Vec<PendingExpense>> {
        self.expense_queue(
            &format!(
                "e.status = '{approved}' AND e.method = '{personal}'",
                approved = ExpenseStatus::Approved.as_str(),
                personal = ExpenseMethod::Personal.as_str()
            ),
            "e.decided_at, e.spent_on, e.id",
        )
        .await
    }

    /// The one joined read behind both queues: the claims a `predicate` selects,
    /// with the claimant's address and the category's name beside each.
    ///
    /// `predicate` and `order` are **code-authored SQL fragments** — the two
    /// callers above are the only ones and both build theirs from the enums'
    /// own words. Nothing a caller of the crate types reaches this string; the
    /// tenant is bound, as every statement in the crate binds it.
    async fn expense_queue(&self, predicate: &str, order: &str) -> Result<Vec<PendingExpense>> {
        let rows = sqlx::query_as::<_, PendingRow>(&format!(
            "SELECT {expense}, COALESCE(u.email, '') AS user_email, c.name AS category_name \
             FROM fin_expenses e \
             LEFT JOIN users u ON u.tenant_id = e.tenant_id AND u.id = e.user_id \
             LEFT JOIN fin_categories c ON c.tenant_id = e.tenant_id AND c.id = e.category_id \
             WHERE e.tenant_id = $1 AND {predicate} \
             ORDER BY {order} LIMIT $2",
            expense = expense_cols_prefixed("e"),
        ))
        .bind(self.tenant().as_str())
        .bind(PENDING_LIMIT)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(PendingRow::into_pending).collect()
    }

    /// One of this tenant's claims by id, whoever it belongs to — **admin
    /// only**, the read behind every decision below.
    ///
    /// Another tenant's id is `None`, exactly like one that was never issued.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Validation`] if a stored
    /// word is one this build does not know.
    pub async fn expense_by_id(&self, id: &FinExpenseId) -> Result<Option<Expense>> {
        let row = sqlx::query_as::<_, ExpenseRow>(&format!(
            "SELECT {EXPENSE_COLS} FROM fin_expenses WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        row.map(ExpenseRow::into_expense).transpose()
    }

    /// Records an approver's decision on a waiting claim — **admin only**.
    ///
    /// An approval fixes the cost as the company's; a rejection hands the claim
    /// back to its claimant, editable, so they can correct it and submit again.
    /// `approver` is the authenticated caller and never request input.
    ///
    /// An admin may decide their own claim: a one-person tenant has nobody else,
    /// and the audit entry records who it was — the rule
    /// [`TenantStore::decide_week`] states for a timesheet.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the claim is not this tenant's;
    /// [`StoreError::Conflict`] when it is not awaiting a decision;
    /// [`StoreError::Validation`] when the note is too long;
    /// [`StoreError::Db`] on failure.
    pub async fn decide_expense(
        &self,
        id: &FinExpenseId,
        decision: ExpenseDecision,
        approver: &UserId,
        note: &str,
    ) -> Result<Expense> {
        let note = bounded("decision note", note, EXPENSE_DECISION_NOTE_MAX)?;
        let row = sqlx::query_as::<_, ExpenseRow>(&format!(
            "UPDATE fin_expenses \
                SET status = $3, decided_by = $4, decided_at = now(), decision_note = $5, \
                    updated_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND status = '{submitted}' \
             RETURNING {EXPENSE_COLS}",
            submitted = ExpenseStatus::Submitted.as_str()
        ))
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(decision.resulting_status().as_str())
        .bind(approver.as_str())
        .bind(&note)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        match row {
            Some(row) => row.into_expense(),
            None => Err(self.expense_decision_refusal(id, "decided").await),
        }
    }

    /// Marks an approved claim paid back, on the day the money moved —
    /// **admin only**.
    ///
    /// Two rules, both refusals rather than silent no-ops:
    ///
    /// - Only an **approved** claim. Money is not repaid against a claim nobody
    ///   agreed to.
    /// - Only one the **employee's own money** paid
    ///   ([`ExpenseMethod::owes_the_employee`]). A company card or petty cash
    ///   left nobody owed anything, so there is nothing to pay back, and
    ///   recording a reimbursement against one would book money out of the bank
    ///   twice.
    ///
    /// The day is the caller's, never the server's clock: it is the date the
    /// reimbursement books on, and a day chosen by whichever zone a container
    /// runs in is a posting in the wrong period.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the claim is not this tenant's;
    /// [`StoreError::Conflict`] when it is not approved, or nobody is owed
    /// anything on it; [`StoreError::Db`] on failure.
    pub async fn reimburse_expense(&self, id: &FinExpenseId, paid_on: Date) -> Result<Expense> {
        let claim = self.expense_by_id(id).await?.ok_or(StoreError::NotFound)?;
        if !claim.status.can_reimburse() {
            return Err(StoreError::Conflict(format!(
                "this claim is {} and cannot be marked reimbursed",
                claim.status.as_str()
            )));
        }
        if !claim.method.owes_the_employee() {
            return Err(StoreError::Conflict(
                "the company's own money paid this claim, so there is nobody to reimburse"
                    .to_owned(),
            ));
        }
        let row = sqlx::query_as::<_, ExpenseRow>(&format!(
            "UPDATE fin_expenses \
                SET status = '{reimbursed}', reimbursed_on = $3, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND status = '{approved}' AND method = '{personal}' \
             RETURNING {EXPENSE_COLS}",
            reimbursed = ExpenseStatus::Reimbursed.as_str(),
            approved = ExpenseStatus::Approved.as_str(),
            personal = ExpenseMethod::Personal.as_str()
        ))
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(paid_on)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        match row {
            Some(row) => row.into_expense(),
            // Somebody decided it between the read and the write.
            None => Err(self.expense_decision_refusal(id, "marked reimbursed").await),
        }
    }

    /// Names, in a refusal, why a decision statement moved nothing. A claim that
    /// is not this tenant's is [`StoreError::NotFound`] and never a conflict.
    async fn expense_decision_refusal(&self, id: &FinExpenseId, verb: &str) -> StoreError {
        match self.expense_by_id(id).await {
            Ok(None) => StoreError::NotFound,
            Ok(Some(claim)) => StoreError::Conflict(format!(
                "this claim is {} and cannot be {verb}",
                claim.status.as_str()
            )),
            Err(error) => error,
        }
    }
}

/// [`EXPENSE_COLS`] qualified with a table alias, for the reads that join a
/// claim to something else — the approvals inbox here, and the journey that
/// became a claim in [`crate::fin_mileage`].
pub(crate) fn expense_cols_prefixed(alias: &str) -> String {
    EXPENSE_COLS
        .split(',')
        .map(|column| format!("{alias}.{}", column.trim()))
        .collect::<Vec<_>>()
        .join(", ")
}

// ---- row types --------------------------------------------------------------

/// A claim exactly as the table holds it.
///
/// Visible to the crate so a joined read elsewhere can flatten it and get the
/// same [`Expense`] the module's own reads produce — the alternative, a second
/// hand-written mapping, is how two readings of one row start to disagree.
#[derive(sqlx::FromRow)]
pub(crate) struct ExpenseRow {
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
    proposed_category_id: Option<String>,
    proposed_at: Option<OffsetDateTime>,
    proposed_reason: String,
    proposal_declined_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl ExpenseRow {
    /// Reads a row back into the typed record. The two enums are re-parsed
    /// rather than trusted: a word this build does not know is a schema
    /// disagreement, and answering `500` is honest where inventing a variant
    /// would put a claim in the wrong queue.
    pub(crate) fn into_expense(self) -> Result<Expense> {
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
            proposed_category_id: self.proposed_category_id.map(FinCategoryId::new),
            proposed_at: self.proposed_at,
            proposed_reason: self.proposed_reason,
            proposal_declined_at: self.proposal_declined_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct PendingRow {
    #[sqlx(flatten)]
    expense: ExpenseRow,
    user_email: String,
    category_name: Option<String>,
}

impl PendingRow {
    fn into_pending(self) -> Result<PendingExpense> {
        Ok(PendingExpense {
            expense: self.expense.into_expense()?,
            user_email: self.user_email,
            category_name: self.category_name,
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
    fn a_claim_is_its_claimants_own_until_somebody_is_deciding_it() {
        // A rejection goes back to the claimant: the whole point of refusing a
        // claim is that the person fixes it and hands it in again.
        for theirs in [ExpenseStatus::Draft, ExpenseStatus::Rejected] {
            assert!(theirs.is_editable(), "{} is frozen", theirs.as_str());
            assert!(
                theirs.can_submit(),
                "{} cannot be handed in",
                theirs.as_str()
            );
        }
        for frozen in [
            ExpenseStatus::Submitted,
            ExpenseStatus::Approved,
            ExpenseStatus::Reimbursed,
        ] {
            assert!(!frozen.is_editable(), "{} is editable", frozen.as_str());
            assert!(!frozen.can_submit(), "{} can be handed in", frozen.as_str());
        }
    }

    #[test]
    fn the_transitions_are_the_state_machine_and_nothing_else() {
        // Only a waiting claim can be taken back, and only by its claimant.
        assert!(ExpenseStatus::Submitted.can_withdraw());
        for no in [
            ExpenseStatus::Draft,
            ExpenseStatus::Approved,
            ExpenseStatus::Rejected,
            ExpenseStatus::Reimbursed,
        ] {
            assert!(!no.can_withdraw(), "{} can be withdrawn", no.as_str());
        }

        // Only a waiting claim is decidable — a rejection is resubmitted by its
        // claimant, not re-decided by an approver.
        assert!(ExpenseStatus::Submitted.can_decide());
        for no in [
            ExpenseStatus::Draft,
            ExpenseStatus::Approved,
            ExpenseStatus::Rejected,
            ExpenseStatus::Reimbursed,
        ] {
            assert!(!no.can_decide(), "{} can be decided", no.as_str());
        }

        // Only an approved claim can be paid back, and paying it back is the
        // end of the line.
        assert!(ExpenseStatus::Approved.can_reimburse());
        for no in [
            ExpenseStatus::Draft,
            ExpenseStatus::Submitted,
            ExpenseStatus::Rejected,
            ExpenseStatus::Reimbursed,
        ] {
            assert!(!no.can_reimburse(), "{} can be reimbursed", no.as_str());
        }
    }

    #[test]
    fn a_decision_names_the_state_it_produces() {
        assert_eq!(
            ExpenseDecision::Approve.resulting_status(),
            ExpenseStatus::Approved
        );
        assert_eq!(
            ExpenseDecision::Reject.resulting_status(),
            ExpenseStatus::Rejected
        );
        // The asymmetry the flow rests on: a refusal hands the claim back,
        // an approval keeps it.
        assert!(ExpenseDecision::Reject.resulting_status().is_editable());
        assert!(!ExpenseDecision::Approve.resulting_status().is_editable());
    }

    #[test]
    fn the_editable_statuses_are_spelled_the_same_way_in_sql_and_in_code() {
        let listed: Vec<String> = CLAIMANTS_STATUSES
            .split(',')
            .map(|word| word.trim().trim_matches('\'').to_owned())
            .collect();
        let predicate: Vec<String> = [
            ExpenseStatus::Draft,
            ExpenseStatus::Submitted,
            ExpenseStatus::Approved,
            ExpenseStatus::Rejected,
            ExpenseStatus::Reimbursed,
        ]
        .into_iter()
        .filter(|status| status.is_editable())
        .map(|status| status.as_str().to_owned())
        .collect();
        assert_eq!(
            listed, predicate,
            "the statements and the predicate disagree about whose claim it is"
        );
    }

    #[test]
    fn the_joined_inbox_read_qualifies_every_column_it_selects() {
        let prefixed = expense_cols_prefixed("e");
        assert!(prefixed.starts_with("e.id, e.user_id, e.spent_on"));
        assert_eq!(
            prefixed.split(", ").count(),
            EXPENSE_COLS.split(',').count(),
            "no column is dropped or duplicated by the prefixing"
        );
        assert!(
            prefixed.split(", ").all(|column| column.starts_with("e.")),
            "an unqualified column would be ambiguous against the joined tables: {prefixed}"
        );
    }

    #[test]
    fn a_refusal_to_write_a_handed_in_claim_names_the_way_back() {
        match handed_in("changed") {
            StoreError::Conflict(message) => {
                assert!(message.contains("cannot be changed"), "{message}");
                assert!(message.contains("withdraw it first"), "{message}");
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }
}
