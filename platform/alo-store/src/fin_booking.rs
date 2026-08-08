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
//! [`AccountStore::issue_billing_invoice`] itself. The note is explicit that a
//! document and its entry share one transaction, and that a posting failure
//! fails the document — which means issuing an invoice starts to depend on the
//! tenant having a chart and on the day their books opened (B4.10's periods and
//! the backfill). Wiring it before those exist would make every tenant's first
//! invoice fail on a chart they have never visited. Until then this is the
//! function the backfill and the finance routes call, and it is the same
//! function the issue path will call inside its own transaction.

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::fin_accounts::AccountRole;
use crate::fin_journal::{EntrySource, SourceEvent, SourceKind};
use crate::fin_rules::{InvoiceAccounts, invoice_issue_entry};
use crate::id::{BillingInvoiceId, FinAccountId, FinEntryId};

impl AccountStore {
    /// The account this tenant's chart gives a role, or a refusal naming the
    /// role that is missing.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the role when no active account holds
    /// it; [`StoreError::Db`] on failure.
    async fn fin_account_required(&self, role: AccountRole) -> Result<FinAccountId> {
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

    /// The entry an invoice's issue already produced, or `None` — the "is this
    /// document in the books?" a screen and a backfill both ask.
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
}
