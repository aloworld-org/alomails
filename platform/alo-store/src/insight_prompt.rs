//! The closed catalog, written out for a model to read (ADR 0037, wave BI1.07).
//!
//! Ask-to-chart turns a sentence into a [`crate::insight_spec::ChartSpec`], and
//! the only way that can be safe is for the model to be choosing from a menu
//! rather than writing a query. This module is that menu, rendered from the
//! very enums the validator and the query engine use
//! ([`crate::insight_catalog`]) — so the vocabulary a model is told about and
//! the vocabulary the server accepts are, by construction, the same list.
//!
//! It lives here rather than in `alo-ai` for exactly that reason: `alo-ai` has
//! no view of the catalog types, and a hand-written copy of the vocabulary in
//! the inference layer would be a second source of truth that drifts the first
//! time a measure is added. `alo-ai` owns the *shape* of the conversation (the
//! envelope, the repair turn); this owns *what may be said in it*.
//!
//! Two properties are load-bearing, and both are tested below:
//!
//! 1. **Totality.** Every dataset, measure, breakdown and filter the catalog
//!    offers appears here, because the text is generated from the catalog and
//!    never typed out. A measure the model is not told about is a measure the
//!    product does not have.
//! 2. **No identifiers, ever.** Filters whose values are record ids
//!    ([`ValueKind::Id`]) are listed as *unavailable*: a model does not know a
//!    tenant's customer ids and must never guess at one. Guessing is already
//!    harmless — an id that is not this tenant's is a refusal at evaluation,
//!    never a chart that is quietly empty — but a wrong chart the user has to
//!    reject is a worse answer than a chart without that filter.
//!
//! The text is English because a system prompt is machinery, not a user-facing
//! string: nothing here reaches a screen. The *question* stays in the user's
//! own language, and so does every caption they keep.

use crate::insight_catalog::{
    self, DATASETS, Dataset, DatasetEntry, Dimension, DimensionKind, Measure, Unit, ValueKind, Viz,
};
use crate::insight_spec::{
    MAX_CATEGORIES, MAX_FILTER_VALUES, MAX_FILTERS, MAX_PERIOD_DAYS, MAX_TIME_BUCKETS,
};

/// A catalog enum's wire word — read back through serde, so the menu can never
/// name something by a spelling the parser would refuse.
fn wire(value: &impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "?".to_owned())
}

/// The wire words of a list, comma-separated.
fn wires<T: serde::Serialize>(values: impl IntoIterator<Item = T>) -> String {
    values
        .into_iter()
        .map(|v| wire(&v))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a dataset holds, in one line. Exhaustive on purpose: a dataset added to
/// the catalog without a sentence here is a compile error, not an omission
/// somebody notices in production.
fn dataset_note(dataset: Dataset) -> &'static str {
    match dataset {
        Dataset::BillingDocuments => {
            "invoices and credit notes that stand (issued or paid); drafts and voided \
             documents are not in it at all"
        }
        Dataset::BillingReceivables => {
            "issued documents that are not fully paid — what is still owed, and how old it is"
        }
        Dataset::BillingPayments => "payments recorded against documents — money that arrived",
        Dataset::CrmDeals => "sales deals, open and closed",
    }
}

/// What a measure counts.
fn measure_note(measure: Measure) -> &'static str {
    match measure {
        Measure::Net => "document total excluding VAT",
        Measure::Vat => "the VAT on documents",
        Measure::Gross => "document total including VAT",
        Measure::Count => "how many rows there are",
        Measure::Outstanding => "how much of an issued document is still unpaid",
        Measure::Amount => "how much a payment was for",
        Measure::Value => "what a deal is worth",
        Measure::WinRate => "deals won divided by deals closed",
    }
}

/// What a breakdown slices by.
fn dimension_note(dimension: Dimension) -> &'static str {
    match dimension {
        Dimension::IssueDate => "the day a document was issued",
        Dimension::Customer => "the customer a document or payment belongs to",
        Dimension::Currency => "the currency it is in",
        Dimension::VatRate => "the VAT rate a subtotal was charged at",
        Dimension::Status => "issued or paid",
        Dimension::AgeBucket => "how overdue it is (not due, 0-30, 31-60, 61-90, 90+ days)",
        Dimension::DueDate => "the day a document falls due",
        Dimension::PaidOn => "the day a payment was received",
        Dimension::Method => "how a payment was made, in the tenant's own words",
        Dimension::Stage => "the pipeline column a deal sits in",
        Dimension::Owner => "the person a deal belongs to",
        Dimension::Source => "where a deal came from",
        Dimension::Outcome => "whether a deal is open, won or lost",
        Dimension::CreatedAt => "the day a deal was raised",
        Dimension::ClosedAt => "the day a deal was closed, won or lost",
        Dimension::ExpectedClose => "the day a deal is expected to close",
    }
}

/// How a measure's figures are to be read.
fn unit_note(unit: Unit) -> &'static str {
    match unit {
        Unit::Money => "money, in integer cents",
        Unit::Count => "a plain count",
        Unit::PercentBp => "a ratio in basis points (10000 = 100%)",
    }
}

/// What a filter's values look like — and, for record ids, that the model may
/// not use it at all.
fn value_note(kind: ValueKind) -> String {
    match kind {
        ValueKind::Id => {
            "record ids — DO NOT USE this filter: you do not know this tenant's ids".to_owned()
        }
        ValueKind::Currency => "three-letter ISO 4217 codes, e.g. \"EUR\"".to_owned(),
        ValueKind::Enum(allowed) => format!("one of: {}", allowed.join(", ")),
        ValueKind::RateBp => {
            "a VAT rate in basis points as a string, e.g. \"2100\" for 21%".to_owned()
        }
        ValueKind::Text => "the tenant's own words for a payment method".to_owned(),
    }
}

/// One dataset's whole vocabulary, as the model reads it.
fn describe(entry: &DatasetEntry, out: &mut String) {
    out.push_str(&format!(
        "\n{} — {}. Its own date: {}.\n",
        wire(&entry.dataset),
        dataset_note(entry.dataset),
        wire(&entry.period)
    ));

    out.push_str("  measures:\n");
    for measure in entry.measures {
        out.push_str(&format!(
            "    {} — {}; {}; agg: {}; breakdowns: {}\n",
            wire(&measure.measure),
            measure_note(measure.measure),
            unit_note(measure.unit),
            wires(measure.aggregates.iter().copied()),
            if measure.dimensions.is_empty() {
                "none".to_owned()
            } else {
                wires(measure.dimensions.iter().copied())
            }
        ));
    }

    out.push_str("  breakdowns:\n");
    for dimension in entry.dimensions {
        let shape = match dimension.kind {
            DimensionKind::Time(grains) => format!(
                "a date; needs a grain, one of: {}",
                wires(grains.iter().copied())
            ),
            DimensionKind::Category => "a category; takes no grain".to_owned(),
        };
        out.push_str(&format!(
            "    {} — {}; {}\n",
            wire(&dimension.dimension),
            dimension_note(dimension.dimension),
            shape
        ));
    }

    out.push_str("  filters:\n");
    for filter in entry.filters {
        out.push_str(&format!(
            "    {} — values are {}\n",
            wire(&filter.field),
            value_note(filter.value)
        ));
    }

    let dates: Vec<Dimension> = entry
        .dimensions
        .iter()
        .filter(|d| matches!(d.kind, DimensionKind::Time(_)))
        .map(|d| d.dimension)
        .collect();
    out.push_str(&format!(
        "  period_on may be: {}\n",
        wires(dates.iter().copied())
    ));
}

/// The whole menu: the envelope's own grammar, every dataset, and the bounds a
/// chart has to stay inside.
///
/// Deterministic — the same string on every call, in catalog order — so a
/// prompt is reproducible and a fixture test means something.
#[must_use]
pub fn catalog_prompt() -> String {
    let mut out = String::with_capacity(4096);
    out.push_str(&format!(
        "A chart specification is a JSON object with these fields:\n\
         - schema_version: always {}\n\
         - dataset: one of the datasets below, spelled exactly\n\
         - measure: {{\"id\": <a measure of that dataset>, \"agg\": <an agg it allows>}}\n\
         - dimension: {{\"id\": <a breakdown that measure allows>, \"grain\": <only for a date>}} \
           — omit it entirely for a single figure\n\
         - period: {{\"kind\":\"last_n\",\"n\":<1-{}>,\"grain\":<grain>}} or \
           {{\"kind\":\"range\",\"from\":\"YYYY-MM-DD\",\"to\":\"YYYY-MM-DD\"}} or {{\"kind\":\"all\"}}\n\
         - period_on (optional): which of the dataset's dates the period narrows on; \
           omit it unless the question means a different date than the chart draws\n\
         - filters (optional): [{{\"id\": <a filter of that dataset>, \"op\": \"in\" | \"not_in\", \
           \"values\": [<strings>]}}]\n\
         - sort (optional): {{\"by\": \"dimension\" | \"value\", \"dir\": \"asc\" | \"desc\"}}\n\
         - limit (optional): 1-{}, how many categories to keep\n\
         - viz: {}\n\n\
         How a chart is drawn has to agree with its breakdown: \"number\" takes NO dimension, \
         \"line\" needs a date breakdown, \"pie\" needs a category breakdown, and \"bar\" and \
         \"table\" need a breakdown of either kind.\n\n\
         Datasets:\n",
        crate::insight_spec::CHART_SPEC_SCHEMA_VERSION,
        MAX_TIME_BUCKETS,
        MAX_CATEGORIES,
        wires([Viz::Number, Viz::Bar, Viz::Line, Viz::Pie, Viz::Table]),
    ));

    for &dataset in DATASETS {
        describe(insight_catalog::dataset(dataset), &mut out);
    }

    out.push_str(&format!(
        "\nBounds: a period may span at most {MAX_PERIOD_DAYS} days and produce at most \
         {MAX_TIME_BUCKETS} buckets; at most {MAX_FILTERS} filters, each with at most \
         {MAX_FILTER_VALUES} values.\n"
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every word the validator accepts is a word the model was offered. A
    /// measure the catalog gained and this menu never mentioned would be a
    /// feature nobody could ask for.
    #[test]
    fn the_menu_lists_the_whole_catalog() {
        let prompt = catalog_prompt();
        for &d in DATASETS {
            let entry = insight_catalog::dataset(d);
            assert!(prompt.contains(&wire(&d)), "{d:?} is not offered");
            for m in entry.measures {
                assert!(
                    prompt.contains(&wire(&m.measure)),
                    "{d:?}/{:?} is not offered",
                    m.measure
                );
                for agg in m.aggregates {
                    assert!(prompt.contains(&wire(agg)), "{agg:?} is not offered");
                }
            }
            for dim in entry.dimensions {
                assert!(
                    prompt.contains(&wire(&dim.dimension)),
                    "{d:?}/{:?} is not offered",
                    dim.dimension
                );
                if let DimensionKind::Time(grains) = dim.kind {
                    for grain in grains {
                        assert!(prompt.contains(&wire(grain)), "{grain:?} is not offered");
                    }
                }
            }
            for f in entry.filters {
                assert!(
                    prompt.contains(&wire(&f.field)),
                    "{d:?}/{:?} is not offered",
                    f.field
                );
                if let ValueKind::Enum(values) = f.value {
                    for value in values {
                        assert!(prompt.contains(value), "{value} is not offered");
                    }
                }
            }
        }
        // The five drawings, and the envelope's own fields.
        for word in [
            "number",
            "bar",
            "line",
            "pie",
            "table",
            "schema_version",
            "period_on",
            "last_n",
            "not_in",
        ] {
            assert!(prompt.contains(word), "{word} is not offered");
        }
    }

    /// A model does not know a tenant's ids, so every id-valued filter is
    /// offered as a refusal rather than as an option. (An invented id is
    /// already a `422` at evaluation — this is what stops the model reaching
    /// for one in the first place.)
    #[test]
    fn record_id_filters_are_offered_only_to_be_refused() {
        let prompt = catalog_prompt();
        let declared = DATASETS
            .iter()
            .flat_map(|d| insight_catalog::dataset(*d).filters)
            .filter(|f| f.value == ValueKind::Id)
            .count();
        assert!(declared > 0, "the catalog has no id filter to refuse");

        let offered: Vec<&str> = prompt
            .lines()
            .filter(|line| line.contains("values are record ids"))
            .collect();
        assert_eq!(
            offered.len(),
            declared,
            "every id filter is listed exactly once"
        );
        for line in offered {
            assert!(line.contains("DO NOT USE"), "{line}");
        }
    }

    /// The bounds a spec is held to are the bounds the model is told about —
    /// read from the same constants the validator enforces.
    #[test]
    fn the_bounds_are_the_validators_own() {
        let prompt = catalog_prompt();
        for bound in [
            MAX_PERIOD_DAYS.to_string(),
            MAX_TIME_BUCKETS.to_string(),
            MAX_FILTERS.to_string(),
            MAX_FILTER_VALUES.to_string(),
            MAX_CATEGORIES.to_string(),
        ] {
            assert!(prompt.contains(&bound), "{bound} is not stated");
        }
    }

    /// A prompt is paid for on every ask, so the menu stays a menu. It is also
    /// the same string every time: a catalog rendered in a different order on
    /// different calls would make a model's answer irreproducible.
    #[test]
    fn the_menu_is_bounded_and_deterministic() {
        let prompt = catalog_prompt();
        assert_eq!(prompt, catalog_prompt(), "the menu must not vary");
        assert!(
            prompt.len() < 8 * 1024,
            "the menu has grown to {} bytes",
            prompt.len()
        );
    }
}
