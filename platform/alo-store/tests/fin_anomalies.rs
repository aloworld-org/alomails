//! **Scanning a period of the journal for what is worth a second look** (alo
//! Finance, ADR 0035, wave B4.14b) — the store half of the agent's
//! `flag_anomalies`, over entries that reached Postgres through the real
//! posting path.
//!
//! `src/fin_anomalies.rs` proves the three rules against hand-built rows. This
//! suite proves the four things a pure test cannot.
//!
//! - **The rules survive the round trip.** A duplicate pair, a monthly cost with
//!   a hole in it and an amount unlike its account's are all found in books that
//!   were written, read back and re-assembled from two queries.
//! - **The evidence is the tenant's own entries.** Every finding cites entry ids
//!   that came back from `post_fin_entry`, so a person can open what they are
//!   being asked about.
//! - **Wrong tenant, the aggregate form (the design note's third isolation
//!   test).** A second tenant's much larger and much dirtier year leaves the
//!   first tenant's scan **byte-identical**, and neither tenant's findings ever
//!   name the other's entries or suppliers. A single-row read test cannot catch
//!   a scan that forgot its `tenant_id`; comparing a whole answer before and
//!   after can.
//! - **A period is a real boundary**, and one that ends before it starts is a
//!   typed refusal rather than an empty answer.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    ANOMALY_DUPLICATE, ANOMALY_MISSING_RECURRING, ANOMALY_UNUSUAL_AMOUNT, AccountStore, Anomaly,
    AnomalyScan, CHART, ChartName, ChartSeed, EntryKind, FinAccountId, FinEntryId, FxSnapshot,
    NewEntry, NewPosting, PARTY_SUPPLIER, Store, StoreError,
};
use time::{Date, Month};

fn on(month: Month, day: u8) -> Date {
    Date::from_calendar_date(2026, month, day).unwrap()
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

async fn tenant_with_chart(store: &Store, tag: &str) -> AccountStore {
    let tenant = store.create_tenant(&format!("anom-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@anomalies.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    account
        .fin_accounts_or_seed(&seed(tag), false)
        .await
        .unwrap();
    account
}

async fn id_of(account: &AccountStore, code: &str) -> FinAccountId {
    account
        .fin_accounts(false)
        .await
        .unwrap()
        .into_iter()
        .find(|entry| entry.code == code)
        .unwrap_or_else(|| panic!("the seeded chart holds {code}"))
        .id
}

/// One bill-shaped entry: a cost against a named supplier, and the payable it
/// created. Balanced, as every entry this store accepts must be.
async fn bill(
    account: &AccountStore,
    day: Date,
    supplier: &str,
    cents: i64,
    memo: &str,
) -> FinEntryId {
    let expense = id_of(account, "6000").await;
    let payable = id_of(account, "2000").await;
    account
        .post_fin_entry(&NewEntry {
            entry_date: day,
            kind: EntryKind::Bill,
            source: None,
            memo: memo.to_owned(),
            reverses_entry_id: None,
            attachment_node_id: None,
            currency: "EUR".to_owned(),
            fx: FxSnapshot::identity("EUR", day),
            postings: vec![
                NewPosting {
                    supplier_key: Some(supplier.to_owned()),
                    ..NewPosting::new(expense, cents, cents)
                },
                NewPosting::new(payable, -cents, -cents),
            ],
        })
        .await
        .unwrap()
}

/// **The seeded books**, scaled by `times` and tagged by `who` so a second
/// tenant's are unmistakably not the first's:
///
/// - a monthly rent to one supplier in January, February, April and May —
///   March is the hole;
/// - the same €300 bill from another supplier entered twice in the same week;
/// - one €7 000 bill, far outside what this account otherwise moves.
async fn seeded_books(account: &AccountStore, who: &str, times: i64) -> Vec<FinEntryId> {
    let landlord = format!("{who} Vastgoed");
    let shop = format!("{who} Supplies");
    let mut ids = Vec::new();
    for (month, day) in [
        (Month::January, 5),
        (Month::February, 5),
        (Month::April, 5),
        (Month::May, 5),
    ] {
        ids.push(bill(account, on(month, day), &landlord, 120_000 * times, "rent").await);
    }
    ids.push(bill(account, on(Month::March, 2), &shop, 30_000 * times, "paper").await);
    ids.push(bill(account, on(Month::March, 5), &shop, 30_000 * times, "paper").await);
    ids.push(
        bill(
            account,
            on(Month::March, 12),
            &format!("{who} Rare"),
            700_000 * times,
            "a one-off",
        )
        .await,
    );
    ids
}

async fn scan(account: &AccountStore) -> AnomalyScan {
    account
        .fin_anomalies(on(Month::January, 1), on(Month::June, 30))
        .await
        .unwrap()
}

fn of_kind<'a>(scan: &'a AnomalyScan, kind: &str) -> Vec<&'a Anomaly> {
    scan.findings.iter().filter(|f| f.kind == kind).collect()
}

#[tokio::test]
async fn a_seeded_period_reports_the_three_rules_with_the_entries_behind_them() {
    let store = common::test_store().await;
    let account = tenant_with_chart(&store, "solo").await;
    let ids = seeded_books(&account, "Solo", 1).await;
    let expense = id_of(&account, "6000").await;
    let found = scan(&account).await;

    assert_eq!(found.scanned, 7, "every entry of the period was read");
    assert!(!found.truncated);
    // Every entry names its supplier on the cost side, so nothing was left
    // uncomparable.
    assert_eq!(found.not_comparable, 0);

    // The same bill, twice in the same week, from the same supplier.
    let duplicates = of_kind(&found, ANOMALY_DUPLICATE);
    assert_eq!(duplicates.len(), 1, "{:?}", found.findings);
    let pair = duplicates[0];
    assert_eq!(pair.account_id, expense);
    assert_eq!(pair.amount_cents, 30_000);
    assert_eq!(
        pair.counterparty.as_ref().map(|p| (p.kind, p.key.as_str())),
        Some((PARTY_SUPPLIER, "Solo Supplies"))
    );
    let cited: Vec<&str> = pair.sources.iter().map(|s| s.entry_id.as_str()).collect();
    assert_eq!(cited, vec![ids[4].as_str(), ids[5].as_str()]);

    // The rent that skipped March, with February and April as the evidence.
    let gaps = of_kind(&found, ANOMALY_MISSING_RECURRING);
    assert_eq!(gaps.len(), 1, "{:?}", found.findings);
    assert_eq!(gaps[0].missing_month, Some(on(Month::March, 1)));
    assert_eq!(gaps[0].typical_cents, Some(120_000));
    let cited: Vec<&str> = gaps[0]
        .sources
        .iter()
        .map(|s| s.entry_id.as_str())
        .collect();
    assert_eq!(cited, vec![ids[1].as_str(), ids[2].as_str()]);

    // The one-off, against what this account otherwise moves. Both sides of it
    // are unusual on their own account, and each is its own question.
    let outliers = of_kind(&found, ANOMALY_UNUSUAL_AMOUNT);
    assert_eq!(outliers.len(), 2, "{:?}", found.findings);
    let cost = outliers
        .iter()
        .find(|f| f.account_id == expense)
        .expect("the cost side");
    assert_eq!(cost.amount_cents, 700_000);
    assert_eq!(cost.typical_cents, Some(120_000));
    assert_eq!(cost.sources.len(), 1);
    assert_eq!(cost.sources[0].entry_id.as_str(), ids[6].as_str());
    assert_eq!(cost.sources[0].entry_date, on(Month::March, 12));
    assert_eq!(cost.sources[0].kind, EntryKind::Bill);
    assert_eq!(cost.sources[0].memo, "a one-off");

    assert_eq!(found.found, found.findings.len());
}

#[tokio::test]
async fn another_tenants_books_move_nothing_on_this_ones_scan() {
    let store = common::test_store().await;
    let ours = tenant_with_chart(&store, "ours").await;
    let our_ids = seeded_books(&ours, "Ours", 1).await;
    let before = scan(&ours).await;

    // A second tenant, larger and messier, in the same days.
    let theirs = tenant_with_chart(&store, "theirs").await;
    let their_ids = seeded_books(&theirs, "Theirs", 7).await;
    bill(
        &theirs,
        on(Month::June, 1),
        "Theirs Extra",
        999_999,
        "noise",
    )
    .await;

    // ★ The aggregate isolation test: the whole answer, unchanged.
    let after = scan(&ours).await;
    assert_eq!(before, after, "a scan that forgot its tenant would differ");

    // …and neither tenant's evidence can reach the other's.
    let ours_cited: Vec<&str> = after
        .findings
        .iter()
        .flat_map(|f| f.sources.iter().map(|s| s.entry_id.as_str()))
        .collect();
    assert!(
        ours_cited
            .iter()
            .all(|id| our_ids.iter().any(|ours| ours.as_str() == *id))
    );
    assert!(
        ours_cited
            .iter()
            .all(|id| !their_ids.iter().any(|t| t.as_str() == *id))
    );
    assert!(
        after
            .findings
            .iter()
            .filter_map(|f| f.counterparty.as_ref())
            .all(|party| !party.key.starts_with("Theirs")),
        "a supplier from the wrong tenant reached the answer"
    );

    let theirs_scan = scan(&theirs).await;
    assert!(
        theirs_scan
            .findings
            .iter()
            .filter_map(|f| f.counterparty.as_ref())
            .all(|party| !party.key.starts_with("Ours"))
    );
    // Their figures are their own, seven times ours.
    let their_gap = of_kind(&theirs_scan, ANOMALY_MISSING_RECURRING);
    assert_eq!(their_gap.len(), 1);
    assert_eq!(their_gap[0].typical_cents, Some(840_000));
}

#[tokio::test]
async fn a_period_is_a_real_boundary_and_a_backwards_one_is_refused() {
    let store = common::test_store().await;
    let account = tenant_with_chart(&store, "bounds").await;
    seeded_books(&account, "Bounds", 1).await;

    // A quarter that holds only the two March bills: no rhythm to break, and
    // too little on the account for anything to be unusual against.
    let march = account
        .fin_anomalies(on(Month::March, 1), on(Month::March, 31))
        .await
        .unwrap();
    assert_eq!(march.scanned, 3);
    assert_eq!(of_kind(&march, ANOMALY_MISSING_RECURRING).len(), 0);
    assert_eq!(of_kind(&march, ANOMALY_DUPLICATE).len(), 1);

    // A period nothing was booked in is a clean answer, not an error.
    let quiet = account
        .fin_anomalies(on(Month::September, 1), on(Month::September, 30))
        .await
        .unwrap();
    assert_eq!(quiet, AnomalyScan::default());

    let backwards = account
        .fin_anomalies(on(Month::June, 30), on(Month::January, 1))
        .await
        .expect_err("a period that ends before it starts");
    assert!(
        matches!(&backwards, StoreError::Validation(message) if message.contains("before its start")),
        "{backwards:?}"
    );
}
