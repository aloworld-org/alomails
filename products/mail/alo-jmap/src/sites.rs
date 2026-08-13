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

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use alo_ai::{
    InferenceError, SiteDraftError, SiteEditEnvelope, SiteEditError, SiteEditOperation,
    SiteSectionTarget, SiteTranslationEnvelope, SiteTranslationError, SiteTranslationPageSnapshot,
    SiteTranslationPostSnapshot, SiteTranslationSource,
};
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
    ImageSources, PageRenderContext, SiteRenderContext, render_page_preview, sections_lenient,
    strings_for,
};
use alo_sites::stylesheet::stylesheet;
use alo_store::{
    BaseFieldId, BaseTableId, BlobId, DriveNodeId, LocalizedSitePage, NewGeneratedSite,
    NewGeneratedSitePage, NewSitePost, Section, SectionsEnvelope, Site, SiteAnalyticsDimension,
    SiteCatalogSnapshot, SiteCollection, SiteCollectionFieldMapping, SiteCollectionId,
    SiteCollectionInput, SiteCollectionItem, SiteCollectionSnapshot, SiteDomain, SiteDomainStatus,
    SiteEditorInviteOutcome, SiteFormId, SiteFormSubmissionId, SiteId, SitePage, SitePageId,
    SitePost, SitePostId, SitePostUpdate, SiteTheme, SiteTranslationPageContent,
    SiteTranslationPageWrite, SiteTranslationPostContent, SiteTranslationPostWrite, StoreError,
    UserId, normalize_site_domain, site_theme::THEME_PRESETS,
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

/// The longest alt text this surface accepts from an AI proposal. The schema
/// bound on `alt` is far larger (it is the bound on any short text a person
/// may type); a *proposed* description that runs past a sentence is a screen
/// reader reading an essay, so the refusal happens here, before the owner is
/// asked to approve it.
const MAX_PROPOSED_ALT_TEXT_CHARS: usize = 200;

// ---- JSON shaping -----------------------------------------------------------

fn iso(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

/// A site as JSON. `theme` is the stored envelope (or the pristine `{}` of a
/// site that never set one) — always a value that passed the theme gate.
pub(crate) fn site_json(s: &Site) -> Value {
    json!({
        "id": s.id.as_str(),
        "name": s.name,
        "subdomain": s.subdomain,
        "status": s.status.as_str(),
        "theme": s.theme,
        "defaultLocale": s.default_locale,
        "enabledLocales": s.enabled_locales,
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
pub(crate) fn page_json(p: &SitePage, with_sections: bool) -> Value {
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

fn localized_page_json(localized: &LocalizedSitePage) -> Value {
    let mut value = page_json(&localized.page, true);
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "requestedLocale".to_owned(),
            json!(localized.requested_locale),
        );
        object.insert(
            "resolvedLocale".to_owned(),
            json!(localized.resolved_locale),
        );
        object.insert("fallback".to_owned(), json!(localized.fallback));
    }
    value
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

fn collection_json(collection: &SiteCollection) -> Value {
    json!({
        "id": collection.id.as_str(),
        "name": collection.name,
        "baseNodeId": collection.base_node_id.as_str(),
        "baseTableId": collection.base_table_id.as_str(),
        "mapping": {
            "title": collection.mapping.title.as_str(),
            "slug": collection.mapping.slug.as_ref().map(BaseFieldId::as_str),
            "summary": collection.mapping.summary.as_ref().map(BaseFieldId::as_str),
            "body": collection.mapping.body.as_ref().map(BaseFieldId::as_str),
            "image": collection.mapping.image.as_ref().map(BaseFieldId::as_str),
            "link": collection.mapping.link.as_ref().map(BaseFieldId::as_str),
            "publishedAt": collection.mapping.published_at.as_ref().map(BaseFieldId::as_str),
        },
        "createdAt": iso(collection.created_at),
        "updatedAt": iso(collection.updated_at),
    })
}

fn collection_item_json(item: &SiteCollectionItem) -> Value {
    json!({
        "title": item.title,
        "slug": item.slug,
        "summary": item.summary,
        "body": item.body,
        "imageBlobId": item.image.as_ref().map(BlobId::as_str),
        "link": item.link,
        "publishedAt": item.published_at,
    })
}

fn collection_preview_json(snapshot: &SiteCollectionSnapshot) -> Value {
    json!({
        "id": snapshot.collection_id.as_str(),
        "name": snapshot.name,
        "items": snapshot.items.iter().map(collection_item_json).collect::<Vec<_>>(),
    })
}

/// The sites-module error map (`docs/design/sites.md` → Errors). The sites
/// store spells every rule violation as `Conflict` with a message naming the
/// violated rule and never echoing another tenant's data, and the design note
/// publishes all of them — subdomain-taken included — as `422`.
pub(crate) fn map_store_err(e: StoreError) -> Problem {
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
    let sites = if !account.is_admin && account.has_role(alo_store::TenantRole::SiteEditor) {
        account.acc.editable_sites().await.map_err(map_store_err)?
    } else {
        account.acc.sites().await.map_err(map_store_err)?
    };
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
#[serde(untagged)]
enum ProposeSiteEditBody {
    Instruction { instruction: String },
    Copy { copy: SiteCopyRequest },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SiteCopyRequest {
    target: SiteSectionTarget,
    pointer: String,
    action: SiteCopyAction,
    tone: Option<String>,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SiteCopyAction {
    Rewrite,
    Tone,
    Shorter,
    Longer,
    /// Draft the alt text of an image. Unlike the four copy actions it may
    /// start from an empty field — an image with no description yet is
    /// exactly the case it exists for.
    AltText,
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

fn invalid_copy_proposal(detail: impl Into<String>) -> Problem {
    Problem::with(
        StatusCode::UNPROCESSABLE_ENTITY,
        format!(
            "The proposed copy change is not safe to apply: {}",
            detail.into()
        ),
    )
    .with_extra(json!({ "reason": "invalid_proposal" }))
}

fn copy_instruction(page: &SectionsEnvelope, request: &SiteCopyRequest) -> Result<String, Problem> {
    let section = page
        .sections
        .get(request.target.index)
        .ok_or_else(|| invalid_copy_proposal("the selected section no longer exists"))?;
    if section.kind() != request.target.kind {
        return Err(invalid_copy_proposal(
            "the selected section changed; reopen it and try again",
        ));
    }
    if request.pointer.is_empty()
        || !request.pointer.starts_with('/')
        || request.pointer.chars().count() > 300
        || request.pointer == "/type"
        || request.pointer.starts_with("/type/")
    {
        return Err(invalid_copy_proposal("the selected text field is invalid"));
    }
    let section_value = serde_json::to_value(section).map_err(|_| Problem::server_error())?;
    let current = section_value
        .pointer(&request.pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_copy_proposal("the selected field does not contain text"))?;

    let action = match request.action {
        // The one action that starts from a field a person has not filled in
        // yet, and the one whose subject the model cannot see.
        SiteCopyAction::AltText => return alt_text_instruction(request, current),
        SiteCopyAction::Rewrite => "Rewrite it for clarity while preserving its meaning".to_owned(),
        SiteCopyAction::Shorter => "Make it shorter while preserving its meaning".to_owned(),
        SiteCopyAction::Longer => {
            "Make it more detailed without inventing facts or changing its meaning".to_owned()
        }
        SiteCopyAction::Tone => {
            let tone = request
                .tone
                .as_deref()
                .map(str::trim)
                .filter(|tone| !tone.is_empty())
                .ok_or_else(|| invalid_copy_proposal("name the tone you want"))?;
            if tone.chars().count() > 60 {
                return Err(invalid_copy_proposal(
                    "the tone must be 60 characters or fewer",
                ));
            }
            format!("Rewrite it in a {tone} tone while preserving its meaning")
        }
    };

    if current.trim().is_empty() {
        return Err(invalid_copy_proposal(
            "the selected field does not contain text",
        ));
    }
    Ok(format!(
        "{action}. Change ONLY section {} (`{}`) at JSON pointer `{}`. The current text is {:?}. Return exactly one `rewrite_copy` operation targeting that same section and pointer.",
        request.target.index, request.target.kind, request.pointer, current
    ))
}

/// The instruction behind “suggest a description” on an image field.
///
/// Nothing in this build shows the model the photograph — `alo-ai` speaks
/// text — so the prompt says so and confines the draft to what the section's
/// own words already claim the picture is there to show. That is why this is
/// a *proposal*: the editor puts the sentence next to the real image and the
/// owner is the one who can see whether it is true. Alt text that describes
/// the wrong photograph is worse for a screen-reader user than alt text that
/// is missing, so inventing detail is forbidden in the prompt and a proposal
/// past one sentence is refused in [`require_scoped_copy_proposal`].
fn alt_text_instruction(request: &SiteCopyRequest, current: &str) -> Result<String, Problem> {
    if !request.pointer.ends_with("/alt") {
        return Err(invalid_copy_proposal(
            "a description can only be written for an image",
        ));
    }
    let existing = if current.trim().is_empty() {
        "The image has no description yet.".to_owned()
    } else {
        format!("The current description is {current:?}; improve it.")
    };
    Ok(format!(
        "Write the alt text (the description read aloud to someone who cannot see the picture) for the image at JSON pointer `{pointer}` of section {index} (`{kind}`). {existing} You have NOT seen this photograph: use only what this section's own text says the image is there to show, and invent no visual detail — no colours, no counts, no names, no logos, no words appearing in the picture. Write one plain sentence of at most {MAX_PROPOSED_ALT_TEXT_CHARS} characters in the language of the section, with no \"image of\" or \"photo of\" prefix. Return exactly one `rewrite_copy` operation targeting that same section and pointer.",
        pointer = request.pointer,
        index = request.target.index,
        kind = request.target.kind,
    ))
}

fn require_scoped_copy_proposal(
    proposal: &SiteEditEnvelope,
    request: &SiteCopyRequest,
) -> Result<(), Problem> {
    match proposal.operations.as_slice() {
        [
            SiteEditOperation::RewriteCopy {
                target,
                pointer,
                text,
            },
        ] if target == &request.target && pointer == &request.pointer => {
            if request.action == SiteCopyAction::AltText {
                if text.trim().is_empty() {
                    return Err(invalid_copy_proposal("the service wrote no description"));
                }
                if text.chars().count() > MAX_PROPOSED_ALT_TEXT_CHARS {
                    return Err(invalid_copy_proposal(format!(
                        "a description must be at most {MAX_PROPOSED_ALT_TEXT_CHARS} characters"
                    )));
                }
            }
            Ok(())
        }
        _ => Err(invalid_copy_proposal(
            "the service changed more than the selected text field",
        )),
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
    let instruction = match &req {
        ProposeSiteEditBody::Instruction { instruction } => instruction.trim().to_owned(),
        ProposeSiteEditBody::Copy { copy } => copy_instruction(&current, copy)?,
    };
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
    let proposal = alo_ai::propose_site_edit(&config, &current, &instruction)
        .await
        .map_err(|error| site_edit_problem(&error))?;
    if let ProposeSiteEditBody::Copy { copy } = &req {
        require_scoped_copy_proposal(&proposal, copy)?;
    }
    let proposed =
        alo_ai::apply_site_edit(&current, &proposal).map_err(|error| site_edit_problem(&error))?;
    let proposed_value = proposed.to_value().map_err(|_| Problem::server_error())?;
    let site = require_site(&account, &sid).await?;
    let preview_html = render_preview_html(&account, &site, &page, &proposed_value).await?;
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProposeSiteTranslationBody {
    source_locale: String,
    target_locale: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplySiteTranslationBody {
    proposal: SiteTranslationEnvelope,
}

fn translation_problem(error: &SiteTranslationError) -> Problem {
    match error {
        SiteTranslationError::Inference(InferenceError::Disabled | InferenceError::NotConfigured) =>
            Problem::with(
                StatusCode::SERVICE_UNAVAILABLE,
                "AI translation is not configured. You can still copy and translate each language manually.",
            ).with_extra(json!({ "reason": "unconfigured" })),
        SiteTranslationError::Inference(
            InferenceError::Backend(_) | InferenceError::Transport | InferenceError::Empty,
        ) => Problem::with(
            StatusCode::BAD_GATEWAY,
            "AI translation could not reach the configured service. Nothing changed; try again shortly.",
        ).with_extra(json!({ "reason": "unreachable" })),
        _ => Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("The proposed translation is not safe to apply: {error}"),
        ).with_extra(json!({ "reason": "invalid_proposal" })),
    }
}

async fn translation_source(
    account: &Account,
    site: &Site,
    source_locale: &str,
    target_locale: &str,
) -> Result<SiteTranslationSource, Problem> {
    let source_locale = alo_store::normalize_locale_tag(source_locale).map_err(map_store_err)?;
    let target_locale = alo_store::normalize_locale_tag(target_locale).map_err(map_store_err)?;
    if source_locale == target_locale {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Choose two different languages.",
        ));
    }
    if source_locale != site.default_locale {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Whole-site translation starts from the website's default language.",
        ));
    }
    if !site.enabled_locales.contains(&source_locale)
        || !site.enabled_locales.contains(&target_locale)
    {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Enable both languages before translating.",
        ));
    }
    let base_pages = account
        .acc
        .site_pages(&site.id)
        .await
        .map_err(map_store_err)?;
    let mut pages = Vec::with_capacity(base_pages.len());
    for base in base_pages {
        let localized = account
            .acc
            .localized_site_page(&site.id, &base.id, &source_locale)
            .await
            .map_err(map_store_err)?
            .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "not found"))?;
        if localized.fallback {
            return Err(Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!(
                    "{} has no {} source draft. Translate or copy that page first.",
                    base.title, source_locale
                ),
            ));
        }
        let page = localized.page;
        let sections =
            SectionsEnvelope::from_value(page.sections).map_err(|_| Problem::server_error())?;
        pages.push(SiteTranslationPageSnapshot {
            id: page.id.to_string(),
            title: page.title,
            slug: page.slug,
            seo_title: page.seo_title,
            seo_description: page.seo_description,
            sections,
        });
    }
    let base_posts = account
        .acc
        .site_posts(&site.id)
        .await
        .map_err(map_store_err)?;
    let localized_posts = account
        .acc
        .site_posts_in_locale_exact(&site.id, &source_locale)
        .await
        .map_err(map_store_err)?;
    if localized_posts.len() != base_posts.len() {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "One or more posts have no {source_locale} source draft. Translate their metadata first."
            ),
        ));
    }
    let posts = localized_posts
        .into_iter()
        .map(|post| SiteTranslationPostSnapshot {
            id: post.id.to_string(),
            title: post.title,
            slug: post.slug,
            excerpt: post.excerpt,
        })
        .collect();
    Ok(SiteTranslationSource {
        source_locale,
        target_locale,
        pages,
        posts,
    })
}

/// `POST /sites/:id/translation-proposals` prepares a complete before/after
/// review. The model has no persistence handle and this route writes nothing.
pub async fn propose_site_translation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let request: ProposeSiteTranslationBody =
        serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let site = require_site(&account, &SiteId::new(id)).await?;
    let source = translation_source(
        &account,
        &site,
        &request.source_locale,
        &request.target_locale,
    )
    .await?;
    let config = tenant_ai_config(&account).await.map_err(|problem| {
        if problem.status == StatusCode::SERVICE_UNAVAILABLE {
            Problem::with(StatusCode::SERVICE_UNAVAILABLE, "AI translation is not configured. You can still copy and translate each language manually.")
                .with_extra(json!({ "reason": "unconfigured" }))
        } else { problem }
    })?;
    let proposal = alo_ai::propose_site_translation(&config, &source)
        .await
        .map_err(|error| translation_problem(&error))?;
    Ok(Json(json!({ "proposal": proposal })))
}

/// `PUT /sites/:id/translation-proposals` applies an explicitly approved
/// proposal. It rebuilds and compares the complete source, then the store
/// locks every source row and repeats the stale check inside one transaction.
pub async fn apply_site_translation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let request: ApplySiteTranslationBody =
        serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let site = require_site(&account, &SiteId::new(id)).await?;
    let source = translation_source(
        &account,
        &site,
        &request.proposal.source_locale,
        &request.proposal.target_locale,
    )
    .await?;
    alo_ai::validate_site_translation(&source, &request.proposal)
        .map_err(|error| translation_problem(&error))?;
    let pages = request
        .proposal
        .pages
        .iter()
        .map(|item| SiteTranslationPageWrite {
            id: SitePageId::new(item.before.id.clone()),
            before: SiteTranslationPageContent {
                title: item.before.title.clone(),
                slug: item.before.slug.clone(),
                seo_title: item.before.seo_title.clone(),
                seo_description: item.before.seo_description.clone(),
                sections: serde_json::to_value(&item.before.sections).unwrap_or(Value::Null),
            },
            after: SiteTranslationPageContent {
                title: item.after.title.clone(),
                slug: item.after.slug.clone(),
                seo_title: item.after.seo_title.clone(),
                seo_description: item.after.seo_description.clone(),
                sections: serde_json::to_value(&item.after.sections).unwrap_or(Value::Null),
            },
        })
        .collect::<Vec<_>>();
    let posts = request
        .proposal
        .posts
        .iter()
        .map(|item| SiteTranslationPostWrite {
            id: SitePostId::new(item.before.id.clone()),
            before: SiteTranslationPostContent {
                title: item.before.title.clone(),
                slug: item.before.slug.clone(),
                excerpt: item.before.excerpt.clone(),
            },
            after: SiteTranslationPostContent {
                title: item.after.title.clone(),
                slug: item.after.slug.clone(),
                excerpt: item.after.excerpt.clone(),
            },
        })
        .collect::<Vec<_>>();
    account
        .acc
        .apply_site_translation(
            &site.id,
            &request.proposal.source_locale,
            &request.proposal.target_locale,
            &pages,
            &posts,
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "applied": true, "pages": pages.len(), "posts": posts.len() }),
    ))
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
    #[serde(default, rename = "defaultLocale")]
    default_locale: Option<String>,
    #[serde(default, rename = "enabledLocales")]
    enabled_locales: Option<Vec<String>>,
}

/// `POST /sites` `{name, subdomain, defaultLocale?, enabledLocales?}` → the
/// created site (status `draft`, empty theme). Omitted locale settings start
/// in English; providing only a default enables that language. The subdomain
/// is claimed in the global namespace; a collision answers taken/free only,
/// never the owner.
pub async fn create_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: SiteBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let default_locale = req.default_locale.as_deref().unwrap_or("en");
    let fallback_locales = [default_locale.to_owned()];
    let enabled_locales = req.enabled_locales.as_deref().unwrap_or(&fallback_locales);
    let id = account
        .acc
        .create_site_with_locales(
            req.name.trim(),
            req.subdomain.trim(),
            default_locale,
            enabled_locales,
        )
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
            "canManageCollaborators".to_owned(),
            json!(account.is_admin || site.created_by == account.user.as_str()),
        );
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

fn require_site_manager(account: &Account, site: &Site) -> Result<(), Problem> {
    if account.is_admin || site.created_by == account.user.as_str() {
        Ok(())
    } else {
        Err(Problem::with(
            StatusCode::FORBIDDEN,
            "Only this website's owner can manage its collaborators.",
        ))
    }
}

fn collaborator_json(collaborator: &alo_store::SiteEditorCollaborator) -> Value {
    json!({
        "id": collaborator.user.as_str(),
        "email": collaborator.email,
        "status": if collaborator.pending { "pending" } else { "active" },
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InviteSiteEditorBody {
    email: String,
}

/// `GET /sites/:id/collaborators` lists only this site's restricted editors.
/// It never returns the workspace user directory.
pub async fn list_collaborators(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site_id = SiteId::new(id);
    let site = account
        .acc
        .site(&site_id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such site"))?;
    require_site_manager(&account, &site)?;
    let collaborators = state
        .store
        .for_tenant(account.tenant)
        .site_editors(&site_id)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "collaborators": collaborators.iter().map(collaborator_json).collect::<Vec<_>>()
    })))
}

/// `POST /sites/:id/collaborators` `{email}` creates a restricted collaborator
/// and answers a one-time setup link, or grants another site to an already
/// active restricted collaborator. No workspace user list is exposed.
pub async fn invite_collaborator(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    const INVITE_TTL_HOURS: i64 = 72;
    let account = authenticate(&state, &headers).await?;
    let site_id = SiteId::new(id);
    let site = account
        .acc
        .site(&site_id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such site"))?;
    require_site_manager(&account, &site)?;
    let req: InviteSiteEditorBody =
        serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let email = req.email.trim().to_ascii_lowercase();
    let plausible_email = email
        .rsplit_once('@')
        .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'))
        && !email.contains(char::is_whitespace);
    if !plausible_email {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "Enter a valid collaborator email address.",
        ));
    }
    if email.eq_ignore_ascii_case(
        &state
            .store
            .for_tenant(account.tenant.clone())
            .email_of(&account.user)
            .await
            .map_err(map_store_err)?
            .unwrap_or_default(),
    ) {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "You already own this website.",
        ));
    }
    if let Some((existing_tenant, _)) = state
        .store
        .account_by_email(&email)
        .await
        .map_err(map_store_err)?
        && existing_tenant != account.tenant
    {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "That address already signs in to another alo workspace. Use a different address.",
        ));
    }

    let token = alo_identity::secret::random_token().map_err(|_| Problem::server_error())?;
    let token_hash = alo_identity::secret::hash_at_rest(token.reveal());
    let outcome = state
        .store
        .for_tenant(account.tenant)
        .invite_site_editor(
            &email,
            &site_id,
            &account.user,
            &token_hash,
            INVITE_TTL_HOURS,
        )
        .await
        .map_err(map_store_err)?;
    let (collaborator, invite_url) = match outcome {
        SiteEditorInviteOutcome::Active(collaborator) => (collaborator, None),
        SiteEditorInviteOutcome::Pending(collaborator) => (
            collaborator,
            Some(format!(
                "{}/sites/invite/{}",
                state.base_url.trim_end_matches('/'),
                token.reveal()
            )),
        ),
    };
    Ok(Json(json!({
        "collaborator": collaborator_json(&collaborator),
        "inviteUrl": invite_url,
        "expiresInHours": invite_url.as_ref().map(|_| INVITE_TTL_HOURS),
    })))
}

/// `DELETE /sites/:id/collaborators/:user` removes one site grant. The
/// invitation-created account disappears when this was its final site.
pub async fn revoke_collaborator(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, user)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site_id = SiteId::new(id);
    let site = account
        .acc
        .site(&site_id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such site"))?;
    require_site_manager(&account, &site)?;
    state
        .store
        .for_tenant(account.tenant)
        .revoke_site_editor(&UserId::new(user), &site_id)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "revoked" })))
}

/// Public, token-gated invitation facts for the setup screen.
pub async fn get_collaborator_invitation(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<Value>, Problem> {
    let token_hash = alo_identity::secret::hash_at_rest(&token);
    let invitation = state
        .store
        .site_editor_invite(&token_hash)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| {
            Problem::with(
                StatusCode::NOT_FOUND,
                "This invitation has expired or has already been used.",
            )
        })?;
    Ok(Json(json!({
        "email": invitation.email,
        "siteName": invitation.site_name,
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptSiteEditorBody {
    password: String,
}

/// Public invitation acceptance: set the collaborator's first password and
/// spend the setup token in one transaction.
pub async fn accept_collaborator_invitation(
    State(state): State<AppState>,
    Path(token): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let req: AcceptSiteEditorBody =
        serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    if req.password.len() < 8 {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "Use at least 8 characters for the password.",
        ));
    }
    if req.password.len() > 256 {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "The password is too long.",
        ));
    }
    let invitation = state
        .identity
        .accept_site_editor_invite(&token, &req.password)
        .await
        .map_err(|_| Problem::server_error())?
        .ok_or_else(|| {
            Problem::with(
                StatusCode::NOT_FOUND,
                "This invitation has expired or has already been used.",
            )
        })?;
    Ok(Json(json!({
        "status": "accepted",
        "email": invitation.email,
        "siteName": invitation.site_name,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CollectionMappingBody {
    title: String,
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CollectionBody {
    name: String,
    base_node_id: String,
    base_table_id: String,
    mapping: CollectionMappingBody,
}

impl CollectionBody {
    fn mapping(&self) -> SiteCollectionFieldMapping {
        SiteCollectionFieldMapping {
            title: BaseFieldId::new(self.mapping.title.trim().to_owned()),
            slug: field_id(self.mapping.slug.as_deref()),
            summary: field_id(self.mapping.summary.as_deref()),
            body: field_id(self.mapping.body.as_deref()),
            image: field_id(self.mapping.image.as_deref()),
            link: field_id(self.mapping.link.as_deref()),
            published_at: field_id(self.mapping.published_at.as_deref()),
        }
    }
}

fn field_id(value: Option<&str>) -> Option<BaseFieldId> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| BaseFieldId::new(value.to_owned()))
}

/// `GET /sites/:id/collections` lists the site's connected Base tables.
pub async fn list_collections(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let collections = account
        .acc
        .site_collections(&site)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "collections": collections.iter().map(collection_json).collect::<Vec<_>>()
    })))
}

/// `POST /sites/:id/collections` connects one readable Base table. The Base
/// and every mapped field remain server-validated through the account door.
pub async fn create_collection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CollectionBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let site = SiteId::new(id);
    let base_node_id = DriveNodeId::new(req.base_node_id.trim().to_owned());
    let base_table_id = BaseTableId::new(req.base_table_id.trim().to_owned());
    let mapping = req.mapping();
    let collection_id = account
        .acc
        .create_site_collection(
            &site,
            &SiteCollectionInput {
                name: &req.name,
                base_node_id: &base_node_id,
                base_table_id: &base_table_id,
                mapping: &mapping,
            },
        )
        .await
        .map_err(map_store_err)?;
    let collection = account
        .acc
        .site_collection(&site, &collection_id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(Problem::server_error)?;
    Ok(Json(collection_json(&collection)))
}

/// `PUT /sites/:id/collections/:collection` atomically replaces the complete
/// source and mapping after validating it against the current Base schema.
pub async fn update_collection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, collection)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CollectionBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let site = SiteId::new(id);
    let collection = SiteCollectionId::new(collection);
    let base_node_id = DriveNodeId::new(req.base_node_id.trim().to_owned());
    let base_table_id = BaseTableId::new(req.base_table_id.trim().to_owned());
    let mapping = req.mapping();
    account
        .acc
        .update_site_collection(
            &site,
            &collection,
            &SiteCollectionInput {
                name: &req.name,
                base_node_id: &base_node_id,
                base_table_id: &base_table_id,
                mapping: &mapping,
            },
        )
        .await
        .map_err(map_store_err)?;
    let stored = account
        .acc
        .site_collection(&site, &collection)
        .await
        .map_err(map_store_err)?
        .ok_or_else(Problem::server_error)?;
    Ok(Json(collection_json(&stored)))
}

/// `DELETE /sites/:id/collections/:collection` removes only the connection;
/// the Base table and its rows are never changed.
pub async fn delete_collection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, collection)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_site_collection(&SiteId::new(id), &SiteCollectionId::new(collection))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `GET /sites/:id/collections/:collection/preview` resolves the current Base
/// rows through the exact publish normalization path without writing.
pub async fn preview_collection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, collection)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let preview = account
        .acc
        .site_collection_preview(&SiteId::new(id), &SiteCollectionId::new(collection))
        .await
        .map_err(map_store_err)?;
    Ok(Json(collection_preview_json(&preview)))
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
        "campaigns": dimension_json(report.campaigns),
        "countries": dimension_json(report.countries),
        "devices": dimension_json(report.devices),
        "entryPages": dimension_json(report.entry_pages),
        "exitPages": dimension_json(report.exit_pages),
        // Beacon-reported (S2.08a2): a site whose visitors run no scripts has
        // these two empty while every other number above stays complete.
        "readTime": dimension_json(report.read_time),
        "outboundDomains": dimension_json(report.outbound),
    })))
}

/// The second-generation dimensions all share one shape: a stored bucket and
/// a view count. An empty label means "not reported" and stays empty here —
/// naming it is the interface's job, in the reader's language.
fn dimension_json(rows: Vec<SiteAnalyticsDimension>) -> Vec<Value> {
    rows.into_iter()
        .map(|row| json!({ "label": row.label, "visits": row.visits }))
        .collect()
}

#[derive(Deserialize)]
struct SiteEditBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    subdomain: Option<String>,
    #[serde(default, rename = "defaultLocale")]
    default_locale: Option<String>,
    #[serde(default, rename = "enabledLocales")]
    enabled_locales: Option<Vec<String>>,
}

/// `PUT /sites/:id` updates identity and/or language settings. Supplying one
/// locale field keeps the other current value; fields absent from the body are
/// untouched.
pub async fn update_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: SiteEditBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    if req.name.is_none()
        && req.subdomain.is_none()
        && req.default_locale.is_none()
        && req.enabled_locales.is_none()
    {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "nothing to update: provide a name, web address, or language setting",
        ));
    }
    let sid = SiteId::new(id);
    let locale_update = if req.default_locale.is_some() || req.enabled_locales.is_some() {
        let current = account
            .acc
            .site(&sid)
            .await
            .map_err(map_store_err)?
            .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such site"))?;
        let default_locale = req
            .default_locale
            .as_deref()
            .unwrap_or(&current.default_locale);
        let enabled_locales = req
            .enabled_locales
            .as_deref()
            .unwrap_or(&current.enabled_locales);
        Some(
            alo_store::normalize_site_locales(default_locale, enabled_locales)
                .map_err(map_store_err)?,
        )
    } else {
        None
    };
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
    if let Some((default_locale, enabled_locales)) = locale_update {
        account
            .acc
            .set_site_locales(&sid, &default_locale, &enabled_locales)
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
pub(crate) async fn require_site(account: &Account, site: &SiteId) -> Result<Site, Problem> {
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

#[derive(Deserialize)]
pub struct LocalizedPageBody {
    title: String,
    slug: String,
    sections: Value,
    #[serde(default, rename = "seoTitle")]
    seo_title: Option<String>,
    #[serde(default, rename = "seoDescription")]
    seo_description: Option<String>,
}

/// `GET /sites/:id/pages/:pid/locales/:locale` resolves an enabled-language
/// draft and explicitly reports whether the site's fallback was used.
pub async fn get_localized_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid, locale)): Path<(String, String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let page = account
        .acc
        .localized_site_page(&SiteId::new(id), &SitePageId::new(pid), &locale)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such page"))?;
    Ok(Json(localized_page_json(&page)))
}

/// `GET /sites/:id/translation-readiness` answers exact draft coverage for
/// every enabled language. It is one bounded store read, not one request per
/// page, so the visible publish check remains fast at the 200-page limit.
pub async fn translation_readiness(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let readiness = account
        .acc
        .site_translation_readiness(&SiteId::new(id))
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such site"))?;
    Ok(Json(json!({
        "defaultLocale": readiness.default_locale,
        "totalPages": readiness.total_pages,
        "languages": readiness.locales.into_iter().map(|locale| json!({
            "locale": locale.locale,
            "translatedPages": locale.translated_pages,
            "ready": locale.translated_pages == readiness.total_pages,
        })).collect::<Vec<_>>()
    })))
}

/// `PUT /sites/:id/pages/:pid/locales/:locale` fully replaces one localized
/// draft while preserving the page's identity, navigation, and home role.
pub async fn put_localized_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid, locale)): Path<(String, String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: LocalizedPageBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let sid = SiteId::new(id);
    let page_id = SitePageId::new(pid);
    account
        .acc
        .set_site_page_locale(
            &sid,
            &page_id,
            &locale,
            req.title.trim(),
            req.slug.trim(),
            req.sections,
            req.seo_title.as_deref(),
            req.seo_description.as_deref(),
        )
        .await
        .map_err(map_store_err)?;
    let page = account
        .acc
        .localized_site_page(&sid, &page_id, &locale)
        .await
        .map_err(map_store_err)?
        .ok_or_else(Problem::server_error)?;
    Ok(Json(localized_page_json(&page)))
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
pub(crate) async fn preview_image_map<'a>(
    account: &Account,
    theme: &SiteTheme,
    sections: &Value,
    collections: impl Iterator<Item = &'a SiteCollectionSnapshot>,
    catalogs: impl Iterator<Item = &'a SiteCatalogSnapshot>,
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
    for collection in collections {
        ids.extend(
            collection
                .items
                .iter()
                .filter_map(|item| item.image.as_ref())
                .map(|blob| blob.as_str().to_owned()),
        );
    }
    for catalog in catalogs {
        ids.extend(
            catalog
                .items
                .iter()
                .filter_map(|item| item.image.as_ref())
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

/// `GET /sites/:id/images/:blob` → one of the tenant's own image blobs, so
/// the editor can show the photograph it is framing (S2.07c). The draft
/// preview inlines its images as `data:` URIs, which is right for a rendered
/// document and wrong for a control that has to draw a crop rectangle over
/// the *source* pixels at their own aspect ratio.
///
/// Tenant scope is the account door twice over: the site must resolve for the
/// caller, and the blob is read through
/// [`AccountStore::site_image`](alo_store::AccountStore::site_image), whose
/// SQL is tenant-scoped. Anything that does not resolve — another tenant's
/// blob, a blob that is not an image, a deleted one — is the same `404`.
/// Bytes are immutable per blob id, so they may be cached, but only
/// privately: this is an authenticated origin.
pub async fn get_site_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, blob)): Path<(String, String)>,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let sid = SiteId::new(id);
    require_site(&account, &sid).await?;
    let image = account
        .acc
        .site_image(&BlobId::new(blob))
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such image"))?;
    Ok((
        [
            (header::CONTENT_TYPE, image.content_type),
            (header::CACHE_CONTROL, "private, max-age=3600, immutable"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            // Keeps an SVG inert if one is ever opened directly on this
            // origin — the same defanging the public service applies.
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; style-src 'unsafe-inline'",
            ),
        ],
        image.bytes,
    )
        .into_response())
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
    let html = render_preview_html(&account, &site, &page, &page.sections).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        html,
    )
        .into_response())
}

/// `GET /sites/:id/pages/:pid/locales/:locale/preview` renders the requested
/// enabled-language draft through the same self-contained preview contract as
/// the base page. A missing exact translation deliberately previews the
/// resolved fallback; the JSON draft read tells the editor that it is a
/// fallback and keeps editing disabled until the owner explicitly copies it.
pub async fn preview_localized_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid, locale)): Path<(String, String, String)>,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let sid = SiteId::new(id);
    let site = require_site(&account, &sid).await?;
    let localized = account
        .acc
        .localized_site_page(&sid, &SitePageId::new(pid), &locale)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such page"))?;
    let html =
        render_preview_html(&account, &site, &localized.page, &localized.page.sections).await?;
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
) -> Result<String, Problem> {
    let theme = SiteTheme::from_stored(site.theme.clone());
    let mut collections = HashMap::new();
    for section in sections_lenient(sections) {
        if let Section::Collection(collection) = section {
            let id = collection.collection_id.as_str().to_owned();
            if collections.contains_key(&id) {
                continue;
            }
            let preview = account
                .acc
                .site_collection_preview(&site.id, &collection.collection_id)
                .await
                .map_err(map_store_err)?;
            collections.insert(id, preview);
        }
    }
    // The same honesty for catalogs: the preview resolves the draft catalog
    // exactly as publishing would — hidden items already gone — so what the
    // editor sees is what the next publish freezes.
    let mut catalogs = HashMap::new();
    for section in sections_lenient(sections) {
        if let Section::Catalog(catalog) = section {
            let id = catalog.catalog_id.as_str().to_owned();
            if catalogs.contains_key(&id) {
                continue;
            }
            let preview = account
                .acc
                .site_catalog_preview(&site.id, &catalog.catalog_id)
                .await
                .map_err(map_store_err)?;
            catalogs.insert(id, preview);
        }
    }
    // And the same for bookable services: the preview offers what the next
    // publish would freeze, so the editor is never shown a service the page
    // could not actually offer.
    let mut bookings = HashMap::new();
    for section in sections_lenient(sections) {
        if let Section::Booking(booking) = section {
            let id = booking.booking_id.as_str().to_owned();
            if bookings.contains_key(&id) {
                continue;
            }
            let preview = account
                .acc
                .site_booking_preview(&site.id, &booking.booking_id)
                .await
                .map_err(map_store_err)?;
            bookings.insert(id, preview);
        }
    }
    let images = preview_image_map(
        account,
        &theme,
        sections,
        collections.values(),
        catalogs.values(),
    )
    .await;
    let base_url = format!("https://{}.{}", site.subdomain, sites_domain());
    let site_ctx = SiteRenderContext {
        name: &site.name,
        base_url: &base_url,
        locale: &page.content_locale,
        theme: &theme,
        strings: strings_for(&page.content_locale),
        images: ImageSources::Inline(&images),
    };
    let language_prefix = if page.content_locale == site.default_locale {
        String::new()
    } else {
        format!("/{}", page.content_locale)
    };
    let path = if page.is_home {
        if language_prefix.is_empty() {
            "/".to_owned()
        } else {
            language_prefix
        }
    } else {
        format!("{language_prefix}/{}", page.slug)
    };
    let page_ctx = PageRenderContext {
        path: &path,
        title: &page.title,
        seo_title: page.seo_title.as_deref(),
        seo_description: page.seo_description.as_deref(),
        sections,
        collections: &collections,
        catalogs: &catalogs,
        bookings: &bookings,
    };
    Ok(render_page_preview(
        &site_ctx,
        &page_ctx,
        &stylesheet(&theme),
    ))
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
