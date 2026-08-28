//! Billing quotes HTTP surface (alo Billing, ADR 0035, wave B1) — the whole
//! life of an offer over [`alo_store::billing_quotes`]: draft CRUD, the
//! status-filtered list, sending, and the three answers an open offer can get.
//!
//! It is deliberately the shape of [`crate::billing_invoices`], because a quote
//! is the same kind of object: authenticated and tenant-scoped through the
//! account door, no validation duplicated from the store, the header and the
//! lines in one body, `PATCH` as a merge onto the stored record, lines and
//! totals rendered by [`crate::billing_document`] so the client never computes
//! money, and a strict `?status=` filter (`422` on an unrecognised value).
//!
//! What is its own:
//!
//! - **`POST …/accept` answers two documents.** Accepting is the point of the
//!   whole surface: it closes the offer *and* raises the draft invoice for it,
//!   in one store transaction, so the response carries the closed quote and the
//!   invoice it produced. A client never has to ask "did it also make one?".
//! - **`expired` is computed, never stored** ([`Quote::is_expired`]) — like an
//!   invoice's `overdue`, a stored flag would be wrong every midnight — and it
//!   is independent of the `expired` *status*, which is a decision somebody
//!   recorded. A lapsed offer may still be accepted; the store refuses on
//!   state, never on a date.
//! - **`GET …/{id}` answers `invoiceId`** — the draft its acceptance raised, or
//!   `null`. That is the link the quote screen (B1.15) follows.
//!
//! Lifecycle transitions are their own `POST`s, never fields on the `PATCH`:
//! sending assigns a number and freezes the document, and answering an offer is
//! a decision with a date; neither may happen because an editor submitted a
//! stale form. The store owns the transitions and their refusals (`409` for an
//! offer in the wrong state, `422` for one that cannot be sent at all) — this
//! layer only maps them.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};
use time::Date;

use alo_store::billing_quote_lines::NewQuoteLine;
use alo_store::billing_quotes::{
    AcceptedAs, NewQuote, Quote, QuoteDocument, QuoteStatus, QuoteSummary,
};
use alo_store::{AccountStore, BillingCustomerId, BillingProductId, BillingQuoteId, Line};

use crate::billing::{iso, iso_date, map_store_err, parse_body};
use crate::billing_document::{LineBody, today, with_body, with_totals};
use time::OffsetDateTime;

use crate::billing_pdf as pdf;
use crate::billing_print::{self as print, Banner, DocumentKind, PrintDocument, PrintQuery};
use crate::error::Problem;
use crate::quote_design::QuoteDesign;
use crate::state::{AppState, authenticate};

/// The header of an offer as JSON, with the derived `expired` flag.
///
/// `number`, `sentDate` and `validUntil` are `null` while the quote is a draft
/// — it has not consumed a number — and `decidedDate` is `null` until the offer
/// is answered, which is how a client tells the three phases of its life apart
/// without parsing `status`.
fn quote_json(q: &Quote, today: Date) -> Value {
    json!({
        "id": q.id.as_str(),
        "customerId": q.customer_id.as_str(),
        "status": q.status.as_str(),
        "currency": q.currency,
        "number": q.number,
        "sentDate": q.sent_date.map(iso_date),
        "validUntil": q.valid_until.map(iso_date),
        "validDays": q.valid_days,
        "decidedDate": q.decided_date.map(iso_date),
        "expired": q.is_expired(today),
        "reference": q.reference,
        "note": q.note,
        "createdBy": q.created_by,
        "createdAt": iso(q.created_at),
        "updatedAt": iso(q.updated_at),
    })
}

/// A whole offer: header, lines in print order, totals.
///
/// A quote line carries `productId` where an invoice line does not — it is the
/// field that decides what accepting the offer raises, so a client editing an
/// offer has to be able to see and set it.
pub(crate) fn document_json(d: &QuoteDocument, today: Date) -> Value {
    let plain: Vec<Line> = d.lines.iter().map(|l| l.line.clone()).collect();
    let mut body = with_body(quote_json(&d.quote, today), &plain, &d.totals);
    if let Some(lines) = body.get_mut("lines").and_then(Value::as_array_mut) {
        for (rendered, stored) in lines.iter_mut().zip(&d.lines) {
            if let Some(object) = rendered.as_object_mut() {
                object.insert(
                    "productId".to_owned(),
                    stored
                        .product_id
                        .as_ref()
                        .map_or(Value::Null, |id| Value::String(id.as_str().to_owned())),
                );
            }
        }
    }
    body
}

/// A list entry: the header and what the offer is worth, without the lines.
fn summary_json(s: &QuoteSummary, today: Date) -> Value {
    with_totals(quote_json(&s.quote, today), &s.totals)
}

/// The stored header as writable input — the base a `PATCH` merges onto.
///
/// The two "take the default" fields are handed over as stated values (`Some`),
/// because on an existing quote they were resolved when it was raised: a
/// `PATCH` that does not mention the currency must keep the document's own,
/// never re-read the customer's current one.
fn editable(q: &Quote) -> NewQuote {
    NewQuote {
        customer_id: q.customer_id.clone(),
        currency: Some(q.currency.clone()),
        valid_days: Some(q.valid_days),
        reference: q.reference.clone(),
        note: q.note.clone(),
    }
}

/// The writable parts of an offer, every one optional.
///
/// The same body serves `POST` (merged onto a blank header for the named
/// customer) and `PATCH` (merged onto the stored one). Unknown fields are
/// ignored so the contract can grow additively; the response carries the stored
/// document, which is where a caller sees that a misspelled field did nothing.
///
/// There is no `status`, `number`, `sentDate`, `validUntil` or `decidedDate`
/// here. They are not writable from a request at all: they move only through
/// the lifecycle routes below.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuoteBody {
    #[serde(default)]
    customer_id: Option<String>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    valid_days: Option<i32>,
    #[serde(default)]
    reference: Option<String>,
    #[serde(default)]
    note: Option<String>,
    /// The whole line set, in print order. Absent leaves the stored lines
    /// alone; `[]` empties the offer, which is a legitimate thing to do to a
    /// draft (it simply cannot then be sent).
    #[serde(default)]
    lines: Option<Vec<QuoteLineBody>>,
}

/// One offered line as a client states it: the shared line body, plus the
/// catalog item it sells.
///
/// Flattened rather than nested so a quote line reads on the wire exactly as an
/// invoice line does with one field more — the same shape a sales-order line
/// already has, and the field that decides what accepting the offer raises
/// (ADR 0054 §5).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuoteLineBody {
    /// The item being sold. Absent, `null` or `""` is a charge in words —
    /// assembly, delivery, a discount — and an offer made only of those becomes
    /// an invoice rather than an order.
    #[serde(default)]
    product_id: Option<String>,
    #[serde(flatten)]
    line: LineBody,
}

impl QuoteLineBody {
    fn into_line(self) -> NewQuoteLine {
        NewQuoteLine {
            // A cleared picker sends `""`; the store reads that as no product.
            product_id: self.product_id.map(BillingProductId::new),
            line: self.line.into_line(),
        }
    }
}

impl QuoteBody {
    /// Whether the body says anything about the header at all.
    ///
    /// A `PATCH` carrying only `lines` must not touch the header: replaying the
    /// stored header would re-resolve the customer, and a draft whose customer
    /// was archived after it was raised would then refuse to have its lines
    /// edited — a dead end with no way out but deleting the draft.
    fn states_header(&self) -> bool {
        self.customer_id.is_some()
            || self.currency.is_some()
            || self.valid_days.is_some()
            || self.reference.is_some()
            || self.note.is_some()
    }

    /// Merges the stated header fields onto `base`, leaving the rest as they
    /// were.
    fn header(&self, base: NewQuote) -> NewQuote {
        NewQuote {
            customer_id: self
                .customer_id
                .clone()
                .map_or(base.customer_id, BillingCustomerId::new),
            currency: self.currency.clone().or(base.currency),
            valid_days: self.valid_days.or(base.valid_days),
            reference: self.reference.clone().unwrap_or(base.reference),
            note: self.note.clone().unwrap_or(base.note),
        }
    }

    /// The line set the body asks for, if it states one.
    fn lines(self) -> Option<Vec<NewQuoteLine>> {
        self.lines
            .map(|lines| lines.into_iter().map(QuoteLineBody::into_line).collect())
    }
}

/// Loads one of the tenant's offers, or fails with the `404` an id from another
/// tenant gets.
async fn load(acc: &AccountStore, id: &BillingQuoteId) -> Result<QuoteDocument, Problem> {
    acc.billing_quote(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such quote"))
}

/// Query string of the list route.
#[derive(Deserialize)]
pub struct ListQuery {
    /// `status=draft|sent|accepted|declined|expired`; absent lists everything.
    #[serde(default)]
    status: Option<String>,
}

/// Reads the status filter, refusing a value that is not one of the five.
///
/// A blank value means "no filter" (a UI whose select is on "all" sends an
/// empty parameter), and the comparison is case-insensitive; anything else is a
/// `422` naming what is accepted. Deliberately strict, like the invoice list:
/// a filter that silently widened to "everything" would show a salesperson
/// declined offers among their open ones.
fn status_filter(raw: Option<&str>) -> Result<Option<QuoteStatus>, Problem> {
    let Some(raw) = raw.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    QuoteStatus::parse(&raw.to_ascii_lowercase())
        .map(Some)
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "status must be one of draft, sent, accepted, declined, expired",
            )
        })
}

/// `GET /billing/quotes[?status=sent]` → `{"quotes":[…]}` — the tenant's
/// offers, newest first, each with its computed totals and `expired` flag but
/// without its lines.
pub async fn list_quotes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let status = status_filter(q.status.as_deref())?;
    let quotes = account
        .acc
        .billing_quotes(status)
        .await
        .map_err(map_store_err)?;
    let today = today();
    Ok(Json(json!({
        "quotes": quotes.iter().map(|s| summary_json(s, today)).collect::<Vec<_>>(),
    })))
}

/// `POST /billing/quotes` `{customerId, lines?, …}` → `{"quote":{…}}` — raise a
/// **draft** offer. Only `customerId` is required; the currency falls back to
/// the customer's own and the validity to the store's default, and both are
/// then snapshotted on the document.
///
/// The lines are validated **before** the header is written, so a typo in the
/// last line does not leave an empty draft behind.
pub async fn create_quote(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: QuoteBody = parse_body(&body)?;
    // The one check that lives at the edge: which customer an offer is made to
    // is not a field rule the store can own, and letting an absent id fall
    // through would answer "no such customer" (404) to a request that never
    // named one.
    if req
        .customer_id
        .as_ref()
        .is_none_or(|id| id.trim().is_empty())
    {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "customerId is required to raise a quote",
        ));
    }
    let header = req.header(NewQuote::for_customer(BillingCustomerId::new("")));
    let lines = req.lines();
    if let Some(lines) = lines.as_deref() {
        account
            .acc
            .billing_line_totals(&lines.iter().map(|l| l.line.clone()).collect::<Vec<_>>())
            .map_err(map_store_err)?;
    }
    let id = account
        .acc
        .create_billing_quote(&header)
        .await
        .map_err(map_store_err)?;
    if let Some(lines) = lines.as_deref() {
        account
            .acc
            .set_billing_quote_lines(&id, lines)
            .await
            .map_err(map_store_err)?;
    }
    let document = load(&account.acc, &id).await?;
    Ok(Json(json!({ "quote": document_json(&document, today()) })))
}

/// `GET /billing/quotes/{id}` → `{"quote":{…},"invoiceId":…}` — the whole offer
/// with its lines and totals, and the draft invoice its acceptance raised
/// (`null` for every offer that was not accepted).
pub async fn get_quote(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = BillingQuoteId::new(id);
    let document = load(&account.acc, &id).await?;
    let invoice = account
        .acc
        .billing_invoice_for_quote(&id)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "quote": document_json(&document, today()),
        "invoiceId": invoice.as_ref().map(alo_store::BillingInvoiceId::as_str),
    })))
}

/// `PATCH /billing/quotes/{id}` `{…, lines?}` → `{"quote":{…}}` — edit a
/// **draft**: merge the stated header fields onto the stored ones, and replace
/// the line set if one is sent.
///
/// An offer that has been sent refuses the whole request with a `409` naming
/// its state; that refusal comes from the store, under the row lock the write
/// itself takes, so an edit that raced a send is refused rather than applied to
/// a document the customer already holds.
pub async fn update_quote(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: QuoteBody = parse_body(&body)?;
    let id = BillingQuoteId::new(id);
    let stored = load(&account.acc, &id).await?;
    let header = req
        .states_header()
        .then(|| req.header(editable(&stored.quote)));
    let lines = req.lines();
    // Validated before either write, so a bad line cannot leave an offer with
    // its new header and its old lines.
    if let Some(lines) = lines.as_deref() {
        account
            .acc
            .billing_line_totals(&lines.iter().map(|l| l.line.clone()).collect::<Vec<_>>())
            .map_err(map_store_err)?;
    }
    if let Some(header) = header {
        account
            .acc
            .update_billing_quote(&id, &header)
            .await
            .map_err(map_store_err)?;
    }
    if let Some(lines) = lines.as_deref() {
        account
            .acc
            .set_billing_quote_lines(&id, lines)
            .await
            .map_err(map_store_err)?;
    }
    let document = load(&account.acc, &id).await?;
    Ok(Json(json!({ "quote": document_json(&document, today()) })))
}

/// `DELETE /billing/quotes/{id}` → `{"status":"ok"}` — discard a **draft** and
/// its lines.
///
/// The only offer that is ever removed: a draft never consumed a number and was
/// never made to anybody. A sent one is answered — declined or expired (`409`
/// here) — which keeps it readable and keeps the series unbroken.
pub async fn delete_quote(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_billing_quote(&BillingQuoteId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `POST /billing/quotes/{id}/send` → `{"quote":{…}}` — assign the next number
/// from the tenant's quote series, stamp the send date and the day the offer
/// stands until, and freeze the content.
///
/// Not idempotent on purpose: re-sending answers `409` and the document it
/// names already carries its number, so a client that retried after a timeout
/// can read what happened rather than spending a second number on the same
/// offer. An offer with no lines is `422`.
///
/// It sends no email — this records that the offer was made; the mail draft is
/// B1.18's.
pub async fn send_quote(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let document = account
        .acc
        .send_billing_quote(&BillingQuoteId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "quote": document_json(&document, today()) })))
}

/// `POST /billing/quotes/{id}/accept` → `{"quote":{…},"invoice":{…}}` — the
/// customer took the offer.
///
/// One store transaction closes the quote and raises the document it becomes, so
/// the response hands back both: the offer with its decision date, and what it
/// turned into.
///
/// **Which document depends on the lines** (ADR 0054 §5). An offer naming any
/// catalog item is for goods and becomes a **draft sales order**, because goods
/// are reserved, picked and delivered before anybody is billed for them; the
/// body then carries `salesOrder` and `invoice` is `null`. An offer of services
/// names no item, has nothing to pick, and becomes the **draft invoice** it
/// always did — that path is unchanged, and a client that only ever sells
/// services sees exactly what it saw before.
///
/// Nothing is issued and nothing is confirmed. The invoice's number comes from
/// the ordinary `/billing/invoices/{id}/issue`, and the order's from
/// `/inventory/sales-orders/{id}/confirm` — which is also where an over-promise
/// is refused, so accepting an offer can never quietly commit stock.
///
/// A lapsed offer can still be accepted: honouring one a few days late is a
/// decision the tenant is entitled to make, and the store refuses on state,
/// never on a date.
pub async fn accept_quote(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let accepted = account
        .acc
        .accept_billing_quote(&BillingQuoteId::new(id))
        .await
        .map_err(map_store_err)?;
    let today = today();
    let mut body = json!({
        "quote": document_json(&accepted.quote, today),
        "invoice": Value::Null,
        "salesOrder": Value::Null,
    });
    match &accepted.outcome {
        AcceptedAs::InvoiceDraft(invoice_id) => {
            let invoice = account
                .acc
                .billing_invoice(invoice_id)
                .await
                .map_err(map_store_err)?
                .ok_or_else(Problem::server_error)?;
            body["invoice"] = crate::billing_invoices::document_json(&invoice, today);
        }
        AcceptedAs::SalesOrder(order_id) => {
            let order = account
                .acc
                .inv_sales_order(order_id)
                .await
                .map_err(map_store_err)?
                .ok_or_else(Problem::server_error)?;
            body["salesOrder"] = crate::inventory_so::document_json(&order, today);
        }
    }
    Ok(Json(body))
}

/// `POST /billing/quotes/{id}/decline` → `{"quote":{…}}` — the customer turned
/// the offer down, or the tenant withdrew it. Either way the document stands,
/// readable, with the day it was closed. Terminal: a change of mind is a new
/// quote.
pub async fn decline_quote(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let document = account
        .acc
        .decline_billing_quote(&BillingQuoteId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "quote": document_json(&document, today()) })))
}

/// `POST /billing/quotes/{id}/expire` → `{"quote":{…}}` — the offer lapsed
/// without an answer and somebody stopped chasing it.
///
/// Deliberately an explicit act rather than a background sweep: nothing in the
/// business changes at midnight on the validity date, and the decision to give
/// up on an offer has a date worth recording. Until then the `expired` flag
/// already tells a reader the offer has lapsed.
pub async fn expire_quote(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let document = account
        .acc
        .expire_billing_quote(&BillingQuoteId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "quote": document_json(&document, today()) })))
}

/// An offer loaded with everything its printed forms need: the document,
/// the two parties, and the design the studio saved for it.
///
/// One loader for `/print` and `/pdf`, so the page and the file are built
/// from the same facts — the **same** [`PrintDocument`] rendered two ways
/// (`docs/design/billing.md`, B1.17).
pub struct PrintableQuote {
    document: QuoteDocument,
    lines: Vec<Line>,
    customer: alo_store::Customer,
    issuer: alo_store::BillingSettings,
    design: Option<QuoteDesign>,
}

impl PrintableQuote {
    /// Loads one of the tenant's offers with its parties and design, or fails
    /// with the `404` an id from another tenant gets.
    pub async fn load(acc: &AccountStore, id: &BillingQuoteId) -> Result<Self, Problem> {
        let document = load(acc, id).await?;
        let (customer, issuer) = print::parties(acc, &document.quote.customer_id).await?;
        let design = acc
            .billing_quote_design(id)
            .await
            .map_err(map_store_err)?
            .map(|record| QuoteDesign::parse(&record.design));
        // The printed document is the offer as the customer reads it: a
        // description, a quantity, a price. Which of our catalog items a line
        // happens to be is our bookkeeping and belongs on no printed page.
        let lines = document.lines.iter().map(|l| l.line.clone()).collect();
        Ok(Self {
            document,
            lines,
            customer,
            issuer,
            design,
        })
    }

    /// The document as the renderers take it.
    ///
    /// The same page an invoice prints on, with an offer's words: its two dates
    /// are the day it was made and the day it stands until, and it says plainly
    /// that nothing is payable on it — a document that merely omitted the bank
    /// details would read as one that forgot them.
    ///
    /// **"Past its date" is not "closed".** The banner is driven by the offer's
    /// *status*, never by [`Quote::is_expired`]: a lapsed offer that nobody has
    /// answered is still open, and the store will still accept it
    /// (`docs/design/billing.md`). The validity date is on the page for the
    /// customer to read.
    #[must_use]
    pub fn as_document(&self) -> PrintDocument<'_> {
        let quote = &self.document.quote;
        PrintDocument {
            kind: DocumentKind::Quote,
            banner: match quote.status {
                QuoteStatus::Draft => Some(Banner::Draft),
                QuoteStatus::Declined | QuoteStatus::Expired => Some(Banner::Closed),
                // An accepted offer is a record of what was agreed, not a spent
                // document: it prints as it was sent.
                QuoteStatus::Sent | QuoteStatus::Accepted => None,
            },
            number: quote.number.as_deref(),
            primary_date: quote.sent_date,
            secondary_date: quote.valid_until,
            reference: &quote.reference,
            note: &quote.note,
            currency: &quote.currency,
            payment_terms_days: None,
            credits_number: None,
            party: print::Party::customer(&self.customer),
            lines: &self.lines,
            totals: &self.document.totals,
            // An offer is not a tax point: nothing is chargeable on it, so there is
            // no rate to freeze and nothing to restate (B1.21). It is converted, if
            // at all, on the invoice the acceptance raises.
            restated: None,
            issuer: &self.issuer,
            content: self.design.as_ref(),
        }
    }
}

/// `GET /billing/quotes/{id}/print[?lang=]` → the printable offer as one
/// self-contained HTML page ([`crate::billing_print`]), its designed content
/// included ([`PrintableQuote::as_document`]).
pub async fn print_quote(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<PrintQuery>,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let printable = PrintableQuote::load(&account.acc, &BillingQuoteId::new(id)).await?;
    let printed = printable.as_document();
    Ok(print::response(print::render(&printed, query.strings())))
}

/// `GET /billing/quotes/{id}/pdf[?lang=]` → the offer as a PDF file
/// ([`crate::billing_pdf`]) — the **same** document the page is rendered
/// from, laid out a second way, with the studio's content and pictures set on
/// the sheet ([`crate::quote_design_pdf`]). Served as an attachment named by
/// the document's own heading, exactly as an invoice's PDF is.
pub async fn pdf_quote(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<PrintQuery>,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let printable = PrintableQuote::load(&account.acc, &BillingQuoteId::new(id)).await?;
    let printed = printable.as_document();
    let strings = query.strings();
    let bytes = pdf::render(&printed, strings, pdf::stamp(OffsetDateTime::now_utc()));
    Ok(pdf::response(bytes, &pdf::file_name(&printed, strings)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: Value) -> QuoteBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn stored() -> NewQuote {
        NewQuote {
            customer_id: BillingCustomerId::new("cust-1"),
            currency: Some("EUR".to_owned()),
            valid_days: Some(14),
            reference: "RFQ-77".to_owned(),
            note: "Prices exclude on-site work.".to_owned(),
        }
    }

    #[test]
    fn an_empty_patch_states_no_header_and_changes_nothing() {
        let req = body(json!({}));
        assert!(
            !req.states_header(),
            "an empty body must not touch the header"
        );
        let merged = req.header(stored());
        assert_eq!(merged.customer_id.as_str(), "cust-1");
        assert_eq!(merged.currency.as_deref(), Some("EUR"));
        assert_eq!(merged.valid_days, Some(14));
        assert_eq!(merged.reference, "RFQ-77");
        assert_eq!(merged.note, "Prices exclude on-site work.");
        assert!(body(json!({})).lines().is_none());
    }

    #[test]
    fn a_lines_only_patch_leaves_the_header_alone() {
        // The header would otherwise be replayed through the store's customer
        // check, and a draft whose customer was archived meanwhile could never
        // have its lines edited again.
        let req = body(json!({ "lines": [] }));
        assert!(!req.states_header());
        assert_eq!(req.lines().map(|l| l.len()), Some(0));
    }

    #[test]
    fn a_stated_field_replaces_and_leaves_its_neighbours_alone() {
        let req = body(json!({ "validDays": 7 }));
        assert!(req.states_header());
        let merged = req.header(stored());
        assert_eq!(merged.valid_days, Some(7));
        assert_eq!(merged.reference, "RFQ-77");
        assert_eq!(merged.customer_id.as_str(), "cust-1");
    }

    #[test]
    fn create_starts_from_the_customers_currency_and_the_default_validity() {
        // `None` on both means "take the default", which is what a UI that has
        // not asked the user must send.
        let merged = body(json!({ "customerId": "cust-2" }))
            .header(NewQuote::for_customer(BillingCustomerId::new("")));
        assert_eq!(merged.customer_id.as_str(), "cust-2");
        assert!(merged.currency.is_none() && merged.valid_days.is_none());
        assert!(merged.reference.is_empty() && merged.note.is_empty());
    }

    #[test]
    fn the_number_the_dates_and_the_status_are_not_writable_fields() {
        // They are ignored like any unknown field: an offer's number and the
        // day it was decided are stamped by the transitions, never sent.
        let req = body(json!({
            "number": "QUO-2026-09999", "sentDate": "2020-01-01",
            "validUntil": "2030-01-01", "decidedDate": "2020-01-01",
            "status": "accepted",
        }));
        assert!(!req.states_header(), "none of those is a writable field");
        let merged = req.header(stored());
        assert_eq!(merged.reference, "RFQ-77");
        assert_eq!(merged.valid_days, Some(14));
    }

    #[test]
    fn a_validity_with_a_decimal_point_is_refused_never_rounded() {
        for bad in [json!({ "validDays": 14.5 }), json!({ "validDays": "14" })] {
            assert!(
                serde_json::from_value::<QuoteBody>(bad.clone()).is_err(),
                "{bad} should have been refused"
            );
        }
    }

    #[test]
    fn the_status_filter_accepts_the_five_states_and_nothing_else() {
        assert_eq!(status_filter(None).ok().flatten(), None);
        for blank in ["", "   "] {
            assert_eq!(status_filter(Some(blank)).ok().flatten(), None, "{blank:?}");
        }
        for (raw, expected) in [
            ("draft", QuoteStatus::Draft),
            ("SENT", QuoteStatus::Sent),
            (" accepted ", QuoteStatus::Accepted),
            ("Declined", QuoteStatus::Declined),
            ("expired", QuoteStatus::Expired),
        ] {
            assert_eq!(
                status_filter(Some(raw)).ok().flatten(),
                Some(expected),
                "{raw:?}"
            );
        }
        // An invoice's states are not a quote's, and "open" is not a status at
        // all — a filter is never approximated.
        for bad in ["issued", "paid", "void", "open", "draft,sent"] {
            let problem = status_filter(Some(bad))
                .err()
                .unwrap_or_else(|| panic!("{bad:?} should have been refused"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        }
    }
}
