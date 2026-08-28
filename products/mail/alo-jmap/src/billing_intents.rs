//! The executors of alo Billing's verbs (ADR 0058) — what runs when the
//! Billing agent uses one of the intents `alo_ai::billing_intents` describes.
//!
//! Every executor runs through the asker's account door and answers with the
//! **record view** Billing's own routes serve (`document_json`,
//! `customer_json`, `quote_json`), so an agent grounds in exactly what a person
//! sees on the detail page — there is no second summary of a quote. A read
//! returns `{"ok": true, "result": …}` into the turn; a write returns the
//! record it changed, and only ever runs from the asker's approval
//! ([`crate::agent::execute_tool`] holds that, not this module).
//!
//! **Resolution is the executor's job, not the model's.** A customer is named;
//! a quote may be named by number or by its customer; several customers
//! matching a name are returned for the person to choose, and a write refuses
//! the ambiguity with a sentence rather than picking one. Nothing here guesses.
//!
//! The three older writes (`create_invoice_draft`, `quote_to_invoice`,
//! `draft_payment_reminder`) keep their executors in [`crate::agent_billing`].

use std::collections::HashMap;

use axum::Json;
use serde_json::{Value, json};
use time::format_description::well_known::Iso8601;
use time::{Date, Duration, Month};

use alo_store::billing_invoices::{InvoiceStatus, InvoiceSummary};
use alo_store::billing_payments::NewPayment;
use alo_store::billing_quotes::{QuoteStatus, QuoteSummary};
use alo_store::{BillingCustomerId, BillingInvoiceId, BillingQuoteId, Customer};

use crate::agent_args::{integer, string_arg, unprocessable};
use crate::billing::{iso_date, map_store_err};
use crate::billing_customers::customer_json;
use crate::billing_document::{today, with_totals};
use crate::error::Problem;
use crate::state::Account;

/// How many documents a list read returns — enough for a question, small
/// enough to sit inside the turn's result window.
const MAX_LISTED: usize = 12;

type Reply = Result<Json<Value>, Problem>;

/// Every read's answer, with its money made readable before it reaches the
/// model ([`with_display`]).
fn ok(result: Value) -> Reply {
    Ok(Json(
        json!({ "ok": true, "result": with_display(result, None) }),
    ))
}

/// `24900` in EUR as `249.00 EUR`; `-1250` as `-12.50 EUR`.
fn money(cents: i64, currency: &str) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let magnitude = cents.unsigned_abs();
    format!(
        "{sign}{}.{:02} {currency}",
        magnitude / 100,
        magnitude % 100
    )
}

/// `1500` milli-units as `1.5`; `2000` as `2`.
fn quantity(milli: i64) -> String {
    let sign = if milli < 0 { "-" } else { "" };
    let magnitude = milli.unsigned_abs();
    let whole = magnitude / 1000;
    let fraction = magnitude % 1000;
    if fraction == 0 {
        format!("{sign}{whole}")
    } else {
        let digits = format!("{fraction:03}");
        format!("{sign}{whole}.{}", digits.trim_end_matches('0'))
    }
}

/// The record views keep money as integer cents by contract. A model reads
/// `24900` as twenty-four thousand more often than it should, so every
/// `…Cents` field gets a `…Display` sibling in the object's own `currency`
/// (or the nearest enclosing one), and `qtyMilli` a `quantityDisplay`. The
/// cents stay: the display is beside the figure, never instead of it.
fn with_display(value: Value, currency: Option<&str>) -> Value {
    match value {
        Value::Object(mut object) => {
            let own = object
                .get("currency")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let currency = own.as_deref().or(currency);
            let mut extra: Vec<(String, Value)> = Vec::new();
            for (key, field) in &object {
                if let Some(base) = key.strip_suffix("Cents") {
                    if let (Some(cents), Some(currency)) = (field.as_i64(), currency) {
                        extra.push((format!("{base}Display"), json!(money(cents, currency))));
                    }
                } else if key == "qtyMilli"
                    && let Some(milli) = field.as_i64()
                {
                    extra.push(("quantityDisplay".to_owned(), json!(quantity(milli))));
                }
            }
            for (key, display) in extra {
                object.insert(key, display);
            }
            let keys: Vec<String> = object.keys().cloned().collect();
            for key in keys {
                if let Some(nested) = object.remove(&key) {
                    object.insert(key, with_display(nested, currency));
                }
            }
            Value::Object(object)
        }
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| with_display(item, currency))
                .collect(),
        ),
        other => other,
    }
}

/// The customers whose name contains `wanted`, an exact match winning alone.
async fn find_customers(account: &Account, wanted: &str) -> Result<Vec<Customer>, Problem> {
    let wanted = wanted.trim().to_lowercase();
    if wanted.is_empty() {
        return Err(unprocessable("name the customer"));
    }
    let all = account
        .acc
        .billing_customers(false)
        .await
        .map_err(map_store_err)?;
    let exact: Vec<Customer> = all
        .iter()
        .filter(|c| c.name.to_lowercase() == wanted)
        .cloned()
        .collect();
    if !exact.is_empty() {
        return Ok(exact);
    }
    Ok(all
        .into_iter()
        .filter(|c| c.name.to_lowercase().contains(&wanted))
        .collect())
}

/// Exactly one customer for a write, or the refusal that names the problem.
async fn one_customer(account: &Account, wanted: &str) -> Result<Customer, Problem> {
    let mut found = find_customers(account, wanted).await?;
    match found.len() {
        0 => Err(unprocessable(format!(
            "no customer named \"{}\"",
            wanted.trim()
        ))),
        1 => Ok(found.remove(0)),
        _ => Err(unprocessable(format!(
            "{} customers match \"{}\": {} — say which",
            found.len(),
            wanted.trim(),
            found
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

async fn names_for(
    account: &Account,
    ids: impl Iterator<Item = BillingCustomerId>,
) -> Result<HashMap<String, String>, Problem> {
    let ids: Vec<BillingCustomerId> = ids.collect();
    account
        .acc
        .billing_customer_names(&ids)
        .await
        .map_err(map_store_err)
}

fn quote_summary_json(q: &QuoteSummary, names: &HashMap<String, String>, day: Date) -> Value {
    let mut value = with_totals(crate::billing_quotes::quote_json(&q.quote, day), &q.totals);
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "customerName".to_owned(),
            json!(names.get(q.quote.customer_id.as_str())),
        );
    }
    value
}

fn invoice_summary_json(i: &InvoiceSummary, names: &HashMap<String, String>, day: Date) -> Value {
    let inv = &i.invoice;
    json!({
        "id": inv.id.as_str(),
        "number": inv.number,
        "customerId": inv.customer_id.as_str(),
        "customerName": names.get(inv.customer_id.as_str()),
        "status": inv.status.as_str(),
        "currency": inv.currency,
        "issueDate": inv.issue_date.map(iso_date),
        "dueDate": inv.due_date.map(iso_date),
        "overdue": inv.is_overdue(day),
        "isCreditNote": inv.is_credit_note,
        "grossCents": i.totals.gross_cents,
        "netCents": i.totals.net_cents,
        "vatCents": i.totals.vat_cents,
        "paidCents": i.paid_cents,
        "outstandingCents": i.totals.gross_cents - i.paid_cents,
    })
}

/// `open_quotes` — the sent, unanswered offers, plus the count of drafts.
pub async fn execute_open_quotes(account: &Account, args: &Value) -> Reply {
    let day = today();
    let only = match string_arg(args, "customer") {
        Some(name) => Some(find_customers(account, &name).await?),
        None => None,
    };
    let keep = |q: &QuoteSummary| {
        only.as_ref()
            .is_none_or(|customers| customers.iter().any(|c| c.id == q.quote.customer_id))
    };
    let sent: Vec<QuoteSummary> = account
        .acc
        .billing_quotes(Some(QuoteStatus::Sent))
        .await
        .map_err(map_store_err)?
        .into_iter()
        .filter(keep)
        .collect();
    let drafts: Vec<QuoteSummary> = account
        .acc
        .billing_quotes(Some(QuoteStatus::Draft))
        .await
        .map_err(map_store_err)?
        .into_iter()
        .filter(keep)
        .collect();
    let names = names_for(
        account,
        sent.iter()
            .chain(drafts.iter())
            .map(|q| q.quote.customer_id.clone()),
    )
    .await?;
    let open: Vec<Value> = sent
        .iter()
        .take(MAX_LISTED)
        .map(|q| quote_summary_json(q, &names, day))
        .collect();
    // Across the whole book a draft is only counted — it is not an offer yet.
    // Asked about one customer, their drafts are listed too: "what did we
    // quote X" is often an offer nobody has sent.
    let listed_drafts: Vec<Value> = if only.is_some() {
        drafts
            .iter()
            .take(MAX_LISTED)
            .map(|q| quote_summary_json(q, &names, day))
            .collect()
    } else {
        Vec::new()
    };
    ok(json!({
        "open": open,
        "openCount": sent.len(),
        "draftCount": drafts.len(),
        "drafts": listed_drafts,
        "customerFilter": only.as_ref().map(|c| c.iter().map(|x| x.name.clone()).collect::<Vec<_>>()),
    }))
}

/// The quote named by number, or the customer's newest, or a refusal.
async fn resolve_quote(
    account: &Account,
    args: &Value,
    want_status: Option<QuoteStatus>,
) -> Result<BillingQuoteId, Problem> {
    if let Some(number) = string_arg(args, "quote").filter(|n| !n.trim().is_empty()) {
        return account
            .acc
            .billing_quote_id_by_number(&number)
            .await
            .map_err(map_store_err)?
            .ok_or_else(|| {
                unprocessable(format!("no quote carries the number \"{}\"", number.trim()))
            });
    }
    let Some(name) = string_arg(args, "customer") else {
        return Err(unprocessable(
            "name the quote by its number or by the customer",
        ));
    };
    let customer = one_customer(account, &name).await?;
    // "What did we quote X" means the offer X holds — the newest *sent* one
    // when there is one — and only then whatever is newest, draft included.
    let mut newest = None;
    let preference: &[Option<QuoteStatus>] = match want_status {
        Some(status) => &[Some(status)],
        None => &[Some(QuoteStatus::Sent), None],
    };
    for status in preference {
        newest = account
            .acc
            .billing_quotes(*status)
            .await
            .map_err(map_store_err)?
            .into_iter()
            .find(|q| q.quote.customer_id == customer.id);
        if newest.is_some() {
            break;
        }
    }
    newest.map(|q| q.quote.id).ok_or_else(|| {
        unprocessable(match want_status {
            Some(QuoteStatus::Draft) => format!("{} has no draft offer to send", customer.name),
            _ => format!("there is no offer for {}", customer.name),
        })
    })
}

/// `quote_lookup` — one offer in full.
pub async fn execute_quote_lookup(account: &Account, args: &Value) -> Reply {
    let id = resolve_quote(account, args, None).await?;
    let document = account
        .acc
        .billing_quote(&id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| unprocessable("no such quote"))?;
    let day = today();
    let names = names_for(account, std::iter::once(document.quote.customer_id.clone())).await?;
    let mut value = crate::billing_quotes::document_json(&document, day);
    // The customer's other offers, as summaries, so "has it been sent?" and
    // "is there a newer draft?" are answered from the same lookup.
    let others: Vec<Value> = account
        .acc
        .billing_quotes(None)
        .await
        .map_err(map_store_err)?
        .iter()
        .filter(|q| {
            q.quote.customer_id == document.quote.customer_id && q.quote.id != document.quote.id
        })
        .take(MAX_LISTED)
        .map(|q| quote_summary_json(q, &names, day))
        .collect();
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "customerName".to_owned(),
            json!(names.get(document.quote.customer_id.as_str())),
        );
        object.insert("otherQuotes".to_owned(), Value::Array(others));
    }
    ok(value)
}

/// `customer_lookup` — the record, with open offers and unpaid invoices; or
/// the candidates when the name matches several.
pub async fn execute_customer_lookup(account: &Account, args: &Value) -> Reply {
    let name = string_arg(args, "customer").ok_or_else(|| unprocessable("name the customer"))?;
    let mut found = find_customers(account, &name).await?;
    if found.is_empty() {
        return ok(json!({ "customer": Value::Null, "candidates": [] }));
    }
    if found.len() > 1 {
        return ok(json!({
            "customer": Value::Null,
            "candidates": found.iter().map(customer_json).collect::<Vec<_>>(),
        }));
    }
    let customer = found.remove(0);
    let day = today();
    let names = HashMap::from([(customer.id.as_str().to_owned(), customer.name.clone())]);
    let open: Vec<Value> = account
        .acc
        .billing_quotes(Some(QuoteStatus::Sent))
        .await
        .map_err(map_store_err)?
        .iter()
        .filter(|q| q.quote.customer_id == customer.id)
        .take(MAX_LISTED)
        .map(|q| quote_summary_json(q, &names, day))
        .collect();
    let unpaid: Vec<Value> = account
        .acc
        .billing_invoices(Some(InvoiceStatus::Issued))
        .await
        .map_err(map_store_err)?
        .iter()
        .filter(|i| i.invoice.customer_id == customer.id && i.totals.gross_cents > i.paid_cents)
        .take(MAX_LISTED)
        .map(|i| invoice_summary_json(i, &names, day))
        .collect();
    ok(json!({
        "customer": customer_json(&customer),
        "openQuotes": open,
        "unpaidInvoices": unpaid,
        "outstandingCents": unpaid.iter().filter_map(|i| i["outstandingCents"].as_i64()).sum::<i64>(),
    }))
}

/// `unpaid_invoices` — issued and not settled, overdue flagged.
pub async fn execute_unpaid_invoices(account: &Account, args: &Value) -> Reply {
    let day = today();
    let only = match string_arg(args, "customer") {
        Some(name) => Some(find_customers(account, &name).await?),
        None => None,
    };
    let unpaid: Vec<InvoiceSummary> = account
        .acc
        .billing_invoices(Some(InvoiceStatus::Issued))
        .await
        .map_err(map_store_err)?
        .into_iter()
        .filter(|i| i.totals.gross_cents > i.paid_cents)
        .filter(|i| {
            only.as_ref()
                .is_none_or(|customers| customers.iter().any(|c| c.id == i.invoice.customer_id))
        })
        .collect();
    let names = names_for(
        account,
        unpaid.iter().map(|i| i.invoice.customer_id.clone()),
    )
    .await?;
    let listed: Vec<Value> = unpaid
        .iter()
        .take(MAX_LISTED)
        .map(|i| invoice_summary_json(i, &names, day))
        .collect();
    ok(json!({
        "unpaid": listed,
        "unpaidCount": unpaid.len(),
        "overdueCount": unpaid.iter().filter(|i| i.invoice.is_overdue(day)).count(),
        "outstandingCents": unpaid.iter().map(|i| i.totals.gross_cents - i.paid_cents).sum::<i64>(),
    }))
}

async fn resolve_invoice(account: &Account, args: &Value) -> Result<BillingInvoiceId, Problem> {
    if let Some(number) = string_arg(args, "invoice").filter(|n| !n.trim().is_empty()) {
        return account
            .acc
            .billing_invoice_id_by_number(&number)
            .await
            .map_err(map_store_err)?
            .ok_or_else(|| {
                unprocessable(format!(
                    "no invoice carries the number \"{}\"",
                    number.trim()
                ))
            });
    }
    let Some(name) = string_arg(args, "customer") else {
        return Err(unprocessable(
            "name the invoice by its number or by the customer",
        ));
    };
    let customer = one_customer(account, &name).await?;
    account
        .acc
        .billing_invoices(None)
        .await
        .map_err(map_store_err)?
        .into_iter()
        .find(|i| i.invoice.customer_id == customer.id)
        .map(|i| i.invoice.id)
        .ok_or_else(|| unprocessable(format!("there is no invoice for {}", customer.name)))
}

/// `invoice_lookup` — one invoice in full.
pub async fn execute_invoice_lookup(account: &Account, args: &Value) -> Reply {
    let id = resolve_invoice(account, args).await?;
    let document = account
        .acc
        .billing_invoice(&id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| unprocessable("no such invoice"))?;
    let names = names_for(
        account,
        std::iter::once(document.invoice.customer_id.clone()),
    )
    .await?;
    let mut value = crate::billing_invoices::document_json(&document, today());
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "customerName".to_owned(),
            json!(names.get(document.invoice.customer_id.as_str())),
        );
    }
    ok(value)
}

/// The period a totals question names — a word, two dates, or this year.
fn period(args: &Value, day: Date) -> Result<(Date, Date), Problem> {
    let parse = |key: &str| -> Result<Option<Date>, Problem> {
        match string_arg(args, key) {
            None => Ok(None),
            Some(text) => Date::parse(text.trim(), &Iso8601::DATE)
                .map(Some)
                .map_err(|_| unprocessable(format!("{key} must be a date, YYYY-MM-DD"))),
        }
    };
    if let (Some(from), Some(to)) = (parse("from")?, parse("to")?) {
        if from > to {
            return Err(unprocessable("from is after to"));
        }
        return Ok((from, to));
    }
    let first_of = |year: i32, month: Month| Date::from_calendar_date(year, month, 1);
    let last_of = |year: i32, month: Month| -> Result<Date, Problem> {
        let next = if month == Month::December {
            first_of(year + 1, Month::January)
        } else {
            first_of(year, month.next())
        };
        next.map(|d| d - Duration::days(1))
            .map_err(|_| unprocessable("period out of range"))
    };
    let word = string_arg(args, "period").unwrap_or_else(|| "this-year".to_owned());
    let (year, month) = (day.year(), day.month());
    let range = match word.trim().to_lowercase().as_str() {
        "this-month" | "this month" => (first_of(year, month), last_of(year, month)?),
        "last-month" | "last month" => {
            let (y, m) = if month == Month::January {
                (year - 1, Month::December)
            } else {
                (year, month.previous())
            };
            (first_of(y, m), last_of(y, m)?)
        }
        "last-year" | "last year" => (
            first_of(year - 1, Month::January),
            last_of(year - 1, Month::December)?,
        ),
        "this-year" | "this year" | "ytd" => (first_of(year, Month::January), day),
        other => {
            return Err(unprocessable(format!(
                "period \"{other}\" is not this-month, last-month, this-year or last-year"
            )));
        }
    };
    Ok((
        range.0.map_err(|_| unprocessable("period out of range"))?,
        range.1,
    ))
}

/// `billing_totals` — invoiced, paid, outstanding and VAT over a period.
pub async fn execute_billing_totals(account: &Account, args: &Value) -> Reply {
    let day = today();
    let (from, to) = period(args, day)?;
    let mut invoiced = 0i64;
    let mut net = 0i64;
    let mut vat = 0i64;
    let mut paid = 0i64;
    let mut credited = 0i64;
    let mut count = 0usize;
    let mut overdue = 0usize;
    for status in [InvoiceStatus::Issued, InvoiceStatus::Paid] {
        for i in account
            .acc
            .billing_invoices(Some(status))
            .await
            .map_err(map_store_err)?
        {
            let Some(issued) = i.invoice.issue_date else {
                continue;
            };
            if issued < from || issued > to {
                continue;
            }
            if i.invoice.is_credit_note {
                credited += i.totals.gross_cents;
                continue;
            }
            count += 1;
            invoiced += i.totals.gross_cents;
            net += i.totals.net_cents;
            vat += i.totals.vat_cents;
            paid += i.paid_cents;
            if i.invoice.is_overdue(day) {
                overdue += 1;
            }
        }
    }
    let currency = account
        .acc
        .billing_settings()
        .await
        .map_err(map_store_err)?
        .base_currency;
    ok(json!({
        "from": iso_date(from),
        "to": iso_date(to),
        "currency": currency,
        "invoiceCount": count,
        "invoicedGrossCents": invoiced,
        "invoicedNetCents": net,
        "vatCents": vat,
        "paidCents": paid,
        "outstandingCents": invoiced - paid,
        "overdueCount": overdue,
        "creditNotesGrossCents": credited,
    }))
}

/// `send_quote` — a write, run on the asker's approval.
pub async fn execute_send_quote(account: &Account, args: &Value) -> Reply {
    let id = resolve_quote(account, args, Some(QuoteStatus::Draft)).await?;
    let document = account
        .acc
        .send_billing_quote(&id)
        .await
        .map_err(map_store_err)?;
    ok(
        json!({ "kind": "quote", "quote": crate::billing_quotes::document_json(&document, today()) }),
    )
}

/// `issue_invoice` — a write: the customer's newest draft becomes owed.
pub async fn execute_issue_invoice(account: &Account, args: &Value) -> Reply {
    let name = string_arg(args, "customer").ok_or_else(|| unprocessable("name the customer"))?;
    let customer = one_customer(account, &name).await?;
    let draft = account
        .acc
        .billing_invoices(Some(InvoiceStatus::Draft))
        .await
        .map_err(map_store_err)?
        .into_iter()
        .find(|i| i.invoice.customer_id == customer.id)
        .ok_or_else(|| unprocessable(format!("{} has no draft invoice to issue", customer.name)))?;
    let document = account
        .acc
        .issue_billing_invoice(&draft.invoice.id)
        .await
        .map_err(map_store_err)?;
    ok(
        json!({ "kind": "invoice", "invoice": crate::billing_invoices::document_json(&document, today()) }),
    )
}

/// `record_payment` — a write: money received against an invoice.
pub async fn execute_record_payment(account: &Account, args: &Value) -> Reply {
    let id = resolve_invoice(account, args).await?;
    let document = account
        .acc
        .billing_invoice(&id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| unprocessable("no such invoice"))?;
    let outstanding = document.totals.gross_cents - document.paid_cents;
    let amount = integer(args.get("amountCents"), "amountCents")
        .map_err(unprocessable)?
        .unwrap_or(outstanding);
    if amount <= 0 {
        return Err(unprocessable("nothing is outstanding on that invoice"));
    }
    let paid_on = match string_arg(args, "paidOn") {
        None => None,
        Some(text) => Some(
            Date::parse(text.trim(), &Iso8601::DATE)
                .map_err(|_| unprocessable("paidOn must be a date, YYYY-MM-DD"))?,
        ),
    };
    let payment = NewPayment {
        paid_on,
        amount_cents: amount,
        method: string_arg(args, "method").unwrap_or_else(|| "transfer".to_owned()),
        reference: string_arg(args, "reference").unwrap_or_default(),
    };
    let payment_id = account
        .acc
        .record_billing_payment(&id, &payment)
        .await
        .map_err(map_store_err)?;
    let after = account
        .acc
        .billing_invoice(&id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| unprocessable("no such invoice"))?;
    ok(json!({
        "kind": "payment",
        "paymentId": payment_id.as_str(),
        "invoice": crate::billing_invoices::document_json(&after, today()),
    }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use alo_ai::billing_intents::BILLING;

    /// Every `/billing/` route the router registers is the adapter of a verb
    /// or excluded with a reason — the coverage ADR 0058 makes structural.
    #[test]
    fn every_billing_route_is_a_verb_or_an_exclusion() {
        let router = include_str!("server.rs");
        let missing = BILLING.uncovered(router, "/billing/");
        assert!(
            missing.is_empty(),
            "routes with neither a verb nor a reason: {missing:?}"
        );
        // …and every verb's route exists, so an intent cannot name a route the
        // app does not have.
        let routes = alo_ai::routes_in(router, "/billing/");
        for intent in BILLING.intents {
            for route in intent.routes {
                assert!(
                    routes.contains(&(*route).to_owned()),
                    "{}: {route} is not a route",
                    intent.name
                );
            }
        }
    }

    #[test]
    fn every_verb_the_registry_offers_is_dispatched() {
        let dispatch = include_str!("agent.rs");
        for intent in BILLING.intents {
            assert!(
                dispatch.contains(&format!("\"{}\" =>", intent.name)),
                "{} has no executor in the dispatch",
                intent.name
            );
        }
    }

    #[test]
    fn money_and_quantities_are_shown_beside_their_integers() {
        assert_eq!(money(24_900, "EUR"), "249.00 EUR");
        assert_eq!(money(5, "EUR"), "0.05 EUR");
        assert_eq!(money(-1_250, "USD"), "-12.50 USD");
        assert_eq!(quantity(1_500), "1.5");
        assert_eq!(quantity(2_000), "2");
        assert_eq!(quantity(250), "0.25");
        let shown = with_display(
            json!({
                "currency": "EUR",
                "netCents": 24_900,
                "lines": [{ "qtyMilli": 1_500, "unitPriceCents": 12_000 }],
                "nested": { "currency": "USD", "grossCents": 100 },
                "note": "untouched"
            }),
            None,
        );
        assert_eq!(shown["netDisplay"], "249.00 EUR");
        assert_eq!(shown["netCents"], 24_900, "the integer stays");
        assert_eq!(shown["lines"][0]["quantityDisplay"], "1.5");
        assert_eq!(shown["lines"][0]["unitPriceDisplay"], "120.00 EUR");
        assert_eq!(
            shown["nested"]["grossDisplay"], "1.00 USD",
            "the nearest currency wins"
        );
        assert_eq!(shown["note"], "untouched");
        // No currency anywhere: cents stay, nothing is invented.
        let bare = with_display(json!({ "grossCents": 100 }), None);
        assert!(bare.get("grossDisplay").is_none());
    }

    #[test]
    fn a_period_word_becomes_the_dates_a_bookkeeper_means() {
        let day = Date::from_calendar_date(2026, Month::August, 28).unwrap();
        let f = |p: &str| period(&json!({ "period": p }), day).unwrap();
        assert_eq!(
            f("this-month"),
            (
                Date::from_calendar_date(2026, Month::August, 1).unwrap(),
                Date::from_calendar_date(2026, Month::August, 31).unwrap()
            )
        );
        assert_eq!(
            f("last month"),
            (
                Date::from_calendar_date(2026, Month::July, 1).unwrap(),
                Date::from_calendar_date(2026, Month::July, 31).unwrap()
            )
        );
        assert_eq!(
            f("this-year"),
            (
                Date::from_calendar_date(2026, Month::January, 1).unwrap(),
                day
            )
        );
        assert_eq!(
            f("last-year"),
            (
                Date::from_calendar_date(2025, Month::January, 1).unwrap(),
                Date::from_calendar_date(2025, Month::December, 31).unwrap()
            )
        );
        assert_eq!(
            period(&json!({ "from": "2026-03-01", "to": "2026-03-31" }), day).unwrap(),
            (
                Date::from_calendar_date(2026, Month::March, 1).unwrap(),
                Date::from_calendar_date(2026, Month::March, 31).unwrap()
            )
        );
        assert!(period(&json!({ "period": "whenever" }), day).is_err());
        assert!(period(&json!({ "from": "2026-04-01", "to": "2026-03-01" }), day).is_err());
        // January's "last month" is December of the year before.
        let jan = Date::from_calendar_date(2026, Month::January, 5).unwrap();
        assert_eq!(
            period(&json!({ "period": "last-month" }), jan).unwrap().0,
            Date::from_calendar_date(2025, Month::December, 1).unwrap()
        );
    }
}
