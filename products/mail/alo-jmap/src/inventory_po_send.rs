//! **Placing** a purchase order over HTTP (alo Inventory, ADR 0035, wave
//! B5.05a2) — `POST /inventory/purchase-orders/{id}/send`.
//!
//! One request, one act: the order draws `PO-YYYY-NNNNN`, is stamped with
//! today, is frozen at `sent`, and the covering letter with the printed order
//! attached is written into the caller's Drafts. `docs/design/inventory.md`
//! records why they are not two routes — a purchase order's *sent* state means
//! precisely "we have asked them", and an order marked sent that nobody ever
//! sent is the state that makes a shortage report lie.
//!
//! The atomicity is the store's ([`alo_store::inv_po_send`]): the letter is
//! written by a callback inside the transaction that numbers the order, so a
//! letter that cannot be written rolls the placement back — number included,
//! which is what the row-locked counter buys us — and the order is still a
//! draft, the honest state.
//!
//! **Nothing is sent.** The letter is a draft in the user's mailbox (ADR 0034,
//! [`crate::document_mail`]), for a person to read, change and send through the
//! one submission path that signs, records and is audited. Its recipient is the
//! supplier's stored address and is **not** a request field: a request must not
//! be able to choose where a purchase order goes.
//!
//! The one thing that *is* the caller's is `?lang=`, which picks the words of
//! both the order and the note it travels in, exactly as on `/print` and
//! `/pdf`.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use time::OffsetDateTime;

use alo_store::InvPurchaseOrderId;
use alo_store::inv_po::PoStatus;

use crate::billing_document::today;
use crate::billing_pdf as pdf;
use crate::billing_print::PrintQuery;
use crate::document_mail::{self, mail_strings_for};
use crate::drafts;
use crate::error::Problem;
use crate::inventory_po::document_json;
use crate::inventory_po_print::{printable, with_parties};
use crate::state::{AppState, authenticate};

/// `POST /inventory/purchase-orders/{id}/send[?lang=]` →
/// `{"purchaseOrder":{…},"draft":{"id","to","subject","attachment":{…}}}`.
///
/// Places a **draft** order with its supplier. The response carries both halves
/// of the one act: the order as it now stands — numbered, dated, `sent` — and
/// what was written to the mailbox, so a UI can say "PO-2026-00001 is in your
/// Drafts" without a second request.
///
/// Its refusals, in the order a caller meets them:
///
/// - the order is **not this tenant's** → `404`, indistinguishable from absent;
/// - it is **not a draft** → `409` naming its state, so one document can never
///   draw two numbers;
/// - the **supplier has no usable email address** → `422` naming the supplier,
///   because a letter with nowhere to go is the whole point of the act;
/// - the caller's account has **no send address** → `422`; a draft with no
///   `From` is not a message anyone can send;
/// - the order has **no lines** → `422`: an order that asks for nothing would
///   have the supplier telephoning.
///
/// The first four are decided *before* the number is drawn. The fifth is the
/// store's, under the same lock.
pub async fn send_purchase_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<PrintQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = InvPurchaseOrderId::new(id);

    // The pre-flight: everything that can be known before the order is
    // numbered is checked before it is, so the ordinary mistakes — a frozen
    // order, a supplier with no address, an account that cannot send — never
    // reach the placing transaction at all.
    let stored = printable(&account.acc, &id).await?;
    sendable(stored.status())?;
    document_mail::recipient(&stored.as_document())?;
    let from = drafts::from_address(&account, &state).await?;

    let strings = query.strings();
    let words = mail_strings_for(query.lang.as_deref().unwrap_or_default());

    // The placement. Everything inside the callback happens with the number
    // drawn and the row locked; anything it returns as an error gives the
    // number back and leaves the order a draft.
    // A shared borrow for the callback: the placing call borrows `account.acc`
    // as its receiver, and the letter needs the same account's mailbox. Both are
    // reads of one value, which is exactly what a shared borrow is for.
    let writer = &account;
    let (order, draft) = account
        .acc
        .send_inv_purchase_order::<Value, Problem, _, _>(&id, |placed| async move {
            let printable = with_parties(&writer.acc, placed).await?;
            let document = printable.as_document();
            let file = pdf::render(&document, strings, pdf::stamp(OffsetDateTime::now_utc()));
            let letter = document_mail::compose(
                &document,
                strings,
                words,
                from,
                pdf::file_name(&document, strings),
                file,
            )?;
            document_mail::save(writer, &letter).await
        })
        .await?;

    Ok(Json(json!({
        "purchaseOrder": document_json(&order, today()),
        "draft": draft,
    })))
}

/// Whether an order in this state is one to place with a supplier.
///
/// Only a draft is. The store's transition table decides the same thing under
/// the row lock and is the authority; this is the same refusal reached one step
/// earlier, so a caller is told what is wrong before a PDF is rendered and an
/// address resolved. The message names the state and, for the two that are
/// already out, says what to do instead — "409" alone leaves a buyer guessing
/// whether to re-send or to raise another order.
fn sendable(status: PoStatus) -> Result<(), Problem> {
    match status {
        PoStatus::Draft => Ok(()),
        PoStatus::Sent => Err(Problem::with(
            StatusCode::CONFLICT,
            "this order has already been sent; it keeps the number the supplier holds — \
             raise another order rather than sending this one twice",
        )),
        PoStatus::PartiallyReceived | PoStatus::Received => Err(Problem::with(
            StatusCode::CONFLICT,
            "goods have already arrived against this order, so it cannot be sent again",
        )),
        PoStatus::Cancelled => Err(Problem::with(
            StatusCode::CONFLICT,
            "this order was cancelled; raise another one to order the goods again",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_draft_may_be_placed_and_the_refusal_says_what_to_do_instead() {
        assert!(sendable(PoStatus::Draft).is_ok());
        for (status, hint) in [
            (PoStatus::Sent, "already been sent"),
            (PoStatus::PartiallyReceived, "already arrived"),
            (PoStatus::Received, "already arrived"),
            (PoStatus::Cancelled, "cancelled"),
        ] {
            let problem = sendable(status)
                .err()
                .unwrap_or_else(|| panic!("{status:?} must not be sendable"));
            assert_eq!(problem.status, StatusCode::CONFLICT);
            let detail = problem.detail.unwrap_or_default();
            assert!(detail.contains(hint), "{status:?}: {detail}");
            // Every refusal points somewhere: a buyer who cannot send this
            // order is told that another order is the answer.
            assert!(
                detail.contains("order"),
                "{status:?} must say what to do instead: {detail}"
            );
        }
    }
}
