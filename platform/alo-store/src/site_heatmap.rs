//! Tenant-scoped owner reads for page heatmaps. Collection lives behind the
//! anonymous public door ([`crate::site_public_heatmap`]); this module is the
//! authenticated mirror and can only answer for the [`AccountStore`]'s tenant.

use time::Date;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::SiteId;
use crate::site_public_heatmap::{
    HEATMAP_COLUMNS, HEATMAP_ROWS, SCROLL_DEPTH_BUCKETS, ViewportClass,
};

/// One page a site collected heatmap events for, most-active first. This is
/// how an owner discovers what there is to look at without typing a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteHeatmapPath {
    pub path: String,
    /// Clicks and scroll reports together — a single "how much is here"
    /// number, not a metric anyone should read as traffic.
    pub events: u64,
}

/// One cell of the click grid, with the number of clicks that landed in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SiteHeatmapCell {
    pub column: u16,
    pub row: u16,
    pub hits: u64,
}

/// One tenth of the page and how many readers reached it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SiteHeatmapScrollBucket {
    /// `0..SCROLL_DEPTH_BUCKETS`; bucket `n` covers `n * 10%` to
    /// `(n + 1) * 10%` of the page.
    pub bucket: u16,
    pub hits: u64,
}

/// One page's heatmap as read on one class of screen. Kept separate per class
/// because a layout that reflows makes a shared grid meaningless.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteHeatmapViewport {
    /// `phone`, `tablet`, or `desktop`.
    pub viewport: String,
    pub clicks: Vec<SiteHeatmapCell>,
    pub click_total: u64,
    /// Always [`SCROLL_DEPTH_BUCKETS`] entries in depth order, quiet buckets
    /// included: a depth curve with its empty tenths removed is a different
    /// claim about the same page.
    pub scroll_depth: Vec<SiteHeatmapScrollBucket>,
    pub scroll_total: u64,
}

/// One page's aggregate heatmap over an inclusive period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteHeatmapReport {
    pub path: String,
    /// The grid the cells are expressed in, so a reader never has to assume
    /// it — an overlay is drawn against these numbers.
    pub columns: u16,
    pub rows: u16,
    /// One entry per class of screen, in [`ViewportClass::ORDERED`] order,
    /// including classes with nothing in them.
    pub viewports: Vec<SiteHeatmapViewport>,
}

#[derive(sqlx::FromRow)]
struct PathRow {
    path: String,
    events: i64,
}

#[derive(sqlx::FromRow)]
struct CellRow {
    viewport: String,
    metric: String,
    grid_x: i16,
    grid_y: i16,
    hits: i64,
}

/// How many pages the path list carries. Beyond this it stops being a menu.
const PATH_LIMIT: usize = 50;

fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

fn cell_index(value: i16) -> u16 {
    u16::try_from(value).unwrap_or_default()
}

impl AccountStore {
    /// Lists the pages this tenant's site collected heatmap events for.
    /// A foreign or missing site id is indistinguishable and answers `None`.
    ///
    /// # Errors
    /// [`StoreError::Db`] when the database cannot answer the list.
    pub async fn site_heatmap_paths(
        &self,
        site: &SiteId,
        from: Date,
        to: Date,
    ) -> Result<Option<Vec<SiteHeatmapPath>>> {
        if !self.owns_site(site).await? {
            return Ok(None);
        }
        let rows = sqlx::query_as::<_, PathRow>(
            "SELECT path, SUM(hits)::BIGINT AS events \
             FROM site_analytics_heatmap_daily \
             WHERE tenant_id = $1 AND site_id = $2 AND day BETWEEN $3 AND $4 \
             GROUP BY path HAVING SUM(hits) > 0 \
             ORDER BY events DESC, path LIMIT $5",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(from)
        .bind(to)
        .bind(i64::try_from(PATH_LIMIT).unwrap_or(50))
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(Some(
            rows.into_iter()
                .map(|row| SiteHeatmapPath {
                    path: row.path,
                    events: count(row.events),
                })
                .collect(),
        ))
    }

    /// Reads one page's heatmap for a site owned by this tenant. A foreign or
    /// missing site id is indistinguishable and answers `None`; an owned site
    /// with nothing collected answers an empty grid rather than `None`, so the
    /// interface can tell "not yours" from "nothing yet".
    ///
    /// # Errors
    /// [`StoreError::Db`] when the database cannot answer the report.
    pub async fn site_heatmap(
        &self,
        site: &SiteId,
        path: &str,
        from: Date,
        to: Date,
    ) -> Result<Option<SiteHeatmapReport>> {
        if !self.owns_site(site).await? {
            return Ok(None);
        }
        let rows = sqlx::query_as::<_, CellRow>(
            "SELECT viewport, metric, grid_x, grid_y, SUM(hits)::BIGINT AS hits \
             FROM site_analytics_heatmap_daily \
             WHERE tenant_id = $1 AND site_id = $2 AND path = $3 \
               AND day BETWEEN $4 AND $5 \
             GROUP BY viewport, metric, grid_x, grid_y \
             HAVING SUM(hits) > 0 \
             ORDER BY viewport, metric, grid_y, grid_x",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(path)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let mut viewports = ViewportClass::ORDERED
            .iter()
            .map(|class| SiteHeatmapViewport {
                viewport: class.as_str().to_owned(),
                clicks: Vec::new(),
                click_total: 0,
                scroll_depth: (0..SCROLL_DEPTH_BUCKETS)
                    .map(|bucket| SiteHeatmapScrollBucket { bucket, hits: 0 })
                    .collect(),
                scroll_total: 0,
            })
            .collect::<Vec<_>>();
        for row in rows {
            let Some(target) = viewports
                .iter_mut()
                .find(|entry| entry.viewport == row.viewport)
            else {
                // The column's check constraint makes this unreachable; a
                // future class is simply not reported until it is read.
                continue;
            };
            let hits = count(row.hits);
            match row.metric.as_str() {
                "click" => {
                    target.clicks.push(SiteHeatmapCell {
                        column: cell_index(row.grid_x),
                        row: cell_index(row.grid_y),
                        hits,
                    });
                    target.click_total += hits;
                }
                "scroll" => {
                    let bucket = cell_index(row.grid_y);
                    if let Some(entry) = target
                        .scroll_depth
                        .iter_mut()
                        .find(|entry| entry.bucket == bucket)
                    {
                        entry.hits += hits;
                    }
                    target.scroll_total += hits;
                }
                _ => continue,
            }
        }

        Ok(Some(SiteHeatmapReport {
            path: path.to_owned(),
            columns: HEATMAP_COLUMNS,
            rows: HEATMAP_ROWS,
            viewports,
        }))
    }

    /// Whether this tenant owns the site at all. Asked before any aggregate
    /// query, so a foreign id costs one existence check and reveals nothing.
    async fn owns_site(&self, site: &SiteId) -> Result<bool> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM sites WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)
    }
}
