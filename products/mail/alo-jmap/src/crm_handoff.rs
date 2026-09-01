//! The won-deal handoff HTTP surface (alo CRM, ADR 0035, wave B2.08) — the two
//! `POST`s that turn an opportunity into a **draft** billing document, over
//! [`alo_store::crm_handoff`].
//!
//! Both routes answer the created document **and the deal**, because raising a
//! document can change the deal: a lead with no customer row gets one, and it
//! is written back onto the card. A caller that re-renders from the answer
//! therefore never has to guess whether it now has a customer, and never has to
//! re-read to find out.
//!
//! The rules live in the store and are not duplicated here — a lost deal
//! raises nothing, a priced deal needs the VAT rate its line is billed at, a
//! lead needs a company name and a country. What this edge owns is the shape of
//! the request: `vatRateBp` is a JSON integer in **basis points** (2100 = 21 %)
//! like every rate in billing, and `country` is the two-letter code, so a
//! client that sends `21` gets a document at 0.21 % refused by the same
//! validator every other rate meets rather than a special rule invented here.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::DealHandoff;
use alo_store::{AccountStore, CrmDealId};

use crate::billing::{iso, map_store_err, parse_body};
use crate::billing_document::today;
use crate::billing_invoices::document_json as invoice_json;
use crate::billing_quotes::document_json as quote_json;
use crate::crm_deals::deal_json;
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// What a caller adds to a deal to make it a document.
///
/// Both fields are optional in the request and the *store* decides when each is
/// required — a deal worth nothing needs no rate, and a deal that already names
/// a customer needs no country. Stating one that is not needed is not an error:
/// it is a client that filled its whole form.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct HandoffBody {
    /// VAT rate in basis points for the single line raised from the deal's
    /// value.
    #[serde(default)]
    vat_rate_bp: Option<i32>,
    /// ISO 3166-1 alpha-2 country of the customer created from a lead.
    #[serde(default)]
    country: Option<String>,
}

impl HandoffBody {
    /// The store's shape. An absent body is a legitimate request — for a deal
    /// that is worth nothing and already has a customer, there is nothing to
    /// state — so an empty one is the default rather than a `400`.
    fn read(body: &axum::body::Bytes) -> Result<DealHandoff, Problem> {
        let req: Self = if body.is_empty() {
            Self::default()
        } else {
            parse_body(body)?
        };
        Ok(DealHandoff {
            vat_rate_bp: req.vat_rate_bp,
            country: req.country.unwrap_or_default(),
        })
    }
}

/// Reads the deal back after the write, so the answer carries the card as it
/// now stands — including the customer a lead just became.
async fn stored_deal(acc: &AccountStore, id: &CrmDealId) -> Result<Value, Problem> {
    let deal = acc
        .crm_deal(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such deal"))?;
    Ok(deal_json(&deal))
}

/// `GET /crm/deals/{id}/documents` returns only Billing documents explicitly
/// raised from this opportunity; sharing a customer is not provenance.
pub async fn deal_documents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let documents = account
        .acc
        .crm_deal_billing_documents(&CrmDealId::new(id))
        .await
        .map_err(map_store_err)?
        .into_iter()
        .map(|document| {
            json!({
                "kind": document.kind,
                "documentId": document.document_id,
                "status": document.status,
                "number": document.number,
                "createdAt": iso(document.created_at),
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({ "documents": documents })))
}

/// `POST /crm/deals/{id}/quote` `{vatRateBp?, country?}` →
/// `{"quote":{…},"deal":{…}}` — raise a **draft** offer for the deal.
///
/// Quoting an open deal is ordinary sales, so this is not restricted to won
/// deals; only a deal recorded as lost is refused. The offer is a draft like
/// any other: it carries no number, and sending it is billing's own route.
pub async fn deal_quote(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let handoff = HandoffBody::read(&body)?;
    let id = CrmDealId::new(id);
    let quote = account
        .acc
        .crm_deal_quote(&id, &handoff)
        .await
        .map_err(map_store_err)?;
    let document = account
        .acc
        .billing_quote(&quote)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| {
            Problem::with(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the quote could not be read back",
            )
        })?;
    Ok(Json(json!({
        "quote": quote_json(&document, today()),
        "deal": stored_deal(&account.acc, &id).await?,
    })))
}

/// `POST /crm/deals/{id}/invoice` `{vatRateBp?, country?}` →
/// `{"invoice":{…},"deal":{…}}` — raise a **draft** invoice for the deal.
///
/// A draft, always: it consumes nothing from the tenant's gapless sequence and
/// carries no number until somebody issues it through billing's own route,
/// which is the one place a number is ever assigned.
pub async fn deal_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let handoff = HandoffBody::read(&body)?;
    let id = CrmDealId::new(id);
    let invoice = account
        .acc
        .crm_deal_invoice(&id, &handoff)
        .await
        .map_err(map_store_err)?;
    let document = account
        .acc
        .billing_invoice(&invoice)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| {
            Problem::with(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the invoice could not be read back",
            )
        })?;
    Ok(Json(json!({
        "invoice": invoice_json(&document, today()),
        "deal": stored_deal(&account.acc, &id).await?,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Bytes;

    fn read(raw: &str) -> DealHandoff {
        HandoffBody::read(&Bytes::from(raw.to_owned()))
            .unwrap_or_else(|e| panic!("rejected: {:?}", e.detail))
    }

    #[test]
    fn a_full_form_reaches_the_store_unchanged() {
        let handoff = read(r#"{"vatRateBp":2100,"country":"de"}"#);
        assert_eq!(handoff.vat_rate_bp, Some(2100));
        // Not upper-cased here: the store owns the one country rule, so the
        // edge cannot disagree with it about what "de" means.
        assert_eq!(handoff.country, "de");
    }

    #[test]
    fn an_absent_body_is_a_request_not_a_refusal() {
        for empty in ["", "{}"] {
            let handoff = read(empty);
            assert_eq!(handoff.vat_rate_bp, None, "{empty:?}");
            assert_eq!(handoff.country, "", "{empty:?}");
        }
    }

    #[test]
    fn a_rate_with_a_decimal_point_is_refused_never_rounded() {
        for bad in [r#"{"vatRateBp":21.5}"#, r#"{"vatRateBp":"2100"}"#] {
            assert!(
                HandoffBody::read(&Bytes::from(bad.to_owned())).is_err(),
                "accepted {bad}"
            );
        }
    }

    #[test]
    fn an_unknown_field_is_ignored_so_the_contract_can_grow() {
        let handoff = read(r#"{"vatRateBp":900,"reference":"PO-9","lines":[]}"#);
        assert_eq!(handoff.vat_rate_bp, Some(900));
    }
}
