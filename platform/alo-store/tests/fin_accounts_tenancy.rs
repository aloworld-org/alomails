//! Tenancy proof for the alo Finance chart of accounts (Law 1: isolation is
//! tested, not assumed), plus the CRUD arc and the two rules that make the
//! chart trustworthy: the default seed runs **once per tenant, ever**, and the
//! by-role lookup every posting rule will make answers with that tenant's own
//! account and nobody else's.
//!
//! The chart is tenant-wide — a co-tenant user reads the same accounts — but an
//! outsider tenant gets the clean `NotFound`/empty on **every** path: read,
//! list, by-role, update, deactivate and delete.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    Account, AccountRole, AccountStore, AccountType, CHART, CHART_SEED_KEY, ChartName, ChartSeed,
    FinAccountId, NewAccount, Store, StoreError, TenantId,
};

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

/// The seed as the HTTP edge will hand it in: one name per default account, in
/// the language of whoever opened the chart. `tag` makes each tenant's names
/// distinguishable, which is how a leak would show itself.
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

/// A tenant with one user, returning the account door plus the tenant id.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("fin-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@finance.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

fn by_code<'a>(accounts: &'a [Account], code: &str) -> &'a Account {
    accounts
        .iter()
        .find(|account| account.code == code)
        .unwrap_or_else(|| panic!("no {code} account in the chart"))
}

#[tokio::test]
async fn fin_accounts_seed_once_and_never_cross_tenant() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "a").await;
    // A co-tenant user of the same tenant: the chart is tenant-wide.
    let uc = store
        .for_tenant(t1.clone())
        .create_user("c@finance.test")
        .await
        .unwrap();
    let c = store.for_account(t1.clone(), uc);
    let (b, t2) = tenant_with_user(&store, "b").await;

    // ---- first read seeds a working chart --------------------------------
    assert!(!a.fin_seed_ran(CHART_SEED_KEY).await.unwrap());
    let chart = a.fin_accounts_or_seed(&seed("A"), false).await.unwrap();
    assert_eq!(chart.len(), CHART.len());
    assert!(a.fin_seed_ran(CHART_SEED_KEY).await.unwrap());
    // In code order, all active, all system, named in the caller's words.
    let codes: Vec<&str> = chart.iter().map(|x| x.code.as_str()).collect();
    let mut sorted = codes.clone();
    sorted.sort_unstable();
    assert_eq!(codes, sorted, "the chart reads in code order");
    assert!(chart.iter().all(|x| x.active && x.system));
    assert_eq!(by_code(&chart, "1100").name, "A 1100");
    assert_eq!(by_code(&chart, "1100").kind, AccountType::Asset);
    assert_eq!(by_code(&chart, "1100").role, Some(AccountRole::Ar));

    // Every posting rule's role resolves from the first document.
    for role in AccountRole::ALL {
        let found = a.fin_account_for_role(*role).await.unwrap();
        assert!(found.is_some(), "no account for {}", role.as_str());
    }

    // ---- seeding is a first-use rule, not an every-read one --------------
    let again = a.fin_accounts_or_seed(&seed("A"), false).await.unwrap();
    assert_eq!(again.len(), CHART.len(), "a second read seeds nothing");
    // A co-tenant reading first would have seeded the same chart; they see it.
    assert_eq!(
        c.fin_accounts(false).await.unwrap().len(),
        CHART.len(),
        "the chart is tenant-wide"
    );
    // A tenant who throws a seeded account away is not handed it again the
    // next morning: the ledger's question is whether the seed ever RAN, not
    // whether its accounts are still there.
    let spare = by_code(&chart, "4900").id.clone();
    a.set_fin_account_active(&spare, false).await.unwrap();
    let after = a.fin_accounts_or_seed(&seed("A"), false).await.unwrap();
    assert_eq!(after.len(), CHART.len() - 1, "the seed does not come back");

    // ---- the other tenant has nothing at all -----------------------------
    assert!(b.fin_accounts(true).await.unwrap().is_empty());
    assert!(!b.fin_seed_ran(CHART_SEED_KEY).await.unwrap());
    let receivables = by_code(&chart, "1100").id.clone();
    assert!(b.fin_account(&receivables).await.unwrap().is_none());
    assert!(
        b.fin_account_for_role(AccountRole::Ar)
            .await
            .unwrap()
            .is_none()
    );
    assert_not_found(
        b.update_fin_account(
            &receivables,
            &NewAccount {
                code: "1100".to_owned(),
                name: "Mine now".to_owned(),
                kind: AccountType::Asset,
                role: Some(AccountRole::Ar),
            },
        )
        .await,
    );
    assert_not_found(b.set_fin_account_active(&receivables, false).await);
    assert_not_found(b.delete_fin_account(&receivables).await);
    // Nothing the outsider did touched the owner's row.
    let still = a.fin_account(&receivables).await.unwrap().unwrap();
    assert_eq!(still.name, "A 1100");
    assert!(still.active);

    // ---- and B's own seed is B's own, with B's words ---------------------
    let bs = b.fin_accounts_or_seed(&seed("B"), false).await.unwrap();
    assert_eq!(by_code(&bs, "1100").name, "B 1100");
    assert_eq!(
        b.fin_account_for_role(AccountRole::Ar)
            .await
            .unwrap()
            .unwrap()
            .name,
        "B 1100",
        "the by-role lookup answers with the caller's own tenant"
    );
    assert_eq!(a.fin_accounts(false).await.unwrap().len(), CHART.len() - 1);

    // ---- tenant deletion purges the chart and its seed ledger ------------
    store.delete_tenant(&t2).await.unwrap();
    assert!(b.fin_accounts(true).await.unwrap().is_empty());
    assert!(!b.fin_seed_ran(CHART_SEED_KEY).await.unwrap());
    assert_eq!(
        a.fin_accounts(false).await.unwrap().len(),
        CHART.len() - 1,
        "deleting one tenant leaves the other's chart untouched"
    );
    let _ = t2;
}

#[tokio::test]
async fn fin_accounts_crud_and_the_chart_rules() {
    let store = common::test_store().await;
    let (a, _t1) = tenant_with_user(&store, "crud").await;
    let (b, _t2) = tenant_with_user(&store, "crud-other").await;
    a.fin_accounts_or_seed(&seed("A"), false).await.unwrap();

    // ---- create a custom account -----------------------------------------
    let id = a
        .create_fin_account(&NewAccount {
            code: "  6410  ".to_owned(),
            name: "  Software subscriptions  ".to_owned(),
            kind: AccountType::Expense,
            role: None,
        })
        .await
        .unwrap();
    let got = a.fin_account(&id).await.unwrap().unwrap();
    assert_eq!(got.code, "6410", "the code is trimmed and uppercased");
    assert_eq!(got.name, "Software subscriptions");
    assert_eq!(got.kind, AccountType::Expense);
    assert_eq!(got.role, None);
    assert!(got.active);
    assert!(!got.system, "a tenant's own account is never a system one");

    // ---- the code is the accountant's key, and it is unique --------------
    assert_conflict(
        a.create_fin_account(&NewAccount {
            code: "6410".to_owned(),
            name: "Another one".to_owned(),
            kind: AccountType::Expense,
            role: None,
        })
        .await,
        "code",
    );
    // Uniqueness is per tenant: the same code in another tenant is fine.
    b.create_fin_account(&NewAccount {
        code: "6410".to_owned(),
        name: "Theirs".to_owned(),
        kind: AccountType::Expense,
        role: None,
    })
    .await
    .unwrap();

    // ---- a role is held by exactly one account ---------------------------
    assert_conflict(
        a.create_fin_account(&NewAccount {
            code: "1101".to_owned(),
            name: "A second receivables account".to_owned(),
            kind: AccountType::Asset,
            role: Some(AccountRole::Ar),
        })
        .await,
        "role",
    );

    // ---- update: a tenant may renumber their chart -----------------------
    let receivables = a
        .fin_account_for_role(AccountRole::Ar)
        .await
        .unwrap()
        .unwrap();
    a.update_fin_account(
        &receivables.id,
        &NewAccount {
            code: "1400".to_owned(),
            name: "Forderungen aus Lieferungen und Leistungen".to_owned(),
            kind: AccountType::Asset,
            role: Some(AccountRole::Ar),
        },
    )
    .await
    .unwrap();
    let moved = a
        .fin_account_for_role(AccountRole::Ar)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(moved.code, "1400");
    assert_eq!(moved.id, receivables.id, "renumbering keeps the account");
    assert!(
        moved.system,
        "a seeded account stays a system account after a rename"
    );

    // Invalid input is a validation error, not a write.
    match a
        .update_fin_account(
            &id,
            &NewAccount {
                code: String::new(),
                name: "No code".to_owned(),
                kind: AccountType::Expense,
                role: None,
            },
        )
        .await
    {
        Err(StoreError::Validation(msg)) => assert!(msg.contains("account code"), "{msg}"),
        other => panic!("expected Validation, got {other:?}"),
    }
    assert_eq!(a.fin_account(&id).await.unwrap().unwrap().code, "6410");

    // ---- deactivate: the account keeps its history, loses its role ------
    let bank = a
        .fin_account_for_role(AccountRole::Bank)
        .await
        .unwrap()
        .unwrap();
    a.set_fin_account_active(&bank.id, false).await.unwrap();
    assert!(
        a.fin_account_for_role(AccountRole::Bank)
            .await
            .unwrap()
            .is_none(),
        "a deactivated account is not an answer a posting rule may use"
    );
    let listed = a.fin_accounts(false).await.unwrap();
    assert!(!listed.iter().any(|x| x.id == bank.id));
    let all = a.fin_accounts(true).await.unwrap();
    assert!(all.iter().any(|x| x.id == bank.id && !x.active));
    assert_eq!(
        all.last().map(|x| x.id.clone()),
        Some(bank.id.clone()),
        "inactive accounts sort after the active ones"
    );
    // Idempotent, and reversible.
    a.set_fin_account_active(&bank.id, false).await.unwrap();
    a.set_fin_account_active(&bank.id, true).await.unwrap();
    assert!(
        a.fin_account_for_role(AccountRole::Bank)
            .await
            .unwrap()
            .is_some()
    );

    // ---- delete: a custom account may go, a system one may not ----------
    assert_conflict(a.delete_fin_account(&bank.id).await, "system account");
    a.delete_fin_account(&id).await.unwrap();
    assert!(a.fin_account(&id).await.unwrap().is_none());
    assert_not_found(a.delete_fin_account(&id).await);
    assert_not_found(a.delete_fin_account(&FinAccountId::generate()).await);
    // The other tenant's identically-coded account survived all of it.
    assert_eq!(b.fin_accounts(false).await.unwrap().len(), 1);
}
