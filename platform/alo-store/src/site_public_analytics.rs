//! Privacy-preserving traffic collection for the anonymous Sites service.
//! The HTTP boundary has already reduced a request to a date, canonical
//! path, referrer **domain**, campaign label, country code, device class, and
//! a one-way 32-byte daily visitor token before this door is called. Raw
//! connection or request metadata is neither accepted nor representable here.

use time::Date;

use crate::error::{Result, StoreError};
use crate::site_public::{PublishedSite, SitePublicStore};

/// Storage bounds mirror the migration checks and keep an unexpected public
/// request from creating unbounded analytics dimensions.
const PATH_MAX_LEN: usize = 2048;
const REFERRER_DOMAIN_MAX_LEN: usize = 253;
const CAMPAIGN_MAX_LEN: usize = 64;

/// The class of device a page was read on, as coarse as it can be while
/// staying useful. Derived at the HTTP boundary from the user agent, which is
/// discarded there: only one of these four words is ever stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceClass {
    Phone,
    Tablet,
    Desktop,
    /// Automated traffic. Counted rather than hidden, so an owner can see how
    /// much of a number is not a reader.
    Bot,
    /// The request named no device at all. Reported as its own bucket rather
    /// than folded into a class it might not be.
    Unknown,
}

impl DeviceClass {
    /// The stored word for this class.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Phone => "phone",
            Self::Tablet => "tablet",
            Self::Desktop => "desktop",
            Self::Bot => "bot",
            Self::Unknown => "unknown",
        }
    }
}

/// One safe, already-reduced page view. Every field is a derivative the
/// public service computed and can defend; there is deliberately no field
/// that could carry an address, a user agent, or a raw query string.
#[derive(Debug, Clone, Copy)]
pub struct PublicSiteVisit<'a> {
    pub day: Date,
    /// Canonical page path, without query or fragment.
    pub path: &'a str,
    /// Referrer reduced to its DNS host; empty means direct or unknown.
    pub referrer_domain: &'a str,
    /// The `utm_campaign` label, lowercased and bounded; empty means none.
    pub campaign: &'a str,
    /// Two-letter country code as reported by the edge proxy, uppercase;
    /// empty means unknown. Never derived from an address inside alo.
    pub country: &'a str,
    pub device: DeviceClass,
    /// Opaque daily visitor token. The service never passes the address or
    /// user agent used to derive it.
    pub visitor_hash: &'a [u8; 32],
}

impl SitePublicStore {
    /// Adds one view to the resolved published site's daily aggregates.
    ///
    /// The resolved site's private tenant id is used in every inserted key,
    /// so callers cannot choose or cross tenant scope. Repeating the same
    /// token increments hits but not unique visitors. The first page a token
    /// sees in a day counts as an entry; the page it is last seen on counts
    /// as an exit, and moving on hands that exit to the newer page — so a
    /// visitor-day contributes exactly one entry and one exit without any
    /// journey being stored.
    ///
    /// # Errors
    /// [`StoreError::Validation`] for a dimension outside its safe bound, or
    /// [`StoreError::Db`] if the atomic aggregate write fails.
    pub async fn record_public_site_view(
        &self,
        site: &PublishedSite,
        visit: &PublicSiteVisit<'_>,
    ) -> Result<()> {
        validate(visit)?;
        let tenant = site.tenant.as_str();
        let site_id = site.site.as_str();

        // The aggregate row is created first so the visitor set can point to
        // it. The transaction serializes a first view and keeps hit/unique
        // counters consistent if any statement fails.
        let mut transaction = self.pool().begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO site_analytics_daily \
                 (tenant_id, site_id, day, path, referrer_domain, hits, unique_visitors) \
             VALUES ($1, $2, $3, $4, $5, 1, 0) \
             ON CONFLICT (tenant_id, site_id, day, path, referrer_domain) \
             DO UPDATE SET hits = site_analytics_daily.hits + 1",
        )
        .bind(tenant)
        .bind(site_id)
        .bind(visit.day)
        .bind(visit.path)
        .bind(visit.referrer_domain)
        .execute(&mut *transaction)
        .await
        .map_err(StoreError::Db)?;

        let inserted = sqlx::query(
            "INSERT INTO site_analytics_daily_visitors \
                 (tenant_id, site_id, day, path, referrer_domain, visitor_hash) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT DO NOTHING",
        )
        .bind(tenant)
        .bind(site_id)
        .bind(visit.day)
        .bind(visit.path)
        .bind(visit.referrer_domain)
        .bind(visit.visitor_hash.as_slice())
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
            .bind(tenant)
            .bind(site_id)
            .bind(visit.day)
            .bind(visit.path)
            .bind(visit.referrer_domain)
            .execute(&mut *transaction)
            .await
            .map_err(StoreError::Db)?;
        }

        for (dimension, value) in [
            ("campaign", visit.campaign),
            ("country", visit.country),
            ("device", visit.device.as_str()),
        ] {
            add_dimension_hit(
                &mut transaction,
                tenant,
                site_id,
                visit.day,
                dimension,
                value,
            )
            .await?;
        }

        // The cursor row is locked before it is read so two simultaneous
        // views by one token cannot both believe they are the first, and
        // cannot both hand the exit to their own page.
        let previous = sqlx::query_scalar::<_, String>(
            "SELECT last_path FROM site_analytics_visitor_day \
             WHERE tenant_id = $1 AND site_id = $2 AND day = $3 AND visitor_hash = $4 \
             FOR UPDATE",
        )
        .bind(tenant)
        .bind(site_id)
        .bind(visit.day)
        .bind(visit.visitor_hash.as_slice())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(StoreError::Db)?;

        match previous {
            None => {
                sqlx::query(
                    "INSERT INTO site_analytics_visitor_day \
                         (tenant_id, site_id, day, visitor_hash, last_path) \
                     VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(tenant)
                .bind(site_id)
                .bind(visit.day)
                .bind(visit.visitor_hash.as_slice())
                .bind(visit.path)
                .execute(&mut *transaction)
                .await
                .map_err(StoreError::Db)?;
                add_dimension_hit(
                    &mut transaction,
                    tenant,
                    site_id,
                    visit.day,
                    "entry",
                    visit.path,
                )
                .await?;
                add_dimension_hit(
                    &mut transaction,
                    tenant,
                    site_id,
                    visit.day,
                    "exit",
                    visit.path,
                )
                .await?;
            }
            Some(last_path) if last_path != visit.path => {
                sqlx::query(
                    "UPDATE site_analytics_visitor_day SET last_path = $5 \
                     WHERE tenant_id = $1 AND site_id = $2 AND day = $3 AND visitor_hash = $4",
                )
                .bind(tenant)
                .bind(site_id)
                .bind(visit.day)
                .bind(visit.visitor_hash.as_slice())
                .bind(visit.path)
                .execute(&mut *transaction)
                .await
                .map_err(StoreError::Db)?;
                // The page the visitor left counts as an exit no longer; the
                // page they moved to does. `GREATEST` keeps the counter at or
                // above zero even if the earlier row was pruned.
                sqlx::query(
                    "UPDATE site_analytics_dimension_daily \
                     SET hits = GREATEST(hits - 1, 0) \
                     WHERE tenant_id = $1 AND site_id = $2 AND day = $3 \
                       AND dimension = 'exit' AND value = $4",
                )
                .bind(tenant)
                .bind(site_id)
                .bind(visit.day)
                .bind(last_path.as_str())
                .execute(&mut *transaction)
                .await
                .map_err(StoreError::Db)?;
                add_dimension_hit(
                    &mut transaction,
                    tenant,
                    site_id,
                    visit.day,
                    "exit",
                    visit.path,
                )
                .await?;
            }
            // Re-reading the same page moves neither entry nor exit.
            Some(_) => {}
        }

        transaction.commit().await.map_err(StoreError::Db)
    }
}

/// Defensive bounds. The public service normalizes every dimension before it
/// gets here; this is the door refusing to widen for a caller that did not.
fn validate(visit: &PublicSiteVisit<'_>) -> Result<()> {
    if visit.path.is_empty() || visit.path.len() > PATH_MAX_LEN {
        return Err(StoreError::Validation(
            "analytics path must be between 1 and 2048 bytes".to_owned(),
        ));
    }
    if visit.referrer_domain.len() > REFERRER_DOMAIN_MAX_LEN {
        return Err(StoreError::Validation(
            "analytics referrer domain must be at most 253 bytes".to_owned(),
        ));
    }
    if visit.campaign.len() > CAMPAIGN_MAX_LEN {
        return Err(StoreError::Validation(
            "analytics campaign must be at most 64 bytes".to_owned(),
        ));
    }
    if !visit.campaign.is_empty()
        && !visit
            .campaign
            .chars()
            .all(|character| matches!(character, 'a'..='z' | '0'..='9' | '-' | '_' | '.' | ' '))
    {
        return Err(StoreError::Validation(
            "analytics campaign must be a lowercase label".to_owned(),
        ));
    }
    if !visit.country.is_empty()
        && (visit.country.len() != 2
            || !visit.country.bytes().all(|byte| byte.is_ascii_uppercase()))
    {
        return Err(StoreError::Validation(
            "analytics country must be an uppercase two-letter code".to_owned(),
        ));
    }
    Ok(())
}

/// Adds one hit to a dimension bucket inside the caller's transaction.
async fn add_dimension_hit(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    site: &str,
    day: Date,
    dimension: &str,
    value: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO site_analytics_dimension_daily \
             (tenant_id, site_id, day, dimension, value, hits) \
         VALUES ($1, $2, $3, $4, $5, 1) \
         ON CONFLICT (tenant_id, site_id, day, dimension, value) \
         DO UPDATE SET hits = site_analytics_dimension_daily.hits + 1",
    )
    .bind(tenant)
    .bind(site)
    .bind(day)
    .bind(dimension)
    .bind(value)
    .execute(&mut **transaction)
    .await
    .map(|_| ())
    .map_err(StoreError::Db)
}
