//! Explicit Billing documents raised from a Sales opportunity.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{BillingInvoiceId, BillingQuoteId, CrmDealId};

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct DealBillingDocument {
    pub kind: String,
    pub document_id: String,
    pub status: String,
    pub number: Option<String>,
    pub created_at: OffsetDateTime,
}

impl AccountStore {
    pub(crate) async fn link_crm_deal_quote(
        &self,
        deal: &CrmDealId,
        quote: &BillingQuoteId,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO crm_deal_billing_documents (tenant_id, deal_id, quote_id) \
             VALUES ($1, $2, $3)",
        )
        .bind(self.tenant.as_str())
        .bind(deal.as_str())
        .bind(quote.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    pub(crate) async fn link_crm_deal_invoice(
        &self,
        deal: &CrmDealId,
        invoice: &BillingInvoiceId,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO crm_deal_billing_documents (tenant_id, deal_id, invoice_id) \
             VALUES ($1, $2, $3)",
        )
        .bind(self.tenant.as_str())
        .bind(deal.as_str())
        .bind(invoice.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    pub async fn crm_deal_billing_documents(
        &self,
        deal: &CrmDealId,
    ) -> Result<Vec<DealBillingDocument>> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM crm_deals WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(deal.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if !exists {
            return Err(StoreError::NotFound);
        }
        sqlx::query_as(
            "SELECT 'quote'::text AS kind, q.id AS document_id, q.status, q.number, l.created_at \
             FROM crm_deal_billing_documents l JOIN billing_quotes q \
               ON q.tenant_id = l.tenant_id AND q.id = l.quote_id \
             WHERE l.tenant_id = $1 AND l.deal_id = $2 \
             UNION ALL \
             SELECT 'invoice'::text, i.id, i.status, i.number, l.created_at \
             FROM crm_deal_billing_documents l JOIN billing_invoices i \
               ON i.tenant_id = l.tenant_id AND i.id = l.invoice_id \
             WHERE l.tenant_id = $1 AND l.deal_id = $2 \
             ORDER BY created_at DESC, document_id DESC",
        )
        .bind(self.tenant.as_str())
        .bind(deal.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)
    }
}
