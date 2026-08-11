//! The jobs a tenant is hiring for (alo HR, ADR 0035, wave B6.06a;
//! `docs/design/hr.md`, "Recruitment-lite").
//!
//! An opening is a small record with a state machine of exactly two
//! transitions — **publish** and **close** — and both are a person's act
//! through an audited route. It carries what a candidate reads on the
//! advertisement (the title, the team, where the work is, the terms on offer)
//! and nothing about anybody: the people are [`crate::hr_applicants`], and the
//! separation is what lets a hiring screen list openings for a tenant that has
//! not yet decided who may see candidates.
//!
//! # `closed` is terminal
//!
//! There is no reopen. A role a company decides to hire for again is next
//! year's opening with next year's dates; a reopened one would silently lose
//! the first round's `opened_on`, and the second round's applicants would sit
//! in the same pipeline as the ones who were turned down months ago. Closing is
//! the only removal — an opening is never deleted, because the applicants filed
//! against it are the record of what happened, and deleting the parent would
//! take them with it.
//!
//! # Team and location are free text
//!
//! Matching `hr_employees.team`: a tenant's teams are their own vocabulary, and
//! a table of them would be a second directory to keep in step with the first.
//! The terms on offer are *not* free text — they come from the same closed
//! vocabulary an employment uses ([`ContractKind`]), so an opening that becomes
//! a hire does not change words on the way.

use time::{Date, OffsetDateTime};

use crate::billing_field::{bounded, required};
use crate::error::{Result, StoreError};
use crate::hr_employments::ContractKind;
use crate::id::{HrOpeningId, UserId};
use crate::store::TenantStore;

/// The longest an opening's title may be — a line on a job board, not a
/// paragraph.
pub const OPENING_TITLE_MAX_CHARS: usize = 200;

/// The longest a team or a location may be. Both are the tenant's own words
/// ("Warehouse — Rotterdam", "remote (EU)"), bounded where a list stays
/// legible.
pub const OPENING_FIELD_MAX_CHARS: usize = 120;

/// Where an opening is in its short life.
///
/// A closed vocabulary matched by the CHECK one layer down. Widening it is a
/// design change (`docs/design/hr.md`), not a schema tweak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpeningStatus {
    /// Written down, not advertised. Editable, and the state an opening is
    /// created in — a company writes the role before it decides to run it.
    Draft,
    /// Published: the round is running and applications are expected.
    Open,
    /// The round is over, however it ended. Terminal (see the module docs).
    Closed,
}

impl OpeningStatus {
    /// The stored word.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }

    /// Reads a status — from a query filter or from a stored row.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set — the caller can fix
    /// it, and the list is short enough to be the whole message.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "draft" => Ok(Self::Draft),
            "open" => Ok(Self::Open),
            "closed" => Ok(Self::Closed),
            _ => Err(StoreError::Validation(
                "opening status must be one of: draft, open, closed".to_owned(),
            )),
        }
    }
}

impl std::fmt::Display for OpeningStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The writable shape of an opening: what a company puts in the advertisement.
#[derive(Debug, Clone)]
pub struct NewOpening {
    /// The role as it is advertised. Required.
    pub title: String,
    /// Which part of the company it is in. Blank is ordinary.
    pub team: String,
    /// Where the work is. Blank is ordinary.
    pub location: String,
    /// The terms on offer.
    pub employment_kind: ContractKind,
}

impl Default for NewOpening {
    /// A permanent role with nothing else stated — the shape most openings
    /// have, so a caller states only what differs.
    fn default() -> Self {
        Self {
            title: String::new(),
            team: String::new(),
            location: String::new(),
            employment_kind: ContractKind::Permanent,
        }
    }
}

/// One stored opening, with the size of its pipeline.
///
/// `applicants` is counted in the same read rather than left to the caller: a
/// hiring screen shows the openings with how many people are in each, and a
/// count per row would be a query per row.
#[derive(Debug, Clone)]
pub struct Opening {
    /// Opaque id, unique within the tenant.
    pub id: HrOpeningId,
    /// The role as it is advertised.
    pub title: String,
    /// Which part of the company it is in.
    pub team: String,
    /// Where the work is.
    pub location: String,
    /// The terms on offer.
    pub employment_kind: ContractKind,
    /// Where it is in its life.
    pub status: OpeningStatus,
    /// The day it was published, when it ever was.
    pub opened_on: Option<Date>,
    /// The day the round was closed.
    pub closed_on: Option<Date>,
    /// How many people have applied — every stage, including the ones who were
    /// turned down.
    pub applicants: i64,
    /// Who wrote it down.
    pub created_by: UserId,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time, including a transition.
    pub updated_at: OffsetDateTime,
}

/// The columns every read of an opening selects, in [`OpeningRow`] order.
const OPENING_COLS: &str = "o.id, o.title, o.team, o.location, o.employment_kind, o.status, \
     o.opened_on, o.closed_on, \
     (SELECT count(*) FROM hr_applicants a \
       WHERE a.tenant_id = o.tenant_id AND a.opening_id = o.id) AS applicants, \
     o.created_by, o.created_at, o.updated_at";

/// Validates and normalises an opening. Pure — no database, no `now()`.
fn normalize(input: &NewOpening) -> Result<NewOpening> {
    Ok(NewOpening {
        title: required("opening title", &input.title, OPENING_TITLE_MAX_CHARS)?,
        team: bounded("opening team", &input.team, OPENING_FIELD_MAX_CHARS)?,
        location: bounded("opening location", &input.location, OPENING_FIELD_MAX_CHARS)?,
        employment_kind: input.employment_kind,
    })
}

/// Today, in UTC. An opening is published and closed on a **day** — the day the
/// company decided, not an instant — and a few hours of zone either way changes
/// nothing that a hiring round depends on.
fn today() -> Date {
    OffsetDateTime::now_utc().date()
}

impl TenantStore {
    /// Writes down a role the company is hiring for — **the HR door**. It
    /// starts as a draft: a company writes the role before it decides to run it.
    ///
    /// `actor` is who wrote it. A [`TenantStore`] has no caller of its own.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a blank or over-long title, team or
    /// location; [`StoreError::Db`] on failure.
    pub async fn create_hr_opening(
        &self,
        input: &NewOpening,
        actor: &UserId,
    ) -> Result<HrOpeningId> {
        let opening = normalize(input)?;
        let id = HrOpeningId::generate();
        sqlx::query(
            "INSERT INTO hr_openings \
                 (tenant_id, id, title, team, location, employment_kind, status, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, 'draft', $7)",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(&opening.title)
        .bind(&opening.team)
        .bind(&opening.location)
        .bind(opening.employment_kind.as_str())
        .bind(actor.as_str())
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// One opening, when it is this tenant's.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure. An id that is not this tenant's is `None`
    /// — the caller answers `404`, so an id is never an existence oracle.
    pub async fn hr_opening(&self, id: &HrOpeningId) -> Result<Option<Opening>> {
        let row = sqlx::query_as::<_, OpeningRow>(&format!(
            "SELECT {OPENING_COLS} FROM hr_openings o WHERE o.tenant_id = $1 AND o.id = $2"
        ))
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        row.map(OpeningRow::into_opening).transpose()
    }

    /// What this tenant is hiring for. Live openings (draft and open) newest
    /// first; `include_closed` appends the rounds that are over.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn hr_openings(&self, include_closed: bool) -> Result<Vec<Opening>> {
        let rows = sqlx::query_as::<_, OpeningRow>(&format!(
            "SELECT {OPENING_COLS} FROM hr_openings o \
              WHERE o.tenant_id = $1 AND ($2 OR o.status <> 'closed') \
              ORDER BY (o.status = 'closed'), o.created_at DESC, o.id"
        ))
        .bind(self.tenant().as_str())
        .bind(include_closed)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(OpeningRow::into_opening).collect()
    }

    /// Edits an opening that is still being run — **the HR door**.
    ///
    /// A **closed** opening is frozen: it is the record of a round that
    /// happened, and rewriting the title of a role thirty people applied for
    /// would rewrite what they applied for.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the opening is not this tenant's;
    /// [`StoreError::Conflict`] when it is closed; [`StoreError::Validation`]
    /// on a field the caller can fix; [`StoreError::Db`] on failure.
    pub async fn update_hr_opening(&self, id: &HrOpeningId, input: &NewOpening) -> Result<()> {
        let opening = normalize(input)?;
        let stored = self.require_hr_opening(id).await?;
        if stored == OpeningStatus::Closed {
            return Err(StoreError::Conflict(
                "this opening is closed; a role being hired for again is a new opening".to_owned(),
            ));
        }
        sqlx::query(
            "UPDATE hr_openings SET title = $3, team = $4, location = $5, employment_kind = $6, \
                    updated_at = now() \
              WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(&opening.title)
        .bind(&opening.team)
        .bind(&opening.location)
        .bind(opening.employment_kind.as_str())
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Publishes a draft: the round is running from today — **the HR door**.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the opening is not this tenant's;
    /// [`StoreError::Conflict`] when it is already open or closed, naming the
    /// state; [`StoreError::Db`] on failure.
    pub async fn publish_hr_opening(&self, id: &HrOpeningId) -> Result<()> {
        let stored = self.require_hr_opening(id).await?;
        if stored != OpeningStatus::Draft {
            return Err(StoreError::Conflict(format!(
                "an opening is published from draft; this one is {stored}"
            )));
        }
        self.set_hr_opening_state(id, OpeningStatus::Open, "opened_on")
            .await
    }

    /// Closes the round, from a draft or from an open opening — **the HR
    /// door**. The applicants stay: they are the record of what happened.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the opening is not this tenant's;
    /// [`StoreError::Conflict`] when it is already closed; [`StoreError::Db`]
    /// on failure.
    pub async fn close_hr_opening(&self, id: &HrOpeningId) -> Result<()> {
        let stored = self.require_hr_opening(id).await?;
        if stored == OpeningStatus::Closed {
            return Err(StoreError::Conflict(
                "this opening is already closed".to_owned(),
            ));
        }
        self.set_hr_opening_state(id, OpeningStatus::Closed, "closed_on")
            .await
    }

    /// The status of one of this tenant's openings, or the clean denial an id
    /// from another tenant gets.
    ///
    /// `pub(crate)` because the applicants module asks the same question before
    /// it records an application, and a second copy of the read would be a
    /// second chance to forget the tenant predicate.
    pub(crate) async fn require_hr_opening(&self, id: &HrOpeningId) -> Result<OpeningStatus> {
        let stored: Option<String> =
            sqlx::query_scalar("SELECT status FROM hr_openings WHERE tenant_id = $1 AND id = $2")
                .bind(self.tenant().as_str())
                .bind(id.as_str())
                .fetch_optional(self.pool())
                .await
                .map_err(StoreError::Db)?;
        OpeningStatus::parse(&stored.ok_or(StoreError::NotFound)?)
    }

    /// Moves an opening to `status` and stamps `date_column` with today.
    ///
    /// `date_column` is one of two literals chosen by the two callers above —
    /// never caller input — so the format is a fixed string rather than a bind.
    async fn set_hr_opening_state(
        &self,
        id: &HrOpeningId,
        status: OpeningStatus,
        date_column: &'static str,
    ) -> Result<()> {
        sqlx::query(&format!(
            "UPDATE hr_openings SET status = $3, {date_column} = $4, updated_at = now() \
              WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(status.as_str())
        .bind(today())
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct OpeningRow {
    id: String,
    title: String,
    team: String,
    location: String,
    employment_kind: String,
    status: String,
    opened_on: Option<Date>,
    closed_on: Option<Date>,
    applicants: Option<i64>,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl OpeningRow {
    fn into_opening(self) -> Result<Opening> {
        Ok(Opening {
            id: HrOpeningId::new(self.id),
            title: self.title,
            team: self.team,
            location: self.location,
            employment_kind: ContractKind::parse(&self.employment_kind)?,
            status: OpeningStatus::parse(&self.status)?,
            opened_on: self.opened_on,
            closed_on: self.closed_on,
            applicants: self.applicants.unwrap_or_default(),
            created_by: UserId::new(self.created_by),
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{NewOpening, OpeningStatus, normalize};
    use crate::hr_employments::ContractKind;

    #[test]
    fn every_status_round_trips_through_its_stored_word() {
        for status in [
            OpeningStatus::Draft,
            OpeningStatus::Open,
            OpeningStatus::Closed,
        ] {
            assert_eq!(
                OpeningStatus::parse(status.as_str()).ok(),
                Some(status),
                "{status} did not round trip"
            );
        }
    }

    #[test]
    fn a_word_no_code_knows_is_refused_with_the_list() {
        let message = match OpeningStatus::parse("on-hold") {
            Err(error) => error.to_string(),
            Ok(status) => panic!("expected a refusal, got {status}"),
        };
        assert!(message.contains("draft"), "the message lists the set");
    }

    #[test]
    fn a_title_is_required_and_the_rest_is_optional() {
        let blank = normalize(&NewOpening {
            title: "   ".to_owned(),
            ..Default::default()
        });
        assert!(blank.is_err(), "an opening with no title is not a role");
        let ok = normalize(&NewOpening {
            title: "  Backend engineer  ".to_owned(),
            employment_kind: ContractKind::FixedTerm,
            ..Default::default()
        })
        .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(ok.title, "Backend engineer", "the title is trimmed");
        assert!(ok.team.is_empty() && ok.location.is_empty());
        assert_eq!(ok.employment_kind, ContractKind::FixedTerm);
    }

    #[test]
    fn an_over_long_field_names_the_rule_and_not_the_value() {
        let message = match normalize(&NewOpening {
            title: "Engineer".to_owned(),
            location: "x".repeat(500),
            ..Default::default()
        }) {
            Err(error) => error.to_string(),
            Ok(_) => panic!("expected a refusal"),
        };
        assert!(message.contains("120"), "the message names the limit");
        assert!(!message.contains("xxx"), "and never echoes the value");
    }
}
