//! Atomic persistence boundary for one generated alo Sites draft.
//!
//! AI types deliberately do not enter the store. The HTTP edge translates an
//! accepted proposal into these store-owned inputs; this module validates the
//! whole draft and commits the site, theme, and every page in one transaction.
//! There is no publish step here and no status input: a generated site is
//! always born `draft`.

use std::collections::HashSet;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{SiteFormId, SiteId, SitePageId};
use crate::site_forms::{FORM_NAME_MAX_CHARS, MAX_FORMS_PER_SITE};
use crate::site_model::{Section, SectionsEnvelope};
use crate::site_pages::{
    MAX_PAGES_PER_SITE, map_page_constraints, normalize_seo, validate_page_slug,
    validate_page_title,
};
use crate::site_theme::SiteTheme;
use crate::sites::{
    DEFAULT_SITE_LOCALE, map_subdomain_unique, validate_site_name, validate_subdomain,
};

const SEO_TITLE_MAX_CHARS: usize = 200;
const SEO_DESCRIPTION_MAX_CHARS: usize = 500;

/// One page in a complete generated draft.
#[derive(Debug, Clone)]
pub struct NewGeneratedSitePage {
    pub title: String,
    pub slug: String,
    pub is_home: bool,
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
    pub sections: SectionsEnvelope,
}

/// A validated generation proposal translated into store-owned values.
#[derive(Debug, Clone)]
pub struct NewGeneratedSite {
    pub name: String,
    pub subdomain: String,
    pub theme: SiteTheme,
    pub pages: Vec<NewGeneratedSitePage>,
}

/// The identifiers minted by one successful atomic draft creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedSiteDraft {
    pub site: SiteId,
    pub pages: Vec<SitePageId>,
}

struct PreparedPage {
    id: SitePageId,
    title: String,
    slug: String,
    is_home: bool,
    seo_title: Option<String>,
    seo_description: Option<String>,
    sections: serde_json::Value,
    forms: Vec<PreparedForm>,
}

struct PreparedForm {
    id: SiteFormId,
    name: String,
}

impl AccountStore {
    /// Creates a complete, unpublished site draft in one transaction.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] if any site, page, theme, or section rule is
    /// violated or the global subdomain is already claimed;
    /// [`StoreError::Db`] on persistence failure. No row is committed on any
    /// error.
    pub async fn create_generated_site(
        &self,
        draft: NewGeneratedSite,
    ) -> Result<GeneratedSiteDraft> {
        validate_site_name(&draft.name)?;
        validate_subdomain(&draft.subdomain)?;
        draft
            .theme
            .validate()
            .map_err(|error| StoreError::Conflict(error.to_string()))?;
        let theme = draft
            .theme
            .to_value()
            .map_err(|error| StoreError::Conflict(error.to_string()))?;

        if draft.pages.is_empty() || draft.pages.len() as i64 > MAX_PAGES_PER_SITE {
            return Err(StoreError::Conflict(format!(
                "a generated site must contain 1-{MAX_PAGES_PER_SITE} pages"
            )));
        }
        let mut homes = 0_usize;
        let mut slugs = HashSet::with_capacity(draft.pages.len());
        let mut pages = Vec::with_capacity(draft.pages.len());
        let mut form_count = 0_i64;
        for page in draft.pages {
            validate_page_title(&page.title)?;
            if page.is_home {
                homes += 1;
                if !page.slug.is_empty() {
                    return Err(StoreError::Conflict(
                        "the generated home page slug must be empty".to_owned(),
                    ));
                }
            } else {
                validate_page_slug(&page.slug)?;
            }
            if !slugs.insert(page.slug.clone()) {
                return Err(StoreError::Conflict(
                    "generated page slugs must be unique".to_owned(),
                ));
            }
            let seo_title =
                normalize_seo(page.seo_title.as_deref(), SEO_TITLE_MAX_CHARS, "SEO title")?;
            let seo_description = normalize_seo(
                page.seo_description.as_deref(),
                SEO_DESCRIPTION_MAX_CHARS,
                "SEO description",
            )?;
            let mut sections = page.sections;
            sections
                .validate()
                .map_err(|error| StoreError::Conflict(error.to_string()))?;
            let mut forms = Vec::new();
            for section in &mut sections.sections {
                let Section::ContactForm(contact) = section else {
                    continue;
                };
                if contact.form_id.is_some() {
                    return Err(StoreError::Conflict(
                        "generated contact forms may not carry a form id".to_owned(),
                    ));
                }
                form_count += 1;
                if form_count > MAX_FORMS_PER_SITE {
                    return Err(StoreError::Conflict(format!(
                        "a site may have at most {MAX_FORMS_PER_SITE} forms"
                    )));
                }
                let id = SiteFormId::generate();
                let name: String = contact
                    .heading
                    .as_deref()
                    .map(str::trim)
                    .filter(|heading| !heading.is_empty())
                    .unwrap_or("Contact form")
                    .chars()
                    .take(FORM_NAME_MAX_CHARS)
                    .collect();
                contact.form_id = Some(id.to_string());
                forms.push(PreparedForm { id, name });
            }
            let sections = sections
                .to_value()
                .map_err(|error| StoreError::Conflict(error.to_string()))?;
            pages.push(PreparedPage {
                id: SitePageId::generate(),
                title: page.title.trim().to_owned(),
                slug: page.slug,
                is_home: page.is_home,
                seo_title,
                seo_description,
                sections,
                forms,
            });
        }
        if homes != 1 {
            return Err(StoreError::Conflict(
                "a generated site must contain exactly one home page".to_owned(),
            ));
        }

        let site = SiteId::generate();
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO sites (tenant_id, id, name, subdomain, theme, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(draft.name.trim())
        .bind(&draft.subdomain)
        .bind(sqlx::types::Json(theme))
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(map_subdomain_unique)?;

        for page in &pages {
            for form in &page.forms {
                sqlx::query(
                    "INSERT INTO site_forms (tenant_id, site_id, id, name) \
                     VALUES ($1, $2, $3, $4)",
                )
                .bind(self.tenant.as_str())
                .bind(site.as_str())
                .bind(form.id.as_str())
                .bind(&form.name)
                .execute(&mut *tx)
                .await
                .map_err(StoreError::Db)?;
            }
        }

        for (nav_order, page) in pages.iter().enumerate() {
            let nav_order = i32::try_from(nav_order)
                .map_err(|_| StoreError::Conflict("too many generated pages".to_owned()))?;
            sqlx::query(
                "INSERT INTO site_pages \
                 (tenant_id, site_id, id, slug, title, sections, seo_title, seo_description, \
                  content_locale, nav_order, is_home) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
            )
            .bind(self.tenant.as_str())
            .bind(site.as_str())
            .bind(page.id.as_str())
            .bind(&page.slug)
            .bind(&page.title)
            .bind(sqlx::types::Json(&page.sections))
            .bind(&page.seo_title)
            .bind(&page.seo_description)
            .bind(DEFAULT_SITE_LOCALE)
            .bind(nav_order)
            .bind(page.is_home)
            .execute(&mut *tx)
            .await
            .map_err(map_page_constraints)?;
        }
        tx.commit().await.map_err(StoreError::Db)?;

        Ok(GeneratedSiteDraft {
            site,
            pages: pages.into_iter().map(|page| page.id).collect(),
        })
    }
}
