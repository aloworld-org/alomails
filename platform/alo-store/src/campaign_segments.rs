//! Segments — a saved question about who to mail (alo Campaigns, ADR 0044,
//! wave C1.4).
//!
//! ADR 0044: *there is nothing to sync, because there is no list. A segment is
//! a query over contacts alo already holds* —
//!
//! > everyone who opened the last campaign, has not bought in ninety days, and
//! > is in Belgium
//!
//! This module stores that sentence and answers it. It stores **conditions,
//! never people**: there is no membership table here and no cached count, so a
//! segment saved in March and read in June is re-asked of
//! [`campaign_audience`](crate::campaign_audience) at the moment of asking.
//! That is not an optimisation left undone — a stored member list is a copy of
//! the audience, and a copy is how somebody who unsubscribed on Monday is
//! mailed on Tuesday. Consent (C1.2) and suppression (C1.3) are applied by the
//! same CTEs the whole audience is built from, because a segment that assembled
//! its own `FROM` would be a second place `contacts` could be read and a second
//! place suppression could be forgotten.
//!
//! ## The count, and its exclusions
//!
//! [`campaign_segment_tally`](AccountStore::campaign_segment_tally) is the
//! item's actual deliverable: **a number nobody has to trust.** It returns how
//! many people the segment may mail *and* the people it selected but will not
//! mail, each bucket named with the reason — no consent record, or suppressed
//! and why. A campaign screen that shows "1 240 recipients" and nothing else is
//! unauditable: the difference between the question asked and the mail sent is
//! exactly where a consent bug hides, and it hides in silence.
//!
//! One person falls in exactly one bucket, and **suppression outranks
//! consent**: somebody who never consented *and* unsubscribed is reported as
//! having unsubscribed, because that is the stronger fact and the one a
//! colleague can act on. The buckets therefore sum to the number of people the
//! conditions selected ([`SegmentTally::matched`]), and a tally that did not
//! add up would be a decode error rather than a rounded-off screen.
//!
//! ## The conditions, and the one that is missing
//!
//! - **Country**, from `billing_customers.country` — the only source that has
//!   one. A country condition therefore **excludes people whose country is
//!   unknown** rather than assuming them in: a form submitter we cannot place
//!   is not evidence that they are in Belgium.
//! - **Bought or not bought, within a period** — an *issued* invoice (never a
//!   draft, never a void one, never a credit note) raised for a billing
//!   customer at this address. A draft is not a purchase and a void invoice is
//!   a purchase that was cancelled; counting either would put people into a
//!   "recent customers" send who bought nothing.
//! - **Has or has not received a given campaign** is what ADR 0044 also names,
//!   and it is **not built here** — deliberately, and recorded in the migration
//!   too. It was deferred because the column would have referenced a table
//!   that did not exist.
//!
//!   **Both tables it needs now exist**: the campaign record (C3.1,
//!   [`crate::campaign_record`]) and the per-recipient send ledger (C4.1,
//!   [`crate::campaign_send`], migration 0800). `campaign_send_recipients` is
//!   keyed `(tenant_id, campaign_id, address)`, which is exactly the index this
//!   condition wants to probe. What remains is what was always described: one
//!   additive column and one extra CTE.
//!
//!   It was not built in the change that unblocked it, because a segment
//!   condition is a saved query somebody's mail depends on and deserves its own
//!   tests rather than a corner of another item's.
//!
//! Nothing in this module sends anything.

use time::{Date, Duration, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field;
use crate::campaign_audience::{
    AudienceMember, AudiencePage, CampaignRecipient, MEMBER_COLUMNS, MemberRow, Reach, people_cte,
};
use crate::campaign_suppression::SuppressionReason;
use crate::error::{Result, StoreError};
use crate::id::{CampaignSegmentId, UserId};

/// The longest name a segment may carry — matching the migration's `CHECK`.
pub const SEGMENT_NAME_MAX: usize = 120;

/// The most countries one segment may name.
///
/// Well past "the countries we sell to" and well short of a list somebody
/// pasted; a segment naming most of the world is the absence of a country
/// condition written the long way round.
pub const SEGMENT_COUNTRIES_MAX: usize = 50;

/// The longest period a purchase condition may look back over, in days — ten
/// years. Beyond that the honest condition is "ever", which
/// [`PurchaseWindow::within_days`] expresses as `None`.
pub const SEGMENT_PERIOD_DAYS_MAX: i32 = 3650;

/// The most saved segments one read returns.
pub const SEGMENT_PAGE_MAX: i64 = 200;

/// The bucket token the tally uses for people the segment may actually mail.
const MAILABLE_BUCKET: &str = "mailable";

/// The prefix the tally puts in front of a suppression reason, so a bucket
/// names both *that* somebody was suppressed and *why*.
const SUPPRESSED_BUCKET_PREFIX: &str = "suppressed:";

/// Whether the segment wants people who have bought, or people who have not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PurchaseCondition {
    /// Somebody with at least one issued invoice in the period.
    Bought,
    /// Somebody with none — including people who have never bought at all,
    /// which is what "has not bought in ninety days" means to the colleague
    /// writing it.
    NotBought,
}

impl PurchaseCondition {
    /// The stored token. Stable: it is written into rows that outlive releases.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bought => "bought",
            Self::NotBought => "not_bought",
        }
    }

    /// Parses a stored token, or `None` when it is not one of ours.
    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "bought" => Some(Self::Bought),
            "not_bought" => Some(Self::NotBought),
            _ => None,
        }
    }
}

/// A purchase condition and the period it looks back over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PurchaseWindow {
    pub condition: PurchaseCondition,
    /// How far back to look, in days, or `None` for "ever".
    ///
    /// `None` is not a missing value: *has never bought from us* is a real
    /// segment, and forcing it to be written as 36 500 days would say the same
    /// thing less clearly and stop being true in a decade.
    pub within_days: Option<i32>,
}

/// What a segment asks. Empty means everybody in the audience.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SegmentConditions {
    /// ISO 3166-1 alpha-2 codes, in any casing; normalised on the way in.
    /// Empty is **no country condition** rather than "no country matches".
    pub countries: Vec<String>,
    /// The purchase condition, or `None` for no purchase condition.
    pub purchase: Option<PurchaseWindow>,
}

/// A saved segment, as it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CampaignSegment {
    pub id: CampaignSegmentId,
    /// What a colleague calls this question. Unique within the tenant, folded
    /// and trimmed, so "send it to the Belgian customers" names one thing.
    pub name: String,
    pub conditions: SegmentConditions,
    /// The colleague who saved it — who to ask what the question meant. Never a
    /// claim that anybody it selects agreed to anything; that is
    /// [`campaign_consent`](crate::campaign_consent).
    pub created_by: UserId,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// A segment to save, or to save over an existing one.
#[derive(Debug, Clone)]
pub struct NewCampaignSegment<'a> {
    /// The name, in whatever spacing it arrived in; trimmed here.
    pub name: &'a str,
    pub conditions: SegmentConditions,
}

/// Why somebody the segment selected will not be mailed.
///
/// The reason a screen prints beside an excluded person, and the key the tally
/// groups them under. Ordered so a tally reads the same way twice, and so the
/// two shapes of exclusion stay visibly different: a missing consent record is
/// something the tenant can still go and obtain, while a suppression is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExclusionReason {
    /// The tenant holds no evidence this person agreed to be mailed (C1.2).
    NoConsent,
    /// They asked to stop, bounced, or complained (C1.3) — absolute, and
    /// stronger than any consent record.
    Suppressed(SuppressionReason),
}

impl ExclusionReason {
    /// The bucket token, matching what the tally query emits.
    pub fn token(&self) -> String {
        match self {
            Self::NoConsent => "no_consent".to_owned(),
            Self::Suppressed(reason) => format!("{SUPPRESSED_BUCKET_PREFIX}{}", reason.as_str()),
        }
    }

    /// Parses a bucket token, or `None` when this build does not know it.
    ///
    /// `None` is never treated as "not excluded": the tally reports it as a
    /// decode failure, because a bucket we cannot name would silently drop
    /// people out of a number whose whole purpose is to add up.
    fn parse(token: &str) -> Option<Self> {
        if token == "no_consent" {
            return Some(Self::NoConsent);
        }
        token
            .strip_prefix(SUPPRESSED_BUCKET_PREFIX)
            .and_then(SuppressionReason::parse)
            .map(Self::Suppressed)
    }

    /// Why this member will not be mailed, or `None` when they will be.
    ///
    /// The Rust half of the precedence the tally's `CASE` applies, for the
    /// screen that lists people rather than counts them (C1.5). **Suppression
    /// first**: somebody with no consent record who also unsubscribed is
    /// reported as having unsubscribed, because that is the stronger fact and
    /// the one that cannot be undone by going and asking them nicely.
    pub fn for_member(member: &AudienceMember) -> Option<Self> {
        if let Some(suppression) = &member.suppression {
            return Some(Self::Suppressed(suppression.reason));
        }
        member.consent.is_none().then_some(Self::NoConsent)
    }
}

/// How many people one reason kept out of a send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentExclusion {
    pub reason: ExclusionReason,
    /// How many people this reason excluded. Always at least one — a reason
    /// that excluded nobody is not reported, because "0 unsubscribed" is noise
    /// on a screen whose job is to name what actually happened.
    pub people: i64,
}

/// What a segment answers: who it may mail, and who it may not, with reasons.
///
/// The item's rule, in a type: *the count and its exclusions are both readable;
/// a number without them is not auditable.* There is no field holding the
/// selected total, because a total stored beside its parts is a total that can
/// disagree with them — [`matched`](Self::matched) adds them up instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentTally {
    /// People this segment selected who have a consent record and no
    /// suppression — the honest number, the one a send is measured against.
    pub mailable: i64,
    /// Everybody else the conditions selected, grouped by why they are out,
    /// ordered so two reads agree. Only non-empty reasons appear.
    pub excluded: Vec<SegmentExclusion>,
}

impl SegmentTally {
    /// How many people the conditions selected, mailable or not.
    ///
    /// The denominator: `matched - mailable` is exactly the number of people a
    /// colleague expected to reach and will not, and every one of them is
    /// accounted for in [`excluded`](Self::excluded).
    pub fn matched(&self) -> i64 {
        self.mailable + self.excluded.iter().map(|e| e.people).sum::<i64>()
    }
}

/// The people this tenant has actually sold something to, and when.
///
/// *Issued* invoices only, and never credit notes: a draft is not a purchase
/// (it is an intention somebody may still delete), a void invoice is a purchase
/// that was cancelled, and a credit note is money going the other way. Counting
/// any of them would put people who bought nothing into a segment written to
/// reach customers.
///
/// The address is folded exactly as the audience folds its three sources, so
/// this CTE joins the same person the audience calls one person.
fn purchases_cte() -> &'static str {
    "purchases AS ( \
       SELECT DISTINCT lower(btrim(c.email)) AS address \
         FROM billing_invoices i \
         JOIN billing_customers c \
           ON c.tenant_id = i.tenant_id AND c.id = i.customer_id \
        WHERE i.tenant_id = $1 \
          AND i.status IN ('issued', 'paid') \
          AND i.is_credit_note = false \
          AND i.issue_date IS NOT NULL \
          AND c.email IS NOT NULL \
          AND ($3::date IS NULL OR i.issue_date >= $3::date) \
     )"
}

/// The purchase half of the `WHERE`, as an `EXISTS` in one direction or the
/// other.
///
/// `EXISTS` rather than `IN`/`NOT IN` on purpose: `address NOT IN (SELECT …)`
/// evaluates to `NULL` — not `true` — the moment the subquery yields a single
/// NULL row, which would silently empty a "has not bought" segment. The
/// subquery here cannot produce one today; the shape is chosen so that a later
/// change to `purchases` cannot make it lie.
fn purchase_predicate(purchase: Option<PurchaseWindow>) -> &'static str {
    match purchase.map(|w| w.condition) {
        None => "true",
        Some(PurchaseCondition::Bought) => {
            "EXISTS (SELECT 1 FROM purchases p WHERE p.address = people.address)"
        }
        Some(PurchaseCondition::NotBought) => {
            "NOT EXISTS (SELECT 1 FROM purchases p WHERE p.address = people.address)"
        }
    }
}

/// The people the conditions select — the audience, narrowed, and nothing else.
///
/// `SELECT * FROM people`, so consent and suppression arrive already joined and
/// a segment cannot reach a table the audience does not permit.
///
/// The country test is `country = ANY($2)`, which is `NULL` — and therefore not
/// a match — for the people no source gave a country. That is the intended
/// reading and the module docs say so: only billing customers have a country,
/// and somebody we cannot place is not evidence of being in Belgium.
fn matched_cte(conditions: &SegmentConditions) -> String {
    format!(
        "matched AS ( \
           SELECT * FROM people \
            WHERE ($2::text[] IS NULL OR country = ANY($2::text[])) \
              AND {purchase} \
         )",
        purchase = purchase_predicate(conditions.purchase),
    )
}

/// One page of a segment, at the given reach.
///
/// The cursor is folded by Postgres for the reason
/// [`normalise_address`](crate::normalise_address) documents, and bracketed for
/// the reason [`Reach`] documents: an unbracketed `OR` would bind the wrong way
/// round and hand the first page of a send to people who never consented.
fn segment_page_sql(conditions: &SegmentConditions, reach: Reach) -> String {
    format!(
        "{people}, {purchases}, {matched} \
         SELECT {columns} \
           FROM matched \
          WHERE ($4::text IS NULL OR address > lower(btrim($4::text))){reach} \
          ORDER BY address \
          LIMIT $5",
        people = people_cte(),
        purchases = purchases_cte(),
        matched = matched_cte(conditions),
        columns = MEMBER_COLUMNS,
        reach = reach.predicate(),
    )
}

/// The count and its exclusions, in one pass over the same rows.
///
/// One query rather than one per bucket, because two queries over a live
/// audience are two different moments: a form submitted between them would make
/// the parts disagree with the whole, and this number's entire value is that it
/// adds up.
///
/// The `CASE` is where the precedence lives — **suppression before consent** —
/// and it is the only place a person is assigned a bucket, so nobody can be
/// counted twice or missed. An unknown suppression reason produces a bucket
/// token this build cannot parse, and the read fails rather than quietly losing
/// those people from the total.
fn segment_tally_sql(conditions: &SegmentConditions) -> String {
    format!(
        "{people}, {purchases}, {matched} \
         SELECT CASE \
                  WHEN suppressed_at IS NOT NULL \
                    THEN '{suppressed}' || suppression_reason \
                  WHEN consented_at IS NULL THEN 'no_consent' \
                  ELSE '{mailable}' \
                END AS bucket, \
                count(*)::bigint AS headcount \
           FROM matched \
          GROUP BY 1 \
          ORDER BY 1",
        people = people_cte(),
        purchases = purchases_cte(),
        matched = matched_cte(conditions),
        suppressed = SUPPRESSED_BUCKET_PREFIX,
        mailable = MAILABLE_BUCKET,
    )
}

/// The columns a saved segment reads back as, in [`SegmentRow`]'s order.
const SEGMENT_COLUMNS: &str =
    "id, name, countries, purchase, purchase_within_days, created_by, created_at, updated_at";

fn insert_sql() -> String {
    format!(
        "INSERT INTO campaign_segments \
             (tenant_id, id, name, countries, purchase, purchase_within_days, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         RETURNING {SEGMENT_COLUMNS}"
    )
}

fn lookup_sql() -> String {
    format!(
        "SELECT {SEGMENT_COLUMNS} FROM campaign_segments \
          WHERE tenant_id = $1 AND id = $2"
    )
}

fn list_sql() -> String {
    format!(
        "SELECT {SEGMENT_COLUMNS} FROM campaign_segments \
          WHERE tenant_id = $1 \
          ORDER BY lower(btrim(name)), id \
          LIMIT $2"
    )
}

fn update_sql() -> String {
    format!(
        "UPDATE campaign_segments \
            SET name = $3, countries = $4, purchase = $5, purchase_within_days = $6, \
                updated_at = now() \
          WHERE tenant_id = $1 AND id = $2 \
         RETURNING {SEGMENT_COLUMNS}"
    )
}

fn delete_sql() -> &'static str {
    "DELETE FROM campaign_segments WHERE tenant_id = $1 AND id = $2 RETURNING id"
}

/// Every statement this module can issue against the database.
///
/// The same list [`crate::campaign_audience`], [`crate::campaign_consent`] and
/// [`crate::campaign_suppression`] keep. A segment is the most likely place for
/// the per-user address book to be reached for — "let me also offer their own
/// contacts" is a plausible-sounding feature request — so the promise is
/// checked against the strings here too.
#[cfg(test)]
fn all_sql() -> Vec<String> {
    let mut sql = vec![
        insert_sql(),
        lookup_sql(),
        list_sql(),
        update_sql(),
        delete_sql().to_owned(),
    ];
    for conditions in [
        SegmentConditions::default(),
        SegmentConditions {
            countries: vec!["BE".to_owned()],
            purchase: Some(PurchaseWindow {
                condition: PurchaseCondition::Bought,
                within_days: Some(90),
            }),
        },
        SegmentConditions {
            countries: Vec::new(),
            purchase: Some(PurchaseWindow {
                condition: PurchaseCondition::NotBought,
                within_days: None,
            }),
        },
    ] {
        sql.push(segment_tally_sql(&conditions));
        for reach in [Reach::Anyone, Reach::Mailable] {
            sql.push(segment_page_sql(&conditions, reach));
        }
    }
    sql
}

/// A saved segment as any of the CRUD statements returns it.
type SegmentRow = (
    String,
    String,
    Vec<String>,
    Option<String>,
    Option<i32>,
    String,
    OffsetDateTime,
    OffsetDateTime,
);

/// Turns a stored row into a segment, refusing a purchase condition this build
/// does not know.
///
/// A token we do not recognise is a decode failure rather than a dropped
/// condition. Dropping it would widen the segment — "customers who have not
/// bought in ninety days" would quietly become "everybody" — and the mistake
/// would arrive as mail rather than as an error.
fn row_to_segment(row: SegmentRow) -> Result<CampaignSegment> {
    let purchase = match (row.3.as_deref(), row.4) {
        (None, None) => None,
        (None, Some(_)) => {
            return Err(StoreError::Db(sqlx::Error::Decode(
                "a campaign segment stores a period with nothing to apply it to".into(),
            )));
        }
        (Some(token), within_days) => {
            let condition = PurchaseCondition::parse(token).ok_or_else(|| {
                StoreError::Db(sqlx::Error::Decode(
                    "a campaign segment names a purchase condition this build does not know".into(),
                ))
            })?;
            Some(PurchaseWindow {
                condition,
                within_days,
            })
        }
    };
    Ok(CampaignSegment {
        id: CampaignSegmentId::new(row.0),
        name: row.1,
        conditions: SegmentConditions {
            countries: row.2,
            purchase,
        },
        created_by: UserId::new(row.5),
        created_at: row.6,
        updated_at: row.7,
    })
}

/// A validated set of conditions, ready to bind.
struct ValidConditions {
    /// Normalised, sorted, de-duplicated — or `None` for no country condition,
    /// which is what the query's `$2::text[] IS NULL` branch tests for.
    countries: Option<Vec<String>>,
    purchase: Option<PurchaseWindow>,
}

/// Checks one set of conditions, in one place, without a database.
///
/// Country codes go through [`billing_field::country`] — the *same* rule
/// `billing_customers.country` was stored under. A segment validated by a
/// second, slightly different rule would be a segment that matches nobody for
/// reasons no screen could explain.
fn validate_conditions(conditions: &SegmentConditions) -> Result<ValidConditions> {
    if conditions.countries.len() > SEGMENT_COUNTRIES_MAX {
        return Err(StoreError::Validation(format!(
            "a segment names at most {SEGMENT_COUNTRIES_MAX} countries"
        )));
    }
    let mut countries = conditions
        .countries
        .iter()
        .map(|raw| billing_field::country(raw))
        .collect::<Result<Vec<String>>>()?;
    countries.sort_unstable();
    countries.dedup();

    if let Some(days) = conditions.purchase.and_then(|window| window.within_days)
        && !(1..=SEGMENT_PERIOD_DAYS_MAX).contains(&days)
    {
        return Err(StoreError::Validation(format!(
            "a purchase period is between 1 and {SEGMENT_PERIOD_DAYS_MAX} days"
        )));
    }

    Ok(ValidConditions {
        countries: (!countries.is_empty()).then_some(countries),
        purchase: conditions.purchase,
    })
}

/// Checks a name: present, trimmed, bounded.
fn validate_name(raw: &str) -> Result<String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(StoreError::Validation(
            "a segment needs a name somebody will recognise later".to_owned(),
        ));
    }
    if name.chars().count() > SEGMENT_NAME_MAX {
        return Err(StoreError::Validation(format!(
            "a segment name fits in {SEGMENT_NAME_MAX} characters"
        )));
    }
    Ok(name.to_owned())
}

/// The earliest issue date a purchase may carry to count, or `None` for "no
/// date bound" — either because there is no purchase condition or because the
/// condition is "ever".
///
/// Measured in whole days against the server's UTC date, because `issue_date`
/// is a `DATE` and has no time of day to be more precise than. "Bought in the
/// last ninety days" therefore means "issued on or after the date ninety days
/// ago", which is what a colleague means and what a screen can restate.
fn bought_since(purchase: Option<PurchaseWindow>, today: Date) -> Option<Date> {
    purchase?
        .within_days
        .map(|days| today - Duration::days(i64::from(days)))
}

/// Turns the tally's buckets into the answer, and refuses one that does not add
/// up to something this build can name.
fn rows_to_tally(rows: Vec<(String, i64)>) -> Result<SegmentTally> {
    let mut mailable = 0;
    let mut excluded = Vec::with_capacity(rows.len());
    for (bucket, headcount) in rows {
        if bucket == MAILABLE_BUCKET {
            mailable = headcount;
            continue;
        }
        let reason = ExclusionReason::parse(&bucket).ok_or_else(|| {
            StoreError::Db(sqlx::Error::Decode(
                "a segment tally named an exclusion this build does not know".into(),
            ))
        })?;
        excluded.push(SegmentExclusion {
            reason,
            people: headcount,
        });
    }
    excluded.sort_unstable_by_key(|e| e.reason);
    Ok(SegmentTally { mailable, excluded })
}

impl AccountStore {
    /// Saves a segment — the question, not the people it currently selects.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the name is blank or too long, a country
    /// is not a two-letter code, or the period is out of range;
    /// [`StoreError::Conflict`] when this tenant already has a segment of that
    /// name; [`StoreError::Db`] on failure.
    pub async fn create_campaign_segment(
        &self,
        segment: &NewCampaignSegment<'_>,
    ) -> Result<CampaignSegment> {
        let name = validate_name(segment.name)?;
        let valid = validate_conditions(&segment.conditions)?;
        let id = CampaignSegmentId::generate();
        let row: SegmentRow = sqlx::query_as(&insert_sql())
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(&name)
            .bind(valid.countries.unwrap_or_default())
            .bind(valid.purchase.map(|w| w.condition.as_str()))
            .bind(valid.purchase.and_then(|w| w.within_days))
            .bind(self.user.as_str())
            .fetch_one(&self.pool)
            .await
            .map_err(name_conflict)?;
        row_to_segment(row)
    }

    /// One saved segment, or `None` when this tenant has no such id.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn campaign_segment(
        &self,
        id: &CampaignSegmentId,
    ) -> Result<Option<CampaignSegment>> {
        let row: Option<SegmentRow> = sqlx::query_as(&lookup_sql())
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        row.map(row_to_segment).transpose()
    }

    /// This tenant's saved segments, by name.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when `limit` is outside
    /// `1..=`[`SEGMENT_PAGE_MAX`]; [`StoreError::Db`] on failure.
    pub async fn campaign_segments(&self, limit: i64) -> Result<Vec<CampaignSegment>> {
        if !(1..=SEGMENT_PAGE_MAX).contains(&limit) {
            return Err(StoreError::Validation(format!(
                "a page of segments is between 1 and {SEGMENT_PAGE_MAX}"
            )));
        }
        let rows: Vec<SegmentRow> = sqlx::query_as(&list_sql())
            .bind(self.tenant.as_str())
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        rows.into_iter().map(row_to_segment).collect()
    }

    /// Rewrites a saved segment's name and conditions.
    ///
    /// Whole-record rather than field-by-field: a segment is one sentence, and
    /// a partial update is how "customers in Belgium who have not bought" turns
    /// into "customers in Belgium" without anybody deciding it should.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the segment is absent or another tenant's;
    /// [`StoreError::Validation`] as for
    /// [`create_campaign_segment`](Self::create_campaign_segment);
    /// [`StoreError::Conflict`] on a duplicate name; [`StoreError::Db`] on
    /// failure.
    pub async fn update_campaign_segment(
        &self,
        id: &CampaignSegmentId,
        segment: &NewCampaignSegment<'_>,
    ) -> Result<CampaignSegment> {
        let name = validate_name(segment.name)?;
        let valid = validate_conditions(&segment.conditions)?;
        let row: Option<SegmentRow> = sqlx::query_as(&update_sql())
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(&name)
            .bind(valid.countries.unwrap_or_default())
            .bind(valid.purchase.map(|w| w.condition.as_str()))
            .bind(valid.purchase.and_then(|w| w.within_days))
            .fetch_optional(&self.pool)
            .await
            .map_err(name_conflict)?;
        row_to_segment(row.ok_or(StoreError::NotFound)?)
    }

    /// Forgets a saved segment.
    ///
    /// Deleting a segment deletes a question, never evidence: consent records
    /// and suppressions are separate tables and are untouched, so a tenant that
    /// tidies up its segments cannot lose the reason somebody may or may not be
    /// mailed.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the segment is absent or another tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_campaign_segment(&self, id: &CampaignSegmentId) -> Result<()> {
        let deleted: Option<String> = sqlx::query_scalar(delete_sql())
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        deleted.map(|_| ()).ok_or(StoreError::NotFound)
    }

    /// How many people a segment may mail, and who it selected but will not —
    /// each named with the reason.
    ///
    /// Takes the conditions rather than a saved id on purpose: the audience
    /// screen (C1.5) shows the count moving as a segment is *being* refined,
    /// before anybody presses save, and a segment that had to be saved to be
    /// counted would make every experiment a stored object somebody has to
    /// clean up. A caller holding a saved segment passes
    /// `&segment.conditions`.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on conditions that are not valid;
    /// [`StoreError::Db`] on failure, including a bucket this build cannot
    /// name — see [`SegmentTally`] for why that is an error rather than a
    /// rounded-off number.
    pub async fn campaign_segment_tally(
        &self,
        conditions: &SegmentConditions,
    ) -> Result<SegmentTally> {
        let valid = validate_conditions(conditions)?;
        let rows: Vec<(String, i64)> = sqlx::query_as(&segment_tally_sql(conditions))
            .bind(self.tenant.as_str())
            .bind(valid.countries)
            .bind(bought_since(
                valid.purchase,
                OffsetDateTime::now_utc().date(),
            ))
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        rows_to_tally(rows)
    }

    /// One page of the people a segment selects — **everybody**, mailable or
    /// not, each carrying their consent or the absence of it and their
    /// suppression or the absence of it.
    ///
    /// What the screen lists, because "excluded, and here is why" is the only
    /// version of a count that can be audited;
    /// [`ExclusionReason::for_member`] turns a member into the reason to print
    /// beside them.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on invalid conditions, a page size outside
    /// `1..=`[`AUDIENCE_PAGE_MAX`](crate::AUDIENCE_PAGE_MAX), or a cursor that
    /// is not an address; [`StoreError::Db`] on failure.
    pub async fn campaign_segment_members(
        &self,
        conditions: &SegmentConditions,
        page: &AudiencePage,
    ) -> Result<Vec<AudienceMember>> {
        let rows = self.segment_rows(conditions, page, Reach::Anyone).await?;
        rows.into_iter().map(MemberRow::into_member).collect()
    }

    /// One page of the people a segment may actually mail.
    ///
    /// The same query with the consent and suppression gates applied inside it
    /// (C1.2, C1.3) — a segment cannot widen who may be mailed, only narrow it,
    /// because the conditions are a `WHERE` over the audience's own CTEs rather
    /// than a source of their own.
    ///
    /// # Errors
    /// As [`campaign_segment_members`](Self::campaign_segment_members).
    pub async fn campaign_segment_recipients(
        &self,
        conditions: &SegmentConditions,
        page: &AudiencePage,
    ) -> Result<Vec<CampaignRecipient>> {
        let rows = self.segment_rows(conditions, page, Reach::Mailable).await?;
        rows.into_iter().map(MemberRow::into_recipient).collect()
    }

    /// The shared read behind both segment page methods: one validation, one
    /// bind order, one query builder.
    async fn segment_rows(
        &self,
        conditions: &SegmentConditions,
        page: &AudiencePage,
        reach: Reach,
    ) -> Result<Vec<MemberRow>> {
        let valid = validate_conditions(conditions)?;
        let after = page.validated_cursor()?;
        sqlx::query_as::<_, MemberRow>(&segment_page_sql(conditions, reach))
            .bind(self.tenant.as_str())
            .bind(valid.countries)
            .bind(bought_since(
                valid.purchase,
                OffsetDateTime::now_utc().date(),
            ))
            .bind(after)
            .bind(page.validated_limit()?)
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Db)
    }
}

/// Names the one uniqueness rule this table has, so a caller is told which
/// field to change rather than being handed "unique constraint".
fn name_conflict(error: sqlx::Error) -> StoreError {
    match StoreError::from(error) {
        StoreError::Conflict(_) => {
            StoreError::Conflict("this workspace already has a segment with that name".to_owned())
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The identifiers a SQL string contains — see the twin of this helper in
    /// `campaign_audience.rs` for why identifiers rather than substrings.
    fn identifiers(sql: &str) -> Vec<&str> {
        sql.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|token| !token.is_empty())
            .collect()
    }

    fn bought(within_days: Option<i32>) -> SegmentConditions {
        SegmentConditions {
            countries: vec!["BE".to_owned()],
            purchase: Some(PurchaseWindow {
                condition: PurchaseCondition::Bought,
                within_days,
            }),
        }
    }

    #[test]
    fn no_query_in_this_module_can_read_the_per_user_address_book() {
        // The likeliest place this promise breaks: a segment is where somebody
        // eventually asks for "and also my own contacts".
        for sql in all_sql() {
            assert!(
                !identifiers(&sql).contains(&"contacts"),
                "a campaign segment query names the per-user address book: {sql}"
            );
        }
    }

    /// The one statement whose tenancy is written rather than tested.
    ///
    /// An `INSERT` has no `WHERE` to carry `tenant_id = $1`, so its scoping is
    /// that `tenant_id` is its **first** column and `$1` its first bound value —
    /// the same `$1` every other statement in the module compares against.
    /// Asserting the shape rather than loosening the check to
    /// `sql.contains("tenant_id")` matters: a column list that merely mentions
    /// the tenant somewhere would pass the loose version while binding a value
    /// the caller chose.
    fn tenant_is_the_first_thing_written(sql: &str) -> bool {
        sql.starts_with("INSERT INTO campaign_segments (tenant_id, id, ")
            && sql.contains("VALUES ($1, $2, ")
    }

    #[test]
    fn every_query_is_scoped_to_one_tenant() {
        for sql in all_sql() {
            assert!(
                sql.contains("tenant_id = $1") || tenant_is_the_first_thing_written(&sql),
                "a segment query without a tenant: {sql}"
            );
        }
    }

    #[test]
    fn a_segment_reads_the_audience_rather_than_assembling_its_own() {
        // The whole reason `people_cte` is crate-visible. If a segment built
        // its own FROM, the privacy boundary, the consent join and the
        // suppression join would each have a second copy — and a second copy is
        // a place one of them is forgotten.
        let people = people_cte();
        for conditions in [SegmentConditions::default(), bought(Some(90))] {
            for sql in [
                segment_tally_sql(&conditions),
                segment_page_sql(&conditions, Reach::Anyone),
                segment_page_sql(&conditions, Reach::Mailable),
            ] {
                assert!(sql.contains(&people), "a segment built its own CTEs: {sql}");
                assert!(sql.contains("SELECT * FROM people"));
            }
        }
    }

    #[test]
    fn every_source_and_join_including_the_purchase_one_carries_a_tenant() {
        // Five from the audience (three sources, the consent join, the
        // suppression join) and one for the invoices a purchase condition
        // reads. Law 1: a neighbour's invoices must not decide who we may mail
        // any more than their customers do.
        for sql in [
            segment_tally_sql(&bought(Some(90))),
            segment_page_sql(&bought(Some(90)), Reach::Mailable),
        ] {
            assert_eq!(
                sql.matches("tenant_id = $1").count(),
                6,
                "every source and every join must carry a tenant predicate: {sql}"
            );
        }
    }

    #[test]
    fn a_segment_cannot_widen_who_may_be_mailed() {
        // A segment is a WHERE over the audience, so the consent and
        // suppression gates travel with it unchanged. The failure this
        // forbids is a "send to everyone in this segment" that reaches
        // somebody who unsubscribed because the segment query forgot the
        // predicate the audience query has.
        for conditions in [SegmentConditions::default(), bought(None)] {
            let mailable = segment_page_sql(&conditions, Reach::Mailable);
            assert!(
                mailable.contains("consented_at IS NOT NULL AND suppressed_at IS NULL"),
                "a segment's recipients are not gated: {mailable}"
            );
            let anyone = segment_page_sql(&conditions, Reach::Anyone);
            assert!(
                !anyone.contains("consented_at IS NOT NULL"),
                "a segment must be able to show who it excluded: {anyone}"
            );
        }
    }

    #[test]
    fn the_cursor_test_is_bracketed_so_the_gates_cannot_be_ored_away() {
        let sql = segment_page_sql(&bought(Some(30)), Reach::Mailable);
        assert!(
            sql.contains("WHERE ($4::text IS NULL OR address > lower(btrim($4::text)))"),
            "the cursor test lost its brackets: {sql}"
        );
    }

    #[test]
    fn a_draft_a_void_invoice_and_a_credit_note_are_not_purchases() {
        // "Customers who bought in the last ninety days" must not include
        // somebody whose invoice was cancelled, or whose draft nobody ever
        // issued. Both would be mail sent to a non-customer on the strength of
        // a document that says nothing.
        let sql = purchases_cte();
        assert!(sql.contains("i.status IN ('issued', 'paid')"));
        assert!(sql.contains("i.is_credit_note = false"));
        assert!(!sql.contains("'draft'"));
        assert!(!sql.contains("'void'"));
    }

    #[test]
    fn a_has_not_bought_segment_asks_it_as_an_exists_rather_than_a_not_in() {
        // `address NOT IN (SELECT …)` is NULL — not true — as soon as the
        // subquery yields one NULL, which would silently empty the segment.
        let not_bought = purchase_predicate(Some(PurchaseWindow {
            condition: PurchaseCondition::NotBought,
            within_days: Some(90),
        }));
        assert!(not_bought.starts_with("NOT EXISTS"));
        assert!(!not_bought.contains("NOT IN"));
        assert_eq!(purchase_predicate(None), "true");
    }

    #[test]
    fn a_country_condition_excludes_people_whose_country_is_unknown() {
        // Only billing customers carry a country. `country = ANY($2)` is NULL
        // for everybody else, and NULL is not a match — which is the intended
        // reading: a form submitter we cannot place is not evidence of being in
        // Belgium, and assuming them in is how a geographic offer reaches the
        // wrong country.
        let sql = segment_page_sql(&bought(Some(90)), Reach::Anyone);
        assert!(sql.contains("country = ANY($2::text[])"));
        assert!(
            sql.contains("$2::text[] IS NULL OR"),
            "an empty country list must mean 'no condition', not 'nobody': {sql}"
        );
    }

    #[test]
    fn the_tally_puts_each_person_in_exactly_one_bucket_and_suppression_wins() {
        // The precedence, in the one place it is decided. Somebody who never
        // consented *and* unsubscribed is reported as having unsubscribed: it
        // is the stronger fact, and it is the one a colleague cannot fix by
        // going and asking them.
        let sql = segment_tally_sql(&SegmentConditions::default());
        let suppressed = sql
            .find("WHEN suppressed_at IS NOT NULL")
            .unwrap_or_else(|| panic!("the tally lost its suppression bucket: {sql}"));
        let no_consent = sql
            .find("WHEN consented_at IS NULL")
            .unwrap_or_else(|| panic!("the tally lost its no-consent bucket: {sql}"));
        assert!(
            suppressed < no_consent,
            "consent is tested before suppression, so a suppressed person could be \
             reported as merely unconsented: {sql}"
        );
        assert!(sql.contains("GROUP BY 1"));
    }

    #[test]
    fn the_tally_names_the_reason_a_suppression_excluded_somebody() {
        // Not "12 suppressed": a screen that cannot say whether twelve people
        // unsubscribed or twelve mailboxes bounced cannot tell a tenant
        // anything worth knowing.
        let sql = segment_tally_sql(&SegmentConditions::default());
        assert!(sql.contains("'suppressed:' || suppression_reason"));
    }

    #[test]
    fn a_tally_adds_up_to_the_people_the_conditions_selected() {
        let tally = rows_to_tally(vec![
            ("mailable".to_owned(), 40),
            ("no_consent".to_owned(), 7),
            ("suppressed:unsubscribe".to_owned(), 2),
            ("suppressed:hard_bounce".to_owned(), 1),
        ])
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(tally.mailable, 40);
        assert_eq!(tally.matched(), 50);
        assert_eq!(
            tally.excluded,
            [
                SegmentExclusion {
                    reason: ExclusionReason::NoConsent,
                    people: 7
                },
                SegmentExclusion {
                    reason: ExclusionReason::Suppressed(SuppressionReason::Unsubscribe),
                    people: 2
                },
                SegmentExclusion {
                    reason: ExclusionReason::Suppressed(SuppressionReason::HardBounce),
                    people: 1
                },
            ]
        );
    }

    #[test]
    fn a_bucket_this_build_cannot_name_fails_the_tally_rather_than_shrinking_it() {
        // The alternative — skipping the bucket — is a count that no longer
        // adds up, reported as if it did. The whole value of this number is
        // that its parts account for its whole.
        for unknown in ["suppressed:changed_their_mind", "unknown", "suppressed:"] {
            assert!(
                matches!(
                    rows_to_tally(vec![("mailable".to_owned(), 1), (unknown.to_owned(), 3)]),
                    Err(StoreError::Db(_))
                ),
                "a tally silently dropped the bucket {unknown:?}"
            );
        }
    }

    #[test]
    fn a_tally_of_nobody_is_a_complete_answer() {
        let tally = rows_to_tally(Vec::new()).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(tally.mailable, 0);
        assert_eq!(tally.matched(), 0);
        assert!(tally.excluded.is_empty());
    }

    #[test]
    fn an_exclusion_token_survives_a_round_trip() {
        let reasons = [
            ExclusionReason::NoConsent,
            ExclusionReason::Suppressed(SuppressionReason::Unsubscribe),
            ExclusionReason::Suppressed(SuppressionReason::HardBounce),
            ExclusionReason::Suppressed(SuppressionReason::Complaint),
            ExclusionReason::Suppressed(SuppressionReason::Manual),
        ];
        for reason in reasons {
            assert_eq!(ExclusionReason::parse(&reason.token()), Some(reason));
        }
        for unknown in ["", "mailable", "consent", "suppressed", "suppressed:none"] {
            assert_eq!(ExclusionReason::parse(unknown), None);
        }
    }

    #[test]
    fn a_purchase_token_survives_a_round_trip_and_an_unknown_one_is_not_guessed() {
        for condition in [PurchaseCondition::Bought, PurchaseCondition::NotBought] {
            assert_eq!(
                PurchaseCondition::parse(condition.as_str()),
                Some(condition)
            );
        }
        for unknown in ["", "Bought", "never_bought", "contacts"] {
            assert_eq!(PurchaseCondition::parse(unknown), None);
        }
    }

    #[test]
    fn a_period_is_counted_back_from_today_and_ever_has_no_bound() {
        let today = Date::from_calendar_date(2026, time::Month::August, 17)
            .unwrap_or_else(|e| panic!("{e:?}"));
        let ninety = bought_since(
            Some(PurchaseWindow {
                condition: PurchaseCondition::NotBought,
                within_days: Some(90),
            }),
            today,
        );
        assert_eq!(ninety, Some(today - Duration::days(90)));
        // "Has never bought" is a real segment, and it has no date bound.
        assert_eq!(
            bought_since(
                Some(PurchaseWindow {
                    condition: PurchaseCondition::NotBought,
                    within_days: None,
                }),
                today
            ),
            None
        );
        // And no purchase condition at all binds nothing either.
        assert_eq!(bought_since(None, today), None);
    }

    #[test]
    fn a_country_is_folded_by_the_same_rule_a_customer_was_stored_under() {
        let valid = validate_conditions(&SegmentConditions {
            countries: vec![" be ".to_owned(), "NL".to_owned(), "Be".to_owned()],
            purchase: None,
        })
        .unwrap_or_else(|e| panic!("{e:?}"));
        // Uppercased, sorted, de-duplicated: `be` and `Be` are one country, and
        // two reads of the same segment produce the same array.
        assert_eq!(
            valid.countries.as_deref(),
            Some(["BE".to_owned(), "NL".to_owned()].as_slice())
        );
    }

    #[test]
    fn an_empty_country_list_is_the_absence_of_a_condition() {
        let valid =
            validate_conditions(&SegmentConditions::default()).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(valid.countries, None, "empty must not mean 'match nobody'");
    }

    #[test]
    fn a_country_that_is_not_one_is_refused_before_it_matches_nobody() {
        for junk in ["", "B", "BEL", "B1", "belgium"] {
            assert!(
                matches!(
                    validate_conditions(&SegmentConditions {
                        countries: vec![junk.to_owned()],
                        purchase: None,
                    }),
                    Err(StoreError::Validation(_))
                ),
                "accepted the country {junk:?}"
            );
        }
        let too_many = SegmentConditions {
            countries: vec!["BE".to_owned(); SEGMENT_COUNTRIES_MAX + 1],
            purchase: None,
        };
        assert!(matches!(
            validate_conditions(&too_many),
            Err(StoreError::Validation(_))
        ));
    }

    #[test]
    fn a_period_outside_the_range_is_refused() {
        for days in [0, -1, SEGMENT_PERIOD_DAYS_MAX + 1] {
            assert!(
                matches!(
                    validate_conditions(&bought(Some(days))),
                    Err(StoreError::Validation(_))
                ),
                "accepted a period of {days} days"
            );
        }
        assert!(validate_conditions(&bought(Some(1))).is_ok());
        assert!(validate_conditions(&bought(Some(SEGMENT_PERIOD_DAYS_MAX))).is_ok());
    }

    #[test]
    fn a_segment_needs_a_name_somebody_will_recognise() {
        for blank in ["", "   ", "\t\n"] {
            assert!(matches!(
                validate_name(blank),
                Err(StoreError::Validation(_))
            ));
        }
        assert_eq!(
            validate_name("  Belgian customers  ").unwrap_or_default(),
            "Belgian customers"
        );
        let long = "x".repeat(SEGMENT_NAME_MAX + 1);
        assert!(matches!(
            validate_name(&long),
            Err(StoreError::Validation(_))
        ));
        // A limit that refuses its own maximum is a different limit.
        assert!(validate_name(&"x".repeat(SEGMENT_NAME_MAX)).is_ok());
    }

    fn segment_row(purchase: Option<&str>, days: Option<i32>) -> SegmentRow {
        (
            "seg".to_owned(),
            "Belgian customers".to_owned(),
            vec!["BE".to_owned()],
            purchase.map(str::to_owned),
            days,
            "u1".to_owned(),
            OffsetDateTime::UNIX_EPOCH,
            OffsetDateTime::UNIX_EPOCH,
        )
    }

    #[test]
    fn a_stored_segment_reads_back_whole() {
        let stored = row_to_segment(segment_row(Some("not_bought"), Some(90)))
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(stored.id, CampaignSegmentId::new("seg"));
        assert_eq!(stored.conditions.countries, ["BE"]);
        assert_eq!(
            stored.conditions.purchase,
            Some(PurchaseWindow {
                condition: PurchaseCondition::NotBought,
                within_days: Some(90),
            })
        );
    }

    #[test]
    fn a_purchase_condition_this_build_cannot_read_fails_rather_than_widening() {
        // Dropping the condition would turn "has not bought in ninety days"
        // into "everybody", and the mistake would arrive as mail.
        assert!(matches!(
            row_to_segment(segment_row(Some("refunded"), Some(90))),
            Err(StoreError::Db(_))
        ));
        // A period with nothing to apply it to is half a condition, and the
        // half that is missing is the one that filters.
        assert!(matches!(
            row_to_segment(segment_row(None, Some(90))),
            Err(StoreError::Db(_))
        ));
        // The control: no purchase condition at all is an ordinary segment.
        assert!(row_to_segment(segment_row(None, None)).is_ok());
    }
}
