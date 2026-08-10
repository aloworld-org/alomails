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
    /// An order we place with a supplier (alo Inventory, B5.05a2): dates are
    /// the day we ordered and the day we expect the goods, the party is the
    /// supplier rather than a customer, and nothing about payment is on it —
    /// we are the buyer, and their invoice is the document that asks for money.
    PurchaseOrder,
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
    /// An order we stopped expecting the goods against. It keeps the number
    /// the supplier holds, and the word says plainly that it is off — a
    /// supplier re-reading their copy must not be able to mistake it for one
    /// still coming.
    Cancelled,
}

/// The counterparty a document names: who it is *to*.
///
/// Borrowed, and deliberately **not** one of the stored records. An invoice
/// names a [`Customer`] and a purchase order names a supplier
/// ([`alo_store::inv_suppliers::Supplier`]); the paper needs the same eight
/// facts from either, and the renderers must not learn which record they came
/// from — that is what would make a second document type mean a second
/// renderer. Whoever holds the record builds the party from it: billing here
/// ([`Party::customer`]), inventory in its own module.
pub struct Party<'a> {
    /// Legal or display name, as it goes on the address block.
    pub name: &'a str,
    /// Street address, first line.
    pub address_line1: &'a str,
    /// Street address, second line; blank when there is none.
    pub address_line2: &'a str,
    /// Postal/ZIP code.
    pub postal_code: &'a str,
    /// City / town.
    pub city: &'a str,
    /// ISO 3166-1 alpha-2 country code — printed only when the document
    /// crosses a border.
    pub country: &'a str,
    /// VAT identification number, `None` when they have not given one.
    pub vat_id: Option<&'a str>,
    /// Where a covering letter about this document goes
    /// ([`crate::billing_send`]), `None` when no address is stored. Never
    /// printed on the paper.
    pub email: Option<&'a str>,
}

impl<'a> Party<'a> {
    /// The party a billing document names.
    pub fn customer(customer: &'a Customer) -> Self {
        Self {
            name: &customer.name,
            address_line1: &customer.address_line1,
            address_line2: &customer.address_line2,
            postal_code: &customer.postal_code,
            city: &customer.city,
            country: &customer.country,
            vat_id: customer.vat_id.as_deref(),
            email: customer.email.as_deref(),
        }
    }
}

/// The document's VAT restated in the issuer's accounting currency, when it was
/// raised in another one (B1.21).
///
/// A foreign-currency invoice **must** print this: art. 230 of the VAT Directive
/// allows any currency on the document provided the VAT payable is also
/// expressed in the member state's own, converted at the art. 91 rate. The rate
/// and the day it was published are printed alongside it, so the customer — and
/// an auditor — can recompute the figure from the paper alone.
///
/// Owned rather than borrowed, unlike everything else here: it is derived from
/// the document's frozen rate rather than stored, and it is four small fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restated {
    /// ISO 4217 code the issuer keeps books in.
    pub currency: String,
    /// The document's whole VAT in that currency, in cents.
    pub vat_cents: i64,
    /// Units of the document's currency per one unit of `currency`
    /// ([`alo_store::billing_fx`]).
    pub rate_micro: i64,
    /// The day that rate was published.
    pub rate_date: Date,
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
    /// Who the document is to — a customer, or a supplier on an order.
    pub party: Party<'a>,
    /// What is on it, in print order.
    pub lines: &'a [Line],
    /// What it comes to — the server's figures, never recomputed here.
    pub totals: &'a Totals,
    /// The same VAT in the issuer's accounting currency, when the document was
    /// raised in another one; `None` when there is nothing to restate.
    pub restated: Option<Restated>,
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
    /// Title of an order placed with a supplier.
    pub purchase_order: &'static str,
    /// Shouted across an unissued document.
    pub draft: &'static str,
    /// Shouted across a cancelled invoice.
    pub void: &'static str,
    /// Shouted across an offer that is no longer open.
    pub closed: &'static str,
    /// Shouted across an order we stopped expecting.
    pub cancelled: &'static str,
    /// Heading over the customer's address.
    pub bill_to: &'static str,
    /// Heading over the supplier's address on an order.
    pub order_to: &'static str,
    /// Label of the issue/sent date.
    pub issue_date: &'static str,
    /// Label of the sent date on an offer.
    pub sent_date: &'static str,
    /// Label of the day an order was placed.
    pub order_date: &'static str,
    /// Label of the due date.
    pub due_date: &'static str,
    /// Label of the valid-until date on an offer.
    pub valid_until: &'static str,
    /// Label of the day the goods are expected on an order.
    pub expected_date: &'static str,
    /// Label of the customer's reference.
    pub reference: &'static str,
    /// Label of our own reference on an order — the same field, read from the
    /// other side of the table.
    pub own_reference: &'static str,
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
    /// Label of the VAT total restated in the issuer's own currency, given that
    /// currency's code.
    pub vat_in: fn(&str) -> String,
    /// The sentence under the totals that states the rate the restatement used.
    pub converted_at: fn(&str, &str) -> String,
    /// Heading over the payment instructions.
    pub payment: &'static str,
    /// Heading over the delivery instructions on an order — what stands where
    /// payment stands on an invoice, because an order's closing block is about
    /// goods arriving, not money moving.
    pub delivery: &'static str,
    /// The sentence that asks for the goods by a day.
    pub deliver_by: fn(&str) -> String,
    /// The sentence used when no day has been agreed.
    pub deliver_unstated: &'static str,
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

/// The English document, and the table every other one is checked against: a
/// language is added by writing a `Strings` and naming it in [`strings_for`],
/// and the struct makes a forgotten field a compile error rather than a blank
/// on a customer's invoice.
static EN: Strings = Strings {
    lang: "en",
    group_separator: "\u{202f}",
    decimal_separator: ".",
    invoice: "Invoice",
    credit_note: "Credit note",
    quote: "Quote",
    purchase_order: "Purchase order",
    draft: "Draft",
    void: "Void",
    closed: "Closed",
    cancelled: "Cancelled",
    bill_to: "Bill to",
    order_to: "Order to",
    issue_date: "Issue date",
    sent_date: "Sent",
    order_date: "Order date",
    due_date: "Due date",
    valid_until: "Valid until",
    expected_date: "Expected delivery",
    reference: "Your reference",
    own_reference: "Our reference",
    customer_vat_id: "VAT id",
    description: "Description",
    quantity: "Qty",
    unit_price: "Unit price",
    vat_rate: "VAT",
    line_net: "Net",
    net_total: "Net total",
    vat_at: |rate| format!("VAT {rate}"),
    gross_total: "Total",
    vat_in: |code| format!("VAT in {code}"),
    converted_at: |rate, day| {
        format!("VAT converted at {rate}, the reference rate published on {day}.")
    },
    payment: "Payment",
    delivery: "Delivery",
    deliver_by: |date| {
        format!("Please deliver to the address above by {date}, quoting the order number.")
    },
    deliver_unstated: "Please deliver to the address above, quoting the order number, and confirm the delivery date.",
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

/// The French document (B1.27).
///
/// Two conventions a French reader would otherwise trip over are in the table
/// rather than in the renderer: amounts group with a narrow no-break space and
/// take a comma for the decimal, and totals are labelled HT/TTC, which is what
/// "net" and "gross" are actually called on a French invoice.
static FR: Strings = Strings {
    lang: "fr",
    group_separator: "\u{202f}",
    decimal_separator: ",",
    invoice: "Facture",
    credit_note: "Avoir",
    quote: "Devis",
    purchase_order: "Bon de commande",
    draft: "Brouillon",
    void: "Annulée",
    closed: "Clos",
    cancelled: "Annulé",
    bill_to: "Facturé à",
    order_to: "Commandé à",
    issue_date: "Date d’émission",
    sent_date: "Envoyé le",
    order_date: "Date de commande",
    due_date: "Échéance",
    valid_until: "Valable jusqu’au",
    expected_date: "Livraison prévue",
    reference: "Votre référence",
    own_reference: "Notre référence",
    customer_vat_id: "N° de TVA",
    description: "Désignation",
    quantity: "Qté",
    unit_price: "Prix unitaire",
    vat_rate: "TVA",
    line_net: "Montant HT",
    net_total: "Total HT",
    vat_at: |rate| format!("TVA {rate}"),
    gross_total: "Total TTC",
    vat_in: |code| format!("TVA en {code}"),
    converted_at: |rate, day| format!("TVA convertie à {rate}, taux de référence publié le {day}."),
    payment: "Paiement",
    delivery: "Livraison",
    deliver_by: |date| {
        format!(
            "Merci de livrer à l’adresse ci-dessus avant le {date}, en rappelant le numéro de commande."
        )
    },
    deliver_unstated: "Merci de livrer à l’adresse ci-dessus en rappelant le numéro de commande, et de confirmer la date de livraison.",
    payable_by: |date| {
        format!(
            "À régler avant le {date} sur le compte ci-dessous, en rappelant le numéro de facture."
        )
    },
    payable_on_terms: |days| {
        format!(
            "À régler sous {days} jours à compter de la date d’émission sur le compte ci-dessous, en rappelant le numéro de facture."
        )
    },
    credit_explanation: |number| {
        format!(
            "Le présent avoir corrige la facture {number}. Le montant indiqué vous est crédité ; rien n’est à payer sur ce document."
        )
    },
    quote_validity: |date| {
        format!(
            "La présente offre est valable jusqu’au {date}. Ce n’est pas une facture et rien n’est à payer."
        )
    },
    iban: "IBAN",
    bic: "BIC",
    account_holder: "Titulaire du compte",
    vat_id: "N° de TVA",
    registration_no: "N° d’immatriculation",
    issuer_unstated: "Vos coordonnées de facturation n’ont pas encore été renseignées.",
    no_lines: "Ce document ne comporte encore aucune ligne.",
};

/// The Dutch document (B1.27).
///
/// Dutch groups thousands with a point and takes a comma for the decimal, so
/// this is the one table where the group separator is a character a reader of
/// another language would read as a decimal point — which is exactly why the
/// separators are per-language data and not a constant.
static NL: Strings = Strings {
    lang: "nl",
    group_separator: ".",
    decimal_separator: ",",
    invoice: "Factuur",
    credit_note: "Creditnota",
    quote: "Offerte",
    purchase_order: "Inkooporder",
    draft: "Concept",
    void: "Geannuleerd",
    closed: "Gesloten",
    cancelled: "Ingetrokken",
    bill_to: "Factuuradres",
    order_to: "Besteld bij",
    issue_date: "Uitgiftedatum",
    sent_date: "Verstuurd op",
    order_date: "Besteldatum",
    due_date: "Vervaldatum",
    valid_until: "Geldig tot",
    expected_date: "Verwachte levering",
    reference: "Uw referentie",
    own_reference: "Onze referentie",
    customer_vat_id: "Btw-nummer",
    description: "Omschrijving",
    quantity: "Aantal",
    unit_price: "Stukprijs",
    vat_rate: "Btw",
    line_net: "Netto",
    net_total: "Totaal netto",
    vat_at: |rate| format!("Btw {rate}"),
    gross_total: "Totaal",
    vat_in: |code| format!("Btw in {code}"),
    converted_at: |rate, day| {
        format!("Btw omgerekend tegen {rate}, de referentiekoers gepubliceerd op {day}.")
    },
    payment: "Betaling",
    delivery: "Levering",
    deliver_by: |date| {
        format!(
            "Graag leveren op bovenstaand adres vóór {date}, onder vermelding van het ordernummer."
        )
    },
    deliver_unstated: "Graag leveren op bovenstaand adres onder vermelding van het ordernummer, en de leverdatum bevestigen.",
    payable_by: |date| {
        format!(
            "Te voldoen vóór {date} op onderstaande rekening, onder vermelding van het factuurnummer."
        )
    },
    payable_on_terms: |days| {
        format!(
            "Te voldoen binnen {days} dagen na de uitgiftedatum op onderstaande rekening, onder vermelding van het factuurnummer."
        )
    },
    credit_explanation: |number| {
        format!(
            "Deze creditnota corrigeert factuur {number}. Het getoonde bedrag wordt u gecrediteerd; op dit document is niets verschuldigd."
        )
    },
    quote_validity: |date| {
        format!(
            "Deze offerte is geldig tot {date}. Het is geen factuur en er is niets op verschuldigd."
        )
    },
    iban: "IBAN",
    bic: "BIC",
    account_holder: "Rekeninghouder",
    vat_id: "Btw-nummer",
    registration_no: "Registratienr.",
    issuer_unstated: "Uw facturatiegegevens zijn nog niet ingevuld.",
    no_lines: "Dit document heeft nog geen regels.",
};

/// The words for a language tag, falling back to English.
///
/// Deliberately forgiving where the `status` filter is strict
/// (`docs/design/billing.md`): a filter that silently widened would show a
/// bookkeeper the wrong list, but a document that refuses to print because of
/// a display preference is worse than one printed in English. Matching is on
/// the primary subtag, so `en-GB` and `en` are the same document, and `fr-BE`
/// prints in French.
pub fn strings_for(tag: &str) -> &'static Strings {
    let primary = tag.split(['-', '_']).next().unwrap_or_default();
    match primary.to_ascii_lowercase().as_str() {
        "fr" => &FR,
        "nl" => &NL,
        // English is the default table, and anything we do not ship prints in
        // it rather than refusing.
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
pub(crate) fn amount(cents: i64, s: &Strings) -> String {
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
pub(crate) fn quantity(qty_milli: i64, s: &Strings) -> String {
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
pub(crate) fn rate(bp: i32, s: &Strings) -> String {
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
pub(crate) fn date(value: Date) -> String {
    value.format(&Iso8601::DATE).unwrap_or_default()
}

/// The initials drawn in place of a logo — up to two, from the first words of
/// the issuer's legal name. A real logo is a Drive file and an upload surface
/// (its own item); a blank rectangle on every invoice is worse than initials.
pub(crate) fn monogram(legal_name: &str) -> String {
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
    pub(crate) fn title(self, s: &Strings) -> &'static str {
        match self {
            Self::Invoice => s.invoice,
            Self::CreditNote => s.credit_note,
            Self::Quote => s.quote,
            Self::PurchaseOrder => s.purchase_order,
        }
    }

    /// What the counterparty is called when a *refusal* has to name them
    /// ([`crate::billing_send::recipient`]).
    ///
    /// Not a translated string: `Problem` details are the API's own English,
    /// and this one has to send a user to the right screen — telling a buyer
    /// that "this customer has no email address" about an order they placed
    /// with a supplier is a wrong instruction, not a wrong word.
    pub(crate) fn party_noun(self) -> &'static str {
        match self {
            Self::Invoice | Self::CreditNote | Self::Quote => "customer",
            Self::PurchaseOrder => "supplier",
        }
    }

    /// Heading over the counterparty's address.
    pub(crate) fn party_label(self, s: &Strings) -> &'static str {
        match self {
            Self::Invoice | Self::CreditNote | Self::Quote => s.bill_to,
            Self::PurchaseOrder => s.order_to,
        }
    }

    /// What the document's first date means: issued, sent, ordered.
    pub(crate) fn primary_date_label(self, s: &Strings) -> &'static str {
        match self {
            Self::Invoice | Self::CreditNote => s.issue_date,
            Self::Quote => s.sent_date,
            Self::PurchaseOrder => s.order_date,
        }
    }

    /// What the document's second date means: owed by, valid until, expected.
    pub(crate) fn secondary_date_label(self, s: &Strings) -> &'static str {
        match self {
            Self::Invoice | Self::CreditNote => s.due_date,
            Self::Quote => s.valid_until,
            Self::PurchaseOrder => s.expected_date,
        }
    }

    /// Whose reference the `reference` field is. Ours on an order we place,
    /// theirs on everything we send a customer — the same stored string, read
    /// from the other side of the table.
    pub(crate) fn reference_label(self, s: &Strings) -> &'static str {
        match self {
            Self::Invoice | Self::CreditNote | Self::Quote => s.reference,
            Self::PurchaseOrder => s.own_reference,
        }
    }

    /// Heading over the closing block: what happens next about the money, or
    /// on an order, about the goods.
    pub(crate) fn closing_label(self, s: &Strings) -> &'static str {
        match self {
            Self::Invoice | Self::CreditNote | Self::Quote => s.payment,
            Self::PurchaseOrder => s.delivery,
        }
    }

    /// Whether the document prints an account to pay into.
    ///
    /// An invoice only. A quote is not paid, a credit note is not paid *to*
    /// us, and on a purchase order the account that matters is the supplier's,
    /// which arrives on *their* invoice — an IBAN under "nothing is payable",
    /// or our own IBAN on an order we placed, is exactly how a document gets
    /// paid twice or paid backwards.
    pub(crate) fn prints_bank_details(self) -> bool {
        matches!(self, Self::Invoice)
    }
}

impl Banner {
    /// The word shouted across the page in `s`.
    pub(crate) fn word(self, s: &Strings) -> &'static str {
        match self {
            Self::Draft => s.draft,
            Self::Void => s.void,
            Self::Closed => s.closed,
            Self::Cancelled => s.cancelled,
        }
    }
}

/// The sentence under the closing label: whether anything is owed, and by
/// when — or, on an order, when the goods are wanted.
///
/// One function for both renderings ([`render`] and [`crate::billing_pdf`]),
/// because the page and the file are one document: a sentence written twice is
/// a sentence that eventually says two things.
pub(crate) fn closing_sentence(doc: &PrintDocument<'_>, s: &Strings) -> String {
    match doc.kind {
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
        DocumentKind::PurchaseOrder => match doc.secondary_date {
            Some(expected) => (s.deliver_by)(&date(expected)),
            None => s.deliver_unstated.to_owned(),
        },
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
.fx { width: 78mm; margin: 1.5mm 0 0 auto; text-align: right; font-size: 8pt; color: #4a4f58; }
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
/// What the document calls itself: its kind, and its number once it has one.
///
/// Shared with the PDF renderer ([`crate::billing_pdf`]) so the paper, the
/// screen and the file name a customer saves cannot disagree about what the
/// document is.
pub fn document_heading(doc: &PrintDocument<'_>, s: &Strings) -> String {
    let title = doc.kind.title(s);
    match doc.number {
        Some(number) => format!("{title} {number}"),
        None => title.to_owned(),
    }
}

/// The document's name reduced to a **file name stem** — what every rendering
/// of it is saved as, before its extension.
///
/// Built from the heading, so the file on a customer's disk is called what the
/// paper inside it is called, and reduced to ASCII alphanumerics and single
/// hyphens: a file name has to survive a `Content-Disposition` header, three
/// operating systems and a mail client, and the document's *kind* comes from a
/// translation table, which is not a place to trust that nobody ever typed a
/// quote mark. Empty only if the heading had no alphanumeric character at all,
/// which every caller replaces with a name of its own.
///
/// One function for every rendering ([`crate::billing_pdf`],
/// [`crate::billing_cii`]) so the PDF and the e-invoice a customer downloads
/// are named the same thing with two extensions.
pub fn file_stem(doc: &PrintDocument<'_>, s: &Strings) -> String {
    let ascii: String = document_heading(doc, s)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    ascii
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn render(doc: &PrintDocument<'_>, s: &Strings) -> String {
    let heading = document_heading(doc, s);

    // The number is in the heading, so it is deliberately NOT repeated in the
    // grid below it: a document that states its own number twice makes a
    // reader check whether the two agree.
    let mut meta = String::new();
    if let Some(primary) = doc.primary_date {
        meta.push_str(&meta_row(doc.kind.primary_date_label(s), &date(primary)));
    }
    if let Some(secondary) = doc.secondary_date {
        meta.push_str(&meta_row(
            doc.kind.secondary_date_label(s),
            &date(secondary),
        ));
    }
    if !doc.reference.is_empty() {
        meta.push_str(&meta_row(doc.kind.reference_label(s), doc.reference));
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

    let party = &doc.party;
    let party_vat = party.vat_id.unwrap_or_default();
    // A domestic address does not print its country: postal convention is to
    // name the country only when the document crosses a border, and a lone
    // "NL" under a Dutch address reads like a stray field. Cross-border, it
    // is exactly the line that matters, so it stays.
    let party_country = if party.country == issuer.country {
        ""
    } else {
        party.country
    };
    let party_block = format!(
        "<p class=\"label\">{}</p><div class=\"party-name\">{}</div>{}{}",
        esc(doc.kind.party_label(s)),
        esc(party.name),
        address_lines(&[
            party.address_line1,
            party.address_line2,
            &format!("{} {}", party.postal_code, party.city),
            party_country,
        ]),
        if party_vat.is_empty() {
            String::new()
        } else {
            format!("<div>{}: {}</div>", esc(s.customer_vat_id), esc(party_vat))
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
    // The VAT in the issuer's own currency, on a document raised in another one
    // — required, not decorative (see `Restated`), and stated with the rate that
    // produced it so the figure can be recomputed from the paper.
    let restated = match doc.restated.as_ref() {
        Some(r) => {
            totals.push_str(&format!(
                "<tr><th>{}</th><td>{} {}</td></tr>",
                esc(&(s.vat_in)(&r.currency)),
                esc(&r.currency),
                esc(&amount(r.vat_cents, s))
            ));
            format!(
                "<p class=\"fx\">{}</p>",
                esc(&(s.converted_at)(
                    &rate_sentence(r, doc.currency),
                    &date(r.rate_date)
                ))
            )
        }
        None => String::new(),
    };

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
         <section class=\"parties\"><div class=\"party\">{party_block}</div></section>\n\
         <table class=\"lines\"><thead><tr><th>{c_desc}</th><th class=\"num\">{c_qty}</th>\
         <th class=\"num\">{c_price}</th><th class=\"num\">{c_vat}</th>\
         <th class=\"num\">{c_net}</th></tr></thead><tbody>{lines}</tbody></table>\n\
         <table class=\"totals\"><tbody>{totals}</tbody></table>\n\
         {restated}\n\
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

/// The rate as it is printed: `1 EUR = 1.1626 USD`, the direction the reference
/// rates are published in, so the number on the page is the number that was
/// applied rather than its reciprocal.
///
/// Shared with the PDF renderer ([`crate::billing_pdf`]) so the paper and the
/// file cannot state the conversion differently.
pub(crate) fn rate_sentence(restated: &Restated, document_currency: &str) -> String {
    format!(
        "1 {} = {} {}",
        restated.currency,
        alo_store::billing_fx::format_rate(restated.rate_micro),
        document_currency
    )
}

/// The block under the totals: what happens about the money, and where it
/// goes. A quote and a credit note both say explicitly that nothing is
/// payable, because a document that merely omits payment details reads as one
/// that forgot them; an order says when the goods are wanted instead.
fn render_payment(doc: &PrintDocument<'_>, s: &Strings) -> String {
    let sentence = closing_sentence(doc, s);

    // Only an invoice prints an account ([`DocumentKind::prints_bank_details`]).
    let bank = if doc.kind.prints_bank_details() {
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
        esc(doc.kind.closing_label(s)),
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
            party: Party::customer(customer),
            lines,
            totals,
            restated: None,
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
        for tag in ["", "en", "en-GB", "xx-YY", "🙂"] {
            assert_eq!(strings_for(tag).lang, "en");
        }
    }

    #[test]
    fn a_shipped_language_is_picked_on_its_primary_subtag() {
        for tag in ["fr", "FR", "fr-BE", "fr_CH"] {
            assert_eq!(strings_for(tag).lang, "fr", "{tag}");
        }
        for tag in ["nl", "NL", "nl-BE", "nl_BE"] {
            assert_eq!(strings_for(tag).lang, "nl", "{tag}");
        }
    }

    #[test]
    fn each_language_writes_money_the_way_its_readers_do() {
        // The separators are per-language data precisely because Dutch groups
        // with the character English reads as a decimal point: a document that
        // borrowed another table's separators would print an amount a
        // thousandfold wrong to the person holding it.
        assert_eq!(
            amount(123_456_789, strings_for("en")),
            "1\u{202f}234\u{202f}567.89"
        );
        assert_eq!(
            amount(123_456_789, strings_for("fr")),
            "1\u{202f}234\u{202f}567,89"
        );
        assert_eq!(amount(123_456_789, strings_for("nl")), "1.234.567,89");
        // A negative is signed once, at the front, in every language.
        assert_eq!(amount(-150, strings_for("nl")), "\u{2212}1,50");
        // Quantities and rates take the same decimal separator as amounts.
        assert_eq!(quantity(1500, strings_for("fr")), "1,5");
        assert_eq!(quantity(1500, strings_for("nl")), "1,5");
        assert_eq!(rate(725, strings_for("nl")), "7,25%");
    }

    #[test]
    fn no_table_leaves_a_word_blank() {
        // The struct makes a *missing* field a compile error; an *empty* one it
        // cannot catch, and a blank label on a customer's invoice is exactly
        // the failure this table's shape exists to prevent.
        for tag in ["en", "fr", "nl"] {
            let s = strings_for(tag);
            for (name, value) in [
                ("lang", s.lang),
                ("decimal_separator", s.decimal_separator),
                ("invoice", s.invoice),
                ("credit_note", s.credit_note),
                ("quote", s.quote),
                ("purchase_order", s.purchase_order),
                ("draft", s.draft),
                ("void", s.void),
                ("closed", s.closed),
                ("cancelled", s.cancelled),
                ("bill_to", s.bill_to),
                ("order_to", s.order_to),
                ("issue_date", s.issue_date),
                ("sent_date", s.sent_date),
                ("order_date", s.order_date),
                ("due_date", s.due_date),
                ("valid_until", s.valid_until),
                ("expected_date", s.expected_date),
                ("reference", s.reference),
                ("own_reference", s.own_reference),
                ("delivery", s.delivery),
                ("deliver_unstated", s.deliver_unstated),
                ("customer_vat_id", s.customer_vat_id),
                ("description", s.description),
                ("quantity", s.quantity),
                ("unit_price", s.unit_price),
                ("vat_rate", s.vat_rate),
                ("line_net", s.line_net),
                ("net_total", s.net_total),
                ("gross_total", s.gross_total),
                ("payment", s.payment),
                ("iban", s.iban),
                ("bic", s.bic),
                ("account_holder", s.account_holder),
                ("vat_id", s.vat_id),
                ("registration_no", s.registration_no),
                ("issuer_unstated", s.issuer_unstated),
                ("no_lines", s.no_lines),
            ] {
                assert!(!value.trim().is_empty(), "{tag}: {name} is blank");
            }
            // Not trimmed: two of the three tables group with a narrow no-break
            // space, which is a real separator and would trim away to nothing.
            assert!(!s.group_separator.is_empty(), "{tag}: group_separator");
            // The sentences, too — each one must actually place what it was
            // given, or a due date would silently vanish off a translated page.
            assert!((s.vat_at)("21%").contains("21%"), "{tag}: vat_at");
            assert!((s.vat_in)("EUR").contains("EUR"), "{tag}: vat_in");
            let converted = (s.converted_at)("1,1626", "2026-08-07");
            assert!(converted.contains("1,1626") && converted.contains("2026-08-07"));
            assert!((s.payable_by)("2026-08-21").contains("2026-08-21"), "{tag}");
            assert!((s.payable_on_terms)(14).contains("14"), "{tag}");
            assert!(
                (s.credit_explanation)("INV-2026-00001").contains("INV-2026-00001"),
                "{tag}"
            );
            assert!(
                (s.quote_validity)("2026-09-01").contains("2026-09-01"),
                "{tag}"
            );
            assert!((s.deliver_by)("2026-09-01").contains("2026-09-01"), "{tag}");
        }
    }

    /// An order to a supplier, built from the same fixtures — the party is a
    /// supplier's, not a customer's, and the renderer never learns which.
    fn order<'a>(
        party: Party<'a>,
        issuer: &'a BillingSettings,
        lines: &'a [Line],
        totals: &'a Totals,
    ) -> PrintDocument<'a> {
        PrintDocument {
            kind: DocumentKind::PurchaseOrder,
            banner: None,
            number: Some("PO-2026-00001"),
            primary_date: Some(day(2026, 8, 10)),
            secondary_date: Some(day(2026, 8, 24)),
            reference: "Project Falkenstein",
            note: "Rear entrance.",
            currency: "CHF",
            payment_terms_days: None,
            credits_number: None,
            party,
            lines,
            totals,
            restated: None,
            issuer,
        }
    }

    /// A supplier's party, written out rather than derived from a store record
    /// — this module deliberately does not know the supplier type.
    fn supplier_party() -> Party<'static> {
        Party {
            name: "Hoffmann Möbel GmbH",
            address_line1: "Werkstraße 9",
            address_line2: "",
            postal_code: "8005",
            city: "Zürich",
            country: "CH",
            vat_id: Some("CHE116281277MWST"),
            email: Some("orders@hoffmann.test"),
        }
    }

    #[test]
    fn an_order_names_the_supplier_the_goods_and_no_bank_account() {
        let issuer = issuer();
        let lines = vec![line("Blue chair", 4000, 4_300, 1900)];
        let totals = figures(&lines);
        let html = render(
            &order(supplier_party(), &issuer, &lines, &totals),
            strings_for("en"),
        );

        assert!(html.contains("<title>Purchase order PO-2026-00001</title>"));
        // The supplier stands where a customer stands, under the order heading.
        assert!(html.contains("Order to") && !html.contains("Bill to"));
        assert!(html.contains("Hoffmann Möbel GmbH") && html.contains("Zürich"));
        // Cross-border (NL → CH), so their country is on the address.
        assert!(html.contains("<div>CH</div>"));
        // The two dates mean what an order's dates mean.
        assert!(html.contains("Order date") && html.contains("2026-08-10"));
        assert!(html.contains("Expected delivery") && html.contains("2026-08-24"));
        assert!(!html.contains("Issue date") && !html.contains("Due date"));
        // The reference is ours on an order we placed.
        assert!(html.contains("Our reference") && html.contains("Project Falkenstein"));
        // The closing block asks for goods, not money — and never prints our
        // own account: an order is not paid *to* us.
        assert!(html.contains("Delivery") && html.contains("Please deliver"));
        assert!(!html.contains("NL91 ABNA"), "{html}");
        assert!(!html.contains("Payable"), "an order owes nobody anything");
        // The money is still the server's, to the cent: 4 × 43.00 at 19%.
        assert_eq!(totals.gross_cents, 20_468);
        assert!(html.contains("CHF 204.68"));
    }

    #[test]
    fn an_order_says_when_it_is_a_draft_when_it_is_off_and_when_no_day_was_agreed() {
        let issuer = issuer();
        let lines = vec![line("Blue chair", 4000, 4_300, 1900)];
        let totals = figures(&lines);

        // A draft order carries no number, exactly like a draft invoice.
        let html = render(
            &PrintDocument {
                banner: Some(Banner::Draft),
                number: None,
                primary_date: None,
                secondary_date: None,
                ..order(supplier_party(), &issuer, &lines, &totals)
            },
            strings_for("en"),
        );
        assert!(html.contains("class=\"banner\">Draft<"));
        assert!(!html.contains("PO-2026"));
        // With no agreed day it asks for one rather than omitting delivery.
        assert!(html.contains("confirm the delivery date"));

        // A cancelled order keeps the number the supplier holds and says
        // plainly that it is off.
        let html = render(
            &PrintDocument {
                banner: Some(Banner::Cancelled),
                ..order(supplier_party(), &issuer, &lines, &totals)
            },
            strings_for("en"),
        );
        assert!(html.contains("class=\"banner\">Cancelled<"));
        assert!(html.contains("PO-2026-00001"));
    }

    #[test]
    fn a_party_is_the_eight_facts_a_document_needs_from_either_record() {
        // The customer's own record, reduced to what the paper prints — the
        // proof that generalising the party changed no billing behaviour.
        let c = customer();
        let party = Party::customer(&c);
        assert_eq!(party.name, "Kunde & Söhne <GmbH>");
        assert_eq!(party.postal_code, "10115");
        assert_eq!(party.country, "DE");
        assert_eq!(party.vat_id, Some("DE811907980"));
        assert_eq!(party.email, None);
    }
}
