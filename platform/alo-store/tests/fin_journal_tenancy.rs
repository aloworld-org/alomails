//! Tenancy proof for the alo Finance journal (Law 1: isolation is tested, not
//! assumed), plus the three rules that make the ledger trustworthy: an entry is
//! **written whole or not at all**, it **balances in both currency columns**,
//! and a document event **posts exactly once**.
//!
//! The wrong-tenant test is the one that matters most here, because a journal
//! is read by aggregates rather than by id: an outsider must get nothing from
//! the entry read, nothing from the postings read, nothing from the range read,
//! nothing from the idempotency lookup and nothing from the health query — and
//! must not be able to *write* a posting onto the owner's account either, which
//! is the leak a foreign-key-only defence would allow.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    Account, AccountRole, AccountStore, CHART, ChartName, ChartSeed, EntryKind, EntrySource,
    FinAccountId, FinEntryId, FxSnapshot, NewEntry, NewPosting, SourceEvent, SourceKind, Store,
    StoreError, TenantId,
};
use time::{Date, Month};

/// Asserts a result is a typed validation refusal whose message names the rule.
fn assert_invalid<T: std::fmt::Debug>(result: Result<T, StoreError>, expect: &str) {
    match result {
        Err(StoreError::Validation(msg)) => {
            assert!(
                msg.contains(expect),
                "message {msg:?} should name {expect:?}"
            );
        }
        other => panic!("expected Validation naming {expect:?}, got: {other:?}"),
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

fn day(day: u8) -> Date {
    Date::from_calendar_date(2026, Month::March, day).expect("a real March day")
}

/// The chart seed as the HTTP edge hands it in, tagged per tenant so a leak
/// would show itself in a name.
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

/// A tenant with one user and a seeded chart.
async fn tenant_with_chart(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store
        .create_tenant(&format!("journal-{tag}"))
        .await
        .unwrap();
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
        .unwrap_or_else(|| panic!("no account for {}", role.as_str()))
}

/// €121.00 invoiced at 21 %: the smallest true entry, and the one B4.04a will
/// later produce from a real invoice.
async fn invoice_entry(account: &AccountStore, source_id: &str) -> NewEntry {
    let ar = role(account, AccountRole::Ar).await;
    let revenue = role(account, AccountRole::Revenue).await;
    let vat = role(account, AccountRole::VatOutput).await;
    NewEntry {
        entry_date: day(4),
        kind: EntryKind::Invoice,
        source: Some(EntrySource {
            kind: SourceKind::Invoice,
            id: source_id.to_owned(),
            event: SourceEvent::Issue,
        }),
        memo: "INV-2026-00001".to_owned(),
        reverses_entry_id: None,
        attachment_node_id: None,
        currency: "EUR".to_owned(),
        fx: FxSnapshot::identity("EUR", day(4)),
        postings: vec![
            NewPosting {
                customer_id: Some("cust-1".to_owned()),
                memo: "Trade receivable".to_owned(),
                ..NewPosting::new(ar.id.clone(), 12_100, 12_100)
            },
            NewPosting {
                project_id: Some("proj-1".to_owned()),
                ..NewPosting::new(revenue.id.clone(), -10_000, -10_000)
            },
            NewPosting {
                vat_rate_bp: Some(2100),
                ..NewPosting::new(vat.id.clone(), -2_100, -2_100)
            },
        ],
    }
}

#[tokio::test]
async fn fin_journal_posts_whole_entries_and_never_crosses_a_tenant() {
    let store = common::test_store().await;
    let (a, _t1) = tenant_with_chart(&store, "a").await;
    let (b, t2) = tenant_with_chart(&store, "b").await;

    // ---- an entry is written whole, and reads back exactly as posted -----
    let id = a
        .post_fin_entry(&invoice_entry(&a, "inv-1").await)
        .await
        .unwrap();
    let read = a.fin_journal_entry(&id).await.unwrap().unwrap();
    assert_eq!(read.entry.entry_date, day(4));
    assert_eq!(read.entry.kind, EntryKind::Invoice);
    assert_eq!(read.entry.memo, "INV-2026-00001");
    assert_eq!(read.entry.currency, "EUR");
    assert_eq!(read.entry.fx.rate_micro, 1_000_000);
    assert!(!read.entry.created_by.is_empty(), "an entry has a hand");
    let source = read.entry.source.clone().expect("the document event");
    assert_eq!(source.kind, SourceKind::Invoice);
    assert_eq!(source.id, "inv-1");
    assert_eq!(source.event, SourceEvent::Issue);

    assert_eq!(read.postings.len(), 3);
    let positions: Vec<i32> = read.postings.iter().map(|p| p.position).collect();
    assert_eq!(positions, vec![0, 1, 2], "the order the rule wrote them in");
    assert_eq!(read.postings[0].amount_cents, 12_100);
    assert_eq!(read.postings[0].debit_cents(), 12_100);
    assert_eq!(read.postings[0].credit_cents(), 0);
    assert_eq!(read.postings[0].customer_id.as_deref(), Some("cust-1"));
    assert_eq!(read.postings[1].credit_cents(), 10_000);
    assert_eq!(read.postings[1].project_id.as_deref(), Some("proj-1"));
    assert_eq!(read.postings[2].vat_rate_bp, Some(2100));
    let sum: i64 = read.postings.iter().map(|p| p.amount_cents).sum();
    assert_eq!(sum, 0, "what was written balances");
    assert!(a.fin_unbalanced_entries().await.unwrap().is_empty());

    // ---- the range read, and the idempotency lookup ----------------------
    let listed = a
        .fin_entries(Some(day(1)), Some(day(31)), 50)
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, id);
    assert!(
        a.fin_entries(Some(day(5)), None, 50)
            .await
            .unwrap()
            .is_empty(),
        "the range is on the accounting date, not on when it was typed"
    );
    let found = a
        .fin_entry_for_source(&EntrySource {
            kind: SourceKind::Invoice,
            id: "inv-1".to_owned(),
            event: SourceEvent::Issue,
        })
        .await
        .unwrap();
    assert_eq!(found, Some(id.clone()));

    // ---- the other tenant sees nothing and can write nothing -------------
    assert!(b.fin_entry(&id).await.unwrap().is_none());
    assert!(b.fin_journal_entry(&id).await.unwrap().is_none());
    assert!(
        b.fin_entry_postings(&id).await.unwrap().is_empty(),
        "a posting is never readable without its header"
    );
    assert!(b.fin_entries(None, None, 50).await.unwrap().is_empty());
    assert!(b.fin_unbalanced_entries().await.unwrap().is_empty());
    assert!(
        b.fin_entry_for_source(&EntrySource {
            kind: SourceKind::Invoice,
            id: "inv-1".to_owned(),
            event: SourceEvent::Issue,
        })
        .await
        .unwrap()
        .is_none(),
        "the idempotency lookup is per tenant: B may post their own inv-1"
    );

    // The write direction: B posting onto A's accounts is refused, and B's own
    // identical source id is a different document.
    let a_ar = role(&a, AccountRole::Ar).await;
    let a_revenue = role(&a, AccountRole::Revenue).await;
    let mut trespass = invoice_entry(&b, "inv-1").await;
    trespass.postings[0].account_id = a_ar.id.clone();
    trespass.postings[1].account_id = a_revenue.id.clone();
    assert_invalid(b.post_fin_entry(&trespass).await, "not in this chart");
    assert_eq!(
        a.fin_entry_postings(&id).await.unwrap().len(),
        3,
        "nothing of the outsider's reached the owner's entry"
    );
    // B's own identical document id posts fine, into B's own books.
    let b_id = b
        .post_fin_entry(&invoice_entry(&b, "inv-1").await)
        .await
        .unwrap();
    assert_ne!(b_id, id);
    assert!(a.fin_entry(&b_id).await.unwrap().is_none());
    assert_eq!(a.fin_entries(None, None, 50).await.unwrap().len(), 1);

    // A reversal may only correct one of the caller's own entries.
    let mut steal = invoice_entry(&b, "reversal-of-a").await;
    steal.kind = EntryKind::Reversal;
    steal.reverses_entry_id = Some(id.clone());
    match b.post_fin_entry(&steal).await {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }

    // ---- deleting a tenant takes its journal and leaves the other's ------
    store.delete_tenant(&t2).await.unwrap();
    assert!(b.fin_entries(None, None, 50).await.unwrap().is_empty());
    assert!(b.fin_entry(&b_id).await.unwrap().is_none());
    assert_eq!(a.fin_entries(None, None, 50).await.unwrap().len(), 1);
    assert_eq!(a.fin_entry_postings(&id).await.unwrap().len(), 3);
}

#[tokio::test]
async fn fin_journal_refuses_what_would_make_the_books_wrong() {
    let store = common::test_store().await;
    let (a, _t) = tenant_with_chart(&store, "rules").await;
    let bank = role(&a, AccountRole::Bank).await;
    let fx_diff = role(&a, AccountRole::FxDiff).await;

    // ---- an unbalanced entry writes NOTHING -----------------------------
    let mut short = invoice_entry(&a, "inv-short").await;
    short.postings[2].amount_cents = -2_099;
    short.postings[2].base_cents = -2_099;
    assert_invalid(a.post_fin_entry(&short).await, "does not balance");
    assert!(
        a.fin_entries(None, None, 50).await.unwrap().is_empty(),
        "a refused entry leaves no header behind"
    );
    assert!(
        a.fin_entry_for_source(&EntrySource {
            kind: SourceKind::Invoice,
            id: "inv-short".to_owned(),
            event: SourceEvent::Issue,
        })
        .await
        .unwrap()
        .is_none()
    );

    // ---- a document event posts exactly once ----------------------------
    let id = a
        .post_fin_entry(&invoice_entry(&a, "inv-1").await)
        .await
        .unwrap();
    assert_conflict(
        a.post_fin_entry(&invoice_entry(&a, "inv-1").await).await,
        "already posted",
    );
    assert_eq!(
        a.fin_entries(None, None, 50).await.unwrap().len(),
        1,
        "the retry added no second set of postings"
    );
    assert_eq!(a.fin_entry_postings(&id).await.unwrap().len(), 3);
    // A different event of the same document is a different entry: voiding is
    // representable without weakening the key.
    let mut void = invoice_entry(&a, "inv-1").await;
    void.kind = EntryKind::Reversal;
    void.source = Some(EntrySource {
        kind: SourceKind::Invoice,
        id: "inv-1".to_owned(),
        event: SourceEvent::Void,
    });
    void.entry_date = day(6);
    void.reverses_entry_id = Some(id.clone());
    for posting in &mut void.postings {
        posting.amount_cents = -posting.amount_cents;
        posting.base_cents = -posting.base_cents;
    }
    let void_id = a.post_fin_entry(&void).await.unwrap();
    let stored = a.fin_entry(&void_id).await.unwrap().unwrap();
    assert_eq!(stored.reverses_entry_id, Some(id.clone()));
    // The ledger of the original and its reversal sums to zero, per account.
    let mut both = a.fin_entry_postings(&id).await.unwrap();
    both.extend(a.fin_entry_postings(&void_id).await.unwrap());
    for account in [
        AccountRole::Ar,
        AccountRole::Revenue,
        AccountRole::VatOutput,
    ] {
        let account_id = role(&a, account).await.id;
        let sum: i64 = both
            .iter()
            .filter(|p| p.account_id == account_id)
            .map(|p| p.amount_cents)
            .sum();
        assert_eq!(sum, 0, "{} does not net out", account.as_str());
    }

    // A reversal may not predate what it corrects.
    let mut early = invoice_entry(&a, "inv-2").await;
    early.kind = EntryKind::Reversal;
    early.entry_date = day(3);
    early.reverses_entry_id = Some(id.clone());
    assert_invalid(a.post_fin_entry(&early).await, "dated before");
    // And it may not point at an entry that does not exist at all.
    let mut nowhere = invoice_entry(&a, "inv-3").await;
    nowhere.reverses_entry_id = Some(FinEntryId::generate());
    match a.post_fin_entry(&nowhere).await {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }

    // ---- a deactivated account refuses new postings ---------------------
    a.set_fin_account_active(&bank.id, false).await.unwrap();
    let mut to_dead = invoice_entry(&a, "inv-4").await;
    to_dead.postings[0].account_id = bank.id.clone();
    let refusal = match a.post_fin_entry(&to_dead).await {
        Err(StoreError::Validation(msg)) => msg,
        other => panic!("expected Validation, got {other:?}"),
    };
    assert!(refusal.contains("deactivated"), "{refusal}");
    assert!(
        refusal.contains(&bank.code),
        "the refusal names the account: {refusal}"
    );
    a.set_fin_account_active(&bank.id, true).await.unwrap();

    // ---- an account that carries postings cannot be deleted -------------
    // This is B4.02's written-and-waiting guard, biting for the first time:
    // the chart is history, not a preference.
    let spare = a
        .fin_accounts(false)
        .await
        .unwrap()
        .into_iter()
        .find(|account| account.role.is_none())
        .expect("the default chart has ordinary accounts");
    let mut manual = invoice_entry(&a, "manual-1").await;
    manual.kind = EntryKind::Manual;
    manual.source = None;
    manual.postings = vec![
        NewPosting::new(spare.id.clone(), 5_000, 5_000),
        NewPosting::new(fx_diff.id.clone(), -5_000, -5_000),
    ];
    let manual_id = a.post_fin_entry(&manual).await.unwrap();
    let stored = a.fin_entry(&manual_id).await.unwrap().unwrap();
    assert!(stored.source.is_none(), "a manual entry answers to nothing");
    // A system account refuses first; the custom copy of the same situation is
    // what proves the foreign key, so make one.
    let custom = a
        .create_fin_account(&alo_store::NewAccount {
            code: "6410".to_owned(),
            name: "Software subscriptions".to_owned(),
            kind: alo_store::AccountType::Expense,
            role: None,
        })
        .await
        .unwrap();
    let mut used = invoice_entry(&a, "manual-2").await;
    used.kind = EntryKind::Manual;
    used.source = None;
    used.postings = vec![
        NewPosting::new(custom.clone(), 2_500, 2_500),
        NewPosting::new(fx_diff.id.clone(), -2_500, -2_500),
    ];
    a.post_fin_entry(&used).await.unwrap();
    assert_conflict(a.delete_fin_account(&custom).await, "carries postings");
    assert!(
        a.fin_account(&custom).await.unwrap().is_some(),
        "the refused delete left the account standing"
    );

    // ---- a posting to an account nobody has ------------------------------
    let mut ghost = invoice_entry(&a, "inv-5").await;
    ghost.postings[0].account_id = FinAccountId::generate();
    assert_invalid(a.post_fin_entry(&ghost).await, "not in this chart");

    // ---- the health query stays empty through all of it ------------------
    assert!(a.fin_unbalanced_entries().await.unwrap().is_empty());
}

#[tokio::test]
async fn fin_journal_balances_a_foreign_currency_entry_in_both_columns() {
    let store = common::test_store().await;
    let (a, _t) = tenant_with_chart(&store, "fx").await;
    let ar = role(&a, AccountRole::Ar).await;
    let bank = role(&a, AccountRole::Bank).await;
    let fx_diff = role(&a, AccountRole::FxDiff).await;

    // A $121.00 invoice settled later: the dollars move exactly, the euro do
    // not, and the difference is a posting of its own rather than a cent
    // smuggled into the bank line.
    let settle = NewEntry {
        entry_date: day(20),
        kind: EntryKind::Payment,
        source: Some(EntrySource {
            kind: SourceKind::Payment,
            id: "pay-1".to_owned(),
            event: SourceEvent::Settle,
        }),
        memo: "USD settlement".to_owned(),
        reverses_entry_id: None,
        attachment_node_id: None,
        currency: "USD".to_owned(),
        fx: FxSnapshot {
            base_currency: "EUR".to_owned(),
            rate_micro: 1_100_000,
            rate_date: day(19),
        },
        postings: vec![
            NewPosting::new(bank.id.clone(), 12_100, 11_000),
            NewPosting {
                customer_id: Some("cust-1".to_owned()),
                ..NewPosting::new(ar.id.clone(), -12_100, -11_001)
            },
            // Zero in the document column, a cent in the base column: the
            // exchange difference, which is the one posting allowed to move no
            // money in the currency the document is written in.
            NewPosting::new(fx_diff.id.clone(), 0, 1),
        ],
    };
    let id = a.post_fin_entry(&settle).await.unwrap();
    let read = a.fin_journal_entry(&id).await.unwrap().unwrap();
    assert_eq!(read.entry.currency, "USD");
    assert_eq!(read.entry.fx.base_currency, "EUR");
    assert_eq!(read.entry.fx.rate_micro, 1_100_000);
    assert_eq!(read.entry.fx.rate_date, day(19));
    let document: i64 = read.postings.iter().map(|p| p.amount_cents).sum();
    let base: i64 = read.postings.iter().map(|p| p.base_cents).sum();
    assert_eq!(document, 0, "the dollars balance");
    assert_eq!(base, 0, "and so do the euro");
    assert!(a.fin_unbalanced_entries().await.unwrap().is_empty());

    // The same entry with the difference left out balances in dollars and not
    // in euro — the failure an eyeball misses, and the reason both columns are
    // checked.
    let mut lopsided = settle.clone();
    lopsided.source = Some(EntrySource {
        kind: SourceKind::Payment,
        id: "pay-2".to_owned(),
        event: SourceEvent::Settle,
    });
    lopsided.postings.pop();
    assert_invalid(a.post_fin_entry(&lopsided).await, "accounting currency");

    // And an entry claiming the accounting currency at a rate other than the
    // identity is refused before it can restate anything.
    let mut wrong_rate = settle.clone();
    wrong_rate.source = Some(EntrySource {
        kind: SourceKind::Payment,
        id: "pay-3".to_owned(),
        event: SourceEvent::Settle,
    });
    wrong_rate.currency = "EUR".to_owned();
    assert_invalid(a.post_fin_entry(&wrong_rate).await, "identity");
}
