//! The words the default chart of accounts is written with (alo Finance, ADR
//! 0035, wave B4.13c; `docs/design/finance.md` § The chart of accounts).
//!
//! [`alo_store::CHART`] states each default account's **code, kind and role**
//! and deliberately no name: a hardcoded English account name would be a bug in
//! a European product, and the store is where a hardcoded string would be
//! hardest to see. So the names live at the edge, one table per language, and
//! the chart a tenant is seeded with is written in the language of whoever
//! opened it first — [`crate::insights_gallery`]'s mechanism, reused whole,
//! down to the `?lang=` the client sends.
//!
//! From the moment it is written the chart is **ordinary tenant data**: a
//! tenant that disagrees with a word renames the account, and nothing here ever
//! overwrites it. That is also why a language added later cannot retranslate an
//! existing tenant's chart — the seed runs once (`fin_seeds`), and a name a
//! human may have edited is not ours to revise.
//!
//! The tables are checked against `CHART` by the tests below, so an account
//! added to the default chart cannot ship with a language missing its word.

use alo_store::{ChartName, ChartSeed};

/// The words one language writes the default chart with.
pub struct ChartWords {
    /// BCP 47 tag this table is written in.
    pub lang: &'static str,
    /// One name per [`alo_store::CHART`] entry, against the code it belongs to.
    pub names: &'static [(&'static str, &'static str)],
}

/// The default table.
static EN: ChartWords = ChartWords {
    lang: "en",
    names: &[
        ("1000", "Bank"),
        ("1010", "Cash"),
        ("1100", "Trade receivables"),
        ("1200", "VAT recoverable"),
        ("1900", "Suspense"),
        ("2000", "Trade payables"),
        ("2100", "VAT payable"),
        ("2200", "Owed to employees"),
        ("3000", "Opening balance"),
        ("3100", "Retained earnings"),
        ("4000", "Sales"),
        ("4900", "Other income"),
        ("5000", "Cost of sales"),
        ("6000", "General expenses"),
        ("6100", "Travel"),
        ("6200", "Professional fees"),
        ("6300", "Premises"),
        ("6400", "Marketing"),
        ("6900", "Rounding differences"),
        ("6950", "Exchange differences"),
    ],
};

/// The French chart.
static FR: ChartWords = ChartWords {
    lang: "fr",
    names: &[
        ("1000", "Banque"),
        ("1010", "Caisse"),
        ("1100", "Clients"),
        ("1200", "TVA déductible"),
        ("1900", "Compte d’attente"),
        ("2000", "Fournisseurs"),
        ("2100", "TVA collectée"),
        ("2200", "Personnel — notes de frais"),
        ("3000", "Bilan d’ouverture"),
        ("3100", "Report à nouveau"),
        ("4000", "Ventes"),
        ("4900", "Autres produits"),
        ("5000", "Achats"),
        ("6000", "Frais généraux"),
        ("6100", "Déplacements"),
        ("6200", "Honoraires"),
        ("6300", "Locaux"),
        ("6400", "Marketing"),
        ("6900", "Écarts d’arrondi"),
        ("6950", "Écarts de change"),
    ],
};

/// The Dutch chart.
static NL: ChartWords = ChartWords {
    lang: "nl",
    names: &[
        ("1000", "Bank"),
        ("1010", "Kas"),
        ("1100", "Debiteuren"),
        ("1200", "Te vorderen btw"),
        ("1900", "Tussenrekening"),
        ("2000", "Crediteuren"),
        ("2100", "Te betalen btw"),
        ("2200", "Te betalen aan personeel"),
        ("3000", "Openingsbalans"),
        ("3100", "Ingehouden winst"),
        ("4000", "Omzet"),
        ("4900", "Overige opbrengsten"),
        ("5000", "Kostprijs van de omzet"),
        ("6000", "Algemene kosten"),
        ("6100", "Reiskosten"),
        ("6200", "Advies- en accountantskosten"),
        ("6300", "Huisvesting"),
        ("6400", "Marketing"),
        ("6900", "Afrondingsverschillen"),
        ("6950", "Koersverschillen"),
    ],
};

/// The chart words for a language tag, falling back to the default table. The
/// primary subtag decides, so `fr-BE` and `fr` get the same chart.
#[must_use]
pub fn chart_words_for(tag: &str) -> &'static ChartWords {
    let primary = tag.split(['-', '_']).next().unwrap_or_default();
    match primary.to_ascii_lowercase().as_str() {
        "fr" => &FR,
        "nl" => &NL,
        _ => &EN,
    }
}

/// The chart a tenant is seeded with, in the caller's language.
///
/// Whose language, when the first person to open the chart is not the tenant's
/// accountant? The language of the client making that read — the only one
/// anybody is actually looking at, and the answer Insights already settled.
#[must_use]
pub fn chart_seed_for(tag: &str) -> ChartSeed {
    let words = chart_words_for(tag);
    ChartSeed {
        names: words
            .names
            .iter()
            .map(|(code, name)| ChartName {
                code: (*code).to_owned(),
                name: (*name).to_owned(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::CHART;

    /// Every table this file offers, so a language added later is checked by
    /// every test below without any of them being edited.
    const TABLES: [&ChartWords; 3] = [&EN, &FR, &NL];

    #[test]
    fn every_language_names_the_whole_chart_and_nothing_else() {
        for words in TABLES {
            assert_eq!(
                words.names.len(),
                CHART.len(),
                "{} does not name the whole chart",
                words.lang
            );
            for account in CHART {
                let named = words
                    .names
                    .iter()
                    .find(|(code, _)| *code == account.code)
                    .unwrap_or_else(|| panic!("{} has no name for {}", words.lang, account.code));
                assert!(
                    !named.1.trim().is_empty(),
                    "{} names {} with a blank",
                    words.lang,
                    account.code
                );
            }
        }
    }

    #[test]
    fn no_language_names_one_account_twice() {
        for words in TABLES {
            for (index, (code, _)) in words.names.iter().enumerate() {
                assert!(
                    !words.names[..index].iter().any(|(seen, _)| seen == code),
                    "{} names {code} twice",
                    words.lang
                );
            }
        }
    }

    #[test]
    fn the_primary_subtag_decides_and_an_unknown_language_is_english() {
        assert_eq!(chart_words_for("fr").lang, "fr");
        assert_eq!(chart_words_for("fr-BE").lang, "fr");
        assert_eq!(chart_words_for("nl_NL").lang, "nl");
        assert_eq!(chart_words_for("NL").lang, "nl");
        assert_eq!(chart_words_for("en-GB").lang, "en");
        assert_eq!(chart_words_for("").lang, "en");
        assert_eq!(chart_words_for("de").lang, "en");
    }

    #[test]
    fn the_seed_is_the_whole_chart_in_the_asked_for_language() {
        let seed = chart_seed_for("fr");
        assert_eq!(seed.names.len(), CHART.len());
        let bank = seed
            .names
            .iter()
            .find(|name| name.code == "1000")
            .unwrap_or_else(|| panic!("no 1000"));
        assert_eq!(bank.name, "Banque");
        // And the store's own validation accepts it — the seed it refuses is a
        // seed a tenant would be handed half a chart from.
        assert!(
            chart_seed_for("de")
                .names
                .iter()
                .all(|n| !n.name.is_empty())
        );
    }
}
