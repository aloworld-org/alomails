-- The two traffic dimensions a server cannot see: how long a page was read,
-- and which outside domain a visitor left for. Both are reported by a tiny
-- script on the published page (the page beacon) rather than derived from a
-- request, so they arrive through a separate public endpoint with its own
-- caps and rate limit.
--
-- No new table: they are values of the existing bounded dimension table, so
-- the privacy shape of 0304 holds unchanged — a bucket label and a hit count,
-- no visitor token, nothing that could join a reading time to a person. Only
-- the closed set of dimension names widens, which is why this migration
-- rewrites one CHECK constraint instead of adding storage:
--
--   * `read_time` — one of six fixed buckets, computed from the reported
--     number of seconds at the collect endpoint; the raw number is never
--     stored, so a precise duration cannot become a fingerprint.
--   * `outbound` — the DNS host a visitor left for, lowercased and bounded to
--     the same shape as a referrer domain. Distinct values per site and day
--     are capped at the door; the overflow bucket is the literal `other`,
--     which cannot be confused for a domain because a stored domain always
--     contains a dot.
ALTER TABLE site_analytics_dimension_daily
    DROP CONSTRAINT site_analytics_dimension_daily_dimension_check;

ALTER TABLE site_analytics_dimension_daily
    ADD CONSTRAINT site_analytics_dimension_daily_dimension_check CHECK (
        dimension IN (
            'campaign', 'country', 'device', 'entry', 'exit',
            'read_time', 'outbound'
        )
    );
