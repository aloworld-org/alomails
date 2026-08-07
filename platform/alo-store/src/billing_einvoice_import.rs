//! Reading somebody else's e-invoice (alo Billing, ADR 0035, wave B1.24) — the
//! EN 16931 semantic model in the inbound direction.
//!
//! B1.22 and B1.23 write our invoice down in the standard's two syntaxes; this
//! is the mirror. A supplier sends a Factur-X (CII) or XRechnung/Peppol (UBL)
//! file, and what comes out here is the document in **our** units — integer
//! cents, milli-units, basis points — ready to become a bill
//! ([`crate::billing_bills`]).
//!
//! It lives in the store rather than beside the writers in `alo-jmap` for one
//! reason: a supplier's invoice mostly arrives **by email**, and the path that
//! will one day book it from an attachment is the delivery pipeline, which has
//! no business depending on the HTTP crate. The writers are in `alo-jmap`
//! because they render from a *print* document that belongs to it; the reader
//! depends on nothing but the tree it walks.
//!
//! ## What "reading" refuses to do
//!
//! The whole module is written around one rule: **a figure we cannot represent
//! exactly is a refusal, never an approximation.** A bill that is a cent wrong
//! is worse than a bill that was not imported, because nobody looks for it
//! again. Concretely, a document is refused when:
//!
//! - a line's stated amount (BT-131) is not its quantity times its price —
//!   which is what a line-level allowance, a charge, or a price base quantity
//!   looks like from here, and none of those fit our line model;
//! - the standard's own total equations do not hold (BR-CO-13/15/16), or the
//!   VAT does not follow from the lines at the rate the document states
//!   (BR-CO-17);
//! - a line carries a VAT category we cannot express. Our lines carry a *rate*,
//!   not a category, so reverse charge (`AE`), intra-community supply (`K`),
//!   export (`G`) and exemption (`E`) all look like 0 %. Storing one as
//!   zero-rated would understate a VAT return and hide that the **buyer** owes
//!   the tax, so the document is refused with the category named. (The same
//!   data-model gap the outbound side records — `docs/autonomy/STATE.md`.)
//!
//! ## Direction
//!
//! A credit note arrives as type 381 with **positive** amounts, which is the
//! standard's convention. It is stored the way our own credit notes are: in
//! ledger direction, negative, so that a bill and the credit note against it
//! sum to zero without every later reader having to know the convention. The
//! flip happens once, in [`InboundInvoice::in_ledger_direction`], after every
//! consistency check has run on the figures exactly as the document states
//! them.

use time::{Date, Month};

use crate::billing_totals::{LineFigures, line_net_cents, totals};
use crate::billing_xml_tree::{self, Element};
use crate::error::{Result, StoreError};

/// The most bytes an uploaded e-invoice may weigh. A CII invoice with our
/// 500-line maximum is well under a megabyte; this admits a generous multiple
/// of that and refuses to read an arbitrary upload into memory.
pub const MAX_EINVOICE_BYTES: usize = 4 * 1024 * 1024;

/// Which of the two syntaxes in law the document arrived in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EInvoiceSyntax {
    /// UN/CEFACT Cross Industry Invoice — what Factur-X and ZUGFeRD carry.
    Cii,
    /// OASIS UBL 2.1 — what XRechnung and Peppol carry.
    Ubl,
}

impl EInvoiceSyntax {
    /// The value this syntax is stored and reported as.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cii => "cii",
            Self::Ubl => "ubl",
        }
    }

    /// The syntax a stored value names, or `None` when it is not one of ours.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "cii" => Some(Self::Cii),
            "ubl" => Some(Self::Ubl),
            _ => None,
        }
    }
}

/// A party as the document states it — here, always the supplier (BG-4).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InboundParty {
    /// Trading or registered name (BT-27).
    pub name: String,
    /// VAT identifier (BT-31), blank when none is stated.
    pub vat_id: String,
    /// Legal registration identifier (BT-30).
    pub legal_id: String,
    /// Address line 1 (BT-35).
    pub line1: String,
    /// Address line 2 (BT-36).
    pub line2: String,
    /// Post code (BT-38).
    pub postal_code: String,
    /// City (BT-37).
    pub city: String,
    /// ISO 3166-1 alpha-2 country code (BT-40).
    pub country: String,
    /// Electronic address (BT-34) when it is an email address.
    pub email: String,
}

/// One line of the inbound document (BG-25), in our units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundLine {
    /// Item name and description (BT-153, BT-154) joined the way our own
    /// writer split them.
    pub description: String,
    /// The unit label a person reads, translated back from the UN/ECE Rec 20
    /// code (BT-130). An unknown code is kept as written rather than guessed
    /// at.
    pub unit: String,
    /// Invoiced quantity (BT-129) in milli-units.
    pub qty_milli: i64,
    /// Item net price (BT-146) in cents.
    pub unit_price_cents: i64,
    /// Line VAT rate (BT-152) in basis points.
    pub vat_rate_bp: i32,
    /// Line net amount (BT-131) in cents, as the document states it. Kept only
    /// to check it against quantity × price; the stored line carries the
    /// quantity and the price, from which the amount follows.
    pub net_cents: i64,
}

/// The document's stated totals (BG-22), in cents.
///
/// Copied across rather than recomputed: the supplier's paper is the authority
/// on what they are charging. They are nevertheless checked against each other
/// and against the lines before anything is stored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatedTotals {
    /// Sum of line net amounts (BT-106).
    pub line_total_cents: i64,
    /// Document-level allowances (BT-107).
    pub allowance_total_cents: i64,
    /// Document-level charges (BT-108).
    pub charge_total_cents: i64,
    /// Total without VAT (BT-109).
    pub tax_exclusive_cents: i64,
    /// Total VAT (BT-110).
    pub tax_total_cents: i64,
    /// Total with VAT (BT-112).
    pub tax_inclusive_cents: i64,
    /// Paid already (BT-113).
    pub prepaid_cents: i64,
    /// Rounding amount (BT-114): the cents a supplier adds or drops to make
    /// the payable amount land on a round figure. Kept only to refuse a
    /// document that carries one — see [`InboundInvoice::checked`].
    pub rounding_cents: i64,
    /// Amount due for payment (BT-115).
    pub payable_cents: i64,
}

/// An inbound invoice or credit note, in our units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundInvoice {
    /// The syntax it arrived in.
    pub syntax: EInvoiceSyntax,
    /// Whether it is a credit note (BT-3 = 381) rather than an invoice (380).
    pub credit_note: bool,
    /// The supplier's own document number (BT-1).
    pub number: String,
    /// Issue date (BT-2).
    pub issue_date: Date,
    /// Payment due date (BT-9), when stated.
    pub due_date: Option<Date>,
    /// Document currency (BT-5).
    pub currency: String,
    /// The reference the supplier quotes for us (BT-10).
    pub buyer_reference: String,
    /// Document note (BT-22).
    pub note: String,
    /// Remittance information to quote when paying (BT-83).
    pub payment_reference: String,
    /// The account the supplier asks to be paid into (BT-84).
    pub iban: String,
    /// The supplier (BG-4).
    pub seller: InboundParty,
    /// The lines, in document order.
    pub lines: Vec<InboundLine>,
    /// The stated totals.
    pub totals: StatedTotals,
}

/// Parses an uploaded e-invoice file.
///
/// The bytes must be the **XML document itself** — a `.xml` file as a supplier
/// sends it, or the `factur-x.xml` taken out of a hybrid PDF. A PDF is
/// recognised and refused with an answer that says so, rather than with a
/// generic "not XML": handing over the invoice PDF is the obvious thing to try.
///
/// # Errors
/// [`StoreError::Validation`] with the reason, for every failure: not UTF-8,
/// not XML, not one of the two syntaxes, a mandatory term missing, a figure we
/// cannot represent, or an internal inconsistency. The message names the term
/// or the rule and **never quotes the document**, which is somebody's
/// commercial data.
pub fn parse_einvoice(bytes: &[u8]) -> Result<InboundInvoice> {
    if bytes.len() > MAX_EINVOICE_BYTES {
        return Err(StoreError::Validation(format!(
            "an e-invoice file must be at most {} MB",
            MAX_EINVOICE_BYTES / (1024 * 1024)
        )));
    }
    if bytes.starts_with(b"%PDF-") {
        return Err(StoreError::Validation(
            "this is a PDF. A hybrid invoice carries its e-invoice as an XML attachment inside \
             the PDF: upload that XML file (usually factur-x.xml or xrechnung.xml)"
                .to_owned(),
        ));
    }
    // A UTF-8 BOM is legal in front of an XML document and is not part of it.
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|_| {
        StoreError::Validation(
            "an e-invoice file must be UTF-8 text; this one is not readable as text".to_owned(),
        )
    })?;

    let root = billing_xml_tree::parse(text)?;
    let invoice = match root.name.as_str() {
        "CrossIndustryInvoice" => crate::billing_cii_read::read(&root)?,
        "Invoice" => crate::billing_ubl_read::read(&root, false)?,
        "CreditNote" => crate::billing_ubl_read::read(&root, true)?,
        _ => {
            return Err(StoreError::Validation(
                "this XML document is not an e-invoice: the standard's two forms are a CII \
                 CrossIndustryInvoice (Factur-X) and a UBL Invoice or CreditNote (XRechnung, \
                 Peppol)"
                    .to_owned(),
            ));
        }
    };
    invoice.checked()
}

impl InboundInvoice {
    /// Checks the document against itself, and returns it unchanged when it
    /// holds together.
    ///
    /// Everything here is arithmetic the standard requires of any conforming
    /// document, checked in integer cents. It runs on the figures **as stated**,
    /// before any direction flip, so a credit note is checked the way its
    /// issuer wrote it.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the rule that failed.
    fn checked(self) -> Result<Self> {
        if self.number.trim().is_empty() {
            return Err(missing("BT-1", "a document number"));
        }
        if self.currency.len() != 3 || !self.currency.chars().all(|c| c.is_ascii_uppercase()) {
            return Err(missing("BT-5", "a three-letter currency code"));
        }
        if self.seller.name.trim().is_empty() {
            return Err(missing("BT-27", "the supplier's name"));
        }
        if self.lines.is_empty() {
            return Err(StoreError::Validation(
                "BR-16: the document states no invoice lines".to_owned(),
            ));
        }

        // Every line's stated amount must be what its quantity and price say
        // it is. A mismatch is a line-level allowance, a charge, or a price
        // base quantity — none of which our line model can hold, and all of
        // which would leave the stored line worth a different amount from the
        // paper.
        let mut computed_line_total: i64 = 0;
        for (index, line) in self.lines.iter().enumerate() {
            let computed = line_net_cents(&LineFigures {
                qty_milli: line.qty_milli,
                unit_price_cents: line.unit_price_cents,
                vat_rate_bp: line.vat_rate_bp,
            });
            if computed != line.net_cents {
                return Err(StoreError::Validation(format!(
                    "line {}: the stated line amount (BT-131) is not the quantity times the net \
                     price. A line-level allowance or charge, or a price base quantity, cannot be \
                     stored as one line",
                    index + 1
                )));
            }
            computed_line_total = computed_line_total.saturating_add(computed);
        }

        let stated = self.totals;
        equals(
            "BR-CO-10",
            "the sum of the line amounts",
            computed_line_total,
            stated.line_total_cents,
        )?;
        equals(
            "BR-CO-13",
            "the total without VAT",
            stated
                .line_total_cents
                .saturating_sub(stated.allowance_total_cents)
                .saturating_add(stated.charge_total_cents),
            stated.tax_exclusive_cents,
        )?;
        equals(
            "BR-CO-15",
            "the total with VAT",
            stated
                .tax_exclusive_cents
                .saturating_add(stated.tax_total_cents),
            stated.tax_inclusive_cents,
        )?;
        // A rounding amount (BT-114) is legal and unrepresentable here: it is a
        // few cents that belong to no line and no rate, and storing the bill
        // without it would leave us paying a different figure from the one the
        // supplier asks for.
        if stated.rounding_cents != 0 {
            return Err(StoreError::Validation(
                "BT-114: this document rounds its payable amount by a few cents. alo stores a \
                 bill from its lines and its VAT, with nothing added on top, so it has to be \
                 entered by hand"
                    .to_owned(),
            ));
        }
        equals(
            "BR-CO-16",
            "the amount due for payment",
            stated
                .tax_inclusive_cents
                .saturating_sub(stated.prepaid_cents),
            stated.payable_cents,
        )?;

        // The VAT the document charges must follow from its lines at the rates
        // it states them at. Only checkable when nothing was allowed or charged
        // at document level: such an amount changes a rate's taxable base, and
        // the standard puts the rate it belongs to in a group we do not store.
        if stated.allowance_total_cents == 0 && stated.charge_total_cents == 0 {
            let figures: Vec<LineFigures> = self
                .lines
                .iter()
                .map(|line| LineFigures {
                    qty_milli: line.qty_milli,
                    unit_price_cents: line.unit_price_cents,
                    vat_rate_bp: line.vat_rate_bp,
                })
                .collect();
            equals(
                "BR-CO-14/BR-CO-17",
                "the VAT total",
                totals(&figures).vat_cents,
                stated.tax_total_cents,
            )?;
        }
        Ok(self)
    }

    /// The same document with its amounts in **ledger direction**: unchanged
    /// for an invoice, negated for a credit note.
    ///
    /// The standard carries "money goes back" in the type code and states
    /// positive amounts; our ledger carries it in the sign, so that a document
    /// and the credit note against it add up to nothing (B1.09). One flip, in
    /// one place, after every check has run.
    #[must_use]
    pub fn in_ledger_direction(mut self) -> Self {
        if !self.credit_note {
            return self;
        }
        for line in &mut self.lines {
            // The price is never flipped: a price is a price in either
            // direction, and a negative one is not storable (BR-27 forbids it
            // on the wire for the same reason).
            line.qty_milli = -line.qty_milli;
            line.net_cents = -line.net_cents;
        }
        let t = &mut self.totals;
        t.line_total_cents = -t.line_total_cents;
        t.allowance_total_cents = -t.allowance_total_cents;
        t.charge_total_cents = -t.charge_total_cents;
        t.tax_exclusive_cents = -t.tax_exclusive_cents;
        t.tax_total_cents = -t.tax_total_cents;
        t.tax_inclusive_cents = -t.tax_inclusive_cents;
        t.prepaid_cents = -t.prepaid_cents;
        t.rounding_cents = -t.rounding_cents;
        t.payable_cents = -t.payable_cents;
        self
    }
}

/// The refusal for a mandatory business term the document does not state.
fn missing(term: &str, what: &str) -> StoreError {
    StoreError::Validation(format!("{term}: the document states no {what}"))
}

/// Checks one of the standard's equations, in integer cents.
fn equals(rule: &str, what: &str, computed: i64, stated: i64) -> Result<()> {
    if computed == stated {
        return Ok(());
    }
    Err(StoreError::Validation(format!(
        "{rule}: {what} stated on the document is not what its own figures add up to \
         (off by {} cents)",
        computed.saturating_sub(stated).abs()
    )))
}

// ---- reading the standard's values ------------------------------------------

/// Reads a signed decimal into an integer scaled by `10^scale`: an amount into
/// cents (`scale` 2), a quantity into milli-units (3), a percentage into basis
/// points (2).
///
/// Deliberately strict, and deliberately not `f64`: a rate that multiplies
/// money never passes through a float. Refused are an empty value, a thousands
/// separator, a decimal comma, exponent notation, and — the one that matters —
/// **more decimal places than the target scale can hold**, unless the extra
/// digits are zeros. `0.3333` hours is a third of an hour we cannot store, and
/// silently keeping `0.333` would make the stored line worth less than the
/// paper says.
///
/// # Errors
/// [`StoreError::Validation`] naming the business term and what is wrong with
/// its value, never the value itself.
pub(crate) fn scaled(term: &str, raw: &str, scale: u32) -> Result<i64> {
    let text = raw.trim();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (whole, fraction) = match digits.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (digits, ""),
    };
    if whole.is_empty() && fraction.is_empty() {
        return Err(bad_value(term, "is not a number"));
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) || !fraction.chars().all(|c| c.is_ascii_digit()) {
        return Err(bad_value(
            term,
            "is not a plain decimal number (no separators, no exponent, and a point for the \
             decimal mark)",
        ));
    }

    let scale = scale as usize;
    let (kept, dropped) = fraction.split_at(fraction.len().min(scale));
    if dropped.chars().any(|c| c != '0') {
        return Err(bad_value(
            term,
            &format!("has more than {scale} decimal places, which cannot be stored exactly"),
        ));
    }

    let mut value: i64 = 0;
    for digit in whole.chars().chain(kept.chars()) {
        let digit = i64::from(digit as u8 - b'0');
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(digit))
            .ok_or_else(|| bad_value(term, "is too large to be a real amount"))?;
    }
    // Pad a short fraction: "12.5" at scale 2 is 1250, not 125.
    for _ in kept.len()..scale {
        value = value
            .checked_mul(10)
            .ok_or_else(|| bad_value(term, "is too large to be a real amount"))?;
    }
    Ok(if negative { -value } else { value })
}

/// Reads an amount into integer cents (BT-106 and friends).
pub(crate) fn amount(term: &str, raw: &str) -> Result<i64> {
    scaled(term, raw, 2)
}

/// Reads a quantity into milli-units (BT-129).
pub(crate) fn quantity(term: &str, raw: &str) -> Result<i64> {
    scaled(term, raw, 3)
}

/// Reads a VAT percentage into basis points (BT-152): `21.00` → 2100.
pub(crate) fn rate_bp(term: &str, raw: &str) -> Result<i32> {
    let value = scaled(term, raw, 2)?;
    i32::try_from(value).map_err(|_| bad_value(term, "is not a VAT rate"))
}

/// Reads a date written either as UN/EDIFACT format 102 (`20260807`, which CII
/// uses) or as ISO `YYYY-MM-DD` (which UBL uses).
///
/// Both spellings are accepted from either syntax: an inbound file is written
/// by a system we do not control, and refusing a date we can read perfectly
/// well would be pedantry with a business cost.
///
/// # Errors
/// [`StoreError::Validation`] naming the term whose date is unreadable.
pub(crate) fn date(term: &str, raw: &str) -> Result<Date> {
    let text = raw.trim();
    let (y, m, d) = if text.len() == 8 && text.chars().all(|c| c.is_ascii_digit()) {
        (&text[0..4], &text[4..6], &text[6..8])
    } else if text.len() == 10
        && text.as_bytes()[4] == b'-'
        && text.as_bytes()[7] == b'-'
        && text
            .char_indices()
            .all(|(i, c)| matches!(i, 4 | 7) || c.is_ascii_digit())
    {
        (&text[0..4], &text[5..7], &text[8..10])
    } else {
        return Err(bad_value(
            term,
            "is not a date of the form YYYY-MM-DD or YYYYMMDD",
        ));
    };
    let parts = (y.parse::<i32>(), m.parse::<u8>(), d.parse::<u8>());
    let (Ok(year), Ok(month), Ok(day)) = parts else {
        return Err(bad_value(term, "is not a date"));
    };
    let month = Month::try_from(month).map_err(|_| bad_value(term, "names no such month"))?;
    Date::from_calendar_date(year, month, day).map_err(|_| bad_value(term, "is not a real date"))
}

/// The refusal for a value that is present but unusable.
pub(crate) fn bad_value(term: &str, what: &str) -> StoreError {
    StoreError::Validation(format!("{term}: the value {what}"))
}

/// Checks a line's VAT category (BT-151) and refuses the ones our lines cannot
/// express.
///
/// A line carries a rate, not a category, so `S` (standard) and `Z` (zero
/// rated) are the two we can hold faithfully. `AE` reverse charge, `K`
/// intra-community supply, `G` export and `E` exemption all print 0 % and mean
/// entirely different things — and in the case of reverse charge, that **we**
/// owe the VAT rather than the supplier. Storing any of them as a zero-rated
/// line would understate a return, so the document is refused with the category
/// named and the bookkeeper can enter it by hand knowing why.
///
/// An absent category is accepted: it is the rate that drives our arithmetic,
/// and a document that states a rate without a category is readable.
///
/// # Errors
/// [`StoreError::Validation`] naming the category and the line.
pub(crate) fn category(line: usize, code: &str) -> Result<()> {
    match code.trim() {
        "" | "S" | "Z" => Ok(()),
        other => Err(StoreError::Validation(format!(
            "line {line}: VAT category {other} cannot be stored. alo holds a VAT rate on a line, \
             not a category, so reverse charge, intra-community supply, export and exemption \
             cannot be told apart from a zero rate — this bill has to be entered by hand"
        ))),
    }
}

/// The unit label a person reads, from the UN/ECE Recommendation 20 code the
/// document states (BT-130).
///
/// The mirror of the outbound mapping in `alo-jmap`'s `billing_einvoice`, and
/// deliberately smaller: it turns the codes that mapping produces, plus the
/// handful every European invoice uses, back into a word. **An unrecognised
/// code is kept as written** rather than translated into a guess — `C62` is not
/// a word a bookkeeper wants on screen, but neither is a wrong unit, and the
/// code is at least exactly what the supplier said.
///
/// `C62` ("one") is the exception: it is the standard's way of saying "a
/// countable thing with no unit", which is a blank label here.
#[must_use]
pub fn unit_label(code: &str) -> String {
    match code.trim().to_ascii_uppercase().as_str() {
        "C62" | "" => String::new(),
        "HUR" => "hour".to_owned(),
        "MIN" => "minute".to_owned(),
        "SEC" => "second".to_owned(),
        "DAY" => "day".to_owned(),
        "WEE" => "week".to_owned(),
        "MON" => "month".to_owned(),
        "ANN" => "year".to_owned(),
        "H87" => "piece".to_owned(),
        "PR" => "pair".to_owned(),
        "SET" => "set".to_owned(),
        "BX" => "box".to_owned(),
        "PK" => "pack".to_owned(),
        "KGM" => "kg".to_owned(),
        "GRM" => "g".to_owned(),
        "TNE" => "tonne".to_owned(),
        "LTR" => "litre".to_owned(),
        "MLT" => "ml".to_owned(),
        "MTR" => "m".to_owned(),
        "KMT" => "km".to_owned(),
        "CMT" => "cm".to_owned(),
        "MTK" => "m²".to_owned(),
        "MTQ" => "m³".to_owned(),
        "P1" => "%".to_owned(),
        other => other.to_owned(),
    }
}

/// Joins an item's name and description (BT-153, BT-154) back into one line
/// description — the inverse of the split the outbound mapping makes.
pub(crate) fn describe(name: &str, description: &str) -> String {
    match (name.trim(), description.trim()) {
        (name, "") => name.to_owned(),
        ("", description) => description.to_owned(),
        (name, description) => format!("{name}\n{description}"),
    }
}

/// Whether an element's `schemeID` says the identifier it carries is a VAT
/// identifier (`VA`) — CII states the scheme, UBL states a tax scheme instead.
pub(crate) fn is_vat_scheme(element: &Element) -> bool {
    element.attr("schemeID").eq_ignore_ascii_case("VA")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(message)) => message,
            other => panic!("expected a Validation refusal, got {other:?}"),
        }
    }

    #[test]
    fn amounts_quantities_and_rates_read_as_exact_integers() {
        assert_eq!(amount("BT-112", "2390.72").ok(), Some(239_072));
        assert_eq!(amount("BT-112", " 0.42 ").ok(), Some(42));
        assert_eq!(amount("BT-112", "1875").ok(), Some(187_500));
        assert_eq!(amount("BT-112", "-100.50").ok(), Some(-10_050));
        assert_eq!(
            amount("BT-112", "12.5").ok(),
            Some(1_250),
            "a short fraction pads"
        );
        assert_eq!(
            amount("BT-112", "1875.000").ok(),
            Some(187_500),
            "zeros drop"
        );
        assert_eq!(quantity("BT-129", "0.333").ok(), Some(333));
        assert_eq!(quantity("BT-129", "15").ok(), Some(15_000));
        assert_eq!(rate_bp("BT-152", "21.00").ok(), Some(2100));
        assert_eq!(rate_bp("BT-152", "0").ok(), Some(0));
        assert_eq!(rate_bp("BT-152", "5.5").ok(), Some(550));
    }

    #[test]
    fn a_figure_we_cannot_hold_exactly_is_refused_not_rounded() {
        // The one that matters: a third of a cent is not a cent, and keeping
        // the first two digits would make the stored line worth less than the
        // paper says.
        assert!(refused(amount("BT-131", "10.005")).contains("decimal places"));
        assert!(refused(quantity("BT-129", "0.3333")).contains("decimal places"));
        for bad in [
            "", " ", "1,50", "1 500.00", "1e3", "twelve", "12.3.4", "--1",
        ] {
            let message = refused(amount("BT-112", bad));
            assert!(
                message.contains("BT-112"),
                "the term is always named: {bad:?} → {message}"
            );
        }
        // An absurd number saturates nothing and panics nowhere.
        assert!(refused(amount("BT-112", &"9".repeat(30))).contains("too large"));
    }

    #[test]
    fn both_spellings_of_a_date_are_read_and_a_third_is_refused() {
        let expected = Date::from_calendar_date(2026, Month::August, 7).unwrap_or(Date::MIN);
        assert_eq!(date("BT-2", "20260807").ok(), Some(expected));
        assert_eq!(date("BT-2", "2026-08-07").ok(), Some(expected));
        for bad in [
            "07/08/2026",
            "2026-13-01",
            "2026-02-30",
            "2026-8-7",
            "",
            "yesterday",
        ] {
            assert!(refused(date("BT-2", bad)).contains("BT-2"), "{bad:?}");
        }
    }

    #[test]
    fn a_vat_category_we_cannot_express_is_refused_with_its_code() {
        assert!(category(1, "S").is_ok());
        assert!(category(1, "Z").is_ok());
        assert!(
            category(1, "").is_ok(),
            "a rate without a category is readable"
        );
        for hidden in ["AE", "K", "G", "E", "O"] {
            let message = refused(category(3, hidden));
            assert!(message.contains("line 3"), "{message}");
            assert!(message.contains(hidden), "the category is named: {message}");
            assert!(message.contains("by hand"), "{message}");
        }
    }

    #[test]
    fn a_unit_code_becomes_a_word_and_an_unknown_one_stays_itself() {
        assert_eq!(unit_label("HUR"), "hour");
        assert_eq!(unit_label("kmt"), "km");
        assert_eq!(unit_label("C62"), "", "a countable thing has no unit label");
        assert_eq!(unit_label(""), "");
        // Not a guess: the code the supplier stated, exactly.
        assert_eq!(unit_label("XPP"), "XPP");
    }

    #[test]
    fn the_item_name_and_its_description_join_the_way_they_were_split() {
        assert_eq!(describe("Consulting", ""), "Consulting");
        assert_eq!(describe("", "March, on site"), "March, on site");
        assert_eq!(
            describe("Consulting", "March, on site"),
            "Consulting\nMarch, on site"
        );
    }

    #[test]
    fn a_pdf_is_recognised_and_answered_for_what_it_is() {
        let message = refused(parse_einvoice(b"%PDF-1.7\n%\xc7\xec\x8f\xa2"));
        assert!(message.contains("PDF"), "{message}");
        assert!(message.contains("factur-x.xml"), "{message}");
    }

    #[test]
    fn a_file_that_is_xml_but_not_an_e_invoice_says_which_two_forms_exist() {
        let message = refused(parse_einvoice(b"<Order><ID>1</ID></Order>"));
        assert!(message.contains("CrossIndustryInvoice"), "{message}");
        assert!(message.contains("CreditNote"), "{message}");
    }

    #[test]
    fn an_oversized_or_unreadable_file_never_reaches_the_parser() {
        let huge = vec![b'<'; MAX_EINVOICE_BYTES + 1];
        assert!(refused(parse_einvoice(&huge)).contains("at most"));
        // Not text at all.
        assert!(refused(parse_einvoice(&[0xff, 0xfe, 0x00, 0x01])).contains("UTF-8"));
    }

    #[test]
    fn the_syntax_name_round_trips() {
        for syntax in [EInvoiceSyntax::Cii, EInvoiceSyntax::Ubl] {
            assert_eq!(EInvoiceSyntax::parse(syntax.as_str()), Some(syntax));
        }
        assert_eq!(EInvoiceSyntax::parse("json"), None);
    }
}
