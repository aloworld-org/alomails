//! Billing invoices HTTP surface (alo Billing, ADR 0035, wave B1) — the whole
//! life of a document over [`alo_store::billing_invoices`]: draft CRUD, the
//! status-filtered list, issuing, voiding and crediting.
//!
//! It shares the conventions of [`crate::billing_customers`] — authenticated
//! and tenant-scoped through the account door, no validation duplicated from
//! the store, every write answered with the stored record, `PATCH` as a merge
//! onto it — and adds four that belong to a document carrying money.
//!
//! - **The client never computes money.** The lines and `totals` of every
//!   response are built by [`crate::billing_document`], which quotes share, so
//!   the two surfaces can never report a line — or a total — in two shapes;
//!   there is no writable total anywhere here, so no request can influence what
//!   a document is worth except by changing its lines.
//! - **The header and the lines are one body.** A draft editor saves the
//!   document it is looking at, not a patch stream, so `lines` is an ordinary
//!   field of the invoice body and replaces the whole set in the order sent.
//!   Absent, it leaves the lines alone.
//! - **`overdue` is computed, never stored** ([`Invoice::is_overdue`]) — a
//!   stored flag would be wrong every midnight.
//! - **The status filter is strict.** Unlike the forgiving boolean flags of
//!   [`crate::billing`], an unrecognised `?status=` is a `422` rather than
//!   being ignored: a filter that silently widens to "everything" would show a
//!   bookkeeper drafts among their issued documents, which is the one place a
//!   list must not be approximate.
//!
//! Lifecycle transitions are their own `POST`s, never fields on the `PATCH`.
//! Issuing assigns a legal number and freezes the document, and voiding
//! cancels one; neither may happen because an editor sent a stale form. The
//! store owns the transitions and their refusals (`409` for a document in the
//! wrong state, `422` for one that cannot be issued at all) — this layer only
//! maps them.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};
use time::{Date, OffsetDateTime};

use alo_store::billing_invoices::{
    Invoice, InvoiceDocument, InvoiceStatus, InvoiceSummary, NewInvoice,
};
use alo_store::billing_payments::Settlement;
use alo_store::billing_settings::BillingSettings;
use alo_store::{
    AccountStore, BillingCustomerId, BillingInvoiceId, BillingQuoteId, Customer, NewLine,
};

use crate::billing::{flag, iso, iso_date, map_store_err, parse_body};
use crate::billing_document::{LineBody, today, with_body, with_totals};
use crate::billing_payments::settlement_json;
use crate::billing_pdf as pdf;
use crate::billing_print::{self as print, Banner, DocumentKind, PrintDocument, PrintQuery};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The header of a document as JSON, with the derived `overdue` flag.
///
/// `number`, `issueDate` and `dueDate` are `null` while the document is a
/// draft — it has not consumed a number — which is also how a client tells the
/// two states apart without parsing `status`.
fn invoice_json(i: &Invoice, today: Date) -> Value {
    json!({
        "id": i.id.as_str(),
        "customerId": i.customer_id.as_str(),
        "status": i.status.as_str(),
        "currency": i.currency,
        "number": i.number,
        "issueDate": i.issue_date.map(iso_date),
        "dueDate": i.due_date.map(iso_date),
        "paymentTermsDays": i.payment_terms_days,
        "overdue": i.is_overdue(today),
        "creditNote": i.is_credit_note,
        "creditsInvoiceId": i.credits_invoice_id.as_ref().map(BillingInvoiceId::as_str),
        "quoteId": i.quote_id.as_ref().map(BillingQuoteId::as_str),
        "reference": i.reference,
        "note": i.note,
        "createdBy": i.created_by,
        "createdAt": iso(i.created_at),
        "updatedAt": iso(i.updated_at),
    })
}

/// A whole document: header, lines in print order, totals, and where it stands
/// against the money received for it.
///
/// `pub(crate)` because accepting a quote answers with the invoice it raised
/// ([`crate::billing_quotes`]), and recording a payment answers with the
/// document the payment changed ([`crate::billing_payments`]); all three must
/// read exactly as this document's own routes do.
pub(crate) fn document_json(d: &InvoiceDocument, today: Date) -> Value {
    with_settlement(
        with_body(invoice_json(&d.invoice, today), &d.lines, &d.totals),
        &d.settlement(),
    )
}

/// A list entry: the header, what it is worth and what is left on it, without
/// the lines.
fn summary_json(s: &InvoiceSummary, today: Date) -> Value {
    with_settlement(
        with_totals(invoice_json(&s.invoice, today), &s.totals),
        &s.settlement(),
    )
}

/// Adds a document's `settlement` to its object — computed from the lines and
/// the payment rows on every read, never stored, so a list entry and the
/// document it summarises can never disagree about what is still owed.
fn with_settlement(mut value: Value, settlement: &Settlement) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.insert("settlement".to_owned(), settlement_json(settlement));
    }
    value
}

/// The stored header as writable input — the base a `PATCH` merges onto.
///
/// The two "take the customer's default" fields are handed over as stated
/// values (`Some`), because on an existing document they were resolved when it
/// was raised: a `PATCH` that does not mention the currency must keep the
/// document's own, never re-read the customer's current one.
fn editable(i: &Invoice) -> NewInvoice {
    NewInvoice {
        customer_id: i.customer_id.clone(),
        currency: Some(i.currency.clone()),
        payment_terms_days: Some(i.payment_terms_days),
        reference: i.reference.clone(),
        note: i.note.clone(),
    }
}

/// The writable parts of a document, every one optional.
///
/// The same body serves `POST` (merged onto a blank header for the named
/// customer) and `PATCH` (merged onto the stored one). Unknown fields are
/// ignored so the contract can grow additively; the response carries the
/// stored document, which is where a caller sees that a misspelled field did
/// nothing.
///
/// There is no `status`, `number`, `issueDate` or `dueDate` here. They are not
/// writable from a request at all: they move only through the lifecycle routes
/// below, and a document whose number a client could set is not a document a
/// tax authority would accept.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InvoiceBody {
    #[serde(default)]
    customer_id: Option<String>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    payment_terms_days: Option<i32>,
    #[serde(default)]
    reference: Option<String>,
    #[serde(default)]
    note: Option<String>,
    /// The whole line set, in print order. Absent leaves the stored lines
    /// alone; `[]` empties the document, which is a legitimate thing to do to
    /// a draft.
    #[serde(default)]
    lines: Option<Vec<LineBody>>,
}

impl InvoiceBody {
    /// Whether the body says anything about the header at all.
    ///
    /// A `PATCH` carrying only `lines` must not touch the header: replaying
    /// the stored header would re-resolve the customer, and a draft whose
    /// customer was archived after it was raised would then refuse to have its
    /// lines edited — a dead end with no way out but deleting the draft.
    fn states_header(&self) -> bool {
        self.customer_id.is_some()
            || self.currency.is_some()
            || self.payment_terms_days.is_some()
            || self.reference.is_some()
            || self.note.is_some()
    }

    /// Merges the stated header fields onto `base`, leaving the rest as they
    /// were.
    fn header(&self, base: NewInvoice) -> NewInvoice {
        NewInvoice {
            customer_id: self
                .customer_id
                .clone()
                .map_or(base.customer_id, BillingCustomerId::new),
            currency: self.currency.clone().or(base.currency),
            payment_terms_days: self.payment_terms_days.or(base.payment_terms_days),
            reference: self.reference.clone().unwrap_or(base.reference),
            note: self.note.clone().unwrap_or(base.note),
        }
    }

    /// The line set the body asks for, if it states one.
    fn lines(self) -> Option<Vec<NewLine>> {
        self.lines.map(LineBody::into_lines)
    }
}

/// Loads one of the tenant's documents, or fails with the `404` an id from
/// another tenant gets.
async fn load(acc: &AccountStore, id: &BillingInvoiceId) -> Result<InvoiceDocument, Problem> {
    acc.billing_invoice(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such invoice"))
}

/// Query string of the list route.
#[derive(Deserialize)]
pub struct ListQuery {
    /// `status=draft|issued|paid|void`; absent lists everything.
    #[serde(default)]
    status: Option<String>,
    /// `overdue=1` narrows the list to what is still owed past its due date —
    /// the collections view. Read with the forgiving [`flag`] (unlike
    /// `status`): it is a view a UI toggles, not a filter whose silent
    /// widening would mislead a bookkeeper about which documents exist.
    #[serde(default)]
    overdue: Option<String>,
}

/// Reads the status filter, refusing a value that is not one of the four.
///
/// A blank value means "no filter" (a UI whose select is on "all" sends an
/// empty parameter), and the comparison is case-insensitive; anything else is
/// a `422` naming what is accepted.
fn status_filter(raw: Option<&str>) -> Result<Option<InvoiceStatus>, Problem> {
    let Some(raw) = raw.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    InvoiceStatus::parse(&raw.to_ascii_lowercase())
        .map(Some)
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "status must be one of draft, issued, paid, void",
            )
        })
}

/// `GET /billing/invoices[?status=issued][&overdue=1]` → `{"invoices":[…]}` —
/// the tenant's documents, newest first, each with its computed totals,
/// settlement and `overdue` flag, but without its lines.
///
/// `overdue=1` is the collections view: issued, past the due date it was
/// stamped with, and not settled — judged against the **server's** date inside
/// the store's own statement, so a browser with a wrong clock cannot clear its
/// own overdue list. It outranks `status`, which would otherwise be able to ask
/// for the overdue drafts (of which there are none, by construction).
pub async fn list_invoices(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    // Parsed even when the overdue view wins, so a misspelled status is still
    // a `422` rather than being silently swallowed by a second parameter.
    let status = status_filter(q.status.as_deref())?;
    let invoices = if flag(q.overdue.as_deref()) {
        account
            .acc
            .billing_overdue_invoices()
            .await
            .map_err(map_store_err)?
    } else {
        account
            .acc
            .billing_invoices(status)
            .await
            .map_err(map_store_err)?
    };
    let today = today();
    Ok(Json(json!({
        "invoices": invoices.iter().map(|s| summary_json(s, today)).collect::<Vec<_>>(),
    })))
}

/// `POST /billing/invoices` `{customerId, lines?, …}` → `{"invoice":{…}}` —
/// raise a **draft**. Only `customerId` is required; the currency and payment
/// terms fall back to the customer's own and are then snapshotted on the
/// document.
///
/// The lines are validated **before** the header is written, so a typo in the
/// last line does not leave an empty draft behind. The two writes are not one
/// transaction (the store exposes them as two operations, so the draft editor
/// can save lines without restating the header); a failure between them leaves
/// exactly a lineless draft, which carries no number, is owed by nobody and is
/// deleted with `DELETE`.
pub async fn create_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: InvoiceBody = parse_body(&body)?;
    // The one check that lives at the edge: which customer a document is
    // raised for is not a field rule the store can own, and letting an absent
    // id fall through would answer "no such customer" (404) to a request that
    // never named one.
    if req
        .customer_id
        .as_ref()
        .is_none_or(|id| id.trim().is_empty())
    {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "customerId is required to raise a document",
        ));
    }
    let header = req.header(NewInvoice::for_customer(BillingCustomerId::new("")));
    let lines = req.lines();
    if let Some(lines) = lines.as_deref() {
        account
            .acc
            .billing_line_totals(lines)
            .map_err(map_store_err)?;
    }
    let id = account
        .acc
        .create_billing_invoice(&header)
        .await
        .map_err(map_store_err)?;
    if let Some(lines) = lines.as_deref() {
        account
            .acc
            .set_billing_invoice_lines(&id, lines)
            .await
            .map_err(map_store_err)?;
    }
    let document = load(&account.acc, &id).await?;
    Ok(Json(
        json!({ "invoice": document_json(&document, today()) }),
    ))
}

/// `GET /billing/invoices/{id}` →
/// `{"invoice":{…},"creditNotes":[…],"payments":[…]}` — the whole document with
/// its lines, totals, settlement, and the two ledgers that explain what is
/// still owed on it.
///
/// `creditNotes` is the ledger of a corrected invoice: what has been raised
/// against this document, drafts included, each with its own (negative)
/// totals. Empty for a document nobody has credited, and for a credit note
/// itself — a credit note is never credited.
///
/// `payments` is the ledger of money received, newest first — the rows the
/// `settlement` on the invoice adds up. Answered here so the document's screen
/// is one read; the same list is also its own route
/// ([`crate::billing_payments`]) for a caller that wants only the ledger.
pub async fn get_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = BillingInvoiceId::new(id);
    let document = load(&account.acc, &id).await?;
    let credits = account
        .acc
        .billing_credit_notes(&id)
        .await
        .map_err(map_store_err)?;
    let payments = account
        .acc
        .billing_payments(&id)
        .await
        .map_err(map_store_err)?;
    let today = today();
    Ok(Json(json!({
        "invoice": document_json(&document, today),
        "creditNotes": credits.iter().map(|s| summary_json(s, today)).collect::<Vec<_>>(),
        "payments": payments.iter().map(crate::billing_payments::payment_json).collect::<Vec<_>>(),
    })))
}

/// `PATCH /billing/invoices/{id}` `{…, lines?}` → `{"invoice":{…}}` — edit a
/// **draft**: merge the stated header fields onto the stored ones, and replace
/// the line set if one is sent.
///
/// A document that is no longer a draft refuses the whole request with a `409`
/// naming its state; that refusal comes from the store, under the row lock the
/// write itself takes, so an edit that raced an issue is refused rather than
/// applied to a numbered document. A body stating only `lines` leaves the
/// header untouched (see [`InvoiceBody::states_header`]).
pub async fn update_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: InvoiceBody = parse_body(&body)?;
    let id = BillingInvoiceId::new(id);
    let stored = load(&account.acc, &id).await?;
    let header = req
        .states_header()
        .then(|| req.header(editable(&stored.invoice)));
    let lines = req.lines();
    // Validated before either write, so a bad line cannot leave a document
    // with its new header and its old lines.
    if let Some(lines) = lines.as_deref() {
        account
            .acc
            .billing_line_totals(lines)
            .map_err(map_store_err)?;
    }
    if let Some(header) = header {
        account
            .acc
            .update_billing_invoice(&id, &header)
            .await
            .map_err(map_store_err)?;
    }
    if let Some(lines) = lines.as_deref() {
        account
            .acc
            .set_billing_invoice_lines(&id, lines)
            .await
            .map_err(map_store_err)?;
    }
    let document = load(&account.acc, &id).await?;
    Ok(Json(
        json!({ "invoice": document_json(&document, today()) }),
    ))
}

/// `DELETE /billing/invoices/{id}` → `{"status":"ok"}` — discard a **draft**
/// and its lines.
///
/// The only document that is ever removed: a draft never consumed a number, so
/// abandoning it leaves no hole in the sequence. An issued one is voided
/// (`409` here), which keeps it readable and keeps the series gapless.
pub async fn delete_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_billing_invoice(&BillingInvoiceId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `POST /billing/invoices/{id}/issue` → `{"invoice":{…}}` — assign the next
/// number from the tenant's gapless series, stamp the issue and due dates, and
/// freeze the document.
///
/// Irreversible, and not idempotent on purpose: re-issuing answers `409` and
/// the document it names already carries its number, so a client that retried
/// after a timeout can read what happened rather than spending a second number.
/// A document with no lines is `422`.
pub async fn issue_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let document = account
        .acc
        .issue_billing_invoice(&BillingInvoiceId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "invoice": document_json(&document, today()) }),
    ))
}

/// `POST /billing/invoices/{id}/void` → `{"invoice":{…}}` — cancel an
/// **issued** document. It keeps its number, its dates and its lines, and
/// stops being owed; nothing is deleted, which is what keeps the series
/// gapless.
///
/// Suitable for a document that never left the building. One the customer
/// already holds is corrected with a credit note instead, so that both
/// parties' copies still reconcile.
pub async fn void_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let document = account
        .acc
        .void_billing_invoice(&BillingInvoiceId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "invoice": document_json(&document, today()) }),
    ))
}

/// `POST /billing/invoices/{id}/credit-note` → `{"invoice":{…}}` — raise a
/// **draft credit note** mirroring an issued (or paid) document: same customer,
/// currency and terms, every line copied with its quantity negated.
///
/// The response is the **new** document, not the original. It is a draft
/// because the mirror is a starting position: a partial credit is made by
/// editing its lines with `PATCH` before issuing it through the ordinary
/// `/issue` route, which draws from the same series.
pub async fn create_credit_note(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = account
        .acc
        .create_billing_credit_note(&BillingInvoiceId::new(id))
        .await
        .map_err(map_store_err)?;
    let document = load(&account.acc, &id).await?;
    Ok(Json(
        json!({ "invoice": document_json(&document, today()) }),
    ))
}

/// `GET /billing/invoices/{id}/print[?lang=]` → the printable document as one
/// self-contained HTML page ([`crate::billing_print`]).
///
/// The same page is the source of the PDF (B1.17) and of the mail attachment
/// (B1.18), which is why it is rendered here and not in the browser
/// (`docs/design/billing.md`).
///
/// What the page says about itself comes from the document's own state, never
/// from the request: a draft prints as a draft and without a number, a void
/// invoice prints as void, and a credit note is titled as one and names the
/// invoice it corrects.
pub async fn print_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<PrintQuery>,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let printable = printable(&account.acc, &BillingInvoiceId::new(id)).await?;
    Ok(print::response(print::render(
        &printable.as_document(),
        query.strings(),
    )))
}

/// `GET /billing/invoices/{id}/pdf[?lang=]` → the same document as a PDF file
/// ([`crate::billing_pdf`]).
///
/// The **same** [`PrintDocument`] the page is rendered from, laid out a second
/// way rather than converted — `docs/design/billing.md` (B1.17) records why we
/// do not run a browser to produce it, and what that costs.
///
/// It is served as an **attachment**, never inline: a PDF rendered inside our
/// own origin is a document context we do not control, and this one exists to
/// be saved, mailed and archived. `Content-Disposition` therefore carries a
/// name built from the document's own heading, reduced to characters that are
/// safe in a header and in a file name on every platform.
pub async fn pdf_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<PrintQuery>,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let printable = printable(&account.acc, &BillingInvoiceId::new(id)).await?;
    let document = printable.as_document();
    let strings = query.strings();
    let bytes = pdf::render(&document, strings, pdf::stamp(OffsetDateTime::now_utc()));
    Ok(pdf::response(bytes, &pdf::file_name(&document, strings)))
}

/// Everything a rendering of one invoice needs, read once.
///
/// Both renderings are built from this — the page by [`print_invoice`], the
/// file by [`pdf_invoice`] — so the paper a customer holds and the file they
/// save cannot disagree about a figure, a date, or what the document is. The
/// covering email ([`crate::billing_send`]) is a third reader of the same
/// value, for the same reason.
pub(crate) struct Printable {
    /// The document itself, with its lines and the store's totals.
    document: InvoiceDocument,
    /// Who it is to, re-read through the account door.
    customer: Customer,
    /// Who it is from: the tenant's own identity, blank if never saved.
    issuer: BillingSettings,
    /// The number of the invoice this one credits, when it credits one.
    credited: Option<String>,
}

/// Loads one of the tenant's invoices and both parties to it, or fails with
/// the `404` an id from another tenant gets.
pub(crate) async fn printable(
    acc: &AccountStore,
    id: &BillingInvoiceId,
) -> Result<Printable, Problem> {
    let document = load(acc, id).await?;
    let (customer, issuer) = print::parties(acc, &document.invoice.customer_id).await?;
    // What this credits is read separately: the store holds the id, and the
    // paper has to name the document the customer already has.
    let credited = match document.invoice.credits_invoice_id.as_ref() {
        Some(original) => acc
            .billing_invoice(original)
            .await
            .map_err(map_store_err)?
            .and_then(|d| d.invoice.number),
        None => None,
    };
    Ok(Printable {
        document,
        customer,
        issuer,
        credited,
    })
}

impl Printable {
    /// Where the document stands, which decides whether it may be sent to the
    /// customer at all ([`crate::billing_send`]). A status rather than the
    /// whole record: nothing outside this module needs the stored row.
    pub(crate) fn status(&self) -> InvoiceStatus {
        self.document.invoice.status
    }

    /// The document as the renderers see it.
    ///
    /// What it says about itself comes from its own stored state, never from
    /// the request: a draft prints as a draft and without a number, a void
    /// invoice prints as void, and a credit note is titled as one.
    pub(crate) fn as_document(&self) -> PrintDocument<'_> {
        let invoice = &self.document.invoice;
        PrintDocument {
            kind: if invoice.is_credit_note {
                DocumentKind::CreditNote
            } else {
                DocumentKind::Invoice
            },
            banner: match invoice.status {
                InvoiceStatus::Draft => Some(Banner::Draft),
                InvoiceStatus::Void => Some(Banner::Void),
                InvoiceStatus::Issued | InvoiceStatus::Paid => None,
            },
            number: invoice.number.as_deref(),
            primary_date: invoice.issue_date,
            secondary_date: invoice.due_date,
            reference: &invoice.reference,
            note: &invoice.note,
            currency: &invoice.currency,
            payment_terms_days: Some(invoice.payment_terms_days),
            credits_number: self.credited.as_deref(),
            customer: &self.customer,
            lines: &self.document.lines,
            totals: &self.document.totals,
            issuer: &self.issuer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: Value) -> InvoiceBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn stored() -> NewInvoice {
        NewInvoice {
            customer_id: BillingCustomerId::new("cust-1"),
            currency: Some("EUR".to_owned()),
            payment_terms_days: Some(14),
            reference: "PO-77".to_owned(),
            note: "Payable within 14 days".to_owned(),
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
        assert_eq!(merged.payment_terms_days, Some(14));
        assert_eq!(merged.reference, "PO-77");
        assert_eq!(merged.note, "Payable within 14 days");
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
        let req = body(json!({ "reference": "PO-88" }));
        assert!(req.states_header());
        let merged = req.header(stored());
        assert_eq!(merged.reference, "PO-88");
        assert_eq!(merged.note, "Payable within 14 days");
        assert_eq!(merged.customer_id.as_str(), "cust-1");
    }

    #[test]
    fn a_blank_reference_or_note_clears_it() {
        // Neither is nullable in the store — they are empty strings — so the
        // ordinary merge already clears them and no null handling is needed.
        let merged = body(json!({ "reference": "", "note": "" })).header(stored());
        assert!(merged.reference.is_empty() && merged.note.is_empty());
    }

    #[test]
    fn create_starts_from_the_customers_own_defaults() {
        // `None` on both means "take the customer's", which is what a UI that
        // has not asked the user must send.
        let merged = body(json!({ "customerId": "cust-2" }))
            .header(NewInvoice::for_customer(BillingCustomerId::new("")));
        assert_eq!(merged.customer_id.as_str(), "cust-2");
        assert!(merged.currency.is_none() && merged.payment_terms_days.is_none());
        assert!(merged.reference.is_empty() && merged.note.is_empty());
    }

    #[test]
    fn a_line_set_reaches_the_store_as_it_was_sent() {
        // The line body itself is `billing_document`'s, and tested there; what
        // matters here is that an invoice body hands the whole set over in
        // order, and that an absent `lines` is not an empty one.
        let lines = body(json!({ "lines": [
            { "description": "Consulting", "unit": "hour", "qtyMilli": 1500,
              "unitPriceCents": 12_500, "vatRateBp": 2100 },
            { "description": "Discount", "qtyMilli": -1000, "unitPriceCents": 5_000 },
        ] }))
        .lines()
        .unwrap_or_else(|| panic!("lines missing"));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].description, "Consulting");
        assert_eq!(lines[1].qty_milli, -1000);
        assert!(body(json!({})).lines().is_none());
    }

    #[test]
    fn money_with_a_decimal_point_is_refused_never_rounded() {
        for bad in [
            json!({ "lines": [{ "description": "X", "unitPriceCents": 19.99 }] }),
            json!({ "paymentTermsDays": "30" }),
        ] {
            assert!(
                serde_json::from_value::<InvoiceBody>(bad.clone()).is_err(),
                "{bad} should have been refused"
            );
        }
    }

    #[test]
    fn the_number_and_the_dates_are_not_writable_fields() {
        // They are ignored like any unknown field: a client cannot set what a
        // tax authority relies on the series to guarantee.
        let req = body(json!({
            "number": "INV-2026-09999", "issueDate": "2020-01-01",
            "dueDate": "2020-01-01", "status": "issued",
        }));
        assert!(!req.states_header(), "none of those is a writable field");
        let merged = req.header(stored());
        assert_eq!(merged.reference, "PO-77");
    }

    #[test]
    fn the_status_filter_accepts_the_four_states_and_nothing_else() {
        assert_eq!(status_filter(None).ok().flatten(), None);
        for blank in ["", "   "] {
            assert_eq!(status_filter(Some(blank)).ok().flatten(), None, "{blank:?}");
        }
        for (raw, expected) in [
            ("draft", InvoiceStatus::Draft),
            ("ISSUED", InvoiceStatus::Issued),
            (" paid ", InvoiceStatus::Paid),
            ("Void", InvoiceStatus::Void),
        ] {
            assert_eq!(
                status_filter(Some(raw)).ok().flatten(),
                Some(expected),
                "{raw:?}"
            );
        }
        for bad in ["sent", "overdue", "all", "draft,issued"] {
            let problem = status_filter(Some(bad))
                .err()
                .unwrap_or_else(|| panic!("{bad:?} should have been refused"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        }
    }
}
