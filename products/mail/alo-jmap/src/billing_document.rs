//! The JSON a billing **document** is made of (alo Billing, ADR 0035, wave B1)
//! — the parts an invoice and a quote share, written once.
//!
//! An invoice and a quote have different lives (`billing_invoices`,
//! `billing_quotes`) but the same body: a list of lines in print order and the
//! money those lines add up to. The store shares that model literally
//! ([`alo_store::billing_line`]); this module is the same sharing at the edge,
//! so the two surfaces can never drift into reporting a line — or a total — in
//! two different shapes.
//!
//! **The client never computes money.** Every figure here is the server's,
//! derived from the lines on each read ([`alo_store::billing_totals`]); there
//! is no writable total in any billing response, so no request can influence
//! what a document is worth except by changing its lines.

use serde::Deserialize;
use time::{Date, OffsetDateTime};

use alo_store::billing_totals::Totals;
use alo_store::{Line, NewLine};

use serde_json::{Value, json};

/// The day a derived flag — an invoice's `overdue`, a quote's `expired` — is
/// judged against: the server's own date.
///
/// Deliberately not a value a client may send. Whether a document is late, or
/// an offer has lapsed, is a fact about the tenant's ledger, not about the
/// reader's clock, and a browser with a wrong date must not be able to clear
/// its own overdue list.
pub fn today() -> Date {
    OffsetDateTime::now_utc().date()
}

/// A line as JSON, including the net it contributes. VAT is deliberately not a
/// per-line figure: it is rounded once per rate subtotal
/// ([`alo_store::billing_totals`]), so a per-line VAT column would not add up
/// to the document's own.
pub fn line_json(l: &Line) -> Value {
    json!({
        "id": l.id.as_str(),
        "description": l.description,
        "unit": l.unit,
        "qtyMilli": l.qty_milli,
        "unitPriceCents": l.unit_price_cents,
        "vatRateBp": l.vat_rate_bp,
        "netCents": l.net_cents(),
    })
}

/// The money of a document: net, the VAT breakdown per rate, and gross — all
/// integer cents, all computed from the lines.
pub fn totals_json(t: &Totals) -> Value {
    json!({
        "netCents": t.net_cents,
        "vatCents": t.vat_cents,
        "grossCents": t.gross_cents,
        "vatByRate": t.vat_by_rate.iter().map(|s| json!({
            "rateBp": s.rate_bp,
            "netCents": s.net_cents,
            "vatCents": s.vat_cents,
        })).collect::<Vec<_>>(),
    })
}

/// Adds a document's `lines` and `totals` to its header object, in print
/// order. A header that is not a JSON object is returned untouched — it cannot
/// happen (every header here is built by `json!({…})`) and is not worth a
/// panic in a route.
pub fn with_body(mut header: Value, lines: &[Line], totals: &Totals) -> Value {
    if let Some(object) = header.as_object_mut() {
        object.insert(
            "lines".to_owned(),
            Value::Array(lines.iter().map(line_json).collect()),
        );
        object.insert("totals".to_owned(), totals_json(totals));
    }
    header
}

/// Adds a list entry's `totals` to its header object — a list carries what a
/// document is worth, never its lines.
pub fn with_totals(mut header: Value, totals: &Totals) -> Value {
    if let Some(object) = header.as_object_mut() {
        object.insert("totals".to_owned(), totals_json(totals));
    }
    header
}

/// One line as sent by a client. Every field is optional and defaults to the
/// blank line, so the store owns what "valid" means — an absent description is
/// an empty one and comes back as the store's own `422`, in the same words the
/// billing agent (B1.25) gets when it calls the store directly.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineBody {
    #[serde(default)]
    description: String,
    #[serde(default)]
    unit: String,
    #[serde(default)]
    qty_milli: i64,
    #[serde(default)]
    unit_price_cents: i64,
    #[serde(default)]
    vat_rate_bp: i32,
}

impl LineBody {
    /// The writable line this body asks for.
    pub fn into_line(self) -> NewLine {
        NewLine {
            description: self.description,
            unit: self.unit,
            qty_milli: self.qty_milli,
            unit_price_cents: self.unit_price_cents,
            vat_rate_bp: self.vat_rate_bp,
        }
    }

    /// A whole line set, in the order it was sent — which is the order it will
    /// print in.
    pub fn into_lines(lines: Vec<Self>) -> Vec<NewLine> {
        lines.into_iter().map(Self::into_line).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::billing_totals::{LineFigures, totals};

    fn body(json: Value) -> Vec<LineBody> {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    #[test]
    fn lines_arrive_in_the_order_they_were_sent() {
        let lines = LineBody::into_lines(body(json!([
            { "description": "Consulting", "unit": "hour", "qtyMilli": 1500,
              "unitPriceCents": 12_500, "vatRateBp": 2100 },
            { "description": "Discount", "qtyMilli": -1000, "unitPriceCents": 5_000 },
        ])));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].description, "Consulting");
        assert_eq!(lines[0].qty_milli, 1500);
        assert_eq!(lines[0].vat_rate_bp, 2100);
        // An omitted field is the blank line's value, not a stored one — a line
        // set is always sent whole.
        assert_eq!(lines[1].unit, "");
        assert_eq!(lines[1].qty_milli, -1000);
        assert_eq!(lines[1].vat_rate_bp, 0);
    }

    #[test]
    fn money_with_a_decimal_point_is_refused_never_rounded() {
        for bad in [
            json!([{ "description": "X", "unitPriceCents": 19.99 }]),
            json!([{ "description": "X", "qtyMilli": 1.5 }]),
            json!([{ "description": "X", "vatRateBp": "2100" }]),
        ] {
            assert!(
                serde_json::from_value::<Vec<LineBody>>(bad.clone()).is_err(),
                "{bad} should have been refused"
            );
        }
    }

    #[test]
    fn the_money_of_a_document_is_reported_per_rate_and_in_whole_cents() {
        let figures = [
            LineFigures {
                qty_milli: 1_500,
                unit_price_cents: 12_500,
                vat_rate_bp: 2100,
            },
            LineFigures {
                qty_milli: 1_000,
                unit_price_cents: 5_000,
                vat_rate_bp: 700,
            },
        ];
        let json = totals_json(&totals(&figures));
        assert_eq!(json["netCents"], json!(18_750 + 5_000));
        assert_eq!(json["vatByRate"][0]["rateBp"], json!(700));
        assert_eq!(json["vatByRate"][1]["rateBp"], json!(2100));
        assert_eq!(json["vatByRate"][1]["vatCents"], json!(3_938));
        assert!(
            json["vatCents"].is_i64() && json["grossCents"].is_i64(),
            "money is always an integer number of cents"
        );
    }

    #[test]
    fn a_body_is_the_header_plus_its_lines_and_totals() {
        let header = json!({ "id": "doc-1", "status": "draft" });
        let with = with_body(header.clone(), &[], &totals(&[]));
        assert_eq!(with["id"], json!("doc-1"));
        assert_eq!(with["lines"], json!([]));
        assert_eq!(with["totals"]["grossCents"], json!(0));
        // A list entry carries what the document is worth, never its lines.
        let summary = with_totals(header, &totals(&[]));
        assert!(summary.get("lines").is_none());
        assert_eq!(summary["totals"]["netCents"], json!(0));
    }
}
