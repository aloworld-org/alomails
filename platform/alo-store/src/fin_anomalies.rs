//! Naming what looks unusual in a period of the journal (alo Finance, ADR 0035,
//! wave B4.14b; `docs/design/finance.md`, "The finance agent") — the store half
//! of the agent's `flag_anomalies`.
//!
//! # A flag is a question about a document, never a statement about a person
//!
//! Everything this module produces points at **entries**. No finding carries a
//! user, and the read the postings are judged on ignores
//! [`crate::fin_journal::Posting::user_id`] entirely. An agent that summarised
//! whose spending looks odd would be a profiling feature nobody asked for, and
//! the design note says so in as many words; the code says it by never putting
//! the column in a group key.
//!
//! And every finding carries **the rows that caused it**
//! ([`Anomaly::sources`]). An unexplained flag is an accusation: a person shown
//! "something is wrong with March" cannot agree or disagree with it, so the
//! answer is the two entries either side of the hole, the entry that is five
//! times the usual, the pair booked in the same week.
//!
//! # No score, no risk, no model
//!
//! [`find_anomalies`] is a pure function over rows: the same period answers the
//! same way twice, and it is unit-tested without a database. There is no
//! confidence, no ranking and no percentage anywhere in it, because a number
//! attached to a suspicion is read as evidence for it. Three deterministic
//! rules, each of which a person can check by hand:
//!
//! - [`ANOMALY_DUPLICATE`] — the same counterparty, the same account and the
//!   **same signed amount** twice inside a week. Signed rather than absolute so
//!   an invoice and the payment that settles it — equal, opposite, days apart on
//!   the very same receivable account — are not reported as a double booking,
//!   which is the one false positive this rule would otherwise produce every
//!   time anybody pays anything.
//! - [`ANOMALY_UNUSUAL_AMOUNT`] — a posting far outside what its account usually
//!   moves, measured against that account's own median in the very same period
//!   ([`OUTLIER_FACTOR`], with a floor so a tenant whose median is €2 does not
//!   get every €12 lunch flagged).
//! - [`ANOMALY_MISSING_RECURRING`] — a cost with a monthly rhythm that skipped a
//!   month. Only **interior** months are ever reported: a rent that starts in
//!   March is not eleven missing months, and a subscription cancelled in
//!   October is not a hole in November.
//!
//! # What was not looked at is part of the answer
//!
//! [`AnomalyScan::truncated`] and [`AnomalyScan::not_comparable`] exist for the
//! reason [`crate::fin_categorise`]'s skipped list does: silence reads as
//! "nothing was wrong" when what it means is "I stopped looking". A scan that
//! hit its ceiling says so, and the entries that name no counterparty — which
//! the duplicate rule therefore cannot compare — are counted rather than quietly
//! dropped.

use std::collections::{BTreeMap, HashSet};

use time::{Date, Month};

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::fin_journal::{Entry, EntryKind, JournalEntry, Posting};
use crate::id::{FinAccountId, FinEntryId};

/// The same counterparty, account and signed amount, booked twice inside
/// [`DUPLICATE_WINDOW_DAYS`].
pub const ANOMALY_DUPLICATE: &str = "duplicate";

/// A posting far outside what its own account moved in the same period.
pub const ANOMALY_UNUSUAL_AMOUNT: &str = "unusualAmount";

/// A monthly cost that skipped a month inside its own run.
pub const ANOMALY_MISSING_RECURRING: &str = "missingRecurring";

/// A counterparty that is one of the tenant's billing customers — the key is
/// that customer's id, which the caller resolves to a name.
pub const PARTY_CUSTOMER: &str = "customer";

/// A counterparty that is a supplier — the key is the supplier key a bill
/// carries, which is already name-shaped.
pub const PARTY_SUPPLIER: &str = "supplier";

/// Most entries one scan reads. The journal's own page ceiling
/// ([`crate::fin_journal::JOURNAL_PAGE_MAX`]) is a screen's; this is an
/// analysis's, and a period with more entries than this comes back with
/// [`AnomalyScan::truncated`] set rather than silently half-read.
pub const ANOMALY_SCAN_MAX: i64 = 2_000;

/// How close together two identical bookings have to be to be worth asking
/// about. A week: long enough to catch the same bill entered on Monday and on
/// Friday, short enough that a genuinely fortnightly charge is never a pair.
pub const DUPLICATE_WINDOW_DAYS: i64 = 7;

/// How many postings an account needs in the period before its median means
/// anything. Below this there is no "usual" to be outside of.
pub const OUTLIER_MIN_SAMPLE: usize = 5;

/// How many times its account's median a posting must be to be worth naming.
pub const OUTLIER_FACTOR: i64 = 5;

/// The smallest amount that can be an outlier at all, in cents (€100).
///
/// Without a floor, a tenant whose median posting is €2 has every ordinary €12
/// lunch flagged, and a flag that fires on everything is read as noise and then
/// as nothing.
pub const OUTLIER_FLOOR_CENTS: i64 = 10_000;

/// How many months a cost must have run in before a gap in it means anything.
pub const RECURRING_MIN_MONTHS: usize = 3;

/// Most entries one finding cites. A duplicate is a pair; a badly split bill
/// entered eleven times is still explained by ten of them.
pub const ANOMALY_SOURCES_MAX: usize = 10;

/// Most findings one scan returns. A list nobody reaches the end of is a list
/// nobody reads; the count of what was found is reported whole either way
/// ([`AnomalyScan::found`]).
pub const ANOMALY_FINDINGS_MAX: usize = 50;

/// Who the other side of a posting was, when it names one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Counterparty {
    /// [`PARTY_CUSTOMER`] or [`PARTY_SUPPLIER`] — which kind of key this is.
    pub kind: &'static str,
    /// The customer id, or the supplier key. Never a user.
    pub key: String,
}

impl Counterparty {
    /// The counterparty a posting names, if it names one.
    ///
    /// A supplier key wins over a customer id when a posting carries both,
    /// which today it never does: the rule is stated so a later posting rule
    /// that sets them together produces one group rather than two halves.
    #[must_use]
    pub fn of(posting: &Posting) -> Option<Self> {
        if let Some(key) = posting.supplier_key.as_ref().filter(|k| !k.is_empty()) {
            return Some(Self {
                kind: PARTY_SUPPLIER,
                key: key.clone(),
            });
        }
        posting
            .customer_id
            .as_ref()
            .filter(|k| !k.is_empty())
            .map(|key| Self {
                kind: PARTY_CUSTOMER,
                key: key.clone(),
            })
    }
}

/// One entry a finding points at — the whole of the evidence for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnomalySource {
    /// The entry.
    pub entry_id: FinEntryId,
    /// Its accounting date, which is the date a person will look for.
    pub entry_date: Date,
    /// What kind of event it books.
    pub kind: EntryKind,
    /// The line a human reading the journal sees.
    pub memo: String,
    /// What it moved on the account the finding is about, in the tenant's
    /// accounting currency. Signed, as the journal holds it.
    pub amount_cents: i64,
}

/// One thing worth asking about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anomaly {
    /// One of the `ANOMALY_*` codes. Machine-readable on purpose: the words a
    /// person reads are written in the client, in their language.
    pub kind: &'static str,
    /// The account it is about. Every finding has one — an anomaly with no
    /// account is a feeling.
    pub account_id: FinAccountId,
    /// The other side, when the postings name one.
    pub counterparty: Option<Counterparty>,
    /// The amount the finding is about, in the accounting currency: the
    /// repeated amount, the outlying one, or the one that usually arrives in
    /// the month that is empty.
    pub amount_cents: i64,
    /// What this account, or this cost, usually moves — the figure the one
    /// above is unusual *against*. `None` where the rule makes no comparison.
    pub typical_cents: Option<i64>,
    /// The first day of the month nothing was booked in
    /// ([`ANOMALY_MISSING_RECURRING`] only).
    pub missing_month: Option<Date>,
    /// The entries that caused it, oldest first, at most
    /// [`ANOMALY_SOURCES_MAX`].
    pub sources: Vec<AnomalySource>,
}

/// What one scan of a period found, and what it could not look at.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnomalyScan {
    /// The findings, at most [`ANOMALY_FINDINGS_MAX`], in a stable order.
    pub findings: Vec<Anomaly>,
    /// How many were found in total, which is not `findings.len()` when the
    /// list was cut.
    pub found: usize,
    /// How many entries were read.
    pub scanned: usize,
    /// Whether the period holds more entries than [`ANOMALY_SCAN_MAX`], so the
    /// scan covers only the first days of it. Set by
    /// [`AccountStore::fin_anomalies`]; [`find_anomalies`] judges what it is
    /// given and never claims it was everything.
    pub truncated: bool,
    /// How many entries name no counterparty at all, and so could not be
    /// compared for duplication.
    pub not_comparable: usize,
}

/// One posting, with the entry it belongs to — how every rule below reads the
/// journal.
#[derive(Debug, Clone, Copy)]
struct Line<'a> {
    entry: &'a Entry,
    posting: &'a Posting,
}

impl Line<'_> {
    /// What it moved, in the tenant's accounting currency. Every rule compares
    /// base amounts so a dollar invoice and a euro one are the same size of
    /// thing.
    fn amount(&self) -> i64 {
        self.posting.base_cents
    }

    /// The evidence line for this entry on this account.
    fn source(&self) -> AnomalySource {
        AnomalySource {
            entry_id: self.entry.id.clone(),
            entry_date: self.entry.entry_date,
            kind: self.entry.kind,
            memo: self.entry.memo.clone(),
            amount_cents: self.amount(),
        }
    }
}

/// The whole judgement, as pure code: three rules over a period of the journal.
///
/// `journal` is expected oldest first ([`AccountStore::fin_journal_range`]) and
/// is not re-sorted, so a caller that hands it a scattered sample gets findings
/// about that sample. Postings that moved nothing are ignored by every rule: a
/// zero is a balancing line, not an event.
#[must_use]
pub fn find_anomalies(journal: &[JournalEntry]) -> AnomalyScan {
    let lines: Vec<Line<'_>> = journal
        .iter()
        .flat_map(|je| {
            je.postings
                .iter()
                .filter(|p| p.base_cents != 0)
                .map(move |posting| Line {
                    entry: &je.entry,
                    posting,
                })
        })
        .collect();

    let mut findings = Vec::new();
    findings.extend(duplicates(&lines));
    findings.extend(outliers(&lines));
    findings.extend(missing_months(&lines));
    // A stable order the client can rely on, and one a person reads in: the
    // kind, then the account, then the day it is about.
    findings.sort_by(|a, b| {
        rank(a.kind)
            .cmp(&rank(b.kind))
            .then_with(|| a.account_id.as_str().cmp(b.account_id.as_str()))
            .then_with(|| first_day(a).cmp(&first_day(b)))
            .then_with(|| a.amount_cents.cmp(&b.amount_cents))
    });
    let found = findings.len();
    findings.truncate(ANOMALY_FINDINGS_MAX);

    AnomalyScan {
        findings,
        found,
        scanned: journal.len(),
        truncated: false,
        not_comparable: not_comparable(journal),
    }
}

/// The order the three kinds are read in — duplicates first, because a double
/// booking is the one a person can act on immediately.
fn rank(kind: &str) -> u8 {
    match kind {
        ANOMALY_DUPLICATE => 0,
        ANOMALY_MISSING_RECURRING => 1,
        _ => 2,
    }
}

/// The day a finding is anchored to, for ordering: the month that is empty, or
/// the first entry it cites.
fn first_day(anomaly: &Anomaly) -> Option<Date> {
    anomaly
        .missing_month
        .or_else(|| anomaly.sources.first().map(|s| s.entry_date))
}

/// Entries the duplicate rule could not compare, because nothing in them names
/// the other side of the transaction.
///
/// Reversals and credit notes are not counted: they are excluded from that rule
/// by *kind*, so counting them as "no counterparty" would overstate what is
/// missing from the books.
fn not_comparable(journal: &[JournalEntry]) -> usize {
    journal
        .iter()
        .filter(|je| comparable_kind(&je.entry))
        .filter(|je| !je.postings.iter().any(|p| Counterparty::of(p).is_some()))
        .count()
}

/// Whether an entry can be one half of a duplicate at all.
///
/// A reversal mirrors an earlier entry and a credit note mirrors an invoice: in
/// both cases the second booking is the *point*, and reporting it as a
/// duplicate would flag the correction rather than the mistake.
fn comparable_kind(entry: &Entry) -> bool {
    entry.reverses_entry_id.is_none()
        && !matches!(entry.kind, EntryKind::Reversal | EntryKind::CreditNote)
}

/// The same counterparty, account and signed amount, twice inside a week.
fn duplicates(lines: &[Line<'_>]) -> Vec<Anomaly> {
    // (counterparty, account, signed amount) → the lines that match it.
    let mut groups: BTreeMap<(Counterparty, String, i64), Vec<Line<'_>>> = BTreeMap::new();
    for line in lines.iter().filter(|l| comparable_kind(l.entry)) {
        let Some(party) = Counterparty::of(line.posting) else {
            continue;
        };
        groups
            .entry((
                party,
                line.posting.account_id.as_str().to_owned(),
                line.amount(),
            ))
            .or_default()
            .push(*line);
    }

    let mut found = Vec::new();
    for ((party, account, amount), mut group) in groups {
        group.sort_by_key(|line| (line.entry.entry_date, line.entry.id.as_str().to_owned()));
        // Greedy clusters: everything within a week of the line that opened the
        // cluster. A charge that repeats every eight days therefore never
        // clusters, which is the intent — that is a schedule, not a mistake.
        let mut cluster: Vec<Line<'_>> = Vec::new();
        for line in group {
            let opened = cluster
                .first()
                .map(|first: &Line<'_>| first.entry.entry_date);
            match opened {
                Some(day) if (line.entry.entry_date - day).whole_days() > DUPLICATE_WINDOW_DAYS => {
                    push_duplicate(&mut found, &cluster, &party, &account, amount);
                    cluster = vec![line];
                }
                _ => cluster.push(line),
            }
        }
        push_duplicate(&mut found, &cluster, &party, &account, amount);
    }
    found
}

/// One duplicate finding, if the cluster names more than one entry.
///
/// Two postings of the *same* entry are not a duplicate — an entry may touch an
/// account twice for perfectly good reasons — so the count is of distinct
/// entries.
fn push_duplicate(
    out: &mut Vec<Anomaly>,
    cluster: &[Line<'_>],
    party: &Counterparty,
    account: &str,
    amount: i64,
) {
    let mut seen = HashSet::new();
    let sources: Vec<AnomalySource> = cluster
        .iter()
        .filter(|line| seen.insert(line.entry.id.as_str().to_owned()))
        .take(ANOMALY_SOURCES_MAX)
        .map(Line::source)
        .collect();
    if sources.len() < 2 {
        return;
    }
    out.push(Anomaly {
        kind: ANOMALY_DUPLICATE,
        account_id: FinAccountId::new(account),
        counterparty: Some(party.clone()),
        amount_cents: amount,
        typical_cents: None,
        missing_month: None,
        sources,
    });
}

/// A posting far outside what its own account moved in the same period.
fn outliers(lines: &[Line<'_>]) -> Vec<Anomaly> {
    let mut by_account: BTreeMap<String, Vec<Line<'_>>> = BTreeMap::new();
    for line in lines {
        // A reversal is the mirror of something already in the sample; counting
        // it would both double the sample and flag the correction.
        if matches!(line.entry.kind, EntryKind::Reversal) {
            continue;
        }
        by_account
            .entry(line.posting.account_id.as_str().to_owned())
            .or_default()
            .push(*line);
    }

    let mut found = Vec::new();
    for (account, group) in by_account {
        if group.len() < OUTLIER_MIN_SAMPLE {
            continue;
        }
        let Some(median) = median(&group) else {
            continue;
        };
        let threshold = median
            .saturating_mul(OUTLIER_FACTOR)
            .max(OUTLIER_FLOOR_CENTS);
        // One finding per entry, however many of its postings are on the
        // account: an entry named twice reads as two problems.
        let mut seen = HashSet::new();
        for line in group {
            let size = line.amount().saturating_abs();
            if size < threshold || !seen.insert(line.entry.id.as_str().to_owned()) {
                continue;
            }
            found.push(Anomaly {
                kind: ANOMALY_UNUSUAL_AMOUNT,
                account_id: FinAccountId::new(&account),
                counterparty: Counterparty::of(line.posting),
                amount_cents: line.amount(),
                typical_cents: Some(median),
                missing_month: None,
                sources: vec![line.source()],
            });
        }
    }
    found
}

/// The middle of what an account moved, by size — the upper of the two middles
/// for an even sample, which needs no arithmetic and therefore no rounding.
///
/// A median rather than a mean because one €500 000 entry among small ones
/// would drag a mean up until nothing looks unusual against it, which is the
/// same as having no rule.
fn median(group: &[Line<'_>]) -> Option<i64> {
    let mut sizes: Vec<i64> = group
        .iter()
        .map(|line| line.amount().saturating_abs())
        .collect();
    sizes.sort_unstable();
    sizes.get(sizes.len() / 2).copied().filter(|m| *m > 0)
}

/// A cost with a monthly rhythm that skipped a month inside its own run.
fn missing_months(lines: &[Line<'_>]) -> Vec<Anomaly> {
    // (account, counterparty) → month → the entries booked in it. A group with
    // no counterparty is kept: rent booked against a rent account is a rhythm
    // whether or not the posting names the landlord.
    let mut groups: BTreeMap<(String, Option<Counterparty>), BTreeMap<i32, Vec<Line<'_>>>> =
        BTreeMap::new();
    for line in lines {
        if matches!(line.entry.kind, EntryKind::Reversal) {
            continue;
        }
        groups
            .entry((
                line.posting.account_id.as_str().to_owned(),
                Counterparty::of(line.posting),
            ))
            .or_default()
            .entry(month_index(line.entry.entry_date))
            .or_default()
            .push(*line);
    }

    let mut found = Vec::new();
    for ((account, party), months) in groups {
        // A rhythm is one booking a month. Two in a month is a habit of a
        // different shape, and a gap in it is not evidence of anything.
        if months.len() < RECURRING_MIN_MONTHS
            || months.values().any(|lines| distinct_entries(lines) != 1)
        {
            continue;
        }
        let (Some(first), Some(last)) = (months.keys().next(), months.keys().next_back()) else {
            continue;
        };
        let span = usize::try_from(last - first + 1).unwrap_or(usize::MAX);
        // Present in more than half of its own span, or it is not a rhythm with
        // a hole in it — it is two bursts with a quiet year between them.
        if months.len().saturating_mul(2) <= span {
            continue;
        }
        let typical = median(&months.values().flatten().copied().collect::<Vec<_>>());
        for empty in (*first + 1)..*last {
            if months.contains_key(&empty) {
                continue;
            }
            let before = months
                .range(..empty)
                .next_back()
                .and_then(|(_, l)| l.first());
            let after = months.range(empty..).next().and_then(|(_, l)| l.first());
            let sources: Vec<AnomalySource> = [before, after]
                .into_iter()
                .flatten()
                .map(|l| l.source())
                .collect();
            let Some(month) = first_of_month(empty) else {
                continue;
            };
            found.push(Anomaly {
                kind: ANOMALY_MISSING_RECURRING,
                account_id: FinAccountId::new(&account),
                counterparty: party.clone(),
                amount_cents: typical.unwrap_or_default(),
                typical_cents: typical,
                missing_month: Some(month),
                sources,
            });
        }
    }
    found
}

/// How many different entries a month's lines belong to.
fn distinct_entries(lines: &[Line<'_>]) -> usize {
    lines
        .iter()
        .map(|line| line.entry.id.as_str())
        .collect::<HashSet<_>>()
        .len()
}

/// A month as one number, so "the month after" is addition and a gap is a
/// missing integer rather than a calendar problem.
fn month_index(day: Date) -> i32 {
    day.year() * 12 + i32::from(u8::from(day.month())) - 1
}

/// Back to the first day of that month, which is how a month travels to a
/// caller: as a date, in the one format every date in this product uses.
fn first_of_month(index: i32) -> Option<Date> {
    let year = index.div_euclid(12);
    let month = u8::try_from(index.rem_euclid(12) + 1).ok()?;
    Date::from_calendar_date(year, Month::try_from(month).ok()?, 1).ok()
}

impl AccountStore {
    /// What looks unusual in the tenant's journal between `from` and `to`, both
    /// days included.
    ///
    /// The **whole tenant's** books, which is why every door onto this is behind
    /// the finance gate (an admin or the accountant): a member reading their
    /// colleagues' entries through an agent would be a hole in exactly the wall
    /// B4.12 built.
    ///
    /// It writes nothing. There is no anomaly table, no "reviewed" flag and no
    /// dismissal: a finding is a question asked of a period, and the answer to
    /// it is a correcting entry in the journal, not a state on the question.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the period ends before it starts;
    /// [`StoreError::Db`] on failure.
    pub async fn fin_anomalies(&self, from: Date, to: Date) -> Result<AnomalyScan> {
        if to < from {
            return Err(StoreError::Validation(
                "the end of the period must not be before its start".to_owned(),
            ));
        }
        // One more than the ceiling, so hitting it is a fact rather than a
        // coincidence to be guessed at.
        let mut journal = self
            .fin_journal_range(from, to, ANOMALY_SCAN_MAX + 1)
            .await?;
        let ceiling = usize::try_from(ANOMALY_SCAN_MAX).unwrap_or(usize::MAX);
        let truncated = journal.len() > ceiling;
        journal.truncate(ceiling);
        let mut scan = find_anomalies(&journal);
        scan.truncated = truncated;
        Ok(scan)
    }
}

/// A silent dependency made loud: the duplicate rule groups on the accounting
/// currency's amount, so a posting's own currency never reaches a group key.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::fin_journal::{Entry, EntrySource, JournalEntry, Posting};
    use crate::id::FinPostingId;
    use time::OffsetDateTime;

    fn day(iso: &str) -> Date {
        Date::parse(iso, &time::format_description::well_known::Iso8601::DATE).expect("a plain day")
    }

    fn entry(id: &str, on: &str, kind: EntryKind) -> Entry {
        Entry {
            id: FinEntryId::new(id),
            entry_date: day(on),
            kind,
            source: None::<EntrySource>,
            memo: format!("memo {id}"),
            reverses_entry_id: None,
            attachment_node_id: None,
            currency: "EUR".to_owned(),
            fx: crate::billing_fx::FxSnapshot::identity("EUR", day(on)),
            created_by: "u1".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn posting(account: &str, cents: i64, customer: Option<&str>) -> Posting {
        Posting {
            id: FinPostingId::new("p"),
            entry_id: FinEntryId::new("e"),
            position: 0,
            account_id: FinAccountId::new(account),
            amount_cents: cents,
            base_cents: cents,
            vat_rate_bp: None,
            customer_id: customer.map(str::to_owned),
            supplier_key: None,
            project_id: None,
            user_id: Some("u1".to_owned()),
            memo: String::new(),
        }
    }

    fn booked(
        id: &str,
        on: &str,
        account: &str,
        cents: i64,
        customer: Option<&str>,
    ) -> JournalEntry {
        JournalEntry {
            entry: entry(id, on, EntryKind::Invoice),
            postings: vec![posting(account, cents, customer)],
        }
    }

    /// Five ordinary postings on an account, so a sixth can be an outlier
    /// against them.
    fn ordinary(account: &str, cents: i64, customer: Option<&str>) -> Vec<JournalEntry> {
        (1..=5)
            .map(|i| {
                booked(
                    &format!("{account}-ord-{i}"),
                    &format!("2026-03-0{i}"),
                    account,
                    cents,
                    customer,
                )
            })
            .collect()
    }

    #[test]
    fn the_same_amount_to_the_same_customer_twice_in_a_week_is_a_duplicate() {
        let journal = vec![
            booked("a", "2026-03-02", "4000", 120_000, Some("cust-1")),
            booked("b", "2026-03-05", "4000", 120_000, Some("cust-1")),
        ];
        let scan = find_anomalies(&journal);
        assert_eq!(scan.findings.len(), 1);
        let found = &scan.findings[0];
        assert_eq!(found.kind, ANOMALY_DUPLICATE);
        assert_eq!(found.amount_cents, 120_000);
        assert_eq!(
            found.counterparty,
            Some(Counterparty {
                kind: PARTY_CUSTOMER,
                key: "cust-1".to_owned()
            })
        );
        // The evidence is both entries, oldest first — an unexplained flag is an
        // accusation.
        assert_eq!(found.sources.len(), 2);
        assert_eq!(found.sources[0].entry_id.as_str(), "a");
        assert_eq!(found.sources[1].entry_id.as_str(), "b");
        assert_eq!(scan.found, 1);
        assert_eq!(scan.scanned, 2);
        assert!(!scan.truncated);
    }

    #[test]
    fn an_invoice_and_the_payment_that_settles_it_are_never_a_duplicate() {
        // The same customer, the same account, the same size, three days apart —
        // and opposite. Grouping on the *signed* amount is the whole reason this
        // is not reported at every tenant, every week.
        let mut payment = booked("pay", "2026-03-05", "1300", -120_000, Some("cust-1"));
        payment.entry.kind = EntryKind::Payment;
        let journal = vec![
            booked("inv", "2026-03-02", "1300", 120_000, Some("cust-1")),
            payment,
        ];
        assert!(find_anomalies(&journal).findings.is_empty());
    }

    #[test]
    fn a_correction_is_not_a_duplicate_of_what_it_corrects() {
        let mut credit = booked("cn", "2026-03-04", "4000", 120_000, Some("cust-1"));
        credit.entry.kind = EntryKind::CreditNote;
        let mut reversal = booked("rev", "2026-03-06", "4000", 120_000, Some("cust-1"));
        reversal.entry.kind = EntryKind::Reversal;
        reversal.entry.reverses_entry_id = Some(FinEntryId::new("inv"));
        let journal = vec![
            booked("inv", "2026-03-02", "4000", 120_000, Some("cust-1")),
            credit,
            reversal,
        ];
        assert!(
            find_anomalies(&journal)
                .findings
                .iter()
                .all(|f| f.kind != ANOMALY_DUPLICATE),
            "a credit note and a reversal mirror an entry by design"
        );
    }

    #[test]
    fn two_bookings_more_than_a_week_apart_are_a_schedule_not_a_mistake() {
        let journal = vec![
            booked("a", "2026-03-02", "4000", 120_000, Some("cust-1")),
            booked("b", "2026-03-10", "4000", 120_000, Some("cust-1")),
        ];
        assert!(find_anomalies(&journal).findings.is_empty());
        // Exactly a week apart still counts as the same week's mistake.
        let journal = vec![
            booked("a", "2026-03-02", "4000", 120_000, Some("cust-1")),
            booked("b", "2026-03-09", "4000", 120_000, Some("cust-1")),
        ];
        assert_eq!(find_anomalies(&journal).findings.len(), 1);
    }

    #[test]
    fn an_entry_with_no_counterparty_is_counted_rather_than_dropped() {
        let journal = vec![
            booked("a", "2026-03-02", "6100", 50_000, None),
            booked("b", "2026-03-03", "6100", 50_000, None),
        ];
        let scan = find_anomalies(&journal);
        // Nothing names the other side, so nothing can be compared — and the
        // scan says how much it could not look at rather than reporting clean.
        assert!(scan.findings.is_empty());
        assert_eq!(scan.not_comparable, 2);
    }

    #[test]
    fn a_posting_far_outside_its_accounts_own_median_is_named_with_that_median() {
        let mut journal = ordinary("6200", 20_000, None);
        journal.push(booked("big", "2026-03-20", "6200", 500_000, None));
        let scan = find_anomalies(&journal);
        let found: Vec<&Anomaly> = scan
            .findings
            .iter()
            .filter(|f| f.kind == ANOMALY_UNUSUAL_AMOUNT)
            .collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].amount_cents, 500_000);
        assert_eq!(found[0].typical_cents, Some(20_000));
        assert_eq!(found[0].sources.len(), 1);
        assert_eq!(found[0].sources[0].entry_id.as_str(), "big");
    }

    #[test]
    fn an_account_with_too_little_history_has_no_usual_to_be_outside_of() {
        let journal = vec![
            booked("a", "2026-03-01", "6200", 1_000, None),
            booked("big", "2026-03-20", "6200", 900_000, None),
        ];
        assert!(
            find_anomalies(&journal)
                .findings
                .iter()
                .all(|f| f.kind != ANOMALY_UNUSUAL_AMOUNT)
        );
    }

    #[test]
    fn small_money_never_becomes_an_outlier_however_many_times_the_median_it_is() {
        // A median of €2 and a €50 posting: twenty-five times the usual, and
        // still nothing anybody wants a flag about.
        let mut journal = ordinary("6300", 200, None);
        journal.push(booked("lunch", "2026-03-20", "6300", 5_000, None));
        assert!(
            find_anomalies(&journal)
                .findings
                .iter()
                .all(|f| f.kind != ANOMALY_UNUSUAL_AMOUNT),
            "the floor is what stops a rule that fires on everything"
        );
    }

    #[test]
    fn a_monthly_cost_that_skipped_a_month_is_named_with_the_entries_either_side() {
        let journal = vec![
            booked("jan", "2026-01-05", "6100", 120_000, Some("supp")),
            booked("feb", "2026-02-05", "6100", 120_000, Some("supp")),
            // March missing.
            booked("apr", "2026-04-05", "6100", 120_000, Some("supp")),
            booked("may", "2026-05-05", "6100", 120_000, Some("supp")),
        ];
        let scan = find_anomalies(&journal);
        let found: Vec<&Anomaly> = scan
            .findings
            .iter()
            .filter(|f| f.kind == ANOMALY_MISSING_RECURRING)
            .collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].missing_month, Some(day("2026-03-01")));
        assert_eq!(found[0].typical_cents, Some(120_000));
        // February and April: what a person needs to see to agree there is a
        // hole between them.
        assert_eq!(found[0].sources.len(), 2);
        assert_eq!(found[0].sources[0].entry_id.as_str(), "feb");
        assert_eq!(found[0].sources[1].entry_id.as_str(), "apr");
    }

    #[test]
    fn a_cost_that_started_or_stopped_is_not_a_hole() {
        // Three months in a row and then nothing: a cancelled subscription, and
        // only interior months are ever reported, so there is nothing to say.
        let journal = vec![
            booked("jan", "2026-01-05", "6100", 120_000, Some("supp")),
            booked("feb", "2026-02-05", "6100", 120_000, Some("supp")),
            booked("mar", "2026-03-05", "6100", 120_000, Some("supp")),
        ];
        assert!(
            find_anomalies(&journal)
                .findings
                .iter()
                .all(|f| f.kind != ANOMALY_MISSING_RECURRING)
        );
    }

    #[test]
    fn two_bursts_with_a_quiet_year_between_them_are_not_a_rhythm() {
        let journal = vec![
            booked("a", "2026-01-05", "6100", 120_000, Some("supp")),
            booked("b", "2026-02-05", "6100", 120_000, Some("supp")),
            booked("c", "2026-11-05", "6100", 120_000, Some("supp")),
        ];
        assert!(
            find_anomalies(&journal)
                .findings
                .iter()
                .all(|f| f.kind != ANOMALY_MISSING_RECURRING),
            "present in three of eleven months is not a monthly cost"
        );
    }

    #[test]
    fn a_cost_booked_twice_in_some_months_has_no_rhythm_to_break() {
        let journal = vec![
            booked("jan", "2026-01-05", "6100", 120_000, Some("supp")),
            booked("jan2", "2026-01-20", "6100", 120_000, Some("supp")),
            booked("feb", "2026-02-05", "6100", 120_000, Some("supp")),
            booked("apr", "2026-04-05", "6100", 120_000, Some("supp")),
            booked("may", "2026-05-05", "6100", 120_000, Some("supp")),
        ];
        assert!(
            find_anomalies(&journal)
                .findings
                .iter()
                .all(|f| f.kind != ANOMALY_MISSING_RECURRING)
        );
    }

    #[test]
    fn nothing_a_finding_carries_names_a_person() {
        let mut journal = ordinary("6200", 20_000, Some("cust-1"));
        journal.push(booked("big", "2026-03-20", "6200", 500_000, Some("cust-1")));
        let scan = find_anomalies(&journal);
        assert!(!scan.findings.is_empty());
        for found in &scan.findings {
            // The postings all carry a user; no finding can reach one, because
            // no rule ever reads that column.
            assert!(found.counterparty.iter().all(|p| p.kind != "user"));
            for source in &found.sources {
                assert!(!source.memo.contains("u1"));
            }
        }
    }

    #[test]
    fn an_empty_period_is_a_clean_answer_and_says_what_it_read() {
        let scan = find_anomalies(&[]);
        assert!(scan.findings.is_empty());
        assert_eq!(scan.found, 0);
        assert_eq!(scan.scanned, 0);
        assert_eq!(scan.not_comparable, 0);
        assert!(!scan.truncated);
    }

    #[test]
    fn a_balancing_line_of_zero_is_not_an_event() {
        let mut journal = ordinary("6200", 20_000, None);
        journal.push(booked("zero-a", "2026-03-20", "6200", 0, Some("cust-1")));
        journal.push(booked("zero-b", "2026-03-21", "6200", 0, Some("cust-1")));
        let scan = find_anomalies(&journal);
        assert!(scan.findings.is_empty(), "{:?}", scan.findings);
    }

    #[test]
    fn a_month_is_a_number_and_comes_back_a_date() {
        assert_eq!(
            month_index(day("2026-01-31")) + 1,
            month_index(day("2026-02-01"))
        );
        assert_eq!(
            month_index(day("2026-12-31")) + 1,
            month_index(day("2027-01-01"))
        );
        assert_eq!(
            first_of_month(month_index(day("2026-03-17"))),
            Some(day("2026-03-01"))
        );
        assert_eq!(
            first_of_month(month_index(day("2027-01-01"))),
            Some(day("2027-01-01"))
        );
    }

    #[test]
    fn the_same_journal_answers_the_same_way_twice() {
        let mut journal = ordinary("6200", 20_000, Some("cust-1"));
        journal.push(booked("big", "2026-03-20", "6200", 500_000, Some("cust-1")));
        journal.push(booked("dup", "2026-03-05", "4000", 90_000, Some("cust-2")));
        journal.push(booked("dup2", "2026-03-06", "4000", 90_000, Some("cust-2")));
        assert_eq!(find_anomalies(&journal), find_anomalies(&journal));
    }
}
