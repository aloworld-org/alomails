//! The people who applied, the stage each is at, and what the people who met
//! them wrote down (alo HR, ADR 0035, wave B6.06a; `docs/design/hr.md`,
//! "Recruitment-lite" and "The EU AI Act posture").
//!
//! # Nothing here reads a CV
//!
//! An applicant's CV is a Drive node id in the tenant's HR area and **is never
//! parsed**. There is no extracted-text field, no parsed-fields field, no
//! score, no rank, no shortlist, no "fit", and no suggest-only form of any of
//! them. Annex III point 4(a) of Regulation (EU) 2024/1689 classifies systems
//! intended to analyse and filter job applications and to evaluate candidates
//! as high-risk, and those obligations apply from 2 August 2026; the derogation
//! in Art. 6(3) does not rescue a ranking feature with a human after it. The
//! absence of a column to put a score in is the cheapest guarantee that nothing
//! ever writes one — and the HR agent tools (B6.09) cannot reach this module at
//! all.
//!
//! # A stage moves because a person moved it
//!
//! [`ApplicantStage`] is closed and ordered, and
//! [`TenantStore::move_hr_applicant`] is the only function that writes it —
//! `update_hr_applicant` deliberately does not, so a `PATCH` correcting a
//! telephone number can never reorder somebody's candidacy. Every move is a
//! person's act through an audited route, which is the record that this
//! decision had a human in it.
//!
//! Moves are not restricted to going forwards. A rejection reversed, an offer
//! withdrawn and a candidate who comes back are all ordinary; a state machine
//! that forbade them would be a state machine people worked around by keeping a
//! spreadsheet.
//!
//! # A name, in one field
//!
//! Not given/family like an employee record: a candidate writes their own name
//! on an application, and splitting it is a guess we would make wrongly for a
//! large part of Europe and most of the world. The parts are asked for when
//! somebody is hired and an employee record is made.
//!
//! # The retention deadline
//!
//! An unsuccessful applicant's data has no employment-law retention behind it,
//! so every row carries [`Applicant::retain_until`] — six months from the day
//! the application was recorded unless the caller states otherwise — and
//! [`Applicant::retention_expired`] says whether that day has passed. **The
//! deletion is still a person pressing a button** ([`TenantStore::delete_hr_applicant`]):
//! a job that erases people unattended is out of scope
//! (`docs/design/hr.md`, "Out of scope"). What the module provides is the thing
//! that remembers to ask.

use time::{Date, OffsetDateTime};

use crate::billing_field::{bounded, email as validate_email, required};
use crate::error::{Result, StoreError};
use crate::hr_leave_math::add_months;
use crate::hr_openings::OpeningStatus;
use crate::id::{DriveNodeId, HrApplicantId, HrApplicantNoteId, HrOpeningId, UserId};
use crate::store::TenantStore;

/// The longest a candidate's name may be, in characters.
pub const APPLICANT_NAME_MAX_CHARS: usize = 200;

/// The longest a telephone number or a source may be.
pub const APPLICANT_FIELD_MAX_CHARS: usize = 120;

/// The longest one interview note may be. Room for what was said and decided in
/// an hour; past this it is a document, and a document belongs in the HR area
/// where its access rule is the same one.
pub const APPLICANT_NOTE_MAX_CHARS: usize = 4_000;

/// How long an applicant's data is kept by default, in months, when the caller
/// states no date.
///
/// Six months is the common European guidance for holding an unsuccessful
/// applicant's data without their consent to a talent pool — long enough to
/// answer a discrimination claim, short enough not to be a filing cabinet of
/// strangers. It is a **default**, not a rule we enforce: the caller may state
/// any date, because the period that is right is the tenant's decision under
/// their own national law.
pub const APPLICANT_RETENTION_MONTHS: u32 = 6;

/// The furthest ahead a retention date may be set, in years. A typo guard: a
/// date beyond this is a slipped digit rather than a policy, and the record it
/// would create is one nobody revisits.
pub const APPLICANT_RETENTION_MAX_YEARS: i32 = 10;

/// Where a candidate is in a hiring round.
///
/// Closed and ordered, matched by the CHECK one layer down. *Rejected:
/// configurable stages per opening, the shape `crm_pipelines`/`crm_stages`
/// has.* A sales process genuinely differs by product line; a hiring process
/// for a company small enough to be replacing Microsoft 365 with us has seven
/// stages and always the same seven. It becomes two tables the day a tenant
/// asks (`docs/design/hr.md`, "Recruitment-lite").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ApplicantStage {
    /// They applied. The stage every application is recorded in.
    Applied,
    /// Somebody is reading the application.
    Reviewing,
    /// Meeting them — however many rounds a tenant runs.
    Interview,
    /// An offer is out.
    Offer,
    /// They accepted, and the employee record is the next act.
    Hired,
    /// The company said no.
    Rejected,
    /// They said no, or stopped answering. Kept apart from `rejected` because
    /// "we turned them down" and "they withdrew" are different facts about the
    /// same person, and a company reading its own funnel needs them apart.
    Withdrawn,
}

impl ApplicantStage {
    /// Every stage, in board order — left to right, outcomes last.
    pub const ALL: [Self; 7] = [
        Self::Applied,
        Self::Reviewing,
        Self::Interview,
        Self::Offer,
        Self::Hired,
        Self::Rejected,
        Self::Withdrawn,
    ];

    /// The stored word.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Reviewing => "reviewing",
            Self::Interview => "interview",
            Self::Offer => "offer",
            Self::Hired => "hired",
            Self::Rejected => "rejected",
            Self::Withdrawn => "withdrawn",
        }
    }

    /// Reads a stage — from a request body or from a stored row.
    ///
    /// # Errors
    /// [`StoreError::Validation`] **listing every stage**, which is the error
    /// the design note's table names: a caller who used the wrong word can see
    /// the whole vocabulary in the refusal rather than go and find it.
    pub fn parse(value: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|stage| stage.as_str() == value.trim())
            .ok_or_else(|| {
                StoreError::Validation(format!(
                    "applicant stage must be one of: {}",
                    Self::ALL
                        .iter()
                        .map(|stage| stage.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })
    }

    /// Whether this stage ends the candidacy — the three a board shows as
    /// outcomes rather than as work in progress.
    #[must_use]
    pub fn is_outcome(self) -> bool {
        matches!(self, Self::Hired | Self::Rejected | Self::Withdrawn)
    }
}

impl std::fmt::Display for ApplicantStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The writable shape of an application. The stage is deliberately absent: it
/// is set to `applied` on the way in and moved only by
/// [`TenantStore::move_hr_applicant`].
#[derive(Debug, Clone, Default)]
pub struct NewApplicant {
    /// What they are called, in one field. Required.
    pub name: String,
    /// Their address, when they gave one.
    pub email: Option<String>,
    /// Their telephone number.
    pub phone: String,
    /// Where the application came from, in the tenant's own words.
    pub source: String,
    /// Their CV: a live node in **this tenant's HR area**, never read by us.
    pub cv_node_id: Option<DriveNodeId>,
    /// The day past which nobody should still be holding this. `None` takes
    /// [`APPLICANT_RETENTION_MONTHS`] from today.
    pub retain_until: Option<Date>,
}

/// One candidate, as the hiring screen reads them.
///
/// The CV's own name and size come from the Drive node in the same read, so a
/// pipeline is one round trip; they are `None` when the node has since been
/// purged through Drive's trash, and the row stays as the honest record that
/// there was a file.
#[derive(Debug, Clone)]
pub struct Applicant {
    /// Opaque id, unique within the tenant.
    pub id: HrApplicantId,
    /// The opening they applied for.
    pub opening_id: HrOpeningId,
    /// What they are called.
    pub name: String,
    /// Their address, when they gave one.
    pub email: Option<String>,
    /// Their telephone number.
    pub phone: String,
    /// Where the application came from.
    pub source: String,
    /// Where they are in the round.
    pub stage: ApplicantStage,
    /// Their CV, as a node in the tenant's HR area.
    pub cv_node_id: Option<DriveNodeId>,
    /// The file's name in Drive, when the node is still there.
    pub cv_file_name: Option<String>,
    /// Its size in bytes, when the node is still there.
    pub cv_size: Option<i64>,
    /// Whether the node is in Drive's trash — attached, but on its way out.
    pub cv_trashed: bool,
    /// The day past which nobody should still be holding this.
    pub retain_until: Date,
    /// Whether that day has passed. Computed at read against today, never
    /// stored: a stored flag is a flag that is wrong every morning.
    pub retention_expired: bool,
    /// When the application was recorded.
    pub created_at: OffsetDateTime,
    /// When it was last edited or moved.
    pub updated_at: OffsetDateTime,
}

/// One thing somebody wrote down about a candidate they met.
#[derive(Debug, Clone)]
pub struct ApplicantNote {
    /// Opaque id, unique within the tenant.
    pub id: HrApplicantNoteId,
    /// Who wrote it.
    pub author: UserId,
    /// What they wrote.
    pub body: String,
    /// When.
    pub created_at: OffsetDateTime,
}

/// The columns every read of an applicant selects, in [`ApplicantRow`] order.
const APPLICANT_COLS: &str = "a.id, a.opening_id, a.name, a.email, a.phone, a.source, a.stage, \
     a.cv_node_id, n.name AS cv_file_name, n.size AS cv_size, \
     coalesce(n.trashed, false) AS cv_trashed, a.retain_until, a.created_at, a.updated_at";

/// The `FROM` every read of an applicant uses: the row, with its CV's own facts
/// joined on so a pipeline is one round trip rather than one query per person.
const APPLICANT_FROM: &str = "hr_applicants a \
     LEFT JOIN drive_nodes n ON n.tenant_id = a.tenant_id AND n.id = a.cv_node_id";

/// Today, in UTC. A retention deadline is a **day** — the day a company decided
/// it would stop holding somebody's application — and a few hours of zone
/// either way changes nothing a person acting on it depends on.
fn today() -> Date {
    OffsetDateTime::now_utc().date()
}

/// The default deadline for an application recorded today: six calendar months
/// on, clamped into a shorter month by the leave module's own arithmetic (31
/// August plus six months is the end of February, not the 3rd of March).
#[must_use]
pub fn default_retain_until() -> Date {
    add_months(today(), i64::from(APPLICANT_RETENTION_MONTHS))
}

/// A validated, normalised application ready to be bound into statements. Pure
/// apart from reading today's date for the default deadline.
fn normalize(input: &NewApplicant) -> Result<NewApplicant> {
    let retain_until = input.retain_until.unwrap_or_else(default_retain_until);
    let ceiling = today().replace_year(today().year() + APPLICANT_RETENTION_MAX_YEARS);
    if ceiling.is_ok_and(|ceiling| retain_until > ceiling) {
        return Err(StoreError::Validation(format!(
            "an applicant's retention date is at most {APPLICANT_RETENTION_MAX_YEARS} years away"
        )));
    }
    Ok(NewApplicant {
        name: required("applicant name", &input.name, APPLICANT_NAME_MAX_CHARS)?,
        email: validate_email(input.email.as_deref())?,
        phone: bounded("applicant phone", &input.phone, APPLICANT_FIELD_MAX_CHARS)?,
        source: bounded("applicant source", &input.source, APPLICANT_FIELD_MAX_CHARS)?,
        cv_node_id: input.cv_node_id.clone(),
        retain_until: Some(retain_until),
    })
}

impl TenantStore {
    /// Records that somebody applied — **the HR door**.
    ///
    /// The opening must be this tenant's and not closed: an application to a
    /// round that is over is a record of something that cannot happen, and the
    /// screen that would show it has no column to put it in. A **draft** opening
    /// accepts one, because a referral often arrives before the advertisement
    /// goes out.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the opening is not this tenant's, or the
    /// CV is not a live node in this tenant's HR area — one answer for both, so
    /// neither is an existence oracle. [`StoreError::Conflict`] when the
    /// opening is closed. [`StoreError::Validation`] on a field the caller can
    /// fix. [`StoreError::Db`] on failure.
    pub async fn record_hr_applicant(
        &self,
        opening: &HrOpeningId,
        input: &NewApplicant,
    ) -> Result<HrApplicantId> {
        let applicant = normalize(input)?;
        if self.require_hr_opening(opening).await? == OpeningStatus::Closed {
            return Err(StoreError::Conflict(
                "this opening is closed; applications belong to a round that is running".to_owned(),
            ));
        }
        if let Some(cv) = applicant.cv_node_id.as_ref() {
            self.assert_hr_area_node(cv).await?;
        }
        let id = HrApplicantId::generate();
        sqlx::query(
            "INSERT INTO hr_applicants \
                 (tenant_id, id, opening_id, name, email, phone, source, stage, cv_node_id, \
                  retain_until) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, 'applied', $8, $9)",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(opening.as_str())
        .bind(&applicant.name)
        .bind(&applicant.email)
        .bind(&applicant.phone)
        .bind(&applicant.source)
        .bind(applicant.cv_node_id.as_ref().map(DriveNodeId::as_str))
        .bind(applicant.retain_until)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// The pipeline for one opening: everybody who applied, in board order —
    /// **the HR door**.
    ///
    /// Ordered by stage and then by when they applied, so the board's columns
    /// read the way a person filled them.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the opening is not this tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn hr_applicants(&self, opening: &HrOpeningId) -> Result<Vec<Applicant>> {
        self.require_hr_opening(opening).await?;
        let rows = sqlx::query_as::<_, ApplicantRow>(&format!(
            "SELECT {APPLICANT_COLS} FROM {APPLICANT_FROM} \
              WHERE a.tenant_id = $1 AND a.opening_id = $2 \
              ORDER BY a.created_at, a.id"
        ))
        .bind(self.tenant().as_str())
        .bind(opening.as_str())
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        let mut applicants = rows
            .into_iter()
            .map(ApplicantRow::into_applicant)
            .collect::<Result<Vec<_>>>()?;
        // Board order is the stage vocabulary's own order, which the database
        // cannot express without repeating the list in a CASE — a second copy of
        // the thing `ApplicantStage::ALL` exists to be.
        applicants.sort_by(|left, right| {
            left.stage
                .cmp(&right.stage)
                .then_with(|| left.created_at.cmp(&right.created_at))
        });
        Ok(applicants)
    }

    /// One candidate, when they are this tenant's.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure. An id that is not this tenant's is `None`
    /// — the caller answers `404`.
    pub async fn hr_applicant(&self, id: &HrApplicantId) -> Result<Option<Applicant>> {
        let row = sqlx::query_as::<_, ApplicantRow>(&format!(
            "SELECT {APPLICANT_COLS} FROM {APPLICANT_FROM} WHERE a.tenant_id = $1 AND a.id = $2"
        ))
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        row.map(ApplicantRow::into_applicant).transpose()
    }

    /// Corrects what is recorded about a candidate — **the HR door**.
    ///
    /// **Never the stage**: a `PATCH` fixing a telephone number must not be
    /// able to reorder somebody's candidacy, so the one field a decision lives
    /// in has one door ([`Self::move_hr_applicant`]) and that door is audited as
    /// a move.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the applicant is not this tenant's, or a
    /// stated CV is not a live node in this tenant's HR area;
    /// [`StoreError::Validation`] on a field the caller can fix;
    /// [`StoreError::Db`] on failure.
    pub async fn update_hr_applicant(
        &self,
        id: &HrApplicantId,
        input: &NewApplicant,
    ) -> Result<()> {
        let applicant = normalize(input)?;
        if let Some(cv) = applicant.cv_node_id.as_ref() {
            self.assert_hr_area_node(cv).await?;
        }
        let done = sqlx::query(
            "UPDATE hr_applicants \
                SET name = $3, email = $4, phone = $5, source = $6, cv_node_id = $7, \
                    retain_until = $8, updated_at = now() \
              WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(&applicant.name)
        .bind(&applicant.email)
        .bind(&applicant.phone)
        .bind(&applicant.source)
        .bind(applicant.cv_node_id.as_ref().map(DriveNodeId::as_str))
        .bind(applicant.retain_until)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Moves a candidate to another stage — **the HR door, and the only way a
    /// stage changes**.
    ///
    /// Any stage may follow any other: a rejection reversed, an offer withdrawn
    /// and a candidate who comes back are all ordinary, and a state machine that
    /// forbade them would be one people worked around with a spreadsheet. What
    /// is *not* ordinary is a decision without a person behind it, which is why
    /// this is a route of its own, audited as `hr.applicant.move`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the applicant is not this tenant's;
    /// [`StoreError::Db`] on failure. An unknown stage word is refused by
    /// [`ApplicantStage::parse`] before it reaches here.
    pub async fn move_hr_applicant(&self, id: &HrApplicantId, stage: ApplicantStage) -> Result<()> {
        let done = sqlx::query(
            "UPDATE hr_applicants SET stage = $3, updated_at = now() \
              WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(stage.as_str())
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Writes down what somebody who met the candidate has to say — **the HR
    /// door**.
    ///
    /// `author` is who was in the room. A note is never edited or deleted on its
    /// own: it is a contemporaneous record of an interview, and it goes when the
    /// candidate's record goes ([`Self::delete_hr_applicant`]).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the applicant is not this tenant's;
    /// [`StoreError::Validation`] on a blank or over-long note;
    /// [`StoreError::Db`] on failure.
    pub async fn add_hr_applicant_note(
        &self,
        id: &HrApplicantId,
        author: &UserId,
        body: &str,
    ) -> Result<HrApplicantNoteId> {
        let body = required("applicant note", body, APPLICANT_NOTE_MAX_CHARS)?;
        self.require_hr_applicant(id).await?;
        let note = HrApplicantNoteId::generate();
        sqlx::query(
            "INSERT INTO hr_applicant_notes (tenant_id, id, applicant_id, author_user_id, body) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(self.tenant().as_str())
        .bind(note.as_str())
        .bind(id.as_str())
        .bind(author.as_str())
        .bind(&body)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(note)
    }

    /// What was written about a candidate, newest first — **the HR door**.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the applicant is not this tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn hr_applicant_notes(&self, id: &HrApplicantId) -> Result<Vec<ApplicantNote>> {
        self.require_hr_applicant(id).await?;
        let rows = sqlx::query_as::<_, NoteRow>(
            "SELECT id, author_user_id, body, created_at FROM hr_applicant_notes \
              WHERE tenant_id = $1 AND applicant_id = $2 \
              ORDER BY created_at DESC, id",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(NoteRow::into_note).collect())
    }

    /// Erases a candidate's record and every note on it — **the HR door**.
    ///
    /// The one HR record that is deleted rather than archived. An employee's
    /// file carries statutory retention in every member state and is kept; an
    /// applicant who was not hired is the opposite case, and a tombstone would
    /// be the same personal data under another name. The notes go with the row
    /// (`ON DELETE CASCADE`), so the erasure is complete in one act.
    ///
    /// **Their CV is not this function's to remove**: it is a Drive node, and
    /// the caller trashes it through Drive's own door before calling here — one
    /// file tree, one deletion path, and a store function that reached across
    /// into Drive's tables would be a second one.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the applicant is not this tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_hr_applicant(&self, id: &HrApplicantId) -> Result<()> {
        let done = sqlx::query("DELETE FROM hr_applicants WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant().as_str())
            .bind(id.as_str())
            .execute(self.pool())
            .await
            .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Ok when the applicant is this tenant's, else [`StoreError::NotFound`] —
    /// the same answer an id that was never issued gets, so a note route is not
    /// an oracle for another tenant's candidates.
    async fn require_hr_applicant(&self, id: &HrApplicantId) -> Result<()> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM hr_applicants WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .fetch_one(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if exists {
            Ok(())
        } else {
            Err(StoreError::NotFound)
        }
    }
}

#[derive(sqlx::FromRow)]
struct ApplicantRow {
    id: String,
    opening_id: String,
    name: String,
    email: Option<String>,
    phone: String,
    source: String,
    stage: String,
    cv_node_id: Option<String>,
    cv_file_name: Option<String>,
    cv_size: Option<i64>,
    cv_trashed: bool,
    retain_until: Date,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl ApplicantRow {
    fn into_applicant(self) -> Result<Applicant> {
        Ok(Applicant {
            id: HrApplicantId::new(self.id),
            opening_id: HrOpeningId::new(self.opening_id),
            name: self.name,
            email: self.email,
            phone: self.phone,
            source: self.source,
            stage: ApplicantStage::parse(&self.stage)?,
            cv_node_id: self.cv_node_id.map(DriveNodeId::new),
            cv_file_name: self.cv_file_name,
            cv_size: self.cv_size,
            cv_trashed: self.cv_trashed,
            retain_until: self.retain_until,
            retention_expired: self.retain_until < today(),
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct NoteRow {
    id: String,
    author_user_id: String,
    body: String,
    created_at: OffsetDateTime,
}

impl NoteRow {
    fn into_note(self) -> ApplicantNote {
        ApplicantNote {
            id: HrApplicantNoteId::new(self.id),
            author: UserId::new(self.author_user_id),
            body: self.body,
            created_at: self.created_at,
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{ApplicantStage, NewApplicant, default_retain_until, normalize};
    use time::{Date, Month};

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("a real date")
    }

    #[test]
    fn every_stage_round_trips_through_its_stored_word() {
        for stage in ApplicantStage::ALL {
            assert_eq!(
                ApplicantStage::parse(stage.as_str()).ok(),
                Some(stage),
                "{stage} did not round trip"
            );
        }
    }

    #[test]
    fn an_unknown_stage_is_refused_with_the_whole_vocabulary() {
        let message = match ApplicantStage::parse("shortlisted") {
            Err(error) => error.to_string(),
            Ok(stage) => panic!("expected a refusal, got {stage}"),
        };
        for stage in ApplicantStage::ALL {
            assert!(
                message.contains(stage.as_str()),
                "the refusal lists {stage}"
            );
        }
    }

    #[test]
    fn the_board_order_is_the_vocabulary_order_with_outcomes_last() {
        let mut sorted = ApplicantStage::ALL;
        sorted.sort_unstable();
        assert_eq!(
            sorted,
            ApplicantStage::ALL,
            "declaration order is the order"
        );
        assert!(!ApplicantStage::Offer.is_outcome());
        for stage in [
            ApplicantStage::Hired,
            ApplicantStage::Rejected,
            ApplicantStage::Withdrawn,
        ] {
            assert!(stage.is_outcome(), "{stage} ends a candidacy");
        }
    }

    #[test]
    fn an_application_that_states_no_deadline_gets_one() {
        let applicant = normalize(&NewApplicant {
            name: "  Amara Diallo ".to_owned(),
            email: Some("  amara@example.test".to_owned()),
            ..Default::default()
        })
        .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(applicant.name, "Amara Diallo", "the name is trimmed");
        assert_eq!(applicant.email.as_deref(), Some("amara@example.test"));
        assert_eq!(
            applicant.retain_until,
            Some(default_retain_until()),
            "six months from today unless the caller says otherwise"
        );
    }

    #[test]
    fn a_deadline_a_century_out_is_a_typo_and_a_past_one_is_not() {
        let refused = normalize(&NewApplicant {
            name: "Amara Diallo".to_owned(),
            retain_until: Some(day(2126, Month::March, 1)),
            ..Default::default()
        });
        let message = match refused {
            Err(error) => error.to_string(),
            Ok(_) => panic!("expected a refusal"),
        };
        assert!(message.contains("10 years"), "the message names the rule");
        // A date already past is exactly the state the screen surfaces, so
        // correcting an expired candidate's telephone number must still work.
        let past = normalize(&NewApplicant {
            name: "Amara Diallo".to_owned(),
            retain_until: Some(day(2020, Month::March, 1)),
            ..Default::default()
        });
        assert!(past.is_ok(), "a deadline in the past is kept, not refused");
    }

    #[test]
    fn a_name_is_required_and_a_malformed_address_is_refused() {
        assert!(
            normalize(&NewApplicant {
                name: "   ".to_owned(),
                ..Default::default()
            })
            .is_err(),
            "an application with no name names nobody"
        );
        assert!(
            normalize(&NewApplicant {
                name: "Amara Diallo".to_owned(),
                email: Some("not-an-address".to_owned()),
                ..Default::default()
            })
            .is_err()
        );
    }
}
