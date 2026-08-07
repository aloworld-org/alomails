//! The alo Insights semantic layer: the **closed catalog** every chart is
//! built from (ADR 0037, wave BI-1).
//!
//! A ChartSpec ([`crate::insight_spec`]) names a dataset, a measure, a
//! dimension and some filters — and every one of those names is an *enum
//! variant declared here*, never a table, column or identifier from a
//! request. That is the property the whole surface rests on: no string a user
//! or a model sends can reach a query as SQL text, because the only strings
//! that ever do are the `&'static str` fragments we write at compile time
//! (`insight_query`, BI1.03).
//!
//! The catalog is also the **compatibility matrix**. Which measures a dataset
//! offers, which dimensions each measure may be broken down by, which grains
//! a time dimension allows, which filters apply and what shape their values
//! take: all declared, none assumed. `sum(deal value)` by `vat_rate` is not
//! an odd chart, it is a validation error — and so is counting documents per
//! VAT rate, which would count a two-rate invoice twice.
//!
//! This module is pure data plus lookups. It contains **no SQL and no
//! persistence**; the query engine reads it, the spec validator reads it, and
//! the catalog route (BI1.04) serves it to the builder UI so the client can
//! only ever offer what the server can actually answer.

use serde::{Deserialize, Serialize};

/// A dataset — one logical view of the business, not a database table.
///
/// A dataset is a Rust row-query plus this entry; deliberately **not** a
/// Postgres view. A view cannot carry the tenant predicate by construction,
/// and the rules that make a figure right (only documents that stand are
/// counted, credit notes subtract, each document's own rounded VAT is summed,
/// a document converts at its own frozen rate) already live in Rust. Restating
/// them in SQL is how a tile and a VAT return come to disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dataset {
    /// Invoices and credit notes that stand (issued or paid).
    #[serde(rename = "billing.documents")]
    BillingDocuments,
    /// Issued, unpaid documents — what is still owed, and how old it is.
    #[serde(rename = "billing.receivables")]
    BillingReceivables,
    /// Payments recorded against documents — money that actually arrived.
    #[serde(rename = "billing.payments")]
    BillingPayments,
    /// Deals, open and closed.
    #[serde(rename = "crm.deals")]
    CrmDeals,
}

/// A quantity a dataset can be measured by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Measure {
    /// Document net (excluding VAT), in cents.
    Net,
    /// Document VAT, in cents.
    Vat,
    /// Document gross (net + VAT), in cents.
    Gross,
    /// How many rows — documents, payments or deals.
    Count,
    /// Still owed on issued documents, in cents.
    Outstanding,
    /// Payment amount, in cents.
    Amount,
    /// Deal value, in cents.
    Value,
    /// Won ÷ closed, in basis points.
    WinRate,
}

/// How a measure is reduced over a bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aggregate {
    /// Add the values up.
    Sum,
    /// Count the rows.
    Count,
    /// A declared ratio the engine computes (the win rate, in basis points) —
    /// never an arbitrary division a caller composes.
    Ratio,
}

/// What a series is measured in. Every value on the wire is an integer; the
/// unit says how to read it, and the client formats it per locale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    /// Integer cents, carried with a currency.
    Money,
    /// A plain count.
    Count,
    /// A ratio in basis points (10 000 = 100 %).
    PercentBp,
}

/// A way of slicing a dataset into buckets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dimension {
    /// The date a document was issued.
    IssueDate,
    /// The document's or payment's customer.
    Customer,
    /// The document's, payment's or deal's currency.
    Currency,
    /// The VAT rate a document subtotal was charged at.
    VatRate,
    /// The document's status.
    Status,
    /// How overdue a receivable is (0–30 / 31–60 / 61–90 / 90+ days).
    AgeBucket,
    /// The date a document falls due.
    DueDate,
    /// The date a payment was received.
    PaidOn,
    /// How a payment was made (the tenant's own words: "SEPA", "card").
    Method,
    /// The pipeline column a deal sits in.
    Stage,
    /// The user a deal belongs to.
    Owner,
    /// Where a deal came from.
    Source,
    /// Whether a deal is open, won or lost.
    Outcome,
    /// When a deal was created.
    CreatedAt,
    /// When a deal was closed.
    ClosedAt,
    /// When a deal is expected to close.
    ExpectedClose,
}

/// The size of a time bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grain {
    /// One day per bucket.
    Day,
    /// One ISO week per bucket.
    Week,
    /// One calendar month per bucket.
    Month,
    /// One calendar quarter per bucket.
    Quarter,
    /// One calendar year per bucket.
    Year,
}

/// A field a spec may filter on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterField {
    /// Restrict to given customers (by id).
    Customer,
    /// Restrict to given currencies.
    Currency,
    /// Restrict to given document statuses.
    Status,
    /// Restrict to given VAT rates, in basis points.
    VatRate,
    /// Restrict to given payment methods.
    Method,
    /// Restrict to one or more pipelines (by id).
    Pipeline,
    /// Restrict to given deal owners (by user id).
    Owner,
    /// Restrict to open / won / lost deals.
    Outcome,
}

/// How a filter compares. Both ops bind their values as parameters — a filter
/// value is data, never syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
    /// Keep rows whose value is one of the listed ones.
    In,
    /// Keep rows whose value is none of the listed ones.
    NotIn,
}

/// The chart form a tile is drawn as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Viz {
    /// A single figure, with no breakdown.
    Number,
    /// Vertical bars, one per bucket.
    Bar,
    /// A line over time.
    Line,
    /// Shares of a whole.
    Pie,
    /// The buckets and their values, as rows.
    Table,
}

/// What shape a filter's values take. Checked by the spec validator; *ids are
/// additionally resolved against the tenant's own records at evaluation time*
/// (BI1.03), so a guessed id from another tenant is a refusal rather than a
/// join that quietly matches nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    /// An opaque record id belonging to this tenant.
    Id,
    /// A three-letter ISO 4217 currency code.
    Currency,
    /// One of a closed set of words.
    Enum(&'static [&'static str]),
    /// An integer in basis points.
    RateBp,
    /// Free text the tenant itself wrote (a payment method).
    Text,
}

/// Whether a dimension buckets by time (and with which grains) or by value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimensionKind {
    /// Time buckets. The spec must name one of these grains.
    Time(&'static [Grain]),
    /// One bucket per distinct value. The spec must not name a grain.
    Category,
}

/// A measure a dataset offers, with the aggregates and breakdowns it allows.
#[derive(Debug, Clone, Copy)]
pub struct MeasureEntry {
    /// Which measure.
    pub measure: Measure,
    /// What its values mean.
    pub unit: Unit,
    /// The aggregates it may be reduced with (usually exactly one).
    pub aggregates: &'static [Aggregate],
    /// The dimensions it may be broken down by — a subset of the dataset's.
    pub dimensions: &'static [Dimension],
}

/// A dimension a dataset offers.
#[derive(Debug, Clone, Copy)]
pub struct DimensionEntry {
    /// Which dimension.
    pub dimension: Dimension,
    /// Time (with its grains) or category.
    pub kind: DimensionKind,
}

/// A filter a dataset accepts.
#[derive(Debug, Clone, Copy)]
pub struct FilterEntry {
    /// Which field.
    pub field: FilterField,
    /// The shape of its values.
    pub value: ValueKind,
    /// The comparisons allowed on it.
    pub operators: &'static [FilterOp],
}

/// One dataset's whole vocabulary.
#[derive(Debug, Clone, Copy)]
pub struct DatasetEntry {
    /// Which dataset.
    pub dataset: Dataset,
    /// Its measures.
    pub measures: &'static [MeasureEntry],
    /// Its dimensions.
    pub dimensions: &'static [DimensionEntry],
    /// Its filters.
    pub filters: &'static [FilterEntry],
}

impl DatasetEntry {
    /// The dataset's entry for `measure`, or `None` when it does not offer it.
    pub fn measure(&self, measure: Measure) -> Option<&'static MeasureEntry> {
        self.measures.iter().find(|m| m.measure == measure)
    }

    /// The dataset's entry for `dimension`, or `None`.
    pub fn dimension(&self, dimension: Dimension) -> Option<&'static DimensionEntry> {
        self.dimensions.iter().find(|d| d.dimension == dimension)
    }

    /// The dataset's entry for `field`, or `None`.
    pub fn filter(&self, field: FilterField) -> Option<&'static FilterEntry> {
        self.filters.iter().find(|f| f.field == field)
    }
}

/// Every dataset BI-1 ships, in catalog order.
pub const DATASETS: &[Dataset] = &[
    Dataset::BillingDocuments,
    Dataset::BillingReceivables,
    Dataset::BillingPayments,
    Dataset::CrmDeals,
];

/// The catalog entry for `dataset`. Total by construction: adding a variant
/// without describing it here is a compile error, which is the point.
pub fn dataset(dataset: Dataset) -> &'static DatasetEntry {
    match dataset {
        Dataset::BillingDocuments => &BILLING_DOCUMENTS,
        Dataset::BillingReceivables => &BILLING_RECEIVABLES,
        Dataset::BillingPayments => &BILLING_PAYMENTS,
        Dataset::CrmDeals => &CRM_DEALS,
    }
}

// ---- the catalog ------------------------------------------------------------

/// Grains a date over billing documents may be bucketed by — a business reads
/// its revenue by day when chasing a spike and by year when planning.
const DATE_GRAINS: &[Grain] = &[
    Grain::Day,
    Grain::Week,
    Grain::Month,
    Grain::Quarter,
    Grain::Year,
];

/// Grains a deal date may be bucketed by. Coarser on purpose: a pipeline is a
/// forecast, and a forecast by day is noise dressed as precision.
const DEAL_GRAINS: &[Grain] = &[Grain::Month, Grain::Quarter, Grain::Year];

/// The statuses a document in `billing.documents` can have. Drafts and voided
/// documents are not in the dataset at all — they are not documents that
/// stand, so no filter can bring them back.
const DOCUMENT_STATUSES: &[&str] = &["issued", "paid"];

/// The states a deal can be filtered to (`crate::crm_deals::DealState`).
const DEAL_OUTCOMES: &[&str] = &["open", "won", "lost"];

const BOTH_OPS: &[FilterOp] = &[FilterOp::In, FilterOp::NotIn];

const DOCUMENT_DIMENSIONS: &[Dimension] = &[
    Dimension::IssueDate,
    Dimension::Customer,
    Dimension::Currency,
    Dimension::VatRate,
    Dimension::Status,
];

/// What a document *count* may be broken down by — everything a money measure
/// may, except the VAT rate: an invoice with a 21 % line and a 0 % line has
/// two rate subtotals and is still one document, so counting per rate would
/// report more invoices than the tenant raised. Its money, by contrast, does
/// split per rate exactly and legitimately.
const DOCUMENT_COUNT_DIMENSIONS: &[Dimension] = &[
    Dimension::IssueDate,
    Dimension::Customer,
    Dimension::Currency,
    Dimension::Status,
];

static BILLING_DOCUMENTS: DatasetEntry = DatasetEntry {
    dataset: Dataset::BillingDocuments,
    measures: &[
        MeasureEntry {
            measure: Measure::Net,
            unit: Unit::Money,
            aggregates: &[Aggregate::Sum],
            dimensions: DOCUMENT_DIMENSIONS,
        },
        MeasureEntry {
            measure: Measure::Vat,
            unit: Unit::Money,
            aggregates: &[Aggregate::Sum],
            dimensions: DOCUMENT_DIMENSIONS,
        },
        MeasureEntry {
            measure: Measure::Gross,
            unit: Unit::Money,
            aggregates: &[Aggregate::Sum],
            dimensions: DOCUMENT_DIMENSIONS,
        },
        MeasureEntry {
            measure: Measure::Count,
            unit: Unit::Count,
            aggregates: &[Aggregate::Count],
            dimensions: DOCUMENT_COUNT_DIMENSIONS,
        },
    ],
    dimensions: &[
        DimensionEntry {
            dimension: Dimension::IssueDate,
            kind: DimensionKind::Time(DATE_GRAINS),
        },
        DimensionEntry {
            dimension: Dimension::Customer,
            kind: DimensionKind::Category,
        },
        DimensionEntry {
            dimension: Dimension::Currency,
            kind: DimensionKind::Category,
        },
        DimensionEntry {
            dimension: Dimension::VatRate,
            kind: DimensionKind::Category,
        },
        DimensionEntry {
            dimension: Dimension::Status,
            kind: DimensionKind::Category,
        },
    ],
    filters: &[
        FilterEntry {
            field: FilterField::Customer,
            value: ValueKind::Id,
            operators: BOTH_OPS,
        },
        FilterEntry {
            field: FilterField::Currency,
            value: ValueKind::Currency,
            operators: BOTH_OPS,
        },
        FilterEntry {
            field: FilterField::Status,
            value: ValueKind::Enum(DOCUMENT_STATUSES),
            operators: BOTH_OPS,
        },
        FilterEntry {
            field: FilterField::VatRate,
            value: ValueKind::RateBp,
            operators: BOTH_OPS,
        },
    ],
};

const RECEIVABLE_DIMENSIONS: &[Dimension] = &[
    Dimension::AgeBucket,
    Dimension::Customer,
    Dimension::DueDate,
    Dimension::Currency,
];

static BILLING_RECEIVABLES: DatasetEntry = DatasetEntry {
    dataset: Dataset::BillingReceivables,
    measures: &[
        MeasureEntry {
            measure: Measure::Outstanding,
            unit: Unit::Money,
            aggregates: &[Aggregate::Sum],
            dimensions: RECEIVABLE_DIMENSIONS,
        },
        MeasureEntry {
            measure: Measure::Count,
            unit: Unit::Count,
            aggregates: &[Aggregate::Count],
            dimensions: RECEIVABLE_DIMENSIONS,
        },
    ],
    dimensions: &[
        DimensionEntry {
            dimension: Dimension::AgeBucket,
            kind: DimensionKind::Category,
        },
        DimensionEntry {
            dimension: Dimension::Customer,
            kind: DimensionKind::Category,
        },
        DimensionEntry {
            dimension: Dimension::DueDate,
            kind: DimensionKind::Time(DATE_GRAINS),
        },
        DimensionEntry {
            dimension: Dimension::Currency,
            kind: DimensionKind::Category,
        },
    ],
    filters: &[
        FilterEntry {
            field: FilterField::Customer,
            value: ValueKind::Id,
            operators: BOTH_OPS,
        },
        FilterEntry {
            field: FilterField::Currency,
            value: ValueKind::Currency,
            operators: BOTH_OPS,
        },
    ],
};

const PAYMENT_DIMENSIONS: &[Dimension] = &[
    Dimension::PaidOn,
    Dimension::Method,
    Dimension::Customer,
    Dimension::Currency,
];

static BILLING_PAYMENTS: DatasetEntry = DatasetEntry {
    dataset: Dataset::BillingPayments,
    measures: &[
        MeasureEntry {
            measure: Measure::Amount,
            unit: Unit::Money,
            aggregates: &[Aggregate::Sum],
            dimensions: PAYMENT_DIMENSIONS,
        },
        MeasureEntry {
            measure: Measure::Count,
            unit: Unit::Count,
            aggregates: &[Aggregate::Count],
            dimensions: PAYMENT_DIMENSIONS,
        },
    ],
    dimensions: &[
        DimensionEntry {
            dimension: Dimension::PaidOn,
            kind: DimensionKind::Time(DATE_GRAINS),
        },
        DimensionEntry {
            dimension: Dimension::Method,
            kind: DimensionKind::Category,
        },
        DimensionEntry {
            dimension: Dimension::Customer,
            kind: DimensionKind::Category,
        },
        DimensionEntry {
            dimension: Dimension::Currency,
            kind: DimensionKind::Category,
        },
    ],
    filters: &[
        FilterEntry {
            field: FilterField::Customer,
            value: ValueKind::Id,
            operators: BOTH_OPS,
        },
        FilterEntry {
            // A method is whatever the tenant typed on the payment ("SEPA",
            // "Bancontact"), so it is text rather than a closed set — the
            // catalog route offers the tenant's own distinct values.
            field: FilterField::Method,
            value: ValueKind::Text,
            operators: BOTH_OPS,
        },
        FilterEntry {
            field: FilterField::Currency,
            value: ValueKind::Currency,
            operators: BOTH_OPS,
        },
    ],
};

const DEAL_DIMENSIONS: &[Dimension] = &[
    Dimension::Stage,
    Dimension::Owner,
    Dimension::Source,
    Dimension::Outcome,
    Dimension::CreatedAt,
    Dimension::ClosedAt,
    Dimension::ExpectedClose,
    Dimension::Currency,
];

/// What a win rate may be broken down by. Not by stage or outcome: every
/// closed deal sits in a won or a lost column, so those breakdowns answer 100 %
/// and 0 % and teach nothing. Who closes, where the deal came from, and when
/// it closed are the three questions a win rate is actually asked.
const WIN_RATE_DIMENSIONS: &[Dimension] =
    &[Dimension::Owner, Dimension::Source, Dimension::ClosedAt];

static CRM_DEALS: DatasetEntry = DatasetEntry {
    dataset: Dataset::CrmDeals,
    measures: &[
        MeasureEntry {
            measure: Measure::Value,
            unit: Unit::Money,
            aggregates: &[Aggregate::Sum],
            dimensions: DEAL_DIMENSIONS,
        },
        MeasureEntry {
            measure: Measure::Count,
            unit: Unit::Count,
            aggregates: &[Aggregate::Count],
            dimensions: DEAL_DIMENSIONS,
        },
        MeasureEntry {
            measure: Measure::WinRate,
            unit: Unit::PercentBp,
            aggregates: &[Aggregate::Ratio],
            dimensions: WIN_RATE_DIMENSIONS,
        },
    ],
    dimensions: &[
        DimensionEntry {
            dimension: Dimension::Stage,
            kind: DimensionKind::Category,
        },
        DimensionEntry {
            dimension: Dimension::Owner,
            kind: DimensionKind::Category,
        },
        DimensionEntry {
            dimension: Dimension::Source,
            kind: DimensionKind::Category,
        },
        DimensionEntry {
            dimension: Dimension::Outcome,
            kind: DimensionKind::Category,
        },
        DimensionEntry {
            dimension: Dimension::CreatedAt,
            kind: DimensionKind::Time(DEAL_GRAINS),
        },
        DimensionEntry {
            dimension: Dimension::ClosedAt,
            kind: DimensionKind::Time(DEAL_GRAINS),
        },
        DimensionEntry {
            dimension: Dimension::ExpectedClose,
            kind: DimensionKind::Time(DEAL_GRAINS),
        },
        DimensionEntry {
            dimension: Dimension::Currency,
            kind: DimensionKind::Category,
        },
    ],
    filters: &[
        FilterEntry {
            field: FilterField::Pipeline,
            value: ValueKind::Id,
            operators: BOTH_OPS,
        },
        FilterEntry {
            field: FilterField::Owner,
            value: ValueKind::Id,
            operators: BOTH_OPS,
        },
        FilterEntry {
            field: FilterField::Outcome,
            value: ValueKind::Enum(DEAL_OUTCOMES),
            operators: BOTH_OPS,
        },
        FilterEntry {
            field: FilterField::Currency,
            value: ValueKind::Currency,
            operators: BOTH_OPS,
        },
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Every dataset the enum has is in [`DATASETS`] and has an entry that
    /// names itself. A dataset added to the enum and forgotten here would be
    /// invisible to the catalog route and the builder UI.
    #[test]
    fn every_dataset_is_listed_and_self_consistent() {
        assert_eq!(DATASETS.len(), 4);
        for &d in DATASETS {
            let entry = dataset(d);
            assert_eq!(entry.dataset, d, "entry names a different dataset");
            assert!(!entry.measures.is_empty(), "{d:?} offers no measure");
            assert!(!entry.dimensions.is_empty(), "{d:?} offers no dimension");
        }
    }

    /// The matrix has to be internally closed: a measure may only be broken
    /// down by dimensions its own dataset offers. A typo here would otherwise
    /// surface as a chart that validates and then cannot be compiled.
    #[test]
    fn every_measure_breakdown_is_a_dimension_of_its_dataset() {
        for &d in DATASETS {
            let entry = dataset(d);
            for measure in entry.measures {
                assert!(
                    !measure.aggregates.is_empty(),
                    "{d:?}/{:?} allows no aggregate",
                    measure.measure
                );
                for dim in measure.dimensions {
                    assert!(
                        entry.dimension(*dim).is_some(),
                        "{d:?}/{:?} may be broken down by {dim:?}, which the dataset has not got",
                        measure.measure
                    );
                }
            }
        }
    }

    /// A time dimension without grains could never be asked for; a category
    /// dimension with them would be a lie the validator repeats.
    #[test]
    fn time_dimensions_declare_at_least_one_grain() {
        for &d in DATASETS {
            for dim in dataset(d).dimensions {
                if let DimensionKind::Time(grains) = dim.kind {
                    assert!(!grains.is_empty(), "{d:?}/{:?} has no grain", dim.dimension);
                }
            }
        }
    }

    /// Every filter declares at least one operator and a value shape whose
    /// closed sets are non-empty.
    #[test]
    fn every_filter_is_usable() {
        for &d in DATASETS {
            for filter in dataset(d).filters {
                assert!(
                    !filter.operators.is_empty(),
                    "{d:?}/{:?} allows no operator",
                    filter.field
                );
                if let ValueKind::Enum(values) = filter.value {
                    assert!(
                        !values.is_empty(),
                        "{d:?}/{:?} accepts no value",
                        filter.field
                    );
                }
            }
        }
    }

    /// No dataset lists the same measure, dimension or filter twice — a
    /// duplicate would make the lookups' "first match" quietly meaningful.
    #[test]
    fn a_dataset_lists_nothing_twice() {
        for &d in DATASETS {
            let entry = dataset(d);
            for (i, m) in entry.measures.iter().enumerate() {
                assert!(
                    !entry.measures[..i].iter().any(|o| o.measure == m.measure),
                    "{d:?} lists {:?} twice",
                    m.measure
                );
            }
            for (i, dim) in entry.dimensions.iter().enumerate() {
                assert!(
                    !entry.dimensions[..i]
                        .iter()
                        .any(|o| o.dimension == dim.dimension),
                    "{d:?} lists {:?} twice",
                    dim.dimension
                );
            }
            for (i, f) in entry.filters.iter().enumerate() {
                assert!(
                    !entry.filters[..i].iter().any(|o| o.field == f.field),
                    "{d:?} lists {:?} twice",
                    f.field
                );
            }
        }
    }

    /// The wire vocabulary is the contract the builder UI and the AI both
    /// speak, so it is pinned rather than left to serde's defaults.
    #[test]
    fn the_wire_names_are_the_documented_ones() {
        assert_eq!(
            serde_json::to_string(&Dataset::BillingDocuments).unwrap_or_default(),
            "\"billing.documents\""
        );
        assert_eq!(
            serde_json::to_string(&Dataset::CrmDeals).unwrap_or_default(),
            "\"crm.deals\""
        );
        assert_eq!(
            serde_json::to_string(&Measure::WinRate).unwrap_or_default(),
            "\"win_rate\""
        );
        assert_eq!(
            serde_json::to_string(&Grain::Quarter).unwrap_or_default(),
            "\"quarter\""
        );
        assert_eq!(
            serde_json::to_string(&FilterOp::NotIn).unwrap_or_default(),
            "\"not_in\""
        );
        assert_eq!(
            serde_json::to_string(&Viz::Table).unwrap_or_default(),
            "\"table\""
        );
    }

    /// Money measures carry money, counts carry counts, the win rate carries
    /// basis points — the unit is what the client formats by, so a wrong one
    /// shows cents as a headcount.
    #[test]
    fn units_match_what_the_measure_means() {
        for &d in DATASETS {
            for m in dataset(d).measures {
                let expected = match m.measure {
                    Measure::Count => Unit::Count,
                    Measure::WinRate => Unit::PercentBp,
                    _ => Unit::Money,
                };
                assert_eq!(m.unit, expected, "{d:?}/{:?} has the wrong unit", m.measure);
            }
        }
    }
}
