//! The difference between the documents a tenant has issued and the books that
//! hold them — and the repair that closes it (A10.2).
//!
//! Two Finance-shaped questions had two different answers on the same tenant in
//! the same minute. `billing_totals` sums the **documents**; `ledger_summary`
//! sums the receivables account of the **journal** — and posting a document to
//! the journal only began when the document paths were wired to the posting
//! rules. Every document issued before that wiring is a document the books do
//! not hold, so Finance answered `0.00` for a year Billing and Insights
//! answered in full, and neither of them said why.
//!
//! This file is the one place that knows the difference, so the reading and the
//! repair cannot drift apart:
//!
//! - [`gap_json`] sets the issued documents of a period against the issue
//!   entries the caller has already read off the receivables ledger
//!   ([`booked_documents`]), so `ledger_summary` can never report a books figure
//!   as the whole truth without naming what is missing from it. It costs the two
//!   list statements `billing_totals` already makes and not one query more.
//! - [`execute_post_missing_documents`] closes it, by putting each missing
//!   document — and each missing settlement — through the same posting doors
//!   the document paths use, at the document's own date. It is idempotent (a
//!   document already in the books is counted, not posted twice), it states what
//!   it posted, and a document it cannot post is named with the store's own
//!   refusal rather than skipped in silence.
//!
//! **It invents no accounting.** Nothing here computes an amount: the postings
//! are [`alo_store::fin_rules`]'s, resolved by role against the tenant's own
//! chart, exactly as an issue made today would be. A document whose rate cannot
//! restate it into the accounting currency, or whose date falls in a closed
//! period, is refused by the store and reported as refused — never booked at a
//! rate nobody applied or into a period somebody closed.

use std::collections::HashSet;

use serde_json::{Value, json};
use time::Date;

use alo_store::{
    AccountStore, InvoiceStatus, InvoiceSummary, LedgerLine, Payment, SourceEvent, SourceKind,
    StoreError, billing_fx::restated_open_cents,
};

use crate::agent_args::unprocessable;
use crate::billing::{iso_date, map_store_err};
use crate::billing_document::today;
use crate::billing_intents::{Reply, ok};
use crate::error::Problem;
use crate::finance_intents::{optional_period_day, period_day};
use crate::state::Account;

/// How many documents a block names before it stops naming them — enough to see
/// what kind of thing is missing, small enough to sit inside a turn's result
/// window. The counts and the sums are always of everything.
const MAX_NAMED: usize = 12;

/// The document ids the journal holds an **issue** entry for, read off the
/// receivables ledger lines the caller already has.
///
/// A document's issue entry always carries the document's own issue date
/// ([`alo_store::fin_rules`]), so a document issued inside the period a ledger
/// was read for has its entry inside that same page — which is what makes this
/// an exact answer rather than a sample.
pub(crate) fn booked_documents(lines: &[LedgerLine]) -> HashSet<String> {
    lines
        .iter()
        .filter_map(|line| {
            let source = line.source.as_ref()?;
            (matches!(source.kind, SourceKind::Invoice)
                && matches!(source.event, SourceEvent::Issue))
            .then(|| source.id.clone())
        })
        .collect()
}

/// The tenant's issued documents over a period — invoices and credit notes,
/// settled ones included — **oldest first, with every credit note after every
/// invoice**.
///
/// That order is not cosmetic: a credit note's entry names the entry of the
/// document it corrects ([`AccountStore::post_credit_note_issue`]), so the
/// original has to be in the books before its mirror can be posted. A credit
/// note only ever corrects an invoice, never another credit note, so putting
/// the credit notes last is enough for any depth of correction the domain
/// allows.
///
/// `from` absent means "however far back the records go" — the default for a
/// repair, whose whole subject is documents older than the books.
///
/// # Errors
/// The store's, rendered as the route edge renders it.
pub(crate) async fn issued_documents(
    acc: &AccountStore,
    from: Option<Date>,
    to: Date,
) -> Result<Vec<InvoiceSummary>, Problem> {
    let mut documents = Vec::new();
    for status in [InvoiceStatus::Issued, InvoiceStatus::Paid] {
        documents.extend(
            acc.billing_invoices(Some(status))
                .await
                .map_err(map_store_err)?,
        );
    }
    documents.retain(|summary| {
        summary
            .invoice
            .issue_date
            .is_some_and(|day| from.is_none_or(|from| day >= from) && day <= to)
    });
    documents.sort_by(|a, b| {
        (
            a.invoice.is_credit_note,
            a.invoice.issue_date,
            &a.invoice.number,
        )
            .cmp(&(
                b.invoice.is_credit_note,
                b.invoice.issue_date,
                &b.invoice.number,
            ))
    });
    Ok(documents)
}

/// A document's gross **as the books would carry it**: restated into the
/// accounting currency at the rate frozen on the document, and signed the way
/// the receivable moves — an invoice adds, a credit note takes away (its lines
/// are the original's with the quantity negated, so its own gross is already
/// negative).
///
/// `None` when the document cannot be restated honestly: a foreign document
/// carrying no rate, or one crossed into a currency the books are no longer kept
/// in. It is exactly the condition [`alo_store::fin_rules`] refuses to book on,
/// so a figure this cannot state is a figure no posting would have produced
/// either — counted apart rather than guessed at.
fn base_gross(base_currency: &str, summary: &InvoiceSummary) -> Option<i64> {
    restated_open_cents(
        base_currency,
        &summary.invoice.currency,
        summary.invoice.fx.as_ref(),
        summary.totals.gross_cents,
    )
}

/// The documents-against-the-books block a reading carries: what the document
/// list says for the period, and which of those documents the journal does not
/// hold.
///
/// `note` is present only when something is missing, and says the one thing a
/// reader of the figures beside it needs to know — that they are short, by how
/// much, and what puts it right.
pub(crate) fn gap_json(
    base_currency: &str,
    documents: &[InvoiceSummary],
    booked: &HashSet<String>,
) -> Value {
    let mut documents_cents = 0i64;
    let mut unconverted = 0usize;
    let mut missing_cents = 0i64;
    let mut missing_unconverted = 0usize;
    let mut missing: Vec<&InvoiceSummary> = Vec::new();
    for summary in documents {
        let gross = base_gross(base_currency, summary);
        match gross {
            Some(cents) => documents_cents += cents,
            None => unconverted += 1,
        }
        if booked.contains(summary.invoice.id.as_str()) {
            continue;
        }
        match gross {
            Some(cents) => missing_cents += cents,
            None => missing_unconverted += 1,
        }
        missing.push(summary);
    }
    let named: Vec<Value> = missing
        .iter()
        .take(MAX_NAMED)
        .map(|summary| {
            json!({
                "number": summary.invoice.number,
                "issueDate": summary.invoice.issue_date.map(iso_date),
                "grossCents": base_gross(base_currency, summary),
                "creditNote": summary.invoice.is_credit_note,
            })
        })
        .collect();
    let note = (!missing.is_empty()).then(|| {
        format!(
            "{} of these documents {} not in the books, so every figure read from the \
             journal above is short by {} — they were issued before the books began \
             recording documents. post_missing_documents puts them in, at their own dates.",
            missing.len(),
            if missing.len() == 1 { "is" } else { "are" },
            crate::billing_intents::money(missing_cents, base_currency),
        )
    });
    json!({
        "compared": true,
        "documentCount": documents.len(),
        "invoicedCents": documents_cents,
        "unconvertedCount": unconverted,
        "unpostedCount": missing.len(),
        "unpostedCents": missing_cents,
        "unpostedUnconvertedCount": missing_unconverted,
        "unposted": named,
        "note": note,
    })
}

/// The block a reading carries when it could not make the comparison — the
/// ledger page it was built from was cut short, so an absence there is "I
/// stopped looking" rather than "this document is not in the books".
pub(crate) fn not_compared_json() -> Value {
    json!({
        "compared": false,
        "note": "the period's ledger read was cut short, so the documents were not set \
                 against it; narrow the period to compare them",
    })
}

/// One refusal, in the store's own words — the sentence the route edge would
/// have shown, so a person reading the agent's report and a person reading a
/// `422` are told the same thing.
fn refusal(subject: Value, error: StoreError) -> Value {
    let problem = map_store_err(error);
    json!({
        "document": subject,
        "reason": problem
            .detail
            .unwrap_or_else(|| "it was refused".to_owned()),
    })
}

/// How a refused document names itself.
fn document_json(summary: &InvoiceSummary) -> Value {
    json!({
        "number": summary.invoice.number,
        "issueDate": summary.invoice.issue_date.map(iso_date),
        "creditNote": summary.invoice.is_credit_note,
    })
}

/// How a refused settlement names itself: the payment's day and amount, on the
/// document it belongs to.
fn payment_json(summary: &InvoiceSummary, payment: &Payment) -> Value {
    json!({
        "number": summary.invoice.number,
        "paidOn": iso_date(payment.paid_on),
        "amountCents": payment.amount_cents,
        "settlement": true,
    })
}

/// `post_missing_documents` — a write: the documents already issued that the
/// books do not hold are posted, at their own dates, by the rules an issue made
/// today would use.
///
/// The whole path in one sentence: take the issued documents of the period
/// (everything, unless the asker named a period), ask the journal whether each
/// is in the books, put the ones that are not through
/// [`AccountStore::post_invoice_issue`] or
/// [`AccountStore::post_credit_note_issue`], and then do the same for every
/// payment recorded against them
/// ([`AccountStore::post_payment_settle`]).
///
/// **Idempotent, and not silently so.** A document already in the books is
/// counted under `alreadyInTheBooksCount` and left alone; running the verb twice
/// posts nothing the second time and says as much. Its idempotency is the
/// journal's own `UNIQUE (tenant_id, source_kind, source_id, source_event)`, not
/// a flag this file keeps.
///
/// **A refusal stops that document and nothing else.** A closed period, a chart
/// missing a role, a foreign document with no usable rate — each is reported
/// against the document it belongs to, with the store's sentence, and the walk
/// goes on. A document that could not be posted has its settlements skipped
/// too, because relieving a receivable that was never booked is exactly the
/// posting the store refuses.
pub async fn execute_post_missing_documents(account: &Account, args: &Value) -> Reply {
    account.require_finance()?;
    let to = period_day(args, "to", today())?;
    let from = optional_period_day(args, "from")?;
    if from.is_some_and(|from| from > to) {
        return Err(unprocessable("from is after to"));
    }
    let base_currency = account
        .acc
        .billing_base_currency()
        .await
        .map_err(map_store_err)?;
    let documents = issued_documents(&account.acc, from, to).await?;

    let mut already = 0usize;
    let mut posted_documents = 0usize;
    let mut posted_cents = 0i64;
    let mut posted_settlements = 0usize;
    let mut refused: Vec<Value> = Vec::new();

    for summary in &documents {
        let id = &summary.invoice.id;
        if account
            .acc
            .fin_invoice_entry(id)
            .await
            .map_err(map_store_err)?
            .is_some()
        {
            already += 1;
        } else {
            let booked = if summary.invoice.is_credit_note {
                account.acc.post_credit_note_issue(id).await
            } else {
                account.acc.post_invoice_issue(id).await
            };
            match booked {
                Ok(_) => {
                    posted_documents += 1;
                    posted_cents += base_gross(&base_currency, summary).unwrap_or_default();
                }
                Err(error) => {
                    refused.push(refusal(document_json(summary), error));
                    // Its settlements would only be refused in turn, and for a
                    // reason that is about this document rather than about them.
                    continue;
                }
            }
        }
        for payment in account
            .acc
            .billing_payments(id)
            .await
            .map_err(map_store_err)?
        {
            if account
                .acc
                .fin_payment_entry(&payment.id)
                .await
                .map_err(map_store_err)?
                .is_some()
            {
                continue;
            }
            match account.acc.post_payment_settle(id, &payment.id).await {
                Ok(_) => posted_settlements += 1,
                Err(error) => refused.push(refusal(payment_json(summary, &payment), error)),
            }
        }
    }

    let refused_count = refused.len();
    refused.truncate(MAX_NAMED);
    ok(json!({
        "kind": "booksBackfill",
        "from": from.map(iso_date),
        "to": iso_date(to),
        "currency": base_currency,
        "documentCount": documents.len(),
        "alreadyInTheBooksCount": already,
        "postedDocumentCount": posted_documents,
        "postedSettlementCount": posted_settlements,
        "postedCents": posted_cents,
        "refusedCount": refused_count,
        "refused": refused,
    }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use alo_store::{
        BillingCustomerId, BillingInvoiceId, EntrySource, FinEntryId, FinPostingId, Invoice,
        billing_totals::Totals,
    };
    use time::{Month, OffsetDateTime};

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap()
    }

    fn document(id: &str, number: &str, gross_cents: i64, credit_note: bool) -> InvoiceSummary {
        let now = OffsetDateTime::UNIX_EPOCH;
        InvoiceSummary {
            invoice: Invoice {
                id: BillingInvoiceId::new(id),
                customer_id: BillingCustomerId::new("cus"),
                status: InvoiceStatus::Issued,
                currency: "EUR".to_owned(),
                number: Some(number.to_owned()),
                issue_date: Some(day(2026, Month::March, 3)),
                due_date: Some(day(2026, Month::April, 2)),
                payment_terms_days: 30,
                is_credit_note: credit_note,
                credits_invoice_id: None,
                quote_id: None,
                schedule_id: None,
                schedule_due_date: None,
                reference: String::new(),
                note: String::new(),
                fx: None,
                created_by: "u".to_owned(),
                created_at: now,
                updated_at: now,
            },
            totals: Totals {
                net_cents: gross_cents,
                vat_cents: 0,
                gross_cents,
                vat_by_rate: Vec::new(),
            },
            paid_cents: 0,
        }
    }

    fn ledger_line(source: Option<EntrySource>) -> LedgerLine {
        LedgerLine {
            posting_id: FinPostingId::new("p"),
            entry_id: FinEntryId::new("e"),
            entry_date: day(2026, Month::March, 3),
            kind: alo_store::EntryKind::Invoice,
            source,
            entry_memo: String::new(),
            memo: String::new(),
            currency: "EUR".to_owned(),
            amount_cents: 0,
            base_cents: 0,
            running_cents: 0,
            vat_rate_bp: None,
            customer_id: None,
            supplier_key: None,
            project_id: None,
            user_id: None,
        }
    }

    /// The ledger's own lines say which documents are in the books — an issue
    /// entry and nothing else, so a payment's settlement on the same account
    /// does not read as its invoice being booked.
    #[test]
    fn the_booked_set_is_the_issue_entries_and_only_those() {
        let lines = [
            ledger_line(Some(EntrySource {
                kind: SourceKind::Invoice,
                id: "inv-1".to_owned(),
                event: SourceEvent::Issue,
            })),
            ledger_line(Some(EntrySource {
                kind: SourceKind::Payment,
                id: "pay-1".to_owned(),
                event: SourceEvent::Settle,
            })),
            ledger_line(None),
        ];
        let booked = booked_documents(&lines);
        assert_eq!(booked.len(), 1);
        assert!(booked.contains("inv-1"));
    }

    /// The block states what the documents say, what the books hold, and the
    /// difference — with a sentence that is present only when there is one.
    #[test]
    fn the_gap_is_the_documents_the_ledger_has_no_entry_for() {
        let documents = [
            document("inv-1", "INV-2026-00001", 121_000, false),
            document("inv-2", "INV-2026-00002", 50_000, false),
            document("cn-1", "INV-2026-00003", -21_000, true),
        ];
        let booked: HashSet<String> = ["inv-1".to_owned()].into_iter().collect();
        let block = gap_json("EUR", &documents, &booked);
        assert_eq!(block["documentCount"], 3);
        assert_eq!(block["invoicedCents"], 150_000);
        assert_eq!(block["unpostedCount"], 2);
        assert_eq!(block["unpostedCents"], 29_000);
        assert_eq!(block["unpostedUnconvertedCount"], 0);
        assert_eq!(block["unposted"][0]["number"], "INV-2026-00002");
        assert_eq!(block["unposted"][1]["creditNote"], true);
        let note = block["note"].as_str().unwrap();
        assert!(
            note.contains("2 of these documents are not in the books"),
            "{note}"
        );
        assert!(note.contains("290.00 EUR"), "{note}");
        assert!(note.contains("post_missing_documents"), "{note}");

        // Everything booked: the figures still stand, and nothing is said.
        let all: HashSet<String> = ["inv-1", "inv-2", "cn-1"]
            .into_iter()
            .map(ToOwned::to_owned)
            .collect();
        let whole = gap_json("EUR", &documents, &all);
        assert_eq!(whole["unpostedCount"], 0);
        assert_eq!(whole["invoicedCents"], 150_000);
        assert!(whole["note"].is_null());
    }

    /// A foreign document carrying no rate is counted and never guessed at: it
    /// is in `unpostedCount` and in `unpostedUnconvertedCount`, and its gross is
    /// in neither sum — which is exactly the document the store would refuse to
    /// book.
    #[test]
    fn a_document_that_cannot_be_restated_is_counted_apart() {
        let mut foreign = document("inv-9", "INV-2026-00009", 100_000, false);
        foreign.invoice.currency = "USD".to_owned();
        let documents = [document("inv-1", "INV-2026-00001", 121_000, false), foreign];
        let block = gap_json("EUR", &documents, &HashSet::new());
        assert_eq!(block["documentCount"], 2);
        assert_eq!(block["invoicedCents"], 121_000, "the euro one, and only it");
        assert_eq!(block["unconvertedCount"], 1);
        assert_eq!(block["unpostedCount"], 2);
        assert_eq!(block["unpostedCents"], 121_000);
        assert_eq!(block["unpostedUnconvertedCount"], 1);
        assert!(block["unposted"][1]["grossCents"].is_null());
    }
}
