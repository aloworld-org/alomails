//! The template catalog over HTTP (alo Sites, ADR 0036, S2.11a) — the manual
//! way to start a website, beside `POST /sites/generate`'s AI way.
//!
//! Three routes, and each is deliberately small:
//!
//! - `GET /sites/templates` answers the catalog itself. It is the same for
//!   every tenant and changes only when the build does, so it carries no
//!   tenant data at all — but it still authenticates, because the catalog is
//!   part of the product rather than of the public internet.
//! - `GET /sites/templates/{id}/preview` renders one template page through the
//!   **same renderer the public service uses**, as a self-contained document.
//!   The gallery therefore shows what the site would actually look like, not a
//!   screenshot that ages the moment a section changes.
//! - `POST /sites/templates/{id}` creates a draft site from the template. It
//!   translates the curated content into the store's own draft input and
//!   commits it through [`alo_store::AccountStore::create_generated_site`] —
//!   the single atomic door the AI path also goes through. Nothing here
//!   publishes.
//!
//! Errors follow the rest of the `/sites` surface: `401` unauthenticated,
//! `404` for a template this build does not ship (the same answer a mistyped
//! id gets), `400` for a missing name or address, and the store's own `422`
//! sentence — subdomain taken, name too long — passed through verbatim so the
//! dialog can show it (S1.30b).

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_sites::render::{
    ImageSources, PageRenderContext, SiteRenderContext, render_page_preview, strings_for,
};
use alo_sites::stylesheet::stylesheet;
use alo_store::{Section, SiteTemplate, SiteTemplatePage, site_template, site_templates};

use crate::error::Problem;
use crate::sites::{map_store_err, page_json, site_json, sites_domain};
use crate::state::{AppState, authenticate};

/// The language a template is written in. The catalog ships English copy; a
/// site made from it is created in the workspace's default site language and
/// can be translated afterwards like any other (S2.01d).
const TEMPLATE_LOCALE: &str = "en";

/// One template as JSON. The section kinds ride along so the gallery can say
/// what a template contains ("hero, features, contact form") without fetching
/// a preview for every card.
fn template_json(template: &SiteTemplate) -> Value {
    json!({
        "id": template.id,
        "version": template.version,
        "kind": template.kind,
        "name": template.name,
        "summary": template.summary,
        "themePreset": template.theme_preset,
        "pages": template.pages.iter().map(template_page_json).collect::<Vec<_>>(),
    })
}

fn template_page_json(page: &SiteTemplatePage) -> Value {
    json!({
        "title": page.title,
        "slug": page.slug,
        "home": page.is_home,
        "path": page.path(),
        "sectionKinds": page.sections.sections.iter().map(Section::kind).collect::<Vec<_>>(),
    })
}

/// `GET /sites/templates` → `{"templates":[…]}` — the shipped catalog in
/// gallery order.
pub async fn list_templates(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    authenticate(&state, &headers).await?;
    Ok(Json(json!({
        "templates": site_templates().iter().map(template_json).collect::<Vec<_>>(),
    })))
}

/// `?page=` on a template preview: which page of the template to render.
#[derive(Deserialize)]
pub struct TemplatePreviewQuery {
    page: Option<String>,
}

/// `GET /sites/templates/{id}/preview?page=<slug>` → the template page as one
/// complete `text/html` document, styled by its own theme preset.
///
/// Without `page`, the home page is rendered — the view the gallery card
/// shows. Templates carry no images by construction, so nothing is inlined
/// beyond the stylesheet and the document is fully self-contained; the same
/// sandboxed iframe the editor's draft preview uses can show it.
/// `Cache-Control: no-store`, like every other preview on this origin.
pub async fn preview_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<TemplatePreviewQuery>,
) -> Result<Response, Problem> {
    authenticate(&state, &headers).await?;
    let template = require_template(&id)?;
    let wanted = query.page.as_deref().map(str::trim).unwrap_or_default();
    let page = if wanted.is_empty() {
        template.home()
    } else {
        template.page(wanted)
    }
    .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such page in this template"))?;

    let theme = template.theme();
    let sections = page.sections.to_value().map_err(|_| {
        // Unreachable for a loaded template: the catalog parsed from exactly
        // this shape. Refusing beats rendering half a document.
        Problem::server_error()
    })?;
    // A template has no subdomain of its own; the preview origin is a stable
    // placeholder so canonical and OG URLs are well-formed rather than absent.
    let base_url = format!("https://preview.{}", sites_domain());
    let images = HashMap::new();
    // A template ships neither a collection nor a catalog (both would bind to
    // tenant data a template cannot own — `site_templates` refuses them), so
    // both frozen sets are deliberately empty here.
    let collections = HashMap::new();
    let catalogs = HashMap::new();
    // A template may not ship a bookable service (it would have to name a
    // calendar the tenant has not made), so this map is empty by construction.
    let bookings = HashMap::new();
    let site_ctx = SiteRenderContext {
        name: &template.name,
        base_url: &base_url,
        locale: TEMPLATE_LOCALE,
        theme: &theme,
        strings: strings_for(TEMPLATE_LOCALE),
        images: ImageSources::Inline(&images),
    };
    let path = page.path();
    let page_ctx = PageRenderContext {
        path: &path,
        title: &page.title,
        seo_title: page.seo_title.as_deref(),
        seo_description: page.seo_description.as_deref(),
        sections: &sections,
        collections: &collections,
        catalogs: &catalogs,
        bookings: &bookings,
    };
    Ok((
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        render_page_preview(&site_ctx, &page_ctx, &stylesheet(&theme)),
    )
        .into_response())
}

/// `POST /sites/templates/{id}` body.
#[derive(Deserialize)]
struct InstantiateBody {
    name: String,
    subdomain: String,
}

/// `POST /sites/templates/{id}` `{name, subdomain}` → `{site, pages,
/// template}` — one complete draft site with the template's pages, its theme
/// preset and its contact form already linked.
///
/// The response is the shape `POST /sites/generate` answers with, plus the
/// template it was built from, so the editor opens on the new home page by the
/// same path either way (S1.30c). The site is a draft: making a template live
/// is still an explicit publish.
pub async fn instantiate_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let template = require_template(&id)?;
    let req: InstantiateBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let name = req.name.trim().to_owned();
    let subdomain = req.subdomain.trim().to_ascii_lowercase();
    if name.is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "Give the website a name.",
        ));
    }
    if subdomain.is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "Give the website an address.",
        ));
    }

    let created = account
        .acc
        .create_generated_site(template.draft(name, subdomain))
        .await
        .map_err(map_store_err)?;
    let site = account
        .acc
        .site(&created.site)
        .await
        .map_err(map_store_err)?
        .ok_or_else(Problem::server_error)?;
    let pages = account
        .acc
        .site_pages(&created.site)
        .await
        .map_err(map_store_err)?;

    Ok(Json(json!({
        "site": site_json(&site),
        "pages": pages.iter().map(|page| page_json(page, true)).collect::<Vec<_>>(),
        "template": { "id": template.id, "version": template.version },
    })))
}

/// The template, or the `404` an id this build does not ship gets.
fn require_template(id: &str) -> Result<&'static SiteTemplate, Problem> {
    site_template(id).ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such template"))
}
