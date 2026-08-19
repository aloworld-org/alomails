//! The order book over HTTP (alo Orders, item O1.d) — one read that answers
//! what has been promised, what has gone out, what has been billed and what is
//! still owed, per order and in total.
//!
//! Its own module rather than another handler in [`crate::inventory_so`],
//! because it is a different thing: that one is the life of one document, this
//! one is the question a manufacturer opens in the morning across all of them.
//!
//! **Every figure is the store's** ([`alo_store::inv_so_book`]) and none is
//! computed here. A screen that added up its own rows would eventually disagree
//! with the server about what a business is owed, and the reader would have no
//! way to tell which was right.
//!
//! `currencies` is emitted beside `totals` deliberately: a tenant who quotes in
//! two currencies has a total that means nothing, and the client needs to be
//! told that rather than left to discover it by printing one.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::inv_so_book::{BookFigures, BookScope, OrderBook, OrderBookRow};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The five numbers, in money and — where goods can actually move — in
/// quantity.
fn figures_json(f: &BookFigures) -> Value {
    json!({
        "orderedNetCents": f.ordered_net_cents,
        "reservedNetCents": f.reserved_net_cents,
        "deliveredNetCents": f.delivered_net_cents,
        "invoicedNetCents": f.invoiced_net_cents,
        "outstandingNetCents": f.outstanding_net_cents,
        "orderedQtyMilli": f.ordered_qty_milli,
        "reservedQtyMilli": f.reserved_qty_milli,
        "deliveredQtyMilli": f.delivered_qty_milli,
        "outstandingQtyMilli": f.outstanding_qty_milli,
    })
}

/// One line of the book: which order, for whom, and its figures.
fn row_json(row: &OrderBookRow) -> Value {
    json!({
        "id": row.id.as_str(),
        "number": row.number,
        "customerId": row.customer_id.as_str(),
        "customerName": row.customer_name,
        "status": row.status.as_str(),
        "currency": row.currency,
        "figures": figures_json(&row.figures),
    })
}

fn book_json(book: &OrderBook, scope: BookScope) -> Value {
    json!({
        "scope": match scope {
            BookScope::Open => "open",
            BookScope::All => "all",
        },
        "orders": book.rows.iter().map(row_json).collect::<Vec<_>>(),
        "totals": figures_json(&book.totals),
        "currencies": book.currencies,
    })
}

/// `?scope=open|all` — public only because it is an axum extractor on a
/// public handler; nothing else reads it.
#[derive(Deserialize)]
pub struct BookQuery {
    #[serde(default)]
    scope: Option<String>,
}

/// `GET /inventory/order-book?scope=open` → `{"orders":[…],"totals":{…}}`.
///
/// Defaults to the **open** orders — confirmed and partly delivered — because a
/// book cluttered with finished business answers a question nobody asked.
/// `scope=all` includes drafts and closed orders for the reader looking for one
/// in particular.
///
/// An unrecognised scope is a `422` naming what is allowed, the same strictness
/// the sales-order list applies to `?status=`: a filter silently ignored is a
/// screen showing more than the reader asked for and believing it asked.
pub async fn get_order_book(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<BookQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let scope = parse_scope(query.scope.as_deref())?;
    let book = account
        .acc
        .inv_order_book(scope)
        .await
        .map_err(crate::billing::map_store_err)?;
    Ok(Json(book_json(&book, scope)))
}

/// Reads the scope, refusing anything that is not one of the two.
///
/// Absent or blank means the default, and the comparison is case-insensitive —
/// the same shape [`crate::inventory_so`]'s `?status=` filter uses, because two
/// list surfaces in one module that treated their filters differently would be a
/// thing for a client to learn for no reason.
fn parse_scope(stated: Option<&str>) -> Result<BookScope, Problem> {
    let Some(raw) = stated.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(BookScope::Open);
    };
    match raw.to_ascii_lowercase().as_str() {
        "open" => Ok(BookScope::Open),
        "all" => Ok(BookScope::All),
        _ => Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "scope must be one of open, all",
        )),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn the_scope_defaults_to_open_and_is_strict_about_the_rest() {
        assert_eq!(parse_scope(None).unwrap(), BookScope::Open);
        assert_eq!(parse_scope(Some("")).unwrap(), BookScope::Open);
        assert_eq!(parse_scope(Some(" open ")).unwrap(), BookScope::Open);
        assert_eq!(parse_scope(Some("all")).unwrap(), BookScope::All);
        // Case-insensitive, like the sales-order list's own filter.
        assert_eq!(parse_scope(Some("OPEN")).unwrap(), BookScope::Open);
        assert_eq!(parse_scope(Some("All")).unwrap(), BookScope::All);
        // Not silently ignored: a filter that does nothing is a screen showing
        // more than the reader asked for while believing it asked.
        assert!(parse_scope(Some("closed")).is_err());
        assert!(parse_scope(Some("everything")).is_err());
    }

    #[test]
    fn the_body_carries_every_figure_the_screen_names() {
        let figures = BookFigures {
            ordered_net_cents: 100,
            reserved_net_cents: 60,
            delivered_net_cents: 40,
            invoiced_net_cents: 25,
            outstanding_net_cents: 60,
            ordered_qty_milli: 6_000,
            reserved_qty_milli: 4_000,
            delivered_qty_milli: 2_000,
            outstanding_qty_milli: 4_000,
        };
        let json = figures_json(&figures);
        for key in [
            "orderedNetCents",
            "reservedNetCents",
            "deliveredNetCents",
            "invoicedNetCents",
            "outstandingNetCents",
            "orderedQtyMilli",
            "reservedQtyMilli",
            "deliveredQtyMilli",
            "outstandingQtyMilli",
        ] {
            assert!(json.get(key).is_some(), "{key} must be on the wire");
        }
        assert_eq!(json["outstandingNetCents"], 60);
    }

    #[test]
    fn an_empty_book_still_answers_with_a_shape_rather_than_nothing() {
        // A tenant with no orders must get zeros and an empty list, not a null
        // a screen has to special-case.
        let empty = OrderBook {
            rows: Vec::new(),
            totals: BookFigures::default(),
            currencies: Vec::new(),
        };
        let json = book_json(&empty, BookScope::Open);
        assert_eq!(json["scope"], "open");
        assert_eq!(json["orders"].as_array().map(Vec::len), Some(0));
        assert_eq!(json["totals"]["orderedNetCents"], 0);
        assert_eq!(json["currencies"].as_array().map(Vec::len), Some(0));
    }
}
