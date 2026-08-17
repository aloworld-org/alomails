//! Which VAT a supply carries, decided from the two countries and a VAT id
//! rather than typed into a box.
//!
//! The gap this closes, found by walking the live billing flow on 2026-08-17: the
//! rate was whatever the caller sent. A Belgian manufacturer quoting a German
//! plant would charge Belgian VAT because that is what was in the box, and
//! nothing would notice. Under-charging is money the seller owes; over-charging a
//! reverse-charge customer produces an invoice their accounts department
//! rejects. Both faults are silent, and both surface months later.
//!
//! **This decides a default; it does not impose one.** The seller may still set
//! any rate — they are the one who knows whether a customer is really a
//! business, whether the goods moved, and whether some exemption applies. What
//! they should not have to do is remember the rule unprompted.
//!
//! Deliberately narrow: the ordinary supply of goods and services between EU
//! member states, which is what this product's customers do all day. It does not
//! model distance-selling thresholds, OSS, margin schemes, second-hand goods, new
//! means of transport, or triangulation — each a real regime with its own rules,
//! none of them guessable from a country code and a VAT id. Where one applies the
//! seller sets the rate and this module stays quiet.

/// The 27 EU member states, for deciding intra-community treatment. Sorted,
/// because [`is_eu`] binary-searches it.
const EU: [&str; 27] = [
    "at", "be", "bg", "cy", "cz", "de", "dk", "ee", "es", "fi", "fr", "gr", "hr", "hu", "ie", "it",
    "lt", "lu", "lv", "mt", "nl", "pl", "pt", "ro", "se", "si", "sk",
];

/// Whether a country is in the EU VAT area for these purposes.
#[must_use]
pub fn is_eu(country: &str) -> bool {
    EU.binary_search(&country.trim().to_ascii_lowercase().as_str())
        .is_ok()
}

/// What VAT a supply attracts, and why — the reason travels with the rate,
/// because an invoice at 0% is lawful only if it says which 0% it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VatTreatment {
    /// Both parties in the same country: the seller's domestic rate.
    Domestic,
    /// A business in another member state that gave a VAT id. The customer
    /// accounts for the VAT and the invoice must say so.
    ReverseCharge,
    /// A consumer — or a business that gave no VAT id — in another member
    /// state. **Not** zero-rated: the seller charges their own domestic rate.
    ConsumerElsewhereInEu,
    /// Outside the EU: an export, outside the scope of EU VAT.
    Export,
    /// Not enough is known. The caller is told so rather than handed a
    /// plausible number.
    Unknown,
}

impl VatTreatment {
    /// The rate in basis points, given the seller's own domestic rate.
    ///
    /// `None` for [`Self::Unknown`], which is the point: a treatment nobody can
    /// determine must not resolve to a number that looks determined.
    #[must_use]
    pub const fn rate_bp(self, domestic_bp: i32) -> Option<i32> {
        match self {
            Self::Domestic | Self::ConsumerElsewhereInEu => Some(domestic_bp),
            Self::ReverseCharge | Self::Export => Some(0),
            Self::Unknown => None,
        }
    }

    /// The EN 16931 / UNTDID 5305 category code an e-invoice carries, and which
    /// `billing_einvoice_import` already reads back.
    ///
    /// `S` standard, `AE` reverse charge, `G` export. These print the same 0%
    /// and mean entirely different things, which is why the code is stored
    /// rather than inferred back from the rate.
    #[must_use]
    pub const fn category_code(self) -> Option<&'static str> {
        match self {
            Self::Domestic | Self::ConsumerElsewhereInEu => Some("S"),
            Self::ReverseCharge => Some("AE"),
            Self::Export => Some("G"),
            Self::Unknown => None,
        }
    }

    /// The sentence an invoice must carry when it charges no VAT. Article 196 of
    /// the VAT Directive requires the reverse-charge case to say so; an export
    /// says why it is out of scope. A domestic supply needs no note.
    #[must_use]
    pub const fn invoice_note(self) -> Option<&'static str> {
        match self {
            Self::ReverseCharge => Some("VAT reverse-charged - Article 196, Directive 2006/112/EC"),
            Self::Export => Some("Export - outside the scope of EU VAT"),
            Self::Domestic | Self::ConsumerElsewhereInEu | Self::Unknown => None,
        }
    }
}

/// Decides the treatment for a supply from `seller_country` to a customer in
/// `buyer_country` who gave `buyer_vat_id` (empty when they gave none).
///
/// The VAT id is read as a **claim to be a business**, not as verified. This
/// does not call VIES: validating an id is a round-trip to a service that is
/// regularly down, and a quote that fails because a tax office is offline is
/// worse than one that trusts a well-formed id and lets the seller correct it.
/// `billing_customers::normalize_vat_id` has already refused a malformed one
/// before anything reaches here.
#[must_use]
pub fn treatment(seller_country: &str, buyer_country: &str, buyer_vat_id: &str) -> VatTreatment {
    let seller = seller_country.trim().to_ascii_lowercase();
    let buyer = buyer_country.trim().to_ascii_lowercase();
    if seller.is_empty() || buyer.is_empty() {
        return VatTreatment::Unknown;
    }
    if seller == buyer {
        // Domestic whether or not a VAT id was given: a Belgian company selling
        // to a Belgian company charges Belgian VAT either way.
        return VatTreatment::Domestic;
    }
    if !is_eu(&buyer) {
        return VatTreatment::Export;
    }
    if !is_eu(&seller) {
        // A non-EU seller shipping into the EU is import VAT and customs, which
        // is not this rule. Say so rather than invent an answer.
        return VatTreatment::Unknown;
    }
    if buyer_vat_id.trim().is_empty() {
        VatTreatment::ConsumerElsewhereInEu
    } else {
        VatTreatment::ReverseCharge
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// The seller in these tests is Belgian, so 21% is the domestic rate.
    const BE: i32 = 2100;

    #[test]
    fn a_belgian_seller_charges_belgian_vat_at_home() {
        let t = treatment("BE", "BE", "BE0123456749");
        assert_eq!(t, VatTreatment::Domestic);
        assert_eq!(t.rate_bp(BE), Some(2100));
        assert_eq!(t.category_code(), Some("S"));
        assert_eq!(t.invoice_note(), None);
    }

    #[test]
    fn a_business_elsewhere_in_the_eu_is_reverse_charged_and_says_so() {
        let t = treatment("BE", "DE", "DE811907980");
        assert_eq!(t, VatTreatment::ReverseCharge);
        assert_eq!(t.rate_bp(BE), Some(0));
        assert_eq!(t.category_code(), Some("AE"));
        assert!(t.invoice_note().expect("a note").contains("196"));
    }

    #[test]
    fn no_vat_id_is_a_consumer_and_pays_the_sellers_own_rate() {
        // The expensive mistake this prevents: zero-rating everybody abroad.
        let t = treatment("BE", "FR", "");
        assert_eq!(t, VatTreatment::ConsumerElsewhereInEu);
        assert_eq!(t.rate_bp(BE), Some(2100));
        assert_eq!(t.category_code(), Some("S"));
    }

    #[test]
    fn outside_the_eu_is_an_export() {
        for country in ["CH", "GB", "US", "NO"] {
            let t = treatment("BE", country, "whatever");
            assert_eq!(t, VatTreatment::Export, "{country}");
            assert_eq!(t.rate_bp(BE), Some(0));
            assert_eq!(t.category_code(), Some("G"));
        }
    }

    #[test]
    fn a_blank_country_yields_no_number_at_all() {
        // Unknown must not collapse into a plausible rate: a guess there is
        // indistinguishable from a decision.
        for t in [treatment("", "DE", "DE1"), treatment("BE", "", "")] {
            assert_eq!(t, VatTreatment::Unknown);
            assert_eq!(t.rate_bp(BE), None);
            assert_eq!(t.category_code(), None);
        }
    }

    #[test]
    fn the_case_that_started_this_reads_correctly_now() {
        // Walking the live flow, NL, FR and CZ customers were all invoiced at
        // 21% because 21% was typed. With a VAT id they are reverse charge;
        // without one they are 21%. The rule decides now, not the box.
        for country in ["NL", "FR", "CZ"] {
            assert_eq!(
                treatment("BE", country, "NL004495445B01").rate_bp(BE),
                Some(0),
                "{country} with a VAT id"
            );
            assert_eq!(
                treatment("BE", country, "").rate_bp(BE),
                Some(2100),
                "{country} without one"
            );
        }
    }

    #[test]
    fn country_codes_are_read_however_they_are_typed() {
        assert_eq!(treatment("be", " BE ", ""), VatTreatment::Domestic);
        assert!(is_eu("Be") && is_eu("de") && !is_eu("ch"));
    }

    #[test]
    fn the_member_list_is_sorted_or_the_lookup_lies() {
        // is_eu binary-searches. An unsorted entry would report a member state
        // as outside the EU and zero-rate the supply as an export.
        let mut sorted = EU;
        sorted.sort_unstable();
        assert_eq!(EU, sorted);
        assert_eq!(EU.len(), 27);
    }
}
