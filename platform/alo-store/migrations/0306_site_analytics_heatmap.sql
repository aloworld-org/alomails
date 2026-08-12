-- Aggregate heatmaps for published pages: where a page was clicked, and how
-- far down it was read. Both are facts a server cannot see, so both arrive
-- through the page beacon (0305) rather than from a request.
--
-- The privacy shape is the strictest of any analytics table we keep, because
-- a coordinate is the dimension most easily turned into a journey:
--
--   * **No identity of any kind is representable.** There is no visitor token
--     column here — not even the day-scoped one page views are counted with —
--     no session, and no time of day. Two clicks by one reader are already
--     indistinguishable from one click by two readers when they are written.
--   * **No coordinate is stored.** A click is reduced at the door to one cell
--     of a fixed 32x64 grid over the page, a scroll to one of ten depth
--     buckets. The grid is the resolution: it is coarse enough that a cell is
--     a region of a layout rather than a pointer position.
--   * **The viewport is a class, never a size.** Three words, matching the
--     device classes of 0304; a pixel width is a fingerprint and is discarded
--     at the boundary that reads it.
--
-- Cells are keyed by the site's own page path, so an owner reads one page's
-- heatmap; the number of distinct paths a site may accumulate in a day is
-- capped in the write door, since the path is named by the visitor's browser.
CREATE TABLE site_analytics_heatmap_daily (
    tenant_id TEXT NOT NULL,
    site_id TEXT NOT NULL,
    day DATE NOT NULL,
    path TEXT NOT NULL CHECK (length(path) BETWEEN 1 AND 2048),
    viewport TEXT NOT NULL CHECK (viewport IN ('phone', 'tablet', 'desktop')),
    metric TEXT NOT NULL CHECK (metric IN ('click', 'scroll')),
    -- Click: the grid column (0-31) and row (0-63) the point fell in.
    -- Scroll: no horizontal meaning, so the column is pinned to zero and the
    -- row is the depth bucket (0-9, each one tenth of the page).
    grid_x SMALLINT NOT NULL CHECK (grid_x BETWEEN 0 AND 31),
    grid_y SMALLINT NOT NULL CHECK (grid_y BETWEEN 0 AND 63),
    hits BIGINT NOT NULL CHECK (hits >= 0),
    CONSTRAINT site_analytics_heatmap_daily_scroll_is_a_bucket CHECK (
        metric <> 'scroll' OR (grid_x = 0 AND grid_y BETWEEN 0 AND 9)
    ),
    PRIMARY KEY (tenant_id, site_id, day, path, viewport, metric, grid_x, grid_y),
    FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites (tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX site_analytics_heatmap_daily_site_day_idx
    ON site_analytics_heatmap_daily (tenant_id, site_id, day DESC);
