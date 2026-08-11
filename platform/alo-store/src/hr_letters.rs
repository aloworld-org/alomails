//! The letters a tenant is willing to write about its own people — an
//! employment confirmation, a reference, a letter for a landlord (alo HR, ADR
//! 0035, wave B6.09b; `docs/design/hr.md`, "The two tools that do ship").
//!
//! # The tenant writes the letter; the agent fills it in
//!
//! A template is a subject and a body **typed by a person in this company**, in
//! this company's language, saying what this company is willing to state about
//! somebody. `draft_letter_from_template` merges the facts into it and leaves
//! the result in the caller's Drafts. There is no free-form path anywhere: a
//! letter this table does not hold is a `422`, never an improvisation — and that
//! is only true because the *text* lives here rather than in a model's head.
//!
//! # The merge vocabulary is closed, and it is the directory
//!
//! Every placeholder resolves to a field the member directory already shows
//! everybody ([`crate::DirectoryEntry`]) — what somebody is called, their work
//! address, their job title, their team, the day they started — plus the
//! company's own letterhead facts and today's date. That is the whole
//! vocabulary, and the sentence that makes this module safe to read twice:
//! **there is no query here that could return a private field**, because the
//! facts arrive as the directory projection, which has none.
//!
//! *Rejected: "any employee column".* It is how a pay figure, a home address or
//! a national id ends up in a letter somebody drafted in a hurry. The design
//! note forbids pay outright, and a closed list is the only form of that rule a
//! later hand cannot widen by accident — [`MergeField::ALL`] is the list, and
//! the test at the foot of this file reads it back for the words it must never
//! contain.
//!
//! # Placeholders are checked when the template is saved
//!
//! [`merge_fields`] parses the subject and the body on every write, so a
//! template naming a field this build does not know is refused **in the editor**,
//! with the vocabulary in the message — rather than at the moment somebody
//! needed the letter. Stored text is therefore always mergeable, and the one
//! remaining way a merge can fail is a fact the person genuinely has not got
//! (no job title on record), which [`render`] refuses rather than papering over
//! with a blank: a letter reading "employed as  since " is worse than a letter
//! that was not written.
//!
//! # What this module deliberately cannot do
//!
//! **Reach pay.** Not a column, not a placeholder, not a join. A certificate
//! that must state a salary is a letter a person completes, not one the agent
//! fills (`docs/design/hr.md`, "One tension in this section" — the strict
//! reading, per the loop's compliance rule).

use time::{Date, OffsetDateTime};

use crate::billing_field::required;
use crate::billing_settings::BillingSettings;
use crate::error::{Result, StoreError};
use crate::hr_employees::DirectoryEntry;
use crate::id::{HrLetterTemplateId, UserId};
use crate::store::TenantStore;

/// The longest a template's name may be: a line in a picker, and the words
/// somebody says to the agent — not a paragraph.
pub const TEMPLATE_NAME_MAX_CHARS: usize = 120;

/// The longest a subject line may be, placeholders included. A mail subject.
pub const LETTER_SUBJECT_MAX_CHARS: usize = 200;

/// The longest a letter body may be. Several pages of prose; past this it is a
/// document, and Drive is where a document belongs.
pub const LETTER_BODY_MAX_CHARS: usize = 20_000;

/// The most placeholders one template may carry. A bound so a pathological
/// body cannot make one merge do unbounded work; no real letter approaches it.
pub const TEMPLATE_FIELDS_MAX: usize = 200;

/// One fact a letter may state about somebody, by name.
///
/// This enum **is** the merge vocabulary. Every variant resolves to a field the
/// member directory already shows everybody, to a letterhead fact of the company
/// itself, or to today's date. Nothing private has a variant here, and adding
/// one would be a disclosure decision made in this file rather than by accident
/// somewhere downstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeField {
    /// What the person is called: their preferred name and family name.
    EmployeeName,
    /// Their given (first) name on its own.
    EmployeeGivenName,
    /// Their family (last) name on its own.
    EmployeeFamilyName,
    /// Their address at work — the one the directory shows.
    EmployeeWorkEmail,
    /// The job title of the employment in force.
    EmployeeJobTitle,
    /// The team of the employment in force.
    EmployeeTeam,
    /// The day the employment in force started, written `YYYY-MM-DD`.
    EmployeeStartedOn,
    /// The company's legal name, as it invoices under.
    CompanyName,
    /// The company's address on one line.
    CompanyAddress,
    /// The company's country, as an ISO 3166-1 alpha-2 code.
    CompanyCountry,
    /// The day the letter is drafted, written `YYYY-MM-DD`.
    LetterDate,
}

impl MergeField {
    /// Every field this build knows, in the order the editor should offer them.
    pub const ALL: [Self; 11] = [
        Self::EmployeeName,
        Self::EmployeeGivenName,
        Self::EmployeeFamilyName,
        Self::EmployeeWorkEmail,
        Self::EmployeeJobTitle,
        Self::EmployeeTeam,
        Self::EmployeeStartedOn,
        Self::CompanyName,
        Self::CompanyAddress,
        Self::CompanyCountry,
        Self::LetterDate,
    ];

    /// The word inside the braces.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EmployeeName => "employee.name",
            Self::EmployeeGivenName => "employee.given_name",
            Self::EmployeeFamilyName => "employee.family_name",
            Self::EmployeeWorkEmail => "employee.work_email",
            Self::EmployeeJobTitle => "employee.job_title",
            Self::EmployeeTeam => "employee.team",
            Self::EmployeeStartedOn => "employee.started_on",
            Self::CompanyName => "company.name",
            Self::CompanyAddress => "company.address",
            Self::CompanyCountry => "company.country",
            Self::LetterDate => "letter.date",
        }
    }

    /// Reads a field name. Case and surrounding space are forgiven, because a
    /// person typing `{{ Employee.Name }}` meant the field.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        let wanted = word.trim().to_lowercase();
        Self::ALL.into_iter().find(|field| field.as_str() == wanted)
    }
}

impl std::fmt::Display for MergeField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The vocabulary as one sentence, for the message a bad placeholder earns. A
/// refusal that names the accepted set is the difference between an editor
/// somebody can use and one they have to guess at (`docs/design/ux-principles.md`).
fn vocabulary() -> String {
    MergeField::ALL
        .iter()
        .map(|field| format!("{{{{{field}}}}}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The facts one letter is merged from: one person as the **directory** shows
/// them, the company's letterhead, and the day.
///
/// Built by [`LetterFacts::of`] from a [`DirectoryEntry`], which is the whole
/// privacy argument of this module in one line — the type carries no private
/// field, so no letter merged from it can state one.
#[derive(Debug, Clone)]
pub struct LetterFacts {
    /// What the person is called.
    pub employee_name: String,
    /// Their given name.
    pub given_name: String,
    /// Their family name.
    pub family_name: String,
    /// Their work address, blank when they have none.
    pub work_email: String,
    /// Job title of the employment in force, blank when they have none yet.
    pub job_title: String,
    /// Team of the employment in force.
    pub team: String,
    /// The day that employment started, when there is one.
    pub started_on: Option<Date>,
    /// The company's legal name.
    pub company_name: String,
    /// The company's address, on one line.
    pub company_address: String,
    /// The company's country code.
    pub company_country: String,
    /// The day the letter is drafted — the server's own date, never a caller's.
    pub date: Date,
}

impl LetterFacts {
    /// The facts of one letter, from the directory entry and the tenant's own
    /// letterhead.
    #[must_use]
    pub fn of(person: &DirectoryEntry, company: &BillingSettings, date: Date) -> Self {
        Self {
            employee_name: person.display_name(),
            given_name: person.given_name.clone(),
            family_name: person.family_name.clone(),
            work_email: person.work_email.clone().unwrap_or_default(),
            job_title: person.job_title.clone(),
            team: person.team.clone(),
            started_on: person.started_on,
            company_name: company.legal_name.clone(),
            company_address: company_address(company),
            company_country: company.country.clone(),
            date,
        }
    }

    /// What this field says, or the empty string when the fact is not on
    /// record. [`render`] is what turns an empty one into a refusal — the
    /// lookup itself stays total.
    #[must_use]
    pub fn value(&self, field: MergeField) -> String {
        match field {
            MergeField::EmployeeName => self.employee_name.clone(),
            MergeField::EmployeeGivenName => self.given_name.clone(),
            MergeField::EmployeeFamilyName => self.family_name.clone(),
            MergeField::EmployeeWorkEmail => self.work_email.clone(),
            MergeField::EmployeeJobTitle => self.job_title.clone(),
            MergeField::EmployeeTeam => self.team.clone(),
            MergeField::EmployeeStartedOn => self.started_on.map(iso_day).unwrap_or_default(),
            MergeField::CompanyName => self.company_name.clone(),
            MergeField::CompanyAddress => self.company_address.clone(),
            MergeField::CompanyCountry => self.company_country.clone(),
            MergeField::LetterDate => iso_day(self.date),
        }
    }
}

/// A day as a letter writes it: `YYYY-MM-DD`.
///
/// Deliberately not a local format. "1 September 2026", "1 september 2026" and
/// "01.09.2026" are three user-facing strings in three languages, and a server
/// that picked one would have chosen a language for a letter the tenant wrote in
/// their own (CLAUDE.md). A tenant who wants their own wording writes the date
/// into the template and leaves the placeholder out.
fn iso_day(day: Date) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        day.year(),
        u8::from(day.month()),
        day.day()
    )
}

/// The company's address as one line: the lines it has, comma-separated.
fn company_address(company: &BillingSettings) -> String {
    let city = match (company.postal_code.trim(), company.city.trim()) {
        ("", city) => city.to_owned(),
        (postal, "") => postal.to_owned(),
        (postal, city) => format!("{postal} {city}"),
    };
    [
        company.address_line1.trim(),
        company.address_line2.trim(),
        city.as_str(),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(", ")
}

/// One placeholder found in a text: where it sits, and what it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Placeholder {
    /// Byte offset of the opening `{{`.
    start: usize,
    /// Byte offset just past the closing `}}`.
    end: usize,
    /// The field it names.
    field: MergeField,
}

/// Every placeholder in `text`, in the order they appear.
///
/// `{{` and `}}` are ASCII, so the offsets are always character boundaries.
/// A single brace is ordinary text and is left alone — a letter is allowed to
/// contain one.
///
/// # Errors
/// [`StoreError::Validation`] naming the whole vocabulary when a placeholder
/// names a field this build does not know, when one is opened and never closed,
/// or when there are more than [`TEMPLATE_FIELDS_MAX`] of them.
fn scan(text: &str) -> Result<Vec<Placeholder>> {
    let mut found = Vec::new();
    let mut at = 0usize;
    while let Some(open) = text[at..].find("{{") {
        let start = at + open;
        let after = start + 2;
        let Some(close) = text[after..].find("}}") else {
            return Err(StoreError::Validation(
                "a merge field is opened with {{ and never closed with }}".to_owned(),
            ));
        };
        let word = &text[after..after + close];
        let field = MergeField::parse(word).ok_or_else(|| {
            StoreError::Validation(format!(
                "this build knows no merge field {{{{{}}}}}; the fields are: {}",
                word.trim(),
                vocabulary()
            ))
        })?;
        found.push(Placeholder {
            start,
            end: after + close + 2,
            field,
        });
        if found.len() > TEMPLATE_FIELDS_MAX {
            return Err(StoreError::Validation(format!(
                "a letter template carries at most {TEMPLATE_FIELDS_MAX} merge fields"
            )));
        }
        at = after + close + 2;
    }
    Ok(found)
}

/// The fields `text` names, in the order they first appear and without
/// repetition — what the editor shows under a template, and what a caller can
/// check a person against before drafting anything.
///
/// # Errors
/// As [`scan`].
pub fn merge_fields(text: &str) -> Result<Vec<MergeField>> {
    let mut fields: Vec<MergeField> = Vec::new();
    for placeholder in scan(text)? {
        if !fields.contains(&placeholder.field) {
            fields.push(placeholder.field);
        }
    }
    Ok(fields)
}

/// Fills `text` in from `facts`.
///
/// A fact the person has not got is a **refusal**, not a blank: a letter reading
/// "employed as  since " is worse than a letter that was not written, and it is
/// the kind of thing somebody signs without re-reading. The message names the
/// field and the person, so the fix ("give them a job title") is the next thing
/// the reader does.
///
/// # Errors
/// [`StoreError::Validation`] as [`scan`], and when a field the text names is
/// not on record for this person.
pub fn render(text: &str, facts: &LetterFacts) -> Result<String> {
    let placeholders = scan(text)?;
    let mut out = String::with_capacity(text.len());
    let mut at = 0usize;
    for placeholder in placeholders {
        let value = facts.value(placeholder.field);
        if value.trim().is_empty() {
            return Err(StoreError::Validation(format!(
                "this letter states {{{{{}}}}}, and there is no {} on record for {}",
                placeholder.field,
                placeholder.field,
                if facts.employee_name.trim().is_empty() {
                    "this person"
                } else {
                    facts.employee_name.trim()
                }
            )));
        }
        out.push_str(&text[at..placeholder.start]);
        out.push_str(&value);
        at = placeholder.end;
    }
    out.push_str(&text[at..]);
    Ok(out)
}

/// The writable shape of a letter template.
#[derive(Debug, Clone, Default)]
pub struct NewLetterTemplate {
    /// The tenant's own word for it — "Werkgeversverklaring", "Attestation".
    pub name: String,
    /// The subject line of the draft, placeholders allowed.
    pub subject: String,
    /// The letter itself, placeholders allowed.
    pub body: String,
}

/// One stored template.
#[derive(Debug, Clone)]
pub struct LetterTemplate {
    /// Opaque id, unique within the tenant.
    pub id: HrLetterTemplateId,
    /// The tenant's own word for it.
    pub name: String,
    /// The subject line, unmerged.
    pub subject: String,
    /// The letter, unmerged.
    pub body: String,
    /// The fields it names, in order of first appearance — derived from the
    /// text rather than stored, so the two can never disagree.
    pub fields: Vec<MergeField>,
    /// Who wrote it.
    pub created_by: String,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time.
    pub updated_at: OffsetDateTime,
}

/// One letter, filled in and ready to be put in somebody's Drafts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedLetter {
    /// The merged subject line.
    pub subject: String,
    /// The merged body.
    pub body: String,
}

/// Fills a whole template in — subject and body from the same facts, so a draft
/// can never carry a merged body under an unmerged subject.
///
/// # Errors
/// As [`render`].
pub fn render_letter(template: &LetterTemplate, facts: &LetterFacts) -> Result<RenderedLetter> {
    Ok(RenderedLetter {
        subject: render(&template.subject, facts)?,
        body: render(&template.body, facts)?,
    })
}

/// A validated, normalised template ready to be bound into a statement.
#[derive(Debug)]
struct Normalized {
    name: String,
    subject: String,
    body: String,
}

/// Validates and normalises a template. Pure — no database.
///
/// The placeholders are parsed here, which is the whole reason a stored template
/// is always mergeable.
fn normalize(input: &NewLetterTemplate) -> Result<Normalized> {
    let name = required("letter template name", &input.name, TEMPLATE_NAME_MAX_CHARS)?;
    let subject = required("letter subject", &input.subject, LETTER_SUBJECT_MAX_CHARS)?;
    let body = required("letter body", &input.body, LETTER_BODY_MAX_CHARS)?;
    merge_fields(&subject)?;
    merge_fields(&body)?;
    Ok(Normalized {
        name,
        subject,
        body,
    })
}

/// Turns the name index's uniqueness violation into an answer naming the rule,
/// and leaves every other database failure alone.
fn map_letter_conflict(error: sqlx::Error) -> StoreError {
    match error {
        sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23505") => {
            match db.constraint().unwrap_or_default() {
                "hr_letter_templates_name_unique" => {
                    StoreError::Conflict("a letter template already has this name".to_owned())
                }
                _ => StoreError::Conflict("unique constraint".to_owned()),
            }
        }
        other => StoreError::Db(other),
    }
}

/// The columns every read selects, in `TemplateRow` order.
const TEMPLATE_COLS: &str = "id, name, subject, body, created_by, created_at, updated_at";

impl TenantStore {
    /// Writes a letter template. **The HR door**: what this company is willing
    /// to state about somebody is a company decision, not one manager's.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a blank or over-long name, subject or body,
    /// or a placeholder outside the vocabulary; [`StoreError::Conflict`] when a
    /// template already has the name; [`StoreError::Db`] on failure.
    pub async fn create_hr_letter_template(
        &self,
        input: &NewLetterTemplate,
        actor: &UserId,
    ) -> Result<HrLetterTemplateId> {
        let template = normalize(input)?;
        let id = HrLetterTemplateId::generate();
        sqlx::query(
            "INSERT INTO hr_letter_templates (tenant_id, id, name, subject, body, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(&template.name)
        .bind(&template.subject)
        .bind(&template.body)
        .bind(actor.as_str())
        .execute(self.pool())
        .await
        .map_err(map_letter_conflict)?;
        Ok(id)
    }

    /// One template of this tenant, or `None` — including when the id belongs to
    /// another tenant, which is indistinguishable by design.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when stored text names a field this build does
    /// not know (a schema disagreement, not user input);
    /// [`StoreError::Db`] on failure.
    pub async fn hr_letter_template(
        &self,
        id: &HrLetterTemplateId,
    ) -> Result<Option<LetterTemplate>> {
        let row = sqlx::query_as::<_, TemplateRow>(&format!(
            "SELECT {TEMPLATE_COLS} FROM hr_letter_templates WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        row.map(TemplateRow::into_template).transpose()
    }

    /// The tenant's letter templates, by name.
    ///
    /// Read whole: a company has a handful of these, and the screen that lists
    /// them shows the letter, so a paged list would only cost a round trip per
    /// row. It is also what the agent resolves a name against, which is why the
    /// order is stable.
    ///
    /// # Errors
    /// As [`TenantStore::hr_letter_template`].
    pub async fn hr_letter_templates(&self) -> Result<Vec<LetterTemplate>> {
        let rows = sqlx::query_as::<_, TemplateRow>(&format!(
            "SELECT {TEMPLATE_COLS} FROM hr_letter_templates \
              WHERE tenant_id = $1 ORDER BY lower(name), id"
        ))
        .bind(self.tenant().as_str())
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(TemplateRow::into_template).collect()
    }

    /// Replaces a template's name, subject and body.
    ///
    /// Letters already drafted are untouched: a draft is a message in somebody's
    /// mailbox, a copy that owes nothing to this row.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the template is not this tenant's;
    /// [`StoreError::Validation`] and [`StoreError::Conflict`] as for create;
    /// [`StoreError::Db`] on failure.
    pub async fn update_hr_letter_template(
        &self,
        id: &HrLetterTemplateId,
        input: &NewLetterTemplate,
    ) -> Result<()> {
        let template = normalize(input)?;
        let done = sqlx::query(
            "UPDATE hr_letter_templates SET name = $3, subject = $4, body = $5, \
                    updated_at = now() \
              WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(&template.name)
        .bind(&template.subject)
        .bind(&template.body)
        .execute(self.pool())
        .await
        .map_err(map_letter_conflict)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes a template.
    ///
    /// Deletion rather than archiving, and it is honest: every letter it ever
    /// produced is a copy in somebody's Drafts or Sent, so nothing anybody holds
    /// depends on this row.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the template is not this tenant's or is
    /// already gone — deleting twice is a clean denial, not a silent success;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_hr_letter_template(&self, id: &HrLetterTemplateId) -> Result<()> {
        let done = sqlx::query("DELETE FROM hr_letter_templates WHERE tenant_id = $1 AND id = $2")
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
}

#[derive(sqlx::FromRow)]
struct TemplateRow {
    id: String,
    name: String,
    subject: String,
    body: String,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl TemplateRow {
    /// Fallible on purpose: stored text naming a field this build does not know
    /// is a schema disagreement, and answering with the placeholder still in the
    /// letter would put `{{whatever}}` in front of a landlord.
    fn into_template(self) -> Result<LetterTemplate> {
        let mut fields = merge_fields(&self.subject)?;
        for field in merge_fields(&self.body)? {
            if !fields.contains(&field) {
                fields.push(field);
            }
        }
        Ok(LetterTemplate {
            id: HrLetterTemplateId::new(self.id),
            name: self.name,
            subject: self.subject,
            body: self.body,
            fields,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
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

    fn facts() -> LetterFacts {
        LetterFacts {
            employee_name: "Ada Byron".to_owned(),
            given_name: "Adelheid".to_owned(),
            family_name: "Byron".to_owned(),
            work_email: "ada@example.test".to_owned(),
            job_title: "Systeembeheerder".to_owned(),
            team: "Techniek".to_owned(),
            started_on: Some(day(2024, Month::March, 4)),
            company_name: "Voorbeeld BV".to_owned(),
            company_address: "Kade 1, 1011 AB Amsterdam".to_owned(),
            company_country: "NL".to_owned(),
            date: day(2026, Month::August, 11),
        }
    }

    fn message<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(message)) => message,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn a_letter_is_filled_in_from_the_facts_it_names() {
        let text = "Hierbij verklaart {{company.name}} dat {{employee.name}} sinds \
                    {{employee.started_on}} in dienst is als {{employee.job_title}}.";
        assert_eq!(
            render(text, &facts()).unwrap(),
            "Hierbij verklaart Voorbeeld BV dat Ada Byron sinds 2024-03-04 in dienst is \
             als Systeembeheerder."
        );
    }

    #[test]
    fn the_fields_a_template_names_are_listed_once_in_the_order_they_appear() {
        let fields = merge_fields(
            "{{employee.name}} — {{company.name}}; {{employee.name}} again, {{letter.date}}",
        )
        .unwrap();
        assert_eq!(
            fields,
            [
                MergeField::EmployeeName,
                MergeField::CompanyName,
                MergeField::LetterDate
            ]
        );
        assert!(merge_fields("no placeholders at all").unwrap().is_empty());
    }

    #[test]
    fn a_field_this_build_does_not_know_is_refused_with_the_whole_vocabulary() {
        // The one mistake this module exists to prevent, in the shape somebody
        // actually makes it: reaching for pay because a certificate asked for it.
        for wanted in [
            "employee.salary",
            "employee.pay",
            "employee.iban",
            "employee.national_id",
            "employee.date_of_birth",
            "employee.home_address",
        ] {
            let refused = message(merge_fields(&format!("Pays {{{{{wanted}}}}} monthly")));
            assert!(refused.contains(wanted), "{refused}");
            assert!(refused.contains("{{employee.name}}"), "{refused}");
            assert!(refused.contains("{{company.name}}"), "{refused}");
        }
    }

    #[test]
    fn a_placeholder_that_is_never_closed_is_refused_rather_than_printed() {
        let refused = message(merge_fields("Dear {{employee.name"));
        assert!(refused.contains("never closed"), "{refused}");
        // A single brace is ordinary text; a letter may contain one.
        assert_eq!(
            render("Salary { see appendix }", &facts()).unwrap(),
            "Salary { see appendix }"
        );
    }

    #[test]
    fn spacing_and_case_inside_the_braces_are_forgiven() {
        assert_eq!(
            render("{{ Employee.Name }} / {{employee.name}}", &facts()).unwrap(),
            "Ada Byron / Ada Byron"
        );
        assert_eq!(
            MergeField::parse("  letter.date "),
            Some(MergeField::LetterDate)
        );
        assert_eq!(MergeField::parse("employee.salary"), None);
    }

    #[test]
    fn a_fact_the_person_has_not_got_refuses_instead_of_leaving_a_gap() {
        let no_job = LetterFacts {
            job_title: String::new(),
            ..facts()
        };
        let refused = message(render(
            "employed as {{employee.job_title}} since {{employee.started_on}}",
            &no_job,
        ));
        assert!(refused.contains("employee.job_title"), "{refused}");
        assert!(refused.contains("Ada Byron"), "{refused}");

        let never_started = LetterFacts {
            started_on: None,
            ..facts()
        };
        assert!(
            message(render("since {{employee.started_on}}", &never_started))
                .contains("employee.started_on")
        );
    }

    #[test]
    fn placeholders_are_checked_when_the_template_is_saved() {
        let good = NewLetterTemplate {
            name: "  Werkgeversverklaring  ".to_owned(),
            subject: "Verklaring voor {{employee.name}}".to_owned(),
            body: "In dienst sinds {{employee.started_on}}.".to_owned(),
        };
        let normalized = normalize(&good).unwrap();
        assert_eq!(normalized.name, "Werkgeversverklaring");

        // Every blank is refused, and so is a bad placeholder in the subject —
        // not only in the body.
        assert!(
            message(normalize(&NewLetterTemplate {
                name: String::new(),
                ..good.clone()
            }))
            .contains("must not be empty")
        );
        assert!(
            message(normalize(&NewLetterTemplate {
                subject: "  ".to_owned(),
                ..good.clone()
            }))
            .contains("must not be empty")
        );
        assert!(
            message(normalize(&NewLetterTemplate {
                body: String::new(),
                ..good.clone()
            }))
            .contains("must not be empty")
        );
        assert!(
            message(normalize(&NewLetterTemplate {
                subject: "For {{employee.salary}}".to_owned(),
                ..good.clone()
            }))
            .contains("knows no merge field")
        );
        assert!(
            message(normalize(&NewLetterTemplate {
                body: "x".repeat(LETTER_BODY_MAX_CHARS + 1),
                ..good
            }))
            .contains("at most")
        );
    }

    #[test]
    fn the_vocabulary_names_nothing_private_and_nothing_about_pay() {
        // The module header's rule, read back off the list itself. A later hand
        // adding a variant for any of these words fails here, which is where the
        // decision belongs.
        for forbidden in [
            "salary",
            "pay",
            "wage",
            "iban",
            "bank",
            "national",
            "birth",
            "home",
            "personal",
            "emergency",
            "phone",
        ] {
            for field in MergeField::ALL {
                assert!(
                    !field.as_str().contains(forbidden),
                    "{} names {forbidden}",
                    field.as_str()
                );
            }
        }
        // …and every field the enum knows is reachable by its own name.
        for field in MergeField::ALL {
            assert_eq!(MergeField::parse(field.as_str()), Some(field));
        }
    }

    #[test]
    fn a_whole_letter_is_merged_subject_and_body_together() {
        let template = LetterTemplate {
            id: HrLetterTemplateId::new("t-1".to_owned()),
            name: "Werkgeversverklaring".to_owned(),
            subject: "Verklaring — {{employee.name}}".to_owned(),
            body: "{{company.name}}, {{letter.date}}".to_owned(),
            fields: Vec::new(),
            created_by: "u-1".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        };
        let letter = render_letter(&template, &facts()).unwrap();
        assert_eq!(letter.subject, "Verklaring — Ada Byron");
        assert_eq!(letter.body, "Voorbeeld BV, 2026-08-11");
    }

    #[test]
    fn the_company_address_is_the_lines_it_actually_has() {
        let mut company = BillingSettings {
            address_line1: "Kade 1".to_owned(),
            postal_code: "1011 AB".to_owned(),
            city: "Amsterdam".to_owned(),
            ..Default::default()
        };
        assert_eq!(company_address(&company), "Kade 1, 1011 AB Amsterdam");
        company.address_line2 = "Unit 3".to_owned();
        assert_eq!(
            company_address(&company),
            "Kade 1, Unit 3, 1011 AB Amsterdam"
        );
        company.postal_code = String::new();
        assert_eq!(company_address(&company), "Kade 1, Unit 3, Amsterdam");
        assert_eq!(company_address(&BillingSettings::default()), "");
    }
}
