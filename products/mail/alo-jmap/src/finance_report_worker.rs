//! Background generation and internal delivery of scheduled Finance CSVs.

use alo_store::{AgedSide, DueFinReportSchedule, Store, TenantId};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use time::{Date, Duration, OffsetDateTime};

const BATCH: i64 = 100;

pub async fn run_due(store: &Store) -> usize {
    let today = OffsetDateTime::now_utc().date();
    let rows = match store.claim_due_fin_report_schedules(today, BATCH).await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(%error,"finance report schedule claim failed");
            return 0;
        }
    };
    let mut delivered = 0;
    for row in rows {
        match deliver(store, &row).await {
            Ok(()) => delivered += 1,
            Err(error) => {
                tracing::warn!(schedule=%row.id,%error,"scheduled finance report delivery failed")
            }
        }
    }
    delivered
}

async fn deliver(store: &Store, row: &DueFinReportSchedule) -> alo_store::Result<()> {
    let tenant = TenantId::new(row.tenant_id.clone());
    let tenant_store = store.for_tenant(tenant.clone());
    let user = tenant_store.user_by_email(&row.recipient).await?;
    let account = store.for_account(tenant, user);
    let to = row.next_run_date - Duration::days(1);
    let days = match row.cadence.as_str() {
        "weekly" => 7,
        "monthly" => 30,
        _ => 90,
    };
    let from = to - Duration::days(days - 1);
    let (name, csv) = match row.report.as_str() {
        "pl" => (
            "profit-and-loss",
            crate::finance_report_pl::report_csv(&account.fin_profit_and_loss(from, to).await?),
        ),
        "balance" => (
            "balance-sheet",
            crate::finance_report_balance::report_csv(&account.fin_balance_sheet(to).await?),
        ),
        "aged_receivable" => (
            "aged-receivables",
            crate::finance_report_aged::report_csv(
                &account.fin_aged(to, AgedSide::Receivable).await?,
            ),
        ),
        "aged_payable" => (
            "aged-payables",
            crate::finance_report_aged::report_csv(&account.fin_aged(to, AgedSide::Payable).await?),
        ),
        _ => (
            "vat-return",
            crate::finance_report_vat::report_csv(&account.fin_vat_return(from, to).await?),
        ),
    };
    account
        .deliver(message(&row.recipient, name, from, to, &csv).as_bytes())
        .await?;
    Ok(())
}

fn message(recipient: &str, name: &str, from: Date, to: Date, csv: &str) -> String {
    let boundary = "alo-finance-report";
    let encoded = STANDARD.encode(csv.as_bytes());
    let wrapped = encoded
        .as_bytes()
        .chunks(76)
        .map(|chunk| String::from_utf8_lossy(chunk))
        .collect::<Vec<_>>()
        .join("\r\n");
    format!(
        "From: alo Finance <no-reply@alo.internal>\r\nTo: {recipient}\r\nSubject: {name} · {from} to {to}\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"{boundary}\"\r\n\r\n--{boundary}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nYour scheduled Finance report is attached.\r\n\r\n--{boundary}\r\nContent-Type: text/csv; charset=utf-8; name=\"{name}-{from}-to-{to}.csv\"\r\nContent-Disposition: attachment; filename=\"{name}-{from}-to-{to}.csv\"\r\nContent-Transfer-Encoding: base64\r\n\r\n{wrapped}\r\n--{boundary}--\r\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    #[test]
    fn scheduled_delivery_is_a_csv_attachment_without_raw_csv_in_headers() {
        let from = Date::from_calendar_date(2026, Month::August, 1).unwrap();
        let to = Date::from_calendar_date(2026, Month::August, 31).unwrap();
        let wire = message(
            "finance@example.test",
            "profit-and-loss",
            from,
            to,
            "a,b\r\n1,2\r\n",
        );
        assert!(wire.contains("Content-Type: text/csv"));
        assert!(wire.contains("Content-Disposition: attachment"));
        assert!(wire.contains("YSxiDQoxLDINCg=="));
        assert!(!wire.contains("a,b\r\n1,2"));
    }
}
