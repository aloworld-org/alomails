//! The shipped site-template catalog (alo Sites, ADR 0036, S2.11a) — the
//! manual, AI-off way to start a website.
//!
//! A template here is **curated content shipped with the build**, not tenant
//! data: there is no table, no migration and no per-tenant row, because every
//! tenant sees the same catalog and nobody edits it through the product. The
//! source of truth is `site_templates/catalog.json`, parsed once into the same
//! [`SectionsEnvelope`](crate::site_model::SectionsEnvelope) types the editor
//! writes — so a template that would not survive the section gate cannot be
//! shipped, and the gallery cannot offer a site the store would refuse.
//!
//! Four rules decide what a template may contain, and each is a decision
//! rather than an implementation detail.
//!
//! - **A template carries no images.** Every picture on a site is a tenant
//!   blob, and the catalog is tenant-less, so a shipped `text_image` or
//!   `gallery` section could only point at a blob that does not exist. The
//!   image-bearing sections are therefore excluded by construction and the
//!   loader refuses a template that carries one; the copy invites the owner to
//!   add pictures in the editor, where the blobs are theirs.
//! - **A template never makes a claim only the customer can make.** No
//!   invented testimonial, no invented team member, no invented price: a
//!   tenant who publishes a template unedited must not thereby publish a lie
//!   about who praised them, who works for them or what they charge. Prices in
//!   the product template are the em dash placeholder, blank on purpose.
//! - **Every internal link resolves inside the template.** A shipped site with
//!   a dead menu item is a broken site the first time it is published, so the
//!   loader checks each site-relative `href` against the template's own page
//!   paths.
//! - **Instantiating uses the one existing door.** [`SiteTemplate::draft`]
//!   translates a template into
//!   [`NewGeneratedSite`](crate::site_generation::NewGeneratedSite), and
//!   [`create_generated_site`](crate::account::AccountStore::create_generated_site)
//!   persists it — the same atomic transaction, the same validation, the same
//!   contact-form linking and the same "born as a draft, never published" rule
//!   the AI path gets. There is no second persistence path to keep in step.
//!
//! Versioning is per template: `version` is bumped whenever curated copy
//! changes, so a support conversation can name the exact content a site was
//! started from. The catalog is versioned, not the sites made from it — a site
//! is the tenant's from the moment it exists, and nothing later reaches back
//! into it.
//!
//! A template that violates a rule is dropped from the catalog with a
//! `tracing::error!` rather than panicking: a malformed shipped file must not
//! take a running server down. The integration test asserts that every
//! template in the JSON survives loading, so the failure is caught by the gate
//! long before a deploy.

use std::collections::HashSet;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::site_generation::{NewGeneratedSite, NewGeneratedSitePage};
use crate::site_model::{Section, SectionsEnvelope};
use crate::site_pages::{MAX_PAGES_PER_SITE, validate_page_slug, validate_page_title};
use crate::site_theme::{SiteTheme, THEME_SCHEMA_VERSION, theme_preset};

/// The curated catalog, embedded in the binary.
const CATALOG_JSON: &str = include_str!("site_templates/catalog.json");

/// The only price string a shipped template may carry: a placeholder that
/// reads as "not set yet" in any language, rather than a number the tenant
/// never chose.
pub const TEMPLATE_PLACEHOLDER_PRICE: &str = "—";

/// One page of a shipped template, in the shape the editor stores.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteTemplatePage {
    /// The page title, and the label the editor opens it under.
    pub title: String,
    /// Site-relative slug; empty for the home page.
    #[serde(default)]
    pub slug: String,
    /// Whether this is the template's single home page.
    #[serde(default)]
    pub is_home: bool,
    /// Optional SEO title override.
    #[serde(default)]
    pub seo_title: Option<String>,
    /// Optional SEO meta description.
    #[serde(default)]
    pub seo_description: Option<String>,
    /// The page's sections, already in the stored envelope shape.
    pub sections: SectionsEnvelope,
}

impl SiteTemplatePage {
    /// The public path this page would be served at, home being `/`.
    pub fn path(&self) -> String {
        if self.is_home {
            "/".to_owned()
        } else {
            format!("/{}", self.slug)
        }
    }
}

/// One curated template: a complete small website, ready to be created as a
/// draft and then edited like any other.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteTemplate {
    /// Stable catalog id (`[a-z0-9-]`), used in URLs and never reused for
    /// different content.
    pub id: String,
    /// Content version of this template, bumped whenever its copy changes.
    pub version: u32,
    /// Coarse site type, for grouping in the gallery ("services",
    /// "hospitality", "creative", "nonprofit", "product", "local-services").
    pub kind: String,
    /// Human name of the template.
    pub name: String,
    /// One-sentence description of who it is for and what it contains.
    pub summary: String,
    /// The shipped theme preset this template is designed against.
    pub theme_preset: String,
    /// The pages it creates, in navigation order; exactly one is the home.
    pub pages: Vec<SiteTemplatePage>,
}

impl SiteTemplate {
    /// The template's own page paths, in order — the set an internal link may
    /// point at.
    pub fn page_paths(&self) -> Vec<String> {
        self.pages.iter().map(SiteTemplatePage::path).collect()
    }

    /// The page the gallery previews and the editor opens first.
    pub fn home(&self) -> Option<&SiteTemplatePage> {
        self.pages
            .iter()
            .find(|page| page.is_home)
            .or_else(|| self.pages.first())
    }

    /// The page with this slug (the home page being the empty slug).
    pub fn page(&self, slug: &str) -> Option<&SiteTemplatePage> {
        self.pages.iter().find(|page| page.slug == slug)
    }

    /// The theme a site made from this template starts with: the template's
    /// preset, no logo, no favicon (both are tenant blobs).
    pub fn theme(&self) -> SiteTheme {
        SiteTheme {
            schema_version: THEME_SCHEMA_VERSION,
            preset: self.theme_preset.clone(),
            logo: None,
            favicon: None,
        }
    }

    /// Translates this template into the store-owned draft input.
    ///
    /// The result goes through
    /// [`create_generated_site`](crate::account::AccountStore::create_generated_site)
    /// like an accepted AI proposal: same validation, same transaction, same
    /// unpublished result. Two calls with the same name and subdomain produce
    /// byte-identical page content — only the minted ids differ.
    pub fn draft(&self, name: String, subdomain: String) -> NewGeneratedSite {
        NewGeneratedSite {
            name,
            subdomain,
            theme: self.theme(),
            pages: self
                .pages
                .iter()
                .map(|page| NewGeneratedSitePage {
                    title: page.title.clone(),
                    slug: page.slug.clone(),
                    is_home: page.is_home,
                    seo_title: page.seo_title.clone(),
                    seo_description: page.seo_description.clone(),
                    sections: page.sections.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    templates: Vec<SiteTemplate>,
}

/// The shipped catalog, in gallery order.
pub fn site_templates() -> &'static [SiteTemplate] {
    static CATALOG: OnceLock<Vec<SiteTemplate>> = OnceLock::new();
    CATALOG.get_or_init(load_catalog).as_slice()
}

/// One shipped template by id, or `None` for an id this build does not ship.
pub fn site_template(id: &str) -> Option<&'static SiteTemplate> {
    site_templates().iter().find(|template| template.id == id)
}

/// Parses and checks the embedded catalog once. Anything that fails a rule is
/// left out and logged; nothing here panics.
fn load_catalog() -> Vec<SiteTemplate> {
    let catalog: Catalog = match serde_json::from_str(CATALOG_JSON) {
        Ok(catalog) => catalog,
        Err(error) => {
            tracing::error!(%error, "the shipped site template catalog does not parse");
            return Vec::new();
        }
    };
    let mut ids = HashSet::with_capacity(catalog.templates.len());
    let mut kept = Vec::with_capacity(catalog.templates.len());
    for template in catalog.templates {
        if let Err(reason) = check_template(&template) {
            tracing::error!(
                template = %template.id,
                %reason,
                "a shipped site template breaks a catalog rule and was not loaded"
            );
            continue;
        }
        if !ids.insert(template.id.clone()) {
            tracing::error!(template = %template.id, "duplicate shipped site template id");
            continue;
        }
        kept.push(template);
    }
    kept
}

/// The catalog rules, checked at load and asserted by the integration test.
///
/// # Errors
/// A one-line reason naming the violated rule.
pub fn check_template(template: &SiteTemplate) -> Result<(), String> {
    if template.id.is_empty()
        || !template
            .id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("template id must be lowercase letters, digits or hyphens".to_owned());
    }
    if template.version == 0 {
        return Err("template version starts at 1".to_owned());
    }
    for (field, value) in [
        ("name", &template.name),
        ("summary", &template.summary),
        ("kind", &template.kind),
    ] {
        if value.trim().is_empty() {
            return Err(format!("template {field} must not be blank"));
        }
    }
    if theme_preset(&template.theme_preset).is_none() {
        return Err(format!(
            "theme preset {} is not shipped",
            template.theme_preset
        ));
    }

    let max_pages = usize::try_from(MAX_PAGES_PER_SITE).unwrap_or(usize::MAX);
    if template.pages.is_empty() || template.pages.len() > max_pages {
        return Err(format!("a template must contain 1-{max_pages} pages"));
    }
    let mut homes = 0_usize;
    let mut slugs = HashSet::with_capacity(template.pages.len());
    for page in &template.pages {
        validate_page_title(&page.title).map_err(|error| error.to_string())?;
        if page.is_home {
            homes += 1;
            if !page.slug.is_empty() {
                return Err("the home page slug must be empty".to_owned());
            }
        } else {
            validate_page_slug(&page.slug).map_err(|error| error.to_string())?;
        }
        if !slugs.insert(page.slug.as_str()) {
            return Err(format!("duplicate page slug {:?}", page.slug));
        }
        page.sections
            .validate()
            .map_err(|error| error.to_string())?;
    }
    if homes != 1 {
        return Err("a template must contain exactly one home page".to_owned());
    }

    let paths = template.page_paths();
    for page in &template.pages {
        for section in &page.sections.sections {
            check_section_content(section)?;
            for href in section_hrefs(section) {
                if href.starts_with('/') && !paths.iter().any(|path| path == href) {
                    return Err(format!("link {href:?} points at no page of this template"));
                }
            }
        }
    }
    Ok(())
}

/// The content rules a shipped section obeys on top of the section schema: no
/// tenant blobs, no collection binding, and no claim only the customer can
/// make.
fn check_section_content(section: &Section) -> Result<(), String> {
    if !section.images().is_empty() {
        return Err(format!(
            "the {} section carries an image, which a tenant-less template cannot own",
            section.kind()
        ));
    }
    match section {
        Section::Testimonials(_) => {
            Err("a template may not ship an invented testimonial".to_owned())
        }
        Section::Team(_) => Err("a template may not ship an invented team member".to_owned()),
        Section::Collection(_) => Err(
            "a template may not bind a collection to a table the tenant has not made".to_owned(),
        ),
        Section::Catalog(_) => {
            Err("a template may not ship a catalog the tenant has not made".to_owned())
        }
        Section::Booking(_) => Err(
            "a template may not ship a bookable service bound to a calendar the tenant has not \
             made"
                .to_owned(),
        ),
        // A template is code we ship into other people's sites. Shipping a
        // block of executable JavaScript that way makes the catalog a supply
        // chain, and a tenant would have no reason to read it before pressing
        // Use. Custom code is written by the tenant, for one site, or not at
        // all.
        Section::CustomCode(_) => {
            Err("a template may not ship custom code for a tenant to run".to_owned())
        }
        Section::Pricing(pricing) => {
            for tier in &pricing.tiers {
                if tier.price != TEMPLATE_PLACEHOLDER_PRICE {
                    return Err(format!(
                        "a template may not ship a price; tier {:?} must use {TEMPLATE_PLACEHOLDER_PRICE:?}",
                        tier.name
                    ));
                }
            }
            Ok(())
        }
        Section::TextImage(_) | Section::Gallery(_) => Err(format!(
            "the {} section always needs an image a template cannot own",
            section.kind()
        )),
        Section::Nav(_)
        | Section::Hero(_)
        | Section::Features(_)
        | Section::Faq(_)
        | Section::Cta(_)
        | Section::ContactForm(_)
        // A tickets section binds nothing and claims nothing: the events and
        // prices behind its link are the tenant's own live Billing state.
        | Section::Tickets(_)
        | Section::Footer(_) => Ok(()),
    }
}

/// Every link target this section renders, in document order. The match is
/// exhaustive on purpose: a new section variant does not compile until it says
/// whether it links anywhere.
fn section_hrefs(section: &Section) -> Vec<&str> {
    match section {
        Section::Nav(nav) => nav
            .links
            .iter()
            .chain(nav.cta.iter())
            .map(|link| link.href.as_str())
            .collect(),
        Section::Hero(hero) => hero
            .primary_cta
            .iter()
            .chain(hero.secondary_cta.iter())
            .map(|link| link.href.as_str())
            .collect(),
        Section::Cta(cta) => vec![cta.button.href.as_str()],
        Section::Footer(footer) => footer.links.iter().map(|link| link.href.as_str()).collect(),
        Section::Pricing(pricing) => pricing
            .tiers
            .iter()
            .filter_map(|tier| tier.cta.as_ref())
            .map(|link| link.href.as_str())
            .collect(),
        Section::Features(_)
        | Section::TextImage(_)
        | Section::Gallery(_)
        | Section::Testimonials(_)
        | Section::Team(_)
        | Section::Faq(_)
        | Section::ContactForm(_)
        | Section::Collection(_)
        | Section::Catalog(_)
        | Section::Booking(_)
        // Its `/tix` link is the renderer's own, not a stored href.
        | Section::Tickets(_)
        | Section::CustomCode(_) => Vec::new(),
    }
}
