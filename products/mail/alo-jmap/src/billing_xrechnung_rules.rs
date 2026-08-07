//! The **XRechnung business rules** (`BR-DE-*`) an EN 16931 invoice has to
//! satisfy on top of the European ones before it is an XRechnung (alo Billing,
//! wave B1.23).
//!
//! XRechnung is a **CIUS**: a Core Invoice Usage Specification, which may
//! narrow EN 16931 but never contradict it. It narrows in one direction only —
//! by *requiring* terms the European standard leaves optional — and that is
//! exactly why a document our Factur-X route renders happily can still be
//! refused here. Everything in [`crate::billing_einvoice_rules`] applies first
//! and unchanged; this is the additional list, and the route runs both.
//!
//! ## What it asks for that EN 16931 does not
//!
//! - **A contact desk for the seller** (BG-6): a name, a telephone number and
//!   an email address. The point is that a public authority receiving a
//!   thousand invoices can reach a human about any one of them.
//! - **Full postal addresses** for both parties — street, city and post code,
//!   where the European standard is satisfied by a country and one of the
//!   others.
//! - **A buyer reference** (BT-10). For German public administration this is
//!   the *Leitweg-ID*, the routing identifier that says which authority and
//!   which department the document belongs to; without it the document cannot
//!   be delivered, which is why it is required rather than merely useful.
//! - **A VAT identifier for the seller** (BT-31). XRechnung accepts a tax
//!   registration number (BT-32) or additional legal information (BT-33)
//!   instead; we hold neither — our `registrationNo` is a *company register*
//!   number (BT-30, the KVK/HRB/SIREN a document prints), which is a different
//!   term and would be a false claim in BT-32's place. So the rule is checked
//!   in its strictest form: state a VAT identifier.
//!
//! ## What this is not
//!
//! **It is not the KoSIT schematron**, for exactly the reason
//! [`crate::billing_einvoice_rules`] is not the CEN one: the normative
//! artefacts are XSLT, and running them means a third language in a repository
//! whose constitution allows two (`CLAUDE.md`), plus a downloaded binary
//! artefact in a public repo. This is a **hand-written subset** — the rules our
//! data model can actually violate, cited by the identifier a rejection quotes
//! back — and the golden documents in `tests/golden/` exist so that running the
//! real validator over them, once, offline, is a human's one-off check rather
//! than a standing risk. That item is recorded in `docs/autonomy/STATE.md`.
//!
//! Rules that cannot be violated by construction are deliberately absent rather
//! than asserted: BR-DE-1 (the payment-instructions group is written on every
//! document, [`crate::billing_ubl::render`]), BR-DE-17 (the type code is 380 or
//! 381, both in the permitted set), BR-DE-21 (the specification identifier is a
//! constant) and BR-DE-23 (a credit-transfer payment means always carries
//! exactly the one account identifier that made it a credit transfer).

use crate::billing_einvoice::{EInvoice, Party};
use crate::billing_einvoice_rules::Violation;

/// The least a telephone number can be and still be one (BR-DE-27).
const MIN_PHONE_CHARS: usize = 3;

/// Every XRechnung rule `invoice` breaks, in the order the specification
/// numbers them.
///
/// **Additional** to [`crate::billing_einvoice_rules::violations`], never
/// instead of it: a document has to satisfy both lists, and the route reports
/// them together so a tenant fixes its details once rather than twice.
#[must_use]
pub fn violations(invoice: &EInvoice) -> Vec<Violation> {
    let mut found = Vec::new();
    seller(&invoice.seller, &mut found);
    buyer(&invoice.buyer, &mut found);
    if invoice.buyer_reference.trim().is_empty() {
        found.push(Violation::new(
            "BR-DE-15",
            "the customer's reference is not stated: XRechnung requires it — for a German public \
             authority it is the Leitweg-ID the invoice is routed by, and for anyone else the \
             reference the customer asked to see on the document",
        ));
    }
    found
}

/// BR-DE-3 … BR-DE-8, BR-DE-16, BR-DE-27, BR-DE-28 — the seller.
fn seller(seller: &Party, found: &mut Vec<Violation>) {
    if seller.line1.trim().is_empty() {
        found.push(Violation::new(
            "BR-DE-3",
            "the seller's street address is not stated: fill in your billing details",
        ));
    }
    if seller.city.trim().is_empty() {
        found.push(Violation::new(
            "BR-DE-4",
            "the seller's city is not stated: fill in your billing details",
        ));
    }
    if seller.postal_code.trim().is_empty() {
        found.push(Violation::new(
            "BR-DE-5",
            "the seller's post code is not stated: fill in your billing details",
        ));
    }
    if seller.contact_name.trim().is_empty() {
        found.push(Violation::new(
            "BR-DE-6",
            "the seller states no contact point: fill in your billing details",
        ));
    }
    if seller.phone.trim().is_empty() {
        found.push(Violation::new(
            "BR-DE-7",
            "the seller states no telephone number: add one to your billing details — XRechnung \
             requires a number a customer can reach you on",
        ));
    } else if seller.phone.trim().chars().count() < MIN_PHONE_CHARS {
        found.push(Violation::new(
            "BR-DE-27",
            "the seller's telephone number is too short to be one",
        ));
    }
    if seller.email.trim().is_empty() {
        found.push(Violation::new(
            "BR-DE-8",
            "the seller states no email address: fill in your billing details",
        ));
    } else if !is_email(seller.email.trim()) {
        found.push(Violation::new(
            "BR-DE-28",
            "the seller's email address is not an email address",
        ));
    }
    if seller.vat_id.trim().is_empty() {
        found.push(Violation::new(
            "BR-DE-16",
            "the seller states no VAT identifier: XRechnung requires one (or a tax registration \
             number, which alo does not hold — a company register number is a different thing)",
        ));
    }
}

/// BR-DE-9, BR-DE-10, BR-DE-11 — the buyer.
fn buyer(buyer: &Party, found: &mut Vec<Violation>) {
    if buyer.line1.trim().is_empty() {
        found.push(Violation::new(
            "BR-DE-9",
            "the customer's street address is not stated",
        ));
    }
    if buyer.city.trim().is_empty() {
        found.push(Violation::new(
            "BR-DE-10",
            "the customer's city is not stated",
        ));
    }
    if buyer.postal_code.trim().is_empty() {
        found.push(Violation::new(
            "BR-DE-11",
            "the customer's post code is not stated",
        ));
    }
}

/// Whether a string is shaped like an email address (BR-DE-28).
///
/// Deliberately the weakest check that catches a real mistake — one `@`, with
/// something either side and a dot in the domain. The address a document is
/// answered on is proved by mail arriving, not by a regular expression, and a
/// stricter pattern here would refuse valid addresses a tenant actually uses.
fn is_email(value: &str) -> bool {
    match value.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !domain.contains('@')
                && !value.contains(char::is_whitespace)
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_einvoice::sample;

    /// The rule identifiers a document breaks.
    fn broken(invoice: &EInvoice) -> Vec<&'static str> {
        violations(invoice).into_iter().map(|v| v.rule).collect()
    }

    #[test]
    fn a_complete_issued_invoice_breaks_no_rule() {
        assert_eq!(violations(&sample()), Vec::new());
    }

    #[test]
    fn the_seller_that_en16931_accepts_can_still_be_refused_here() {
        // Everything EN 16931 asks of a seller, and nothing XRechnung adds:
        // this is the document our Factur-X route renders happily.
        let mut invoice = sample();
        invoice.seller.postal_code = String::new();
        invoice.seller.contact_name = String::new();
        invoice.seller.phone = String::new();
        assert_eq!(broken(&invoice), ["BR-DE-5", "BR-DE-6", "BR-DE-7"]);
        assert!(
            violations(&invoice)[2].detail.contains("billing details"),
            "the message has to say where to fix it"
        );
    }

    #[test]
    fn a_tenant_that_has_stated_nothing_is_told_every_field_it_owes() {
        let mut blank = sample();
        blank.seller.line1 = String::new();
        blank.seller.city = String::new();
        blank.seller.postal_code = String::new();
        blank.seller.contact_name = String::new();
        blank.seller.phone = String::new();
        blank.seller.email = String::new();
        blank.seller.vat_id = String::new();
        blank.buyer_reference = String::new();
        assert_eq!(
            broken(&blank),
            [
                "BR-DE-3", "BR-DE-4", "BR-DE-5", "BR-DE-6", "BR-DE-7", "BR-DE-8", "BR-DE-16",
                "BR-DE-15",
            ]
        );
    }

    #[test]
    fn a_customer_without_a_full_address_cannot_be_invoiced_this_way() {
        let mut invoice = sample();
        invoice.buyer.line1 = String::new();
        invoice.buyer.city = String::new();
        invoice.buyer.postal_code = String::new();
        assert_eq!(broken(&invoice), ["BR-DE-9", "BR-DE-10", "BR-DE-11"]);
    }

    #[test]
    fn the_leitweg_id_is_what_makes_the_document_deliverable() {
        let mut invoice = sample();
        invoice.buyer_reference = "   ".to_owned();
        assert_eq!(broken(&invoice), ["BR-DE-15"]);
        assert!(violations(&invoice)[0].detail.contains("Leitweg-ID"));
    }

    #[test]
    fn a_telephone_number_and_an_email_have_to_be_ones() {
        let mut invoice = sample();
        invoice.seller.phone = "12".to_owned();
        assert_eq!(broken(&invoice), ["BR-DE-27"]);

        let mut invoice = sample();
        invoice.seller.email = "billing.alo.test".to_owned();
        assert_eq!(broken(&invoice), ["BR-DE-28"]);
    }

    #[test]
    fn the_email_check_refuses_only_what_is_not_an_address() {
        for good in [
            "billing@alo.test",
            "a.b+c@sub.example.co.uk",
            "x@y.z",
            "büro@alo.test",
        ] {
            assert!(is_email(good), "{good} is an address");
        }
        for bad in [
            "",
            "alo.test",
            "@alo.test",
            "billing@alo",
            "billing@.test",
            "billing@alo.test.",
            "a@b@alo.test",
            "billing @alo.test",
        ] {
            assert!(!is_email(bad), "{bad} is not an address");
        }
    }

    #[test]
    fn a_company_register_number_does_not_stand_in_for_a_vat_identifier() {
        // BT-30 is not BT-32, and pretending otherwise would put a KVK number
        // in a tax-registration field on a German authority's desk.
        let mut invoice = sample();
        invoice.seller.vat_id = String::new();
        assert!(!invoice.seller.legal_id.is_empty());
        assert_eq!(broken(&invoice), ["BR-DE-16"]);
    }
}
