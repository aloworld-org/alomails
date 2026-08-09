//! **The profit and loss on a seeded year** (alo Finance, ADR 0035, wave
//! B4.11a) — the first of the four reports, asserted against figures computed
//! by hand from a year of postings written through the real journal.
//!
//! `src/fin_pl.rs` proves the fold: given two trial balances, these are the
//! lines and these are the totals. This suite proves the four things a pure
//! test cannot.
//!
//! - **The year adds up to the hand-computed figure**, account by account and
//!   in total, over entries that reached Postgres through
//!   [`alo_store::AccountStore::post_fin_entry`] — invoices, a credit note,
//!   expenses, and payments that must move the balance sheet without touching
//!   the result.
//! - **The period is a real boundary**: an entry dated five days into the next
//!   year is not in this year's report, and one dated in the previous year is
//!   in the comparative rather than nowhere.
//! - **The comparative rolls with the period asked for** — a quarter compares
//!   against the ninety days before it, which is a different window from the
//!   year's.
//! - **Tenancy**: a second tenant's much larger year moves nothing on the
//!   first's report, and each reads only their own accounts.
//!
//! And the tie back to the ledger it summarises: the result equals the trial
//! balance's own income and expense movement, negated — the two are the same
//! fold or one of them is wrong (P10's P&L half, `docs/design/finance.md`).
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    Account, AccountStore, AccountType, CHART, ChartName, ChartSeed, EntryKind, FinAccountId,
    FxSnapshot, NewEntry, NewPosting, PlLine, ProfitAndLoss, Store, StoreError, TenantId,
};
use time::{Date, Month};

fn on(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).unwrap()
}

/// The chart, named per tenant so a leak between two of them shows up as a name
/// from the wrong tenant rather than as a number that happens to match.
fn seed(tag: &str) -> ChartSeed {
    ChartSeed {
        names: CHART
            .iter()
            .map(|account| ChartName {
                code: account.code.to_owned(),
                name: format!("{tag} {}", account.code),
            })
            .collect(),
    }
}

async fn tenant_with_chart(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("pl-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@pl.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    account
        .fin_accounts_or_seed(&seed(tag), false)
        .await
        .unwrap();
    (account, tenant)
}

/// The tenant's chart, by code — the report is read by code, so the test writes
/// its postings by code too.
async fn chart(account: &AccountStore) -> Vec<Account> {
    account.fin_accounts(false).await.unwrap()
}

fn id_of(chart: &[Account], code: &str) -> FinAccountId {
    chart
        .iter()
        .find(|account| account.code == code)
        .unwrap_or_else(|| panic!("the seeded chart holds {code}"))
        .id
        .clone()
}

/// One entry of the seeded year: a date, what it books, and the postings by
/// account code — signed the ledger's way, positive debits.
struct Booking {
    date: Date,
    kind: EntryKind,
    memo: &'static str,
    postings: &'static [(&'static str, i64)],
}

/// **The seeded year.** A small consultancy's 2026, with the end of 2025 before
/// it and the beginning of 2027 after it, so both of the report's boundaries
/// are crossed by a real entry.
///
/// Every entry balances (that is the journal's own rule, and posting one that
/// did not would fail here rather than in a report), and the payments are in
/// deliberately: money arriving moves the bank and the receivable and must
/// leave the result exactly where it found it.
fn year() -> Vec<Booking> {
    vec![
        // ---- 2025, which is the comparative for the whole of 2026 ----------
        Booking {
            date: on(2025, Month::November, 10),
            kind: EntryKind::Invoice,
            memo: "INV-2025-00007",
            postings: &[("1100", 50_000), ("4000", -50_000)],
        },
        Booking {
            date: on(2025, Month::December, 5),
            kind: EntryKind::Bill,
            memo: "hosting, December",
            postings: &[("6000", 20_000), ("2000", -20_000)],
        },
        // ---- 2026, the year under test -------------------------------------
        Booking {
            date: on(2026, Month::January, 15),
            kind: EntryKind::Invoice,
            memo: "INV-2026-00001",
            postings: &[("1100", 121_000), ("4000", -100_000), ("2100", -21_000)],
        },
        Booking {
            date: on(2026, Month::February, 1),
            kind: EntryKind::Bill,
            memo: "hosting, February",
            postings: &[("6000", 15_000), ("2000", -15_000)],
        },
        // Money arriving: the balance sheet moves, the result does not.
        Booking {
            date: on(2026, Month::March, 31),
            kind: EntryKind::Payment,
            memo: "INV-2026-00001 settled",
            postings: &[("1000", 121_000), ("1100", -121_000)],
        },
        Booking {
            date: on(2026, Month::April, 20),
            kind: EntryKind::Invoice,
            memo: "INV-2026-00002",
            postings: &[("1100", 60_500), ("4000", -50_000), ("2100", -10_500)],
        },
        Booking {
            date: on(2026, Month::May, 15),
            kind: EntryKind::Bill,
            memo: "train fares",
            postings: &[("6100", 7_500), ("1000", -7_500)],
        },
        Booking {
            date: on(2026, Month::July, 1),
            kind: EntryKind::Invoice,
            memo: "INV-2026-00003",
            postings: &[("1100", 24_200), ("4900", -20_000), ("2100", -4_200)],
        },
        // A credit note takes revenue back out of the year it was earned in.
        Booking {
            date: on(2026, Month::August, 9),
            kind: EntryKind::CreditNote,
            memo: "CRN-2026-00001",
            postings: &[("4000", 10_000), ("2100", 2_100), ("1100", -12_100)],
        },
        Booking {
            date: on(2026, Month::September, 30),
            kind: EntryKind::Bill,
            memo: "hosting, September",
            postings: &[("6000", 5_000), ("1000", -5_000)],
        },
        // The last day of the year is in the year.
        Booking {
            date: on(2026, Month::December, 31),
            kind: EntryKind::Bill,
            memo: "accountancy fees",
            postings: &[("6200", 2_500), ("1000", -2_500)],
        },
        // ---- 2027, which must not reach 2026's report ----------------------
        Booking {
            date: on(2027, Month::January, 5),
            kind: EntryKind::Invoice,
            memo: "INV-2027-00001",
            postings: &[("1100", 12_100), ("4000", -10_000), ("2100", -2_100)],
        },
    ]
}

/// Posts the seeded year for one tenant, scaled by `times` so a second tenant's
/// books are unmistakably not the first's.
async fn post_year(account: &AccountStore, times: i64) {
    let chart = chart(account).await;
    for booking in year() {
        let postings = booking
            .postings
            .iter()
            .map(|&(code, cents)| {
                let cents = cents * times;
                NewPosting::new(id_of(&chart, code), cents, cents)
            })
            .collect();
        account
            .post_fin_entry(&NewEntry {
                entry_date: booking.date,
                kind: booking.kind,
                source: None,
                memo: booking.memo.to_owned(),
                reverses_entry_id: None,
                attachment_node_id: None,
                currency: "EUR".to_owned(),
                fx: FxSnapshot::identity("EUR", booking.date),
                postings,
            })
            .await
            .unwrap_or_else(|e| panic!("{} should post: {e:?}", booking.memo));
    }
}

fn line<'a>(lines: &'a [PlLine], code: &str) -> &'a PlLine {
    lines
        .iter()
        .find(|line| line.code == code)
        .unwrap_or_else(|| panic!("{code} should be on the report"))
}

fn codes(lines: &[PlLine]) -> Vec<&str> {
    lines.iter().map(|line| line.code.as_str()).collect()
}

async fn year_2026(account: &AccountStore) -> ProfitAndLoss {
    account
        .fin_profit_and_loss(on(2026, Month::January, 1), on(2026, Month::December, 31))
        .await
        .unwrap()
}

#[tokio::test]
async fn a_seeded_year_reports_the_figures_computed_by_hand() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant_with_chart(&store, "solo").await;
    post_year(&account, 1).await;

    let report = year_2026(&account).await;
    assert_eq!(report.from, on(2026, Month::January, 1));
    assert_eq!(report.to, on(2026, Month::December, 31));
    assert_eq!(report.currency, "EUR", "the tenant's accounting currency");

    // Income: 1 000.00 + 500.00 of consulting less the 100.00 credit note, and
    // 200.00 of other income.
    assert_eq!(codes(&report.income), ["4000", "4900"]);
    assert_eq!(line(&report.income, "4000").amount_cents, 140_000);
    assert_eq!(line(&report.income, "4900").amount_cents, 20_000);
    assert_eq!(report.income_cents, 160_000);

    // Expense: 150.00 + 50.00 of hosting, 75.00 of fares, 25.00 of fees.
    assert_eq!(codes(&report.expense), ["6000", "6100", "6200"]);
    assert_eq!(line(&report.expense, "6000").amount_cents, 20_000);
    assert_eq!(line(&report.expense, "6100").amount_cents, 7_500);
    assert_eq!(line(&report.expense, "6200").amount_cents, 2_500);
    assert_eq!(report.expense_cents, 30_000);

    assert_eq!(
        report.result_cents, 130_000,
        "1 600.00 earned, 300.00 spent"
    );

    // The receivable, the bank and the VAT all moved, and none of them is a
    // result: no balance-sheet account is on either side of the report.
    for line in report.income.iter().chain(&report.expense) {
        assert!(!line.kind.is_balance_sheet(), "{}", line.code);
    }
    assert!(
        !codes(&report.income).contains(&"1100") && !codes(&report.expense).contains(&"1000"),
        "a payment moves the balance sheet and leaves the result alone"
    );

    // The account names are this tenant's own, which is what a leak would
    // change first.
    assert_eq!(line(&report.income, "4000").name, "solo 4000");
}

#[tokio::test]
async fn the_year_ends_where_the_period_ends_and_the_year_before_is_the_comparative() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant_with_chart(&store, "edges").await;
    post_year(&account, 1).await;

    let report = year_2026(&account).await;
    // 2025-01-01 … 2025-12-31: the same length, ending the day before.
    assert_eq!(report.previous_from, on(2025, Month::January, 1));
    assert_eq!(report.previous_to, on(2025, Month::December, 31));
    assert_eq!(report.previous_income_cents, 50_000);
    assert_eq!(report.previous_expense_cents, 20_000);
    assert_eq!(report.previous_result_cents, 30_000);
    assert_eq!(line(&report.income, "4000").previous_cents, 50_000);
    assert_eq!(line(&report.expense, "6000").previous_cents, 20_000);
    assert_eq!(
        line(&report.expense, "6100").previous_cents,
        0,
        "the fares are this year's alone"
    );

    // The 2027 invoice is in neither column of the 2026 report: 4000 is
    // 140 000, not 150 000.
    assert_eq!(line(&report.income, "4000").amount_cents, 140_000);
    // …and it is the whole of 2027's, whose comparative is this year.
    let next = account
        .fin_profit_and_loss(on(2027, Month::January, 1), on(2027, Month::December, 31))
        .await
        .unwrap();
    assert_eq!(next.income_cents, 10_000);
    assert_eq!(next.expense_cents, 0);
    assert_eq!(next.previous_income_cents, 160_000);
    assert_eq!(next.previous_result_cents, 130_000);
    assert_eq!(
        line(&next.expense, "6000").amount_cents,
        0,
        "an account only the comparative moved is still a line"
    );
    assert_eq!(line(&next.expense, "6000").previous_cents, 20_000);
    assert_eq!(line(&next.expense, "6000").postings, 0);
}

#[tokio::test]
async fn a_quarter_compares_against_the_ninety_days_before_it_not_the_year() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant_with_chart(&store, "quarter").await;
    post_year(&account, 1).await;

    let q1 = account
        .fin_profit_and_loss(on(2026, Month::January, 1), on(2026, Month::March, 31))
        .await
        .unwrap();
    // Ninety days of 2026, so ninety days of 2025 — from 3 October, not from
    // 1 January.
    assert_eq!(q1.previous_from, on(2025, Month::October, 3));
    assert_eq!(q1.previous_to, on(2025, Month::December, 31));
    assert_eq!(q1.income_cents, 100_000);
    assert_eq!(q1.expense_cents, 15_000);
    assert_eq!(q1.result_cents, 85_000);
    assert_eq!(q1.previous_income_cents, 50_000, "November's invoice");
    assert_eq!(q1.previous_expense_cents, 20_000, "December's hosting");

    // The quarter the credit note falls in: revenue net of it, which is the
    // one arithmetic a P&L must not get backwards.
    let q3 = account
        .fin_profit_and_loss(on(2026, Month::July, 1), on(2026, Month::September, 30))
        .await
        .unwrap();
    assert_eq!(line(&q3.income, "4000").amount_cents, -10_000);
    assert_eq!(line(&q3.income, "4900").amount_cents, 20_000);
    assert_eq!(q3.income_cents, 10_000);
    assert_eq!(q3.expense_cents, 5_000);
    assert_eq!(q3.result_cents, 5_000);
}

#[tokio::test]
async fn the_result_is_the_trial_balance_it_summarises() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant_with_chart(&store, "fold").await;
    post_year(&account, 1).await;

    let from = on(2026, Month::January, 1);
    let to = on(2026, Month::December, 31);
    let report = account.fin_profit_and_loss(from, to).await.unwrap();
    let trial = account
        .fin_trial_balance(Some(from), Some(to))
        .await
        .unwrap();

    assert!(trial.balances(), "every entry balances, so the year does");
    // The ledger keeps income negative; the report flips it once. The two are
    // the same fold or one of them is wrong.
    assert_eq!(report.income_cents, -trial.net_of(AccountType::Income));
    assert_eq!(report.expense_cents, trial.net_of(AccountType::Expense));
    assert_eq!(
        report.result_cents,
        -(trial.net_of(AccountType::Income) + trial.net_of(AccountType::Expense))
    );
}

#[tokio::test]
async fn one_tenants_year_is_no_part_of_anothers_report() {
    let store = common::test_store().await;
    let (ours, _tenant) = tenant_with_chart(&store, "ours").await;
    let (theirs, _other) = tenant_with_chart(&store, "theirs").await;
    post_year(&ours, 1).await;

    let before = year_2026(&ours).await;
    // A hundred times our year, in the same accounts, on the same days.
    post_year(&theirs, 100).await;
    let after = year_2026(&ours).await;
    assert_eq!(
        before, after,
        "their books moved nothing of ours, line for line"
    );

    let theirs_report = year_2026(&theirs).await;
    assert_eq!(theirs_report.result_cents, 13_000_000);
    assert_eq!(line(&theirs_report.income, "4000").name, "theirs 4000");
    // And nothing of ours is in theirs: the names are per tenant, so a leak
    // would read as the wrong tag rather than as a plausible number.
    for line in theirs_report.income.iter().chain(&theirs_report.expense) {
        assert!(line.name.starts_with("theirs "), "{}", line.name);
    }
}

#[tokio::test]
async fn a_period_that_ends_before_it_starts_is_refused() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant_with_chart(&store, "backwards").await;

    match account
        .fin_profit_and_loss(on(2026, Month::December, 31), on(2026, Month::January, 1))
        .await
    {
        Err(StoreError::Validation(message)) => {
            assert!(message.contains("before its start"), "{message}");
        }
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn a_tenant_that_has_never_posted_reads_a_report_of_zeroes() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant_with_chart(&store, "quiet").await;

    let report = year_2026(&account).await;
    assert!(report.income.is_empty() && report.expense.is_empty());
    assert_eq!(report.income_cents, 0);
    assert_eq!(report.expense_cents, 0);
    assert_eq!(report.result_cents, 0);
    assert_eq!(report.previous_result_cents, 0);
    assert_eq!(report.currency, "EUR");
}
