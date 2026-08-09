//! Privacy-preserving traffic collection for the anonymous Sites service.
//! The HTTP boundary has already reduced a request to a date, canonical
//! path, referrer **domain**, and a one-way 32-byte daily visitor token before
//! this door is called. Raw connection or request metadata is neither
//! accepted nor representable here.

use time::Date;

use crate::error::{Result, StoreError};
use crate::site_public::{PublishedSite, SitePublicStore};

/// Storage bounds mirror the migration checks and keep an unexpected public
/// request from creating unbounded analytics dimensions.
const PATH_MAX_LEN: usize = 2048;
const REFERRER_DOMAIN_MAX_LEN: usize = 253;

impl SitePublicStore {
    /// Adds one view to the resolved published site's daily aggregate.
    /// `visitor_hash` must be an opaque daily token; the public service never
    /// passes the address or user agent used to derive it.
    ///
    /// The resolved site's private tenant id is used in every inserted key,
    /// so callers cannot choose or cross tenant scope. Repeating the same
    /// token increments hits but not unique visitors.
    ///
    /// # Errors
    /// [`StoreError::Validation`] for a dimension outside its safe bound, or
    /// [`StoreError::Db`] if the atomic aggregate write fails.
    pub async fn record_public_site_view(
        &self,
        site: &PublishedSite,
        day: Date,
        path: &str,
        referrer_domain: &str,
        visitor_hash: &[u8; 32],
    ) -> Result<()> {
        if path.is_empty() || path.len() > PATH_MAX_LEN {
            return Err(StoreError::Validation(
                "analytics path must be between 1 and 2048 bytes".to_owned(),
            ));
        }
        if referrer_domain.len() > REFERRER_DOMAIN_MAX_LEN {
            return Err(StoreError::Validation(
                "analytics referrer domain must be at most 253 bytes".to_owned(),
            ));
        }

        // The aggregate row is created first so the visitor set can point to
        // it. The transaction serializes a first view and keeps hit/unique
        // counters consistent if either statement fails.
        let mut transaction = self.pool().begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO site_analytics_daily \
                 (tenant_id, site_id, day, path, referrer_domain, hits, unique_visitors) \
             VALUES ($1, $2, $3, $4, $5, 1, 0) \
             ON CONFLICT (tenant_id, site_id, day, path, referrer_domain) \
             DO UPDATE SET hits = site_analytics_daily.hits + 1",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(day)
        .bind(path)
        .bind(referrer_domain)
        .execute(&mut *transaction)
        .await
        .map_err(StoreError::Db)?;

        let inserted = sqlx::query(
            "INSERT INTO site_analytics_daily_visitors \
                 (tenant_id, site_id, day, path, referrer_domain, visitor_hash) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT DO NOTHING",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(day)
        .bind(path)
        .bind(referrer_domain)
        .bind(visitor_hash.as_slice())
        .execute(&mut *transaction)
        .await
        .map_err(StoreError::Db)?;

        if inserted.rows_affected() == 1 {
            sqlx::query(
                "UPDATE site_analytics_daily \
                 SET unique_visitors = unique_visitors + 1 \
                 WHERE tenant_id = $1 AND site_id = $2 AND day = $3 \
                   AND path = $4 AND referrer_domain = $5",
            )
            .bind(site.tenant.as_str())
            .bind(site.site.as_str())
            .bind(day)
            .bind(path)
            .bind(referrer_domain)
            .execute(&mut *transaction)
            .await
            .map_err(StoreError::Db)?;
        }

        transaction.commit().await.map_err(StoreError::Db)
    }
}
