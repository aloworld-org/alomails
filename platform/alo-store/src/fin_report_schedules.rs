//! Tenant-owned scheduled finance report exports.

use sqlx::FromRow;
use time::{Date, OffsetDateTime};

use crate::error::{Result, StoreError};
use crate::id::{FinReportScheduleId, UserId};
use crate::{Store, TenantStore};

#[derive(Debug, Clone, FromRow)]
pub struct FinReportSchedule {
    pub id: String,
    pub report: String,
    pub cadence: String,
    pub format: String,
    pub recipient: String,
    pub active: bool,
    pub next_run_date: Date,
    pub last_run_at: Option<OffsetDateTime>,
    pub created_by: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, FromRow)]
pub struct DueFinReportSchedule {
    pub tenant_id: String,
    pub id: String,
    pub report: String,
    pub cadence: String,
    pub recipient: String,
    pub next_run_date: Date,
}

impl TenantStore {
    pub async fn fin_report_schedules(&self) -> Result<Vec<FinReportSchedule>> {
        sqlx::query_as("SELECT id,report,cadence,format,recipient,active,next_run_date,last_run_at,created_by,created_at,updated_at FROM fin_report_schedules WHERE tenant_id=$1 ORDER BY active DESC,next_run_date,id")
            .bind(self.tenant().as_str()).fetch_all(self.pool()).await.map_err(StoreError::Db)
    }

    pub async fn create_fin_report_schedule(
        &self,
        report: &str,
        cadence: &str,
        recipient: &str,
        next_run_date: Date,
        user: &UserId,
    ) -> Result<FinReportSchedule> {
        let report = choice(
            "report",
            report,
            &["pl", "balance", "aged_receivable", "aged_payable", "vat"],
        )?;
        let cadence = choice("cadence", cadence, &["weekly", "monthly", "quarterly"])?;
        let recipient = recipient.trim();
        if !recipient.contains('@') || recipient.len() > 254 {
            return Err(StoreError::Validation(
                "recipient must be an email address".into(),
            ));
        }
        sqlx::query_as("INSERT INTO fin_report_schedules(tenant_id,id,report,cadence,recipient,next_run_date,anchor_day,created_by) VALUES($1,$2,$3,$4,$5,$6,EXTRACT(day FROM $6::date),$7) RETURNING id,report,cadence,format,recipient,active,next_run_date,last_run_at,created_by,created_at,updated_at")
            .bind(self.tenant().as_str()).bind(FinReportScheduleId::generate().as_str()).bind(report).bind(cadence).bind(recipient).bind(next_run_date).bind(user.as_str()).fetch_one(self.pool()).await.map_err(StoreError::Db)
    }

    pub async fn delete_fin_report_schedule(&self, id: &str) -> Result<bool> {
        Ok(
            sqlx::query("DELETE FROM fin_report_schedules WHERE tenant_id=$1 AND id=$2")
                .bind(self.tenant().as_str())
                .bind(id)
                .execute(self.pool())
                .await
                .map_err(StoreError::Db)?
                .rows_affected()
                == 1,
        )
    }
}

impl Store {
    /// Atomically advances and returns due schedules. Claim-before-delivery is
    /// deliberate at-most-once behaviour: an email is never duplicated after
    /// a process crash.
    pub async fn claim_due_fin_report_schedules(
        &self,
        today: Date,
        limit: i64,
    ) -> Result<Vec<DueFinReportSchedule>> {
        sqlx::query_as("WITH due AS (SELECT tenant_id,id,next_run_date FROM fin_report_schedules WHERE active AND next_run_date <= $1 ORDER BY next_run_date,id FOR UPDATE SKIP LOCKED LIMIT $2), targets AS (SELECT s.tenant_id,s.id,s.cadence,s.anchor_day,s.next_run_date,due.next_run_date AS claimed_date,CASE WHEN s.cadence='quarterly' THEN 3 ELSE 1 END AS months FROM fin_report_schedules s JOIN due USING(tenant_id,id)) UPDATE fin_report_schedules s SET last_run_at=now(),next_run_date=CASE WHEN targets.cadence='weekly' THEN targets.next_run_date+7 ELSE (date_trunc('month',targets.next_run_date + make_interval(months=>targets.months)) + (LEAST(targets.anchor_day::int,EXTRACT(day FROM (date_trunc('month',targets.next_run_date + make_interval(months=>targets.months+1))-interval '1 day'))::int)-1)*interval '1 day')::date END,updated_at=now() FROM targets WHERE s.tenant_id=targets.tenant_id AND s.id=targets.id RETURNING s.tenant_id,s.id,s.report,s.cadence,s.recipient,targets.claimed_date AS next_run_date")
            .bind(today).bind(limit).fetch_all(self.pool()).await.map_err(StoreError::Db)
    }
}

fn choice<'a>(name: &str, raw: &'a str, allowed: &[&str]) -> Result<&'a str> {
    let value = raw.trim();
    if allowed.contains(&value) {
        Ok(value)
    } else {
        Err(StoreError::Validation(format!("{name} is not supported")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn report_and_cadence_are_closed_sets() {
        assert_eq!(choice("report", "pl", &["pl", "vat"]).unwrap(), "pl");
        assert!(choice("report", "payroll", &["pl", "vat"]).is_err());
    }
}
