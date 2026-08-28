//! Booking a document — the step between a posting rule and the journal (ADR
//! 0035, wave B4.04; `docs/design/finance.md`, "Posting rules, per document
//! type").
//!
//! [`crate::fin_rules`] says *what* an invoice does to the books and knows
//! nothing about a database; [`crate::fin_journal`] writes an entry and knows
//! nothing about invoices. This file is the one that reads the document,
//! resolves the accounts **by role**, applies the rule and posts what it
//! returns — one function per document event, so a caller books a document
//! rather than assembling one.
//!
//! Two behaviours belong to this layer rather than to either neighbour:
//!
//! - **A missing role refuses the document.** If the chart has no receivable
//!   account, booking is a [`StoreError::Validation`] naming the role, which
//!   the route edge renders as a `422` pointing at the Accounts screen. Never a
//!   posting to `suspense`: that account is for money whose owner is unknown,
//!   not for a configuration mistake nobody would find until the year end.
//! - **Booking is idempotent, and says so.** The journal's
//!   `UNIQUE (tenant_id, source_kind, source_id, source_event)` is what makes a
//!   retry, a double-click or a re-run of the backfill a
//!   [`StoreError::Conflict`] instead of a second set of postings — and
//!   [`AccountStore::fin_invoice_entry`] is the question a caller asks when it
//!   would rather look than catch.
//!
//! **The document paths call this layer in-process** (B7.01): issuing a
//! document — invoice or credit note — books it inside the issue's own
//! transaction ([`AccountStore::book_issue_in`], called by
//! [`AccountStore::issue_billing_invoice`]), recording a payment books its
//! settlement the same way ([`AccountStore::book_payment_in`], called by
//! [`AccountStore::record_billing_payment`]), and the corrections un-book:
//! voiding posts a reversal of the issue entry, deleting a booked payment posts
//! a reversal of its settlement. A posting failure fails the document — an
//! issue that cannot book rolls back whole and burns no number — and a document
//! from before this wiring is **backfilled** the moment something needs its
//! entry (a payment, a credit note, a bank match), at the document's own issue
//! date, exactly as [`crate::bank_reconcile`] has always done. The public
//! `post_*` doors below remain for the finance routes and read as an explicit
//! backfill; calling one on a document the wiring already booked is the
//! `Conflict` the idempotency key exists to give.

use crate::account::AccountStore;
use crate::billing_fx::FxSnapshot;
use crate::billing_invoices::InvoiceDocument;
use crate::billing_payments::Payment;
use crate::error::{Result, StoreError};
use crate::fin_accounts::AccountRole;
use crate::fin_journal::{EntrySource, SourceEvent, SourceKind};
use crate::fin_rules::{
    InvoiceAccounts, PaymentAccounts, credit_note_entry, credit_note_original, invoice_issue_entry,
    payment_settle_entry, payment_settlement_role, settlement_needs_exchange_account,
};
use crate::id::{BillingInvoiceId, BillingPaymentId, FinAccountId, FinEntryId};

impl AccountStore {
    /// The account this tenant's chart gives a role, or a refusal naming the
    /// role that is missing.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the role when no active account holds
    /// it; [`StoreError::Db`] on failure.
    pub(crate) async fn fin_account_required(&self, role: AccountRole) -> Result<FinAccountId> {
        self.fin_account_for_role(role)
            .await?
            .map(|account| account.id)
            .ok_or_else(|| missing_role(role))
    }

    /// [`AccountStore::fin_account_required`], inside a transaction the caller
    /// owns — the form the document paths use, so the chart a booking resolves
    /// against is the one its own transaction sees.
    ///
    /// # Errors
    /// [`AccountStore::fin_account_required`]'s.
    async fn fin_account_required_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        role: AccountRole,
    ) -> Result<FinAccountId> {
        self.fin_account_for_role_on(&mut **tx, role)
            .await?
            .map(|account| account.id)
            .ok_or_else(|| missing_role(role))
    }

    /// The three sales-side roles resolved together, inside the caller's
    /// transaction — every issue-shaped booking needs exactly this set.
    async fn invoice_accounts_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<InvoiceAccounts> {
        Ok(InvoiceAccounts {
            ar: self.fin_account_required_in(tx, AccountRole::Ar).await?,
            revenue: self
                .fin_account_required_in(tx, AccountRole::Revenue)
                .await?,
            vat_output: self
                .fin_account_required_in(tx, AccountRole::VatOutput)
                .await?,
        })
    }

    /// **Books an issued document inside the transaction that issued it** —
    /// the B7.01 wiring of `docs/design/finance.md`'s rule that "the posting
    /// happens inside the document's own transaction".
    ///
    /// The document is read by the caller *after* its issue `UPDATE`, so the
    /// entry books the number, dates and frozen rate that transaction is about
    /// to commit. An ordinary invoice books the invoice rule; a credit note
    /// books the mirror, naming the original's entry — and when the original
    /// predates this wiring and is not in the books, its issue entry is
    /// **backfilled first**, at the original's own issue date, exactly as a
    /// confirmed bank match has always done ([`crate::bank_reconcile`]).
    ///
    /// Any refusal here fails the whole issue: the caller's transaction rolls
    /// back, the drawn number returns to the sequence, and the document stays
    /// a draft.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the chart is missing a role or the
    /// document cannot be restated into the accounting currency;
    /// [`StoreError::Conflict`] when the entry's date falls in a closed period,
    /// or the document event is somehow already posted; [`StoreError::NotFound`]
    /// when a credit note's original has vanished; [`StoreError::Db`] on
    /// failure.
    pub(crate) async fn book_issue_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        document: &InvoiceDocument,
        base_currency: &str,
    ) -> Result<FinEntryId> {
        let accounts = self.invoice_accounts_in(tx).await?;
        if document.invoice.is_credit_note {
            let original_id = credit_note_original(document)?.clone();
            let reverses = match self.fin_invoice_entry_in(tx, &original_id).await? {
                Some(entry) => entry,
                None => {
                    let original = self
                        .billing_invoice_in(tx, &original_id)
                        .await?
                        .ok_or(StoreError::NotFound)?;
                    let entry = invoice_issue_entry(&original, base_currency, &accounts)?;
                    self.post_fin_entry_in(tx, &entry).await?
                }
            };
            let entry = credit_note_entry(document, base_currency, &accounts, &reverses)?;
            self.post_fin_entry_in(tx, &entry).await
        } else {
            let entry = invoice_issue_entry(document, base_currency, &accounts)?;
            self.post_fin_entry_in(tx, &entry).await
        }
    }

    /// **Books a recorded payment inside the transaction that recorded it** —
    /// the settlement leg of the B7.01 wiring, called by
    /// [`AccountStore::record_billing_payment`] after the row is written.
    ///
    /// The document is read inside the transaction, so the payment sequence the
    /// relief telescopes over includes the row just inserted. An invoice from
    /// before this wiring is backfilled first, like everywhere else.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the chart is missing a role, no
    /// reference rate covers the day the money arrived, or the document cannot
    /// be restated; [`StoreError::Conflict`] when the entry's date falls in a
    /// closed period; [`StoreError::NotFound`] when the document or payment has
    /// vanished; [`StoreError::Db`] on failure.
    pub(crate) async fn book_payment_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        invoice_id: &BillingInvoiceId,
        payment_id: &BillingPaymentId,
    ) -> Result<FinEntryId> {
        let document = self
            .billing_invoice_in(tx, invoice_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        let base_currency =
            crate::billing_settings::base_currency_in(tx, self.tenant.as_str()).await?;

        if self.fin_invoice_entry_in(tx, invoice_id).await?.is_none() {
            self.book_issue_in(tx, &document, &base_currency).await?;
        }

        let payments = self.billing_payments_on(&mut **tx, invoice_id).await?;
        let (payment, paid_before_cents) =
            crate::billing_payments::payment_in_sequence(payments, payment_id)?;
        let settled_at = self
            .settlement_rate_in(
                tx,
                &document.invoice.currency,
                &base_currency,
                payment.paid_on,
            )
            .await?;
        let accounts = PaymentAccounts {
            settled_into: self
                .fin_account_required_in(tx, payment_settlement_role(&payment.method))
                .await?,
            ar: self.fin_account_required_in(tx, AccountRole::Ar).await?,
            fx_diff: if settlement_needs_exchange_account(&document, &base_currency) {
                Some(
                    self.fin_account_required_in(tx, AccountRole::FxDiff)
                        .await?,
                )
            } else {
                None
            },
        };
        let entry = payment_settle_entry(
            &payment,
            &document,
            paid_before_cents,
            &base_currency,
            &settled_at,
            &accounts,
        )?;
        self.post_fin_entry_in(tx, &entry).await
    }

    /// [`AccountStore::fin_invoice_entry`], inside a transaction the caller
    /// owns.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn fin_invoice_entry_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &BillingInvoiceId,
    ) -> Result<Option<FinEntryId>> {
        self.fin_entry_for_source_on(
            &mut **tx,
            &EntrySource {
                kind: SourceKind::Invoice,
                id: id.as_str().to_owned(),
                event: SourceEvent::Issue,
            },
        )
        .await
    }

    /// **Books an issued invoice**: the receivable against the revenue and the
    /// output tax it is made of ([`crate::fin_rules::invoice_issue_entry`]).
    ///
    /// The whole path in one sentence: read the document under this tenant's
    /// handle, take the currency the books are kept in, resolve `ar`, `revenue`
    /// and `vat_output` by role, apply the rule, and post the entry it returns
    /// in one transaction.
    ///
    /// Calling it twice is safe and is *not* silent: the second call is a
    /// [`StoreError::Conflict`], because a caller that retried after a timeout
    /// and a caller that clicked twice want different answers from "it is
    /// already booked" and "it was booked now".
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the invoice is not this tenant's —
    /// including when it is another tenant's, which is indistinguishable by
    /// design; [`StoreError::Conflict`] when the document is a draft, a void
    /// one, a credit note, or is already booked; [`StoreError::Validation`]
    /// when the chart is missing a role, or the document cannot be restated
    /// into the accounting currency; [`StoreError::Db`] on failure.
    pub async fn post_invoice_issue(&self, id: &BillingInvoiceId) -> Result<FinEntryId> {
        let document = self
            .billing_invoice(id)
            .await?
            .ok_or(StoreError::NotFound)?;
        let base_currency = self.billing_base_currency().await?;
        let accounts = InvoiceAccounts {
            ar: self.fin_account_required(AccountRole::Ar).await?,
            revenue: self.fin_account_required(AccountRole::Revenue).await?,
            vat_output: self.fin_account_required(AccountRole::VatOutput).await?,
        };
        let entry = invoice_issue_entry(&document, &base_currency, &accounts)?;
        self.post_fin_entry(&entry).await
    }

    /// **Books an issued credit note**: the exact mirror of the document it
    /// corrects ([`crate::fin_rules::credit_note_entry`]).
    ///
    /// The path is the invoice's, with one read in front of it: the original's
    /// entry, which the credit note's own entry names as the one it corrects
    /// (`fin_entries.reverses_entry_id`, so a journal reader walks from a
    /// correction to what it corrected without guessing from the memo).
    ///
    /// **The original must be in the books first.** Mirroring a receivable that
    /// was never booked leaves the customer owing a negative amount and every
    /// aged-debtors report wrong, so an unbooked original is a
    /// [`StoreError::Conflict`] naming what to do — the same rule, for the same
    /// reason, as a payment refusing to settle an unbooked invoice.
    ///
    /// The entry is read back with [`AccountStore::fin_invoice_entry`] like any
    /// other document's: a credit note is an invoice row, and its entry is
    /// keyed on its own id.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the credit note is not this tenant's;
    /// [`StoreError::Conflict`] when the document is an ordinary invoice, is a
    /// draft or void, is already booked, or its original is not booked;
    /// [`StoreError::Validation`] when the chart is missing a role, or the
    /// document cannot be restated into the accounting currency;
    /// [`StoreError::Db`] on failure.
    pub async fn post_credit_note_issue(&self, id: &BillingInvoiceId) -> Result<FinEntryId> {
        let document = self
            .billing_invoice(id)
            .await?
            .ok_or(StoreError::NotFound)?;
        let original_id = credit_note_original(&document)?;
        let reverses = self.fin_invoice_entry(original_id).await?.ok_or_else(|| {
            StoreError::Conflict(
                "the invoice this credit note corrects is not in the books yet; book the \
                     invoice before its credit notes"
                    .to_owned(),
            )
        })?;

        let base_currency = self.billing_base_currency().await?;
        let accounts = InvoiceAccounts {
            ar: self.fin_account_required(AccountRole::Ar).await?,
            revenue: self.fin_account_required(AccountRole::Revenue).await?,
            vat_output: self.fin_account_required(AccountRole::VatOutput).await?,
        };
        let entry = credit_note_entry(&document, &base_currency, &accounts, &reverses)?;
        self.post_fin_entry(&entry).await
    }

    /// The entry an invoice's issue already produced, or `None` — the "is this
    /// document in the books?" a screen and a backfill both ask. A credit note
    /// is one of these too: it is an invoice row, booked under its own id.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn fin_invoice_entry(&self, id: &BillingInvoiceId) -> Result<Option<FinEntryId>> {
        self.fin_entry_for_source(&EntrySource {
            kind: SourceKind::Invoice,
            id: id.as_str().to_owned(),
            event: SourceEvent::Issue,
        })
        .await
    }

    /// **Books a recorded payment**: the money where it landed, against the
    /// receivable it relieves ([`crate::fin_rules::payment_settle_entry`]).
    ///
    /// The path: read the document and its payments under this tenant's handle,
    /// establish where this payment sits in that sequence, take the rate the
    /// accounting currency actually received the money at, resolve the accounts
    /// by role — `bank` or `cash` by the method, `ar`, and `fx_diff` when the
    /// document is in a foreign currency — apply the rule and post it.
    ///
    /// **The invoice must be in the books first.** Relieving a receivable that
    /// was never booked would leave the customer's ledger negative and every
    /// aged-debtors report wrong, so an unbooked invoice is a
    /// [`StoreError::Conflict`] naming what to do rather than a posting nobody
    /// can explain. Booking the same payment twice is a `Conflict` too, from
    /// the journal's own idempotency key.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the invoice or the payment is absent or
    /// another tenant's; [`StoreError::Conflict`] when the invoice is not
    /// booked, is a draft, void or a credit note, or the payment is already
    /// posted; [`StoreError::Validation`] when the chart is missing a role, or
    /// no reference rate covers the day the money arrived;
    /// [`StoreError::Db`] on failure.
    pub async fn post_payment_settle(
        &self,
        invoice_id: &BillingInvoiceId,
        payment_id: &BillingPaymentId,
    ) -> Result<FinEntryId> {
        let document = self
            .billing_invoice(invoice_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        let (payment, paid_before_cents) = self.payment_in_sequence(invoice_id, payment_id).await?;
        if self.fin_invoice_entry(invoice_id).await?.is_none() {
            return Err(StoreError::Conflict(
                "the invoice this payment settles is not in the books yet; book the invoice \
                 before its payments"
                    .to_owned(),
            ));
        }

        let base_currency = self.billing_base_currency().await?;
        let settled_at = self
            .settlement_rate(&document.invoice.currency, &base_currency, payment.paid_on)
            .await?;
        let accounts = PaymentAccounts {
            settled_into: self
                .fin_account_required(payment_settlement_role(&payment.method))
                .await?,
            ar: self.fin_account_required(AccountRole::Ar).await?,
            // Required exactly when a difference can arise: a chart without an
            // `fx_diff` account must not refuse an ordinary euro payment over a
            // role that payment's rule will never reach for.
            fx_diff: if settlement_needs_exchange_account(&document, &base_currency) {
                Some(self.fin_account_required(AccountRole::FxDiff).await?)
            } else {
                None
            },
        };
        let entry = payment_settle_entry(
            &payment,
            &document,
            paid_before_cents,
            &base_currency,
            &settled_at,
            &accounts,
        )?;
        self.post_fin_entry(&entry).await
    }

    /// The entry a payment already produced, or `None`.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn fin_payment_entry(&self, id: &BillingPaymentId) -> Result<Option<FinEntryId>> {
        self.fin_entry_for_source(&EntrySource {
            kind: SourceKind::Payment,
            id: id.as_str().to_owned(),
            event: SourceEvent::Settle,
        })
        .await
    }

    /// One of a document's payments, with the sum of the payments that come
    /// before it — [`crate::billing_payments::payment_in_sequence`] over this
    /// document's rows.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the payment is not one of this document's
    /// — including when the document is another tenant's, which reads as an
    /// empty list; [`StoreError::Db`] on failure.
    async fn payment_in_sequence(
        &self,
        invoice_id: &BillingInvoiceId,
        payment_id: &BillingPaymentId,
    ) -> Result<(Payment, i64)> {
        let payments = self.billing_payments(invoice_id).await?;
        crate::billing_payments::payment_in_sequence(payments, payment_id)
    }

    /// The rate the accounting currency received a payment at: the reference
    /// rate published for the day the money arrived, or the identity when the
    /// document is already in the currency the books are kept in.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when no usable rate has been imported for
    /// that day — the payment is refused rather than booked at a guessed rate,
    /// exactly as issuing a document is; [`StoreError::Db`] on failure.
    pub(crate) async fn settlement_rate(
        &self,
        currency: &str,
        base_currency: &str,
        on: time::Date,
    ) -> Result<FxSnapshot> {
        if currency == base_currency {
            return Ok(FxSnapshot::identity(base_currency, on));
        }
        // A read-only transaction, because the rate lookup is written to run
        // inside the transaction that freezes a rate onto a document; nothing
        // here freezes anything, so it is opened and rolled back.
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let snapshot = self
            .settlement_rate_in(&mut tx, currency, base_currency, on)
            .await;
        tx.rollback().await.map_err(StoreError::Db)?;
        snapshot
    }

    /// [`AccountStore::settlement_rate`], inside a transaction the caller owns
    /// — the form [`AccountStore::book_payment_in`] uses.
    ///
    /// # Errors
    /// Exactly [`AccountStore::settlement_rate`]'s.
    async fn settlement_rate_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        currency: &str,
        base_currency: &str,
        on: time::Date,
    ) -> Result<FxSnapshot> {
        if currency == base_currency {
            return Ok(FxSnapshot::identity(base_currency, on));
        }
        crate::billing_fx_rates::snapshot_at(tx, self.tenant.as_str(), base_currency, currency, on)
            .await
            .map_err(|error| match error {
                StoreError::Validation(_) => StoreError::Validation(format!(
                    "no reference rate covers {on} for {currency}; import the rates for that day \
                     before booking this payment"
                )),
                other => other,
            })
    }
}

/// The refusal a chart that cannot answer a role gets, worded once for the
/// pool door and the in-transaction door alike.
fn missing_role(role: AccountRole) -> StoreError {
    StoreError::Validation(format!(
        "this chart of accounts has no active account for the role '{}'; \
         set one on the Accounts screen before booking documents",
        role.as_str()
    ))
}
