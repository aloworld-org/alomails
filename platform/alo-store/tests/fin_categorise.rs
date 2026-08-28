//! The agent's suggested category, against the real database (alo Finance,
//! ADR 0035, wave B4.14a): what a suggestion is, who can see and answer one,
//! and the three ways it stops existing.
//!
//! Two isolation questions, with the same answer the claims themselves have
//! (`fin_expenses_tenancy.rs`): a **colleague inside the same tenant** is as
//! blind to somebody's suggestions as another tenant entirely, because a claim
//! is personal data about one employee. Both read absent, never `Forbidden` —
//! which would confirm that somebody claimed something that day.
//!
//! And one rule that is this slice's own: **a suggestion is not a
//! classification**. Nothing this module writes reaches `category_id`, which is
//! the only column a posting rule, a report or a VAT return ever reads, until a
//! human accepts it.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountRole, AccountStore, CHART, ChartName, ChartSeed, ExpenseMethod, ExpenseStatus,
    FinAccountId, FinCategoryId, FinExpenseId, NewExpense, NewExpenseCategory,
    REASON_MERCHANT_HISTORY, SKIP_ALREADY_PROPOSED, SKIP_DECLINED, SKIP_NO_HISTORY,
    SKIP_NO_MERCHANT, Store, StoreError, TenantId,
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
        Err(StoreError::Conflict(msg)) => assert!(
            msg.contains(expect),
            "conflict {msg:?} should name {expect:?}"
        ),
        other => panic!("expected Conflict naming {expect:?}, got: {other:?}"),
    }
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

/// A tenant with one user and a seeded chart, returning the account door, the
/// tenant id and the tenant's `expense_default` account.
async fn tenant_with_chart(store: &Store, tag: &str) -> (AccountStore, TenantId, FinAccountId) {
    let tenant = store.create_tenant(&format!("cat-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@categorise.test"))
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

/// The whole March the tests work in.
fn march() -> (Date, Date) {
    (day(1), day(31))
}

fn category(name: &str, account: &FinAccountId) -> NewExpenseCategory {
    NewExpenseCategory {
        name: name.to_owned(),
        account_id: account.clone(),
        default_vat_rate_bp: None,
    }
}

/// A claim from `merchant` on the given day, classified or not.
fn claim(merchant: &str, on: u8, category_id: Option<FinCategoryId>) -> NewExpense {
    NewExpense {
        category_id,
        merchant: merchant.to_owned(),
        ..NewExpense::spent(day(on), 2_500, ExpenseMethod::Personal)
    }
}

/// The suggestion on one claim, as the claimant reads it back.
async fn suggestion(door: &AccountStore, id: &FinExpenseId) -> Option<FinCategoryId> {
    door.expense(id)
        .await
        .unwrap()
        .expect("the claim is the caller's own")
        .proposed_category_id
}

#[tokio::test]
async fn a_suggestion_comes_from_the_claimants_own_history_and_is_not_a_classification() {
    let store = common::test_store().await;
    let (a, _t1, account) = tenant_with_chart(&store, "a").await;
    let travel = a
        .create_fin_category(&category("Reisekosten", &account))
        .await
        .unwrap();

    // Two claims from the same payee this person has already classified…
    a.log_expense(&claim("Bahn", 2, Some(travel.clone())))
        .await
        .unwrap();
    a.log_expense(&claim("bahn  ", 5, Some(travel.clone())))
        .await
        .unwrap();
    // …one unclassified claim from that payee, one from a payee they have never
    // classified, and one with no payee at all.
    let waiting = a.log_expense(&claim("BAHN", 20, None)).await.unwrap().id;
    let stranger = a
        .log_expense(&claim("Neuer Laden", 21, None))
        .await
        .unwrap()
        .id;
    let nameless = a.log_expense(&claim("", 22, None)).await.unwrap().id;

    let (from, to) = march();
    let plan = a.propose_expense_categories(from, to).await.unwrap();

    assert_eq!(plan.proposed.len(), 1, "only the recognised payee");
    let proposal = &plan.proposed[0];
    assert_eq!(proposal.expense_id, waiting);
    assert_eq!(proposal.category_id, travel);
    assert_eq!(proposal.reason, REASON_MERCHANT_HISTORY);
    assert_eq!(proposal.evidence, 2, "the two claims that back it");

    let reasons: Vec<(FinExpenseId, &str)> = plan
        .skipped
        .iter()
        .map(|s| (s.expense_id.clone(), s.reason))
        .collect();
    assert!(reasons.contains(&(stranger.clone(), SKIP_NO_HISTORY)));
    assert!(reasons.contains(&(nameless.clone(), SKIP_NO_MERCHANT)));

    // The suggestion is written where nothing reads it, and the claim is still
    // unclassified: this is the rule the whole slice exists for.
    let claim_now = a.expense(&waiting).await.unwrap().unwrap();
    assert_eq!(claim_now.category_id, None, "a suggestion books nothing");
    assert_eq!(claim_now.proposed_category_id, Some(travel.clone()));
    assert_eq!(claim_now.proposed_reason, REASON_MERCHANT_HISTORY);
    assert!(claim_now.proposed_at.is_some());
    assert!(claim_now.proposal_declined_at.is_none());
    // Nothing was written on the two it could not answer for.
    assert_eq!(suggestion(&a, &stranger).await, None);
    assert_eq!(suggestion(&a, &nameless).await, None);

    // ---- asking twice suggests nothing twice ------------------------------
    let again = a.propose_expense_categories(from, to).await.unwrap();
    assert!(again.proposed.is_empty());
    assert!(
        again
            .skipped
            .iter()
            .any(|s| s.expense_id == waiting && s.reason == SKIP_ALREADY_PROPOSED)
    );

    // ---- accepting is the human's act, and it is the only thing that books -
    let accepted = a.accept_category_proposal(&waiting).await.unwrap();
    assert_eq!(accepted.category_id, Some(travel.clone()));
    assert_eq!(accepted.proposed_category_id, None);
    assert_eq!(accepted.proposed_reason, "");
    // Answered once: there is nothing left to accept.
    assert_conflict(
        a.accept_category_proposal(&waiting).await,
        "no suggested category",
    );
}

#[tokio::test]
async fn declining_survives_the_suggestion_it_was_about() {
    let store = common::test_store().await;
    let (a, _t, account) = tenant_with_chart(&store, "d").await;
    let meals = a
        .create_fin_category(&category("Bewirtung", &account))
        .await
        .unwrap();
    a.log_expense(&claim("Kantine", 2, Some(meals.clone())))
        .await
        .unwrap();
    let waiting = a.log_expense(&claim("Kantine", 20, None)).await.unwrap().id;
    let (from, to) = march();

    assert_eq!(
        a.propose_expense_categories(from, to)
            .await
            .unwrap()
            .proposed
            .len(),
        1
    );
    let declined = a.decline_category_proposal(&waiting).await.unwrap();
    assert_eq!(declined.proposed_category_id, None);
    assert_eq!(declined.proposed_reason, "");
    assert!(declined.proposal_declined_at.is_some());
    assert_eq!(declined.category_id, None, "declining classifies nothing");

    // The "no" outlives the suggestion: asking again offers nothing.
    let again = a.propose_expense_categories(from, to).await.unwrap();
    assert!(again.proposed.is_empty());
    assert_eq!(again.skipped[0].reason, SKIP_DECLINED);
    assert_eq!(suggestion(&a, &waiting).await, None);

    // …and there is nothing left to answer.
    assert_conflict(
        a.decline_category_proposal(&waiting).await,
        "no suggested category",
    );
    assert_conflict(
        a.accept_category_proposal(&waiting).await,
        "no suggested category",
    );

    // A human can still classify it by hand: saying no silences the machine,
    // not the person.
    let by_hand = a
        .edit_expense(&waiting, &claim("Kantine", 20, Some(meals.clone())))
        .await
        .unwrap();
    assert_eq!(by_hand.category_id, Some(meals));
}

#[tokio::test]
async fn a_claim_in_somebody_elses_queue_is_never_suggested_at_or_answered() {
    let store = common::test_store().await;
    let (a, _t, account) = tenant_with_chart(&store, "q").await;
    let travel = a
        .create_fin_category(&category("Reisekosten", &account))
        .await
        .unwrap();
    a.log_expense(&claim("Bahn", 2, Some(travel.clone())))
        .await
        .unwrap();
    let handed_in = a.log_expense(&claim("Bahn", 20, None)).await.unwrap().id;
    let (from, to) = march();

    // Suggested while it is still the claimant's own…
    a.propose_expense_categories(from, to).await.unwrap();
    assert_eq!(suggestion(&a, &handed_in).await, Some(travel.clone()));
    // …and frozen the moment somebody is deciding it.
    a.submit_expense(&handed_in).await.unwrap();
    assert_conflict(a.accept_category_proposal(&handed_in).await, "handed in");

    // A claim that is already in a queue is not looked at at all: a suggestion
    // on it would be an offer the accept verb then refuses.
    let second = a.log_expense(&claim("Bahn", 21, None)).await.unwrap().id;
    a.submit_expense(&second).await.unwrap();
    let plan = a.propose_expense_categories(from, to).await.unwrap();
    assert!(plan.proposed.iter().all(|p| p.expense_id != second));
    assert!(plan.skipped.iter().all(|s| s.expense_id != second));
    assert_eq!(suggestion(&a, &second).await, None);

    // Withdrawn, it is the claimant's again — and the suggestion survived being
    // handed in, so it does not have to be asked for twice.
    a.withdraw_expense(&handed_in).await.unwrap();
    assert_eq!(
        a.accept_category_proposal(&handed_in)
            .await
            .unwrap()
            .category_id,
        Some(travel)
    );
    assert_eq!(
        a.expense(&handed_in).await.unwrap().unwrap().status,
        ExpenseStatus::Draft
    );
}

#[tokio::test]
async fn a_retired_word_is_neither_suggested_nor_accepted() {
    let store = common::test_store().await;
    let (a, _t, account) = tenant_with_chart(&store, "r").await;
    let travel = a
        .create_fin_category(&category("Reisekosten", &account))
        .await
        .unwrap();
    a.log_expense(&claim("Bahn", 2, Some(travel.clone())))
        .await
        .unwrap();
    let waiting = a.log_expense(&claim("Bahn", 20, None)).await.unwrap().id;
    let (from, to) = march();

    // Suggested, then the tenant retires the word before the person answers.
    a.propose_expense_categories(from, to).await.unwrap();
    a.set_fin_category_active(&travel, false).await.unwrap();
    match a.accept_category_proposal(&waiting).await {
        Err(StoreError::Validation(msg)) => {
            assert!(msg.contains("no longer offered"), "{msg}");
        }
        other => panic!("expected the claim form's own refusal, got {other:?}"),
    }

    // And a retired word is not suggested afresh on another claim.
    let another = a.log_expense(&claim("Bahn", 22, None)).await.unwrap().id;
    let plan = a.propose_expense_categories(from, to).await.unwrap();
    assert!(plan.proposed.iter().all(|p| p.expense_id != another));
    assert!(
        plan.skipped
            .iter()
            .any(|s| s.expense_id == another && s.reason == SKIP_NO_HISTORY)
    );
}

#[tokio::test]
async fn an_edit_clears_the_suggestion_it_was_made_about() {
    let store = common::test_store().await;
    let (a, _t, account) = tenant_with_chart(&store, "e").await;
    let travel = a
        .create_fin_category(&category("Reisekosten", &account))
        .await
        .unwrap();
    a.log_expense(&claim("Bahn", 2, Some(travel.clone())))
        .await
        .unwrap();
    let waiting = a.log_expense(&claim("Bahn", 20, None)).await.unwrap().id;
    let (from, to) = march();
    a.propose_expense_categories(from, to).await.unwrap();

    // The claim becomes a different purchase; the suggestion was about the old
    // one.
    let edited = a
        .edit_expense(&waiting, &claim("Apotheke", 20, None))
        .await
        .unwrap();
    assert_eq!(edited.proposed_category_id, None);
    assert_eq!(edited.proposed_reason, "");
    assert!(edited.proposal_declined_at.is_none(), "an edit is not a no");
    assert_conflict(
        a.accept_category_proposal(&waiting).await,
        "no suggested category",
    );
}

#[tokio::test]
async fn nobody_else_sees_or_answers_a_suggestion_not_even_a_colleague() {
    let store = common::test_store().await;
    let (a, t1, account) = tenant_with_chart(&store, "x").await;
    let travel = a
        .create_fin_category(&category("Reisekosten", &account))
        .await
        .unwrap();
    a.log_expense(&claim("Bahn", 2, Some(travel.clone())))
        .await
        .unwrap();
    let waiting = a.log_expense(&claim("Bahn", 20, None)).await.unwrap().id;
    let (from, to) = march();
    a.propose_expense_categories(from, to).await.unwrap();

    // A colleague in the SAME tenant: the configuration is shared, the claims
    // are not.
    let colleague_user = store
        .for_tenant(t1.clone())
        .create_user("colleague@categorise.test")
        .await
        .unwrap();
    let colleague = store.for_account(t1.clone(), colleague_user);
    assert!(colleague.expense(&waiting).await.unwrap().is_none());
    assert_not_found(colleague.accept_category_proposal(&waiting).await);
    assert_not_found(colleague.decline_category_proposal(&waiting).await);
    // Their own run sees nothing of somebody else's, and writes nothing.
    let theirs = colleague
        .propose_expense_categories(from, to)
        .await
        .unwrap();
    assert!(theirs.proposed.is_empty() && theirs.skipped.is_empty());

    // Another tenant entirely: the same answers, for the same reason.
    let (b, _t2, _) = tenant_with_chart(&store, "y").await;
    assert_not_found(b.accept_category_proposal(&waiting).await);
    assert_not_found(b.decline_category_proposal(&waiting).await);
    assert!(
        b.propose_expense_categories(from, to)
            .await
            .unwrap()
            .proposed
            .is_empty()
    );

    // A's suggestion is untouched by every one of those attempts.
    let after = a.expense(&waiting).await.unwrap().unwrap();
    assert_eq!(after.proposed_category_id, Some(travel));
    assert_eq!(after.category_id, None);
    assert!(after.proposal_declined_at.is_none());

    // …and a claim that does not exist is absent rather than an internal error.
    assert_not_found(
        a.accept_category_proposal(&FinExpenseId::new("nope".to_owned()))
            .await,
    );
    assert_not_found(
        a.decline_category_proposal(&FinExpenseId::new("nope".to_owned()))
            .await,
    );
}

#[tokio::test]
async fn a_period_that_runs_backwards_is_refused() {
    let store = common::test_store().await;
    let (a, _t, _account) = tenant_with_chart(&store, "p").await;
    match a.propose_expense_categories(day(31), day(1)).await {
        Err(StoreError::Validation(msg)) => assert!(msg.contains("before its start"), "{msg}"),
        other => panic!("expected Validation, got {other:?}"),
    }
    // A period with nothing in it is an empty plan, not a failure.
    let empty = a.propose_expense_categories(day(1), day(31)).await.unwrap();
    assert!(empty.proposed.is_empty() && empty.skipped.is_empty());
}
