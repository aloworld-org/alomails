//! Previewing a past version of a website (ADR 0036, S2.04b): the
//! authenticated `GET /sites/{id}/publishes/{publish}/pages/{page}/preview`
//! route, which renders one **frozen page snapshot** as a complete,
//! self-contained HTML document.
//!
//! It is a separate module from [`crate::site_versions`] because it has a
//! separate reason to change: that module answers JSON about the history
//! (what exists, what differs, what to put back), this one renders history as
//! a document. It is separate from the draft preview in [`crate::sites`] for
//! the same reason — that one renders what the *next* publish would contain,
//! read from the editable draft; this one renders what a *previous* publish
//! did contain, read only from immutable snapshot rows.
//!
//! Nothing here reads a draft, and nothing here reads live Base rows: the
//! theme, the language contract, the sections and the collection rows all come
//! from the chosen publish, so what the owner sees before restoring is exactly
//! what restoring would put back on the internet. The one value taken from the
//! present is the site's **name**, which a publish does not freeze.
//!
//! Error contract, identical to the rest of the `/sites/{id}` surface: `401`
//! unauthenticated, and `404` for a site, a version or a page that does not
//! resolve in the caller's tenant — a version of another tenant is
//! indistinguishable from one that never existed.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use alo_sites::render::{
    ImageSources, PageRenderContext, SiteRenderContext, render_page_preview, sections_lenient,
    strings_for,
};
use alo_sites::stylesheet::stylesheet;
use alo_store::{Section, SiteId, SitePageId, SitePageSnapshot, SitePublishId, SiteTheme};

use crate::error::Problem;
use crate::sites::{map_store_err, preview_image_map, require_site, sites_domain};
use crate::state::{AppState, authenticate};

/// `?locale=` on a version preview: which frozen language to render.
#[derive(Deserialize)]
pub struct VersionPreviewQuery {
    locale: Option<String>,
}

/// `GET /sites/:id/publishes/:publish/pages/:page/preview?locale=` → the page
/// as that version froze it, as one complete `text/html` document rendered by
/// the same library the public service renders published snapshots with.
/// Images are inlined as `data:` URIs and the stylesheet is inlined, because
/// the public asset paths do not resolve on the edit origin — the same
/// contract as the draft preview, so the editor can show both in the same
/// sandboxed iframe.
///
/// Without `locale`, the version's own default language is rendered. A locale
/// that version never froze for this page falls back to that default rather
/// than refusing: history is read, not edited, and a language added later is
/// not a broken screen. `Cache-Control: no-store` — an authenticated render of
/// tenant content has no cache life on any shared hop.
pub async fn preview_version_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, publish, page)): Path<(String, String, String)>,
    Query(query): Query<VersionPreviewQuery>,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let sid = SiteId::new(id);
    let site = require_site(&account, &sid).await?;
    let pid = SitePublishId::new(publish);
    let version = account
        .acc
        .site_publish(&sid, &pid)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such version of this website"))?;
    let snapshots = account
        .acc
        .site_publish_snapshots(&sid, &pid)
        .await
        .map_err(map_store_err)?;
    let page_id = SitePageId::new(page);
    let wanted = query
        .locale
        .as_deref()
        .map(str::trim)
        .filter(|locale| !locale.is_empty())
        .unwrap_or(&version.default_locale);
    let snapshot = choose_snapshot(&snapshots, &page_id, wanted, &version.default_locale)
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such page in this version"))?;

    // The frozen Base rows, not today's — a version preview must not change
    // under the owner because somebody edited a table since.
    let mut collections = HashMap::new();
    for collection in account
        .acc
        .site_publish_collection_snapshots(&sid, &pid)
        .await
        .map_err(map_store_err)?
    {
        collections.insert(collection.collection_id.as_str().to_owned(), collection);
    }
    // A section pointing at a collection this publish did not freeze renders
    // as the renderer's own empty state; that is the public service's
    // behaviour too, and a preview must not be kinder than the internet.
    let referenced: Vec<String> = sections_lenient(&snapshot.sections)
        .into_iter()
        .filter_map(|section| match section {
            Section::Collection(collection) => Some(collection.collection_id.as_str().to_owned()),
            _ => None,
        })
        .collect();
    collections.retain(|id, _| referenced.iter().any(|wanted| wanted == id));

    // Catalogs frozen with the same publish, filtered the same way: a version
    // preview shows what that publish served, never what the catalog says now.
    let mut catalogs = HashMap::new();
    for catalog in account
        .acc
        .site_publish_catalog_snapshots(&sid, &pid)
        .await
        .map_err(map_store_err)?
    {
        catalogs.insert(catalog.catalog_id.as_str().to_owned(), catalog);
    }
    let referenced_catalogs: Vec<String> = sections_lenient(&snapshot.sections)
        .into_iter()
        .filter_map(|section| match section {
            Section::Catalog(catalog) => Some(catalog.catalog_id.as_str().to_owned()),
            _ => None,
        })
        .collect();
    catalogs.retain(|id, _| referenced_catalogs.iter().any(|wanted| wanted == id));

    // And the booking services that publish froze: a version preview shows what
    // that publish offered to book, never what the service says now.
    let mut bookings = HashMap::new();
    for booking in account
        .acc
        .site_publish_booking_snapshots(&sid, &pid)
        .await
        .map_err(map_store_err)?
    {
        bookings.insert(booking.booking_id.as_str().to_owned(), booking);
    }
    let referenced_bookings: Vec<String> = sections_lenient(&snapshot.sections)
        .into_iter()
        .filter_map(|section| match section {
            Section::Booking(booking) => Some(booking.booking_id.as_str().to_owned()),
            _ => None,
        })
        .collect();
    bookings.retain(|id, _| referenced_bookings.iter().any(|wanted| wanted == id));

    let theme = SiteTheme::from_stored(version.theme.clone());
    let images = preview_image_map(
        &account,
        &theme,
        &snapshot.sections,
        collections.values(),
        catalogs.values(),
    )
    .await;
    let base_url = format!("https://{}.{}", site.subdomain, sites_domain());
    let site_ctx = SiteRenderContext {
        name: &site.name,
        base_url: &base_url,
        locale: &snapshot.locale,
        theme: &theme,
        strings: strings_for(&snapshot.locale),
        images: ImageSources::Inline(&images),
    };
    let path = snapshot_path(snapshot, &version.default_locale);
    let page_ctx = PageRenderContext {
        path: &path,
        title: &snapshot.title,
        seo_title: snapshot.seo_title.as_deref(),
        seo_description: snapshot.seo_description.as_deref(),
        sections: &snapshot.sections,
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

/// The snapshot to render: the exact page and language when that pair was
/// frozen, otherwise the same page in the version's default language.
fn choose_snapshot<'a>(
    snapshots: &'a [SitePageSnapshot],
    page: &SitePageId,
    wanted: &str,
    default_locale: &str,
) -> Option<&'a SitePageSnapshot> {
    let of_page = |locale: &str| {
        snapshots
            .iter()
            .find(|snapshot| &snapshot.page_id == page && snapshot.locale == locale)
    };
    of_page(wanted).or_else(|| of_page(default_locale))
}

/// The public path this snapshot was served at — the same shape the public
/// service and the draft preview build, so canonical and OG URLs in the
/// preview document match what the version actually advertised.
fn snapshot_path(snapshot: &SitePageSnapshot, default_locale: &str) -> String {
    let prefix = if snapshot.locale == default_locale {
        String::new()
    } else {
        format!("/{}", snapshot.locale)
    };
    if snapshot.is_home {
        if prefix.is_empty() {
            "/".to_owned()
        } else {
            prefix
        }
    } else {
        format!("{prefix}/{}", snapshot.slug)
    }
}
