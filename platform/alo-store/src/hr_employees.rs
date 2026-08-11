//! Employees — the people a tenant employs (alo HR, ADR 0035, wave B6.02a;
//! `docs/design/hr.md`, "The data model").
//!
//! This is the most sensitive record in the suite, and the module's shape is
//! the argument for how it stays that way.
//!
//! # One table, two projections, chosen by which function was called
//!
//! [`DirectoryEntry`] is what any member of the tenant may read: name, job
//! title, team, work contact, who they report to. [`Employee`] is the whole
//! record, including the home address, the date of birth, the national
//! identifier and the bank account, and it is returned by exactly two
//! functions — [`AccountStore::my_hr_employee`], which can only ever answer
//! with the caller's **own** record because the statement carries their user
//! id, and [`TenantStore::hr_employee`], which the API only reaches behind the
//! HR gate.
//!
//! The projection is chosen in the store, by which function was called, rather
//! than by a field filter applied at the edge — *a filter at the edge is a
//! filter somebody forgets on the second route*. The list projection cannot
//! carry a national id at all, which is the structural form of the design
//! note's rule that such a field never appears in a list response.
//!
//! # Three doors, and the two this file opens
//!
//! - **Your own** — [`AccountStore`], `user_id = self.user` in the statement.
//!   Reaching a colleague's record through it is unrepresentable rather than
//!   merely rejected, and it doubles as the subject-access answer: an employee
//!   asking what we hold about them opens a screen.
//! - **The directory** — [`AccountStore::hr_directory`], public fields only,
//!   active people only.
//! - **HR** — the [`TenantStore`] functions, which the API reaches only behind
//!   `require_hr` (an admin or [`crate::TenantRole::Hr`]). Every write takes
//!   the acting user explicitly, the way a role grant does, because a
//!   [`TenantStore`] has no caller of its own and "who changed this person's
//!   record" is a question an employee, a works council and an auditor each
//!   have standing to ask.
//!
//! # What is refused
//!
//! Archiving is the only removal there is (statutory retention outlives the
//! employment), a manager link that would make somebody their own manager is
//! refused **on write**, and no refusal in this file ever quotes a name, an
//! address or a number — a conflict names the record id, exactly as
//! `docs/design/hr.md`'s error table requires of the staff-number clash.

use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::{
    bounded, country as validate_country, email as validate_email, required,
};
use crate::error::{Result, StoreError};
use crate::iban;
use crate::id::{DriveNodeId, HrEmployeeId, UserId};
use crate::store::TenantStore;

/// A person's name field — long enough for any European name, bounded because
/// an unbounded text column in a directory is a denial-of-service surface.
pub const NAME_MAX_CHARS: usize = 120;
/// The tenant's own number for a person, as their payroll bureau writes it.
pub const STAFF_NUMBER_MAX_CHARS: usize = 64;
/// A national identifier / social-security number. Long enough for every
/// member state's format; never in a list, never in a log.
pub const NATIONAL_ID_MAX_CHARS: usize = 64;

const ADDRESS_LINE_MAX_CHARS: usize = 200;
const POSTAL_CODE_MAX_CHARS: usize = 20;
const CITY_MAX_CHARS: usize = 120;
const REGION_MAX_CHARS: usize = 120;
const PHONE_MAX_CHARS: usize = 40;

/// Nobody employed today was born before this year; a date before it is a typo
/// (a mistyped century) rather than a person.
const EARLIEST_PLAUSIBLE_BIRTH_YEAR: i32 = 1900;

/// The columns a whole-record read selects, in `EmployeeRow` order.
const EMPLOYEE_COLS: &str = "id, user_id, staff_number, given_name, family_name, preferred_name, \
     work_email, work_phone, personal_email, personal_phone, date_of_birth, address_line1, \
     address_line2, postal_code, city, region, country, national_id, iban, emergency_name, \
     emergency_phone, manager_id, photo_node_id, archived_at, created_by, created_at, updated_at";

/// The public columns, in `DirectoryRow` order. The private ones are not
/// listed here and cannot be: this constant IS the directory projection.
const DIRECTORY_COLS: &str = "e.id, e.given_name, e.family_name, e.preferred_name, e.work_email, \
     e.work_phone, e.manager_id, e.photo_node_id, e.archived_at";

/// The writable shape of an employee, used for both create and update (an
/// update is a full replace — the route layer merges a partial `PATCH` onto the
/// stored record before calling, the rule every master record in this codebase
/// follows).
///
/// [`Default`] gives the blanks, so a caller can write
/// `NewEmployee { given_name, family_name, ..Default::default() }`. Only the
/// two name parts have no meaningful default.
///
/// The fields **absent** from this struct are as much a decision as the ones
/// present: nationality, ethnicity, religion, union membership, health
/// condition, disability status, marital status and dependants are refused by
/// design (`docs/design/hr.md`, "Data minimisation, and the fields we refuse").
#[derive(Debug, Clone, Default)]
pub struct NewEmployee {
    /// Their login, when they have one. `None` is ordinary: a warehouse hand or
    /// a seasonal picker is employed without a mailbox. Unique per tenant when
    /// set — two records cannot claim the same colleague.
    pub user_id: Option<UserId>,
    /// The tenant's own staff number. Unique per tenant when set.
    pub staff_number: Option<String>,
    /// Given (first) name. Required.
    pub given_name: String,
    /// Family (last) name. Required.
    pub family_name: String,
    /// What they are actually called, when that is not the given name. Blank
    /// means "use the given name".
    pub preferred_name: String,
    /// Their address at work — the one the directory shows.
    pub work_email: Option<String>,
    /// Their telephone number at work.
    pub work_phone: String,
    /// Private: their own address, for the day their work account is closed.
    pub personal_email: Option<String>,
    /// Private: their own telephone number.
    pub personal_phone: String,
    /// Private: date of birth. Held because payroll and statutory entitlements
    /// need it, not because a directory wants a birthday.
    pub date_of_birth: Option<Date>,
    /// Private: home address, first line.
    pub address_line1: String,
    /// Private: home address, second line.
    pub address_line2: String,
    /// Private: postal code.
    pub postal_code: String,
    /// Private: city / town.
    pub city: String,
    /// Private: region / province / state, where the address has one.
    pub region: String,
    /// Private: ISO 3166-1 alpha-2 country code, or blank. Not required — a
    /// person's home country is not always the employer's business.
    pub country: String,
    /// Private: national identifier / social-security number. The single most
    /// sensitive plain field in the schema; the payroll export is the only
    /// place it leaves the system.
    pub national_id: Option<String>,
    /// Private: the account wages are paid into. Mod-97 checked.
    pub iban: Option<String>,
    /// Private: who to call if something happens at work.
    pub emergency_name: String,
    /// Private: that person's telephone number.
    pub emergency_phone: String,
    /// Who they report to, as an employee id — the chart links employees, not
    /// accounts, so it is complete where the logins are not.
    pub manager_id: Option<HrEmployeeId>,
    /// An optional Drive node holding their photo. Optional by decision: a
    /// mandatory face on a directory is a discrimination surface.
    pub photo_node_id: Option<DriveNodeId>,
}

/// A whole employee record — every field, including the private ones.
///
/// Returned by exactly two reads: the caller's own record, and the HR door's
/// single-record read. Nothing that lists people returns this type.
#[derive(Debug, Clone)]
pub struct Employee {
    /// Opaque id, unique within the tenant.
    pub id: HrEmployeeId,
    /// Their login, when they have one.
    pub user_id: Option<UserId>,
    /// The tenant's own staff number.
    pub staff_number: Option<String>,
    /// Given (first) name.
    pub given_name: String,
    /// Family (last) name.
    pub family_name: String,
    /// What they are called, blank when the given name is right.
    pub preferred_name: String,
    /// Work email address.
    pub work_email: Option<String>,
    /// Work telephone number.
    pub work_phone: String,
    /// Private: personal email address.
    pub personal_email: Option<String>,
    /// Private: personal telephone number.
    pub personal_phone: String,
    /// Private: date of birth.
    pub date_of_birth: Option<Date>,
    /// Private: home address, first line.
    pub address_line1: String,
    /// Private: home address, second line.
    pub address_line2: String,
    /// Private: postal code.
    pub postal_code: String,
    /// Private: city / town.
    pub city: String,
    /// Private: region / province.
    pub region: String,
    /// Private: ISO 3166-1 alpha-2 country code, uppercase, or blank.
    pub country: String,
    /// Private: national identifier / social-security number.
    pub national_id: Option<String>,
    /// Private: the account wages are paid into, canonical form.
    pub iban: Option<String>,
    /// Private: emergency contact's name.
    pub emergency_name: String,
    /// Private: emergency contact's telephone number.
    pub emergency_phone: String,
    /// Who they report to.
    pub manager_id: Option<HrEmployeeId>,
    /// The Drive node holding their photo, when they have one.
    pub photo_node_id: Option<DriveNodeId>,
    /// When the record was archived; `None` while they are employed here.
    pub archived_at: Option<OffsetDateTime>,
    /// The user who created the record.
    pub created_by: String,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time.
    pub updated_at: OffsetDateTime,
}

impl Employee {
    /// Whether the record is archived — out of the directory, the org chart and
    /// the absence layer, still readable through the HR door because statutory
    /// retention outlives the employment.
    #[must_use]
    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }

    /// What to call them: the preferred name when they gave one, otherwise the
    /// given name. A projection, never a stored duplicate.
    #[must_use]
    pub fn display_name(&self) -> String {
        display_name(&self.preferred_name, &self.given_name, &self.family_name)
    }
}

/// One person as the **directory** shows them: public fields only, plus the
/// job title and team of whichever employment covers today.
///
/// There is deliberately no private field on this type. A list that could carry
/// a home address is a list somebody eventually returns to the wrong door.
#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    /// Opaque id, unique within the tenant.
    pub id: HrEmployeeId,
    /// Given (first) name.
    pub given_name: String,
    /// Family (last) name.
    pub family_name: String,
    /// What they are called, blank when the given name is right.
    pub preferred_name: String,
    /// Work email address.
    pub work_email: Option<String>,
    /// Work telephone number.
    pub work_phone: String,
    /// Who they report to.
    pub manager_id: Option<HrEmployeeId>,
    /// The Drive node holding their photo, when they have one.
    pub photo_node_id: Option<DriveNodeId>,
    /// Job title from the employment in force (blank when they have none yet).
    pub job_title: String,
    /// Team from the employment in force.
    pub team: String,
    /// The day the current employment started, when there is one.
    pub started_on: Option<Date>,
    /// Whether the person is archived. Always `false` through the member
    /// directory, which never lists them; `true` is only ever seen by HR.
    pub archived: bool,
}

impl DirectoryEntry {
    /// What to call them — same rule as [`Employee::display_name`].
    #[must_use]
    pub fn display_name(&self) -> String {
        display_name(&self.preferred_name, &self.given_name, &self.family_name)
    }
}

/// The one naming rule, in one place, so the directory and the record can never
/// disagree about what somebody is called.
///
/// `pub(crate)` because the leave surfaces name people too — a request queue,
/// the absence layer — and a second rule spelled into SQL would be a second
/// answer to "what is this person called" the first time somebody uses their
/// preferred name (`hr_leave_requests`, `hr_absences`).
pub(crate) fn display_name(preferred: &str, given: &str, family: &str) -> String {
    let first = if preferred.is_empty() {
        given
    } else {
        preferred
    };
    if family.is_empty() {
        first.to_owned()
    } else {
        format!("{first} {family}")
    }
}

/// A validated, normalised employee ready to be bound into a statement.
#[derive(Debug)]
struct Normalized {
    staff_number: Option<String>,
    given_name: String,
    family_name: String,
    preferred_name: String,
    work_email: Option<String>,
    work_phone: String,
    personal_email: Option<String>,
    personal_phone: String,
    date_of_birth: Option<Date>,
    address_line1: String,
    address_line2: String,
    postal_code: String,
    city: String,
    region: String,
    country: String,
    national_id: Option<String>,
    iban: Option<String>,
    emergency_name: String,
    emergency_phone: String,
}

/// Blank-to-`None` for the optional plain fields: a form that posts an empty
/// box means "not given", not "the empty string".
fn optional(field: &str, value: Option<&str>, max: usize) -> Result<Option<String>> {
    let trimmed = bounded(field, value.unwrap_or_default(), max)?;
    Ok(if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    })
}

/// Validates a date of birth: a real past date within living memory. A future
/// birthday is a typo, and so is the year 20 — and either would go straight
/// into a payroll file.
fn date_of_birth(value: Option<Date>, today: Date) -> Result<Option<Date>> {
    let Some(dob) = value else { return Ok(None) };
    if dob >= today {
        return Err(StoreError::Validation(
            "date of birth must be in the past".to_owned(),
        ));
    }
    if dob.year() < EARLIEST_PLAUSIBLE_BIRTH_YEAR {
        return Err(StoreError::Validation(format!(
            "date of birth must be no earlier than {EARLIEST_PLAUSIBLE_BIRTH_YEAR}"
        )));
    }
    Ok(Some(dob))
}

/// Validates and normalises a whole employee. Pure — no database, so every rule
/// here is unit-tested directly, and no message ever echoes what was typed.
fn normalize(input: &NewEmployee, today: Date) -> Result<Normalized> {
    Ok(Normalized {
        staff_number: optional(
            "staff number",
            input.staff_number.as_deref(),
            STAFF_NUMBER_MAX_CHARS,
        )?,
        given_name: required("given name", &input.given_name, NAME_MAX_CHARS)?,
        family_name: required("family name", &input.family_name, NAME_MAX_CHARS)?,
        preferred_name: bounded("preferred name", &input.preferred_name, NAME_MAX_CHARS)?,
        work_email: validate_email(input.work_email.as_deref())?,
        work_phone: bounded("work phone", &input.work_phone, PHONE_MAX_CHARS)?,
        personal_email: validate_email(input.personal_email.as_deref())?,
        personal_phone: bounded("personal phone", &input.personal_phone, PHONE_MAX_CHARS)?,
        date_of_birth: date_of_birth(input.date_of_birth, today)?,
        address_line1: bounded(
            "address line 1",
            &input.address_line1,
            ADDRESS_LINE_MAX_CHARS,
        )?,
        address_line2: bounded(
            "address line 2",
            &input.address_line2,
            ADDRESS_LINE_MAX_CHARS,
        )?,
        postal_code: bounded("postal code", &input.postal_code, POSTAL_CODE_MAX_CHARS)?,
        city: bounded("city", &input.city, CITY_MAX_CHARS)?,
        region: bounded("region", &input.region, REGION_MAX_CHARS)?,
        // A home country is optional — unlike a customer's, it decides no tax
        // treatment — but a stated one is held to the same two-letter shape.
        country: if input.country.trim().is_empty() {
            String::new()
        } else {
            validate_country(&input.country)?
        },
        national_id: optional(
            "national id",
            input.national_id.as_deref(),
            NATIONAL_ID_MAX_CHARS,
        )?,
        // The IBAN's own module owns the rule (country length plus the ISO 7064
        // mod-97 check); its message names the rule and never the number.
        iban: iban::canonicalize(input.iban.as_deref().unwrap_or_default())
            .map_err(|error| StoreError::Validation(error.to_string()))?,
        emergency_name: bounded("emergency contact", &input.emergency_name, NAME_MAX_CHARS)?,
        emergency_phone: bounded("emergency phone", &input.emergency_phone, PHONE_MAX_CHARS)?,
    })
}

/// Today, in UTC. Dates of birth are calendar facts rather than instants; the
/// only thing "today" is used for here is refusing a birthday in the future,
/// where a few hours of timezone either way changes nothing that matters.
fn today() -> Date {
    OffsetDateTime::now_utc().date()
}

impl TenantStore {
    /// Creates an active employee record, through the HR door.
    ///
    /// `actor` is the user doing it — a [`TenantStore`] has no caller of its
    /// own, and "who created this person's record" is a question with standing
    /// behind it.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on any field the caller can fix (blank name,
    /// implausible date of birth, bad email/country/IBAN shape) or a manager
    /// link that would create a cycle; [`StoreError::NotFound`] when the named
    /// manager, user or photo node is not this tenant's;
    /// [`StoreError::Conflict`] when the staff number or the user is already
    /// claimed by another record; [`StoreError::Db`] on failure.
    pub async fn create_hr_employee(
        &self,
        input: &NewEmployee,
        actor: &UserId,
    ) -> Result<HrEmployeeId> {
        let e = normalize(input, today())?;
        if let Some(user) = input.user_id.as_ref() {
            self.assert_user(user).await?;
        }
        let id = HrEmployeeId::generate();
        // A fresh id cannot be in any chain yet, so this only proves the
        // manager exists in this tenant — but it is the same call the update
        // path makes, so the rule has one implementation.
        self.assert_manager_link_sound(&id, input.manager_id.as_ref())
            .await?;
        self.assert_tenant_drive_node(input.photo_node_id.as_ref())
            .await?;
        sqlx::query(
            "INSERT INTO hr_employees (tenant_id, id, user_id, staff_number, given_name, \
                 family_name, preferred_name, work_email, work_phone, personal_email, \
                 personal_phone, date_of_birth, address_line1, address_line2, postal_code, city, \
                 region, country, national_id, iban, emergency_name, emergency_phone, \
                 manager_id, photo_node_id, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, \
                 $18, $19, $20, $21, $22, $23, $24, $25)",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(input.user_id.as_ref().map(UserId::as_str))
        .bind(&e.staff_number)
        .bind(&e.given_name)
        .bind(&e.family_name)
        .bind(&e.preferred_name)
        .bind(&e.work_email)
        .bind(&e.work_phone)
        .bind(&e.personal_email)
        .bind(&e.personal_phone)
        .bind(e.date_of_birth)
        .bind(&e.address_line1)
        .bind(&e.address_line2)
        .bind(&e.postal_code)
        .bind(&e.city)
        .bind(&e.region)
        .bind(&e.country)
        .bind(&e.national_id)
        .bind(&e.iban)
        .bind(&e.emergency_name)
        .bind(&e.emergency_phone)
        .bind(input.manager_id.as_ref().map(HrEmployeeId::as_str))
        .bind(input.photo_node_id.as_ref().map(DriveNodeId::as_str))
        .bind(actor.as_str())
        .execute(self.pool())
        .await
        .map_err(claim_conflict)?;
        Ok(id)
    }

    /// One whole employee record — the HR door's single-record read, and the
    /// only place a national id is ever returned about somebody else.
    ///
    /// `None` when the id is not this tenant's, which is indistinguishable from
    /// an id that was never issued.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn hr_employee(&self, id: &HrEmployeeId) -> Result<Option<Employee>> {
        let row = sqlx::query_as::<_, EmployeeRow>(&format!(
            "SELECT {EMPLOYEE_COLS} FROM hr_employees WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(EmployeeRow::into_employee))
    }

    /// The employee record belonging to a user of this tenant, if there is one
    /// — the HR door's "who is this login?", and the resolution every leave
    /// decision needs before it can ask whose manager somebody is.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn hr_employee_of_user(&self, user: &UserId) -> Result<Option<Employee>> {
        let row = sqlx::query_as::<_, EmployeeRow>(&format!(
            "SELECT {EMPLOYEE_COLS} FROM hr_employees WHERE tenant_id = $1 AND user_id = $2"
        ))
        .bind(self.tenant().as_str())
        .bind(user.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(EmployeeRow::into_employee))
    }

    /// The people of this tenant as the directory shows them, family name
    /// first. HR's list differs from a member's by one thing only: it can
    /// include the archived, who sort last.
    ///
    /// It is the **same projection** either way — the private fields are not in
    /// this type, so no caller of this function can leak one.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn hr_directory(&self, include_archived: bool) -> Result<Vec<DirectoryEntry>> {
        directory(self.pool(), self.tenant().as_str(), include_archived).await
    }

    /// Replaces every writable field of an employee record. Archiving is a
    /// separate operation ([`TenantStore::set_hr_employee_archived`]) so an
    /// ordinary edit can never drop somebody out of the directory by accident.
    ///
    /// # Errors
    /// As [`TenantStore::create_hr_employee`], plus [`StoreError::NotFound`]
    /// when the employee is not this tenant's.
    pub async fn update_hr_employee(&self, id: &HrEmployeeId, input: &NewEmployee) -> Result<()> {
        let e = normalize(input, today())?;
        if let Some(user) = input.user_id.as_ref() {
            self.assert_user(user).await?;
        }
        self.assert_manager_link_sound(id, input.manager_id.as_ref())
            .await?;
        self.assert_tenant_drive_node(input.photo_node_id.as_ref())
            .await?;
        let done = sqlx::query(
            "UPDATE hr_employees SET user_id = $3, staff_number = $4, given_name = $5, \
                 family_name = $6, preferred_name = $7, work_email = $8, work_phone = $9, \
                 personal_email = $10, personal_phone = $11, date_of_birth = $12, \
                 address_line1 = $13, address_line2 = $14, postal_code = $15, city = $16, \
                 region = $17, country = $18, national_id = $19, iban = $20, \
                 emergency_name = $21, emergency_phone = $22, manager_id = $23, \
                 photo_node_id = $24, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(input.user_id.as_ref().map(UserId::as_str))
        .bind(&e.staff_number)
        .bind(&e.given_name)
        .bind(&e.family_name)
        .bind(&e.preferred_name)
        .bind(&e.work_email)
        .bind(&e.work_phone)
        .bind(&e.personal_email)
        .bind(&e.personal_phone)
        .bind(e.date_of_birth)
        .bind(&e.address_line1)
        .bind(&e.address_line2)
        .bind(&e.postal_code)
        .bind(&e.city)
        .bind(&e.region)
        .bind(&e.country)
        .bind(&e.national_id)
        .bind(&e.iban)
        .bind(&e.emergency_name)
        .bind(&e.emergency_phone)
        .bind(input.manager_id.as_ref().map(HrEmployeeId::as_str))
        .bind(input.photo_node_id.as_ref().map(DriveNodeId::as_str))
        .execute(self.pool())
        .await
        .map_err(claim_conflict)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Archives or restores an employee record.
    ///
    /// **Archiving is the only removal HR performs.** Employment records carry
    /// statutory retention in every member state, so the row stays readable
    /// through this door with its archive time; an erasure once a retention
    /// period has genuinely expired is an admin's deliberate act taken with
    /// legal advice, never something this module does on a schedule.
    ///
    /// Archiving somebody who still has active direct reports is refused: the
    /// chart hides archived people, so allowing it would silently cut a branch
    /// off the org chart and leave their reports with nobody to decide their
    /// leave. Reassigning the reports first is the act that was meant.
    /// Idempotent — archiving an archived record keeps the original time.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the employee is not this tenant's;
    /// [`StoreError::Conflict`] naming how many reports must be reassigned
    /// first; [`StoreError::Db`] on failure.
    pub async fn set_hr_employee_archived(&self, id: &HrEmployeeId, archived: bool) -> Result<()> {
        if archived {
            let reports: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM hr_employees \
                  WHERE tenant_id = $1 AND manager_id = $2 AND archived_at IS NULL",
            )
            .bind(self.tenant().as_str())
            .bind(id.as_str())
            .fetch_one(self.pool())
            .await
            .map_err(StoreError::Db)?;
            if reports > 0 {
                return Err(StoreError::Conflict(format!(
                    "reassign {reports} direct report(s) before archiving this employee"
                )));
            }
        }
        let done = sqlx::query(
            "UPDATE hr_employees \
             SET archived_at = CASE WHEN $3 THEN COALESCE(archived_at, now()) ELSE NULL END, \
                 updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(archived)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Proves a Drive node is this tenant's, so a photo can never point at
    /// another tenant's file. `None` passes — no photo is the default.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the node is not this tenant's;
    /// [`StoreError::Db`] on failure.
    async fn assert_tenant_drive_node(&self, node: Option<&DriveNodeId>) -> Result<()> {
        let Some(node) = node else { return Ok(()) };
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM drive_nodes WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant().as_str())
        .bind(node.as_str())
        .fetch_one(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if !exists {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

impl AccountStore {
    /// **The own door.** The caller's own employee record, whole — the screen
    /// an employee opens to see what their employer holds about them, which is
    /// also the subject-access answer.
    ///
    /// `None` when the signed-in user has no employee record (a contractor with
    /// a mailbox, an admin who is not on the payroll). A colleague's record is
    /// not reachable through this function at all: the statement carries
    /// `user_id = self.user`, so there is no argument that could ask for one.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn my_hr_employee(&self) -> Result<Option<Employee>> {
        let row = sqlx::query_as::<_, EmployeeRow>(&format!(
            "SELECT {EMPLOYEE_COLS} FROM hr_employees WHERE tenant_id = $1 AND user_id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(EmployeeRow::into_employee))
    }

    /// **The directory door.** The tenant's active people, public fields only.
    ///
    /// A company where you cannot find out who your colleague's manager is has
    /// an org chart in a filing cabinet, and we are replacing filing cabinets —
    /// so every member gets this read, and it carries nothing private.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn hr_directory(&self) -> Result<Vec<DirectoryEntry>> {
        directory(&self.pool, self.tenant.as_str(), false).await
    }
}

/// The directory read, shared by both doors so the projection has exactly one
/// implementation. The lateral join picks the employment in force — the open
/// one if there is one, otherwise the most recent — because a job title is a
/// property of the terms, not of the person.
async fn directory(
    pool: &sqlx::PgPool,
    tenant: &str,
    include_archived: bool,
) -> Result<Vec<DirectoryEntry>> {
    let rows = sqlx::query_as::<_, DirectoryRow>(&format!(
        "SELECT {DIRECTORY_COLS}, coalesce(m.job_title, '') AS job_title, \
                coalesce(m.team, '') AS team, m.started_on \
           FROM hr_employees e \
           LEFT JOIN LATERAL ( \
                SELECT job_title, team, started_on FROM hr_employments p \
                 WHERE p.tenant_id = e.tenant_id AND p.employee_id = e.id \
                 ORDER BY (p.ended_on IS NULL) DESC, p.started_on DESC, p.id \
                 LIMIT 1 \
           ) m ON TRUE \
          WHERE e.tenant_id = $1 AND ($2 OR e.archived_at IS NULL) \
          ORDER BY (e.archived_at IS NOT NULL), lower(e.family_name), lower(e.given_name), e.id"
    ))
    .bind(tenant)
    .bind(include_archived)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(rows.into_iter().map(DirectoryRow::into_entry).collect())
}

/// Turns the two unique-index violations this table can raise into conflicts
/// that name **which claim** clashed, without quoting the value — a message
/// that echoed the staff number or the login would be an oracle for what
/// another record holds.
fn claim_conflict(error: sqlx::Error) -> StoreError {
    let constraint = match &error {
        sqlx::Error::Database(db) => db.constraint().unwrap_or_default().to_owned(),
        _ => String::new(),
    };
    match constraint.as_str() {
        "hr_employees_staff_number_unique" => {
            StoreError::Conflict("that staff number is already used in this tenant".to_owned())
        }
        "hr_employees_user_unique" => {
            StoreError::Conflict("that user already has an employee record".to_owned())
        }
        _ => StoreError::from(error),
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct EmployeeRow {
    id: String,
    user_id: Option<String>,
    staff_number: Option<String>,
    given_name: String,
    family_name: String,
    preferred_name: String,
    work_email: Option<String>,
    work_phone: String,
    personal_email: Option<String>,
    personal_phone: String,
    date_of_birth: Option<Date>,
    address_line1: String,
    address_line2: String,
    postal_code: String,
    city: String,
    region: String,
    country: String,
    national_id: Option<String>,
    iban: Option<String>,
    emergency_name: String,
    emergency_phone: String,
    manager_id: Option<String>,
    photo_node_id: Option<String>,
    archived_at: Option<OffsetDateTime>,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl EmployeeRow {
    fn into_employee(self) -> Employee {
        Employee {
            id: HrEmployeeId::new(self.id),
            user_id: self.user_id.map(UserId::new),
            staff_number: self.staff_number,
            given_name: self.given_name,
            family_name: self.family_name,
            preferred_name: self.preferred_name,
            work_email: self.work_email,
            work_phone: self.work_phone,
            personal_email: self.personal_email,
            personal_phone: self.personal_phone,
            date_of_birth: self.date_of_birth,
            address_line1: self.address_line1,
            address_line2: self.address_line2,
            postal_code: self.postal_code,
            city: self.city,
            region: self.region,
            country: self.country,
            national_id: self.national_id,
            iban: self.iban,
            emergency_name: self.emergency_name,
            emergency_phone: self.emergency_phone,
            manager_id: self.manager_id.map(HrEmployeeId::new),
            photo_node_id: self.photo_node_id.map(DriveNodeId::new),
            archived_at: self.archived_at,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct DirectoryRow {
    id: String,
    given_name: String,
    family_name: String,
    preferred_name: String,
    work_email: Option<String>,
    work_phone: String,
    manager_id: Option<String>,
    photo_node_id: Option<String>,
    archived_at: Option<OffsetDateTime>,
    job_title: String,
    team: String,
    started_on: Option<Date>,
}

impl DirectoryRow {
    fn into_entry(self) -> DirectoryEntry {
        DirectoryEntry {
            id: HrEmployeeId::new(self.id),
            given_name: self.given_name,
            family_name: self.family_name,
            preferred_name: self.preferred_name,
            work_email: self.work_email,
            work_phone: self.work_phone,
            manager_id: self.manager_id.map(HrEmployeeId::new),
            photo_node_id: self.photo_node_id.map(DriveNodeId::new),
            job_title: self.job_title,
            team: self.team,
            started_on: self.started_on,
            archived: self.archived_at.is_some(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use time::Month;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("a real date")
    }

    fn now() -> Date {
        day(2026, Month::August, 11)
    }

    fn ines() -> NewEmployee {
        NewEmployee {
            given_name: "Inès".to_owned(),
            family_name: "Dupont".to_owned(),
            ..Default::default()
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn both_name_parts_are_required_and_bounded() {
        for blank in ["", "   ", "\t\n"] {
            let given = NewEmployee {
                given_name: blank.to_owned(),
                ..ines()
            };
            assert!(invalid(normalize(&given, now())).contains("given name"));
            let family = NewEmployee {
                family_name: blank.to_owned(),
                ..ines()
            };
            assert!(invalid(normalize(&family, now())).contains("family name"));
        }
        let long = NewEmployee {
            given_name: "x".repeat(NAME_MAX_CHARS + 1),
            ..ines()
        };
        assert!(invalid(normalize(&long, now())).contains("at most"));
    }

    #[test]
    fn normalize_trims_and_canonicalises_every_stored_form() {
        let input = NewEmployee {
            given_name: "  Inès ".to_owned(),
            family_name: " Dupont  ".to_owned(),
            preferred_name: " Nes ".to_owned(),
            work_email: Some("  Ines.Dupont@example.test ".to_owned()),
            country: "fr".to_owned(),
            iban: Some("nl91 abna 0417 1643 00".to_owned()),
            staff_number: Some(" 00417 ".to_owned()),
            ..Default::default()
        };
        let e = normalize(&input, now()).unwrap_or_else(|error| panic!("rejected: {error}"));
        assert_eq!(e.given_name, "Inès");
        assert_eq!(e.family_name, "Dupont");
        assert_eq!(e.preferred_name, "Nes");
        // Trimmed, and otherwise left exactly as typed: the case of a mailbox
        // is the mail server's business, not ours to fold.
        assert_eq!(e.work_email.as_deref(), Some("Ines.Dupont@example.test"));
        assert_eq!(e.country, "FR");
        assert_eq!(e.iban.as_deref(), Some("NL91ABNA0417164300"));
        assert_eq!(e.staff_number.as_deref(), Some("00417"));
    }

    #[test]
    fn a_blank_optional_field_is_absent_rather_than_empty() {
        let input = NewEmployee {
            staff_number: Some("   ".to_owned()),
            national_id: Some("".to_owned()),
            iban: Some("  ".to_owned()),
            work_email: Some(" ".to_owned()),
            ..ines()
        };
        let e = normalize(&input, now()).unwrap_or_else(|error| panic!("rejected: {error}"));
        assert_eq!(e.staff_number, None, "no number is not the empty number");
        assert_eq!(e.national_id, None);
        assert_eq!(e.iban, None);
        assert_eq!(e.work_email, None);
    }

    #[test]
    fn a_home_country_is_optional_but_a_stated_one_is_a_country() {
        let none = NewEmployee {
            country: "  ".to_owned(),
            ..ines()
        };
        assert_eq!(
            normalize(&none, now())
                .unwrap_or_else(|error| panic!("rejected: {error}"))
                .country,
            "",
            "not every employer records where their people live"
        );
        for bad in ["F", "FRA", "F1", "France"] {
            let input = NewEmployee {
                country: bad.to_owned(),
                ..ines()
            };
            assert!(
                invalid(normalize(&input, now())).contains("country"),
                "expected rejection: {bad:?}"
            );
        }
    }

    #[test]
    fn a_birthday_is_in_the_past_and_within_living_memory() {
        assert_eq!(date_of_birth(None, now()).unwrap_or(None), None);
        let ok = day(1988, Month::March, 2);
        assert_eq!(date_of_birth(Some(ok), now()).unwrap_or(None), Some(ok));
        // The typo that would otherwise reach a payroll file.
        for bad in [
            now(),
            day(2027, Month::January, 1),
            day(88, Month::March, 2),
        ] {
            let message = invalid(date_of_birth(Some(bad), now()));
            assert!(
                message.contains("date of birth"),
                "expected rejection of {bad}: {message}"
            );
        }
    }

    #[test]
    fn an_iban_is_mod97_checked_and_never_echoed() {
        // One digit changed: the check digits are the whole point of an IBAN,
        // and wages paid into a mistyped account are gone.
        let typo = NewEmployee {
            iban: Some("NL92ABNA0417164300".to_owned()),
            ..ines()
        };
        let message = invalid(normalize(&typo, now()));
        assert!(message.to_lowercase().contains("iban"), "{message}");
        assert!(!message.contains("ABNA0417164300"), "{message}");
    }

    #[test]
    fn no_validation_message_ever_quotes_what_was_typed() {
        // The rule the whole module rests on: a refusal names the rule and the
        // field, never the personal datum that broke it.
        let input = NewEmployee {
            given_name: "Wolfeschlegelsteinhausenbergerdorff".repeat(8),
            national_id: Some("79 06 12 345 67".to_owned()),
            ..ines()
        };
        let message = invalid(normalize(&input, now()));
        assert!(!message.contains("Wolfeschlegel"), "{message}");
        assert!(!message.contains("345 67"), "{message}");
    }

    #[test]
    fn the_free_text_fields_are_bounded() {
        let long = "x".repeat(3_000);
        for input in [
            NewEmployee {
                address_line1: long.clone(),
                ..ines()
            },
            NewEmployee {
                address_line2: long.clone(),
                ..ines()
            },
            NewEmployee {
                postal_code: long.clone(),
                ..ines()
            },
            NewEmployee {
                city: long.clone(),
                ..ines()
            },
            NewEmployee {
                region: long.clone(),
                ..ines()
            },
            NewEmployee {
                work_phone: long.clone(),
                ..ines()
            },
            NewEmployee {
                personal_phone: long.clone(),
                ..ines()
            },
            NewEmployee {
                emergency_name: long.clone(),
                ..ines()
            },
            NewEmployee {
                emergency_phone: long.clone(),
                ..ines()
            },
            NewEmployee {
                national_id: Some(long.clone()),
                ..ines()
            },
            NewEmployee {
                staff_number: Some(long.clone()),
                ..ines()
            },
        ] {
            assert!(invalid(normalize(&input, now())).contains("at most"));
        }
    }

    #[test]
    fn a_person_is_called_what_they_asked_to_be_called() {
        assert_eq!(display_name("", "Inès", "Dupont"), "Inès Dupont");
        assert_eq!(display_name("Nes", "Inès", "Dupont"), "Nes Dupont");
        // A mononym is a name. The rule never produces a trailing space.
        assert_eq!(display_name("", "Prince", ""), "Prince");
    }

    #[test]
    fn the_directory_projection_cannot_carry_a_private_field() {
        // The structural half of the wrong-role test: this type has no field a
        // home address, a birthday, a national id or a bank account could be
        // put in, so no caller of the directory can leak one however careless.
        // The DB-backed half (a filled-in record, read through both doors) is
        // in `tests/hr_employees_tenancy.rs`.
        let entry = DirectoryEntry {
            id: HrEmployeeId::new("e".to_owned()),
            given_name: "Inès".to_owned(),
            family_name: "Dupont".to_owned(),
            preferred_name: String::new(),
            work_email: Some("ines@example.test".to_owned()),
            work_phone: String::new(),
            manager_id: None,
            photo_node_id: None,
            job_title: "Verkoop".to_owned(),
            team: "Sales".to_owned(),
            started_on: None,
            archived: false,
        };
        let rendered = format!("{entry:?}");
        for private in [
            "date_of_birth",
            "national_id",
            "iban",
            "address_line1",
            "personal_email",
            "personal_phone",
            "emergency_name",
            "pay_amount_cents",
        ] {
            assert!(
                !rendered.contains(private),
                "the directory projection grew a private field: {private}"
            );
        }
    }
}
