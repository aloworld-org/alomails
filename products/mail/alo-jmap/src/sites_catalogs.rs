//! The owner's side of what a site *offers* at the HTTP edge (ADR 0036, the
//! one catalog of ADR 0041): the catalogs of a site, their categories, and
//! their items.
//!
//! A separate module from [`crate::sites`] for a separate reason to change:
//! these routes answer about the tenant's own price list — the record a
//! publish freezes into a snapshot and a public order is then priced from
//! ([`crate::sites_orders`]). Everything here goes through the account door,
//! so a catalog id from another tenant — or from another site of the same
//! tenant — is indistinguishable from one that never existed.
//!
//! Two rules shape the wire contract:
//!
//! * **A price is typed, never computed here.** The body carries what the
//!   owner wrote (`"12,50"`, `"€12.50"`, `"12"`) and the store's own
//!   [`parse_price_minor_units`] turns it into integer minor units of the
//!   catalog's currency. There is no second, weaker price parser in the
//!   browser, and no float touches the path.
//! * **A handle may be left to us — once.** On a create, an absent or blank
//!   `slug` is derived from the name with the store's
//!   [`catalog_slug_from_name`], so the editor never forces a stranger to
//!   invent a URL fragment before they can add a croissant. On a replace, an
//!   absent `slug` keeps the stored one: a handle is public — a section names
//!   a category by it and an order names an item by it — so correcting a name
//!   must not silently rename the thing underneath it.
//!
//! Errors follow the `/sites/{id}` contract: `401` unauthenticated, `404` for
//! anything that does not resolve in the caller's tenant, `422` for a rule the
//! store names, `400` for a body that is not the shape.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, body::Bytes};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{
    SiteCatalog, SiteCatalogAvailability, SiteCatalogCategory, SiteCatalogCategoryId,
    SiteCatalogCategoryInput, SiteCatalogId, SiteCatalogInput, SiteCatalogItem, SiteCatalogItemId,
    SiteCatalogItemInput, SiteId, catalog_slug_from_name, currency_exponent,
    parse_price_minor_units,
};

use crate::billing::iso;
use crate::error::Problem;
use crate::sites::{map_store_err, require_site};
use crate::state::{Account, AppState, authenticate};

/// One catalog as both the list and the single read answer it.
///
/// `currencyExponent` travels with the currency because the editor formats
/// minor units without owning a second copy of the ISO 4217 exception table.
fn catalog_json(catalog: &SiteCatalog) -> Value {
    json!({
        "id": catalog.id.as_str(),
        "name": catalog.name,
        "currency": catalog.currency,
        "currencyExponent": currency_exponent(&catalog.currency),
        "ordersEnabled": catalog.orders_enabled,
        "createdAt": iso(catalog.created_at),
        "updatedAt": iso(catalog.updated_at),
    })
}

fn category_json(category: &SiteCatalogCategory) -> Value {
    json!({
        "id": category.id.as_str(),
        "name": category.name,
        "slug": category.slug,
        "position": category.position,
    })
}

fn item_json(item: &SiteCatalogItem) -> Value {
    json!({
        "id": item.id.as_str(),
        "categoryId": item.category_id.as_ref().map(SiteCatalogCategoryId::as_str),
        "name": item.name,
        "slug": item.slug,
        "description": item.description,
        "priceCents": item.price_cents,
        "priceNote": item.price_note,
        "imageBlobId": item.image.as_ref().map(alo_store::BlobId::as_str),
        "availability": item.availability.as_str(),
        "position": item.position,
        "sourceKey": item.source_key,
    })
}

/// `GET /sites/:id/catalogs` -> every catalog of the site, in creation order.
/// Lean by design: the items of a catalog are one read away, and a site with
/// four catalogs must not pay for four hundred items to draw a list.
pub async fn list_catalogs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let catalogs = account
        .acc
        .site_catalogs(&site)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "catalogs": catalogs.iter().map(catalog_json).collect::<Vec<_>>()
    })))
}

/// The body of a catalog create or replace. `ordersEnabled` defaults to
/// *off*: a new price list is a price list, and taking orders is a decision
/// somebody makes on purpose.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CatalogBody {
    name: String,
    currency: String,
    #[serde(default)]
    orders_enabled: bool,
}

/// `POST /sites/:id/catalogs` -> the stored catalog.
pub async fn create_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CatalogBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let site = SiteId::new(id);
    let catalog = account
        .acc
        .create_site_catalog(
            &site,
            &SiteCatalogInput {
                name: &req.name,
                currency: &req.currency,
                orders_enabled: req.orders_enabled,
            },
        )
        .await
        .map_err(map_store_err)?;
    let stored = require_catalog(&account, &site, &catalog).await?;
    Ok(Json(catalog_json(&stored)))
}

/// `GET /sites/:id/catalogs/:catalog` -> the catalog with its categories and
/// **every** item, hidden ones included: this is the editor's view, and an
/// item withheld from the public site is exactly the one its owner needs to
/// find in order to put it back.
pub async fn get_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, catalog)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    let catalog = SiteCatalogId::new(catalog);
    let stored = require_catalog(&account, &site, &catalog).await?;
    let categories = account
        .acc
        .site_catalog_categories(&site, &catalog)
        .await
        .map_err(map_store_err)?;
    let items = account
        .acc
        .site_catalog_items(&site, &catalog)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "catalog": catalog_json(&stored),
        "categories": categories.iter().map(category_json).collect::<Vec<_>>(),
        "items": items.iter().map(item_json).collect::<Vec<_>>(),
    })))
}

/// `PUT /sites/:id/catalogs/:catalog` -> the catalog as it now stands.
///
/// Changing the currency reinterprets the stored minor units rather than
/// converting them — the store says so, and the editor asks before offering
/// it. Turning ordering on or off takes effect at the next publish, because
/// the public door reads the frozen snapshot and not this row.
pub async fn update_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, catalog)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CatalogBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let site = SiteId::new(id);
    let catalog = SiteCatalogId::new(catalog);
    account
        .acc
        .update_site_catalog(
            &site,
            &catalog,
            &SiteCatalogInput {
                name: &req.name,
                currency: &req.currency,
                orders_enabled: req.orders_enabled,
            },
        )
        .await
        .map_err(map_store_err)?;
    let stored = require_catalog(&account, &site, &catalog).await?;
    Ok(Json(catalog_json(&stored)))
}

/// `DELETE /sites/:id/catalogs/:catalog` -> `204`. The categories and items go
/// with it; already-published snapshots keep serving until the next publish,
/// so deleting a price list never blanks a live page mid-afternoon.
pub async fn delete_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, catalog)): Path<(String, String)>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_site_catalog(&SiteId::new(id), &SiteCatalogId::new(catalog))
        .await
        .map_err(map_store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The body of a category create or replace.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CategoryBody {
    name: String,
    /// Public handle; absent or blank is derived from the name.
    #[serde(default)]
    slug: Option<String>,
    /// Display order; absent appends on create and keeps the stored place on
    /// replace.
    #[serde(default)]
    position: Option<i32>,
}

/// `POST /sites/:id/catalogs/:catalog/categories` -> the stored category.
pub async fn create_category(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, catalog)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CategoryBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let site = SiteId::new(id);
    let catalog = SiteCatalogId::new(catalog);
    let existing = account
        .acc
        .site_catalog_categories(&site, &catalog)
        .await
        .map_err(map_store_err)?;
    let slug = handle_for(req.slug.as_deref(), &req.name);
    let created = account
        .acc
        .create_site_catalog_category(
            &site,
            &catalog,
            &SiteCatalogCategoryInput {
                name: &req.name,
                slug: &slug,
                position: req
                    .position
                    .unwrap_or_else(|| next_position(&existing, |c| c.position)),
            },
        )
        .await
        .map_err(map_store_err)?;
    let stored = require_category(&account, &site, &catalog, &created).await?;
    Ok(Json(category_json(&stored)))
}

/// `PUT /sites/:id/catalogs/:catalog/categories/:category` -> the category as
/// it now stands. A whole replace, like the store's write: a partial update
/// cannot leave a half-described grouping on a public page.
pub async fn update_category(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, catalog, category)): Path<(String, String, String)>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CategoryBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let site = SiteId::new(id);
    let catalog = SiteCatalogId::new(catalog);
    let category = SiteCatalogCategoryId::new(category);
    let current = require_category(&account, &site, &catalog, &category).await?;
    let slug = kept_handle(req.slug.as_deref(), &current.slug);
    account
        .acc
        .update_site_catalog_category(
            &site,
            &catalog,
            &category,
            &SiteCatalogCategoryInput {
                name: &req.name,
                slug: &slug,
                position: req.position.unwrap_or(current.position),
            },
        )
        .await
        .map_err(map_store_err)?;
    let stored = require_category(&account, &site, &catalog, &category).await?;
    Ok(Json(category_json(&stored)))
}

/// `DELETE /sites/:id/catalogs/:catalog/categories/:category` -> `204`. The
/// items it grouped stay, uncategorised: deleting a grouping must never delete
/// the things grouped by it.
pub async fn delete_category(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, catalog, category)): Path<(String, String, String)>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_site_catalog_category(
            &SiteId::new(id),
            &SiteCatalogId::new(catalog),
            &SiteCatalogCategoryId::new(category),
        )
        .await
        .map_err(map_store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The body of an item create or replace.
///
/// `price` is what the owner typed. An absent or blank price is an item with
/// **no** price — an enquiry-only service — which is not the same as zero, and
/// the public order door refuses to sell it for one.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ItemBody {
    name: String,
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    category_id: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    price: Option<String>,
    #[serde(default)]
    price_note: Option<String>,
    #[serde(default)]
    image_blob_id: Option<String>,
    /// `available`, `sold_out`, or `hidden`; absent means available.
    #[serde(default)]
    availability: Option<String>,
    #[serde(default)]
    position: Option<i32>,
}

/// `POST /sites/:id/catalogs/:catalog/items` -> the stored item.
pub async fn create_item(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, catalog)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ItemBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let site = SiteId::new(id);
    let catalog = SiteCatalogId::new(catalog);
    let stored_catalog = require_catalog(&account, &site, &catalog).await?;
    let existing = account
        .acc
        .site_catalog_items(&site, &catalog)
        .await
        .map_err(map_store_err)?;
    let write = ItemWrite::read(&req, &stored_catalog.currency, None)?;
    let created = account
        .acc
        .create_site_catalog_item(
            &site,
            &catalog,
            &write.input(
                req.position
                    .unwrap_or_else(|| next_position(&existing, |i| i.position)),
            ),
        )
        .await
        .map_err(map_store_err)?;
    let stored = require_item(&account, &site, &catalog, &created).await?;
    Ok(Json(item_json(&stored)))
}

/// `PUT /sites/:id/catalogs/:catalog/items/:item` -> the item as it now
/// stands. A whole replace: every field the editor shows is sent every time,
/// so no combination of screens can leave a published card half-written.
pub async fn update_item(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, catalog, item)): Path<(String, String, String)>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ItemBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let site = SiteId::new(id);
    let catalog = SiteCatalogId::new(catalog);
    let item = SiteCatalogItemId::new(item);
    let stored_catalog = require_catalog(&account, &site, &catalog).await?;
    let current = require_item(&account, &site, &catalog, &item).await?;
    let write = ItemWrite::read(&req, &stored_catalog.currency, Some(&current.slug))?;
    account
        .acc
        .update_site_catalog_item(
            &site,
            &catalog,
            &item,
            &write.input(req.position.unwrap_or(current.position)),
        )
        .await
        .map_err(map_store_err)?;
    let stored = require_item(&account, &site, &catalog, &item).await?;
    Ok(Json(item_json(&stored)))
}

/// `DELETE /sites/:id/catalogs/:catalog/items/:item` -> `204`. Published
/// snapshots that already carry the item are untouched; the public page
/// changes at the next publish, not before.
pub async fn delete_item(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, catalog, item)): Path<(String, String, String)>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_site_catalog_item(
            &SiteId::new(id),
            &SiteCatalogId::new(catalog),
            &SiteCatalogItemId::new(item),
        )
        .await
        .map_err(map_store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The owned, already-parsed values of one item write. It exists so create and
/// replace read a body exactly once and in exactly the same way: the borrowed
/// [`SiteCatalogItemInput`] the store takes cannot own its strings, and two
/// hand-written conversions would be two chances to disagree.
struct ItemWrite {
    name: String,
    slug: String,
    category: Option<SiteCatalogCategoryId>,
    description: Option<String>,
    price_cents: Option<i64>,
    price_note: Option<String>,
    image: Option<alo_store::BlobId>,
    availability: SiteCatalogAvailability,
}

impl ItemWrite {
    /// `current` is the handle the item already carries on a replace, and
    /// `None` when one is being created.
    fn read(req: &ItemBody, currency: &str, current: Option<&str>) -> Result<Self, Problem> {
        Ok(Self {
            name: req.name.clone(),
            slug: match current {
                Some(stored) => kept_handle(req.slug.as_deref(), stored),
                None => handle_for(req.slug.as_deref(), &req.name),
            },
            category: some_text(req.category_id.as_deref()).map(SiteCatalogCategoryId::new),
            description: some_text(req.description.as_deref()).map(str::to_owned),
            price_cents: some_text(req.price.as_deref())
                .map(|price| parse_price_minor_units(price, currency))
                .transpose()
                .map_err(map_store_err)?,
            price_note: some_text(req.price_note.as_deref()).map(str::to_owned),
            image: some_text(req.image_blob_id.as_deref()).map(alo_store::BlobId::new),
            availability: availability_of(req.availability.as_deref())?,
        })
    }

    fn input(&self, position: i32) -> SiteCatalogItemInput<'_> {
        SiteCatalogItemInput {
            category: self.category.as_ref(),
            name: &self.name,
            slug: &self.slug,
            description: self.description.as_deref(),
            price_cents: self.price_cents,
            price_note: self.price_note.as_deref(),
            image: self.image.as_ref(),
            availability: self.availability,
            position,
        }
    }
}

/// The handle a **new** row gets: what was typed when something was, and the
/// store's own derivation from the name when the field was left alone. An
/// empty derivation (a name with no ASCII letters at all) is passed through so
/// the store can answer with the same sentence it would give a blank handle.
fn handle_for(slug: Option<&str>, name: &str) -> String {
    match some_text(slug) {
        Some(typed) => typed.to_owned(),
        None => catalog_slug_from_name(name),
    }
}

/// The handle a **replace** gets: the stored one unless a new one was typed.
///
/// Deliberately not [`handle_for`]: a handle is public. A catalog section
/// names a category by its handle, and an order names an item by its handle,
/// so re-deriving it from a corrected name — "Breads" becoming "Breads &
/// rolls" — would silently unhook a section from the grouping it shows. A
/// rename changes what the page *says*; changing what it is *called* has to be
/// typed on purpose.
fn kept_handle(slug: Option<&str>, stored: &str) -> String {
    some_text(slug).unwrap_or(stored).to_owned()
}

/// A field the browser sends as `""` to mean "cleared" reads as absent.
fn some_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// Parses the three availability words, naming all of them when it refuses —
/// the value is the owner's choice, not stored data, so the sentence has to be
/// one they can act on.
fn availability_of(value: Option<&str>) -> Result<SiteCatalogAvailability, Problem> {
    match some_text(value) {
        None | Some("available") => Ok(SiteCatalogAvailability::Available),
        Some("sold_out") => Ok(SiteCatalogAvailability::SoldOut),
        Some("hidden") => Ok(SiteCatalogAvailability::Hidden),
        Some(other) => Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{other} is not an availability; use available, sold_out, or hidden"),
        )),
    }
}

/// Appends after everything already there. Positions are the editor's order,
/// not a dense index: a gap left by a deletion is harmless.
fn next_position<T>(existing: &[T], position: impl Fn(&T) -> i32) -> i32 {
    existing
        .iter()
        .map(position)
        .max()
        .map_or(0, |last| last.saturating_add(1))
}

/// Resolves a catalog the caller's tenant owns on this site, or the
/// tenant-hiding 404.
async fn require_catalog(
    account: &Account,
    site: &SiteId,
    catalog: &SiteCatalogId,
) -> Result<SiteCatalog, Problem> {
    account
        .acc
        .site_catalog(site, catalog)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such catalog"))
}

async fn require_category(
    account: &Account,
    site: &SiteId,
    catalog: &SiteCatalogId,
    category: &SiteCatalogCategoryId,
) -> Result<SiteCatalogCategory, Problem> {
    account
        .acc
        .site_catalog_categories(site, catalog)
        .await
        .map_err(map_store_err)?
        .into_iter()
        .find(|stored| stored.id.as_str() == category.as_str())
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such category"))
}

async fn require_item(
    account: &Account,
    site: &SiteId,
    catalog: &SiteCatalogId,
    item: &SiteCatalogItemId,
) -> Result<SiteCatalogItem, Problem> {
    account
        .acc
        .site_catalog_item(site, catalog, item)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such item"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn a_blank_handle_is_derived_from_the_name() {
        assert_eq!(handle_for(None, "Wedding Cake"), "wedding-cake");
        assert_eq!(handle_for(Some("   "), "Wedding Cake"), "wedding-cake");
        assert_eq!(handle_for(Some("cake"), "Wedding Cake"), "cake");
    }

    #[test]
    fn a_replace_keeps_the_public_handle_unless_a_new_one_is_typed() {
        assert_eq!(kept_handle(None, "breads"), "breads");
        assert_eq!(kept_handle(Some(""), "breads"), "breads");
        assert_eq!(kept_handle(Some("breads-rolls"), "breads"), "breads-rolls");
    }

    #[test]
    fn a_name_with_no_ascii_letters_derives_nothing_and_is_left_to_the_store() {
        assert_eq!(handle_for(None, "———"), "");
    }

    #[test]
    fn availability_words_parse_and_an_unknown_one_names_the_three() {
        assert_eq!(
            availability_of(None).unwrap(),
            SiteCatalogAvailability::Available
        );
        assert_eq!(
            availability_of(Some("sold_out")).unwrap(),
            SiteCatalogAvailability::SoldOut
        );
        assert_eq!(
            availability_of(Some("hidden")).unwrap(),
            SiteCatalogAvailability::Hidden
        );
        let refused = availability_of(Some("maybe")).unwrap_err();
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(refused.detail.unwrap_or_default().contains("sold_out"));
    }

    #[test]
    fn positions_append_after_the_last_one() {
        assert_eq!(next_position::<i32>(&[], |value| *value), 0);
        assert_eq!(next_position(&[0, 4, 2], |value| *value), 5);
    }
}
