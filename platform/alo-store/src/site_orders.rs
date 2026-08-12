//! Orders placed from a published alo Sites catalog — the owner's side of the
//! order form (ADR 0036; the no-checkout wave of ADR 0041).
//!
//! An order is a **request**, not a sale: the visitor names what they want and
//! how to be reached, and the owner confirms, fulfils or cancels it by hand.
//! Nothing here takes payment, reserves stock, or promises availability — a
//! bakery taking Saturday orders needs exactly this, and paid checkout (ADR
//! 0041, wave three) is built on top of this record rather than beside it.
//!
//! Everything is addressed as (site, order) and every statement scopes by
//! tenant AND site, so an order can be reached neither from another tenant nor
//! from another site of the same tenant. The anonymous write door — the one a
//! visitor's POST reaches — lives in [`crate::site_public_orders`] and funnels
//! through the same write gates as this module.
//!
//! Money is integer minor units of the order's own currency, frozen from the
//! published catalog snapshot at the moment the order arrived; the visitor
//! never sends a price, and no float ever touches this path.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{SiteId, SiteOrderId};

/// Cap on the customer's name — the same bound a contact form takes.
pub const ORDER_NAME_MAX_CHARS: usize = 200;
/// Cap on the customer's email address (the SMTP path limit).
pub const ORDER_EMAIL_MAX_CHARS: usize = 254;
/// Cap on the optional phone number. Deliberately loose on shape (an
/// international number is written a dozen ways) and tight on length.
pub const ORDER_PHONE_MAX_CHARS: usize = 40;
/// Cap on the optional note ("no nuts, please", a delivery address).
pub const ORDER_NOTE_MAX_CHARS: usize = 2_000;
/// Most distinct items one order may name. Above any real order form and far
/// below the size at which a request becomes a denial-of-service payload.
pub const ORDER_MAX_LINES: usize = 50;
/// Most of one item a single order may ask for. A visitor ordering a thousand
/// loaves is a typo or an attack, and either way it is the owner's call —
/// they can be phoned, which is what the contact details are for.
pub const ORDER_MAX_QUANTITY: i32 = 999;

/// Where an order stands in the owner's own workflow. There is no payment
/// state here by design: an order is confirmed when the owner says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteOrderStatus {
    /// Arrived, nobody has looked at it yet.
    New,
    /// The owner accepted it.
    Confirmed,
    /// Handed over, delivered, served.
    Fulfilled,
    /// Called off — by the owner or, over the phone, by the customer.
    Cancelled,
}

impl SiteOrderStatus {
    /// The exact string stored in the `status` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SiteOrderStatus::New => "new",
            SiteOrderStatus::Confirmed => "confirmed",
            SiteOrderStatus::Fulfilled => "fulfilled",
            SiteOrderStatus::Cancelled => "cancelled",
        }
    }

    /// Parses a stored or posted status; an unknown value is an error rather
    /// than a silent default.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the value is not one of the four.
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "new" => Ok(SiteOrderStatus::New),
            "confirmed" => Ok(SiteOrderStatus::Confirmed),
            "fulfilled" => Ok(SiteOrderStatus::Fulfilled),
            "cancelled" => Ok(SiteOrderStatus::Cancelled),
            other => Err(StoreError::Validation(format!(
                "{other} is not an order status"
            ))),
        }
    }
}

/// One order as the owner reads it — the header; its lines are read with
/// [`AccountStore::site_order_lines`].
#[derive(Debug, Clone)]
pub struct SiteOrder {
    pub id: SiteOrderId,
    /// The catalog the order came from, and the name it carried then. The
    /// catalog itself may since have been renamed or deleted.
    pub catalog_id: String,
    pub catalog_name: String,
    /// ISO 4217 code the line prices are denominated in.
    pub currency: String,
    pub customer_name: String,
    pub customer_email: String,
    pub customer_phone: Option<String>,
    pub note: Option<String>,
    /// Sum of the priced lines, in minor units of `currency`. Lines whose item
    /// carried no price contribute nothing — they are quoted by hand.
    pub total_cents: i64,
    pub status: SiteOrderStatus,
    pub received_at: OffsetDateTime,
}

/// One requested item, frozen with the order.
#[derive(Debug, Clone)]
pub struct SiteOrderLine {
    pub position: i32,
    /// The published handle of the item, kept so the owner can find it again.
    pub item_slug: String,
    /// The name the visitor saw on the page.
    pub item_name: String,
    pub quantity: i32,
    /// `None` for an item published without a price.
    pub unit_price_cents: Option<i64>,
    /// `unit_price_cents * quantity`, or `None` for an unpriced item.
    pub line_total_cents: Option<i64>,
}

/// The contact fields of an order after normalization — trimmed, non-blank
/// where required, within the caps. The only shape the store will insert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderContact {
    pub customer_name: String,
    pub customer_email: String,
    pub customer_phone: Option<String>,
    pub note: Option<String>,
}

/// One line of an order request as it arrives from the wire: a published item
/// handle and how many of it. No price — prices come from the published
/// snapshot, never from the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderRequestLine {
    pub item_slug: String,
    pub quantity: i32,
}

/// Normalizes an order's contact fields. Name and email are required (an order
/// nobody can answer is worse than no order); phone and note are optional and
/// collapse to `None` when blank.
///
/// The email rule is deliberately the contact form's loose one — one `@` with
/// something either side and no whitespace or control characters — because
/// this is a website form, not an SMTP envelope.
///
/// # Errors
/// [`StoreError::Validation`] naming the violated rule (field-level, safe to
/// show the visitor).
pub fn normalize_order_contact(
    customer_name: &str,
    customer_email: &str,
    customer_phone: &str,
    note: &str,
) -> Result<OrderContact> {
    let customer_name = customer_name.trim();
    if customer_name.is_empty() {
        return Err(StoreError::Validation("name must not be empty".to_owned()));
    }
    if customer_name.chars().count() > ORDER_NAME_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "name must be at most {ORDER_NAME_MAX_CHARS} characters"
        )));
    }
    let customer_email = customer_email.trim();
    if customer_email.is_empty() {
        return Err(StoreError::Validation("email must not be empty".to_owned()));
    }
    if customer_email.chars().count() > ORDER_EMAIL_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "email must be at most {ORDER_EMAIL_MAX_CHARS} characters"
        )));
    }
    let looks_like_address = matches!(
        customer_email.split_once('@'),
        Some((local, domain)) if !local.is_empty() && !domain.is_empty()
    );
    if !looks_like_address
        || customer_email
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(StoreError::Validation(
            "email must be a valid address".to_owned(),
        ));
    }
    let customer_phone = customer_phone.trim();
    if customer_phone.chars().count() > ORDER_PHONE_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "phone must be at most {ORDER_PHONE_MAX_CHARS} characters"
        )));
    }
    let note = note.trim();
    if note.chars().count() > ORDER_NOTE_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "note must be at most {ORDER_NOTE_MAX_CHARS} characters"
        )));
    }
    Ok(OrderContact {
        customer_name: customer_name.to_owned(),
        customer_email: customer_email.to_owned(),
        customer_phone: (!customer_phone.is_empty()).then(|| customer_phone.to_owned()),
        note: (!note.is_empty()).then(|| note.to_owned()),
    })
}

/// Normalizes the requested lines: drops quantity-zero entries (a rendered
/// order form posts one field per item, and most of them are zero), merges a
/// repeated handle into one line rather than refusing an order the visitor
/// cannot fix, and bounds both the number of lines and each quantity.
///
/// Order is preserved: the first time a handle appears decides where its line
/// sits, so the order reads in the sequence of the page it came from.
///
/// # Errors
/// [`StoreError::Validation`] when nothing was ordered, a quantity is negative
/// or over [`ORDER_MAX_QUANTITY`], or there are more than [`ORDER_MAX_LINES`]
/// distinct items.
pub fn normalize_order_lines(requested: &[OrderRequestLine]) -> Result<Vec<OrderRequestLine>> {
    let mut lines: Vec<OrderRequestLine> = Vec::new();
    for line in requested {
        if line.quantity < 0 {
            return Err(StoreError::Validation(
                "a quantity may not be negative".to_owned(),
            ));
        }
        if line.quantity == 0 {
            continue;
        }
        let slug = line.item_slug.trim();
        if slug.is_empty() {
            continue;
        }
        match lines.iter_mut().find(|kept| kept.item_slug == slug) {
            Some(kept) => kept.quantity = kept.quantity.saturating_add(line.quantity),
            None => {
                if lines.len() >= ORDER_MAX_LINES {
                    return Err(StoreError::Validation(format!(
                        "an order may name at most {ORDER_MAX_LINES} different items"
                    )));
                }
                lines.push(OrderRequestLine {
                    item_slug: slug.to_owned(),
                    quantity: line.quantity,
                });
            }
        }
    }
    if lines.is_empty() {
        return Err(StoreError::Validation(
            "choose at least one item before ordering".to_owned(),
        ));
    }
    if let Some(over) = lines.iter().find(|line| line.quantity > ORDER_MAX_QUANTITY) {
        return Err(StoreError::Validation(format!(
            "at most {ORDER_MAX_QUANTITY} of one item can be ordered at a time; \
             call us for {}",
            over.item_slug
        )));
    }
    Ok(lines)
}

impl AccountStore {
    /// The site's orders, newest first. Empty when the site isn't the
    /// tenant's — indistinguishable from a site nobody has ordered from,
    /// by design.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_orders(&self, site: &SiteId) -> Result<Vec<SiteOrder>> {
        let rows = sqlx::query_as::<_, SiteOrderRow>(
            "SELECT id, catalog_id, catalog_name, currency, customer_name, customer_email, \
                    customer_phone, note, total_cents, status, received_at \
             FROM site_orders WHERE tenant_id = $1 AND site_id = $2 \
             ORDER BY received_at DESC, id DESC",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(SiteOrderRow::into_order).collect()
    }

    /// One order of the tenant's site, or `None` — including when it belongs
    /// to another tenant or another site.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Conflict`] on an
    /// unreadable stored status.
    pub async fn site_order(
        &self,
        site: &SiteId,
        order: &SiteOrderId,
    ) -> Result<Option<SiteOrder>> {
        let row = sqlx::query_as::<_, SiteOrderRow>(
            "SELECT id, catalog_id, catalog_name, currency, customer_name, customer_email, \
                    customer_phone, note, total_cents, status, received_at \
             FROM site_orders WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(order.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(SiteOrderRow::into_order).transpose()
    }

    /// The order's lines in the sequence they were requested. Empty for an
    /// order that isn't the tenant's or the site's.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_order_lines(
        &self,
        site: &SiteId,
        order: &SiteOrderId,
    ) -> Result<Vec<SiteOrderLine>> {
        let rows = sqlx::query_as::<_, SiteOrderLineRow>(
            "SELECT l.position, l.item_slug, l.item_name, l.quantity, l.unit_price_cents, \
                    l.line_total_cents \
             FROM site_order_lines l \
             JOIN site_orders o ON o.tenant_id = l.tenant_id AND o.id = l.order_id \
             WHERE l.tenant_id = $1 AND o.site_id = $2 AND l.order_id = $3 \
             ORDER BY l.position",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(order.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(SiteOrderLineRow::into_line).collect())
    }

    /// Every line of every order of the site, each paired with the order it
    /// belongs to, in order-then-position sequence. This is what an inbox
    /// listing and a CSV export read: one query for the whole site rather than
    /// one per order.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_all_order_lines(
        &self,
        site: &SiteId,
    ) -> Result<Vec<(SiteOrderId, SiteOrderLine)>> {
        let rows = sqlx::query_as::<_, SiteOrderLineOfOrderRow>(
            "SELECT l.order_id, l.position, l.item_slug, l.item_name, l.quantity, \
                    l.unit_price_cents, l.line_total_cents \
             FROM site_order_lines l \
             JOIN site_orders o ON o.tenant_id = l.tenant_id AND o.id = l.order_id \
             WHERE l.tenant_id = $1 AND o.site_id = $2 \
             ORDER BY l.order_id, l.position",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(SiteOrderLineOfOrderRow::into_pair)
            .collect())
    }

    /// Moves an order through the owner's workflow. Every transition is
    /// allowed in both directions — a cancelled order that turns out to have
    /// been a misunderstanding is confirmed again, not re-typed.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the order isn't the tenant's or the
    /// site's; [`StoreError::Db`] on failure.
    pub async fn set_site_order_status(
        &self,
        site: &SiteId,
        order: &SiteOrderId,
        status: SiteOrderStatus,
    ) -> Result<()> {
        let done = sqlx::query(
            "UPDATE site_orders SET status = $4 \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(order.as_str())
        .bind(status.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes an order with its lines — spam, a duplicate, or a customer's
    /// data-removal request (an order carries their name and address).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the order isn't the tenant's or the
    /// site's; [`StoreError::Db`] on failure.
    pub async fn delete_site_order(&self, site: &SiteId, order: &SiteOrderId) -> Result<()> {
        let done = sqlx::query(
            "DELETE FROM site_orders WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(order.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct SiteOrderRow {
    id: String,
    catalog_id: String,
    catalog_name: String,
    currency: String,
    customer_name: String,
    customer_email: String,
    customer_phone: Option<String>,
    note: Option<String>,
    total_cents: i64,
    status: String,
    received_at: OffsetDateTime,
}

impl SiteOrderRow {
    fn into_order(self) -> Result<SiteOrder> {
        Ok(SiteOrder {
            id: SiteOrderId::new(self.id),
            catalog_id: self.catalog_id,
            catalog_name: self.catalog_name,
            currency: self.currency,
            customer_name: self.customer_name,
            customer_email: self.customer_email,
            customer_phone: self.customer_phone,
            note: self.note,
            total_cents: self.total_cents,
            status: SiteOrderStatus::parse(&self.status)?,
            received_at: self.received_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct SiteOrderLineRow {
    position: i32,
    item_slug: String,
    item_name: String,
    quantity: i32,
    unit_price_cents: Option<i64>,
    line_total_cents: Option<i64>,
}

impl SiteOrderLineRow {
    fn into_line(self) -> SiteOrderLine {
        SiteOrderLine {
            position: self.position,
            item_slug: self.item_slug,
            item_name: self.item_name,
            quantity: self.quantity,
            unit_price_cents: self.unit_price_cents,
            line_total_cents: self.line_total_cents,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SiteOrderLineOfOrderRow {
    order_id: String,
    position: i32,
    item_slug: String,
    item_name: String,
    quantity: i32,
    unit_price_cents: Option<i64>,
    line_total_cents: Option<i64>,
}

impl SiteOrderLineOfOrderRow {
    fn into_pair(self) -> (SiteOrderId, SiteOrderLine) {
        (
            SiteOrderId::new(self.order_id),
            SiteOrderLine {
                position: self.position,
                item_slug: self.item_slug,
                item_name: self.item_name,
                quantity: self.quantity,
                unit_price_cents: self.unit_price_cents,
                line_total_cents: self.line_total_cents,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn line(slug: &str, quantity: i32) -> OrderRequestLine {
        OrderRequestLine {
            item_slug: slug.to_owned(),
            quantity,
        }
    }

    #[test]
    fn contacts_are_trimmed_and_optional_fields_collapse_to_none() {
        let contact =
            normalize_order_contact("  Ada Lovelace ", " ada@example.test ", "  ", " \n ").unwrap();
        assert_eq!(contact.customer_name, "Ada Lovelace");
        assert_eq!(contact.customer_email, "ada@example.test");
        assert_eq!(contact.customer_phone, None);
        assert_eq!(contact.note, None);

        let with_extras =
            normalize_order_contact("Ada", "ada@example.test", " +32 2 555 01 ", " no nuts ")
                .unwrap();
        assert_eq!(with_extras.customer_phone.as_deref(), Some("+32 2 555 01"));
        assert_eq!(with_extras.note.as_deref(), Some("no nuts"));
    }

    #[test]
    fn contacts_require_a_name_and_a_usable_address() {
        for (name, email) in [
            ("", "ada@example.test"),
            ("   ", "ada@example.test"),
            ("Ada", ""),
            ("Ada", "not-an-email"),
            ("Ada", "@example.test"),
            ("Ada", "ada@"),
            ("Ada", "spa ce@example.test"),
        ] {
            assert!(
                matches!(
                    normalize_order_contact(name, email, "", ""),
                    Err(StoreError::Validation(_))
                ),
                "expected rejection: {name:?} {email:?}"
            );
        }
    }

    #[test]
    fn contacts_bound_every_field() {
        let long = "x".repeat(ORDER_NAME_MAX_CHARS + 1);
        assert!(matches!(
            normalize_order_contact(&long, "a@b.test", "", ""),
            Err(StoreError::Validation(_))
        ));
        assert!(matches!(
            normalize_order_contact("Ada", &format!("a@{}.test", "x".repeat(300)), "", ""),
            Err(StoreError::Validation(_))
        ));
        assert!(matches!(
            normalize_order_contact(
                "Ada",
                "a@b.test",
                &"9".repeat(ORDER_PHONE_MAX_CHARS + 1),
                ""
            ),
            Err(StoreError::Validation(_))
        ));
        assert!(matches!(
            normalize_order_contact("Ada", "a@b.test", "", &"x".repeat(ORDER_NOTE_MAX_CHARS + 1)),
            Err(StoreError::Validation(_))
        ));
        assert!(
            normalize_order_contact("Ada", "a@b.test", "", &"x".repeat(ORDER_NOTE_MAX_CHARS))
                .is_ok()
        );
    }

    #[test]
    fn zero_quantities_drop_out_and_repeats_merge_in_page_order() {
        let lines = normalize_order_lines(&[
            line("sourdough", 2),
            line("croissant", 0),
            line("  ", 3),
            line("sourdough", 1),
            line("focaccia", 1),
        ])
        .unwrap();
        assert_eq!(lines, vec![line("sourdough", 3), line("focaccia", 1)]);
    }

    #[test]
    fn an_empty_order_is_refused_with_a_sentence_a_visitor_can_act_on() {
        let error = normalize_order_lines(&[line("sourdough", 0)]).unwrap_err();
        assert!(format!("{error}").contains("at least one item"), "{error}");
        assert!(matches!(
            normalize_order_lines(&[]),
            Err(StoreError::Validation(_))
        ));
    }

    #[test]
    fn quantities_and_line_counts_are_bounded() {
        assert!(matches!(
            normalize_order_lines(&[line("sourdough", -1)]),
            Err(StoreError::Validation(_))
        ));
        assert!(matches!(
            normalize_order_lines(&[line("sourdough", ORDER_MAX_QUANTITY + 1)]),
            Err(StoreError::Validation(_))
        ));
        // A merge that crosses the ceiling is refused too — the cap is on what
        // is ordered, not on how it was typed.
        assert!(matches!(
            normalize_order_lines(&[line("a", ORDER_MAX_QUANTITY), line("a", 1)]),
            Err(StoreError::Validation(_))
        ));
        let many: Vec<OrderRequestLine> = (0..=ORDER_MAX_LINES)
            .map(|n| line(&format!("item-{n}"), 1))
            .collect();
        assert!(matches!(
            normalize_order_lines(&many),
            Err(StoreError::Validation(_))
        ));
        assert!(normalize_order_lines(&many[..ORDER_MAX_LINES]).is_ok());
    }

    #[test]
    fn statuses_round_trip_and_reject_nonsense() {
        for status in [
            SiteOrderStatus::New,
            SiteOrderStatus::Confirmed,
            SiteOrderStatus::Fulfilled,
            SiteOrderStatus::Cancelled,
        ] {
            assert_eq!(SiteOrderStatus::parse(status.as_str()).unwrap(), status);
        }
        assert!(matches!(
            SiteOrderStatus::parse("paid"),
            Err(StoreError::Validation(_))
        ));
    }
}
