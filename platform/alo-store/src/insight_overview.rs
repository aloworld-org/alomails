//! The **prebuilt questions** — alo Insights' gallery, and the zero-setup
//! Business overview a tenant is given on its first visit (ADR 0037, wave
//! BI1.06).
//!
//! Every entry here is an ordinary [`ChartSpec`] over the closed catalog, built
//! from the typed model rather than parsed from a JSON literal: a prebuilt
//! question that the validator would refuse is a dead tile on a board nobody
//! asked for, and the compiler is a cheaper place to find that out than a
//! tenant's screen. The unit tests below walk the whole gallery through the
//! same write gate a caller's spec goes through.
//!
//! Two things make this file the seed rather than a table of constants:
//!
//! - **The overview is real rows.** It is materialised into an ordinary
//!   dashboard with ordinary tiles the first time somebody lists boards, inside
//!   one transaction. *Rejected: rendering it virtually from code on every
//!   visit* — that would be a second kind of dashboard, one that cannot be
//!   renamed, reordered or extended, and the first request would be to change
//!   one tile on it (`docs/design/insights.md` § Dashboards and tiles).
//! - **The seed runs once per tenant, ever.** Once is recorded in
//!   `insight_seeds`, separately from the board it wrote, because the board can
//!   be deleted and the question "have we already given this tenant an
//!   overview?" still has to have an answer. The primary key on that ledger is
//!   what makes two simultaneous first visits produce exactly one board without
//!   a lock: both try to insert the row, one wins, and the winner is the
//!   transaction that writes the board.
//!
//! **No English lives here.** The board's name and every tile's caption arrive
//! already translated, from the HTTP edge, in the language of the client making
//! that first read — the same seam a CRM pipeline seed uses
//! (`products/mail/alo-jmap/src/insights_gallery.rs`). The store writes the
//! words it is handed and invents none.

use serde::Serialize;

use crate::account::AccountStore;
use crate::billing_field::required;
use crate::error::{Result, StoreError};
use crate::id::InsightDashboardId;
use crate::insight_catalog::{
    Aggregate, Dataset, Dimension, FilterField, FilterOp, Grain, Measure, Viz,
};
use crate::insight_dashboards::{
    BUSINESS_OVERVIEW_KEY, Dashboard, NewDashboard, insert_dashboard, normalize as normalize_name,
    normalize_key,
};
use crate::insight_spec::{
    CHART_SPEC_SCHEMA_VERSION, ChartSpec, DimensionRef, Filter, MeasureRef, Period, Sort, SortBy,
    SortDir,
};
use crate::insight_tiles::{NewTile, TILE_TITLE_MAX_CHARS, insert_tile};

/// Which part of the business a prebuilt question is about. The gallery is
/// grouped by it, and it grows a variant per module the catalog gains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GalleryModule {
    /// Invoices, receivables and payments.
    Billing,
    /// Deals and the pipeline.
    Crm,
}

/// One ready-made chart: a stable key, the module it belongs to, how wide it
/// sits on a board, and the question itself.
///
/// The **key is the contract**. It is what the client translates a title and a
/// description from (the server sends no English — a hardcoded English string
/// is a bug in a European product), what the seed's captions are matched on,
/// and what a later wave would migrate if a prebuilt question changed meaning.
#[derive(Debug, Clone, Copy)]
pub struct GalleryEntry {
    /// The stable id of this question.
    pub key: &'static str,
    /// Which module it reads.
    pub module: GalleryModule,
    /// How many of the four grid columns it wants when pinned.
    pub span: i16,
    /// The question, built from the typed model.
    spec: fn() -> ChartSpec,
}

impl GalleryEntry {
    /// The question this entry asks.
    pub fn spec(&self) -> ChartSpec {
        (self.spec)()
    }

    /// How it is drawn — derived from the spec, never stated twice.
    pub fn viz(&self) -> Viz {
        self.spec().viz
    }
}

/// Every prebuilt question BI-1 ships, in gallery order.
pub const GALLERY: &[GalleryEntry] = &[
    GalleryEntry {
        key: "revenue_by_month",
        module: GalleryModule::Billing,
        span: 2,
        spec: revenue_by_month,
    },
    GalleryEntry {
        key: "outstanding",
        module: GalleryModule::Billing,
        span: 1,
        spec: outstanding,
    },
    GalleryEntry {
        key: "overdue_aging",
        module: GalleryModule::Billing,
        span: 2,
        spec: overdue_aging,
    },
    GalleryEntry {
        key: "vat_by_quarter",
        module: GalleryModule::Billing,
        span: 2,
        spec: vat_by_quarter,
    },
    GalleryEntry {
        key: "top_customers",
        module: GalleryModule::Billing,
        span: 2,
        spec: top_customers,
    },
    GalleryEntry {
        key: "payments_by_month",
        module: GalleryModule::Billing,
        span: 2,
        spec: payments_by_month,
    },
    GalleryEntry {
        key: "pipeline_by_stage",
        module: GalleryModule::Crm,
        span: 2,
        spec: pipeline_by_stage,
    },
    GalleryEntry {
        key: "won_this_month",
        module: GalleryModule::Crm,
        span: 1,
        spec: won_this_month,
    },
    GalleryEntry {
        key: "win_rate_by_quarter",
        module: GalleryModule::Crm,
        span: 2,
        spec: win_rate_by_quarter,
    },
    GalleryEntry {
        key: "won_by_month",
        module: GalleryModule::Crm,
        span: 2,
        spec: won_by_month,
    },
];

/// The Business overview, in board order: what a business is owed, what it
/// earned, how late its money is, what it owes the tax office, what is in the
/// funnel, and what closed.
///
/// The money a tenant is *waiting for* leads, because it is the number a small
/// business actually opens a dashboard to see.
pub const BUSINESS_OVERVIEW: &[&str] = &[
    "outstanding",
    "won_this_month",
    "revenue_by_month",
    "overdue_aging",
    "pipeline_by_stage",
    "vat_by_quarter",
    "win_rate_by_quarter",
];

/// The gallery entry with this key, or `None`.
pub fn gallery_entry(key: &str) -> Option<&'static GalleryEntry> {
    GALLERY.iter().find(|entry| entry.key == key)
}

// ---- the questions -----------------------------------------------------------

/// The envelope every prebuilt question starts from: this build's schema
/// version, no filters, no limit, and nothing sorted until it says so.
fn spec(dataset: Dataset, measure: Measure, agg: Aggregate, viz: Viz) -> ChartSpec {
    ChartSpec {
        schema_version: CHART_SPEC_SCHEMA_VERSION,
        dataset,
        measure: MeasureRef { id: measure, agg },
        dimension: None,
        period: Period::All,
        period_on: None,
        filters: Vec::new(),
        sort: None,
        limit: None,
        viz,
    }
}

fn over_time(id: Dimension, grain: Grain) -> Option<DimensionRef> {
    Some(DimensionRef {
        id,
        grain: Some(grain),
    })
}

fn by(id: Dimension) -> Option<DimensionRef> {
    Some(DimensionRef { id, grain: None })
}

fn only(field: FilterField, value: &str) -> Vec<Filter> {
    vec![Filter {
        id: field,
        op: FilterOp::In,
        values: vec![value.to_owned()],
    }]
}

const CHRONOLOGICAL: Sort = Sort {
    by: SortBy::Dimension,
    dir: SortDir::Asc,
};

const LARGEST_FIRST: Sort = Sort {
    by: SortBy::Value,
    dir: SortDir::Desc,
};

/// Net revenue, month by month, over the last year.
fn revenue_by_month() -> ChartSpec {
    ChartSpec {
        dimension: over_time(Dimension::IssueDate, Grain::Month),
        period: Period::LastN {
            n: 12,
            grain: Grain::Month,
        },
        sort: Some(CHRONOLOGICAL),
        ..spec(
            Dataset::BillingDocuments,
            Measure::Net,
            Aggregate::Sum,
            Viz::Bar,
        )
    }
}

/// Everything still owed on issued documents, as one figure.
fn outstanding() -> ChartSpec {
    spec(
        Dataset::BillingReceivables,
        Measure::Outstanding,
        Aggregate::Sum,
        Viz::Number,
    )
}

/// What is owed, by how late it is.
///
/// Ordered by the bucket rather than by the amount, so the bars keep the one
/// order an aging chart is read in — 0–30, 31–60, 61–90, 90+, and what is not
/// due yet standing apart at the end.
fn overdue_aging() -> ChartSpec {
    ChartSpec {
        dimension: by(Dimension::AgeBucket),
        sort: Some(CHRONOLOGICAL),
        ..spec(
            Dataset::BillingReceivables,
            Measure::Outstanding,
            Aggregate::Sum,
            Viz::Bar,
        )
    }
}

/// VAT charged, quarter by quarter — the shape a return is filed in.
fn vat_by_quarter() -> ChartSpec {
    ChartSpec {
        dimension: over_time(Dimension::IssueDate, Grain::Quarter),
        period: Period::LastN {
            n: 4,
            grain: Grain::Quarter,
        },
        sort: Some(CHRONOLOGICAL),
        ..spec(
            Dataset::BillingDocuments,
            Measure::Vat,
            Aggregate::Sum,
            Viz::Bar,
        )
    }
}

/// Who the year's revenue came from — the ten largest, with the tail folded
/// into one `other` bucket by the engine rather than silently dropped.
fn top_customers() -> ChartSpec {
    ChartSpec {
        dimension: by(Dimension::Customer),
        period: Period::LastN {
            n: 12,
            grain: Grain::Month,
        },
        sort: Some(LARGEST_FIRST),
        limit: Some(10),
        ..spec(
            Dataset::BillingDocuments,
            Measure::Net,
            Aggregate::Sum,
            Viz::Bar,
        )
    }
}

/// Money that actually arrived, month by month. Never restated into one
/// currency: the rate frozen on an invoice is the rate of its tax point, not of
/// the day the money landed (`docs/design/insights.md`).
fn payments_by_month() -> ChartSpec {
    ChartSpec {
        dimension: over_time(Dimension::PaidOn, Grain::Month),
        period: Period::LastN {
            n: 12,
            grain: Grain::Month,
        },
        sort: Some(CHRONOLOGICAL),
        ..spec(
            Dataset::BillingPayments,
            Measure::Amount,
            Aggregate::Sum,
            Viz::Bar,
        )
    }
}

/// What is in the funnel, column by column — open deals only, because a
/// pipeline is what has not been decided yet.
fn pipeline_by_stage() -> ChartSpec {
    ChartSpec {
        dimension: by(Dimension::Stage),
        filters: only(FilterField::Outcome, "open"),
        sort: Some(LARGEST_FIRST),
        ..spec(Dataset::CrmDeals, Measure::Value, Aggregate::Sum, Viz::Bar)
    }
}

/// What was won this month — dated by the day a deal **closed**, which is what
/// the sentence means; without saying so it would read "raised this month and
/// since won", which is a different question.
fn won_this_month() -> ChartSpec {
    ChartSpec {
        period: Period::LastN {
            n: 1,
            grain: Grain::Month,
        },
        period_on: Some(Dimension::ClosedAt),
        filters: only(FilterField::Outcome, "won"),
        ..spec(
            Dataset::CrmDeals,
            Measure::Value,
            Aggregate::Sum,
            Viz::Number,
        )
    }
}

/// How often a decided deal was won, quarter by quarter. A quarter in which
/// nothing closed has no win rate at all, and the engine leaves it absent
/// rather than drawing a 0 % nobody stated.
fn win_rate_by_quarter() -> ChartSpec {
    ChartSpec {
        dimension: over_time(Dimension::ClosedAt, Grain::Quarter),
        period: Period::LastN {
            n: 4,
            grain: Grain::Quarter,
        },
        sort: Some(CHRONOLOGICAL),
        ..spec(
            Dataset::CrmDeals,
            Measure::WinRate,
            Aggregate::Ratio,
            Viz::Line,
        )
    }
}

/// Deal value won, month by month over the last year.
fn won_by_month() -> ChartSpec {
    ChartSpec {
        dimension: over_time(Dimension::ClosedAt, Grain::Month),
        period: Period::LastN {
            n: 12,
            grain: Grain::Month,
        },
        filters: only(FilterField::Outcome, "won"),
        sort: Some(CHRONOLOGICAL),
        ..spec(Dataset::CrmDeals, Measure::Value, Aggregate::Sum, Viz::Bar)
    }
}

// ---- the seed ----------------------------------------------------------------

/// The words the Business overview is written with, handed in by the edge in
/// the language of whoever opened Insights first.
#[derive(Debug, Clone, Default)]
pub struct OverviewSeed {
    /// The board's own name.
    pub name: String,
    /// A caption per key in [`BUSINESS_OVERVIEW`], in any order. A key the
    /// layout wants and this list has not got is a bug in the caller, and it is
    /// refused rather than filled in with something we invented.
    pub captions: Vec<OverviewCaption>,
}

/// One tile's caption, against the gallery key it belongs to.
#[derive(Debug, Clone)]
pub struct OverviewCaption {
    /// The [`GalleryEntry::key`] this caption is for.
    pub key: String,
    /// What the tile is called on the board, in the caller's language.
    pub title: String,
}

/// A validated tile of the seed, ready to be written.
#[derive(Debug)]
struct SeedTile {
    input: NewTile,
    position: f64,
}

/// Checks the whole seed: the board's name, and one non-blank caption for every
/// tile the layout asks for, whose spec still passes the write gate.
///
/// It is *our* input rather than a caller's, so a failure here is a bug — and a
/// bug that hands a tenant a half-built board is worse than one that refuses to
/// build it.
fn normalize_seed(seed: &OverviewSeed) -> Result<(String, Vec<SeedTile>)> {
    let name = normalize_name(&NewDashboard {
        name: seed.name.clone(),
    })?;
    let mut tiles = Vec::with_capacity(BUSINESS_OVERVIEW.len());
    for (index, key) in BUSINESS_OVERVIEW.iter().enumerate() {
        let entry = gallery_entry(key).ok_or_else(|| {
            StoreError::Validation(format!(
                "the overview names {key}, which is not in the gallery"
            ))
        })?;
        let caption = seed
            .captions
            .iter()
            .find(|caption| caption.key == *key)
            .ok_or_else(|| StoreError::Validation(format!("no caption for the {key} tile")))?;
        let title = required(
            &format!("the {key} caption"),
            &caption.title,
            TILE_TITLE_MAX_CHARS,
        )?;
        let spec = entry
            .spec()
            .to_value()
            .map_err(|error| StoreError::Validation(format!("the {key} spec {error}")))?;
        tiles.push(SeedTile {
            input: NewTile {
                title,
                spec,
                span: entry.span,
            },
            // Positions 1, 2, 3 … leave room below the first tile for a
            // fractional insert, exactly as a task board does.
            position: index as f64 + 1.0,
        });
    }
    Ok((name, tiles))
}

impl AccountStore {
    /// The tenant's boards, **seeding the Business overview on first use**.
    ///
    /// A tenant that has never opened Insights is given one working board — the
    /// item's whole point: live numbers with zero clicks, no builder, no setup
    /// form. From the moment it exists it is an ordinary board.
    ///
    /// Seeding is a first-use rule, not an every-read one. A tenant that
    /// deleted the overview is not handed a new one the next morning, because
    /// the question asked is whether the seed has ever *run* (the
    /// `insight_seeds` ledger), not whether the board is still there.
    ///
    /// Two first reads at the same instant produce exactly one board: the loser
    /// of the race on the ledger's primary key writes nothing and simply reads
    /// back what the winner wrote.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the seed itself is malformed (a blank
    /// name, a missing caption); [`StoreError::Db`] on failure.
    pub async fn insight_dashboards_or_seed(&self, seed: &OverviewSeed) -> Result<Vec<Dashboard>> {
        let (name, tiles) = normalize_seed(seed)?;
        if !self.insight_seed_ran(BUSINESS_OVERVIEW_KEY).await? {
            match self.seed_business_overview(&name, &tiles).await {
                // A concurrent first visit won: its board is the tenant's.
                Ok(()) | Err(StoreError::Conflict(_)) => {}
                Err(other) => return Err(other),
            }
        }
        self.insight_dashboards().await
    }

    /// Whether the seed named by `system_key` has ever run for this tenant —
    /// the ledger's question, which survives the board it wrote.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a malformed key; [`StoreError::Db`] on
    /// failure.
    pub async fn insight_seed_ran(&self, system_key: &str) -> Result<bool> {
        let key = normalize_key(system_key)?;
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM insight_seeds WHERE tenant_id = $1 AND system_key = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(&key)
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)
    }

    /// Writes the ledger row, the board and every tile in **one transaction**:
    /// a tenant is never left holding half an overview, and never left with a
    /// ledger row and no board.
    ///
    /// The board is written even when the tenant is already at its dashboard
    /// cap — a runaway guard is not worth withholding the one board the product
    /// promises, and a tenant with thirty boards has evidently found Insights
    /// without our help.
    async fn seed_business_overview(&self, name: &str, tiles: &[SeedTile]) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let claimed = sqlx::query(
            "INSERT INTO insight_seeds (tenant_id, system_key, seeded_by) \
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(BUSINESS_OVERVIEW_KEY)
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if claimed.rows_affected() == 0 {
            // Somebody else is writing it, or already has. Nothing of ours is
            // committed, and the caller reads back their board.
            return Ok(());
        }
        let board = InsightDashboardId::generate();
        insert_dashboard(
            &mut tx,
            self.tenant.as_str(),
            &board,
            name,
            Some(BUSINESS_OVERVIEW_KEY),
            self.user.as_str(),
        )
        .await?;
        for tile in tiles {
            insert_tile(
                &mut tx,
                self.tenant.as_str(),
                &board,
                &tile.input,
                tile.position,
            )
            .await?;
        }
        tx.commit().await.map_err(StoreError::Db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insight_catalog::DimensionKind;
    use crate::insight_catalog::dataset;

    fn seed() -> OverviewSeed {
        OverviewSeed {
            name: "Business overview".to_owned(),
            captions: BUSINESS_OVERVIEW
                .iter()
                .map(|key| OverviewCaption {
                    key: (*key).to_owned(),
                    title: format!("The {key} tile"),
                })
                .collect(),
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    /// The test this file exists for: a prebuilt question that the validator
    /// would refuse is a tile that draws nothing, on a board the tenant never
    /// asked for. Every one of them goes through the same gate a caller's spec
    /// does — including the round trip through the stored JSON, because that is
    /// what a tile actually holds.
    #[test]
    fn every_prebuilt_question_passes_the_write_gate() {
        for entry in GALLERY {
            let spec = entry.spec();
            spec.validate()
                .unwrap_or_else(|e| panic!("{}: {e}", entry.key));
            let stored = spec
                .to_value()
                .unwrap_or_else(|e| panic!("{}: {e}", entry.key));
            let reparsed = ChartSpec::from_value(stored)
                .unwrap_or_else(|e| panic!("{} does not round-trip: {e}", entry.key));
            assert_eq!(
                reparsed, spec,
                "{} changed on the way to storage",
                entry.key
            );
            assert_eq!(reparsed.viz, entry.viz(), "{}", entry.key);
        }
    }

    #[test]
    fn the_gallery_keys_are_unique_and_fit_a_board() {
        for (index, entry) in GALLERY.iter().enumerate() {
            assert!(
                !GALLERY[..index].iter().any(|other| other.key == entry.key),
                "{} is listed twice",
                entry.key
            );
            assert!(
                !entry.key.is_empty()
                    && entry
                        .key
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c == '_'),
                "{} is not a stable key",
                entry.key
            );
            assert!(
                (crate::insight_tiles::TILE_SPAN_MIN..=crate::insight_tiles::TILE_SPAN_MAX)
                    .contains(&entry.span),
                "{} does not fit the grid",
                entry.key
            );
        }
    }

    #[test]
    fn the_overview_is_built_from_the_gallery_and_names_nothing_twice() {
        assert!(!BUSINESS_OVERVIEW.is_empty());
        for (index, key) in BUSINESS_OVERVIEW.iter().enumerate() {
            assert!(gallery_entry(key).is_some(), "{key} is not in the gallery");
            assert!(
                !BUSINESS_OVERVIEW[..index].contains(key),
                "{key} is on the overview twice"
            );
        }
        // Both modules are represented: a business's first board shows what it
        // is owed and what it is chasing, not one of the two.
        let modules: Vec<GalleryModule> = BUSINESS_OVERVIEW
            .iter()
            .filter_map(|key| gallery_entry(key).map(|e| e.module))
            .collect();
        assert!(modules.contains(&GalleryModule::Billing));
        assert!(modules.contains(&GalleryModule::Crm));
    }

    /// A chart whose period narrows on one date while it draws another would be
    /// a sentence nobody could read off the screen. Every prebuilt question
    /// therefore narrows on the date it draws, or says which other one it means.
    #[test]
    fn every_prebuilt_question_dates_itself_the_way_it_reads() {
        for entry in GALLERY {
            let spec = entry.spec();
            let on = spec.period_dimension();
            let entry_kind = dataset(spec.dataset)
                .dimension(on)
                .unwrap_or_else(|| panic!("{}: {on:?} is not a dimension", entry.key));
            assert!(
                matches!(entry_kind.kind, DimensionKind::Time(_)),
                "{}: periods narrow on dates",
                entry.key
            );
            if let Some(DimensionRef { id, grain: Some(_) }) = spec.dimension {
                assert_eq!(
                    on, id,
                    "{} draws one date and narrows on another",
                    entry.key
                );
            }
        }
    }

    #[test]
    fn the_seed_is_checked_as_strictly_as_anything_a_user_types() {
        let (name, tiles) = normalize_seed(&seed()).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(name, "Business overview");
        assert_eq!(tiles.len(), BUSINESS_OVERVIEW.len());
        assert_eq!(tiles[0].position, 1.0);
        assert_eq!(
            tiles[0].input.span,
            gallery_entry(BUSINESS_OVERVIEW[0])
                .map(|e| e.span)
                .unwrap_or_default(),
            "a tile is as wide as its gallery entry asks"
        );

        let mut nameless = seed();
        nameless.name = "   ".to_owned();
        assert!(invalid(normalize_seed(&nameless)).contains("name"));

        let mut missing = seed();
        missing.captions.remove(0);
        assert!(invalid(normalize_seed(&missing)).contains("no caption"));

        let mut blank = seed();
        blank.captions[1].title = String::new();
        assert!(invalid(normalize_seed(&blank)).contains("caption"));
    }
}
