//! Sending an invoice **to the customer** (alo Billing, ADR 0035, wave B1.18) —
//! the step between a document that exists and a document that has been put in
//! front of the person who owes it.
//!
//! `POST /billing/invoices/{id}/send` composes a short covering email to the
//! customer's own address, attaches the invoice PDF ([`crate::billing_pdf`]),
//! and **saves it as a draft** in the user's Drafts folder. It does not send.
//! That is the same rule the agent's draft tools follow (ADR 0034): anything
//! this product writes on a user's behalf lands where they can read it, change
//! it, and send it themselves through the ordinary submission path — which is
//! the one path that signs, records and is audited. A billing route that put
//! mail on the wire would be a second send path, drifting from the audited one,
//! for no gain a review step does not already give.
//!
//! Three things are the server's and not the caller's, each for the same
//! reason — a request must not be able to choose where an invoice goes:
//!
//! - **The recipient** is the customer's stored invoice address. There is no
//!   `to` field on this route; a document is sent to the party it names.
//! - **The author** is the caller's own canonical address
//!   ([`crate::drafts::from_address`]).
//! - **The attachment** is rendered here, now, from the stored document — never
//!   uploaded, never referenced by a client-supplied id.
//!
//! The one thing that *is* the caller's is `?lang=`, which picks the words of
//! both the covering note and the document, exactly as on `/print` and `/pdf`.
//!
//! **Not to be confused with `POST /billing/quotes/{id}/send`**, which is a
//! lifecycle transition (a quote becomes *sent*) and touches no mail. This
//! route writes a draft and changes nothing about the invoice.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use time::OffsetDateTime;

use alo_store::BillingInvoiceId;
use alo_store::billing_invoices::InvoiceStatus;

use crate::billing_invoices::printable;
use crate::billing_pdf as pdf;
use crate::billing_print::PrintQuery;
use crate::document_mail::{self, mail_strings_for};
use crate::drafts;
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// `POST /billing/invoices/{id}/send[?lang=]` →
/// `{"draft":{"id","to","subject","attachment":{"name","sizeBytes"}}}`.
///
/// Writes a draft covering email with the invoice PDF attached into the
/// caller's Drafts. **Nothing is sent**, and the invoice itself is not
/// modified: calling this twice writes two drafts and changes no billing
/// record, which is the behaviour a user who closed the compose window without
/// sending expects.
///
/// The refusals are the ones a document's own state dictates: a **draft**
/// invoice carries no number and prints a DRAFT banner, and a **void** one has
/// been cancelled — neither is a document to put in front of a customer, so
/// both are a `409` naming the state rather than a draft nobody should send.
pub async fn send_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<PrintQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let printable = printable(&account.acc, &BillingInvoiceId::new(id)).await?;
    sendable(printable.status())?;

    let document = printable.as_document();
    let strings = query.strings();
    let words = mail_strings_for(query.lang.as_deref().unwrap_or_default());

    // The author is resolved before anything is rendered: a document with
    // nobody to send it should fail before we spend a PDF on it, and so should
    // one with nowhere to go — which `compose` decides next.
    let from = drafts::from_address(&account, &state).await?;
    let file = pdf::render(&document, strings, pdf::stamp(OffsetDateTime::now_utc()));
    let letter = document_mail::compose(
        &document,
        strings,
        words,
        from,
        pdf::file_name(&document, strings),
        file,
    )?;
    let draft = document_mail::save(&account, &letter).await?;
    Ok(Json(json!({ "draft": draft })))
}

/// Whether a document in this state is one to put in front of a customer.
///
/// Issued and paid both are: a paid invoice is legitimately re-sent as a copy
/// for the customer's own records. A draft and a void one are not, and the
/// refusal says which, because "409" alone leaves a user guessing whether to
/// issue the document or to raise a new one.
fn sendable(status: InvoiceStatus) -> Result<(), Problem> {
    match status {
        InvoiceStatus::Issued | InvoiceStatus::Paid => Ok(()),
        InvoiceStatus::Draft => Err(Problem::with(
            StatusCode::CONFLICT,
            "a draft is not sent to a customer — issue it first",
        )),
        InvoiceStatus::Void => Err(Problem::with(
            StatusCode::CONFLICT,
            "a void invoice is not sent to a customer",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_document_a_customer_should_see_may_be_sent() {
        assert!(sendable(InvoiceStatus::Issued).is_ok());
        assert!(
            sendable(InvoiceStatus::Paid).is_ok(),
            "a copy is legitimate"
        );
        for (status, hint) in [
            (InvoiceStatus::Draft, "issue it first"),
            (InvoiceStatus::Void, "void"),
        ] {
            let problem = sendable(status)
                .err()
                .unwrap_or_else(|| panic!("{status:?} must not be sendable"));
            assert_eq!(problem.status, StatusCode::CONFLICT);
            assert!(
                problem.detail.as_deref().unwrap_or_default().contains(hint),
                "the refusal must say which state: {:?}",
                problem.detail
            );
        }
    }
}
