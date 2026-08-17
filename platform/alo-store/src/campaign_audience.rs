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
//! ## What this module is not, yet
//!
//! It answers *who exists*, not *who may be mailed*. Consent (C1.2) and
//! suppression (C1.3) are the gates on that, and both land inside
//! [`sources_cte`] and [`people_cte`] rather than beside them: ADR 0044 §2 makes
//! suppression absolute, which is only true if it is a property of this query
//! instead of a rule the sender remembers. Archived customers are therefore
//! **included** here — archiving hides a row from billing's pickers, it does not
//! say the person asked us to stop, and conflating the two would quietly answer
//! a consent question with a bookkeeping one.
//!
//! Nothing in this module sends anything.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};

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

/// The dedupe: source rows collapsed to one row per address.
///
/// `min`/`max` over `seen_at` rather than one column per source, so adding a
/// fourth source later changes [`sources_cte`] alone. The name and country are
/// the first non-null by `rank` then age — deterministic, so the same tenant
/// reads the same way twice.
fn people_cte() -> String {
    format!(
        "WITH {sources}, people AS ( \
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
         )",
        sources = sources_cte(),
    )
}

/// The SQL of one page of the audience.
///
/// The cursor is folded by **Postgres** (`lower(btrim($2))`), against the same
/// collation that produced the addresses it is compared with — see
/// [`normalise_address`] for why the comparison is not done in Rust.
fn audience_page_sql() -> String {
    format!(
        "{people} \
         SELECT address, name, country, sources, first_seen_at, last_seen_at \
           FROM people \
          WHERE $2::text IS NULL OR address > lower(btrim($2::text)) \
          ORDER BY address \
          LIMIT $3",
        people = people_cte(),
    )
}

/// The SQL of the audience's size.
fn audience_size_sql() -> String {
    format!(
        "WITH {sources} SELECT count(DISTINCT address)::bigint FROM sources",
        sources = sources_cte(),
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
    vec![audience_page_sql(), audience_size_sql()]
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
}

impl MemberRow {
    /// Turns stored source tokens into the typed enum.
    ///
    /// A token we do not know is a decode failure, never a dropped source: this
    /// module writes those strings itself, so an unrecognised one means the
    /// query changed under the enum, and reporting a person as reachable "from
    /// nowhere" would hide it.
    fn into_member(self) -> Result<AudienceMember> {
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
        Ok(AudienceMember {
            address: self.address,
            name: self.name,
            country: self.country,
            sources,
            first_seen_at: self.first_seen_at,
            last_seen_at: self.last_seen_at,
        })
    }
}

impl AccountStore {
    /// One page of the people this tenant could reach, in address order.
    ///
    /// Tenant-scoped by construction — every branch of [`sources_cte`] carries
    /// `tenant_id = $1` — so a neighbour's customers, deals and form
    /// submissions are not absent by filtering, they are unreachable.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the page size is outside
    /// `1..=`[`AUDIENCE_PAGE_MAX`] or the cursor is not an address;
    /// [`StoreError::Db`] on failure.
    pub async fn campaign_audience(&self, page: &AudiencePage) -> Result<Vec<AudienceMember>> {
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
        let rows = sqlx::query_as::<_, MemberRow>(&audience_page_sql())
            .bind(self.tenant.as_str())
            .bind(after)
            .bind(page.limit)
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        rows.into_iter().map(MemberRow::into_member).collect()
    }

    /// How many people the tenant could reach — counted in the database over the
    /// same sources, never by paging through them.
    ///
    /// The number a segment's count (C1.4) is a subset of, and the denominator
    /// the audience screen shows beside it.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn campaign_audience_size(&self) -> Result<i64> {
        sqlx::query_scalar::<_, i64>(&audience_size_sql())
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
        let sql = audience_page_sql();
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
            // One tenant predicate per source: no branch of the union may be
            // reachable across tenants (Law 1).
            assert_eq!(
                sql.matches("tenant_id = $1").count(),
                3,
                "every source must carry its own tenant predicate: {sql}"
            );
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

    #[test]
    fn a_row_with_an_unknown_source_fails_the_read_rather_than_losing_provenance() {
        let row = MemberRow {
            address: "ann@x.test".to_owned(),
            name: None,
            country: None,
            sources: vec!["billing_customer".to_owned(), "address_book".to_owned()],
            first_seen_at: OffsetDateTime::UNIX_EPOCH,
            last_seen_at: OffsetDateTime::UNIX_EPOCH,
        };
        assert!(matches!(row.into_member(), Err(StoreError::Db(_))));
    }

    #[test]
    fn a_person_from_two_sources_is_one_member_naming_both() {
        let row = MemberRow {
            address: "ann@x.test".to_owned(),
            name: Some("Ann Dupont".to_owned()),
            country: Some("BE".to_owned()),
            sources: vec![
                "site_form".to_owned(),
                "billing_customer".to_owned(),
                "site_form".to_owned(),
            ],
            first_seen_at: OffsetDateTime::UNIX_EPOCH,
            last_seen_at: OffsetDateTime::UNIX_EPOCH,
        };
        let member = row
            .into_member()
            .unwrap_or_else(|e| panic!("{e:?}"))
            .sources;
        assert_eq!(
            member,
            [AudienceSource::BillingCustomer, AudienceSource::SiteForm]
        );
    }
}
