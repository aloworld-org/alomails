//! The words a tenant's starting locations are written with (alo Inventory,
//! ADR 0035, wave B5.04b; `docs/design/inventory.md` § Locations).
//!
//! [`alo_store::inv_locations`] states each seeded location's **code and kind**
//! and deliberately no name: a location called `Warehouse` in a Dutch tenant is
//! a hardcoded English string in a European product, and the store is where a
//! hardcoded string would be hardest to see. So the names live at the edge, one
//! table per language, and the set a tenant is seeded with is written in the
//! language of whoever opened Inventory first —
//! [`crate::finance_chart_names`]' mechanism, reused whole, down to the `?lang=`
//! the client sends.
//!
//! From the moment they are written the locations are **ordinary tenant data**:
//! a tenant who disagrees with a word renames the place, and nothing here ever
//! overwrites it. That is also why a language added later cannot retranslate an
//! existing tenant's locations — the seed runs once (`inv_seeds`), and a name a
//! human may have edited is not ours to revise.
//!
//! `transit` has no name here because it is not seeded: a tenant shipping from
//! one room does not need one, and a tenant with two warehouses creates it and
//! names it themselves.

use alo_store::inv_locations::LocationSeed;

/// The words one language writes a tenant's starting locations with, in the
/// order [`LocationSeed`] declares them.
pub struct LocationWords {
    /// BCP 47 tag this table is written in.
    pub lang: &'static str,
    /// The one real place a tenant that ships from a single room needs.
    pub stock: &'static str,
    /// Where received goods come from.
    pub supplier: &'static str,
    /// Where delivered goods go.
    pub customer: &'static str,
    /// The counterparty of every correction and stocktake variance.
    pub adjustment: &'static str,
    /// Reserved for assembly, seeded so the day it is needed is not a
    /// migration.
    pub production: &'static str,
}

/// The default table.
static EN: LocationWords = LocationWords {
    lang: "en",
    stock: "Main warehouse",
    supplier: "Suppliers",
    customer: "Customers",
    adjustment: "Stock corrections",
    production: "Production",
};

/// The French table.
static FR: LocationWords = LocationWords {
    lang: "fr",
    stock: "Entrepôt principal",
    supplier: "Fournisseurs",
    customer: "Clients",
    adjustment: "Corrections de stock",
    production: "Production",
};

/// The Dutch table.
static NL: LocationWords = LocationWords {
    lang: "nl",
    stock: "Hoofdmagazijn",
    supplier: "Leveranciers",
    customer: "Klanten",
    adjustment: "Voorraadcorrecties",
    production: "Productie",
};

/// The location words for a language tag, falling back to the default table.
/// The primary subtag decides, so `fr-BE` and `fr` get the same words.
#[must_use]
pub fn location_words_for(tag: &str) -> &'static LocationWords {
    let primary = tag.split(['-', '_']).next().unwrap_or_default();
    match primary.to_ascii_lowercase().as_str() {
        "fr" => &FR,
        "nl" => &NL,
        _ => &EN,
    }
}

/// The locations a tenant is seeded with, in the caller's language.
#[must_use]
pub fn location_seed_for(tag: &str) -> LocationSeed {
    let words = location_words_for(tag);
    LocationSeed {
        stock: words.stock.to_owned(),
        supplier: words.supplier.to_owned(),
        customer: words.customer.to_owned(),
        adjustment: words.adjustment.to_owned(),
        production: words.production.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every table this file offers, so a language added later is checked by
    /// every test below without any of them being edited.
    const TABLES: [&LocationWords; 3] = [&EN, &FR, &NL];

    #[test]
    fn every_language_names_every_seeded_location() {
        for words in TABLES {
            let seed = location_seed_for(words.lang);
            for (field, name) in [
                ("stock", &seed.stock),
                ("supplier", &seed.supplier),
                ("customer", &seed.customer),
                ("adjustment", &seed.adjustment),
                ("production", &seed.production),
            ] {
                assert!(
                    !name.trim().is_empty(),
                    "{}: {field} has no name — the store refuses a half seed",
                    words.lang
                );
            }
        }
    }

    #[test]
    fn a_language_we_do_not_have_gets_the_default_rather_than_a_refusal() {
        for tag in ["", "de", "pt-BR", "xx", "ZZ_zz"] {
            assert_eq!(location_words_for(tag).lang, "en", "{tag}");
        }
    }

    #[test]
    fn the_primary_subtag_decides() {
        assert_eq!(location_words_for("fr-BE").lang, "fr");
        assert_eq!(location_words_for("nl_NL").lang, "nl");
        assert_eq!(location_words_for("FR").lang, "fr");
    }

    #[test]
    fn the_four_counterparties_are_distinct_words_in_every_language() {
        // Two locations sharing a name is not a constraint violation — the
        // codes differ — but it is a picker a person cannot use.
        for words in TABLES {
            let seed = location_seed_for(words.lang);
            let mut names = vec![
                seed.stock,
                seed.supplier,
                seed.customer,
                seed.adjustment,
                seed.production,
            ];
            let total = names.len();
            names.sort();
            names.dedup();
            assert_eq!(names.len(), total, "{} repeats a name", words.lang);
        }
    }
}
