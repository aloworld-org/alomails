//! Paying suppliers (alo Billing, ADR 0035, wave B2.12) — which approved bills
//! go into a SEPA credit-transfer instruction, and the mark that says they
//! have.
//!
//! This module is the **business half** of the pain.001 export. It decides what
//! may be paid, what the bank is being told to move, and records that the
//! instruction was given; the ISO 20022 message itself is written in
//! `products/mail/alo-jmap/src/billing_pain001.rs`, exactly as the two
//! e-invoice syntaxes are written above the store while the reader lives below
//! it ([`crate::billing_einvoice_import`]).
//!
//! Four rules decide the whole surface, and each of them is somebody's money:
//!
//! - **Only an approved bill is paid.** A bill nobody has decided about is a
//!   claim, not a liability ([`crate::billing_bills`]), and a rejected one is a
//!   refusal. Neither belongs in a file the bank executes.
//! - **A bill goes into one payment run.** Paying a supplier twice is the
//!   accident the whole record type exists to prevent, so an exported bill
//!   carries the run it went into and the second export is refused — unless the
//!   caller says, in as many words, that they mean to repeat it (the bank
//!   rejected the file; the file was lost). That is a different act and reads
//!   as one.
//! - **Euro only, positive only.** A SEPA credit transfer is a euro
//!   instruction; a foreign-currency bill is a different payment product a bank
//!   prices differently, and a credit note is money coming back, which no
//!   transfer of ours will ever move. Both are refused by name rather than
//!   quietly skipped, because a payment run that silently pays *fewer* bills
//!   than the person selected is how a supplier goes unpaid.
//! - **A file is an instruction, never a settlement.** Nothing here marks a
//!   bill paid: the money moves when the bank says it moved, which arrives back
//!   as a statement line and is reconciled in B4.09. What is recorded is that
//!   we asked.
//!
//! Money stays integer cents throughout, and the debtor is always this tenant's
//! own account from [`crate::billing_settings`] — never anything a request
//! states, so no body can redirect a payment run to another account.

use sha2::{Digest, Sha256};
use time::Date;

use crate::account::AccountStore;
use crate::billing_bills::{Bill, BillStatus};
use crate::billing_settings::BillingSettings;
use crate::error::{Result, StoreError};
use crate::iban::{canonicalize, canonicalize_bic};
use crate::id::BillingBillId;

/// The most transfers one file may carry. A batch beyond this is a data feed
/// rather than a payment run, and several banks cap a single upload lower still.
pub const SEPA_MAX_TRANSFERS: usize = 500;

/// How far ahead a payment may be dated, in days. Banks hold a forward-dated
/// instruction, but a year out is a typo rather than a plan.
pub const SEPA_MAX_DAYS_AHEAD: i64 = 365;

/// Longest `MsgId`/`EndToEndId` ISO 20022 allows (`Max35Text`).
pub const SEPA_ID_MAX_CHARS: usize = 35;

/// Longest unstructured remittance line the EPC scheme carries (`Max140Text`),
/// and the only free text that reaches the supplier's statement.
pub const SEPA_REMITTANCE_MAX_CHARS: usize = 140;

/// One supplier payment: what the bank is told to move, and to whom.
///
/// Every string here is the tenant's own data in its own script — `Söhne`,
/// `Kraków`. Reducing it to the character set a SEPA message may carry is the
/// writer's job, not this module's: what is *paid* and what a message may
/// *spell* are two different questions, and folding them together here would
/// mean the stored record and the file disagreed about who was paid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreditTransfer {
    /// The bill this pays.
    pub bill_id: BillingBillId,
    /// The supplier, as their document named them.
    pub creditor_name: String,
    /// Their account, canonical (uppercase, unspaced).
    pub creditor_iban: String,
    /// Their bank's BIC when the document stated one, else empty: since 2016 a
    /// SEPA transfer is IBAN-only, and a BIC we guessed would be worse than the
    /// one the bank derives.
    pub creditor_bic: String,
    /// What to move, in integer cents. Always positive.
    pub amount_cents: i64,
    /// The reference that travels with the payment end to end — the supplier's
    /// own document number, so their ledger recognises it.
    pub end_to_end_id: String,
    /// What the supplier reads on their statement: the remittance reference
    /// they asked for (BT-83) when they stated one, else their number.
    pub remittance: String,
}

/// One payment run: this tenant's account, one execution date, and the
/// transfers to make from it.
///
/// One debtor account and one date, by construction, which is exactly the
/// grouping ISO 20022 calls a `PaymentInformation` block — so the message the
/// writer produces has one of them, and the file needs no grouping logic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentFile {
    /// The run's identifier, unique per file and quoted back by the bank. Also
    /// stamped on every bill it pays, so a statement line can always be traced
    /// to the run that caused it.
    pub message_id: String,
    /// The day the bank is asked to execute — a **day**, never an instant.
    pub execution_date: Date,
    /// Whose account the money leaves: the name it is held in.
    pub debtor_name: String,
    /// Our account, canonical.
    pub debtor_iban: String,
    /// Our bank's BIC when the tenant has stated one, else empty.
    pub debtor_bic: String,
    /// The tenant's country, for the debtor's address when it is stated.
    pub debtor_country: String,
    /// The payments, oldest liability first.
    pub transfers: Vec<CreditTransfer>,
}

impl PaymentFile {
    /// How many transfers the file carries (`NbOfTxs`).
    #[must_use]
    pub fn count(&self) -> usize {
        self.transfers.len()
    }

    /// The sum of every transfer, in integer cents (`CtrlSum`) — what the
    /// tenant's account will be short by.
    ///
    /// Saturating rather than wrapping: a sum that could not be represented is
    /// refused when the file is planned, so this can only be reached with a
    /// file that already passed that bound.
    #[must_use]
    pub fn control_sum_cents(&self) -> i64 {
        self.transfers.iter().fold(0i64, |sum, transfer| {
            sum.saturating_add(transfer.amount_cents)
        })
    }
}

impl AccountStore {
    /// This tenant's bills that are waiting to be paid: approved, not yet in a
    /// payment file, oldest due date first.
    ///
    /// The order is the order a payment run is prepared in — the bill closest
    /// to being late is the one at the top — and a bill with no stated due date
    /// sorts on its issue date, because a document with no term is due on
    /// receipt.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn payable_billing_bills(&self) -> Result<Vec<Bill>> {
        let rows = sqlx::query_as::<_, crate::billing_bills::BillRow>(&format!(
            "SELECT {cols} FROM billing_bills \
             WHERE tenant_id = $1 AND status = 'approved' AND exported_at IS NULL \
             ORDER BY COALESCE(due_date, issue_date), issue_date, id",
            cols = crate::billing_bills::BILL_COLS
        ))
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter()
            .map(crate::billing_bills::BillRow::into_bill)
            .collect()
    }

    /// Plans the payment run for `ids`, without recording anything.
    ///
    /// Split from [`AccountStore::record_sepa_payment_file`] on purpose: the
    /// message is written above the store, and marking bills as instructed
    /// *before* the file exists would leave a liability looking paid because a
    /// renderer failed. So the caller plans, writes the file, and only then
    /// records — and the record re-checks every rule under the row locks, so
    /// two runs racing over the same bill still produce exactly one instruction.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when an id is absent **or another tenant's**;
    /// [`StoreError::Conflict`] when a bill is undecided, rejected, or already
    /// in a file (and `repeat` is not set); [`StoreError::Validation`] when the
    /// tenant's own account is not stated, the date is not one a bank can be
    /// given, or a bill cannot be paid by SEPA credit transfer at all;
    /// [`StoreError::Db`] on failure.
    pub async fn plan_sepa_payment_file(
        &self,
        ids: &[BillingBillId],
        execution_date: Date,
        repeat: bool,
    ) -> Result<PaymentFile> {
        let wanted = deduplicate(ids);
        if wanted.is_empty() {
            return Err(StoreError::Validation(
                "a payment file must pay at least one bill".to_owned(),
            ));
        }
        if wanted.len() > SEPA_MAX_TRANSFERS {
            return Err(StoreError::Validation(format!(
                "a payment file carries at most {SEPA_MAX_TRANSFERS} payments; split the run"
            )));
        }
        // The database's date, never the process's: the same reason issuing
        // reads `CURRENT_DATE` (crate::billing_invoices).
        let today: Date = sqlx::query_scalar("SELECT CURRENT_DATE")
            .fetch_one(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        let settings = self.billing_settings().await?;
        let bills = self.bills_for_payment(&wanted).await?;
        if bills.len() != wanted.len() {
            // One of the ids is not this tenant's. Which one is deliberately
            // not said: that answer is an existence oracle across tenants.
            return Err(StoreError::NotFound);
        }
        plan(
            &settings,
            &bills,
            execution_date,
            today,
            repeat,
            mint_message_id(today),
        )
    }

    /// Records that `file` was given to the bank: every bill it pays is stamped
    /// with the run, the moment and the person.
    ///
    /// Every rule [`AccountStore::plan_sepa_payment_file`] checked is checked
    /// again here, under each bill's row lock and inside one transaction, so a
    /// bill approved-then-exported by a colleague between the plan and the
    /// record cannot be paid twice. Who instructed it and when are stamped from
    /// the account handle and the database's clock — never from the caller.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when a bill has gone or is another tenant's;
    /// [`StoreError::Conflict`] when it is no longer payable; [`StoreError::Db`]
    /// on failure.
    pub async fn record_sepa_payment_file(&self, file: &PaymentFile, repeat: bool) -> Result<()> {
        let mut ids: Vec<&BillingBillId> = file
            .transfers
            .iter()
            .map(|transfer| &transfer.bill_id)
            .collect();
        // Locked in id order, always: two runs sharing bills then queue behind
        // each other instead of deadlocking half way through.
        ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        for id in ids {
            let row: Option<(String, Option<String>)> = sqlx::query_as(
                "SELECT status, export_message_id FROM billing_bills \
                 WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
            )
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .fetch_optional(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
            let (status, exported) = row.ok_or(StoreError::NotFound)?;
            let status = BillStatus::parse(&status).ok_or_else(|| {
                StoreError::Db(sqlx::Error::Decode(
                    "billing_bills.status is not a known status".into(),
                ))
            })?;
            payable(status, exported.is_some(), repeat, id)?;

            sqlx::query(
                "UPDATE billing_bills \
                 SET exported_at = now(), exported_by = $3, export_message_id = $4 \
                 WHERE tenant_id = $1 AND id = $2",
            )
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(self.user.as_str())
            .bind(&file.message_id)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// The requested bills of this tenant, in the order a payment run is
    /// prepared in. Bills of another tenant are simply not returned, which is
    /// what turns a guessed id into a `NotFound` above.
    async fn bills_for_payment(&self, ids: &[BillingBillId]) -> Result<Vec<Bill>> {
        let keys: Vec<String> = ids.iter().map(|id| id.as_str().to_owned()).collect();
        let rows = sqlx::query_as::<_, crate::billing_bills::BillRow>(&format!(
            "SELECT {cols} FROM billing_bills \
             WHERE tenant_id = $1 AND id = ANY($2) \
             ORDER BY COALESCE(due_date, issue_date), issue_date, id",
            cols = crate::billing_bills::BILL_COLS
        ))
        .bind(self.tenant.as_str())
        .bind(&keys)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter()
            .map(crate::billing_bills::BillRow::into_bill)
            .collect()
    }
}

// ---- the plan, as pure code --------------------------------------------------

/// Builds the payment run, or the first refusal that stops it.
///
/// Pure, so every rule below is unit-tested against a fixture rather than
/// against a database, and one function decides what may be paid however the
/// caller reached it.
fn plan(
    settings: &BillingSettings,
    bills: &[Bill],
    execution_date: Date,
    today: Date,
    repeat: bool,
    message_id: String,
) -> Result<PaymentFile> {
    let (debtor_name, debtor_iban, debtor_bic) = debtor(settings)?;
    if execution_date < today {
        return Err(StoreError::Validation(
            "a payment cannot be dated before today; a bank executes forward, not backward"
                .to_owned(),
        ));
    }
    if (execution_date - today).whole_days() > SEPA_MAX_DAYS_AHEAD {
        return Err(StoreError::Validation(format!(
            "a payment cannot be dated more than {SEPA_MAX_DAYS_AHEAD} days ahead"
        )));
    }

    let mut transfers = Vec::with_capacity(bills.len());
    let mut control_sum: i64 = 0;
    for bill in bills {
        payable(
            bill.status,
            bill.export_message_id.is_some(),
            repeat,
            &bill.id,
        )?;
        transfers.push(transfer(bill)?);
        control_sum = control_sum
            .checked_add(bill.totals.payable_cents)
            .ok_or_else(|| {
                StoreError::Validation(
                    "this payment run adds up to more than any real payment file".to_owned(),
                )
            })?;
    }

    Ok(PaymentFile {
        message_id,
        execution_date,
        debtor_name,
        debtor_iban,
        debtor_bic,
        debtor_country: settings.country.clone(),
        transfers,
    })
}

/// The tenant's own side of every transfer: the name the account is held in,
/// the account, and the bank.
///
/// A tenant that has not stated its own account cannot instruct a payment at
/// all, and the useful answer to that is which field of Billing settings is
/// missing — not a file the bank rejects tomorrow.
fn debtor(settings: &BillingSettings) -> Result<(String, String, String)> {
    let name = settings.effective_account_holder().trim().to_owned();
    if name.is_empty() {
        return Err(StoreError::Validation(
            "your own name is not stated in Billing settings, and a payment file has to say \
             whose account the money leaves"
                .to_owned(),
        ));
    }
    let iban = settings.iban.as_deref().unwrap_or_default();
    let iban = canonicalize(iban)
        .map_err(|e| StoreError::Validation(format!("your own IBAN cannot be used: {e}")))?
        .ok_or_else(|| {
            StoreError::Validation(
                "your own IBAN is not stated in Billing settings, and a payment file has to say \
                 which account the money leaves"
                    .to_owned(),
            )
        })?;
    let bic = canonicalize_bic(settings.bic.as_deref().unwrap_or_default())
        .map_err(|e| StoreError::Validation(format!("your own BIC cannot be used: {e}")))?
        .unwrap_or_default();
    Ok((name, iban, bic))
}

/// Whether a bill in this state may be instructed, in the words the person
/// preparing the run needs.
///
/// The bill id is named because the caller sent it and a run of forty payments
/// is unfixable without knowing which row to look at; nothing else about the
/// document appears, because a refusal travels into logs (law 1).
fn payable(
    status: BillStatus,
    already_exported: bool,
    repeat: bool,
    id: &BillingBillId,
) -> Result<()> {
    match status {
        BillStatus::Approved => {}
        BillStatus::Received => {
            return Err(StoreError::Conflict(format!(
                "bill {id} has not been approved yet; only an approved bill is paid"
            )));
        }
        BillStatus::Rejected => {
            return Err(StoreError::Conflict(format!(
                "bill {id} was rejected; a bill we refused is not paid"
            )));
        }
    }
    if already_exported && !repeat {
        return Err(StoreError::Conflict(format!(
            "bill {id} is already in a payment file; repeat it deliberately if the bank never \
             executed that one"
        )));
    }
    Ok(())
}

/// One bill as a transfer, or the reason it cannot be one.
fn transfer(bill: &Bill) -> Result<CreditTransfer> {
    if bill.currency != "EUR" {
        return Err(StoreError::Validation(format!(
            "bill {id} is not in euro, and a SEPA credit transfer is a euro instruction; pay it \
             through your bank directly",
            id = bill.id
        )));
    }
    if bill.totals.payable_cents <= 0 {
        return Err(StoreError::Validation(format!(
            "bill {id} has nothing to pay; a credit note is money coming back, not a transfer to \
             make",
            id = bill.id
        )));
    }
    let iban = canonicalize(&bill.supplier.iban)
        .map_err(|e| {
            StoreError::Validation(format!(
                "the account stated on bill {id} cannot be paid to: {e}",
                id = bill.id
            ))
        })?
        .ok_or_else(|| {
            StoreError::Validation(format!(
                "bill {id} states no IBAN to pay into; ask the supplier for one",
                id = bill.id
            ))
        })?;
    let remittance = if bill.payment_reference.trim().is_empty() {
        bill.number.clone()
    } else {
        bill.payment_reference.clone()
    };
    Ok(CreditTransfer {
        bill_id: bill.id.clone(),
        creditor_name: bill.supplier.name.clone(),
        creditor_iban: iban,
        // Bills carry no BIC: the standard's payment-means group states the
        // account, and since 2016 a SEPA transfer needs nothing else.
        creditor_bic: String::new(),
        amount_cents: bill.totals.payable_cents,
        end_to_end_id: bill.number.clone(),
        remittance,
    })
}

/// The requested ids, each once, in the order they were first asked for.
///
/// A body listing the same bill twice is a selection mistake, not an
/// instruction to pay twice.
fn deduplicate(ids: &[BillingBillId]) -> Vec<BillingBillId> {
    let mut seen = std::collections::HashSet::new();
    ids.iter()
        .filter(|id| seen.insert(id.as_str().to_owned()))
        .cloned()
        .collect()
}

/// Mints a `MsgId` for one run: `ALO`, the day, and twelve random hex digits.
///
/// Three properties, all required by somebody other than us. It is **unique**,
/// because a bank uses it to recognise a file it has already executed and
/// refuse the duplicate. It is **within the SEPA character set** (`A–Z`, `0–9`,
/// `-`) and at most 35 characters, which is what the message may carry at all.
/// And it **carries the day**, so a tenant reading their bank's file list
/// recognises the run without opening it. Our opaque ids are base64url and
/// contain `_`, which the scheme's character set does not allow — hence hex.
fn mint_message_id(today: Date) -> String {
    let digest = Sha256::digest(crate::id::generate_token().as_bytes());
    let hex: String = digest
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02X}"))
        .collect();
    format!(
        "ALO{year:04}{month:02}{day:02}-{hex}",
        year = today.year(),
        month = u8::from(today.month()),
        day = today.day(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_bills::{BillTotals, Supplier};
    use crate::billing_einvoice_import::EInvoiceSyntax;
    use time::{Month, OffsetDateTime};

    fn day(year: i32, month: u8, day: u8) -> Date {
        Date::from_calendar_date(year, Month::try_from(month).unwrap_or(Month::January), day)
            .unwrap_or(Date::MIN)
    }

    fn today() -> Date {
        day(2026, 8, 7)
    }

    fn settings() -> BillingSettings {
        BillingSettings {
            legal_name: "Alo Werkplaats B.V.".to_owned(),
            country: "NL".to_owned(),
            iban: Some("NL91ABNA0417164300".to_owned()),
            bic: Some("ABNANL2A".to_owned()),
            updated_at: Some(OffsetDateTime::UNIX_EPOCH),
            ..BillingSettings::default()
        }
    }

    fn bill(id: &str, number: &str) -> Bill {
        Bill {
            id: BillingBillId::new(id.to_owned()),
            source_syntax: Some(EInvoiceSyntax::Cii),
            source_sha256: "a".repeat(64),
            credit_note: false,
            status: BillStatus::Approved,
            supplier: Supplier {
                name: "Lieferant GmbH".to_owned(),
                vat_id: "DE811907980".to_owned(),
                country: "DE".to_owned(),
                iban: "DE89 3704 0044 0532 0130 00".to_owned(),
                ..Supplier::default()
            },
            number: number.to_owned(),
            issue_date: day(2026, 7, 1),
            due_date: Some(day(2026, 8, 1)),
            currency: "EUR".to_owned(),
            buyer_reference: String::new(),
            note: String::new(),
            payment_reference: String::new(),
            totals: BillTotals {
                line_total_cents: 110_080,
                tax_exclusive_cents: 110_080,
                tax_total_cents: 23_117,
                tax_inclusive_cents: 133_197,
                payable_cents: 133_197,
                ..BillTotals::default()
            },
            imported_by: "u-1".to_owned(),
            imported_at: OffsetDateTime::UNIX_EPOCH,
            decided_by: Some("u-1".to_owned()),
            decided_at: Some(OffsetDateTime::UNIX_EPOCH),
            exported_at: None,
            exported_by: None,
            export_message_id: None,
        }
    }

    fn planned(bills: &[Bill]) -> PaymentFile {
        plan(
            &settings(),
            bills,
            today(),
            today(),
            false,
            "ALO20260807-ABCDEF012345".to_owned(),
        )
        .unwrap_or_else(|e| panic!("refused: {e}"))
    }

    fn refused(bills: &[Bill]) -> String {
        match plan(&settings(), bills, today(), today(), false, "M".to_owned()) {
            Err(StoreError::Validation(message) | StoreError::Conflict(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_run_states_the_tenants_account_and_every_supplier_payment() {
        let file = planned(&[bill("b-1", "R-2026-77"), bill("b-2", "R-2026-78")]);
        assert_eq!(file.debtor_name, "Alo Werkplaats B.V.");
        assert_eq!(file.debtor_iban, "NL91ABNA0417164300");
        assert_eq!(file.debtor_bic, "ABNANL2A");
        assert_eq!(file.count(), 2);
        assert_eq!(file.control_sum_cents(), 266_394);
        let transfer = &file.transfers[0];
        assert_eq!(transfer.creditor_name, "Lieferant GmbH");
        // The supplier's IBAN is canonicalised: what is printed is spaced, what
        // is instructed never is.
        assert_eq!(transfer.creditor_iban, "DE89370400440532013000");
        assert_eq!(transfer.amount_cents, 133_197);
        assert_eq!(transfer.end_to_end_id, "R-2026-77");
        // With no reference of their own, the supplier reads their number back.
        assert_eq!(transfer.remittance, "R-2026-77");
    }

    #[test]
    fn the_supplier_gets_the_reference_they_asked_for_when_they_asked_for_one() {
        let with_reference = Bill {
            payment_reference: "RF18 5390 0754 7034".to_owned(),
            ..bill("b-1", "R-2026-77")
        };
        let file = planned(&[with_reference]);
        assert_eq!(file.transfers[0].remittance, "RF18 5390 0754 7034");
        // The end-to-end id stays the document number either way: it is what
        // the supplier's own ledger recognises.
        assert_eq!(file.transfers[0].end_to_end_id, "R-2026-77");
    }

    #[test]
    fn only_an_approved_bill_is_paid() {
        for (status, expected) in [
            (BillStatus::Received, "has not been approved"),
            (BillStatus::Rejected, "was rejected"),
        ] {
            let message = refused(&[Bill {
                status,
                ..bill("b-1", "R-1")
            }]);
            assert!(message.contains(expected), "{message}");
            assert!(message.contains("b-1"), "names the row: {message}");
        }
    }

    #[test]
    fn a_bill_already_in_a_file_is_not_paid_again_unless_that_is_the_point() {
        let exported = Bill {
            exported_at: Some(OffsetDateTime::UNIX_EPOCH),
            exported_by: Some("u-1".to_owned()),
            export_message_id: Some("ALO20260801-000000000000".to_owned()),
            ..bill("b-1", "R-1")
        };
        let message = refused(std::slice::from_ref(&exported));
        assert!(message.contains("already in a payment file"), "{message}");
        // …and deliberately repeating it is allowed, because a file the bank
        // never executed still has to be paid.
        let repeated = plan(
            &settings(),
            std::slice::from_ref(&exported),
            today(),
            today(),
            true,
            "M".to_owned(),
        );
        assert!(repeated.is_ok(), "{repeated:?}");
    }

    #[test]
    fn a_payment_is_euro_positive_and_goes_to_an_account_we_have() {
        let foreign = refused(&[Bill {
            currency: "USD".to_owned(),
            ..bill("b-1", "R-1")
        }]);
        assert!(foreign.contains("not in euro"), "{foreign}");

        let credit = refused(&[Bill {
            credit_note: true,
            totals: BillTotals {
                payable_cents: -30_250,
                ..bill("b-1", "R-1").totals
            },
            ..bill("b-1", "R-1")
        }]);
        assert!(credit.contains("nothing to pay"), "{credit}");

        let nothing = refused(&[Bill {
            totals: BillTotals {
                payable_cents: 0,
                ..bill("b-1", "R-1").totals
            },
            ..bill("b-1", "R-1")
        }]);
        assert!(nothing.contains("nothing to pay"), "{nothing}");

        let no_account = refused(&[Bill {
            supplier: Supplier {
                iban: String::new(),
                ..bill("b-1", "R-1").supplier
            },
            ..bill("b-1", "R-1")
        }]);
        assert!(no_account.contains("no IBAN"), "{no_account}");

        let typo = refused(&[Bill {
            supplier: Supplier {
                iban: "DE89370400440532013001".to_owned(),
                ..bill("b-1", "R-1").supplier
            },
            ..bill("b-1", "R-1")
        }]);
        assert!(typo.contains("check digits"), "{typo}");
        // No refusal quotes the account it refused.
        assert!(!typo.contains("DE89"), "{typo}");
    }

    #[test]
    fn a_tenant_that_has_not_stated_its_own_account_cannot_instruct_a_payment() {
        let nameless = plan(
            &BillingSettings {
                legal_name: String::new(),
                ..settings()
            },
            &[bill("b-1", "R-1")],
            today(),
            today(),
            false,
            "M".to_owned(),
        );
        assert!(
            matches!(nameless, Err(StoreError::Validation(ref m)) if m.contains("your own name")),
            "{nameless:?}"
        );
        let accountless = plan(
            &BillingSettings {
                iban: None,
                ..settings()
            },
            &[bill("b-1", "R-1")],
            today(),
            today(),
            false,
            "M".to_owned(),
        );
        assert!(
            matches!(accountless, Err(StoreError::Validation(ref m)) if m.contains("your own IBAN")),
            "{accountless:?}"
        );
    }

    #[test]
    fn the_account_holder_is_the_stated_one_when_the_trading_name_differs() {
        let file = plan(
            &BillingSettings {
                account_holder: "Alo Werkplaats Holding B.V.".to_owned(),
                ..settings()
            },
            &[bill("b-1", "R-1")],
            today(),
            today(),
            false,
            "M".to_owned(),
        )
        .unwrap_or_else(|e| panic!("refused: {e}"));
        assert_eq!(file.debtor_name, "Alo Werkplaats Holding B.V.");
    }

    #[test]
    fn a_bank_is_instructed_forward_and_not_a_year_out() {
        let yesterday = plan(
            &settings(),
            &[bill("b-1", "R-1")],
            day(2026, 8, 6),
            today(),
            false,
            "M".to_owned(),
        );
        assert!(
            matches!(yesterday, Err(StoreError::Validation(ref m)) if m.contains("before today")),
            "{yesterday:?}"
        );
        // The far edge is a year out, and one day past it is a typo.
        assert!(
            plan(
                &settings(),
                &[bill("b-1", "R-1")],
                day(2027, 8, 7),
                today(),
                false,
                "M".to_owned()
            )
            .is_ok()
        );
        let far = plan(
            &settings(),
            &[bill("b-1", "R-1")],
            day(2027, 8, 8),
            today(),
            false,
            "M".to_owned(),
        );
        assert!(
            matches!(far, Err(StoreError::Validation(ref m)) if m.contains("days ahead")),
            "{far:?}"
        );
    }

    #[test]
    fn one_bill_asked_for_twice_is_paid_once() {
        let ids = [
            BillingBillId::new("b-1".to_owned()),
            BillingBillId::new("b-2".to_owned()),
            BillingBillId::new("b-1".to_owned()),
        ];
        let unique = deduplicate(&ids);
        assert_eq!(unique.len(), 2);
        assert_eq!(unique[0].as_str(), "b-1");
        assert_eq!(unique[1].as_str(), "b-2");
    }

    #[test]
    fn a_message_id_is_unique_dated_and_spellable_in_a_sepa_message() {
        let first = mint_message_id(today());
        let second = mint_message_id(today());
        assert_ne!(first, second, "a bank rejects a repeated MsgId");
        assert!(first.starts_with("ALO20260807-"), "{first}");
        assert!(first.len() <= SEPA_ID_MAX_CHARS, "{first}");
        assert!(
            first
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-'),
            "{first}"
        );
    }
}
