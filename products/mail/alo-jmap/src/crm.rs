//! The CRM HTTP edge (alo CRM, ADR 0035, wave B2) — the one thing every
//! `/crm/*` route module shares that billing does not already own.
//!
//! Deliberately small. The store-error map, the body parser that never echoes
//! a request back, the RFC 3339 stamp and the `PATCH` helpers all live in
//! [`crate::billing`] and are *used* here rather than copied: it is a
//! store-error map, not a billing rule, and CRM is the second caller the design
//! note said would move it to a shared module the moment a third one appeared
//! (`docs/design/crm.md` § Errors). Moving it now would rename a file for no
//! behaviour, and a rename is not a contract.
//!
//! What is genuinely CRM's own is the **first-use seed**: the board a tenant is
//! given the first time somebody opens the module. The store never invents a
//! name — it writes the names it is handed ([`alo_store::crm_pipelines`]) — so
//! the words live here, at the edge, in the language the caller asked for with
//! `?lang=`, exactly as the covering emails of [`crate::billing_send`] do.

use alo_store::crm_pipelines::{PipelineSeed, StageSeed};

/// The words a tenant's first board is built from.
///
/// Five columns, in board order: three open ones, then the winning and the
/// losing column. The *order and the flags* are the design (`docs/design/crm.md`
/// § Seeding) and are the same in every language; only the words below change.
pub struct SeedWords {
    /// BCP 47 tag this table is written in.
    pub lang: &'static str,
    /// The board's own name.
    pub pipeline: &'static str,
    /// The five column headers, left to right: open, open, open, won, lost.
    pub stages: [&'static str; 5],
}

/// The default table.
static EN: SeedWords = SeedWords {
    lang: "en",
    pipeline: "Sales",
    stages: ["New", "Qualified", "Proposal", "Won", "Lost"],
};

/// The French board.
static FR: SeedWords = SeedWords {
    lang: "fr",
    pipeline: "Ventes",
    stages: ["Nouveau", "Qualifié", "Proposition", "Gagné", "Perdu"],
};

/// The Dutch board.
static NL: SeedWords = SeedWords {
    lang: "nl",
    pipeline: "Verkoop",
    stages: [
        "Nieuw",
        "Gekwalificeerd",
        "Voorstel",
        "Gewonnen",
        "Verloren",
    ],
};

/// The seed words for a language tag, falling back to the default table.
///
/// The same seam as [`crate::billing_send::mail_strings_for`]: the primary
/// subtag decides, so `fr-BE` and `fr` get the same board.
#[must_use]
pub fn seed_words_for(tag: &str) -> &'static SeedWords {
    let primary = tag.split(['-', '_']).next().unwrap_or_default();
    match primary.to_ascii_lowercase().as_str() {
        "fr" => &FR,
        "nl" => &NL,
        _ => &EN,
    }
}

/// The board a tenant is seeded with, in the caller's language.
///
/// **This answers the question B2.01 left open** — whose language names the
/// stages when the first user to open CRM is not the tenant's admin: the
/// language of the client making that first read, because it is the only one
/// anybody is actually looking at. The names are ordinary user data from that
/// moment on, so a tenant that disagrees renames them and nothing else changes.
#[must_use]
pub fn seed_for(tag: &str) -> PipelineSeed {
    let words = seed_words_for(tag);
    let flags = [
        (false, false),
        (false, false),
        (false, false),
        (true, false),
        (false, true),
    ];
    PipelineSeed {
        name: words.pipeline.to_owned(),
        stages: words
            .stages
            .iter()
            .zip(flags)
            .map(|(name, (is_won, is_lost))| StageSeed {
                name: (*name).to_owned(),
                is_won,
                is_lost,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_seeds_the_same_board_with_different_words() {
        for tag in ["", "en", "fr", "nl", "de", "sw"] {
            let seed = seed_for(tag);
            assert!(!seed.name.trim().is_empty(), "{tag}");
            assert_eq!(seed.stages.len(), 5, "{tag}");
            let won = seed.stages.iter().filter(|s| s.is_won).count();
            let lost = seed.stages.iter().filter(|s| s.is_lost).count();
            assert_eq!(
                (won, lost),
                (1, 1),
                "one winning and one losing column: {tag}"
            );
            assert!(
                seed.stages.iter().all(|s| !(s.is_won && s.is_lost)),
                "no column is both: {tag}"
            );
            assert!(
                seed.stages.iter().all(|s| !s.name.trim().is_empty()),
                "no blank header: {tag}"
            );
        }
    }

    #[test]
    fn the_flags_land_on_the_last_two_columns() {
        let seed = seed_for("fr");
        assert_eq!(seed.name, "Ventes");
        assert_eq!(seed.stages[0].name, "Nouveau");
        assert!(seed.stages[3].is_won && seed.stages[3].name == "Gagné");
        assert!(seed.stages[4].is_lost && seed.stages[4].name == "Perdu");
    }

    #[test]
    fn a_region_subtag_picks_the_language_and_an_unknown_tag_falls_back() {
        for tag in ["fr", "fr-BE", "FR_ch", "fr-CA"] {
            assert_eq!(seed_words_for(tag).lang, "fr", "{tag}");
        }
        for tag in ["nl", "nl-BE", "NL"] {
            assert_eq!(seed_words_for(tag).lang, "nl", "{tag}");
        }
        for tag in ["", "en", "en-GB", "de", "klingon", "-"] {
            assert_eq!(seed_words_for(tag).lang, "en", "{tag}");
        }
    }
}
