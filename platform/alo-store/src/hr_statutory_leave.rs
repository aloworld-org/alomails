//! The statutory minimum of paid annual leave, per country — the figure a new
//! tenant's first leave policy is seeded from (alo HR, ADR 0035, wave B6.03a;
//! `docs/design/hr.md`, "Policies").
//!
//! # A default, not advice
//!
//! **We are not this tenant's employment lawyer.** Every figure here is the
//! statutory *minimum* a member state sets, stated as working days on a
//! five-day week, with the instrument that sets it named beside it. What a
//! particular employee is owed also depends on their collective agreement,
//! their seniority, their contract and, in several states, their age — none of
//! which this table knows. The screen that shows a seeded policy says so, and
//! the policy is editable from the first minute.
//!
//! It exists because the alternative is worse: a tenant who presses nothing
//! would otherwise have a policy granting **nothing**, and a balance of zero
//! looks like an answer rather than an unanswered question.
//!
//! # Seed data, in the repo, with its source named
//!
//! No external service and no network call — the sovereignty promise forbids the
//! dependency, and a statutory figure that changes once a decade does not need a
//! feed. A country this table does not carry falls back to the European floor:
//! **four weeks**, Directive 2003/88/EC Art. 7, which binds every member state.
//!
//! Days become minutes through the working pattern that defines a full-time day
//! ([`crate::hr_leave_math::average_working_day_minutes`]), so "20 days" means
//! 9 600 minutes where a full day is eight hours and 7 600 where it is 7h36.

use crate::hr_employments::{FULL_TIME_PATTERN, PATTERN_DAYS};
use crate::hr_leave_math::average_working_day_minutes;

/// The European floor: four weeks of paid annual leave, Directive 2003/88/EC
/// Art. 7. Binding on every member state, and the answer for a country this
/// table does not carry.
pub const EU_FLOOR_WORKING_DAYS: i32 = 20;

/// One country's statutory minimum, and where the figure comes from.
#[derive(Debug, Clone, Copy)]
pub struct StatutoryLeave {
    /// ISO 3166-1 alpha-2, uppercase.
    pub country: &'static str,
    /// Working days per full leave year, on a five-day week.
    pub working_days: i32,
    /// The instrument that sets it — carried so a tenant asking "why 25?" gets
    /// an answer rather than a shrug.
    pub source: &'static str,
}

/// The table. Member states where the minimum is **above** the European floor
/// are stated with their own figure; the large markets are stated explicitly
/// even at the floor, so a reader can tell "we checked" from "we defaulted".
///
/// Figures normalised to a five-day week: several states count in *working
/// days* including Saturday (France's 30 *jours ouvrables*, Austria's 30
/// *Werktage*, Finland's 30 *arkipäivää*), which is the same amount of time as
/// 25 days on a five-day week. Converting here rather than storing the national
/// counting rule keeps one arithmetic in the balance fold.
const STATUTORY: &[StatutoryLeave] = &[
    StatutoryLeave {
        country: "AT",
        working_days: 25,
        source: "Urlaubsgesetz §2 (30 Werktage on a six-day week)",
    },
    StatutoryLeave {
        country: "BE",
        working_days: 20,
        source: "Arrêté royal du 30 mars 1967 (four weeks)",
    },
    StatutoryLeave {
        country: "DE",
        working_days: 20,
        source: "Bundesurlaubsgesetz §3 (24 Werktage on a six-day week)",
    },
    StatutoryLeave {
        country: "DK",
        working_days: 25,
        source: "Ferieloven §8 (2,08 days per month)",
    },
    StatutoryLeave {
        country: "ES",
        working_days: 22,
        source: "Estatuto de los Trabajadores art. 38 (30 calendar days)",
    },
    StatutoryLeave {
        country: "FI",
        working_days: 25,
        source: "Vuosilomalaki 162/2005 §5 (30 arkipäivää)",
    },
    StatutoryLeave {
        country: "FR",
        working_days: 25,
        source: "Code du travail L3141-3 (30 jours ouvrables)",
    },
    StatutoryLeave {
        country: "IE",
        working_days: 20,
        source: "Organisation of Working Time Act 1997 s.19 (four weeks)",
    },
    StatutoryLeave {
        country: "IT",
        working_days: 20,
        source: "D.Lgs. 66/2003 art. 10 (four weeks)",
    },
    StatutoryLeave {
        country: "LU",
        working_days: 26,
        source: "Code du travail L.233-4 (26 days since 2019)",
    },
    StatutoryLeave {
        country: "MT",
        working_days: 24,
        source: "Organisation of Working Time Regulations (192 hours)",
    },
    StatutoryLeave {
        country: "NL",
        working_days: 20,
        source: "Burgerlijk Wetboek 7:634 (four times the weekly working days)",
    },
    StatutoryLeave {
        country: "PL",
        working_days: 20,
        source: "Kodeks pracy art. 154 (20 days under ten years' service)",
    },
    StatutoryLeave {
        country: "PT",
        working_days: 22,
        source: "Código do Trabalho art. 238 (22 working days)",
    },
    StatutoryLeave {
        country: "SE",
        working_days: 25,
        source: "Semesterlagen 4 § (25 days)",
    },
];

/// The statutory minimum for `country`, in working days on a five-day week.
///
/// An unknown, blank or malformed country answers the European floor rather
/// than nothing: a tenant we have not tabulated is still in a member state, and
/// four weeks is the least any of them may set.
#[must_use]
pub fn statutory_working_days(country: &str) -> i32 {
    let wanted = country.trim().to_ascii_uppercase();
    STATUTORY
        .iter()
        .find(|entry| entry.country == wanted)
        .map_or(EU_FLOOR_WORKING_DAYS, |entry| entry.working_days)
}

/// The whole row for `country`, when the table carries it — the figure *and*
/// the instrument behind it, for the screen that explains a seeded policy.
#[must_use]
pub fn statutory_leave(country: &str) -> Option<StatutoryLeave> {
    let wanted = country.trim().to_ascii_uppercase();
    STATUTORY.iter().copied().find(|e| e.country == wanted)
}

/// The statutory minimum for `country` as the **minutes** a policy stores,
/// against the working pattern that defines a full-time day.
///
/// A policy's entitlement is a full-year figure at a full-time pattern; this is
/// that figure. Pass [`crate::hr_employments::FULL_TIME_PATTERN`] unless the
/// tenant has stated a different full week.
#[must_use]
pub fn statutory_minutes(country: &str, full_time_pattern: &[i32; PATTERN_DAYS]) -> i64 {
    let day_minutes = i64::from(average_working_day_minutes(full_time_pattern));
    i64::from(statutory_working_days(country)) * day_minutes
}

/// The seeded entitlement for a tenant, on the suite's default full-time
/// pattern — the one figure the seeding path needs.
#[must_use]
pub fn seeded_entitlement_minutes(country: &str) -> i64 {
    statutory_minutes(country, &FULL_TIME_PATTERN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hr_leave_math::ENTITLEMENT_MAX_MINUTES;

    #[test]
    fn the_table_is_well_formed_and_never_below_the_european_floor() {
        for entry in STATUTORY {
            assert_eq!(
                entry.country.to_ascii_uppercase(),
                entry.country,
                "{} is not canonical",
                entry.country
            );
            assert_eq!(entry.country.len(), 2, "{} is not alpha-2", entry.country);
            assert!(
                entry.working_days >= EU_FLOOR_WORKING_DAYS,
                "{} is below the Directive's floor",
                entry.country
            );
            assert!(entry.working_days <= 40, "{} is implausible", entry.country);
            assert!(
                !entry.source.is_empty(),
                "{} has no named source",
                entry.country
            );
        }
        // No country twice: two answers to one question.
        for (index, entry) in STATUTORY.iter().enumerate() {
            assert!(
                !STATUTORY[..index]
                    .iter()
                    .any(|e| e.country == entry.country),
                "{} appears twice",
                entry.country
            );
        }
    }

    #[test]
    fn a_country_answers_with_its_own_figure_and_its_source() {
        assert_eq!(statutory_working_days("FR"), 25);
        assert_eq!(statutory_working_days("fr"), 25, "case does not matter");
        assert_eq!(statutory_working_days(" be "), 20);
        assert_eq!(statutory_working_days("LU"), 26);
        let france = statutory_leave("FR").unwrap_or(StatutoryLeave {
            country: "",
            working_days: 0,
            source: "",
        });
        assert!(france.source.contains("L3141-3"));
        assert!(statutory_leave("ZZ").is_none());
    }

    #[test]
    fn an_untabulated_country_gets_the_directive_floor_rather_than_nothing() {
        for country in ["ZZ", "", "  ", "SI", "not-a-country"] {
            assert_eq!(
                statutory_working_days(country),
                EU_FLOOR_WORKING_DAYS,
                "{country} fell through to something other than four weeks"
            );
        }
    }

    #[test]
    fn days_become_the_minutes_a_policy_stores() {
        // Twenty eight-hour days.
        assert_eq!(seeded_entitlement_minutes("DE"), 20 * 480);
        assert_eq!(seeded_entitlement_minutes("FR"), 25 * 480);
        assert_eq!(seeded_entitlement_minutes("ZZ"), 20 * 480);
        // A seven-and-a-half-hour full-time day gives a different figure, and
        // that is the point of scaling rather than storing minutes per country.
        assert_eq!(
            statutory_minutes("FR", &[450, 450, 450, 450, 450, 0, 0]),
            25 * 450
        );
        // Every seeded figure is inside the policy bound.
        for entry in STATUTORY {
            assert!(seeded_entitlement_minutes(entry.country) <= ENTITLEMENT_MAX_MINUTES);
            assert!(seeded_entitlement_minutes(entry.country) > 0);
        }
    }
}
