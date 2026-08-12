//! The public half of password-protected pages (ADR 0036, S2.06a): what the
//! anonymous `alo-sites` service is allowed to ask about a page's password.
//!
//! Exactly two questions, both scoped by a [`PublishedSite`] the Host header
//! resolved to — the caller never names a tenant, a site, or a publish:
//!
//! - *Which pages of the site I am serving are protected, and against which
//!   session version?* ([`SitePublicStore::published_page_protections`]) — the
//!   gate needs this before it can decide whether to serve bytes.
//! - *Does this password open that page?*
//!   ([`SitePublicStore::verify_page_password`]) — the answer is the session
//!   version or nothing; the stored hash never leaves the store.
//!
//! Both are deliberately blind: a page id that belongs to another tenant, or to
//! another site of the same tenant, is answered exactly like an unknown one.
//! An unprotected (or unknown) page still costs one argon2 run on the verify
//! path, so a guesser cannot learn which pages carry a password by timing the
//! answers.

use crate::error::{Result, StoreError};
use crate::id::SitePageId;
use crate::site_page_protection::{hash_site_page_password, verify_site_page_password};
use crate::site_public::{PublishedSite, SitePublicStore};

/// The longest page id this door will send to the database. Real ids are 22
/// characters (base64url of 16 random bytes); anything far outside that shape
/// is noise from the wire, not a lookup.
const PAGE_ID_MAX_LEN: usize = 64;

/// One protected page of a served site, as the public gate needs it: which
/// page, and the opaque token visitor sessions are minted against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedPageProtection {
    /// The page identity the served snapshots carry.
    pub page: SitePageId,
    /// The current session version. It changes whenever the password is set
    /// again, which is what ends sessions opened with the previous password.
    pub version: String,
}

impl SitePublicStore {
    /// Every protected page of the resolved site, with its session version.
    /// Read live rather than from the publish: a password set (or lifted) a
    /// moment ago has to hold on the very next request.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn published_page_protections(
        &self,
        site: &PublishedSite,
    ) -> Result<Vec<PublishedPageProtection>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT page_id, version FROM site_page_passwords \
             WHERE tenant_id = $1 AND site_id = $2",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(|(page_id, version)| PublishedPageProtection {
                page: SitePageId::new(page_id),
                version,
            })
            .collect())
    }

    /// Checks a visitor's password against the protection on `page` of the
    /// resolved site. `Some(version)` is the session token the caller may mint
    /// a cookie against; `None` is every refusal — wrong password, unprotected
    /// page, unknown page, another tenant's page — with no way to tell them
    /// apart, in the answer or in the time it takes.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn verify_page_password(
        &self,
        site: &PublishedSite,
        page: &str,
        password: &str,
    ) -> Result<Option<String>> {
        let stored: Option<(String, String)> = if page.is_empty() || page.len() > PAGE_ID_MAX_LEN {
            None
        } else {
            sqlx::query_as(
                "SELECT password_hash, version FROM site_page_passwords \
                 WHERE tenant_id = $1 AND site_id = $2 AND page_id = $3",
            )
            .bind(site.tenant.as_str())
            .bind(site.site.as_str())
            .bind(page)
            .fetch_optional(self.pool())
            .await
            .map_err(StoreError::Db)?
        };
        let candidate = password.to_owned();
        // argon2 is expensive on purpose: run it on a blocking thread so one
        // guess cannot stall the pages this worker is serving.
        tokio::task::spawn_blocking(move || match stored {
            Some((phc, version)) => verify_site_page_password(&candidate, &phc).then_some(version),
            // No protection to check, but the same work is done anyway and the
            // result discarded — a miss must not be measurably faster than a
            // wrong password.
            None => {
                let _ = hash_site_page_password(&candidate);
                None
            }
        })
        .await
        .map_err(|_| StoreError::Crypto)
    }
}
