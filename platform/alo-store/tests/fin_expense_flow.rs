//! The life of an expense claim (alo Finance, ADR 0035, wave B4.05b): handed
//! in, decided, and — when the employee's own money paid — paid back.
//!
//! `fin_expenses_tenancy.rs` proves who may *see* a claim. This proves what may
//! *happen* to one, and the three rules that hold the flow together:
//!
//! - **The freeze.** A claim in somebody's queue cannot be changed or removed
//!   by its claimant. B4.05a wrote that rule and could not test it, because
//!   nothing could yet set a status other than `draft`; this is that test.
//! - **A rejection is the claimant's again.** Refusing a claim is only useful if
//!   the person can fix it and hand it in again, so a rejected claim is
//!   editable, deletable and submittable — and handing it in clears the refusal
//!   rather than leaving a decision on the record that no longer stands.
//! - **The approver's door is the tenant's, and only the tenant's.** Every
//!   cross-user statement binds `tenant_id`; tenant B's handle decides nothing
//!   of tenant A's, and gets the same clean absence a claim that never existed
//!   would give.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, Expense, ExpenseDecision, ExpenseMethod, ExpenseStatus, FinExpenseId, NewExpense,
    Store, StoreError, TenantId, TenantStore, UserId,
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

fn day(d: u8) -> Date {
    Date::from_calendar_date(2026, Month::March, d).unwrap()
}

/// A €119.00 train ticket showing €19.00 of VAT at 19 %, out of the traveller's
/// own pocket — the claim the whole flow is about.
fn ticket() -> NewExpense {
    NewExpense {
        merchant: "Bahn".to_owned(),
        description: "Berlin → München".to_owned(),
        vat_cents: 1900,
        vat_rate_bp: Some(1900),
        ..NewExpense::spent(day(14), 11_900, ExpenseMethod::Personal)
    }
}

/// One tenant with two people: the claimant's personal door, the approver's
/// tenant door, and the approver's own id (what `decided_by` must end up
/// holding).
struct Office {
    claimant: AccountStore,
    claimant_email: String,
    tenant: TenantStore,
    approver: UserId,
    tenant_id: TenantId,
}

async fn office(store: &Store, tag: &str) -> Office {
    let tenant_id = store.create_tenant(&format!("flow-{tag}")).await.unwrap();
    let tenant = store.for_tenant(tenant_id.clone());
    let claimant_email = format!("{tag}-claimant@expenses.test");
    let claimant_user = tenant.create_user(&claimant_email).await.unwrap();
    let approver = tenant
        .create_user(&format!("{tag}-approver@expenses.test"))
        .await
        .unwrap();
    Office {
        claimant: store.for_account(tenant_id.clone(), claimant_user),
        claimant_email,
        tenant,
        approver,
        tenant_id,
    }
}

/// The claim as the claimant's own door reads it back — the read every
/// assertion below is made against, so nothing is asserted about a value a
/// write happened to return.
async fn reread(door: &AccountStore, id: &FinExpenseId) -> Expense {
    door.expense(id)
        .await
        .unwrap()
        .expect("the claim is theirs")
}

#[tokio::test]
async fn a_claim_runs_from_a_draft_to_money_paid_back() {
    let store = common::test_store().await;
    let office = office(&store, "arc").await;
    let id = office.claimant.log_expense(&ticket()).await.unwrap().id;

    // ---- a draft is nobody's business but the claimant's -------------------
    let draft = reread(&office.claimant, &id).await;
    assert_eq!(draft.status, ExpenseStatus::Draft);
    assert!(draft.submitted_at.is_none());
    assert!(draft.is_editable());
    // …and it is in no queue.
    assert!(office.tenant.pending_expenses().await.unwrap().is_empty());
    // Deciding a claim nobody handed in is a conflict naming what it is.
    assert_conflict(
        office
            .tenant
            .decide_expense(&id, ExpenseDecision::Approve, &office.approver, "")
            .await,
        "this claim is draft",
    );

    // ---- handed in: it freezes, and appears in the queue -------------------
    let submitted = office.claimant.submit_expense(&id).await.unwrap();
    assert_eq!(submitted.status, ExpenseStatus::Submitted);
    assert!(submitted.submitted_at.is_some());
    assert_conflict(
        office
            .claimant
            .edit_expense(
                &id,
                &NewExpense {
                    gross_cents: 99_900,
                    ..ticket()
                },
            )
            .await,
        "withdraw it first",
    );
    assert_conflict(
        office.claimant.delete_expense(&id).await,
        "withdraw it first",
    );
    assert_conflict(office.claimant.submit_expense(&id).await, "submitted");
    // Nothing the refused writes attempted reached the row.
    let frozen = reread(&office.claimant, &id).await;
    assert_eq!(frozen.gross_cents, 11_900);
    assert_eq!(frozen.status, ExpenseStatus::Submitted);

    let queue = office.tenant.pending_expenses().await.unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].expense.id, id);
    assert_eq!(
        queue[0].user_email, office.claimant_email,
        "the inbox names the person, not an opaque id"
    );
    assert_eq!(queue[0].category_name, None, "nobody classified this one");

    // ---- taken back, corrected, handed in again ---------------------------
    let withdrawn = office.claimant.withdraw_expense(&id).await.unwrap();
    assert_eq!(withdrawn.status, ExpenseStatus::Draft);
    assert!(
        withdrawn.submitted_at.is_none(),
        "a claim in no queue was not handed in"
    );
    assert!(office.tenant.pending_expenses().await.unwrap().is_empty());
    assert_conflict(office.claimant.withdraw_expense(&id).await, "draft");
    office
        .claimant
        .edit_expense(
            &id,
            &NewExpense {
                description: "Berlin → München, Rückfahrt".to_owned(),
                ..ticket()
            },
        )
        .await
        .unwrap();
    office.claimant.submit_expense(&id).await.unwrap();

    // ---- refused: it is the claimant's again ------------------------------
    let rejected = office
        .tenant
        .decide_expense(
            &id,
            ExpenseDecision::Reject,
            &office.approver,
            "the receipt is missing",
        )
        .await
        .unwrap();
    assert_eq!(rejected.status, ExpenseStatus::Rejected);
    assert_eq!(rejected.decided_by.as_ref(), Some(&office.approver));
    assert!(rejected.decided_at.is_some());
    assert_eq!(rejected.decision_note, "the receipt is missing");
    assert!(
        rejected.is_editable(),
        "the point of a refusal is that it can be fixed"
    );
    assert!(office.tenant.pending_expenses().await.unwrap().is_empty());
    // A decided claim is not re-decided: the claimant hands it in again.
    assert_conflict(
        office
            .tenant
            .decide_expense(&id, ExpenseDecision::Approve, &office.approver, "")
            .await,
        "this claim is rejected",
    );

    office
        .claimant
        .edit_expense(
            &id,
            &NewExpense {
                description: "Berlin → München, Beleg nachgereicht".to_owned(),
                ..ticket()
            },
        )
        .await
        .unwrap();
    let resubmitted = office.claimant.submit_expense(&id).await.unwrap();
    assert_eq!(resubmitted.status, ExpenseStatus::Submitted);
    assert!(
        resubmitted.decided_by.is_none()
            && resubmitted.decided_at.is_none()
            && resubmitted.decision_note.is_empty(),
        "a decision that no longer stands must not still be on the record"
    );

    // ---- approved: the company owes the money -----------------------------
    let approved = office
        .tenant
        .decide_expense(
            &id,
            ExpenseDecision::Approve,
            &office.approver,
            "Beleg vollständig",
        )
        .await
        .unwrap();
    assert_eq!(approved.status, ExpenseStatus::Approved);
    assert_eq!(approved.decided_by.as_ref(), Some(&office.approver));
    assert!(approved.method.owes_the_employee());
    assert_eq!(approved.net_cents(), 10_000);
    // An approved claim is nobody's to unmake through this door.
    assert_conflict(office.claimant.withdraw_expense(&id).await, "approved");
    assert_conflict(
        office.claimant.delete_expense(&id).await,
        "withdraw it first",
    );
    assert_conflict(office.claimant.submit_expense(&id).await, "approved");
    assert_conflict(
        office
            .claimant
            .edit_expense(&id, &NewExpense { ..ticket() })
            .await,
        "withdraw it first",
    );

    // ---- paid back: the end of the line -----------------------------------
    let reimbursed = office.tenant.reimburse_expense(&id, day(31)).await.unwrap();
    assert_eq!(reimbursed.status, ExpenseStatus::Reimbursed);
    assert_eq!(reimbursed.reimbursed_on, Some(day(31)));
    assert_eq!(
        reimbursed.decision_note, "Beleg vollständig",
        "paying the money back does not erase what the approver said"
    );
    assert_conflict(
        office.tenant.reimburse_expense(&id, day(31)).await,
        "this claim is reimbursed",
    );
    assert_conflict(
        office.claimant.delete_expense(&id).await,
        "withdraw it first",
    );

    // The claimant's own list still holds exactly this one claim, and the
    // status filter finds it where it now is and nowhere else.
    let mine = office
        .claimant
        .expenses(day(1), day(31), None)
        .await
        .unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].status, ExpenseStatus::Reimbursed);
    assert!(
        office
            .claimant
            .expenses(day(1), day(31), Some(ExpenseStatus::Draft))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn only_money_the_employee_paid_is_ever_reimbursed() {
    let store = common::test_store().await;
    let office = office(&store, "card").await;
    let id = office
        .claimant
        .log_expense(&NewExpense {
            merchant: "Hetzner".to_owned(),
            ..NewExpense::spent(day(9), 2_499, ExpenseMethod::Card)
        })
        .await
        .unwrap()
        .id;
    office.claimant.submit_expense(&id).await.unwrap();

    // Before approval there is nothing to pay back either, and the refusal says
    // which of the two rules stopped it.
    assert_conflict(
        office.tenant.reimburse_expense(&id, day(31)).await,
        "this claim is submitted",
    );
    office
        .tenant
        .decide_expense(&id, ExpenseDecision::Approve, &office.approver, "")
        .await
        .unwrap();
    assert_conflict(
        office.tenant.reimburse_expense(&id, day(31)).await,
        "nobody to reimburse",
    );
    // …and the claim is exactly as the approval left it.
    let after = reread(&office.claimant, &id).await;
    assert_eq!(after.status, ExpenseStatus::Approved);
    assert_eq!(after.reimbursed_on, None);
}

#[tokio::test]
async fn the_queue_is_this_tenants_and_nobody_decides_across_the_line() {
    let store = common::test_store().await;
    let a = office(&store, "own").await;
    let b = office(&store, "other").await;
    let id = a.claimant.log_expense(&ticket()).await.unwrap().id;
    a.claimant.submit_expense(&id).await.unwrap();

    // Tenant B's own queue is empty, and A's claim is absent from every read
    // and every statement B's handle can make about it.
    assert!(b.tenant.pending_expenses().await.unwrap().is_empty());
    assert!(b.tenant.reimbursable_expenses().await.unwrap().is_empty());
    assert!(b.tenant.expense_by_id(&id).await.unwrap().is_none());
    assert_not_found(
        b.tenant
            .decide_expense(&id, ExpenseDecision::Approve, &b.approver, "mine now")
            .await,
    );
    assert_not_found(
        b.tenant
            .decide_expense(&id, ExpenseDecision::Reject, &b.approver, "")
            .await,
    );
    assert_not_found(b.tenant.reimburse_expense(&id, day(31)).await);
    // The claimant's own doors are equally closed to the other tenant's user.
    let outsider = store.for_account(b.tenant_id.clone(), a.approver.clone());
    assert_not_found(outsider.submit_expense(&id).await);
    assert_not_found(outsider.withdraw_expense(&id).await);

    // A's claim is byte-identical after all of that, and still waiting.
    let untouched = a.tenant.expense_by_id(&id).await.unwrap().unwrap();
    assert_eq!(untouched.status, ExpenseStatus::Submitted);
    assert!(untouched.decided_by.is_none());
    assert_eq!(a.tenant.pending_expenses().await.unwrap().len(), 1);

    // A colleague inside A's own tenant is as blind as the outsider: the
    // approver's *personal* door holds nothing of the claimant's.
    let colleague = store.for_account(a.tenant_id.clone(), a.approver.clone());
    assert!(colleague.expense(&id).await.unwrap().is_none());
    assert_not_found(colleague.submit_expense(&id).await);
    assert_not_found(colleague.withdraw_expense(&id).await);
    assert_not_found(colleague.delete_expense(&id).await);

    // Once A approves it, A owes A's employee — and B's payer queue is still
    // empty, so a debt never appears on another company's books.
    a.tenant
        .decide_expense(&id, ExpenseDecision::Approve, &a.approver, "")
        .await
        .unwrap();
    assert_eq!(a.tenant.reimbursable_expenses().await.unwrap().len(), 1);
    assert!(b.tenant.reimbursable_expenses().await.unwrap().is_empty());
    assert_not_found(b.tenant.reimburse_expense(&id, day(31)).await);
}

#[tokio::test]
async fn the_inbox_holds_the_waiting_claims_of_everybody_oldest_purchase_first() {
    let store = common::test_store().await;
    let office = office(&store, "queue").await;
    // A second person in the same tenant, with their own claim.
    let other_user = office
        .tenant
        .create_user("queue-second@expenses.test")
        .await
        .unwrap();
    let other = store.for_account(office.tenant_id.clone(), other_user);

    let older = other
        .log_expense(&NewExpense::spent(day(2), 500, ExpenseMethod::Cash))
        .await
        .unwrap()
        .id;
    let newer = office.claimant.log_expense(&ticket()).await.unwrap().id;
    let never = office
        .claimant
        .log_expense(&NewExpense::spent(day(3), 700, ExpenseMethod::Personal))
        .await
        .unwrap()
        .id;
    other.submit_expense(&older).await.unwrap();
    office.claimant.submit_expense(&newer).await.unwrap();

    let queue = office.tenant.pending_expenses().await.unwrap();
    let ids: Vec<&str> = queue.iter().map(|p| p.expense.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![older.as_str(), newer.as_str()],
        "a queue is worked oldest purchase first, and a draft is in no queue"
    );
    assert!(
        !ids.contains(&never.as_str()),
        "a claim nobody handed in is nobody's to decide"
    );
    assert_eq!(queue[0].user_email, "queue-second@expenses.test");
    assert_eq!(queue[1].user_email, office.claimant_email);
    assert_eq!(queue[1].expense.merchant, "Bahn");
    assert_eq!(queue[1].expense.gross_cents, 11_900);
}

#[tokio::test]
async fn the_payers_queue_holds_only_what_the_company_still_owes_a_person() {
    let store = common::test_store().await;
    let office = office(&store, "owed").await;
    let colleague_user = office
        .tenant
        .create_user("owed-second@expenses.test")
        .await
        .unwrap();
    let colleague = store.for_account(office.tenant_id.clone(), colleague_user);

    // Four claims: one out of a pocket, one on the company card, one still
    // waiting for a decision, and one already paid back.
    let owed = office.claimant.log_expense(&ticket()).await.unwrap().id;
    let on_the_card = colleague
        .log_expense(&NewExpense::spent(day(4), 2_499, ExpenseMethod::Card))
        .await
        .unwrap()
        .id;
    let waiting = colleague
        .log_expense(&NewExpense::spent(day(5), 800, ExpenseMethod::Personal))
        .await
        .unwrap()
        .id;
    let settled = office
        .claimant
        .log_expense(&NewExpense::spent(day(6), 1_250, ExpenseMethod::Personal))
        .await
        .unwrap()
        .id;
    for id in [&settled, &owed, &on_the_card, &waiting] {
        let door = if id == &on_the_card || id == &waiting {
            &colleague
        } else {
            &office.claimant
        };
        door.submit_expense(id).await.unwrap();
    }
    // Decided in this order, so "oldest decision first" is a claim about the
    // decision and not about the purchase date, which runs the other way.
    for id in [&settled, &owed, &on_the_card] {
        office
            .tenant
            .decide_expense(id, ExpenseDecision::Approve, &office.approver, "")
            .await
            .unwrap();
    }
    office
        .tenant
        .reimburse_expense(&settled, day(30))
        .await
        .unwrap();

    let payable = office.tenant.reimbursable_expenses().await.unwrap();
    let ids: Vec<&str> = payable.iter().map(|p| p.expense.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![owed.as_str()],
        "only an approved claim the employee's own money paid is still owed"
    );
    assert!(!ids.contains(&on_the_card.as_str()), "the card owes nobody");
    assert!(!ids.contains(&waiting.as_str()), "nobody has approved it");
    assert!(!ids.contains(&settled.as_str()), "already paid back");
    assert_eq!(
        payable[0].user_email, office.claimant_email,
        "the payer's queue names the person who is owed the money"
    );
    assert_eq!(payable[0].expense.gross_cents, 11_900);

    // Paying it clears the queue, which is the only way a line leaves it.
    office
        .tenant
        .reimburse_expense(&owed, day(31))
        .await
        .unwrap();
    assert!(
        office
            .tenant
            .reimbursable_expenses()
            .await
            .unwrap()
            .is_empty()
    );
}
