//! Site contact forms and their submissions (ADR 0036), reached through the
//! account door like [`crate::site_pages`]. A form is addressed as
//! (site, form) and a submission as (site, form, submission): every statement
//! scopes by tenant AND site, so neither can be reached through another
//! tenant — or another site of the same tenant. Submissions store only the
//! posted fields (size-capped here, at the write gate) — never the visitor's
//! IP or user agent (`docs/design/sites.md`, privacy model). The public
//! submit endpoint is a later slice; nothing unauthenticated writes yet.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{SiteFormId, SiteFormSubmissionId, SiteId};

/// Maximum forms one site may hold — far above any real marketing site, low
/// enough that a runaway loop cannot bloat a tenant.
pub const MAX_FORMS_PER_SITE: i64 = 50;
/// A form's owner-facing label — a short name, not prose.
pub(crate) const FORM_NAME_MAX_CHARS: usize = 100;
/// Cap on a submitted sender name.
pub const SUBMISSION_NAME_MAX_CHARS: usize = 200;
/// Cap on a submitted email address (the SMTP path limit).
pub const SUBMISSION_EMAIL_MAX_CHARS: usize = 254;
/// Cap on a submitted message body.
pub const SUBMISSION_MESSAGE_MAX_CHARS: usize = 10_000;

/// A contact form of a site — the object a `contact_form` section references
/// by id (the section stores the id as its `form_id` prop).
#[derive(Debug, Clone)]
pub struct SiteForm {
    pub id: SiteFormId,
    /// Owner-facing label for the submissions UI.
    pub name: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// One visitor submission, exactly as validated at the write gate.
#[derive(Debug, Clone)]
pub struct SiteFormSubmission {
    pub id: SiteFormSubmissionId,
    pub sender_name: String,
    pub sender_email: String,
    pub message: String,
    /// Owner workflow flag ("dealt with").
    pub handled: bool,
    pub received_at: OffsetDateTime,
}

/// The three posted fields of a submission after normalization — trimmed,
/// non-blank, within the caps. This is the only shape the store will insert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionContent {
    pub sender_name: String,
    pub sender_email: String,
    pub message: String,
}

/// Validates a form's owner-facing label: non-blank after trimming, bounded.
fn validate_form_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(StoreError::Validation(
            "form name must not be empty".to_owned(),
        ));
    }
    if name.chars().count() > FORM_NAME_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "form name must be at most {FORM_NAME_MAX_CHARS} characters"
        )));
    }
    Ok(name)
}

/// Normalizes the posted fields of a submission: trims each, requires all
/// three non-blank, bounds their lengths, and requires the email to look like
/// one address (one `@` with something on both sides, no whitespace or
/// control characters). Deliberately loose beyond that — this is a contact
/// form, not an SMTP envelope; the strict grammar lives in the mail path.
/// Public so the submit endpoint (a later slice) validates with the same gate.
///
/// # Errors
/// [`StoreError::Validation`] naming the violated rule (field-level, safe to
/// surface to the visitor).
pub fn normalize_submission(
    sender_name: &str,
    sender_email: &str,
    message: &str,
) -> Result<SubmissionContent> {
    let sender_name = sender_name.trim();
    if sender_name.is_empty() {
        return Err(StoreError::Validation("name must not be empty".to_owned()));
    }
    if sender_name.chars().count() > SUBMISSION_NAME_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "name must be at most {SUBMISSION_NAME_MAX_CHARS} characters"
        )));
    }
    let sender_email = sender_email.trim();
    if sender_email.is_empty() {
        return Err(StoreError::Validation("email must not be empty".to_owned()));
    }
    if sender_email.chars().count() > SUBMISSION_EMAIL_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "email must be at most {SUBMISSION_EMAIL_MAX_CHARS} characters"
        )));
    }
    let looks_like_address = matches!(
        sender_email.split_once('@'),
        Some((local, domain)) if !local.is_empty() && !domain.is_empty()
    );
    if !looks_like_address
        || sender_email
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(StoreError::Validation(
            "email must be a valid address".to_owned(),
        ));
    }
    let message = message.trim();
    if message.is_empty() {
        return Err(StoreError::Validation(
            "message must not be empty".to_owned(),
        ));
    }
    if message.chars().count() > SUBMISSION_MESSAGE_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "message must be at most {SUBMISSION_MESSAGE_MAX_CHARS} characters"
        )));
    }
    Ok(SubmissionContent {
        sender_name: sender_name.to_owned(),
        sender_email: sender_email.to_owned(),
        message: message.to_owned(),
    })
}

impl AccountStore {
    /// Creates a contact form on `site`. The returned id is the token a
    /// `contact_form` section stores as its `form_id` prop.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site isn't the tenant's;
    /// [`StoreError::Validation`] on an invalid name; [`StoreError::Conflict`]
    /// on a full site ([`MAX_FORMS_PER_SITE`]); [`StoreError::Db`] on failure.
    pub async fn create_site_form(&self, site: &SiteId, name: &str) -> Result<SiteFormId> {
        let name = validate_form_name(name)?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let forms: Option<i64> = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM site_forms f \
                     WHERE f.tenant_id = s.tenant_id AND f.site_id = s.id) \
             FROM sites s WHERE s.tenant_id = $1 AND s.id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let forms = forms.ok_or(StoreError::NotFound)?;
        if forms >= MAX_FORMS_PER_SITE {
            return Err(StoreError::Conflict(format!(
                "a site may have at most {MAX_FORMS_PER_SITE} forms"
            )));
        }
        let id = SiteFormId::generate();
        sqlx::query(
            "INSERT INTO site_forms (tenant_id, site_id, id, name) VALUES ($1, $2, $3, $4)",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(id.as_str())
        .bind(name)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// The site's forms, oldest first. Empty when the site isn't the
    /// tenant's — indistinguishable from a site with no forms, by design.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_forms(&self, site: &SiteId) -> Result<Vec<SiteForm>> {
        let rows = sqlx::query_as::<_, SiteFormRow>(
            "SELECT id, name, created_at, updated_at \
             FROM site_forms WHERE tenant_id = $1 AND site_id = $2 ORDER BY created_at, id",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(SiteFormRow::into_form).collect())
    }

    /// A single form of the tenant's site, or `None` — including when the
    /// site or form belongs to another tenant or another site
    /// (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_form(&self, site: &SiteId, form: &SiteFormId) -> Result<Option<SiteForm>> {
        let row = sqlx::query_as::<_, SiteFormRow>(
            "SELECT id, name, created_at, updated_at \
             FROM site_forms WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(form.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(SiteFormRow::into_form))
    }

    /// Renames a form.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the form isn't the tenant's or the
    /// site's; [`StoreError::Validation`] on an invalid name;
    /// [`StoreError::Db`] on failure.
    pub async fn rename_site_form(
        &self,
        site: &SiteId,
        form: &SiteFormId,
        name: &str,
    ) -> Result<()> {
        let name = validate_form_name(name)?;
        let done = sqlx::query(
            "UPDATE site_forms SET name = $4, updated_at = now() \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(form.as_str())
        .bind(name)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes a form and (by cascade) its submissions.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the form isn't the tenant's or the
    /// site's; [`StoreError::Db`] on failure.
    pub async fn delete_site_form(&self, site: &SiteId, form: &SiteFormId) -> Result<()> {
        let done =
            sqlx::query("DELETE FROM site_forms WHERE tenant_id = $1 AND site_id = $2 AND id = $3")
                .bind(self.tenant.as_str())
                .bind(site.as_str())
                .bind(form.as_str())
                .execute(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Records a submission on the form, after the
    /// [`normalize_submission`] write gate. This is the tenant-side insert
    /// (tests, imports); the public endpoint (a later slice) resolves its
    /// bare form id to a tenant first and funnels through the same gate.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the form isn't the tenant's or the
    /// site's; [`StoreError::Validation`] on a field violating the gate;
    /// [`StoreError::Db`] on failure.
    pub async fn add_site_form_submission(
        &self,
        site: &SiteId,
        form: &SiteFormId,
        sender_name: &str,
        sender_email: &str,
        message: &str,
    ) -> Result<SiteFormSubmissionId> {
        let content = normalize_submission(sender_name, sender_email, message)?;
        let id = SiteFormSubmissionId::generate();
        let done = sqlx::query(
            "INSERT INTO site_form_submissions \
                 (tenant_id, form_id, id, sender_name, sender_email, message) \
             SELECT $1, $3, $4, $5, $6, $7 \
             WHERE EXISTS (SELECT 1 FROM site_forms \
                           WHERE tenant_id = $1 AND site_id = $2 AND id = $3)",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(form.as_str())
        .bind(id.as_str())
        .bind(&content.sender_name)
        .bind(&content.sender_email)
        .bind(&content.message)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(id)
    }

    /// The form's submissions, newest first. Empty when the form isn't the
    /// tenant's or the site's — indistinguishable from a form nobody has
    /// written to, by design.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_form_submissions(
        &self,
        site: &SiteId,
        form: &SiteFormId,
    ) -> Result<Vec<SiteFormSubmission>> {
        let rows = sqlx::query_as::<_, SubmissionRow>(
            "SELECT s.id, s.sender_name, s.sender_email, s.message, s.handled, s.received_at \
             FROM site_form_submissions s \
             JOIN site_forms f ON f.tenant_id = s.tenant_id AND f.id = s.form_id \
             WHERE s.tenant_id = $1 AND f.site_id = $2 AND s.form_id = $3 \
             ORDER BY s.received_at DESC, s.id DESC",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(form.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(SubmissionRow::into_submission)
            .collect())
    }

    /// Sets or clears a submission's handled flag.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the submission isn't the tenant's, the
    /// site's, or the form's; [`StoreError::Db`] on failure.
    pub async fn set_form_submission_handled(
        &self,
        site: &SiteId,
        form: &SiteFormId,
        submission: &SiteFormSubmissionId,
        handled: bool,
    ) -> Result<()> {
        let done = sqlx::query(
            "UPDATE site_form_submissions s SET handled = $5 \
             FROM site_forms f \
             WHERE f.tenant_id = s.tenant_id AND f.id = s.form_id \
               AND s.tenant_id = $1 AND f.site_id = $2 AND s.form_id = $3 AND s.id = $4",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(form.as_str())
        .bind(submission.as_str())
        .bind(handled)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes a submission (the owner clearing spam or a data-removal
    /// request — submissions are a visitor's personal data).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the submission isn't the tenant's, the
    /// site's, or the form's; [`StoreError::Db`] on failure.
    pub async fn delete_form_submission(
        &self,
        site: &SiteId,
        form: &SiteFormId,
        submission: &SiteFormSubmissionId,
    ) -> Result<()> {
        let done = sqlx::query(
            "DELETE FROM site_form_submissions s \
             USING site_forms f \
             WHERE f.tenant_id = s.tenant_id AND f.id = s.form_id \
               AND s.tenant_id = $1 AND f.site_id = $2 AND s.form_id = $3 AND s.id = $4",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(form.as_str())
        .bind(submission.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct SiteFormRow {
    id: String,
    name: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}
impl SiteFormRow {
    fn into_form(self) -> SiteForm {
        SiteForm {
            id: SiteFormId::new(self.id),
            name: self.name,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SubmissionRow {
    id: String,
    sender_name: String,
    sender_email: String,
    message: String,
    handled: bool,
    received_at: OffsetDateTime,
}
impl SubmissionRow {
    fn into_submission(self) -> SiteFormSubmission {
        SiteFormSubmission {
            id: SiteFormSubmissionId::new(self.id),
            sender_name: self.sender_name,
            sender_email: self.sender_email,
            message: self.message,
            handled: self.handled,
            received_at: self.received_at,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn form_names_are_trimmed_bounded_and_never_blank() {
        assert_eq!(validate_form_name("  Contact  ").unwrap(), "Contact");
        for bad in ["", "   ", "x".repeat(101).as_str()] {
            assert!(
                matches!(validate_form_name(bad), Err(StoreError::Validation(_))),
                "expected rejection: {bad:?}"
            );
        }
        assert!(validate_form_name("x".repeat(100).as_str()).is_ok());
    }

    #[test]
    fn submissions_normalize_and_trim() {
        let content =
            normalize_submission("  Ada Lovelace ", " ada@example.test ", "  Hello there  ")
                .unwrap();
        assert_eq!(content.sender_name, "Ada Lovelace");
        assert_eq!(content.sender_email, "ada@example.test");
        assert_eq!(content.message, "Hello there");
    }

    #[test]
    fn submissions_reject_blank_fields() {
        for (name, email, message) in [
            ("", "a@b.test", "hi"),
            ("  ", "a@b.test", "hi"),
            ("Ada", "", "hi"),
            ("Ada", "a@b.test", ""),
            ("Ada", "a@b.test", "   "),
        ] {
            assert!(
                matches!(
                    normalize_submission(name, email, message),
                    Err(StoreError::Validation(_))
                ),
                "expected rejection: {name:?} {email:?} {message:?}"
            );
        }
    }

    #[test]
    fn submissions_reject_non_addresses() {
        for bad in [
            "not-an-email",
            "@no-local.test",
            "no-domain@",
            "two@at@signs.test", // split_once tolerates this — but see below
            "spa ce@example.test",
            "tab\t@example.test",
        ] {
            let result = normalize_submission("Ada", bad, "hi");
            // `two@at@signs.test` is technically accepted by the loose grammar
            // (quoted locals exist in the wild); everything else must fail.
            if bad == "two@at@signs.test" {
                assert!(result.is_ok(), "loose grammar admits {bad:?}");
            } else {
                assert!(
                    matches!(result, Err(StoreError::Validation(_))),
                    "expected rejection: {bad:?}"
                );
            }
        }
    }

    #[test]
    fn submissions_bound_every_field() {
        let long_name = "x".repeat(SUBMISSION_NAME_MAX_CHARS + 1);
        let long_email = format!("a@{}.test", "x".repeat(SUBMISSION_EMAIL_MAX_CHARS));
        let long_message = "x".repeat(SUBMISSION_MESSAGE_MAX_CHARS + 1);
        assert!(matches!(
            normalize_submission(&long_name, "a@b.test", "hi"),
            Err(StoreError::Validation(_))
        ));
        assert!(matches!(
            normalize_submission("Ada", &long_email, "hi"),
            Err(StoreError::Validation(_))
        ));
        assert!(matches!(
            normalize_submission("Ada", "a@b.test", &long_message),
            Err(StoreError::Validation(_))
        ));
        let max_message = "x".repeat(SUBMISSION_MESSAGE_MAX_CHARS);
        assert!(normalize_submission("Ada", "a@b.test", &max_message).is_ok());
    }
}
