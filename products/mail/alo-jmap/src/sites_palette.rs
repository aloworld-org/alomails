//! The section palette over HTTP (alo Sites, ADR 0042 §4, S3.01d) — what the
//! editor offers when somebody adds a block to a page.
//!
//! Two routes, and both are reads:
//!
//! - `GET /sites/{id}/pages/{pid}/palette` answers one entry per section type,
//!   each either a **ready section made of this tenant's own content**
//!   ([`alo_store::site_seed`]) or the reason there is nothing of theirs to put
//!   in it yet. The editor drops a ready one straight onto the page through the
//!   existing `POST …/sections`; the rest open the prop form they always did.
//! - `GET /sites/{id}/pages/{pid}/palette/{kind}/preview` renders that seeded
//!   section through the **same renderer the public service uses**, in this
//!   site's own theme — which is the half of ADR 0042's "rather than lorem
//!   ipsum" that a JSON body cannot show. A tile is therefore a picture of what
//!   dropping it would actually produce, not an illustration that ages the
//!   moment a section's markup changes.
//!
//! Nothing here writes. The palette is a read of the tenant's own website
//! reshaped into sixteen offers, so a page is only changed by the same section
//! ops every other gesture goes through — one door, one validation, one undo
//! history (`docs/design/sites.md`).
//!
//! Errors follow the rest of the `/sites` surface: `401` unauthenticated, `404`
//! for a site, page or section type this build cannot resolve — a foreign
//! tenant's site is indistinguishable from a mistyped id — and `422` when a
//! preview is asked for a tile that has nothing to show.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use alo_sites::render::sections_lenient;
use alo_store::id::{SiteId, SitePageId};
use alo_store::{
    SECTION_KINDS, Section, SectionSeed, SectionsEnvelope, SeedBinding, SeedContext, SeedPage,
    Site, SitePage, seed_section,
};

use crate::error::Problem;
use crate::sites::{map_store_err, page_record, render_preview_html, require_site};
use crate::state::{Account, AppState, authenticate};

/// One palette entry as the editor reads it.
fn seed_json(kind: &str, seed: &SectionSeed) -> Result<Value, Problem> {
    Ok(match seed {
        SectionSeed::Ready(section) => json!({
            "kind": kind,
            "ready": true,
            "section": serde_json::to_value(&**section).map_err(|_| Problem::server_error())?,
        }),
        SectionSeed::NeedsInput(need) => json!({
            "kind": kind,
            "ready": false,
            "needs": need.as_str(),
        }),
    })
}

/// `GET /sites/{id}/pages/{pid}/palette` → `{"items":[{kind, ready,
/// section?, needs?}, …]}` — every section type, in the order the editor
/// offers them, seeded from this website.
pub async fn page_palette(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let sid = SiteId::new(id);
    let page_id = SitePageId::new(pid);
    let site = require_site(&account, &sid).await?;
    let page = page_record(&account, &sid, &page_id).await?;
    let context = seed_context(&account, &site, &page).await?;
    let mut items = Vec::with_capacity(SECTION_KINDS.len());
    for kind in SECTION_KINDS {
        // Unreachable: the list and the seeder are the same vocabulary, and a
        // store test holds them to that. A tile is dropped rather than a
        // palette refused if that ever stops being true.
        let Some(seed) = seed_section(kind, &context) else {
            continue;
        };
        items.push(seed_json(kind, &seed)?);
    }
    Ok(Json(json!({ "items": items })))
}

/// `GET /sites/{id}/pages/{pid}/palette/{kind}/preview` → the seeded section
/// as one complete, self-contained `text/html` document in the site's own
/// theme, for the tile's picture. Read-only and `Cache-Control: no-store`,
/// exactly like the draft preview it sits beside; a tile with nothing of the
/// tenant's own to show answers `422` rather than an empty page.
pub async fn palette_preview(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid, kind)): Path<(String, String, String)>,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let sid = SiteId::new(id);
    let page_id = SitePageId::new(pid);
    let site = require_site(&account, &sid).await?;
    let page = page_record(&account, &sid, &page_id).await?;
    let context = seed_context(&account, &site, &page).await?;
    let seed = seed_section(&kind, &context)
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such section type"))?;
    let SectionSeed::Ready(section) = seed else {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "this block has nothing of yours to show yet",
        ));
    };
    let envelope = SectionsEnvelope {
        schema_version: alo_store::SECTIONS_SCHEMA_VERSION,
        sections: vec![*section],
    };
    let sections = envelope.to_value().map_err(|_| Problem::server_error())?;
    // The one renderer, and never the editable one: a palette tile is a
    // picture of a section that does not exist yet, so a click into it must
    // not offer to rewrite copy at a coordinate no stored page has.
    let html = render_preview_html(&account, &site, &page, &sections).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        html,
    )
        .into_response())
}

/// Everything the seeds may draw on, gathered through the tenant-scoped store
/// door — this site's pages and their sections, and the first catalog,
/// collection and bookable service it owns.
///
/// The page being edited comes first in the section list, so a block added to
/// a page starts from the words on *that* page before the words elsewhere on
/// the site; the rest follow in navigation order.
async fn seed_context(
    account: &Account,
    site: &Site,
    editing: &SitePage,
) -> Result<SeedContext, Problem> {
    let id = &site.id;
    let pages = account.acc.site_pages(id).await.map_err(map_store_err)?;
    let mut sections: Vec<Section> = sections_lenient(&editing.sections);
    for page in &pages {
        if page.id == editing.id {
            continue;
        }
        sections.extend(sections_lenient(&page.sections));
    }
    let catalog = account
        .acc
        .site_catalogs(id)
        .await
        .map_err(map_store_err)?
        .into_iter()
        .next()
        .map(|catalog| SeedBinding {
            id: catalog.id.as_str().to_owned(),
            name: catalog.name,
        });
    let collection = account
        .acc
        .site_collections(id)
        .await
        .map_err(map_store_err)?
        .into_iter()
        .next()
        .map(|collection| SeedBinding {
            id: collection.id.as_str().to_owned(),
            name: collection.name,
        });
    // A service that takes no bookings is offered only when it is the only one
    // there is: a section bound to it renders honestly ("not taking bookings"),
    // which beats a tile that claims the site cannot be booked at all.
    let bookings = account.acc.site_bookings(id).await.map_err(map_store_err)?;
    let booking = bookings
        .iter()
        .find(|booking| booking.active)
        .or_else(|| bookings.first())
        .map(|booking| SeedBinding {
            id: booking.id.as_str().to_owned(),
            name: booking.name.clone(),
        });
    Ok(SeedContext {
        site_name: site.name.clone(),
        pages: pages.iter().map(seed_page).collect(),
        sections,
        catalog,
        collection,
        booking,
    })
}

fn seed_page(page: &SitePage) -> SeedPage {
    SeedPage {
        title: page.title.clone(),
        path: if page.is_home {
            "/".to_owned()
        } else {
            format!("/{}", page.slug)
        },
        is_home: page.is_home,
        description: page.seo_description.clone(),
    }
}
