//! Payments HTTP surface (alo Billing, ADR 0035, wave B1.19) — the money
//! received against an invoice, over [`alo_store::billing_payments`].
//!
//! The routes hang **under the invoice** (`/billing/invoices/{id}/payments`)
//! rather than at `/billing/payments`, because a payment does not exist on its
//! own: it settles one document, and addressing it through that document is
//! what makes an id from another invoice — or another tenant — a plain `404`
//! instead of a write landing somewhere unexpected. (`docs/design/billing.md`
//! records the flat shape this replaced, and why.)
//!
//! It shares the conventions of [`crate::billing_invoices`] — authenticated and
//! tenant-scoped through the account door, no validation duplicated from the
//! store, every write answered with the stored record — and adds two of its
//! own.
//!
//! - **The settlement is computed, never sent.** `paidCents`,
//!   `outstandingCents` and `paymentState` are derived on every read from the
//!   document's lines and its payment rows; there is no writable total and no
//!   writable state anywhere here, so no request can make a document look
//!   settled without money to show for it.
//! - **Recording money answers with the invoice too.** The document's status is
//!   a projection of this ledger, so a caller that has just posted the last
//!   instalment learns in the same response that the invoice is now `paid` —
//!   without a second round trip that could read a different moment.
//!
//! There is no `PATCH`. A payment is a fact that happened: a mis-keyed one is
//! removed and re-entered (`DELETE`), so the ledger reads as a list of
//! movements rather than a list of movements as last edited.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::billing_payments::{Payment, Settlement};
use alo_store::{BillingInvoiceId, BillingPaymentId, NewPayment};

use crate::billing::{iso, iso_date, map_store_err, parse_body, parse_iso_date};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// One recorded payment as JSON.
///
/// `pub(crate)` because a document's own read answers its ledger too
/// ([`crate::billing_invoices::get_invoice`]), and both must report a payment
/// in one shape.
pub(crate) fn payment_json(p: &Payment) -> Value {
    json!({
        "id": p.id.as_str(),
        "invoiceId": p.invoice_id.as_str(),
        "paidOn": iso_date(p.paid_on),
        "amountCents": p.amount_cents,
        "method": p.method,
        "reference": p.reference,
        "createdBy": p.created_by,
        "createdAt": iso(p.created_at),
    })
}

/// Where a document stands against the money that has arrived for it.
///
/// `pub(crate)` because every invoice response carries it — the list entry, the
/// document, and the answer to recording a payment — and all three must report
/// it in one shape.
///
/// `outstandingCents` is **negative when the customer overpaid**, deliberately:
/// the figure a bookkeeper needs is what is actually left, including the
/// direction.
pub(crate) fn settlement_json(s: &Settlement) -> Value {
    json!({
        "grossCents": s.gross_cents,
        "paidCents": s.paid_cents,
        "outstandingCents": s.outstanding_cents,
        "state": s.state.as_str(),
    })
}

/// A payment as sent by a client.
///
/// Every field is optional and defaults to the blank payment, so the store owns
/// what "valid" means — an absent amount is zero and comes back as the store's
/// own `422`, in the same words the billing agent (B1.25) will get when it
/// calls the store directly. There is no `invoiceId`: the document is the path.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaymentBody {
    /// `YYYY-MM-DD`, the day the money arrived as the bank states it. Absent
    /// means today according to the **server**, which is the only date a
    /// caller that has not asked the user should imply.
    #[serde(default)]
    paid_on: Option<String>,
    #[serde(default)]
    amount_cents: i64,
    #[serde(default)]
    method: String,
    #[serde(default)]
    reference: String,
}

impl PaymentBody {
    /// The writable payment this body asks for.
    ///
    /// The date is the one field parsed here rather than in the store: it
    /// arrives as text and a malformed day is a request the store would never
    /// see a value for. A blank string is the same as absent — a form that
    /// clears its date box sends `""` and means "today".
    fn into_payment(self) -> Result<NewPayment, Problem> {
        let paid_on = match self
            .paid_on
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
        {
            Some(raw) => Some(parse_iso_date(raw).ok_or_else(|| {
                Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "paidOn must be a date of the form YYYY-MM-DD",
                )
            })?),
            None => None,
        };
        Ok(NewPayment {
            paid_on,
            amount_cents: self.amount_cents,
            method: self.method,
            reference: self.reference,
        })
    }
}

/// `GET /billing/invoices/{id}/payments` →
/// `{"payments":[…],"settlement":{…}}` — the payment ledger of one document,
/// newest first, and what it adds up to.
///
/// An invoice that is absent or another tenant's is a `404`, from the document
/// read rather than from the ledger: a list read is never an existence oracle.
pub async fn list_payments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = BillingInvoiceId::new(id);
    let document = account
        .acc
        .billing_invoice(&id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such invoice"))?;
    let payments = account
        .acc
        .billing_payments(&id)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "payments": payments.iter().map(payment_json).collect::<Vec<_>>(),
        "settlement": settlement_json(&document.settlement()),
    })))
}

/// `POST /billing/invoices/{id}/payments` `{amountCents, paidOn?, method?,
/// reference?}` → `{"payment":{…},"invoice":{…}}` — record money received.
///
/// The invoice in the response is the document **after** the ledger changed, so
/// the caller sees the status the payment projected (`paid` once the whole
/// gross has arrived) without a second read.
///
/// The store refuses a document that cannot carry money — a draft, a void one,
/// a credit note — with a `409` naming which case it is, and an amount that is
/// not positive, or a date in the future, with a `422`.
pub async fn create_payment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let input: PaymentBody = parse_body(&body)?;
    let input = input.into_payment()?;
    let id = BillingInvoiceId::new(id);
    let body = crate::billing_intents::record_payment(&account, &id, &input).await?;
    Ok(Json(body))
}

/// `DELETE /billing/invoices/{id}/payments/{paymentId}` →
/// `{"status":"ok","invoice":{…}}` — remove a payment recorded wrongly.
///
/// The correction path, and the only one: a mis-keyed amount is removed and
/// re-entered, never patched. A document that was settled by it goes back to
/// `issued` — and becomes overdue again if its date has passed, which is the
/// honest answer, since the money is not there.
pub async fn delete_payment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, payment_id)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = BillingInvoiceId::new(id);
    let invoice =
        crate::billing_intents::remove_payment(&account, &id, &BillingPaymentId::new(payment_id))
            .await?;
    Ok(Json(json!({ "status": "ok", "invoice": invoice })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::billing_payments::PaymentState;
    use time::Date;

    fn body(json: Value) -> PaymentBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    #[test]
    fn a_body_without_a_date_means_today_and_the_store_decides_the_rest() {
        let payment = body(json!({ "amountCents": 121_000 }))
            .into_payment()
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(payment.paid_on, None, "None means the database's today");
        assert_eq!(payment.amount_cents, 121_000);
        assert!(payment.method.is_empty() && payment.reference.is_empty());
    }

    #[test]
    fn a_blank_date_is_the_same_as_an_absent_one() {
        // A form that clears its date box sends `""` and means "today".
        for blank in [json!({ "paidOn": "" }), json!({ "paidOn": "  " })] {
            let payment = body(blank.clone())
                .into_payment()
                .unwrap_or_else(|e| panic!("{blank} → {e:?}"));
            assert_eq!(payment.paid_on, None, "{blank}");
        }
    }

    #[test]
    fn a_stated_date_is_read_as_a_plain_day() {
        let payment = body(json!({ "paidOn": "2026-08-07", "amountCents": 1 }))
            .into_payment()
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(
            payment.paid_on,
            Some(
                Date::from_calendar_date(2026, time::Month::August, 7)
                    .unwrap_or_else(|e| panic!("{e}"))
            )
        );
    }

    #[test]
    fn a_date_that_is_not_a_date_is_refused_never_guessed_at() {
        for bad in [
            "07/08/2026",
            "2026-13-01",
            "yesterday",
            "2026-08-07T10:00:00Z",
        ] {
            let problem = body(json!({ "paidOn": bad, "amountCents": 1 }))
                .into_payment()
                .err()
                .unwrap_or_else(|| panic!("{bad:?} should have been refused"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad:?}");
        }
    }

    #[test]
    fn money_with_a_decimal_point_is_refused_never_rounded() {
        for bad in [
            json!({ "amountCents": 19.99 }),
            json!({ "amountCents": "121000" }),
        ] {
            assert!(
                serde_json::from_value::<PaymentBody>(bad.clone()).is_err(),
                "{bad} should have been refused"
            );
        }
    }

    #[test]
    fn the_invoice_id_is_the_path_and_never_a_field() {
        // Sent as a field it is ignored like any unknown one, so a request can
        // never attach money to a document other than the one it addressed.
        let payment = body(json!({ "invoiceId": "somebody-elses", "amountCents": 500 }))
            .into_payment()
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(payment.amount_cents, 500);
    }

    #[test]
    fn a_settlement_reports_the_four_figures_a_bookkeeper_reads() {
        let value = settlement_json(&Settlement::of(121_000, 50_000));
        assert_eq!(value["grossCents"], 121_000);
        assert_eq!(value["paidCents"], 50_000);
        assert_eq!(value["outstandingCents"], 71_000);
        assert_eq!(value["state"], PaymentState::PartiallyPaid.as_str());
        // Overpaid: settled, and what is left is negative.
        let over = settlement_json(&Settlement::of(121_000, 130_000));
        assert_eq!(over["outstandingCents"], -9_000);
        assert_eq!(over["state"], "paid");
    }
}
