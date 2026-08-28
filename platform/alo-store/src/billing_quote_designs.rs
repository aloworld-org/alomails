//! The designed presentation of a quotation — what the quotation studio lays
//! out around the price table (alo Billing, ADR 0035).
//!
//! A design is one JSON document per quote. The **web client owns its shape**;
//! the store keeps it whole and the print path reads the parts it renders
//! (`alo-jmap`'s `quote_design`). That split is deliberate: a new block kind is
//! a client release and a renderer change, never a migration, and a design
//! saved by a newer client is never truncated by an older server.
//!
//! What the store does enforce is what it always enforces: the row is this
//! tenant's, the quote exists under this handle, the document is a JSON object
//! of bounded size, and a quote that has been sent is frozen — its design
//! included, because the paper the customer holds must not change after the
//! fact.
//!
//! Tenancy is structural: every statement carries `tenant_id` from the handle
//! and the composite foreign key pins the design to the same tenant's quote.

use serde_json::Value;
use sqlx::types::Json;
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_quotes::QuoteStatus;
use crate::error::{Result, StoreError};
use crate::id::BillingQuoteId;

/// The most a design may occupy, serialised. Pictures travel inside it as data
/// URLs (the client scales them down before upload), so the ceiling is
/// generous — but it is a ceiling: a design is a page layout, not a file store.
pub const QUOTE_DESIGN_MAX_BYTES: usize = 12 * 1024 * 1024;
// A layout with a handful of scaled photos, not an archive: several 1600 px
// JPEGs as data URLs fit; a design that is really a file upload does not.
const _: () = assert!(QUOTE_DESIGN_MAX_BYTES >= 8 * 1024 * 1024);
const _: () = assert!(QUOTE_DESIGN_MAX_BYTES <= 16 * 1024 * 1024);

/// A stored design and when it was last written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteDesignRecord {
    /// The design, exactly as the client saved it.
    pub design: Value,
    /// When it was last written.
    pub updated_at: OffsetDateTime,
}

#[derive(sqlx::FromRow)]
struct DesignRow {
    design: Json<Value>,
    updated_at: OffsetDateTime,
}

impl AccountStore {
    /// The design of one of this tenant's quotes: `None` when the quote exists
    /// but has never been designed.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the quote is absent **or another tenant's**
    /// (indistinguishable by design); [`StoreError::Db`] on failure.
    pub async fn billing_quote_design(
        &self,
        id: &BillingQuoteId,
    ) -> Result<Option<QuoteDesignRecord>> {
        let row: Option<DesignRow> = sqlx::query_as(
            "SELECT design, updated_at FROM billing_quote_designs \
             WHERE tenant_id = $1 AND quote_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        match row {
            Some(row) => Ok(Some(QuoteDesignRecord {
                design: row.design.0,
                updated_at: row.updated_at,
            })),
            // No design is only an answer about a quote that exists.
            None => {
                self.quote_status(id).await?;
                Ok(None)
            }
        }
    }

    /// Writes the design of one of this tenant's **draft** quotes, replacing
    /// whatever was there.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the quote is absent or another tenant's;
    /// [`StoreError::Validation`] when the design is not a JSON object or
    /// exceeds [`QUOTE_DESIGN_MAX_BYTES`]; [`StoreError::Conflict`] when the
    /// quote is no longer a draft — a sent offer is the document the customer
    /// read, and its presentation is as frozen as its figures;
    /// [`StoreError::Db`] on failure.
    pub async fn set_billing_quote_design(
        &self,
        id: &BillingQuoteId,
        design: &Value,
    ) -> Result<()> {
        if !design.is_object() {
            return Err(StoreError::Validation(
                "a quote design must be a JSON object".to_owned(),
            ));
        }
        let size = serde_json::to_vec(design)
            .map_err(|_| StoreError::Validation("the design cannot be serialised".to_owned()))?
            .len();
        if size > QUOTE_DESIGN_MAX_BYTES {
            return Err(StoreError::Validation(format!(
                "the design is {size} bytes; the most a quote design may be is \
                 {QUOTE_DESIGN_MAX_BYTES}"
            )));
        }
        if self.quote_status(id).await? != QuoteStatus::Draft {
            return Err(StoreError::Conflict(
                "the offer has been sent; its design is frozen with it".to_owned(),
            ));
        }
        sqlx::query(
            "INSERT INTO billing_quote_designs (tenant_id, quote_id, design, updated_by) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (tenant_id, quote_id) DO UPDATE \
             SET design = EXCLUDED.design, updated_by = EXCLUDED.updated_by, updated_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(Json(design))
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }
}
