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
//! **What is deliberately not here yet:** the call from
//! [`AccountStore::issue_billing_invoice`] (which issues credit notes too) and
//! [`AccountStore::record_billing_payment`] themselves, and the *un*-booking
//! that [`AccountStore::delete_billing_payment`] will need (a booked payment
//! that is removed has to be reversed, not forgotten — a reversal entry, which
//! `fin_journal` already supports). The note is explicit that a
//! document and its entry share one transaction, and that a posting failure
//! fails the document — which means issuing an invoice starts to depend on the
//! tenant having a chart and on the day their books opened (B4.10's periods and
//! the backfill). Wiring it before those exist would make every tenant's first
//! invoice fail on a chart they have never visited. Until then this is the
//! function the backfill and the finance routes call, and it is the same
//! function the issue path will call inside its own transaction.

use crate::account::AccountStore;
use crate::billing_fx::FxSnapshot;
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
            .ok_or_else(|| {
                StoreError::Validation(format!(
                    "this chart of accounts has no active account for the role '{}'; \
                     set one on the Accounts screen before booking documents",
                    role.as_str()
                ))
            })
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
        let snapshot = crate::billing_fx_rates::snapshot_at(
            &mut tx,
            self.tenant.as_str(),
            base_currency,
            currency,
            on,
        )
        .await
        .map_err(|error| match error {
            StoreError::Validation(_) => StoreError::Validation(format!(
                "no reference rate covers {on} for {currency}; import the rates for that day \
                 before booking this payment"
            )),
            other => other,
        });
        tx.rollback().await.map_err(StoreError::Db)?;
        snapshot
    }
}
