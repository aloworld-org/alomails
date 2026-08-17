//! The reachable audience (alo Campaigns, ADR 0044, wave C1.1) — every person
//! this tenant could conceivably mail, assembled from records it already holds
//! rather than from a list somebody keeps in sync.
//!
//! ADR 0044's central claim is that **there is no list**: a segment is a
//! question asked of CRM, Billing and site events, so the same person cannot be
//! in two audiences, cannot be billed twice, and cannot unsubscribe from one
//! copy of themselves. This module is the bottom of that claim — the one place
//! that decides which tables a campaign may draw a person from.
//!
//! ## Three sources, and one that is forbidden
//!
//! Permitted, because each is **tenant-wide** — every colleague sees the same
//! rows, and the company as a company knows these people:
//!
//! - [`billing_customers`](crate::billing_customers) — somebody the tenant
//!   invoices;
//! - [`crm_deals`](crate::crm_deals) `contact_email` — the contact on an
//!   opportunity, carried as a column precisely because the whole team must see
//!   it;
//! - [`site_form_submissions`](crate::site_forms) `sender_email` — somebody who
//!   filled in a form on the tenant's published site.
//!
//! Forbidden: **`contacts`**. That table is keyed `(tenant_id, user_id, id)` —
//! it is one person's private address book, not the company's. A campaign drawn
//! from it would mail an employee's doctor, their landlord and their friends,
//! and it would look like the feature working rather than like a bug. That is a
//! privacy boundary rather than a preference, so it is not a filter a caller can
//! forget. It is proved twice: a unit test below walks every statement this
//! module can issue and asserts that none of them names the table — against the
//! strings themselves rather than by reading them — and
//! `tests/campaign_audience_tenancy.rs` proves the same at runtime, with a
//! contact that really exists and really is readable by its owner.
//!
//! ## What "one row per person" means
//!
//! The identity of a recipient is their **address, normalised** — trimmed and
//! lowercased ([`normalise_address`]). Somebody who is a customer, the contact
//! on two deals and a form submitter is one [`AudienceMember`] naming four
//! sources. RFC 5321 permits a case-sensitive local part; we fold it anyway,
//! because the failure the fold prevents (mailing `Ann@x.test` and `ann@x.test`
//! as two people, and honouring an unsubscribe for only one of them) is real,
//! while a provider that distinguishes them is a curiosity nobody sends bulk
//! mail to.
//!
//! Addresses that cannot be one are dropped rather than carried:
//! `crm_deals.contact_email` defaults to the empty string and every one of these
//! columns is free text a person typed. [`ADDRESS_SHAPE`] is the single rule,
//! applied in SQL so the **count** is right, and mirrored by
//! [`normalise_address`] so Rust and Postgres agree — a test holds them to it.
//!
//! ## Who exists, and who may be mailed
//!
//! Two different questions, and this module answers both — from the same SQL,
//! so they cannot disagree.
//!
//! - [`campaign_audience`](AccountStore::campaign_audience) is **who exists**:
//!   every person the tenant holds a record of, each carrying their consent or
//!   the absence of it. The audience screen (C1.5) needs the people it will not
//!   mail, because "excluded, and here is why" is the only version of a count
//!   that can be audited.
//! - [`campaign_recipients`](AccountStore::campaign_recipients) is **who may be
//!   mailed**: the same query with `consented_at IS NOT NULL AND suppressed_at
//!   IS NULL` applied *inside* it. ADR 0044 §2 says a campaign cannot be sent
//!   to somebody without a consent record and that suppression is absolute, and
//!   both are only true if they are properties of the query rather than filters
//!   every future caller remembers.
//!
//! The two return different types on purpose. A [`CampaignRecipient`] holds a
//! [`ConsentEvidence`], not an `Option<ConsentEvidence>`, and nothing
//! constructs one from an [`AudienceMember`] — so a sender that takes
//! recipients cannot be handed the audience by mistake, and code that has a
//! recipient in its hand is also holding the reason they are one.
//!
//! Suppression ([`campaign_suppression`](crate::campaign_suppression), C1.3)
//! sits in the same `WHERE`, and is the stronger of the two rules: an
//! unsubscribe, a hard bounce or a complaint removes somebody **whatever their
//! consent record says**, so an import that re-states an agreement cannot
//! resurrect them. The audience still shows them, carrying the reason, because
//! "excluded, and here is why" is the only version of a count that can be
//! audited — and a person who unsubscribed is still a customer the tenant
//! invoices.
//!
//! Archived customers are **included** in both — archiving hides a row from
//! billing's pickers, it does not say the person asked us to stop, and
//! conflating the two would quietly answer a consent question with a
//! bookkeeping one.
//!
//! Nothing in this module sends anything.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::campaign_consent::{ConsentEvidence, ConsentSource};
use crate::campaign_suppression::{SuppressionEvidence, SuppressionReason};
use crate::error::{Result, StoreError};
use crate::id::{CampaignConsentId, CampaignSuppressionId};

/// The shape an address must have to be a recipient at all, as a POSIX regular
/// expression Postgres and [`normalise_address`] both implement.
///
/// Deliberately structural rather than clever: something before an `@`,
/// something after it, at least one dot in the domain, and no whitespace
/// anywhere. It is not an RFC 5322 parser and does not pretend to be one — its
/// job is to keep `''`, `"n/a"` and `"ask reception"` out of a send, not to
/// adjudicate exotic-but-legal addresses. Anything that survives this is still
/// subject to consent and suppression before a message is built.
pub const ADDRESS_SHAPE: &str = r"^[^[:space:]@]+@[^[:space:]@.]+(\.[^[:space:]@.]+)+$";

/// The longest address this module will carry, matching the practical SMTP
/// ceiling (RFC 5321 §4.5.3.1: 64-octet local part, 255-octet domain, plus the
/// `@`). A longer string is not a truncated address, it is not an address.
pub const ADDRESS_MAX: usize = 320;

/// The most people one page of the audience may return.
///
/// A tenant's audience is unbounded, so it is read in pages rather than loaded
/// whole: the screen (C1.5) shows a window and the count, and nothing in this
/// crate ever holds an entire audience in memory.
pub const AUDIENCE_PAGE_MAX: i64 = 500;

/// Which kind of record holds a person.
///
/// Provenance rather than decoration: "how do we know this address" is the
/// question a complaint asks, and C1.2 records consent against the same three
/// origins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AudienceSource {
    /// A customer the tenant invoices ([`crate::billing_customers`]).
    BillingCustomer,
    /// The contact on a CRM deal ([`crate::crm_deals`]).
    CrmDeal,
    /// A submission through a form on the tenant's published site.
    SiteForm,
}

impl AudienceSource {
    /// The stored token, stable because it is written into consent records and
    /// read back by the screen.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BillingCustomer => "billing_customer",
            Self::CrmDeal => "crm_deal",
            Self::SiteForm => "site_form",
        }
    }

    /// Parses a stored token, or `None` when it is not one of ours.
    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "billing_customer" => Some(Self::BillingCustomer),
            "crm_deal" => Some(Self::CrmDeal),
            "site_form" => Some(Self::SiteForm),
            _ => None,
        }
    }
}

/// One person the tenant could reach, however many records hold them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudienceMember {
    /// The normalised address — the person's identity in this module.
    pub address: String,
    /// The best name any source offers, billing first, or `None` when every
    /// source left it blank. Never invented from the address: `"Hi
    /// j.dupont"` is worse than no greeting at all (C3.4 makes the fallback a
    /// save-time error rather than a send-time surprise).
    pub name: Option<String>,
    /// ISO 3166-1 alpha-2, from the billing customer that names one — the only
    /// source that has a country at all. `None` is *unknown*, which is why a
    /// country segment (C1.4) must exclude rather than assume.
    pub country: Option<String>,
    /// Every kind of record that holds this address, ascending and without
    /// repeats, so two reads of an unchanged tenant agree byte for byte.
    pub sources: Vec<AudienceSource>,
    /// When this address first entered the tenant's records.
    pub first_seen_at: OffsetDateTime,
    /// When it last did — the freshest evidence the person is still in the
    /// tenant's orbit.
    pub last_seen_at: OffsetDateTime,
    /// The freshest consent record for this address, or `None` when the tenant
    /// has no evidence this person agreed to anything.
    ///
    /// `None` is not a gap to be filled in later by whoever sends: it is the
    /// reason this person is not a [`CampaignRecipient`], and the audience
    /// screen names it as such (C1.5).
    pub consent: Option<ConsentEvidence>,
    /// Why this person may never be mailed again, or `None` when nothing has
    /// suppressed them.
    ///
    /// `Some` **overrides consent entirely** (ADR 0044 §2: absolute and
    /// tenant-wide), and it is carried here rather than merely acted on because
    /// the audience screen has to be able to say *who* was excluded and *why* —
    /// somebody who unsubscribed is still a customer the tenant invoices, and a
    /// count that quietly dropped them would be unauditable.
    pub suppression: Option<SuppressionEvidence>,
}

/// Somebody this tenant may actually mail.
///
/// The whole difference from [`AudienceMember`] is one field's type: consent is
/// present, not optional. There is no constructor that takes an
/// `Option<ConsentEvidence>` and no conversion from an audience member, so the
/// only way to hold one of these is to have read it out of
/// [`campaign_recipients`](AccountStore::campaign_recipients), which filters in
/// SQL. ADR 0044 §2's "a campaign cannot be sent to somebody without one" is
/// therefore a fact about the type a sender is handed, rather than a rule the
/// sender has to apply.
///
/// There is deliberately **no suppression field**: a suppressed person is not a
/// recipient at all, so the only honest value would be a permanent `None`, and
/// an `Option` a sender can read invites a sender that checks it — which is the
/// caller-applied rule C1.3 exists to abolish. The absence is enforced twice:
/// the query excludes them, and [`MemberRow::into_recipient`] refuses a row
/// that arrives carrying one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CampaignRecipient {
    /// The normalised address.
    pub address: String,
    /// The best name any source offers, or `None`.
    pub name: Option<String>,
    /// ISO 3166-1 alpha-2 where a billing customer names one.
    pub country: Option<String>,
    /// Every kind of record that holds this address.
    pub sources: Vec<AudienceSource>,
    /// How we know they agreed — the freshest record, and the id of the row to
    /// read for its statement.
    pub consent: ConsentEvidence,
}

/// A window onto the audience: keyset pagination by address.
///
/// Keyset rather than `OFFSET`, because the audience is a live query over three
/// moving tables — a form submitted between page two and page three shifts every
/// offset after it, which would silently skip somebody. An address is a stable
/// cursor: the same page boundary means the same thing however much lands behind
/// it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudiencePage {
    /// Return people strictly after this address, or from the start when
    /// `None`. Normalised before use, so a cursor echoed back from a screen in
    /// any casing lands where the caller means.
    pub after: Option<String>,
    /// How many people to return, `1..=`[`AUDIENCE_PAGE_MAX`].
    pub limit: i64,
}

impl Default for AudiencePage {
    fn default() -> Self {
        Self {
            after: None,
            limit: 100,
        }
    }
}

/// Trims and lowercases an address, returning `None` when the result is not a
/// plausible one under [`ADDRESS_SHAPE`].
///
/// The Rust half of the rule Postgres applies inside [`sources_cte`], and a test
/// asserts the database agrees with it case by case so the two cannot drift
/// apart unnoticed. It exists to **judge** an address — a page cursor, an
/// imported line — never to produce one a query is then compared against:
/// `lower()` is a collation's opinion and `to_lowercase` is Unicode's, and the
/// two need not agree on every alphabet. Folding a cursor here and comparing it
/// to a column folded there could skip a person, so [`audience_page_sql`] does
/// its own folding and this function only says yes or no.
pub fn normalise_address(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.len() > ADDRESS_MAX {
        return None;
    }
    let lowered = trimmed.to_lowercase();
    let (local, domain) = lowered.split_once('@')?;
    let plausible = !local.is_empty()
        && !local.contains(char::is_whitespace)
        && !domain.is_empty()
        && !domain.contains('@')
        && !domain.contains(char::is_whitespace)
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains("..")
        && domain.contains('.');
    plausible.then_some(lowered)
}

/// The three permitted sources, as one `UNION ALL` over `$1 = tenant_id`.
///
/// **This function is the privacy boundary.** Every read of the audience goes
/// through it, so the set of tables a campaign can draw a person from is this
/// text and nothing else — which is what makes "`contacts` is never a source"
/// a testable claim rather than a promise. Suppression (C1.3) and consent
/// (C1.2) join in here too, for the same reason: a rule applied further out is
/// a rule a future caller can skip.
///
/// `rank` orders the sources for the name and country a person ends up with:
/// the invoiced name is the one the tenant is surest of, a deal's contact next,
/// a form's self-reported name last.
fn sources_cte() -> String {
    format!(
        "sources AS ( \
           SELECT lower(btrim(c.email)) AS address, \
                  'billing_customer'::text AS source, \
                  1 AS rank, \
                  NULLIF(btrim(c.name), '') AS name, \
                  NULLIF(btrim(c.country), '') AS country, \
                  c.created_at AS seen_at \
             FROM billing_customers c \
            WHERE c.tenant_id = $1 \
              AND c.email IS NOT NULL \
              AND lower(btrim(c.email)) ~ '{shape}' \
              AND octet_length(btrim(c.email)) <= {max} \
           UNION ALL \
           SELECT lower(btrim(d.contact_email)), \
                  'crm_deal'::text, \
                  2, \
                  NULLIF(btrim(d.contact_name), ''), \
                  NULL, \
                  d.created_at \
             FROM crm_deals d \
            WHERE d.tenant_id = $1 \
              AND lower(btrim(d.contact_email)) ~ '{shape}' \
              AND octet_length(btrim(d.contact_email)) <= {max} \
           UNION ALL \
           SELECT lower(btrim(s.sender_email)), \
                  'site_form'::text, \
                  3, \
                  NULLIF(btrim(s.sender_name), ''), \
                  NULL, \
                  s.received_at \
             FROM site_form_submissions s \
            WHERE s.tenant_id = $1 \
              AND lower(btrim(s.sender_email)) ~ '{shape}' \
              AND octet_length(btrim(s.sender_email)) <= {max} \
         )",
        shape = ADDRESS_SHAPE,
        max = ADDRESS_MAX,
    )
}

/// The freshest consent record per address, from
/// [`campaign_consent`](crate::campaign_consent).
///
/// `DISTINCT ON` rather than a `max()` join, because the row we want is not
/// just the newest timestamp: it is the whole record behind it, id included, so
/// the caller can read the statement it was given for. The tie-break on `id`
/// makes two records sharing a timestamp resolve to the same one on every read
/// — a campaign that reported a different provenance on each refresh would be
/// evidence of nothing.
///
/// A `LEFT JOIN` onto this, never an inner one: the audience must be able to
/// show the people it may **not** mail, and the recipients query gets its
/// exclusion from an explicit `consented_at IS NOT NULL` instead (see
/// [`Reach`]).
fn consent_cte() -> &'static str {
    "consent AS ( \
       SELECT DISTINCT ON (address) \
              address, id AS consent_id, source AS consent_source, \
              occurred_at AS consented_at \
         FROM campaign_consent \
        WHERE tenant_id = $1 \
        ORDER BY address, occurred_at DESC, id DESC \
     )"
}

/// Who this tenant may never mail again, from
/// [`campaign_suppression`](crate::campaign_suppression).
///
/// No `DISTINCT ON` and no ordering, because that table is keyed
/// `(tenant_id, address)`: one person, one answer, decided by the schema rather
/// than by a window function that a later migration could quietly change the
/// meaning of.
///
/// A `LEFT JOIN` again, and for a sharper reason than consent's: a suppressed
/// person must still appear in the audience, carrying their reason. They are
/// usually somebody the tenant invoices — dropping them from the count of who
/// it holds records for would answer a mailing question with a bookkeeping one,
/// and the screen (C1.5) has to name them.
fn suppression_cte() -> &'static str {
    "suppression AS ( \
       SELECT address, id AS suppression_id, reason AS suppression_reason, \
              occurred_at AS suppressed_at \
         FROM campaign_suppression \
        WHERE tenant_id = $1 \
     )"
}

/// The dedupe: source rows collapsed to one row per address, with that person's
/// consent beside them.
///
/// `min`/`max` over `seen_at` rather than one column per source, so adding a
/// fourth source later changes [`sources_cte`] alone. The name and country are
/// the first non-null by `rank` then age — deterministic, so the same tenant
/// reads the same way twice.
///
/// Consent and suppression both join here, at the bottom, rather than in each
/// query above them: that is what lets [`Reach::Mailable`] be a predicate on two
/// columns instead of a rule four call sites have to remember.
fn people_cte() -> String {
    format!(
        "WITH {sources}, {consent}, {suppression}, grouped AS ( \
           SELECT address, \
                  min(seen_at) AS first_seen_at, \
                  max(seen_at) AS last_seen_at, \
                  array_agg(DISTINCT source ORDER BY source) AS sources, \
                  (array_agg(name ORDER BY rank, seen_at, name) \
                     FILTER (WHERE name IS NOT NULL))[1] AS name, \
                  (array_agg(country ORDER BY rank, seen_at, country) \
                     FILTER (WHERE country IS NOT NULL))[1] AS country \
             FROM sources \
            GROUP BY address \
         ), people AS ( \
           SELECT g.address, g.name, g.country, g.sources, \
                  g.first_seen_at, g.last_seen_at, \
                  c.consent_id, c.consent_source, c.consented_at, \
                  s.suppression_id, s.suppression_reason, s.suppressed_at \
             FROM grouped g \
             LEFT JOIN consent c ON c.address = g.address \
             LEFT JOIN suppression s ON s.address = g.address \
         )",
        sources = sources_cte(),
        consent = consent_cte(),
        suppression = suppression_cte(),
    )
}

/// How far a query reaches: everybody the tenant holds, or only the people it
/// may mail.
///
/// One enum instead of two hand-written query pairs, so "no consent record and
/// no suppression, or no send" is a single string that the page query and the
/// count query both build from and cannot drift apart on. A count that
/// disagreed with the list it counts is the failure this shape exists to make
/// impossible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// Everybody the tenant holds a record of — what the audience screen shows,
    /// exclusions included and each carrying its reason.
    Anyone,
    /// Only people with a consent record and no suppression (ADR 0044 §2).
    Mailable,
}

impl Reach {
    /// The predicate, always appended to an existing `WHERE`, so both queries
    /// apply it identically or not at all.
    ///
    /// Both halves in one string on purpose. They are two different rules —
    /// consent is permission the tenant was given, suppression is permission
    /// taken back and is the stronger of the two — but there is no query in
    /// this crate that wants one without the other, and offering the choice is
    /// how a "just the consented ones" call site eventually mails somebody who
    /// unsubscribed.
    fn predicate(self) -> &'static str {
        match self {
            Self::Anyone => "",
            Self::Mailable => " AND consented_at IS NOT NULL AND suppressed_at IS NULL",
        }
    }
}

/// The columns both reads select, in the order [`MemberRow`] declares them.
const MEMBER_COLUMNS: &str = "address, name, country, sources, first_seen_at, last_seen_at, \
                              consent_id, consent_source, consented_at, \
                              suppression_id, suppression_reason, suppressed_at";

/// The SQL of one page, at the given reach.
///
/// The cursor is folded by **Postgres** (`lower(btrim($2))`), against the same
/// collation that produced the addresses it is compared with — see
/// [`normalise_address`] for why the comparison is not done in Rust. The cursor
/// test is parenthesised: an unbracketed `OR` with a reach predicate `AND`ed
/// after it would bind the wrong way round and quietly return unconsented
/// people on the first page.
fn page_sql(reach: Reach) -> String {
    format!(
        "{people} \
         SELECT {columns} \
           FROM people \
          WHERE ($2::text IS NULL OR address > lower(btrim($2::text))){reach} \
          ORDER BY address \
          LIMIT $3",
        people = people_cte(),
        columns = MEMBER_COLUMNS,
        reach = reach.predicate(),
    )
}

/// The SQL of a count, at the given reach — counted in the database over the
/// same CTEs the page walks, so the number and the list are the same question.
fn count_sql(reach: Reach) -> String {
    format!(
        "{people} SELECT count(*)::bigint FROM people WHERE true{reach}",
        people = people_cte(),
        reach = reach.predicate(),
    )
}

/// Every statement this module can issue against the database.
///
/// The list exists so the promise at the top of this file is checked rather than
/// asserted: a test walks these strings for the name of a table no campaign may
/// read. A new query that forgets to appear here is caught by the same test
/// counting them.
#[cfg(test)]
fn all_sql() -> Vec<String> {
    vec![
        page_sql(Reach::Anyone),
        page_sql(Reach::Mailable),
        count_sql(Reach::Anyone),
        count_sql(Reach::Mailable),
    ]
}

/// A row as the page query returns it.
#[derive(sqlx::FromRow)]
struct MemberRow {
    address: String,
    name: Option<String>,
    country: Option<String>,
    sources: Vec<String>,
    first_seen_at: OffsetDateTime,
    last_seen_at: OffsetDateTime,
    consent_id: Option<String>,
    consent_source: Option<String>,
    consented_at: Option<OffsetDateTime>,
    suppression_id: Option<String>,
    suppression_reason: Option<String>,
    suppressed_at: Option<OffsetDateTime>,
}

impl MemberRow {
    /// Turns stored source tokens into the typed enum.
    ///
    /// A token we do not know is a decode failure, never a dropped source: this
    /// module writes those strings itself, so an unrecognised one means the
    /// query changed under the enum, and reporting a person as reachable "from
    /// nowhere" would hide it.
    fn into_member(self) -> Result<AudienceMember> {
        let sources = self.typed_sources()?;
        let consent = self.consent()?;
        let suppression = self.suppression()?;
        Ok(AudienceMember {
            address: self.address,
            name: self.name,
            country: self.country,
            sources,
            first_seen_at: self.first_seen_at,
            last_seen_at: self.last_seen_at,
            consent,
            suppression,
        })
    }

    /// The same row read as somebody who may be mailed.
    ///
    /// Missing consent here is a **decode error rather than a skipped row**.
    /// The query already excludes people without a record, so a row arriving
    /// without one means the filter and this type have come apart — and
    /// dropping the person quietly would turn that into a campaign that mails
    /// fewer people than its count promised, which nobody would report as a
    /// bug.
    fn into_recipient(self) -> Result<CampaignRecipient> {
        let sources = self.typed_sources()?;
        // Suppression is checked first and hardest. It is the stronger rule —
        // a suppressed person is not a recipient however good their consent
        // record is — and it is the one whose failure arrives at somebody who
        // has already asked us to stop.
        if let Some(suppression) = self.suppression()? {
            return Err(StoreError::Db(sqlx::Error::Decode(
                format!(
                    "the recipients query returned somebody suppressed as {}",
                    suppression.reason.as_str()
                )
                .into(),
            )));
        }
        let consent = self.consent()?.ok_or_else(|| {
            StoreError::Db(sqlx::Error::Decode(
                "the recipients query returned somebody with no consent record".into(),
            ))
        })?;
        Ok(CampaignRecipient {
            address: self.address,
            name: self.name,
            country: self.country,
            sources,
            consent,
        })
    }

    /// The source tokens as the typed enum, ascending and without repeats.
    fn typed_sources(&self) -> Result<Vec<AudienceSource>> {
        let mut sources = Vec::with_capacity(self.sources.len());
        for token in &self.sources {
            let source = AudienceSource::parse(token).ok_or_else(|| {
                StoreError::Db(sqlx::Error::Decode(
                    "campaign audience source is not a known source".into(),
                ))
            })?;
            sources.push(source);
        }
        sources.sort_unstable();
        sources.dedup();
        Ok(sources)
    }

    /// The joined consent, or `None` when this person has no record.
    ///
    /// The three columns come from one row of one table, so they are all
    /// present or all absent; a partial triple means the join changed shape and
    /// is reported rather than patched over with a default timestamp.
    fn consent(&self) -> Result<Option<ConsentEvidence>> {
        match (&self.consent_id, &self.consent_source, self.consented_at) {
            (None, None, None) => Ok(None),
            (Some(id), Some(token), Some(occurred_at)) => {
                let source = ConsentSource::parse(token).ok_or_else(|| {
                    StoreError::Db(sqlx::Error::Decode(
                        "campaign consent names a source this build does not know".into(),
                    ))
                })?;
                Ok(Some(ConsentEvidence {
                    record: CampaignConsentId::new(id.clone()),
                    source,
                    occurred_at,
                }))
            }
            _ => Err(StoreError::Db(sqlx::Error::Decode(
                "a campaign consent join returned half a record".into(),
            ))),
        }
    }

    /// The joined suppression, or `None` when nothing has suppressed this
    /// person.
    ///
    /// Same discipline as [`consent`](Self::consent), and the stakes are higher
    /// in one direction: half a triple here would be a person the tenant is
    /// told it may mail. Rather than guessing which half is right, the read
    /// fails.
    fn suppression(&self) -> Result<Option<SuppressionEvidence>> {
        match (
            &self.suppression_id,
            &self.suppression_reason,
            self.suppressed_at,
        ) {
            (None, None, None) => Ok(None),
            (Some(id), Some(token), Some(occurred_at)) => {
                let reason = SuppressionReason::parse(token).ok_or_else(|| {
                    StoreError::Db(sqlx::Error::Decode(
                        "campaign suppression names a reason this build does not know".into(),
                    ))
                })?;
                Ok(Some(SuppressionEvidence {
                    record: CampaignSuppressionId::new(id.clone()),
                    reason,
                    occurred_at,
                }))
            }
            _ => Err(StoreError::Db(sqlx::Error::Decode(
                "a campaign suppression join returned half a record".into(),
            ))),
        }
    }
}

impl AccountStore {
    /// One page of the people this tenant holds a record of, in address order,
    /// each carrying their consent or the absence of it.
    ///
    /// **Who exists, not who may be mailed** — see
    /// [`campaign_recipients`](Self::campaign_recipients) for that, and note
    /// that these return different types precisely so the distinction cannot be
    /// lost in a call. A member whose `consent` is `None`, or whose
    /// `suppression` is `Some`, is a person this tenant may not send to — shown
    /// with the reason, because a count with no visible exclusions is not
    /// auditable.
    ///
    /// Tenant-scoped by construction — every branch of [`sources_cte`], the
    /// consent join and the suppression join all carry `tenant_id = $1` — so a
    /// neighbour's customers, deals, form submissions, consent records and
    /// suppressions are not absent by filtering, they are unreachable.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the page size is outside
    /// `1..=`[`AUDIENCE_PAGE_MAX`] or the cursor is not an address;
    /// [`StoreError::Db`] on failure.
    pub async fn campaign_audience(&self, page: &AudiencePage) -> Result<Vec<AudienceMember>> {
        let rows = self.audience_rows(page, Reach::Anyone).await?;
        rows.into_iter().map(MemberRow::into_member).collect()
    }

    /// How many people the tenant holds a record of — counted in the database
    /// over the same CTEs, never by paging through them.
    ///
    /// The denominator the audience screen shows beside the number it may
    /// actually mail.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn campaign_audience_size(&self) -> Result<i64> {
        self.audience_count(Reach::Anyone).await
    }

    /// One page of the people this tenant **may mail**, in address order.
    ///
    /// Two exclusions, both inside the query (ADR 0044 §2). `consented_at IS
    /// NOT NULL`: somebody with no consent record is not filtered out
    /// afterwards, they are not in the result set, and a caller that forgets to
    /// check cannot reach them because the type it gets back carries the
    /// evidence rather than an `Option` of it. `suppressed_at IS NULL`:
    /// suppression is absolute and tenant-wide, so an unsubscribe, a hard
    /// bounce or a complaint removes somebody here whatever their consent
    /// record says — and no import that re-states an agreement can bring them
    /// back, because the newer consent row does not touch the suppression join.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the page size is outside
    /// `1..=`[`AUDIENCE_PAGE_MAX`] or the cursor is not an address;
    /// [`StoreError::Db`] on failure.
    pub async fn campaign_recipients(&self, page: &AudiencePage) -> Result<Vec<CampaignRecipient>> {
        let rows = self.audience_rows(page, Reach::Mailable).await?;
        rows.into_iter().map(MemberRow::into_recipient).collect()
    }

    /// How many people the tenant may mail.
    ///
    /// The honest number: the one a campaign's cost, its warm-up and its
    /// complaint rate are all measured against, and never larger than
    /// [`campaign_audience_size`](Self::campaign_audience_size).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn campaign_recipient_count(&self) -> Result<i64> {
        self.audience_count(Reach::Mailable).await
    }

    /// The shared read behind both page methods: one validation of the page,
    /// one bind order, one query builder.
    async fn audience_rows(&self, page: &AudiencePage, reach: Reach) -> Result<Vec<MemberRow>> {
        if page.limit < 1 || page.limit > AUDIENCE_PAGE_MAX {
            return Err(StoreError::Validation(format!(
                "a page of the audience is between 1 and {AUDIENCE_PAGE_MAX} people"
            )));
        }
        // A cursor that is not an address would silently return page one, which
        // reads as "the audience restarted" rather than as the mistake it is.
        let after = match page.after.as_deref() {
            None => None,
            Some(raw) => Some(normalise_address(raw).ok_or_else(|| {
                StoreError::Validation(
                    "the page cursor is not an address this audience could contain".to_owned(),
                )
            })?),
        };
        sqlx::query_as::<_, MemberRow>(&page_sql(reach))
            .bind(self.tenant.as_str())
            .bind(after)
            .bind(page.limit)
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Db)
    }

    /// The shared count behind both count methods.
    async fn audience_count(&self, reach: Reach) -> Result<i64> {
        sqlx::query_scalar::<_, i64>(&count_sql(reach))
            .bind(self.tenant.as_str())
            .fetch_one(&self.pool)
            .await
            .map_err(StoreError::Db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The identifiers a SQL string contains, split on everything that cannot be
    /// part of one. `contact_email` is one identifier and is not `contacts`,
    /// which is exactly the distinction a substring search would get wrong.
    fn identifiers(sql: &str) -> Vec<&str> {
        sql.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|token| !token.is_empty())
            .collect()
    }

    #[test]
    fn no_query_in_this_module_can_read_the_per_user_address_book() {
        // The promise of ADR 0044's audience, checked against the SQL itself:
        // `contacts` is somebody's private address book, and a company campaign
        // drawn from it is a privacy breach that looks like a working feature.
        for sql in all_sql() {
            assert!(
                !identifiers(&sql).contains(&"contacts"),
                "a campaign audience query names the per-user address book: {sql}"
            );
        }
    }

    #[test]
    fn a_column_that_merely_mentions_a_contact_is_not_the_contacts_table() {
        // Guards the guard: the test above must keep passing for the right
        // reason, and `crm_deals.contact_email` is a source we depend on.
        let sql = page_sql(Reach::Anyone);
        assert!(identifiers(&sql).contains(&"contact_email"));
        assert!(identifiers(&sql).contains(&"contact_name"));
        assert!(!identifiers(&sql).contains(&"contacts"));
        assert!(identifiers("select x from contacts").contains(&"contacts"));
    }

    #[test]
    fn every_query_names_the_three_permitted_sources_and_scopes_them_to_a_tenant() {
        for sql in all_sql() {
            let names = identifiers(&sql);
            for table in ["billing_customers", "crm_deals", "site_form_submissions"] {
                assert!(names.contains(&table), "{table} missing from: {sql}");
            }
            // One tenant predicate per source, plus one on the consent join
            // and one on the suppression join: nothing in this query may be
            // reachable across tenants (Law 1). A neighbour's consent record
            // must not make our address mailable any more than their customer
            // list does — and their suppression must not silence ours, which
            // would be the same leak wearing the opposite sign.
            assert_eq!(
                sql.matches("tenant_id = $1").count(),
                5,
                "every source and both joins must carry a tenant predicate: {sql}"
            );
        }
    }

    #[test]
    fn only_the_recipients_queries_exclude_people_with_no_consent_record() {
        // ADR 0044 §2, as a property of the SQL: the exclusion is in the query
        // the sender reads from, and it is *not* in the audience query, which
        // has to show who was left out and why.
        for consented in [page_sql(Reach::Mailable), count_sql(Reach::Mailable)] {
            assert!(
                consented.contains("consented_at IS NOT NULL"),
                "a recipients query without the consent gate: {consented}"
            );
        }
        for everyone in [page_sql(Reach::Anyone), count_sql(Reach::Anyone)] {
            assert!(
                !everyone.contains("consented_at IS NOT NULL"),
                "the audience must show its exclusions: {everyone}"
            );
        }
    }

    #[test]
    fn the_recipients_queries_exclude_suppressed_people_in_sql() {
        // C1.3, and the whole of it: "if the sender applies the rule, it is not
        // absolute". So the rule is in the string the sender's query is built
        // from, and there is no reach that has consent without suppression —
        // one predicate, both halves, or the choice itself becomes the bug.
        for mailable in [page_sql(Reach::Mailable), count_sql(Reach::Mailable)] {
            assert!(
                mailable.contains("consented_at IS NOT NULL AND suppressed_at IS NULL"),
                "a recipients query that can be sent to somebody who unsubscribed: {mailable}"
            );
        }
        // And the audience keeps them, with the reason: a suppressed person is
        // usually still a customer, and a count that dropped them quietly could
        // not be audited.
        for everyone in [page_sql(Reach::Anyone), count_sql(Reach::Anyone)] {
            assert!(
                !everyone.contains("suppressed_at IS NULL"),
                "the audience must show who was suppressed: {everyone}"
            );
        }
        assert!(
            page_sql(Reach::Anyone).contains("suppression_reason"),
            "the audience must be able to say why somebody was excluded"
        );
    }

    #[test]
    fn nothing_but_a_suppression_row_can_suppress() {
        // The join is on the suppression table alone, tenant-scoped, and the
        // audience never derives suppression from anything else — an archived
        // customer, a stale deal or a bounced invoice are bookkeeping, not a
        // person asking to be left alone (see the module docs).
        let sql = page_sql(Reach::Mailable);
        assert!(identifiers(&sql).contains(&"campaign_suppression"));
        assert_eq!(
            sql.matches("campaign_suppression").count(),
            1,
            "suppression comes from one place, or it is not absolute: {sql}"
        );
    }

    #[test]
    fn the_cursor_test_is_bracketed_so_the_consent_gate_cannot_be_ored_away() {
        // `WHERE a OR b AND c` binds as `a OR (b AND c)`: without the brackets,
        // the first page of the recipients — the one with no cursor — would
        // return everybody, consent or not. The bug would be invisible in any
        // test that starts by reading page one of a fully-consented tenant.
        let sql = page_sql(Reach::Mailable);
        assert!(
            sql.contains("WHERE ($2::text IS NULL OR address > lower(btrim($2::text)))"),
            "the cursor test lost its brackets: {sql}"
        );
    }

    #[test]
    fn a_count_and_the_page_it_counts_are_the_same_question() {
        // Both are built from `people_cte`, so a source added to one is added
        // to the other. A count over a different set is how "1 200 recipients"
        // becomes 900 sends.
        let people = people_cte();
        for sql in all_sql() {
            assert!(sql.contains(&people), "a query built its own CTEs: {sql}");
        }
    }

    #[test]
    fn an_address_is_trimmed_and_folded_to_one_identity() {
        assert_eq!(
            normalise_address("  Ann.Dupont@Example.TEST "),
            Some("ann.dupont@example.test".to_owned())
        );
        // The failure the fold prevents: one person, mailed once.
        assert_eq!(
            normalise_address("ANN@x.test"),
            normalise_address("ann@x.test")
        );
    }

    #[test]
    fn what_is_not_an_address_is_not_a_recipient() {
        for junk in [
            "",
            "   ",
            "n/a",
            "ask reception",
            "@example.test",
            "ann@",
            "ann@localhost",
            "ann@.test",
            "ann@example.",
            "ann@example..test",
            "ann example@x.test",
            "ann@ex ample.test",
            "ann@@example.test",
            "ann@example.test extra",
        ] {
            assert_eq!(normalise_address(junk), None, "accepted {junk:?}");
        }
    }

    #[test]
    fn an_address_longer_than_smtp_allows_is_refused() {
        let long = format!("{}@example.test", "a".repeat(ADDRESS_MAX));
        assert_eq!(normalise_address(&long), None);
        let just_fits = format!("{}@example.test", "a".repeat(ADDRESS_MAX - 13));
        assert_eq!(just_fits.len(), ADDRESS_MAX);
        assert!(normalise_address(&just_fits).is_some());
    }

    #[test]
    fn a_source_token_survives_a_round_trip_and_an_unknown_one_is_not_guessed() {
        for source in [
            AudienceSource::BillingCustomer,
            AudienceSource::CrmDeal,
            AudienceSource::SiteForm,
        ] {
            assert_eq!(AudienceSource::parse(source.as_str()), Some(source));
        }
        for unknown in ["contacts", "", "BillingCustomer", "imported"] {
            assert_eq!(AudienceSource::parse(unknown), None);
        }
    }

    /// A row as the page query would return it, with no consent joined.
    fn member_row(sources: &[&str]) -> MemberRow {
        MemberRow {
            address: "ann@x.test".to_owned(),
            name: None,
            country: None,
            sources: sources.iter().map(|s| (*s).to_owned()).collect(),
            first_seen_at: OffsetDateTime::UNIX_EPOCH,
            last_seen_at: OffsetDateTime::UNIX_EPOCH,
            consent_id: None,
            consent_source: None,
            consented_at: None,
            suppression_id: None,
            suppression_reason: None,
            suppressed_at: None,
        }
    }

    /// The same row, with consent joined — somebody who would be a recipient.
    fn consented_row(sources: &[&str]) -> MemberRow {
        MemberRow {
            consent_id: Some("cns".to_owned()),
            consent_source: Some("site_form".to_owned()),
            consented_at: Some(OffsetDateTime::UNIX_EPOCH),
            ..member_row(sources)
        }
    }

    #[test]
    fn a_row_with_an_unknown_source_fails_the_read_rather_than_losing_provenance() {
        let row = member_row(&["billing_customer", "address_book"]);
        assert!(matches!(row.into_member(), Err(StoreError::Db(_))));
    }

    #[test]
    fn a_person_from_two_sources_is_one_member_naming_both() {
        let mut row = member_row(&["site_form", "billing_customer", "site_form"]);
        row.name = Some("Ann Dupont".to_owned());
        row.country = Some("BE".to_owned());
        let member = row
            .into_member()
            .unwrap_or_else(|e| panic!("{e:?}"))
            .sources;
        assert_eq!(
            member,
            [AudienceSource::BillingCustomer, AudienceSource::SiteForm]
        );
    }

    #[test]
    fn a_person_with_no_consent_record_is_in_the_audience_and_is_not_a_recipient() {
        // The two answers this module gives about the same person, at the type
        // level: they exist, and they may not be mailed.
        let member = member_row(&["billing_customer"])
            .into_member()
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(member.consent, None);
        assert!(matches!(
            member_row(&["billing_customer"]).into_recipient(),
            Err(StoreError::Db(_))
        ));
    }

    #[test]
    fn a_recipient_carries_the_evidence_they_are_one() {
        let mut row = member_row(&["site_form"]);
        row.consent_id = Some("cns".to_owned());
        row.consent_source = Some("site_form".to_owned());
        row.consented_at = Some(OffsetDateTime::UNIX_EPOCH);
        let recipient = row.into_recipient().unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(
            recipient.consent,
            ConsentEvidence {
                record: CampaignConsentId::new("cns"),
                source: ConsentSource::SiteForm,
                occurred_at: OffsetDateTime::UNIX_EPOCH,
            }
        );
    }

    #[test]
    fn half_a_consent_record_is_reported_rather_than_completed() {
        // All three columns come from one row of one table. A partial triple
        // means the join changed shape, and inventing the missing part — a
        // default timestamp, an "unknown" source — would put a person into a
        // send with provenance we made up.
        let mut row = member_row(&["crm_deal"]);
        row.consent_id = Some("cns".to_owned());
        assert!(matches!(row.into_member(), Err(StoreError::Db(_))));

        let mut row = member_row(&["crm_deal"]);
        row.consent_source = Some("import".to_owned());
        row.consented_at = Some(OffsetDateTime::UNIX_EPOCH);
        assert!(matches!(row.into_member(), Err(StoreError::Db(_))));
    }

    #[test]
    fn a_consent_source_this_build_does_not_know_fails_the_read() {
        let mut row = member_row(&["billing_customer"]);
        row.consent_id = Some("cns".to_owned());
        row.consent_source = Some("assumed".to_owned());
        row.consented_at = Some(OffsetDateTime::UNIX_EPOCH);
        assert!(matches!(row.into_member(), Err(StoreError::Db(_))));
    }

    /// A row for somebody who consented and was later suppressed — the exact
    /// shape an import that "re-confirmed" a person who had unsubscribed would
    /// produce.
    fn suppressed_row(reason: &str) -> MemberRow {
        MemberRow {
            suppression_id: Some("sup".to_owned()),
            suppression_reason: Some(reason.to_owned()),
            suppressed_at: Some(OffsetDateTime::UNIX_EPOCH),
            ..consented_row(&["billing_customer"])
        }
    }

    #[test]
    fn a_suppressed_person_is_in_the_audience_carrying_the_reason() {
        let member = suppressed_row("unsubscribe")
            .into_member()
            .unwrap_or_else(|e| panic!("{e:?}"));
        let Some(suppression) = member.suppression else {
            panic!("the audience must say why somebody was excluded")
        };
        assert_eq!(suppression.reason, SuppressionReason::Unsubscribe);
        assert_eq!(suppression.record, CampaignSuppressionId::new("sup"));
        // And they still carry their consent, because the record is real. The
        // exclusion is not a claim that they never agreed — it is a stronger
        // fact that arrived afterwards.
        assert!(member.consent.is_some());
    }

    #[test]
    fn a_suppressed_person_can_never_be_read_as_a_recipient() {
        // The second of the two places C1.3's rule lives. The query already
        // excludes them; if a row arrives anyway — a rewritten `Reach`, a hand
        // -written query, a future join that drops the predicate — the read
        // fails rather than handing a sender somebody who asked to stop. Every
        // reason, because "we only forgot the bounces" is how this returns.
        for reason in ["unsubscribe", "hard_bounce", "complaint", "manual"] {
            assert!(
                matches!(
                    suppressed_row(reason).into_recipient(),
                    Err(StoreError::Db(_))
                ),
                "a {reason} was handed to a sender"
            );
        }
        // The control: without the suppression, the same row is a recipient.
        assert!(
            consented_row(&["billing_customer"])
                .into_recipient()
                .is_ok()
        );
    }

    #[test]
    fn half_a_suppression_record_is_reported_rather_than_completed() {
        // Guessing here is worse than guessing at consent: the half we would
        // have to invent is "there is no suppression", and that is a person the
        // tenant is told it may mail.
        let mut row = consented_row(&["crm_deal"]);
        row.suppression_id = Some("sup".to_owned());
        assert!(matches!(row.into_member(), Err(StoreError::Db(_))));

        let mut row = consented_row(&["crm_deal"]);
        row.suppression_reason = Some("complaint".to_owned());
        row.suppressed_at = Some(OffsetDateTime::UNIX_EPOCH);
        assert!(matches!(row.into_member(), Err(StoreError::Db(_))));
    }

    #[test]
    fn a_suppression_reason_this_build_does_not_know_fails_the_read() {
        assert!(matches!(
            suppressed_row("changed_their_mind").into_member(),
            Err(StoreError::Db(_))
        ));
        // And it fails on the recipients path too, rather than being read as
        // "no suppression" and mailed.
        assert!(matches!(
            suppressed_row("changed_their_mind").into_recipient(),
            Err(StoreError::Db(_))
        ));
    }
}
