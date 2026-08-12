//! The anonymous write door for aggregate page heatmaps: where a published
//! page was clicked, and how far down it was read.
//!
//! Like [`crate::site_public_analytics`]'s beacon door, everything arriving
//! here has already been reduced by the public service, and the reduction is
//! the privacy argument rather than a formatting convenience:
//!
//! - A click is a **cell** of a fixed [`HEATMAP_COLUMNS`] x [`HEATMAP_ROWS`]
//!   grid over the page, never a coordinate. There is no type here that can
//!   hold a pixel position.
//! - A scroll is one of [`SCROLL_DEPTH_BUCKETS`] tenths of the page.
//! - A viewport is one of three words, never a size.
//! - **No visitor token exists in this module at all** — not even the daily
//!   one page views carry. These aggregates count events; two events can
//!   never be shown to have come from one reader.
//!
//! The one value a *browser* names freely is the page path, so the number of
//! distinct paths a site may open in a day is capped ([`HEATMAP_DAILY_PATHS`])
//! exactly as outbound domains are.

use time::Date;

use crate::error::{Result, StoreError};
use crate::site_public::{PublishedSite, SitePublicStore};

/// Horizontal resolution of the click grid. Coarse on purpose: a cell should
/// name a region of a layout ("the right-hand call to action"), not a point.
pub const HEATMAP_COLUMNS: u16 = 32;

/// Vertical resolution of the click grid, over the whole scrollable page
/// rather than the viewport, so a cell means the same thing on every screen.
pub const HEATMAP_ROWS: u16 = 64;

/// How many depth buckets a scroll report is reduced to — tenths of the page.
pub const SCROLL_DEPTH_BUCKETS: u16 = 10;

/// How many distinct page paths one site may accumulate heatmap rows for in
/// one day. The path is named by the visitor's browser, so it is the only
/// unbounded key here; past the cap a new path is dropped rather than folded
/// into an overflow bucket, because a heatmap of "some other page" would be
/// an overlay over nothing.
pub const HEATMAP_DAILY_PATHS: i64 = 100;

/// Storage bound for a page path, mirroring the migration's check.
const PATH_MAX_LEN: usize = 2048;

/// The class of screen a page was read on. Derived at the HTTP boundary from
/// a reported width which is discarded there: only one of these three words
/// is ever stored, and it is deliberately the same vocabulary the device
/// dimension already uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportClass {
    Phone,
    Tablet,
    Desktop,
}

impl ViewportClass {
    /// The class a reported CSS pixel width falls in. Total by construction:
    /// an absurd width is still one of three words, because the number itself
    /// never matters and is never kept.
    #[must_use]
    pub const fn from_width(width: u32) -> Self {
        match width {
            0..=599 => Self::Phone,
            600..=1023 => Self::Tablet,
            _ => Self::Desktop,
        }
    }

    /// The stored word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Phone => "phone",
            Self::Tablet => "tablet",
            Self::Desktop => "desktop",
        }
    }

    /// The classes in reading order — the order a report shows them in.
    pub const ORDERED: [Self; 3] = [Self::Phone, Self::Tablet, Self::Desktop];
}

/// One cell of the click grid. The constructor is the only way to make one,
/// so a cell is in range by construction and no coordinate survives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeatmapCell {
    column: u16,
    row: u16,
}

impl HeatmapCell {
    /// The cell a point falls in, given as permille of the page's own width
    /// and height (0-1000). Total: anything at or past the far edge lands in
    /// the last cell rather than being rejected, since a rounding disagreement
    /// between two browsers must not become an error path.
    #[must_use]
    pub const fn from_permille(x: u16, y: u16) -> Self {
        Self {
            column: quantize(x, HEATMAP_COLUMNS),
            row: quantize(y, HEATMAP_ROWS),
        }
    }

    /// The grid column, `0..HEATMAP_COLUMNS`.
    #[must_use]
    pub const fn column(self) -> u16 {
        self.column
    }

    /// The grid row, `0..HEATMAP_ROWS`.
    #[must_use]
    pub const fn row(self) -> u16 {
        self.row
    }
}

/// How far down a page one reader got, in tenths. Same shape and same reason
/// as [`HeatmapCell`]: the reported number is reduced here and forgotten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollDepth {
    bucket: u16,
}

impl ScrollDepth {
    /// The bucket a depth given as permille of the page (0-1000) falls in.
    #[must_use]
    pub const fn from_permille(depth: u16) -> Self {
        Self {
            bucket: quantize(depth, SCROLL_DEPTH_BUCKETS),
        }
    }

    /// The bucket index, `0..SCROLL_DEPTH_BUCKETS`; bucket `n` means the
    /// reader reached between `n * 10%` and `(n + 1) * 10%` of the page.
    #[must_use]
    pub const fn bucket(self) -> u16 {
        self.bucket
    }
}

/// Maps a permille value onto `buckets` equal parts, clamped to the last.
const fn quantize(permille: u16, buckets: u16) -> u16 {
    let scaled = (permille as u32 * buckets as u32) / 1000;
    let last = (buckets - 1) as u32;
    if scaled > last {
        buckets - 1
    } else {
        scaled as u16
    }
}

/// What one heatmap beacon reported about one page. There is deliberately no
/// field for a visitor, a session, or a time of day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeatmapSignal {
    /// A click landed in this cell of the page grid.
    Click(HeatmapCell),
    /// The reader got this far down the page.
    Scroll(ScrollDepth),
}

/// One already-reduced heatmap report for a published page.
#[derive(Debug, Clone, Copy)]
pub struct PublicSiteHeatmapReport<'a> {
    pub day: Date,
    /// Canonical page path, without query or fragment — the same shape a page
    /// view is counted under, so the two reports line up.
    pub path: &'a str,
    pub viewport: ViewportClass,
    pub signal: HeatmapSignal,
}

impl SitePublicStore {
    /// Adds one heatmap event to the resolved published site's daily grid.
    ///
    /// The resolved site's private tenant id is used in the inserted key, so a
    /// caller cannot choose or cross tenant scope. Nothing written identifies
    /// a visitor, so the counters are hits and there is no unique count to
    /// keep.
    ///
    /// A path this site has not seen today is only opened while the site is
    /// under [`HEATMAP_DAILY_PATHS`] distinct paths for the day; past that the
    /// event is dropped and `Ok(())` is returned, because a heatmap is read
    /// per page and an overflow page would be an overlay over nothing. Two
    /// simultaneous first sightings can each pass that check, so the true
    /// ceiling is the cap plus the writer concurrency — a bound, which is all
    /// an abuse limit needs to be.
    ///
    /// # Errors
    /// [`StoreError::Validation`] for a path outside its safe bound, or
    /// [`StoreError::Db`] if the aggregate write fails.
    pub async fn record_public_site_heatmap(
        &self,
        site: &PublishedSite,
        report: &PublicSiteHeatmapReport<'_>,
    ) -> Result<()> {
        validate_path(report.path)?;
        let tenant = site.tenant.as_str();
        let site_id = site.site.as_str();
        let (metric, x, y) = match report.signal {
            HeatmapSignal::Click(cell) => ("click", cell.column(), cell.row()),
            HeatmapSignal::Scroll(depth) => ("scroll", 0, depth.bucket()),
        };
        // Both are bounded by construction; the conversion is the door
        // refusing to widen for a future type that is not.
        let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
            return Err(StoreError::Validation(
                "heatmap cell must fit the stored grid".to_owned(),
            ));
        };

        // Bumping a cell this page already has is the common case and costs
        // one statement; only a genuinely new cell has to ask whether its page
        // may be opened at all.
        let bumped = sqlx::query(
            "UPDATE site_analytics_heatmap_daily SET hits = hits + 1 \
             WHERE tenant_id = $1 AND site_id = $2 AND day = $3 AND path = $4 \
               AND viewport = $5 AND metric = $6 AND grid_x = $7 AND grid_y = $8",
        )
        .bind(tenant)
        .bind(site_id)
        .bind(report.day)
        .bind(report.path)
        .bind(report.viewport.as_str())
        .bind(metric)
        .bind(x)
        .bind(y)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if bumped.rows_affected() == 1 {
            return Ok(());
        }

        let known_path = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM site_analytics_heatmap_daily \
             WHERE tenant_id = $1 AND site_id = $2 AND day = $3 AND path = $4)",
        )
        .bind(tenant)
        .bind(site_id)
        .bind(report.day)
        .bind(report.path)
        .fetch_one(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if !known_path {
            let distinct = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(DISTINCT path) FROM site_analytics_heatmap_daily \
                 WHERE tenant_id = $1 AND site_id = $2 AND day = $3",
            )
            .bind(tenant)
            .bind(site_id)
            .bind(report.day)
            .fetch_one(self.pool())
            .await
            .map_err(StoreError::Db)?;
            if distinct >= HEATMAP_DAILY_PATHS {
                return Ok(());
            }
        }

        sqlx::query(
            "INSERT INTO site_analytics_heatmap_daily \
                 (tenant_id, site_id, day, path, viewport, metric, grid_x, grid_y, hits) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 1) \
             ON CONFLICT (tenant_id, site_id, day, path, viewport, metric, grid_x, grid_y) \
             DO UPDATE SET hits = site_analytics_heatmap_daily.hits + 1",
        )
        .bind(tenant)
        .bind(site_id)
        .bind(report.day)
        .bind(report.path)
        .bind(report.viewport.as_str())
        .bind(metric)
        .bind(x)
        .bind(y)
        .execute(self.pool())
        .await
        .map(|_| ())
        .map_err(StoreError::Db)
    }
}

/// Defensive bound. The public service canonicalizes the path before it gets
/// here; this is the door refusing to widen for a caller that did not.
fn validate_path(path: &str) -> Result<()> {
    if path.starts_with('/') && path.len() <= PATH_MAX_LEN && !path.contains(['?', '#']) {
        Ok(())
    } else {
        Err(StoreError::Validation(
            "heatmap path must be an absolute page path of at most 2048 bytes".to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_becomes_a_cell_and_the_point_is_gone() {
        assert_eq!(
            HeatmapCell::from_permille(0, 0),
            HeatmapCell { column: 0, row: 0 }
        );
        // A cell is 1000/32 permille wide and 1000/64 tall.
        assert_eq!(HeatmapCell::from_permille(31, 0).column(), 0);
        assert_eq!(HeatmapCell::from_permille(32, 0).column(), 1);
        assert_eq!(HeatmapCell::from_permille(500, 500).column(), 16);
        assert_eq!(HeatmapCell::from_permille(500, 500).row(), 32);
        // The far edge and beyond stay inside the grid rather than erroring.
        assert_eq!(HeatmapCell::from_permille(1000, 1000).column(), 31);
        assert_eq!(HeatmapCell::from_permille(1000, 1000).row(), 63);
        assert_eq!(HeatmapCell::from_permille(u16::MAX, u16::MAX).column(), 31);
        assert_eq!(HeatmapCell::from_permille(u16::MAX, u16::MAX).row(), 63);
    }

    #[test]
    fn a_scroll_depth_is_one_of_ten_tenths() {
        assert_eq!(ScrollDepth::from_permille(0).bucket(), 0);
        assert_eq!(ScrollDepth::from_permille(99).bucket(), 0);
        assert_eq!(ScrollDepth::from_permille(100).bucket(), 1);
        assert_eq!(ScrollDepth::from_permille(999).bucket(), 9);
        assert_eq!(ScrollDepth::from_permille(1000).bucket(), 9);
        assert_eq!(ScrollDepth::from_permille(u16::MAX).bucket(), 9);
    }

    #[test]
    fn a_width_becomes_a_class_and_the_width_is_gone() {
        assert_eq!(ViewportClass::from_width(0).as_str(), "phone");
        assert_eq!(ViewportClass::from_width(599).as_str(), "phone");
        assert_eq!(ViewportClass::from_width(600).as_str(), "tablet");
        assert_eq!(ViewportClass::from_width(1023).as_str(), "tablet");
        assert_eq!(ViewportClass::from_width(1024).as_str(), "desktop");
        assert_eq!(ViewportClass::from_width(u32::MAX).as_str(), "desktop");
    }

    #[test]
    fn a_path_is_an_absolute_page_path_or_it_is_refused() {
        assert!(validate_path("/").is_ok());
        assert!(validate_path("/prices").is_ok());
        for hostile in [
            "prices",
            "",
            "https://elsewhere.example/prices",
            "/prices?utm_campaign=spring",
            "/prices#pricing-table",
        ] {
            assert!(validate_path(hostile).is_err(), "{hostile} was accepted");
        }
        let long = format!("/{}", "a".repeat(PATH_MAX_LEN));
        assert!(validate_path(&long).is_err());
    }
}
