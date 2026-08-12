//! Tenant-scoped owner reads for conversion events. Collection lives behind
//! the anonymous public door ([`crate::site_public_conversions`]); this module
//! is the authenticated mirror and can only answer for the [`AccountStore`]'s
//! tenant.
//!
//! The report is a funnel of **totals**, not of people: views, starts and
//! submits were counted independently, so their ratios describe a conversion
//! point and can never be resolved to a visitor. That is a property of the
//! stored rows, not of this query — there is nothing here to be careful with.

use std::collections::HashMap;

use time::Date;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::SiteId;
use crate::site_public_conversions::ConversionSource;

/// One conversion point of a site over the period, with its three counters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteConversionSource {
    /// The stored source word — `form` today.
    pub kind: String,
    /// The site-owned id of the conversion point.
    pub id: String,
    /// The owner-facing label, or `None` when the source has since been
    /// deleted: its counts stay in the record and are reported as an unnamed
    /// source rather than silently vanishing.
    pub name: Option<String>,
    pub views: u64,
    pub starts: u64,
    pub submits: u64,
}

/// A site's conversion funnel over an inclusive period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteConversionReport {
    /// Every conversion point of the site — including ones with nothing on
    /// them, so an interface can say "no one has seen this form yet" instead
    /// of hiding it — plus any deleted source that still has counts.
    pub sources: Vec<SiteConversionSource>,
    pub views: u64,
    pub starts: u64,
    pub submits: u64,
}

#[derive(sqlx::FromRow)]
struct CounterRow {
    source_kind: String,
    source_id: String,
    stage: String,
    hits: i64,
}

#[derive(sqlx::FromRow)]
struct FormRow {
    id: String,
    name: String,
}

fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

impl AccountStore {
    /// Reads the conversion funnel for a site owned by this tenant. A foreign
    /// or missing site id is indistinguishable and answers `None`; an owned
    /// site with nothing collected answers a report with zeroed counters, so
    /// the interface can tell "not yours" from "nothing yet".
    ///
    /// # Errors
    /// [`StoreError::Db`] when the database cannot answer the report.
    pub async fn site_conversions(
        &self,
        site: &SiteId,
        from: Date,
        to: Date,
    ) -> Result<Option<SiteConversionReport>> {
        let owns = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM sites WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if !owns {
            return Ok(None);
        }

        let forms = sqlx::query_as::<_, FormRow>(
            "SELECT id, name FROM site_forms \
             WHERE tenant_id = $1 AND site_id = $2 ORDER BY created_at, id",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let counters = sqlx::query_as::<_, CounterRow>(
            "SELECT source_kind, source_id, stage, SUM(hits)::BIGINT AS hits \
             FROM site_conversion_daily \
             WHERE tenant_id = $1 AND site_id = $2 AND day BETWEEN $3 AND $4 \
             GROUP BY source_kind, source_id, stage HAVING SUM(hits) > 0",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        // Every form of the site comes first, in creation order, so the list
        // is stable while an owner watches it; a source with counts but no
        // form left follows.
        let mut sources = forms
            .into_iter()
            .map(|form| SiteConversionSource {
                kind: ConversionSource::Form.as_str().to_owned(),
                id: form.id,
                name: Some(form.name),
                views: 0,
                starts: 0,
                submits: 0,
            })
            .collect::<Vec<_>>();
        let mut index = sources
            .iter()
            .enumerate()
            .map(|(position, source)| ((source.kind.clone(), source.id.clone()), position))
            .collect::<HashMap<_, _>>();

        let mut report = SiteConversionReport {
            sources: Vec::new(),
            views: 0,
            starts: 0,
            submits: 0,
        };
        for row in counters {
            let key = (row.source_kind.clone(), row.source_id.clone());
            let position = match index.get(&key) {
                Some(position) => *position,
                None => {
                    sources.push(SiteConversionSource {
                        kind: row.source_kind,
                        id: row.source_id,
                        name: None,
                        views: 0,
                        starts: 0,
                        submits: 0,
                    });
                    let position = sources.len() - 1;
                    index.insert(key, position);
                    position
                }
            };
            let hits = count(row.hits);
            let Some(source) = sources.get_mut(position) else {
                continue;
            };
            match row.stage.as_str() {
                "view" => {
                    source.views += hits;
                    report.views += hits;
                }
                "start" => {
                    source.starts += hits;
                    report.starts += hits;
                }
                "submit" => {
                    source.submits += hits;
                    report.submits += hits;
                }
                // The column's check constraint makes this unreachable; a
                // future stage is simply not reported until it is read.
                _ => continue,
            }
        }

        report.sources = sources;
        Ok(Some(report))
    }
}
