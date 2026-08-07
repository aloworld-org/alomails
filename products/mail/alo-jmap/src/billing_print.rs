//! The printable billing document (alo Billing, ADR 0035, wave B1) — one
//! self-contained HTML page per invoice or quote, laid out for A4.
//!
//! This is the paper the customer holds. It is rendered **on the server**, and
//! `docs/design/billing.md` records why: the same page is the source of the
//! PDF (B1.17) and of the attachment on a mail draft (B1.18), neither of which
//! has a browser session to render in, and a document composed from the app's
//! own tokens would inherit the app's layout instead of an A4 sheet.
//!
//! Four rules the renderer holds itself to:
//!
//! - **Everything printed goes through [`esc`], and the page can reach
//!   nothing.** There is no script, no font, no image and no request in what
//!   this module emits. Two *different* mechanisms keep that true in the two
//!   places the page is used, and neither is the other's: fetched directly
//!   (headless chromium, B1.17) the response's own
//!   `Content-Security-Policy: default-src 'none'` ([`response`]) is what
//!   binds it; mounted by the web app it goes into a `srcdoc` frame, which is
//!   same-origin and *inherits the app's* policy rather than this one, so the
//!   frame is sandboxed without `allow-scripts` (`web/src/billing/printSheet.ts`).
//! - **The document says what it is.** A draft prints as a draft and carries
//!   no number, because it has none; a void invoice prints as void; a credit
//!   note is titled as one. Paper that could be mistaken for an issued invoice
//!   is a legal problem, not a cosmetic one.
//! - **No money is computed here.** Every figure is the store's integer cents
//!   ([`alo_store::billing_totals`]); this module only groups digits.
//! - **Its words are a table** ([`Strings`]), not literals in the markup — the
//!   same externalisation rule the web catalogues follow, in the one place a
//!   customer-facing string is emitted by Rust. `en` ships now; fr/nl at the
//!   wave review (B1.27).
//!
//! Dates print as **ISO `YYYY-MM-DD`** in every language, deliberately:
//! `05/03/2026` is two different days depending on who reads it, and a due
//! date that a customer can misread by two months is a dispute. EN 16931 dates
//! are ISO for the same reason.

use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;
use time::Date;
use time::format_description::well_known::Iso8601;

use alo_store::billing_settings::BillingSettings;
use alo_store::billing_totals::Totals;
use alo_store::{AccountStore, BillingCustomerId, Customer, Line};

use crate::billing::map_store_err;
use crate::error::Problem;

/// Which document this is, which decides its title, the meaning of its two
/// dates and whether it talks about payment at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    /// A bill: dates are issue and due, and it says how to pay.
    Invoice,
    /// A correction of a bill: same dates, and it says money comes back.
    CreditNote,
    /// An offer: dates are sent and valid-until, and nothing is owed yet.
    Quote,
}

/// A state that has to be legible across the whole page rather than tucked
/// into a corner — every one of them changes what the paper *means*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Banner {
    /// Not issued or sent: it carries no number and is owed by nobody.
    Draft,
    /// An issued invoice that was cancelled. It keeps its number.
    Void,
    /// An offer that was turned down or has lapsed.
    Closed,
}

/// Everything the page prints, gathered by the route from the store.
///
/// Borrowed throughout: rendering never owns the document, and never mutates
/// it — the same values are on their way into a JSON response.
pub struct PrintDocument<'a> {
    /// What this document is.
    pub kind: DocumentKind,
    /// The state to shout, or `None` when the document stands as it reads.
    pub banner: Option<Banner>,
    /// The legal number, or `None` on a draft.
    pub number: Option<&'a str>,
    /// Issue date (invoice) or sent date (quote).
    pub primary_date: Option<Date>,
    /// Due date (invoice) or valid-until (quote).
    pub secondary_date: Option<Date>,
    /// The customer's own reference; blank when none.
    pub reference: &'a str,
    /// The note typed on the document; blank when none.
    pub note: &'a str,
    /// ISO 4217 code the document is denominated in.
    pub currency: &'a str,
    /// Days from issue to due — printed as the payment term on an invoice.
    pub payment_terms_days: Option<i32>,
    /// The number of the invoice this one credits, when it credits one.
    pub credits_number: Option<&'a str>,
    /// Who the document is to.
    pub customer: &'a Customer,
    /// What is on it, in print order.
    pub lines: &'a [Line],
    /// What it comes to — the server's figures, never recomputed here.
    pub totals: &'a Totals,
    /// Who the document is from.
    pub issuer: &'a BillingSettings,
}

/// Every word the printed document says, in one language.
///
/// A struct rather than a lookup by key so a missing translation is a compile
/// error in the language table, never a blank on a customer's invoice.
pub struct Strings {
    /// BCP 47 tag for the page's `lang` attribute.
    pub lang: &'static str,
    /// Thousands separator used when grouping an amount.
    pub group_separator: &'static str,
    /// Decimal separator used in an amount and a quantity.
    pub decimal_separator: &'static str,
    /// Title of a bill.
    pub invoice: &'static str,
    /// Title of a correction to a bill.
    pub credit_note: &'static str,
    /// Title of an offer.
    pub quote: &'static str,
    /// Shouted across an unissued document.
    pub draft: &'static str,
    /// Shouted across a cancelled invoice.
    pub void: &'static str,
    /// Shouted across an offer that is no longer open.
    pub closed: &'static str,
    /// Heading over the customer's address.
    pub bill_to: &'static str,
    /// Label of the issue/sent date.
    pub issue_date: &'static str,
    /// Label of the sent date on an offer.
    pub sent_date: &'static str,
    /// Label of the due date.
    pub due_date: &'static str,
    /// Label of the valid-until date on an offer.
    pub valid_until: &'static str,
    /// Label of the customer's reference.
    pub reference: &'static str,
    /// Label of the customer's VAT id in the address block.
    pub customer_vat_id: &'static str,
    /// Column heading: what was sold.
    pub description: &'static str,
    /// Column heading: how much of it.
    pub quantity: &'static str,
    /// Column heading: price of one.
    pub unit_price: &'static str,
    /// Column heading: the VAT rate of the line.
    pub vat_rate: &'static str,
    /// Column heading: the line's net amount.
    pub line_net: &'static str,
    /// Total before VAT.
    pub net_total: &'static str,
    /// One rate's VAT, formatted with [`Strings::vat_at`].
    pub vat_at: fn(&str) -> String,
    /// Total including VAT.
    pub gross_total: &'static str,
    /// Heading over the payment instructions.
    pub payment: &'static str,
    /// The sentence that asks for the money, given the due date.
    pub payable_by: fn(&str) -> String,
    /// The sentence used when no due date is known (a draft).
    pub payable_on_terms: fn(i32) -> String,
    /// What a credit note says instead of asking for money.
    pub credit_explanation: fn(&str) -> String,
    /// What an offer says instead of asking for money.
    pub quote_validity: fn(&str) -> String,
    /// Label of the bank account.
    pub iban: &'static str,
    /// Label of the BIC.
    pub bic: &'static str,
    /// Label of the account holder.
    pub account_holder: &'static str,
    /// Label of the issuer's VAT id in the footer.
    pub vat_id: &'static str,
    /// Label of the issuer's company-register number.
    pub registration_no: &'static str,
    /// Printed in place of the issuer block when nothing has been saved.
    pub issuer_unstated: &'static str,
    /// Printed in place of the lines when a document has none.
    pub no_lines: &'static str,
}

/// The English document. The only table today; `fr`/`nl` land at B1.27, and
/// [`strings_for`] is the seam they plug into.
static EN: Strings = Strings {
    lang: "en",
    group_separator: "\u{202f}",
    decimal_separator: ".",
    invoice: "Invoice",
    credit_note: "Credit note",
    quote: "Quote",
    draft: "Draft",
    void: "Void",
    closed: "Closed",
    bill_to: "Bill to",
    issue_date: "Issue date",
    sent_date: "Sent",
    due_date: "Due date",
    valid_until: "Valid until",
    reference: "Your reference",
    customer_vat_id: "VAT id",
    description: "Description",
    quantity: "Qty",
    unit_price: "Unit price",
    vat_rate: "VAT",
    line_net: "Net",
    net_total: "Net total",
    vat_at: |rate| format!("VAT {rate}"),
    gross_total: "Total",
    payment: "Payment",
    payable_by: |date| {
        format!("Payable by {date} to the account below, quoting the invoice number.")
    },
    payable_on_terms: |days| {
        format!(
            "Payable within {days} days of the issue date to the account below, quoting the invoice number."
        )
    },
    credit_explanation: |number| {
        format!(
            "This credit note corrects invoice {number}. The amount shown is credited to you; nothing is payable on this document."
        )
    },
    quote_validity: |date| {
        format!(
            "This offer stands until {date}. It is not an invoice and nothing is payable on it."
        )
    },
    iban: "IBAN",
    bic: "BIC",
    account_holder: "Account holder",
    vat_id: "VAT id",
    registration_no: "Reg. no",
    issuer_unstated: "Your billing details have not been filled in yet.",
    no_lines: "This document has no lines yet.",
};

/// The words for a language tag, falling back to English.
///
/// Deliberately forgiving where the `status` filter is strict
/// (`docs/design/billing.md`): a filter that silently widened would show a
/// bookkeeper the wrong list, but a document that refuses to print because of
/// a display preference is worse than one printed in English. Matching is on
/// the primary subtag, so `en-GB` and `en` are the same document.
pub fn strings_for(tag: &str) -> &'static Strings {
    let primary = tag.split(['-', '_']).next().unwrap_or_default();
    match primary.to_ascii_lowercase().as_str() {
        "en" => &EN,
        // fr/nl join here at B1.27 without touching a caller; until then
        // (and for anything else) the default table prints the document.
        _ => &EN,
    }
}

/// Escapes text for HTML.
///
/// Escapes all five of `&<>"'` regardless of position: the renderer puts text
/// into attributes as well as elements, and one escaper that is safe
/// everywhere is worth more than two that have to be chosen between.
fn esc(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Formats integer cents as a grouped decimal — `1 234.56`, `-12.00`.
///
/// The only arithmetic in this module, and it is presentation: split into
/// units and hundredths, group the units in threes. Uses `i128` so
/// `i64::MIN` has no special case, and the sign is placed once, at the front.
fn amount(cents: i64, s: &Strings) -> String {
    let value = i128::from(cents);
    let negative = value < 0;
    let magnitude = value.unsigned_abs();
    let units = magnitude / 100;
    let hundredths = magnitude % 100;

    let digits = units.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push_str(s.group_separator);
        }
        grouped.push(c);
    }
    let sign = if negative { "\u{2212}" } else { "" };
    format!("{sign}{grouped}{}{hundredths:02}", s.decimal_separator)
}

/// Formats a quantity in milli-units: `1500` → `1.5`, `2000` → `2`, and a
/// discount's `-500` → `−0.5`. Trailing zeros are dropped, because "2 hours"
/// reads better than "2.000 hours" and no precision is lost.
fn quantity(qty_milli: i64, s: &Strings) -> String {
    let value = i128::from(qty_milli);
    let negative = value < 0;
    let magnitude = value.unsigned_abs();
    let whole = magnitude / 1000;
    let thousandths = magnitude % 1000;
    let sign = if negative { "\u{2212}" } else { "" };
    if thousandths == 0 {
        return format!("{sign}{whole}");
    }
    let fraction = format!("{thousandths:03}");
    let fraction = fraction.trim_end_matches('0');
    format!("{sign}{whole}{}{fraction}", s.decimal_separator)
}

/// Formats a VAT rate in basis points as a percentage: `2100` → `21%`,
/// `725` → `7.25%`, `0` → `0%`.
fn rate(bp: i32, s: &Strings) -> String {
    let whole = bp / 100;
    let hundredths = (bp % 100).abs();
    if hundredths == 0 {
        return format!("{whole}%");
    }
    let fraction = format!("{hundredths:02}");
    let fraction = fraction.trim_end_matches('0');
    format!("{whole}{}{fraction}%", s.decimal_separator)
}

/// Formats a date the one way that means the same thing in every member
/// state. See the module docs for why this is not localised.
fn date(value: Date) -> String {
    value.format(&Iso8601::DATE).unwrap_or_default()
}

/// The initials drawn in place of a logo — up to two, from the first words of
/// the issuer's legal name. A real logo is a Drive file and an upload surface
/// (its own item); a blank rectangle on every invoice is worse than initials.
fn monogram(legal_name: &str) -> String {
    legal_name
        .split_whitespace()
        .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}

/// Joins the non-empty parts of an address into `<div>` lines.
fn address_lines(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| format!("<div>{}</div>", esc(p)))
        .collect()
}

/// One `<tr>` of the label/value grid beside the title.
fn meta_row(label: &str, value: &str) -> String {
    format!(
        "<tr><th scope=\"row\">{}</th><td>{}</td></tr>",
        esc(label),
        esc(value)
    )
}

impl DocumentKind {
    /// The document's title in `s`.
    fn title(self, s: &Strings) -> &'static str {
        match self {
            Self::Invoice => s.invoice,
            Self::CreditNote => s.credit_note,
            Self::Quote => s.quote,
        }
    }
}

impl Banner {
    /// The word shouted across the page in `s`.
    fn word(self, s: &Strings) -> &'static str {
        match self {
            Self::Draft => s.draft,
            Self::Void => s.void,
            Self::Closed => s.closed,
        }
    }
}

/// The stylesheet, inline because the page must be one file that renders the
/// same in a print dialog, in a headless browser and inside an email client.
///
/// `@page` fixes A4 and the margins, so the sheet the customer holds is the
/// sheet the PDF measures. Everything is in `mm` and `pt`: a document that is
/// laid out in `px` is laid out for a screen.
const STYLE: &str = "\
@page { size: A4 portrait; margin: 16mm 15mm 14mm 15mm; }
* { box-sizing: border-box; }
html, body { margin: 0; padding: 0; }
body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
  font-size: 9.5pt; line-height: 1.45; color: #16181d; background: #fff;
  -webkit-print-color-adjust: exact; print-color-adjust: exact;
}
.sheet { width: 180mm; margin: 0 auto; padding: 0; }
@media screen { body { background: #eceef2; padding: 8mm 0; } .sheet { background: #fff; width: 210mm; padding: 16mm 15mm; box-shadow: 0 1px 6px rgba(0,0,0,.18); } }
.head { display: flex; justify-content: space-between; align-items: flex-start; gap: 12mm; }
.issuer { display: flex; gap: 4mm; align-items: flex-start; }
.mark {
  width: 13mm; height: 13mm; flex: none; border-radius: 2mm; background: #16181d; color: #fff;
  display: flex; align-items: center; justify-content: center; font-size: 13pt; font-weight: 600; letter-spacing: .5pt;
}
.issuer-name { font-size: 12pt; font-weight: 600; margin: 0 0 1mm; }
.issuer-address { color: #4a4f58; }
.title-block { text-align: right; }
h1 { font-size: 17pt; font-weight: 600; margin: 0 0 2mm; letter-spacing: -.2pt; }
.meta { border-collapse: collapse; margin-left: auto; }
.meta th, .meta td { padding: .4mm 0 .4mm 4mm; text-align: right; font-weight: 400; vertical-align: top; }
.meta th { color: #4a4f58; }
.meta td { font-variant-numeric: tabular-nums; }
.banner {
  margin: 5mm 0 0; padding: 2mm 3mm; border: .4mm solid #16181d; border-radius: 1mm;
  font-weight: 600; text-transform: uppercase; letter-spacing: 1pt; text-align: center;
}
.parties { display: flex; gap: 10mm; margin: 7mm 0 6mm; }
.party { flex: 1 1 0; }
.label { font-size: 8pt; text-transform: uppercase; letter-spacing: .6pt; color: #6b7280; margin: 0 0 1.5mm; }
.party-name { font-weight: 600; }
table.lines { width: 100%; border-collapse: collapse; margin-top: 2mm; }
table.lines th, table.lines td { padding: 1.8mm 2mm; text-align: left; vertical-align: top; }
table.lines thead th {
  border-bottom: .5mm solid #16181d; font-size: 8pt; text-transform: uppercase;
  letter-spacing: .5pt; color: #16181d;
}
table.lines tbody td { border-bottom: .2mm solid #dcdfe5; }
table.lines .num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
table.lines tbody tr { page-break-inside: avoid; }
.unit { color: #6b7280; }
.totals { width: 78mm; margin-left: auto; margin-top: 4mm; border-collapse: collapse; page-break-inside: avoid; }
.totals th, .totals td { padding: 1.2mm 2mm; }
.totals th { text-align: left; font-weight: 400; color: #4a4f58; }
.totals td { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
.totals .grand th, .totals .grand td { border-top: .5mm solid #16181d; font-weight: 700; font-size: 11pt; padding-top: 2mm; }
.pay { margin-top: 7mm; page-break-inside: avoid; }
.pay p { margin: 0 0 2mm; }
.bank { display: flex; gap: 8mm; flex-wrap: wrap; }
.bank div span { display: block; }
.bank .k { font-size: 8pt; text-transform: uppercase; letter-spacing: .5pt; color: #6b7280; }
.bank .v { font-variant-numeric: tabular-nums; }
.note { margin-top: 6mm; white-space: pre-wrap; }
.foot {
  margin-top: 9mm; padding-top: 3mm; border-top: .2mm solid #dcdfe5;
  font-size: 8pt; color: #6b7280;
}
.foot p { margin: 0 0 1mm; }
.empty { color: #6b7280; font-style: italic; padding: 4mm 0; }
";

/// Renders the whole document as one self-contained HTML page.
///
/// The result is a complete document (`<!doctype html>` … `</html>`) with its
/// stylesheet inline and no external reference of any kind, so it renders
/// identically in a print dialog, in headless chromium (B1.17) and as a mail
/// attachment (B1.18).
pub fn render(doc: &PrintDocument<'_>, s: &Strings) -> String {
    let title = doc.kind.title(s);
    let heading = match doc.number {
        Some(number) => format!("{title} {number}"),
        None => title.to_owned(),
    };

    // The number is in the heading, so it is deliberately NOT repeated in the
    // grid below it: a document that states its own number twice makes a
    // reader check whether the two agree.
    let mut meta = String::new();
    if let Some(primary) = doc.primary_date {
        let label = if doc.kind == DocumentKind::Quote {
            s.sent_date
        } else {
            s.issue_date
        };
        meta.push_str(&meta_row(label, &date(primary)));
    }
    if let Some(secondary) = doc.secondary_date {
        let label = if doc.kind == DocumentKind::Quote {
            s.valid_until
        } else {
            s.due_date
        };
        meta.push_str(&meta_row(label, &date(secondary)));
    }
    if !doc.reference.is_empty() {
        meta.push_str(&meta_row(s.reference, doc.reference));
    }

    let issuer = doc.issuer;
    let issuer_block = if issuer.legal_name.is_empty() {
        format!("<p class=\"empty\">{}</p>", esc(s.issuer_unstated))
    } else {
        format!(
            "<div class=\"mark\">{}</div>\
             <div><p class=\"issuer-name\">{}</p><div class=\"issuer-address\">{}</div></div>",
            esc(&monogram(&issuer.legal_name)),
            esc(&issuer.legal_name),
            address_lines(&[
                &issuer.address_line1,
                &issuer.address_line2,
                &format!("{} {}", issuer.postal_code, issuer.city),
                &issuer.country,
            ])
        )
    };

    let customer = doc.customer;
    let customer_vat = customer.vat_id.as_deref().unwrap_or_default();
    // A domestic address does not print its country: postal convention is to
    // name the country only when the document crosses a border, and a lone
    // "NL" under a Dutch address reads like a stray field. Cross-border, it
    // is exactly the line that matters, so it stays.
    let customer_country = if customer.country == issuer.country {
        ""
    } else {
        customer.country.as_str()
    };
    let customer_block = format!(
        "<p class=\"label\">{}</p><div class=\"party-name\">{}</div>{}{}",
        esc(s.bill_to),
        esc(&customer.name),
        address_lines(&[
            &customer.address_line1,
            &customer.address_line2,
            &format!("{} {}", customer.postal_code, customer.city),
            customer_country,
        ]),
        if customer_vat.is_empty() {
            String::new()
        } else {
            format!(
                "<div>{}: {}</div>",
                esc(s.customer_vat_id),
                esc(customer_vat)
            )
        }
    );

    let lines = if doc.lines.is_empty() {
        format!(
            "<tr><td colspan=\"5\" class=\"empty\">{}</td></tr>",
            esc(s.no_lines)
        )
    } else {
        doc.lines
            .iter()
            .map(|l| {
                format!(
                    "<tr><td>{}</td><td class=\"num\">{}{}</td><td class=\"num\">{}</td>\
                     <td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
                    esc(&l.description),
                    esc(&quantity(l.qty_milli, s)),
                    if l.unit.is_empty() {
                        String::new()
                    } else {
                        format!(" <span class=\"unit\">{}</span>", esc(&l.unit))
                    },
                    esc(&amount(l.unit_price_cents, s)),
                    esc(&rate(l.vat_rate_bp, s)),
                    esc(&amount(l.net_cents(), s)),
                )
            })
            .collect()
    };

    let currency = esc(doc.currency);
    let mut totals = format!(
        "<tr><th>{}</th><td>{} {}</td></tr>",
        esc(s.net_total),
        currency,
        esc(&amount(doc.totals.net_cents, s))
    );
    for subtotal in &doc.totals.vat_by_rate {
        totals.push_str(&format!(
            "<tr><th>{}</th><td>{} {}</td></tr>",
            esc(&(s.vat_at)(&rate(subtotal.rate_bp, s))),
            currency,
            esc(&amount(subtotal.vat_cents, s))
        ));
    }
    totals.push_str(&format!(
        "<tr class=\"grand\"><th>{}</th><td>{} {}</td></tr>",
        esc(s.gross_total),
        currency,
        esc(&amount(doc.totals.gross_cents, s))
    ));

    let payment = render_payment(doc, s);

    let mut footer_parts: Vec<String> = Vec::new();
    if let Some(vat_id) = issuer.vat_id.as_deref().filter(|v| !v.is_empty()) {
        footer_parts.push(format!("{}: {}", esc(s.vat_id), esc(vat_id)));
    }
    if !issuer.registration_no.is_empty() {
        footer_parts.push(format!(
            "{}: {}",
            esc(s.registration_no),
            esc(&issuer.registration_no)
        ));
    }
    for contact in [&issuer.email, &issuer.phone, &issuer.website] {
        if !contact.is_empty() {
            footer_parts.push(esc(contact));
        }
    }
    let mut footer = String::new();
    if !footer_parts.is_empty() {
        footer.push_str(&format!("<p>{}</p>", footer_parts.join(" · ")));
    }
    if !issuer.footer_note.is_empty() {
        footer.push_str(&format!("<p>{}</p>", esc(&issuer.footer_note)));
    }

    let banner = doc.banner.map_or_else(String::new, |b| {
        format!("<p class=\"banner\">{}</p>", esc(b.word(s)))
    });
    let note = if doc.note.is_empty() {
        String::new()
    } else {
        format!("<div class=\"note\">{}</div>", esc(doc.note))
    };

    format!(
        "<!doctype html>\n<html lang=\"{lang}\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{heading_title}</title>\n<style>{STYLE}</style>\n</head>\n\
         <body>\n<main class=\"sheet\">\n\
         <header class=\"head\"><div class=\"issuer\">{issuer_block}</div>\
         <div class=\"title-block\"><h1>{heading_html}</h1>\
         <table class=\"meta\"><tbody>{meta}</tbody></table></div></header>\n\
         {banner}\n\
         <section class=\"parties\"><div class=\"party\">{customer_block}</div></section>\n\
         <table class=\"lines\"><thead><tr><th>{c_desc}</th><th class=\"num\">{c_qty}</th>\
         <th class=\"num\">{c_price}</th><th class=\"num\">{c_vat}</th>\
         <th class=\"num\">{c_net}</th></tr></thead><tbody>{lines}</tbody></table>\n\
         <table class=\"totals\"><tbody>{totals}</tbody></table>\n\
         {payment}{note}\n\
         <footer class=\"foot\">{footer}</footer>\n\
         </main>\n</body>\n</html>\n",
        lang = esc(s.lang),
        heading_title = esc(&heading),
        heading_html = esc(&heading),
        c_desc = esc(s.description),
        c_qty = esc(s.quantity),
        c_price = esc(s.unit_price),
        c_vat = esc(s.vat_rate),
        c_net = esc(s.line_net),
    )
}

/// The block under the totals: what happens about the money, and where it
/// goes. A quote and a credit note both say explicitly that nothing is
/// payable, because a document that merely omits payment details reads as one
/// that forgot them.
fn render_payment(doc: &PrintDocument<'_>, s: &Strings) -> String {
    let sentence = match doc.kind {
        DocumentKind::Quote => (s.quote_validity)(
            &doc.secondary_date
                .map(date)
                .unwrap_or_else(|| "\u{2014}".to_owned()),
        ),
        DocumentKind::CreditNote => {
            (s.credit_explanation)(doc.credits_number.unwrap_or("\u{2014}"))
        }
        DocumentKind::Invoice => match doc.secondary_date {
            Some(due) => (s.payable_by)(&date(due)),
            None => (s.payable_on_terms)(doc.payment_terms_days.unwrap_or(0)),
        },
    };

    // A quote is not paid, and a credit note is not paid *to* us, so neither
    // prints the bank account: an IBAN under "nothing is payable" is exactly
    // how a customer pays a document twice.
    let bank = if doc.kind == DocumentKind::Invoice {
        let issuer = doc.issuer;
        let mut fields = String::new();
        if let Some(iban) = issuer.iban.as_deref().filter(|v| !v.is_empty()) {
            fields.push_str(&format!(
                "<div><span class=\"k\">{}</span><span class=\"v\">{}</span></div>",
                esc(s.iban),
                esc(&alo_store::iban::grouped(iban))
            ));
        }
        if let Some(bic) = issuer.bic.as_deref().filter(|v| !v.is_empty()) {
            fields.push_str(&format!(
                "<div><span class=\"k\">{}</span><span class=\"v\">{}</span></div>",
                esc(s.bic),
                esc(bic)
            ));
        }
        if !fields.is_empty() {
            let holder = issuer.effective_account_holder();
            if !holder.is_empty() {
                fields.push_str(&format!(
                    "<div><span class=\"k\">{}</span><span class=\"v\">{}{}</span></div>",
                    esc(s.account_holder),
                    esc(holder),
                    if issuer.bank_name.is_empty() {
                        String::new()
                    } else {
                        format!(" \u{00b7} {}", esc(&issuer.bank_name))
                    }
                ));
            }
        }
        fields
    } else {
        String::new()
    };

    format!(
        "<section class=\"pay\"><p class=\"label\">{}</p><p>{}</p><div class=\"bank\">{}</div></section>",
        esc(s.payment),
        esc(&sentence),
        bank
    )
}

// ---- route support ----------------------------------------------------------

/// Query string of both print routes.
#[derive(Deserialize)]
pub struct PrintQuery {
    /// BCP 47 tag for the document's language; absent or unknown prints the
    /// default (see [`strings_for`]).
    #[serde(default)]
    pub lang: Option<String>,
}

impl PrintQuery {
    /// The words this request's document is printed in.
    pub fn strings(&self) -> &'static Strings {
        strings_for(self.lang.as_deref().unwrap_or_default())
    }
}

/// The two records every printed document needs beyond the document itself:
/// who it is to, and who it is from.
///
/// The customer is re-read **through the account door**, so a document is only
/// ever printed with its own tenant's customer; the issuer identity is that
/// tenant's single row, blank when it has never been saved.
///
/// A document whose customer has vanished is a `404` rather than a page with a
/// hole in it: an invoice that does not name a party is not an invoice.
pub async fn parties(
    acc: &AccountStore,
    customer_id: &BillingCustomerId,
) -> Result<(Customer, BillingSettings), Problem> {
    let customer = acc
        .billing_customer(customer_id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such customer"))?;
    let issuer = acc.billing_settings().await.map_err(map_store_err)?;
    Ok((customer, issuer))
}

/// Serves a rendered document as HTML.
///
/// Three headers, each earning its place:
///
/// - **`Content-Security-Policy: default-src 'none'`** with only inline styles
///   allowed. The page is self-contained by construction; this makes it
///   self-contained by *enforcement*, so a future defect in the escaping still
///   cannot become a request that carries customer data off the page.
///   It binds whoever loads this response **as a document** — a PDF renderer
///   (B1.17), a saved file, a mail client. It does **not** reach the web app's
///   print path, which copies the body into a same-origin `srcdoc` frame that
///   inherits the app's policy instead; that frame is sandboxed script-free for
///   the same purpose (`web/src/billing/printSheet.ts`).
/// - **`X-Content-Type-Options: nosniff`**, so nothing re-interprets the body.
/// - **`Cache-Control: no-store`**: this is a customer's invoice, not a
///   cacheable asset, and it must not sit in a shared proxy or a disk cache
///   after the session that fetched it has gone.
pub fn response(html: String) -> Response {
    (
        [
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; style-src 'unsafe-inline'; img-src data:; \
                 form-action 'none'; base-uri 'none'",
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Html(html),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    use alo_store::billing_totals::totals;
    use alo_store::{BillingCustomerId, BillingLineId, LineFigures};
    use time::{Month, OffsetDateTime};

    /// A calendar date without a macro (`time` is built here without its
    /// `macros` feature) and without an `unwrap` (denied workspace-wide).
    fn day(year: i32, month: u8, day: u8) -> Date {
        Date::from_calendar_date(year, Month::try_from(month).unwrap_or(Month::January), day)
            .unwrap_or(Date::MIN)
    }

    fn customer() -> Customer {
        Customer {
            id: BillingCustomerId::new("cus-1".to_owned()),
            name: "Kunde & Söhne <GmbH>".to_owned(),
            address_line1: "Hauptstraße 5".to_owned(),
            address_line2: String::new(),
            postal_code: "10115".to_owned(),
            city: "Berlin".to_owned(),
            country: "DE".to_owned(),
            vat_id: Some("DE811907980".to_owned()),
            email: None,
            payment_terms_days: 14,
            currency: "EUR".to_owned(),
            contact_id: None,
            archived_at: None,
            created_by: "u1".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn issuer() -> BillingSettings {
        BillingSettings {
            legal_name: "Alo Werkplaats B.V.".to_owned(),
            address_line1: "Keizersgracht 1".to_owned(),
            postal_code: "1015 CJ".to_owned(),
            city: "Amsterdam".to_owned(),
            country: "NL".to_owned(),
            vat_id: Some("NL812345678B01".to_owned()),
            registration_no: "KVK 90123456".to_owned(),
            email: "billing@alo.test".to_owned(),
            iban: Some("NL91ABNA0417164300".to_owned()),
            bic: Some("ABNANL2A".to_owned()),
            bank_name: "ABN AMRO".to_owned(),
            footer_note: "Retention of title until paid in full.".to_owned(),
            updated_by: Some("u1".to_owned()),
            updated_at: Some(OffsetDateTime::UNIX_EPOCH),
            ..Default::default()
        }
    }

    fn line(description: &str, qty_milli: i64, unit_price_cents: i64, vat_rate_bp: i32) -> Line {
        Line {
            id: BillingLineId::new(format!("l-{description}")),
            line_order: 0,
            description: description.to_owned(),
            unit: "hour".to_owned(),
            qty_milli,
            unit_price_cents,
            vat_rate_bp,
        }
    }

    fn figures(lines: &[Line]) -> Totals {
        totals(
            &lines
                .iter()
                .map(|l| LineFigures {
                    qty_milli: l.qty_milli,
                    unit_price_cents: l.unit_price_cents,
                    vat_rate_bp: l.vat_rate_bp,
                })
                .collect::<Vec<_>>(),
        )
    }

    fn invoice<'a>(
        customer: &'a Customer,
        issuer: &'a BillingSettings,
        lines: &'a [Line],
        totals: &'a Totals,
    ) -> PrintDocument<'a> {
        PrintDocument {
            kind: DocumentKind::Invoice,
            banner: None,
            number: Some("INV-2026-00001"),
            primary_date: Some(day(2026, 8, 7)),
            secondary_date: Some(day(2026, 8, 21)),
            reference: "PO-42",
            note: "Thank you.",
            currency: "EUR",
            payment_terms_days: Some(14),
            credits_number: None,
            customer,
            lines,
            totals,
            issuer,
        }
    }

    #[test]
    fn amounts_are_grouped_and_never_lose_a_cent() {
        let s = strings_for("en");
        assert_eq!(amount(0, s), "0.00");
        assert_eq!(amount(5, s), "0.05");
        assert_eq!(amount(123_456, s), "1\u{202f}234.56");
        assert_eq!(amount(100_000_000, s), "1\u{202f}000\u{202f}000.00");
        assert_eq!(amount(-22_688, s), "\u{2212}226.88");
        // Total for any input a caller could hand it, i64 bounds included.
        assert!(!amount(i64::MIN, s).is_empty());
        assert!(!amount(i64::MAX, s).is_empty());
    }

    #[test]
    fn quantities_drop_their_trailing_zeros() {
        let s = strings_for("en");
        assert_eq!(quantity(2000, s), "2");
        assert_eq!(quantity(1500, s), "1.5");
        assert_eq!(quantity(1250, s), "1.25");
        assert_eq!(quantity(1001, s), "1.001");
        assert_eq!(quantity(-500, s), "\u{2212}0.5");
        assert_eq!(quantity(0, s), "0");
    }

    #[test]
    fn rates_read_as_percentages() {
        let s = strings_for("en");
        assert_eq!(rate(2100, s), "21%");
        assert_eq!(rate(0, s), "0%");
        assert_eq!(rate(725, s), "7.25%");
        assert_eq!(rate(10_000, s), "100%");
    }

    #[test]
    fn customer_data_is_escaped_everywhere_it_appears() {
        let mut c = customer();
        c.name = "<script>alert('x')</script> & Co".to_owned();
        c.city = "\"Berlin\"".to_owned();
        let lines = vec![line("<b>Consulting</b> & advice", 2000, 12_000, 2100)];
        let totals = figures(&lines);
        let issuer = issuer();
        let html = render(&invoice(&c, &issuer, &lines, &totals), strings_for("en"));

        assert!(
            !html.contains("<script>"),
            "a tag from customer data survived"
        );
        assert!(html.contains("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt; &amp; Co"));
        assert!(html.contains("&quot;Berlin&quot;"));
        assert!(html.contains("&lt;b&gt;Consulting&lt;/b&gt; &amp; advice"));
    }

    #[test]
    fn the_page_reaches_nothing_outside_itself() {
        let c = customer();
        let issuer = issuer();
        let lines = vec![line("Consulting", 2000, 12_000, 2100)];
        let totals = figures(&lines);
        let html = render(&invoice(&c, &issuer, &lines, &totals), strings_for("en"));

        for forbidden in ["<script", "http://", "https://", "src=", "@import", "url("] {
            assert!(
                !html.contains(forbidden),
                "the printed document must be self-contained, found: {forbidden}"
            );
        }
    }

    #[test]
    fn an_invoice_prints_its_number_dates_money_and_bank() {
        let c = customer();
        let issuer = issuer();
        let lines = vec![
            line("Consulting", 2000, 12_000, 2100),
            line("Discount", -500, 12_000, 2100),
        ];
        let totals = figures(&lines);
        let html = render(&invoice(&c, &issuer, &lines, &totals), strings_for("en"));

        assert!(html.contains("<title>Invoice INV-2026-00001</title>"));
        assert!(html.contains("2026-08-07") && html.contains("2026-08-21"));
        assert!(html.contains("PO-42"));
        // The server's figures, to the cent: 2 h + (−0.5 h) at 120.00.
        assert_eq!(totals.net_cents, 18_000);
        assert!(html.contains("EUR 180.00"));
        assert!(html.contains("VAT 21%"));
        assert!(html.contains("EUR 217.80"));
        // The bank details, grouped the way one is read out loud.
        assert!(html.contains("NL91 ABNA 0417 1643 00"));
        assert!(html.contains("ABNANL2A"));
        // …held in the legal name, since no separate holder was stated.
        assert!(html.contains("Alo Werkplaats B.V. \u{00b7} ABN AMRO"));
        assert!(html.contains("Payable by 2026-08-21"));
        // Both parties' VAT ids are on the document, as EN 16931 wants.
        assert!(html.contains("DE811907980") && html.contains("NL812345678B01"));
        assert!(html.contains("A4 portrait"));
    }

    #[test]
    fn the_number_is_stated_once_and_the_country_only_when_it_matters() {
        let c = customer();
        let issuer = issuer();
        let lines = vec![line("Consulting", 2000, 12_000, 2100)];
        let totals = figures(&lines);

        /// The customer's own block, so an assertion about their address
        /// cannot be satisfied by the issuer's identical country code.
        fn parties(html: &str) -> String {
            html.split_once("<section class=\"parties\">")
                .and_then(|(_, rest)| rest.split_once("</section>"))
                .map_or_else(String::new, |(block, _)| block.to_owned())
        }

        // Cross-border (NL → DE): the customer's country is on the address,
        // because it is the line that decides the VAT treatment.
        let html = render(&invoice(&c, &issuer, &lines, &totals), strings_for("en"));
        assert!(parties(&html).contains("<div>DE</div>"));
        // …and the number appears exactly once outside the <title>.
        let body = html.split_once("</head>").map_or("", |(_, b)| b);
        assert_eq!(body.matches("INV-2026-00001").count(), 1);

        // Domestic (NL → NL): a lone country code under a Dutch address is
        // noise, so it is not printed.
        let domestic = Customer {
            country: "NL".to_owned(),
            city: "Utrecht".to_owned(),
            ..c
        };
        let html = render(
            &invoice(&domestic, &issuer, &lines, &totals),
            strings_for("en"),
        );
        let block = parties(&html);
        assert!(block.contains("Utrecht"));
        assert!(!block.contains("<div>NL</div>"));
    }

    #[test]
    fn a_draft_says_so_and_carries_no_number() {
        let c = customer();
        let issuer = issuer();
        let lines = vec![line("Consulting", 2000, 12_000, 2100)];
        let totals = figures(&lines);
        let doc = PrintDocument {
            banner: Some(Banner::Draft),
            number: None,
            primary_date: None,
            secondary_date: None,
            ..invoice(&c, &issuer, &lines, &totals)
        };
        let html = render(&doc, strings_for("en"));

        assert!(html.contains("<title>Invoice</title>"));
        assert!(html.contains("class=\"banner\">Draft<"));
        assert!(
            !html.contains("INV-2026"),
            "a draft must not print a number"
        );
        // With no due date it states the term instead, so the page never
        // simply omits when the money is owed.
        assert!(html.contains("within 14 days"));
    }

    #[test]
    fn a_void_invoice_keeps_its_number_and_says_it_is_void() {
        let c = customer();
        let issuer = issuer();
        let lines = vec![line("Consulting", 2000, 12_000, 2100)];
        let totals = figures(&lines);
        let doc = PrintDocument {
            banner: Some(Banner::Void),
            ..invoice(&c, &issuer, &lines, &totals)
        };
        let html = render(&doc, strings_for("en"));
        assert!(html.contains("class=\"banner\">Void<"));
        assert!(html.contains("INV-2026-00001"));
    }

    #[test]
    fn a_credit_note_names_what_it_corrects_and_asks_for_nothing() {
        let c = customer();
        let issuer = issuer();
        let lines = vec![line("Consulting", -2000, 12_000, 2100)];
        let totals = figures(&lines);
        let doc = PrintDocument {
            kind: DocumentKind::CreditNote,
            number: Some("INV-2026-00002"),
            credits_number: Some("INV-2026-00001"),
            ..invoice(&c, &issuer, &lines, &totals)
        };
        let html = render(&doc, strings_for("en"));

        assert!(html.contains("<title>Credit note INV-2026-00002</title>"));
        assert!(html.contains("corrects invoice INV-2026-00001"));
        assert!(html.contains("nothing is payable"));
        // The money is negative, and the account is deliberately absent: an
        // IBAN under "nothing is payable" is how a document gets paid twice.
        assert!(html.contains("\u{2212}290.40"));
        assert!(!html.contains("NL91 ABNA"));
    }

    #[test]
    fn a_quote_is_dated_as_an_offer_and_owes_nothing() {
        let c = customer();
        let issuer = issuer();
        let lines = vec![line("Consulting", 2000, 12_000, 2100)];
        let totals = figures(&lines);
        let doc = PrintDocument {
            kind: DocumentKind::Quote,
            number: Some("QUO-2026-00001"),
            ..invoice(&c, &issuer, &lines, &totals)
        };
        let html = render(&doc, strings_for("en"));

        assert!(html.contains("<title>Quote QUO-2026-00001</title>"));
        assert!(html.contains("Sent") && html.contains("Valid until"));
        assert!(!html.contains("Due date"));
        assert!(html.contains("stands until 2026-08-21"));
        assert!(!html.contains("NL91 ABNA"));
    }

    #[test]
    fn a_tenant_that_has_not_filled_its_details_in_still_gets_a_document() {
        let c = customer();
        let issuer = BillingSettings::default();
        let lines = vec![line("Consulting", 2000, 12_000, 2100)];
        let totals = figures(&lines);
        let html = render(&invoice(&c, &issuer, &lines, &totals), strings_for("en"));

        // It prints, it says what is missing, and it invents nothing.
        assert!(html.contains("have not been filled in yet"));
        assert!(html.contains("EUR 290.40"));
        // No issuer means no bank block and no footer identifiers — never a
        // placeholder that reads like a real account.
        assert!(!html.contains("IBAN"));
        assert!(html.contains("Kunde &amp; Söhne &lt;GmbH&gt;"));
    }

    #[test]
    fn an_empty_document_says_it_is_empty_rather_than_showing_a_bare_table() {
        let c = customer();
        let issuer = issuer();
        let lines: Vec<Line> = Vec::new();
        let totals = figures(&lines);
        let html = render(&invoice(&c, &issuer, &lines, &totals), strings_for("en"));
        assert!(html.contains("no lines yet"));
        assert!(html.contains("EUR 0.00"));
    }

    #[test]
    fn a_monogram_is_at_most_two_initials() {
        assert_eq!(monogram("Alo Werkplaats B.V."), "AW");
        assert_eq!(monogram("Acme"), "A");
        assert_eq!(monogram("  "), "");
        // Punctuation-led words fall through to their first real character.
        assert_eq!(monogram("'t Winkeltje bv"), "TW");
    }

    #[test]
    fn an_unknown_language_still_prints() {
        // A display preference must never be the reason a document cannot be
        // printed; the fallback is the default table.
        for tag in ["", "en", "en-GB", "fr", "xx-YY", "🙂"] {
            assert_eq!(strings_for(tag).lang, "en");
        }
    }
}
