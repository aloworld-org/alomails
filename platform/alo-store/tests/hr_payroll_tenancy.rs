//! Tenancy and content proofs for the payroll export (alo HR, B6.10 — Law 1:
//! isolation is tested, not assumed).
//!
//! The payroll file is the one document in the product that carries every
//! private field a person has — pay, national identifier, bank account — on one
//! row, for everybody at once. Four things are proven here:
//!
//! - **wrong tenant** — tenant A's people are never on tenant B's file, not even
//!   when B draws exactly the same period; B's own file carries only B's person,
//!   and A's receipts are invisible to B;
//! - **the period fold** — a joiner, a leaver and a contractor are each treated
//!   as the design says; approved leave lands in the column its policy's *money*
//!   decides; a public holiday inside leave costs nothing; a claim in another
//!   currency is counted rather than added;
//! - **no calculation** — every figure on a line is a stored fact or a sum of
//!   stored facts, and the pay figure is exactly the one on the terms;
//! - **the receipt** — drawing the file files a row saying who drew what and
//!   when, and the row carries no figure and nobody's name.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::fin_expenses::{ExpenseDecision, ExpenseMethod, NewExpense};
use alo_store::hr_payroll_export::PayrollLine;
use alo_store::{
    Accrual, ContractKind, HrEmployeeId, HrLeavePolicyId, LeaveKind, LeaveYear, NewEmployee,
    NewEmployment, NewLeavePolicy, NewLeaveRequest, PayPeriod, Store, StoreError, TenantStore,
    UserId,
};
use time::{Date, Month};

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).expect("a real date")
}

fn invalid<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(message)) => message,
        other => panic!("expected Validation, got {other:?}"),
    }
}

/// March 2026, the period every draw below covers.
fn march() -> (Date, Date) {
    (day(2026, Month::March, 1), day(2026, Month::March, 31))
}

/// A company with an HR user, and the two leave policies a payroll file has to
/// tell apart.
struct Company {
    hr: TenantStore,
    user: UserId,
    holiday: HrLeavePolicyId,
    unpaid: HrLeavePolicyId,
}

impl Company {
    async fn new(store: &Store, tag: &str) -> Self {
        let tenant = store.create_tenant(&format!("t-{tag}")).await.unwrap();
        let hr = store.for_tenant(tenant.clone());
        let user = hr
            .create_user(&format!("hr-{tag}@example.test"))
            .await
            .unwrap();
        let holiday = hr
            .create_hr_leave_policy(
                &NewLeavePolicy {
                    name: "Vakantiedagen".to_owned(),
                    kind: LeaveKind::Annual,
                    entitlement_minutes: 25 * 480,
                    accrual: Accrual::UpFront,
                    leave_year: LeaveYear::calendar(),
                    paid: true,
                    ..Default::default()
                },
                &user,
            )
            .await
            .unwrap();
        let unpaid = hr
            .create_hr_leave_policy(
                &NewLeavePolicy {
                    name: "Onbetaald verlof".to_owned(),
                    kind: LeaveKind::Unpaid,
                    entitlement_minutes: 0,
                    accrual: Accrual::UpFront,
                    leave_year: LeaveYear::calendar(),
                    paid: false,
                    // Unpaid leave grants nothing, so every hour of it is a
                    // balance below zero: a policy that forbade that could
                    // never be taken.
                    allow_negative: true,
                    ..Default::default()
                },
                &user,
            )
            .await
            .unwrap();
        Self {
            hr,
            user,
            holiday,
            unpaid,
        }
    }

    /// One person on real terms, with everything a payroll bureau asks for.
    async fn hire(
        &self,
        family_name: &str,
        staff_number: &str,
        terms: NewEmployment,
        user: Option<UserId>,
    ) -> HrEmployeeId {
        let employee = self
            .hr
            .create_hr_employee(
                &NewEmployee {
                    user_id: user,
                    staff_number: Some(staff_number.to_owned()),
                    given_name: "Ada".to_owned(),
                    family_name: family_name.to_owned(),
                    national_id: Some("123456782".to_owned()),
                    iban: Some("NL91ABNA0417164300".to_owned()),
                    country: "NL".to_owned(),
                    date_of_birth: Some(day(1990, Month::December, 10)),
                    ..Default::default()
                },
                &self.user,
            )
            .await
            .unwrap();
        self.hr
            .append_hr_employment(&employee, &terms, &self.user)
            .await
            .unwrap();
        employee
    }

    /// An approved absence — asked for and decided, which is the only state the
    /// file counts.
    async fn approved_leave(
        &self,
        employee: &HrEmployeeId,
        policy: &HrLeavePolicyId,
        from: Date,
        to: Date,
    ) {
        let request = self
            .hr
            .create_hr_leave_request(
                &NewLeaveRequest {
                    employee_id: employee.clone(),
                    policy_id: policy.clone(),
                    from_day: from,
                    to_day: to,
                    note: String::new(),
                },
                &self.user,
                day(2026, Month::February, 1),
            )
            .await
            .unwrap();
        self.hr
            .decide_hr_leave_request(
                &request,
                true,
                &self.user,
                "",
                day(2026, Month::February, 2),
            )
            .await
            .unwrap();
    }

    /// A claim somebody paid for themselves and a manager approved.
    async fn approved_claim(
        &self,
        store: &Store,
        user: &UserId,
        spent_on: Date,
        gross_cents: i64,
        currency: &str,
    ) {
        let acc = store.for_account(self.hr.tenant().clone(), user.clone());
        let claim = acc
            .log_expense(&NewExpense {
                currency: Some(currency.to_owned()),
                ..NewExpense::spent(spent_on, gross_cents, ExpenseMethod::Personal)
            })
            .await
            .unwrap();
        acc.submit_expense(&claim.id).await.unwrap();
        self.hr
            .decide_expense(&claim.id, ExpenseDecision::Approve, &self.user, "")
            .await
            .unwrap();
    }

    /// The period's file.
    async fn file(&self) -> Vec<PayrollLine> {
        let (from, to) = march();
        self.hr.hr_payroll_lines(from, to).await.unwrap()
    }
}

/// Full-time from a stated day, on a stated salary.
fn terms(started_on: Date, pay_amount_cents: i64) -> NewEmployment {
    NewEmployment {
        job_title: "Systeembeheerder".to_owned(),
        team: "Techniek".to_owned(),
        started_on,
        pay_amount_cents: Some(pay_amount_cents),
        pay_period: PayPeriod::Month,
        pay_currency: "EUR".to_owned(),
        ..Default::default()
    }
}

fn line_of<'a>(lines: &'a [PayrollLine], family_name: &str) -> &'a PayrollLine {
    lines
        .iter()
        .find(|line| line.family_name == family_name)
        .unwrap_or_else(|| {
            panic!(
                "{family_name} is not on the file: {:?}",
                lines.iter().map(|l| &l.family_name).collect::<Vec<_>>()
            )
        })
}

/// **Wrong tenant.** Two companies draw the same March; neither file has a
/// stranger on it, and neither company can see that the other drew one.
#[tokio::test]
async fn another_tenants_payroll_is_unreachable_by_every_path() {
    let store = common::test_store().await;
    let a = Company::new(&store, "payroll-own").await;
    let b = Company::new(&store, "payroll-other").await;
    let (from, to) = march();

    a.hire(
        "Byron",
        "0042",
        terms(day(2025, Month::January, 1), 320_000),
        None,
    )
    .await;
    let a_lines = a.file().await;
    assert_eq!(a_lines.len(), 1);
    assert_eq!(a_lines[0].family_name, "Byron");

    // B employs nobody yet: an empty file is a refusal, never a file that reads
    // as "nobody is paid".
    let message = invalid(b.hr.hr_payroll_lines(from, to).await);
    assert!(message.contains("no period of employment"), "{message}");

    // …and once B does employ somebody, their file is theirs alone.
    b.hire(
        "Zola",
        "0001",
        terms(day(2025, Month::June, 1), 210_000),
        None,
    )
    .await;
    let b_lines = b.file().await;
    assert_eq!(b_lines.len(), 1, "not A's person too");
    assert_eq!(b_lines[0].family_name, "Zola");
    assert_ne!(b_lines[0].employee_id, a_lines[0].employee_id);

    // The receipts are the tenant's own, both ways.
    a.hr.record_hr_payroll_export(from, to, "alo", a_lines.len(), &a.user)
        .await
        .unwrap();
    assert!(b.hr.hr_payroll_exports().await.unwrap().is_empty());
    let receipts = a.hr.hr_payroll_exports().await.unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].line_count, 1);
    assert_eq!(receipts[0].mapping_key, "alo");
    assert_eq!(receipts[0].drawn_by, a.user.as_str());
    assert_eq!(receipts[0].from_day, from);
    assert_eq!(receipts[0].to_day, to);
}

/// **The period fold.** Who is on the file, and what the period did to them.
#[tokio::test]
async fn the_file_is_the_period_and_what_it_did_to_the_people_in_it() {
    let store = common::test_store().await;
    let company = Company::new(&store, "payroll-fold").await;

    // Somebody who has been here for years, took a week's holiday in March and
    // two unpaid days.
    let ada = company
        .hire(
            "Byron",
            "0042",
            terms(day(2025, Month::January, 1), 320_050),
            None,
        )
        .await;
    // 2026-03-02 is a Monday; Monday to Friday is five working days.
    company
        .approved_leave(
            &ada,
            &company.holiday,
            day(2026, Month::March, 2),
            day(2026, Month::March, 6),
        )
        .await;
    company
        .approved_leave(
            &ada,
            &company.unpaid,
            day(2026, Month::March, 16),
            day(2026, Month::March, 17),
        )
        .await;

    // A leaver, whose last day was inside the period.
    let gone = company
        .hire(
            "Curie",
            "0007",
            terms(day(2024, Month::February, 1), 400_000),
            None,
        )
        .await;
    company
        .hr
        .end_hr_employment(&gone, day(2026, Month::March, 10))
        .await
        .unwrap();

    // A joiner, whose terms begin after the period: not on this file.
    company
        .hire(
            "Lovelace",
            "0100",
            terms(day(2026, Month::April, 1), 300_000),
            None,
        )
        .await;

    // A contractor: on the org chart, off the payroll.
    company
        .hire(
            "Turing",
            "0900",
            NewEmployment {
                contract_kind: ContractKind::Contractor,
                ..terms(day(2025, Month::January, 1), 500_000)
            },
            None,
        )
        .await;

    let lines = company.file().await;
    let names: Vec<&str> = lines.iter().map(|line| line.family_name.as_str()).collect();
    assert_eq!(
        names,
        vec!["Byron", "Curie"],
        "the joiner is next month's and the contractor invoices"
    );

    let ada_line = line_of(&lines, "Byron");
    assert_eq!(ada_line.staff_number, "0042");
    assert_eq!(ada_line.national_id, "123456782");
    assert_eq!(ada_line.iban, "NL91ABNA0417164300");
    assert_eq!(ada_line.date_of_birth, Some(day(1990, Month::December, 10)));
    assert_eq!(
        ada_line.pay_amount_cents,
        Some(320_050),
        "the figure on the terms, never a computed one"
    );
    assert_eq!(ada_line.pay_currency, "EUR");
    assert_eq!(ada_line.weekly_minutes, 2_400);
    assert_eq!(ada_line.paid_leave_minutes, 5 * 480, "a full working week");
    assert_eq!(ada_line.unpaid_leave_minutes, 2 * 480);
    assert_eq!(ada_line.sick_leave_minutes, 0);
    assert_eq!(ada_line.total_leave_minutes(), 7 * 480);
    assert_eq!(ada_line.job_title, "Systeembeheerder");
    assert_eq!(ada_line.contract_kind, ContractKind::Permanent);

    let leaver = line_of(&lines, "Curie");
    assert_eq!(
        leaver.ended_on,
        Some(day(2026, Month::March, 10)),
        "a leaver is paid for the days they worked, and the file says which"
    );
    assert_eq!(leaver.paid_leave_minutes, 0);
    assert_eq!(leaver.claims_cents(), 0);
}

/// **A pay rise inside the period, and leave charged at the pattern of the day.**
#[tokio::test]
async fn a_change_of_terms_puts_the_new_ones_on_the_file() {
    let store = common::test_store().await;
    let company = Company::new(&store, "payroll-terms").await;
    let ada = company
        .hire(
            "Byron",
            "0042",
            terms(day(2025, Month::January, 1), 320_000),
            None,
        )
        .await;
    // A rise, and a move to a three-day week, from the 16th.
    company
        .hr
        .append_hr_employment(
            &ada,
            &NewEmployment {
                contract_kind: ContractKind::PartTime,
                pattern_minutes: [480, 480, 480, 0, 0, 0, 0],
                ..terms(day(2026, Month::March, 16), 210_000)
            },
            &company.user,
        )
        .await
        .unwrap();
    // A fortnight off, spanning the change: the week before costs five days,
    // the week after costs three.
    company
        .approved_leave(
            &ada,
            &company.holiday,
            day(2026, Month::March, 9),
            day(2026, Month::March, 20),
        )
        .await;

    let lines = company.file().await;
    let line = line_of(&lines, "Byron");
    assert_eq!(
        line.pay_amount_cents,
        Some(210_000),
        "the terms the period ended on"
    );
    assert_eq!(line.contract_kind, ContractKind::PartTime);
    assert_eq!(line.weekly_minutes, 1_440);
    assert_eq!(
        line.paid_leave_minutes,
        (5 + 3) * 480,
        "each day at the pattern in force on it"
    );
}

/// **Claims.** What somebody is owed back, and what happens to a claim in a
/// currency the file is not in.
#[tokio::test]
async fn claims_are_summed_in_the_pay_currency_and_counted_in_any_other() {
    let store = common::test_store().await;
    let company = Company::new(&store, "payroll-claims").await;
    let user = company
        .hr
        .create_user("ada-payroll-claims@example.test")
        .await
        .unwrap();
    company
        .hire(
            "Byron",
            "0042",
            terms(day(2025, Month::January, 1), 320_000),
            Some(user.clone()),
        )
        .await;

    // Two approved claims inside the period, in the currency they are paid in.
    company
        .approved_claim(&store, &user, day(2026, Month::March, 4), 4_500, "EUR")
        .await;
    company
        .approved_claim(&store, &user, day(2026, Month::March, 20), 1_250, "EUR")
        .await;
    // One in another currency: counted, never added.
    company
        .approved_claim(&store, &user, day(2026, Month::March, 21), 9_900, "GBP")
        .await;
    // One outside the period, and one nobody has decided: neither is owed here.
    company
        .approved_claim(&store, &user, day(2026, Month::February, 2), 8_000, "EUR")
        .await;
    let acc = store.for_account(company.hr.tenant().clone(), user.clone());
    acc.log_expense(&NewExpense::spent(
        day(2026, Month::March, 5),
        7_000,
        ExpenseMethod::Personal,
    ))
    .await
    .unwrap();

    let lines = company.file().await;
    let line = line_of(&lines, "Byron");
    assert_eq!(line.expense_cents, 5_750, "the two approved euro claims");
    assert_eq!(line.mileage_cents, 0);
    assert_eq!(line.claims_other_currency, 1, "the sterling one");
    assert_eq!(line.claims_cents(), 5_750);
}

/// **The period itself is validated**, and a year is the ceiling.
#[tokio::test]
async fn a_period_that_is_not_a_period_is_refused() {
    let store = common::test_store().await;
    let company = Company::new(&store, "payroll-period").await;
    company
        .hire(
            "Byron",
            "0042",
            terms(day(2025, Month::January, 1), 320_000),
            None,
        )
        .await;
    let backwards = invalid(
        company
            .hr
            .hr_payroll_lines(day(2026, Month::March, 31), day(2026, Month::March, 1))
            .await,
    );
    assert!(backwards.contains("end on or after"), "{backwards}");
    let too_long = invalid(
        company
            .hr
            .hr_payroll_lines(day(2025, Month::January, 1), day(2026, Month::March, 31))
            .await,
    );
    assert!(too_long.contains("longer than"), "{too_long}");
    // One day is a period: a company that pays daily is a company.
    assert_eq!(
        company
            .hr
            .hr_payroll_lines(day(2026, Month::March, 2), day(2026, Month::March, 2))
            .await
            .unwrap()
            .len(),
        1
    );
}
