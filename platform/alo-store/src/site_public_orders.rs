//! The public **order write** of alo Sites: a visitor's order request arriving
//! through the anonymous `POST /o/:catalog_id` endpoint on `alo-sites`.
//!
//! The endpoint holds only the bare catalog id a rendered order form carries.
//! This module resolves it to the **currently published snapshot** of that
//! catalog, and everything the order records about what was bought — the item
//! names, the unit prices, the currency — is read from that snapshot. The
//! visitor sends handles and quantities and nothing else: a posted price is
//! unrepresentable, so a page rewritten in a browser cannot buy a loaf for a
//! cent. The tenant is likewise never named from outside; it comes out of the
//! resolving read and is what the insert scopes itself to.
//!
//! An order is accepted only while the catalog is part of a **live** site's
//! current publish and was published with ordering switched on. An unknown id,
//! a catalog on a draft site, a catalog whose site has been unpublished, and a
//! catalog published without ordering are all the same `Ok(None)`: the public
//! wire turns that into one uniform 404 with no existence leak.
//!
//! Per the privacy model, an order stores what the visitor typed plus what the
//! owner needs to answer them — never anything about the connection.

use crate::error::{Result, StoreError};
use crate::id::SiteOrderId;
use crate::site_catalog_publish::SiteCatalogSnapshotItem;
use crate::site_orders::{OrderContact, OrderRequestLine, normalize_order_lines};
use crate::site_public::SitePublicStore;

/// The longest id token this door will even send to the database. Real ids are
/// 22 characters (base64url of 16 random bytes); anything far outside that
/// shape is noise, not a lookup.
const CATALOG_ID_MAX_LEN: usize = 64;

/// One priced line, resolved against the published snapshot.
#[derive(Debug)]
struct PricedLine {
    item_slug: String,
    item_name: String,
    quantity: i32,
    unit_price_cents: Option<i64>,
    line_total_cents: Option<i64>,
}

impl SitePublicStore {
    /// Records a visitor's order against the published catalog `catalog_id`.
    ///
    /// `contact` has already passed
    /// [`normalize_order_contact`](crate::site_orders::normalize_order_contact);
    /// `lines` are the raw posted (handle, quantity) pairs and pass
    /// [`normalize_order_lines`] here, so both doors share one write gate.
    ///
    /// Returns `Ok(None)` when the id resolves to no orderable published
    /// catalog (unknown, unpublished, or published with ordering off —
    /// deliberately indistinguishable).
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the violated rule, including an item
    /// that is no longer on the published page (safe to show the visitor);
    /// [`StoreError::Db`] on failure; [`StoreError::Conflict`] when the stored
    /// snapshot cannot be read back.
    pub async fn place_public_order(
        &self,
        catalog_id: &str,
        contact: &OrderContact,
        lines: &[OrderRequestLine],
    ) -> Result<Option<SiteOrderId>> {
        let wanted = normalize_order_lines(lines)?;
        if catalog_id.is_empty()
            || catalog_id.len() > CATALOG_ID_MAX_LEN
            || !catalog_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Ok(None);
        }

        // The site's *current* publish is the one the visitor is looking at,
        // and the only one that may be ordered from: a superseded publish is
        // a page nobody is being served any more.
        let resolved: Option<ResolvedCatalog> = sqlx::query_as(
            "SELECT sn.tenant_id, p.site_id, sn.name, sn.currency, sn.items \
             FROM site_catalog_snapshots sn \
             JOIN site_publishes p ON p.tenant_id = sn.tenant_id AND p.id = sn.publish_id \
             JOIN sites s ON s.tenant_id = p.tenant_id AND s.id = p.site_id \
                         AND s.published_publish_id = sn.publish_id \
             WHERE sn.catalog_id = $1 AND sn.orders_enabled \
             LIMIT 1",
        )
        .bind(catalog_id)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        let Some(resolved) = resolved else {
            return Ok(None);
        };
        let items: Vec<SiteCatalogSnapshotItem> = serde_json::from_value(resolved.items.0)
            .map_err(|_| {
                StoreError::Conflict("catalog snapshot has invalid stored items".to_owned())
            })?;

        let priced = price_lines(&wanted, &items)?;
        let total_cents: i64 = priced
            .iter()
            .filter_map(|line| line.line_total_cents)
            .try_fold(0_i64, |sum, line| sum.checked_add(line))
            .ok_or_else(|| {
                StoreError::Validation("that order is larger than we can take online".to_owned())
            })?;

        // The header and its lines are one atomic record: an order that lost
        // half its lines to a failure would be worse than no order at all.
        let id = SiteOrderId::generate();
        let mut tx = self.pool().begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO site_orders \
                 (tenant_id, site_id, id, catalog_id, catalog_name, currency, customer_name, \
                  customer_email, customer_phone, note, total_cents) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(&resolved.tenant_id)
        .bind(&resolved.site_id)
        .bind(id.as_str())
        .bind(catalog_id)
        .bind(&resolved.name)
        .bind(&resolved.currency)
        .bind(&contact.customer_name)
        .bind(&contact.customer_email)
        .bind(contact.customer_phone.as_deref())
        .bind(contact.note.as_deref())
        .bind(total_cents)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        for (position, line) in priced.iter().enumerate() {
            sqlx::query(
                "INSERT INTO site_order_lines \
                     (tenant_id, order_id, position, item_slug, item_name, quantity, \
                      unit_price_cents, line_total_cents) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(&resolved.tenant_id)
            .bind(id.as_str())
            .bind(i32::try_from(position).unwrap_or(i32::MAX))
            .bind(&line.item_slug)
            .bind(&line.item_name)
            .bind(line.quantity)
            .bind(line.unit_price_cents)
            .bind(line.line_total_cents)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(Some(id))
    }
}

/// Resolves each requested handle against the published items, taking the name
/// and unit price from the snapshot.
///
/// An unknown handle and a sold-out item are both refused with a sentence the
/// visitor can act on: their page is older than the catalog, which is a real
/// and recoverable situation (reload and order again), not a server error.
fn price_lines(
    wanted: &[OrderRequestLine],
    published: &[SiteCatalogSnapshotItem],
) -> Result<Vec<PricedLine>> {
    let mut priced = Vec::with_capacity(wanted.len());
    for line in wanted {
        let Some(item) = published.iter().find(|item| item.slug == line.item_slug) else {
            return Err(StoreError::Validation(
                "one of those items is no longer offered; please reload the page and try again"
                    .to_owned(),
            ));
        };
        if item.sold_out {
            return Err(StoreError::Validation(format!(
                "{} is not available at the moment",
                item.name
            )));
        }
        let line_total_cents = match item.price_cents {
            Some(price) => Some(price.checked_mul(i64::from(line.quantity)).ok_or_else(|| {
                StoreError::Validation("that order is larger than we can take online".to_owned())
            })?),
            None => None,
        };
        priced.push(PricedLine {
            item_slug: item.slug.clone(),
            item_name: item.name.clone(),
            quantity: line.quantity,
            unit_price_cents: item.price_cents,
            line_total_cents,
        });
    }
    Ok(priced)
}

#[derive(sqlx::FromRow)]
struct ResolvedCatalog {
    tenant_id: String,
    site_id: String,
    name: String,
    currency: String,
    items: sqlx::types::Json<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn published(slug: &str, price: Option<i64>, sold_out: bool) -> SiteCatalogSnapshotItem {
        SiteCatalogSnapshotItem {
            slug: slug.to_owned(),
            name: format!("The {slug}"),
            category: None,
            description: None,
            price_cents: price,
            price_note: None,
            image: None,
            image_alt: None,
            sold_out,
        }
    }

    fn asked(slug: &str, quantity: i32) -> OrderRequestLine {
        OrderRequestLine {
            item_slug: slug.to_owned(),
            quantity,
        }
    }

    #[test]
    fn prices_and_names_come_from_the_published_snapshot() {
        let items = [
            published("sourdough", Some(450), false),
            published("consultation", None, false),
        ];
        let priced =
            price_lines(&[asked("sourdough", 3), asked("consultation", 1)], &items).unwrap();
        assert_eq!(priced[0].item_name, "The sourdough");
        assert_eq!(priced[0].unit_price_cents, Some(450));
        assert_eq!(priced[0].line_total_cents, Some(1_350));
        // An item published without a price is orderable and quoted by hand.
        assert_eq!(priced[1].unit_price_cents, None);
        assert_eq!(priced[1].line_total_cents, None);
    }

    #[test]
    fn an_unknown_or_sold_out_handle_is_refused_with_an_actionable_sentence() {
        let items = [
            published("sourdough", Some(450), false),
            published("focaccia", Some(600), true),
        ];
        let unknown = price_lines(&[asked("brioche", 1)], &items).unwrap_err();
        assert!(format!("{unknown}").contains("reload"), "{unknown}");
        let gone = price_lines(&[asked("focaccia", 1)], &items).unwrap_err();
        assert!(format!("{gone}").contains("The focaccia"), "{gone}");
    }
}
