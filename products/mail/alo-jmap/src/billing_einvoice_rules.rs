//! The **EN 16931 business rules** an invoice has to satisfy before it is an
//! e-invoice at all (alo Billing, wave B1.22).
//!
//! The standard is not only a list of fields; it is a list of *rules* over
//! them, each with an identifier a receiving system quotes back at you when it
//! refuses a document (`BR-06`, `BR-CO-15`, `BR-S-09`). This module checks the
//! ones our own model can break, cited by identifier, and it runs in **two**
//! places for one reason each:
//!
//! - **On the route**, before anything is rendered. A tenant that has not
//!   filled in its own country cannot produce a legal e-invoice, and the
//!   useful answer to that is "BR-09: the seller's country is not stated", not
//!   an XML file that a customer's system rejects a week later.
//! - **In the tests**, over every golden document. A rule that only ever runs
//!   in production is a rule nobody has seen pass.
//!
//! ## What this is not
//!
//! **It is not the official schematron.** The normative artefacts (the CEN
//! schematron for EN 16931 and the Factur-X/XRechnung ones on top of it) are
//! XSLT, and running them means an XSLT processor — a third language in a repo
//! whose constitution allows two (`CLAUDE.md`), and a downloaded binary
//! artefact in a public repository. So this is a **hand-written subset**: the
//! rules our data model can actually violate, with the official identifiers
//! where they are cited, and nothing invented that pretends to be a rule of
//! the standard. Running the normative schematron in CI stays an open item for
//! a human, recorded in `docs/autonomy/STATE.md` — this checker narrows what
//! that run could ever find, it does not replace it.
//!
//! Rules that **cannot** be violated by construction are deliberately absent
//! rather than asserted: BR-01 (the specification identifier is a constant),
//! BR-21/22/23/24/26 (every line carries an id, a quantity, a unit code, a net
//! amount and a price because the type has no way not to), and the BR-DEC
//! family (every amount is integer cents, so two decimals is the only thing
//! the formatter can produce — pinned by the golden files instead).

use crate::billing_einvoice::{EInvoice, TypeCode, VatCategory};

/// One rule an invoice breaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// The rule identifier as EN 16931 states it, e.g. `BR-CO-15`.
    pub rule: &'static str,
    /// What is wrong, in the words the person who has to fix it needs.
    pub detail: String,
}

impl Violation {
    /// A broken rule, cited by the identifier a receiving system quotes.
    ///
    /// Public because the national rule sets are their own modules — XRechnung
    /// ([`crate::billing_xrechnung_rules`]) reports in exactly this shape, and
    /// a route reports both lists as one.
    pub fn new(rule: &'static str, detail: impl Into<String>) -> Self {
        Self {
            rule,
            detail: detail.into(),
        }
    }
}

/// Every rule `invoice` breaks, in the order the standard numbers them.
///
/// An empty result means the document can be expressed as an e-invoice, not
/// that it is beyond dispute: the subset is stated in the module
/// documentation.
#[must_use]
pub fn violations(invoice: &EInvoice) -> Vec<Violation> {
    let mut found = Vec::new();
    identity(invoice, &mut found);
    parties(invoice, &mut found);
    lines(invoice, &mut found);
    totals(invoice, &mut found);
    categories(invoice, &mut found);
    found
}

/// BR-02, BR-03, BR-05, BR-53 — what the document is.
fn identity(invoice: &EInvoice, found: &mut Vec<Violation>) {
    if invoice.number.trim().is_empty() {
        found.push(Violation::new(
            "BR-02",
            "the document has no invoice number: a draft is not an e-invoice until it is issued",
        ));
    }
    if invoice.issue_date.is_none() {
        found.push(Violation::new(
            "BR-03",
            "the document has no issue date: a draft is not an e-invoice until it is issued",
        ));
    }
    if invoice.currency.len() != 3 || !invoice.currency.bytes().all(|b| b.is_ascii_uppercase()) {
        found.push(Violation::new(
            "BR-05",
            "the invoice currency is not a three-letter ISO 4217 code",
        ));
    }
    // BR-53: an invoice that states a VAT accounting currency has to state the
    // VAT total in it. The two travel together in the model, so this can only
    // fail if that ever stops being true.
    if let Some(tax_currency) = &invoice.tax_currency
        && (tax_currency.code.len() != 3
            || !tax_currency.code.bytes().all(|b| b.is_ascii_uppercase()))
    {
        found.push(Violation::new(
            "BR-53",
            "the VAT accounting currency is not a three-letter ISO 4217 code",
        ));
    }
}

/// BR-06 … BR-11, BR-CO-09, BR-CO-26 — who the two parties are.
fn parties(invoice: &EInvoice, found: &mut Vec<Violation>) {
    if invoice.seller.name.trim().is_empty() {
        found.push(Violation::new(
            "BR-06",
            "the seller's name is not stated: fill in your billing details",
        ));
    }
    if invoice.buyer.name.trim().is_empty() {
        found.push(Violation::new("BR-07", "the customer's name is not stated"));
    }
    if invoice.seller.city.trim().is_empty() && invoice.seller.line1.trim().is_empty() {
        found.push(Violation::new(
            "BR-08",
            "the seller has no postal address: fill in your billing details",
        ));
    }
    if !is_country_code(&invoice.seller.country) {
        found.push(Violation::new(
            "BR-09",
            "the seller's country is not a two-letter ISO 3166-1 code",
        ));
    }
    if invoice.buyer.city.trim().is_empty() && invoice.buyer.line1.trim().is_empty() {
        found.push(Violation::new(
            "BR-10",
            "the customer has no postal address",
        ));
    }
    if !is_country_code(&invoice.buyer.country) {
        found.push(Violation::new(
            "BR-11",
            "the customer's country is not a two-letter ISO 3166-1 code",
        ));
    }
    // BR-CO-09: a VAT identifier is prefixed with the country that issued it.
    // Checked on both parties, and only when one is stated — a B2C customer
    // legitimately has none.
    for (rule, who, vat_id) in [
        ("BR-CO-09", "seller", &invoice.seller.vat_id),
        ("BR-CO-09", "customer", &invoice.buyer.vat_id),
    ] {
        if !vat_id.is_empty() && !vat_id.bytes().take(2).all(|b| b.is_ascii_alphabetic()) {
            found.push(Violation::new(
                rule,
                format!("the {who}'s VAT identifier is not prefixed with a country code"),
            ));
        }
    }
    // BR-CO-26: a buyer has to be able to identify the supplier from the
    // document alone.
    if invoice.seller.vat_id.trim().is_empty() && invoice.seller.legal_id.trim().is_empty() {
        found.push(Violation::new(
            "BR-CO-26",
            "the seller states neither a VAT identifier nor a legal registration number",
        ));
    }
}

/// BR-16, BR-25, BR-27 — the lines.
fn lines(invoice: &EInvoice, found: &mut Vec<Violation>) {
    if invoice.lines.is_empty() {
        found.push(Violation::new("BR-16", "the document has no invoice lines"));
    }
    for line in &invoice.lines {
        if line.name.trim().is_empty() {
            found.push(Violation::new(
                "BR-25",
                format!("line {} has no item name", line.id),
            ));
        }
        if line.unit_price_cents < 0 {
            found.push(Violation::new(
                "BR-27",
                format!(
                    "line {} has a negative unit price; credit with a credit note rather than a negative price",
                    line.id
                ),
            ));
        }
    }
}

/// BR-CO-10, BR-CO-13, BR-CO-14, BR-CO-15, BR-CO-16, BR-CO-18, BR-CO-25 — the
/// arithmetic that has to hold for the document to be readable as money.
fn totals(invoice: &EInvoice, found: &mut Vec<Violation>) {
    let line_sum: i64 = invoice
        .lines
        .iter()
        .fold(0i64, |sum, line| sum.saturating_add(line.net_cents));
    if line_sum != invoice.line_total_cents {
        found.push(Violation::new(
            "BR-CO-10",
            "the sum of the line net amounts is not the document's net total",
        ));
    }
    if invoice.tax_basis_cents != invoice.line_total_cents {
        found.push(Violation::new(
            "BR-CO-13",
            "the total without VAT is not the sum of the line net amounts",
        ));
    }
    let tax_sum: i64 = invoice
        .vat_breakdown
        .iter()
        .fold(0i64, |sum, group| sum.saturating_add(group.tax_cents));
    if tax_sum != invoice.tax_total_cents {
        found.push(Violation::new(
            "BR-CO-14",
            "the VAT total is not the sum of the VAT breakdown",
        ));
    }
    if invoice.grand_total_cents
        != invoice
            .tax_basis_cents
            .saturating_add(invoice.tax_total_cents)
    {
        found.push(Violation::new(
            "BR-CO-15",
            "the total with VAT is not the total without VAT plus the VAT total",
        ));
    }
    if invoice.due_payable_cents != invoice.grand_total_cents {
        found.push(Violation::new(
            "BR-CO-16",
            "the amount due for payment is not the total with VAT",
        ));
    }
    if invoice.vat_breakdown.is_empty() {
        found.push(Violation::new(
            "BR-CO-18",
            "the document has no VAT breakdown",
        ));
    }
    // BR-CO-25: money is owed, so the document has to say by when — either a
    // due date or terms in words. A credit note owes nothing and is exempt by
    // the rule's own wording (it applies when the amount due is positive).
    if invoice.type_code == TypeCode::Invoice
        && invoice.due_payable_cents > 0
        && invoice.due_date.is_none()
        && invoice.payment_terms.trim().is_empty()
    {
        found.push(Violation::new(
            "BR-CO-25",
            "money is due but the document states neither a due date nor payment terms",
        ));
    }
}

/// BR-CO-17, BR-S-02, BR-S-05, BR-S-08, BR-S-09, BR-Z-05, BR-Z-08, BR-Z-09 —
/// the VAT categories and what each breakdown group has to add up to.
fn categories(invoice: &EInvoice, found: &mut Vec<Violation>) {
    let standard_rated = invoice
        .lines
        .iter()
        .any(|line| line.category == VatCategory::Standard);
    if standard_rated && invoice.seller.vat_id.trim().is_empty() {
        found.push(Violation::new(
            "BR-S-02",
            "the document charges VAT but the seller states no VAT identifier",
        ));
    }
    for line in &invoice.lines {
        match line.category {
            VatCategory::Standard if line.rate_bp <= 0 => found.push(Violation::new(
                "BR-S-05",
                format!("line {} is standard-rated at a rate of zero", line.id),
            )),
            VatCategory::Zero if line.rate_bp != 0 => found.push(Violation::new(
                "BR-Z-05",
                format!("line {} is zero-rated at a rate above zero", line.id),
            )),
            _ => {}
        }
    }
    for group in &invoice.vat_breakdown {
        let taxable: i64 = invoice
            .lines
            .iter()
            .filter(|line| line.category == group.category && line.rate_bp == group.rate_bp)
            .fold(0i64, |sum, line| sum.saturating_add(line.net_cents));
        let (basis_rule, amount_rule) = match group.category {
            VatCategory::Standard => ("BR-S-08", "BR-S-09"),
            VatCategory::Zero => ("BR-Z-08", "BR-Z-09"),
        };
        if taxable != group.taxable_cents {
            found.push(Violation::new(
                basis_rule,
                format!(
                    "the taxable amount at {} is not the sum of the lines taxed at it",
                    rate_text(group.rate_bp)
                ),
            ));
        }
        if group.category == VatCategory::Zero && group.tax_cents != 0 {
            found.push(Violation::new(
                amount_rule,
                "a zero-rated group carries a VAT amount",
            ));
            continue;
        }
        // BR-CO-17 (and BR-S-09, which states it for the standard rate): the
        // VAT of a group is its taxable amount at its rate, rounded to two
        // decimals. A cent of tolerance, because "rounded" does not name a
        // direction and half-up and half-away-from-zero differ on a negative
        // subtotal — the tolerance the normative schematron itself allows.
        let expected = i64::try_from(
            (i128::from(group.taxable_cents) * i128::from(group.rate_bp) + 5_000) / 10_000,
        )
        .unwrap_or(i64::MAX);
        if (group.tax_cents - expected).abs() > 1 {
            found.push(Violation::new(
                if group.category == VatCategory::Standard {
                    amount_rule
                } else {
                    "BR-CO-17"
                },
                format!(
                    "the VAT at {} is not its taxable amount at that rate",
                    rate_text(group.rate_bp)
                ),
            ));
        }
    }
}

/// Whether a country field holds a two-letter ISO 3166-1 alpha-2 code.
fn is_country_code(value: &str) -> bool {
    value.len() == 2 && value.bytes().all(|b| b.is_ascii_uppercase())
}

/// A rate in basis points as a percentage, for a message a person reads.
fn rate_text(rate_bp: i32) -> String {
    format!("{}.{:02} %", rate_bp / 100, (rate_bp % 100).abs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_einvoice::{TaxCurrency, sample};

    /// The rule identifiers a document breaks.
    fn broken(invoice: &EInvoice) -> Vec<&'static str> {
        violations(invoice).into_iter().map(|v| v.rule).collect()
    }

    #[test]
    fn a_complete_issued_invoice_breaks_no_rule() {
        assert_eq!(violations(&sample()), Vec::new());
    }

    #[test]
    fn a_draft_is_refused_by_the_two_rules_that_make_it_a_draft() {
        let mut draft = sample();
        draft.number = String::new();
        draft.issue_date = None;
        assert_eq!(broken(&draft), ["BR-02", "BR-03"]);
    }

    #[test]
    fn a_tenant_that_has_not_stated_its_own_identity_is_told_which_field() {
        // The unstated issuer: exactly what a tenant that has never opened the
        // billing settings looks like.
        let mut blank = sample();
        blank.seller.name = String::new();
        blank.seller.line1 = String::new();
        blank.seller.city = String::new();
        blank.seller.country = String::new();
        blank.seller.vat_id = String::new();
        blank.seller.legal_id = String::new();
        assert_eq!(
            broken(&blank),
            ["BR-06", "BR-08", "BR-09", "BR-CO-26", "BR-S-02"]
        );
        assert!(
            violations(&blank)[0].detail.contains("billing details"),
            "the message has to say where to fix it"
        );
    }

    #[test]
    fn a_customer_without_an_address_cannot_be_invoiced_electronically() {
        let mut invoice = sample();
        invoice.buyer.name = String::new();
        invoice.buyer.line1 = String::new();
        invoice.buyer.city = String::new();
        invoice.buyer.country = "Germany".to_owned();
        assert_eq!(broken(&invoice), ["BR-07", "BR-10", "BR-11"]);
    }

    #[test]
    fn a_vat_identifier_without_its_country_prefix_is_refused_on_either_party() {
        let mut invoice = sample();
        invoice.seller.vat_id = "812345678B01".to_owned();
        invoice.buyer.vat_id = "811907980".to_owned();
        assert_eq!(broken(&invoice), ["BR-CO-09", "BR-CO-09"]);
    }

    #[test]
    fn totals_that_do_not_add_up_are_each_caught_by_their_own_rule() {
        let mut invoice = sample();
        invoice.line_total_cents += 1;
        assert_eq!(broken(&invoice), ["BR-CO-10", "BR-CO-13"]);

        let mut invoice = sample();
        invoice.tax_basis_cents += 100;
        assert_eq!(broken(&invoice), ["BR-CO-13", "BR-CO-15"]);

        let mut invoice = sample();
        invoice.tax_total_cents += 100;
        assert_eq!(broken(&invoice), ["BR-CO-14", "BR-CO-15"]);

        let mut invoice = sample();
        invoice.due_payable_cents -= 1;
        assert_eq!(broken(&invoice), ["BR-CO-16"]);
    }

    #[test]
    fn a_document_with_no_lines_and_no_breakdown_is_not_an_invoice() {
        let mut empty = sample();
        empty.lines.clear();
        empty.vat_breakdown.clear();
        empty.line_total_cents = 0;
        empty.tax_basis_cents = 0;
        empty.tax_total_cents = 0;
        empty.grand_total_cents = 0;
        empty.due_payable_cents = 0;
        assert_eq!(broken(&empty), ["BR-16", "BR-CO-18"]);
    }

    #[test]
    fn a_line_that_credits_by_pricing_negatively_is_refused() {
        // BR-27 is the reason our credit notes negate the quantity and never
        // the price — the standard forbids the other spelling outright.
        let mut invoice = sample();
        invoice.lines[0].unit_price_cents = -12_500;
        assert!(broken(&invoice).contains(&"BR-27"));
        assert!(
            violations(&invoice)[0].detail.contains("credit note"),
            "the message has to say what to do instead"
        );
    }

    #[test]
    fn an_item_without_a_name_is_refused() {
        let mut invoice = sample();
        invoice.lines[0].name = "  ".to_owned();
        assert_eq!(broken(&invoice), ["BR-25"]);
    }

    #[test]
    fn a_breakdown_group_has_to_be_the_lines_it_claims_to_group() {
        let mut invoice = sample();
        invoice.vat_breakdown[0].taxable_cents += 1_000;
        // The group no longer matches its lines, and its VAT no longer matches
        // the group — two rules, both true.
        assert_eq!(broken(&invoice), ["BR-S-08", "BR-S-09"]);
    }

    #[test]
    fn a_zero_rated_group_may_not_carry_vat_and_a_rated_one_may_not_be_zero() {
        let mut zero = sample();
        zero.lines[0].category = VatCategory::Zero;
        zero.lines[0].rate_bp = 0;
        zero.vat_breakdown[0].category = VatCategory::Zero;
        zero.vat_breakdown[0].rate_bp = 0;
        // The VAT amount is still the standard-rated one, which is exactly the
        // mistake BR-Z-09 exists for.
        assert!(broken(&zero).contains(&"BR-Z-09"));

        let mut rated = sample();
        rated.lines[0].rate_bp = 0;
        assert!(broken(&rated).contains(&"BR-S-05"));
    }

    #[test]
    fn a_credit_note_needs_no_due_date_but_an_invoice_does() {
        let mut invoice = sample();
        invoice.due_date = None;
        invoice.payment_terms = String::new();
        assert_eq!(broken(&invoice), ["BR-CO-25"]);
        // Terms in words satisfy the rule on their own.
        invoice.payment_terms = "Payable within 14 days.".to_owned();
        assert_eq!(violations(&invoice), Vec::new());
        // A credit note owes nothing, so neither is required of it.
        let mut credit = sample();
        credit.type_code = TypeCode::CreditNote;
        credit.due_date = None;
        credit.payment_terms = String::new();
        assert_eq!(violations(&credit), Vec::new());
    }

    #[test]
    fn a_foreign_currency_document_states_its_accounting_currency_properly() {
        let mut invoice = sample();
        invoice.currency = "usd".to_owned();
        invoice.tax_currency = Some(TaxCurrency {
            code: "EU".to_owned(),
            tax_cents: 3_000,
        });
        assert_eq!(broken(&invoice), ["BR-05", "BR-53"]);
    }
}
