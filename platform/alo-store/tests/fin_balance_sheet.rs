//! **The balance sheet on a seeded year** (alo Finance, ADR 0035, wave B4.11b)
//! — the second of the four reports, asserted against figures computed by hand
//! from the same year of postings the P&L suite reads.
//!
//! `src/fin_balance.rs` proves the fold: given a cumulative trial balance, these
//! are the lines and these are the totals. This suite proves the five things a
//! pure test cannot.
//!
//! - **The date adds up to the hand-computed figure**, account by account and in
//!   total, over entries that reached Postgres through
//!   [`alo_store::AccountStore::post_fin_entry`] — invoices, a credit note,
//!   bills and payments.
//! - **It balances** (P10): `assets = liabilities + equity + result`, on every
//!   date asked for, including one in the middle of a month.
//! - **A balance sheet is cumulative, and the date is a real boundary**: the
//!   entry dated five days into 2027 is on the 2027 sheet and on no earlier one,
//!   and the sheet at the end of 2025 holds only what had happened by then.
//! - **It ties to the P&L it shares a journal with**: the result on the sheet at
//!   a year end is every P&L up to that year end added together — the two are
//!   the same fold or one of them is wrong.
//! - **Tenancy**: a second tenant's much larger books move nothing on the
//!   first's sheet, and each reads only their own accounts.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    Account, AccountStore, AccountType, BalanceLine, BalanceSheet, CHART, ChartName, ChartSeed,
    EntryKind, FinAccountId, FxSnapshot, NewEntry, NewPosting, Store, TenantId,
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
    let tenant = store.create_tenant(&format!("bs-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@bs.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    account
        .fin_accounts_or_seed(&seed(tag), false)
        .await
        .unwrap();
    (account, tenant)
}

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

/// **The seeded year** — deliberately the same books as `fin_pl_report.rs`, so
/// the two reports are asserted over one set of facts and the tie between them
/// (`the_result_on_the_sheet_is_every_profit_and_loss_before_it`) means
/// something.
fn year() -> Vec<Booking> {
    vec![
        // ---- 2025 -----------------------------------------------------------
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
        // ---- 2026 -----------------------------------------------------------
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
        Booking {
            date: on(2026, Month::December, 31),
            kind: EntryKind::Bill,
            memo: "accountancy fees",
            postings: &[("6200", 2_500), ("1000", -2_500)],
        },
        // ---- 2027, which must not reach a 2026 sheet -------------------------
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

fn line<'a>(lines: &'a [BalanceLine], code: &str) -> &'a BalanceLine {
    lines
        .iter()
        .find(|line| line.code == code)
        .unwrap_or_else(|| panic!("{code} should be on the sheet"))
}

fn codes(lines: &[BalanceLine]) -> Vec<&str> {
    lines.iter().map(|line| line.code.as_str()).collect()
}

async fn sheet_at(account: &AccountStore, date: Date) -> BalanceSheet {
    account.fin_balance_sheet(date).await.unwrap()
}

/// The one assertion every sheet in this suite makes, wherever it stands.
fn assert_balances(sheet: &BalanceSheet) {
    assert_eq!(
        sheet.asset_cents, sheet.liability_equity_cents,
        "assets must equal liabilities + equity + result on {}",
        sheet.on
    );
    assert_eq!(sheet.difference_cents, 0);
    assert!(sheet.balances());
}

#[tokio::test]
async fn a_seeded_year_end_reports_the_figures_computed_by_hand() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant_with_chart(&store, "solo").await;
    post_year(&account, 1).await;

    let sheet = sheet_at(&account, on(2026, Month::December, 31)).await;
    assert_eq!(sheet.on, on(2026, Month::December, 31));
    assert_eq!(sheet.currency, "EUR", "the tenant's accounting currency");

    // Owned: the bank (1 210.00 in, 75.00 + 50.00 + 25.00 out) and what is
    // still owed to us (1 226.00 of invoices net of the credit note).
    assert_eq!(codes(&sheet.assets), ["1000", "1100"]);
    assert_eq!(line(&sheet.assets, "1000").amount_cents, 106_000);
    assert_eq!(line(&sheet.assets, "1100").amount_cents, 122_600);
    assert_eq!(sheet.asset_cents, 228_600);

    // Owed: suppliers (350.00) and the VAT collected net of the credit note
    // (336.00) — both positive, on the side they belong to.
    assert_eq!(codes(&sheet.liabilities), ["2000", "2100"]);
    assert_eq!(line(&sheet.liabilities, "2000").amount_cents, 35_000);
    assert_eq!(line(&sheet.liabilities, "2100").amount_cents, 33_600);
    assert_eq!(sheet.liability_cents, 68_600);

    // Nobody has posted equity, and nothing invents any.
    assert!(sheet.equity.is_empty());
    assert_eq!(sheet.equity_cents, 0);

    // 2 100.00 earned since the books opened, 500.00 spent.
    assert_eq!(sheet.result_cents, 160_000);
    assert_balances(&sheet);

    // No income or expense account is a line, and no balance-sheet account is
    // missing from one.
    for line in sheet
        .assets
        .iter()
        .chain(&sheet.liabilities)
        .chain(&sheet.equity)
    {
        assert!(line.kind.is_balance_sheet(), "{}", line.code);
        assert!(line.postings > 0, "{} is on the sheet unmoved", line.code);
    }

    // The account names are this tenant's own, which is what a leak would
    // change first.
    assert_eq!(line(&sheet.assets, "1000").name, "solo 1000");
    // …and the role travels with the line, so a screen need not read codes.
    assert_eq!(
        line(&sheet.assets, "1000").role.map(|role| role.as_str()),
        Some("bank")
    );
    assert_eq!(
        line(&sheet.liabilities, "2100")
            .role
            .map(|role| role.as_str()),
        Some("vat_output")
    );
}

#[tokio::test]
async fn a_sheet_is_cumulative_and_the_date_is_a_real_boundary() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant_with_chart(&store, "edges").await;
    post_year(&account, 1).await;

    // The end of 2025: only the November invoice and the December bill exist.
    let then = sheet_at(&account, on(2025, Month::December, 31)).await;
    assert_eq!(codes(&then.assets), ["1100"]);
    assert_eq!(then.asset_cents, 50_000);
    assert_eq!(then.liability_cents, 20_000);
    assert_eq!(then.result_cents, 30_000, "500.00 earned, 200.00 spent");
    assert_balances(&then);

    // Nine days before the first entry there is nothing at all — and a sheet of
    // zeroes rather than an absence.
    let before = sheet_at(&account, on(2025, Month::November, 1)).await;
    assert!(before.assets.is_empty() && before.liabilities.is_empty());
    assert_eq!(before.asset_cents, 0);
    assert_eq!(before.result_cents, 0);
    assert_eq!(before.currency, "EUR");
    assert_balances(&before);

    // The 2027 invoice is on no 2026 sheet…
    let year_end = sheet_at(&account, on(2026, Month::December, 31)).await;
    assert_eq!(line(&year_end.assets, "1100").amount_cents, 122_600);
    // …and is on the next one, receivable, VAT and revenue together.
    let next = sheet_at(&account, on(2027, Month::December, 31)).await;
    assert_eq!(line(&next.assets, "1100").amount_cents, 134_700);
    assert_eq!(line(&next.liabilities, "2100").amount_cents, 35_700);
    assert_eq!(next.result_cents, 170_000);
    assert_balances(&next);

    // A date in the middle of a month is a date like any other: the payment of
    // 31 March has happened, the fares of 15 May have not.
    let mid = sheet_at(&account, on(2026, Month::April, 30)).await;
    assert_eq!(line(&mid.assets, "1000").amount_cents, 121_000);
    assert_eq!(line(&mid.assets, "1100").amount_cents, 110_500);
    assert_balances(&mid);
}

#[tokio::test]
async fn the_result_on_the_sheet_is_every_profit_and_loss_before_it() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant_with_chart(&store, "fold").await;
    post_year(&account, 1).await;

    let year_end = on(2026, Month::December, 31);
    let sheet = sheet_at(&account, year_end).await;

    // 2025 and 2026, added up: the sheet carries a result no closing entry has
    // moved into equity, so it is every period's result there has ever been.
    let first = account
        .fin_profit_and_loss(on(2025, Month::January, 1), on(2025, Month::December, 31))
        .await
        .unwrap();
    let second = account
        .fin_profit_and_loss(on(2026, Month::January, 1), year_end)
        .await
        .unwrap();
    assert_eq!(first.result_cents, 30_000);
    assert_eq!(second.result_cents, 130_000);
    assert_eq!(sheet.result_cents, first.result_cents + second.result_cents);

    // And the sides are the trial balance they summarise, sign for sign.
    let trial = account
        .fin_trial_balance(None, Some(year_end))
        .await
        .unwrap();
    assert!(trial.balances(), "every entry balances, so the books do");
    assert_eq!(sheet.asset_cents, trial.net_of(AccountType::Asset));
    assert_eq!(sheet.liability_cents, -trial.net_of(AccountType::Liability));
    assert_eq!(sheet.equity_cents, -trial.net_of(AccountType::Equity));
    assert_eq!(
        sheet.result_cents,
        -(trial.net_of(AccountType::Income) + trial.net_of(AccountType::Expense))
    );
}

#[tokio::test]
async fn one_tenants_books_are_no_part_of_anothers_sheet() {
    let store = common::test_store().await;
    let (ours, _tenant) = tenant_with_chart(&store, "ours").await;
    let (theirs, _other) = tenant_with_chart(&store, "theirs").await;
    post_year(&ours, 1).await;

    let year_end = on(2026, Month::December, 31);
    let before = sheet_at(&ours, year_end).await;
    // A hundred times our books, in the same accounts, on the same days.
    post_year(&theirs, 100).await;
    let after = sheet_at(&ours, year_end).await;
    assert_eq!(
        before, after,
        "their books moved nothing of ours, line for line"
    );

    let theirs_sheet = sheet_at(&theirs, year_end).await;
    assert_eq!(theirs_sheet.asset_cents, 22_860_000);
    assert_eq!(theirs_sheet.result_cents, 16_000_000);
    assert_balances(&theirs_sheet);
    // Nothing of ours is in theirs: the names are per tenant, so a leak would
    // read as the wrong tag rather than as a plausible number.
    for line in theirs_sheet
        .assets
        .iter()
        .chain(&theirs_sheet.liabilities)
        .chain(&theirs_sheet.equity)
    {
        assert!(line.name.starts_with("theirs "), "{}", line.name);
    }
}
