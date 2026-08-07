//! Insights tiles — the questions pinned to a dashboard (ADR 0037, wave
//! BI-1), reached through the account door like every other business record.
//!
//! A tile stores a **ChartSpec** ([`crate::insight_spec`]) and nothing
//! computed: no snapshot, no cached total, no results table. A stored subtotal
//! outlives the rows that justify it, and a fast number that disagrees with
//! the invoice underneath it is worse than a slow one — so a tile holds the
//! question and the answer is evaluated from the documents each time
//! (`docs/design/insights.md`).
//!
//! Two rules give this module its shape:
//!
//! - **Strict on write.** Every spec goes through the typed model before it is
//!   stored, and what lands in the column is the *canonical* serialisation of
//!   the parsed value — so whatever is on disk always round-trips through the
//!   Rust types. The `viz` column is derived from that same parsed value and
//!   never taken from the caller separately, so it cannot drift.
//! - **Tolerant on read.** A tile whose spec this build cannot parse — one
//!   written by a newer version mid-deploy — comes back marked
//!   [`TileSpec::Unreadable`] rather than failing the read. A dashboard never
//!   breaks because one tile is from the future.

use serde_json::Value;
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_field::required;
use crate::error::{Result, StoreError};
use crate::id::{InsightDashboardId, InsightTileId};
use crate::insight_catalog::Viz;
use crate::insight_spec::ChartSpec;

/// A tile title is a caption, not a sentence.
pub const TILE_TITLE_MAX_CHARS: usize = 120;

/// Most tiles one dashboard may hold. Past this a board is a report, and a
/// report is a different feature.
pub const TILES_PER_DASHBOARD_MAX: i64 = 40;

/// Narrowest a tile may be, in grid columns.
pub const TILE_SPAN_MIN: i16 = 1;
/// Widest a tile may be, in grid columns — the full row.
pub const TILE_SPAN_MAX: i16 = 4;

/// The columns every read of a tile selects, in `TileRow` order.
const TILE_COLS: &str =
    "id, dashboard_id, title, spec, viz, position, span, created_at, updated_at";

/// The writable shape of a tile. Used for both pinning and editing — an edit
/// is a full replace, with the route layer merging a partial `PATCH` onto the
/// stored record before calling.
#[derive(Debug, Clone)]
pub struct NewTile {
    /// The caption on the board. Required, non-blank.
    pub title: String,
    /// The ChartSpec envelope, as it arrived on the wire.
    pub spec: Value,
    /// How many of the four grid columns the tile occupies.
    pub span: i16,
}

impl Default for NewTile {
    fn default() -> Self {
        NewTile {
            title: String::new(),
            spec: Value::Null,
            span: TILE_SPAN_MIN,
        }
    }
}

/// A tile's stored question, as this build can read it.
#[derive(Debug, Clone)]
pub enum TileSpec {
    /// A spec this build understands.
    Readable(Box<ChartSpec>),
    /// A spec this build cannot parse — almost always one written by a newer
    /// version during a deploy. The raw JSON is handed back untouched so a
    /// newer client can still render it, and the reason says why we could not.
    Unreadable {
        /// The stored envelope, exactly as it is on disk.
        raw: Value,
        /// Why this build could not read it.
        reason: String,
    },
}

impl TileSpec {
    /// The parsed spec, or `None` when this build cannot read it.
    pub fn readable(&self) -> Option<&ChartSpec> {
        match self {
            TileSpec::Readable(spec) => Some(spec),
            TileSpec::Unreadable { .. } => None,
        }
    }
}

/// A stored tile.
#[derive(Debug, Clone)]
pub struct Tile {
    /// Opaque id, unique within the tenant.
    pub id: InsightTileId,
    /// The board it is pinned to.
    pub dashboard_id: InsightDashboardId,
    /// The caption on the board.
    pub title: String,
    /// The question it asks.
    pub spec: TileSpec,
    /// The chart form, derived from the spec when it was written. `None` when
    /// the stored word is one this build does not know — which travels with an
    /// unreadable spec, and lets the UI draw a placeholder rather than nothing.
    pub viz: Option<Viz>,
    /// Fractional ordering on the board: an ordering, never a quantity.
    pub position: f64,
    /// How many of the four grid columns it occupies.
    pub span: i16,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time.
    pub updated_at: OffsetDateTime,
}

/// A validated tile ready to be bound into a statement.
#[derive(Debug)]
struct Normalized {
    title: String,
    /// The canonical serialisation of the parsed spec.
    spec: Value,
    /// The wire word for the spec's chart form.
    viz: String,
    span: i16,
}

/// Validates and normalises a whole tile: the caption, the spec (through the
/// typed model — this is the write gate), and the span. Pure apart from the
/// spec parse, so the rules are unit-tested directly.
fn normalize(input: &NewTile) -> Result<Normalized> {
    let title = required("title", &input.title, TILE_TITLE_MAX_CHARS)?;
    let spec = ChartSpec::from_value(input.spec.clone())
        .map_err(|error| StoreError::Validation(format!("spec {error}")))?;
    let canonical = spec
        .to_value()
        .map_err(|error| StoreError::Validation(format!("spec {error}")))?;
    if !(TILE_SPAN_MIN..=TILE_SPAN_MAX).contains(&input.span) {
        return Err(StoreError::Validation(format!(
            "span must be between {TILE_SPAN_MIN} and {TILE_SPAN_MAX} columns"
        )));
    }
    Ok(Normalized {
        title,
        viz: viz_word(spec.viz)?,
        spec: canonical,
        span: input.span,
    })
}

/// The wire word for a chart form, read back through serde so the stored
/// column can never drift from the vocabulary the spec speaks.
fn viz_word(viz: Viz) -> Result<String> {
    serde_json::to_value(viz)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| StoreError::Validation("spec viz is not a known chart form".to_owned()))
}

/// Rejects a position that is not a real place on the board. `NaN` compares
/// false against everything, so it would sort unpredictably.
fn check_position(position: f64) -> Result<()> {
    if !position.is_finite() {
        return Err(StoreError::Validation(
            "position must be a finite number".to_owned(),
        ));
    }
    Ok(())
}

/// A tile can only ever be pinned to a board of its own tenant, and the
/// composite foreign key is what enforces it. A violation therefore means the
/// board is not this tenant's (or does not exist) — the same clean denial a
/// missing row gets, with no way to tell the two apart.
fn map_dashboard_missing(error: sqlx::Error) -> StoreError {
    match error {
        sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23503") => {
            StoreError::NotFound
        }
        other => StoreError::Db(other),
    }
}

impl AccountStore {
    /// Pins a spec to a board as a new tile, at the end of the layout.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a blank or over-long title, a spec the
    /// typed model rejects (the message names the offending field), a span
    /// outside the grid, or a board already holding
    /// [`TILES_PER_DASHBOARD_MAX`] tiles; [`StoreError::NotFound`] when the
    /// board isn't the tenant's; [`StoreError::Db`] on failure.
    pub async fn create_insight_tile(
        &self,
        dashboard: &InsightDashboardId,
        input: &NewTile,
    ) -> Result<InsightTileId> {
        let tile = normalize(input)?;
        let id = InsightTileId::generate();
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let held: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM insight_tiles WHERE tenant_id = $1 AND dashboard_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(dashboard.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if held >= TILES_PER_DASHBOARD_MAX {
            return Err(StoreError::Validation(format!(
                "a dashboard may hold at most {TILES_PER_DASHBOARD_MAX} tiles"
            )));
        }
        // Positions 1, 2, 3 … leave room below the first tile for a fractional
        // insert, exactly as a task board does.
        let position: f64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM insight_tiles \
             WHERE tenant_id = $1 AND dashboard_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(dashboard.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO insight_tiles \
             (tenant_id, id, dashboard_id, title, spec, viz, position, span) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(dashboard.as_str())
        .bind(&tile.title)
        .bind(sqlx::types::Json(&tile.spec))
        .bind(&tile.viz)
        .bind(position)
        .bind(tile.span)
        .execute(&mut *tx)
        .await
        .map_err(map_dashboard_missing)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// One board's tiles, in layout order.
    ///
    /// A board that isn't the tenant's yields an empty list rather than an
    /// error — the same answer as a board with no tiles, so the read reveals
    /// nothing about whose board it is. The route reads the dashboard first,
    /// which is where the `404` comes from.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn insight_tiles(&self, dashboard: &InsightDashboardId) -> Result<Vec<Tile>> {
        let rows = sqlx::query_as::<_, TileRow>(&format!(
            "SELECT {TILE_COLS} FROM insight_tiles \
             WHERE tenant_id = $1 AND dashboard_id = $2 ORDER BY position, created_at, id"
        ))
        .bind(self.tenant.as_str())
        .bind(dashboard.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(TileRow::into_tile).collect())
    }

    /// One tile of the tenant, or `None` — including when the id belongs to
    /// another tenant, which is indistinguishable by design.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn insight_tile(&self, id: &InsightTileId) -> Result<Option<Tile>> {
        let row = sqlx::query_as::<_, TileRow>(&format!(
            "SELECT {TILE_COLS} FROM insight_tiles WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(TileRow::into_tile))
    }

    /// Replaces a tile's title, spec and span. Its board and its place on that
    /// board are not writable here: moving is [`Self::move_insight_tile`], so
    /// editing a chart can never make it jump across the layout.
    ///
    /// Replacing the spec re-runs the whole write gate, so a tile that was
    /// stored as unreadable is rewritten as a spec this build understands —
    /// or refused.
    ///
    /// # Errors
    /// [`StoreError::Validation`] as for create; [`StoreError::NotFound`] when
    /// the tile isn't the tenant's; [`StoreError::Db`] on failure.
    pub async fn update_insight_tile(&self, id: &InsightTileId, input: &NewTile) -> Result<()> {
        let tile = normalize(input)?;
        let done = sqlx::query(
            "UPDATE insight_tiles \
             SET title = $3, spec = $4, viz = $5, span = $6, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&tile.title)
        .bind(sqlx::types::Json(&tile.spec))
        .bind(&tile.viz)
        .bind(tile.span)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Moves a tile to `position` on its board — the fractional place between
    /// its new neighbours, which the caller computes exactly as a task board
    /// does.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the position is not a finite number;
    /// [`StoreError::NotFound`] when the tile isn't the tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn move_insight_tile(&self, id: &InsightTileId, position: f64) -> Result<()> {
        check_position(position)?;
        let done = sqlx::query(
            "UPDATE insight_tiles SET position = $3, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(position)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Unpins a tile. Nothing is lost that the documents underneath do not
    /// still hold — a tile is a question, not a record.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the tile isn't the tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_insight_tile(&self, id: &InsightTileId) -> Result<()> {
        let done = sqlx::query("DELETE FROM insight_tiles WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct TileRow {
    id: String,
    dashboard_id: String,
    title: String,
    spec: sqlx::types::Json<Value>,
    viz: String,
    position: f64,
    span: i16,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl TileRow {
    fn into_tile(self) -> Tile {
        let raw = self.spec.0;
        let spec = match ChartSpec::from_value(raw.clone()) {
            Ok(spec) => TileSpec::Readable(Box::new(spec)),
            Err(error) => TileSpec::Unreadable {
                raw,
                reason: error.to_string(),
            },
        };
        Tile {
            id: InsightTileId::new(self.id),
            dashboard_id: InsightDashboardId::new(self.dashboard_id),
            title: self.title,
            spec,
            viz: serde_json::from_value(Value::String(self.viz)).ok(),
            position: self.position,
            span: self.span,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Outstanding receivables as one figure — the Business overview's first
    /// tile, and the fixture the rest of these tests bend.
    fn outstanding() -> Value {
        json!({
            "schema_version": 1,
            "dataset": "billing.receivables",
            "measure": { "id": "outstanding", "agg": "sum" },
            "period": { "kind": "all" },
            "viz": "number"
        })
    }

    fn tile() -> NewTile {
        NewTile {
            title: "  Outstanding  ".to_owned(),
            spec: outstanding(),
            span: 2,
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn a_tile_is_trimmed_and_its_spec_stored_canonically() {
        let normalized = normalize(&tile()).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(normalized.title, "Outstanding");
        assert_eq!(normalized.span, 2);
        // The chart form is DERIVED from the spec, never taken separately.
        assert_eq!(normalized.viz, "number");
        // What is stored parses back through the typed model.
        assert!(ChartSpec::from_value(normalized.spec).is_ok());
    }

    #[test]
    fn a_title_is_required_and_bounded() {
        for blank in ["", "   "] {
            let input = NewTile {
                title: blank.to_owned(),
                ..tile()
            };
            assert!(invalid(normalize(&input)).contains("title"));
        }
        let over = NewTile {
            title: "x".repeat(TILE_TITLE_MAX_CHARS + 1),
            ..tile()
        };
        assert!(invalid(normalize(&over)).contains("at most"));
    }

    #[test]
    fn the_spec_write_gate_names_the_field_it_refused() {
        let mut broken = outstanding();
        broken["measure"] = json!({ "id": "value", "agg": "sum" });
        let input = NewTile {
            spec: broken,
            ..tile()
        };
        let message = invalid(normalize(&input));
        assert!(message.starts_with("spec measure"), "{message}");

        let input = NewTile {
            spec: json!({ "dataset": "billing.receivables" }),
            ..tile()
        };
        assert!(invalid(normalize(&input)).contains("spec chart spec"));
    }

    #[test]
    fn a_span_must_fit_the_grid() {
        for bad in [i16::MIN, 0, TILE_SPAN_MAX + 1, i16::MAX] {
            let input = NewTile {
                span: bad,
                ..tile()
            };
            assert!(invalid(normalize(&input)).contains("span"));
        }
        for ok in TILE_SPAN_MIN..=TILE_SPAN_MAX {
            let input = NewTile { span: ok, ..tile() };
            assert!(normalize(&input).is_ok(), "expected valid span: {ok}");
        }
    }

    #[test]
    fn a_position_must_be_a_real_place_on_the_board() {
        for ok in [0.0, 1.5, -3.0, f64::MIN, f64::MAX] {
            assert!(check_position(ok).is_ok(), "expected valid: {ok}");
        }
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(invalid(check_position(bad)).contains("finite"));
        }
    }

    #[test]
    fn a_spec_from_the_future_reads_back_marked_unreadable() {
        // What a newer version might have written. The read must hand it back
        // rather than fail: one tile from the future never breaks a board.
        let stored = json!({ "schema_version": 2, "dataset": "billing.documents" });
        let row = TileRow {
            id: "t1".to_owned(),
            dashboard_id: "d1".to_owned(),
            title: "Later".to_owned(),
            spec: sqlx::types::Json(stored.clone()),
            viz: "sankey".to_owned(),
            position: 1.0,
            span: 1,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        };
        let tile = row.into_tile();
        assert!(tile.spec.readable().is_none());
        match tile.spec {
            TileSpec::Unreadable { raw, reason } => {
                assert_eq!(raw, stored, "the raw envelope is handed back untouched");
                assert!(reason.contains("schema_version"), "{reason}");
            }
            TileSpec::Readable(spec) => panic!("expected unreadable, got {spec:?}"),
        }
        assert_eq!(tile.viz, None, "an unknown chart form is not guessed at");
    }
}
