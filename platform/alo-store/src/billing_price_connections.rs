//! Tenant-scoped price-list connections for alo Billing.
//!
//! The relationship and products it exposes are durable business data. Remote
//! credentials and imported payloads deliberately do not live here.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{BillingPriceConnectionId, BillingProductId};

/// Direction of the price flow relative to this tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceConnectionDirection {
    /// A supplier publishes buying prices to this tenant.
    Received,
    /// This tenant publishes selected selling prices to a client.
    Shared,
}

impl PriceConnectionDirection {
    /// Stable database/API spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::Shared => "shared",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "received" => Ok(Self::Received),
            "shared" => Ok(Self::Shared),
            _ => Err(StoreError::Db(sqlx::Error::Decode(
                "unknown price-connection direction".into(),
            ))),
        }
    }
}

/// Operational state shown in the Billing connection list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceConnectionHealth {
    Connected,
    Attention,
    Paused,
    Expired,
}

impl PriceConnectionHealth {
    /// Stable database/API spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Attention => "attention",
            Self::Paused => "paused",
            Self::Expired => "expired",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "connected" => Ok(Self::Connected),
            "attention" => Ok(Self::Attention),
            "paused" => Ok(Self::Paused),
            "expired" => Ok(Self::Expired),
            _ => Err(StoreError::Db(sqlx::Error::Decode(
                "unknown price-connection health".into(),
            ))),
        }
    }
}

/// A new durable price connection.
#[derive(Debug, Clone)]
pub struct NewPriceConnection {
    pub direction: PriceConnectionDirection,
    pub company: String,
    pub catalogue: String,
    pub cadence: String,
    pub channel: String,
    pub product_ids: Vec<BillingProductId>,
}

/// A stored connection and the products it exposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceConnection {
    pub id: BillingPriceConnectionId,
    pub direction: PriceConnectionDirection,
    pub company: String,
    pub catalogue: String,
    pub health: PriceConnectionHealth,
    pub cadence: String,
    pub channel: String,
    pub changes_count: i32,
    pub last_synced_at: Option<OffsetDateTime>,
    pub expires_at: Option<OffsetDateTime>,
    pub product_ids: Vec<BillingProductId>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

type ConnectionRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    i32,
    Option<OffsetDateTime>,
    Option<OffsetDateTime>,
    OffsetDateTime,
    OffsetDateTime,
);

fn text(field: &str, value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 200 {
        return Err(StoreError::Validation(format!(
            "{field} must contain 1 to 200 characters"
        )));
    }
    Ok(value.to_owned())
}

fn choice(field: &str, value: &str, choices: &[&str]) -> Result<String> {
    if choices.contains(&value) {
        Ok(value.to_owned())
    } else {
        Err(StoreError::Validation(format!("unknown {field}")))
    }
}

impl AccountStore {
    /// Lists this tenant's price connections, newest first.
    pub async fn billing_price_connections(&self) -> Result<Vec<PriceConnection>> {
        let rows: Vec<ConnectionRow> = sqlx::query_as(
            "SELECT id,direction,company,catalogue,health,cadence,channel,changes_count,\
             last_synced_at,expires_at,created_at,updated_at FROM billing_price_connections \
             WHERE tenant_id=$1 ORDER BY updated_at DESC,id",
        )
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let links: Vec<(String, String)> = sqlx::query_as(
            "SELECT connection_id,product_id FROM billing_price_connection_products \
             WHERE tenant_id=$1 ORDER BY connection_id,product_id",
        )
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut products = std::collections::HashMap::<String, Vec<BillingProductId>>::new();
        for (connection, product) in links {
            products
                .entry(connection)
                .or_default()
                .push(BillingProductId::new(product));
        }
        rows.into_iter()
            .map(|row| {
                let (
                    id,
                    direction,
                    company,
                    catalogue,
                    health,
                    cadence,
                    channel,
                    changes_count,
                    last_synced_at,
                    expires_at,
                    created_at,
                    updated_at,
                ) = row;
                Ok(PriceConnection {
                    product_ids: products.remove(&id).unwrap_or_default(),
                    id: BillingPriceConnectionId::new(id),
                    direction: PriceConnectionDirection::parse(&direction)?,
                    company,
                    catalogue,
                    health: PriceConnectionHealth::parse(&health)?,
                    cadence,
                    channel,
                    changes_count,
                    last_synced_at,
                    expires_at,
                    created_at,
                    updated_at,
                })
            })
            .collect()
    }

    /// Creates a connection and its tenant-bound product links atomically.
    pub async fn create_billing_price_connection(
        &self,
        input: &NewPriceConnection,
    ) -> Result<BillingPriceConnectionId> {
        let company = text("company", &input.company)?;
        let catalogue = text("catalogue", &input.catalogue)?;
        let cadence = choice(
            "cadence",
            input.cadence.trim(),
            &["hourly", "daily", "weekly", "manual", "live", "approval"],
        )?;
        let channel = choice("channel", input.channel.trim(), &["alo", "api"])?;
        let mut ids: Vec<String> = input
            .product_ids
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect();
        ids.sort();
        ids.dedup();
        if !ids.is_empty() {
            let count: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM billing_products WHERE tenant_id=$1 AND id=ANY($2::text[])")
                .bind(self.tenant.as_str()).bind(&ids).fetch_one(&self.pool).await.map_err(StoreError::Db)?;
            if usize::try_from(count).ok() != Some(ids.len()) {
                return Err(StoreError::NotFound);
            }
        }
        let id = BillingPriceConnectionId::generate();
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query("INSERT INTO billing_price_connections (tenant_id,id,direction,company,catalogue,cadence,channel,created_by) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(self.tenant.as_str()).bind(id.as_str()).bind(input.direction.as_str())
            .bind(company).bind(catalogue).bind(cadence).bind(channel).bind(self.user.as_str())
            .execute(&mut *tx).await.map_err(StoreError::Db)?;
        for product in ids {
            sqlx::query("INSERT INTO billing_price_connection_products (tenant_id,connection_id,product_id) VALUES ($1,$2,$3)")
                .bind(self.tenant.as_str()).bind(id.as_str()).bind(product)
                .execute(&mut *tx).await.map_err(StoreError::Db)?;
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Pauses, resumes, expires or marks one of this tenant's connections.
    pub async fn set_billing_price_connection_health(
        &self,
        id: &BillingPriceConnectionId,
        health: PriceConnectionHealth,
    ) -> Result<()> {
        let done = sqlx::query("UPDATE billing_price_connections SET health=$3,updated_at=now() WHERE tenant_id=$1 AND id=$2")
            .bind(self.tenant.as_str()).bind(id.as_str()).bind(health.as_str())
            .execute(&self.pool).await.map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Records a successful sync and clears pending changes.
    pub async fn sync_billing_price_connection(&self, id: &BillingPriceConnectionId) -> Result<()> {
        let done = sqlx::query("UPDATE billing_price_connections SET health='connected',changes_count=0,last_synced_at=now(),updated_at=now() WHERE tenant_id=$1 AND id=$2")
            .bind(self.tenant.as_str()).bind(id.as_str()).execute(&self.pool).await.map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Disconnects one of this tenant's connections.
    pub async fn delete_billing_price_connection(
        &self,
        id: &BillingPriceConnectionId,
    ) -> Result<()> {
        let done =
            sqlx::query("DELETE FROM billing_price_connections WHERE tenant_id=$1 AND id=$2")
                .bind(self.tenant.as_str())
                .bind(id.as_str())
                .execute(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}
