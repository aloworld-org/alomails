//! The **EN 16931 semantic invoice** (alo Billing, ADR 0035, wave B1.22) — our
//! document expressed in the European standard's terms, before any syntax.
//!
//! EN 16931 separates two things that are usually confused. The **semantic
//! model** is a list of business terms (BT-1 the invoice number, BT-112 the
//! total with VAT, …) and the rules those terms must satisfy; the **syntax
//! binding** is how they are written down. There are two bindings in law —
//! UN/CEFACT CII (Factur-X, [`crate::billing_cii`], B1.22) and OASIS UBL
//! (XRechnung, B1.23) — and they are the *same invoice* twice.
//!
//! So this module is the invoice, and it renders nothing. It exists because
//! the alternative — mapping our store's records straight into two dialects of
//! XML — would put the decisions below (what a credit note's sign means, what
//! `hour` is in UN/ECE Rec 20, which VAT category a 0 % line is) in two places
//! that would drift.
//!
//! ## The decisions this mapping makes
//!
//! - **A credit note is issued in credit direction, not in negatives.** Our
//!   store mirrors an invoice by negating its quantities, so a stored credit
//!   note is a document of negative amounts (`docs/design/billing.md`, B1.09).
//!   EN 16931 carries the direction in the type code instead — 381 *is* "money
//!   goes back" — and receiving systems overwhelmingly expect a 381 whose
//!   amounts are positive. Every quantity and amount is therefore multiplied by
//!   −1 for a credit note, which leaves a partial credit's internal structure
//!   intact (a mixed-sign draft stays mixed, flipped) rather than taking
//!   absolute values line by line. Flagged for human review in
//!   `docs/autonomy/STATE.md`: the standard does not forbid the other reading.
//! - **The e-invoice states the document, not its settlement.** Payments
//!   recorded against the invoice (B1.19) are deliberately *not* BT-113
//!   "paid amount", so BT-115 amount due equals BT-112 total with VAT. It is
//!   the same figure the paper carries, and an e-invoice whose amount due
//!   moved every time a payment landed would contradict the copy the customer
//!   already has.
//! - **Only two VAT categories are expressible**: standard-rated (`S`) for any
//!   positive rate, zero-rated (`Z`) for 0 %. A line carries a *rate*, not a
//!   category, so reverse charge (`AE`), intra-community supply (`K`), export
//!   (`G`) and exemption (`E`) — all of which also print 0 % but mean
//!   different things and require an exemption reason — cannot be told apart.
//!   That is a data-model gap recorded for a human, not something to guess at:
//!   labelling an intra-community supply `Z` would understate somebody's
//!   return.
//! - **The unit is a code, not the label the user typed.** BT-130 is mandatory
//!   and comes from UN/ECE Recommendation 20; [`unit_code`] maps the labels a
//!   European price list actually uses, and anything unrecognised becomes
//!   `C62` ("one"), the code for a countable thing without a unit.

use time::Date;

use crate::billing_print::{DocumentKind, PrintDocument, Strings};

/// The specification this document claims to follow (BT-24) — the EN 16931
/// core invoice model itself, not a national CIUS of it.
pub const SPECIFICATION_ID: &str = "urn:cen.eu:en16931:2017";

/// What kind of document this is (BT-3), as the UNTDID 1001 code a receiving
/// system switches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeCode {
    /// Commercial invoice (380): money is owed.
    Invoice,
    /// Credit note (381): money goes back.
    CreditNote,
}

impl TypeCode {
    /// The UNTDID 1001 code.
    #[must_use]
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Invoice => "380",
            Self::CreditNote => "381",
        }
    }
}

/// A VAT category (BT-151 on a line, BT-118 in the breakdown), as the UNTDID
/// 5305 code.
///
/// Two of the standard's eight, because a rate is all our lines carry — see
/// the module documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VatCategory {
    /// Standard rate (`S`): VAT is charged at a positive rate.
    Standard,
    /// Zero rated (`Z`): the supply is taxable at 0 %.
    Zero,
}

impl VatCategory {
    /// The UNTDID 5305 code.
    #[must_use]
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Standard => "S",
            Self::Zero => "Z",
        }
    }

    /// The category a line at `rate_bp` basis points falls in.
    #[must_use]
    pub fn of_rate(rate_bp: i32) -> Self {
        if rate_bp == 0 {
            Self::Zero
        } else {
            Self::Standard
        }
    }
}

/// A party to the invoice (BG-4 seller, BG-7 buyer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Party {
    /// Trading name (BT-27 / BT-44).
    pub name: String,
    /// Address line 1 (BT-35 / BT-50).
    pub line1: String,
    /// Address line 2 (BT-36 / BT-51).
    pub line2: String,
    /// Post code (BT-38 / BT-53).
    pub postal_code: String,
    /// City (BT-37 / BT-52).
    pub city: String,
    /// ISO 3166-1 alpha-2 country code (BT-40 / BT-55).
    pub country: String,
    /// VAT identifier (BT-31 / BT-48), blank when the party has none.
    pub vat_id: String,
    /// Legal registration identifier (BT-30), seller only.
    pub legal_id: String,
    /// Electronic address (BT-34 / BT-49) — an email address here.
    pub email: String,
    /// Contact point (BT-41 / BT-56): the person or desk a question about the
    /// document goes to. We hold no contact *person* for either party, so the
    /// seller names itself — the company is the desk — and the buyer, of whom
    /// no national rule asks a contact, states none.
    pub contact_name: String,
    /// Contact telephone (BT-42 / BT-57), blank when none is stated.
    ///
    /// Optional in EN 16931 and **mandatory in XRechnung** (BR-DE-7), which is
    /// why a term the CII rendering never writes is on the model at all.
    pub phone: String,
}

/// Where the money should go (BG-16 / BG-17), when the issuer has stated a bank
/// account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreditTransfer {
    /// Account identifier (BT-84).
    pub iban: String,
    /// Account name (BT-85).
    pub holder: String,
    /// Servicing bank identifier (BT-86).
    pub bic: String,
}

/// One invoice line (BG-25).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EInvoiceLine {
    /// Line identifier (BT-126) — the position, one-based.
    pub id: String,
    /// Item name (BT-153): the first line of the description, since the name
    /// is a name and a paragraph belongs in BT-154.
    pub name: String,
    /// Item description (BT-154): whatever else was typed, blank when the
    /// description was a single line.
    pub description: String,
    /// Invoiced quantity (BT-129) in milli-units, in credit direction.
    pub qty_milli: i64,
    /// Unit of measure (BT-130), a UN/ECE Rec 20 code.
    pub unit_code: &'static str,
    /// Item net price (BT-146) in cents — never negated, since a price is a
    /// price whichever direction the document runs.
    pub unit_price_cents: i64,
    /// Line net amount (BT-131) in cents, in credit direction.
    pub net_cents: i64,
    /// Line VAT category (BT-151).
    pub category: VatCategory,
    /// Line VAT rate (BT-152) in basis points.
    pub rate_bp: i32,
}

/// One VAT breakdown group (BG-23) — one per category and rate present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VatBreakdown {
    /// Category code (BT-118).
    pub category: VatCategory,
    /// Rate (BT-119) in basis points.
    pub rate_bp: i32,
    /// Taxable amount (BT-116) in cents, in credit direction.
    pub taxable_cents: i64,
    /// VAT amount (BT-117) in cents, in credit direction.
    pub tax_cents: i64,
}

/// The VAT total restated in the issuer's accounting currency (BT-6 + BT-111),
/// present only when the document was raised in another currency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaxCurrency {
    /// ISO 4217 code the issuer keeps books in (BT-6).
    pub code: String,
    /// The document's whole VAT in that currency (BT-111), in cents, in credit
    /// direction.
    pub tax_cents: i64,
}

/// An invoice in the standard's own terms.
///
/// Every amount is integer cents and every quantity milli-units, exactly as
/// the store holds them: the syntax bindings format, and formatting is the
/// only place a decimal point exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EInvoice {
    /// Invoice number (BT-1); blank on a draft, which is why a draft is not a
    /// valid e-invoice ([`crate::billing_einvoice_rules`]).
    pub number: String,
    /// Invoice type code (BT-3).
    pub type_code: TypeCode,
    /// Issue date (BT-2); absent on a draft.
    pub issue_date: Option<Date>,
    /// Payment due date (BT-9).
    pub due_date: Option<Date>,
    /// Invoice currency (BT-5).
    pub currency: String,
    /// VAT accounting currency and the VAT stated in it (BT-6, BT-111).
    pub tax_currency: Option<TaxCurrency>,
    /// Buyer reference (BT-10) — the customer's own reference for the document.
    pub buyer_reference: String,
    /// Invoice note (BT-22).
    pub note: String,
    /// Payment terms (BT-20), in the words the paper prints.
    pub payment_terms: String,
    /// Preceding invoice reference (BT-25): the number a credit note corrects.
    pub preceding_invoice: String,
    /// The seller (BG-4).
    pub seller: Party,
    /// The buyer (BG-7).
    pub buyer: Party,
    /// Credit-transfer instructions (BG-17), when a bank account is stated.
    pub credit_transfer: Option<CreditTransfer>,
    /// The lines (BG-25), in print order.
    pub lines: Vec<EInvoiceLine>,
    /// The VAT breakdown (BG-23), ascending by rate.
    pub vat_breakdown: Vec<VatBreakdown>,
    /// Sum of line net amounts (BT-106) in cents.
    pub line_total_cents: i64,
    /// Total without VAT (BT-109) in cents. Equal to BT-106: our documents
    /// carry no document-level allowances or charges.
    pub tax_basis_cents: i64,
    /// Total VAT (BT-110) in cents.
    pub tax_total_cents: i64,
    /// Total with VAT (BT-112) in cents.
    pub grand_total_cents: i64,
    /// Amount due for payment (BT-115) in cents.
    pub due_payable_cents: i64,
}

impl EInvoice {
    /// The document expressed in the standard's terms, or `None` when it is
    /// not an invoice at all.
    ///
    /// A **quote** is the `None`: an offer is not a document EN 16931 knows,
    /// and the honest answer to "give me the e-invoice for this quote" is that
    /// there is not one, not an invoice that says 380.
    ///
    /// Total for everything else, including a draft. What a draft *lacks* — a
    /// number, an issue date — is caught by the rules
    /// ([`crate::billing_einvoice_rules::violations`]) and reported as the
    /// business rules it breaks, which is a far better answer than "not
    /// found": it is the same answer the receiving system would give.
    #[must_use]
    pub fn from_document(doc: &PrintDocument<'_>, s: &Strings) -> Option<Self> {
        let type_code = match doc.kind {
            DocumentKind::Invoice => TypeCode::Invoice,
            DocumentKind::CreditNote => TypeCode::CreditNote,
            // Neither an offer nor an order we placed is an e-invoice of
            // ours: EN 16931 describes a bill from a seller to a buyer, and on
            // a purchase order the bill that follows is the *supplier's*
            // (B1.24 is where one of those is read).
            DocumentKind::Quote | DocumentKind::PurchaseOrder => return None,
        };
        // Credit direction: a stored credit note is the mirror of its
        // original, and the standard carries that mirroring in BT-3 instead.
        let sign: i64 = if type_code == TypeCode::CreditNote {
            -1
        } else {
            1
        };

        let lines = doc
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let (name, description) = split_description(&line.description);
                EInvoiceLine {
                    id: (index + 1).to_string(),
                    name,
                    description,
                    qty_milli: line.qty_milli.saturating_mul(sign),
                    unit_code: unit_code(&line.unit),
                    unit_price_cents: line.unit_price_cents,
                    net_cents: line.net_cents().saturating_mul(sign),
                    category: VatCategory::of_rate(line.vat_rate_bp),
                    rate_bp: line.vat_rate_bp,
                }
            })
            .collect();

        let vat_breakdown = doc
            .totals
            .vat_by_rate
            .iter()
            .map(|subtotal| VatBreakdown {
                category: VatCategory::of_rate(subtotal.rate_bp),
                rate_bp: subtotal.rate_bp,
                taxable_cents: subtotal.net_cents.saturating_mul(sign),
                tax_cents: subtotal.vat_cents.saturating_mul(sign),
            })
            .collect();

        let net = doc.totals.net_cents.saturating_mul(sign);
        let gross = doc.totals.gross_cents.saturating_mul(sign);
        Some(Self {
            number: doc.number.unwrap_or_default().to_owned(),
            type_code,
            issue_date: doc.primary_date,
            due_date: doc.secondary_date,
            currency: doc.currency.to_owned(),
            tax_currency: doc.restated.as_ref().map(|restated| TaxCurrency {
                code: restated.currency.clone(),
                tax_cents: restated.vat_cents.saturating_mul(sign),
            }),
            buyer_reference: doc.reference.to_owned(),
            note: doc.note.to_owned(),
            payment_terms: payment_terms(doc, s),
            preceding_invoice: doc.credits_number.unwrap_or_default().to_owned(),
            seller: Party {
                name: doc.issuer.legal_name.clone(),
                line1: doc.issuer.address_line1.clone(),
                line2: doc.issuer.address_line2.clone(),
                postal_code: doc.issuer.postal_code.clone(),
                city: doc.issuer.city.clone(),
                country: doc.issuer.country.clone(),
                vat_id: doc.issuer.vat_id.clone().unwrap_or_default(),
                legal_id: doc.issuer.registration_no.clone(),
                email: doc.issuer.email.clone(),
                // The company is its own contact desk: the billing settings
                // hold a billing address and telephone, not a named person.
                contact_name: doc.issuer.legal_name.clone(),
                phone: doc.issuer.phone.clone(),
            },
            buyer: Party {
                name: doc.party.name.to_owned(),
                line1: doc.party.address_line1.to_owned(),
                line2: doc.party.address_line2.to_owned(),
                postal_code: doc.party.postal_code.to_owned(),
                city: doc.party.city.to_owned(),
                country: doc.party.country.to_owned(),
                vat_id: doc.party.vat_id.unwrap_or_default().to_owned(),
                legal_id: String::new(),
                email: doc.party.email.unwrap_or_default().to_owned(),
                contact_name: String::new(),
                phone: String::new(),
            },
            credit_transfer: doc.issuer.iban.as_ref().map(|iban| CreditTransfer {
                iban: iban.clone(),
                holder: doc.issuer.effective_account_holder().to_owned(),
                bic: doc.issuer.bic.clone().unwrap_or_default(),
            }),
            lines,
            vat_breakdown,
            line_total_cents: net,
            tax_basis_cents: net,
            tax_total_cents: doc.totals.vat_cents.saturating_mul(sign),
            grand_total_cents: gross,
            due_payable_cents: gross,
        })
    }
}

/// The payment terms text (BT-20): the sentence the paper prints under the
/// totals, so the two documents ask for the money in the same words.
fn payment_terms(doc: &PrintDocument<'_>, s: &Strings) -> String {
    match (doc.kind, doc.secondary_date, doc.payment_terms_days) {
        (DocumentKind::CreditNote, _, _) => String::new(),
        (_, Some(due), _) => (s.payable_by)(&crate::billing_print::date(due)),
        (_, None, Some(days)) => (s.payable_on_terms)(days),
        (_, None, None) => String::new(),
    }
}

/// Splits a line's description into the item name (BT-153) and the rest
/// (BT-154).
///
/// A name is a name: the standard's own guidance is that BT-153 identifies the
/// item, and a three-paragraph specification pasted into a description would
/// make an unusable one. So the first line names the item and everything after
/// it is the description — and a single-line description, which is what almost
/// every line is, produces exactly what was typed and no BT-154 at all.
fn split_description(description: &str) -> (String, String) {
    let trimmed = description.trim();
    match trimmed.split_once('\n') {
        Some((first, rest)) => (first.trim_end().to_owned(), rest.trim().to_owned()),
        None => (trimmed.to_owned(), String::new()),
    }
}

/// The UN/ECE Recommendation 20 code for a unit label (BT-130).
///
/// BT-130 is mandatory and coded, while our price list holds whatever the user
/// typed. The table below is the labels a European price list actually uses,
/// in the three languages the product ships in, plus the symbols; matching
/// ignores case, surrounding space and a trailing plural `s`, because `Hours`
/// and `hour` are the same unit.
///
/// **Anything unrecognised becomes `C62`** — "one", the code for a countable
/// item with no unit of measure. That is the honest default: it says "a
/// number of things", which is exactly what an unlabelled quantity is, and it
/// never claims the wrong dimension. The label the user typed still prints on
/// the paper; it is only the code that is generalised.
#[must_use]
pub fn unit_code(label: &str) -> &'static str {
    let lowered = label.trim().to_lowercase();
    let singular = lowered.strip_suffix('s').unwrap_or(&lowered);
    match singular {
        "hour" | "hr" | "h" | "heure" | "uur" | "stunde" | "std" => "HUR",
        "minute" | "min" => "MIN",
        "second" | "sec" => "SEC",
        "day" | "jour" | "dag" | "tag" | "d" => "DAY",
        "week" | "semaine" | "weel" | "woche" => "WEE",
        "month" | "mois" | "maand" | "monat" => "MON",
        "year" | "an" | "année" | "jaar" | "jahr" => "ANN",
        "piece" | "pièce" | "pc" | "pcs" | "stuk" | "stück" | "stk" | "item" | "unit" | "unité"
        | "eenheid" => "H87",
        "pair" | "paire" | "paar" => "PR",
        "set" | "kit" => "SET",
        "box" | "boîte" | "doo" | "doos" | "karton" => "BX",
        "pack" | "packet" | "paquet" | "pak" => "PK",
        "kilogram" | "kilo" | "kg" => "KGM",
        "gram" | "gramme" | "g" => "GRM",
        "tonne" | "ton" | "t" => "TNE",
        "litre" | "liter" | "l" => "LTR",
        "millilitre" | "milliliter" | "ml" => "MLT",
        "metre" | "meter" | "mètre" | "m" => "MTR",
        "kilometre" | "kilometer" | "km" => "KMT",
        "centimetre" | "centimeter" | "cm" => "CMT",
        "m2" | "m²" | "sqm" | "square metre" | "square meter" => "MTK",
        "m3" | "m³" | "cbm" | "cubic metre" | "cubic meter" => "MTQ",
        "percent" | "%" => "P1",
        // "one": a countable thing with no unit — an unlabelled line, and the
        // fallback for a label we do not recognise.
        _ => "C62",
    }
}

/// A complete, rule-valid invoice, for the tests of every module that consumes
/// one — the rules ([`crate::billing_einvoice_rules`]) and the CII rendering
/// ([`crate::billing_cii`]).
///
/// It lives here, with the model, so those tests each start from the *same*
/// valid document and state only what they change about it: a rules test that
/// built its own "valid" invoice would be testing its own fixture.
#[cfg(test)]
pub(crate) fn sample() -> EInvoice {
    use time::Month;

    let day = |y, m: u8, d| {
        Date::from_calendar_date(y, Month::try_from(m).unwrap_or(Month::January), d)
            .unwrap_or(Date::MIN)
    };
    EInvoice {
        number: "INV-2026-00001".to_owned(),
        type_code: TypeCode::Invoice,
        issue_date: Some(day(2026, 8, 7)),
        due_date: Some(day(2026, 8, 21)),
        currency: "EUR".to_owned(),
        tax_currency: None,
        buyer_reference: "PO-42".to_owned(),
        note: "Thank you.".to_owned(),
        payment_terms: "Payable by 2026-08-21.".to_owned(),
        preceding_invoice: String::new(),
        seller: Party {
            name: "Alo Werkplaats B.V.".to_owned(),
            line1: "Keizersgracht 1".to_owned(),
            line2: String::new(),
            postal_code: "1015 CJ".to_owned(),
            city: "Amsterdam".to_owned(),
            country: "NL".to_owned(),
            vat_id: "NL812345678B01".to_owned(),
            legal_id: "KVK 90123456".to_owned(),
            email: "billing@alo.test".to_owned(),
            contact_name: "Alo Werkplaats B.V.".to_owned(),
            phone: "+31 20 123 4567".to_owned(),
        },
        buyer: Party {
            name: "Kunde & Söhne GmbH".to_owned(),
            line1: "Hauptstraße 5".to_owned(),
            line2: String::new(),
            postal_code: "10115".to_owned(),
            city: "Berlin".to_owned(),
            country: "DE".to_owned(),
            vat_id: "DE811907980".to_owned(),
            legal_id: String::new(),
            email: "einkauf@kunde.test".to_owned(),
            contact_name: String::new(),
            phone: String::new(),
        },
        credit_transfer: Some(CreditTransfer {
            iban: "NL91ABNA0417164300".to_owned(),
            holder: "Alo Werkplaats B.V.".to_owned(),
            bic: "ABNANL2A".to_owned(),
        }),
        lines: vec![EInvoiceLine {
            id: "1".to_owned(),
            name: "Consulting".to_owned(),
            description: String::new(),
            qty_milli: 1_500,
            unit_code: "HUR",
            unit_price_cents: 12_500,
            net_cents: 18_750,
            category: VatCategory::Standard,
            rate_bp: 2100,
        }],
        vat_breakdown: vec![VatBreakdown {
            category: VatCategory::Standard,
            rate_bp: 2100,
            taxable_cents: 18_750,
            tax_cents: 3_938,
        }],
        line_total_cents: 18_750,
        tax_basis_cents: 18_750,
        tax_total_cents: 3_938,
        grand_total_cents: 22_688,
        due_payable_cents: 22_688,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use alo_store::billing_settings::BillingSettings;
    use alo_store::billing_totals::{LineFigures, Totals, totals};
    use alo_store::{BillingCustomerId, BillingLineId, Customer, Line};
    use time::{Month, OffsetDateTime};

    use crate::billing_print::{Party as PrintParty, strings_for};

    fn day(year: i32, month: u8, day: u8) -> Date {
        Date::from_calendar_date(year, Month::try_from(month).unwrap_or(Month::January), day)
            .unwrap_or(Date::MIN)
    }

    fn customer() -> Customer {
        Customer {
            id: BillingCustomerId::new("cus-1".to_owned()),
            name: "Kunde & Söhne GmbH".to_owned(),
            address_line1: "Hauptstraße 5".to_owned(),
            address_line2: String::new(),
            postal_code: "10115".to_owned(),
            city: "Berlin".to_owned(),
            country: "DE".to_owned(),
            vat_id: Some("DE811907980".to_owned()),
            email: Some("einkauf@kunde.test".to_owned()),
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
            phone: "+31 20 123 4567".to_owned(),
            iban: Some("NL91ABNA0417164300".to_owned()),
            bic: Some("ABNANL2A".to_owned()),
            ..Default::default()
        }
    }

    /// Lines from `(qty_milli, unit_price_cents, vat_rate_bp)` triples, with
    /// the totals the store would compute for them.
    fn lines(rows: &[(i64, i64, i32)]) -> (Vec<Line>, Totals) {
        let lines: Vec<Line> = rows
            .iter()
            .enumerate()
            .map(|(index, (qty, price, rate))| Line {
                id: BillingLineId::new(format!("l-{index}")),
                line_order: i32::try_from(index).unwrap_or_default(),
                description: "Consulting".to_owned(),
                unit: "hour".to_owned(),
                qty_milli: *qty,
                unit_price_cents: *price,
                vat_rate_bp: *rate,
            })
            .collect();
        let totals = totals(
            &lines
                .iter()
                .map(|l| LineFigures {
                    qty_milli: l.qty_milli,
                    unit_price_cents: l.unit_price_cents,
                    vat_rate_bp: l.vat_rate_bp,
                })
                .collect::<Vec<_>>(),
        );
        (lines, totals)
    }

    fn document<'a>(
        kind: DocumentKind,
        customer: &'a Customer,
        issuer: &'a BillingSettings,
        lines: &'a [Line],
        totals: &'a Totals,
    ) -> PrintDocument<'a> {
        PrintDocument {
            kind,
            banner: None,
            number: Some("INV-2026-00001"),
            primary_date: Some(day(2026, 8, 7)),
            secondary_date: Some(day(2026, 8, 21)),
            reference: "PO-42",
            note: "Thank you.",
            currency: "EUR",
            payment_terms_days: Some(14),
            credits_number: None,
            party: PrintParty::customer(customer),
            lines,
            totals,
            restated: None,
            issuer,
        }
    }

    /// The document as an e-invoice, which every document here is.
    fn einvoice(doc: &PrintDocument<'_>) -> EInvoice {
        EInvoice::from_document(doc, strings_for("en"))
            .unwrap_or_else(|| panic!("the document has no e-invoice"))
    }

    #[test]
    fn a_credit_note_is_issued_in_credit_direction_with_a_positive_face() {
        let (customer, issuer) = (customer(), issuer());
        let (rows, totals) = lines(&[(-2_000, 12_500, 2100)]);
        let doc = document(DocumentKind::CreditNote, &customer, &issuer, &rows, &totals);
        let e = einvoice(&doc);
        assert_eq!(e.type_code.as_code(), "381");
        // The stored document is −250.00 net; the e-invoice states 250.00 and
        // says "credit note" in BT-3.
        assert_eq!(e.line_total_cents, 25_000);
        assert_eq!(e.grand_total_cents, 30_250);
        assert_eq!(e.due_payable_cents, 30_250);
        assert_eq!(e.lines[0].qty_milli, 2_000);
        assert_eq!(e.lines[0].net_cents, 25_000);
        // …and the price is never flipped: a price is a price in either
        // direction, and BR-27 forbids a negative one.
        assert_eq!(e.lines[0].unit_price_cents, 12_500);
        assert_eq!(e.vat_breakdown[0].taxable_cents, 25_000);
        assert_eq!(e.vat_breakdown[0].tax_cents, 5_250);
        // A credit note asks for nothing, so it states no payment terms.
        assert_eq!(e.payment_terms, "");
    }

    #[test]
    fn an_invoice_states_the_document_not_what_has_been_paid_against_it() {
        let (customer, issuer) = (customer(), issuer());
        let (rows, totals) = lines(&[(1_000, 10_000, 2100)]);
        let doc = document(DocumentKind::Invoice, &customer, &issuer, &rows, &totals);
        let e = einvoice(&doc);
        assert_eq!(e.type_code.as_code(), "380");
        assert_eq!(e.line_total_cents, 10_000);
        assert_eq!(e.tax_basis_cents, 10_000);
        assert_eq!(e.tax_total_cents, 2_100);
        assert_eq!(e.grand_total_cents, 12_100);
        // BT-115 is BT-112: the payments ledger is not part of the document.
        assert_eq!(e.due_payable_cents, e.grand_total_cents);
    }

    #[test]
    fn a_quote_is_not_an_invoice_and_says_so() {
        let (customer, issuer) = (customer(), issuer());
        let (rows, totals) = lines(&[(1_000, 10_000, 2100)]);
        let doc = document(DocumentKind::Quote, &customer, &issuer, &rows, &totals);
        assert!(EInvoice::from_document(&doc, strings_for("en")).is_none());
    }

    #[test]
    fn a_zero_rated_line_is_category_z_and_a_taxed_one_is_category_s() {
        let (customer, issuer) = (customer(), issuer());
        let (rows, totals) = lines(&[(1_000, 10_000, 0), (1_000, 5_000, 2100)]);
        let doc = document(DocumentKind::Invoice, &customer, &issuer, &rows, &totals);
        let e = einvoice(&doc);
        assert_eq!(e.lines[0].category.as_code(), "Z");
        assert_eq!(e.lines[1].category.as_code(), "S");
        // One breakdown group per rate, ascending, as the store groups them.
        assert_eq!(e.vat_breakdown.len(), 2);
        assert_eq!(e.vat_breakdown[0].rate_bp, 0);
        assert_eq!(e.vat_breakdown[0].tax_cents, 0);
        assert_eq!(e.vat_breakdown[1].rate_bp, 2100);
    }

    #[test]
    fn the_unit_a_user_typed_becomes_the_code_the_standard_names() {
        for (label, code) in [
            ("hour", "HUR"),
            ("Hours", "HUR"),
            ("  h ", "HUR"),
            ("uur", "HUR"),
            ("day", "DAY"),
            ("month", "MON"),
            ("pcs", "H87"),
            ("stuks", "H87"),
            ("kg", "KGM"),
            ("km", "KMT"),
            ("m²", "MTK"),
            ("litres", "LTR"),
            ("%", "P1"),
            ("", "C62"),
            ("licence", "C62"),
            ("whatever the user typed", "C62"),
        ] {
            assert_eq!(unit_code(label), code, "unit {label:?}");
        }
    }

    #[test]
    fn a_multi_line_description_names_the_item_and_describes_it_separately() {
        assert_eq!(
            split_description("Consulting"),
            ("Consulting".to_owned(), String::new())
        );
        assert_eq!(
            split_description("Consulting\nMarch, on site\nTwo engineers"),
            (
                "Consulting".to_owned(),
                "March, on site\nTwo engineers".to_owned()
            )
        );
        assert_eq!(split_description("  \n "), (String::new(), String::new()));
    }
}
