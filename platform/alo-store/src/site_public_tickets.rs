//! The public service's read of one ticket (ADR 0041, item S3.04d): what the
//! buyer holds, resolved by the token the fulfilment sweep minted
//! ([`crate::site_ticket_fulfil`]).
//!
//! The token is a capability, exactly like a booking's manage token: it
//! travels only in the buyer's own confirmation surfaces (the checkout
//! return page, the `.ics` in their calendar), and every read here requires
//! **both** the token and the [`PublishedSite`] the request arrived on — a
//! ticket minted on one site answers nothing on any other host, so a leaked
//! token cannot even confirm it exists elsewhere. Unknown, malformed,
//! foreign-site and foreign-tenant tokens are all the same `None`, and the
//! public wire turns that into its uniform 404.
//!
//! What the page may see is deliberately less than what the sale knows: the
//! description, the when, the quantity and the holder's name — never the
//! buyer's address, never the money, never the invoice. A ticket names its
//! holder; it does not carry their file.

use time::OffsetDateTime;

use crate::error::{Result, StoreError};
use crate::site_public::{PublishedSite, SitePublicStore};

/// The longest token this door will even send to the database — real tokens
/// are 22 characters (base64url of 16 random bytes).
const TICKET_TOKEN_MAX_LEN: usize = 64;

/// One ticket as its holder may see it.
#[derive(Debug, Clone)]
pub struct PublicTicket {
    /// What was sold, as the fulfilment recorded it ("<product> — <date>");
    /// may still be empty in the moment between claim and record.
    pub description: String,
    /// When the event happens.
    pub starts_at: OffsetDateTime,
    /// How many seats this ticket admits.
    pub quantity: i32,
    /// Who the ticket was issued to, as the buyer gave it.
    pub holder: String,
}

#[derive(sqlx::FromRow)]
struct TicketRow {
    description: String,
    starts_at: OffsetDateTime,
    quantity: i32,
    holder: String,
}

impl SitePublicStore {
    /// Resolves a ticket token **on the site it was minted for**, or `None` —
    /// one answer for unknown, malformed, foreign-site and foreign-tenant
    /// tokens alike.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn public_ticket(
        &self,
        site: &PublishedSite,
        token: &str,
    ) -> Result<Option<PublicTicket>> {
        let Some(token) = plausible(token) else {
            return Ok(None);
        };
        let row: Option<TicketRow> = sqlx::query_as(
            "SELECT f.description, e.starts_at, o.quantity, o.buyer_name AS holder \
               FROM site_ticket_fulfilments f \
               JOIN site_ticket_orders o \
                 ON o.tenant_id = f.tenant_id AND o.id = f.order_id \
               JOIN site_ticket_events e \
                 ON e.tenant_id = f.tenant_id AND e.id = f.event_id \
              WHERE f.tenant_id = $1 AND f.site_id = $2 AND f.token = $3",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(token)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(|row| PublicTicket {
            description: row.description,
            starts_at: row.starts_at,
            quantity: row.quantity,
            holder: row.holder,
        }))
    }
}

/// The same shape gate the manage-token door applies: refuse anything that
/// cannot be a minted token before the database is involved at all.
fn plausible(token: &str) -> Option<&str> {
    let token = token.trim();
    (!token.is_empty()
        && token.len() <= TICKET_TOKEN_MAX_LEN
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
    .then_some(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_mintable_shape_reaches_the_database() {
        assert_eq!(plausible("  tok-1_A  "), Some("tok-1_A"));
        assert!(plausible("").is_none());
        assert!(plausible("   ").is_none());
        assert!(plausible(&"x".repeat(65)).is_none());
        assert!(plausible("a token").is_none());
        assert!(plausible("tok;drop").is_none());
    }
}
