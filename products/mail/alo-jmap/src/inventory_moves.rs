//! The move ledger over HTTP (alo Inventory, ADR 0035, wave B5.04b) — reading
//! what moved, and the one door that writes a movement **by hand**
//! ([`alo_store::inv_adjust`]).
//!
//! `POST /inventory/moves` is the most carefully guarded write in the business
//! modules, and the reason is stated once in `docs/design/inventory.md`: a stock
//! adjustment is the write that can make theft look like paperwork. So it is
//! also the route that brings `inventory` into
//! [`crate::audit_action::AUDITED_MODULES`] — "who adjusted this stock down by
//! forty, and when" is exactly the question the audit trail exists for, and from
//! this item on `tests/audit_routes.rs` holds every mutating `/inventory/*`
//! route to it.
//!
//! Three things this layer does **not** do, all deliberate:
//!
//! - **It does not correct.** There is no `PATCH` and no `DELETE` here, and
//!   there never will be: a movement recorded in error is corrected by a
//!   movement in the other direction with a reason code and a note. What
//!   happened, happened.
//! - **It does not restate a rule.** Which reasons a person may pick, which
//!   locations they may name, and whether the goods are actually there are the
//!   store's, and their refusals arrive on the wire through
//!   [`crate::billing::map_store_err`] unedited — a `422` that names the rule,
//!   or a `409` that names the product, the place, what is available and what
//!   was asked for.
//! - **It does not log what a person typed.** The note on an adjustment names
//!   people and incidents; it crosses this layer into the store and appears in
//!   no span, exactly as a message body has not since Phase 1.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::inv_adjust::{AdjustReason, NewManualMove};
use alo_store::inv_moves::{MOVES_PAGE_MAX, Move, MoveFilter, MoveReason};
use alo_store::{BillingProductId, InvLocationId, InvMoveId};

use crate::billing::{iso, map_store_err, parse_body, parse_rfc3339};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// One movement as JSON, with the names of what moved and where — an id is not
/// an explanation, and this feed is read by a person.
fn move_json(m: &Move) -> Value {
    json!({
        "id": m.id.as_str(),
        "productId": m.product_id.as_str(),
        "productName": m.product_name,
        "fromLocationId": m.from_location_id.as_str(),
        "fromCode": m.from_code,
        "fromName": m.from_name,
        "toLocationId": m.to_location_id.as_str(),
        "toCode": m.to_code,
        "toName": m.to_name,
        "qtyMilli": m.qty_milli,
        "reason": m.reason.as_str(),
        "reasonCode": m.reason_code.map(AdjustReason::as_str),
        "note": m.note,
        "refKind": m.reference.as_ref().map(|r| r.kind.as_str()),
        "refId": m.reference.as_ref().map(|r| r.id.clone()),
        "occurredAt": iso(m.occurred_at),
        "createdBy": m.created_by,
        "createdAt": iso(m.created_at),
    })
}

/// The ledger read's query string. Every field narrows; all of them absent is
/// the tenant's whole history, newest first, capped at [`MOVES_PAGE_MAX`].
#[derive(Deserialize)]
pub struct MovesQuery {
    /// One product's history.
    #[serde(default, rename = "productId")]
    product_id: Option<String>,
    /// Everything that touched one location, in either direction.
    #[serde(default, rename = "locationId")]
    location_id: Option<String>,
    /// Movements that happened at or after this instant (RFC 3339).
    #[serde(default)]
    from: Option<String>,
    /// Movements that happened at or before this instant (RFC 3339).
    #[serde(default)]
    to: Option<String>,
    /// How many rows at most. Clamped by the store to [`MOVES_PAGE_MAX`].
    #[serde(default)]
    limit: Option<i64>,
}

/// Reads one of the two instants, refusing text that is not one.
///
/// A bad filter is the caller's `422` rather than a silently ignored word: a
/// history page that quietly answers "everything" when it was asked for "since
/// Monday" is worse than one that says the date was unreadable.
fn instant(name: &str, raw: Option<&str>) -> Result<Option<time::OffsetDateTime>, Problem> {
    match raw.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(None),
        Some(value) => parse_rfc3339(value).map(Some).ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("{name} must be an RFC 3339 timestamp"),
            )
        }),
    }
}

/// `GET /inventory/moves[?productId&locationId&from&to&limit]` →
/// `{"moves":[…]}` — the ledger, newest first.
///
/// A location filter matches **either end**: "what happened at this warehouse"
/// is one question, not two.
pub async fn list_moves(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<MovesQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let filter = MoveFilter {
        product_id: q.product_id.map(BillingProductId::new),
        location_id: q.location_id.map(InvLocationId::new),
        from: instant("from", q.from.as_deref())?,
        to: instant("to", q.to.as_deref())?,
        limit: q.limit,
    };
    let moves = account
        .acc
        .inv_moves(&filter)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "moves": moves.iter().map(move_json).collect::<Vec<_>>(),
        "limit": filter.limit.unwrap_or(MOVES_PAGE_MAX).clamp(0, MOVES_PAGE_MAX),
    })))
}

/// The body of a manual movement.
///
/// Every field is stated: there is no sensible default for what moved, from
/// where, to where or how much, and a movement written from a half-filled form
/// is the one kind of row this ledger must never contain. `reason` defaults to
/// `transfer` — the movement that needs no explanation beyond the two places it
/// names.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveBody {
    product_id: String,
    from_location_id: String,
    to_location_id: String,
    qty_milli: i64,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    reason_code: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    occurred_at: Option<String>,
}

impl MoveBody {
    /// Turns the body into the store's shape, reading the two closed
    /// vocabularies and the optional instant. Every refusal here is a `422`
    /// that names what was expected.
    fn into_input(self) -> Result<NewManualMove, Problem> {
        let reason = MoveReason::parse(self.reason.as_deref().unwrap_or("transfer"))
            .map_err(map_store_err)?;
        let reason_code = match self
            .reason_code
            .as_deref()
            .map(str::trim)
            .filter(|code| !code.is_empty())
        {
            Some(code) => Some(AdjustReason::parse(code).map_err(map_store_err)?),
            None => None,
        };
        let occurred_at = match self.occurred_at.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(stated) => Some(parse_rfc3339(stated).ok_or_else(|| {
                Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "occurredAt must be an RFC 3339 timestamp",
                )
            })?),
        };
        Ok(NewManualMove {
            product_id: BillingProductId::new(self.product_id),
            from_location_id: InvLocationId::new(self.from_location_id),
            to_location_id: InvLocationId::new(self.to_location_id),
            qty_milli: self.qty_milli,
            reason,
            reason_code,
            note: self.note.unwrap_or_default(),
            occurred_at,
        })
    }
}

/// `POST /inventory/moves` `{productId, fromLocationId, toLocationId, qtyMilli,
/// reason, reasonCode?, note?, occurredAt?}` → `{"move":{…}}`.
///
/// The two jobs it serves are the same operation: a **transfer** between two of
/// the tenant's own places, and an **adjustment** against the adjustment
/// location — a loss out of stock or a surplus into it, carrying one of the
/// seven reason codes.
///
/// Every other movement in the system is a consequence of a document, and this
/// route refuses to pretend otherwise: a reason that names one, or an end that
/// is the `supplier` or `customer` counterparty, is a `422`.
pub async fn create_move(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: MoveBody = parse_body(&body)?;
    let input = req.into_input()?;
    let id = account
        .acc
        .record_manual_move(&input)
        .await
        .map_err(map_store_err)?;
    let recorded = load(&account.acc, &id).await?;
    Ok(Json(json!({ "move": move_json(&recorded) })))
}

/// Reads back a movement just written. Its absence would mean the ledger lost a
/// row it had committed, which is a `500` and not a `404`.
async fn load(acc: &alo_store::AccountStore, id: &InvMoveId) -> Result<Move, Problem> {
    acc.inv_move(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::INTERNAL_SERVER_ERROR, "the movement vanished"))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use alo_store::inv_adjust::ADJUST_REASONS;

    fn body(json: Value) -> Result<NewManualMove, Problem> {
        serde_json::from_value::<MoveBody>(json)
            .unwrap_or_else(|e| panic!("body rejected: {e}"))
            .into_input()
    }

    fn ok(json: Value) -> NewManualMove {
        body(json).unwrap_or_else(|e| panic!("rejected: {e:?}"))
    }

    fn full() -> Value {
        json!({
            "productId": "p1",
            "fromLocationId": "l1",
            "toLocationId": "l2",
            "qtyMilli": 2_000,
        })
    }

    #[test]
    fn a_movement_with_no_reason_is_a_transfer() {
        let input = ok(full());
        assert_eq!(input.reason, MoveReason::Transfer);
        assert!(input.reason_code.is_none());
        assert_eq!(input.qty_milli, 2_000);
        assert!(input.note.is_empty());
        assert!(
            input.occurred_at.is_none(),
            "absent means now, which the store stamps — not a midnight this layer invents"
        );
    }

    #[test]
    fn an_adjustment_carries_its_code_through() {
        let mut request = full();
        request["reason"] = json!("adjustment");
        request["reasonCode"] = json!("damaged");
        request["note"] = json!("Two chairs crushed by the forklift");
        let input = ok(request);
        assert_eq!(input.reason, MoveReason::Adjustment);
        assert_eq!(input.reason_code, Some(AdjustReason::Damaged));
        assert_eq!(input.note, "Two chairs crushed by the forklift");
    }

    #[test]
    fn a_blank_code_is_an_absent_one_and_an_unknown_one_is_a_422() {
        // A cleared picker sends "", and it means "no code" rather than "the
        // code is the empty string" — the refusal a person then gets is the
        // store's, naming the seven words.
        let mut cleared = full();
        cleared["reasonCode"] = json!("  ");
        assert!(ok(cleared).reason_code.is_none());

        for bad in ["shrinkage", "LOST", "stolen"] {
            let mut request = full();
            request["reason"] = json!("adjustment");
            request["reasonCode"] = json!(bad);
            let refused = body(request).expect_err("an unknown code must not be dropped");
            assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
            let detail = refused.detail.unwrap_or_default();
            for code in ADJUST_REASONS {
                assert!(detail.contains(code.as_str()), "{detail} omits {code:?}");
            }
        }
    }

    #[test]
    fn an_unknown_reason_is_refused_rather_than_defaulted() {
        let mut request = full();
        request["reason"] = json!("shrinkage");
        let refused = body(request).expect_err("an unknown reason must not become `transfer`");
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn a_back_dated_movement_states_a_moment_and_not_a_day() {
        let mut dated = full();
        dated["occurredAt"] = json!("2026-08-07T14:05:00+02:00");
        let input = ok(dated);
        assert_eq!(
            input.occurred_at.map(|t| t.hour()),
            Some(12),
            "normalised to UTC, as every other instant on this service is"
        );

        // A bare day states no moment at all, and a silent midnight in
        // whichever zone the server runs in is not an answer.
        let mut day = full();
        day["occurredAt"] = json!("2026-08-07");
        assert_eq!(
            body(day).expect_err("a bare day is not an instant").status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn the_four_facts_of_a_movement_are_required() {
        // No default is sensible for any of them, and a movement written from a
        // half-filled form is the one row this ledger must never contain.
        for missing in ["productId", "fromLocationId", "toLocationId", "qtyMilli"] {
            let mut request = full();
            let object = request.as_object_mut().unwrap_or_else(|| unreachable!());
            object.remove(missing);
            assert!(
                serde_json::from_value::<MoveBody>(request).is_err(),
                "{missing} must be required"
            );
        }
        // And a quantity is a whole number of milli-units, never a float.
        let mut fractional = full();
        fractional["qtyMilli"] = json!(1.5);
        assert!(serde_json::from_value::<MoveBody>(fractional).is_err());
    }

    #[test]
    fn the_from_filter_refuses_text_that_is_not_an_instant() {
        assert!(instant("from", None).unwrap_or(None).is_none());
        assert!(instant("from", Some("  ")).unwrap_or(None).is_none());
        assert!(
            instant("from", Some("2026-08-07T00:00:00Z"))
                .unwrap_or(None)
                .is_some()
        );
        assert_eq!(
            instant("from", Some("last tuesday"))
                .expect_err("not an instant")
                .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}
