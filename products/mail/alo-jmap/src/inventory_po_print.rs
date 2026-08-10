//! The **printed purchase order** (alo Inventory, ADR 0035, wave B5.05a2) — the
//! paper a supplier reads, and the file that travels attached to the letter.
//!
//! It is B1.16 and B1.17's machinery with the party generalised, not a second
//! renderer: the order is turned into a [`PrintDocument`]
//! ([`PrintableOrder::as_document`]) and the same two renderers lay it out —
//! [`crate::billing_print`] for the page, [`crate::billing_pdf`] for the file.
//! What the document *says about itself* comes from its stored state and
//! nothing else: a draft prints DRAFT and carries no number because it has
//! none, a cancelled order prints CANCELLED under the number the supplier
//! already holds, and every figure is the store's integer cents.
//!
//! The two things an order does differently from an invoice are both in the
//! document kind, where either renderer can read them: its party is the
//! supplier ([`DocumentKind::PurchaseOrder`]), and its closing block asks for
//! goods by a day instead of money into an account — printing our own IBAN on
//! an order we placed would be an invitation to pay ourselves.
//!
//! The issuer block is unchanged: on an order *we* are the buyer, and the
//! tenant's billing identity is exactly the "who this is from, and where to
//! deliver" a supplier needs. It is read from
//! [`alo_store::billing_settings`] — one identity per tenant, not a second
//! address book for purchasing.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use time::OffsetDateTime;

use alo_store::billing_settings::BillingSettings;
use alo_store::billing_totals::Totals;
use alo_store::inv_po::{PoStatus, PurchaseOrderDocument};
use alo_store::inv_suppliers::Supplier;
use alo_store::{AccountStore, InvPurchaseOrderId, InvSupplierId, Line};

use crate::billing::map_store_err;
use crate::billing_pdf as pdf;
use crate::billing_print::{self as print, Banner, DocumentKind, Party, PrintDocument, PrintQuery};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// Everything a rendering of one order needs, read once through the account
/// door.
///
/// Both renderings are built from this — the page by [`print_purchase_order`],
/// the file by [`pdf_purchase_order`] — and so is the covering letter
/// ([`crate::inventory_po_send`]), so the paper the supplier holds, the file
/// attached to the email and the sentence in the email cannot disagree about a
/// figure, a date, or what the document is.
pub(crate) struct PrintableOrder {
    /// The order itself, with its lines and the store's totals.
    order: PurchaseOrderDocument,
    /// The document lines, without their catalog links.
    ///
    /// A purchase-order line is a shared document line plus a product
    /// ([`alo_store::inv_po_lines`]); the renderers take the shared part, and
    /// the product link — which matters when the goods arrive (B5.05b) — is not
    /// something a supplier's copy states. Held here rather than rebuilt per
    /// rendering because [`PrintDocument`] borrows its lines.
    lines: Vec<Line>,
    /// Who the order is to, re-read through the account door.
    supplier: Supplier,
    /// Who it is from: the tenant's own identity, blank if never saved.
    issuer: BillingSettings,
}

/// Loads one of the tenant's orders and both parties to it, or fails with the
/// `404` an id from another tenant gets.
///
/// The supplier is re-read **through the account door**, so an order is only
/// ever printed with its own tenant's supplier; a supplier that has vanished is
/// a `404` rather than a page with a hole in it, because a document that does
/// not name a party is not a document.
pub(crate) async fn printable(
    acc: &AccountStore,
    id: &InvPurchaseOrderId,
) -> Result<PrintableOrder, Problem> {
    let order = acc
        .inv_purchase_order(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such purchase order"))?;
    with_parties(acc, order).await
}

/// The same, for an order the caller already holds — the placing route has one
/// in hand and must not re-read it, since the read that matters happened inside
/// the transaction that numbered it.
pub(crate) async fn with_parties(
    acc: &AccountStore,
    order: PurchaseOrderDocument,
) -> Result<PrintableOrder, Problem> {
    let supplier = supplier_of(acc, &order.order.supplier_id).await?;
    let issuer = acc.billing_settings().await.map_err(map_store_err)?;
    let lines = order.lines.iter().map(|l| l.line.clone()).collect();
    Ok(PrintableOrder {
        order,
        lines,
        supplier,
        issuer,
    })
}

/// The order's supplier, through the account door.
async fn supplier_of(acc: &AccountStore, id: &InvSupplierId) -> Result<Supplier, Problem> {
    acc.inv_supplier(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such supplier"))
}

impl PrintableOrder {
    /// Where the order stands, which decides whether it may be placed at all
    /// ([`crate::inventory_po_send`]).
    pub(crate) fn status(&self) -> PoStatus {
        self.order.order.status
    }

    /// The order as the renderers see it.
    ///
    /// Its two dates are an order's dates — the day we asked, and the day we
    /// expect the goods — and its banner is its own state: a draft says so and
    /// carries no number, a cancelled order says so and keeps the number the
    /// supplier holds, and one that is on its way or has arrived stands as it
    /// was sent.
    pub(crate) fn as_document(&self) -> PrintDocument<'_> {
        let order = &self.order.order;
        PrintDocument {
            kind: DocumentKind::PurchaseOrder,
            banner: match order.status {
                PoStatus::Draft => Some(Banner::Draft),
                PoStatus::Cancelled => Some(Banner::Cancelled),
                // An order on its way, part-delivered or complete prints as it
                // was sent: what has arrived against it is our record, not a
                // correction to their copy.
                PoStatus::Sent | PoStatus::PartiallyReceived | PoStatus::Received => None,
            },
            number: order.number.as_deref(),
            primary_date: order.ordered_date,
            secondary_date: order.expected_date,
            reference: &order.reference,
            note: &order.note,
            currency: &order.currency,
            // Nothing about payment is on an order: we are the buyer, and the
            // terms that matter arrive on the supplier's invoice.
            payment_terms_days: None,
            credits_number: None,
            party: Party {
                name: &self.supplier.name,
                address_line1: &self.supplier.address_line1,
                address_line2: &self.supplier.address_line2,
                postal_code: &self.supplier.postal_code,
                city: &self.supplier.city,
                country: &self.supplier.country,
                vat_id: self.supplier.vat_id.as_deref(),
                email: self.supplier.email.as_deref(),
            },
            lines: &self.lines,
            totals: self.totals(),
            // An order is not a tax point: nothing is chargeable on it, so
            // there is no rate to freeze and nothing to restate (B1.21).
            restated: None,
            issuer: &self.issuer,
        }
    }

    /// What the order comes to — the store's figures, derived from the lines.
    pub(crate) fn totals(&self) -> &Totals {
        &self.order.totals
    }
}

/// `GET /inventory/purchase-orders/{id}/print[?lang=]` → the order as one
/// self-contained HTML page, laid out for A4.
///
/// The same page the PDF is laid out from, and the same rules: no script, no
/// external reference, `Content-Security-Policy: default-src 'none'` on the
/// response and `Cache-Control: no-store`, because this is a document about a
/// tenant's suppliers and prices, not a cacheable asset.
pub async fn print_purchase_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<PrintQuery>,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let printable = printable(&account.acc, &InvPurchaseOrderId::new(id)).await?;
    let document = printable.as_document();
    Ok(print::response(print::render(&document, query.strings())))
}

/// `GET /inventory/purchase-orders/{id}/pdf[?lang=]` → the same order as a PDF
/// file ([`crate::billing_pdf`]).
///
/// The **same** [`PrintDocument`] the page is rendered from, laid out a second
/// way rather than converted. Served as an attachment, never inline, under a
/// name built from the document's own heading — `Purchase-order-PO-2026-00001.pdf`
/// — so the file on a buyer's disk is called what the paper inside it is called.
///
/// Deliberately **no e-invoice**: EN 16931 describes a bill from a seller to a
/// buyer, and an order we place is neither. The bill that follows is the
/// supplier's, and reading one of those is B1.24's job.
pub async fn pdf_purchase_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<PrintQuery>,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let printable = printable(&account.acc, &InvPurchaseOrderId::new(id)).await?;
    let document = printable.as_document();
    let strings = query.strings();
    let bytes = pdf::render(&document, strings, pdf::stamp(OffsetDateTime::now_utc()));
    Ok(pdf::response(bytes, &pdf::file_name(&document, strings)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use alo_store::billing_totals::{LineFigures, totals};
    use alo_store::inv_po::PurchaseOrder;
    use alo_store::inv_po_lines::PoLine;
    use alo_store::{BillingLineId, Line};
    use time::{Month, OffsetDateTime};

    use crate::billing_print::strings_for;

    fn day(year: i32, month: u8, day: u8) -> time::Date {
        time::Date::from_calendar_date(year, Month::try_from(month).unwrap_or(Month::January), day)
            .unwrap_or(time::Date::MIN)
    }

    fn supplier() -> Supplier {
        Supplier {
            id: InvSupplierId::new("sup-1"),
            name: "Hoffmann Möbel GmbH".to_owned(),
            address_line1: "Werkstraße 9".to_owned(),
            address_line2: String::new(),
            postal_code: "8005".to_owned(),
            city: "Zürich".to_owned(),
            country: "CH".to_owned(),
            vat_id: Some("CHE116281277MWST".to_owned()),
            registration_no: "CH-020.3.000.000-0".to_owned(),
            email: Some("orders@hoffmann.test".to_owned()),
            phone: "+41 44 000 00 00".to_owned(),
            iban: Some("CH9300762011623852957".to_owned()),
            currency: "CHF".to_owned(),
            payment_terms_days: 30,
            lead_time_days: 9,
            note: String::new(),
            archived_at: None,
            created_by: "u1".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn order(status: PoStatus, number: Option<&str>) -> PurchaseOrderDocument {
        let line = PoLine {
            product_id: None,
            line: Line {
                id: BillingLineId::new("l-1"),
                line_order: 0,
                description: "Blue chair".to_owned(),
                unit: "piece".to_owned(),
                qty_milli: 4_000,
                unit_price_cents: 4_300,
                vat_rate_bp: 1900,
            },
            received_qty_milli: 0,
        };
        let figures = [LineFigures {
            qty_milli: 4_000,
            unit_price_cents: 4_300,
            vat_rate_bp: 1900,
        }];
        PurchaseOrderDocument {
            order: PurchaseOrder {
                id: InvPurchaseOrderId::new("po-1"),
                supplier_id: InvSupplierId::new("sup-1"),
                status,
                currency: "CHF".to_owned(),
                number: number.map(str::to_owned),
                ordered_date: number.map(|_| day(2026, 8, 10)),
                expected_date: Some(day(2026, 8, 24)),
                closed_date: None,
                reference: "Project Falkenstein".to_owned(),
                note: "Rear entrance.".to_owned(),
                created_by: "u1".to_owned(),
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
            },
            supplier_name: "Hoffmann Möbel GmbH".to_owned(),
            lines: vec![line],
            totals: totals(&figures),
        }
    }

    fn printable(status: PoStatus, number: Option<&str>) -> PrintableOrder {
        let document = order(status, number);
        let lines = document.lines.iter().map(|l| l.line.clone()).collect();
        PrintableOrder {
            order: document,
            lines,
            supplier: supplier(),
            issuer: BillingSettings {
                legal_name: "Alo Werkplaats B.V.".to_owned(),
                country: "NL".to_owned(),
                iban: Some("NL91ABNA0417164300".to_owned()),
                ..Default::default()
            },
        }
    }

    #[test]
    fn an_order_is_printed_as_the_supplier_document_it_is() {
        let printable = printable(PoStatus::Sent, Some("PO-2026-00001"));
        let document = printable.as_document();
        assert_eq!(document.kind, DocumentKind::PurchaseOrder);
        assert_eq!(document.number, Some("PO-2026-00001"));
        // An order's two dates: the day we asked, and the day we expect them.
        assert_eq!(document.primary_date, Some(day(2026, 8, 10)));
        assert_eq!(document.secondary_date, Some(day(2026, 8, 24)));
        // The party is the supplier's record, not a customer's.
        assert_eq!(document.party.name, "Hoffmann Möbel GmbH");
        assert_eq!(document.party.country, "CH");
        assert_eq!(document.party.vat_id, Some("CHE116281277MWST"));
        assert_eq!(document.party.email, Some("orders@hoffmann.test"));
        // Nothing about payment terms is on it, and nothing is restated: an
        // order is not a tax point.
        assert!(document.payment_terms_days.is_none());
        assert!(document.restated.is_none() && document.credits_number.is_none());
        // The money is the store's, and the rendered page carries no account of
        // ours — an IBAN on an order we placed is an invitation to pay
        // ourselves.
        assert_eq!(document.totals.gross_cents, 20_468);
        let html = print::render(&document, strings_for("en"));
        assert!(html.contains("Purchase order PO-2026-00001"));
        assert!(!html.contains("NL91 ABNA"), "{html}");
    }

    #[test]
    fn what_an_order_shouts_is_its_own_state() {
        // A draft has no number to print and says what it is.
        let draft = printable(PoStatus::Draft, None);
        let document = draft.as_document();
        assert_eq!(document.banner, Some(Banner::Draft));
        assert!(document.number.is_none() && document.primary_date.is_none());

        // A cancelled order keeps the number the supplier holds and says
        // plainly that it is off.
        let cancelled = printable(PoStatus::Cancelled, Some("PO-2026-00001"));
        let document = cancelled.as_document();
        assert_eq!(document.banner, Some(Banner::Cancelled));
        assert_eq!(document.number, Some("PO-2026-00001"));

        // Everything on its way, part-delivered or arrived prints as it was
        // sent: what has come in is our record, not a correction to their copy.
        for quiet in [
            PoStatus::Sent,
            PoStatus::PartiallyReceived,
            PoStatus::Received,
        ] {
            let printable = printable(quiet, Some("PO-2026-00001"));
            assert!(printable.as_document().banner.is_none(), "{quiet:?}");
        }
    }
}
