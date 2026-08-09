//! Tenant-scoped owner reads for Sites traffic analytics. Collection lives
//! behind the anonymous public door; this module is the authenticated mirror
//! and can only answer for the [`AccountStore`]'s tenant.

use time::Date;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::SiteId;

/// One day in an owner's traffic report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteAnalyticsDay {
    pub day: Date,
    pub visits: u64,
    /// Anonymous daily visitors. The collection token changes each day, so
    /// this is intentionally not a cross-day identity.
    pub unique_visitors: u64,
}

/// One ranked page or referrer dimension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteAnalyticsRank {
    pub label: String,
    pub visits: u64,
    pub unique_visitors: u64,
}

/// The actionable aggregate report for one owned site and inclusive period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteAnalyticsReport {
    pub daily: Vec<SiteAnalyticsDay>,
    pub top_pages: Vec<SiteAnalyticsRank>,
    pub top_referrers: Vec<SiteAnalyticsRank>,
}

#[derive(sqlx::FromRow)]
struct DailyRow {
    day: Date,
    visits: i64,
    unique_visitors: i64,
}

#[derive(sqlx::FromRow)]
struct RankRow {
    label: String,
    visits: i64,
    unique_visitors: i64,
}

fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

impl AccountStore {
    /// Reads traffic for one site owned by this tenant. A foreign or missing
    /// id is indistinguishable and returns `None` before any aggregate query.
    ///
    /// # Errors
    /// [`StoreError::Db`] when the database cannot answer the report.
    pub async fn site_analytics(
        &self,
        site: &SiteId,
        from: Date,
        to: Date,
    ) -> Result<Option<SiteAnalyticsReport>> {
        let owned = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM sites WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if !owned {
            return Ok(None);
        }

        let daily = sqlx::query_as::<_, DailyRow>(
            "SELECT d.day, SUM(d.hits)::BIGINT AS visits, \
                    (SELECT COUNT(DISTINCT v.visitor_hash)::BIGINT \
                     FROM site_analytics_daily_visitors v \
                     WHERE v.tenant_id = d.tenant_id AND v.site_id = d.site_id \
                       AND v.day = d.day) AS unique_visitors \
             FROM site_analytics_daily d \
             WHERE d.tenant_id = $1 AND d.site_id = $2 \
               AND d.day BETWEEN $3 AND $4 \
             GROUP BY d.tenant_id, d.site_id, d.day ORDER BY d.day",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?
        .into_iter()
        .map(|row| SiteAnalyticsDay {
            day: row.day,
            visits: count(row.visits),
            unique_visitors: count(row.unique_visitors),
        })
        .collect();

        let top_pages = sqlx::query_as::<_, RankRow>(
            "SELECT d.path AS label, SUM(d.hits)::BIGINT AS visits, \
                    (SELECT COUNT(DISTINCT (v.day, v.visitor_hash))::BIGINT \
                     FROM site_analytics_daily_visitors v \
                     WHERE v.tenant_id = d.tenant_id AND v.site_id = d.site_id \
                       AND v.path = d.path AND v.day BETWEEN $3 AND $4) AS unique_visitors \
             FROM site_analytics_daily d \
             WHERE d.tenant_id = $1 AND d.site_id = $2 \
               AND d.day BETWEEN $3 AND $4 \
             GROUP BY d.tenant_id, d.site_id, d.path \
             ORDER BY visits DESC, d.path LIMIT 10",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?
        .into_iter()
        .map(|row| SiteAnalyticsRank {
            label: row.label,
            visits: count(row.visits),
            unique_visitors: count(row.unique_visitors),
        })
        .collect();

        let top_referrers = sqlx::query_as::<_, RankRow>(
            "SELECT d.referrer_domain AS label, SUM(d.hits)::BIGINT AS visits, \
                    (SELECT COUNT(DISTINCT (v.day, v.visitor_hash))::BIGINT \
                     FROM site_analytics_daily_visitors v \
                     WHERE v.tenant_id = d.tenant_id AND v.site_id = d.site_id \
                       AND v.referrer_domain = d.referrer_domain \
                       AND v.day BETWEEN $3 AND $4) AS unique_visitors \
             FROM site_analytics_daily d \
             WHERE d.tenant_id = $1 AND d.site_id = $2 \
               AND d.day BETWEEN $3 AND $4 \
             GROUP BY d.tenant_id, d.site_id, d.referrer_domain \
             ORDER BY visits DESC, d.referrer_domain LIMIT 10",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?
        .into_iter()
        .map(|row| SiteAnalyticsRank {
            label: row.label,
            visits: count(row.visits),
            unique_visitors: count(row.unique_visitors),
        })
        .collect();

        Ok(Some(SiteAnalyticsReport {
            daily,
            top_pages,
            top_referrers,
        }))
    }
}
