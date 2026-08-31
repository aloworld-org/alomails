//! Tenant-scoped presentation snapshots for invoices.

use serde_json::Value;
use sqlx::types::Json;
use time::OffsetDateTime;

use crate::QUOTE_DESIGN_MAX_BYTES;
use crate::account::AccountStore;
use crate::billing_invoices::InvoiceStatus;
use crate::error::{Result, StoreError};
use crate::id::BillingInvoiceId;

/// A stored invoice design and its last write time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoiceDesignRecord {
    pub design: Value,
    pub updated_at: OffsetDateTime,
}

#[derive(sqlx::FromRow)]
struct DesignRow {
    design: Json<Value>,
    updated_at: OffsetDateTime,
}

impl AccountStore {
    /// Reads a design only through this account's tenant boundary.
    pub async fn billing_invoice_design(
        &self,
        id: &BillingInvoiceId,
    ) -> Result<Option<InvoiceDesignRecord>> {
        let row: Option<DesignRow> = sqlx::query_as(
            "SELECT design, updated_at FROM billing_invoice_designs \
             WHERE tenant_id = $1 AND invoice_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        match row {
            Some(row) => Ok(Some(InvoiceDesignRecord {
                design: row.design.0,
                updated_at: row.updated_at,
            })),
            None => {
                self.billing_invoice(id)
                    .await?
                    .ok_or(StoreError::NotFound)?;
                Ok(None)
            }
        }
    }

    /// Replaces the presentation of a draft invoice. Issued documents are
    /// immutable, including their appearance.
    pub async fn set_billing_invoice_design(
        &self,
        id: &BillingInvoiceId,
        design: &Value,
    ) -> Result<()> {
        if !design.is_object() {
            return Err(StoreError::Validation(
                "an invoice design must be a JSON object".to_owned(),
            ));
        }
        let size = serde_json::to_vec(design)
            .map_err(|_| StoreError::Validation("the design cannot be serialised".to_owned()))?
            .len();
        if size > QUOTE_DESIGN_MAX_BYTES {
            return Err(StoreError::Validation(format!(
                "the design is {size} bytes; the most an invoice design may be is {QUOTE_DESIGN_MAX_BYTES}"
            )));
        }
        let invoice = self
            .billing_invoice(id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if invoice.invoice.status != InvoiceStatus::Draft {
            return Err(StoreError::Conflict(
                "the invoice has been issued; its design is frozen with it".to_owned(),
            ));
        }
        sqlx::query(
            "INSERT INTO billing_invoice_designs (tenant_id, invoice_id, design, updated_by) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (tenant_id, invoice_id) DO UPDATE \
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
