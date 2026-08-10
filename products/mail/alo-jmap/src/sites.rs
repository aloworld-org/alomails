//! alo Sites edit surface (ADR 0036): the authenticated `/sites/*` routes —
//! site CRUD, page CRUD, section operations, theme, and publish. This is the
//! edit half of the two-service boundary in `docs/design/sites.md`; the
//! public serving half is the separate `alo-sites` binary, which reads only
//! published snapshots.
//!
//! Every handler resolves the caller with [`authenticate`] and reaches data
//! only through the account door, so an id from another tenant simply does
//! not resolve (`404`, indistinguishable from nonexistent). Rule violations —
//! bad subdomain, reserved word, slug collision, section or theme JSON
//! failing the typed schema, publish preconditions, and a subdomain taken by
//! any tenant (taken/free is all the message says) — are `422` with the
//! store's rule-naming message as detail, the contract the design note
//! publishes. The sites store family spells all of those as
//! [`StoreError::Conflict`], so this module maps `Conflict` to `422`, not
//! `409`.
//!
//! Sections are addressed **by index** into the page's ordered envelope
//! (`docs/design/sites.md`): they are entries of one JSON document with no
//! identity of their own, and the AI edit ops (S1.27) speak the same index
//! vocabulary. Section operations are read-modify-write through the store's
//! schema write gate; the editor is single-writer per page by design, so no
//! optimistic-concurrency header exists yet (recorded as an S2 seam).

use std::collections::BTreeMap;
use std::sync::Arc;

use alo_ai::{InferenceError, SiteDraftError, SiteEditEnvelope, SiteEditError};
use axum::Json;
use axum::extract::{Extension, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

use alo_sites::render::{
    EN, ImageSources, PageRenderContext, SiteRenderContext, render_page_preview, sections_lenient,
};
use alo_sites::stylesheet::stylesheet;
use alo_store::{
    BlobId, DriveNodeId, NewGeneratedSite, NewGeneratedSitePage, NewSitePost, Section,
    SectionsEnvelope, Site, SiteDomain, SiteDomainStatus, SiteFormId, SiteFormSubmissionId, SiteId,
    SitePage, SitePageId, SitePost, SitePostId, SitePostUpdate, SiteTheme, StoreError,
    normalize_site_domain, site_theme::THEME_PRESETS,
};

use crate::ai::tenant_ai_config;
use crate::error::Problem;
use crate::state::{Account, AppState, authenticate};

/// TXT lookup boundary used by custom-domain verification. Production uses
/// the system resolver; tests inject a deterministic implementation and never
/// call external DNS.
pub trait SiteDomainTxtLookup: Send + Sync {
    fn lookup(&self, name: String) -> BoxFuture<'static, Vec<String>>;
}

/// Production TXT lookup through the same Hickory system resolver as the
/// Security & trust checks.
pub struct SystemSiteDomainTxtLookup;

impl SiteDomainTxtLookup for SystemSiteDomainTxtLookup {
    fn lookup(&self, name: String) -> BoxFuture<'static, Vec<String>> {
        Box::pin(async move {
            let Some(resolver) = crate::security::build_resolver() else {
                return Vec::new();
            };
            crate::security::txt_records(&resolver, &name).await
        })
    }
}

/// DNS label and value used to prove control of a custom site host.
const SITE_DOMAIN_VERIFY_PREFIX: &str = "_alo-sites";
const SITE_DOMAIN_VERIFY_VALUE_PREFIX: &str = "alo-site-verification=";

/// A business description is prompt input, not a document upload. Bound it
/// before buffering independently of the server's larger upload ceiling.
pub const MAX_SITE_GENERATE_BYTES: usize = 16 * 1024;
const MAX_SITE_DESCRIPTION_CHARS: usize = 8_000;
pub const MAX_SITE_EDIT_BYTES: usize = 64 * 1024;
const MAX_SITE_EDIT_INSTRUCTION_CHARS: usize = 4_000;

// ---- JSON shaping -----------------------------------------------------------

fn iso(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

/// A site as JSON. `theme` is the stored envelope (or the pristine `{}` of a
/// site that never set one) — always a value that passed the theme gate.
fn site_json(s: &Site) -> Value {
    json!({
        "id": s.id.as_str(),
        "name": s.name,
        "subdomain": s.subdomain,
        "status": s.status.as_str(),
        "theme": s.theme,
        "createdAt": iso(s.created_at),
        "updatedAt": iso(s.updated_at),
    })
}

fn site_domain_json(domain: &SiteDomain) -> Value {
    json!({
        "domain": domain.domain,
        "status": domain.status.as_str(),
        "verifiedAt": domain.verified_at.map(iso),
        "verifyRecord": {
            "name": format!("{SITE_DOMAIN_VERIFY_PREFIX}.{}", domain.domain),
            "type": "TXT",
            "value": format!("{SITE_DOMAIN_VERIFY_VALUE_PREFIX}{}", domain.verify_token),
        },
        "createdAt": iso(domain.created_at),
        "updatedAt": iso(domain.updated_at),
    })
}

/// A page as JSON. The sections envelope rides along only where the caller
/// asked for one page (`with_sections`); the list stays lean.
fn page_json(p: &SitePage, with_sections: bool) -> Value {
    let mut j = json!({
        "id": p.id.as_str(),
        "slug": p.slug,
        "title": p.title,
        "seoTitle": p.seo_title,
        "seoDescription": p.seo_description,
        "navOrder": p.nav_order,
        "home": p.is_home,
        "createdAt": iso(p.created_at),
        "updatedAt": iso(p.updated_at),
    });
    if with_sections && let Some(obj) = j.as_object_mut() {
        obj.insert("sections".to_owned(), p.sections.clone());
    }
    j
}

fn post_json(post: &SitePost) -> Value {
    json!({
        "id": post.id.as_str(),
        "docNodeId": post.doc_node_id.as_str(),
        "slug": post.slug,
        "title": post.title,
        "excerpt": post.excerpt,
        "coverBlobId": post.cover_blob_id.as_ref().map(BlobId::as_str),
        "status": post.status.as_str(),
        "publishedAt": post.published_at.map(iso),
        "createdAt": iso(post.created_at),
        "updatedAt": iso(post.updated_at),
    })
}

/// The sites-module error map (`docs/design/sites.md` → Errors). The sites
/// store spells every rule violation as `Conflict` with a message naming the
/// violated rule and never echoing another tenant's data, and the design note
/// publishes all of them — subdomain-taken included — as `422`.
fn map_store_err(e: StoreError) -> Problem {
    match e {
        StoreError::NotFound => Problem::with(StatusCode::NOT_FOUND, "not found"),
        StoreError::Forbidden => Problem::with(StatusCode::FORBIDDEN, "forbidden"),
        StoreError::Conflict(msg) | StoreError::Validation(msg) => {
            Problem::with(StatusCode::UNPROCESSABLE_ENTITY, msg)
        }
        _ => Problem::server_error(),
    }
}

// ---- sites ------------------------------------------------------------------

/// `GET /sites` → `{"sites":[...]}` — the tenant's sites.
pub async fn list_sites(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let sites = account.acc.sites().await.map_err(map_store_err)?;
    Ok(Json(
        json!({ "sites": sites.iter().map(site_json).collect::<Vec<_>>() }),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GenerateSiteBody {
    description: String,
}

fn generation_problem(error: &SiteDraftError) -> Problem {
    match error {
        SiteDraftError::Inference(InferenceError::Disabled | InferenceError::NotConfigured) =>
            Problem::with(
                StatusCode::SERVICE_UNAVAILABLE,
                "Website generation is not configured. You can create a blank site instead.",
            )
            .with_extra(json!({ "reason": "unconfigured" })),
        SiteDraftError::Inference(
            InferenceError::Backend(_) | InferenceError::Transport | InferenceError::Empty,
        ) => Problem::with(
            StatusCode::BAD_GATEWAY,
            "Website generation could not reach the configured AI service. Try again shortly.",
        )
        .with_extra(json!({ "reason": "unreachable" })),
        SiteDraftError::MissingObject
        | SiteDraftError::UnsupportedVersion(_)
        | SiteDraftError::Shape(_)
        | SiteDraftError::Invalid(_)
        | SiteDraftError::RepairFailed(_) => Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "AI could not create a valid website. Nothing was changed; refine the description and try again.",
        )
        .with_extra(json!({ "reason": "invalid_generation" })),
    }
}

/// `POST /sites/generate` `{description}` creates one complete draft site.
///
/// The model may propose content, but it never owns persistence. Its complete
/// envelope passes the strict alo-ai parser, is translated to store-owned
/// inputs, and is committed atomically. This route never publishes.
pub async fn generate_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: GenerateSiteBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let description = req.description.trim();
    if description.is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "Describe the business or website to generate.",
        ));
    }
    if description.chars().count() > MAX_SITE_DESCRIPTION_CHARS {
        return Err(Problem::with(
            StatusCode::PAYLOAD_TOO_LARGE,
            "The website description is too long. Shorten it and try again.",
        ));
    }

    let config = tenant_ai_config(&account).await.map_err(|problem| {
        if problem.status == StatusCode::SERVICE_UNAVAILABLE {
            Problem::with(
                StatusCode::SERVICE_UNAVAILABLE,
                "Website generation is not configured. You can create a blank site instead.",
            )
            .with_extra(json!({ "reason": "unconfigured" }))
        } else {
            problem
        }
    })?;
    let proposal = alo_ai::generate_site_draft(&config, description)
        .await
        .map_err(|error| generation_problem(&error))?;
    let draft = NewGeneratedSite {
        name: proposal.site.name,
        subdomain: proposal.site.subdomain,
        theme: proposal.site.theme,
        pages: proposal
            .pages
            .into_iter()
            .map(|page| NewGeneratedSitePage {
                title: page.title,
                slug: page.slug,
                is_home: page.is_home,
                seo_title: page.seo_title,
                seo_description: page.seo_description,
                sections: page.sections,
            })
            .collect(),
    };
    let created = account
        .acc
        .create_generated_site(draft)
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
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposeSiteEditBody {
    instruction: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplySiteEditBody {
    proposal: SiteEditEnvelope,
}

fn site_edit_problem(error: &SiteEditError) -> Problem {
    match error {
        SiteEditError::Inference(InferenceError::Disabled | InferenceError::NotConfigured) => {
            Problem::with(
                StatusCode::SERVICE_UNAVAILABLE,
                "AI editing is not configured. You can keep editing sections directly.",
            )
            .with_extra(json!({ "reason": "unconfigured" }))
        }
        SiteEditError::Inference(
            InferenceError::Backend(_) | InferenceError::Transport | InferenceError::Empty,
        ) => Problem::with(
            StatusCode::BAD_GATEWAY,
            "AI editing could not reach the configured service. Try again shortly.",
        )
        .with_extra(json!({ "reason": "unreachable" })),
        _ => Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("The proposed website change is not safe to apply: {error}"),
        )
        .with_extra(json!({ "reason": "invalid_proposal" })),
    }
}

/// `POST /sites/:id/pages/:pid/ai-edits` `{instruction}` proposes a typed
/// operation set for review. It loads the caller-owned current page, validates
/// the proposal against that exact envelope, and writes nothing.
pub async fn propose_page_edit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let sid = SiteId::new(id);
    let page_id = SitePageId::new(pid);
    let page = page_record(&account, &sid, &page_id).await?;
    let current = parse_envelope(page.sections.clone())?;
    let req: ProposeSiteEditBody =
        serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let instruction = req.instruction.trim();
    if instruction.is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "Describe the change you want to make.",
        ));
    }
    if instruction.chars().count() > MAX_SITE_EDIT_INSTRUCTION_CHARS {
        return Err(Problem::with(
            StatusCode::PAYLOAD_TOO_LARGE,
            "The change request is too long. Shorten it and try again.",
        ));
    }
    let config = tenant_ai_config(&account).await.map_err(|problem| {
        if problem.status == StatusCode::SERVICE_UNAVAILABLE {
            Problem::with(
                StatusCode::SERVICE_UNAVAILABLE,
                "AI editing is not configured. You can keep editing sections directly.",
            )
            .with_extra(json!({ "reason": "unconfigured" }))
        } else {
            problem
        }
    })?;
    let proposal = alo_ai::propose_site_edit(&config, &current, instruction)
        .await
        .map_err(|error| site_edit_problem(&error))?;
    let proposed =
        alo_ai::apply_site_edit(&current, &proposal).map_err(|error| site_edit_problem(&error))?;
    let proposed_value = proposed.to_value().map_err(|_| Problem::server_error())?;
    let site = require_site(&account, &sid).await?;
    let preview_html = render_preview_html(&account, &site, &page, &proposed_value).await;
    Ok(Json(json!({
        "proposal": proposal,
        "previewHtml": preview_html,
    })))
}

/// `PUT /sites/:id/pages/:pid/ai-edits` `{proposal}` applies an explicitly
/// approved operation set. It re-loads the page and re-applies every guarded
/// target, so a proposal made stale by another edit is refused rather than
/// overwriting newer work.
pub async fn apply_page_edit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ApplySiteEditBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let sid = SiteId::new(id);
    let page_id = SitePageId::new(pid);
    let current = page_envelope(&account, &sid, &page_id).await?;
    let result = alo_ai::apply_site_edit(&current, &req.proposal)
        .map_err(|error| site_edit_problem(&error))?;
    store_sections(&account, &sid, &page_id, &result).await
}

// ---- custom domains --------------------------------------------------------

/// `GET /sites/:id/domains` → `{domains:[...]}` for an owned site.
pub async fn list_domains(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let domains = account
        .acc
        .site_domains(&SiteId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "domains": domains.iter().map(site_domain_json).collect::<Vec<_>>()
    })))
}

#[derive(Deserialize)]
struct SiteDomainBody {
    domain: String,
}

/// `POST /sites/:id/domains` `{domain}` → a pending claim and the exact TXT
/// ownership record to publish.
pub async fn create_domain(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let body: SiteDomainBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let domain = account
        .acc
        .create_site_domain(&SiteId::new(id), &body.domain)
        .await
        .map_err(map_store_err)?;
    Ok(Json(site_domain_json(&domain)))
}

/// `DELETE /sites/:id/domains/:domain` releases an owned claim.
pub async fn delete_domain(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, domain)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_site_domain(&SiteId::new(id), &domain)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "deleted" })))
}

/// `POST /sites/:id/domains/:domain/verify` checks the current DNS TXT set.
/// A missing record is a normal, retryable 200 response; the claim changes to
/// `live` only after the exact opaque token is observed. Verification and
/// serving activation are one user action now that alo-sites supports the
/// custom Host; there is no second ceremony step to discover.
pub async fn verify_domain(
    State(state): State<AppState>,
    Extension(dns): Extension<Arc<dyn SiteDomainTxtLookup>>,
    headers: HeaderMap,
    Path((id, value)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    let domain = normalize_site_domain(&value).map_err(map_store_err)?;
    let claims = account
        .acc
        .site_domains(&site)
        .await
        .map_err(map_store_err)?;
    let claim = claims
        .into_iter()
        .find(|claim| claim.domain == domain)
        .ok_or_else(Problem::not_found)?;
    if claim.status != SiteDomainStatus::Pending {
        return Ok(Json(site_domain_json(&claim)));
    }

    let record_name = format!("{SITE_DOMAIN_VERIFY_PREFIX}.{domain}");
    let expected = format!("{SITE_DOMAIN_VERIFY_VALUE_PREFIX}{}", claim.verify_token);
    let found = dns
        .lookup(record_name)
        .await
        .iter()
        .any(|record| record.trim() == expected);
    if !found {
        return Ok(Json(site_domain_json(&claim)));
    }
    account
        .acc
        .verify_site_domain(&site, &domain)
        .await
        .map_err(map_store_err)?;
    let live = account
        .acc
        .activate_site_domain(&site, &domain)
        .await
        .map_err(map_store_err)?;
    Ok(Json(site_domain_json(&live)))
}

#[derive(Deserialize)]
struct SiteBody {
    name: String,
    subdomain: String,
}

/// `POST /sites` `{name, subdomain}` → the created site (status `draft`,
/// empty theme). The subdomain is claimed in the global namespace; a claim
/// that collides answers taken/free only, never the owner.
pub async fn create_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: SiteBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let id = account
        .acc
        .create_site(req.name.trim(), req.subdomain.trim())
        .await
        .map_err(map_store_err)?;
    let site = account
        .acc
        .site(&id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(Problem::server_error)?;
    Ok(Json(site_json(&site)))
}

#[derive(Deserialize)]
pub struct SubdomainQuery {
    subdomain: String,
}

/// `GET /sites/subdomain-check?subdomain=` → `{"subdomain","available"}` —
/// the live claim check for the create form. Syntactically invalid or
/// reserved labels are `422` naming the rule; a well-formed label answers
/// taken/free only.
pub async fn check_subdomain(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<SubdomainQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let subdomain = q.subdomain.trim().to_lowercase();
    let available = account
        .acc
        .subdomain_available(&subdomain)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "subdomain": subdomain, "available": available }),
    ))
}

/// `GET /sites/:id` → the site plus its current publish (`"publish"` is
/// `null` while unpublished — the live/draft status chip's data).
pub async fn get_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let sid = SiteId::new(id);
    let site = account
        .acc
        .site(&sid)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such site"))?;
    let publish = account
        .acc
        .current_site_publish(&sid)
        .await
        .map_err(map_store_err)?;
    let mut j = site_json(&site);
    if let Some(obj) = j.as_object_mut() {
        obj.insert(
            "publish".to_owned(),
            publish.map_or(
                Value::Null,
                |p| json!({ "id": p.id.as_str(), "publishedAt": iso(p.published_at) }),
            ),
        );
    }
    Ok(Json(j))
}

#[derive(Deserialize)]
pub struct AnalyticsQuery {
    days: Option<u16>,
}

/// `GET /sites/:id/analytics?days=30` -> privacy-preserving aggregate traffic
/// for the caller's site. Quiet days are included so the chart keeps an
/// honest time axis without inventing data in the browser.
pub async fn get_analytics(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<AnalyticsQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let days = query.days.unwrap_or(30);
    if !(1..=365).contains(&days) {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "analytics period must be between 1 and 365 days",
        ));
    }

    let to = OffsetDateTime::now_utc().date();
    let from = to - Duration::days(i64::from(days - 1));
    let report = account
        .acc
        .site_analytics(&SiteId::new(id), from, to)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such site"))?;
    let sparse = report
        .daily
        .iter()
        .map(|row| (row.day, row))
        .collect::<BTreeMap<_, _>>();
    let mut daily = Vec::with_capacity(usize::from(days));
    let mut day = from;
    while day <= to {
        let (visits, unique_visitors) = sparse
            .get(&day)
            .map_or((0, 0), |row| (row.visits, row.unique_visitors));
        daily.push(json!({
            "date": day.to_string(),
            "visits": visits,
            "uniqueVisitors": unique_visitors,
        }));
        day += Duration::days(1);
    }
    let visits = report.daily.iter().map(|row| row.visits).sum::<u64>();
    let unique_visitors = report
        .daily
        .iter()
        .map(|row| row.unique_visitors)
        .sum::<u64>();

    Ok(Json(json!({
        "from": from.to_string(),
        "to": to.to_string(),
        "totals": { "visits": visits, "uniqueVisitors": unique_visitors },
        "daily": daily,
        "topPages": report.top_pages.into_iter().map(|row| json!({
            "path": row.label,
            "visits": row.visits,
            "uniqueVisitors": row.unique_visitors,
        })).collect::<Vec<_>>(),
        "topReferrers": report.top_referrers.into_iter().map(|row| json!({
            "domain": row.label,
            "visits": row.visits,
            "uniqueVisitors": row.unique_visitors,
        })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
struct SiteEditBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    subdomain: Option<String>,
}

/// `PUT /sites/:id` `{name?, subdomain?}` → `{status:"ok"}` — rename and/or
/// move to a new subdomain; fields absent from the body are untouched.
pub async fn update_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: SiteEditBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    if req.name.is_none() && req.subdomain.is_none() {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "nothing to update: provide name and/or subdomain",
        ));
    }
    let sid = SiteId::new(id);
    if let Some(name) = &req.name {
        account
            .acc
            .rename_site(&sid, name.trim())
            .await
            .map_err(map_store_err)?;
    }
    if let Some(subdomain) = &req.subdomain {
        account
            .acc
            .set_site_subdomain(&sid, subdomain.trim())
            .await
            .map_err(map_store_err)?;
    }
    Ok(Json(json!({ "status": "ok" })))
}

/// `DELETE /sites/:id` → `{status:"ok"}` — deletes the site with its pages,
/// publishes, and snapshots (the site goes off the air), releasing the
/// subdomain.
pub async fn delete_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_site(&SiteId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

// ---- form submissions ------------------------------------------------------

struct SubmissionRow {
    id: String,
    form_id: String,
    form_name: String,
    sender_name: String,
    sender_email: String,
    message: String,
    handled: bool,
    received_at: OffsetDateTime,
}

async fn site_submissions(
    account: &Account,
    site: &SiteId,
) -> Result<(Site, Vec<SubmissionRow>), Problem> {
    let stored_site = account
        .acc
        .site(site)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such site"))?;

    let forms = account.acc.site_forms(site).await.map_err(map_store_err)?;
    let mut rows = Vec::new();
    for form in forms {
        for submission in account
            .acc
            .site_form_submissions(site, &form.id)
            .await
            .map_err(map_store_err)?
        {
            rows.push(SubmissionRow {
                id: submission.id.as_str().to_owned(),
                form_id: form.id.as_str().to_owned(),
                form_name: form.name.clone(),
                sender_name: submission.sender_name,
                sender_email: submission.sender_email,
                message: submission.message,
                handled: submission.handled,
                received_at: submission.received_at,
            });
        }
    }
    rows.sort_by_key(|row| std::cmp::Reverse(row.received_at));
    Ok((stored_site, rows))
}

/// `GET /sites/:id/submissions` -> every contact-form submission for the
/// site, newest first. Form labels ride with each row so the owner can tell
/// which page invitation produced it without another request.
pub async fn list_submissions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    let (_, rows) = site_submissions(&account, &site).await?;
    Ok(Json(json!({
        "submissions": rows.into_iter().map(|row| json!({
            "id": row.id,
            "formId": row.form_id,
            "formName": row.form_name,
            "senderName": row.sender_name,
            "senderEmail": row.sender_email,
            "message": row.message,
            "handled": row.handled,
            "receivedAt": iso(row.received_at),
        })).collect::<Vec<_>>()
    })))
}

/// Neutralises user-authored text before it reaches a spreadsheet cell.
/// Excel and compatible tools treat these leading characters as formulas,
/// including when whitespace precedes them; a leading apostrophe keeps the
/// visible text and prevents evaluation.
fn csv_text(value: &str) -> String {
    if value
        .trim_start()
        .chars()
        .next()
        .is_some_and(|first| matches!(first, '=' | '+' | '-' | '@'))
    {
        format!("'{value}")
    } else {
        value.to_owned()
    }
}

/// `GET /sites/:id/submissions.csv` -> the same tenant-scoped inbox as a
/// spreadsheet-ready download. The site subdomain is validated ASCII, so it
/// makes a recognisable filename without putting user-authored prose in a
/// response header.
pub async fn export_submissions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    let (stored_site, rows) = site_submissions(&account, &site).await?;
    let mut body = crate::csv::row(&[
        "receivedAt",
        "form",
        "senderName",
        "senderEmail",
        "message",
        "status",
    ]);
    for row in rows {
        let received_at = iso(row.received_at);
        let form = csv_text(&row.form_name);
        let sender_name = csv_text(&row.sender_name);
        let sender_email = csv_text(&row.sender_email);
        let message = csv_text(&row.message);
        let status = if row.handled {
            "handled"
        } else {
            "needs reply"
        };
        body.push_str(&crate::csv::row(&[
            &received_at,
            &form,
            &sender_name,
            &sender_email,
            &message,
            status,
        ]));
    }
    let file_name = format!("submissions-{}.csv", stored_site.subdomain);
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{file_name}\""),
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        body,
    )
        .into_response())
}

#[derive(Deserialize)]
struct HandledBody {
    handled: bool,
}

/// `PUT /sites/:id/forms/:form/submissions/:submission` `{handled}` -> ok.
/// The account door scopes every id, so an id copied from another tenant or
/// another site receives the same 404 as an invented one.
pub async fn set_submission_handled(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, form, submission)): Path<(String, String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: HandledBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    account
        .acc
        .set_form_submission_handled(
            &SiteId::new(id),
            &SiteFormId::new(form),
            &SiteFormSubmissionId::new(submission),
            req.handled,
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `GET /sites/theme-presets` → `{"presets":[...]}` — the shipped theme
/// presets in picker order (the first is the default), with the palette and
/// typography tokens the theme UI renders its swatches from. Static product
/// data, but authenticated like every `/sites/*` route — the edit surface
/// has no anonymous corners.
pub async fn list_theme_presets(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    authenticate(&state, &headers).await?;
    let presets: Vec<Value> = THEME_PRESETS
        .iter()
        .map(|preset| {
            json!({
                "id": preset.id,
                "name": preset.name,
                "palette": {
                    "background": preset.palette.background,
                    "surface": preset.palette.surface,
                    "text": preset.palette.text,
                    "mutedText": preset.palette.muted_text,
                    "primary": preset.palette.primary,
                    "onPrimary": preset.palette.on_primary,
                    "border": preset.palette.border,
                },
                "typography": {
                    "headingFamily": preset.typography.heading_family,
                    "bodyFamily": preset.typography.body_family,
                    "headingWeight": preset.typography.heading_weight,
                },
            })
        })
        .collect();
    Ok(Json(json!({ "presets": presets })))
}

/// `GET /sites/config` → `{"domain": <SITES_DOMAIN>}` — the deployment-wide
/// apex under which published sites serve (`<subdomain>.<domain>`, the same
/// contract the public `alo-sites` service is configured with). The web
/// composes the "goes live at" copy and live links from it instead of
/// hardcoding a domain. Static deployment data, but authenticated like every
/// `/sites/*` route — the edit surface has no anonymous corners.
pub async fn sites_config(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    authenticate(&state, &headers).await?;
    Ok(Json(json!({ "domain": sites_domain() })))
}

/// `PUT /sites/:id/theme` (body = the theme envelope) → `{status:"ok"}` —
/// the theme gate: the body must parse as a current-version [`alo_store::SiteTheme`].
pub async fn set_theme(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let theme: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    account
        .acc
        .set_site_theme(&SiteId::new(id), theme)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `POST /sites/:id/publish` → `{"publishId","status":"live"}` — freezes the
/// current pages + theme into immutable snapshots and points the public
/// service at them. A site with no pages or no home page is refused (`422`).
pub async fn publish_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let publish = account
        .acc
        .publish_site(&SiteId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "publishId": publish.as_str(), "status": "live" }),
    ))
}

/// `POST /sites/:id/unpublish` → `{"status":"draft"}` — takes the site off
/// the air (history is retained). Idempotent.
pub async fn unpublish_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .unpublish_site(&SiteId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "draft" })))
}

// ---- pages ------------------------------------------------------------------

/// Resolves the caller's site or answers `404` — used where the store read
/// alone could not distinguish another tenant's site from an empty one.
async fn require_site(account: &Account, site: &SiteId) -> Result<Site, Problem> {
    account
        .acc
        .site(site)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such site"))
}

/// `GET /sites/:id/pages` → `{"pages":[...]}` in navigation order (lean:
/// no sections — `GET` one page for its envelope).
pub async fn list_pages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let sid = SiteId::new(id);
    require_site(&account, &sid).await?;
    let pages = account.acc.site_pages(&sid).await.map_err(map_store_err)?;
    Ok(Json(json!({
        "pages": pages.iter().map(|p| page_json(p, false)).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
struct PageBody {
    title: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    home: bool,
}

/// `POST /sites/:id/pages` `{title, slug?, home?}` → the created page (at the
/// end of the nav order, empty sections). The empty slug is accepted only
/// together with `home: true` — it is the home page's spelling.
pub async fn create_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: PageBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let sid = SiteId::new(id);
    let pid = account
        .acc
        .create_site_page(&sid, req.title.trim(), req.slug.trim(), req.home)
        .await
        .map_err(map_store_err)?;
    let page = account
        .acc
        .site_page(&sid, &pid)
        .await
        .map_err(map_store_err)?
        .ok_or_else(Problem::server_error)?;
    Ok(Json(page_json(&page, true)))
}

/// `GET /sites/:id/pages/:pid` → the page including its sections envelope.
pub async fn get_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let page = account
        .acc
        .site_page(&SiteId::new(id), &SitePageId::new(pid))
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such page"))?;
    Ok(Json(page_json(&page, true)))
}

/// The apex domain draft previews advertise in canonical/OG URLs — the same
/// `SITES_DOMAIN` contract the public `alo-sites` service is configured
/// with (`docs/design/sites.md`). Optional here because for a preview those
/// URLs are head metadata only; the default is the product's sites domain.
pub(crate) fn sites_domain() -> &'static str {
    static DOMAIN: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DOMAIN.get_or_init(|| {
        std::env::var("SITES_DOMAIN")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "alosites.com".to_owned())
    })
}

/// The largest image the preview inlines as a `data:` URI. Beyond this the
/// image falls back to its public path (unresolvable on the edit origin — a
/// broken image in the preview only), keeping the preview document bounded.
const PREVIEW_INLINE_IMAGE_MAX_BYTES: usize = 4 * 1024 * 1024;

/// The draft page's images as `data:` URIs, keyed by blob id — theme
/// logo/favicon plus every section image, read tenant-scoped through the
/// account door. Ids that don't resolve, aren't images, or are oversized are
/// simply absent (the renderer then falls back to the public path).
async fn preview_image_map(
    account: &Account,
    theme: &SiteTheme,
    sections: &Value,
) -> std::collections::HashMap<String, String> {
    use base64::Engine;

    let mut ids: Vec<String> = [theme.logo.as_ref(), theme.favicon.as_ref()]
        .into_iter()
        .flatten()
        .map(|blob| blob.as_str().to_owned())
        .collect();
    for section in sections_lenient(sections) {
        ids.extend(
            section
                .image_blob_ids()
                .into_iter()
                .map(|blob| blob.as_str().to_owned()),
        );
    }
    ids.sort_unstable();
    ids.dedup();

    let mut map = std::collections::HashMap::with_capacity(ids.len());
    for id in ids {
        match account.acc.site_image(&BlobId::new(id.clone())).await {
            Ok(Some(image)) if image.bytes.len() <= PREVIEW_INLINE_IMAGE_MAX_BYTES => {
                let uri = format!(
                    "data:{};base64,{}",
                    image.content_type,
                    base64::engine::general_purpose::STANDARD.encode(&image.bytes)
                );
                map.insert(id, uri);
            }
            Ok(_) => {} // absent, non-image, or oversized: public-path fallback
            Err(error) => {
                tracing::warn!(%error, "preview image read failed; falling back to public path");
            }
        }
    }
    map
}

/// `GET /sites/:id/pages/:pid/preview` → the DRAFT page as one complete,
/// self-contained HTML document (`text/html`), rendered by the same library
/// the public service renders published snapshots with — the stylesheet is
/// inlined because the public asset paths do not resolve on this origin, and
/// images are inlined as `data:` URIs for the same reason. Authenticated
/// like every edit route; the editor fetches this and shows it in a
/// sandboxed iframe. `Cache-Control: no-store` — a draft has no cache life.
pub async fn preview_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let sid = SiteId::new(id);
    let site = require_site(&account, &sid).await?;
    let page = page_record(&account, &sid, &SitePageId::new(pid)).await?;
    let html = render_preview_html(&account, &site, &page, &page.sections).await;
    Ok((
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        html,
    )
        .into_response())
}

/// Renders either the stored page or an already-validated proposed envelope
/// through the public renderer. Keeping this one path means the AI review's
/// “after” view is the page that approval would actually store, not a client
/// approximation.
async fn render_preview_html(
    account: &Account,
    site: &Site,
    page: &SitePage,
    sections: &Value,
) -> String {
    let theme = SiteTheme::from_stored(site.theme.clone());
    let images = preview_image_map(account, &theme, sections).await;
    let base_url = format!("https://{}.{}", site.subdomain, sites_domain());
    let site_ctx = SiteRenderContext {
        name: &site.name,
        base_url: &base_url,
        theme: &theme,
        strings: &EN,
        images: ImageSources::Inline(&images),
    };
    let path = if page.is_home {
        "/".to_owned()
    } else {
        format!("/{}", page.slug)
    };
    let page_ctx = PageRenderContext {
        path: &path,
        title: &page.title,
        seo_title: page.seo_title.as_deref(),
        seo_description: page.seo_description.as_deref(),
        sections,
    };
    render_page_preview(&site_ctx, &page_ctx, &stylesheet(&theme))
}

#[derive(Deserialize)]
struct PageEditBody {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    slug: Option<String>,
    #[serde(default, rename = "seoTitle")]
    seo_title: Option<String>,
    #[serde(default, rename = "seoDescription")]
    seo_description: Option<String>,
}

/// `PUT /sites/:id/pages/:pid` `{title?, slug?, seoTitle?, seoDescription?}`
/// → `{status:"ok"}` — fields absent from the body are untouched; a blank
/// SEO string clears the override.
pub async fn update_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: PageEditBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    if req.title.is_none()
        && req.slug.is_none()
        && req.seo_title.is_none()
        && req.seo_description.is_none()
    {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "nothing to update: provide title, slug, seoTitle, and/or seoDescription",
        ));
    }
    let sid = SiteId::new(id);
    let page_id = SitePageId::new(pid);
    if let Some(title) = &req.title {
        account
            .acc
            .set_page_title(&sid, &page_id, title.trim())
            .await
            .map_err(map_store_err)?;
    }
    if let Some(slug) = &req.slug {
        account
            .acc
            .set_page_slug(&sid, &page_id, slug.trim())
            .await
            .map_err(map_store_err)?;
    }
    if req.seo_title.is_some() || req.seo_description.is_some() {
        // Partial update over the two-field setter: an absent field keeps
        // its stored value, a present blank clears it (the store's rule).
        let page = account
            .acc
            .site_page(&sid, &page_id)
            .await
            .map_err(map_store_err)?
            .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such page"))?;
        let seo_title = req.seo_title.as_deref().or(page.seo_title.as_deref());
        let seo_description = req
            .seo_description
            .as_deref()
            .or(page.seo_description.as_deref());
        account
            .acc
            .set_page_seo(&sid, &page_id, seo_title, seo_description)
            .await
            .map_err(map_store_err)?;
    }
    Ok(Json(json!({ "status": "ok" })))
}

/// `DELETE /sites/:id/pages/:pid` → `{status:"ok"}` — published snapshots of
/// the page survive by design (they belong to the publish, not the draft).
pub async fn delete_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_site_page(&SiteId::new(id), &SitePageId::new(pid))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `POST /sites/:id/pages/:pid/home` → `{status:"ok"}` — makes the page the
/// site's home page, demoting the current one in the same transaction.
pub async fn set_home_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .set_home_page(&SiteId::new(id), &SitePageId::new(pid))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

#[derive(Deserialize)]
struct OrderBody {
    order: Vec<String>,
}

/// `PUT /sites/:id/pages/order` `{order:[pageId,…]}` → `{status:"ok"}` — the
/// full navigation permutation (every page exactly once).
pub async fn reorder_pages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: OrderBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let order: Vec<SitePageId> = req.order.into_iter().map(SitePageId::new).collect();
    account
        .acc
        .reorder_site_pages(&SiteId::new(id), &order)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

// ---- blog posts ------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PostCreateBody {
    doc_node_id: String,
    slug: String,
    title: String,
    #[serde(default)]
    excerpt: String,
    #[serde(default)]
    cover_blob_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PostUpdateBody {
    slug: String,
    title: String,
    #[serde(default)]
    excerpt: String,
    #[serde(default)]
    cover_blob_id: Option<String>,
}

/// `GET /sites/:id/posts` returns the site's blog metadata newest first.
pub async fn list_posts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let posts = account.acc.site_posts(&site).await.map_err(map_store_err)?;
    Ok(Json(json!({
        "posts": posts.iter().map(post_json).collect::<Vec<_>>()
    })))
}

/// `POST /sites/:id/posts` binds a readable alo document to a new draft post.
pub async fn create_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: PostCreateBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let site = SiteId::new(id);
    let document = DriveNodeId::new(req.doc_node_id);
    let cover = req.cover_blob_id.map(BlobId::new);
    let post_id = account
        .acc
        .create_site_post(
            &site,
            &NewSitePost {
                doc_node_id: &document,
                slug: req.slug.trim(),
                title: req.title.trim(),
                excerpt: req.excerpt.trim(),
                cover_blob_id: cover.as_ref(),
            },
        )
        .await
        .map_err(map_store_err)?;
    let post = account
        .acc
        .site_post(&site, &post_id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(Problem::server_error)?;
    Ok(Json(post_json(&post)))
}

/// `GET /sites/:id/posts/:post` returns one post's metadata.
pub async fn get_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, post)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let post = account
        .acc
        .site_post(&SiteId::new(id), &SitePostId::new(post))
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such post"))?;
    Ok(Json(post_json(&post)))
}

/// `PUT /sites/:id/posts/:post` replaces the editable public metadata.
pub async fn update_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, post)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: PostUpdateBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let cover = req.cover_blob_id.map(BlobId::new);
    account
        .acc
        .update_site_post(
            &SiteId::new(id),
            &SitePostId::new(post),
            &SitePostUpdate {
                slug: req.slug.trim(),
                title: req.title.trim(),
                excerpt: req.excerpt.trim(),
                cover_blob_id: cover.as_ref(),
            },
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `DELETE /sites/:id/posts/:post` removes only the blog metadata; the alo
/// document remains in Drive.
pub async fn delete_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, post)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_site_post(&SiteId::new(id), &SitePostId::new(post))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

pub async fn publish_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, post)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .publish_site_post(&SiteId::new(id), &SitePostId::new(post))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

pub async fn unpublish_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, post)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .unpublish_site_post(&SiteId::new(id), &SitePostId::new(post))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

// ---- sections ---------------------------------------------------------------

/// Parses a stored or submitted envelope, mapping schema violations to `422`
/// with the rule-naming message.
fn parse_envelope(value: Value) -> Result<SectionsEnvelope, Problem> {
    SectionsEnvelope::from_value(value)
        .map_err(|e| Problem::with(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))
}

/// Parses one submitted section against the closed v1 vocabulary.
fn parse_section(value: Value) -> Result<Section, Problem> {
    serde_json::from_value(value).map_err(|e| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("invalid section: {e}"),
        )
    })
}

/// Loads the page's current envelope for a read-modify-write section op.
async fn page_record(
    account: &Account,
    site: &SiteId,
    page: &SitePageId,
) -> Result<SitePage, Problem> {
    account
        .acc
        .site_page(site, page)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such page"))
}

/// Loads the page's current envelope for a read-modify-write section op.
async fn page_envelope(
    account: &Account,
    site: &SiteId,
    page: &SitePageId,
) -> Result<SectionsEnvelope, Problem> {
    let p = page_record(account, site, page).await?;
    parse_envelope(p.sections)
}

/// Writes the envelope back through the store's schema gate (which re-checks
/// the content rules) and answers `{"sections": <canonical envelope>}` so the
/// editor renders exactly what was stored.
async fn store_sections(
    account: &Account,
    site: &SiteId,
    page: &SitePageId,
    envelope: &SectionsEnvelope,
) -> Result<Json<Value>, Problem> {
    let canonical = envelope.to_value().map_err(|_| Problem::server_error())?;
    account
        .acc
        .set_page_sections(site, page, canonical.clone())
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "sections": canonical })))
}

fn parse_index(raw: &str) -> Result<usize, Problem> {
    raw.parse().map_err(|_| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "section index must be a non-negative number",
        )
    })
}

fn index_in(len: usize, index: usize) -> Result<(), Problem> {
    if index >= len {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("no section at index {index} (the page has {len})"),
        ));
    }
    Ok(())
}

/// `PUT /sites/:id/pages/:pid/sections` (body = the sections envelope) →
/// `{"sections":…}` — the editor's atomic save of the whole stack.
pub async fn set_sections(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let value: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let envelope = parse_envelope(value)?;
    store_sections(&account, &SiteId::new(id), &SitePageId::new(pid), &envelope).await
}

#[derive(Deserialize)]
struct AddSectionBody {
    section: Value,
    /// Insert position; appends when absent.
    #[serde(default)]
    index: Option<usize>,
}

/// The forms store's owner-facing name cap. Section headings may be longer,
/// so auto-created names are shortened by characters (never raw UTF-8 bytes).
const AUTO_FORM_NAME_MAX_CHARS: usize = 100;

/// `POST /sites/:id/pages/:pid/sections` `{section, index?}` → the updated
/// envelope — inserts at `index` (append when absent).
pub async fn add_section(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: AddSectionBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let mut section = parse_section(req.section)?;
    let sid = SiteId::new(id);
    let page_id = SitePageId::new(pid);
    let mut envelope = page_envelope(&account, &sid, &page_id).await?;
    let index = req.index.unwrap_or(envelope.sections.len());
    if index > envelope.sections.len() {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "insert index {index} is past the end (the page has {})",
                envelope.sections.len()
            ),
        ));
    }
    let created_form = match &mut section {
        Section::ContactForm(contact) => match contact.form_id.as_deref() {
            Some(raw) => {
                let form = SiteFormId::new(raw);
                if account
                    .acc
                    .site_form(&sid, &form)
                    .await
                    .map_err(map_store_err)?
                    .is_none()
                {
                    return Err(Problem::with(StatusCode::NOT_FOUND, "no such form"));
                }
                None
            }
            None => {
                let name: String = contact
                    .heading
                    .as_deref()
                    .map(str::trim)
                    .filter(|heading| !heading.is_empty())
                    .unwrap_or("Contact form")
                    .chars()
                    .take(AUTO_FORM_NAME_MAX_CHARS)
                    .collect();
                let form = account
                    .acc
                    .create_site_form(&sid, &name)
                    .await
                    .map_err(map_store_err)?;
                contact.form_id = Some(form.to_string());
                Some(form)
            }
        },
        _ => None,
    };
    envelope.sections.insert(index, section);
    match store_sections(&account, &sid, &page_id, &envelope).await {
        Ok(response) => Ok(response),
        Err(error) => {
            if let Some(form) = created_form {
                let _ = account.acc.delete_site_form(&sid, &form).await;
            }
            Err(error)
        }
    }
}

#[derive(Deserialize)]
struct SectionBody {
    section: Value,
}

/// `PUT /sites/:id/pages/:pid/sections/:index` `{section}` → the updated
/// envelope — replaces the section at `index`.
pub async fn update_section(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid, index)): Path<(String, String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: SectionBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let section = parse_section(req.section)?;
    let index = parse_index(&index)?;
    let sid = SiteId::new(id);
    let page_id = SitePageId::new(pid);
    let mut envelope = page_envelope(&account, &sid, &page_id).await?;
    index_in(envelope.sections.len(), index)?;
    envelope.sections[index] = section;
    store_sections(&account, &sid, &page_id, &envelope).await
}

#[derive(Deserialize)]
struct MoveSectionBody {
    to: usize,
}

/// `POST /sites/:id/pages/:pid/sections/:index/move` `{to}` → the updated
/// envelope — moves the section from `index` to position `to`.
pub async fn move_section(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid, index)): Path<(String, String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: MoveSectionBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let index = parse_index(&index)?;
    let sid = SiteId::new(id);
    let page_id = SitePageId::new(pid);
    let mut envelope = page_envelope(&account, &sid, &page_id).await?;
    index_in(envelope.sections.len(), index)?;
    index_in(envelope.sections.len(), req.to)?;
    let section = envelope.sections.remove(index);
    envelope.sections.insert(req.to, section);
    store_sections(&account, &sid, &page_id, &envelope).await
}

/// `DELETE /sites/:id/pages/:pid/sections/:index` → the updated envelope —
/// removes the section at `index`.
pub async fn remove_section(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid, index)): Path<(String, String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let index = parse_index(&index)?;
    let sid = SiteId::new(id);
    let page_id = SitePageId::new(pid);
    let mut envelope = page_envelope(&account, &sid, &page_id).await?;
    index_in(envelope.sections.len(), index)?;
    envelope.sections.remove(index);
    store_sections(&account, &sid, &page_id, &envelope).await
}
