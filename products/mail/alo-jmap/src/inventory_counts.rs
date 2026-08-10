//! The stocktake over HTTP (alo Inventory, ADR 0035, wave B5.08a) — over
//! [`alo_store::inv_count`].
//!
//! Six routes, one document: a count is opened for a place, its sheet is worked
//! down a row at a time, and it ends either applied (B5.08b) or walked away
//! from.
//!
//! Two shapes are worth stating here rather than leaving to be inferred.
//!
//! - **A row is `PUT`, not `POST`ed.** Its identity is the pair (count,
//!   product), so `PUT /inventory/counts/{id}/lines/{product_id}` states the row
//!   whole and is idempotent: a wedge scanner that fires twice on one barcode
//!   records one row rather than two, and a re-count overwrites rather than
//!   accumulates. Because it states the row whole, a `PUT` with no
//!   `countedQtyMilli` clears the row **back to uncounted** — the undo of a
//!   mis-scan (`docs/design/ux-principles.md`: undo over confirm). A client
//!   adding a note to a counted row sends the quantity with it.
//! - **The client never does the arithmetic.** Every row states what was
//!   expected, what was found, the variance between them, what is on that shelf
//!   *now* and whether it moved since the sheet was opened — all computed
//!   server-side, because a screen that subtracts its own numbers and the apply
//!   that follows must not be able to disagree about what is missing.
//!
//! `moved_since` is the field the design note is really about: a warehouse does
//! not stop while it is counted, so a row whose shelf moved under the counter is
//! flagged here and skipped by the apply, rather than having a frozen difference
//! written over a delivery that went out at the far end of the room.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::AccountStore;
use alo_store::inv_count::{Count, CountEntry, CountFilter, CountLine, CountStatus, NewCount};
use alo_store::{BillingProductId, InvCountId, InvLocationId};

use crate::billing::{iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// A stocktake as JSON. The location carries its code and name as well as its
/// id: a count showing one opaque string is a count nobody can check.
fn count_json(c: &Count) -> Value {
    json!({
        "id": c.id.as_str(),
        "locationId": c.location_id.as_str(),
        "locationCode": c.location_code,
        "locationName": c.location_name,
        "status": c.status.as_str(),
        "note": c.note,
        "lineCount": c.line_count,
        "countedCount": c.counted_count,
        "varianceCount": c.variance_count,
        "createdBy": c.created_by,
        "createdAt": iso(c.created_at),
        "updatedAt": iso(c.updated_at),
        "closedAt": c.closed_at.map(iso),
        "closedBy": c.closed_by,
    })
}

/// One row of the sheet as JSON.
fn line_json(l: &CountLine) -> Value {
    json!({
        "productId": l.product_id.as_str(),
        "productName": l.product_name,
        "sku": l.sku,
        "barcode": l.barcode,
        "unit": l.unit,
        "expectedQtyMilli": l.expected_qty_milli,
        "countedQtyMilli": l.counted_qty_milli,
        "varianceQtyMilli": l.variance_qty_milli,
        "onHandQtyMilli": l.on_hand_qty_milli,
        "movedSince": l.moved_since,
        "note": l.note,
        "countedAt": l.counted_at.map(iso),
        "countedBy": l.counted_by,
    })
}

/// What opening a count states.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenBody {
    #[serde(default)]
    location_id: Option<String>,
    #[serde(default)]
    note: Option<String>,
}

/// What a `PATCH` on a count may change: the note, and nothing else. The place
/// a count is about is what the count *is*, and its state moves only through
/// `cancel` and (B5.08b) `apply` — a status a client can type is a status a
/// stale form can undo.
#[derive(Deserialize)]
struct NoteBody {
    #[serde(default)]
    note: Option<String>,
}

/// What a counter records against one row. Both fields are optional and both
/// are stated whole: this is a `PUT`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LineBody {
    #[serde(default)]
    counted_qty_milli: Option<i64>,
    #[serde(default)]
    note: Option<String>,
}

/// Loads one of the tenant's counts, or fails with the `404` an id from another
/// tenant gets.
async fn load(acc: &AccountStore, id: &InvCountId) -> Result<Count, Problem> {
    acc.inv_count(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such stocktake"))
}

/// Query string of the stocktake list.
#[derive(Deserialize)]
pub struct CountListQuery {
    /// One place, across every time it has been counted.
    #[serde(default, rename = "locationId")]
    location_id: Option<String>,
    /// `open`, `applied` or `cancelled`.
    #[serde(default)]
    status: Option<String>,
    /// How many at most, newest first. Clamped by the store.
    #[serde(default)]
    limit: Option<i64>,
}

/// `GET /inventory/counts[?locationId&status&limit]` → `{"counts":[…]}` — the
/// stocktakes this tenant has run, newest first.
pub async fn list_counts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<CountListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let status = q
        .status
        .as_deref()
        .map(CountStatus::parse)
        .transpose()
        .map_err(map_store_err)?;
    let filter = CountFilter {
        location_id: q.location_id.map(InvLocationId::new),
        status,
        limit: q.limit,
    };
    let counts = account
        .acc
        .inv_counts(&filter)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "counts": counts.iter().map(count_json).collect::<Vec<_>>(),
    })))
}

/// `POST /inventory/counts` `{locationId, note?}` → `{"count":{…},"lines":[…]}`
/// — opens a count and answers with the sheet it snapshotted, so a phone that
/// starts a stocktake has the rows in one call.
///
/// A `422` on a location that is not a real shelf; a `409` when the place is
/// archived or already has a count open, because two people counting one shelf
/// produce two truths.
pub async fn open_count(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: OpenBody = parse_body(&body)?;
    let location_id = req
        .location_id
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "locationId is required: a stocktake counts one place",
            )
        })?;
    let id = account
        .acc
        .open_inv_count(&NewCount {
            location_id: InvLocationId::new(location_id),
            note: req.note.unwrap_or_default(),
        })
        .await
        .map_err(map_store_err)?;
    sheet(&account.acc, &id).await
}

/// `GET /inventory/counts/{id}` → `{"count":{…},"lines":[…]}` — the sheet, with
/// what is on the shelf now beside what was expected.
pub async fn get_count(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    sheet(&account.acc, &InvCountId::new(id)).await
}

/// `PATCH /inventory/counts/{id}` `{note}` → `{"count":{…},"lines":[…]}`.
///
/// A `409` once the count is closed: a finished sheet is a record of what
/// happened, and records are not edited.
pub async fn update_count(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: NoteBody = parse_body(&body)?;
    let id = InvCountId::new(id);
    // The stored note when the body does not state one, so an empty `PATCH` is
    // a no-op rather than a silent erasure of what somebody wrote.
    let note = match req.note {
        Some(note) => note,
        None => load(&account.acc, &id).await?.note,
    };
    account
        .acc
        .update_inv_count_note(&id, &note)
        .await
        .map_err(map_store_err)?;
    sheet(&account.acc, &id).await
}

/// `PUT /inventory/counts/{id}/lines/{product_id}` `{countedQtyMilli?, note?}` →
/// `{"line":{…},"count":{…}}` — records what was found on one row.
///
/// The count summary rides along because every write to a row changes it, and a
/// phone showing "38 of 51 counted" must not need a second call to stay honest.
///
/// A `422` on a negative or over-bound quantity or a service product; a `404`
/// when the count or the product is not this tenant's; a `409` once the count is
/// closed.
pub async fn set_count_line(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, product_id)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: LineBody = parse_body(&body)?;
    let id = InvCountId::new(id);
    let line = account
        .acc
        .set_inv_count_line(
            &id,
            &BillingProductId::new(product_id),
            &CountEntry {
                counted_qty_milli: req.counted_qty_milli,
                note: req.note.unwrap_or_default(),
            },
        )
        .await
        .map_err(map_store_err)?;
    let count = load(&account.acc, &id).await?;
    Ok(Json(json!({
        "line": line_json(&line),
        "count": count_json(&count),
    })))
}

/// `POST /inventory/counts/{id}/cancel` → `{"count":{…},"lines":[…]}` — walks
/// away from a count. The sheet is kept exactly as it was; the ledger is
/// untouched, and the place is free to be counted again.
pub async fn cancel_count(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = InvCountId::new(id);
    account
        .acc
        .cancel_inv_count(&id)
        .await
        .map_err(map_store_err)?;
    sheet(&account.acc, &id).await
}

/// The count and its sheet, the one answer four of these routes give — a screen
/// that opens, patches, counts or cancels never has to ask twice, and the two
/// halves can never be read from different moments.
async fn sheet(acc: &AccountStore, id: &InvCountId) -> Result<Json<Value>, Problem> {
    let count = load(acc, id).await?;
    let lines = acc.inv_count_sheet(id).await.map_err(map_store_err)?;
    Ok(Json(json!({
        "count": count_json(&count),
        "lines": lines.iter().map(line_json).collect::<Vec<_>>(),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    fn line_body(value: Value) -> LineBody {
        serde_json::from_value(value).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn counted(counted_qty_milli: Option<i64>) -> CountLine {
        CountLine {
            product_id: BillingProductId::new("p1"),
            product_name: "Blue chair".to_owned(),
            sku: "CH-1".to_owned(),
            barcode: "4006381333931".to_owned(),
            unit: "piece".to_owned(),
            expected_qty_milli: 5_000,
            counted_qty_milli,
            variance_qty_milli: alo_store::inv_count::variance_qty_milli(counted_qty_milli, 5_000),
            on_hand_qty_milli: 7_000,
            moved_since: true,
            note: "one broken".to_owned(),
            counted_at: counted_qty_milli.map(|_| OffsetDateTime::UNIX_EPOCH),
            counted_by: counted_qty_milli.map(|_| "u".to_owned()),
        }
    }

    fn stocktake() -> Count {
        Count {
            id: InvCountId::new("c1"),
            location_id: InvLocationId::new("l1"),
            location_code: "MAIN".to_owned(),
            location_name: "Hoofdmagazijn".to_owned(),
            status: CountStatus::Open,
            note: "Tuesday, back shelves".to_owned(),
            line_count: 51,
            counted_count: 38,
            variance_count: 4,
            created_by: "u".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            closed_at: None,
            closed_by: None,
        }
    }

    #[test]
    fn a_row_states_every_number_it_compared() {
        let rendered = line_json(&counted(Some(4_000)));
        assert_eq!(rendered["expectedQtyMilli"], 5_000);
        assert_eq!(rendered["countedQtyMilli"], 4_000);
        assert_eq!(
            rendered["varianceQtyMilli"], -1_000,
            "the client is told the difference, never left to compute it"
        );
        assert_eq!(rendered["onHandQtyMilli"], 7_000);
        assert_eq!(
            rendered["movedSince"], true,
            "the shelf moved under the counter, and the apply will skip this row"
        );
        assert_eq!(rendered["barcode"], "4006381333931");
    }

    #[test]
    fn an_uncounted_row_claims_nothing() {
        let rendered = line_json(&counted(None));
        assert_eq!(
            rendered["countedQtyMilli"],
            Value::Null,
            "'nobody got to this shelf' is not 'there are none left'"
        );
        assert_eq!(rendered["varianceQtyMilli"], Value::Null);
        assert_eq!(rendered["countedAt"], Value::Null);
        assert_eq!(rendered["countedBy"], Value::Null);
    }

    #[test]
    fn a_count_states_its_own_tallies() {
        let rendered = count_json(&stocktake());
        assert_eq!(rendered["status"], "open");
        assert_eq!(rendered["lineCount"], 51);
        assert_eq!(rendered["countedCount"], 38);
        assert_eq!(rendered["varianceCount"], 4);
        assert_eq!(rendered["locationCode"], "MAIN");
        assert_eq!(rendered["closedAt"], Value::Null);
        assert_eq!(rendered["closedBy"], Value::Null);
    }

    #[test]
    fn a_put_with_no_quantity_clears_the_row() {
        // A `PUT` states the row whole, so an absent quantity is "uncounted"
        // rather than "leave it as it was" — the undo of a mis-scan.
        assert_eq!(line_body(json!({})).counted_qty_milli, None);
        assert_eq!(
            line_body(json!({ "countedQtyMilli": null })).counted_qty_milli,
            None
        );
        assert_eq!(
            line_body(json!({ "countedQtyMilli": 0 })).counted_qty_milli,
            Some(0),
            "counting zero is the strongest claim a stocktake makes, and is not a clear"
        );
    }

    #[test]
    fn quantities_are_integers_on_the_wire() {
        // 1.5 units is 1500 milli-units; a client that sends 1.5 gets a 400,
        // not a silently rounded finding.
        assert!(serde_json::from_value::<LineBody>(json!({"countedQtyMilli": 1.5})).is_err());
        assert!(serde_json::from_value::<LineBody>(json!({"countedQtyMilli": "4"})).is_err());
        assert!(serde_json::from_value::<OpenBody>(json!({"locationId": 7})).is_err());
    }

    #[test]
    fn the_status_filter_speaks_the_stores_vocabulary() {
        assert_eq!(CountStatus::parse("open").ok(), Some(CountStatus::Open));
        let refused = CountStatus::parse("closed")
            .err()
            .map(map_store_err)
            .map(|p| p.status);
        assert_eq!(
            refused,
            Some(StatusCode::UNPROCESSABLE_ENTITY),
            "an unknown status is the caller's mistake, not a 500"
        );
    }
}
