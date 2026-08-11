//! Website version history (ADR 0036, S2.04a): the authenticated
//! `/sites/{id}/publishes*` routes — list every version a website has had,
//! compare two of them, and put one back online.
//!
//! It is a separate module from [`crate::sites`] because it has a separate
//! reason to change: that module is the *editing* surface (what the next
//! publish will contain), this one is the *history* surface (what previous
//! publishes did contain, and which one is live). Both live behind the same
//! guards — [`authenticate`] plus, for a restricted collaborator, the
//! per-site grant proved once in [`crate::scoped_roles`] for every
//! `/sites/{id}` template.
//!
//! Error contract, identical to the editing surface: `401` unauthenticated,
//! `404` for anything that does not resolve in the caller's tenant (a
//! version of another tenant is indistinguishable from one that never
//! existed), `422` with the store's rule-naming sentence for a refusal.
//!
//! Restoring never rewrites history — it appends a copy of the chosen
//! version and points the site at it ([`alo_store::site_versions`]), so the
//! answer carries both the new publish id and the one it came from.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{
    SiteCollectionVersionChange, SiteId, SitePageVersionChange, SitePageVersionField,
    SitePublishComparison, SitePublishId, SitePublishVersion,
};

use crate::error::Problem;
use crate::sites::{map_store_err, require_site};
use crate::state::{AppState, authenticate};

/// Versions returned when the caller does not ask for a number.
const DEFAULT_HISTORY_LIMIT: i64 = 50;

// ---- JSON shaping -----------------------------------------------------------

fn iso(t: time::OffsetDateTime) -> String {
    t.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// One version as JSON: what it froze, not the frozen content itself.
fn version_json(version: &SitePublishVersion) -> Value {
    json!({
        "id": version.id.as_str(),
        "publishedAt": iso(version.published_at),
        "publishedBy": version.published_by,
        "defaultLocale": version.default_locale,
        "enabledLocales": version.enabled_locales,
        "restoredFrom": version.restored_from.as_ref().map(SitePublishId::as_str),
        "current": version.is_current,
        "pages": version.pages,
        "locales": version.locales,
        "collections": version.collections,
    })
}

/// The wire token for a frozen page field. The store names its columns; this
/// is the API's camelCase vocabulary, and the exhaustive match means a new
/// frozen field cannot reach the wire unnamed.
fn field_token(field: SitePageVersionField) -> &'static str {
    match field {
        SitePageVersionField::Title => "title",
        SitePageVersionField::Slug => "slug",
        SitePageVersionField::Sections => "sections",
        SitePageVersionField::SeoTitle => "seoTitle",
        SitePageVersionField::SeoDescription => "seoDescription",
        SitePageVersionField::NavOrder => "navOrder",
        SitePageVersionField::Home => "home",
    }
}

fn page_change_json(change: &SitePageVersionChange) -> Value {
    json!({
        "pageId": change.page_id.as_str(),
        "locale": change.locale,
        "slug": change.slug,
        "title": change.title,
        "change": change.change.as_str(),
        "fields": change.fields.iter().map(|f| field_token(*f)).collect::<Vec<_>>(),
    })
}

fn collection_change_json(change: &SiteCollectionVersionChange) -> Value {
    json!({
        "collectionId": change.collection_id.as_str(),
        "name": change.name,
        "change": change.change.as_str(),
        "itemsBefore": change.items_before,
        "itemsAfter": change.items_after,
    })
}

fn comparison_json(comparison: &SitePublishComparison) -> Value {
    json!({
        "from": version_json(&comparison.from),
        "to": version_json(&comparison.to),
        "identical": comparison.is_identical(),
        "themeChanged": comparison.theme_changed,
        "defaultLocaleChanged": comparison.default_locale_changed,
        "localesAdded": comparison.locales_added,
        "localesRemoved": comparison.locales_removed,
        "pages": comparison.pages.iter().map(page_change_json).collect::<Vec<_>>(),
        "unchangedPages": comparison.unchanged_pages,
        "collections": comparison
            .collections
            .iter()
            .map(collection_change_json)
            .collect::<Vec<_>>(),
        "unchangedCollections": comparison.unchanged_collections,
    })
}

// ---- routes -----------------------------------------------------------------

/// `?limit=` on the history read. Deliberately a string the handler parses
/// itself: a typed `i64` would let axum's own rejection answer a plain-text
/// `400`, and every other answer on this surface is an RFC 9457 `Problem`.
#[derive(Deserialize)]
pub struct HistoryQuery {
    limit: Option<String>,
}

/// `GET /sites/:id/publishes?limit=` → `{"publishes":[…],"current":id|null}` —
/// every version of the site, newest first. `current` repeats the id of the
/// version on the internet (`null` while the site is offline) so a caller
/// need not scan the list for it.
///
/// A `limit` outside the store's allowed band is clamped, and one that is not
/// a number at all falls back to the default: a history list is not a place
/// to fail a screen over a query string.
pub async fn list_publishes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let versions = account
        .acc
        .site_publish_history(
            &site,
            query
                .limit
                .and_then(|limit| limit.trim().parse::<i64>().ok())
                .unwrap_or(DEFAULT_HISTORY_LIMIT),
        )
        .await
        .map_err(map_store_err)?;
    let current = versions
        .iter()
        .find(|version| version.is_current)
        .map_or(Value::Null, |version| json!(version.id.as_str()));
    Ok(Json(json!({
        "publishes": versions.iter().map(version_json).collect::<Vec<_>>(),
        "current": current,
    })))
}

/// `GET /sites/:id/publishes/:publish/pages` → `{"pages":[…]}` — what one
/// version froze, in navigation order: one entry per page *and language*, each
/// naming the draft page it came from (which may since have been renamed or
/// deleted without touching this row).
///
/// This is the index the visible history surface previews from
/// ([`crate::site_version_preview`]); the list read carries a page *count*,
/// because a history list has no use for every path in every version.
pub async fn list_publish_pages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, publish)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let version = SitePublishId::new(publish);
    // The version read is the guard: an unknown or foreign version must be a
    // `404`, not an empty page list that reads like a version with no pages.
    if account
        .acc
        .site_publish_version(&site, &version)
        .await
        .map_err(map_store_err)?
        .is_none()
    {
        return Err(Problem::with(
            StatusCode::NOT_FOUND,
            "no such version of this website",
        ));
    }
    let snapshots = account
        .acc
        .site_publish_snapshots(&site, &version)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "pages": snapshots
            .iter()
            .map(|snapshot| json!({
                "pageId": snapshot.page_id.as_str(),
                "locale": snapshot.locale,
                "slug": snapshot.slug,
                "title": snapshot.title,
                "home": snapshot.is_home,
                "navOrder": snapshot.nav_order,
            }))
            .collect::<Vec<_>>(),
    })))
}

/// The two ends of a comparison. Both are optional in the type and required
/// by the handler, so a caller who omits one gets this module's `Problem`
/// rather than axum's plain-text query rejection.
#[derive(Deserialize)]
pub struct CompareQuery {
    from: Option<String>,
    to: Option<String>,
}

/// `GET /sites/:id/publishes/compare?from=&to=` → what a visitor would see
/// differently between two versions: theme, languages, pages (with the
/// frozen fields that differ), and collections. Metadata only — never the
/// section content itself, which is what the visible surface previews.
///
/// A request naming fewer than two versions is `400`; either end not
/// resolving in the caller's tenant and site is `404`.
pub async fn compare_publishes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<CompareQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let (Some(from), Some(to)) = (query.from, query.to) else {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "a comparison needs both a from and a to version",
        ));
    };
    let comparison = account
        .acc
        .compare_site_publishes(&site, &SitePublishId::new(from), &SitePublishId::new(to))
        .await
        .map_err(|error| match error {
            alo_store::StoreError::NotFound => {
                Problem::with(StatusCode::NOT_FOUND, "no such version of this website")
            }
            other => map_store_err(other),
        })?;
    Ok(Json(comparison_json(&comparison)))
}

/// `POST /sites/:id/publishes/:publish/restore` →
/// `{"publishId","restoredFrom","status":"live"}` — puts that version back on
/// the internet as a NEW publish holding a copy of it. History keeps every
/// row it had, and the editable draft is not touched.
pub async fn restore_publish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, publish)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let source = SitePublishId::new(publish);
    let restored = account
        .acc
        .restore_site_publish(&site, &source)
        .await
        .map_err(|error| match error {
            alo_store::StoreError::NotFound => {
                Problem::with(StatusCode::NOT_FOUND, "no such version of this website")
            }
            other => map_store_err(other),
        })?;
    Ok(Json(json!({
        "publishId": restored.as_str(),
        "restoredFrom": source.as_str(),
        "status": "live",
    })))
}
