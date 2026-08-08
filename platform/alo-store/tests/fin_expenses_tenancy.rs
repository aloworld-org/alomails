//! Tenancy proof for alo Finance's expense claims and the categories that
//! classify them (Law 1: isolation is tested, not assumed), plus the CRUD arc
//! and the two rules that decide who may see a claim at all.
//!
//! There are **two** isolation questions here and they have different answers,
//! which is the whole reason this suite exists:
//!
//! - A **category** is tenant-wide configuration, like the chart it points
//!   into: a co-tenant reads the same list. An outsider tenant gets the clean
//!   `NotFound`/empty on every path — read, list, update, deactivate, delete —
//!   and cannot point a category of their own at another tenant's account.
//! - A **claim** is personal data about one employee: a *colleague inside the
//!   same tenant* is as blind to it as another tenant entirely. Absent, never
//!   `Forbidden`, which would confirm somebody claimed something that day.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    AccountRole, AccountStore, AccountType, CHART, ChartName, ChartSeed, DriveNodeId,
    ExpenseMethod, ExpenseStatus, FinAccountId, FinCategoryId, FinExpenseId, NewAccount,
    NewExpense, NewExpenseCategory, Store, StoreError, TenantId,
};
use time::{Date, Month};

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error.
fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

/// Asserts a result is a typed conflict whose message names the rule.
fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>, expect: &str) {
    match result {
        Err(StoreError::Conflict(msg)) => {
            assert!(
                msg.contains(expect),
                "conflict {msg:?} should name {expect:?}"
            );
        }
        other => panic!("expected Conflict naming {expect:?}, got: {other:?}"),
    }
}

/// Asserts a result is a typed validation failure whose message names the rule.
fn assert_invalid<T: std::fmt::Debug>(result: Result<T, StoreError>, expect: &str) {
    match result {
        Err(StoreError::Validation(msg)) => {
            assert!(
                msg.contains(expect),
                "validation {msg:?} should name {expect:?}"
            );
        }
        other => panic!("expected Validation naming {expect:?}, got: {other:?}"),
    }
}

/// The chart seed as the HTTP edge hands it in — names in the caller's own
/// language, tagged per tenant so a leak would show itself.
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

/// A tenant with one user and a seeded chart, returning the account door, the
/// tenant id and the tenant's `expense_default` account.
async fn tenant_with_chart(store: &Store, tag: &str) -> (AccountStore, TenantId, FinAccountId) {
    let tenant = store.create_tenant(&format!("exp-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@expenses.test"))
        .await
        .unwrap();
    let door = store.for_account(tenant.clone(), user);
    door.fin_accounts_or_seed(&seed(tag), false).await.unwrap();
    let account = door
        .fin_account_for_role(AccountRole::ExpenseDefault)
        .await
        .unwrap()
        .expect("the seeded chart holds expense_default")
        .id;
    (door, tenant, account)
}

fn day(d: u8) -> Date {
    Date::from_calendar_date(2026, Month::March, d).unwrap()
}

/// A category booking to `account`.
fn category(name: &str, account: &FinAccountId) -> NewExpenseCategory {
    NewExpenseCategory {
        name: name.to_owned(),
        account_id: account.clone(),
        default_vat_rate_bp: Some(1900),
    }
}

/// A €119.00 train ticket showing €19.00 of VAT at 19 %, paid personally.
fn ticket(category_id: Option<FinCategoryId>) -> NewExpense {
    NewExpense {
        category_id,
        merchant: "Bahn".to_owned(),
        description: "Berlin → München".to_owned(),
        vat_cents: 1900,
        vat_rate_bp: Some(1900),
        ..NewExpense::spent(day(14), 11_900, ExpenseMethod::Personal)
    }
}

// ---- categories -------------------------------------------------------------

#[tokio::test]
async fn fin_categories_are_tenant_wide_and_never_cross_tenant() {
    let store = common::test_store().await;
    let (a, t1, a_account) = tenant_with_chart(&store, "a").await;
    // A co-tenant of the same tenant: configuration is shared.
    let uc = store
        .for_tenant(t1.clone())
        .create_user("c@expenses.test")
        .await
        .unwrap();
    let c = store.for_account(t1.clone(), uc);
    let (b, _t2, b_account) = tenant_with_chart(&store, "b").await;

    // ---- the CRUD arc ----------------------------------------------------
    let travel = a
        .create_fin_category(&category("Reisekosten", &a_account))
        .await
        .unwrap();
    let listed = a.fin_categories(false).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "Reisekosten");
    assert_eq!(listed[0].account_id, a_account);
    assert_eq!(listed[0].default_vat_rate_bp, Some(1900));
    assert!(listed[0].active);
    // A co-tenant sees the same configuration.
    assert_eq!(c.fin_categories(false).await.unwrap().len(), 1);
    assert!(c.fin_category(&travel).await.unwrap().is_some());

    // A name that reads the same is the same word, whatever the case.
    assert_conflict(
        a.create_fin_category(&category("reisekosten", &a_account))
            .await,
        "already exists",
    );

    a.update_fin_category(
        &travel,
        &NewExpenseCategory {
            default_vat_rate_bp: None,
            ..category("Reisen", &a_account)
        },
    )
    .await
    .unwrap();
    let renamed = a.fin_category(&travel).await.unwrap().unwrap();
    assert_eq!(renamed.name, "Reisen");
    assert_eq!(renamed.default_vat_rate_bp, None);

    // Deactivating keeps it readable and drops it from the picker.
    a.set_fin_category_active(&travel, false).await.unwrap();
    assert!(a.fin_categories(false).await.unwrap().is_empty());
    assert_eq!(a.fin_categories(true).await.unwrap().len(), 1);
    a.set_fin_category_active(&travel, true).await.unwrap();

    // ---- an outsider tenant sees, and changes, nothing --------------------
    assert!(b.fin_categories(true).await.unwrap().is_empty());
    assert!(b.fin_category(&travel).await.unwrap().is_none());
    assert_not_found(
        b.update_fin_category(&travel, &category("Stolen", &b_account))
            .await,
    );
    assert_not_found(b.set_fin_category_active(&travel, false).await);
    assert_not_found(b.delete_fin_category(&travel).await);
    // …and cannot point a category of their own at tenant A's account: the
    // account reads as absent, exactly as it must.
    assert_not_found(b.create_fin_category(&category("Reisen", &a_account)).await);

    // A's category is byte-identical after every one of those attempts.
    let after = a.fin_category(&travel).await.unwrap().unwrap();
    assert_eq!(after.name, "Reisen");
    assert_eq!(after.account_id, a_account);
    assert!(after.active);

    // ---- a category books to an active expense account, and only that ----
    let revenue = a
        .fin_account_for_role(AccountRole::Revenue)
        .await
        .unwrap()
        .unwrap()
        .id;
    assert_invalid(
        a.create_fin_category(&category("Wrong side", &revenue))
            .await,
        "expense account",
    );
    let retired = a
        .create_fin_account(&NewAccount {
            code: "6410".to_owned(),
            name: "Alt".to_owned(),
            kind: AccountType::Expense,
            role: None,
        })
        .await
        .unwrap();
    a.set_fin_account_active(&retired, false).await.unwrap();
    assert_invalid(
        a.create_fin_category(&category("Retired", &retired)).await,
        "deactivated account",
    );
    assert_not_found(
        a.create_fin_category(&category("Nowhere", &FinAccountId::new("nope".to_owned())))
            .await,
    );

    // ---- deleting: only what nothing points at ---------------------------
    let spare = a
        .create_fin_category(&category("Software", &a_account))
        .await
        .unwrap();
    a.delete_fin_category(&spare).await.unwrap();
    assert!(a.fin_category(&spare).await.unwrap().is_none());
    assert_not_found(a.delete_fin_category(&spare).await);

    // An account a category books to is not deletable, and the refusal says so
    // rather than blaming postings that do not exist.
    let own = a
        .create_fin_account(&NewAccount {
            code: "6420".to_owned(),
            name: "Fortbildung".to_owned(),
            kind: AccountType::Expense,
            role: None,
        })
        .await
        .unwrap();
    let training = a
        .create_fin_category(&category("Fortbildung", &own))
        .await
        .unwrap();
    assert_conflict(a.delete_fin_account(&own).await, "expense category");
    a.delete_fin_category(&training).await.unwrap();
    a.delete_fin_account(&own).await.unwrap();
}

// ---- claims -----------------------------------------------------------------

#[tokio::test]
async fn an_expense_claim_is_reachable_only_by_the_person_who_made_it() {
    let store = common::test_store().await;
    let (a, t1, a_account) = tenant_with_chart(&store, "claim-a").await;
    // A colleague inside the SAME tenant — the case that separates this suite
    // from an ordinary tenancy test.
    let colleague_id = store
        .for_tenant(t1.clone())
        .create_user("colleague@expenses.test")
        .await
        .unwrap();
    let colleague = store.for_account(t1.clone(), colleague_id);
    let (b, _t2, _b_account) = tenant_with_chart(&store, "claim-b").await;

    let travel = a
        .create_fin_category(&category("Reisekosten", &a_account))
        .await
        .unwrap();
    let claim = a.log_expense(&ticket(Some(travel.clone()))).await.unwrap();

    // ---- what the claimant sees ------------------------------------------
    assert_eq!(claim.gross_cents, 11_900);
    assert_eq!(claim.vat_cents, 1900);
    assert_eq!(claim.net_cents(), 10_000, "net is derived, never stored");
    assert_eq!(claim.currency, "EUR");
    assert_eq!(claim.status, ExpenseStatus::Draft);
    assert_eq!(claim.method, ExpenseMethod::Personal);
    assert!(claim.method.owes_the_employee());
    assert!(claim.is_editable());
    assert!(claim.submitted_at.is_none() && claim.decided_at.is_none());
    assert_eq!(a.expense(&claim.id).await.unwrap().unwrap().id, claim.id);
    assert_eq!(
        a.expenses(day(1), day(31), None).await.unwrap().len(),
        1,
        "my claims"
    );
    assert!(
        a.expenses(day(1), day(31), Some(ExpenseStatus::Submitted))
            .await
            .unwrap()
            .is_empty(),
        "nothing is in anybody's queue yet"
    );

    // ---- what a colleague of the same tenant sees: nothing ---------------
    assert!(colleague.expense(&claim.id).await.unwrap().is_none());
    assert!(
        colleague
            .expenses(day(1), day(31), None)
            .await
            .unwrap()
            .is_empty()
    );
    assert_not_found(colleague.edit_expense(&claim.id, &ticket(None)).await);
    assert_not_found(colleague.delete_expense(&claim.id).await);

    // ---- what another tenant sees: the same nothing ----------------------
    assert!(b.expense(&claim.id).await.unwrap().is_none());
    assert!(b.expenses(day(1), day(31), None).await.unwrap().is_empty());
    assert_not_found(b.edit_expense(&claim.id, &ticket(None)).await);
    assert_not_found(b.delete_expense(&claim.id).await);
    // Nor can B classify a claim of their own with A's category.
    assert_not_found(b.log_expense(&ticket(Some(travel.clone()))).await);
    assert!(b.expenses(day(1), day(31), None).await.unwrap().is_empty());

    // A's claim is untouched after every one of those attempts.
    let after = a.expense(&claim.id).await.unwrap().unwrap();
    assert_eq!(after.gross_cents, 11_900);
    assert_eq!(after.category_id, Some(travel));
    assert_eq!(after.merchant, "Bahn");
    assert_eq!(after.updated_at, claim.updated_at);
}

#[tokio::test]
async fn the_claim_crud_arc_and_the_rules_that_freeze_it() {
    let store = common::test_store().await;
    let (a, _t1, a_account) = tenant_with_chart(&store, "arc").await;
    let travel = a
        .create_fin_category(&category("Reisekosten", &a_account))
        .await
        .unwrap();

    // ---- create, correct, list -------------------------------------------
    let claim = a.log_expense(&ticket(Some(travel.clone()))).await.unwrap();
    let corrected = a
        .edit_expense(
            &claim.id,
            &NewExpense {
                merchant: "Deutsche Bahn".to_owned(),
                gross_cents: 12_900,
                vat_cents: 2059,
                ..ticket(Some(travel.clone()))
            },
        )
        .await
        .unwrap();
    assert_eq!(corrected.merchant, "Deutsche Bahn");
    assert_eq!(corrected.gross_cents, 12_900);
    assert_eq!(corrected.net_cents(), 10_841);

    // A claim outside the window is not in it; the window's ends are included.
    let older = a
        .log_expense(&NewExpense {
            ..NewExpense::spent(day(2), 500, ExpenseMethod::Cash)
        })
        .await
        .unwrap();
    let window = a.expenses(day(2), day(14), None).await.unwrap();
    assert_eq!(window.len(), 2, "both ends are included");
    assert_eq!(window[0].id, corrected.id, "newest purchase first");
    assert!(a.expenses(day(3), day(13), None).await.unwrap().is_empty());
    assert_invalid(a.expenses(day(14), day(2), None).await, "before its start");

    // ---- the rules a claim refuses at the door ---------------------------
    assert_invalid(
        a.log_expense(&NewExpense {
            vat_rate_bp: None,
            ..ticket(None)
        })
        .await,
        "VAT rate",
    );
    assert_invalid(
        a.log_expense(&NewExpense {
            gross_cents: 100,
            vat_cents: 101,
            ..ticket(None)
        })
        .await,
        "exceed",
    );
    assert_not_found(
        a.log_expense(&NewExpense {
            receipt_node_id: Some(DriveNodeId::new("no-such-node".to_owned())),
            ..ticket(None)
        })
        .await,
    );

    // A category nobody may pick any more cannot be picked afresh — but the
    // claim that already carries it is untouched.
    a.set_fin_category_active(&travel, false).await.unwrap();
    assert_invalid(
        a.log_expense(&ticket(Some(travel.clone()))).await,
        "no longer offered",
    );
    assert_eq!(
        a.expense(&corrected.id)
            .await
            .unwrap()
            .unwrap()
            .category_id
            .as_ref(),
        Some(&travel),
        "a cost does not become uncategorised because a word was retired"
    );
    a.set_fin_category_active(&travel, true).await.unwrap();

    // A category that has classified a claim is history, not a preference.
    assert_conflict(a.delete_fin_category(&travel).await, "cannot be deleted");

    // ---- delete ----------------------------------------------------------
    a.delete_expense(&older.id).await.unwrap();
    assert!(a.expense(&older.id).await.unwrap().is_none());
    assert_not_found(a.delete_expense(&older.id).await);
    // An id that never existed reads exactly like one that has been removed.
    let never = FinExpenseId::new("no-such-claim".to_owned());
    assert!(a.expense(&never).await.unwrap().is_none());
    assert_not_found(a.delete_expense(&never).await);
}

#[tokio::test]
async fn a_tenant_with_claims_can_still_be_deleted() {
    // 0106's lesson, re-asserted for the two new foreign keys: both restrict at
    // the END of the statement (`NO ACTION`), so dropping a tenant whose
    // categories point into its own chart and whose claims point at those
    // categories succeeds whichever cascade Postgres runs first.
    let store = common::test_store().await;
    let (a, tenant, account) = tenant_with_chart(&store, "drop").await;
    let travel = a
        .create_fin_category(&category("Reisekosten", &account))
        .await
        .unwrap();
    a.log_expense(&ticket(Some(travel))).await.unwrap();
    store.delete_tenant(&tenant).await.unwrap();
    assert!(a.fin_categories(true).await.unwrap().is_empty());
    assert!(a.expenses(day(1), day(31), None).await.unwrap().is_empty());
}
