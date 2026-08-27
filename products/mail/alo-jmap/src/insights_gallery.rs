//! The alo Insights **gallery** at the HTTP edge (ADR 0037, wave BI1.06) — the
//! ready-made questions a tenant pins from, and the words the zero-setup
//! Business overview is written with.
//!
//! The questions themselves live in [`alo_store::insight_overview`], built from
//! the typed model over the closed catalog. Two things are this edge's own:
//!
//! - **`GET /insights/gallery`** hands the client each entry's key, module,
//!   chart form, width and the ChartSpec itself, so pinning one is the ordinary
//!   `POST …/tiles` every tile goes through — the spec is re-validated by the
//!   write gate like any other, and the gallery gets no privileged route.
//! - **The seed's words.** The store never invents a name; it writes the ones it
//!   is handed ([`alo_store::insight_overview::OverviewSeed`]), so the language
//!   of the board a tenant is given is decided here, from `?lang=` on the first
//!   read — exactly the seam a CRM pipeline seed uses ([`crate::crm`]).
//!
//! **No English crosses the wire from the gallery route.** An entry carries its
//! key and the client translates it; hardcoded English in a European product is
//! a bug, and a chart's caption is not an exception. The seed is the one place
//! words are written, because a stored tile title is the tenant's own data from
//! the moment it exists — renameable, and never re-translated behind their back.

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use serde_json::{Value, json};

use alo_store::insight_overview::{BUSINESS_OVERVIEW, GALLERY, OverviewCaption, OverviewSeed};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The words one language writes the seeded overview with.
pub struct SeedWords {
    /// BCP 47 tag this table is written in.
    pub lang: &'static str,
    /// The board's own name.
    pub board: &'static str,
    /// A caption per key in [`BUSINESS_OVERVIEW`]. Checked against that layout
    /// by the tests below, so a tile added to the overview cannot ship with a
    /// language missing its words.
    pub captions: &'static [(&'static str, &'static str)],
}

/// The default table.
static EN: SeedWords = SeedWords {
    lang: "en",
    board: "Business overview",
    captions: &[
        ("outstanding", "Outstanding"),
        ("won_this_month", "Won this month"),
        ("revenue_by_month", "Revenue by month"),
        ("overdue_aging", "Overdue by age"),
        ("pipeline_by_stage", "Pipeline by stage"),
        ("vat_by_quarter", "VAT by quarter"),
        ("win_rate_by_quarter", "Win rate by quarter"),
    ],
};

/// The French board.
static FR: SeedWords = SeedWords {
    lang: "fr",
    board: "Aperçu de l’activité",
    captions: &[
        ("outstanding", "Créances en cours"),
        ("won_this_month", "Gagné ce mois-ci"),
        ("revenue_by_month", "Chiffre d’affaires par mois"),
        ("overdue_aging", "Retards par ancienneté"),
        ("pipeline_by_stage", "Pipeline par étape"),
        ("vat_by_quarter", "TVA par trimestre"),
        ("win_rate_by_quarter", "Taux de réussite par trimestre"),
    ],
};

/// The Dutch board.
static NL: SeedWords = SeedWords {
    lang: "nl",
    board: "Bedrijfsoverzicht",
    captions: &[
        ("outstanding", "Openstaand"),
        ("won_this_month", "Gewonnen deze maand"),
        ("revenue_by_month", "Omzet per maand"),
        ("overdue_aging", "Achterstand per ouderdom"),
        ("pipeline_by_stage", "Pipeline per fase"),
        ("vat_by_quarter", "Btw per kwartaal"),
        ("win_rate_by_quarter", "Winstpercentage per kwartaal"),
    ],
};

/// The German board. These captions must say the same words as the web
/// catalog's `insightsGallery*` titles (`web/src/i18n/de.ts`) — the gallery
/// offers the same charts, and a seeded tile that disagrees with the gallery
/// caption looks like a different chart.
static DE: SeedWords = SeedWords {
    lang: "de",
    board: "Geschäftsübersicht",
    captions: &[
        ("outstanding", "Offene Forderungen"),
        ("won_this_month", "Diesen Monat gewonnen"),
        ("revenue_by_month", "Umsatz nach Monat"),
        ("overdue_aging", "Überfällig nach Alter"),
        ("pipeline_by_stage", "Pipeline nach Phase"),
        ("vat_by_quarter", "MwSt. nach Quartal"),
        ("win_rate_by_quarter", "Erfolgsquote nach Quartal"),
    ],
};

/// The seed words for a language tag, falling back to the default table. The
/// primary subtag decides, so `fr-BE` and `fr` get the same board.
#[must_use]
pub fn seed_words_for(tag: &str) -> &'static SeedWords {
    let primary = tag.split(['-', '_']).next().unwrap_or_default();
    match primary.to_ascii_lowercase().as_str() {
        "fr" => &FR,
        "nl" => &NL,
        "de" => &DE,
        _ => &EN,
    }
}

/// The overview a tenant is seeded with, in the caller's language.
///
/// Whose language, when the first person to open Insights is not the tenant's
/// admin? The language of the client making that read — the only one anybody is
/// actually looking at, and the answer CRM already settled. The captions are
/// ordinary user data from that moment on, so a tenant that disagrees renames
/// them and nothing else changes.
#[must_use]
pub fn overview_seed_for(tag: &str) -> OverviewSeed {
    let words = seed_words_for(tag);
    OverviewSeed {
        name: words.board.to_owned(),
        captions: words
            .captions
            .iter()
            .map(|(key, title)| OverviewCaption {
                key: (*key).to_owned(),
                title: (*title).to_owned(),
            })
            .collect(),
    }
}

/// One gallery entry as JSON — its key, where it belongs, how it draws, how
/// wide it wants to sit, and the question itself.
///
/// The spec travels with the entry so that pinning it is one ordinary request:
/// the client sends it straight back to `POST /insights/dashboards/{id}/tiles`,
/// where the write gate validates it exactly as it validates a spec a builder
/// or a model produced. The gallery is a set of good defaults, never a
/// privileged path into the store.
fn entry_json(entry: &alo_store::GalleryEntry) -> Value {
    json!({
        "key": entry.key,
        "module": entry.module,
        "viz": entry.viz(),
        "span": entry.span,
        "spec": entry.spec().to_value().ok(),
    })
}

/// `GET /insights/gallery` → `{"entries":[…]}` — the prebuilt questions, and
/// which of them the Business overview is built from.
///
/// Authenticated like every other Insights route even though the answer is the
/// same for every tenant: it is part of the product surface, not a public
/// catalogue, and an unauthenticated route here would be the one door in the
/// module that opens without a key.
pub async fn list_gallery(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    authenticate(&state, &headers).await?;
    Ok(Json(json!({
        "entries": GALLERY.iter().map(entry_json).collect::<Vec<_>>(),
        "overview": BUSINESS_OVERVIEW,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::gallery_entry;

    #[test]
    fn every_language_writes_the_whole_overview() {
        for words in [&EN, &FR, &NL, &DE] {
            assert!(!words.board.trim().is_empty(), "{}", words.lang);
            assert_eq!(
                words.captions.len(),
                BUSINESS_OVERVIEW.len(),
                "{} does not caption the whole overview",
                words.lang
            );
            for key in BUSINESS_OVERVIEW {
                let caption = words
                    .captions
                    .iter()
                    .find(|(k, _)| k == key)
                    .unwrap_or_else(|| panic!("{} has no caption for {key}", words.lang));
                assert!(
                    !caption.1.trim().is_empty(),
                    "{} leaves {key} blank",
                    words.lang
                );
            }
        }
    }

    #[test]
    fn a_region_subtag_picks_the_language_and_an_unknown_tag_falls_back() {
        for tag in ["fr", "fr-BE", "FR_ch"] {
            assert_eq!(seed_words_for(tag).lang, "fr", "{tag}");
        }
        for tag in ["nl", "nl-BE", "NL"] {
            assert_eq!(seed_words_for(tag).lang, "nl", "{tag}");
        }
        for tag in ["de", "de-AT", "DE_ch"] {
            assert_eq!(seed_words_for(tag).lang, "de", "{tag}");
        }
        for tag in ["", "en", "en-GB", "klingon", "-"] {
            assert_eq!(seed_words_for(tag).lang, "en", "{tag}");
        }
    }

    #[test]
    fn the_seed_the_store_receives_is_the_layout_it_asks_for() {
        let seed = overview_seed_for("fr");
        assert_eq!(seed.name, "Aperçu de l’activité");
        assert_eq!(seed.captions.len(), BUSINESS_OVERVIEW.len());
        for key in BUSINESS_OVERVIEW {
            assert!(
                seed.captions.iter().any(|c| c.key == *key),
                "{key} has no caption"
            );
        }
    }

    /// The wire shape the client builds its gallery from: a key it translates,
    /// never a word we chose for it, and a spec it can pin unchanged.
    #[test]
    fn an_entry_crosses_as_a_key_a_shape_and_the_question_itself() {
        let entry = gallery_entry("revenue_by_month").unwrap_or_else(|| {
            panic!("the gallery lost its lead question");
        });
        let wire = entry_json(entry);
        assert_eq!(wire["key"], "revenue_by_month");
        assert_eq!(wire["module"], "billing");
        assert_eq!(wire["viz"], "bar");
        assert_eq!(wire["span"], 2);
        assert_eq!(wire["spec"]["dataset"], "billing.documents");
        assert_eq!(wire["spec"]["measure"]["id"], "net");
        assert!(
            wire.as_object().is_some_and(|o| !o.contains_key("title")),
            "no English crosses the wire: the client translates the key"
        );
    }
}
