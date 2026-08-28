//! **The journal's property suite** (alo Finance, ADR 0035, wave B4.03b) — the
//! invariants of `docs/design/finance.md` § "The invariant, and how it is
//! proven", asserted over a randomly generated business month rather than over
//! a hand-picked example.
//!
//! A hand-written fixture proves the code works on the case its author thought
//! of. These properties are the ones that would catch the case nobody thought
//! of, and every one of them is asserted **against the database**, not against
//! the values the test just held in memory: an in-memory check that lies about
//! what was written is exactly the failure this suite exists to find.
//!
//! **Everything is generated through the real store functions.** The month is
//! built by calling [`alo_store::AccountStore::post_fin_entry`] the way a
//! posting rule will, never by inserting rows — a property that holds only for
//! rows a test wrote by hand proves nothing about the code that will run.
//!
//! **The generator is seeded**, with a tiny xorshift64\* the same shape
//! `billing_totals`' property tests already use, so a failure names the seed
//! that produced it and is replayable to the posting.
//!
//! Which properties are here, and which are not yet:
//!
//! | Property | Where |
//! |---|---|
//! | **P1** every entry balances, in both columns, re-derived from the database | `every_generated_month_balances_in_both_columns` |
//! | **P2** no posting moves nothing in both currencies | same |
//! | **P4** a document and its credit note sum to zero per account and dimension | `a_document_and_its_credit_note_leave_the_ledger_where_they_found_it` |
//! | **P7** a document event posts exactly once | `posting_the_same_document_event_twice_changes_nothing` |
//! | **P8** no posted row is ever changed or removed | `every_posting_written_is_still_there_byte_for_byte` |
//! | **P9** another tenant's month moves none of ours | `one_tenants_month_leaves_another_tenants_books_untouched` |
//! | P3, P5, P6, P10 | need the posting **rules** (B4.04) and the **reports** (B4.11); they arrive with them |
//!
//! P3/P5/P6/P10 are deliberately absent rather than weakly approximated: each
//! asserts that a *rule* books what B1 computed, and there is no rule yet to
//! assert it about. What stands in for them today is stronger than a stub —
//! [`alo_store::AccountStore::fin_trial_balance`] is checked against an
//! independent tally the generator keeps as it posts, so the aggregate the
//! reports will read is already proven to be the documents added up.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use std::collections::BTreeMap;

use alo_store::{
    Account, AccountRole, AccountStore, CHART, ChartName, ChartSeed, EntryKind, EntrySource,
    FinAccountId, FinEntryId, FxSnapshot, LedgerDimension, LedgerScope, NewEntry, NewPosting,
    Posting, SourceEvent, SourceKind, Store, StoreError, TenantId, billing_fx::convert_cents,
};
use time::{Date, Month};

/// A tiny deterministic generator — xorshift64\*, seeded per test, so the month
/// below is a different month every run only in the sense that *the seed*
/// chooses it: the same seed replays the same postings, which is what makes a
/// property failure a bug report rather than a rumour.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A value in `0..=max`.
    fn upto(&mut self, max: u64) -> u64 {
        self.next() % (max + 1)
    }

    /// A value in `low..=high`, as the cents an amount is written in.
    fn cents(&mut self, low: i64, high: i64) -> i64 {
        low + i64::try_from(self.upto(u64::try_from(high - low).unwrap_or(0))).unwrap_or(0)
    }

    /// One of a slice, uniformly.
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        let index = usize::try_from(self.upto(u64::try_from(items.len() - 1).unwrap_or(0)))
            .unwrap_or_default();
        &items[index]
    }

    /// A one-in-`n` chance.
    fn chance(&mut self, one_in: u64) -> bool {
        self.upto(one_in - 1) == 0
    }
}

/// The VAT rates a European small business actually charges, per the note's
/// generator plan.
const RATES: &[i32] = &[0, 500, 700, 900, 1900, 2100, 2500];

/// The currencies the month is raised in, with the rate a document raised in
/// one carries. `EUR` is the accounting currency, so it converts at the
/// identity and the other two do not.
const CURRENCIES: &[(&str, i64)] = &[("EUR", 1_000_000), ("USD", 1_087_500), ("GBP", 843_200)];

fn day(day: u8) -> Date {
    Date::from_calendar_date(2026, Month::April, day.clamp(1, 30)).expect("a real April day")
}

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
    let tenant = store.create_tenant(&format!("props-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@finance.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    account
        .fin_accounts_or_seed(&seed(tag), false)
        .await
        .unwrap();
    (account, tenant)
}

async fn role(account: &AccountStore, role: AccountRole) -> Account {
    account
        .fin_account_for_role(role)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("the seeded chart has a {} account", role.as_str()))
}

/// The accounts a posting rule resolves, resolved once so the generator reads
/// like the rules of `docs/design/finance.md` § "Posting rules".
struct Chart {
    ar: FinAccountId,
    ap: FinAccountId,
    bank: FinAccountId,
    revenue: FinAccountId,
    vat_output: FinAccountId,
    vat_input: FinAccountId,
    expense: FinAccountId,
    employee_payable: FinAccountId,
    fx_diff: FinAccountId,
    rounding: FinAccountId,
}

impl Chart {
    async fn of(account: &AccountStore) -> Self {
        Self {
            ar: role(account, AccountRole::Ar).await.id,
            ap: role(account, AccountRole::Ap).await.id,
            bank: role(account, AccountRole::Bank).await.id,
            revenue: role(account, AccountRole::Revenue).await.id,
            vat_output: role(account, AccountRole::VatOutput).await.id,
            vat_input: role(account, AccountRole::VatInput).await.id,
            expense: role(account, AccountRole::ExpenseDefault).await.id,
            employee_payable: role(account, AccountRole::EmployeePayable).await.id,
            fx_diff: role(account, AccountRole::FxDiff).await.id,
            rounding: role(account, AccountRole::Rounding).await.id,
        }
    }
}

/// What the generator posted, tallied independently as it went — the model the
/// database is then checked against.
#[derive(Default)]
struct Tally {
    /// Every entry it posted, in order.
    entries: Vec<FinEntryId>,
    /// Account id → balance in the accounting currency, as the generator
    /// believes it.
    by_account: BTreeMap<String, i64>,
    /// Customer id → the receivable balance it believes.
    receivable_by_customer: BTreeMap<String, i64>,
    /// VAT rate → the output-tax balance it believes.
    output_tax_by_rate: BTreeMap<i32, i64>,
    /// How many postings it wrote.
    postings: usize,
    /// The document events it posted, so P7 can re-post one.
    sources: Vec<EntrySource>,
    /// The invoices it issued, as (source id, its postings), so P4 can credit
    /// one and check the pair nets out.
    invoices: Vec<(String, Vec<NewPosting>, Date, String, FxSnapshot)>,
    /// How many entries were raised in a currency other than the accounting
    /// one, and how many of the two zero-in-one-column posting shapes the
    /// month produced.
    ///
    /// Asserted rather than merely counted: a generator that quietly stops
    /// producing the interesting case still passes every property, and a suite
    /// that has stopped testing the FX exception cannot tell you so.
    foreign: usize,
    /// Rounding residuals, where crossing each posting left the base column a
    /// cent off.
    residuals: usize,
    /// Exchange differences on a settlement — the postings that move nothing in
    /// the document column and real money in the base one (P2's exception).
    differences: usize,
}

impl Tally {
    /// Records one posted entry in the model, exactly as the database will see
    /// it. Deliberately written from `NewPosting` rather than read back — the
    /// point of the model is that it is derived some other way than the query
    /// under test.
    fn record(&mut self, id: FinEntryId, postings: &[NewPosting], vat_output: &FinAccountId) {
        for posting in postings {
            *self
                .by_account
                .entry(posting.account_id.as_str().to_owned())
                .or_default() += posting.base_cents;
            if let Some(customer) = &posting.customer_id {
                *self
                    .receivable_by_customer
                    .entry(customer.clone())
                    .or_default() += posting.base_cents;
            }
            if posting.account_id.as_str() == vat_output.as_str()
                && let Some(rate) = posting.vat_rate_bp
            {
                *self.output_tax_by_rate.entry(rate).or_default() += posting.base_cents;
            }
            self.postings += 1;
        }
        self.entries.push(id);
    }
}

/// Restates a document-currency posting set into the accounting currency and
/// adds the residual the rounding account exists for, returning postings that
/// balance in **both** columns.
///
/// This is `docs/design/finance.md`'s rule made executable: cross each posting,
/// never the total, and give the cent that rounding leaves over a home of its
/// own rather than absorbing it into whichever posting happens to be last.
fn restate(
    mut postings: Vec<NewPosting>,
    rate_micro: i64,
    rounding: &FinAccountId,
    residuals: &mut usize,
) -> Vec<NewPosting> {
    let mut residual = 0i64;
    for posting in &mut postings {
        posting.base_cents =
            convert_cents(posting.amount_cents, rate_micro).expect("a positive rate converts");
        residual += posting.base_cents;
    }
    if residual != 0 {
        *residuals += 1;
        // Zero in the document column, non-zero in the base column: the one
        // posting shape the journal allows exactly here.
        postings.push(NewPosting {
            memo: "rounding".to_owned(),
            ..NewPosting::new(rounding.clone(), 0, -residual)
        });
    }
    postings
}

/// Generates and posts a random business month, returning the model to check
/// the database against.
///
/// The shapes are the ones B4.04's rules will produce — an issued invoice, a
/// customer payment, an approved supplier bill, an approved expense claim —
/// because a property suite that exercises shapes the rules will never write
/// proves the wrong thing.
///
/// `batch` prefixes the document ids, so a second month posted into the *same*
/// tenant is a second set of documents rather than the same ones again — which
/// the idempotency key would (correctly) refuse.
async fn generate_month(
    account: &AccountStore,
    chart: &Chart,
    rng: &mut Rng,
    batch: &str,
) -> Tally {
    let mut tally = Tally::default();
    let customers: Vec<String> = (1..=rng.cents(1, 8)).map(|n| format!("cust-{n}")).collect();
    let suppliers: Vec<String> = (1..=rng.cents(1, 5)).map(|n| format!("supp-{n}")).collect();
    let people: Vec<String> = (1..=rng.cents(1, 4)).map(|n| format!("user-{n}")).collect();

    let invoices = rng.cents(4, 24);
    for number in 1..=invoices {
        let (currency, rate) = *rng.pick(CURRENCIES);
        let on = day(u8::try_from(rng.cents(1, 28)).unwrap_or(1));
        let customer = rng.pick(&customers).clone();
        let source_id = format!("{batch}inv-{number}");

        // The lines, in the document's own currency, with the tax each rate
        // carries — the AR debit is their gross, as a posting rule will compute
        // it from `billing_totals`.
        let mut postings = Vec::new();
        let mut gross = 0i64;
        for _ in 0..rng.cents(1, 6) {
            let net = rng.cents(100, 500_000);
            let rate_bp = *rng.pick(RATES);
            let vat = net * i64::from(rate_bp) / 10_000;
            gross += net + vat;
            postings.push(NewPosting {
                project_id: rng.chance(3).then(|| format!("proj-{}", rng.cents(1, 3))),
                ..NewPosting::new(chart.revenue.clone(), -net, 0)
            });
            if vat != 0 {
                postings.push(NewPosting {
                    vat_rate_bp: Some(rate_bp),
                    ..NewPosting::new(chart.vat_output.clone(), -vat, 0)
                });
            }
        }
        postings.insert(
            0,
            NewPosting {
                customer_id: Some(customer.clone()),
                memo: format!("INV-2026-{number:05}"),
                ..NewPosting::new(chart.ar.clone(), gross, 0)
            },
        );
        let postings = restate(postings, rate, &chart.rounding, &mut tally.residuals);
        if currency != "EUR" {
            tally.foreign += 1;
        }

        let fx = if currency == "EUR" {
            FxSnapshot::identity("EUR", on)
        } else {
            FxSnapshot {
                base_currency: "EUR".to_owned(),
                rate_micro: rate,
                rate_date: on,
            }
        };
        let source = EntrySource {
            kind: SourceKind::Invoice,
            id: source_id.clone(),
            event: SourceEvent::Issue,
        };
        let entry = NewEntry {
            entry_date: on,
            kind: EntryKind::Invoice,
            source: Some(source.clone()),
            memo: format!("INV-2026-{number:05}"),
            reverses_entry_id: None,
            attachment_node_id: None,
            currency: currency.to_owned(),
            fx: fx.clone(),
            postings: postings.clone(),
        };
        let id = account.post_fin_entry(&entry).await.unwrap();
        tally.record(id, &postings, &chart.vat_output);
        tally.sources.push(source);
        tally
            .invoices
            .push((source_id.clone(), postings, on, currency.to_owned(), fx));

        // 0–2 payments against it. The bank takes the money at the day's own
        // rate, so the euro leg rarely matches what the invoice was booked at —
        // which is what `fx_diff` is for, and what makes the base column an
        // independent balance rather than a copy of the document column.
        for part in 0..rng.upto(2) {
            let paid_on = day(u8::try_from(rng.cents(1, 30)).unwrap_or(1));
            let amount = (gross / 2).max(1);
            let settle_rate = if currency == "EUR" {
                1_000_000
            } else {
                rate + rng.cents(-20_000, 20_000)
            };
            let mut postings = vec![
                NewPosting {
                    memo: "receipt".to_owned(),
                    ..NewPosting::new(chart.bank.clone(), amount, 0)
                },
                NewPosting {
                    customer_id: Some(customer.clone()),
                    ..NewPosting::new(chart.ar.clone(), -amount, 0)
                },
            ];
            postings[0].base_cents =
                convert_cents(amount, settle_rate).expect("a positive rate converts");
            // The receivable is relieved at the rate it was booked at; the
            // difference between the two is the exchange result.
            postings[1].base_cents =
                -convert_cents(amount, rate).expect("a positive rate converts");
            let difference = postings[0].base_cents + postings[1].base_cents;
            if difference != 0 {
                tally.differences += 1;
                postings.push(NewPosting {
                    memo: "exchange difference".to_owned(),
                    ..NewPosting::new(chart.fx_diff.clone(), 0, -difference)
                });
            }
            let source = EntrySource {
                kind: SourceKind::Payment,
                id: format!("{batch}pay-{number}-{part}"),
                event: SourceEvent::Settle,
            };
            let entry = NewEntry {
                entry_date: paid_on,
                kind: EntryKind::Payment,
                source: Some(source.clone()),
                memo: format!("payment on INV-2026-{number:05}"),
                reverses_entry_id: None,
                attachment_node_id: None,
                currency: currency.to_owned(),
                fx: FxSnapshot {
                    base_currency: "EUR".to_owned(),
                    rate_micro: settle_rate,
                    rate_date: paid_on,
                },
                postings: postings.clone(),
            };
            let id = account.post_fin_entry(&entry).await.unwrap();
            tally.record(id, &postings, &chart.vat_output);
            tally.sources.push(source);
        }
    }

    // Approved supplier bills: the other side of the same shape.
    for number in 1..=rng.cents(1, 12) {
        let on = day(u8::try_from(rng.cents(1, 28)).unwrap_or(1));
        let supplier = rng.pick(&suppliers).clone();
        let net = rng.cents(500, 300_000);
        let rate_bp = *rng.pick(RATES);
        let vat = net * i64::from(rate_bp) / 10_000;
        let mut postings = vec![NewPosting {
            supplier_key: Some(supplier.clone()),
            ..NewPosting::new(chart.expense.clone(), net, net)
        }];
        if vat != 0 {
            postings.push(NewPosting {
                vat_rate_bp: Some(rate_bp),
                ..NewPosting::new(chart.vat_input.clone(), vat, vat)
            });
        }
        postings.push(NewPosting {
            supplier_key: Some(supplier),
            ..NewPosting::new(chart.ap.clone(), -(net + vat), -(net + vat))
        });
        let source = EntrySource {
            kind: SourceKind::Bill,
            id: format!("{batch}bill-{number}"),
            event: SourceEvent::Approve,
        };
        let entry = NewEntry {
            entry_date: on,
            kind: EntryKind::Bill,
            source: Some(source.clone()),
            memo: format!("bill {number}"),
            reverses_entry_id: None,
            attachment_node_id: None,
            currency: "EUR".to_owned(),
            fx: FxSnapshot::identity("EUR", on),
            postings: postings.clone(),
        };
        let id = account.post_fin_entry(&entry).await.unwrap();
        tally.record(id, &postings, &chart.vat_output);
        tally.sources.push(source);
    }

    // Approved expense claims, paid by the employee: what we owe a person.
    for number in 1..=rng.cents(1, 10) {
        let on = day(u8::try_from(rng.cents(1, 28)).unwrap_or(1));
        let person = rng.pick(&people).clone();
        let net = rng.cents(200, 40_000);
        let rate_bp = *rng.pick(RATES);
        let vat = net * i64::from(rate_bp) / 10_000;
        let mut postings = vec![NewPosting {
            user_id: Some(person.clone()),
            project_id: rng.chance(2).then(|| format!("proj-{}", rng.cents(1, 3))),
            ..NewPosting::new(chart.expense.clone(), net, net)
        }];
        if vat != 0 {
            postings.push(NewPosting {
                vat_rate_bp: Some(rate_bp),
                ..NewPosting::new(chart.vat_input.clone(), vat, vat)
            });
        }
        postings.push(NewPosting {
            user_id: Some(person),
            ..NewPosting::new(chart.employee_payable.clone(), -(net + vat), -(net + vat))
        });
        let source = EntrySource {
            kind: SourceKind::Expense,
            id: format!("{batch}exp-{number}"),
            event: SourceEvent::Approve,
        };
        let entry = NewEntry {
            entry_date: on,
            kind: EntryKind::Expense,
            source: Some(source.clone()),
            memo: format!("expense {number}"),
            reverses_entry_id: None,
            attachment_node_id: None,
            currency: "EUR".to_owned(),
            fx: FxSnapshot::identity("EUR", on),
            postings: postings.clone(),
        };
        let id = account.post_fin_entry(&entry).await.unwrap();
        tally.record(id, &postings, &chart.vat_output);
        tally.sources.push(source);
    }

    tally
}

/// Reads every posting of a tenant back, entry by entry, as the tuple P8
/// compares.
async fn all_postings(
    account: &AccountStore,
    entries: &[FinEntryId],
) -> Vec<(String, String, i64, i64)> {
    let mut rows = Vec::new();
    for id in entries {
        for posting in account.fin_entry_postings(id).await.unwrap() {
            rows.push((
                posting.entry_id.as_str().to_owned(),
                posting.account_id.as_str().to_owned(),
                posting.amount_cents,
                posting.base_cents,
            ));
        }
    }
    rows.sort();
    rows
}

/// **P1 and P2**, plus the aggregate the reports will read.
///
/// Every entry of a generated month balances in the document column and in the
/// accounting column; no posting moves nothing in both; the health query — which
/// asks the database, not this process — finds nothing unbalanced; and
/// `fin_trial_balance` reproduces, account by account, the tally the generator
/// kept while posting.
#[tokio::test]
async fn every_generated_month_balances_in_both_columns() {
    let store = common::test_store().await;
    let (account, _) = tenant_with_chart(&store, "p1").await;
    let chart = Chart::of(&account).await;

    // Several independent months, each from its own seed: one month is one
    // sample, and a property asserted on one sample is an example.
    for seed in [0x5EED_0001_u64, 0xA11CE, 0x0FF1CE, 0xD15EA5E] {
        let mut rng = Rng(seed);
        let (account, _) = tenant_with_chart(&store, &format!("p1-{seed:x}")).await;
        let chart = Chart::of(&account).await;
        let tally = generate_month(&account, &chart, &mut rng, "").await;
        assert!(
            tally.entries.len() >= 6,
            "seed {seed:x} generated too small a month to prove anything"
        );
        // The month must contain the cases that are hard, not only the easy
        // ones. Without this the suite can go green for years while quietly
        // testing nothing but same-currency arithmetic.
        assert!(
            tally.foreign > 0,
            "seed {seed:x} raised nothing in a foreign currency"
        );
        assert!(
            tally.residuals > 0,
            "seed {seed:x} produced no rounding residual — the base column never \
             disagreed with the document column"
        );
        assert!(
            tally.differences > 0,
            "seed {seed:x} produced no exchange difference — P2's exception went \
             untested"
        );

        // P1, entry by entry, read back from the database.
        for id in &tally.entries {
            let entry = account
                .fin_journal_entry(id)
                .await
                .unwrap()
                .expect("posted");
            let document: i64 = entry.postings.iter().map(|p| p.amount_cents).sum();
            let base: i64 = entry.postings.iter().map(|p| p.base_cents).sum();
            assert_eq!(document, 0, "seed {seed:x}: entry {id:?} is unbalanced");
            assert_eq!(base, 0, "seed {seed:x}: entry {id:?} is unbalanced in EUR");
            assert!(entry.postings.len() >= 2, "seed {seed:x}: one-legged entry");

            // P2: the FX-difference exception is not a hole.
            for posting in &entry.postings {
                assert!(
                    posting.amount_cents != 0 || posting.base_cents != 0,
                    "seed {seed:x}: posting {posting:?} moves no money at all"
                );
            }
        }

        // P1 again, asked of the database rather than of this process.
        assert!(
            account.fin_unbalanced_entries().await.unwrap().is_empty(),
            "seed {seed:x}: the health query found an unbalanced entry"
        );

        // The aggregate the reports are folds over reproduces the generator's
        // own tally — the standing-in-for-P10 check the module header explains.
        let trial = account.fin_trial_balance(None, None).await.unwrap();
        assert!(
            trial.balances(),
            "seed {seed:x}: the trial balance does not"
        );
        assert_eq!(
            trial.debit_cents,
            trial.accounts.iter().map(|a| a.debit_cents).sum::<i64>()
        );
        let posted: i64 = trial.accounts.iter().map(|a| a.postings).sum();
        assert_eq!(
            usize::try_from(posted).unwrap_or_default(),
            tally.postings,
            "seed {seed:x}: the trial balance counts a different number of postings"
        );
        for account_row in &trial.accounts {
            let expected = tally
                .by_account
                .get(account_row.account_id.as_str())
                .copied()
                .unwrap_or_default();
            assert_eq!(
                account_row.balance_cents, expected,
                "seed {seed:x}: account {} disagrees with the documents",
                account_row.code
            );
            assert_eq!(
                account_row.debit_cents - account_row.credit_cents,
                account_row.balance_cents,
                "seed {seed:x}: the two columns of {} do not make its balance",
                account_row.code
            );
        }

        // And the two grouped reads, against the same tally: receivables by
        // customer, output tax by rate — the shapes B4.11c and B4.11d are.
        let receivables = account
            .fin_dimension_balances(
                &LedgerScope::Role(AccountRole::Ar),
                LedgerDimension::Customer,
                None,
                None,
            )
            .await
            .unwrap();
        assert!(!receivables.truncated);
        for row in &receivables.rows {
            let customer = row
                .value
                .clone()
                .expect("every receivable names a customer");
            assert_eq!(
                row.balance_cents,
                tally
                    .receivable_by_customer
                    .get(&customer)
                    .copied()
                    .unwrap_or_default(),
                "seed {seed:x}: {customer}'s receivable disagrees"
            );
        }

        let tax = account
            .fin_dimension_balances(
                &LedgerScope::Role(AccountRole::VatOutput),
                LedgerDimension::VatRate,
                None,
                None,
            )
            .await
            .unwrap();
        for row in &tax.rows {
            let rate = row.vat_rate_bp().expect("every tax posting names its rate");
            assert_eq!(
                row.balance_cents,
                tally
                    .output_tax_by_rate
                    .get(&rate)
                    .copied()
                    .unwrap_or_default(),
                "seed {seed:x}: the output tax at {rate}bp disagrees"
            );
        }

        // The drill-down behind a figure adds up to it: the account ledger's
        // closing balance is that account's trial-balance row.
        let bank = trial
            .accounts
            .iter()
            .find(|row| row.account_id.as_str() == chart.bank.as_str());
        if let Some(bank_row) = bank {
            let ledger = account
                .fin_account_ledger(&chart.bank, None, None, 2_000)
                .await
                .unwrap();
            assert!(
                !ledger.truncated,
                "seed {seed:x}: the bank ledger overflowed"
            );
            assert_eq!(ledger.opening_cents, 0);
            assert_eq!(
                ledger.closing_cents, bank_row.balance_cents,
                "seed {seed:x}: the bank drill-down does not add up to its balance"
            );
            // The running column is the opening plus every line before it.
            let mut running = 0;
            for line in &ledger.lines {
                running += line.base_cents;
                assert_eq!(line.running_cents, running);
            }
        }
    }

    // The first tenant of the test never had a month generated into it, and its
    // books are therefore empty rather than "nearly empty".
    let empty = account.fin_trial_balance(None, None).await.unwrap();
    assert!(empty.accounts.is_empty());
    assert!(empty.balances());
    assert_eq!(
        account
            .fin_account_ledger(&chart.ar, None, None, 50)
            .await
            .unwrap()
            .lines
            .len(),
        0
    );
}

/// **P4**: a document and its full credit note leave the ledger exactly where
/// they found it — per account and per dimension, not merely in total.
#[tokio::test]
async fn a_document_and_its_credit_note_leave_the_ledger_where_they_found_it() {
    let store = common::test_store().await;
    let (account, _) = tenant_with_chart(&store, "p4").await;
    let chart = Chart::of(&account).await;
    let mut rng = Rng(0xC0FFEE);
    let tally = generate_month(&account, &chart, &mut rng, "").await;

    let before = account.fin_trial_balance(None, None).await.unwrap();
    let receivables_before = account
        .fin_dimension_balances(
            &LedgerScope::Role(AccountRole::Ar),
            LedgerDimension::Customer,
            None,
            None,
        )
        .await
        .unwrap();

    // Credit every invoice the month issued, as B1.09's credit note does: the
    // exact mirror, dated on or after the original, on the same rate snapshot
    // (the tax point does not move because a correction was raised later).
    let mut credited = 0;
    for (source_id, postings, on, currency, fx) in &tally.invoices {
        let mirror: Vec<NewPosting> = postings
            .iter()
            .map(|posting| NewPosting {
                amount_cents: -posting.amount_cents,
                base_cents: -posting.base_cents,
                ..posting.clone()
            })
            .collect();
        let entry = NewEntry {
            entry_date: *on,
            kind: EntryKind::CreditNote,
            source: Some(EntrySource {
                kind: SourceKind::Invoice,
                id: source_id.clone(),
                event: SourceEvent::Void,
            }),
            memo: format!("credit note against {source_id}"),
            reverses_entry_id: None,
            attachment_node_id: None,
            currency: currency.clone(),
            fx: fx.clone(),
            postings: mirror,
        };
        account.post_fin_entry(&entry).await.unwrap();
        credited += 1;
    }
    assert!(credited > 0, "the month issued no invoice to credit");

    // Every invoice and its credit note now net to zero, so the only balances
    // left are the ones the payments, bills and expenses put there.
    let after = account.fin_trial_balance(None, None).await.unwrap();
    assert!(after.balances());
    for row in &after.accounts {
        let was = before
            .accounts
            .iter()
            .find(|earlier| earlier.account_id == row.account_id)
            .map(|earlier| earlier.balance_cents)
            .unwrap_or_default();
        let invoiced: i64 = tally
            .invoices
            .iter()
            .flat_map(|(_, postings, ..)| postings.iter())
            .filter(|posting| posting.account_id == row.account_id)
            .map(|posting| posting.base_cents)
            .sum();
        assert_eq!(
            row.balance_cents,
            was - invoiced,
            "crediting every invoice should have removed exactly the invoices from {}",
            row.code
        );
    }

    // Per dimension too: what each customer is owed drops by exactly what they
    // were invoiced.
    let receivables_after = account
        .fin_dimension_balances(
            &LedgerScope::Role(AccountRole::Ar),
            LedgerDimension::Customer,
            None,
            None,
        )
        .await
        .unwrap();
    for row in &receivables_after.rows {
        let was = receivables_before
            .rows
            .iter()
            .find(|earlier| earlier.value == row.value)
            .map(|earlier| earlier.balance_cents)
            .unwrap_or_default();
        let invoiced: i64 = tally
            .invoices
            .iter()
            .flat_map(|(_, postings, ..)| postings.iter())
            .filter(|posting| posting.account_id == chart.ar && posting.customer_id == row.value)
            .map(|posting| posting.base_cents)
            .sum();
        assert_eq!(row.balance_cents, was - invoiced);
    }

    assert!(account.fin_unbalanced_entries().await.unwrap().is_empty());
}

/// **P7**: posting a document event a second time yields a typed conflict, one
/// entry, and not one extra posting — under any number of retries.
#[tokio::test]
async fn posting_the_same_document_event_twice_changes_nothing() {
    let store = common::test_store().await;
    let (account, _) = tenant_with_chart(&store, "p7").await;
    let chart = Chart::of(&account).await;
    let mut rng = Rng(0xBEEF_0007);
    let tally = generate_month(&account, &chart, &mut rng, "").await;

    let before = account.fin_trial_balance(None, None).await.unwrap();
    let entries_before = account.fin_entries(None, None, 500).await.unwrap().len();

    // A backfill re-run: every source event of the month, posted again, with
    // deliberately different amounts so a second write would be visible.
    for source in &tally.sources {
        let entry = NewEntry {
            entry_date: day(28),
            kind: EntryKind::Manual,
            source: Some(source.clone()),
            memo: "the backfill, run twice".to_owned(),
            reverses_entry_id: None,
            attachment_node_id: None,
            currency: "EUR".to_owned(),
            fx: FxSnapshot::identity("EUR", day(28)),
            postings: vec![
                NewPosting::new(chart.bank.clone(), 99_999, 99_999),
                NewPosting::new(chart.revenue.clone(), -99_999, -99_999),
            ],
        };
        match account.post_fin_entry(&entry).await {
            Err(StoreError::Conflict(message)) => assert!(
                message.contains("already posted"),
                "conflict should name the rule, got {message:?}"
            ),
            other => panic!("a re-post must conflict, got {other:?}"),
        }
        // And the question behind the key answers with the entry that stands.
        assert!(
            account
                .fin_entry_for_source(source)
                .await
                .unwrap()
                .is_some(),
            "the original entry must still answer for its document event"
        );
    }

    let after = account.fin_trial_balance(None, None).await.unwrap();
    assert_eq!(after, before, "a refused re-post moved a balance");
    assert_eq!(
        account.fin_entries(None, None, 500).await.unwrap().len(),
        entries_before
    );
    assert!(account.fin_unbalanced_entries().await.unwrap().is_empty());
}

/// **P8**: the journal is append-only, proven behaviourally — after a second
/// month posted on top, every posting written by the first is still there,
/// tuple for tuple, and so is every entry header.
#[tokio::test]
async fn every_posting_written_is_still_there_byte_for_byte() {
    let store = common::test_store().await;
    let (account, _) = tenant_with_chart(&store, "p8").await;
    let chart = Chart::of(&account).await;

    let mut rng = Rng(0x5111_0008);
    let first = generate_month(&account, &chart, &mut rng, "one-").await;
    let written = all_postings(&account, &first.entries).await;
    let headers: Vec<(String, Date, String)> = {
        let mut rows = Vec::new();
        for id in &first.entries {
            let entry = account.fin_entry(id).await.unwrap().expect("posted");
            rows.push((
                entry.id.as_str().to_owned(),
                entry.entry_date,
                entry.memo.clone(),
            ));
        }
        rows.sort();
        rows
    };

    // A second month, reversals of a few of the first's entries, and a batch of
    // refused writes — everything the API can do to the books.
    let mut rng = Rng(0x5111_0009);
    let second = generate_month(&account, &chart, &mut rng, "two-").await;
    for id in first.entries.iter().take(3) {
        let entry = account
            .fin_journal_entry(id)
            .await
            .unwrap()
            .expect("posted");
        let mirror: Vec<NewPosting> = entry
            .postings
            .iter()
            .map(|posting: &Posting| NewPosting {
                vat_rate_bp: posting.vat_rate_bp,
                customer_id: posting.customer_id.clone(),
                supplier_key: posting.supplier_key.clone(),
                project_id: posting.project_id.clone(),
                user_id: posting.user_id.clone(),
                memo: posting.memo.clone(),
                ..NewPosting::new(
                    posting.account_id.clone(),
                    -posting.amount_cents,
                    -posting.base_cents,
                )
            })
            .collect();
        account
            .post_fin_entry(&NewEntry {
                entry_date: entry.entry.entry_date,
                kind: EntryKind::Reversal,
                source: None,
                memo: "a correction".to_owned(),
                reverses_entry_id: Some(entry.entry.id.clone()),
                attachment_node_id: None,
                currency: entry.entry.currency.clone(),
                fx: entry.entry.fx.clone(),
                postings: mirror,
            })
            .await
            .unwrap();
    }
    // Refusals leave nothing behind either.
    let unbalanced = NewEntry {
        entry_date: day(15),
        kind: EntryKind::Manual,
        source: None,
        memo: "wrong".to_owned(),
        reverses_entry_id: None,
        attachment_node_id: None,
        currency: "EUR".to_owned(),
        fx: FxSnapshot::identity("EUR", day(15)),
        postings: vec![
            NewPosting::new(chart.bank.clone(), 1_000, 1_000),
            NewPosting::new(chart.revenue.clone(), -999, -999),
        ],
    };
    assert!(account.post_fin_entry(&unbalanced).await.is_err());

    assert_eq!(
        all_postings(&account, &first.entries).await,
        written,
        "a posting written earlier changed"
    );
    let mut headers_now: Vec<(String, Date, String)> = Vec::new();
    for id in &first.entries {
        let entry = account.fin_entry(id).await.unwrap().expect("still posted");
        headers_now.push((
            entry.id.as_str().to_owned(),
            entry.entry_date,
            entry.memo.clone(),
        ));
    }
    headers_now.sort();
    assert_eq!(headers_now, headers, "an entry header changed");
    assert!(!second.entries.is_empty());
    assert!(account.fin_unbalanced_entries().await.unwrap().is_empty());
}

/// **P9**: one tenant's month moves nothing of another's — the aggregate form
/// of law 1, which is the form that catches a total missing its `tenant_id`
/// rather than a single read.
#[tokio::test]
async fn one_tenants_month_leaves_another_tenants_books_untouched() {
    let store = common::test_store().await;
    let (alpha, _) = tenant_with_chart(&store, "p9-a").await;
    let (beta, _) = tenant_with_chart(&store, "p9-b").await;
    let alpha_chart = Chart::of(&alpha).await;
    let beta_chart = Chart::of(&beta).await;

    let mut rng = Rng(0x9999_0001);
    let beta_month = generate_month(&beta, &beta_chart, &mut rng, "").await;
    let beta_before = beta.fin_trial_balance(None, None).await.unwrap();
    let beta_receivables = beta
        .fin_dimension_balances(
            &LedgerScope::Role(AccountRole::Ar),
            LedgerDimension::Customer,
            None,
            None,
        )
        .await
        .unwrap();
    let beta_ledger = beta
        .fin_account_ledger(&beta_chart.ar, None, None, 2_000)
        .await
        .unwrap();

    let mut rng = Rng(0x9999_0002);
    let alpha_month = generate_month(&alpha, &alpha_chart, &mut rng, "").await;

    // Beta's every aggregate is what it was before alpha existed.
    assert_eq!(
        beta.fin_trial_balance(None, None).await.unwrap(),
        beta_before
    );
    assert_eq!(
        beta.fin_dimension_balances(
            &LedgerScope::Role(AccountRole::Ar),
            LedgerDimension::Customer,
            None,
            None,
        )
        .await
        .unwrap(),
        beta_receivables
    );
    assert_eq!(
        beta.fin_account_ledger(&beta_chart.ar, None, None, 2_000)
            .await
            .unwrap(),
        beta_ledger
    );

    // And alpha's own aggregates contain only alpha's accounts — not one row of
    // beta's chart, whose ids are all different and whose balances are large.
    let alpha_trial = alpha.fin_trial_balance(None, None).await.unwrap();
    assert!(!alpha_trial.accounts.is_empty());
    for row in &alpha_trial.accounts {
        assert!(
            beta_before
                .accounts
                .iter()
                .all(|beta_row| beta_row.account_id != row.account_id),
            "alpha's trial balance names an account of beta's chart"
        );
        assert!(
            row.name.starts_with("p9-a "),
            "an account of another tenant's chart leaked into the aggregate: {}",
            row.name
        );
    }

    // Beta's account read through alpha's handle is an empty ledger, not beta's.
    let foreign = alpha
        .fin_account_ledger(&beta_chart.ar, None, None, 2_000)
        .await
        .unwrap();
    assert!(foreign.lines.is_empty());
    assert_eq!(foreign.opening_cents, 0);
    assert_eq!(foreign.closing_cents, 0);
    let scoped = alpha
        .fin_dimension_balances(
            &LedgerScope::Account(beta_chart.ar.clone()),
            LedgerDimension::Customer,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(scoped.rows.is_empty());

    // Neither tenant's entries are visible through the other's handle.
    for id in beta_month.entries.iter().take(5) {
        assert!(alpha.fin_entry(id).await.unwrap().is_none());
        assert!(alpha.fin_entry_postings(id).await.unwrap().is_empty());
    }
    for id in alpha_month.entries.iter().take(5) {
        assert!(beta.fin_entry(id).await.unwrap().is_none());
    }
    assert!(alpha.fin_unbalanced_entries().await.unwrap().is_empty());
    assert!(beta.fin_unbalanced_entries().await.unwrap().is_empty());
}

/// The period window is judged on the accounting date, and a range never cuts
/// an entry in half — the rule every report in B4.11 depends on for its
/// comparative column.
#[tokio::test]
async fn a_period_window_takes_whole_entries_and_judges_them_by_their_own_date() {
    let store = common::test_store().await;
    let (account, _) = tenant_with_chart(&store, "window").await;
    let chart = Chart::of(&account).await;
    let mut rng = Rng(0x0DAD_0001);
    generate_month(&account, &chart, &mut rng, "").await;

    // Every sub-period of the month balances on its own, because a date bound
    // takes entries whole.
    for (from, to) in [(1, 10), (11, 20), (21, 30), (5, 5), (1, 30)] {
        let trial = account
            .fin_trial_balance(Some(day(from)), Some(day(to)))
            .await
            .unwrap();
        assert!(
            trial.balances(),
            "the window {from}–{to} does not balance: {} vs {}",
            trial.debit_cents,
            trial.credit_cents
        );
    }

    // The three windows above partition the month, so their movements add up to
    // the whole — the property a comparative column is.
    let whole = account.fin_trial_balance(None, None).await.unwrap();
    let mut parts = 0i64;
    for (from, to) in [(1, 10), (11, 20), (21, 30)] {
        parts += account
            .fin_trial_balance(Some(day(from)), Some(day(to)))
            .await
            .unwrap()
            .debit_cents;
    }
    assert_eq!(parts, whole.debit_cents);

    // A window before the books opened is empty, not an error.
    let before = account
        .fin_trial_balance(
            Some(Date::from_calendar_date(2025, Month::January, 1).unwrap()),
            Some(Date::from_calendar_date(2025, Month::December, 31).unwrap()),
        )
        .await
        .unwrap();
    assert!(before.accounts.is_empty());

    // And an account ledger's opening balance is everything strictly before the
    // window: opening + the window's movement is the closing balance of the
    // cumulative read at the same date.
    let ledger = account
        .fin_account_ledger(&chart.ar, Some(day(15)), Some(day(30)), 2_000)
        .await
        .unwrap();
    let cumulative = account
        .fin_trial_balance(None, Some(day(30)))
        .await
        .unwrap()
        .accounts
        .iter()
        .find(|row| row.account_id == chart.ar)
        .map(|row| row.balance_cents)
        .unwrap_or_default();
    assert_eq!(ledger.closing_cents, cumulative);
    let opening_only = account
        .fin_trial_balance(None, Some(day(14)))
        .await
        .unwrap()
        .accounts
        .iter()
        .find(|row| row.account_id == chart.ar)
        .map(|row| row.balance_cents)
        .unwrap_or_default();
    assert_eq!(ledger.opening_cents, opening_only);
}
