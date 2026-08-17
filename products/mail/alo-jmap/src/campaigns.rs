//! The `/campaigns/*` HTTP edge (alo Campaigns, ADR 0044, wave C1) — the
//! conventions the audience, consent, suppression and segment routes share, so
//! four modules answer a caller in one dialect rather than four.
//!
//! Three of them are decisions rather than plumbing, and they are why this file
//! exists at all:
//!
//! - **A segment is asked as query parameters, never stored to be counted.**
//!   [`SegmentConditions`] arrives on the URL of every read that narrows the
//!   audience, because the screen shows the count moving as the question is
//!   *being* refined — a segment that had to be saved before it could be
//!   counted would make every experiment a stored object somebody has to tidy
//!   up afterwards. A saved segment is the same conditions read back from
//!   `GET /campaigns/segments/{id}`, so there is one counting path and not two.
//! - **Nothing here sends, and nothing here is a list of people to send to.**
//!   The routes below read who could be reached and why; generating a message
//!   and putting it on the wire is C2, which waits on a second IP that has to
//!   be bought (ADR 0044 §1). A route that produced a send is out of this
//!   wave's scope, not merely unbuilt.
//! - **Every parameter is read as text and judged here.** A typed
//!   `Query<T>` rejection answers in axum's shape rather than in our
//!   [`Problem`], so `limit=banana` would reach the caller as a different kind
//!   of error from `limit=9000`. Both are the caller's `422`, and both say
//!   which parameter.
//!
//! The one thing this module must never grow is a query of its own. Who may be
//! mailed is decided by `alo_store::campaign_audience`'s CTEs — the consent
//! join, the suppression exclusion and the promise that the per-user address
//! book is never a source are properties of that SQL. An edge that assembled
//! its own filter would be a second place each of those could be forgotten.

use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{
    AUDIENCE_PAGE_MAX, AudiencePage, PurchaseCondition, PurchaseWindow, SegmentConditions,
    normalise_address,
};

use crate::error::Problem;

/// Trims a parameter and treats a blank one as absent — a screen whose select
/// is on "all" sends an empty value rather than omitting the key.
pub(crate) fn stated(raw: Option<&str>) -> Option<&str> {
    raw.map(str::trim).filter(|value| !value.is_empty())
}

/// The `422` a parameter earns, naming the parameter and the rule.
///
/// Verbatim and specific, per `docs/design/ux-principles.md`: "invalid request"
/// tells the person at the screen nothing they can act on, and this surface is
/// read by a colleague building a question, not only by a program.
pub(crate) fn unprocessable(detail: impl Into<String>) -> Problem {
    Problem::with(StatusCode::UNPROCESSABLE_ENTITY, detail)
}

/// Reads an address a route is *about* — the path segment of a consent history
/// or a suppression lookup.
///
/// Judged here so that a request for `not-an-address` is a `422` naming the
/// mistake rather than a `404` that reads as "we have never heard of them",
/// which on this surface is a sentence about a person rather than about a URL.
/// The value returned is normalised, and the store normalises again: one rule,
/// applied twice, because an address that reached a query unfolded would be a
/// person mailed twice or unsubscribed once.
pub(crate) fn address_of(raw: &str) -> Result<String, Problem> {
    normalise_address(raw)
        .ok_or_else(|| unprocessable("address must be an email address this audience could hold"))
}

/// One page of an audience-shaped read, or the `422` naming the parameter.
///
/// A free function rather than a `Query` struct of its own: axum hands a
/// handler exactly one `Query` extractor and `serde_urlencoded` does not
/// support `#[serde(flatten)]`, so a route that takes conditions *and* a page
/// declares one flat struct and calls this. One parsing rule for every route
/// that pages.
///
/// A cursor is refused rather than ignored, exactly as the store refuses it:
/// ignoring one silently returns page one, which reads as "the audience
/// restarted" and walks a caller round the same people for ever.
pub(crate) fn page_from(after: Option<&str>, limit: Option<&str>) -> Result<AudiencePage, Problem> {
    let limit = match stated(limit) {
        None => AudiencePage::default().limit,
        Some(raw) => raw
            .parse::<i64>()
            .ok()
            .filter(|value| (1..=AUDIENCE_PAGE_MAX).contains(value))
            .ok_or_else(|| {
                unprocessable(format!(
                    "limit is a whole number of people, 1 to {AUDIENCE_PAGE_MAX}"
                ))
            })?,
    };
    let after =
        match stated(after) {
            None => None,
            Some(raw) => Some(address_of(raw).map_err(|_| {
                unprocessable("after must be an address this audience could contain")
            })?),
        };
    Ok(AudiencePage { after, limit })
}

/// The conditions of a segment, as a colleague's question fits on a URL.
///
/// `countries` is comma-separated rather than a repeated key because axum's
/// query deserialiser keeps the last value of a repeated one and would silently
/// narrow "Belgium and the Netherlands" to "the Netherlands" — a wrong number
/// on a screen, arrived at without an error.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionsQuery {
    /// ISO 3166-1 alpha-2 codes, comma-separated, any casing. Absent or blank
    /// is **no country condition** — which is everybody, not nobody.
    #[serde(default)]
    pub countries: Option<String>,
    /// `bought` or `not_bought`; absent is no purchase condition.
    #[serde(default)]
    pub purchase: Option<String>,
    /// How far back the purchase condition looks, in days. Absent means
    /// "ever" — *has never bought from us* is a real question, not a missing
    /// value.
    #[serde(default)]
    pub within_days: Option<String>,
}

impl ConditionsQuery {
    /// The conditions, or the `422` naming the parameter that is not one.
    pub(crate) fn conditions(&self) -> Result<SegmentConditions, Problem> {
        conditions_from(
            self.countries.as_deref(),
            self.purchase.as_deref(),
            self.within_days.as_deref(),
        )
    }
}

/// The conditions a question states, or the `422` naming the parameter that is
/// not one.
///
/// The country codes are only *split* here; whether `XX` is a country is the
/// store's rule (`billing_field::country`), applied by the same validation a
/// saved segment goes through, so an unsaved question and a saved one cannot
/// disagree about what a country is.
///
/// `withinDays` without `purchase` is refused rather than dropped: a colleague
/// who typed a period meant to ask about purchases, and quietly answering the
/// unrestricted question instead would hand them a bigger number than they
/// asked for — which on this screen is a bigger send.
pub(crate) fn conditions_from(
    countries: Option<&str>,
    purchase: Option<&str>,
    within_days: Option<&str>,
) -> Result<SegmentConditions, Problem> {
    let countries = stated(countries)
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|code| !code.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let within_days = match stated(within_days) {
        None => None,
        Some(raw) => Some(raw.parse::<i32>().ok().ok_or_else(|| {
            unprocessable("withinDays is a whole number of days to look back over")
        })?),
    };

    let purchase = match stated(purchase) {
        None => {
            if within_days.is_some() {
                return Err(unprocessable(
                    "withinDays is the period of a purchase condition, so purchase must say \
                     bought or not_bought",
                ));
            }
            None
        }
        Some(raw) => Some(PurchaseWindow {
            condition: PurchaseCondition::parse(&raw.to_ascii_lowercase())
                .ok_or_else(|| unprocessable("purchase must be bought or not_bought"))?,
            within_days,
        }),
    };

    Ok(SegmentConditions {
        countries,
        purchase,
    })
}

/// A segment's conditions as JSON — the shape the screen edits and posts back.
///
/// `purchase` is `null` or an object, never a pair of loose fields: a period
/// without a condition is not a question, and a shape that can express one
/// invites a client to send it.
pub(crate) fn conditions_json(conditions: &SegmentConditions) -> Value {
    json!({
        "countries": conditions.countries,
        "purchase": conditions.purchase.map(|window| json!({
            "condition": window.condition.as_str(),
            "withinDays": window.within_days,
        })),
    })
}

#[cfg(test)]
mod tests {
    use super::{ConditionsQuery, page_from};
    use alo_store::{PurchaseCondition, PurchaseWindow, SegmentConditions};

    // `unwrap`/`expect` are denied workspace-wide and a `#![allow]` in a test
    // module does not reach `src/`, so every assertion below reads the answer
    // through an `Option` instead.
    fn asked(
        countries: Option<&str>,
        purchase: Option<&str>,
        days: Option<&str>,
    ) -> Option<SegmentConditions> {
        ConditionsQuery {
            countries: countries.map(str::to_owned),
            purchase: purchase.map(str::to_owned),
            within_days: days.map(str::to_owned),
        }
        .conditions()
        .ok()
    }

    fn window(purchase: Option<&str>, days: Option<&str>) -> Option<Option<PurchaseWindow>> {
        asked(None, purchase, days).map(|conditions| conditions.purchase)
    }

    fn paged(after: Option<&str>, limit: Option<&str>) -> Option<(Option<String>, i64)> {
        page_from(after, limit)
            .ok()
            .map(|page| (page.after, page.limit))
    }

    #[test]
    fn an_absent_condition_asks_about_everybody_rather_than_nobody() {
        assert_eq!(asked(None, None, None), Some(SegmentConditions::default()));
        // A blank parameter is the same as an absent one — a select on "all"
        // sends `countries=`, and reading that as "no country matches" would
        // empty the screen rather than fill it.
        assert_eq!(
            asked(Some("  "), Some(""), Some(" ")),
            Some(SegmentConditions::default()),
            "a blank parameter is absent, not invalid"
        );
    }

    #[test]
    fn countries_are_split_on_commas_and_kept_whole() {
        assert_eq!(
            asked(Some(" be , NL ,, "), None, None).map(|c| c.countries),
            Some(vec!["be".to_owned(), "NL".to_owned()]),
            "a repeated key would keep only the last country and quietly \
             narrow the question"
        );
    }

    #[test]
    fn a_period_with_no_purchase_condition_is_refused_rather_than_dropped() {
        // Dropping it would answer a wider question than the one asked, and on
        // this surface a wider question is a bigger send.
        assert_eq!(window(None, Some("90")), None);
        assert_eq!(
            window(Some("not_bought"), Some("90")),
            Some(Some(PurchaseWindow {
                condition: PurchaseCondition::NotBought,
                within_days: Some(90),
            }))
        );
    }

    #[test]
    fn a_purchase_condition_this_build_cannot_name_fails_rather_than_widening() {
        assert_eq!(window(Some("maybe"), None), None);
        assert_eq!(window(Some("bought"), Some("ninety")), None);
        // "ever" is a real answer, not a missing one.
        assert_eq!(
            window(Some("BOUGHT"), None),
            Some(Some(PurchaseWindow {
                condition: PurchaseCondition::Bought,
                within_days: None,
            }))
        );
    }

    #[test]
    fn a_cursor_that_is_not_an_address_is_refused_rather_than_restarted() {
        // Ignoring it would return page one, which reads as "the audience
        // restarted" and walks a caller round the same people for ever.
        assert_eq!(paged(Some("page-two"), None), None);
        assert_eq!(
            paged(Some(" Ann@Example.TEST "), None).map(|(after, _)| after),
            Some(Some("ann@example.test".to_owned())),
            "the cursor a screen echoes back in any casing lands where it means"
        );
    }

    #[test]
    fn a_page_size_outside_the_stores_bounds_is_the_callers_error() {
        assert_eq!(paged(None, Some("0")), None);
        assert_eq!(paged(None, Some("501")), None);
        assert_eq!(paged(None, Some("banana")), None);
        assert_eq!(paged(None, Some("25")).map(|(_, limit)| limit), Some(25));
        assert_eq!(paged(None, None).map(|(_, limit)| limit), Some(100));
    }
}
