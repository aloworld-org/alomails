//! Locations HTTP surface (alo Inventory, ADR 0035, wave B5.04b) — the places
//! stock can be, over [`alo_store::inv_locations`].
//!
//! Four decisions this file makes rather than the store.
//!
//! - **The list seeds the tenant's locations on first read**, in the caller's
//!   language ([`crate::inventory_location_names`]). A tenant who has never
//!   opened Inventory is handed one real warehouse and the four virtual
//!   counterparties every document movement needs, rather than an empty screen
//!   with an "add your first location" button — which is what makes receiving
//!   the first purchase order book itself instead of failing with "there is
//!   nowhere to put it". The seed runs once per tenant (`inv_seeds`), so a
//!   tenant who deleted ours and typed their own is not handed ours again the
//!   next morning.
//! - **Archiving is its own `POST`, never a field on the `PATCH`.** The
//!   convention `/billing/customers/{id}/archive` set, for its reason: an
//!   ordinary rename must never be able to drop a warehouse out of every
//!   picker because a stale form carried the flag.
//! - **`DELETE` stays strict.** It is the escape hatch for the row created a
//!   minute ago with a typo in its code, and the store refuses it with a `409`
//!   once anything has moved through the place — because the location's name is
//!   part of the explanation of that movement. Silently archiving instead would
//!   answer a different request than the one that was made.
//! - **A `PATCH` is merged onto the stored record here**, because the store's
//!   update is a full replace. Absent means "leave it alone", so renaming a
//!   place cannot silently rewrite its code — and `kind` is not writable at all:
//!   it is what every rule in the ledger is about.
//!
//! Nothing here is personal data: a shelf label, a name a tenant gave their own
//! warehouse, and a kind.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::AccountStore;
use alo_store::InvLocationId;
use alo_store::inv_locations::{Location, LocationKind, NewLocation};

use crate::billing::{flag, iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::inventory_location_names::location_seed_for;
use crate::state::{AppState, authenticate};

/// A location as JSON.
///
/// `system` says we seeded it and the rules depend on its kind: renameable,
/// never archivable, never deletable, and never creatable a second time. A
/// client shows the real places in a picker and needs the virtual ones only to
/// explain a movement that already happened.
pub(crate) fn location_json(l: &Location) -> Value {
    json!({
        "id": l.id.as_str(),
        "code": l.code,
        "name": l.name,
        "kind": l.kind.as_str(),
        "system": l.kind.is_virtual(),
        "archived": l.is_archived(),
        "archivedAt": l.archived_at.map(iso),
        "createdBy": l.created_by,
        "createdAt": iso(l.created_at),
        "updatedAt": iso(l.updated_at),
    })
}

/// The stored record as writable input — the base a `PATCH` merges onto.
fn editable(l: &Location) -> NewLocation {
    NewLocation {
        code: l.code.clone(),
        name: l.name.clone(),
        kind: l.kind,
    }
}

/// The writable fields of a location.
///
/// `kind` is accepted on create and **ignored on update**: a stored location's
/// kind never changes, because re-kinding one retroactively rewrites the
/// meaning of every movement already recorded there. The store refuses a
/// changed kind outright; this layer simply never offers it the chance.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocationBody {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    kind: Option<String>,
}

impl LocationBody {
    /// Merges the stated fields onto `base`, leaving the rest as they were.
    /// `kind` is read only when `base` has not got one to keep.
    fn apply(self, base: NewLocation, keep_kind: bool) -> Result<NewLocation, Problem> {
        let kind = match (keep_kind, self.kind.as_deref()) {
            (true, _) | (false, None) => base.kind,
            (false, Some(stated)) => LocationKind::parse(stated).map_err(map_store_err)?,
        };
        Ok(NewLocation {
            code: self.code.unwrap_or(base.code),
            name: self.name.unwrap_or(base.name),
            kind,
        })
    }
}

/// Loads one of the tenant's locations, or fails with the `404` an id from
/// another tenant gets.
pub(crate) async fn load(acc: &AccountStore, id: &InvLocationId) -> Result<Location, Problem> {
    acc.inv_location(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such location"))
}

/// Query string of the list route.
#[derive(Deserialize)]
pub struct ListQuery {
    /// `includeArchived=1` also returns archived locations, sorted after the
    /// active ones.
    #[serde(default, rename = "includeArchived")]
    include_archived: Option<String>,
    /// The language the starting locations are named in, on the first read a
    /// tenant ever makes. Ignored afterwards — the seed runs once.
    #[serde(default)]
    lang: Option<String>,
}

/// `GET /inventory/locations[?includeArchived=1&lang=nl]` →
/// `{"locations":[…]}` — the tenant's places in code order, **seeding the
/// starting set on first use**.
pub async fn list_locations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let locations = account
        .acc
        .inv_locations_or_seed(
            &location_seed_for(q.lang.as_deref().unwrap_or_default()),
            flag(q.include_archived.as_deref()),
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "locations": locations.iter().map(location_json).collect::<Vec<_>>(),
    })))
}

/// `POST /inventory/locations` `{code, name, kind}` → `{"location":{…}}` — a
/// second warehouse, a van, a shop floor, or the transit two warehouses need.
///
/// `kind` defaults to `stock`; the four virtual counterparties are refused with
/// a `422`, because exactly one of each exists per tenant and a receipt that
/// could choose between two supplier locations makes every balance on it a
/// half-truth.
pub async fn create_location(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: LocationBody = parse_body(&body)?;
    let input = req.apply(NewLocation::default(), false)?;
    let id = account
        .acc
        .create_inv_location(&input)
        .await
        .map_err(map_store_err)?;
    let location = load(&account.acc, &id).await?;
    Ok(Json(json!({ "location": location_json(&location) })))
}

/// `GET /inventory/locations/{id}` → `{"location":{…}}`. Archived locations are
/// readable by id, so a movement recorded last year can still be explained.
pub async fn get_location(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let location = load(&account.acc, &InvLocationId::new(id)).await?;
    Ok(Json(json!({ "location": location_json(&location) })))
}

/// `PATCH /inventory/locations/{id}` `{code?, name?}` → `{"location":{…}}` —
/// rename or recode, including the seeded counterparties (our word for them was
/// a starting point, not a claim).
pub async fn update_location(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: LocationBody = parse_body(&body)?;
    let id = InvLocationId::new(id);
    let stored = load(&account.acc, &id).await?;
    let input = req.apply(editable(&stored), true)?;
    account
        .acc
        .update_inv_location(&id, &input)
        .await
        .map_err(map_store_err)?;
    let location = load(&account.acc, &id).await?;
    Ok(Json(json!({ "location": location_json(&location) })))
}

/// `DELETE /inventory/locations/{id}` → `{"deleted":true}` — only while the
/// place has never carried a movement.
///
/// Afterwards the store answers `409` and the answer is
/// [`archive_location`]: a location's name is part of the explanation of every
/// movement recorded there.
pub async fn delete_location(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_inv_location(&InvLocationId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "deleted": true })))
}

#[derive(Deserialize)]
struct ArchiveBody {
    /// `false` restores. Required when a body is sent; an **empty** body
    /// archives, because the route's name is already the intent.
    archived: bool,
}

/// `POST /inventory/locations/{id}/archive` `{"archived":true}` →
/// `{"location":{…}}` — take a place out of the pickers without losing what it
/// explains. Idempotent; re-archiving keeps the original time.
///
/// A location holding stock may be archived, deliberately: a shed is archived
/// while it is being emptied, and the movements *out* of it are exactly what
/// must keep working (`alo_store::inv_adjust` refuses movements *into* it).
pub async fn archive_location(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ArchiveBody = parse_body(if body.is_empty() {
        br#"{"archived":true}"#
    } else {
        &body
    })?;
    let id = InvLocationId::new(id);
    account
        .acc
        .set_inv_location_archived(&id, req.archived)
        .await
        .map_err(map_store_err)?;
    let location = load(&account.acc, &id).await?;
    Ok(Json(json!({ "location": location_json(&location) })))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn body(json: Value) -> LocationBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn stored() -> NewLocation {
        NewLocation {
            code: "WH2".to_owned(),
            name: "Tweede magazijn".to_owned(),
            kind: LocationKind::Stock,
        }
    }

    #[test]
    fn an_empty_patch_changes_nothing() {
        let merged = body(json!({}))
            .apply(stored(), true)
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(merged.code, "WH2");
        assert_eq!(merged.name, "Tweede magazijn");
        assert_eq!(merged.kind, LocationKind::Stock);
    }

    #[test]
    fn a_rename_leaves_the_code_alone() {
        let merged = body(json!({ "name": "Winkel Amsterdam" }))
            .apply(stored(), true)
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(merged.name, "Winkel Amsterdam");
        assert_eq!(merged.code, "WH2");
    }

    #[test]
    fn an_update_cannot_re_kind_a_location() {
        // Re-kinding retroactively rewrites the meaning of every movement
        // already recorded there, so the field is not even read on a PATCH.
        let merged = body(json!({ "kind": "supplier" }))
            .apply(stored(), true)
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(merged.kind, LocationKind::Stock);
    }

    #[test]
    fn create_defaults_to_a_real_place_and_reads_a_stated_kind() {
        let plain = body(json!({ "code": "van1", "name": "Bestelbus" }))
            .apply(NewLocation::default(), false)
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(plain.kind, LocationKind::Stock);
        // Normalisation is the store's: this layer passes the typed word on.
        assert_eq!(plain.code, "van1");

        let transit = body(json!({ "code": "TR", "name": "Onderweg", "kind": "transit" }))
            .apply(NewLocation::default(), false)
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(transit.kind, LocationKind::Transit);
    }

    #[test]
    fn an_unknown_kind_is_the_caller_s_422_and_not_a_silent_default() {
        let refused = body(json!({ "code": "X", "name": "X", "kind": "shelf" }))
            .apply(NewLocation::default(), false)
            .expect_err("an unknown kind must not become `stock`");
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    }
}
