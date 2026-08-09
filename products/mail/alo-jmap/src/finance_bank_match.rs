//! Reconciliation over HTTP (alo Finance, ADR 0035, wave B4.09c) — the read a
//! reconciliation screen is drawn from, and the four verbs a person has on a
//! staged line.
//!
//! - `GET /finance/bank/suggestions` — every unmatched line with what the two
//!   guessing stages think it is. A read: it writes nothing, posts nothing, and
//!   is worth exactly as much as the person looking at it (ADR 0023).
//! - `POST /finance/bank/lines/{id}/match` — *this line settled that document*.
//!   The one call in this file that moves money.
//! - `POST /finance/bank/lines/{id}/unmatch` — take it back: the payment goes,
//!   the entry is reversed, the line returns to the pile.
//! - `POST /finance/bank/lines/{id}/ignore` · `/unignore` — *this line is not
//!   ours to book*, with the reason, and the undo of that.
//!
//! Three things this edge owns, and nothing else — every rule about what a
//! legitimate match is lives in the store, so a second caller can never get a
//! weaker definition of one.
//!
//! - **The client states the amount, and it is compared rather than trusted.**
//!   `amountCents` is what the person saw attributed on the screen they clicked;
//!   the store checks it against what the bank said the line moves, under the
//!   row locks, so a stale screen is a `422` instead of a payment for the wrong
//!   money.
//! - **A suggestion sent back is not evidence.** `ruleId` is recorded and its
//!   hit counted, but the *rule of the match* is re-derived on the server from
//!   the line and the document as they are then. A client cannot talk us into a
//!   match by describing one.
//! - **The verbs are `POST`s on the line, not a `PATCH` of its status.** A
//!   status a client can set is a status a client can set to anything; these are
//!   four named acts with four different sets of consequences, and the audit log
//!   records them by name (B2.13).
//!
//! Nothing here logs a counterparty, a remittance or an amount: a bank line is
//! the tenant's own money moving (Law 1).

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{
    BankLineId, BankStatementId, BankSuggestions, BillingInvoiceId, ConfirmedMatch, ExactMatch,
    FinMatchRuleId, LikelyMatch, LineSuggestions, MatchEvidence, UnmatchedLine,
};

use crate::billing::{map_store_err, parse_body};
use crate::error::Problem;
use crate::finance_bank::{line_json, stated};
use crate::state::{AppState, authenticate};

/// One exact match as JSON — the certain suggestion.
fn exact_json(matched: &ExactMatch) -> Value {
    json!({
        "invoiceId": matched.invoice_id.as_str(),
        "number": matched.number,
        "amountCents": matched.amount_cents,
        "daysAfterIssue": matched.days_after_issue,
    })
}

/// One piece of evidence: a **token and its numbers**, never a sentence.
///
/// The screen writes the sentence, in the reader's own language — "the payer
/// quoted this number but paid part of it" is French for a French tenant, and a
/// string built here could only ever be English (Law: user-facing strings are
/// externalised from day one). What the wire carries is the datum the sentence
/// is built from.
fn evidence_json(evidence: &MatchEvidence) -> Value {
    match evidence {
        MatchEvidence::NumberQuoted => json!({ "kind": "numberQuoted" }),
        MatchEvidence::RuleSaved { rule_id, match_on } => json!({
            "kind": "ruleSaved",
            "ruleId": rule_id.as_str(),
            "matchOn": match_on.as_str(),
        }),
        MatchEvidence::CustomerNamed { similarity_bp } => json!({
            "kind": "customerNamed",
            "similarityBp": similarity_bp,
        }),
        MatchEvidence::WholeAmount => json!({ "kind": "wholeAmount" }),
        MatchEvidence::OnlyDocumentForTheAmount => json!({ "kind": "onlyDocumentForTheAmount" }),
        MatchEvidence::NearDue { days } => json!({ "kind": "nearDue", "days": days }),
        MatchEvidence::PartPayment { remaining_cents } => json!({
            "kind": "partPayment",
            "remainingCents": remaining_cents,
        }),
    }
}

/// One ranked guess as JSON.
fn likely_json(likely: &LikelyMatch) -> Value {
    json!({
        "invoiceId": likely.invoice_id.as_str(),
        "number": likely.number,
        "amountCents": likely.amount_cents,
        "outstandingCents": likely.outstanding_cents,
        "customerId": likely.customer_id.as_str(),
        "daysAfterIssue": likely.days_after_issue,
        "score": likely.score,
        "evidence": likely.evidence.iter().map(evidence_json).collect::<Vec<_>>(),
        "ruleId": likely.rule_id.as_ref().map(FinMatchRuleId::as_str),
    })
}

/// One line with what it might be.
fn suggestions_json(line: &LineSuggestions) -> Value {
    json!({
        "line": line_json(&line.line),
        "exact": line.exact.iter().map(exact_json).collect::<Vec<_>>(),
        "likely": line.likely.iter().map(likely_json).collect::<Vec<_>>(),
    })
}

/// The whole read, caps included.
fn read_json(suggestions: &BankSuggestions) -> Value {
    json!({
        "lines": suggestions.lines.iter().map(suggestions_json).collect::<Vec<_>>(),
        // Never silent: a screen that shows a short list has to be able to say
        // it is short, or a bookkeeper concludes there is nothing to match.
        "numbersCapped": suggestions.numbers_capped,
        "ledgerCapped": suggestions.ledger_capped,
    })
}

/// What a settlement did, as the client gets it back.
fn confirmed_json(confirmed: &ConfirmedMatch) -> Value {
    let matched = &confirmed.matched;
    json!({
        "id": matched.id.as_str(),
        "lineId": matched.line_id.as_str(),
        "targetKind": matched.target.kind(),
        "targetId": matched.target.id(),
        "amountCents": matched.amount_cents,
        "paymentId": matched.payment_id.as_ref().map(alo_store::BillingPaymentId::as_str),
        "entryId": matched.entry_id.as_ref().map(alo_store::FinEntryId::as_str),
        "ruleId": matched.rule_id,
        "confirmedBy": matched.confirmed_by.as_str(),
        "confirmedAt": crate::billing::iso(matched.confirmed_at),
        "invoiceEntryId": confirmed.invoice_entry_id.as_str(),
        // The first entries a tenant ever sees appear because of this act, and
        // a bookkeeper deserves to be told which act created them.
        "invoiceBookedNow": confirmed.invoice_booked_now,
    })
}

/// What taking it back did.
fn unmatched_json(unmatched: &UnmatchedLine) -> Value {
    json!({
        "lineId": unmatched.line_id.as_str(),
        "targetKind": unmatched.target.kind(),
        "targetId": unmatched.target.id(),
        "amountCents": unmatched.amount_cents,
        // Not a deletion: the correction is an entry of its own, and this is it.
        "reversalEntryId": unmatched.reversal_entry_id.as_str(),
    })
}

/// What a suggestions read may be narrowed by.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionsQuery {
    /// One import. Absent reads every unmatched line the tenant holds.
    #[serde(default)]
    statement: Option<String>,
}

/// `GET /finance/bank/suggestions?statement=` → `{"suggestions":{…}}`.
///
/// An unknown statement narrows to nothing and answers an empty list, like every
/// other narrowing in this service — answering otherwise would make the filter
/// an oracle for another tenant's ids.
pub async fn list_bank_suggestions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SuggestionsQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let statement = stated(query.statement).map(BankStatementId::new);
    let suggestions = account
        .acc
        .bank_match_suggestions(statement.as_ref())
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "suggestions": read_json(&suggestions) })))
}

/// The pick a person makes.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchBody {
    /// The document this line settled.
    invoice_id: String,
    /// What of the line is attributed to it, in integer cents — what the person
    /// saw on their screen, compared with what the bank said.
    amount_cents: i64,
    /// The learned rule whose suggestion they took, when they took one.
    #[serde(default)]
    rule_id: Option<String>,
}

/// `POST /finance/bank/lines/{id}/match` → `{"match":{…}}` — records the
/// payment, moves the books, marks the line matched.
///
/// `404` when the line, the invoice or the rule is not this tenant's, `409`
/// when the line or the document is in no state to be matched, `422` when the
/// pick is not a legitimate one (more than is owed, the wrong currency, money
/// going the other way, an amount that is not what the line moves).
pub async fn match_bank_line(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(line_id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let input: MatchBody = parse_body(&body)?;
    let invoice_id = stated(Some(input.invoice_id)).ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "say which invoice this bank line settled",
        )
    })?;
    let confirmed = account
        .acc
        .match_bank_line(
            &BankLineId::new(line_id),
            &BillingInvoiceId::new(invoice_id),
            input.amount_cents,
            stated(input.rule_id).map(FinMatchRuleId::new).as_ref(),
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "match": confirmed_json(&confirmed) })))
}

/// `POST /finance/bank/lines/{id}/unmatch` → `{"unmatched":{…}}`.
///
/// `404` when the line is not this tenant's or carries no match, `409` when it
/// is not matched any more or a later payment on the same document has to be
/// taken back first.
pub async fn unmatch_bank_line(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(line_id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let unmatched = account
        .acc
        .unmatch_bank_line(&BankLineId::new(line_id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "unmatched": unmatched_json(&unmatched) })))
}

/// Why a line is not ours to book.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IgnoreBody {
    /// The sentence that goes with the decision. Required: see
    /// [`alo_store::bank_ignore`].
    reason: String,
}

/// `POST /finance/bank/lines/{id}/ignore` → `{"line":{…}}` — the line, as it now
/// stands.
///
/// `422` with no reason, `409` when the line is matched to a document (take that
/// back first), `404` when it is not this tenant's.
pub async fn ignore_bank_line(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(line_id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let input: IgnoreBody = parse_body(&body)?;
    let line = account
        .acc
        .ignore_bank_line(&BankLineId::new(line_id), &input.reason)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "line": line_json(&line) })))
}

/// `POST /finance/bank/lines/{id}/unignore` → `{"line":{…}}` — back in the pile,
/// with the reason cleared.
pub async fn unignore_bank_line(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(line_id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let line = account
        .acc
        .unignore_bank_line(&BankLineId::new(line_id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "line": line_json(&line) })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use alo_store::MatchOn;

    #[test]
    fn evidence_travels_as_a_token_and_its_numbers_never_as_a_sentence() {
        let rendered = evidence_json(&MatchEvidence::RuleSaved {
            rule_id: FinMatchRuleId::new("rule-1".to_owned()),
            match_on: MatchOn::Iban,
        });
        assert_eq!(rendered["kind"], "ruleSaved");
        assert_eq!(rendered["ruleId"], "rule-1");
        assert_eq!(rendered["matchOn"], "iban");

        assert_eq!(
            evidence_json(&MatchEvidence::PartPayment {
                remaining_cents: 80_700
            }),
            json!({ "kind": "partPayment", "remainingCents": 80_700 })
        );
        assert_eq!(
            evidence_json(&MatchEvidence::NearDue { days: -3 }),
            json!({ "kind": "nearDue", "days": -3 })
        );
        assert_eq!(
            evidence_json(&MatchEvidence::WholeAmount),
            json!({ "kind": "wholeAmount" })
        );
    }

    #[test]
    fn a_match_body_reads_the_three_fields_and_defaults_the_rule() {
        let body: MatchBody =
            serde_json::from_value(json!({ "invoiceId": "inv-1", "amountCents": 130_700 }))
                .expect("a readable pick");
        assert_eq!(body.invoice_id, "inv-1");
        assert_eq!(body.amount_cents, 130_700);
        assert_eq!(
            body.rule_id, None,
            "taking no suggestion is the common case"
        );

        let with_rule: MatchBody = serde_json::from_value(json!({
            "invoiceId": "inv-1",
            "amountCents": 1,
            "ruleId": "  ",
        }))
        .expect("a readable pick");
        assert!(
            stated(with_rule.rule_id).is_none(),
            "a blank rule id is an unstated one, never a lookup for ''"
        );
    }
}
