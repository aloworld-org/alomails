//! **Reconciliation, the heuristic stage** (alo Finance, ADR 0035, wave B4.09b)
//! — the lines nobody quoted a number on, ranked against a real ledger.
//!
//! `src/bank_match_heuristic.rs` states the ranking as arithmetic and proves it
//! without a database; `src/fin_match_rules.rs` proves what a rule reads. This
//! suite asserts the five things a pure test cannot:
//!
//! - a payer who quotes **nothing** is recognised by their name and the amount
//!   they owe, with the evidence a bookkeeper reads on the screen;
//! - a **part payment** that quotes our number is offered with what would be
//!   left, and the exact stage stays silent about it;
//! - a **rule a person saved** recognises the payer whose bank spells them
//!   differently — the case the fold deliberately does not guess at — and the
//!   rule's hits are counted where a confirmation happens, not where a screen
//!   refreshes;
//! - a rule is **written once, read back folded and deleted** by the person who
//!   owns it, and refuses a second one that looks for the same thing;
//! - **tenancy**: another tenant's rule is invisible, unusable and undeletable,
//!   their customers cannot be pointed at, their lines cannot be learned from,
//!   and two tenants holding the same ledger never rank each other's documents
//!   (Law 1).
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::bank_read::BankImportRequest;
use alo_store::{
    AccountStore, BankCsvDates, BankCsvDecimal, BankCsvMapping, BankLineId, BankSource,
    BillingCustomerId, BillingInvoiceId, FinMatchRuleId, MatchEvidence, MatchOn, NewCustomer,
    NewInvoice, NewLine, NewMatchRule, NewPayment, Store, StoreError,
};
use time::Date;

/// The account every statement in this suite is of — the IBAN specification's
/// own test number, never a real one.
const ACCOUNT: &str = "DE02120300000000202051";

/// The payer's account on the lines below; a second specification test number.
const PAYER_IBAN: &str = "DE89370400440532013000";

/// €1 307.00, the same two-rate document the exact stage's suite uses.
const GROSS_CENTS: i64 = 130_700;

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

fn refused<T: std::fmt::Debug>(result: Result<T, StoreError>, expect: &str) {
    match result {
        Err(StoreError::Validation(message) | StoreError::Conflict(message)) => assert!(
            message.contains(expect),
            "refusal {message:?} should name {expect:?}"
        ),
        other => panic!("expected a refusal naming {expect:?}, got {other:?}"),
    }
}

async fn tenant(store: &Store, tag: &str) -> AccountStore {
    let tenant = store
        .create_tenant(&format!("suggest-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@suggesting.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    common::seed_default_chart(&account).await;
    account
}

async fn customer(account: &AccountStore, name: &str) -> BillingCustomerId {
    account
        .create_billing_customer(&NewCustomer {
            name: name.to_owned(),
            country: "DE".to_owned(),
            currency: "EUR".to_owned(),
            payment_terms_days: 30,
            ..Default::default()
        })
        .await
        .unwrap()
}

fn lines() -> Vec<NewLine> {
    [
        (10_000_i64, 10_000_i64, 2100_i32),
        (4_000, 5_000, 900),
        (-1_000, 10_000, 2100),
    ]
    .iter()
    .enumerate()
    .map(
        |(index, &(qty_milli, unit_price_cents, vat_rate_bp))| NewLine {
            description: format!("Line {index}"),
            unit: "hour".to_owned(),
            qty_milli,
            unit_price_cents,
            vat_rate_bp,
        },
    )
    .collect()
}

/// An issued invoice worth [`GROSS_CENTS`] for `customer`, with its number and
/// the day it was issued.
async fn issued_invoice(
    account: &AccountStore,
    customer: &BillingCustomerId,
) -> (BillingInvoiceId, String, Date) {
    let id = account
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    account
        .set_billing_invoice_lines(&id, &lines())
        .await
        .unwrap();
    let document = account.issue_billing_invoice(&id).await.unwrap();
    assert_eq!(document.totals.gross_cents, GROSS_CENTS);
    (
        id,
        document.invoice.number.clone().expect("an issued number"),
        document.invoice.issue_date.expect("an issue date"),
    )
}

/// A CSV statement of `rows` — `(booked on, signed cents, counterparty, what the
/// payer wrote)` — in the shape a bank portal exports.
fn statement(rows: &[(Date, i64, String, String)]) -> Vec<u8> {
    let mut csv = String::from("date,amount,counterparty,iban,description,reference\n");
    for (index, (booked_on, cents, counterparty, remittance)) in rows.iter().enumerate() {
        let sign = if *cents < 0 { "-" } else { "" };
        csv.push_str(&format!(
            "{booked_on},{sign}{}.{:02},{counterparty},{PAYER_IBAN},{remittance},REF{index}\n",
            cents.abs() / 100,
            cents.abs() % 100
        ));
    }
    csv.into_bytes()
}

fn csv_request() -> BankImportRequest {
    BankImportRequest {
        source: Some(BankSource::Csv),
        account_iban: ACCOUNT.to_owned(),
        currency: Some("EUR".to_owned()),
        dates: BankCsvDates::Ymd,
        decimal: BankCsvDecimal::Dot,
        mapping: BankCsvMapping {
            booked_on: Some("date".to_owned()),
            amount: Some("amount".to_owned()),
            counterparty_name: Some("counterparty".to_owned()),
            counterparty_iban: Some("iban".to_owned()),
            remittance: Some("description".to_owned()),
            bank_ref: Some("reference".to_owned()),
            ..Default::default()
        },
    }
}

/// Imports `rows` as a statement and answers the staged lines, in file order.
async fn stage(account: &AccountStore, rows: &[(Date, i64, String, String)]) -> Vec<BankLineId> {
    let report = account
        .import_bank_file(&csv_request(), &statement(rows))
        .await
        .unwrap();
    let imported = report.imported.expect("the file imports");
    assert_eq!(imported.staged, rows.len(), "every row stages");
    let mut lines = account
        .bank_lines(Some(&imported.statement.id), None)
        .await
        .unwrap();
    lines.sort_by_key(|line| line.line_no);
    lines.into_iter().map(|line| line.id).collect()
}

/// The suggestions for the line staged at `at`, in file order.
async fn suggestions_for(account: &AccountStore, lines: &[BankLineId], at: usize) -> LineAnswer {
    let all = account.bank_match_suggestions(None).await.unwrap();
    assert!(!all.numbers_capped);
    assert!(!all.ledger_capped, "these ledgers are three documents long");
    let found = all
        .lines
        .into_iter()
        .find(|entry| entry.line.id == lines[at])
        .expect("the staged line is unmatched and therefore listed");
    LineAnswer {
        exact: found.exact.len(),
        likely: found
            .likely
            .into_iter()
            .map(|offered| Offer {
                number: offered.number,
                amount_cents: offered.amount_cents,
                evidence: offered.evidence,
                rule_id: offered.rule_id,
            })
            .collect(),
    }
}

/// What one line was offered: how many documents the exact stage claimed, and
/// what the heuristic one ranked.
struct LineAnswer {
    exact: usize,
    likely: Vec<Offer>,
}

/// One ranked document, flattened to what the assertions read.
#[derive(Debug)]
struct Offer {
    number: String,
    amount_cents: i64,
    evidence: Vec<MatchEvidence>,
    rule_id: Option<FinMatchRuleId>,
}

#[tokio::test]
async fn a_payer_who_quotes_nothing_is_recognised_by_their_name_and_what_they_owe() {
    let store = common::test_store().await;
    let acc = tenant(&store, "named").await;
    let payer = customer(&acc, "Kaffeehaus Bergmann GmbH").await;
    let (invoice, number, issued_on) = issued_invoice(&acc, &payer).await;
    // A second document of somebody else, for exactly the same money: the
    // uniqueness argument must not be what carries this one.
    let other = customer(&acc, "Bäckerei Nord").await;
    let (_, _, _) = issued_invoice(&acc, &other).await;

    let lines = stage(
        &acc,
        &[(
            issued_on,
            GROSS_CENTS,
            "KAFFEEHAUS BERGMANN GMBH".to_owned(),
            "SEPA-Ueberweisung".to_owned(),
        )],
    )
    .await;

    let answer = suggestions_for(&acc, &lines, 0).await;
    assert_eq!(answer.exact, 0, "the payer quoted no number");
    assert_eq!(answer.likely.len(), 1, "and only one payer is named");
    let offered = &answer.likely[0];
    assert_eq!(offered.number, number);
    assert_eq!(offered.amount_cents, GROSS_CENTS);
    assert_eq!(offered.rule_id, None, "no rule was needed");
    let evidence = &offered.evidence;
    assert!(evidence.contains(&MatchEvidence::WholeAmount));
    assert!(
        evidence.iter().any(|why| matches!(
            why,
            MatchEvidence::CustomerNamed { similarity_bp } if similarity_bp == &10_000
        )),
        "{evidence:?}"
    );
    // Nothing was confirmed by any of this: the invoice is still owed in full.
    let document = acc.billing_invoice(&invoice).await.unwrap().unwrap();
    assert_eq!(document.paid_cents, 0);
    assert_eq!(document.settlement().outstanding_cents, GROSS_CENTS);
}

#[tokio::test]
async fn a_part_payment_quoting_the_number_is_offered_with_what_would_be_left() {
    let store = common::test_store().await;
    let acc = tenant(&store, "part").await;
    let payer = customer(&acc, "Kaffeehaus Bergmann GmbH").await;
    let (_, number, issued_on) = issued_invoice(&acc, &payer).await;

    let lines = stage(
        &acc,
        &[(
            issued_on,
            50_000,
            "KAFFEEHAUS BERGMANN GMBH".to_owned(),
            format!("Abschlag {number}"),
        )],
    )
    .await;

    let answer = suggestions_for(&acc, &lines, 0).await;
    assert_eq!(answer.exact, 0, "half of what is owed is not exact");
    assert_eq!(answer.likely.len(), 1);
    let offered = &answer.likely[0];
    assert_eq!(offered.number, number);
    assert_eq!(
        offered.amount_cents, 50_000,
        "the line is what it is; nothing is invented"
    );
    let evidence = &offered.evidence;
    assert!(evidence.contains(&MatchEvidence::NumberQuoted));
    assert!(evidence.contains(&MatchEvidence::PartPayment {
        remaining_cents: GROSS_CENTS - 50_000
    }));
}

#[tokio::test]
async fn money_already_received_moves_what_the_rest_of_the_document_is_matched_against() {
    let store = common::test_store().await;
    let acc = tenant(&store, "rest").await;
    let payer = customer(&acc, "Kaffeehaus Bergmann GmbH").await;
    let (invoice, number, issued_on) = issued_invoice(&acc, &payer).await;
    acc.record_billing_payment(
        &invoice,
        &NewPayment {
            paid_on: Some(issued_on),
            amount_cents: 30_000,
            method: "bank transfer".to_owned(),
            reference: "deposit".to_owned(),
        },
    )
    .await
    .unwrap();

    // The rest of it, quoted by number: that is exact, so the heuristic stage
    // says nothing at all.
    let rest = GROSS_CENTS - 30_000;
    let lines = stage(
        &acc,
        &[
            (
                issued_on,
                rest,
                "KAFFEEHAUS BERGMANN GMBH".to_owned(),
                format!("Rest {number}"),
            ),
            (
                issued_on,
                GROSS_CENTS,
                "KAFFEEHAUS BERGMANN GMBH".to_owned(),
                format!("Rechnung {number}"),
            ),
        ],
    )
    .await;

    let settling = suggestions_for(&acc, &lines, 0).await;
    assert_eq!(settling.exact, 1, "the remainder settles it exactly");
    assert!(settling.likely.is_empty(), "and is not also a guess");

    // The gross, on the other hand, is now MORE than the document owes: never
    // offered for it, in either stage.
    let too_much = suggestions_for(&acc, &lines, 1).await;
    assert_eq!(too_much.exact, 0);
    assert!(too_much.likely.is_empty(), "{:?}", too_much.likely);
}

#[tokio::test]
async fn a_rule_a_person_saved_recognises_the_payer_a_name_alone_cannot() {
    let store = common::test_store().await;
    let acc = tenant(&store, "rule").await;
    // The transliteration the fold deliberately does not undo.
    let payer = customer(&acc, "Müller Bau").await;
    let (_, number, issued_on) = issued_invoice(&acc, &payer).await;
    let lines = stage(
        &acc,
        &[(
            issued_on,
            50_000,
            "MUELLER BAU GMBH".to_owned(),
            "Abschlagszahlung".to_owned(),
        )],
    )
    .await;

    // Without a rule: a part payment, no number, a name the bank spelled its own
    // way. Nothing identifies the document, so nothing is offered.
    let before = suggestions_for(&acc, &lines, 0).await;
    assert_eq!(before.exact, 0);
    assert!(before.likely.is_empty());

    // The person says "this payer is Müller Bau", on the line in front of them.
    let saved = acc
        .learn_fin_match_rule(&lines[0], MatchOn::Counterparty, &payer)
        .await
        .unwrap();
    assert_eq!(
        saved.pattern, "mueller bau gmbh",
        "stored as it is compared"
    );
    assert_eq!(saved.customer_id, payer);
    assert_eq!(saved.hits, 0);

    let after = suggestions_for(&acc, &lines, 0).await;
    assert_eq!(after.likely.len(), 1);
    let offered = &after.likely[0];
    assert_eq!(offered.number, number);
    assert_eq!(offered.rule_id.as_ref(), Some(&saved.id));
    let evidence = &offered.evidence;
    assert!(evidence.contains(&MatchEvidence::RuleSaved {
        rule_id: saved.id.clone(),
        match_on: MatchOn::Counterparty,
    }));

    // A hit is counted where a confirmation happens, never by a read.
    let listed = acc.fin_match_rules().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].hits, 0, "reading the screen is not a hit");
    acc.fin_match_rule_hit(&saved.id).await.unwrap();
    assert_eq!(acc.fin_match_rules().await.unwrap()[0].hits, 1);

    // Forgetting it takes the suggestion with it.
    acc.delete_fin_match_rule(&saved.id).await.unwrap();
    assert!(acc.fin_match_rules().await.unwrap().is_empty());
    assert!(suggestions_for(&acc, &lines, 0).await.likely.is_empty());
    assert_not_found(acc.delete_fin_match_rule(&saved.id).await);
}

#[tokio::test]
async fn an_iban_rule_is_learned_from_the_line_and_a_remittance_one_is_not() {
    let store = common::test_store().await;
    let acc = tenant(&store, "iban").await;
    let payer = customer(&acc, "Müller Bau").await;
    let (_, _, issued_on) = issued_invoice(&acc, &payer).await;
    let lines = stage(
        &acc,
        &[(
            issued_on,
            50_000,
            "MUELLER BAU GMBH".to_owned(),
            "Abschlagszahlung".to_owned(),
        )],
    )
    .await;

    let saved = acc
        .learn_fin_match_rule(&lines[0], MatchOn::Iban, &payer)
        .await
        .unwrap();
    assert_eq!(saved.pattern, PAYER_IBAN.to_lowercase());
    assert_eq!(suggestions_for(&acc, &lines, 0).await.likely.len(), 1);

    // What the payer wrote on one transfer names that transfer.
    refused(
        acc.learn_fin_match_rule(&lines[0], MatchOn::Remittance, &payer)
            .await,
        "type the part of it",
    );
    // The same account, saved twice, is one rule.
    refused(
        acc.create_fin_match_rule(&NewMatchRule {
            match_on: MatchOn::Iban,
            pattern: PAYER_IBAN.to_owned(),
            customer_id: payer.clone(),
        })
        .await,
        "already looks for that",
    );
    // A pattern that identifies nothing is refused before it is written.
    refused(
        acc.create_fin_match_rule(&NewMatchRule {
            match_on: MatchOn::Counterparty,
            pattern: "  x ".to_owned(),
            customer_id: payer.clone(),
        })
        .await,
        "at least",
    );
    refused(
        acc.create_fin_match_rule(&NewMatchRule {
            match_on: MatchOn::Iban,
            pattern: "DE00 0000 0000 0000 0000 00".to_owned(),
            customer_id: payer,
        })
        .await,
        "not an IBAN",
    );
    assert_eq!(acc.fin_match_rules().await.unwrap().len(), 1);
}

#[tokio::test]
async fn two_tenants_holding_the_same_ledger_never_rank_or_reach_each_others_rules() {
    let store = common::test_store().await;
    let ours = tenant(&store, "ours").await;
    let theirs = tenant(&store, "theirs").await;

    // The same payer name, the same amount, the same statement — twice over.
    let our_payer = customer(&ours, "Kaffeehaus Bergmann GmbH").await;
    let their_payer = customer(&theirs, "Kaffeehaus Bergmann GmbH").await;
    let (_, our_number, issued_on) = issued_invoice(&ours, &our_payer).await;
    let (_, their_number, _) = issued_invoice(&theirs, &their_payer).await;
    assert_eq!(
        our_number, their_number,
        "the sequence is per tenant, so both hold the same number"
    );

    let rows = [(
        issued_on,
        GROSS_CENTS,
        "KAFFEEHAUS BERGMANN GMBH".to_owned(),
        "SEPA-Ueberweisung".to_owned(),
    )];
    let our_lines = stage(&ours, &rows).await;
    let their_lines = stage(&theirs, &rows).await;

    let our_rule = ours
        .learn_fin_match_rule(&our_lines[0], MatchOn::Iban, &our_payer)
        .await
        .unwrap();

    // Their rules are their own: ours is not in their list, not theirs to
    // count against, and not theirs to delete.
    assert!(theirs.fin_match_rules().await.unwrap().is_empty());
    assert_not_found(theirs.fin_match_rule_hit(&our_rule.id).await);
    assert_not_found(theirs.delete_fin_match_rule(&our_rule.id).await);
    // Nor can they point a rule of their own at our customer, or learn one from
    // our line: both read as absent, exactly as an id that never existed does.
    assert_not_found(
        theirs
            .create_fin_match_rule(&NewMatchRule {
                match_on: MatchOn::Counterparty,
                pattern: "kaffeehaus".to_owned(),
                customer_id: our_payer.clone(),
            })
            .await,
    );
    assert_not_found(
        theirs
            .learn_fin_match_rule(&our_lines[0], MatchOn::Counterparty, &their_payer)
            .await,
    );

    // And each sees exactly one suggestion, which is their own document.
    for (who, lines, mine) in [
        (&ours, &our_lines, &our_payer),
        (&theirs, &their_lines, &their_payer),
    ] {
        let answer = suggestions_for(who, lines, 0).await;
        assert_eq!(answer.likely.len(), 1, "one suggestion, not two");
        let document = who
            .billing_invoice_id_by_number(&answer.likely[0].number)
            .await
            .unwrap()
            .expect("their own document");
        let held = who.billing_invoice(&document).await.unwrap().unwrap();
        assert_eq!(&held.invoice.customer_id, mine, "and it is their customer");
    }
}
