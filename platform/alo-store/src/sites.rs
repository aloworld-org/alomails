//! Sites — a tenant's websites (alo Sites, ADR 0036), reached through the
//! account door like [`crate::tasks`] and [`crate::spaces`]. Sites are
//! tenant-wide: every user of the tenant sees and manages all of its sites
//! (no per-site membership in v1). The `subdomain` column is the module's one
//! deliberate cross-tenant surface — `<subdomain>.<SITES_DOMAIN>` is a single
//! public namespace guarded by a global unique index — and the claim check
//! reveals only taken/free, never the owner (`docs/design/sites.md`).

use serde_json::Value;
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::SiteId;
use crate::site_theme::SiteTheme;

/// The language every existing and newly-created site starts in.
pub const DEFAULT_SITE_LOCALE: &str = "en";
/// A deliberate UX and publish-size bound: twelve visible languages cover a
/// serious multilingual site without turning its editor into an unbounded
/// locale registry.
pub const MAX_SITE_LOCALES: usize = 12;

/// Subdomain length bounds (DNS label rules, tightened for a public product
/// namespace: real DNS allows 63 octets, we cap at 40 for URL sanity).
pub const SUBDOMAIN_MIN_LEN: usize = 3;
/// See [`SUBDOMAIN_MIN_LEN`].
pub const SUBDOMAIN_MAX_LEN: usize = 40;

/// A site name is a human label, not an identifier — generous but bounded.
const SITE_NAME_MAX_CHARS: usize = 120;

/// Subdomains a tenant can never claim: infrastructure labels, mail/protocol
/// hostnames, product and brand names, and abuse-prone words. Checked after
/// the syntax rules, so entries here are all lowercase and DNS-safe.
const RESERVED_SUBDOMAINS: &[&str] = &[
    // Infrastructure / web convention.
    "www",
    "mail",
    "email",
    "webmail",
    "admin",
    "api",
    "app",
    "cdn",
    "static",
    "assets",
    "status",
    "ftp",
    "vpn",
    "ns1",
    "ns2",
    "localhost",
    "internal",
    "dev",
    "staging",
    "test",
    "demo",
    // Mail / protocol hostnames a mail platform must keep.
    "smtp",
    "imap",
    "pop",
    "pop3",
    "jmap",
    "dav",
    "caldav",
    "carddav",
    "autodiscover",
    "autoconfig",
    "mta-sts",
    "dkim",
    "dmarc",
    "spf",
    "postmaster",
    "abuse",
    "webmaster",
    "hostmaster",
    "noreply",
    "no-reply",
    // Identity / account surfaces.
    "login",
    "auth",
    "sso",
    "identity",
    "account",
    "accounts",
    "signup",
    "register",
    "oauth",
    // Brand + product names (the alo suite).
    "alo",
    "aloworkplace",
    "alomails",
    "sites",
    "docs",
    "drive",
    "tasks",
    "calendar",
    "contacts",
    "chat",
    "meet",
    "spaces",
    "base",
    "billing",
    "crm",
    "help",
    "support",
    "security",
    "blog",
    "root",
    "official",
];

/// Where a site is in its lifecycle. `Live` is set by the publish flow only —
/// there is no direct status setter on the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteStatus {
    /// Being built; nothing is publicly reachable.
    Draft,
    /// Published: the public service serves its snapshots.
    Live,
}

impl SiteStatus {
    /// The wire/storage token for this status.
    pub fn as_str(self) -> &'static str {
        match self {
            SiteStatus::Draft => "draft",
            SiteStatus::Live => "live",
        }
    }

    /// Parses a stored/wire token, rejecting anything else.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(SiteStatus::Draft),
            "live" => Some(SiteStatus::Live),
            _ => None,
        }
    }
}

/// One website of the tenant.
#[derive(Debug, Clone)]
pub struct Site {
    pub id: SiteId,
    pub name: String,
    /// The site's label under the public sites domain (globally unique).
    pub subdomain: String,
    pub status: SiteStatus,
    /// The theme envelope as stored (see [`crate::site_theme`]) — always a
    /// value that passed [`crate::site_theme::SiteTheme::from_value`], or the
    /// pristine `{}` default of a site that never set one.
    pub theme: Value,
    /// Lowercase BCP-47-like tag used when a visitor enters without an
    /// explicit language path.
    pub default_locale: String,
    /// Ordered language choices shown by the editor and public switcher. The
    /// default locale is always present and the list never contains duplicates.
    pub enabled_locales: Vec<String>,
    pub created_by: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// Normalize one language tag into the stable wire/storage representation.
/// We accept the useful BCP-47 subset browsers and search engines understand:
/// a 2-3 letter language followed by optional 2-8 character alphanumeric
/// subtags. Lowercase storage makes equality and URL construction unambiguous.
fn normalize_locale_tag(locale: &str) -> Result<String> {
    let locale = locale.trim().to_ascii_lowercase();
    let mut parts = locale.split('-');
    let Some(language) = parts.next() else {
        return Err(StoreError::Conflict(
            "language code must start with 2 or 3 letters".to_owned(),
        ));
    };
    if !(2..=3).contains(&language.len()) || !language.bytes().all(|b| b.is_ascii_lowercase()) {
        return Err(StoreError::Conflict(
            "language code must start with 2 or 3 letters".to_owned(),
        ));
    }
    for subtag in parts {
        if !(2..=8).contains(&subtag.len()) || !subtag.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(StoreError::Conflict(
                "language code subtags must contain 2-8 letters or digits".to_owned(),
            ));
        }
    }
    Ok(locale)
}

/// Validate and canonicalize a site's locale settings before any write.
///
/// # Errors
/// [`StoreError::Conflict`] when there are no languages, too many languages,
/// a malformed/duplicate tag, or the default language is not enabled.
pub fn normalize_site_locales(
    default_locale: &str,
    enabled_locales: &[String],
) -> Result<(String, Vec<String>)> {
    if enabled_locales.is_empty() {
        return Err(StoreError::Conflict(
            "enable at least one site language".to_owned(),
        ));
    }
    if enabled_locales.len() > MAX_SITE_LOCALES {
        return Err(StoreError::Conflict(format!(
            "a site may enable at most {MAX_SITE_LOCALES} languages"
        )));
    }
    let default_locale = normalize_locale_tag(default_locale)?;
    let mut normalized = Vec::with_capacity(enabled_locales.len());
    for locale in enabled_locales {
        let locale = normalize_locale_tag(locale)?;
        if normalized.contains(&locale) {
            return Err(StoreError::Conflict(format!(
                "language {locale} is enabled more than once"
            )));
        }
        normalized.push(locale);
    }
    if !normalized.contains(&default_locale) {
        return Err(StoreError::Conflict(format!(
            "default language '{default_locale}' must also be enabled"
        )));
    }
    Ok((default_locale, normalized))
}

/// Validates a subdomain claim: DNS-safe `[a-z0-9-]`, 3–40 chars, no leading
/// or trailing hyphen, and not a reserved word. The rules are strict on
/// write — a stored subdomain is always safe to put on the wire as a host
/// label.
///
/// # Errors
/// [`StoreError::Conflict`] naming the violated rule (safe to surface as a
/// field-level validation detail).
pub fn validate_subdomain(subdomain: &str) -> Result<()> {
    if subdomain.len() < SUBDOMAIN_MIN_LEN || subdomain.len() > SUBDOMAIN_MAX_LEN {
        return Err(StoreError::Conflict(format!(
            "subdomain must be {SUBDOMAIN_MIN_LEN}-{SUBDOMAIN_MAX_LEN} characters"
        )));
    }
    if !subdomain
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(StoreError::Conflict(
            "subdomain may only contain lowercase letters, digits, and hyphens".to_owned(),
        ));
    }
    if subdomain.starts_with('-') || subdomain.ends_with('-') {
        return Err(StoreError::Conflict(
            "subdomain may not start or end with a hyphen".to_owned(),
        ));
    }
    if RESERVED_SUBDOMAINS.contains(&subdomain) {
        return Err(StoreError::Conflict("subdomain is reserved".to_owned()));
    }
    Ok(())
}

/// Validates a site's display name: non-blank after trimming, bounded.
pub(crate) fn validate_site_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(StoreError::Conflict(
            "site name must not be empty".to_owned(),
        ));
    }
    if name.chars().count() > SITE_NAME_MAX_CHARS {
        return Err(StoreError::Conflict(format!(
            "site name must be at most {SITE_NAME_MAX_CHARS} characters"
        )));
    }
    Ok(())
}

/// Translates a unique-index violation on the global subdomain namespace into
/// the taken/free answer — the only information the cross-tenant surface may
/// reveal. Anything else passes through the standard mapping.
pub(crate) fn map_subdomain_unique(error: sqlx::Error) -> StoreError {
    if let sqlx::Error::Database(ref db) = error
        && db.constraint() == Some("sites_subdomain_unique")
    {
        return StoreError::Conflict("subdomain is already taken".to_owned());
    }
    error.into()
}

impl AccountStore {
    /// Creates a site in `draft` status with an empty theme, claiming
    /// `subdomain` in the global namespace.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] on an invalid name, an invalid or reserved
    /// subdomain, or a subdomain already taken (by any tenant — the message
    /// says taken, nothing more); [`StoreError::Db`] on failure.
    pub async fn create_site(&self, name: &str, subdomain: &str) -> Result<SiteId> {
        self.create_site_with_locales(
            name,
            subdomain,
            DEFAULT_SITE_LOCALE,
            &[DEFAULT_SITE_LOCALE.to_owned()],
        )
        .await
    }

    /// Creates a draft site with explicitly chosen, validated languages.
    /// Existing callers use [`Self::create_site`] and receive the English
    /// default, keeping the S1 API source-compatible.
    ///
    /// # Errors
    /// The same errors as [`Self::create_site`], plus locale validation errors.
    pub async fn create_site_with_locales(
        &self,
        name: &str,
        subdomain: &str,
        default_locale: &str,
        enabled_locales: &[String],
    ) -> Result<SiteId> {
        validate_site_name(name)?;
        validate_subdomain(subdomain)?;
        let (default_locale, enabled_locales) =
            normalize_site_locales(default_locale, enabled_locales)?;
        let id = SiteId::generate();
        sqlx::query(
            "INSERT INTO sites (tenant_id, id, name, subdomain, default_locale, \
                                enabled_locales, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(name.trim())
        .bind(subdomain)
        .bind(default_locale)
        .bind(enabled_locales)
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(map_subdomain_unique)?;
        Ok(id)
    }

    /// The tenant's sites, name order.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn sites(&self) -> Result<Vec<Site>> {
        let rows = sqlx::query_as::<_, SiteRow>(
            "SELECT id, name, subdomain, status, theme, default_locale, enabled_locales, \
                    created_by, created_at, updated_at \
             FROM sites WHERE tenant_id = $1 ORDER BY lower(name), id",
        )
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(SiteRow::into_site).collect()
    }

    /// A single site of the tenant, or `None` — including when the id belongs
    /// to another tenant (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site(&self, id: &SiteId) -> Result<Option<Site>> {
        let row = sqlx::query_as::<_, SiteRow>(
            "SELECT id, name, subdomain, status, theme, default_locale, enabled_locales, \
                    created_by, created_at, updated_at \
             FROM sites WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(SiteRow::into_site).transpose()
    }

    /// Renames a site.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site isn't the tenant's;
    /// [`StoreError::Conflict`] on an invalid name; [`StoreError::Db`].
    pub async fn rename_site(&self, id: &SiteId, name: &str) -> Result<()> {
        validate_site_name(name)?;
        let done = sqlx::query(
            "UPDATE sites SET name = $3, updated_at = now() WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(name.trim())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Moves a site to a new subdomain, claiming it in the global namespace.
    /// The old subdomain is released atomically by the same statement.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site isn't the tenant's;
    /// [`StoreError::Conflict`] on an invalid, reserved, or taken subdomain;
    /// [`StoreError::Db`].
    pub async fn set_site_subdomain(&self, id: &SiteId, subdomain: &str) -> Result<()> {
        validate_subdomain(subdomain)?;
        let done = sqlx::query(
            "UPDATE sites SET subdomain = $3, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(subdomain)
        .execute(&self.pool)
        .await
        .map_err(map_subdomain_unique)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Replaces the languages available on a site. Validation happens before
    /// the tenant-scoped update, so a malformed request never partially writes.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site isn't the tenant's;
    /// [`StoreError::Conflict`] for invalid locale settings; [`StoreError::Db`].
    pub async fn set_site_locales(
        &self,
        id: &SiteId,
        default_locale: &str,
        enabled_locales: &[String],
    ) -> Result<()> {
        let (default_locale, enabled_locales) =
            normalize_site_locales(default_locale, enabled_locales)?;
        let done = sqlx::query(
            "UPDATE sites SET default_locale = $3, enabled_locales = $4, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(default_locale)
        .bind(enabled_locales)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Replaces a site's theme with `theme`, which must be a valid
    /// current-version envelope pointing at a shipped preset — this is the
    /// theme write gate. The stored value is the canonical serialization of
    /// the parsed theme, so whatever is on disk always round-trips through
    /// the typed model.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site isn't the tenant's;
    /// [`StoreError::Conflict`] carrying the schema violation (version,
    /// shape, unknown preset, or malformed blob ref — see
    /// [`crate::site_theme::ThemeSchemaError`]); [`StoreError::Db`].
    pub async fn set_site_theme(&self, id: &SiteId, theme: Value) -> Result<()> {
        let theme = SiteTheme::from_value(theme)
            .map_err(|schema| StoreError::Conflict(schema.to_string()))?;
        let canonical = theme
            .to_value()
            .map_err(|schema| StoreError::Conflict(schema.to_string()))?;
        let done = sqlx::query(
            "UPDATE sites SET theme = $3, updated_at = now() WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(sqlx::types::Json(canonical))
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes a site, releasing its subdomain. Dependent rows (pages,
    /// snapshots, posts, …) cascade as their tables land.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site isn't the tenant's;
    /// [`StoreError::Db`].
    pub async fn delete_site(&self, id: &SiteId) -> Result<()> {
        let done = sqlx::query("DELETE FROM sites WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Whether `subdomain` is free to claim. This is the deliberate
    /// cross-tenant read: it touches the global unique index and answers
    /// taken/free only — never who holds it. A syntactically invalid or
    /// reserved subdomain errs instead, so the UI can show the specific rule.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] on an invalid or reserved subdomain;
    /// [`StoreError::Db`] on failure.
    pub async fn subdomain_available(&self, subdomain: &str) -> Result<bool> {
        validate_subdomain(subdomain)?;
        let taken: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sites WHERE subdomain = $1)")
                .bind(subdomain)
                .fetch_one(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        Ok(!taken)
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct SiteRow {
    id: String,
    name: String,
    subdomain: String,
    status: String,
    theme: sqlx::types::Json<Value>,
    default_locale: String,
    enabled_locales: Vec<String>,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}
impl SiteRow {
    fn into_site(self) -> Result<Site> {
        Ok(Site {
            id: SiteId::new(self.id),
            name: self.name,
            subdomain: self.subdomain,
            status: SiteStatus::parse(&self.status).ok_or(StoreError::NotFound)?,
            theme: self.theme.0,
            default_locale: self.default_locale,
            enabled_locales: self.enabled_locales,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subdomain_rules_accept_dns_safe_labels() {
        for ok in ["abc", "my-site", "a1b2c3", "x".repeat(40).as_str(), "123"] {
            assert!(validate_subdomain(ok).is_ok(), "expected valid: {ok}");
        }
    }

    #[test]
    fn subdomain_rules_reject_bad_syntax() {
        let too_long = "x".repeat(41);
        for bad in [
            "",
            "ab",              // too short
            too_long.as_str(), // too long
            "-leading",
            "trailing-",
            "Upper",
            "under_score",
            "dot.dot",
            "spa ce",
            "ünïcode",
        ] {
            assert!(
                matches!(validate_subdomain(bad), Err(StoreError::Conflict(_))),
                "expected rejection: {bad:?}"
            );
        }
    }

    #[test]
    fn subdomain_rules_reject_reserved_words() {
        for reserved in [
            "www", "mail", "admin", "api", "smtp", "alo", "sites", "login",
        ] {
            assert!(
                matches!(validate_subdomain(reserved), Err(StoreError::Conflict(_))),
                "expected reserved: {reserved}"
            );
        }
    }

    #[test]
    fn reserved_list_entries_all_pass_the_syntax_rules() {
        // A reserved word that fails syntax would be dead weight — the syntax
        // check runs first and would already have rejected it.
        for entry in RESERVED_SUBDOMAINS {
            assert!(
                entry.len() >= SUBDOMAIN_MIN_LEN
                    && entry.len() <= SUBDOMAIN_MAX_LEN
                    && entry
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                    && !entry.starts_with('-')
                    && !entry.ends_with('-'),
                "reserved entry not DNS-safe or out of bounds: {entry}"
            );
        }
    }

    #[test]
    fn status_tokens_round_trip() {
        for status in [SiteStatus::Draft, SiteStatus::Live] {
            assert_eq!(SiteStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(SiteStatus::parse("published"), None);
    }

    #[test]
    fn locale_settings_normalize_and_keep_the_default_enabled() {
        let Ok(settings) = normalize_site_locales(
            "PT-br",
            &["EN".to_owned(), "pt-BR".to_owned(), "zh-Hant".to_owned()],
        ) else {
            panic!("valid locale settings were rejected");
        };
        assert_eq!(settings.0, "pt-br");
        assert_eq!(settings.1, ["en", "pt-br", "zh-hant"]);

        for (default, enabled) in [
            ("en", Vec::new()),
            ("en", vec!["fr".to_owned()]),
            ("en", vec!["en".to_owned(), "EN".to_owned()]),
            ("english", vec!["english".to_owned()]),
            ("en", vec!["en-".to_owned()]),
        ] {
            assert!(
                matches!(
                    normalize_site_locales(default, &enabled),
                    Err(StoreError::Conflict(_))
                ),
                "expected invalid locale settings: {default:?}, {enabled:?}"
            );
        }
    }
}
