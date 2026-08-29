//! The executors of alo Sites' verbs (ADR 0058, AC.5) — what runs when the
//! Website agent uses one of the intents `alo_ai::sites_intents` describes.
//!
//! Every executor runs through the asker's account door. The seven verbs the
//! old tool set already had keep their executors in [`crate::agent_sites`] —
//! the grounded answer, the editing pair and the publish are that module's
//! subject matter — and are dispatched from here so the agent has one place
//! to look. What this module itself executes is the site as a *business*
//! subject: the page list, where the site stands on the internet, the order
//! inbox, and the services offered for booking.
//!
//! Three seams are deliberate reuse rather than new reach:
//!
//! - **The site and its pages read as the routes' own records.**
//!   [`crate::agent_sites::site_ref`] and [`crate::sites::page_json`] are the
//!   serializers the Sites screens are fed from, so what the agent says about
//!   a page and what the screen shows cannot disagree.
//! - **The order inbox is the orders screen's own two reads.**
//!   `site_orders` + `site_all_order_lines`, exactly as `GET
//!   /sites/:id/orders` reads them — and every amount is repeated in minor
//!   units with a readable amount beside it, never recomputed.
//! - **A site is named, never identified.** Every verb resolves through
//!   [`crate::agent_sites::resolve_site`], which picks out of the caller's
//!   own `sites()` — a site of another tenant is not merely refused, it is
//!   not among the things that can be named.

use serde_json::{Value, json};

use alo_store::{SiteOrder, SiteOrderStatus, SiteStatus};

use crate::agent_sites::{resolve_site, site_ref};
use crate::billing::map_store_err;
use crate::error::Problem;
use crate::sites::page_json;
use crate::state::{Account, AppState};

/// How many of the newest orders the summary lists in full. The counts cover
/// the whole inbox; past this the tail is named by count so the model knows
/// it is not looking at everything.
const MAX_ORDERS_LISTED: usize = 8;

/// How many publishes the status lists. "When did we last publish" needs the
/// newest few, not the whole history the history screen pages through.
const MAX_PUBLISHES_LISTED: i64 = 5;

type Reply = Result<axum::Json<Value>, Problem>;

fn ok(result: Value) -> Reply {
    Ok(axum::Json(json!({ "ok": true, "result": result })))
}

/// `24900` in EUR as `249.00 EUR` — the readable amount beside the integer,
/// so the model repeats money instead of doing arithmetic on it.
fn money(cents: i64, currency: &str) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let magnitude = cents.unsigned_abs();
    format!(
        "{sign}{}.{:02} {currency}",
        magnitude / 100,
        magnitude % 100
    )
}

/// `site_pages` — the map of the site: every page of the draft, in
/// navigation order, as the page list route renders them (without their
/// sections — reading ONE page's text is `site_page_read`'s job, and twelve
/// pages of sections would drown the turn).
///
/// # Errors
/// 422 when no site of the tenant's matches the argument; the store's own
/// failure otherwise.
pub async fn execute_site_pages(account: &Account, args: &Value) -> Reply {
    let site = resolve_site(account, args).await?;
    let pages = account
        .acc
        .site_pages(&site.id)
        .await
        .map_err(map_store_err)?;
    ok(json!({
        "kind": "sitePages",
        "site": site_ref(&site),
        "total": pages.len(),
        "pages": pages.iter().map(|page| page_json(page, false)).collect::<Vec<_>>(),
    }))
}

/// `site_status` — where the website stands: live or draft, its address, how
/// many pages the draft holds, and the newest publishes with who made each
/// and when.
///
/// # Errors
/// 422 when no site of the tenant's matches the argument; the store's own
/// failure otherwise.
pub async fn execute_site_status(account: &Account, args: &Value) -> Reply {
    let site = resolve_site(account, args).await?;
    let pages = account
        .acc
        .site_pages(&site.id)
        .await
        .map_err(map_store_err)?;
    let history = account
        .acc
        .site_publish_history(&site.id, MAX_PUBLISHES_LISTED)
        .await
        .map_err(map_store_err)?;
    let last_published_at = history
        .first()
        .map(|publish| crate::sites::iso(publish.published_at));
    ok(json!({
        "kind": "siteStatus",
        "site": site_ref(&site),
        // Said plainly beside the raw status, because "is the site live" is
        // the question this verb exists to answer.
        "live": site.status == SiteStatus::Live,
        "draftPages": pages.len(),
        "lastPublishedAt": last_published_at,
        "publishes": history.iter().map(|publish| json!({
            "id": publish.id.as_str(),
            "publishedAt": crate::sites::iso(publish.published_at),
            "pages": publish.pages,
            "locales": publish.locales,
            "current": publish.is_current,
            "restored": publish.restored_from.is_some(),
        })).collect::<Vec<_>>(),
    }))
}

/// The whole inbox counted by status — the orders screen's four columns as
/// four numbers. Split out so the counting is unit-tested without a store.
fn order_counts(orders: &[SiteOrder]) -> Value {
    let of = |status: SiteOrderStatus| orders.iter().filter(|o| o.status == status).count();
    json!({
        "new": of(SiteOrderStatus::New),
        "confirmed": of(SiteOrderStatus::Confirmed),
        "fulfilled": of(SiteOrderStatus::Fulfilled),
        "cancelled": of(SiteOrderStatus::Cancelled),
    })
}

/// One order as the summary lists it: who, what it comes to (integer minor
/// units with the readable amount beside), where it stands, and when it
/// arrived. Never the customer's phone or note — the summary answers "did
/// any orders come in", and the person answering a customer opens the orders
/// screen, where the whole order is.
fn order_line(order: &SiteOrder, line_count: usize) -> Value {
    json!({
        "customer": order.customer_name,
        "status": order.status.as_str(),
        "totalCents": order.total_cents,
        "total": money(order.total_cents, &order.currency),
        "currency": order.currency,
        "lines": line_count,
        "receivedAt": crate::sites::iso(order.received_at),
    })
}

/// `site_orders` — the order inbox: counts by status over the whole inbox,
/// and the newest orders in full. Orders come from the store newest first,
/// which is the order a "did anything come in" answer wants.
///
/// # Errors
/// 422 when no site of the tenant's matches the argument; the store's own
/// failure otherwise.
pub async fn execute_site_orders(account: &Account, args: &Value) -> Reply {
    let site = resolve_site(account, args).await?;
    let orders = account
        .acc
        .site_orders(&site.id)
        .await
        .map_err(map_store_err)?;
    let lines = account
        .acc
        .site_all_order_lines(&site.id)
        .await
        .map_err(map_store_err)?;
    let listed: Vec<Value> = orders
        .iter()
        .take(MAX_ORDERS_LISTED)
        .map(|order| {
            let count = lines
                .iter()
                .filter(|(id, _)| id.as_str() == order.id.as_str())
                .count();
            order_line(order, count)
        })
        .collect();
    ok(json!({
        "kind": "siteOrders",
        "site": site_ref(&site),
        "total": orders.len(),
        "byStatus": order_counts(&orders),
        "orders": listed,
        "truncated": orders.len() > MAX_ORDERS_LISTED,
    }))
}

/// `site_bookings` — what the website offers visitors to book: each service
/// with its duration, where it happens, how many weekly windows it is
/// offered in, and whether it is taking bookings right now. The appointments
/// themselves land in the owner's calendar, which is the Agenda's to read.
///
/// # Errors
/// 422 when no site of the tenant's matches the argument; the store's own
/// failure otherwise.
pub async fn execute_site_bookings(account: &Account, args: &Value) -> Reply {
    let site = resolve_site(account, args).await?;
    let services = account
        .acc
        .site_bookings(&site.id)
        .await
        .map_err(map_store_err)?;
    let active = services.iter().filter(|service| service.active).count();
    ok(json!({
        "kind": "siteBookings",
        "site": site_ref(&site),
        "total": services.len(),
        "active": active,
        "services": services.iter().map(|service| json!({
            "name": service.name,
            "description": service.description,
            "durationMinutes": service.duration_minutes,
            "location": service.location,
            "weeklyWindows": service.hours.len(),
            "active": service.active,
        })).collect::<Vec<_>>(),
    }))
}

/// The module's verbs by name (A4.1c) — Sites' one row in the agent's
/// dispatcher list, `crate::agent::MODULES`. `None` is "not mine": the
/// dispatcher then asks the next module. The seven verbs the old tool set
/// already had keep their executors in [`crate::agent_sites`].
pub(crate) fn dispatch<'a>(
    _state: &'a AppState,
    account: &'a Account,
    tool: &'a str,
    args: &'a Value,
) -> Option<crate::agent::Dispatched<'a>> {
    let run: crate::agent::Dispatched<'a> = match tool {
        "site_answer" => Box::pin(crate::agent_sites::execute_site_answer(account, args)),
        "site_pages" => Box::pin(execute_site_pages(account, args)),
        "site_status" => Box::pin(execute_site_status(account, args)),
        "site_orders" => Box::pin(execute_site_orders(account, args)),
        "site_bookings" => Box::pin(execute_site_bookings(account, args)),
        "site_page_read" => Box::pin(crate::agent_sites::execute_site_page_read(account, args)),
        "site_seo_review" => Box::pin(crate::agent_sites::execute_site_seo_review(account, args)),
        "site_translation_status" => Box::pin(crate::agent_sites::execute_site_translation_status(
            account, args,
        )),
        "site_page_draft" => Box::pin(crate::agent_sites::execute_site_page_draft(account, args)),
        "site_page_edit" => Box::pin(crate::agent_sites::execute_site_page_edit(account, args)),
        "site_publish" => Box::pin(crate::agent_sites::execute_site_publish(account, args)),
        _ => return None,
    };
    Some(run)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use alo_store::SiteOrderId;
    use time::OffsetDateTime;

    use alo_ai::sites_intents::SITES;

    /// Every `/sites` route the router registers is the adapter of a verb or
    /// excluded with a reason — the coverage ADR 0058 makes structural, over
    /// the largest route surface of any module.
    #[test]
    fn every_sites_route_is_a_verb_or_an_exclusion() {
        let router = include_str!("server.rs");
        let missing = SITES.uncovered(router, "/sites");
        assert!(
            missing.is_empty(),
            "routes with neither a verb nor a reason: {missing:?}"
        );
        // …and every verb's route exists, so an intent cannot name a route
        // the app does not have.
        let routes = alo_ai::routes_in(router, "/sites");
        for intent in SITES.intents {
            for route in intent.routes {
                assert!(
                    routes.contains(&(*route).to_owned()),
                    "{}: {route} is not a route",
                    intent.name
                );
            }
        }
        // An exclusion for a route the router no longer registers is stale
        // documentation wearing a test's clothes.
        for excluded in SITES.excluded {
            assert!(
                routes.contains(&excluded.route.to_owned()),
                "{} is excluded but is not a route",
                excluded.route
            );
        }
    }

    #[test]
    fn every_verb_the_registry_offers_is_dispatched() {
        let dispatch = include_str!("sites_intents.rs");
        for intent in SITES.intents {
            assert!(
                dispatch.contains(&format!("\"{}\" =>", intent.name)),
                "{} has no executor in the dispatch",
                intent.name
            );
        }
    }

    /// Sites' registration is one row in each list (A4.1c): the agent's
    /// dispatcher names this module once, and the two lists are the same
    /// length — every moved module has its dispatcher.
    #[test]
    fn the_module_is_one_row_in_each_list() {
        let agent = include_str!("agent.rs");
        assert_eq!(
            agent.matches("sites_intents::").count(),
            1,
            "agent.rs names Sites only in MODULES"
        );
        assert!(agent.contains("crate::sites_intents::dispatch"));
        assert_eq!(
            crate::agent::MODULES.len(),
            alo_ai::MOVED.len(),
            "a moved module without a dispatcher, or the reverse"
        );
    }

    /// The rule the whole module was named after, held structurally across
    /// the move: the ONLY call to `publish_site` an agent can reach is the
    /// declared write's executor, in `agent_sites` — nothing in this file
    /// publishes, and nothing here writes at all.
    #[test]
    fn nothing_in_this_module_publishes_or_writes() {
        let source = include_str!("sites_intents.rs");
        // The needles are assembled at runtime so this test's own literals do
        // not match themselves in the included source.
        for write in [
            "publish_site",
            "create_site_page",
            "set_page_sections",
            "set_site_order_status",
        ] {
            assert!(
                !source.contains(&format!("{write}(")),
                "{write} is a write and this module only reads"
            );
        }
    }

    fn order(name: &str, status: SiteOrderStatus, cents: i64, currency: &str) -> SiteOrder {
        SiteOrder {
            id: SiteOrderId::new(format!("order-{name}")),
            catalog_id: "cat-1".to_owned(),
            catalog_name: "Saturday bread".to_owned(),
            currency: currency.to_owned(),
            customer_name: name.to_owned(),
            customer_email: format!("{name}@example.test"),
            customer_phone: Some("+32 470 00 00 00".to_owned()),
            note: Some("no nuts, please".to_owned()),
            total_cents: cents,
            status,
            received_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    /// The counts cover the whole inbox, status by status — the four columns
    /// of the orders screen as four numbers.
    #[test]
    fn the_inbox_is_counted_by_status() {
        let orders = vec![
            order("An", SiteOrderStatus::New, 2400, "EUR"),
            order("Ben", SiteOrderStatus::New, 900, "EUR"),
            order("Cas", SiteOrderStatus::Confirmed, 4550, "EUR"),
            order("Dre", SiteOrderStatus::Cancelled, 100, "EUR"),
        ];
        assert_eq!(
            order_counts(&orders),
            json!({ "new": 2, "confirmed": 1, "fulfilled": 0, "cancelled": 1 })
        );
        assert_eq!(
            order_counts(&[]),
            json!({ "new": 0, "confirmed": 0, "fulfilled": 0, "cancelled": 0 })
        );
    }

    /// An order line carries the figure twice — the integer exactly, and the
    /// readable amount beside it in the order's own currency — and never the
    /// customer's phone or note, which belong on the orders screen.
    #[test]
    fn money_is_repeated_readably_and_the_customers_details_stay_home() {
        let line = order_line(&order("An", SiteOrderStatus::New, 2400, "EUR"), 3);
        assert_eq!(line["totalCents"], json!(2400));
        assert_eq!(line["total"], json!("24.00 EUR"));
        assert_eq!(line["lines"], json!(3));
        assert_eq!(line["status"], json!("new"));
        let rendered = line.to_string();
        assert!(
            !rendered.contains("+32 470"),
            "the phone leaked into the summary: {rendered}"
        );
        assert!(
            !rendered.contains("no nuts"),
            "the note leaked into the summary: {rendered}"
        );
        assert!(
            !rendered.contains("example.test"),
            "the email leaked into the summary: {rendered}"
        );
        // The formatting the model must never be tempted to compute.
        assert_eq!(money(0, "EUR"), "0.00 EUR");
        assert_eq!(money(5, "EUR"), "0.05 EUR");
        assert_eq!(money(-1250, "SEK"), "-12.50 SEK");
    }
}
