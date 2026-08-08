//! The alo Projects HTTP edge's own concern (ADR 0035, wave B3) — the **words**
//! that reach a document a customer reads.
//!
//! Deliberately small, like [`crate::crm`]: the store-error map, the body
//! parser, the RFC 3339 stamp and the date helpers live in [`crate::billing`]
//! and are used rather than copied. What is genuinely this module's is the unit
//! label on an invoice line raised from a timesheet (B3.06). The store writes
//! the words it is handed and never invents one — a store that spelled `hour`
//! into a French document would be a hardcoded English string on the one
//! surface a client actually reads — so the word lives here, at the edge, in the
//! language the caller asked for with `?lang=`, exactly as
//! [`crate::billing_send`]'s covering emails and [`crate::crm`]'s first-use board
//! do.

/// The words a timesheet handoff puts on a document.
pub struct HourWords {
    /// BCP 47 tag this table is written in.
    pub lang: &'static str,
    /// The unit label of a line billed by the hour, singular — a unit label is a
    /// word beside a quantity (`7,5 h`), not a sentence, and every European
    /// invoice writes it in the singular.
    pub hour: &'static str,
}

/// The default table.
static EN: HourWords = HourWords {
    lang: "en",
    hour: "hour",
};

/// The French table.
static FR: HourWords = HourWords {
    lang: "fr",
    hour: "heure",
};

/// The Dutch table.
static NL: HourWords = HourWords {
    lang: "nl",
    hour: "uur",
};

/// The words for a language tag, falling back to the default table.
///
/// The same seam as [`crate::billing_send::mail_strings_for`] and
/// [`crate::crm::seed_words_for`]: the primary subtag decides, so `fr-BE` and
/// `fr` write the same word.
#[must_use]
pub fn hour_words_for(tag: &str) -> &'static HourWords {
    let primary = tag.split(['-', '_']).next().unwrap_or_default();
    match primary.to_ascii_lowercase().as_str() {
        "fr" => &FR,
        "nl" => &NL,
        _ => &EN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_names_the_hour_and_none_of_them_is_blank() {
        for tag in ["", "en", "fr", "nl", "de", "sw"] {
            let words = hour_words_for(tag);
            assert!(!words.hour.trim().is_empty(), "{tag}");
            // A unit label is bounded in the store; a word is nowhere near it.
            assert!(words.hour.chars().count() <= 32, "{tag}");
        }
    }

    #[test]
    fn a_region_subtag_picks_the_language_and_an_unknown_tag_falls_back() {
        for tag in ["fr", "fr-BE", "FR_ca"] {
            assert_eq!(hour_words_for(tag).lang, "fr", "{tag}");
        }
        for tag in ["nl", "nl-BE"] {
            assert_eq!(hour_words_for(tag).lang, "nl", "{tag}");
        }
        for tag in ["", "de", "en-GB", "xx"] {
            assert_eq!(hour_words_for(tag).lang, "en", "{tag}");
        }
    }
}
