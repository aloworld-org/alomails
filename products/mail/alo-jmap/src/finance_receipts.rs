//! Reading a receipt over HTTP (alo Finance, ADR 0035, wave B4.06b) — one
//! route, over [`alo_store::fin_receipt_read`].
//!
//! `POST /finance/receipts` `{"nodeId": "…"}` → the candidate fields of a
//! receipt already in the caller's Drive, **for a human to confirm**. It writes
//! nothing at all: no expense, no draft, no row. The claim is created afterwards
//! by the ordinary `POST /finance/expenses` with whatever the person actually
//! agreed to, and the same node id attached — which is why this file has one
//! function and [`crate::finance_expenses`] needed no new one.
//!
//! Three things about the shape, each decided in `docs/design/finance.md` rather
//! than here:
//!
//! - **The file arrives as a Drive node, not as bytes.** A receipt is evidence
//!   the books keep pointing at, so it is uploaded the way every other
//!   attachment in the product is (`POST /jmap/upload` then `POST /drive/files`)
//!   and lives under the claimant's own node. Finance is given the id. A second
//!   upload door here would be a second answer to where a person's files live.
//! - **It is a `POST` that is a read**, like `/crm/imports/leads/preview`: it
//!   joins `audit_action::READ_ONLY_POSTS`, because an audit entry claiming
//!   somebody *created* something they only looked at is a false line in a log
//!   an auditor will read.
//! - **Nothing is decided and nothing is computed.** Every field comes back
//!   optional, with a coarse confidence and the characters it was read from, and
//!   the VAT is what the paper printed — never the gross times a rate. The
//!   deliberate consequence is that a receipt showing a rate and no tax yields a
//!   rate and no tax amount.
//!
//! The response carries the receipt's own lines so the form can highlight the
//! evidence. They are somebody's restaurant, pharmacy and city on a date:
//! nothing in this module logs them, and they go to the person who uploaded the
//! file and to nobody else.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::fin_receipt::{Confidence, Evidence, Found, ParsedReceipt};
use alo_store::{DriveNodeId, ReceiptReading};

use crate::billing::{iso_date, map_store_err, parse_body};
use crate::billing_document::today;
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// Which file to read.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReceiptBody {
    /// The Drive node the caller has just uploaded, or an older file of theirs.
    #[serde(default)]
    node_id: Option<String>,
}

/// How sure the extractor is, as one of three words.
///
/// Deliberately not a number: a percentage invites a threshold, and a threshold
/// invites skipping the confirmation above it. It exists to order the person's
/// attention, not to let a client act without them.
fn confidence_str(confidence: Confidence) -> &'static str {
    match confidence {
        Confidence::High => "high",
        Confidence::Medium => "medium",
        Confidence::Low => "low",
    }
}

/// Where a field was read from, so the form can show the person *why*.
///
/// A character range into the `lines` of the same response, or the file's own
/// name — which is the honest answer for a photograph, where nothing in the
/// document said anything.
fn evidence_json(evidence: &Evidence) -> Value {
    match *evidence {
        Evidence::Text { line, start, end } => json!({
            "kind": "text",
            "line": line,
            "start": start,
            "end": end,
        }),
        Evidence::Filename => json!({ "kind": "filename" }),
    }
}

/// One candidate field: the value, how sure we are, and where it came from.
///
/// An absent field is `null` rather than a missing key, so the client's shape is
/// the same for a receipt that gave up everything and one that gave up nothing.
fn field_json<T>(found: Option<&Found<T>>, value: impl Fn(&T) -> Value) -> Value {
    match found {
        None => Value::Null,
        Some(found) => json!({
            "value": value(&found.value),
            "confidence": confidence_str(found.confidence),
            "evidence": evidence_json(&found.evidence),
        }),
    }
}

/// The candidates, one key per field of a claim's create form.
///
/// Money is integer cents and a rate is basis points, exactly as they are stored
/// — the client fills a form with these and posts them back; nothing here is a
/// float and nothing here is a total anybody computed.
fn fields_json(parsed: &ParsedReceipt) -> Value {
    json!({
        "merchant": field_json(parsed.merchant.as_ref(), |value| json!(value)),
        "spentOn": field_json(parsed.spent_on.as_ref(), |value| json!(iso_date(*value))),
        "grossCents": field_json(parsed.gross_cents.as_ref(), |value| json!(value)),
        "vatCents": field_json(parsed.vat_cents.as_ref(), |value| json!(value)),
        "vatRateBp": field_json(parsed.vat_rate_bp.as_ref(), |value| json!(value)),
        "currency": field_json(parsed.currency.as_ref(), |value| json!(value)),
    })
}

/// One reading as JSON: the file, whether there was text in it, and the
/// candidates.
///
/// `foundAnything` and `textLayer` are two different facts and a UI needs both:
/// "we read this and it says nothing we recognise" is a different sentence to
/// "there was nothing here to read" (a photograph, which is the ordinary case
/// until an AI backend is wired — see `alo_store::fin_receipt`).
fn reading_json(reading: &ReceiptReading) -> Value {
    json!({
        "nodeId": reading.node_id.as_str(),
        "filename": reading.filename,
        "contentType": reading.content_type,
        "sizeBytes": reading.size,
        "textLayer": reading.had_text,
        "foundAnything": reading.parsed.found_anything(),
        "lines": reading.parsed.lines,
        "fields": fields_json(&reading.parsed),
    })
}

/// `POST /finance/receipts` `{"nodeId"}` → `{"receipt": {…}}` — read a receipt
/// of the caller's own and answer with fields for them to confirm.
///
/// # Errors
/// `401` without a valid bearer token; `400` when the body is not JSON; `422`
/// when `nodeId` is missing, when the node holds no bytes (a folder), or when
/// the file is larger than the store's ceiling; `404` when the node is not one
/// the caller can read — another tenant's, a colleague's private file and one
/// that never existed are the same answer.
pub async fn read_receipt(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ReceiptBody = parse_body(&body)?;
    let node = req
        .node_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "nodeId is required: upload the receipt to Drive first, then read it",
            )
        })?;
    let reading = account
        .acc
        .read_receipt(&DriveNodeId::new(node.to_owned()), today())
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "receipt": reading_json(&reading) })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use alo_store::{PatternExtractor, ReceiptExtractor, ReceiptInput};
    use time::{Date, Month};

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("a real day")
    }

    /// A reading as the store would return it for a German till roll.
    fn reading() -> ReceiptReading {
        let parsed = PatternExtractor.extract(&ReceiptInput {
            text: "REWE Markt GmbH\nDatum 14.03.2026\nSUMME EUR 11,90\nMwSt 19% 1,90\n",
            filename: Some("REWE_2026-03-14.pdf"),
            today: day(2026, Month::March, 20),
        });
        ReceiptReading {
            node_id: DriveNodeId::new("node-1".to_owned()),
            filename: "REWE_2026-03-14.pdf".to_owned(),
            content_type: Some("application/pdf".to_owned()),
            size: 48_213,
            had_text: true,
            parsed,
        }
    }

    #[test]
    fn a_reading_reports_every_field_with_its_confidence_and_its_evidence() {
        let value = reading_json(&reading());
        assert_eq!(value["nodeId"], "node-1");
        assert_eq!(value["sizeBytes"], json!(48_213));
        assert_eq!(value["textLayer"], json!(true));
        assert_eq!(value["foundAnything"], json!(true));
        assert_eq!(value["fields"]["merchant"]["value"], "REWE Markt GmbH");
        assert_eq!(value["fields"]["merchant"]["confidence"], "high");
        assert_eq!(value["fields"]["spentOn"]["value"], "2026-03-14");
        assert_eq!(value["fields"]["grossCents"]["value"], json!(1190));
        assert_eq!(value["fields"]["vatCents"]["value"], json!(190));
        assert_eq!(value["fields"]["vatRateBp"]["value"], json!(1900));
        assert_eq!(value["fields"]["currency"]["value"], "EUR");
        assert!(
            value["fields"]["grossCents"]["value"].is_i64(),
            "money is an integer on the wire, never a float"
        );
    }

    #[test]
    fn the_evidence_indexes_the_lines_of_the_same_response() {
        let value = reading_json(&reading());
        let evidence = &value["fields"]["grossCents"]["evidence"];
        assert_eq!(evidence["kind"], "text");
        let line = evidence["line"].as_u64().expect("a line index") as usize;
        let start = evidence["start"].as_u64().expect("a start") as usize;
        let end = evidence["end"].as_u64().expect("an end") as usize;
        let text = value["lines"][line].as_str().expect("that line").to_owned();
        let quoted: String = text.chars().skip(start).take(end - start).collect();
        assert_eq!(quoted, "11,90");
    }

    #[test]
    fn a_field_the_file_name_gave_up_says_so() {
        // A photograph: no text layer at all, and only the name to go on.
        let parsed = PatternExtractor.extract(&ReceiptInput {
            text: "",
            filename: Some("REWE_2026-03-14.jpg"),
            today: day(2026, Month::March, 20),
        });
        let value = reading_json(&ReceiptReading {
            node_id: DriveNodeId::new("node-2".to_owned()),
            filename: "REWE_2026-03-14.jpg".to_owned(),
            content_type: Some("image/jpeg".to_owned()),
            size: 2_100_000,
            had_text: false,
            parsed,
        });
        assert_eq!(value["textLayer"], json!(false));
        assert_eq!(
            value["foundAnything"],
            json!(true),
            "the name said two things"
        );
        assert_eq!(value["fields"]["spentOn"]["value"], "2026-03-14");
        assert_eq!(
            value["fields"]["spentOn"]["evidence"]["kind"], "filename",
            "nothing in the document said it"
        );
        assert_eq!(value["fields"]["spentOn"]["confidence"], "low");
        assert_eq!(value["fields"]["grossCents"], json!(null));
    }

    #[test]
    fn a_receipt_that_read_as_nothing_still_answers_in_the_same_shape() {
        let value = reading_json(&ReceiptReading {
            node_id: DriveNodeId::new("node-3".to_owned()),
            filename: "scan.jpg".to_owned(),
            content_type: Some("image/jpeg".to_owned()),
            size: 900_000,
            had_text: false,
            parsed: ParsedReceipt::default(),
        });
        assert_eq!(value["foundAnything"], json!(false));
        assert_eq!(value["lines"], json!([]));
        for field in [
            "merchant",
            "spentOn",
            "grossCents",
            "vatCents",
            "vatRateBp",
            "currency",
        ] {
            assert_eq!(value["fields"][field], json!(null), "{field}");
        }
    }

    #[test]
    fn a_rate_without_a_printed_tax_yields_no_tax_amount() {
        // The module's whole reason for caring: 11,90 × 19/119 is not a fact.
        let parsed = PatternExtractor.extract(&ReceiptInput {
            text: "Café Central\n14.03.2026\nTotal 11,90\ninkl. 19% MwSt\n",
            filename: None,
            today: day(2026, Month::March, 20),
        });
        let value = fields_json(&parsed);
        assert_eq!(value["grossCents"]["value"], json!(1190));
        assert_eq!(value["vatRateBp"]["value"], json!(1900));
        assert_eq!(value["vatCents"], json!(null));
    }
}
