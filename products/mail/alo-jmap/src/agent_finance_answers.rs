//! The **answering** half of the Finance agent (ADR 0034, ADR 0035 wave B4.14b)
//! — `vat_summary` and `flag_anomalies`, executed against the caller's own
//! tenant-scoped store.
//!
//! Apart from [`crate::agent_finance`], which drafts, because these two have a
//! different reason to change: a tool that *writes a suggestion onto somebody's
//! claim* answers to ADR 0023's approval rules, and a tool that *reads the
//! tenant's books* answers to B4.12's access rules. They meet only at the shared
//! period reader.
//!
//! Four rules shape this file.
//!
//! - **The same gate as the reports themselves.** Both tools read the whole
//!   tenant's position, not the caller's own work, so both call
//!   [`crate::state::Account::require_finance`] — an admin or the accountant, and
//!   nobody else. This is the wall B4.12 built, and an agent is exactly the way
//!   round it somebody would try: the proposal is composed by a model, but the
//!   execution is a request from a browser holding a token, and it is gated like
//!   every other.
//! - **The same store function as the screen.** `vat_summary` reads
//!   [`alo_store::AccountStore::fin_vat_return`] and renders it through
//!   [`crate::finance_report_vat`]'s own shape, so the agent and
//!   `GET /finance/reports/vat` cannot disagree about a cent. There is no second
//!   path to a figure in this product.
//! - **Figures and codes, never a sentence.** Nothing here composes prose: a
//!   sentence written in the server is a user-facing string in one language,
//!   which is a bug in a European product (CLAUDE.md). The client writes the
//!   words around these numbers, in the reader's own catalogue.
//! - **A finding names entries, never people.** The store's rules never read a
//!   posting's user ([`alo_store::fin_anomalies`]) and nothing is added back
//!   here: what travels is an account, a counterparty, an amount and the entries
//!   behind it.

use std::collections::HashMap;

use axum::Json;
use serde_json::{Value, json};
use time::{Date, OffsetDateTime};

use alo_store::{
    Anomaly, AnomalyScan, AnomalySource, BillingCustomerId, Counterparty, PARTY_CUSTOMER,
};

use crate::agent_args::{string_arg, unprocessable};
use crate::agent_finance::period;
use crate::billing::{iso_date, map_store_err, parse_iso_date};
use crate::error::Problem;
use crate::finance_report_vat::report_json;
use crate::state::Account;

/// How far back a scan of the books looks when the proposal states no period.
///
/// A year, where `categorise_transactions` takes a quarter: the rules that make
/// this tool worth running — a monthly cost that stopped, an amount unlike the
/// rest of its account's — need enough months to have a rhythm to be outside of.
/// Twelve of them, so a period stated in months is never longer than the ceiling
/// the whole finance agent shares.
const ANOMALY_PERIOD_DAYS: i64 = 365;

/// `vat_summary` — the VAT figures the tenant's books carry for a stated period.
///
/// Both days are **required**, exactly as `GET /finance/reports/vat` requires
/// them ([`crate::finance_reports`]): a report that quietly defaulted to "this
/// quarter" would put a figure under a heading nobody asked for, and this is the
/// figure most likely to be copied into a filing. The model is told to resolve
/// the user's "last quarter" into two plain days before proposing anything.
///
/// It writes nothing, and it files nothing: these are **figures for a return,
/// not a return** (ADR 0035).
///
/// # Errors
/// `403` for a caller who is neither an admin nor an accountant; `422` when
/// either day is missing or is not a plain `YYYY-MM-DD`; the store's own `422`
/// when the period ends before it starts or states more rates than one read
/// carries; `500` on a store failure.
pub async fn execute_vat_summary(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    account.require_finance()?;
    let from = stated_day(args, "from")?;
    let to = stated_day(args, "to")?;
    let report = account
        .acc
        .fin_vat_return(from, to)
        .await
        .map_err(map_store_err)?;
    // The report's own shape, from the file that owns it — the agent is another
    // reader of the same figures, never a second rendering of them.
    let mut result = report_json(&report);
    if let Some(object) = result.as_object_mut() {
        object.insert("kind".to_owned(), json!("vatSummary"));
    }
    Ok(Json(json!({ "ok": true, "result": result })))
}

/// `flag_anomalies` — what is worth a second look in a period of the journal.
///
/// The scan itself is the store's ([`alo_store::find_anomalies`]), a pure
/// function over rows with no score and no ranking in it. What this executor
/// adds is the vocabulary a person reads the findings in: the account's code and
/// name, and the counterparty's name where the store could only carry an id.
///
/// It writes nothing at all — there is no anomaly table, no "reviewed" flag and
/// no dismissal. The answer to a finding is a correcting entry in the journal.
///
/// # Errors
/// `403` for a caller who is neither an admin nor an accountant; `422` when a
/// bound is not a plain `YYYY-MM-DD`, when the period runs backwards or is
/// longer than [`crate::agent_finance::MAX_PERIOD_DAYS`]; `500` on a store
/// failure.
pub async fn execute_flag_anomalies(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    account.require_finance()?;
    let (from, to) = period(args, OffsetDateTime::now_utc().date(), ANOMALY_PERIOD_DAYS)?;
    let scan = account
        .acc
        .fin_anomalies(from, to)
        .await
        .map_err(map_store_err)?;

    // Every figure in a finding is in the tenant's accounting currency (the
    // store compares base amounts so a dollar invoice and a euro one are the
    // same size of thing), and a column of money that does not say what it is in
    // is a question rather than an answer.
    let currency = account
        .acc
        .billing_base_currency()
        .await
        .map_err(map_store_err)?;
    let accounts = account
        .acc
        .fin_accounts(true)
        .await
        .map_err(map_store_err)?;
    let chart: HashMap<&str, (&str, &str)> = accounts
        .iter()
        .map(|one| (one.id.as_str(), (one.code.as_str(), one.name.as_str())))
        .collect();
    let customers = customer_names(account, &scan).await?;

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "journalAnomalies",
            "from": iso_date(from),
            "to": iso_date(to),
            "currency": currency,
            "findings": scan
                .findings
                .iter()
                .map(|found| finding_json(found, &chart, &customers))
                .collect::<Vec<_>>(),
            // How many were found, how many are in the list, how many entries
            // were read and whether the period held more than one scan carries:
            // a scan that stopped looking says so, because silence reads as
            // "nothing else happened".
            "found": scan.found,
            "shown": scan.findings.len(),
            "scanned": scan.scanned,
            "truncated": scan.truncated,
            // Entries that name no counterparty, which the duplicate rule
            // therefore could not compare.
            "notComparable": scan.not_comparable,
        }
    })))
}

/// One finding, in the words a person reads it in.
fn finding_json(
    found: &Anomaly,
    chart: &HashMap<&str, (&str, &str)>,
    customers: &HashMap<String, String>,
) -> Value {
    let named = chart.get(found.account_id.as_str());
    json!({
        "kind": found.kind,
        "accountId": found.account_id.as_str(),
        "accountCode": named.map(|(code, _)| *code),
        "accountName": named.map(|(_, name)| *name),
        "counterparty": found
            .counterparty
            .as_ref()
            .map(|party| counterparty_json(party, customers)),
        "amountCents": found.amount_cents,
        "typicalCents": found.typical_cents,
        "missingMonth": found.missing_month.map(iso_date),
        // The evidence. An unexplained flag is an accusation, so this list is
        // never empty for a finding that has entries behind it.
        "entries": found.sources.iter().map(source_json).collect::<Vec<_>>(),
    })
}

/// The other side of the transaction: the key the journal holds, and the name
/// the tenant knows it by when we have one.
///
/// A supplier key is already name-shaped, so it is its own name; a customer id
/// is not, and an id printed at somebody is not an answer.
fn counterparty_json(party: &Counterparty, customers: &HashMap<String, String>) -> Value {
    let name = if party.kind == PARTY_CUSTOMER {
        customers.get(&party.key).cloned()
    } else {
        Some(party.key.clone())
    };
    json!({ "kind": party.kind, "id": party.key, "name": name })
}

/// One entry a finding points at.
fn source_json(source: &AnomalySource) -> Value {
    json!({
        "id": source.entry_id.as_str(),
        "entryDate": iso_date(source.entry_date),
        "entryKind": source.kind.as_str(),
        "memo": source.memo,
        "amountCents": source.amount_cents,
    })
}

/// The names of the customers the findings name, in one read.
///
/// One query for the whole scan rather than one per finding, and an id that is
/// not this tenant's is simply absent from the answer — the same behaviour every
/// other read on that door has.
async fn customer_names(
    account: &Account,
    scan: &AnomalyScan,
) -> Result<HashMap<String, String>, Problem> {
    let ids: Vec<BillingCustomerId> = scan
        .findings
        .iter()
        .filter_map(|found| found.counterparty.as_ref())
        .filter(|party| party.kind == PARTY_CUSTOMER)
        .map(|party| BillingCustomerId::new(&party.key))
        .collect();
    account
        .acc
        .billing_customer_names(&ids)
        .await
        .map_err(map_store_err)
}

/// A day the proposal must state, with no default to fall back on.
///
/// The refusal names which end is wrong, so a caller with two malformed dates
/// learns which one it is being told about — the same courtesy
/// [`crate::finance_reports::day`] extends to the report routes, in the same
/// words a person would read there.
fn stated_day(args: &Value, name: &str) -> Result<Date, Problem> {
    let stated = string_arg(args, name).ok_or_else(|| {
        unprocessable(format!(
            "{name} is required: a VAT summary is always for a stated period"
        ))
    })?;
    parse_iso_date(&stated)
        .ok_or_else(|| unprocessable(format!("{name} must be a date written YYYY-MM-DD")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use alo_store::{ANOMALY_DUPLICATE, ANOMALY_MISSING_RECURRING, EntryKind, PARTY_SUPPLIER};
    use alo_store::{FinAccountId, FinEntryId};

    fn day(iso: &str) -> Date {
        parse_iso_date(iso).expect("a plain day")
    }

    fn source(id: &str, on: &str, cents: i64) -> AnomalySource {
        AnomalySource {
            entry_id: FinEntryId::new(id),
            entry_date: day(on),
            kind: EntryKind::Invoice,
            memo: format!("memo {id}"),
            amount_cents: cents,
        }
    }

    fn chart() -> HashMap<&'static str, (&'static str, &'static str)> {
        HashMap::from([("acc-1", ("4000", "Sales"))])
    }

    #[test]
    fn both_ends_of_a_vat_period_are_required_and_the_refusal_says_which() {
        for name in ["from", "to"] {
            let problem = stated_day(&json!({}), name).expect_err("accepted a missing day");
            assert_eq!(problem.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
            let detail = problem.detail.unwrap_or_default();
            assert!(detail.starts_with(name), "{detail}");
            assert!(detail.contains("required"), "{detail}");
        }
        assert_eq!(
            stated_day(&json!({ "from": " 2026-07-01 " }), "from").unwrap(),
            day("2026-07-01")
        );
    }

    #[test]
    fn a_vat_day_that_is_not_a_plain_day_is_refused_rather_than_guessed() {
        for bad in [
            "last quarter",
            "01/07/2026",
            "2026-07-01T00:00:00Z",
            "2026-13-01",
        ] {
            let problem =
                stated_day(&json!({ "from": bad }), "from").expect_err("accepted a bad day");
            assert_eq!(problem.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(
                problem.detail.as_deref(),
                Some("from must be a date written YYYY-MM-DD")
            );
        }
    }

    #[test]
    fn a_finding_carries_the_account_in_words_and_the_entries_behind_it() {
        let found = Anomaly {
            kind: ANOMALY_DUPLICATE,
            account_id: FinAccountId::new("acc-1"),
            counterparty: Some(Counterparty {
                kind: PARTY_CUSTOMER,
                key: "cust-1".to_owned(),
            }),
            amount_cents: 120_000,
            typical_cents: None,
            missing_month: None,
            sources: vec![
                source("e1", "2026-03-02", 120_000),
                source("e2", "2026-03-05", 120_000),
            ],
        };
        let names = HashMap::from([("cust-1".to_owned(), "Hansen BV".to_owned())]);
        let value = finding_json(&found, &chart(), &names);
        assert_eq!(value["kind"], ANOMALY_DUPLICATE);
        assert_eq!(value["accountCode"], "4000");
        assert_eq!(value["accountName"], "Sales");
        assert_eq!(value["counterparty"]["name"], "Hansen BV");
        assert_eq!(value["amountCents"], 120_000);
        assert_eq!(value["typicalCents"], Value::Null);
        assert_eq!(value["entries"].as_array().map(Vec::len), Some(2));
        assert_eq!(value["entries"][0]["id"], "e1");
        assert_eq!(value["entries"][0]["entryDate"], "2026-03-02");
        assert_eq!(value["entries"][0]["entryKind"], "invoice");
    }

    #[test]
    fn a_customer_we_cannot_name_travels_as_an_id_and_never_as_a_wrong_name() {
        let found = Anomaly {
            kind: ANOMALY_DUPLICATE,
            account_id: FinAccountId::new("acc-9"),
            counterparty: Some(Counterparty {
                kind: PARTY_CUSTOMER,
                key: "cust-gone".to_owned(),
            }),
            amount_cents: 1,
            typical_cents: None,
            missing_month: None,
            sources: vec![source("e1", "2026-03-02", 1)],
        };
        let value = finding_json(&found, &chart(), &HashMap::new());
        assert_eq!(value["counterparty"]["id"], "cust-gone");
        assert_eq!(value["counterparty"]["name"], Value::Null);
        // An account outside the chart we read is null too, never an id shown as
        // if it were a name.
        assert_eq!(value["accountCode"], Value::Null);
        assert_eq!(value["accountId"], "acc-9");
    }

    #[test]
    fn a_supplier_key_is_its_own_name_and_a_missing_month_is_a_day() {
        let found = Anomaly {
            kind: ANOMALY_MISSING_RECURRING,
            account_id: FinAccountId::new("acc-1"),
            counterparty: Some(Counterparty {
                kind: PARTY_SUPPLIER,
                key: "Vermeer Vastgoed".to_owned(),
            }),
            amount_cents: 120_000,
            typical_cents: Some(120_000),
            missing_month: Some(day("2026-03-01")),
            sources: vec![source("feb", "2026-02-05", 120_000)],
        };
        let value = finding_json(&found, &chart(), &HashMap::new());
        assert_eq!(value["counterparty"]["name"], "Vermeer Vastgoed");
        assert_eq!(value["missingMonth"], "2026-03-01");
        assert_eq!(value["typicalCents"], 120_000);
    }

    #[test]
    fn nothing_a_finding_renders_can_carry_a_user() {
        // The store never reads a posting's user, and this layer adds nothing
        // back: the rendered keys are the whole of what a client can show.
        let found = Anomaly {
            kind: ANOMALY_DUPLICATE,
            account_id: FinAccountId::new("acc-1"),
            counterparty: None,
            amount_cents: 5,
            typical_cents: None,
            missing_month: None,
            sources: vec![source("e1", "2026-03-02", 5)],
        };
        let value = finding_json(&found, &chart(), &HashMap::new());
        let keys: Vec<&String> = value
            .as_object()
            .map(|o| o.keys().collect())
            .unwrap_or_default();
        assert!(!keys.iter().any(|key| key.contains("user")), "{keys:?}");
        let entry_keys: Vec<&String> = value["entries"][0]
            .as_object()
            .map(|o| o.keys().collect())
            .unwrap_or_default();
        assert!(
            !entry_keys.iter().any(|key| key.contains("user")),
            "{entry_keys:?}"
        );
        assert_eq!(value["counterparty"], Value::Null);
    }

    #[test]
    fn a_scan_of_the_books_looks_back_a_year_when_no_period_is_stated() {
        let today = day("2026-08-10");
        let (from, to) = period(&json!({}), today, ANOMALY_PERIOD_DAYS).unwrap();
        assert_eq!(to, today);
        assert_eq!(from, day("2025-08-11"));
        // …and stays inside the one ceiling the whole finance agent shares.
        assert!((to - from).whole_days() < crate::agent_finance::MAX_PERIOD_DAYS);
    }
}
