//! DMARC aggregate-report event log (RFC 7489 §7.2) — the host-level
//! record behind outbound `rua=` reports. The MX records one row per
//! evaluated inbound message; the reporter job aggregates a window per
//! From-domain, sends the report, and deletes the window. No tenant
//! and no message content ever touch this table: the columns are
//! precisely the fields the report discloses to the domain owner.

use crate::Store;
use crate::error::Result;

/// One recorded DMARC evaluation (write side).
#[derive(Debug, Clone)]
pub struct DmarcEventRecord {
    /// RFC 5322 From domain the policy was evaluated for.
    pub from_domain: String,
    /// Connecting client IP.
    pub source_ip: String,
    /// Applied disposition token: `none` | `quarantine` | `reject`.
    pub disposition: String,
    /// DKIM alignment outcome.
    pub dkim_aligned: bool,
    /// SPF alignment outcome.
    pub spf_aligned: bool,
}

/// One aggregated report row (read side): identical evaluations from
/// one source IP collapsed to a count (Appendix C `<record>`).
#[derive(Debug, Clone)]
pub struct DmarcAggregateRow {
    /// Source IP the messages arrived from.
    pub source_ip: String,
    /// How many messages shared this exact outcome.
    pub count: i64,
    /// Applied disposition token.
    pub disposition: String,
    /// DKIM alignment outcome.
    pub dkim_aligned: bool,
    /// SPF alignment outcome.
    pub spf_aligned: bool,
}

impl Store {
    /// Records one DMARC evaluation for later aggregate reporting.
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn record_dmarc_event(&self, event: &DmarcEventRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO dmarc_report_events \
             (from_domain, source_ip, disposition, dkim_aligned, spf_aligned) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(event.from_domain.to_ascii_lowercase())
        .bind(&event.source_ip)
        .bind(&event.disposition)
        .bind(event.dkim_aligned)
        .bind(event.spf_aligned)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Domains with at least one unreported event strictly before the
    /// epoch-second `cutoff`, bounded by `limit` (oldest-first so a
    /// backlog drains fairly across ticks).
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn dmarc_report_domains(&self, cutoff: i64, limit: i64) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT from_domain FROM dmarc_report_events \
             WHERE evaluated_at < to_timestamp($1) \
             GROUP BY from_domain ORDER BY MIN(evaluated_at) LIMIT $2",
        )
        .bind(cutoff)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|(d,)| d).collect())
    }

    /// Aggregates `domain`'s events before `cutoff` into report rows
    /// (grouped by source IP + outcome), plus the window start — the
    /// epoch second of the oldest aggregated event. Returns row count
    /// capped at `limit` groups (a report with more distinct sources
    /// than that is truncated, never unbounded).
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn dmarc_report_rows(
        &self,
        domain: &str,
        cutoff: i64,
        limit: i64,
    ) -> Result<(Vec<DmarcAggregateRow>, Option<i64>)> {
        let rows: Vec<(String, i64, String, bool, bool)> = sqlx::query_as(
            "SELECT source_ip, COUNT(*), disposition, dkim_aligned, spf_aligned \
             FROM dmarc_report_events \
             WHERE from_domain = $1 AND evaluated_at < to_timestamp($2) \
             GROUP BY source_ip, disposition, dkim_aligned, spf_aligned \
             ORDER BY COUNT(*) DESC LIMIT $3",
        )
        .bind(domain)
        .bind(cutoff)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        let begin: Option<(Option<i64>,)> = sqlx::query_as(
            "SELECT EXTRACT(EPOCH FROM MIN(evaluated_at))::bigint \
             FROM dmarc_report_events \
             WHERE from_domain = $1 AND evaluated_at < to_timestamp($2)",
        )
        .bind(domain)
        .bind(cutoff)
        .fetch_optional(self.pool())
        .await?;
        Ok((
            rows.into_iter()
                .map(
                    |(source_ip, count, disposition, dkim_aligned, spf_aligned)| {
                        DmarcAggregateRow {
                            source_ip,
                            count,
                            disposition,
                            dkim_aligned,
                            spf_aligned,
                        }
                    },
                )
                .collect(),
            begin.and_then(|(b,)| b),
        ))
    }

    /// Deletes `domain`'s events before `cutoff` — called after the
    /// report covering them was enqueued (or when the domain publishes
    /// no usable `rua=`, so the window will never be reportable).
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn delete_dmarc_events(&self, domain: &str, cutoff: i64) -> Result<u64> {
        let done = sqlx::query(
            "DELETE FROM dmarc_report_events \
             WHERE from_domain = $1 AND evaluated_at < to_timestamp($2)",
        )
        .bind(domain)
        .bind(cutoff)
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected())
    }
}
