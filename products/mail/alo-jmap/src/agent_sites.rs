//! Executing the **Website** tools of the Sites agent (ADR 0034, queue item
//! A2.1) — the acting half of what [`alo_ai::agent_sites`] describes to the
//! model.
//!
//! The reading tools run inside the turn ([`crate::agent_turn`]); the three that
//! change something run only from [`crate::agent::agent_execute`], after the
//! owner approved the proposal. Everything here goes through the caller's own
//! tenant-scoped store handle, so the Website agent reaches exactly the sites,
//! pages and publishes the person who asked could already open — no site of
//! another tenant's is nameable from here, because the resolver picks out of
//! `account.acc.sites()` and never out of an id the model stated.
//!
//! Four rules shape this module, and none of them is thin glue:
//!
//! - **A question about the site is answered from the internet, not the draft.**
//!   [`execute_site_answer`] reads
//!   [`alo_store::AccountStore::site_grounding_corpus`] — the pages of the
//!   *current publish*, the live posts, the documents the owner put in the
//!   site's public knowledge — and a site that is not live has an empty corpus
//!   by construction (ADR 0040 §1). Half-written copy is not what a visitor can
//!   read, and answering from it would put a promise on the record that nobody
//!   published.
//! - **Nothing here publishes as a side effect.** Drafting a page and rewriting
//!   a heading write to the draft and stop there; the only call to
//!   [`alo_store::AccountStore::publish_site`] in this file is
//!   [`execute_site_publish`], which is declared a **write** in the registry and
//!   therefore cannot run without the owner's own approval (ADR 0047 §3). That
//!   is the whole of "publishing is proposed, never silent", and it is a
//!   property of the registry rather than of anybody's good behaviour.
//! - **An agent edits the words, never the wiring.** [`copy_leaves`] is the only
//!   thing that decides what text an edit may touch: the string leaves of a
//!   section, minus a link's target, an image's blob, a form's id and every
//!   field of a `custom_code` block. [`execute_site_page_edit`] refuses a
//!   pointer that is not in that set, so a model cannot re-point a button at
//!   another domain, claim an asset it does not have, or write a script into a
//!   page — whatever it puts in its arguments.
//! - **Results carry facts and reason codes, never sentences.** The review
//!   answers `noSeoDescription`, not "this page has no description": a
//!   user-facing sentence composed in the server would be English authored in
//!   one language, which is a bug in a European product (CLAUDE.md). The words
//!   the user reads are the model's own, in their language, or the client's.

use axum::Json;
use serde_json::{Value, json};

use alo_ai::{SITE_EDIT_SCHEMA_VERSION, SiteEditEnvelope, SiteEditOperation, SiteSectionTarget};
use alo_store::{
    GroundingCitation, GroundingDocument, Section, SectionsEnvelope, Site, SiteId, SitePage,
    SitePageId, SiteStatus,
};

use crate::agent_args::{pick, string_arg, unprocessable};
use crate::error::Problem;
use crate::sites::{map_store_err, page_json, site_json};
use crate::state::Account;

/// How many published passages one answer is grounded in. Enough for a question
/// that spans a couple of pages, short enough that the whole result still fits
/// beside the question in the turn's budget.
const MAX_PASSAGES: usize = 5;

/// How much of one published passage the model is shown.
const MAX_PASSAGE_CHARS: usize = 600;

/// How much of one piece of on-page text the model is shown when it reads a
/// page. Longer than a passage because this is the text it is about to rewrite,
/// and a rewrite that starts from a truncated original loses the end of it —
/// which is why the leaf says when it was cut.
const MAX_LEAF_CHARS: usize = 800;

/// How many blocks one drafted page may carry.
const MAX_DRAFT_BLOCKS: usize = 12;

/// How many pieces of text one edit may rewrite.
const MAX_REWRITES: usize = 20;

/// The bounds a search-engine review judges against. Not laws — no search engine
/// publishes one — but the ranges every guide agrees on, and the review reports
/// the code rather than an instruction, so the client and the model can say it
/// in the reader's own language.
const SEO_DESCRIPTION_MIN: usize = 50;
const SEO_DESCRIPTION_MAX: usize = 160;
const SEO_TITLE_MAX: usize = 60;

/// Keys whose string value is **not copy**: an address, a reference, or a token
/// a renderer matches on. An agent may rewrite what a page says and never what
/// it points at — a link's target is the one field where a rewritten string
/// sends a visitor somewhere else entirely.
const NOT_COPY: &[&str] = &[
    "type",
    "href",
    "blob_id",
    "form_id",
    "collection_id",
    "catalog_id",
    "service_id",
    "calendar_id",
    "id",
    "icon",
    "image_side",
    "shape",
    "preset",
    "html",
    "css",
    "js",
];

// ---- the reading tools -------------------------------------------------------

/// `site_answer` — what the site says **on the internet** right now.
///
/// The corpus is the published one and nothing else: a site with no publish
/// comes back `live: false` with no passages, which is a truthful answer and the
/// one the prompt tells the model to repeat. Ranking is
/// [`rank_passages`] — the question's own content words against the published
/// text — so an answer cites the page a visitor could load.
///
/// # Errors
/// `422` when no site of the tenant's matches the argument; the store's own
/// failure otherwise.
pub async fn execute_site_answer(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let question = string_arg(args, "question")
        .ok_or_else(|| unprocessable("what to look for on the site is required"))?;
    let site = resolve_site(account, args).await?;
    let corpus = account
        .acc
        .site_grounding_corpus(&site.id)
        .await
        .map_err(map_store_err)?;
    let matched = rank_passages(&corpus, &question, MAX_PASSAGES);

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "siteAnswer",
            "site": site_ref(&site),
            // Whether anything of this site is on the internet at all. Said
            // plainly, because "no passages" and "no website yet" are different
            // answers and the model must not merge them.
            "live": site.status == SiteStatus::Live,
            "passages": matched.iter().map(|doc| passage_json(doc)).collect::<Vec<_>>(),
            "matched": matched.len(),
            "published": corpus.len(),
        }
    })))
}

/// `site_page_read` — one page of the **draft**, and every piece of text on it
/// an edit may rewrite.
///
/// This is the read half of the editing pair: the position, the type and the
/// pointer of each leaf come from here, so [`execute_site_page_edit`] never has
/// to trust a target the model worked out for itself. The page's own sections
/// are deliberately **not** returned whole — a link's target and an image's blob
/// are not copy, and showing them invites an edit that changes them.
///
/// # Errors
/// `422` when the site or the page cannot be resolved to exactly one of the
/// tenant's; `500` when a stored page fails to parse (a write-gate invariant
/// broken upstream).
pub async fn execute_site_page_read(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let site = resolve_site(account, args).await?;
    let page = resolve_page(account, &site.id, args).await?;
    let envelope = page_envelope(&page)?;
    let sections = copy_leaves(&envelope);

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "sitePage",
            "site": site_ref(&site),
            "page": page_json(&page, false),
            "sections": sections.iter().map(section_copy_json).collect::<Vec<_>>(),
            // This is the draft. Whether the site is live says nothing about
            // whether *this* text is, and the model is told so rather than left
            // to assume either way.
            "public": false,
            "siteLive": site.status == SiteStatus::Live,
        }
    })))
}

/// `site_seo_review` — what search engines will find missing, page by page.
///
/// Every finding is a reason code from [`review_page`] and [`review_site`], and
/// every one of them is something this file can see on the page. Nothing here
/// looks at anybody's index, so nothing here reports a ranking.
///
/// # Errors
/// `422` when no site of the tenant's matches; the store's own failure
/// otherwise.
pub async fn execute_site_seo_review(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let site = resolve_site(account, args).await?;
    let pages = account
        .acc
        .site_pages(&site.id)
        .await
        .map_err(map_store_err)?;
    let reviewed: Vec<(&SitePage, Vec<&'static str>)> = pages
        .iter()
        .map(|page| (page, review_page(page, &pages)))
        .collect();
    let findings: usize = reviewed.iter().map(|(_, found)| found.len()).sum();

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "siteSeoReview",
            "site": site_ref(&site),
            "pages": reviewed.iter().map(|(page, found)| json!({
                "pageId": page.id.as_str(),
                "title": page.title,
                "slug": page.slug,
                "home": page.is_home,
                "findings": found,
            })).collect::<Vec<_>>(),
            "siteFindings": review_site(&site, &pages),
            "pagesReviewed": pages.len(),
            "findings": findings,
        }
    })))
}

// ---- the tools that change something ----------------------------------------

/// `site_page_draft` — a new page in the site's **draft**.
///
/// The vocabulary is deliberately small: a headline, a line under it, and a
/// block of prose under each of its own subheadings. That is a page a person can
/// read at a glance before approving it, and it is structurally incapable of
/// carrying the things a website must never invent — there is no argument for a
/// price, a person, a quote or an asset, so none can arrive.
///
/// The page is never the home page and never replaces an existing one: a
/// structural change to somebody's site is not something an agent proposes in
/// passing.
///
/// # Errors
/// `422` for a missing headline, an unusable address, a block with no heading or
/// no body, or a slug the site already uses; the store's own failure otherwise.
pub async fn execute_site_page_draft(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let site = resolve_site(account, args).await?;
    let title =
        string_arg(args, "title").ok_or_else(|| unprocessable("the page's title is required"))?;
    let heading = string_arg(args, "heading")
        .ok_or_else(|| unprocessable("the page's heading is required"))?;
    let slug = match string_arg(args, "slug") {
        Some(stated) => stated.trim_start_matches('/').to_owned(),
        None => slugify(&title),
    };
    if slug.is_empty() {
        return Err(unprocessable(
            "the page's address is required — say which slug to use",
        ));
    }
    alo_store::validate_page_slug(&slug).map_err(map_store_err)?;

    // Built and validated through the authoritative Sites gate BEFORE anything
    // is created, so a badly-shaped block leaves no half-made page behind.
    let sections = draft_sections(&heading, string_arg(args, "intro").as_deref(), args)?;
    let blocks = sections.sections.len();
    let sections = sections.to_value().map_err(|_| Problem::server_error())?;

    let id = account
        .acc
        .create_site_page(&site.id, &title, &slug, false)
        .await
        .map_err(map_store_err)?;
    account
        .acc
        .set_page_sections(&site.id, &id, sections)
        .await
        .map_err(map_store_err)?;
    if let Some(description) = string_arg(args, "seo_description") {
        account
            .acc
            .set_page_seo(&site.id, &id, None, Some(&description))
            .await
            .map_err(map_store_err)?;
    }
    let page = read_back(account, &site.id, &id).await?;

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "sitePageDraft",
            "site": site_ref(&site),
            "page": page_json(&page, false),
            "blocks": blocks,
            // The sentence the whole item is named after, in the result the
            // client renders: this page is not on the internet.
            "public": false,
        }
    })))
}

/// `site_page_edit` — the words of a page that already exists, in the draft.
///
/// Three changes, each optional and all applied together: the page's title, its
/// search-engine title and description, and the text at positions
/// [`execute_site_page_read`] handed out. The rewrites go through
/// [`alo_ai::apply_site_edit`], which is atomic and refuses a stale target — a
/// position whose section type no longer matches is a refusal rather than an
/// edit to whatever moved into that slot.
///
/// # Errors
/// `422` when nothing was asked for, when a pointer is not a piece of copy on
/// that page, or when the edit does not apply; the store's own failure
/// otherwise.
pub async fn execute_site_page_edit(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let site = resolve_site(account, args).await?;
    let page = resolve_page(account, &site.id, args).await?;
    let title = string_arg(args, "title");
    let seo_title = string_arg(args, "seo_title");
    let seo_description = string_arg(args, "seo_description");
    let rewrites = rewrite_operations(args, &page_envelope(&page)?)?;
    if title.is_none() && seo_title.is_none() && seo_description.is_none() && rewrites.is_empty() {
        return Err(unprocessable(
            "say what to change: a title, a description, or the wording of something on the page",
        ));
    }

    let rewritten = rewrites.len();
    if !rewrites.is_empty() {
        let edited = alo_ai::apply_site_edit(
            &page_envelope(&page)?,
            &SiteEditEnvelope {
                schema_version: SITE_EDIT_SCHEMA_VERSION,
                operations: rewrites,
            },
        )
        .map_err(|error| unprocessable(error.to_string()))?;
        let value = edited.to_value().map_err(|_| Problem::server_error())?;
        account
            .acc
            .set_page_sections(&site.id, &page.id, value)
            .await
            .map_err(map_store_err)?;
    }
    if let Some(title) = &title {
        account
            .acc
            .set_page_title(&site.id, &page.id, title)
            .await
            .map_err(map_store_err)?;
    }
    // Both SEO fields are written by one store call, so whichever was not
    // stated is written back as it stands. Passing `None` for an unstated one
    // would CLEAR it — setting a description would silently drop the title
    // somebody wrote by hand.
    if seo_title.is_some() || seo_description.is_some() {
        account
            .acc
            .set_page_seo(
                &site.id,
                &page.id,
                seo_title.as_deref().or(page.seo_title.as_deref()),
                seo_description
                    .as_deref()
                    .or(page.seo_description.as_deref()),
            )
            .await
            .map_err(map_store_err)?;
    }
    let page = read_back(account, &site.id, &page.id).await?;

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "sitePageEdit",
            "site": site_ref(&site),
            "page": page_json(&page, false),
            "rewritten": rewritten,
            "retitled": title.is_some(),
            "seoChanged": seo_title.is_some() || seo_description.is_some(),
            "public": false,
        }
    })))
}

/// `site_publish` — the site's draft, on the internet.
///
/// The only call to [`alo_store::AccountStore::publish_site`] an agent can
/// reach, and it is declared a write, so it runs from an approval and from
/// nothing else. What goes live is the whole draft as it stands — including work
/// somebody else did — which is why the result says how many pages went out
/// rather than only that it worked.
///
/// # Errors
/// `422` when the site cannot be resolved or the store refuses to publish it
/// (no home page, nothing to publish); the store's own failure otherwise.
pub async fn execute_site_publish(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let site = resolve_site(account, args).await?;
    let publish = account
        .acc
        .publish_site(&site.id)
        .await
        .map_err(map_store_err)?;
    let pages = account
        .acc
        .site_publish_snapshots(&site.id, &publish)
        .await
        .map_err(map_store_err)?;

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "sitePublish",
            "site": site_ref(&site),
            "publishId": publish.as_str(),
            "pages": pages.len(),
            "public": true,
        }
    })))
}

// ---- resolving what the model named ------------------------------------------

/// The site an argument names, or the tenant's only one.
///
/// Names and addresses, never ids: the model is given the words the user used
/// and the candidates come from [`alo_store::AccountStore::sites`], so a site
/// belonging to another tenant is not merely refused here — it is not among the
/// things that can be named.
async fn resolve_site(account: &Account, args: &Value) -> Result<Site, Problem> {
    let sites = account.acc.sites().await.map_err(map_store_err)?;
    if sites.is_empty() {
        return Err(unprocessable("there is no website in this workspace yet"));
    }
    let Some(wanted) = string_arg(args, "site") else {
        let mut sites = sites;
        if sites.len() == 1 {
            return Ok(sites.remove(0));
        }
        return Err(unprocessable(format!(
            "more than one website: {} — say which",
            sites
                .iter()
                .map(|site| site.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    };
    // An address is exact or it is not a match: subdomains are not prose, and a
    // partial one would resolve "shop" to "shop-archive".
    let needle = wanted.trim().to_lowercase();
    if let Some(found) = sites
        .iter()
        .find(|site| site.subdomain.to_lowercase() == needle)
    {
        return Ok(found.clone());
    }
    pick(
        &wanted,
        sites
            .iter()
            .map(|site| (site.name.as_str(), site.clone()))
            .collect(),
        "website",
    )
}

/// The page an argument names, within one site.
///
/// An address wins over a title, exactly: `/about` is unambiguous where "About"
/// might be one of several. The home page answers to its own title and to the
/// site root, which is what a person means by "the front page".
async fn resolve_page(account: &Account, site: &SiteId, args: &Value) -> Result<SitePage, Problem> {
    let wanted = string_arg(args, "page")
        .ok_or_else(|| unprocessable("which page was meant is required"))?;
    let pages = account.acc.site_pages(site).await.map_err(map_store_err)?;
    if pages.is_empty() {
        return Err(unprocessable("this website has no pages yet"));
    }
    let needle = wanted.trim().trim_start_matches('/').to_lowercase();
    if let Some(found) = pages
        .iter()
        .find(|page| !page.slug.is_empty() && page.slug.to_lowercase() == needle)
    {
        return Ok(found.clone());
    }
    if needle.is_empty()
        && let Some(home) = pages.iter().find(|page| page.is_home)
    {
        return Ok(home.clone());
    }
    pick(
        &wanted,
        pages
            .iter()
            .map(|page| (page.title.as_str(), page.clone()))
            .collect(),
        "page",
    )
}

/// The page as it stands after a write — read back rather than assembled here,
/// so what the result says is what the store actually holds.
async fn read_back(
    account: &Account,
    site: &SiteId,
    page: &SitePageId,
) -> Result<SitePage, Problem> {
    account
        .acc
        .site_page(site, page)
        .await
        .map_err(map_store_err)?
        .ok_or_else(Problem::server_error)
}

/// The stored sections of a page, typed.
///
/// A stored page always passed the schema gate at write time, so a parse failure
/// here is a broken invariant rather than anything the caller did — a `500`,
/// never a `422` blaming the model's arguments.
fn page_envelope(page: &SitePage) -> Result<SectionsEnvelope, Problem> {
    SectionsEnvelope::from_value(page.sections.clone()).map_err(|_| Problem::server_error())
}

/// The site, said the way every result says it.
fn site_ref(site: &Site) -> Value {
    let mut value = site_json(site);
    if let Some(object) = value.as_object_mut() {
        // The theme is a page of JSON nobody reading an agent's answer needs,
        // and the turn's budget is shared with the question.
        object.remove("theme");
    }
    value
}

// ---- what a published answer is made of --------------------------------------

/// The published documents that answer `question`, best first.
///
/// Scored on the question's own content words: a word in the title counts double,
/// because a page called "Opening hours" is about opening hours in a way a page
/// that mentions them once is not. A document no word touches is not returned at
/// all — an unranked passage in the sources is an invitation to answer from
/// something that merely came up.
fn rank_passages<'a>(
    corpus: &'a [GroundingDocument],
    question: &str,
    limit: usize,
) -> Vec<&'a GroundingDocument> {
    let words = content_words(question);
    let mut scored: Vec<(usize, usize, &GroundingDocument)> = corpus
        .iter()
        .enumerate()
        .filter_map(|(position, document)| {
            let title = document.title.to_lowercase();
            let text = document.text.to_lowercase();
            let score: usize = words
                .iter()
                .map(|word| {
                    usize::from(title.contains(word)) * 2 + usize::from(text.contains(word))
                })
                .sum();
            (score > 0).then_some((score, position, document))
        })
        .collect();
    // Highest score first, and the corpus's own order (navigation order, then
    // newest posts) breaks a tie — never the hash order of a map.
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, _, document)| document)
        .collect()
}

/// The words of a question worth matching on: three letters or more, and not one
/// of the words every question contains.
///
/// A near-copy of `alo_store`'s own search vocabulary, which is `pub(crate)`
/// there. Kept small and tested rather than exported, because this one matches
/// an in-memory corpus while that one is composed into SQL — and because
/// widening a published surface of the store to reach it is a change to a crate
/// this item does not otherwise touch.
fn content_words(question: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "the", "and", "for", "you", "your", "this", "that", "with", "from", "about", "have", "has",
        "had", "what", "which", "where", "who", "whom", "when", "why", "how", "are", "was", "were",
        "did", "does", "can", "could", "would", "should", "will", "into", "our", "their", "they",
        "them", "there", "here", "any", "all", "some", "get", "got", "give", "tell", "show",
        "find", "not", "but", "out", "off", "site", "website", "page",
    ];
    let mut seen = Vec::new();
    for word in question.split(|c: char| !c.is_alphanumeric()) {
        let word = word.to_lowercase();
        if word.chars().count() >= 3 && !STOP.contains(&word.as_str()) && !seen.contains(&word) {
            seen.push(word);
            if seen.len() >= 12 {
                break;
            }
        }
    }
    seen
}

/// One published passage, with the citation that says where a visitor can read
/// it. Every answer the model gives has to point at one of these.
fn passage_json(document: &GroundingDocument) -> Value {
    let (text, truncated) = clamp(&document.text, MAX_PASSAGE_CHARS);
    let citation = match &document.citation {
        GroundingCitation::Page { slug, locale } => json!({
            "kind": "page", "slug": slug, "locale": locale,
        }),
        GroundingCitation::Post { slug } => json!({ "kind": "post", "slug": slug }),
        GroundingCitation::Knowledge { source_id } => json!({
            "kind": "knowledge", "sourceId": source_id.as_str(),
        }),
    };
    json!({
        "citation": citation,
        "title": document.title,
        "text": text,
        "truncated": truncated,
    })
}

/// `text`, cut to `cap` characters on a character boundary, and whether it was
/// cut. Counted in characters rather than bytes: a byte slice through a European
/// site's copy would panic on the first accented letter.
fn clamp(text: &str, cap: usize) -> (String, bool) {
    if text.chars().count() <= cap {
        return (text.to_owned(), false);
    }
    (text.chars().take(cap).collect(), true)
}

// ---- what a page's copy is ---------------------------------------------------

/// One section, and the text on it an edit may rewrite.
struct SectionCopy {
    index: usize,
    kind: &'static str,
    leaves: Vec<(String, String)>,
}

fn section_copy_json(section: &SectionCopy) -> Value {
    json!({
        "index": section.index,
        "type": section.kind,
        "copy": section.leaves.iter().map(|(pointer, text)| {
            let (text, truncated) = clamp(text, MAX_LEAF_CHARS);
            json!({ "pointer": pointer, "text": text, "truncated": truncated })
        }).collect::<Vec<_>>(),
    })
}

/// Every piece of text on a page an agent may rewrite, by position and pointer.
///
/// **This function is the permission**, not a listing: `site_page_edit` refuses
/// any pointer it did not produce. Two exclusions carry the weight —
/// [`NOT_COPY`] keeps a link's target, an image's blob and a form's id out, and
/// a `custom_code` block contributes nothing at all, because its fields are
/// markup and script rather than prose and an agent does not write code into
/// somebody's website.
fn copy_leaves(envelope: &SectionsEnvelope) -> Vec<SectionCopy> {
    envelope
        .sections
        .iter()
        .enumerate()
        .map(|(index, section)| SectionCopy {
            index,
            kind: section.kind(),
            leaves: section_leaves(section),
        })
        .collect()
}

fn section_leaves(section: &Section) -> Vec<(String, String)> {
    if matches!(section, Section::CustomCode(_)) {
        return Vec::new();
    }
    let Ok(value) = serde_json::to_value(section) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    collect_leaves(&value, String::new(), &mut out);
    out
}

/// Walks one section's JSON, collecting a pointer for every string leaf that is
/// prose. Pointers are RFC 6901, which is what
/// [`alo_ai::SiteEditOperation::RewriteCopy`] speaks.
fn collect_leaves(value: &Value, prefix: String, out: &mut Vec<(String, String)>) {
    match value {
        Value::Object(fields) => {
            for (key, field) in fields {
                if NOT_COPY.contains(&key.as_str()) {
                    continue;
                }
                collect_leaves(field, format!("{prefix}/{}", escape_pointer(key)), out);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_leaves(item, format!("{prefix}/{index}"), out);
            }
        }
        Value::String(text) => out.push((prefix, text.clone())),
        _ => {}
    }
}

/// RFC 6901 escaping. The field names in the Sites schema contain neither
/// character today; escaping them anyway is what keeps that from becoming a
/// silent mis-target the day one does.
fn escape_pointer(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// The rewrites an argument asks for, checked against what the page actually
/// carries.
///
/// Each entry must name a position that exists, the type that is there, and a
/// pointer [`copy_leaves`] produced for that position. A pointer outside the set
/// — a link's `href`, an image's `blob_id`, anything on a `custom_code` block —
/// is refused here, before [`alo_ai::apply_site_edit`] would happily rewrite any
/// string leaf it was pointed at.
fn rewrite_operations(
    args: &Value,
    envelope: &SectionsEnvelope,
) -> Result<Vec<SiteEditOperation>, Problem> {
    let Some(entries) = args.get("copy") else {
        return Ok(Vec::new());
    };
    let Some(entries) = entries.as_array() else {
        return Err(unprocessable("copy must be a list of pieces of text"));
    };
    if entries.len() > MAX_REWRITES {
        return Err(unprocessable(format!(
            "one edit may rewrite at most {MAX_REWRITES} pieces of text"
        )));
    }
    let allowed = copy_leaves(envelope);
    let mut operations = Vec::with_capacity(entries.len());
    for (position, entry) in entries.iter().enumerate() {
        let at = position + 1;
        let index = entry
            .get("index")
            .and_then(Value::as_u64)
            .ok_or_else(|| unprocessable(format!("rewrite {at} must say which block, by index")))?;
        let index = usize::try_from(index)
            .map_err(|_| unprocessable(format!("rewrite {at} names no block on this page")))?;
        let pointer = string_arg(entry, "pointer").ok_or_else(|| {
            unprocessable(format!("rewrite {at} must say which text, by pointer"))
        })?;
        let text = entry
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| unprocessable(format!("rewrite {at} must carry the new wording")))?;
        let kind = string_arg(entry, "type")
            .ok_or_else(|| unprocessable(format!("rewrite {at} must say the block's type")))?;
        let section = allowed
            .iter()
            .find(|section| section.index == index)
            .ok_or_else(|| unprocessable(format!("rewrite {at} names no block on this page")))?;
        if section.kind != kind {
            return Err(unprocessable(format!(
                "rewrite {at} says block {index} is a {kind} and it is a {} — read the page again",
                section.kind
            )));
        }
        if !section
            .leaves
            .iter()
            .any(|(known, _)| known.as_str() == pointer)
        {
            return Err(unprocessable(format!(
                "rewrite {at} points at {pointer}, which is not text you may rewrite on that block"
            )));
        }
        operations.push(SiteEditOperation::RewriteCopy {
            target: SiteSectionTarget { index, kind },
            pointer,
            text: text.to_owned(),
        });
    }
    Ok(operations)
}

// ---- what a drafted page is made of ------------------------------------------

/// The sections of a drafted page: a hero, and a features grid of the blocks.
///
/// Two section types and nothing else, on purpose. Every other type either
/// carries a claim a website must not invent (prices, people, testimonials) or
/// needs something the tenant owns and an agent does not have (an image, a form,
/// a collection). What comes back has already passed the Sites schema gate.
fn draft_sections(
    heading: &str,
    intro: Option<&str>,
    args: &Value,
) -> Result<SectionsEnvelope, Problem> {
    let mut sections = vec![json!({
        "type": "hero",
        "heading": heading,
        "subheading": intro,
    })];
    let blocks = match args.get("sections") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items.clone(),
        Some(_) => return Err(unprocessable("sections must be a list of blocks")),
    };
    if blocks.len() > MAX_DRAFT_BLOCKS {
        return Err(unprocessable(format!(
            "a drafted page may have at most {MAX_DRAFT_BLOCKS} blocks"
        )));
    }
    let mut items = Vec::with_capacity(blocks.len());
    for (position, block) in blocks.iter().enumerate() {
        let at = position + 1;
        let heading = string_arg(block, "heading")
            .ok_or_else(|| unprocessable(format!("block {at} needs a heading")))?;
        let body = string_arg(block, "body")
            .ok_or_else(|| unprocessable(format!("block {at} needs something to say")))?;
        items.push(json!({ "title": heading, "body": body }));
    }
    if !items.is_empty() {
        sections.push(json!({ "type": "features", "items": items }));
    }
    SectionsEnvelope::from_value(json!({
        "schema_version": alo_store::SECTIONS_SCHEMA_VERSION,
        "sections": sections,
    }))
    .map_err(|error| unprocessable(error.to_string()))
}

/// A title, as a page address: lowercase, letters and digits, hyphens between.
///
/// Empty when the title carries nothing the address rules accept — a Greek or
/// Japanese title, say — and the caller asks for a slug instead rather than
/// inventing one.
fn slugify(title: &str) -> String {
    let mut slug = String::new();
    for character in title.trim().to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').chars().take(80).collect::<String>()
}

// ---- what a review finds -----------------------------------------------------

/// Everything wrong with one page, as reason codes.
///
/// `pages` is the whole site, because one of the findings — two pages sharing a
/// title — is not a property of a page on its own.
fn review_page(page: &SitePage, pages: &[SitePage]) -> Vec<&'static str> {
    let mut found = Vec::new();
    match page.seo_description.as_deref() {
        None => found.push("noSeoDescription"),
        Some(description) => {
            let length = description.chars().count();
            if length < SEO_DESCRIPTION_MIN {
                found.push("seoDescriptionTooShort");
            } else if length > SEO_DESCRIPTION_MAX {
                found.push("seoDescriptionTooLong");
            }
        }
    }
    if page
        .seo_title
        .as_deref()
        .is_some_and(|title| title.chars().count() > SEO_TITLE_MAX)
    {
        found.push("seoTitleTooLong");
    }
    if pages
        .iter()
        .any(|other| other.id != page.id && other.title.to_lowercase() == page.title.to_lowercase())
    {
        found.push("duplicateTitle");
    }
    let envelope = SectionsEnvelope::from_value(page.sections.clone()).ok();
    match envelope {
        None => found.push("unreadableSections"),
        Some(envelope) if envelope.sections.is_empty() => found.push("emptyPage"),
        Some(envelope) => {
            if !envelope.sections.iter().any(has_heading) {
                found.push("noHeading");
            }
            if envelope
                .sections
                .iter()
                .flat_map(Section::images)
                .any(|image| image.alt.trim().is_empty())
            {
                found.push("imageWithoutAltText");
            }
        }
    }
    found
}

/// Whether this section carries a visible heading of its own — the thing a
/// search engine reads first and a page with none is missing.
fn has_heading(section: &Section) -> bool {
    serde_json::to_value(section)
        .ok()
        .and_then(|value| {
            value
                .get("heading")
                .and_then(Value::as_str)
                .map(|heading| !heading.trim().is_empty())
        })
        .unwrap_or(false)
}

/// Everything wrong with the site rather than with one of its pages.
fn review_site(site: &Site, pages: &[SitePage]) -> Vec<&'static str> {
    let mut found = Vec::new();
    if pages.is_empty() {
        found.push("noPages");
    } else if !pages.iter().any(|page| page.is_home) {
        found.push("noHomePage");
    }
    if site.status != SiteStatus::Live {
        // Not a fault of the copy, and reported anyway: a flawless page nobody
        // published is invisible to every search engine there is.
        found.push("notPublished");
    }
    found
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use alo_store::SiteKnowledgeSourceId;
    use time::OffsetDateTime;

    fn document(citation: GroundingCitation, title: &str, text: &str) -> GroundingDocument {
        GroundingDocument {
            citation,
            title: title.to_owned(),
            text: text.to_owned(),
        }
    }

    fn page(title: &str, slug: &str, sections: Value) -> SitePage {
        SitePage {
            id: SitePageId::new(format!("page-{slug}")),
            slug: slug.to_owned(),
            title: title.to_owned(),
            sections,
            seo_title: None,
            seo_description: None,
            content_locale: "en".to_owned(),
            nav_order: 0,
            is_home: slug.is_empty(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn envelope(sections: Value) -> SectionsEnvelope {
        SectionsEnvelope::from_value(json!({
            "schema_version": alo_store::SECTIONS_SCHEMA_VERSION,
            "sections": sections,
        }))
        .expect("the fixture is a valid page")
    }

    /// The question's own words decide, and a document none of them touch is not
    /// among the sources at all — the difference between an answer from the site
    /// and an answer from whatever happened to be published.
    #[test]
    fn an_answer_is_ranked_by_the_question_and_nothing_else_is_offered() {
        let corpus = vec![
            document(
                GroundingCitation::Page {
                    slug: String::new(),
                    locale: "en".to_owned(),
                },
                "Juniper Bakery",
                "Sourdough baked every morning.",
            ),
            document(
                GroundingCitation::Page {
                    slug: "visit".to_owned(),
                    locale: "en".to_owned(),
                },
                "Opening hours",
                "We open at seven and close at four, Tuesday to Sunday.",
            ),
            document(
                GroundingCitation::Post {
                    slug: "new-oven".to_owned(),
                },
                "A new oven",
                "It arrived on Tuesday and it is enormous.",
            ),
        ];
        let ranked = rank_passages(&corpus, "what are your opening hours?", MAX_PASSAGES);
        assert_eq!(ranked.len(), 1, "{ranked:?}");
        assert_eq!(ranked[0].title, "Opening hours");

        // A word in the title outranks the same word in the body.
        let ranked = rank_passages(
            &corpus,
            "when did the oven arrive on Tuesday?",
            MAX_PASSAGES,
        );
        assert_eq!(ranked[0].title, "A new oven");
        assert_eq!(ranked[1].title, "Opening hours");

        // Nothing matching is nothing returned — never the whole corpus.
        assert!(rank_passages(&corpus, "do you sell bicycles?", MAX_PASSAGES).is_empty());
        assert!(rank_passages(&[], "anything", MAX_PASSAGES).is_empty());
    }

    #[test]
    fn the_limit_holds_and_a_passage_says_when_it_was_cut() {
        let corpus: Vec<GroundingDocument> = (0..9)
            .map(|n| {
                document(
                    GroundingCitation::Post {
                        slug: format!("post-{n}"),
                    },
                    "Bread",
                    "bread bread bread",
                )
            })
            .collect();
        assert_eq!(
            rank_passages(&corpus, "bread", MAX_PASSAGES).len(),
            MAX_PASSAGES
        );

        // Counted in characters: a byte cut would panic mid-letter on a
        // European site's own copy.
        let long = document(
            GroundingCitation::Knowledge {
                source_id: SiteKnowledgeSourceId::new("k1".to_owned()),
            },
            "Voorwaarden",
            &"é".repeat(MAX_PASSAGE_CHARS + 40),
        );
        let value = passage_json(&long);
        assert_eq!(value["truncated"], json!(true));
        assert_eq!(
            value["text"].as_str().unwrap().chars().count(),
            MAX_PASSAGE_CHARS
        );
        assert_eq!(value["citation"]["kind"], json!("knowledge"));
        assert_eq!(value["citation"]["sourceId"], json!("k1"));
    }

    /// **The rule that keeps an edit to the words.** A link's target, an
    /// image's blob and a form's id are on the page and are not copy; a
    /// `custom_code` block contributes nothing at all.
    #[test]
    fn only_prose_is_rewritable_and_the_wiring_is_not() {
        let page = envelope(json!([
            {
                "type": "hero",
                "heading": "Fresh bread",
                "subheading": "Every morning",
                "primary_cta": { "label": "Visit us", "href": "/visit" },
            },
            {
                "type": "custom_code",
                "title": "Widget",
                "html": "<p>anything</p>",
                "height_px": 200,
            },
        ]));
        let leaves = copy_leaves(&page);
        assert_eq!(leaves.len(), 2);
        assert_eq!(leaves[0].kind, "hero");
        let pointers: Vec<&str> = leaves[0]
            .leaves
            .iter()
            .map(|(pointer, _)| pointer.as_str())
            .collect();
        assert!(pointers.contains(&"/heading"));
        assert!(pointers.contains(&"/subheading"));
        assert!(pointers.contains(&"/primary_cta/label"));
        assert!(
            !pointers.contains(&"/primary_cta/href"),
            "a link's target is not copy: {pointers:?}"
        );
        assert!(!pointers.contains(&"/type"), "{pointers:?}");
        // The code block is listed — a reader should see it is there — and
        // offers nothing to rewrite.
        assert_eq!(leaves[1].kind, "custom_code");
        assert!(leaves[1].leaves.is_empty());
    }

    #[test]
    fn a_rewrite_must_name_a_block_that_is_there_a_type_that_matches_and_text_that_is_copy() {
        let page = envelope(json!([
            {
                "type": "hero",
                "heading": "Fresh bread",
                "primary_cta": { "label": "Visit us", "href": "/visit" },
            },
        ]));
        let ok = rewrite_operations(
            &json!({ "copy": [
                { "index": 0, "type": "hero", "pointer": "/heading", "text": "Fresh sourdough" },
            ]}),
            &page,
        )
        .expect("a pointer the read tool handed out");
        assert_eq!(ok.len(), 1);

        let refused = |args: Value| {
            rewrite_operations(&args, &page)
                .expect_err("this one must be refused")
                .detail
                .unwrap_or_default()
        };
        // The wiring, asked for directly.
        assert!(
            refused(json!({ "copy": [
                { "index": 0, "type": "hero", "pointer": "/primary_cta/href", "text": "https://elsewhere.example" },
            ]}))
            .contains("not text you may rewrite"),
        );
        // A block that is not there, and a type that no longer matches.
        assert!(
            refused(json!({ "copy": [
                { "index": 4, "type": "hero", "pointer": "/heading", "text": "x" },
            ]}))
            .contains("names no block")
        );
        assert!(
            refused(json!({ "copy": [
                { "index": 0, "type": "features", "pointer": "/heading", "text": "x" },
            ]}))
            .contains("read the page again")
        );
        // A leaf that does not exist on that block at all.
        assert!(
            refused(json!({ "copy": [
                { "index": 0, "type": "hero", "pointer": "/subheading", "text": "x" },
            ]}))
            .contains("not text you may rewrite")
        );
        // The shapes that are not an edit.
        assert!(refused(json!({ "copy": "everything" })).contains("must be a list"));
        assert!(
            refused(json!({ "copy": [{ "type": "hero", "pointer": "/heading", "text": "x" }] }))
                .contains("which block")
        );
        assert!(
            refused(json!({ "copy": [{ "index": 0, "type": "hero", "pointer": "/heading" }] }))
                .contains("new wording")
        );
        // …and nothing asked for is no operations rather than an error: the
        // caller decides whether a title on its own is enough.
        assert!(rewrite_operations(&json!({}), &page).unwrap().is_empty());
    }

    #[test]
    fn a_rewritten_page_is_the_page_with_one_thing_changed() {
        let page = envelope(json!([
            { "type": "hero", "heading": "Fresh bread", "primary_cta": { "label": "Visit us", "href": "/visit" } },
        ]));
        let operations = rewrite_operations(
            &json!({ "copy": [
                { "index": 0, "type": "hero", "pointer": "/heading", "text": "Fresh sourdough" },
            ]}),
            &page,
        )
        .unwrap();
        let edited = alo_ai::apply_site_edit(
            &page,
            &SiteEditEnvelope {
                schema_version: SITE_EDIT_SCHEMA_VERSION,
                operations,
            },
        )
        .expect("the rewrite applies");
        let value = serde_json::to_value(&edited.sections[0]).unwrap();
        assert_eq!(value["heading"], json!("Fresh sourdough"));
        // The wiring came through untouched, which is the other half of the
        // rule: an edit changes the one leaf it named.
        assert_eq!(value["primary_cta"]["href"], json!("/visit"));
        assert_eq!(value["primary_cta"]["label"], json!("Visit us"));
    }

    /// A drafted page is a headline and prose. Every argument that could carry
    /// an invented fact is absent by construction — there is nowhere to put a
    /// price, a person or an asset.
    #[test]
    fn a_drafted_page_is_a_hero_and_its_blocks_and_nothing_else() {
        let sections = draft_sections(
            "Our services",
            Some("What we do"),
            &json!({ "sections": [
                { "heading": "Repairs", "body": "We fix what you bring in." },
                { "heading": "Maintenance", "body": "Yearly, on a plan." },
            ]}),
        )
        .expect("a valid drafted page");
        assert_eq!(sections.sections.len(), 2);
        assert_eq!(sections.sections[0].kind(), "hero");
        assert_eq!(sections.sections[1].kind(), "features");
        let hero = serde_json::to_value(&sections.sections[0]).unwrap();
        assert_eq!(hero["heading"], json!("Our services"));
        assert_eq!(hero["subheading"], json!("What we do"));
        assert!(hero.get("image").is_none(), "an agent claims no asset");
        let grid = serde_json::to_value(&sections.sections[1]).unwrap();
        assert_eq!(grid["items"][1]["title"], json!("Maintenance"));

        // A page with no blocks is a hero on its own, not an empty page.
        let bare = draft_sections("Contact", None, &json!({})).unwrap();
        assert_eq!(bare.sections.len(), 1);

        // And a block that says nothing is refused, by position.
        let why = draft_sections(
            "x",
            None,
            &json!({ "sections": [{ "heading": "Repairs" }] }),
        )
        .expect_err("a block with no body")
        .detail
        .unwrap_or_default();
        assert!(why.contains("block 1 needs something to say"), "{why}");
        let why = draft_sections("x", None, &json!({ "sections": "prose" }))
            .expect_err("blocks are a list")
            .detail
            .unwrap_or_default();
        assert!(why.contains("must be a list"), "{why}");
    }

    #[test]
    fn a_title_becomes_an_address_or_asks_for_one() {
        assert_eq!(slugify("Our services"), "our-services");
        assert_eq!(slugify("  Prices & terms  "), "prices-terms");
        assert_eq!(slugify("Über uns"), "ber-uns");
        assert_eq!(slugify("//weird//"), "weird");
        // Nothing the address rules accept: the executor asks rather than
        // inventing one.
        assert_eq!(slugify("日本語"), "");
        assert!(alo_store::validate_page_slug(&slugify("Our services")).is_ok());
    }

    #[test]
    fn a_review_reports_what_is_on_the_page_by_code() {
        let hero = json!({
            "schema_version": alo_store::SECTIONS_SCHEMA_VERSION,
            "sections": [{ "type": "hero", "heading": "Fresh bread" }],
        });
        let mut home = page("Home", "", hero.clone());
        home.seo_description =
            Some("A bakery in Ghent baking sourdough every single morning, from seven.".to_owned());
        let mut about = page("Home", "about", hero.clone());
        about.seo_description = Some("Too short.".to_owned());
        about.seo_title = Some("x".repeat(SEO_TITLE_MAX + 1));
        let bare = page(
            "Contact",
            "contact",
            json!({
                "schema_version": alo_store::SECTIONS_SCHEMA_VERSION,
                "sections": [],
            }),
        );
        let pages = vec![home.clone(), about.clone(), bare.clone()];

        assert!(review_page(&home, &pages).contains(&"duplicateTitle"));
        assert_eq!(
            review_page(&home, &pages),
            ["duplicateTitle"],
            "a complete page reports nothing else"
        );
        let found = review_page(&about, &pages);
        assert!(found.contains(&"seoDescriptionTooShort"));
        assert!(found.contains(&"seoTitleTooLong"));
        assert!(found.contains(&"duplicateTitle"));
        let found = review_page(&bare, &pages);
        assert!(found.contains(&"noSeoDescription"));
        assert!(found.contains(&"emptyPage"));
        // An over-long description is the other end of the same finding.
        let mut wordy = home.clone();
        wordy.seo_description = Some("a".repeat(SEO_DESCRIPTION_MAX + 1));
        assert!(review_page(&wordy, &pages).contains(&"seoDescriptionTooLong"));
    }

    #[test]
    fn a_page_with_no_heading_or_an_unlabelled_picture_is_found() {
        let pages = vec![page(
            "Gallery",
            "gallery",
            json!({
                "schema_version": alo_store::SECTIONS_SCHEMA_VERSION,
                "sections": [{
                    "type": "gallery",
                    "images": [{ "blob_id": "b1", "alt": "  " }],
                }],
            }),
        )];
        let found = review_page(&pages[0], &pages);
        assert!(found.contains(&"noHeading"), "{found:?}");
        assert!(found.contains(&"imageWithoutAltText"), "{found:?}");
    }

    #[test]
    fn a_site_nobody_published_is_a_finding_of_its_own() {
        let site = Site {
            id: SiteId::new("s1".to_owned()),
            name: "Bakery".to_owned(),
            subdomain: "bakery".to_owned(),
            status: SiteStatus::Draft,
            theme: json!({}),
            default_locale: "en".to_owned(),
            enabled_locales: vec!["en".to_owned()],
            created_by: "u1".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        };
        assert_eq!(review_site(&site, &[]), ["noPages", "notPublished"]);

        let pages = vec![page(
            "About",
            "about",
            json!({
                "schema_version": alo_store::SECTIONS_SCHEMA_VERSION,
                "sections": [],
            }),
        )];
        assert_eq!(review_site(&site, &pages), ["noHomePage", "notPublished"]);

        let live = Site {
            status: SiteStatus::Live,
            ..site
        };
        let pages = vec![page(
            "Home",
            "",
            json!({
                "schema_version": alo_store::SECTIONS_SCHEMA_VERSION,
                "sections": [],
            }),
        )];
        assert!(review_site(&live, &pages).is_empty());
    }

    /// The stop list is what keeps "what does the site say about delivery?" from
    /// matching every page that contains the word "about".
    #[test]
    fn the_words_worth_matching_on_are_the_ones_the_question_is_about() {
        assert_eq!(
            content_words("What are your opening hours?"),
            ["opening", "hours"]
        );
        assert_eq!(content_words("Do you deliver?"), ["deliver"]);
        // Its own product's words are noise here: every question the Website
        // agent is asked contains one of them, so matching on them would rank
        // the whole corpus.
        assert_eq!(content_words("what does my website page say"), ["say"]);
        // Repeats are asked once, and the list is bounded — a question nobody
        // meant as a question cannot become twenty passes over the corpus.
        assert_eq!(content_words("bread bread bread"), ["bread"]);
        let long: String = (0..20).map(|n| format!("word{n} ")).collect();
        assert_eq!(content_words(&long).len(), 12);
    }
}
