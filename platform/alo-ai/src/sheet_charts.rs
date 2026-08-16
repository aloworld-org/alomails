//! Charts in an alo Sheet — alo's own record of one, and the envelope it
//! lives in (ADR 0051).
//!
//! A chart here is **kind, title and the ranges it reads**. It is not a picture
//! and it is not a copy of the numbers: it stores addresses, so a chart can
//! never disagree with the cells it came from — the same rule that makes the
//! Sheet agent cite `B7` instead of restating what `B7` said.
//!
//! ## Why an envelope
//!
//! The stored blob of an alo Sheet is a Univer workbook snapshot, which the
//! editor asks the engine for and persists verbatim. The snapshot is
//! *regenerated* from the engine's state on every save rather than merged into,
//! so a key alo adds to it does not survive the next keystroke. That — not the
//! drawing — is the whole reason charts appeared to need the grid engine's
//! commercial plugin: a plugin is how a foreign structure survives that
//! round-trip.
//!
//! So the chart never enters the round-trip. The blob becomes
//! `{schemaVersion, workbook, charts}`: the engine keeps the grid and never
//! sees a chart, alo keeps the charts and never parses a plugin structure, and
//! a chart outlives an engine change. A blob with no `workbook` key but with
//! `sheets` is the older bare snapshot and is read as one, so every sheet that
//! exists today opens unchanged and gains an envelope the first time it is
//! saved.
//!
//! Nothing here draws. The renderer is `web/src/insights/chart/EChart.tsx`, the
//! one file in the product that imports a chart library, and it is handed the
//! resolved figures below rather than any of this vocabulary.
use serde_json::{Map, Value, json};

use crate::sheet_grid::{Tab, Workbook, cell_ref, parse_a1};

/// The envelope version this module writes. Read is tolerant of anything it
/// understands; write always produces the current shape.
pub const SCHEMA_VERSION: u64 = 1;

/// The three shapes the product can already draw. Deliberately not "whatever
/// the engine supports": a kind here is a promise that `EChart.tsx` renders it,
/// and that file draws bar, line and pie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKind {
    Bar,
    Line,
    Pie,
}

impl ChartKind {
    /// The stored word, which is also what the web model reads.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bar => "bar",
            Self::Line => "line",
            Self::Pie => "pie",
        }
    }

    /// Reads a stored kind. An unknown word is refused rather than defaulted:
    /// silently drawing a pie where a line was asked for is worse than saying
    /// the chart cannot be read.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "bar" => Some(Self::Bar),
            "line" => Some(Self::Line),
            "pie" => Some(Self::Pie),
            _ => None,
        }
    }
}

/// A rectangle of cells, zero-based and inclusive at both ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeRef {
    pub start_row: u32,
    pub start_col: u32,
    pub end_row: u32,
    pub end_col: u32,
}

impl RangeRef {
    /// Reads `A2:A10`, or a single cell `B4` as the range of one.
    ///
    /// Normalised so the start is always the top-left: `C9:A2` and `A2:C9` are
    /// the same rectangle, because a user dragging a selection upwards should
    /// not produce a chart that reads nothing.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let trimmed = text.trim();
        let (first, last) = match trimmed.split_once(':') {
            Some((a, b)) => (a, b),
            None => (trimmed, trimmed),
        };
        let (r1, c1) = parse_a1(first)?;
        let (r2, c2) = parse_a1(last)?;
        Some(Self {
            start_row: r1.min(r2),
            start_col: c1.min(c2),
            end_row: r1.max(r2),
            end_col: c1.max(c2),
        })
    }

    /// The A1 form, which is what is stored and what a person reads.
    #[must_use]
    pub fn reference(&self) -> String {
        if self.start_row == self.end_row && self.start_col == self.end_col {
            return cell_ref(self.start_row, self.start_col);
        }
        format!(
            "{}:{}",
            cell_ref(self.start_row, self.start_col),
            cell_ref(self.end_row, self.end_col)
        )
    }

    /// Every address in the rectangle, row by row.
    fn cells(&self) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        for row in self.start_row..=self.end_row {
            for col in self.start_col..=self.end_col {
                out.push((row, col));
            }
        }
        out
    }

    /// How many cells it covers — the bound a caller checks before asking for
    /// them all.
    #[must_use]
    pub const fn len(&self) -> u64 {
        ((self.end_row - self.start_row) as u64 + 1) * ((self.end_col - self.start_col) as u64 + 1)
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false // a parsed range always covers at least one cell
    }
}

/// One drawn set of figures: a name for the legend, and where to read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartSeriesRef {
    pub name: String,
    pub range: RangeRef,
}

/// A chart as alo stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetChart {
    /// Stable across edits, so a chart can be replaced rather than duplicated.
    pub id: String,
    pub title: String,
    pub kind: ChartKind,
    /// The tab key the ranges are read from — the same key `Tab::key` carries,
    /// so a renamed tab does not orphan its charts.
    pub tab: String,
    /// The labels down the axis, or the slice names of a pie.
    pub categories: RangeRef,
    pub series: Vec<ChartSeriesRef>,
}

/// A chart that will not be drawn, and the reason, so a caller can say which
/// chart is wrong rather than that the workbook is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartError {
    /// The tab it names is not in the workbook — a deleted sheet.
    NoSuchTab,
    /// The category range and a series range are different lengths, so no
    /// point could be paired with a label.
    Ragged,
    /// More cells than a chart is allowed to read.
    TooLarge,
}

impl ChartError {
    /// A code rather than a sentence: the words a person reads are the client's,
    /// in their own language.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoSuchTab => "chartTabMissing",
            Self::Ragged => "chartRangesRagged",
            Self::TooLarge => "chartTooLarge",
        }
    }
}

/// The most cells one chart may read. A chart of a hundred thousand points is
/// not a chart, it is a way to freeze a browser, and the cap is here rather
/// than in the renderer so the same limit holds for an agent proposing one.
pub const MAX_CHART_CELLS: u64 = 5_000;

/// The figures a chart resolves to — plain values, ready to draw, carrying no
/// vocabulary from this module or any chart library.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedChart {
    pub id: String,
    pub title: String,
    pub kind: ChartKind,
    /// One label per point, in range order. A blank cell keeps its place: a
    /// chart that silently closed the gap would shift every later point onto
    /// the wrong label.
    pub categories: Vec<String>,
    pub series: Vec<ResolvedSeries>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedSeries {
    pub name: String,
    /// `None` where the cell was blank or not a number — drawn as a gap, never
    /// as a zero nobody measured.
    pub values: Vec<Option<f64>>,
}

/// The workbook half of a stored blob, whichever shape it is in.
///
/// An envelope hands back its `workbook`; a bare snapshot is its own workbook.
/// Every reader of a sheet goes through here rather than testing for the key
/// itself, so the legacy shape is understood in exactly one place.
#[must_use]
pub fn workbook_value(raw: &Value) -> &Value {
    raw.get("workbook").map_or(raw, |inner| inner)
}

/// The charts stored in a blob, in stored order.
///
/// Tolerant by the same argument as `Workbook::read`: a chart that cannot be
/// understood is skipped, not fatal, because one malformed record must not cost
/// a person the workbook it was in.
#[must_use]
pub fn charts(raw: &Value) -> Vec<SheetChart> {
    raw.get("charts")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(read_chart).collect())
        .unwrap_or_default()
}

fn read_chart(raw: &Value) -> Option<SheetChart> {
    let object = raw.as_object()?;
    let id = object.get("id").and_then(Value::as_str)?.to_owned();
    let kind = ChartKind::parse(object.get("kind").and_then(Value::as_str)?)?;
    let tab = object.get("tab").and_then(Value::as_str)?.to_owned();
    let categories = RangeRef::parse(object.get("categories").and_then(Value::as_str)?)?;
    let series: Vec<ChartSeriesRef> = object
        .get("series")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|entry| {
            let entry = entry.as_object()?;
            Some(ChartSeriesRef {
                name: entry
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                range: RangeRef::parse(entry.get("range").and_then(Value::as_str)?)?,
            })
        })
        .collect();
    // A chart with no readable series draws nothing; it is a record of an
    // intention, not a chart, and keeping it would show an empty frame.
    if series.is_empty() {
        return None;
    }
    Some(SheetChart {
        id,
        title: object
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        kind,
        tab,
        categories,
        series,
    })
}

fn chart_value(chart: &SheetChart) -> Value {
    json!({
        "id": chart.id,
        "title": chart.title,
        "kind": chart.kind.as_str(),
        "tab": chart.tab,
        "categories": chart.categories.reference(),
        "series": chart.series.iter().map(|s| json!({
            "name": s.name,
            "range": s.range.reference(),
        })).collect::<Vec<_>>(),
    })
}

/// Puts a blob back together with this list of charts.
///
/// Always writes the envelope, so a sheet saved by this product is in the
/// current shape whatever it arrived as. The workbook half is carried through
/// **untouched** — this module never edits a grid, and a chart write that
/// altered a cell would be the one bug nobody would look for here.
#[must_use]
pub fn with_charts(raw: &Value, charts: &[SheetChart]) -> Value {
    let mut envelope = Map::new();
    envelope.insert("schemaVersion".to_owned(), json!(SCHEMA_VERSION));
    envelope.insert("workbook".to_owned(), workbook_value(raw).clone());
    envelope.insert(
        "charts".to_owned(),
        Value::Array(charts.iter().map(chart_value).collect()),
    );
    Value::Object(envelope)
}

/// Reads the figures a chart names, against the workbook it belongs to.
///
/// # Errors
/// [`ChartError`] when the tab is gone, the ranges do not line up, or the chart
/// asks for more cells than [`MAX_CHART_CELLS`].
pub fn resolve(chart: &SheetChart, book: &Workbook) -> Result<ResolvedChart, ChartError> {
    let total = chart.categories.len() + chart.series.iter().map(|s| s.range.len()).sum::<u64>();
    if total > MAX_CHART_CELLS {
        return Err(ChartError::TooLarge);
    }
    let tab = book
        .tabs
        .iter()
        .find(|tab| tab.key == chart.tab)
        .ok_or(ChartError::NoSuchTab)?;

    let categories: Vec<String> = chart
        .categories
        .cells()
        .into_iter()
        .map(|(row, col)| {
            tab.cell(row, col)
                .map(|cell| cell.text.clone())
                .unwrap_or_default()
        })
        .collect();

    let mut series = Vec::with_capacity(chart.series.len());
    for entry in &chart.series {
        let cells = entry.range.cells();
        if cells.len() != categories.len() {
            return Err(ChartError::Ragged);
        }
        series.push(ResolvedSeries {
            name: entry.name.clone(),
            values: cells
                .into_iter()
                .map(|(row, col)| numeric(tab, row, col))
                .collect(),
        });
    }

    Ok(ResolvedChart {
        id: chart.id.clone(),
        title: chart.title.clone(),
        kind: chart.kind,
        categories,
        series,
    })
}

/// A cell's figure, or `None` when the sheet does not hold one there.
///
/// Reads `numeric` rather than trying the text: a column of numbers typed as
/// text is the commonest fault in a spreadsheet, and a chart that quietly
/// parsed it would draw a total the grid itself does not agree with.
fn numeric(tab: &Tab, row: u32, col: u32) -> Option<f64> {
    let cell = tab.cell(row, col)?;
    if !cell.numeric {
        return None;
    }
    cell.text.trim().parse::<f64>().ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn book_json() -> Value {
        json!({
            "name": "Q3",
            "sheetOrder": ["s1"],
            "sheets": {
                "s1": {
                    "name": "Sales",
                    "cellData": {
                        "0": { "0": { "v": "Month" }, "1": { "v": "Revenue" } },
                        "1": { "0": { "v": "Jan" },   "1": { "v": 120 } },
                        "2": { "0": { "v": "Feb" },   "1": { "v": 90 } },
                        "3": { "0": { "v": "Mar" },   "1": { "v": "n/a" } }
                    }
                }
            }
        })
    }

    fn a_chart() -> SheetChart {
        SheetChart {
            id: "c1".to_owned(),
            title: "Revenue".to_owned(),
            kind: ChartKind::Bar,
            tab: "s1".to_owned(),
            categories: RangeRef::parse("A2:A4").expect("range"),
            series: vec![ChartSeriesRef {
                name: "Revenue".to_owned(),
                range: RangeRef::parse("B2:B4").expect("range"),
            }],
        }
    }

    #[test]
    fn a_range_reads_either_way_round() {
        // Dragging a selection upwards must not produce a chart of nothing.
        let up = RangeRef::parse("C9:A2").expect("range");
        let down = RangeRef::parse("A2:C9").expect("range");
        assert_eq!(up, down);
        assert_eq!(down.reference(), "A2:C9");
    }

    #[test]
    fn a_single_cell_is_a_range_of_one() {
        let one = RangeRef::parse("B4").expect("range");
        assert_eq!(one.len(), 1);
        assert_eq!(one.reference(), "B4");
    }

    #[test]
    fn an_unknown_kind_is_refused_rather_than_defaulted() {
        // Drawing a pie where a scatter was asked for is worse than refusing.
        assert!(ChartKind::parse("scatter").is_none());
        assert_eq!(ChartKind::parse("LINE"), Some(ChartKind::Line));
    }

    #[test]
    fn a_bare_snapshot_is_its_own_workbook_and_holds_no_charts() {
        // Every sheet that exists today is this shape.
        let raw = book_json();
        assert!(workbook_value(&raw).get("sheets").is_some());
        assert!(charts(&raw).is_empty());
        assert!(Workbook::read(workbook_value(&raw)).is_ok());
    }

    #[test]
    fn an_envelope_round_trips_and_leaves_the_grid_untouched() {
        let raw = book_json();
        let wrapped = with_charts(&raw, &[a_chart()]);
        assert_eq!(wrapped["schemaVersion"], json!(SCHEMA_VERSION));
        // The grid is carried through byte for byte: a chart write that edited
        // a cell is the one bug nobody would look for in this module.
        assert_eq!(workbook_value(&wrapped), &raw);
        assert_eq!(charts(&wrapped), vec![a_chart()]);
        // And wrapping twice does not nest.
        let again = with_charts(&wrapped, &charts(&wrapped));
        assert_eq!(workbook_value(&again), &raw);
        assert_eq!(charts(&again).len(), 1);
    }

    #[test]
    fn a_malformed_chart_is_skipped_rather_than_costing_the_workbook() {
        let mut wrapped = with_charts(&book_json(), &[a_chart()]);
        wrapped["charts"]
            .as_array_mut()
            .expect("array")
            .push(json!({ "id": "bad", "kind": "spiral", "tab": "s1" }));
        assert_eq!(charts(&wrapped).len(), 1);
    }

    #[test]
    fn a_chart_with_no_readable_series_is_not_a_chart() {
        let wrapped = json!({
            "schemaVersion": 1,
            "workbook": book_json(),
            "charts": [{ "id": "c", "kind": "bar", "tab": "s1",
                         "categories": "A2:A4", "series": [] }]
        });
        assert!(charts(&wrapped).is_empty());
    }

    #[test]
    fn resolving_reads_the_cells_and_leaves_a_gap_where_there_is_no_figure() {
        let book = Workbook::read(&book_json()).expect("workbook");
        let drawn = resolve(&a_chart(), &book).expect("resolved");
        assert_eq!(drawn.categories, vec!["Jan", "Feb", "Mar"]);
        // "n/a" is text, so it is a gap — not a zero, and not a parse of the
        // characters, which would draw a figure the grid does not hold.
        assert_eq!(drawn.series[0].values, vec![Some(120.0), Some(90.0), None]);
    }

    #[test]
    fn a_series_that_does_not_line_up_with_its_labels_is_refused() {
        let mut chart = a_chart();
        chart.series[0].range = RangeRef::parse("B2:B3").expect("range");
        assert_eq!(resolve(&chart, &book_json_book()), Err(ChartError::Ragged));
    }

    #[test]
    fn a_chart_naming_a_deleted_tab_says_so() {
        let mut chart = a_chart();
        chart.tab = "gone".to_owned();
        assert_eq!(
            resolve(&chart, &book_json_book()),
            Err(ChartError::NoSuchTab)
        );
    }

    #[test]
    fn a_chart_may_not_ask_for_the_whole_sheet() {
        let mut chart = a_chart();
        chart.categories = RangeRef::parse("A1:A6000").expect("range");
        assert_eq!(
            resolve(&chart, &book_json_book()),
            Err(ChartError::TooLarge)
        );
    }

    fn book_json_book() -> Workbook {
        Workbook::read(&book_json()).expect("workbook")
    }
}
